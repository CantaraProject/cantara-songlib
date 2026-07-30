//! Importer for the plain-text export of [CCLI SongSelect](https://songselect.ccli.com).
//!
//! SongSelect lets a user download the lyrics of a licensed song as a `.txt`
//! file (here `.ccli`). The layout is always the same:
//!
//! ```text
//! Weiß ich den Weg auch nicht (Pax Dei)   ← title
//!
//! Vers 1                                  ← section heading
//! Weiß ich den Weg auch nicht, …          ← lyrics
//! …
//!
//! Vers 2
//! …
//!
//! CCLI-Liednummer 5973691                 ← trailer
//! Hedwig Von Redern | John Bacchus Dykes
//! © Words: Public Domain
//! CCLI-Lizenznummer 0000000
//! ```
//!
//! # Working in any language
//!
//! SongSelect translates the file it hands out: the very same song is
//! `Vers 1 / Refrain / CCLI-Liednummer` in German, `Verse 1 / Chorus /
//! CCLI Song #` in English, `Verso 1 / Coro / Número de Canción CCLI` in
//! Spanish, and so on. This importer therefore leans on the parts of the format
//! that do **not** change with the language:
//!
//! * **Where things are.** The title is the first block, the trailer starts at
//!   the first line mentioning CCLI, and every block in between is one section
//!   whose first line is its heading. None of that depends on wording.
//! * **`CCLI` is a brand name** and stays untranslated, so it is a reliable
//!   anchor for the metadata trailer in every localisation.
//! * **`©` and `|`** are punctuation, not words: they mark the copyright block
//!   and separate authors regardless of language.
//!
//! Only one step needs vocabulary — deciding whether a heading means "verse" or
//! "chorus" — and that step is allowed to fail. [`classify_heading`] knows the
//! wording of a good number of languages, and a heading it does not recognise
//! still produces a part, typed [`SongPartType::Other`], with the original
//! heading kept in [`SongPart::label`]. A song in an unsupported language
//! therefore imports completely; only the automatic verse/refrain ordering is
//! lost, and nothing is silently dropped.

use std::error::Error;
use std::sync::OnceLock;

use crate::song::{LyricLanguage, Song, SongPart, SongPartContent, SongPartType};

/// Parse the text of a CCLI SongSelect export into a [`Song`].
///
/// ```
/// use cantara_songlib::importer::ccli;
///
/// let song = ccli::import_from_ccli_string(
///     "My Song\n\nVerse 1\nline one\nline two\n\nCCLI Song # 12345\nA. Writer\n",
/// )
/// .unwrap();
///
/// assert_eq!(song.title, "My Song");
/// assert_eq!(song.tag("ccli_song_number").unwrap(), "12345");
/// assert_eq!(song.tag("author").unwrap(), "A. Writer");
/// ```
///
/// # Errors
/// Returns an error if the text contains no title or no section at all, which
/// means it is not a SongSelect export.
pub fn import_from_ccli_string(content: &str) -> Result<Song, Box<dyn Error>> {
    // SongSelect ships CRLF line endings; normalise them away first.
    let normalised = content.replace("\r\n", "\n").replace('\r', "\n");

    let (body, trailer) = split_off_trailer(&normalised);
    let mut blocks = into_blocks(body);

    if blocks.is_empty() {
        return Err("not a CCLI SongSelect export: the file is empty".into());
    }

    // --- Title -----------------------------------------------------------
    // The first block is the title. Some exports add the artist or an
    // alternate title on the following lines; those are kept as a tag rather
    // than guessed at.
    let title_block = blocks.remove(0);
    let mut song = Song::new(title_block[0]);
    if title_block.len() > 1 {
        song.set_tag("subtitle", &title_block[1..].join("\n"));
    }

    if blocks.is_empty() {
        return Err("not a CCLI SongSelect export: the file contains no sections".into());
    }

    // --- Sections --------------------------------------------------------
    for block in blocks {
        add_section(&mut song, &block);
    }

    // --- Trailer ---------------------------------------------------------
    if let Some(trailer) = trailer {
        parse_trailer(&mut song, trailer);
    }

    song.add_guessed_part_order();

    Ok(song)
}

// ---------------------------------------------------------------------------
// Splitting the file up
// ---------------------------------------------------------------------------

/// Split the text into the song body and the metadata trailer.
///
/// The trailer begins at the first line that mentions CCLI, which in every
/// localisation is the song number line (`CCLI Song #`, `CCLI-Liednummer`,
/// `Número de Canción CCLI`, …). `CCLI` is a brand name and is never
/// translated, which makes it the one dependable anchor in the file.
fn split_off_trailer(content: &str) -> (&str, Option<&str>) {
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        if is_ccli_reference(line) {
            return (&content[..offset], Some(&content[offset..]));
        }
        offset += line.len();
    }
    (content, None)
}

/// Whether a line is one of the `CCLI …` reference lines.
///
/// Requires both the brand name and a number so that a lyric line happening to
/// mention CCLI cannot end the song early.
fn is_ccli_reference(line: &str) -> bool {
    line.to_uppercase().contains("CCLI") && line.chars().any(|c| c.is_ascii_digit())
}

/// Group non-empty lines into blocks, splitting on blank lines.
///
/// Exports differ in how many blank lines they put between sections, so any
/// number of them separates a block.
fn into_blocks(body: &str) -> Vec<Vec<&str>> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    blocks
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

/// Turn one block of the body into a song part.
///
/// The first line of a block is its heading — that is how the format is built,
/// independent of what the heading says. The only exception is a block of a
/// single line, which cannot be a heading plus lyrics; such a block is treated
/// as lyrics unless the line is a heading this importer recognises.
fn add_section(song: &mut Song, block: &[&str]) {
    let classified = classify_heading(block[0]);

    // A block of several lines always starts with its heading — that is how the
    // format is built. A single line has to earn it: a heading is a word or
    // two, so a longer line is lyrics that merely happen to begin with a word
    // from the table (say "Solo mit dir will ich gehen"), and treating it as a
    // heading would drop the text.
    let is_heading = block.len() > 1
        || (classified.is_some() && block[0].split_whitespace().count() <= 3);

    let (heading, lyrics) = if is_heading {
        (Some(block[0]), &block[1..])
    } else {
        (None, block)
    };

    let (part_type, number) = match classified {
        Some(classified) => (classified.part_type, classified.number),
        None => (SongPartType::Other, None),
    };

    let id = song.add_part_of_type(part_type, number);
    // Unwrap is safe: the part was just added.
    let part = song.part_mut(&id).unwrap();
    part.label = heading.map(|heading| heading.to_string());

    if !lyrics.is_empty() {
        part.add_content(SongPartContent::lyrics(
            LyricLanguage::Default,
            lyrics.join("\n"),
        ));
    }
}

/// What a section heading was understood to mean.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClassifiedHeading {
    /// The role the heading names.
    pub part_type: SongPartType,
    /// The number it carried, e.g. `2` for `"Vers 2"`; `None` if it had none.
    pub number: Option<u32>,
}

/// Section headings this importer knows, grouped by the type they mean.
///
/// The entries are compared against a *normalised* heading: lower-cased, with
/// accents folded to ASCII and every separator removed, so one entry covers
/// `"Pre-Chorus"`, `"pre chorus"` and `"PreChorus"` alike.
///
/// The list is matched longest-entry-first, which is why `"prechorus"` wins
/// over `"chorus"` for a heading like `"Pre-Chorus 2"`.
///
/// It covers the Latin-script languages SongSelect publishes in plus a few
/// common CJK headings. Adding a language means adding entries here; nothing
/// else in the importer has to change, and an unlisted heading still imports
/// (see the module documentation).
const HEADINGS: &[(SongPartType, &[&str])] = &[
    (
        SongPartType::PreChorus,
        &[
            "prechorus",      // English
            "prerefrain",     // French, German
            "vorrefrain",     // German
            "prerefrao",      // Portuguese
            "prerefren",      // Polish, Czech
            "precoro",        // Spanish, Italian
            "preestribillo",  // Spanish
            "voorrefrein",    // Dutch
        ],
    ),
    (
        SongPartType::PostChorus,
        &["postchorus", "postrefrain", "nachrefrain", "postcoro", "postrefrao"],
    ),
    (
        SongPartType::Chorus,
        &[
            "chorus",      // English
            "refrain",     // German, French
            "refrein",     // Dutch
            "refrao",      // Portuguese
            "refren",      // Polish, Czech, Slovak
            "refrang",     // Swedish (refräng), Danish/Norwegian (refreng)
            "omkvaed",     // Danish (omkvæd)
            "coro",        // Spanish, Italian
            "estribillo",  // Spanish
            "ritornello",  // Italian
            "kertosae",    // Finnish (kertosäe)
            "refren",      // Romanian
            "副歌",         // Chinese
            "후렴",         // Korean
            "サビ",         // Japanese
        ],
    ),
    (
        SongPartType::Verse,
        &[
            "verse",     // English
            "vers",      // German, Dutch, Swedish, Norwegian, Danish
            "strophe",   // German, French
            "strofe",    // Dutch, Danish, Norwegian
            "strofa",    // Italian, Polish
            "verso",     // Spanish, Portuguese, Italian
            "estrofa",   // Spanish, Portuguese
            "couplet",   // French
            "zwrotka",   // Polish
            "sloka",     // Czech, Slovak
            "versszak",  // Hungarian
            "sakeisto",  // Finnish (säkeistö)
            "visa",      // Icelandic
            "主歌",       // Chinese
            "절",         // Korean
        ],
    ),
    (
        SongPartType::Bridge,
        &[
            "bridge",   // English, French
            "brucke",   // German (Brücke)
            "brug",     // Dutch
            "brygga",   // Swedish
            "bro",      // Norwegian, Danish
            "silta",    // Finnish
            "puente",   // Spanish
            "ponte",    // Portuguese, Italian
            "pont",     // French
            "most",     // Polish, Czech
            "hid",      // Hungarian (híd)
            "桥段",      // Chinese
        ],
    ),
    (
        SongPartType::Intro,
        &[
            "intro",
            "introduction",
            "introduccion",
            "introducao",
            "einleitung",
            "vorspiel",
            "inledning",
            "alkusoitto",
        ],
    ),
    (
        SongPartType::Outro,
        &[
            "outro",
            "ending",
            "finale",
            "final",
            "coda",
            "schluss",
            "nachspiel",
            "avslutning",
            "slutt",
        ],
    ),
    (
        SongPartType::Interlude,
        &[
            "interlude",
            "interludio",
            "interludium",
            "zwischenspiel",
            "mellanspel",
            "valisoitto",
        ],
    ),
    (
        SongPartType::Instrumental,
        &["instrumental", "instrumentaal", "instrumentell"],
    ),
    (SongPartType::Solo, &["solo", "solistisch"]),
];

/// [`HEADINGS`] flattened and sorted longest-entry-first, built once.
///
/// The order is what makes `"Pre-Chorus"` match `prechorus` rather than the
/// `chorus` entry it also contains.
fn candidates() -> &'static [(SongPartType, &'static str)] {
    static CANDIDATES: OnceLock<Vec<(SongPartType, &'static str)>> = OnceLock::new();
    CANDIDATES.get_or_init(|| {
        let mut candidates: Vec<(SongPartType, &'static str)> = HEADINGS
            .iter()
            .flat_map(|(part_type, names)| names.iter().map(move |name| (*part_type, *name)))
            .collect();
        candidates.sort_by_key(|(_, name)| std::cmp::Reverse(name.len()));
        candidates
    })
}

/// Work out what a section heading means.
///
/// Returns `None` for a heading in a language this importer has no vocabulary
/// for, or for a line that is not a heading at all. Callers treat that as
/// [`SongPartType::Other`] and keep the wording — see the module documentation.
///
/// ```
/// use cantara_songlib::importer::ccli::classify_heading;
/// use cantara_songlib::song::SongPartType;
///
/// // The same section, in the languages SongSelect hands the file out in.
/// for heading in ["Verse 2", "Vers 2", "Strophe 2", "Couplet 2", "Verso 2"] {
///     let classified = classify_heading(heading).unwrap();
///     assert_eq!(classified.part_type, SongPartType::Verse);
///     assert_eq!(classified.number, Some(2));
/// }
///
/// // Spelling variants of one heading all land on the same type.
/// for heading in ["Pre-Chorus", "pre chorus", "PreChorus"] {
///     assert_eq!(classify_heading(heading).unwrap().part_type, SongPartType::PreChorus);
/// }
///
/// // A lyric line is not a heading.
/// assert_eq!(classify_heading("Lorem ipsum dolor sit amet,"), None);
/// ```
pub fn classify_heading(heading: &str) -> Option<ClassifiedHeading> {
    let (text, number) = split_trailing_number(heading.trim());
    let normalised = normalise(text);

    if normalised.is_empty() {
        return None;
    }

    for &(part_type, name) in candidates() {
        // `starts_with` rather than equality so that headings which glue the
        // wording together ("Verseone", "Chorus2x") are still recognised.
        if normalised.starts_with(name) {
            return Some(ClassifiedHeading { part_type, number });
        }
    }

    None
}

/// Split a trailing section number off a heading: `"Vers 2"` → `("Vers", 2)`.
///
/// Numbers are written the same way in every localisation, which makes this
/// step language-independent. A parenthesised repeat count such as `"(2x)"` is
/// dropped rather than read as the section number.
fn split_trailing_number(heading: &str) -> (&str, Option<u32>) {
    // Drop a parenthesised suffix such as "(2x)" before looking for a number.
    let without_parens = match heading.find('(') {
        Some(position) => heading[..position].trim_end(),
        None => heading.trim_end(),
    };

    // Walk back over the trailing digits. Indexing by `char_indices` keeps this
    // safe for headings written in a multi-byte script.
    let boundary = without_parens
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(index, _)| index);

    match boundary {
        // `index == 0` means the heading is nothing but digits, which is not a
        // section heading at all.
        Some(index) if index > 0 => match without_parens[index..].parse::<u32>() {
            Ok(number) => (without_parens[..index].trim_end(), Some(number)),
            Err(_) => (without_parens, None),
        },
        _ => (without_parens, None),
    }
}

/// Reduce a heading to a comparable form: lower case, accents folded to ASCII,
/// separators removed.
///
/// Folding accents is what lets one table entry cover `"Brücke"` and
/// `"Brucke"`, `"refrão"` and `"refrao"` — spellings that vary with the
/// keyboard the file was produced on. Characters outside the Latin script, such
/// as `副歌`, are left untouched so that CJK headings still match.
fn normalise(heading: &str) -> String {
    heading
        .chars()
        .flat_map(fold_char)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Map one character to its accent-free lower-case form.
///
/// Returns more than one character for the few letters that expand: `ß` → `ss`,
/// `æ` → `ae`.
fn fold_char(c: char) -> FoldedChar {
    let folded: &'static [char] = match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ą' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ą' => &['a'],
        'é' | 'è' | 'ê' | 'ë' | 'ę' | 'É' | 'È' | 'Ê' | 'Ë' | 'Ę' => &['e'],
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => &['i'],
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ő' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' | 'Ő' => &['o'],
        'ú' | 'ù' | 'û' | 'ü' | 'ű' | 'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ű' => &['u'],
        'ý' | 'ÿ' | 'Ý' => &['y'],
        'ñ' | 'Ñ' => &['n'],
        'ç' | 'ć' | 'č' | 'Ç' | 'Ć' | 'Č' => &['c'],
        'ł' | 'Ł' => &['l'],
        'ś' | 'š' | 'ş' | 'Ś' | 'Š' | 'Ş' => &['s'],
        'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' | 'Ž' => &['z'],
        'ř' | 'Ř' => &['r'],
        'ť' | 'Ť' => &['t'],
        'ď' | 'Ď' => &['d'],
        'ň' | 'Ň' => &['n'],
        'ß' => &['s', 's'],
        'æ' | 'Æ' => &['a', 'e'],
        // Everything else — including CJK, which has no case — is only
        // lower-cased, so scripts the table lists literally still match.
        other => return FoldedChar::Lowercase(other.to_lowercase()),
    };
    FoldedChar::Mapped(folded.iter().copied())
}

/// Iterator returned by [`fold_char`]. Exists so that folding does not have to
/// allocate a `String` for every character of every heading.
enum FoldedChar {
    Mapped(std::iter::Copied<std::slice::Iter<'static, char>>),
    Lowercase(std::char::ToLowercase),
}

impl Iterator for FoldedChar {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        match self {
            FoldedChar::Mapped(iter) => iter.next(),
            FoldedChar::Lowercase(iter) => iter.next(),
        }
    }
}

// ---------------------------------------------------------------------------
// The metadata trailer
// ---------------------------------------------------------------------------

/// Read the trailer into the song's tags.
///
/// The trailer is laid out the same way in every localisation:
///
/// ```text
/// CCLI-Liednummer 5973691                 ← song number, always first
/// Hedwig Von Redern | John Bacchus Dykes  ← authors, separated by '|'
/// © Words: Public Domain                  ← copyright, starts at the '©'
/// Music: Public Domain
/// CCLI-Lizenznummer 0000000               ← licence number, always last
/// ```
///
/// The parser goes by that order and by punctuation, never by wording, so it
/// reads a German, English or Korean file the same way. It sets the tags
/// `ccli_song_number`, `ccli_license_number`, `author` and `copyright`,
/// omitting any that the file does not contain.
fn parse_trailer(song: &mut Song, trailer: &str) {
    let lines: Vec<&str> = trailer
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();

    let mut reference_numbers: Vec<String> = Vec::new();
    let mut authors: Vec<&str> = Vec::new();
    let mut copyright: Vec<&str> = Vec::new();
    let mut in_copyright = false;

    for line in lines {
        if is_ccli_reference(line) {
            reference_numbers.push(extract_number(line));
            // A licence line closes the copyright block.
            in_copyright = false;
            continue;
        }

        if starts_copyright(line) {
            in_copyright = true;
        }

        if in_copyright {
            copyright.push(line);
        } else {
            authors.push(line);
        }
    }

    // The song number comes first and the licence number last; with only one
    // reference line present it is the song number.
    if let Some(first) = reference_numbers.first() {
        if !first.is_empty() {
            song.set_tag("ccli_song_number", first);
        }
    }
    if reference_numbers.len() > 1 {
        let last = reference_numbers.last().unwrap();
        if !last.is_empty() {
            song.set_tag("ccli_license_number", last);
        }
    }

    if !authors.is_empty() {
        // SongSelect separates co-authors with '|'; normalise to a comma so
        // that the metadata templating reads naturally.
        let joined = authors.join(" | ");
        let names: Vec<&str> = joined
            .split('|')
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .collect();
        if !names.is_empty() {
            song.set_tag("author", &names.join(", "));
        }
    }

    if !copyright.is_empty() {
        song.set_tag("copyright", &copyright.join("\n"));
    }
}

/// Whether a line opens the copyright block.
///
/// `©` is punctuation and appears in every localisation; the spelled-out words
/// are accepted as a fallback for exports that lack the symbol.
fn starts_copyright(line: &str) -> bool {
    let lowered = line.to_lowercase();
    line.starts_with('©')
        || lowered.starts_with("(c)")
        || lowered.starts_with("copyright")
        || lowered.starts_with("℗")
}

/// Pull the reference number out of a `CCLI …` line.
///
/// Takes the last run of digits so that it does not matter whether the number
/// precedes or follows the words, which differs between localisations.
fn extract_number(line: &str) -> String {
    let mut digits = String::new();
    for c in line.chars().rev() {
        if c.is_ascii_digit() {
            digits.insert(0, c);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits
}

// ---------------------------------------------------------------------------
// Convenience
// ---------------------------------------------------------------------------

/// Read a CCLI SongSelect export from a file.
///
/// If the file has no title line of its own the file stem is used instead,
/// which some exports rely on.
pub fn import_from_file(path: &std::path::Path) -> Result<Song, Box<dyn Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut song = import_from_ccli_string(&content)?;

    if song.title.trim().is_empty() {
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            song.title = stem.to_string();
        }
    }

    Ok(song)
}

/// Every part of the song, as `(heading, lyrics)` pairs in singing order.
///
/// A small helper for callers that only want the text, e.g. to show a preview.
pub fn sections(song: &Song) -> Vec<(String, String)> {
    song.ordered_parts()
        .iter()
        .map(|part: &&SongPart| {
            let lyrics = part
                .lyrics_for(None, song.default_language.as_deref())
                .map(|content| content.content.clone())
                .unwrap_or_default();
            (part.display_label(), lyrics)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::SongPartId;

    fn german_example() -> Song {
        import_from_file(std::path::Path::new(
            "tests/data/Weiß ich den Weg auch nicht.ccli",
        ))
        .unwrap()
    }

    fn generic_example() -> Song {
        import_from_file(std::path::Path::new("tests/data/ExampleCCLISong1.ccli")).unwrap()
    }

    // --- The two sample files -------------------------------------------

    #[test]
    fn test_german_export_with_crlf_line_endings() {
        let song = german_example();

        assert_eq!(song.title, "Weiß ich den Weg auch nicht (Pax Dei)");
        assert_eq!(song.part_count_of_type(SongPartType::Verse), 3);
        assert_eq!(song.part_count(), 3);

        // The original German headings survive next to the typed structure.
        let verse1 = song.part(&SongPartId::new(SongPartType::Verse, 1)).unwrap();
        assert_eq!(verse1.label.as_deref(), Some("Vers 1"));

        let lyrics = verse1.lyrics_for(None, None).unwrap();
        assert!(lyrics.content.starts_with("Weiß ich den Weg auch nicht"));
        assert_eq!(lyrics.content.lines().count(), 4);
        // No stray carriage returns from the CRLF line endings.
        assert!(!lyrics.content.contains('\r'));

        let verse3 = song.part(&SongPartId::new(SongPartType::Verse, 3)).unwrap();
        assert!(verse3
            .lyrics_for(None, None)
            .unwrap()
            .content
            .ends_with("das ist genug."));
    }

    #[test]
    fn test_german_trailer() {
        let song = german_example();

        assert_eq!(song.tag("ccli_song_number").unwrap(), "5973691");
        assert_eq!(song.tag("ccli_license_number").unwrap(), "0000000");
        assert_eq!(
            song.tag("author").unwrap(),
            "Hedwig Von Redern, John Bacchus Dykes"
        );
        assert_eq!(
            song.tag("copyright").unwrap(),
            "© Words: Public Domain\nMusic: Public Domain"
        );

        // The trailer must not leak into the lyrics.
        for part in song.parts() {
            let lyrics = part.lyrics_for(None, None).unwrap().content.clone();
            assert!(!lyrics.contains("CCLI"), "trailer leaked into {}", part.id());
        }
    }

    #[test]
    fn test_mixed_section_types() {
        let song = generic_example();

        assert_eq!(song.title, "Example Song");
        assert_eq!(song.part_count_of_type(SongPartType::Verse), 2);
        assert_eq!(song.part_count_of_type(SongPartType::PreChorus), 1);
        assert_eq!(song.part_count_of_type(SongPartType::Chorus), 1);

        // "Pre-Chorus" must not be swallowed by the "Chorus" entry.
        let prechorus = song
            .part(&SongPartId::new(SongPartType::PreChorus, 1))
            .unwrap();
        assert_eq!(prechorus.label.as_deref(), Some("Pre-Chorus"));
        // Its lyrics repeat the heading — the heading line must not be counted
        // as lyrics, nor a lyric line as the heading.
        assert_eq!(
            prechorus.lyrics_for(None, None).unwrap().content,
            "Missing Pre-Chorus\nMissing Pre-Chorus"
        );

        assert_eq!(song.tag("ccli_song_number").unwrap(), "000000");
        assert_eq!(song.tag("ccli_license_number").unwrap(), "000000");
        assert_eq!(song.tag("author").unwrap(), "Lorem Ipsum");
        assert!(song.tag("copyright").is_none());
    }

    #[test]
    fn test_singing_order_is_derived() {
        let song = generic_example();
        let sung: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();

        // Verse, pre-chorus, chorus — and the same again for verse 2.
        assert_eq!(
            sung,
            [
                "verse.1",
                "prechorus.1",
                "chorus.1",
                "verse.2",
                "prechorus.1",
                "chorus.1"
            ]
        );
    }

    // --- Language independence -------------------------------------------

    /// The same song, as SongSelect hands it out in different languages.
    #[test]
    fn test_the_same_song_in_several_languages() {
        let cases = [
            ("Verse 1", "Chorus", "Bridge", "CCLI Song # 1234"),
            ("Vers 1", "Refrain", "Brücke", "CCLI-Liednummer 1234"),
            ("Verso 1", "Coro", "Puente", "Número de Canción CCLI 1234"),
            ("Couplet 1", "Refrain", "Pont", "CCLI Chant N° 1234"),
            ("Verso 1", "Refrão", "Ponte", "Número da Música CCLI 1234"),
            ("Vers 1", "Refrein", "Brug", "CCLI-Liednummer 1234"),
            ("Zwrotka 1", "Refren", "Most", "Numer pieśni CCLI 1234"),
        ];

        for (verse, chorus, bridge, reference) in cases {
            let text = format!(
                "A Song\n\n{verse}\nfirst line\n\n{chorus}\nsecond line\n\n{bridge}\nthird line\n\n{reference}\nAn Author\n"
            );
            let song = import_from_ccli_string(&text).unwrap();

            assert_eq!(
                song.part_count_of_type(SongPartType::Verse),
                1,
                "verse not recognised: {}",
                verse
            );
            assert_eq!(
                song.part_count_of_type(SongPartType::Chorus),
                1,
                "chorus not recognised: {}",
                chorus
            );
            assert_eq!(
                song.part_count_of_type(SongPartType::Bridge),
                1,
                "bridge not recognised: {}",
                bridge
            );
            assert_eq!(
                song.tag("ccli_song_number").unwrap(),
                "1234",
                "reference not recognised: {}",
                reference
            );
        }
    }

    #[test]
    fn test_accents_do_not_matter() {
        for heading in ["Brücke", "Brucke", "BRÜCKE"] {
            assert_eq!(
                classify_heading(heading).unwrap().part_type,
                SongPartType::Bridge,
                "failed for {}",
                heading
            );
        }
        for heading in ["Refrão", "refrao", "Säkeistö"] {
            assert!(classify_heading(heading).is_some(), "failed for {}", heading);
        }
    }

    #[test]
    fn test_prechorus_wins_over_chorus() {
        assert_eq!(
            classify_heading("Pre-Chorus").unwrap().part_type,
            SongPartType::PreChorus
        );
        assert_eq!(
            classify_heading("Post-Chorus").unwrap().part_type,
            SongPartType::PostChorus
        );
        assert_eq!(
            classify_heading("Chorus").unwrap().part_type,
            SongPartType::Chorus
        );
    }

    #[test]
    fn test_section_numbers() {
        assert_eq!(classify_heading("Verse 2").unwrap().number, Some(2));
        assert_eq!(classify_heading("Verse").unwrap().number, None);
        assert_eq!(classify_heading("Vers 12").unwrap().number, Some(12));
        // A repeat count is not a section number.
        assert_eq!(classify_heading("Chorus (2x)").unwrap().number, None);
    }

    /// A heading in a language the table does not cover still has to import.
    #[test]
    fn test_unknown_heading_is_kept_verbatim() {
        let text = "Lagu\n\nBagian Pertama\nbaris satu\nbaris dua\n\nCCLI Song # 42\nPenulis\n";
        let song = import_from_ccli_string(text).unwrap();

        assert_eq!(song.part_count(), 1);
        let part = song.part_at(0).unwrap();
        assert_eq!(part.part_type, SongPartType::Other);
        assert_eq!(part.label.as_deref(), Some("Bagian Pertama"));
        assert_eq!(
            part.lyrics_for(None, None).unwrap().content,
            "baris satu\nbaris dua"
        );
        assert_eq!(song.tag("ccli_song_number").unwrap(), "42");
    }

    /// A one-line block that merely starts with a heading word is lyrics, not
    /// an empty section.
    #[test]
    fn test_single_lyric_line_is_not_mistaken_for_a_heading() {
        let song = import_from_ccli_string(
            "Title\n\nVerse 1\nfirst\n\nSolo mit dir will ich gehen\n\nCCLI Song # 1\n",
        )
        .unwrap();

        let part = song.part_at(1).unwrap();
        assert_eq!(part.label, None);
        assert_eq!(
            part.lyrics_for(None, None).unwrap().content,
            "Solo mit dir will ich gehen"
        );

        // A genuine one-word heading still works.
        let song = import_from_ccli_string("Title\n\nSolo\n\nCCLI Song # 1\n").unwrap();
        assert_eq!(song.part_at(0).unwrap().part_type, SongPartType::Solo);
    }

    #[test]
    fn test_cjk_headings() {
        assert_eq!(
            classify_heading("主歌 1").unwrap().part_type,
            SongPartType::Verse
        );
        assert_eq!(
            classify_heading("副歌").unwrap().part_type,
            SongPartType::Chorus
        );
    }

    // --- Robustness ------------------------------------------------------

    #[test]
    fn test_file_without_a_trailer() {
        let song =
            import_from_ccli_string("Title\n\nVerse 1\nsome words\n").unwrap();
        assert_eq!(song.title, "Title");
        assert_eq!(song.part_count(), 1);
        assert!(song.tag("ccli_song_number").is_none());
    }

    #[test]
    fn test_single_reference_line_is_the_song_number() {
        let song =
            import_from_ccli_string("Title\n\nVerse 1\nwords\n\nCCLI Song # 77\n").unwrap();
        assert_eq!(song.tag("ccli_song_number").unwrap(), "77");
        assert!(song.tag("ccli_license_number").is_none());
    }

    #[test]
    fn test_subtitle_line_is_kept() {
        let song = import_from_ccli_string(
            "The Title\nThe Artist\n\nVerse 1\nwords\n\nCCLI Song # 1\n",
        )
        .unwrap();
        assert_eq!(song.title, "The Title");
        assert_eq!(song.tag("subtitle").unwrap(), "The Artist");
    }

    #[test]
    fn test_empty_input_is_an_error() {
        assert!(import_from_ccli_string("").is_err());
        assert!(import_from_ccli_string("Only a title\n").is_err());
    }

    #[test]
    fn test_a_lyric_line_mentioning_ccli_needs_a_number() {
        // "CCLI" without digits must not be mistaken for the trailer.
        let song = import_from_ccli_string(
            "Title\n\nVerse 1\nwe sing about CCLI today\n\nCCLI Song # 5\n",
        )
        .unwrap();
        assert_eq!(
            song.part_at(0).unwrap().lyrics_for(None, None).unwrap().content,
            "we sing about CCLI today"
        );
        assert_eq!(song.tag("ccli_song_number").unwrap(), "5");
    }

    #[test]
    fn test_sections_helper() {
        let sections = sections(&german_example());
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].0, "Vers 1");
        assert!(sections[0].1.starts_with("Weiß ich"));
    }
}
