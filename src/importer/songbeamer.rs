//! Importer for SongBeamer song files (`.sng`).
//!
//! [SongBeamer](https://www.songbeamer.de) is a widely used German
//! presentation program for churches. Its song files are plain text: a block of
//! `#Tag=value` header lines, then the sung text, cut into slides by separator
//! lines.
//!
//! ```text
//! #LangCount=1
//! #Title=Amazing Grace
//! #Author=John Newton
//! #(c)=Public Domain
//! #VerseOrder=Verse 1,Refrain,Verse 2
//! ---
//! Verse 1              ← optional verse mark
//! Amazing grace, how sweet the sound
//! --                   ← slide break inside the same verse
//! That saved a wretch like me
//! ---
//! Refrain
//! …
//! ```
//!
//! # What the parser relies on
//!
//! * **The header ends at the first separator.** Everything before it that
//!   starts with `#` is a tag; `=` splits the tag from its value.
//! * **`---` starts a new block**, i.e. a new [`crate::song::SongPart`]. The
//!   very first one only opens the body and creates no part of its own.
//! * **`--` breaks a slide** inside the current block. The data model has no
//!   notion of a slide, so a break becomes a blank line inside the part's
//!   lyrics — which is what [`crate::exporter::songbeamer`] turns back into
//!   `--`, and what the slide exporter reads as a paragraph.
//! * **The first line of a block may be a verse mark** such as `Verse 2`,
//!   `Refrain` or `$$M=Zwischenteil`. A line that is not a mark is lyrics, so a
//!   file without marks still imports completely — every block simply becomes a
//!   verse.
//!
//! # Encoding
//!
//! SongBeamer writes one of four encodings and identifies them by the byte
//! order mark: UTF-8, UTF-16 little endian, UTF-16 big endian, or — with no BOM
//! at all — the Windows ANSI code page 1252. [`import_from_bytes`] handles all
//! four, so umlauts survive whichever version of the program wrote the file.
//!
//! # Several languages
//!
//! With `#LangCount=2` (or more) the languages are interleaved *line by line*
//! inside every slide: the first line is the first language, the second line
//! its translation, and so on. The importer splits them apart into one
//! [`crate::song::SongPartContent`] per language. Nothing in the file names the
//! languages, so pass the codes in through [`SngImportSettings::languages`] if
//! you know them; otherwise the first language becomes
//! [`LyricLanguage::Default`] and the others `lang2`, `lang3`, ….
//!
//! # Sources
//!
//! The format is not formally specified. This implementation follows the
//! SongBeamer wiki's description of the separators and verse marks
//! (<http://wiki.songbeamer.de/index.php?title=Song>) and the tag list and
//! encoding rules used by OpenLP's importer
//! (<https://gitlab.com/openlp/openlp>, `songbeamer.py`).

use std::error::Error;
use std::path::Path;

use crate::base64;
use crate::song::{
    LyricLanguage, PartOrder, PartOrderName, PartOrderRule, Song, SongPartContent, SongPartId,
    SongPartType,
};

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// How to read a `.sng` file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SngImportSettings {
    /// The language codes behind `#LangCount`, in the order the file
    /// interleaves them, e.g. `["de", "en"]`.
    ///
    /// The file itself never names its languages. Leaving this empty makes the
    /// first language [`LyricLanguage::Default`] and names the rest `lang2`,
    /// `lang3`, … so that they can still be told apart.
    pub languages: Vec<String>,
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Read a `.sng` file from its raw bytes, detecting the encoding.
///
/// This is the entry point to prefer: a `.sng` file is not necessarily UTF-8,
/// and reading one as if it were mangles every umlaut.
pub fn import_from_bytes(bytes: &[u8]) -> Result<Song, Box<dyn Error>> {
    import_from_bytes_with(bytes, &SngImportSettings::default())
}

/// [`import_from_bytes`] with explicit settings.
pub fn import_from_bytes_with(
    bytes: &[u8],
    settings: &SngImportSettings,
) -> Result<Song, Box<dyn Error>> {
    import_from_sng_string_with(&decode_bytes(bytes), settings)
}

/// Parse the text of a `.sng` file into a [`Song`].
///
/// ```
/// use cantara_songlib::importer::songbeamer;
/// use cantara_songlib::song::SongPartType;
///
/// let song = songbeamer::import_from_sng_string(
///     "#Title=Amazing Grace\n#Author=John Newton\n---\nVerse 1\nAmazing grace\n",
/// )
/// .unwrap();
///
/// assert_eq!(song.title, "Amazing Grace");
/// assert_eq!(song.tag("author").unwrap(), "John Newton");
/// assert_eq!(song.part_count_of_type(SongPartType::Verse), 1);
/// ```
///
/// # Errors
/// Returns an error if the text contains no separator line at all, which means
/// it is not a SongBeamer file.
pub fn import_from_sng_string(content: &str) -> Result<Song, Box<dyn Error>> {
    import_from_sng_string_with(content, &SngImportSettings::default())
}

/// [`import_from_sng_string`] with explicit settings.
pub fn import_from_sng_string_with(
    content: &str,
    settings: &SngImportSettings,
) -> Result<Song, Box<dyn Error>> {
    parse(content, settings)
}

/// Read a `.sng` file from disk.
///
/// A file without a `#Title` is titled after its name, which is how SongBeamer
/// itself displays such a song.
pub fn import_from_file(path: &Path) -> Result<Song, Box<dyn Error>> {
    import_from_file_with(path, &SngImportSettings::default())
}

/// [`import_from_file`] with explicit settings.
pub fn import_from_file_with(
    path: &Path,
    settings: &SngImportSettings,
) -> Result<Song, Box<dyn Error>> {
    let bytes = std::fs::read(path)?;
    let mut song = import_from_bytes_with(&bytes, settings)?;

    if song.title.trim().is_empty()
        && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
    {
        song.title = stem.to_string();
    }

    Ok(song)
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// Decode the bytes of a `.sng` file into text.
///
/// The byte order mark decides: `EF BB BF` is UTF-8, `FF FE` is UTF-16 little
/// endian, `FE FF` is UTF-16 big endian. Without a BOM the file is UTF-8 if it
/// happens to be valid UTF-8 — which every ASCII file is — and Windows code
/// page 1252 otherwise, the encoding older SongBeamer versions wrote.
///
/// ```
/// use cantara_songlib::importer::songbeamer::decode_bytes;
///
/// // cp1252: 0xFC is 'ü'.
/// assert_eq!(decode_bytes(&[b'f', b'\xfc', b'r']), "für");
/// // The same word as UTF-8 with a BOM.
/// assert_eq!(decode_bytes("\u{feff}für".as_bytes()), "für");
/// ```
pub fn decode_bytes(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, false);
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => decode_cp1252(bytes),
    }
}

/// Decode UTF-16 with the given endianness, replacing broken surrogate pairs.
fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();

    String::from_utf16_lossy(&units)
}

/// Decode Windows code page 1252.
///
/// It agrees with Latin-1 everywhere except `0x80..=0x9F`, where Windows put
/// typographic characters — the curly quotes and dashes that a song text picked
/// up from a word processor.
fn decode_cp1252(bytes: &[u8]) -> String {
    /// The characters `0x80..=0x9F` map to. `\u{FFFD}` marks the five positions
    /// cp1252 leaves undefined.
    const HIGH: [char; 32] = [
        '€', '\u{FFFD}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{FFFD}', 'Ž',
        '\u{FFFD}', '\u{FFFD}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
        '\u{FFFD}', 'ž', 'Ÿ',
    ];

    bytes
        .iter()
        .map(|byte| match byte {
            0x80..=0x9F => HIGH[(byte - 0x80) as usize],
            other => *other as char,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Verse marks
// ---------------------------------------------------------------------------

/// What the first line of a block was understood to mean.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ClassifiedMark {
    /// The role the mark names.
    pub part_type: SongPartType,
    /// The number it carried, e.g. `2` for `"Verse 2"`; `None` if it had none.
    pub number: Option<u32>,
    /// The text of a `$$M=…` mark, which names a section the format has no
    /// keyword for.
    pub custom_name: Option<String>,
}

/// The verse marks SongBeamer knows, and the part type each one means.
///
/// The German and the English wording of a mark are both in use — SongBeamer is
/// a German program with an English interface — so both are listed. Compared
/// against the mark lower-cased and with its separators removed, so `Pre-Chorus`,
/// `pre chorus` and `PreChorus` are one entry.
const MARKS: &[(&str, SongPartType)] = &[
    ("prechorus", SongPartType::PreChorus),
    ("prerefrain", SongPartType::PreChorus),
    ("vorrefrain", SongPartType::PreChorus),
    ("postchorus", SongPartType::PostChorus),
    ("postrefrain", SongPartType::PostChorus),
    ("nachrefrain", SongPartType::PostChorus),
    ("refrain", SongPartType::Refrain),
    ("chorus", SongPartType::Chorus),
    ("verse", SongPartType::Verse),
    ("vers", SongPartType::Verse),
    ("strophe", SongPartType::Verse),
    ("intro", SongPartType::Intro),
    ("vorspiel", SongPartType::Intro),
    ("coda", SongPartType::Outro),
    ("ending", SongPartType::Outro),
    ("outro", SongPartType::Outro),
    ("schluss", SongPartType::Outro),
    ("nachspiel", SongPartType::Outro),
    ("bridge", SongPartType::Bridge),
    ("bruecke", SongPartType::Bridge),
    ("interlude", SongPartType::Interlude),
    ("zwischenspiel", SongPartType::Interlude),
    ("instrumental", SongPartType::Instrumental),
    ("solo", SongPartType::Solo),
    // Marks that exist only to say "this is a section": they carry no role.
    ("part", SongPartType::Other),
    ("teil", SongPartType::Other),
    ("misc", SongPartType::Other),
    ("prebridge", SongPartType::Other),
    ("precoda", SongPartType::Other),
    ("unbekannt", SongPartType::Other),
    ("unbenannt", SongPartType::Other),
    ("unknown", SongPartType::Other),
];

/// Work out whether a line is a verse mark, and what it means.
///
/// A mark is one keyword, optionally followed by a number — anything longer is
/// a line of lyrics that happens to begin with the keyword, and treating it as
/// a mark would drop the text.
///
/// ```
/// use cantara_songlib::importer::songbeamer::classify_mark;
/// use cantara_songlib::song::SongPartType;
///
/// let mark = classify_mark("Strophe 2").unwrap();
/// assert_eq!(mark.part_type, SongPartType::Verse);
/// assert_eq!(mark.number, Some(2));
///
/// // A custom mark names its own section.
/// assert_eq!(classify_mark("$$M=Zwischenteil").unwrap().custom_name.as_deref(), Some("Zwischenteil"));
///
/// // Lyrics that start with a keyword are not a mark.
/// assert_eq!(classify_mark("Refrain of the angels above"), None);
/// ```
pub fn classify_mark(line: &str) -> Option<ClassifiedMark> {
    let trimmed = line.trim();

    // `$$M=` introduces a mark SongBeamer has no keyword for. It cannot be
    // numbered, and everything behind the `=` is its name.
    if trimmed.len() >= 4 && trimmed[..4].eq_ignore_ascii_case("$$M=") {
        let name = trimmed[4..].trim();
        return Some(ClassifiedMark {
            part_type: SongPartType::Other,
            number: None,
            custom_name: (!name.is_empty()).then(|| name.to_string()),
        });
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() > 2 {
        return None;
    }

    // A trailing ':' or '.' is a habit of hand-written files, not part of the
    // keyword.
    let keyword = normalise_mark(tokens[0].trim_end_matches([':', '.']));
    let part_type = MARKS
        .iter()
        .find(|(name, _)| *name == keyword)
        .map(|(_, part_type)| *part_type)?;

    let number = match tokens.get(1) {
        // A second token that is not a number means this is not a mark.
        Some(token) => Some(token.parse::<u32>().ok()?),
        None => None,
    };

    Some(ClassifiedMark {
        part_type,
        number,
        custom_name: None,
    })
}

/// Reduce a mark keyword to its comparable form: lower case, without separators
/// and with the German umlaut of `Brücke` spelled out.
fn normalise_mark(keyword: &str) -> String {
    keyword
        .chars()
        .flat_map(|c| match c {
            'ü' | 'Ü' => vec!['u', 'e'],
            'ö' | 'Ö' => vec!['o', 'e'],
            'ä' | 'Ä' => vec!['a', 'e'],
            'ß' => vec!['s', 's'],
            other => other.to_lowercase().collect(),
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// One block of the body: everything between two `---` lines.
#[derive(Default)]
struct Block {
    /// The verse mark on its first line, if it had one.
    mark: Option<ClassifiedMark>,
    /// The original wording of that mark, kept as the part's label.
    label: Option<String>,
    /// The slides of the block; `--` starts a new one.
    slides: Vec<Vec<String>>,
}

impl Block {
    /// Whether the block carries any text at all.
    fn is_empty(&self) -> bool {
        self.slides
            .iter()
            .all(|slide| slide.iter().all(|line| line.trim().is_empty()))
    }
}

fn parse(content: &str, settings: &SngImportSettings) -> Result<Song, Box<dyn Error>> {
    // SongBeamer writes CRLF; normalise so that the rest of the parser only
    // ever sees '\n'.
    let normalised = content
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");

    let mut header: Vec<(String, String)> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut in_body = false;

    for line in normalised.lines() {
        let trimmed = line.trim();

        if !in_body && trimmed.starts_with('#') {
            if let Some((tag, value)) = trimmed.split_once('=') {
                header.push((tag.trim().to_string(), value.trim().to_string()));
            }
            continue;
        }

        if trimmed.starts_with("---") {
            // The first separator only opens the body; every later one starts
            // the next part.
            in_body = true;
            blocks.push(Block {
                slides: vec![Vec::new()],
                ..Block::default()
            });
            continue;
        }

        if !in_body {
            // Junk between the header and the first separator; SongBeamer
            // ignores it and so do we.
            continue;
        }

        // Unwrap is safe: entering the body always pushes a block first.
        let block = blocks.last_mut().unwrap();

        // '--' (also '--A' and friends) breaks the slide without ending the
        // part.
        if trimmed.starts_with("--") {
            block.slides.push(Vec::new());
            continue;
        }

        block.slides.last_mut().unwrap().push(strip_markup(line));
    }

    if !in_body {
        return Err("not a SongBeamer file: there is no '---' separator".into());
    }

    // The first line of a block may name it.
    for block in &mut blocks {
        let first = block
            .slides
            .first()
            .and_then(|slide| slide.first())
            .cloned();
        if let Some(first) = first
            && let Some(mark) = classify_mark(&first)
        {
            block.label = Some(first.trim().to_string());
            block.mark = Some(mark);
            block.slides[0].remove(0);
        }
    }

    let language_count = header
        .iter()
        .find(|(tag, _)| tag.eq_ignore_ascii_case("#LangCount"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .unwrap_or(1);

    let mut song = Song::new("");
    apply_header(&mut song, &header, settings);

    for block in &blocks {
        if block.is_empty() && block.mark.is_none() {
            continue;
        }
        add_block(&mut song, block, language_count, settings);
    }

    apply_verse_order(&mut song, &header);

    Ok(song)
}

/// Turn one block into a song part.
fn add_block(song: &mut Song, block: &Block, language_count: usize, settings: &SngImportSettings) {
    let (part_type, number) = match &block.mark {
        Some(mark) => (mark.part_type, mark.number),
        // A block without a mark is a verse — that is what a file that uses no
        // marks at all consists of.
        None => (SongPartType::Verse, None),
    };

    let id = song.add_part_of_type(part_type, number);
    // Unwrap is safe: the part was just added.
    let part = song.part_mut(&id).unwrap();

    // A `$$M=…` mark names the section itself; an ordinary mark keeps its
    // original wording so that "Strophe" does not silently become "Verse".
    part.label = block
        .mark
        .as_ref()
        .and_then(|mark| mark.custom_name.clone())
        .or_else(|| block.label.clone());

    for language_index in 0..language_count {
        let text = language_text(block, language_index, language_count);
        if text.is_empty() {
            continue;
        }
        part.add_content(SongPartContent::lyrics(
            language_of(language_index, settings),
            text,
        ));
    }
}

/// Collect one language's lines out of a block.
///
/// Inside a slide the languages take turns line by line, so the lines of
/// language *n* are those whose index leaves remainder *n*. Slides are joined
/// by a blank line — the data model's stand-in for the `--` slide break.
fn language_text(block: &Block, language_index: usize, language_count: usize) -> String {
    let slides: Vec<String> = block
        .slides
        .iter()
        .map(|slide| {
            slide
                .iter()
                .enumerate()
                .filter(|(index, _)| index % language_count == language_index)
                .map(|(_, line)| line.trim_end().to_string())
                .collect::<Vec<String>>()
                .join("\n")
        })
        .map(|slide| slide.trim().to_string())
        .filter(|slide| !slide.is_empty())
        .collect();

    slides.join("\n\n")
}

/// The [`LyricLanguage`] of the *n*-th language of a file.
fn language_of(index: usize, settings: &SngImportSettings) -> LyricLanguage {
    match settings.languages.get(index) {
        Some(code) => LyricLanguage::specific(code),
        // Nothing in the file names the languages. The first one is the song's
        // own, the others get a placeholder code so that they stay apart and
        // can be renamed later.
        None if index == 0 => LyricLanguage::Default,
        None => LyricLanguage::specific(&format!("lang{}", index + 1)),
    }
}

/// Remove SongBeamer's inline formatting tags.
///
/// The format allows a handful of HTML-like tags (`<b>`, `<i>`, `<color …>`, …)
/// to style a line. They say nothing about the song, and every exporter of this
/// crate works on plain text, so they are dropped rather than translated.
fn strip_markup(line: &str) -> String {
    /// The tag names SongBeamer uses. Anything else — a `<` in the lyrics, say —
    /// is left alone.
    const TAGS: &[&str] = &[
        "b",
        "i",
        "u",
        "p",
        "super",
        "sub",
        "br",
        "wordwrap",
        "strike",
        "align",
        "valign",
        "linespacing",
        "color",
        "size",
        "font",
        "h",
        "s",
        "c",
    ];

    let mut output = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(start) = rest.find('<') {
        let Some(length) = rest[start..].find('>').map(|end| end + 1) else {
            // An unclosed '<' is not a tag.
            break;
        };
        let inner = &rest[start + 1..start + length - 1];
        let name = inner
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .next()
            .unwrap_or("")
            .to_lowercase();

        output.push_str(&rest[..start]);
        if !TAGS.contains(&name.as_str()) {
            // Not one of ours — keep it verbatim.
            output.push_str(&rest[start..start + length]);
        }
        rest = &rest[start + length..];
    }

    output.push_str(rest);
    output
}

// ---------------------------------------------------------------------------
// The header
// ---------------------------------------------------------------------------

/// Copy the header into the song's title, tags and score settings.
///
/// Tags that describe *the song* are mapped onto this crate's names, so that a
/// SongBeamer file and a `.song.yml` file yield the same metadata. Tags that
/// describe *SongBeamer's presentation* — fonts, colours, background images —
/// are dropped: they mean nothing outside the program.
fn apply_header(song: &mut Song, header: &[(String, String)], settings: &SngImportSettings) {
    if let Some(first) = settings.languages.first() {
        song.default_language = Some(first.trim().to_lowercase());
    }

    for (tag, value) in header {
        if value.is_empty() {
            continue;
        }
        let key = tag.trim_start_matches('#').to_lowercase();

        match key.as_str() {
            "title" => song.title = value.clone(),
            "otitle" => song.set_tag("original_title", value),
            "author" => song.set_tag("author", value),
            "melody" => song.set_tag("composer", value),
            "translation" => song.set_tag("translation", value),
            "(c)" => song.set_tag("copyright", value),
            "natcopyright" => song.set_tag("national_copyright", value),
            "rights" => song.set_tag("rights", value),
            "ccli" => song.set_tag("ccli_song_number", value),
            "churchsongid" => song.set_tag("church_song_id", value),
            "bible" => song.set_tag("bible", value),
            "categories" => song.set_tag("categories", value),
            "keywords" => song.set_tag("keywords", value),
            "tempo" | "speed" => song.set_tag("tempo", value),
            // A songbook reference is written as "Book / 123".
            "songbook" => match value.split_once('/') {
                Some((book, number)) => {
                    song.set_tag("songbook", book.trim());
                    let number = number.trim();
                    if !number.is_empty() {
                        song.set_tag("song_number", number);
                    }
                }
                None => song.set_tag("songbook", value),
            },
            "key" => {
                song.set_tag("key", value);
                if let Some(lilypond) = key_to_lilypond(value) {
                    song.score.key = Some(lilypond);
                }
            }
            // Both of these are base64 in a well-formed file, but hand-edited
            // files carry them as plain text.
            "comments" => song.set_tag("comments", &decode_text_field(value)),
            "chords" => song.set_tag("songbeamer_chords", &decode_text_field(value)),
            _ => {}
        }
    }
}

/// Decode a base64 field, falling back to the raw value.
fn decode_text_field(value: &str) -> String {
    match base64::decode(value) {
        Some(bytes) => decode_bytes(&bytes).replace("\r\n", "\n").replace('\r', "\n"),
        None => value.to_string(),
    }
}

/// Read `#VerseOrder` into the song's singing order.
///
/// Marks the song has no part for are skipped rather than invented, and a file
/// without a usable order falls back to the guess every other importer makes.
fn apply_verse_order(song: &mut Song, header: &[(String, String)]) {
    let order = header
        .iter()
        .find(|(tag, _)| tag.eq_ignore_ascii_case("#VerseOrder"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();

    let ids: Vec<SongPartId> = order
        .split(',')
        .filter_map(classify_mark)
        .map(|mark| SongPartId::new(mark.part_type, mark.number.unwrap_or(1)))
        .filter(|id| song.part(id).is_some())
        .collect();

    if ids.is_empty() {
        song.add_guessed_part_order();
    } else {
        song.part_orders.push(PartOrder::new(
            PartOrderName::Default,
            PartOrderRule::Custom(ids),
        ));
    }
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// Convert a SongBeamer key into the LilyPond spelling used by
/// [`crate::song::ScoreSettings::key`].
///
/// SongBeamer writes a chord symbol: `C`, `Am`, `F#`, `B<` (its notation for a
/// flat). Returns `None` for anything that is not a key, which leaves the
/// score settings untouched rather than filling them with nonsense.
///
/// ```
/// use cantara_songlib::importer::songbeamer::key_to_lilypond;
///
/// assert_eq!(key_to_lilypond("C").as_deref(), Some("c major"));
/// assert_eq!(key_to_lilypond("Am").as_deref(), Some("a minor"));
/// assert_eq!(key_to_lilypond("F#").as_deref(), Some("fis major"));
/// assert_eq!(key_to_lilypond("B<").as_deref(), Some("bes major"));
/// assert_eq!(key_to_lilypond("nonsense"), None);
/// ```
pub fn key_to_lilypond(key: &str) -> Option<String> {
    let trimmed = key.trim();
    let mut characters = trimmed.chars();

    let letter = characters.next()?.to_ascii_lowercase();
    if !('a'..='g').contains(&letter) {
        return None;
    }

    let rest = characters.as_str();
    // '<' is SongBeamer's flat sign; 'b' and '#' are the usual ones.
    let (accidental, rest) = match rest.chars().next() {
        Some('#') | Some('♯') => ("is", &rest[rest.char_indices().nth(1).map_or(rest.len(), |(i, _)| i)..]),
        Some('<') | Some('♭') | Some('b') => (
            if matches!(letter, 'a' | 'e') { "s" } else { "es" },
            &rest[rest.char_indices().nth(1).map_or(rest.len(), |(i, _)| i)..],
        ),
        _ => ("", rest),
    };

    let mode = match rest.trim().to_lowercase().as_str() {
        "" | "dur" | "maj" | "major" => "major",
        "m" | "mi" | "min" | "moll" | "minor" => "minor",
        // A chord symbol with a suffix ("C7", "Csus4") still names its key.
        other if other.starts_with('m') && !other.starts_with("maj") => "minor",
        _ => "major",
    };

    Some(format!("{}{} {}", letter, accidental, mode))
}

/// The inverse of [`key_to_lilypond`]: `"bes major"` → `"B<"`.
///
/// Used by [`crate::exporter::songbeamer`] to write `#Key` back out.
pub fn key_from_lilypond(key: &str) -> Option<String> {
    let mut words = key.trim().to_lowercase();
    let mode = if words.ends_with("minor") || words.ends_with("aeolian") {
        "m"
    } else {
        ""
    };
    words = words
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();

    let letter = words.chars().next()?;
    if !('a'..='g').contains(&letter) {
        return None;
    }

    let accidental = match &words[1..] {
        "is" => "#",
        "es" | "s" => "<",
        "" => "",
        _ => return None,
    };

    Some(format!(
        "{}{}{}",
        letter.to_ascii_uppercase(),
        accidental,
        mode
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::SongPartType;

    fn lyrics_of(song: &Song, id: &str) -> String {
        song.part(&id.parse().unwrap())
            .unwrap_or_else(|| panic!("no part {}", id))
            .lyrics_for(None, song.default_language.as_deref())
            .unwrap()
            .content
            .clone()
    }

    // --- The body --------------------------------------------------------

    #[test]
    fn test_a_minimal_file() {
        let song = import_from_sng_string(
            "#Title=Test\n---\nVerse 1\nline one\nline two\n---\nRefrain\nchorus line\n",
        )
        .unwrap();

        assert_eq!(song.title, "Test");
        assert_eq!(song.part_count(), 2);
        assert_eq!(lyrics_of(&song, "verse.1"), "line one\nline two");
        assert_eq!(lyrics_of(&song, "refrain.1"), "chorus line");
    }

    /// A file without any marks is all verses — nothing may be dropped.
    #[test]
    fn test_a_file_without_marks() {
        let song =
            import_from_sng_string("#Title=Test\n---\nfirst block\n---\nsecond block\n").unwrap();

        assert_eq!(song.part_count_of_type(SongPartType::Verse), 2);
        assert_eq!(lyrics_of(&song, "verse.1"), "first block");
        assert_eq!(lyrics_of(&song, "verse.2"), "second block");
    }

    /// `--` breaks a slide but not the part; the break survives as a blank line.
    #[test]
    fn test_slide_breaks_stay_inside_the_part() {
        let song = import_from_sng_string(
            "#Title=Test\n---\nVerse 1\nfirst slide\n--\nsecond slide\n---\nVerse 2\nlater\n",
        )
        .unwrap();

        assert_eq!(song.part_count(), 2);
        assert_eq!(lyrics_of(&song, "verse.1"), "first slide\n\nsecond slide");
    }

    /// The number on the mark decides the part's number, not the block's
    /// position.
    #[test]
    fn test_mark_numbers_are_honoured() {
        let song =
            import_from_sng_string("#Title=T\n---\nVerse 3\nthird\n---\nVerse 1\nfirst\n").unwrap();

        assert_eq!(lyrics_of(&song, "verse.3"), "third");
        assert_eq!(lyrics_of(&song, "verse.1"), "first");
    }

    #[test]
    fn test_the_original_mark_wording_is_kept() {
        let song = import_from_sng_string("#Title=T\n---\nStrophe 1\ntext\n").unwrap();
        let part = song.part(&"verse.1".parse().unwrap()).unwrap();

        assert_eq!(part.part_type, SongPartType::Verse);
        assert_eq!(part.label.as_deref(), Some("Strophe 1"));
    }

    #[test]
    fn test_custom_marks() {
        let song = import_from_sng_string("#Title=T\n---\n$$M=Zwischenteil\ntext\n").unwrap();
        let part = song.part_at(0).unwrap();

        assert_eq!(part.part_type, SongPartType::Other);
        assert_eq!(part.label.as_deref(), Some("Zwischenteil"));
        assert_eq!(part.lyrics_for(None, None).unwrap().content, "text");
    }

    /// A lyric line that begins with a mark keyword must stay lyrics.
    #[test]
    fn test_lyrics_are_not_mistaken_for_a_mark() {
        let song =
            import_from_sng_string("#Title=T\n---\nRefrain of the angels above\nsecond line\n")
                .unwrap();

        assert_eq!(song.part_count_of_type(SongPartType::Verse), 1);
        assert_eq!(
            lyrics_of(&song, "verse.1"),
            "Refrain of the angels above\nsecond line"
        );
    }

    #[test]
    fn test_formatting_tags_are_stripped() {
        let song = import_from_sng_string(
            "#Title=T\n---\nVerse 1\n<b>bold</b> and <i>italic</i>\n<color=255>coloured</color>\n",
        )
        .unwrap();

        assert_eq!(lyrics_of(&song, "verse.1"), "bold and italic\ncoloured");
    }

    /// A '<' that is not a tag is part of the text.
    #[test]
    fn test_a_stray_angle_bracket_is_kept() {
        let song = import_from_sng_string("#Title=T\n---\n1 < 2 and <unknown>\n").unwrap();
        assert_eq!(lyrics_of(&song, "verse.1"), "1 < 2 and <unknown>");
    }

    #[test]
    fn test_crlf_line_endings() {
        let song =
            import_from_sng_string("#Title=T\r\n---\r\nVerse 1\r\nline one\r\nline two\r\n").unwrap();

        assert_eq!(lyrics_of(&song, "verse.1"), "line one\nline two");
        assert!(!lyrics_of(&song, "verse.1").contains('\r'));
    }

    #[test]
    fn test_a_file_without_a_separator_is_rejected() {
        assert!(import_from_sng_string("#Title=T\nsome text\n").is_err());
        assert!(import_from_sng_string("").is_err());
    }

    // --- The header ------------------------------------------------------

    #[test]
    fn test_metadata_is_mapped_onto_this_crates_names() {
        let song = import_from_sng_string(
            "#Title=Amazing Grace\n\
             #Author=John Newton\n\
             #Melody=New Britain\n\
             #(c)=Public Domain\n\
             #CCLI=22025\n\
             #Songbook=Feiert Jesus 3 / 42\n\
             #Bible=Eph 2,8\n\
             #Categories=Gnade,Lobpreis\n\
             #Key=F\n\
             ---\ntext\n",
        )
        .unwrap();

        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.tag("author").unwrap(), "John Newton");
        assert_eq!(song.tag("composer").unwrap(), "New Britain");
        assert_eq!(song.tag("copyright").unwrap(), "Public Domain");
        assert_eq!(song.tag("ccli_song_number").unwrap(), "22025");
        assert_eq!(song.tag("songbook").unwrap(), "Feiert Jesus 3");
        assert_eq!(song.tag("song_number").unwrap(), "42");
        assert_eq!(song.tag("bible").unwrap(), "Eph 2,8");
        assert_eq!(song.tag("categories").unwrap(), "Gnade,Lobpreis");
        assert_eq!(song.score.key.as_deref(), Some("f major"));
    }

    /// Presentation settings say nothing about the song and must not end up in
    /// its metadata.
    #[test]
    fn test_presentation_tags_are_dropped() {
        let song = import_from_sng_string(
            "#Title=T\n#Font=Arial\n#FontSize=40\n#BackgroundImage=wood.jpg\n#TextAlign=Left\n---\ntext\n",
        )
        .unwrap();

        assert_eq!(song.tags().len(), 0);
    }

    #[test]
    fn test_base64_comments() {
        let encoded = crate::base64::encode("Nur die erste Strophe".as_bytes());
        let song =
            import_from_sng_string(&format!("#Title=T\n#Comments={}\n---\ntext\n", encoded))
                .unwrap();

        assert_eq!(song.tag("comments").unwrap(), "Nur die erste Strophe");
    }

    /// Hand-edited files carry the comment as plain text; it must not be lost.
    #[test]
    fn test_a_plain_text_comment_survives() {
        let song =
            import_from_sng_string("#Title=T\n#Comments=Nur Strophe 1!\n---\ntext\n").unwrap();
        assert_eq!(song.tag("comments").unwrap(), "Nur Strophe 1!");
    }

    #[test]
    fn test_verse_order() {
        let song = import_from_sng_string(
            "#Title=T\n#VerseOrder=Verse 1,Refrain,Verse 2,Refrain\n\
             ---\nVerse 1\none\n---\nRefrain\nref\n---\nVerse 2\ntwo\n",
        )
        .unwrap();

        let sung: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(sung, ["verse.1", "refrain.1", "verse.2", "refrain.1"]);
    }

    /// An order naming a part the file does not contain must not invent it.
    #[test]
    fn test_verse_order_skips_unknown_parts() {
        let song = import_from_sng_string(
            "#Title=T\n#VerseOrder=Verse 1,Bridge,Verse 2\n---\nVerse 1\none\n---\nVerse 2\ntwo\n",
        )
        .unwrap();

        let sung: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(sung, ["verse.1", "verse.2"]);
    }

    #[test]
    fn test_without_a_verse_order_the_order_is_guessed() {
        let song =
            import_from_sng_string("#Title=T\n---\nVerse 1\none\n---\nRefrain\nref\n").unwrap();

        assert_eq!(song.part_orders.len(), 1);
        let sung: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(sung, ["verse.1", "refrain.1"]);
    }

    // --- Languages -------------------------------------------------------

    #[test]
    fn test_two_languages_are_split_apart() {
        let song = import_from_sng_string_with(
            "#Title=T\n#LangCount=2\n---\nVerse 1\nZeile eins\nline one\nZeile zwei\nline two\n",
            &SngImportSettings {
                languages: vec!["de".to_string(), "en".to_string()],
            },
        )
        .unwrap();

        assert_eq!(song.default_language.as_deref(), Some("de"));
        let part = song.part(&"verse.1".parse().unwrap()).unwrap();
        assert_eq!(
            part.lyrics_for(Some("de"), None).unwrap().content,
            "Zeile eins\nZeile zwei"
        );
        assert_eq!(
            part.lyrics_for(Some("en"), None).unwrap().content,
            "line one\nline two"
        );
    }

    /// Without language codes the languages still have to stay apart.
    #[test]
    fn test_unnamed_languages_get_placeholder_codes() {
        let song = import_from_sng_string(
            "#Title=T\n#LangCount=2\n---\nVerse 1\nZeile eins\nline one\n",
        )
        .unwrap();

        let part = song.part(&"verse.1".parse().unwrap()).unwrap();
        assert_eq!(part.all_lyrics().count(), 2);
        assert_eq!(
            part.lyrics_in(&LyricLanguage::Default).unwrap().content,
            "Zeile eins"
        );
        assert_eq!(
            part.lyrics_in(&LyricLanguage::specific("lang2"))
                .unwrap()
                .content,
            "line one"
        );
    }

    // --- Encoding --------------------------------------------------------

    #[test]
    fn test_cp1252_without_a_bom() {
        // "Grüße" in cp1252.
        let bytes = b"#Title=Gr\xfc\xdfe\r\n---\r\nText\r\n";
        let song = import_from_bytes(bytes).unwrap();
        assert_eq!(song.title, "Grüße");
    }

    #[test]
    fn test_utf8_with_a_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("#Title=Grüße\r\n---\r\nText\r\n".as_bytes());
        assert_eq!(import_from_bytes(&bytes).unwrap().title, "Grüße");
    }

    #[test]
    fn test_utf16_in_both_byte_orders() {
        let text = "#Title=Grüße\r\n---\r\nText\r\n";

        let mut little = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            little.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(import_from_bytes(&little).unwrap().title, "Grüße");

        let mut big = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            big.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(import_from_bytes(&big).unwrap().title, "Grüße");
    }

    #[test]
    fn test_cp1252_typographic_characters() {
        // 0x93/0x94 are the curly double quotes, 0x96 an en dash.
        assert_eq!(decode_bytes(b"\x93a\x94\x96"), "“a”–");
    }

    // --- Marks and keys --------------------------------------------------

    #[test]
    fn test_mark_vocabulary() {
        let cases = [
            ("Verse 2", SongPartType::Verse, Some(2)),
            ("Vers 2", SongPartType::Verse, Some(2)),
            ("Strophe", SongPartType::Verse, None),
            ("Refrain", SongPartType::Refrain, None),
            ("Chorus", SongPartType::Chorus, None),
            ("Pre-Chorus", SongPartType::PreChorus, None),
            ("Vorrefrain", SongPartType::PreChorus, None),
            ("Brücke", SongPartType::Bridge, None),
            ("Bridge 1", SongPartType::Bridge, Some(1)),
            ("Zwischenspiel", SongPartType::Interlude, None),
            ("Coda", SongPartType::Outro, None),
            ("Intro", SongPartType::Intro, None),
            ("Teil 3", SongPartType::Other, Some(3)),
            // Case and a trailing colon do not matter.
            ("REFRAIN:", SongPartType::Refrain, None),
        ];

        for (line, part_type, number) in cases {
            let mark = classify_mark(line).unwrap_or_else(|| panic!("{} was not a mark", line));
            assert_eq!(mark.part_type, part_type, "for {}", line);
            assert_eq!(mark.number, number, "for {}", line);
        }

        assert_eq!(classify_mark("Verse two"), None);
        assert_eq!(classify_mark("Amazing grace"), None);
        assert_eq!(classify_mark(""), None);
    }

    #[test]
    fn test_key_conversion_round_trips() {
        for (sng, lilypond) in [
            ("C", "c major"),
            ("Am", "a minor"),
            ("F#", "fis major"),
            ("B<", "bes major"),
            ("E<", "es major"),
            ("Dm", "d minor"),
        ] {
            assert_eq!(key_to_lilypond(sng).as_deref(), Some(lilypond), "{}", sng);
            assert_eq!(key_from_lilypond(lilypond).as_deref(), Some(sng), "{}", sng);
        }

        // 'Bb' is the other common spelling of a flat.
        assert_eq!(key_to_lilypond("Bb").as_deref(), Some("bes major"));
        assert_eq!(key_to_lilypond("Hallo"), None);
        assert_eq!(key_from_lilypond("not a key"), None);
    }
}
