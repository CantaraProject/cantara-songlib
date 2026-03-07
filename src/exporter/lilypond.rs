//! LilyPond exporter — generates complete standalone `.ly` files from a Song.
//!
//! Supports two export approaches:
//!
//! 1. **Paper score** (`lilypond_from_song`): Produces a hymn-book–style layout
//!    where all verse lyrics appear as `\addlyrics` under one combined melody
//!    staff. This is the traditional printed music approach.
//!
//! 2. **Sequential** (`lilypond_sequential_from_song`): Produces the song with
//!    every part printed in the exact order as it is sung (e.g. stanza 1 →
//!    refrain → stanza 2 → refrain). Each part gets its own `\score` block.
//!
//! Both approaches use Handlebars templates and produce variable-based LilyPond
//! output with `\relative c'` wrapping, `\paper`, `\layout`, and named
//! variables.
//!
//! Additionally, the module provides functions to:
//! - Generate standalone `.ly` files for individual song parts
//!   (`lilypond_parts_from_song`)
//! - Render LilyPond content to SVG or PDF via the LilyPond binary
//!   (`render_lilypond_to_svg`, `render_lilypond_to_pdf`)
//! - Render all song parts as cropped SVGs (`render_song_parts_to_svg`)
//! - Render the paper score as SVG or PDF (`render_paper_score_to_svg`,
//!   `render_paper_score_to_pdf`)

use std::path::Path;

use handlebars::Handlebars;
use serde::Serialize;

use crate::song::{Song, SongPart, SongPartContent, SongPartContentType, SongPartType};

/// Font configuration for LilyPond export.
#[derive(Clone, PartialEq, Debug)]
pub enum FontSetting {
    /// Use LilyPond's default font settings.
    Default,
    /// Use a specific font family for the roman (text) font.
    Specific { family: String },
}

impl std::default::Default for FontSetting {
    fn default() -> Self {
        FontSetting::Default
    }
}

/// Configuration for LilyPond export output.
#[derive(Clone, PartialEq, Debug)]
pub struct LilypondSettings {
    /// Paper size for the `\paper` block (default: "a4")
    pub paper_size: String,
    /// Indent setting for `\layout` block (default: "#0")
    pub layout_indent: String,
    /// Font configuration (default: LilyPond defaults)
    pub font: FontSetting,
    /// Optional global staff size override (LilyPond default is 20)
    pub staff_size: Option<f32>,
}

impl Default for LilypondSettings {
    fn default() -> Self {
        LilypondSettings {
            paper_size: "a4".to_string(),
            layout_indent: "#0".to_string(),
            font: FontSetting::Default,
            staff_size: None,
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

/// Data for a voice definition (e.g. sopranoVoiceStanza, sopranoVoiceRefrain).
#[derive(Serialize)]
struct VoiceDefinition {
    var_name: String,
    content: String,
    /// Whether to include `\global` at the top of this voice definition.
    /// Only the first voice in the combined sequence should include it,
    /// so that key/time signatures are not repeated.
    include_global: bool,
}

/// All data needed to render the paper score LilyPond template.
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
    voice_defs: Vec<VoiceDefinition>,
    /// Combined voice variable references for the Staff, e.g. `\sopranoVoiceStanza \sopranoVoiceRefrain`
    combined_voice_refs: String,
    voice_part_name: String,
    /// The part reference including backslash, e.g. `\sopranoVoicePart`
    voice_part_ref: String,
    midi_instrument: String,
    verses: Vec<VerseData>,
    staff_size: Option<f32>,
    font_block: Option<String>,
}

/// The Handlebars template for the **paper score** LilyPond file structure.
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
{{#if staff_size}}
#(set-global-staff-size {{{staff_size}}})
{{/if}}
\paper {
  #(set-paper-size "{{{paper_size}}}")
{{#if font_block}}{{{font_block}}}
{{/if}}}

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
{{#each voice_defs}}
{{{this.var_name}}} = \relative c' {
{{#if this.include_global}}  \global
{{/if}}{{{this.content}}}
}

{{/each}}
{{#each verses}}
{{{this.var_name}}} = \lyricmode {
  \set stanza = "{{{this.stanza}}}"
{{{this.content}}}
}

{{/each}}
{{{voice_part_name}}} = \new Staff \with {
  midiInstrument = "{{{midi_instrument}}}"
} { {{{combined_voice_refs}}} }
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

/// Ensure voice content ends with `\bar "|."` (final bar line).
fn ensure_final_bar(content: &str) -> String {
    let trimmed = content.trim_end();
    if trimmed.ends_with("\\bar \"|.\"") {
        trimmed.to_string()
    } else {
        format!("{} \\bar \"|.\"", trimmed)
    }
}

/// Build the LilyPond `#(define fonts ...)` block for a custom font setting.
fn build_font_block(font: &FontSetting) -> Option<String> {
    match font {
        FontSetting::Default => None,
        FontSetting::Specific { family } => Some(format!(
            "  #(define fonts\n    (set-global-fonts\n      #:roman \"{}\"\n    ))",
            family
        )),
    }
}

/// Build the global content string (key, time, partial) from song tags.
fn build_global_content(song: &Song) -> String {
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
    indent_lines(&global_lines.join("\n"), "  ")
}

// ---------------------------------------------------------------------------
// Sequential export types and template
// ---------------------------------------------------------------------------

/// Data for a lyrics definition in the sequential template.
#[derive(Serialize)]
struct LyricsDefinition {
    var_name: String,
    stanza: Option<String>,
    content: String,
}

/// Data for a single section in the sequential output (one `\score` block).
#[derive(Serialize)]
struct SequentialSection {
    label: String,
    voice_ref: String,
    lyrics_ref: String,
    midi_instrument: String,
}

/// All data needed to render the sequential LilyPond template.
#[derive(Serialize)]
struct SequentialTemplateData {
    version: String,
    title: String,
    composer: Option<String>,
    paper_size: String,
    layout_indent: String,
    global_content: String,
    voice_defs: Vec<VoiceDefinition>,
    lyrics_defs: Vec<LyricsDefinition>,
    sections: Vec<SequentialSection>,
    staff_size: Option<f32>,
    font_block: Option<String>,
}

/// The Handlebars template for the **sequential** LilyPond file structure.
///
/// Each section of the singing order gets its own `\score` block, producing
/// output that mirrors exactly how the song is performed.
const LILYPOND_SEQUENTIAL_TEMPLATE: &str = r#"\version "{{{version}}}"

\header {
  title = "{{{title}}}"
{{#if composer}}  composer = "{{{composer}}}"
{{/if}}  tagline = ##f
}
{{#if staff_size}}
#(set-global-staff-size {{{staff_size}}})
{{/if}}
\paper {
  #(set-paper-size "{{{paper_size}}}")
{{#if font_block}}{{{font_block}}}
{{/if}}}

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

{{#each voice_defs}}
{{{this.var_name}}} = \relative c' {
{{#if this.include_global}}  \global
{{/if}}{{{this.content}}}
}

{{/each}}
{{#each lyrics_defs}}
{{{this.var_name}}} = \lyricmode {
{{#if this.stanza}}  \set stanza = "{{{this.stanza}}}"
{{/if}}{{{this.content}}}
}

{{/each}}
{{#each sections}}
% === {{{this.label}}} ===
\score {
  <<
    \new Staff \with {
      midiInstrument = "{{{this.midi_instrument}}}"
    } { {{{this.voice_ref}}} }
    \addlyrics { {{{this.lyrics_ref}}} }
  >>
  \header { piece = "{{{this.label}}}" }
  \layout { }
}

{{/each}}"#;

// ---------------------------------------------------------------------------
// Per-part export types and template
// ---------------------------------------------------------------------------

/// A standalone LilyPond file for a single song part.
pub struct LilypondPart {
    /// Human-readable label (e.g. "Stanza 1", "Refrain")
    pub label: String,
    /// Complete `.ly` file content for this part
    pub ly_content: String,
}

/// All data needed to render a standalone per-part LilyPond file.
#[derive(Serialize)]
struct PartTemplateData {
    version: String,
    global_content: String,
    voice_var_name: String,
    voice_content: String,
    voice_ref: String,
    lyrics_var_name: String,
    lyrics_content: String,
    lyrics_ref: String,
    stanza: Option<String>,
    staff_size: Option<f32>,
    font_block: Option<String>,
}

/// The Handlebars template for a standalone single-part LilyPond file.
///
/// Used to generate cropped SVGs for individual song parts.
const LILYPOND_PART_TEMPLATE: &str = r#"\version "{{{version}}}"
{{#if staff_size}}
#(set-global-staff-size {{{staff_size}}})
{{/if}}
\paper {
  indent = #0
{{#if font_block}}{{{font_block}}}
{{/if}}}

\layout {
  \context {
    \Voice
    \consists "Melody_engraver"
    \override Stem.neutral-direction = #'()
  }
}

global = {
{{{global_content}}}
}

{{{voice_var_name}}} = \relative c' {
  \global
{{{voice_content}}}
}

{{{lyrics_var_name}}} = \lyricmode {
{{#if stanza}}  \set stanza = "{{{stanza}}}"
{{/if}}{{{lyrics_content}}}
}

\score {
  <<
    \new Staff { {{{voice_ref}}} }
    \addlyrics { {{{lyrics_ref}}} }
  >>
  \layout { }
}
"#;

/// Generate a complete LilyPond (.ly) file from a Song.
///
/// The output uses a variable-based structure with `\relative c'` wrapping,
/// configurable `\paper` and `\layout` blocks, and numbered verse variables.
///
/// When a refrain or chorus has its own melody (voice content), the exporter
/// creates separate voice variables for stanza and refrain (e.g.
/// `sopranoVoiceStanza` and `sopranoVoiceRefrain`). The first verse's lyrics
/// include the refrain text appended (or prepended for refrain-first songs),
/// so all syllables align with the full combined melody. Subsequent verses
/// only contain their stanza lyrics.
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

    let base_voice_name = voice_type_to_var_name(&stanza_voice.voice_type).to_string();
    let voice_part_name = format!("{}Part", base_voice_name);
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

    // --- Step 3: Determine ordering and build voice definitions ---
    let is_refrain_first = song
        .part_orders
        .first()
        .map_or(false, |o| o.is_refrain_first());

    let mut voice_defs: Vec<VoiceDefinition> = Vec::new();
    let combined_voice_refs: String;

    if let Some(rv) = refrain_own_voice {
        // Separate voice variables for stanza and refrain
        let stanza_var = format!("{}Stanza", base_voice_name);
        let refrain_var = format!("{}Refrain", base_voice_name);

        let stanza_content = indent_lines(stanza_voice.content.trim(), "  ");
        let refrain_content = indent_lines(rv.content.trim(), "  ");

        if is_refrain_first {
            combined_voice_refs = format!("\\{} \\{}", refrain_var, stanza_var);
            voice_defs.push(VoiceDefinition {
                var_name: refrain_var.clone(),
                content: refrain_content,
                include_global: true,
            });
            voice_defs.push(VoiceDefinition {
                var_name: stanza_var.clone(),
                content: ensure_final_bar(&stanza_content),
                include_global: false,
            });
        } else {
            combined_voice_refs = format!("\\{} \\{}", stanza_var, refrain_var);
            voice_defs.push(VoiceDefinition {
                var_name: stanza_var.clone(),
                content: stanza_content,
                include_global: true,
            });
            voice_defs.push(VoiceDefinition {
                var_name: refrain_var.clone(),
                content: ensure_final_bar(&refrain_content),
                include_global: false,
            });
        }
    } else {
        // Single voice variable (no independent refrain melody)
        voice_defs.push(VoiceDefinition {
            var_name: base_voice_name.clone(),
            content: ensure_final_bar(&indent_lines(stanza_voice.content.trim(), "  ")),
            include_global: true,
        });
        combined_voice_refs = format!("\\{}", base_voice_name);
    }

    // --- Step 4: Build global content (key, time, partial) ---
    let global_content = build_global_content(song);

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

                // For the first verse, embed refrain lyrics if the refrain has
                // its own melody. The position (before/after stanza lyrics)
                // depends on the song order.
                let mut lyrics_text = content.content.clone();
                if is_first_verse {
                    if let Some(ref refrain_lyrics) = refrain_lyrics_for_embedding {
                        if is_refrain_first {
                            lyrics_text = format!(
                                "{}\n\n{}",
                                refrain_lyrics.trim(),
                                lyrics_text.trim()
                            );
                        } else {
                            lyrics_text = format!(
                                "{}\n\n{}",
                                lyrics_text.trim(),
                                refrain_lyrics.trim()
                            );
                        }
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
        voice_defs,
        combined_voice_refs,
        voice_part_name,
        voice_part_ref,
        midi_instrument: "choir aahs".to_string(),
        verses,
        staff_size: settings.staff_size,
        font_block: build_font_block(&settings.font),
    };

    let handlebars = Handlebars::new();
    handlebars
        .render_template(LILYPOND_TEMPLATE, &data)
        .map_err(|e| format!("Template rendering failed: {}", e))
}

// ---------------------------------------------------------------------------
// Sequential export
// ---------------------------------------------------------------------------

/// Generate a **sequential** LilyPond (.ly) file from a Song.
///
/// The output contains one `\score` block for every part in the singing order
/// (e.g. stanza 1, refrain, stanza 2, refrain, …). Voice and lyrics definitions
/// are shared via LilyPond variables and referenced from each score block.
///
/// Returns an error if the song has no voice content to export.
pub fn lilypond_sequential_from_song(
    song: &Song,
    settings: &LilypondSettings,
) -> Result<String, String> {
    let parts = song.get_unpacked_parts();

    // --- Find the stanza (verse) voice ---
    let stanza_voice = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Verse)
        .find_map(|part| song.get_voice_for_part(part))
        .ok_or_else(|| "Song has no voice content for LilyPond export".to_string())?;

    let base_voice_name = voice_type_to_var_name(&stanza_voice.voice_type).to_string();

    // --- Check refrain/chorus parts for their own independent voice ---
    let refrain_parts: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Refrain || p.part_type == SongPartType::Chorus)
        .collect();

    let refrain_own_voice: Option<&SongPartContent> =
        refrain_parts.iter().find_map(|part| find_own_voice(part));

    // --- Build voice definitions ---
    let mut voice_defs: Vec<VoiceDefinition> = Vec::new();
    let stanza_voice_ref: String;
    let refrain_voice_ref: Option<String>;

    if let Some(rv) = refrain_own_voice {
        let stanza_var = format!("{}Stanza", base_voice_name);
        let refrain_var = format!("{}Refrain", base_voice_name);

        stanza_voice_ref = format!("\\{}", stanza_var);
        refrain_voice_ref = Some(format!("\\{}", refrain_var));

        voice_defs.push(VoiceDefinition {
            var_name: stanza_var,
            content: ensure_final_bar(&indent_lines(stanza_voice.content.trim(), "  ")),
            include_global: true,
        });
        voice_defs.push(VoiceDefinition {
            var_name: refrain_var,
            content: ensure_final_bar(&indent_lines(rv.content.trim(), "  ")),
            include_global: false,
        });
    } else {
        voice_defs.push(VoiceDefinition {
            var_name: base_voice_name.clone(),
            content: ensure_final_bar(&indent_lines(stanza_voice.content.trim(), "  ")),
            include_global: true,
        });
        stanza_voice_ref = format!("\\{}", base_voice_name);
        refrain_voice_ref = None;
    }

    // --- Build lyrics definitions ---
    let mut lyrics_defs: Vec<LyricsDefinition> = Vec::new();

    // Collect verse lyrics
    let mut verse_parts_sorted: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Verse)
        .collect();
    verse_parts_sorted.sort_by_key(|p| p.number);

    let mut verse_var_refs: Vec<(u32, String)> = Vec::new(); // (verse_number, var_ref)
    let mut verse_number: u32 = 1;

    for part in &verse_parts_sorted {
        for content in &part.contents {
            if content.voice_type.is_lyrics() {
                let var_name = format!("verse{}", number_to_word(verse_number));
                let var_ref = format!("\\{}", var_name);
                verse_var_refs.push((verse_number, var_ref));
                lyrics_defs.push(LyricsDefinition {
                    var_name,
                    stanza: Some(format!("{}.", verse_number)),
                    content: indent_lines(&content.content, "  "),
                });
                verse_number += 1;
            }
        }
    }

    // Collect refrain lyrics
    let mut refrain_lyrics_ref: Option<String> = None;
    for part in &refrain_parts {
        for content in &part.contents {
            if content.voice_type.is_lyrics() && refrain_lyrics_ref.is_none() {
                let var_name = "refrainLyrics".to_string();
                let var_ref = format!("\\{}", var_name);
                refrain_lyrics_ref = Some(var_ref);
                lyrics_defs.push(LyricsDefinition {
                    var_name,
                    stanza: None,
                    content: indent_lines(&content.content, "  "),
                });
            }
        }
    }

    // --- Build the singing order sections ---
    let is_refrain_first = song
        .part_orders
        .first()
        .map_or(false, |o| o.is_refrain_first());

    let r_voice_ref = refrain_voice_ref
        .as_deref()
        .unwrap_or(&stanza_voice_ref);
    let midi_instrument = "choir aahs".to_string();

    let mut sections: Vec<SequentialSection> = Vec::new();

    if is_refrain_first {
        if let Some(ref r_lyrics_ref) = refrain_lyrics_ref {
            sections.push(SequentialSection {
                label: "Refrain".to_string(),
                voice_ref: r_voice_ref.to_string(),
                lyrics_ref: r_lyrics_ref.clone(),
                midi_instrument: midi_instrument.clone(),
            });
        }
    }

    for (vnum, v_lyrics_ref) in &verse_var_refs {
        sections.push(SequentialSection {
            label: format!("Stanza {}", vnum),
            voice_ref: stanza_voice_ref.clone(),
            lyrics_ref: v_lyrics_ref.clone(),
            midi_instrument: midi_instrument.clone(),
        });
        if let Some(ref r_lyrics_ref) = refrain_lyrics_ref {
            sections.push(SequentialSection {
                label: "Refrain".to_string(),
                voice_ref: r_voice_ref.to_string(),
                lyrics_ref: r_lyrics_ref.clone(),
                midi_instrument: midi_instrument.clone(),
            });
        }
    }

    // --- Build global content and render ---
    let global_content = build_global_content(song);

    let data = SequentialTemplateData {
        version: "2.24.0".to_string(),
        title: song.title.clone(),
        composer: song.get_tag("author").cloned(),
        paper_size: settings.paper_size.clone(),
        layout_indent: settings.layout_indent.clone(),
        global_content,
        voice_defs,
        lyrics_defs,
        sections,
        staff_size: settings.staff_size,
        font_block: build_font_block(&settings.font),
    };

    let handlebars = Handlebars::new();
    handlebars
        .render_template(LILYPOND_SEQUENTIAL_TEMPLATE, &data)
        .map_err(|e| format!("Template rendering failed: {}", e))
}

// ---------------------------------------------------------------------------
// Per-part export
// ---------------------------------------------------------------------------

/// Generate standalone LilyPond (.ly) files for each part in singing order.
///
/// Each returned [`LilypondPart`] contains a complete, self-contained `.ly`
/// file that can be compiled independently to produce a cropped SVG for that
/// song part.
pub fn lilypond_parts_from_song(
    song: &Song,
    settings: &LilypondSettings,
) -> Result<Vec<LilypondPart>, String> {
    let parts = song.get_unpacked_parts();

    // --- Find the stanza (verse) voice ---
    let stanza_voice = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Verse)
        .find_map(|part| song.get_voice_for_part(part))
        .ok_or_else(|| "Song has no voice content for LilyPond export".to_string())?;

    let base_voice_name = voice_type_to_var_name(&stanza_voice.voice_type).to_string();

    // --- Check refrain/chorus for own voice ---
    let refrain_parts: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Refrain || p.part_type == SongPartType::Chorus)
        .collect();

    let refrain_own_voice: Option<&SongPartContent> =
        refrain_parts.iter().find_map(|part| find_own_voice(part));

    // --- Determine voice content for stanza and refrain ---
    let stanza_voice_content =
        ensure_final_bar(&indent_lines(stanza_voice.content.trim(), "  "));
    let stanza_voice_var = if refrain_own_voice.is_some() {
        format!("{}Stanza", base_voice_name)
    } else {
        base_voice_name.clone()
    };

    let refrain_voice_var: String;
    let refrain_voice_content: String;
    if let Some(rv) = refrain_own_voice {
        refrain_voice_var = format!("{}Refrain", base_voice_name);
        refrain_voice_content =
            ensure_final_bar(&indent_lines(rv.content.trim(), "  "));
    } else {
        refrain_voice_var = stanza_voice_var.clone();
        refrain_voice_content = stanza_voice_content.clone();
    }

    // --- Collect verse lyrics ---
    let mut verse_parts_sorted: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Verse)
        .collect();
    verse_parts_sorted.sort_by_key(|p| p.number);

    let mut verse_lyrics: Vec<(u32, String)> = Vec::new(); // (number, content)
    let mut verse_number: u32 = 1;
    for part in &verse_parts_sorted {
        for content in &part.contents {
            if content.voice_type.is_lyrics() {
                verse_lyrics.push((verse_number, content.content.clone()));
                verse_number += 1;
            }
        }
    }

    // --- Collect refrain lyrics ---
    let refrain_lyrics: Option<String> = refrain_parts
        .iter()
        .find_map(|part| find_lyrics(part))
        .map(|c| c.content.clone());

    // --- Build global content ---
    let global_content = build_global_content(song);

    // --- Build singing order ---
    let is_refrain_first = song
        .part_orders
        .first()
        .map_or(false, |o| o.is_refrain_first());

    let handlebars = Handlebars::new();
    let font_block = build_font_block(&settings.font);

    let mut result: Vec<LilypondPart> = Vec::new();

    // Helper closure to create a part LY from a voice/lyrics pair
    let render_part = |voice_var: &str,
                       voice_content: &str,
                       lyrics_var: &str,
                       lyrics_content: &str,
                       stanza: Option<String>|
     -> Result<String, String> {
        let data = PartTemplateData {
            version: "2.24.0".to_string(),
            global_content: global_content.clone(),
            voice_var_name: voice_var.to_string(),
            voice_content: voice_content.to_string(),
            voice_ref: format!("\\{}", voice_var),
            lyrics_var_name: lyrics_var.to_string(),
            lyrics_content: indent_lines(lyrics_content, "  "),
            lyrics_ref: format!("\\{}", lyrics_var),
            stanza,
            staff_size: settings.staff_size,
            font_block: font_block.clone(),
        };
        handlebars
            .render_template(LILYPOND_PART_TEMPLATE, &data)
            .map_err(|e| format!("Template rendering failed: {}", e))
    };

    if is_refrain_first {
        if let Some(ref r_lyrics) = refrain_lyrics {
            let ly = render_part(
                &refrain_voice_var,
                &refrain_voice_content,
                "refrainLyrics",
                r_lyrics,
                None,
            )?;
            result.push(LilypondPart {
                label: "Refrain".to_string(),
                ly_content: ly,
            });
        }
    }

    for (vnum, v_lyrics) in &verse_lyrics {
        let lyrics_var = format!("verse{}", number_to_word(*vnum));
        let ly = render_part(
            &stanza_voice_var,
            &stanza_voice_content,
            &lyrics_var,
            v_lyrics,
            Some(format!("{}.", vnum)),
        )?;
        result.push(LilypondPart {
            label: format!("Stanza {}", vnum),
            ly_content: ly,
        });

        if let Some(ref r_lyrics) = refrain_lyrics {
            let ly = render_part(
                &refrain_voice_var,
                &refrain_voice_content,
                "refrainLyrics",
                r_lyrics,
                None,
            )?;
            result.push(LilypondPart {
                label: "Refrain".to_string(),
                ly_content: ly,
            });
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// LilyPond rendering (SVG / PDF via the LilyPond binary)
// ---------------------------------------------------------------------------

/// A rendered song part with its label and SVG content.
pub struct RenderedPart {
    /// Human-readable label (e.g. "Stanza 1", "Refrain")
    pub label: String,
    /// The SVG content as bytes
    pub svg: Vec<u8>,
}

/// Render a LilyPond string to a **cropped SVG** via the LilyPond binary.
///
/// # Arguments
/// * `ly_content` – Complete `.ly` file content
/// * `lilypond_bin` – Path to the `lilypond` executable
///
/// # Returns
/// The SVG file content as a byte vector.
pub fn render_lilypond_to_svg(ly_content: &str, lilypond_bin: &Path) -> Result<Vec<u8>, String> {
    let temp_dir =
        tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let ly_path = temp_dir.path().join("input.ly");
    let out_base = temp_dir.path().join("output");

    std::fs::write(&ly_path, ly_content)
        .map_err(|e| format!("Failed to write LY file: {}", e))?;

    let output = std::process::Command::new(lilypond_bin)
        .arg("-dcrop")
        .arg("-dbackend=svg")
        .arg("-o")
        .arg(&out_base)
        .arg(&ly_path)
        .output()
        .map_err(|e| format!("Failed to run LilyPond: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "LilyPond failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // LilyPond with -dcrop generates output.cropped.svg
    let svg_path = temp_dir.path().join("output.cropped.svg");
    if svg_path.exists() {
        return std::fs::read(&svg_path)
            .map_err(|e| format!("Failed to read SVG output: {}", e));
    }

    // Fallback: try output.svg (older LilyPond versions)
    let svg_fallback = temp_dir.path().join("output.svg");
    std::fs::read(&svg_fallback)
        .map_err(|e| format!("Failed to read SVG output: {}", e))
}

/// Render a LilyPond string to **PDF** via the LilyPond binary.
///
/// # Arguments
/// * `ly_content` – Complete `.ly` file content
/// * `lilypond_bin` – Path to the `lilypond` executable
///
/// # Returns
/// The PDF file content as a byte vector.
pub fn render_lilypond_to_pdf(ly_content: &str, lilypond_bin: &Path) -> Result<Vec<u8>, String> {
    let temp_dir =
        tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let ly_path = temp_dir.path().join("input.ly");
    let out_base = temp_dir.path().join("output");

    std::fs::write(&ly_path, ly_content)
        .map_err(|e| format!("Failed to write LY file: {}", e))?;

    let output = std::process::Command::new(lilypond_bin)
        .arg("-o")
        .arg(&out_base)
        .arg(&ly_path)
        .output()
        .map_err(|e| format!("Failed to run LilyPond: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "LilyPond failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let pdf_path = temp_dir.path().join("output.pdf");
    std::fs::read(&pdf_path)
        .map_err(|e| format!("Failed to read PDF output: {}", e))
}

/// Render each song part as a **cropped SVG** via LilyPond.
///
/// Generates a standalone `.ly` file for each part in singing order, compiles
/// each one with LilyPond, and returns the cropped SVG for every part.
///
/// # Arguments
/// * `song` – The song to render
/// * `settings` – LilyPond export settings
/// * `lilypond_bin` – Path to the `lilypond` executable
pub fn render_song_parts_to_svg(
    song: &Song,
    settings: &LilypondSettings,
    lilypond_bin: &Path,
) -> Result<Vec<RenderedPart>, String> {
    let ly_parts = lilypond_parts_from_song(song, settings)?;
    let mut rendered: Vec<RenderedPart> = Vec::new();

    for part in ly_parts {
        let svg = render_lilypond_to_svg(&part.ly_content, lilypond_bin)?;
        rendered.push(RenderedPart {
            label: part.label,
            svg,
        });
    }

    Ok(rendered)
}

/// Render the **paper score** of a song as SVG via LilyPond.
///
/// # Arguments
/// * `song` – The song to render
/// * `settings` – LilyPond export settings (paper_size controls the SVG dimensions)
/// * `lilypond_bin` – Path to the `lilypond` executable
pub fn render_paper_score_to_svg(
    song: &Song,
    settings: &LilypondSettings,
    lilypond_bin: &Path,
) -> Result<Vec<u8>, String> {
    let ly_content = lilypond_from_song(song, settings)?;
    render_lilypond_to_svg(&ly_content, lilypond_bin)
}

/// Render the **paper score** of a song as PDF via LilyPond.
///
/// # Arguments
/// * `song` – The song to render
/// * `settings` – LilyPond export settings
/// * `lilypond_bin` – Path to the `lilypond` executable
pub fn render_paper_score_to_pdf(
    song: &Song,
    settings: &LilypondSettings,
    lilypond_bin: &Path,
) -> Result<Vec<u8>, String> {
    let ly_content = lilypond_from_song(song, settings)?;
    render_lilypond_to_pdf(&ly_content, lilypond_bin)
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

        // Separate voice variables for stanza and refrain
        assert!(
            ly_output.contains("sopranoVoiceStanza = \\relative c'"),
            "Stanza voice variable missing"
        );
        assert!(
            ly_output.contains("sopranoVoiceRefrain = \\relative c'"),
            "Refrain voice variable missing"
        );

        // Both melodies should be present
        assert!(
            ly_output.contains("d8 e | fis4 fis g4 fis8 e"),
            "Stanza melody missing"
        );
        assert!(
            ly_output.contains("fis8( g ) | a8 a a a d,4. d8"),
            "Refrain melody missing"
        );

        // Staff should reference both voice variables in stanza-refrain order
        assert!(
            ly_output.contains("\\sopranoVoiceStanza \\sopranoVoiceRefrain"),
            "Staff should combine stanza and refrain voices"
        );

        // There should NOT be a single combined sopranoVoice variable
        assert!(
            !ly_output.contains("sopranoVoice = \\relative c'"),
            "Should not have a single combined sopranoVoice"
        );

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
            ..LilypondSettings::default()
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

    #[test]
    fn test_lilypond_export_refrain_stanza_refrain() {
        // Test refrain-stanza-refrain ordering: refrain melody comes first
        let yml = r#"
version: 0.1
title: Refrain First Song
default_language: en
tags:
  author: Test Author
score:
  key: c major
  time: 4/4
orders:
  - refrain-stanza-refrain
parts:
  - type: refrain
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f | g2 g2
    - type: lyrics
      number: 1
      content: |
        Refrain text here, la la la la
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        e4 f g a | b2 b2
    - type: lyrics
      number: 1
      content: |
        First verse lyrics here
    - type: lyrics
      number: 2
      content: |
        Second verse lyrics here
"#;
        let song = song_yml::import_from_yml_string(yml).unwrap();
        let ly_output = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();

        // Separate voice variables for refrain and stanza
        assert!(
            ly_output.contains("sopranoVoiceRefrain = \\relative c'"),
            "Refrain voice variable missing"
        );
        assert!(
            ly_output.contains("sopranoVoiceStanza = \\relative c'"),
            "Stanza voice variable missing"
        );

        // Staff should reference refrain BEFORE stanza (refrain-first order)
        assert!(
            ly_output.contains("\\sopranoVoiceRefrain \\sopranoVoiceStanza"),
            "Staff should have refrain before stanza for refrain-first songs"
        );

        // Verse 1 lyrics should have refrain lyrics BEFORE stanza lyrics
        assert!(ly_output.contains("verseOne = \\lyricmode"));
        // The refrain lyrics should appear before the stanza lyrics in verse 1
        let verse_one_start = ly_output.find("verseOne = \\lyricmode").unwrap();
        let verse_two_start = ly_output.find("verseTwo = \\lyricmode").unwrap();
        let verse_one_block = &ly_output[verse_one_start..verse_two_start];
        let refrain_pos = verse_one_block.find("Refrain text").unwrap();
        let stanza_pos = verse_one_block.find("First verse").unwrap();
        assert!(
            refrain_pos < stanza_pos,
            "Refrain lyrics should come before stanza lyrics in verse 1 for refrain-first songs"
        );

        // Verse 2 should only contain stanza lyrics (no refrain)
        let verse_two_end = ly_output[verse_two_start..].find("\n}\n").unwrap();
        let verse_two_block = &ly_output[verse_two_start..verse_two_start + verse_two_end];
        assert!(
            !verse_two_block.contains("Refrain text"),
            "Verse 2 should not contain refrain lyrics"
        );
        assert!(
            verse_two_block.contains("Second verse"),
            "Verse 2 should contain its own stanza lyrics"
        );

        // There should be exactly 2 \addlyrics (one per verse)
        let addlyrics_count = ly_output.matches("\\addlyrics").count();
        assert_eq!(
            addlyrics_count, 2,
            "Expected 2 addlyrics, got {}",
            addlyrics_count
        );
    }

    #[test]
    fn test_ensure_final_bar() {
        // Should add \bar "|." when not present
        assert_eq!(
            ensure_final_bar("  c4 d e f"),
            "  c4 d e f \\bar \"|.\""
        );
        // Should not duplicate when already present
        assert_eq!(
            ensure_final_bar("  f2. \\bar \"|.\""),
            "  f2. \\bar \"|.\""
        );
        // Should handle trailing whitespace
        assert_eq!(
            ensure_final_bar("  c4 d e f  \n"),
            "  c4 d e f \\bar \"|.\""
        );
    }

    #[test]
    fn test_final_bar_added_multi_section_melody() {
        // "Sei nicht stolz" has separate stanza and refrain voices; the stanza voice
        // does NOT end with \bar "|." in the source, but the export should add it.
        let content = std::fs::read_to_string(
            "testfiles/Sei nicht stolz auf das, was du bist.song.yml",
        )
        .unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();
        let ly_output = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();

        // The last voice definition (refrain) should end with \bar "|."
        // Ensure that the final occurrence of \bar "|." appears after the last
        // voice variable header (sopranoVoiceRefrain), so we fail if the bar
        // line is added in the wrong place.
        let last_voice_header = "sopranoVoiceRefrain = \\relative c'";
        let last_voice_start = ly_output
            .find(last_voice_header)
            .expect("Last voice header (refrain) not found in LilyPond output");
        let final_bar_pos = ly_output
            .rfind("\\bar \"|.\"")
            .expect("LilyPond output should contain final bar line");
        assert!(
            final_bar_pos > last_voice_start,
            "Final bar line should appear after the last voice definition header"
        );
    }

    #[test]
    fn test_final_bar_not_duplicated() {
        // Amazing Grace already has \bar "|." in source - should not be duplicated
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();
        let ly_output = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();

        let bar_count = ly_output.matches("\\bar \"|.\"").count();
        assert_eq!(
            bar_count, 1,
            "Expected exactly 1 final bar, got {}",
            bar_count
        );
    }

    #[test]
    fn test_global_only_in_first_voice() {
        // "Sei nicht stolz" has separate stanza and refrain voices.
        // \global should only appear in the first voice definition.
        let content = std::fs::read_to_string(
            "testfiles/Sei nicht stolz auf das, was du bist.song.yml",
        )
        .unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();
        let ly_output = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();

        // The stanza voice (first in stanza-refrain order) should have \global
        let stanza_start = ly_output.find("sopranoVoiceStanza = \\relative c'").unwrap();
        let refrain_start = ly_output.find("sopranoVoiceRefrain = \\relative c'").unwrap();
        let stanza_block = &ly_output[stanza_start..refrain_start];
        assert!(
            stanza_block.contains("\\global"),
            "First voice (stanza) should include \\global"
        );

        // The refrain voice (second) should NOT have \global
        let refrain_end = ly_output[refrain_start..].find("\n}\n").unwrap();
        let refrain_block = &ly_output[refrain_start..refrain_start + refrain_end];
        assert!(
            !refrain_block.contains("\\global"),
            "Second voice (refrain) should NOT include \\global"
        );
    }

    // ===================================================================
    // Sequential export tests
    // ===================================================================

    #[test]
    fn test_sequential_export_simple_verses() {
        // Amazing Grace: verses only → each verse gets its own \score
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let ly = lilypond_sequential_from_song(&song, &LilypondSettings::default()).unwrap();

        // Version and header
        assert!(ly.contains("\\version \"2.24.0\""));
        assert!(ly.contains("title = \"Amazing Grace\""));

        // Global variable
        assert!(ly.contains("global = {"));
        assert!(ly.contains("\\key f \\major"));

        // Voice definition
        assert!(ly.contains("sopranoVoice = \\relative c'"));

        // Verse lyrics as separate variables
        assert!(ly.contains("verseOne = \\lyricmode"));
        assert!(ly.contains("verseTwo = \\lyricmode"));
        assert!(ly.contains("verseThree = \\lyricmode"));

        // Three \score blocks (one per verse)
        let score_count = ly.matches("\\score {").count();
        assert_eq!(score_count, 3, "Expected 3 score blocks, got {}", score_count);

        // Section labels
        assert!(ly.contains("piece = \"Stanza 1\""));
        assert!(ly.contains("piece = \"Stanza 2\""));
        assert!(ly.contains("piece = \"Stanza 3\""));

        // Each score should reference the voice
        assert!(ly.contains("\\sopranoVoice"));

        // Should NOT have embedded refrain lyrics in verse
        // (sequential mode keeps them separate)
        assert!(
            !ly.contains("\\addlyrics { \\verseOne }\\n\\addlyrics"),
            "Sequential mode should not combine all addlyrics"
        );
    }

    #[test]
    fn test_sequential_export_stanza_refrain() {
        // "Sei nicht stolz" has stanza + refrain with separate melodies
        let content = std::fs::read_to_string(
            "testfiles/Sei nicht stolz auf das, was du bist.song.yml",
        )
        .unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let ly = lilypond_sequential_from_song(&song, &LilypondSettings::default()).unwrap();

        // Separate voice variables
        assert!(
            ly.contains("sopranoVoiceStanza = \\relative c'"),
            "Stanza voice variable missing"
        );
        assert!(
            ly.contains("sopranoVoiceRefrain = \\relative c'"),
            "Refrain voice variable missing"
        );

        // Separate lyrics variables
        assert!(ly.contains("verseOne = \\lyricmode"));
        assert!(ly.contains("verseTwo = \\lyricmode"));
        assert!(ly.contains("verseThree = \\lyricmode"));
        assert!(
            ly.contains("refrainLyrics = \\lyricmode"),
            "Refrain lyrics variable missing"
        );

        // 6 score blocks: stanza1, refrain, stanza2, refrain, stanza3, refrain
        let score_count = ly.matches("\\score {").count();
        assert_eq!(score_count, 6, "Expected 6 score blocks, got {}", score_count);

        // Section labels
        assert!(ly.contains("piece = \"Stanza 1\""));
        assert!(ly.contains("piece = \"Refrain\""));
        assert!(ly.contains("piece = \"Stanza 2\""));
        assert!(ly.contains("piece = \"Stanza 3\""));

        // Refrain sections should reference the refrain voice
        // Find the first refrain section and check it references refrain voice
        let refrain_section_pos = ly.find("piece = \"Refrain\"").unwrap();
        let section_start = ly[..refrain_section_pos].rfind("\\score {").unwrap();
        let section_block = &ly[section_start..refrain_section_pos + 50];
        assert!(
            section_block.contains("\\sopranoVoiceRefrain"),
            "Refrain score block should reference refrain voice"
        );
    }

    #[test]
    fn test_sequential_export_refrain_first() {
        let yml = r#"
version: 0.1
title: Refrain First Song
default_language: en
tags:
  author: Test Author
score:
  key: c major
  time: 4/4
orders:
  - refrain-stanza-refrain
parts:
  - type: refrain
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f | g2 g2
    - type: lyrics
      number: 1
      content: |
        Refrain text here, la la la la
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        e4 f g a | b2 b2
    - type: lyrics
      number: 1
      content: |
        First verse lyrics here
    - type: lyrics
      number: 2
      content: |
        Second verse lyrics here
"#;
        let song = song_yml::import_from_yml_string(yml).unwrap();
        let ly = lilypond_sequential_from_song(&song, &LilypondSettings::default()).unwrap();

        // 5 score blocks: refrain, stanza1, refrain, stanza2, refrain
        let score_count = ly.matches("\\score {").count();
        assert_eq!(score_count, 5, "Expected 5 score blocks, got {}", score_count);

        // The first score block should be the refrain
        let first_piece = ly.find("piece = ").unwrap();
        let first_piece_block = &ly[first_piece..first_piece + 30];
        assert!(
            first_piece_block.contains("Refrain"),
            "First section should be Refrain for refrain-first songs"
        );
    }

    #[test]
    fn test_sequential_no_refrain_lyrics_when_no_refrain() {
        // Amazing Grace has no refrain → no refrainLyrics variable
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let ly = lilypond_sequential_from_song(&song, &LilypondSettings::default()).unwrap();

        assert!(
            !ly.contains("refrainLyrics"),
            "Should not have refrain lyrics variable when no refrain exists"
        );
    }

    // ===================================================================
    // Per-part export tests
    // ===================================================================

    #[test]
    fn test_parts_export_simple_verses() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let parts = lilypond_parts_from_song(&song, &LilypondSettings::default()).unwrap();

        // 3 verses → 3 parts
        assert_eq!(parts.len(), 3, "Expected 3 parts, got {}", parts.len());

        // Check labels
        assert_eq!(parts[0].label, "Stanza 1");
        assert_eq!(parts[1].label, "Stanza 2");
        assert_eq!(parts[2].label, "Stanza 3");

        // Each part should be a standalone LY file
        for part in &parts {
            assert!(part.ly_content.contains("\\version \"2.24.0\""));
            assert!(part.ly_content.contains("\\score {"));
            assert!(part.ly_content.contains("global = {"));
            assert!(part.ly_content.contains("\\relative c'"));
        }

        // First part should have stanza 1 lyrics
        assert!(parts[0].ly_content.contains("A -- ma -- zing grace"));
        assert!(parts[0].ly_content.contains("\\set stanza = \"1.\""));

        // Second part should have stanza 2 lyrics
        assert!(parts[1].ly_content.contains("Twas grace"));
        assert!(parts[1].ly_content.contains("\\set stanza = \"2.\""));
    }

    #[test]
    fn test_parts_export_stanza_refrain() {
        let content = std::fs::read_to_string(
            "testfiles/Sei nicht stolz auf das, was du bist.song.yml",
        )
        .unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let parts = lilypond_parts_from_song(&song, &LilypondSettings::default()).unwrap();

        // 3 stanzas + 3 refrains = 6 parts
        assert_eq!(parts.len(), 6, "Expected 6 parts, got {}", parts.len());

        assert_eq!(parts[0].label, "Stanza 1");
        assert_eq!(parts[1].label, "Refrain");
        assert_eq!(parts[2].label, "Stanza 2");
        assert_eq!(parts[3].label, "Refrain");
        assert_eq!(parts[4].label, "Stanza 3");
        assert_eq!(parts[5].label, "Refrain");

        // Stanza parts should use stanza voice
        assert!(parts[0].ly_content.contains("sopranoVoiceStanza"));
        assert!(parts[0].ly_content.contains("Sei nicht stolz"));

        // Refrain parts should use refrain voice
        assert!(parts[1].ly_content.contains("sopranoVoiceRefrain"));
        assert!(parts[1].ly_content.contains("Denn wer sich"));

        // Refrain parts should NOT have a stanza marker
        assert!(!parts[1].ly_content.contains("\\set stanza"));
    }

    #[test]
    fn test_parts_export_refrain_first() {
        let yml = r#"
version: 0.1
title: Refrain First Song
default_language: en
tags:
  author: Test Author
score:
  key: c major
  time: 4/4
orders:
  - refrain-stanza-refrain
parts:
  - type: refrain
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f | g2 g2
    - type: lyrics
      number: 1
      content: |
        Refrain lyrics
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        e4 f g a | b2 b2
    - type: lyrics
      number: 1
      content: |
        First verse
"#;
        let song = song_yml::import_from_yml_string(yml).unwrap();
        let parts = lilypond_parts_from_song(&song, &LilypondSettings::default()).unwrap();

        // refrain, stanza1, refrain = 3 parts
        assert_eq!(parts.len(), 3, "Expected 3 parts, got {}", parts.len());
        assert_eq!(parts[0].label, "Refrain");
        assert_eq!(parts[1].label, "Stanza 1");
        assert_eq!(parts[2].label, "Refrain");
    }

    #[test]
    fn test_parts_each_is_standalone() {
        // Each part LY should be independently compilable
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let parts = lilypond_parts_from_song(&song, &LilypondSettings::default()).unwrap();

        for (i, part) in parts.iter().enumerate() {
            // Must have version
            assert!(
                part.ly_content.contains("\\version"),
                "Part {} ({}) missing \\version",
                i,
                part.label
            );
            // Must have global
            assert!(
                part.ly_content.contains("global = {"),
                "Part {} ({}) missing global",
                i,
                part.label
            );
            // Must have exactly one \score
            let score_count = part.ly_content.matches("\\score {").count();
            assert_eq!(
                score_count, 1,
                "Part {} ({}) should have exactly 1 score, got {}",
                i, part.label, score_count
            );
            // Must have key signature
            assert!(
                part.ly_content.contains("\\key f \\major"),
                "Part {} ({}) missing key signature",
                i,
                part.label
            );
        }
    }

    // ===================================================================
    // Font and staff size setting tests
    // ===================================================================

    #[test]
    fn test_font_setting_default_no_font_block() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let ly = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();

        // Default font should not produce a #(define fonts ...) block
        assert!(
            !ly.contains("define fonts"),
            "Default font should not produce a fonts block"
        );
        // Should not have staff size override
        assert!(
            !ly.contains("set-global-staff-size"),
            "Default should not set staff size"
        );
    }

    #[test]
    fn test_font_setting_specific() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = LilypondSettings {
            font: FontSetting::Specific {
                family: "Times New Roman".to_string(),
            },
            ..LilypondSettings::default()
        };

        let ly = lilypond_from_song(&song, &settings).unwrap();

        assert!(
            ly.contains("#(define fonts"),
            "Specific font should produce a fonts block"
        );
        assert!(
            ly.contains("Times New Roman"),
            "Font family should appear in output"
        );
    }

    #[test]
    fn test_staff_size_setting() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = LilypondSettings {
            staff_size: Some(18.0),
            ..LilypondSettings::default()
        };

        let ly = lilypond_from_song(&song, &settings).unwrap();

        assert!(
            ly.contains("#(set-global-staff-size 18"),
            "Staff size should appear in output"
        );
    }

    #[test]
    fn test_font_and_staff_size_in_sequential() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = LilypondSettings {
            font: FontSetting::Specific {
                family: "Helvetica".to_string(),
            },
            staff_size: Some(16.0),
            ..LilypondSettings::default()
        };

        let ly = lilypond_sequential_from_song(&song, &settings).unwrap();

        assert!(
            ly.contains("#(set-global-staff-size 16"),
            "Staff size should appear in sequential output"
        );
        assert!(
            ly.contains("Helvetica"),
            "Font family should appear in sequential output"
        );
    }

    #[test]
    fn test_font_and_staff_size_in_parts() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = LilypondSettings {
            font: FontSetting::Specific {
                family: "Garamond".to_string(),
            },
            staff_size: Some(22.0),
            ..LilypondSettings::default()
        };

        let parts = lilypond_parts_from_song(&song, &settings).unwrap();

        for part in &parts {
            assert!(
                part.ly_content.contains("#(set-global-staff-size 22"),
                "Staff size should appear in part output"
            );
            assert!(
                part.ly_content.contains("Garamond"),
                "Font family should appear in part output"
            );
        }
    }

    // ===================================================================
    // Rendering tests (require LilyPond binary)
    // ===================================================================

    /// Try to find the LilyPond binary. Returns None if not available.
    fn find_lilypond_bin() -> Option<std::path::PathBuf> {
        std::process::Command::new("lilypond")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|_| std::path::PathBuf::from("lilypond"))
    }

    #[test]
    fn test_render_paper_score_svg() {
        let lilypond_bin = match find_lilypond_bin() {
            Some(bin) => bin,
            None => {
                eprintln!("Skipping test_render_paper_score_svg: lilypond not found");
                return;
            }
        };

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let svg = render_paper_score_to_svg(&song, &LilypondSettings::default(), &lilypond_bin)
            .unwrap();

        assert!(!svg.is_empty(), "SVG output should not be empty");
        let svg_str = String::from_utf8_lossy(&svg);
        assert!(
            svg_str.contains("<svg") || svg_str.contains("<?xml"),
            "Output should be valid SVG"
        );
    }

    #[test]
    fn test_render_paper_score_pdf() {
        let lilypond_bin = match find_lilypond_bin() {
            Some(bin) => bin,
            None => {
                eprintln!("Skipping test_render_paper_score_pdf: lilypond not found");
                return;
            }
        };

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let pdf = render_paper_score_to_pdf(&song, &LilypondSettings::default(), &lilypond_bin)
            .unwrap();

        assert!(!pdf.is_empty(), "PDF output should not be empty");
        // PDF files start with %PDF-
        assert!(
            pdf.starts_with(b"%PDF-"),
            "Output should be a valid PDF file"
        );
    }

    #[test]
    fn test_render_song_parts_svg() {
        let lilypond_bin = match find_lilypond_bin() {
            Some(bin) => bin,
            None => {
                eprintln!("Skipping test_render_song_parts_svg: lilypond not found");
                return;
            }
        };

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let rendered =
            render_song_parts_to_svg(&song, &LilypondSettings::default(), &lilypond_bin).unwrap();

        assert_eq!(rendered.len(), 3, "Expected 3 rendered parts");

        for part in &rendered {
            assert!(!part.svg.is_empty(), "SVG for {} should not be empty", part.label);
            let svg_str = String::from_utf8_lossy(&part.svg);
            assert!(
                svg_str.contains("<svg") || svg_str.contains("<?xml"),
                "Output for {} should be valid SVG",
                part.label
            );
        }
    }

    #[test]
    fn test_render_paper_score_svg_with_settings() {
        let lilypond_bin = match find_lilypond_bin() {
            Some(bin) => bin,
            None => {
                eprintln!(
                    "Skipping test_render_paper_score_svg_with_settings: lilypond not found"
                );
                return;
            }
        };

        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = LilypondSettings {
            paper_size: "a5".to_string(),
            staff_size: Some(16.0),
            ..LilypondSettings::default()
        };

        let svg = render_paper_score_to_svg(&song, &settings, &lilypond_bin).unwrap();

        assert!(!svg.is_empty(), "SVG output should not be empty");
    }

    #[test]
    fn test_render_stanza_refrain_parts_svg() {
        let lilypond_bin = match find_lilypond_bin() {
            Some(bin) => bin,
            None => {
                eprintln!(
                    "Skipping test_render_stanza_refrain_parts_svg: lilypond not found"
                );
                return;
            }
        };

        let content = std::fs::read_to_string(
            "testfiles/Sei nicht stolz auf das, was du bist.song.yml",
        )
        .unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let rendered =
            render_song_parts_to_svg(&song, &LilypondSettings::default(), &lilypond_bin).unwrap();

        // 3 stanzas + 3 refrains = 6 parts
        assert_eq!(rendered.len(), 6, "Expected 6 rendered parts");

        for part in &rendered {
            assert!(!part.svg.is_empty(), "SVG for {} should not be empty", part.label);
        }
    }
}
