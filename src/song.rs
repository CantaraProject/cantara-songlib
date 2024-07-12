extern crate regex;
use core::fmt;
use regex::Regex;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

extern crate serde;
use serde::{Deserialize, Serialize};

/// Object which represents a song in Cantara
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Song {
    pub title: String,
    tags: HashMap<String, String>,
    parts: Vec<Rc<RefCell<SongPart>>>,
    part_order: Vec<PartOrderRule>,
}

impl Song {
    /// Create a new song with the given title
    pub fn new(title: &str) -> Song {
        Song {
            title: title.to_string(),
            tags: HashMap::new(),
            parts: Vec::new(),
            part_order: Vec::new(),
        }
    }

    /// Add a tag to the song
    pub fn add_tag(&mut self, key: &str, value: &str) {
        self.tags.insert(key.to_string(), value.to_string());
    }

    /// Get the value of a tag
    pub fn get_tag(&self, key: &str) -> Option<&String> {
        self.tags.get(key)
    }

    /// Add a part to the song
    pub fn add_part(&mut self, part: SongPart) {
        self.parts.push(Rc::new(RefCell::new(part)));
    }

    /// Get the number of parts of a specific type
    /// # Arguments
    /// * `part_type` - The type of the part
    /// # Returns
    /// The number of parts of the given type
    /// # Example
    /// ```
    /// use cantara_songlib::song::{Song, SongPart, SongPartType, SongPartId};
    /// let mut song = Song::new("Test Song");
    /// let part = SongPart::new(SongPartId::parse("verse.1").unwrap(), None);
    /// song.add_part(part);
    /// assert_eq!(song.get_part_count(SongPartType::Verse), 1);
    /// ```
    pub fn get_part_count(&self, part_type: SongPartType) -> u32 {
        let mut count = 0;
        for part_box in &self.parts {
            let part = part_box.borrow();
            if part.part_type.eq(&part_type) {
                count += 1;
            }
        }
        count
    }

    /// Add a song part of a specific type
    /// # Arguments
    /// * `part_type` - The type of the part
    /// * `specific_number` - The number of the part (e.g. 1 for "verse.1")
    /// # Returns
    /// A countable reference in the form Rc<RefCell<SongPart>> of the created song part
    pub fn add_part_of_type(
        &mut self,
        part_type: SongPartType,
        specific_number: Option<u32>,
    ) -> Rc<RefCell<SongPart>> {
        let id = format!(
            "{}.{}",
            part_type.to_string(),
            specific_number.unwrap_or_else(|| self.get_part_count(part_type) + 1)
        );
        
        let part: SongPart = SongPart::new(SongPartId::parse(&id).unwrap(), specific_number);
        self.add_part(part);
        // Unwrap is safe here, because we just added the part
        self.parts.last().unwrap().clone()
    }

    /// Returns a list of all ContentTypes that are used in the song
    /// # Returns
    /// A list of all ContentTypes that are used in the song
    /// # Example
    /// ```
    /// use cantara_songlib::song::{Song, SongPart, SongPartContent, SongPartContentType, LyricLanguage, SongPartId};
    /// let mut song = Song::new("Test Song");
    /// let mut part = SongPart::new(SongPartId::parse("verse.1").unwrap(), Some(1));
    /// part.add_content(SongPartContent {
    ///   voice_type: SongPartContentType::Lyrics {
    ///     language: LyricLanguage::Default
    ///   },
    ///   content: "Amazing Grace, how sweet the sound...".to_string(),
    /// });
    /// song.add_part(part);
    /// let mut part = SongPart::new(SongPartId::parse("chorus.1").unwrap(), Some(1));
    /// part.add_content(SongPartContent {
    ///  voice_type: SongPartContentType::LeadVoice,
    /// content: "c4 d4 e4 f4 g4".to_string(),
    /// });
    /// song.add_part(part);
    ///
    /// let content_types = song.get_content_types();
    /// assert_eq!(content_types.len(), 2);  
    /// ```
    pub fn get_content_types(&self) -> Vec<SongPartContentType> {
        let mut content_types: Vec<SongPartContentType> = Vec::new();
        for part in &self.parts {
            for content in &part.borrow().contents {
                if !content_types.contains(&content.voice_type) {
                    content_types.push(content.voice_type.clone());
                }
            }
        }
        content_types
    }

    /// Finds a content anywhere in the song and returns all positions where it was found
    /// # Arguments
    /// * `content` - The content string to search for
    /// # Returns
    /// A list of references to the song parts where the content was found
    /// # Example
    /// ```
    /// use cantara_songlib::song::{Song, SongPart, SongPartContent, SongPartContentType, LyricLanguage, SongPartId};
    /// let mut song = Song::new("Test Song");
    /// let mut part = SongPart::new(SongPartId::parse("verse.1").unwrap(), Some(1));
    /// part.add_content(SongPartContent {
    ///  voice_type: SongPartContentType::Lyrics {
    ///     language: LyricLanguage::Default
    ///  },
    /// content: "Amazing Grace, how sweet
    /// the sound...".to_string(),
    /// });
    /// song.add_part(part);
    /// let mut part = SongPart::new(SongPartId::parse("chorus.1").unwrap(), Some(1));
    /// part.add_content(SongPartContent {
    /// voice_type: SongPartContentType::LeadVoice,
    /// content: "c4 d4 e4 f4 g4".to_string(),
    /// });
    /// song.add_part(part);
    /// let positions = song.find_content_in_part("Amazing Grace");
    /// assert_eq!(positions.len(), 0);
    /// ```
    /// # Note
    /// The search is case-insensitive
    /// The search is done on the content string of the SongPartContent
    /// # Note
    /// The search is done on the content string of the SongPartContent
    pub fn find_content_in_part(&self, content: &str) -> Vec<Rc<RefCell<SongPart>>> {
        let mut positions: Vec<Rc<RefCell<SongPart>>> = Vec::new();
        for part_refcall in &self.parts {
            let cloned_part_refcall = part_refcall.clone();
            let part = part_refcall.borrow();
            for content_part in &part.contents {
                if content_part.content.to_lowercase().as_str() == content.to_lowercase() {
                    positions.push(cloned_part_refcall.clone());
                }
            }
        }
        positions
    }

    pub fn find_first_content_in_part(&self, content: &str) -> Option<Rc<RefCell<SongPart>>> {
        self.find_content_in_part(content).first().cloned()
    }

    /// Get a part by its ID
    /// # Arguments
    /// * `id` - The ID of the part
    /// # Returns
    /// An Option with the reference to the song part with the given ID
    pub fn get_part_by_id(&self, id: &str) -> Option<Rc<RefCell<SongPart>>> {
        for part_refcall in &self.parts {
            // We need to clone the reference here to avoid consuming of the reference
            let cloned_part_refcall = part_refcall.clone();
            let part = cloned_part_refcall.borrow();
            if part.id.get_id() == id {
                return Some(part_refcall.clone());
            }
        }
        None
    }

    pub fn get_parts_by_type(&self, part_type: SongPartType) -> Vec<Rc<RefCell<SongPart>>> {
        let mut parts: Vec<Rc<RefCell<SongPart>>> = Vec::new();
        for part_refcall in &self.parts {
            let part = part_refcall.borrow();
            if part.part_type == part_type {
                parts.push(part_refcall.clone());
            }
        }
        parts
    }

    /// Unpacks all parts of the song
    /// # Returns
    /// A list of all parts of the song
    /// # Example
    /// ```
    /// use cantara_songlib::song::{Song, SongPart, SongPartContent, SongPartContentType, LyricLanguage, SongPartId};
    /// let mut song = Song::new("Amazing Grace");
    /// let mut part = SongPart::new(SongPartId::parse("verse.1").unwrap(), Some(1));
    /// part.add_content(SongPartContent {
    /// voice_type: SongPartContentType::Lyrics {
    /// language: LyricLanguage::Default
    /// },
    /// content: "Amazing Grace, how sweet the sound that saved a wretch like me!
    ///             I once was lost but now am found, was blind, but know I see!"
    /// .to_string(),
    /// });
    /// song.add_part(part);
    /// let parts = song.get_unpacked_parts();
    /// assert_eq!(parts.len(), 1);
    /// ```
    /// # Note
    /// The parts are returned in the order they were added to the song
    /// After you have unpacked them, modifications to the returned parts will not be reflected in the song.
    pub fn get_unpacked_parts(&self) -> Vec<SongPart> {
        let mut parts: Vec<SongPart> = Vec::new();
        for part_refcall in &self.parts {
            let part = part_refcall.borrow();
            parts.push(part.clone());
        }
        parts
    }

    /// Get the number of parts
    /// # Returns
    /// The number of parts in the song
    pub fn get_total_part_count(&self) -> usize {
        self.parts.len()
    }
}

/// All possible types of a song part. Some are repeatable (like refrains, etc.), some are not.
#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Debug)]
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
    pub fn to_string(&self) -> String {
        match self {
            SongPartType::Verse => "Verse".to_string(),
            SongPartType::Chorus => "Chorus".to_string(),
            SongPartType::Bridge => "Bridge".to_string(),
            SongPartType::Intro => "Intro".to_string(),
            SongPartType::Outro => "Outro".to_string(),
            SongPartType::Interlude => "Interlude".to_string(),
            SongPartType::Instrumental => "Instrumental".to_string(),
            SongPartType::Solo => "Solo".to_string(),
            SongPartType::PreChorus => "PreChorus".to_string(),
            SongPartType::PostChorus => "PostChorus".to_string(),
            SongPartType::Refrain => "Refrain".to_string(),
            SongPartType::Other => "Other".to_string(),
        }
    }

    /// Create a SongPartType from a string (case-insensitive)
    pub fn from_string(s: &str) -> SongPartType {
        // Make the string lowercase
        let s: String = s.to_lowercase();
        match s.as_str() {
            "verse" => SongPartType::Verse,
            "chorus" => SongPartType::Chorus,
            "bridge" => SongPartType::Bridge,
            "intro" => SongPartType::Intro,
            "outro" => SongPartType::Outro,
            "interlude" => SongPartType::Interlude,
            "instrumental" => SongPartType::Instrumental,
            "solo" => SongPartType::Solo,
            "preChorus" => SongPartType::PreChorus,
            "postChorus" => SongPartType::PostChorus,
            "refrain" => SongPartType::Refrain,
            _ => SongPartType::Other,
        }
    }

    /// Returns whether a song part type is repeatable.
    /// A song part is *repeatable* if it can be used multiple times in a song with all of its contents (e.g. lyrics, chords, etc.).
    pub fn is_repeatable(&self) -> bool {
        match self {
            SongPartType::Verse => false,
            SongPartType::Chorus => true,
            SongPartType::Bridge => false,
            SongPartType::Intro => false,
            SongPartType::Outro => false,
            SongPartType::Interlude => false,
            SongPartType::Instrumental => false,
            SongPartType::Solo => false,
            SongPartType::PreChorus => true,
            SongPartType::PostChorus => true,
            SongPartType::Refrain => true,
            SongPartType::Other => false,
        }
    }
}

/// The language of the lyrics in a lyric element of a song content
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum LyricLanguage {
    /// No specific language information is given
    Default,
    /// A specific language is given, in that case, the language code is stored in the string
    Specific(String),
}
/// The type which a song part content element can have
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum SongPartContentType {
    LeadVoice,
    SupranoVoice,
    AltoVoice,
    TenorVoice,
    BassVoice,
    Instrumental,
    Solo,
    Chords,
    Lyrics { language: LyricLanguage },
}

impl SongPartContentType {
    pub fn is_lyrics(&self) -> bool {
        match self {
            SongPartContentType::Lyrics { .. } => true,
            _ => false,
        }
    }
    pub fn to_string(&self) -> String {
        match self {
            SongPartContentType::LeadVoice => "LeadVoice".to_string(),
            SongPartContentType::SupranoVoice => "SupranoVoice".to_string(),
            SongPartContentType::AltoVoice => "AltoVoice".to_string(),
            SongPartContentType::TenorVoice => "TenorVoice".to_string(),
            SongPartContentType::BassVoice => "BassVoice".to_string(),
            SongPartContentType::Instrumental => "Instrumental".to_string(),
            SongPartContentType::Solo => "Solo".to_string(),
            SongPartContentType::Chords => "Chords".to_string(),
            SongPartContentType::Lyrics { language } => match language {
                LyricLanguage::Default => "Lyrics".to_string(),
                LyricLanguage::Specific(lang) => format!("Lyrics ({})", lang),
            },
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SongPartContent {
    pub voice_type: SongPartContentType,
    pub content: String,
}

/// The ID of a song part.
/// The ID is in the format 'part_type.number' (e.g. 'verse.1')
/// Use the parse method to create a SongPartId from a string.
/// In addition, an ID should be unique inside a song. This can only be checked after a SongPart with a certain SongId has been added to a Song.
/// Use the unique method to determine whether the SongPartId has been successfully defined as unique.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SongPartId {
    /// The actual ID of the song part, proven to have a correct format
    id: String,
    /// Whether the ID is unique in the song
    /// This is only checked after the SongPartId has been added to a Song
    /// If the ID is not unique, the SongPartId is not valid
    /// If the ID is unique, the SongPartId is valid
    checked_unique: bool,
}

impl SongPartId {
    /// Parse the ID of a song part by a given &str. The id has to be in the format 'part_type.number' (e.g. 'verse.1')
    /// # Arguments
    /// * `id` - The ID of the song part
    /// # Returns
    /// An Option with the SongPartId, if the ID is in the correct format
    /// or None, if the ID is not in the correct format.
    /// # Example
    /// ```
    /// use cantara_songlib::song::SongPartId;
    /// let id = SongPartId::parse("verse.1");
    /// assert_eq!(id.unwrap().to_string(), "verse.1");
    /// let id = SongPartId::parse("abcdefg");
    /// assert_eq!(id, None);
    /// ```
    /// # Panics
    /// If the ID is not in the format 'part_type.number' (e.g. 'verse.1')
    /// # Note
    /// The ID is case-insensitive
    pub fn parse(id: &str) -> Option<SongPartId> {
        let re: Regex = Regex::new(r"([a-zA-Z]+)\.(\d+)").unwrap();
        let caps_found: bool = re.captures(id).is_some();
        match caps_found {
            true => Some(SongPartId {
                id: id.to_string(),
                checked_unique: false,
            }),
            false => None,
        }
    }

    /// Get whether the SongPartId is unique in a song
    pub fn get_checked_unique(&self) -> bool {
        self.checked_unique
    }

    /// Set whether the SongPartId is unique in a song
    /// # Arguments
    /// * `checked_unique` - Whether the SongPartId is unique in a song
    /// # Note
    /// This is only used internally by the Song struct to set the checked_unique flag
    pub fn set_checked_unique(&mut self, checked_unique: bool) {
        self.checked_unique = checked_unique;
    }

    /// Get the ID of the SongPartId
    /// # Returns
    /// The ID of the SongPartId as a string
    pub fn get_id(&self) -> String {
        self.id.clone()
    }

}

impl fmt::Display for SongPartId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

/// A part of a song, which can contain multiple voices (e.g. lyrics, chords, etc.)
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SongPart {
    /// Every song part has an unique ID which is used to identify the part.
    /// The ID is in the format 'part_type.number' (e.g. 'verse.1')
    pub id: SongPartId,
    /// The type of the part (e.g. Verse, Chorus, Bridge, etc.)
    pub part_type: SongPartType,
    /// The number of the part (e.g. 1 for 'verse.1')
    pub number: u32,
    /// All the contents which the part contains (e.g. lyrics, chords, etc.)
    pub contents: Vec<SongPartContent>,
    /// defines whether this part is a repetition of a previous part
    pub is_repetition_of: Option<Rc<RefCell<SongPart>>>,
    occurs_after: Option<Rc<RefCell<SongPart>>>,
}

impl SongPart {
    pub fn new(id: SongPartId, specific_number: Option<u32>) -> SongPart {
        // get part_type from a regex in the format ('part_type'.'number')
        let re: Regex = Regex::new(r"([a-zA-Z]+)\.(\d+)").unwrap();
        let id_string = id.to_string();
        let caps: regex::Captures = re.captures(&id_string).unwrap();
        let part_type: SongPartType = SongPartType::from_string(&caps[1]);
        let is_repetition: Option<Rc<RefCell<SongPart>>> = None;
        let number: u32 = specific_number.unwrap_or(1);
        SongPart {
            id,
            part_type,
            number,
            contents: Vec::new(),
            is_repetition_of: is_repetition,
            occurs_after: None,
        }
    }

    pub fn add_content(&mut self, content: SongPartContent) {
        self.contents.push(content);
    }

    pub fn get_content(&self, voice_type: SongPartContentType) -> Option<&SongPartContent> {
        self.contents
            .iter()
            .find(|voice| voice.voice_type == voice_type)
    }

    pub fn has_lyrics(&self) -> bool {
        self.contents.iter().any(|voice| match voice.voice_type {
            SongPartContentType::Lyrics { .. } => true,
            _ => false,
        })
    }

    pub fn is_repeatable(&self) -> bool {
        self.part_type.is_repeatable()
    }

    pub fn is_repition(&self) -> Option<Rc<RefCell<SongPart>>> {
        self.is_repetition_of.as_ref().map(|repetition| repetition.clone())
    }

    pub fn set_repition(&mut self, is_repetition: Option<Rc<RefCell<SongPart>>>) {
        self.is_repetition_of = is_repetition.map(|is_repetition| is_repetition.clone());
    }

    pub fn get_occurs_after(&self) -> Option<Rc<RefCell<SongPart>>> {
        self.occurs_after.as_ref().map(|occurs_after| occurs_after.clone())
    }

    pub fn set_occurs_after(&mut self, occurs_after: Option<Rc<RefCell<SongPart>>>) {
        self.occurs_after = occurs_after.map(|occurs_after| occurs_after.clone())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum PartOrderName {
    Default,
    Custom(String),
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PartOrder {
    pub name: PartOrderName,
    partorderrule: PartOrderRule,
}

impl PartOrder {
    pub fn new(name: PartOrderName, partorderrule: PartOrderRule) -> PartOrder {
        PartOrder {
            name,
            partorderrule,
        }
    }
    /// Create a PartOrder which is guessed by the song structure (the parts in the song)
    /// # Arguments
    /// * `song` - The song for which the PartOrder should be guessed
    /// # Returns
    /// A PartOrder which is guessed by the song structure
    pub fn from_guess(song: &Song) -> PartOrder {
        let song_part_count = song.get_total_part_count();

        // If the song has less then two parts, return Custom with the parts in the order they were added
        if song_part_count < 2 {
            return PartOrder::new(
                PartOrderName::Default,
                PartOrderRule::Custom(song.parts.clone()),
            );
        }

        // TODO: If the song begins with a verse, it is likely that the song has the structure VerseRefrainBridgeRefrain
        // TODO: If the song begins with a refrain, it is likely that the song has the structure RefrainVerseBridgeRefrain
        // TODO: If the song has no refrain or bridge, it is likely that the song has the structure VerseRefrainBridgeRefrain

        // In every other case, we have a custom song structure
        PartOrder::new(
            PartOrderName::Default,
            PartOrderRule::Custom(song.parts.clone()),
        )
    }
}

/// A rule which defines the order of the parts in a song
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum PartOrderRule {
    /// This represents a song which begins with a verse followed by the refrain. The refrain is repeated after each verse.
    /// Before the last refrain, a bridge is played.
    /// However, refrains and bridges are not necessary parts of the song.
    /// If the song does not contain a refrain or a bridge, all the stanzas will just be played after each other.
    /// If the song does not contain a bridge, the refrain will be played after the last verse.
    /// In this rule, only one refrain and one bridge are allowed.
    /// Use Custom for more complex song structures.
    VerseRefrainBridgeRefrain,
    /// This represents a song which begins with a refrain followed by the stanza. The refrain is repeated after each verse.
    /// Before the last refrain, a bridge is played.
    /// However, a bridges is not necessary to be part of the song.
    /// If the song does not contain a bridge, the refrain will be played after the last verse.
    /// In this rule, only one refrain and one bridge are allowed.
    /// Use Custom for more complex song structures.
    RefrainVerseBridgeRefrain,
    /// Any song structure which is more complex than the other rules.
    /// In that case, you need to define the order of the parts manually.
    Custom(Vec<Rc<RefCell<SongPart>>>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_song() {
        let song = Song::new("Test Song");

        assert_eq!(song.title, "Test Song");
    }

    #[test]
    fn test_add_tag() {
        let mut song: Song = Song::new("Test Song");
        song.add_tag("key", "value");
        song.add_tag("key2", "value2");
        song.add_tag("key3", "value3");
        assert_eq!(song.get_tag("key").unwrap(), "value");
        assert_eq!(song.get_tag("key2").unwrap(), "value2");
        assert_eq!(song.get_tag("key3").unwrap(), "value3");
    }

    #[test]
    fn test_new_song_part() {
        let part = SongPart::new(SongPartId::parse("verse.1").unwrap(), Some(1));
        assert_eq!(part.part_type, SongPartType::Verse);
        assert_eq!(part.number, 1);
    }

    #[test]
    fn test_adding_song_parts() {
        let mut song: Song = Song::new("Amazing Grace");
        let part: SongPart = SongPart::new(SongPartId::parse("verse.1").unwrap(), None);
        song.add_part(part);
        assert_eq!(song.parts.len(), 1);
        dbg!(song.parts);
    }
}
