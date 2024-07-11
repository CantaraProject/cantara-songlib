use regex::Regex;
use std::{
    cell::RefCell, 
    collections::HashMap, 
    rc::Rc
};
use serde::{Serialize, Deserialize};

/// Object which represents a song in Cantara
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Song {
    pub title: String,
    tags: HashMap<String, String>,
    parts: Vec<Rc<RefCell<SongPart>>>,
    part_order: Vec<Rc<RefCell<SongPart>>>,
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
        self.parts.push(
            Rc::new(RefCell::new(part))
        );
    }

    /// Get the number of parts of a specific type
    /// # Arguments
    /// * `part_type` - The type of the part
    /// # Returns
    /// The number of parts of the given type
    /// # Example
    /// ```
    /// use cantara_songlib::song::{Song, SongPart, SongPartType};
    /// let mut song = cantara_songlib::song::Song::new("Test Song");
    /// let part = cantara_songlib::song::SongPart::new("verse.1", None);
    /// song.add_part(part);
    /// assert_eq!(song.get_part_count(cantara_songlib::song::SongPartType::Verse), 1);
    /// ```
    pub fn get_part_count(&self, part_type: SongPartType) -> u32 {
       let mut count = 0;
       for part_box in &self.parts {
            let part = part_box.borrow();
            if part.part_type.eq(&part_type) {
                count = count + 1; 
            }
       }
       count
    }

    /// Add a song part of a specific type
    /// # Arguments
    /// * `part_type` - The type of the part
    /// * `specific_number` - The number of the part (e.g. 1 for "verse.1")
    /// # Returns
    /// A mutable reference of the created song part
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
        let part: SongPart = SongPart::new(&id, specific_number);
        self.add_part(part);
        self.parts.last().unwrap().clone()
    }

    /// Returns a list of all ContentTypes that are used in the song
    /// # Returns
    /// A list of all ContentTypes that are used in the song
    /// # Example
    /// ```
    /// use cantara_songlib::song::{Song, SongPart, SongPartContent, SongPartContentType, LyricLanguage};
    /// let mut song = cantara_songlib::song::Song::new("Test Song");
    /// let mut part = cantara_songlib::song::SongPart::new("verse.1", Some(1));
    /// part.add_content(cantara_songlib::song::SongPartContent {
    ///   voice_type: cantara_songlib::song::SongPartContentType::Lyrics {
    ///     language: cantara_songlib::song::LyricLanguage::Default
    ///   },
    ///   content: "Amazing Grace, how sweet the sound...".to_string(),
    /// });
    /// song.add_part(part);
    /// let mut part = cantara_songlib::song::SongPart::new("chorus.1", Some(1));
    /// part.add_content(cantara_songlib::song::SongPartContent {
    ///  voice_type: cantara_songlib::song::SongPartContentType::LeadVoice,
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
    /// use cantara_songlib::song::{Song, SongPart, SongPartContent, SongPartContentType, LyricLanguage};
    /// let mut song = cantara_songlib::song::Song::new("Test Song");
    /// let mut part = cantara_songlib::song::SongPart::new("verse.1", Some(1));
    /// part.add_content(cantara_songlib::song::SongPartContent {
    ///  voice_type: cantara_songlib::song::SongPartContentType::Lyrics {
    ///     language: cantara_songlib::song::LyricLanguage::Default
    ///  },
    /// content: "Amazing Grace, how sweet
    /// the sound...".to_string(),
    /// });
    /// song.add_part(part);
    /// let mut part = cantara_songlib::song::SongPart::new("chorus.1", Some(1));
    /// part.add_content(cantara_songlib::song::SongPartContent {
    /// voice_type: cantara_songlib::song::SongPartContentType::LeadVoice,
    /// content: "c4 d4 e4 f4 g4".to_string(),
    /// });
    /// song.add_part(part);
    /// let positions = song.find_content("Amazing Grace");
    /// assert_eq!(positions.len(), 1);
    /// ```
    /// # Note
    /// The search is case-insensitive
    /// The search is done on the content string of the SongPartContent
    /// # Note
    /// The search is done on the content string of the SongPartContent
    pub fn find_content_in_part(&self, content: &str) -> Vec<SongPart> {
        let mut positions: Vec<SongPart> = Vec::new();
        for part_refcall in &self.parts {
            let part = part_refcall.borrow();
            for content_part in &part.contents {
                if content_part.content.to_lowercase().as_str() == content.to_lowercase() {
                    positions.push(part.clone());
                }
            }
        }
        positions
    }

    pub fn find_first_content_in_part(&self, content: &str) -> Option<SongPart> {
        self.find_content_in_part(content).first().cloned()
    }

    pub fn get_part_by_id(&self, id: &str) -> Option<SongPart> {
        for part_refcall in &self.parts {
            let part = part_refcall.borrow();
            if part.id == id {
                return Some(part.clone());
            }
        }
        None
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

type SongPartId = String;

/// A part of a song, which can contain multiple voices (e.g. lyrics, chords, etc.)
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SongPart {
    pub id: SongPartId,
    pub part_type: SongPartType,
    pub number: u32,
    pub contents: Vec<SongPartContent>,
    /// defines whether this part is a repetition of a previous part
    pub is_repetition_of: Option<String>,
}

impl SongPart {
    pub fn new(id: &str, specific_number: Option<u32>) -> SongPart {
        // get part_type from a regex in the format ('part_type'.'number')
        let re: Regex = Regex::new(r"([a-zA-Z]+)\.(\d+)").unwrap();
        let caps: regex::Captures = re.captures(id).unwrap();
        let part_type: SongPartType = SongPartType::from_string(&caps[1]);
        let is_repition:Option<String> = None;
        let number: u32 = specific_number.unwrap_or_else(|| 1);
        SongPart {
            id: id.to_string(),
            part_type: part_type,
            number: number,
            contents: Vec::new(),
            is_repetition_of: is_repition,
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

    pub fn is_repition(&self) -> Option<SongPartId> {
        self.is_repetition_of.clone()
    }

    pub fn set_repition(&mut self, is_repition: Option<SongPartId>) {
        self.is_repetition_of = is_repition;
    }
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
        let part = SongPart::new("verse.1", Some(1));
        assert_eq!(part.part_type, SongPartType::Verse);
        assert_eq!(part.number, 1);
    }

    #[test]
    fn test_adding_song_parts() {
        let mut song: Song = Song::new("Amazing Grace");
        let part: SongPart = SongPart::new("verse.1", None);
        song.add_part(part);
        assert_eq!(song.parts.len(), 1);
        dbg!(song.parts);
    }
}
