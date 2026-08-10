//! SongBeamer exporter — writes a [`Song`] out as a `.sng` file.
//!
//! This is the counterpart of [`crate::importer::songbeamer`], which documents
//! the format. The two are meant to round trip: importing an exported file has
//! to give the same song back.
//!
//! ```text
//! #LangCount=1
//! #Title=Amazing Grace
//! #Author=John Newton
//! #Editor=cantara-songlib 0.2.4
//! #Version=3
//! #VerseOrder=Verse 1,Refrain
//! ---
//! Verse 1
//! Amazing grace, how sweet the sound
//! ---
//! Refrain
//! …
//! ```
//!
//! # What is written, and what is not
//!
//! * Metadata is mapped back onto the SongBeamer tag it came from. Tags this
//!   crate has that the format has no place for are left out rather than
//!   invented.
//! * A blank line inside a part becomes a `--` slide break, which is how the
//!   importer read one in.
//! * Lyrics are written the way an audience sees them: the LilyPond syllable
//!   markup (`--`, `_`, `\set …`) that the notation formats carry is stripped,
//!   since SongBeamer only ever shows text.
//! * **Notation is dropped.** The format stores no melody at all, so exporting
//!   a song with a score keeps its lyrics and loses its music.
//!
//! # Encoding
//!
//! [`sng_bytes_from_song`] writes UTF-8 with a byte order mark and CRLF line
//! endings — the combination SongBeamer itself writes, and the one that makes
//! umlauts survive on every version of the program. Use it rather than writing
//! [`sng_from_song`] out yourself.

use crate::exporter::slides::lyrics_for_reading;
use crate::importer::songbeamer::{classify_mark, key_from_lilypond};
use crate::song::{
    LyricLanguage, PartOrderRule, Song, SongPart, SongPartContent, SongPartType,
};

/// How to write a `.sng` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SngExportSettings {
    /// The languages to write, in the order they should be interleaved.
    ///
    /// Empty means: work them out from the song — its default language first,
    /// then the rest in alphabetical order.
    pub languages: Vec<String>,

    /// What to write into `#Editor`, the field naming the program that produced
    /// the file.
    pub editor: String,
}

impl Default for SngExportSettings {
    fn default() -> SngExportSettings {
        SngExportSettings {
            languages: Vec::new(),
            editor: format!("cantara-songlib {}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// The format version written into `#Version`. 3 is what SongBeamer 4 writes
/// and what every version in use reads.
const FORMAT_VERSION: u32 = 3;

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Export a song as the text of a `.sng` file, with CRLF line endings.
///
/// ```
/// use cantara_songlib::exporter::songbeamer::{sng_from_song, SngExportSettings};
/// use cantara_songlib::importer::songbeamer::import_from_sng_string;
///
/// let song = import_from_sng_string("#Title=Test\n---\nVerse 1\nline one\n").unwrap();
/// let sng = sng_from_song(&song, &SngExportSettings::default());
///
/// assert!(sng.starts_with("#LangCount=1\r\n#Title=Test\r\n"));
/// assert!(sng.contains("\r\n---\r\nVerse 1\r\nline one\r\n"));
/// ```
pub fn sng_from_song(song: &Song, settings: &SngExportSettings) -> String {
    let languages = languages_of(song, settings);

    let mut lines: Vec<String> = header_lines(song, &languages, settings);

    for part in song.parts() {
        lines.push("---".to_string());
        lines.push(mark_for(part));
        lines.extend(body_lines(song, part, &languages));
    }

    // A trailing newline: the file ends with a complete line, like every other
    // text file.
    let mut output = lines.join("\r\n");
    output.push_str("\r\n");
    output
}

/// Export a song as the bytes of a `.sng` file: UTF-8 with a byte order mark.
///
/// The BOM is not decoration — it is how SongBeamer tells the four encodings it
/// accepts apart. Without it, a file with umlauts is read as Windows-1252 and
/// shows mojibake.
///
/// ```
/// use cantara_songlib::exporter::songbeamer::{sng_bytes_from_song, SngExportSettings};
/// use cantara_songlib::importer::songbeamer::{import_from_bytes, import_from_sng_string};
///
/// let song = import_from_sng_string("#Title=Grüße\n---\nText\n").unwrap();
/// let bytes = sng_bytes_from_song(&song, &SngExportSettings::default());
///
/// assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
/// assert_eq!(import_from_bytes(&bytes).unwrap().title, "Grüße");
/// ```
pub fn sng_bytes_from_song(song: &Song, settings: &SngExportSettings) -> Vec<u8> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(sng_from_song(song, settings).as_bytes());
    bytes
}

// ---------------------------------------------------------------------------
// The header
// ---------------------------------------------------------------------------

/// Build the `#Tag=value` lines.
///
/// The order follows what SongBeamer itself writes: the language count first —
/// it decides how the body is read — then the song's identity, then the fields
/// that only matter to a program.
fn header_lines(
    song: &Song,
    languages: &[LyricLanguage],
    settings: &SngExportSettings,
) -> Vec<String> {
    /// Append one tag, unless it has nothing to say.
    ///
    /// A tag occupies exactly one line, so a value that carries a line break —
    /// a multi-line copyright, say — is folded into one line rather than
    /// breaking the header apart.
    fn push(lines: &mut Vec<String>, tag: &str, value: &str) {
        let value = value.replace(['\r', '\n'], " ");
        if !value.trim().is_empty() {
            lines.push(format!("#{}={}", tag, value.trim()));
        }
    }

    let mut lines: Vec<String> = Vec::new();

    push(&mut lines, "LangCount", &languages.len().max(1).to_string());
    push(&mut lines, "Title", &song.title);

    for (tag, key) in [
        ("OTitle", "original_title"),
        ("Author", "author"),
        ("Melody", "composer"),
        ("Translation", "translation"),
        ("(c)", "copyright"),
        ("NatCopyright", "national_copyright"),
        ("Rights", "rights"),
        ("CCLI", "ccli_song_number"),
        ("ChurchSongID", "church_song_id"),
        ("Bible", "bible"),
        ("Categories", "categories"),
        ("Keywords", "keywords"),
        ("Tempo", "tempo"),
    ] {
        if let Some(value) = song.tag(key) {
            push(&mut lines, tag, value);
        }
    }

    // A songbook reference is one field: "Book / 123".
    if let Some(book) = song.tag("songbook") {
        match song.tag("song_number") {
            Some(number) => push(&mut lines, "Songbook", &format!("{} / {}", book, number)),
            None => push(&mut lines, "Songbook", book),
        }
    }

    // The key the file came with wins over the one derived from the score, so
    // that a chord symbol SongBeamer wrote comes back unchanged.
    let key = song
        .tag("key")
        .cloned()
        .or_else(|| song.score.key.as_deref().and_then(key_from_lilypond));
    if let Some(key) = key {
        push(&mut lines, "Key", &key);
    }

    // Both of these are base64 in the format.
    for (tag, key) in [("Comments", "comments"), ("Chords", "songbeamer_chords")] {
        if let Some(value) = song.tag(key) {
            let encoded = crate::base64::encode(value.replace('\n', "\r").as_bytes());
            lines.push(format!("#{}={}", tag, encoded));
        }
    }

    push(&mut lines, "Editor", &settings.editor);
    lines.push(format!("#Version={}", FORMAT_VERSION));

    if let Some(order) = verse_order(song) {
        push(&mut lines, "VerseOrder", &order);
    }

    lines
}

/// Render the song's default singing order as a `#VerseOrder` value.
///
/// Only an explicit order is written. The rule-based orders are this crate's
/// idea of how a song is sung rather than something the file stated, and
/// writing them out would turn a guess into a fact.
fn verse_order(song: &Song) -> Option<String> {
    let order = song.part_orders.first()?;
    let PartOrderRule::Custom(ids) = order.rule() else {
        return None;
    };

    let entries: Vec<String> = ids
        .iter()
        .filter_map(|id| song.part(id))
        .map(mark_for)
        .collect();

    (!entries.is_empty()).then(|| entries.join(","))
}

// ---------------------------------------------------------------------------
// The body
// ---------------------------------------------------------------------------

/// The verse mark line for a part.
///
/// A label the importer would read back as *this very part* is reused verbatim,
/// so a file whose verses are called `Strophe 2` keeps them. Any other label
/// gets a mark built from the part's type instead: the type is what every
/// exporter and the singing order work from, so keeping it identifiable matters
/// more than keeping the wording — which for a song that came from another
/// format is a keyword of that format (`stanza`) rather than a heading anyone
/// wrote.
///
/// A part that has no type to state — [`SongPartType::Other`] — keeps its
/// wording through a `$$M=` custom mark, which is exactly what that mark is
/// for.
fn mark_for(part: &SongPart) -> String {
    if let Some(label) = &part.label {
        let matches_part = classify_mark(label).is_some_and(|mark| {
            mark.part_type == part.part_type
                && mark.custom_name.is_none()
                && mark.number.unwrap_or(1) == part.number
        });
        if matches_part {
            return label.trim().to_string();
        }

        if part.part_type == SongPartType::Other && !label.trim().is_empty() {
            return format!("$$M={}", label.trim());
        }
    }

    let keyword = match part.part_type {
        SongPartType::Verse => "Verse",
        SongPartType::Chorus => "Chorus",
        SongPartType::Refrain => "Refrain",
        SongPartType::PreChorus => "Pre-Chorus",
        SongPartType::PostChorus => "Post-Chorus",
        SongPartType::Bridge => "Bridge",
        SongPartType::Intro => "Intro",
        SongPartType::Outro => "Coda",
        SongPartType::Interlude => "Interlude",
        SongPartType::Instrumental => "Instrumental",
        SongPartType::Solo => "Solo",
        SongPartType::Other => "Part",
    };

    // Only a numbered section needs its number; a single refrain is just
    // "Refrain", which is how SongBeamer writes it.
    if part.number > 1 || part.part_type == SongPartType::Verse {
        format!("{} {}", keyword, part.number)
    } else {
        keyword.to_string()
    }
}

/// The lyrics of one part, with the languages interleaved line by line and
/// blank lines turned into `--` slide breaks.
fn body_lines(song: &Song, part: &SongPart, languages: &[LyricLanguage]) -> Vec<String> {
    // One entry per language: its slides, each a list of lines.
    let per_language: Vec<Vec<Vec<String>>> = languages
        .iter()
        .map(|language| slides_of(song, part, language))
        .collect();

    let slide_count = per_language
        .iter()
        .map(|slides| slides.len())
        .max()
        .unwrap_or(0);

    let mut lines: Vec<String> = Vec::new();
    for slide in 0..slide_count {
        if slide > 0 {
            lines.push("--".to_string());
        }

        // Inside a slide the languages take turns line by line; a language that
        // runs out contributes an empty line so that the turns stay aligned.
        let empty: Vec<String> = Vec::new();
        let slides: Vec<&Vec<String>> = per_language
            .iter()
            .map(|language| language.get(slide).unwrap_or(&empty))
            .collect();
        let line_count = slides.iter().map(|lines| lines.len()).max().unwrap_or(0);

        for line in 0..line_count {
            for language in &slides {
                lines.push(language.get(line).cloned().unwrap_or_default());
            }
        }
    }

    lines
}

/// One language's lyrics for a part, split into slides at blank lines.
fn slides_of(song: &Song, part: &SongPart, language: &LyricLanguage) -> Vec<Vec<String>> {
    let content = match language {
        // With a single unnamed language, take whatever lyrics the part has.
        LyricLanguage::Default => part.lyrics_for(None, song.default_language.as_deref()),
        LyricLanguage::Specific(code) => part
            .lyrics_in(language)
            .or_else(|| part.lyrics_for(Some(code), song.default_language.as_deref())),
    };

    let Some(content) = content else {
        return Vec::new();
    };

    split_into_slides(content)
}

/// Split a lyrics block into slides at its blank lines.
fn split_into_slides(content: &SongPartContent) -> Vec<Vec<String>> {
    let readable = lyrics_for_reading(&content.content);

    let mut slides: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for line in readable.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !current.is_empty() {
                slides.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(line.to_string());
    }
    if !current.is_empty() {
        slides.push(current);
    }

    slides
}

/// Which languages to write, in order.
///
/// The setting wins. Without one, a song that labels its lyrics is written in
/// its own default language first and the rest alphabetically; a song whose
/// lyrics carry no language at all is written as a single-language file.
fn languages_of(song: &Song, settings: &SngExportSettings) -> Vec<LyricLanguage> {
    if !settings.languages.is_empty() {
        return settings
            .languages
            .iter()
            .map(|code| LyricLanguage::specific(code))
            .collect();
    }

    let available = song.available_languages();

    // Lyrics that name no language belong to the song's default language — or,
    // if the song does not name one either, to the only language there is.
    let has_unlabelled = song
        .parts()
        .iter()
        .flat_map(|part| part.all_lyrics())
        .any(|(language, _)| *language == LyricLanguage::Default);

    let mut languages: Vec<LyricLanguage> = Vec::new();
    match &song.default_language {
        Some(default) => languages.push(LyricLanguage::specific(default)),
        None if has_unlabelled => languages.push(LyricLanguage::Default),
        None => {}
    }

    for code in &available {
        let language = LyricLanguage::specific(code);
        if !languages.contains(&language) {
            languages.push(language);
        }
    }

    if languages.is_empty() {
        languages.push(LyricLanguage::Default);
    }

    languages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::songbeamer::{
        import_from_sng_string, import_from_sng_string_with, SngImportSettings,
    };
    use crate::song::{SongPartContent, SongPartContentType};

    fn export(song: &Song) -> String {
        sng_from_song(song, &SngExportSettings::default())
    }

    /// The property the module exists for: a file that goes through the model
    /// and back out is the same file.
    fn assert_round_trips(sng: &str) -> Song {
        let song = import_from_sng_string(sng).unwrap();
        let exported = export(&song);
        let back = import_from_sng_string(&exported)
            .unwrap_or_else(|error| panic!("the export does not parse: {}\n{}", error, exported));

        assert_eq!(back.title, song.title, "{}", exported);
        assert_eq!(back.tags(), song.tags(), "{}", exported);

        let ids = |s: &Song| -> Vec<String> {
            s.parts().iter().map(|part| part.id().to_string()).collect()
        };
        assert_eq!(ids(&back), ids(&song), "{}", exported);

        for (before, after) in song.parts().iter().zip(back.parts()) {
            assert_eq!(after.label, before.label, "{}", exported);
            assert_eq!(
                after.contents, before.contents,
                "the contents of {} changed\n{}",
                before.id(),
                exported
            );
        }

        let sung = |s: &Song| -> Vec<String> {
            s.ordered_parts()
                .iter()
                .map(|part| part.id().to_string())
                .collect()
        };
        assert_eq!(sung(&back), sung(&song), "{}", exported);

        song
    }

    // --- Shape of the output ---------------------------------------------

    #[test]
    fn test_the_file_starts_with_the_language_count_and_title() {
        let song = import_from_sng_string("#Title=Test\n---\nVerse 1\nline\n").unwrap();
        let exported = export(&song);

        assert!(exported.starts_with("#LangCount=1\r\n#Title=Test\r\n"), "{}", exported);
        assert!(exported.contains("#Version=3\r\n"), "{}", exported);
        assert!(exported.ends_with("\r\n"), "{}", exported);
    }

    #[test]
    fn test_crlf_everywhere() {
        let exported = export(&import_from_sng_string("#Title=T\n---\nVerse 1\na\nb\n").unwrap());
        assert_eq!(exported.matches('\n').count(), exported.matches("\r\n").count());
    }

    #[test]
    fn test_the_byte_export_carries_a_bom() {
        let song = import_from_sng_string("#Title=T\n---\nText\n").unwrap();
        let bytes = sng_bytes_from_song(&song, &SngExportSettings::default());

        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(
            crate::importer::songbeamer::import_from_bytes(&bytes)
                .unwrap()
                .title,
            "T"
        );
    }

    // --- Round trips ------------------------------------------------------

    #[test]
    fn test_a_full_file_round_trips() {
        assert_round_trips(
            "#LangCount=1\r\n\
             #Title=Amazing Grace\r\n\
             #Author=John Newton\r\n\
             #Melody=New Britain\r\n\
             #(c)=Public Domain\r\n\
             #CCLI=22025\r\n\
             #Songbook=Feiert Jesus 3 / 42\r\n\
             #Key=F\r\n\
             #VerseOrder=Verse 1,Refrain,Verse 2,Refrain\r\n\
             ---\r\n\
             Verse 1\r\n\
             Amazing grace, how sweet the sound\r\n\
             That saved a wretch like me\r\n\
             ---\r\n\
             Refrain\r\n\
             Praise God\r\n\
             ---\r\n\
             Verse 2\r\n\
             'Twas grace that taught my heart to fear\r\n",
        );
    }

    #[test]
    fn test_slide_breaks_round_trip() {
        let song = assert_round_trips(
            "#Title=T\n---\nVerse 1\nfirst slide\n--\nsecond slide\n---\nVerse 2\nlater\n",
        );

        let exported = export(&song);
        assert!(
            exported.contains("first slide\r\n--\r\nsecond slide"),
            "the slide break was lost:\n{}",
            exported
        );
    }

    #[test]
    fn test_german_marks_round_trip() {
        let song =
            assert_round_trips("#Title=T\n---\nStrophe 1\neins\n---\nRefrain\nkehrvers\n");

        let exported = export(&song);
        assert!(exported.contains("\r\nStrophe 1\r\n"), "{}", exported);
        assert!(exported.contains("\r\nRefrain\r\n"), "{}", exported);
    }

    #[test]
    fn test_custom_marks_round_trip() {
        let song = assert_round_trips("#Title=T\n---\n$$M=Zwischenteil\ntext\n");
        assert!(export(&song).contains("$$M=Zwischenteil"), "{}", export(&song));
    }

    #[test]
    fn test_umlauts_round_trip_through_the_bytes() {
        let song = import_from_sng_string("#Title=Grüße\n---\nVerse 1\nDank sei dir, für ...\n")
            .unwrap();
        let bytes = sng_bytes_from_song(&song, &SngExportSettings::default());
        let back = crate::importer::songbeamer::import_from_bytes(&bytes).unwrap();

        assert_eq!(back.title, "Grüße");
        assert!(back
            .part_at(0)
            .unwrap()
            .lyrics_for(None, None)
            .unwrap()
            .content
            .contains("für"));
    }

    #[test]
    fn test_two_languages_round_trip() {
        let settings = SngImportSettings {
            languages: vec!["de".to_string(), "en".to_string()],
        };
        let song = import_from_sng_string_with(
            "#Title=T\n#LangCount=2\n---\nVerse 1\nZeile eins\nline one\nZeile zwei\nline two\n",
            &settings,
        )
        .unwrap();

        let exported = sng_from_song(&song, &SngExportSettings::default());
        assert!(exported.contains("#LangCount=2"), "{}", exported);

        let back = import_from_sng_string_with(&exported, &settings).unwrap();
        let part = back.part(&"verse.1".parse().unwrap()).unwrap();
        assert_eq!(
            part.lyrics_for(Some("de"), None).unwrap().content,
            "Zeile eins\nZeile zwei"
        );
        assert_eq!(
            part.lyrics_for(Some("en"), None).unwrap().content,
            "line one\nline two"
        );
    }

    /// A language with fewer lines must not shift the other one out of turn.
    #[test]
    fn test_uneven_translations_stay_aligned() {
        let mut song = Song::new("Uneven");
        song.default_language = Some("de".to_string());
        let id = song.add_part_of_type(SongPartType::Verse, Some(1));
        let part = song.part_mut(&id).unwrap();
        part.add_content(SongPartContent::lyrics(
            LyricLanguage::specific("de"),
            "eins\nzwei\ndrei",
        ));
        part.add_content(SongPartContent::lyrics(
            LyricLanguage::specific("en"),
            "one\ntwo",
        ));

        let exported = sng_from_song(&song, &SngExportSettings::default());
        let back = import_from_sng_string_with(
            &exported,
            &SngImportSettings {
                languages: vec!["de".to_string(), "en".to_string()],
            },
        )
        .unwrap();

        let part = back.part(&"verse.1".parse().unwrap()).unwrap();
        assert_eq!(
            part.lyrics_for(Some("de"), None).unwrap().content,
            "eins\nzwei\ndrei"
        );
        assert_eq!(part.lyrics_for(Some("en"), None).unwrap().content, "one\ntwo");
    }

    // --- Songs from other formats ----------------------------------------

    /// The point of the exporter: a song from any importer can be handed to
    /// SongBeamer.
    #[test]
    fn test_a_classic_song_exports() {
        let content =
            std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();
        let song = crate::importer::classic_song::import_song(&content).unwrap();

        let exported = export(&song);
        let back = import_from_sng_string(&exported).unwrap();

        assert_eq!(back.title, song.title);
        assert_eq!(back.part_count(), song.part_count());
        for (before, after) in song.parts().iter().zip(back.parts()) {
            assert_eq!(
                after.lyrics_for(None, None).map(|c| c.content.clone()),
                before.lyrics_for(None, None).map(|c| c.content.clone()),
                "the lyrics of {} changed\n{}",
                before.id(),
                exported
            );
        }
    }

    /// The syllable markup of a sung score is not something an audience reads.
    #[test]
    fn test_lilypond_syllable_markup_is_stripped() {
        let mut song = Song::new("Sung");
        let id = song.add_part_of_type(SongPartType::Verse, Some(1));
        let part = song.part_mut(&id).unwrap();
        part.add_content(SongPartContent::new(
            SongPartContentType::LeadVoice,
            "c4 d e f",
        ));
        part.add_content(SongPartContent::lyrics(
            LyricLanguage::Default,
            "A -- ma -- zing grace",
        ));

        let exported = export(&song);

        assert!(exported.contains("Amazing grace"), "{}", exported);
        assert!(!exported.contains(" -- "), "{}", exported);
        // The melody has no place in the format and must not leak into it.
        assert!(!exported.contains("c4 d e f"), "{}", exported);
    }

    /// A guessed order is this crate's idea, not the file's — writing it out
    /// would turn a guess into a stated fact.
    #[test]
    fn test_only_an_explicit_order_is_written() {
        let guessed = import_from_sng_string("#Title=T\n---\nVerse 1\na\n---\nRefrain\nb\n").unwrap();
        assert!(!export(&guessed).contains("#VerseOrder"), "{}", export(&guessed));

        let stated = import_from_sng_string(
            "#Title=T\n#VerseOrder=Verse 1,Refrain\n---\nVerse 1\na\n---\nRefrain\nb\n",
        )
        .unwrap();
        assert!(
            export(&stated).contains("#VerseOrder=Verse 1,Refrain"),
            "{}",
            export(&stated)
        );
    }

    #[test]
    fn test_absent_metadata_is_left_out() {
        let song = Song::new("Bare");
        let exported = export(&song);

        assert!(!exported.contains("#Author"), "{}", exported);
        assert!(!exported.contains("#(c)"), "{}", exported);
        assert!(!exported.contains("=\r\n"), "no empty values:\n{}", exported);
    }

    /// Exporting twice has to give the same bytes.
    #[test]
    fn test_the_export_is_stable() {
        let song = import_from_sng_string(
            "#Title=T\n#Author=A\n#VerseOrder=Verse 1\n---\nVerse 1\nline\n",
        )
        .unwrap();

        let once = export(&song);
        let twice = export(&import_from_sng_string(&once).unwrap());
        assert_eq!(once, twice);
    }
}
