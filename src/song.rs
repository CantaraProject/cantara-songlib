//! The in-memory data model for a song.
//!
//! Every importer produces a [`Song`], and every exporter consumes one. The
//! model is deliberately format-agnostic: it knows about *structure* (verses,
//! refrains, bridges and the order they are sung in) and about *content*
//! (melodies, chords, lyrics in one or more languages), but nothing about
//! LilyPond, ABC or slides.
//!
//! # Structure
//!
//! ```text
//! Song
//! ├── title, tags (free-form metadata), score (key/time/anacrusis)
//! ├── parts: Vec<SongPart>          ← every distinct block, stored once
//! │   └── SongPart
//! │       ├── id: SongPartId        ← "verse.1", "refrain.1", …
//! │       └── contents: Vec<SongPartContent>
//! │           └── { content_type, content }
//! └── part_orders: Vec<PartOrder>   ← how the parts are strung together
//! ```
//!
//! A [`Song`] **owns** its parts. Anything that needs to refer to a part —
//! a custom singing order, or a part that repeats another part's melody —
//! stores a [`SongPartId`], not a pointer. That keeps the model plain data:
//! it serialises to JSON/YAML without duplicating parts, survives a
//! round trip unchanged, and is `Send + Sync`.
//!
//! # Example
//!
//! ```
//! use cantara_songlib::song::*;
//!
//! let mut song = Song::new("Amazing Grace");
//! song.set_tag("author", "John Newton");
//!
//! let verse = song.add_part_of_type(SongPartType::Verse, None);
//! song.part_mut(&verse).unwrap().add_content(SongPartContent::lyrics(
//!     LyricLanguage::Default,
//!     "A -- ma -- zing grace",
//! ));
//!
//! assert_eq!(verse.to_string(), "verse.1");
//! assert_eq!(song.parts_of_type(SongPartType::Verse).count(), 1);
//! ```

use core::fmt;
use std::collections::BTreeMap;
use std::str::FromStr;

extern crate serde;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Something that can go wrong while building a [`Song`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SongError {
    /// A part with this id is already present. Ids have to be unique inside a
    /// song, otherwise lookups would be ambiguous.
    DuplicatePartId(SongPartId),
    /// A [`SongPartId`] could not be parsed from a string.
    MalformedPartId(String),
}

impl fmt::Display for SongError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SongError::DuplicatePartId(id) => {
                write!(f, "the song already contains a part with the id '{}'", id)
            }
            SongError::MalformedPartId(text) => write!(
                f,
                "'{}' is not a valid song part id (expected '<type>.<number>', e.g. 'verse.1')",
                text
            ),
        }
    }
}

impl std::error::Error for SongError {}

// ---------------------------------------------------------------------------
// Part types
// ---------------------------------------------------------------------------

/// The role a block of a song plays.
#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum SongPartType {
    Verse,
    Chorus,
    Bridge,
    Intro,
    Outro,
    Interlude,
    Instrumental,
    Solo,
    PreChorus,
    PostChorus,
    Refrain,
    Other,
}

impl SongPartType {
    /// The canonical lower-case name used in [`SongPartId`]s, e.g. `"verse"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            SongPartType::Verse => "verse",
            SongPartType::Chorus => "chorus",
            SongPartType::Bridge => "bridge",
            SongPartType::Intro => "intro",
            SongPartType::Outro => "outro",
            SongPartType::Interlude => "interlude",
            SongPartType::Instrumental => "instrumental",
            SongPartType::Solo => "solo",
            SongPartType::PreChorus => "prechorus",
            SongPartType::PostChorus => "postchorus",
            SongPartType::Refrain => "refrain",
            SongPartType::Other => "other",
        }
    }

    /// Whether this type acts as a refrain, i.e. a block that is repeated after
    /// every verse.
    ///
    /// The classic `.song` importer labels such blocks [`SongPartType::Chorus`]
    /// while the `.song.yml` format calls them `refrain`
    /// ([`SongPartType::Refrain`]). Musically they are the same thing, so
    /// anything that reasons about song structure has to treat them alike.
    pub fn is_chorus_like(&self) -> bool {
        matches!(self, SongPartType::Chorus | SongPartType::Refrain)
    }

    /// Whether a block of this type can be sung more than once with all of its
    /// content (lyrics, chords, …).
    pub fn is_repeatable(&self) -> bool {
        match self {
            SongPartType::Chorus
            | SongPartType::PreChorus
            | SongPartType::PostChorus
            | SongPartType::Refrain => true,
            SongPartType::Verse
            | SongPartType::Bridge
            | SongPartType::Intro
            | SongPartType::Outro
            | SongPartType::Interlude
            | SongPartType::Instrumental
            | SongPartType::Solo
            | SongPartType::Other => false,
        }
    }
}

impl fmt::Display for SongPartType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SongPartType {
    type Err = std::convert::Infallible;

    /// Parse a part type, case-insensitively. Unknown names — and the aliases
    /// used by the different file formats — map onto the closest known type;
    /// anything unrecognised becomes [`SongPartType::Other`], so this never
    /// fails.
    ///
    /// ```
    /// use cantara_songlib::song::SongPartType;
    /// assert_eq!("STANZA".parse(), Ok(SongPartType::Verse));
    /// assert_eq!("Refrain".parse(), Ok(SongPartType::Refrain));
    /// assert_eq!("wat".parse(), Ok(SongPartType::Other));
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "verse" | "stanza" => SongPartType::Verse,
            "chorus" => SongPartType::Chorus,
            "bridge" => SongPartType::Bridge,
            "intro" => SongPartType::Intro,
            "outro" => SongPartType::Outro,
            "interlude" => SongPartType::Interlude,
            "instrumental" => SongPartType::Instrumental,
            "solo" => SongPartType::Solo,
            "prechorus" | "pre-chorus" => SongPartType::PreChorus,
            "postchorus" | "post-chorus" => SongPartType::PostChorus,
            "refrain" => SongPartType::Refrain,
            _ => SongPartType::Other,
        })
    }
}

// ---------------------------------------------------------------------------
// Part ids
// ---------------------------------------------------------------------------

/// Identifies a part within a song, e.g. `verse.1` or `refrain.2`.
///
/// The id is a *value*: it is the part's type plus its number, nothing more.
/// Because both components are typed, an id can never drift out of sync with
/// the part it names, and comparing ids is exact.
///
/// Parsing is case-insensitive and accepts the aliases of the supported file
/// formats, so `"Stanza.1"`, `"stanza.1"` and `"verse.1"` all denote the same
/// part. Formatting always produces the canonical lower-case spelling.
///
/// ```
/// use cantara_songlib::song::{SongPartId, SongPartType};
///
/// let id: SongPartId = "Stanza.2".parse().unwrap();
/// assert_eq!(id.part_type, SongPartType::Verse);
/// assert_eq!(id.number, 2);
/// assert_eq!(id.to_string(), "verse.2");
///
/// // The whole string has to be an id — no leading or trailing junk.
/// assert!("please sing verse.1 now".parse::<SongPartId>().is_err());
/// ```
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct SongPartId {
    /// The role this part plays in the song.
    pub part_type: SongPartType,
    /// Distinguishes parts of the same type; 1-based.
    pub number: u32,
}

impl SongPartId {
    /// Build an id from its components.
    pub fn new(part_type: SongPartType, number: u32) -> SongPartId {
        SongPartId { part_type, number }
    }

    /// Parse an id, returning `None` instead of an error.
    ///
    /// Convenience wrapper around the [`FromStr`] implementation for callers
    /// that do not care *why* a string was rejected.
    pub fn parse(id: &str) -> Option<SongPartId> {
        id.parse().ok()
    }
}

impl fmt::Display for SongPartId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.part_type, self.number)
    }
}

impl FromStr for SongPartId {
    type Err = SongError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let text = s.trim();
        let (type_text, number_text) = text
            .rsplit_once('.')
            .ok_or_else(|| SongError::MalformedPartId(s.to_string()))?;

        if type_text.is_empty() || !type_text.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
            return Err(SongError::MalformedPartId(s.to_string()));
        }

        let number: u32 = number_text
            .parse()
            .map_err(|_| SongError::MalformedPartId(s.to_string()))?;

        // `from_str` for SongPartType is infallible.
        let part_type = type_text.parse().unwrap();
        Ok(SongPartId { part_type, number })
    }
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

/// The language of a lyrics block.
///
/// Language codes are normalised to lower case on construction so that `"EN"`
/// and `"en"` compare equal.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub enum LyricLanguage {
    /// No language was stated. Which language this actually is depends on
    /// [`Song::default_language`].
    Default,
    /// A specific language, given as a code such as `"en"` or `"de"`.
    Specific(String),
}

impl LyricLanguage {
    /// Build a [`LyricLanguage::Specific`] with a normalised code.
    ///
    /// ```
    /// use cantara_songlib::song::LyricLanguage;
    /// assert_eq!(LyricLanguage::specific("DE"), LyricLanguage::specific("de"));
    /// ```
    pub fn specific(code: &str) -> LyricLanguage {
        LyricLanguage::Specific(code.trim().to_lowercase())
    }

    /// The language code, or `None` for [`LyricLanguage::Default`].
    pub fn code(&self) -> Option<&str> {
        match self {
            LyricLanguage::Default => None,
            LyricLanguage::Specific(code) => Some(code),
        }
    }

    /// Whether this language matches the requested code.
    ///
    /// [`LyricLanguage::Default`] matches the song's default language, which
    /// therefore has to be passed in.
    pub fn matches(&self, wanted: &str, song_default: Option<&str>) -> bool {
        let wanted = wanted.trim().to_lowercase();
        match self {
            LyricLanguage::Specific(code) => *code == wanted,
            LyricLanguage::Default => song_default
                .map(|default| default.trim().to_lowercase() == wanted)
                .unwrap_or(false),
        }
    }
}

impl fmt::Display for LyricLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LyricLanguage::Default => f.write_str("default"),
            LyricLanguage::Specific(code) => f.write_str(code),
        }
    }
}

/// What a piece of content inside a song part is.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub enum SongPartContentType {
    /// The main melody.
    LeadVoice,
    SupranoVoice,
    AltoVoice,
    TenorVoice,
    BassVoice,
    Instrumental,
    Solo,
    /// Chord symbols.
    Chords,
    /// Sung text in one language.
    Lyrics { language: LyricLanguage },
}

impl SongPartContentType {
    /// Whether this is sung text.
    pub fn is_lyrics(&self) -> bool {
        matches!(self, SongPartContentType::Lyrics { .. })
    }

    /// Whether this is a notated voice, i.e. a melody line rather than lyrics
    /// or chords.
    pub fn is_voice(&self) -> bool {
        matches!(
            self,
            SongPartContentType::LeadVoice
                | SongPartContentType::SupranoVoice
                | SongPartContentType::AltoVoice
                | SongPartContentType::TenorVoice
                | SongPartContentType::BassVoice
        )
    }
}

impl fmt::Display for SongPartContentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SongPartContentType::LeadVoice => f.write_str("LeadVoice"),
            SongPartContentType::SupranoVoice => f.write_str("SupranoVoice"),
            SongPartContentType::AltoVoice => f.write_str("AltoVoice"),
            SongPartContentType::TenorVoice => f.write_str("TenorVoice"),
            SongPartContentType::BassVoice => f.write_str("BassVoice"),
            SongPartContentType::Instrumental => f.write_str("Instrumental"),
            SongPartContentType::Solo => f.write_str("Solo"),
            SongPartContentType::Chords => f.write_str("Chords"),
            SongPartContentType::Lyrics { language } => match language {
                LyricLanguage::Default => f.write_str("Lyrics"),
                LyricLanguage::Specific(code) => write!(f, "Lyrics ({})", code),
            },
        }
    }
}

/// One piece of content inside a song part: a melody, a chord line or the
/// lyrics of one language.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct SongPartContent {
    /// What kind of content this is.
    pub content_type: SongPartContentType,
    /// The content itself, in whatever notation the source format used.
    pub content: String,
}

impl SongPartContent {
    /// Build a content block.
    pub fn new(content_type: SongPartContentType, content: impl Into<String>) -> SongPartContent {
        SongPartContent {
            content_type,
            content: content.into(),
        }
    }

    /// Shorthand for a lyrics block.
    pub fn lyrics(language: LyricLanguage, content: impl Into<String>) -> SongPartContent {
        SongPartContent::new(SongPartContentType::Lyrics { language }, content)
    }
}

// ---------------------------------------------------------------------------
// Song parts
// ---------------------------------------------------------------------------

/// One block of a song — a verse, the refrain, a bridge, … — together with
/// everything that is sung or played during it.
///
/// A part holds every voice, chord line and language variant of its lyrics.
/// Which of them an exporter uses is its own decision.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SongPart {
    /// The role this part plays. Together with [`SongPart::number`] this forms
    /// the part's [`SongPartId`].
    pub part_type: SongPartType,
    /// Distinguishes parts of the same type; 1-based.
    pub number: u32,
    /// The heading the source file gave this part, e.g. `"Vers 1"`,
    /// `"Pre-Chorus"` or `"주 후렴"`.
    ///
    /// The sung structure lives in [`SongPart::part_type`]; this keeps the
    /// original wording so that nothing is lost when a heading does not map
    /// cleanly onto a type — which is common for songs downloaded in a
    /// language the importer has no vocabulary for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Melodies, chords and lyrics belonging to this part.
    pub contents: Vec<SongPartContent>,
    /// Set when this part reuses another part's music, e.g. verse 2 sharing
    /// verse 1's melody. Resolve it with [`Song::voice_for_part`].
    pub is_repetition_of: Option<SongPartId>,
}

impl SongPart {
    /// Create an empty part with the given id.
    pub fn new(id: SongPartId) -> SongPart {
        SongPart {
            part_type: id.part_type,
            number: id.number,
            label: None,
            contents: Vec::new(),
            is_repetition_of: None,
        }
    }

    /// This part's id. It is derived from the type and number, so it can never
    /// disagree with them.
    pub fn id(&self) -> SongPartId {
        SongPartId::new(self.part_type, self.number)
    }

    /// The heading to show a human: the importer's original wording if there
    /// was one, otherwise the id.
    ///
    /// ```
    /// use cantara_songlib::song::*;
    /// let mut part = SongPart::new(SongPartId::new(SongPartType::Verse, 1));
    /// assert_eq!(part.display_label(), "verse.1");
    /// part.label = Some("Vers 1".to_string());
    /// assert_eq!(part.display_label(), "Vers 1");
    /// ```
    pub fn display_label(&self) -> String {
        match &self.label {
            Some(label) => label.clone(),
            None => self.id().to_string(),
        }
    }

    /// Append a content block.
    pub fn add_content(&mut self, content: SongPartContent) {
        self.contents.push(content);
    }

    /// The first content block of exactly this type.
    pub fn content(&self, content_type: &SongPartContentType) -> Option<&SongPartContent> {
        self.contents
            .iter()
            .find(|content| content.content_type == *content_type)
    }

    /// The first notated voice stored directly on this part.
    ///
    /// Returns `None` for a part that only repeats another part's music — use
    /// [`Song::voice_for_part`] to follow that reference.
    pub fn own_voice(&self) -> Option<&SongPartContent> {
        self.contents
            .iter()
            .find(|content| content.content_type.is_voice())
    }

    /// Whether this part carries any lyrics.
    pub fn has_lyrics(&self) -> bool {
        self.contents
            .iter()
            .any(|content| content.content_type.is_lyrics())
    }

    /// Every lyrics block of this part, paired with its language.
    pub fn all_lyrics(&self) -> impl Iterator<Item = (&LyricLanguage, &SongPartContent)> {
        self.contents.iter().filter_map(|content| {
            match &content.content_type {
                SongPartContentType::Lyrics { language } => Some((language, content)),
                _ => None,
            }
        })
    }

    /// The lyrics in exactly this language, without any fallback.
    pub fn lyrics_in(&self, language: &LyricLanguage) -> Option<&SongPartContent> {
        self.all_lyrics()
            .find(|(candidate, _)| *candidate == language)
            .map(|(_, content)| content)
    }

    /// The lyrics to display, with a fallback chain.
    ///
    /// Resolution order:
    /// 1. the requested language, matching [`LyricLanguage::Default`] too when
    ///    `song_default` says that is the same language;
    /// 2. the song's default language;
    /// 3. an unlabelled ([`LyricLanguage::Default`]) block;
    /// 4. whatever lyrics the part has.
    ///
    /// Passing `None` as `wanted` starts at step 2.
    ///
    /// ```
    /// use cantara_songlib::song::*;
    ///
    /// let mut part = SongPart::new(SongPartId::new(SongPartType::Verse, 1));
    /// part.add_content(SongPartContent::lyrics(LyricLanguage::specific("en"), "Hello"));
    /// part.add_content(SongPartContent::lyrics(LyricLanguage::specific("de"), "Hallo"));
    ///
    /// assert_eq!(part.lyrics_for(Some("de"), None).unwrap().content, "Hallo");
    /// // An unknown language falls back rather than returning nothing.
    /// assert_eq!(part.lyrics_for(Some("fr"), Some("en")).unwrap().content, "Hello");
    /// ```
    pub fn lyrics_for(
        &self,
        wanted: Option<&str>,
        song_default: Option<&str>,
    ) -> Option<&SongPartContent> {
        if let Some(wanted) = wanted
            && let Some((_, content)) = self
                .all_lyrics()
                .find(|(language, _)| language.matches(wanted, song_default))
            {
                return Some(content);
            }

        if let Some(default) = song_default
            && let Some((_, content)) = self
                .all_lyrics()
                .find(|(language, _)| language.matches(default, song_default))
            {
                return Some(content);
            }

        self.all_lyrics()
            .find(|(language, _)| **language == LyricLanguage::Default)
            .or_else(|| self.all_lyrics().next())
            .map(|(_, content)| content)
    }

    /// Whether this part's type can be sung more than once.
    pub fn is_repeatable(&self) -> bool {
        self.part_type.is_repeatable()
    }
}

// ---------------------------------------------------------------------------
// Score settings
// ---------------------------------------------------------------------------

/// Notation settings that apply to the whole song.
///
/// These are kept apart from [`Song::tags`] on purpose: tags are free-form
/// metadata for humans (author, copyright, a bible reference), whereas these
/// values are consumed by the sheet-music exporters and have a fixed meaning.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct ScoreSettings {
    /// Key signature in LilyPond spelling, e.g. `"f major"` or `"d minor"`.
    pub key: Option<String>,
    /// Time signature, e.g. `"3/4"`.
    pub time: Option<String>,
    /// Length of the anacrusis as a note denominator: `4` means a quarter note
    /// pickup (LilyPond `\partial 4`).
    pub partial: Option<u32>,
}

impl ScoreSettings {
    /// Whether any setting is present at all.
    pub fn is_empty(&self) -> bool {
        self.key.is_none() && self.time.is_none() && self.partial.is_none()
    }
}

// ---------------------------------------------------------------------------
// Song
// ---------------------------------------------------------------------------

/// A complete song.
///
/// See the [module documentation](self) for the overall shape of the model.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Song {
    /// The title of the song.
    pub title: String,

    /// Which language unlabelled lyrics are written in, e.g. `"de"`.
    pub default_language: Option<String>,

    /// Notation settings for the sheet-music exporters.
    pub score: ScoreSettings,

    /// Free-form metadata (author, copyright, …).
    ///
    /// A [`BTreeMap`] rather than a `HashMap` so that serialising a song twice
    /// produces byte-identical output.
    tags: BTreeMap<String, String>,

    /// Every distinct block of the song, stored exactly once and in the order
    /// it was added.
    parts: Vec<SongPart>,

    /// The ways this song can be sung. The first entry is the default.
    pub part_orders: Vec<PartOrder>,
}

impl Song {
    /// Create an empty song.
    pub fn new(title: &str) -> Song {
        Song {
            title: title.to_string(),
            ..Song::default()
        }
    }

    // --- Metadata -------------------------------------------------------

    /// Set a metadata tag, replacing any previous value.
    pub fn set_tag(&mut self, key: &str, value: &str) {
        self.tags.insert(key.to_string(), value.to_string());
    }

    /// Read a metadata tag.
    pub fn tag(&self, key: &str) -> Option<&String> {
        self.tags.get(key)
    }

    /// Remove a metadata tag, returning the previous value.
    pub fn remove_tag(&mut self, key: &str) -> Option<String> {
        self.tags.remove(key)
    }

    /// All metadata tags, sorted by key.
    pub fn tags(&self) -> &BTreeMap<String, String> {
        &self.tags
    }

    // --- Parts ----------------------------------------------------------

    /// Every part of the song, in insertion order.
    pub fn parts(&self) -> &[SongPart] {
        &self.parts
    }

    /// Whether any part carries notation, following repetition references.
    ///
    /// The notation exporters use this to tell a song they can engrave from a
    /// lyrics-only one, which they typeset as plain text instead.
    pub fn has_voice_content(&self) -> bool {
        self.parts
            .iter()
            .any(|part| self.voice_for_part(part).is_some())
    }

    /// Whether any part carries lyrics.
    pub fn has_lyrics_content(&self) -> bool {
        self.parts.iter().any(|part| part.has_lyrics())
    }

    /// Number the parts of each type through from 1, keeping their order.
    ///
    /// Importers that number parts by position leave gaps behind when a part is
    /// reclassified afterwards: a song that opens with its refrain has that
    /// block imported as `verse.1` and promoted to `chorus.1` once the repeat
    /// shows up, which leaves the remaining verses starting at 2 and no
    /// `verse.1` at all.
    ///
    /// The ids held in [`SongPart::is_repetition_of`] and in the singing orders
    /// are moved along, so the song stays internally consistent.
    ///
    /// ```
    /// use cantara_songlib::song::*;
    /// let mut song = Song::new("Test");
    /// song.add_part_of_type(SongPartType::Verse, Some(2));
    /// song.add_part_of_type(SongPartType::Verse, Some(5));
    /// song.renumber_parts();
    /// assert!(song.part(&"verse.1".parse().unwrap()).is_some());
    /// assert!(song.part(&"verse.2".parse().unwrap()).is_some());
    /// ```
    pub fn renumber_parts(&mut self) {
        let mut next: BTreeMap<SongPartType, u32> = BTreeMap::new();
        let mut mapping: BTreeMap<SongPartId, SongPartId> = BTreeMap::new();

        for part in &self.parts {
            let number = next.entry(part.part_type).or_insert(1);
            mapping.insert(part.id(), SongPartId::new(part.part_type, *number));
            *number += 1;
        }

        // Nothing moved — leave the song untouched rather than rewriting ids
        // that are already right.
        if mapping.iter().all(|(old, new)| old == new) {
            return;
        }

        for part in &mut self.parts {
            let number = mapping
                .get(&part.id())
                .map(|id| id.number)
                .unwrap_or(part.number);
            part.number = number;
        }

        for part in &mut self.parts {
            if let Some(reference) = &part.is_repetition_of
                && let Some(new_id) = mapping.get(reference)
            {
                part.is_repetition_of = Some(*new_id);
            }
        }

        for order in &mut self.part_orders {
            order.remap_ids(&mapping);
        }
    }

    /// Mutable access to every part.
    pub fn parts_mut(&mut self) -> &mut [SongPart] {
        &mut self.parts
    }

    /// How many parts the song has.
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// Look a part up by id.
    ///
    /// ```
    /// use cantara_songlib::song::*;
    /// let mut song = Song::new("Test");
    /// song.add_part_of_type(SongPartType::Verse, Some(1));
    ///
    /// // Ids are values, so spelling and casing cannot get in the way.
    /// assert!(song.part(&"verse.1".parse().unwrap()).is_some());
    /// assert!(song.part(&"Stanza.1".parse().unwrap()).is_some());
    /// ```
    pub fn part(&self, id: &SongPartId) -> Option<&SongPart> {
        self.parts.iter().find(|part| part.id() == *id)
    }

    /// Mutable access to a part by id.
    pub fn part_mut(&mut self, id: &SongPartId) -> Option<&mut SongPart> {
        self.parts.iter_mut().find(|part| part.id() == *id)
    }

    /// The part at a position in the part list.
    pub fn part_at(&self, index: usize) -> Option<&SongPart> {
        self.parts.get(index)
    }

    /// Every part of a given type, in insertion order.
    pub fn parts_of_type(&self, part_type: SongPartType) -> impl Iterator<Item = &SongPart> {
        self.parts
            .iter()
            .filter(move |part| part.part_type == part_type)
    }

    /// How many parts of a given type the song has.
    pub fn part_count_of_type(&self, part_type: SongPartType) -> u32 {
        self.parts_of_type(part_type).count() as u32
    }

    /// Every part that acts as a refrain — see [`SongPartType::is_chorus_like`].
    pub fn chorus_like_parts(&self) -> impl Iterator<Item = &SongPart> {
        self.parts
            .iter()
            .filter(|part| part.part_type.is_chorus_like())
    }

    /// Add a fully built part.
    ///
    /// # Errors
    /// [`SongError::DuplicatePartId`] if a part with the same id already
    /// exists — ids have to stay unique for lookups to be meaningful.
    pub fn add_part(&mut self, part: SongPart) -> Result<SongPartId, SongError> {
        let id = part.id();
        if self.part(&id).is_some() {
            return Err(SongError::DuplicatePartId(id));
        }
        self.parts.push(part);
        Ok(id)
    }

    /// Add an empty part of a given type and return its id.
    ///
    /// Passing `None` as `number` picks the next free number for that type.
    /// A `number` that is already taken is also moved to the next free one, so
    /// this never fails and never creates a duplicate id. Use [`Song::add_part`]
    /// when you would rather be told about a clash.
    ///
    /// ```
    /// use cantara_songlib::song::*;
    /// let mut song = Song::new("Test");
    /// assert_eq!(song.add_part_of_type(SongPartType::Verse, None).number, 1);
    /// assert_eq!(song.add_part_of_type(SongPartType::Verse, None).number, 2);
    /// // 1 is taken, so this becomes 3 rather than a duplicate.
    /// assert_eq!(song.add_part_of_type(SongPartType::Verse, Some(1)).number, 3);
    /// ```
    pub fn add_part_of_type(
        &mut self,
        part_type: SongPartType,
        number: Option<u32>,
    ) -> SongPartId {
        let mut candidate = number.unwrap_or_else(|| self.part_count_of_type(part_type) + 1);
        while self.part(&SongPartId::new(part_type, candidate)).is_some() {
            candidate += 1;
        }

        let id = SongPartId::new(part_type, candidate);
        self.parts.push(SongPart::new(id));
        id
    }

    /// Parts whose content matches `content` exactly (after trimming).
    ///
    /// Used by the classic `.song` importer to recognise a block that is
    /// repeated verbatim, which is how a refrain is detected in that format.
    pub fn parts_with_content(&self, content: &str) -> Vec<&SongPart> {
        let needle = content.trim().to_lowercase();
        self.parts
            .iter()
            .filter(|part| {
                part.contents
                    .iter()
                    .any(|item| item.content.trim().to_lowercase() == needle)
            })
            .collect()
    }

    /// The last part whose content matches `content` exactly.
    pub fn last_part_with_content(&self, content: &str) -> Option<&SongPart> {
        self.parts_with_content(content).last().copied()
    }

    // --- Content --------------------------------------------------------

    /// Every distinct content type used anywhere in the song.
    pub fn content_types(&self) -> Vec<SongPartContentType> {
        let mut types: Vec<SongPartContentType> = Vec::new();
        for part in &self.parts {
            for content in &part.contents {
                if !types.contains(&content.content_type) {
                    types.push(content.content_type.clone());
                }
            }
        }
        types
    }

    /// The notated voice a part is sung to, following
    /// [`SongPart::is_repetition_of`] if the part has no melody of its own.
    ///
    /// A broken or circular chain of repetitions yields `None` rather than
    /// looping forever.
    pub fn voice_for_part<'a>(&'a self, part: &'a SongPart) -> Option<&'a SongPartContent> {
        let mut current = part;
        let mut visited: Vec<SongPartId> = vec![current.id()];

        loop {
            if let Some(voice) = current.own_voice() {
                return Some(voice);
            }

            let next_id = current.is_repetition_of?;
            if visited.contains(&next_id) {
                // The song describes a cycle; there is no melody to find.
                return None;
            }
            visited.push(next_id);
            current = self.part(&next_id)?;
        }
    }

    /// Every language that appears on a [`LyricLanguage::Specific`] lyrics
    /// block, sorted and without duplicates.
    ///
    /// Unlabelled lyrics are not listed — they belong to
    /// [`Song::default_language`].
    pub fn available_languages(&self) -> Vec<String> {
        let mut languages: Vec<String> = self
            .parts
            .iter()
            .flat_map(|part| part.all_lyrics())
            .filter_map(|(language, _)| language.code().map(|code| code.to_string()))
            .collect();
        languages.sort();
        languages.dedup();
        languages
    }

    // --- Orders ---------------------------------------------------------

    /// Append an order that is guessed from the parts the song has.
    ///
    /// ```
    /// use cantara_songlib::song::*;
    /// let mut song = Song::new("And can it be");
    /// song.add_part_of_type(SongPartType::Verse, None);
    /// song.add_part_of_type(SongPartType::Refrain, None);
    /// song.add_guessed_part_order();
    ///
    /// assert_eq!(song.part_orders.len(), 1);
    /// ```
    pub fn add_guessed_part_order(&mut self) {
        let order = PartOrder::from_guess(self);
        self.part_orders.push(order);
    }

    /// The parts in singing order, using the song's first (default) order.
    ///
    /// Falls back to the plain part list when the song has no order at all.
    pub fn ordered_parts(&self) -> Vec<&SongPart> {
        match self.part_orders.first() {
            Some(order) => order.to_parts(self),
            None => self.parts.iter().collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

/// The name of a [`PartOrder`].
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum PartOrderName {
    /// The song's usual order.
    Default,
    /// A named alternative, e.g. a short version for a service.
    Custom(String),
}

/// One way of stringing a song's parts together.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct PartOrder {
    /// How this order is called.
    pub name: PartOrderName,
    rule: PartOrderRule,
}

impl PartOrder {
    /// Build an order from a name and a rule.
    pub fn new(name: PartOrderName, rule: PartOrderRule) -> PartOrder {
        PartOrder { name, rule }
    }

    /// The rule behind this order.
    pub fn rule(&self) -> &PartOrderRule {
        &self.rule
    }

    /// Rewrite the part ids this order refers to.
    ///
    /// Only [`PartOrderRule::Custom`] names parts explicitly; the other rules
    /// are derived from the part types and need no adjustment.
    fn remap_ids(&mut self, mapping: &BTreeMap<SongPartId, SongPartId>) {
        if let PartOrderRule::Custom(ids) = &mut self.rule {
            for id in ids.iter_mut() {
                if let Some(new_id) = mapping.get(id) {
                    *id = *new_id;
                }
            }
        }
    }

    /// Guess an order from the parts a song has.
    pub fn from_guess(song: &Song) -> PartOrder {
        // With fewer than two parts there is nothing to arrange.
        if song.part_count() < 2 {
            return PartOrder::new(
                PartOrderName::Default,
                PartOrderRule::Custom(song.parts().iter().map(|part| part.id()).collect()),
            );
        }

        // Unwrap is safe: the song has at least two parts.
        let first_type = song.part_at(0).unwrap().part_type;

        if first_type == SongPartType::Verse {
            return PartOrder::new(
                PartOrderName::Default,
                PartOrderRule::VerseRefrainBridgeRefrain,
            );
        }

        if first_type.is_chorus_like() {
            return PartOrder::new(
                PartOrderName::Default,
                PartOrderRule::RefrainVerseBridgeRefrain,
            );
        }

        // Without a refrain or a bridge the verses simply follow each other,
        // which the verse-first rule already describes.
        if song.chorus_like_parts().next().is_none()
            || song.part_count_of_type(SongPartType::Bridge) == 0
        {
            return PartOrder::new(
                PartOrderName::Default,
                PartOrderRule::VerseRefrainBridgeRefrain,
            );
        }

        PartOrder::new(
            PartOrderName::Default,
            PartOrderRule::Custom(song.parts().iter().map(|part| part.id()).collect()),
        )
    }

    /// Whether this order starts with the refrain.
    pub fn is_refrain_first(&self) -> bool {
        matches!(self.rule, PartOrderRule::RefrainVerseBridgeRefrain)
    }

    /// Resolve this order into the ids of the parts to sing, in order.
    ///
    /// Ids of parts the song does not contain are skipped.
    pub fn to_part_ids(&self, song: &Song) -> Vec<SongPartId> {
        match &self.rule {
            PartOrderRule::Custom(ids) => ids
                .iter()
                .filter(|id| song.part(id).is_some())
                .copied()
                .collect(),
            PartOrderRule::VerseRefrainBridgeRefrain => interleave_verses_and_refrain(song, false),
            PartOrderRule::RefrainVerseBridgeRefrain => interleave_verses_and_refrain(song, true),
        }
    }

    /// Resolve this order into the parts to sing, in order.
    pub fn to_parts<'song>(&self, song: &'song Song) -> Vec<&'song SongPart> {
        self.to_part_ids(song)
            .into_iter()
            .filter_map(|id| song.part(&id))
            .collect()
    }
}

/// Build the singing order for the *verse — refrain — … — bridge — refrain*
/// family of rules.
///
/// Every verse is followed by the pre-chorus (if any) and the refrain, and the
/// bridge is inserted before the very last refrain. When `refrain_first` is
/// set, the refrain also opens the song.
///
/// Parts are looked up by *type*, not by their position in the part list, so
/// the result is the same whether the source format interleaves the blocks
/// (classic `.song`) or lists all verses first and the refrain last
/// (`.song.yml`).
fn interleave_verses_and_refrain(song: &Song, refrain_first: bool) -> Vec<SongPartId> {
    let verses: Vec<SongPartId> = song
        .parts_of_type(SongPartType::Verse)
        .map(|part| part.id())
        .collect();

    // These rules describe one refrain and one bridge; anything more complex
    // needs PartOrderRule::Custom.
    let refrain = song.chorus_like_parts().next().map(|part| part.id());
    let prechorus = song
        .parts_of_type(SongPartType::PreChorus)
        .next()
        .map(|part| part.id());
    let bridge = song
        .parts_of_type(SongPartType::Bridge)
        .next()
        .map(|part| part.id());

    // Without verses there is nothing to interleave — keep the original order.
    if verses.is_empty() {
        return song.parts().iter().map(|part| part.id()).collect();
    }

    if refrain.is_none() && bridge.is_none() && prechorus.is_none() {
        return verses;
    }

    let mut order: Vec<SongPartId> = Vec::new();

    if refrain_first
        && let Some(refrain) = refrain {
            order.push(refrain);
        }

    let last_verse = verses.len() - 1;
    for (index, verse) in verses.iter().enumerate() {
        order.push(*verse);

        // The bridge is played right before the final refrain.
        if index == last_verse
            && let Some(bridge) = bridge {
                order.push(bridge);
            }
        if let Some(prechorus) = prechorus {
            order.push(prechorus);
        }
        if let Some(refrain) = refrain {
            order.push(refrain);
        }
    }

    order
}

/// How the parts of a song follow each other.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum PartOrderRule {
    /// The song starts with a verse; the refrain follows every verse and a
    /// bridge is played before the last refrain. Refrain and bridge are
    /// optional — without them the verses simply follow each other.
    ///
    /// Only one refrain and one bridge are honoured. Use
    /// [`PartOrderRule::Custom`] for anything more involved.
    VerseRefrainBridgeRefrain,

    /// Like [`PartOrderRule::VerseRefrainBridgeRefrain`], but the refrain also
    /// opens the song.
    RefrainVerseBridgeRefrain,

    /// An explicit list of parts to sing, by id. The same part may appear more
    /// than once.
    Custom(Vec<SongPartId>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lyrics_part(song: &mut Song, part_type: SongPartType, text: &str) -> SongPartId {
        let id = song.add_part_of_type(part_type, None);
        song.part_mut(&id)
            .unwrap()
            .add_content(SongPartContent::lyrics(LyricLanguage::Default, text));
        id
    }

    // --- Ids ------------------------------------------------------------

    #[test]
    fn test_part_id_roundtrip_is_case_insensitive() {
        let canonical: SongPartId = "verse.1".parse().unwrap();
        assert_eq!("Verse.1".parse::<SongPartId>().unwrap(), canonical);
        assert_eq!("STANZA.1".parse::<SongPartId>().unwrap(), canonical);
        assert_eq!(canonical.to_string(), "verse.1");
    }

    #[test]
    fn test_part_id_rejects_surrounding_junk() {
        // The old regex was unanchored and happily swallowed this.
        assert!("please sing verse.1 now".parse::<SongPartId>().is_err());
        assert!("verse".parse::<SongPartId>().is_err());
        assert!("verse.".parse::<SongPartId>().is_err());
        assert!("verse.x".parse::<SongPartId>().is_err());
        assert!(SongPartId::parse("abcdefg").is_none());
    }

    #[test]
    fn test_part_id_cannot_drift_from_its_part() {
        let mut part = SongPart::new(SongPartId::new(SongPartType::Verse, 1));
        assert_eq!(part.id().to_string(), "verse.1");
        // Changing the type changes the id, with no separate update step.
        part.part_type = SongPartType::Refrain;
        assert_eq!(part.id().to_string(), "refrain.1");
    }

    // --- Parts ----------------------------------------------------------

    #[test]
    fn test_lookup_by_id() {
        let mut song = Song::new("Test");
        song.add_part_of_type(SongPartType::Verse, Some(1));

        assert!(song.part(&"verse.1".parse().unwrap()).is_some());
        assert!(song.part(&"Verse.1".parse().unwrap()).is_some());
        assert!(song.part(&"stanza.1".parse().unwrap()).is_some());
        assert!(song.part(&"verse.2".parse().unwrap()).is_none());
    }

    #[test]
    fn test_add_part_rejects_duplicates() {
        let mut song = Song::new("Test");
        let id = SongPartId::new(SongPartType::Verse, 1);
        assert!(song.add_part(SongPart::new(id)).is_ok());

        let error = song.add_part(SongPart::new(id)).unwrap_err();
        assert_eq!(error, SongError::DuplicatePartId(id));
        assert_eq!(song.part_count(), 1);
    }

    #[test]
    fn test_add_part_of_type_never_duplicates() {
        let mut song = Song::new("Test");
        assert_eq!(song.add_part_of_type(SongPartType::Verse, None).number, 1);
        assert_eq!(song.add_part_of_type(SongPartType::Verse, Some(1)).number, 2);
        assert_eq!(song.add_part_of_type(SongPartType::Verse, Some(5)).number, 5);
        assert_eq!(song.part_count(), 3);
    }

    #[test]
    fn test_part_counts() {
        let mut song = Song::new("Test");
        lyrics_part(&mut song, SongPartType::Verse, "one");
        lyrics_part(&mut song, SongPartType::Verse, "two");
        lyrics_part(&mut song, SongPartType::Refrain, "ref");

        assert_eq!(song.part_count_of_type(SongPartType::Verse), 2);
        assert_eq!(song.part_count_of_type(SongPartType::Refrain), 1);
        assert_eq!(song.chorus_like_parts().count(), 1);
        assert_eq!(song.part_count(), 3);
    }

    // --- Renumbering ----------------------------------------------------

    /// The part ids of a song, in storage order.
    fn ids(song: &Song) -> Vec<String> {
        song.parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect()
    }

    #[test]
    fn test_renumber_closes_gaps_per_type() {
        let mut song = Song::new("Test");
        song.add_part_of_type(SongPartType::Chorus, Some(1));
        song.add_part_of_type(SongPartType::Verse, Some(2));
        song.add_part_of_type(SongPartType::Verse, Some(3));
        song.add_part_of_type(SongPartType::Verse, Some(7));

        song.renumber_parts();

        assert_eq!(ids(&song), ["chorus.1", "verse.1", "verse.2", "verse.3"]);
    }

    #[test]
    fn test_renumber_leaves_a_correct_song_alone() {
        let mut song = Song::new("Test");
        song.add_part_of_type(SongPartType::Verse, Some(1));
        song.add_part_of_type(SongPartType::Chorus, Some(1));
        song.add_part_of_type(SongPartType::Verse, Some(2));

        let before = ids(&song);
        song.renumber_parts();

        assert_eq!(ids(&song), before);
    }

    /// A repetition points at another part by id, so it has to follow the part
    /// it names rather than keep pointing at the old number.
    #[test]
    fn test_renumber_moves_repetition_references_along() {
        let mut song = Song::new("Test");
        let first = song.add_part_of_type(SongPartType::Verse, Some(2));
        song.part_mut(&first)
            .unwrap()
            .add_content(SongPartContent::new(
                SongPartContentType::LeadVoice,
                "c4 d e f",
            ));
        let second = song.add_part_of_type(SongPartType::Verse, Some(3));
        song.part_mut(&second).unwrap().is_repetition_of = Some(first);

        song.renumber_parts();

        let renumbered = song.part(&"verse.2".parse().unwrap()).unwrap();
        assert_eq!(
            renumbered.is_repetition_of,
            Some("verse.1".parse().unwrap()),
            "the reference still points at the old number"
        );
        // The melody is still reachable through the moved reference.
        assert_eq!(song.voice_for_part(renumbered).unwrap().content, "c4 d e f");
    }

    /// An explicit singing order names parts by id and has to be rewritten too.
    #[test]
    fn test_renumber_rewrites_a_custom_order() {
        let mut song = Song::new("Test");
        song.add_part_of_type(SongPartType::Chorus, Some(1));
        song.add_part_of_type(SongPartType::Verse, Some(2));
        song.add_part_of_type(SongPartType::Verse, Some(3));

        song.part_orders.push(PartOrder::new(
            PartOrderName::Default,
            PartOrderRule::Custom(vec![
                "chorus.1".parse().unwrap(),
                "verse.2".parse().unwrap(),
                "chorus.1".parse().unwrap(),
                "verse.3".parse().unwrap(),
            ]),
        ));

        song.renumber_parts();

        let PartOrderRule::Custom(order) = song.part_orders[0].rule() else {
            panic!("the custom rule was replaced");
        };
        let named: Vec<String> = order.iter().map(|id| id.to_string()).collect();
        assert_eq!(named, ["chorus.1", "verse.1", "chorus.1", "verse.2"]);
    }

    // --- Repetitions ----------------------------------------------------

    #[test]
    fn test_voice_is_resolved_through_repetitions() {
        let mut song = Song::new("Test");
        let first = song.add_part_of_type(SongPartType::Verse, None);
        song.part_mut(&first).unwrap().add_content(SongPartContent::new(
            SongPartContentType::LeadVoice,
            "c4 d e f",
        ));

        let second = song.add_part_of_type(SongPartType::Verse, None);
        song.part_mut(&second).unwrap().is_repetition_of = Some(first);

        let part = song.part(&second).unwrap();
        assert_eq!(song.voice_for_part(part).unwrap().content, "c4 d e f");
    }

    #[test]
    fn test_repetition_cycle_does_not_hang() {
        let mut song = Song::new("Test");
        let a = song.add_part_of_type(SongPartType::Verse, None);
        let b = song.add_part_of_type(SongPartType::Verse, None);
        song.part_mut(&a).unwrap().is_repetition_of = Some(b);
        song.part_mut(&b).unwrap().is_repetition_of = Some(a);

        // Neither part has a melody, so this has to terminate with None.
        assert!(song.voice_for_part(song.part(&a).unwrap()).is_none());
    }

    #[test]
    fn test_repetition_of_missing_part_is_not_fatal() {
        let mut song = Song::new("Test");
        let id = song.add_part_of_type(SongPartType::Verse, None);
        song.part_mut(&id).unwrap().is_repetition_of =
            Some(SongPartId::new(SongPartType::Verse, 99));

        assert!(song.voice_for_part(song.part(&id).unwrap()).is_none());
    }

    // --- Languages ------------------------------------------------------

    #[test]
    fn test_language_codes_are_normalised() {
        assert_eq!(LyricLanguage::specific("EN"), LyricLanguage::specific("en"));
        assert_eq!(LyricLanguage::specific(" de ").code(), Some("de"));
    }

    #[test]
    fn test_available_languages_is_sorted_and_deduplicated() {
        let mut song = Song::new("Test");
        let id = song.add_part_of_type(SongPartType::Verse, None);
        {
            let part = song.part_mut(&id).unwrap();
            part.add_content(SongPartContent::lyrics(LyricLanguage::specific("de"), "a"));
            part.add_content(SongPartContent::lyrics(LyricLanguage::specific("en"), "b"));
            part.add_content(SongPartContent::lyrics(LyricLanguage::Default, "c"));
        }
        let other = song.add_part_of_type(SongPartType::Verse, None);
        song.part_mut(&other)
            .unwrap()
            .add_content(SongPartContent::lyrics(LyricLanguage::specific("de"), "d"));

        assert_eq!(song.available_languages(), vec!["de", "en"]);
    }

    #[test]
    fn test_lyrics_fallback_chain() {
        let mut part = SongPart::new(SongPartId::new(SongPartType::Verse, 1));
        part.add_content(SongPartContent::lyrics(LyricLanguage::specific("en"), "English"));
        part.add_content(SongPartContent::lyrics(LyricLanguage::Default, "Unlabelled"));

        // Exact match wins.
        assert_eq!(part.lyrics_for(Some("en"), None).unwrap().content, "English");
        // An unlabelled block counts as the song's default language.
        assert_eq!(part.lyrics_for(Some("de"), Some("de")).unwrap().content, "Unlabelled");
        // Unknown language falls back to the unlabelled block.
        assert_eq!(part.lyrics_for(Some("fr"), None).unwrap().content, "Unlabelled");
        // No preference at all still yields something.
        assert!(part.lyrics_for(None, None).is_some());
    }

    // --- Orders ---------------------------------------------------------

    #[test]
    fn test_refrain_is_repeated_after_every_verse() {
        let mut song = Song::new("Test");
        lyrics_part(&mut song, SongPartType::Verse, "one");
        lyrics_part(&mut song, SongPartType::Verse, "two");
        lyrics_part(&mut song, SongPartType::Refrain, "ref");
        song.add_guessed_part_order();

        let ids: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(ids, ["verse.1", "refrain.1", "verse.2", "refrain.1"]);
    }

    #[test]
    fn test_refrain_first_order() {
        let mut song = Song::new("Test");
        lyrics_part(&mut song, SongPartType::Refrain, "ref");
        lyrics_part(&mut song, SongPartType::Verse, "one");
        lyrics_part(&mut song, SongPartType::Verse, "two");
        song.add_guessed_part_order();

        assert!(song.part_orders[0].is_refrain_first());
        let ids: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(
            ids,
            ["refrain.1", "verse.1", "refrain.1", "verse.2", "refrain.1"]
        );
    }

    #[test]
    fn test_bridge_comes_before_the_last_refrain() {
        let mut song = Song::new("Test");
        lyrics_part(&mut song, SongPartType::Verse, "one");
        lyrics_part(&mut song, SongPartType::Verse, "two");
        lyrics_part(&mut song, SongPartType::Refrain, "ref");
        lyrics_part(&mut song, SongPartType::Bridge, "bridge");
        song.add_guessed_part_order();

        let ids: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(
            ids,
            ["verse.1", "refrain.1", "verse.2", "bridge.1", "refrain.1"]
        );
    }

    #[test]
    fn test_custom_order_skips_unknown_ids() {
        let mut song = Song::new("Test");
        lyrics_part(&mut song, SongPartType::Verse, "one");
        song.part_orders.push(PartOrder::new(
            PartOrderName::Custom("short".to_string()),
            PartOrderRule::Custom(vec![
                SongPartId::new(SongPartType::Verse, 1),
                SongPartId::new(SongPartType::Refrain, 9),
            ]),
        ));

        let parts = song.part_orders[0].to_parts(&song);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].id().to_string(), "verse.1");
    }

    /// The whole point of referring to parts by id: a song survives being
    /// written out and read back in.
    #[test]
    fn test_custom_order_survives_a_serde_round_trip() {
        let mut song = Song::new("Round trip");
        lyrics_part(&mut song, SongPartType::Verse, "one");
        lyrics_part(&mut song, SongPartType::Refrain, "ref");
        song.part_orders.push(PartOrder::new(
            PartOrderName::Custom("short".to_string()),
            PartOrderRule::Custom(vec![
                SongPartId::new(SongPartType::Refrain, 1),
                SongPartId::new(SongPartType::Verse, 1),
            ]),
        ));

        let json = serde_json::to_string(&song).unwrap();
        let restored: Song = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, song);

        // The order still points at the song's own parts, not at copies.
        let ordered = restored.part_orders[0].to_parts(&restored);
        assert_eq!(ordered.len(), 2);
        for part in ordered {
            assert!(std::ptr::eq(part, restored.part(&part.id()).unwrap()));
        }

        // Parts are stored once, so they appear once in the serialised form.
        assert_eq!(json.matches("\"Refrain\"").count(), 2, "{}", json);
    }

    #[test]
    fn test_serialisation_is_deterministic() {
        let mut song = Song::new("Tags");
        song.set_tag("author", "A");
        song.set_tag("copyright", "C");
        song.set_tag("bible", "B");

        let first = serde_json::to_string(&song).unwrap();
        let second = serde_json::to_string(&song).unwrap();
        assert_eq!(first, second);
        // BTreeMap keeps the keys sorted.
        assert!(first.find("author").unwrap() < first.find("bible").unwrap());
    }

    /// A `Song` is plain data, so it can be moved between threads.
    #[test]
    fn test_song_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Song>();
        assert_send_sync::<SongPart>();
        assert_send_sync::<PartOrder>();
    }

    // --- Metadata -------------------------------------------------------

    #[test]
    fn test_tags() {
        let mut song = Song::new("Test");
        song.set_tag("author", "first");
        song.set_tag("author", "second");
        assert_eq!(song.tag("author").unwrap(), "second");
        assert_eq!(song.tags().len(), 1);
        assert_eq!(song.remove_tag("author").as_deref(), Some("second"));
        assert!(song.tag("author").is_none());
    }

    #[test]
    fn test_content_types_are_distinct() {
        let mut song = Song::new("Test");
        let verse = song.add_part_of_type(SongPartType::Verse, None);
        song.part_mut(&verse).unwrap().add_content(SongPartContent::lyrics(
            LyricLanguage::Default,
            "text",
        ));
        let refrain = song.add_part_of_type(SongPartType::Refrain, None);
        song.part_mut(&refrain)
            .unwrap()
            .add_content(SongPartContent::new(
                SongPartContentType::LeadVoice,
                "c4 d4",
            ));

        assert_eq!(song.content_types().len(), 2);
    }
}
