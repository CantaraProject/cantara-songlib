use regex::Regex;
use std::collections::HashMap;

/// Object which represents a song in Cantara
pub struct Song {
    pub title: String,
    tags: HashMap<String, String>,
    parts: Vec<SongPart>,
}

impl Song {
    /// Create a new song with the given title
    pub fn new(title: &str) -> Song {
        Song {
            title: title.to_string(),
            tags: HashMap::new(),
            parts: Vec::new(),
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
        self.parts.push(part);
    }
}

#[derive(PartialEq)]
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

    /// Returns whether a song part type is repeatable
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

#[derive(PartialEq)]
pub enum LyricLanguage {
    Default,
    Specific(String),
}

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

impl PartialEq for SongPartContentType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SongPartContentType::Lyrics { language: lang1 }, SongPartContentType::Lyrics { language: lang2 }) => lang1 == lang2,
            (SongPartContentType::Lyrics { language: _ }, _) => false,
            (_, SongPartContentType::Lyrics { language: _ }) => false,
            (SongPartContentType::LeadVoice, SongPartContentType::LeadVoice) => true,
            (SongPartContentType::SupranoVoice, SongPartContentType::SupranoVoice) => true,
            (SongPartContentType::AltoVoice, SongPartContentType::AltoVoice) => true,
            (SongPartContentType::TenorVoice, SongPartContentType::TenorVoice) => true,
            (SongPartContentType::BassVoice, SongPartContentType::BassVoice) => true,
            (SongPartContentType::Instrumental, SongPartContentType::Instrumental) => true,
            (SongPartContentType::Solo, SongPartContentType::Solo) => true,
            (SongPartContentType::Chords, SongPartContentType::Chords) => true,
            _ => false,
        }
    }
}

pub struct SongPartContent {
    pub voice_type: SongPartContentType,
    pub content: String,
}

pub struct SongPart {
    pub id: String,
    pub part_type: SongPartType,
    pub number: u32,
    pub voices: Vec<SongPartContent>,
    is_repitition: bool,
}

impl SongPart {
    pub fn new(id: &str, number: u32) -> SongPart {
        // get part_type from a regex in the format ('part_type'.'number')
        let re = Regex::new(r"([a-zA-Z]+)\.(\d+)").unwrap();
        let caps = re.captures(id).unwrap();
        let part_type: SongPartType = SongPartType::from_string(&caps[1]);
        let is_repition: bool = false;
        SongPart {
            id: id.to_string(),
            part_type: part_type,
            number: number,
            voices: Vec::new(),
            is_repitition: is_repition,
        }
    }

    pub fn add_content(&mut self, content: SongPartContent) {
        self.voices.push(content);
    }

    pub fn get_content(&self, voice_type: SongPartContentType) -> Option<&SongPartContent> {
        self.voices.iter().find(|voice| voice.voice_type == voice_type)
    }
    
    pub fn has_lyrics(&self) -> bool {
        self.voices.iter().any(|voice| match voice.voice_type {
            SongPartContentType::Lyrics { .. } => true,
            _ => false,
        })
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
        let mut song = Song::new("Test Song");
        song.add_tag("key", "value");
        song.add_tag("key2", "value2");
        song.add_tag("key3", "value3");
        assert_eq!(song.get_tag("key").unwrap(), "value");
        assert_eq!(song.get_tag("key2").unwrap(), "value2");
        assert_eq!(song.get_tag("key3").unwrap(), "value3");
    }
}