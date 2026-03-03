//! LilyPond exporter — generates a complete standalone `.ly` file from a Song.
//!
//! Uses a Handlebars template for the file structure and produces variable-based
//! LilyPond output with `\relative c'` wrapping, `\paper`, `\layout`, and
//! numbered verse variables.
//!
//! When a song has sections with independent melodies (e.g. stanza + refrain),
//! the voices are concatenated into a single combined melody variable. The first
//! verse's lyrics include the refrain text so that all syllables align with the
//! full melody. Subsequent verses only contain the stanza lyrics.

use handlebars::Handlebars;
use serde::Serialize;

use crate::song::{Song, SongPart, SongPartContent, SongPartContentType, SongPartType};

/// Configuration for LilyPond export output.
#[derive(Clone, PartialEq, Debug)]
pub struct LilypondSettings {
    /// Paper size for the `\paper` block (default: "a4")
    pub paper_size: String,
    /// Indent setting for `\layout` block (default: "#0")
    pub layout_indent: String,
}

impl Default for LilypondSettings {
    fn default() -> Self {
        LilypondSettings {
            paper_size: "a4".to_string(),
            layout_indent: "#0".to_string(),
        }
    }
}

/// Data for a single verse/refrain variable in the template.
#[derive(Serialize)]
struct VerseData {
    var_name: String,
    stanza: String,
    content: String,
    /// The variable reference including backslash, e.g. `\verseOne`
    var_ref: String,
}

/// All data needed to render the LilyPond template.
#[derive(Serialize)]
struct LilypondTemplateData {
    version: String,
    title: String,
    composer: Option<String>,
    paper_size: String,
    layout_indent: String,
    global_content: String,
    has_chords: bool,
    chord_content: String,
    voice_var_name: String,
    voice_content: String,
    voice_var_ref: String,
    voice_part_name: String,
    /// The part reference including backslash, e.g. `\sopranoVoicePart`
    voice_part_ref: String,
    midi_instrument: String,
    verses: Vec<VerseData>,
}

/// The Handlebars template for the LilyPond file structure.
///
/// Notes on escaping:
/// - LilyPond `\commands` (like `\version`, `\header`) pass through Handlebars
///   unchanged since they are never followed by `{{`.
/// - All variable insertions use triple-braces `{{{...}}}` to avoid HTML-escaping.
/// - Variable references with backslashes (e.g. `\sopranoVoice`) are stored
///   pre-escaped in the data and output via `{{{var_ref}}}`.
const LILYPOND_TEMPLATE: &str = r#"\version "{{{version}}}"

\header {
  title = "{{{title}}}"
{{#if composer}}  composer = "{{{composer}}}"
{{/if}}  tagline = ##f
}

\paper {
  #(set-paper-size "{{{paper_size}}}")
}

\layout {
  indent = {{{layout_indent}}}
  \context {
    \Voice
    \consists "Melody_engraver"
    \override Stem.neutral-direction = #'()
  }
}

global = {
{{{global_content}}}
}

{{#if has_chords}}
chordNames = \chordmode {
  \global
{{{chord_content}}}
}

{{/if}}
{{{voice_var_name}}} = \relative c' {
  \global
{{{voice_content}}}
}

{{#each verses}}
{{{this.var_name}}} = \lyricmode {
  \set stanza = "{{{this.stanza}}}"
{{{this.content}}}
}

{{/each}}
{{{voice_part_name}}} = \new Staff \with {
  midiInstrument = "{{{midi_instrument}}}"
} { {{{voice_var_ref}}} }
{{#each verses}}
\addlyrics { {{{this.var_ref}}} }
{{/each}}

{{#if has_chords}}
chordsPart = \new ChordNames \chordNames

{{/if}}
\score {
  <<
{{#if has_chords}}
    \chordsPart
{{/if}}
    {{{voice_part_ref}}}
  >>
  \layout { }
  \midi { }
}
"#;

/// Convert a verse number to an English word for LilyPond variable naming.
fn number_to_word(n: u32) -> String {
    match n {
        1 => "One".to_string(),
        2 => "Two".to_string(),
        3 => "Three".to_string(),
        4 => "Four".to_string(),
        5 => "Five".to_string(),
        6 => "Six".to_string(),
        7 => "Seven".to_string(),
        8 => "Eight".to_string(),
        9 => "Nine".to_string(),
        10 => "Ten".to_string(),
        11 => "Eleven".to_string(),
        12 => "Twelve".to_string(),
        13 => "Thirteen".to_string(),
        14 => "Fourteen".to_string(),
        15 => "Fifteen".to_string(),
        16 => "Sixteen".to_string(),
        17 => "Seventeen".to_string(),
        18 => "Eighteen".to_string(),
        19 => "Nineteen".to_string(),
        20 => "Twenty".to_string(),
        _ => format!("N{}", n),
    }
}

/// Map a voice content type to a LilyPond variable name.
fn voice_type_to_var_name(vt: &SongPartContentType) -> &str {
    match vt {
        SongPartContentType::LeadVoice | SongPartContentType::SupranoVoice => "sopranoVoice",
        SongPartContentType::AltoVoice => "altoVoice",
        SongPartContentType::TenorVoice => "tenorVoice",
        SongPartContentType::BassVoice => "bassVoice",
        _ => "sopranoVoice",
    }
}

/// Convert a human-readable key string (e.g. "f major") to LilyPond format (`\key f \major`).
fn format_lilypond_key(key_str: &str) -> Option<String> {
    let parts: Vec<&str> = key_str.trim().split_whitespace().collect();
    if parts.len() == 2 {
        let note = parts[0].to_lowercase();
        let mode = parts[1].to_lowercase();
        Some(format!("\\key {} \\{}", note, mode))
    } else {
        None
    }
}

/// Indent each line of a multiline string by the given prefix.
fn indent_lines(text: &str, prefix: &str) -> String {
    text.trim()
        .lines()
        .map(|line| format!("{}{}", prefix, line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if a part has its own voice content directly (not inherited via `is_repetition_of`).
fn find_own_voice(part: &SongPart) -> Option<&SongPartContent> {
    part.contents.iter().find(|c| {
        matches!(
            c.voice_type,
            SongPartContentType::LeadVoice
                | SongPartContentType::SupranoVoice
                | SongPartContentType::AltoVoice
                | SongPartContentType::TenorVoice
                | SongPartContentType::BassVoice
        )
    })
}

/// Find the first lyrics content in a part.
fn find_lyrics(part: &SongPart) -> Option<&SongPartContent> {
    part.contents.iter().find(|c| c.voice_type.is_lyrics())
}

/// Generate a complete LilyPond (.ly) file from a Song.
///
/// The output uses a variable-based structure with `\relative c'` wrapping,
/// configurable `\paper` and `\layout` blocks, and numbered verse variables.
///
/// When a refrain or chorus has its own melody (voice content), the exporter
/// concatenates the stanza melody + refrain melody into one combined voice
/// variable. The first verse's lyrics include the refrain text appended, so
/// all syllables align with the full combined melody. Subsequent verses only
/// contain their stanza lyrics.
///
/// Returns an error if the song has no voice content to export.
pub fn lilypond_from_song(song: &Song, settings: &LilypondSettings) -> Result<String, String> {
    let parts = song.get_unpacked_parts();

    // --- Step 1: Find the stanza (verse) voice ---
    let stanza_voice = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Verse)
        .find_map(|part| song.get_voice_for_part(part))
        .ok_or_else(|| "Song has no voice content for LilyPond export".to_string())?;

    let voice_var_name = voice_type_to_var_name(&stanza_voice.voice_type).to_string();
    let voice_var_ref = format!("\\{}", voice_var_name);
    let voice_part_name = format!("{}Part", voice_var_name);
    let voice_part_ref = format!("\\{}", voice_part_name);

    // --- Step 2: Check refrain/chorus parts for their own independent voice ---
    let refrain_parts: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Refrain || p.part_type == SongPartType::Chorus)
        .collect();

    // A refrain "owns" its voice if the voice content is directly on the part,
    // not inherited from a verse via is_repetition_of.
    let refrain_own_voice: Option<&SongPartContent> =
        refrain_parts.iter().find_map(|part| find_own_voice(part));

    // Only collect refrain lyrics for embedding into verse 1 if the refrain
    // has its own melody (i.e. it's a separate musical section).
    let refrain_lyrics_for_embedding: Option<String> = if refrain_own_voice.is_some() {
        refrain_parts
            .iter()
            .find_map(|part| find_lyrics(part))
            .map(|c| c.content.clone())
    } else {
        None
    };

    // --- Step 3: Build combined voice content ---
    // Stanza melody first, then refrain melody appended (if it has its own).
    let mut combined_voice = stanza_voice.content.trim().to_string();
    if let Some(rv) = refrain_own_voice {
        combined_voice = format!("{}\n\n{}", combined_voice, rv.content.trim());
    }

    // --- Step 4: Build global content (key, time, partial) ---
    let mut global_lines: Vec<String> = Vec::new();
    if let Some(key_str) = song.get_tag("key") {
        if let Some(ly_key) = format_lilypond_key(key_str) {
            global_lines.push(ly_key);
        }
    }
    if let Some(time_str) = song.get_tag("time") {
        global_lines.push(format!("\\time {}", time_str));
    }
    if let Some(partial_str) = song.get_tag("partial") {
        global_lines.push(format!("\\partial {}", partial_str));
    }
    let global_content = indent_lines(&global_lines.join("\n"), "  ");

    // --- Step 5: Collect verse lyrics as numbered variables ---
    let mut verse_parts_sorted: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Verse)
        .collect();
    verse_parts_sorted.sort_by_key(|p| p.number);

    let mut verses: Vec<VerseData> = Vec::new();
    let mut verse_number: u32 = 1;
    let mut is_first_verse = true;

    for part in &verse_parts_sorted {
        for content in &part.contents {
            if content.voice_type.is_lyrics() {
                let var_name = format!("verse{}", number_to_word(verse_number));
                let var_ref = format!("\\{}", var_name);

                // For the first verse, append refrain lyrics if the refrain has
                // its own melody. This ensures the lyrics align with the combined
                // stanza+refrain voice.
                let mut lyrics_text = content.content.clone();
                if is_first_verse {
                    if let Some(ref refrain_lyrics) = refrain_lyrics_for_embedding {
                        lyrics_text = format!(
                            "{}\n\n{}",
                            lyrics_text.trim(),
                            refrain_lyrics.trim()
                        );
                    }
                    is_first_verse = false;
                }

                verses.push(VerseData {
                    var_name,
                    stanza: format!("{}.", verse_number),
                    content: indent_lines(&lyrics_text, "  "),
                    var_ref,
                });
                verse_number += 1;
            }
        }
    }

    // --- Step 6: Refrain/chorus parts WITHOUT their own voice ---
    // If the refrain shares the verse melody (no independent voice), add its
    // lyrics as separate variables (traditional \addlyrics approach).
    if refrain_own_voice.is_none() {
        let mut refrain_number: u32 = 1;
        for part in &refrain_parts {
            for content in &part.contents {
                if content.voice_type.is_lyrics() {
                    let var_name = format!("refrain{}", number_to_word(refrain_number));
                    let var_ref = format!("\\{}", var_name);
                    verses.push(VerseData {
                        var_name,
                        stanza: format!("R{}.", refrain_number),
                        content: indent_lines(&content.content, "  "),
                        var_ref,
                    });
                    refrain_number += 1;
                }
            }
        }
    }

    // --- Step 7: Check for chord content ---
    let chord_content_opt = parts.iter().find_map(|part| {
        part.contents
            .iter()
            .find(|c| matches!(c.voice_type, SongPartContentType::Chords))
            .map(|c| c.content.clone())
    });
    let has_chords = chord_content_opt.is_some();
    let chord_content = chord_content_opt
        .map(|c| indent_lines(&c, "  "))
        .unwrap_or_default();

    // --- Step 8: Build template data and render ---
    let data = LilypondTemplateData {
        version: "2.24.0".to_string(),
        title: song.title.clone(),
        composer: song.get_tag("author").cloned(),
        paper_size: settings.paper_size.clone(),
        layout_indent: settings.layout_indent.clone(),
        global_content,
        has_chords,
        chord_content,
        voice_var_name,
        voice_content: indent_lines(&combined_voice, "  "),
        voice_var_ref,
        voice_part_name,
        voice_part_ref,
        midi_instrument: "choir aahs".to_string(),
        verses,
    };

    let handlebars = Handlebars::new();
    handlebars
        .render_template(LILYPOND_TEMPLATE, &data)
        .map_err(|e| format!("Template rendering failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::song_yml;

    #[test]
    fn test_lilypond_export_simple_verses() {
        // Amazing Grace: verses only, no refrain with its own melody
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let ly_output = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();

        // Version
        assert!(ly_output.contains("\\version \"2.24.0\""));
        // Header
        assert!(ly_output.contains("title = \"Amazing Grace\""));
        assert!(ly_output.contains("composer = \"John Newton\""));
        assert!(ly_output.contains("tagline = ##f"));
        // Paper and layout
        assert!(ly_output.contains("#(set-paper-size \"a4\")"));
        assert!(ly_output.contains("indent = #0"));
        assert!(ly_output.contains("Melody_engraver"));
        // Global variable
        assert!(ly_output.contains("global = {"));
        assert!(ly_output.contains("\\key f \\major"));
        assert!(ly_output.contains("\\time 3/4"));
        assert!(ly_output.contains("\\partial 4"));
        // Voice in \relative c'
        assert!(ly_output.contains("sopranoVoice = \\relative c'"));
        assert!(ly_output.contains("\\global"));
        assert!(ly_output.contains("c4 | f2 a8( f)"));
        // Verse variables with stanza numbering
        assert!(ly_output.contains("verseOne = \\lyricmode"));
        assert!(ly_output.contains("\\set stanza = \"1.\""));
        assert!(ly_output.contains("A -- ma -- zing grace"));
        assert!(ly_output.contains("verseTwo = \\lyricmode"));
        assert!(ly_output.contains("\\set stanza = \"2.\""));
        assert!(ly_output.contains("verseThree = \\lyricmode"));
        assert!(ly_output.contains("\\set stanza = \"3.\""));
        // Part assembly
        assert!(ly_output.contains("sopranoVoicePart = \\new Staff"));
        assert!(ly_output.contains("midiInstrument = \"choir aahs\""));
        assert!(ly_output.contains("\\addlyrics { \\verseOne }"));
        assert!(ly_output.contains("\\addlyrics { \\verseTwo }"));
        assert!(ly_output.contains("\\addlyrics { \\verseThree }"));
        // Score
        assert!(ly_output.contains("\\score {"));
        assert!(ly_output.contains("\\layout { }"));
        assert!(ly_output.contains("\\midi { }"));
    }

    #[test]
    fn test_lilypond_export_stanza_refrain() {
        // "Sei nicht stolz" has stanza voice + refrain voice (separate melodies)
        let content = std::fs::read_to_string(
            "testfiles/Sei nicht stolz auf das, was du bist.song.yml",
        )
        .unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let ly_output = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();

        // Combined voice should contain BOTH stanza and refrain melodies
        assert!(
            ly_output.contains("d8 e | fis4 fis g4 fis8 e"),
            "Stanza melody missing"
        );
        assert!(
            ly_output.contains("fis8( g ) | a8 a a a d,4. d8"),
            "Refrain melody missing from combined voice"
        );

        // Voice should be a single \relative c' block with both melodies
        assert!(ly_output.contains("sopranoVoice = \\relative c'"));

        // Verse 1 should contain stanza 1 lyrics AND refrain lyrics
        assert!(ly_output.contains("verseOne = \\lyricmode"));
        assert!(
            ly_output.contains("Sei nicht stolz"),
            "Verse 1 stanza lyrics missing"
        );
        assert!(
            ly_output.contains("Denn wer sich"),
            "Refrain lyrics should be embedded in verse 1"
        );

        // Verse 2 should contain only stanza 2 lyrics (no refrain)
        assert!(ly_output.contains("verseTwo = \\lyricmode"));

        // Verse 3 should contain stanza 3 lyrics
        assert!(ly_output.contains("verseThree = \\lyricmode"));

        // There should NOT be a separate refrainOne variable
        // (refrain lyrics are embedded into verse 1 instead)
        assert!(
            !ly_output.contains("refrainOne"),
            "Refrain with own voice should not produce separate refrain variable"
        );

        // There should be exactly 3 \addlyrics (one per verse)
        let addlyrics_count = ly_output.matches("\\addlyrics").count();
        assert_eq!(
            addlyrics_count, 3,
            "Expected 3 addlyrics, got {}",
            addlyrics_count
        );
    }

    #[test]
    fn test_custom_settings() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = LilypondSettings {
            paper_size: "a5".to_string(),
            layout_indent: "#10".to_string(),
        };

        let ly_output = lilypond_from_song(&song, &settings).unwrap();

        assert!(ly_output.contains("#(set-paper-size \"a5\")"));
        assert!(ly_output.contains("indent = #10"));
    }

    #[test]
    fn test_format_lilypond_key() {
        assert_eq!(
            format_lilypond_key("f major"),
            Some("\\key f \\major".to_string())
        );
        assert_eq!(
            format_lilypond_key("d minor"),
            Some("\\key d \\minor".to_string())
        );
        assert_eq!(format_lilypond_key("invalid"), None);
    }

    #[test]
    fn test_number_to_word() {
        assert_eq!(number_to_word(1), "One");
        assert_eq!(number_to_word(5), "Five");
        assert_eq!(number_to_word(10), "Ten");
        assert_eq!(number_to_word(20), "Twenty");
        assert_eq!(number_to_word(21), "N21");
    }
}
