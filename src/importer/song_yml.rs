//! This module handles the loading of `.song.yml` (YAML-based song) files.
//! It deserializes YAML into intermediate structs, then converts them into the Song data model.

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use serde::Deserialize;

use crate::song::{
    LyricLanguage, PartOrder, PartOrderName, PartOrderRule, Song, SongPart, SongPartContent,
    SongPartContentType, SongPartType,
};

// --- Intermediate YAML deserialization structs ---

#[derive(Deserialize, Debug)]
pub struct SongYmlFile {
    pub version: f64,
    pub title: String,
    pub default_language: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    pub score: Option<SongYmlScore>,
    #[serde(default)]
    pub orders: Vec<SongYmlOrder>,
    #[serde(default)]
    pub parts: Vec<SongYmlPart>,
}

#[derive(Deserialize, Debug)]
pub struct SongYmlScore {
    pub key: Option<String>,
    pub time: Option<String>,
    pub partial: Option<u32>,
}

#[derive(Deserialize, Debug)]
pub struct SongYmlPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub name: Option<String>,
    #[serde(default)]
    pub contents: Vec<SongYmlContent>,
}

#[derive(Deserialize, Debug)]
pub struct SongYmlContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub number: Option<u32>,
    pub language: Option<String>,
    pub content: String,
}

/// An order can be either a standard pattern name (string) or a named custom order with explicit parts.
#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum SongYmlOrder {
    Standard(String),
    Custom {
        name: String,
        parts: Vec<String>,
    },
}

// --- Conversion from YAML structs to Song ---

/// Parse a YAML string into a Song.
pub fn import_from_yml_string(yml_content: &str) -> Result<Song, Box<dyn Error>> {
    let yml_file: SongYmlFile = serde_yaml::from_str(yml_content)?;
    song_from_yml(yml_file)
}

/// Convert a deserialized SongYmlFile into a Song.
fn song_from_yml(yml: SongYmlFile) -> Result<Song, Box<dyn Error>> {
    let mut song = Song::new(&yml.title);
    song.default_language = yml.default_language.clone();

    // Store tags
    for (key, value) in &yml.tags {
        song.add_tag(key, value);
    }

    // Store score metadata as tags (so they're accessible via get_tag)
    if let Some(ref score) = yml.score {
        if let Some(ref key) = score.key {
            song.add_tag("key", key);
        }
        if let Some(ref time) = score.time {
            song.add_tag("time", time);
        }
        if let Some(partial) = score.partial {
            song.add_tag("partial", &partial.to_string());
        }
    }

    // Process parts
    for yml_part in &yml.parts {
        let part_type = SongPartType::from_string(&yml_part.part_type);

        // Separate voice contents from lyrics contents
        let voice_contents: Vec<&SongYmlContent> = yml_part
            .contents
            .iter()
            .filter(|c| c.content_type != "lyrics" && c.content_type != "chords")
            .collect();

        let lyrics_contents: Vec<&SongYmlContent> = yml_part
            .contents
            .iter()
            .filter(|c| c.content_type == "lyrics")
            .collect();

        let chord_contents: Vec<&SongYmlContent> = yml_part
            .contents
            .iter()
            .filter(|c| c.content_type == "chords")
            .collect();

        if lyrics_contents.is_empty() {
            // Part with no lyrics (e.g. an instrumental intro) — create one part with voice only
            let number = 1u32;
            let part_ref = song.add_part_of_type(part_type, Some(number));
            let mut part = part_ref.borrow_mut();
            for vc in &voice_contents {
                part.add_content(map_voice_content(vc));
            }
            continue;
        }

        // For each numbered lyrics entry, create a SongPart.
        // The first one gets the voice content; subsequent ones reference it via is_repetition_of.
        let mut first_part_ref: Option<Rc<RefCell<SongPart>>> = None;

        for lyrics in &lyrics_contents {
            let number = lyrics.number.unwrap_or(1);
            let part_ref = song.add_part_of_type(part_type, Some(number));

            {
                let mut part = part_ref.borrow_mut();

                // Add voice content to the first part only
                if first_part_ref.is_none() {
                    for vc in &voice_contents {
                        part.add_content(map_voice_content(vc));
                    }
                    for cc in &chord_contents {
                        part.add_content(SongPartContent {
                            voice_type: SongPartContentType::Chords,
                            content: cc.content.trim().to_string(),
                        });
                    }
                }

                // Determine the lyrics language
                let language = match &lyrics.language {
                    Some(lang) => LyricLanguage::Specific(lang.clone()),
                    None => match &yml.default_language {
                        Some(default_lang) => LyricLanguage::Specific(default_lang.clone()),
                        None => LyricLanguage::Default,
                    },
                };

                part.add_content(SongPartContent {
                    voice_type: SongPartContentType::Lyrics { language },
                    content: lyrics.content.trim().to_string(),
                });
            }

            // Set is_repetition_of for subsequent parts
            if let Some(ref first_ref) = first_part_ref {
                let cloned: Rc<RefCell<SongPart>> = first_ref.clone();
                part_ref.borrow_mut().set_repition(Some(cloned));
            } else {
                first_part_ref = Some(part_ref.clone());
            }
        }
    }

    // Process orders
    for yml_order in &yml.orders {
        match yml_order {
            SongYmlOrder::Standard(pattern) => {
                let rule = match pattern.as_str() {
                    "stanza-refrain-stanza" | "verse-refrain-verse" | "verse-chorus-verse" => {
                        PartOrderRule::VerseRefrainBridgeRefrain
                    }
                    "refrain-stanza-refrain" | "refrain-verse-refrain" | "chorus-verse-chorus" => {
                        PartOrderRule::RefrainVerseBridgeRefrain
                    }
                    _ => {
                        // Unknown pattern, fall back to guessing
                        continue;
                    }
                };
                song.part_orders.push(PartOrder::new(
                    PartOrderName::Default,
                    rule,
                ));
            }
            SongYmlOrder::Custom { name, parts } => {
                let mut part_refs: Vec<Rc<RefCell<SongPart>>> = Vec::new();
                for part_id_str in parts {
                    if let Some(part_ref) = song.get_part_by_id(part_id_str) {
                        part_refs.push(part_ref);
                    }
                }
                song.part_orders.push(PartOrder::new(
                    PartOrderName::Custom(name.clone()),
                    PartOrderRule::Custom(part_refs),
                ));
            }
        }
    }

    // If no orders were specified, add a guessed one
    if song.part_orders.is_empty() {
        song.add_guessed_part_order();
    }

    Ok(song)
}

/// Map a YAML voice content entry to a SongPartContent
fn map_voice_content(vc: &SongYmlContent) -> SongPartContent {
    let voice_type = match vc.content_type.as_str() {
        "voice" | "lead" => SongPartContentType::LeadVoice,
        "soprano" => SongPartContentType::SupranoVoice,
        "alto" => SongPartContentType::AltoVoice,
        "tenor" => SongPartContentType::TenorVoice,
        "bass" => SongPartContentType::BassVoice,
        "instrumental" => SongPartContentType::Instrumental,
        "solo" => SongPartContentType::Solo,
        _ => SongPartContentType::LeadVoice,
    };

    SongPartContent {
        voice_type,
        content: vc.content.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_amazing_grace_yml() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = import_from_yml_string(&content).unwrap();

        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.default_language, Some("en".to_string()));
        assert_eq!(song.get_tag("author").unwrap(), "John Newton");
        assert_eq!(song.get_tag("bible").unwrap(), "John 3:16");
        assert_eq!(song.get_tag("key").unwrap(), "f major");
        assert_eq!(song.get_tag("time").unwrap(), "3/4");
        assert_eq!(song.get_tag("partial").unwrap(), "4");

        // 3 verses
        assert_eq!(song.get_part_count(SongPartType::Verse), 3);

        // First verse should have voice + lyrics
        let verse1 = song.get_part_by_id("Verse.1").unwrap();
        let v1 = verse1.borrow();
        assert!(v1.contents.len() >= 2); // voice + lyrics
        assert!(v1.is_repetition_of.is_none());

        // Second verse should reference the first via is_repetition_of
        let verse2 = song.get_part_by_id("Verse.2").unwrap();
        let v2 = verse2.borrow();
        assert!(v2.is_repetition_of.is_some());

        // Voice content should be findable via get_voice_for_part
        let voice = song.get_voice_for_part(&v2);
        assert!(voice.is_some());
    }

    #[test]
    fn test_parse_minimal_yml() {
        let yml = r#"
version: 0.1
title: Minimal Song
parts:
- type: verse
  contents:
  - type: lyrics
    number: 1
    content: |
      Hello world
"#;
        let song = import_from_yml_string(yml).unwrap();
        assert_eq!(song.title, "Minimal Song");
        assert_eq!(song.get_part_count(SongPartType::Verse), 1);
    }
}
