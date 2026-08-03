//! This module handles the loading of `.song.yml` (YAML-based song) files.
//! It deserializes YAML into intermediate structs, then converts them into the Song data model.

use std::collections::HashMap;
use std::error::Error;

use serde::Deserialize;

use crate::song::{
    LyricLanguage, PartOrder, PartOrderName, PartOrderRule, Song, SongPartContent,
    SongPartContentType, SongPartId, SongPartType,
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

    for (key, value) in &yml.tags {
        song.set_tag(key, value);
    }

    // The `score:` block holds notation settings, not free-form metadata, so it
    // gets its own typed home on the song.
    if let Some(score) = &yml.score {
        song.score.key = score.key.clone();
        song.score.time = score.time.clone();
        song.score.partial = score.partial;
    }

    for yml_part in &yml.parts {
        // `from_str` for SongPartType is infallible.
        let part_type: SongPartType = yml_part.part_type.parse().unwrap();

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
            // A part without lyrics (e.g. an instrumental intro) becomes a
            // single part carrying just the music.
            let id = song.add_part_of_type(part_type, Some(1));
            let part = song.part_mut(&id).unwrap();
            // The heading the file gave this part, e.g. "Strophe".
            part.label = yml_part.name.clone();
            for voice in &voice_contents {
                part.add_content(map_voice_content(voice));
            }
            continue;
        }

        // Lyrics entries are grouped by their `number`: entries sharing a
        // number are the same verse in different languages and belong on one
        // part, while different numbers are different verses.
        //
        // The first group carries the music; the others reference it, so the
        // melody is stored exactly once.
        let mut groups: Vec<(Option<u32>, Vec<&SongYmlContent>)> = Vec::new();
        for lyrics in &lyrics_contents {
            match lyrics.number.and_then(|number| {
                groups
                    .iter_mut()
                    .find(|(key, _)| *key == Some(number))
            }) {
                Some((_, group)) => group.push(lyrics),
                // An entry without a number is always a verse of its own.
                None => groups.push((lyrics.number, vec![lyrics])),
            }
        }

        let mut first_id: Option<SongPartId> = None;

        for (number, group) in &groups {
            let id = song.add_part_of_type(part_type, *number);
            let part = song.part_mut(&id).unwrap();
            // Every verse of the block keeps the block's heading.
            part.label = yml_part.name.clone();

            if first_id.is_none() {
                for voice in &voice_contents {
                    part.add_content(map_voice_content(voice));
                }
                for chords in &chord_contents {
                    part.add_content(SongPartContent::new(
                        SongPartContentType::Chords,
                        chords.content.trim(),
                    ));
                }
            } else {
                part.is_repetition_of = first_id;
            }

            for lyrics in group {
                // An entry without an explicit language is in the song's
                // default language; without that it stays unlabelled.
                let language = match &lyrics.language {
                    Some(code) => LyricLanguage::specific(code),
                    None => match &yml.default_language {
                        Some(default) => LyricLanguage::specific(default),
                        None => LyricLanguage::Default,
                    },
                };
                part.add_content(SongPartContent::lyrics(language, lyrics.content.trim()));
            }

            if first_id.is_none() {
                first_id = Some(id);
            }
        }
    }

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
                    // Unknown pattern — fall back to guessing further down.
                    _ => continue,
                };
                song.part_orders
                    .push(PartOrder::new(PartOrderName::Default, rule));
            }
            SongYmlOrder::Custom { name, parts } => {
                // Part ids are parsed, not compared as strings, so `verse.1`,
                // `Verse.1` and `stanza.1` all refer to the same part.
                let mut ids: Vec<SongPartId> = Vec::new();
                for text in parts {
                    let id: SongPartId = text.parse()?;
                    if song.part(&id).is_none() {
                        return Err(format!(
                            "the order '{}' refers to '{}', which the song does not contain",
                            name, id
                        )
                        .into());
                    }
                    ids.push(id);
                }
                song.part_orders.push(PartOrder::new(
                    PartOrderName::Custom(name.clone()),
                    PartOrderRule::Custom(ids),
                ));
            }
        }
    }

    if song.part_orders.is_empty() {
        song.add_guessed_part_order();
    }

    Ok(song)
}

/// Map a YAML voice content entry to a SongPartContent
fn map_voice_content(vc: &SongYmlContent) -> SongPartContent {
    let content_type = match vc.content_type.as_str() {
        "voice" | "lead" => SongPartContentType::LeadVoice,
        "soprano" => SongPartContentType::SupranoVoice,
        "alto" => SongPartContentType::AltoVoice,
        "tenor" => SongPartContentType::TenorVoice,
        "bass" => SongPartContentType::BassVoice,
        "instrumental" => SongPartContentType::Instrumental,
        "solo" => SongPartContentType::Solo,
        _ => SongPartContentType::LeadVoice,
    };

    SongPartContent::new(content_type, vc.content.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_amazing_grace_yml() {
        let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml").unwrap();
        let song = import_from_yml_string(&content).unwrap();

        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.default_language, Some("en".to_string()));
        assert_eq!(song.tag("author").unwrap(), "John Newton");
        assert_eq!(song.tag("bible").unwrap(), "John 3:16");
        assert_eq!(song.score.key.as_ref().unwrap(), "f major");
        assert_eq!(song.score.time.as_ref().unwrap(), "3/4");
        assert_eq!(song.score.partial.unwrap(), 4);

        // 3 verses
        assert_eq!(song.part_count_of_type(SongPartType::Verse), 3);

        // The first verse carries the melody and the lyrics.
        let verse1 = song.part(&"verse.1".parse().unwrap()).unwrap();
        assert!(verse1.contents.len() >= 2);
        assert!(verse1.is_repetition_of.is_none());
        assert!(verse1.own_voice().is_some());

        // Later verses only reference it, so the melody is stored once.
        let verse2 = song.part(&"verse.2".parse().unwrap()).unwrap();
        assert_eq!(verse2.is_repetition_of, Some(verse1.id()));
        assert!(verse2.own_voice().is_none());
        assert!(song.voice_for_part(verse2).is_some());
    }

    /// A named order in the YAML file used to resolve to nothing because part
    /// ids were compared as case-sensitive strings.
    #[test]
    fn test_named_custom_order_resolves() {
        let yml = r#"
version: 0.1
title: Custom Order
orders:
  - name: short
    parts: [verse.1, refrain.1]
parts:
  - type: verse
    contents:
    - type: lyrics
      number: 1
      content: one
  - type: refrain
    contents:
    - type: lyrics
      number: 1
      content: two
"#;
        let song = import_from_yml_string(yml).unwrap();
        let order = song.part_orders.first().unwrap();
        assert_eq!(order.name, PartOrderName::Custom("short".to_string()));

        let ids: Vec<String> = order
            .to_parts(&song)
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(ids, ["verse.1", "refrain.1"]);
    }

    #[test]
    fn test_named_order_with_unknown_part_is_an_error() {
        let yml = r#"
version: 0.1
title: Bad Order
orders:
  - name: short
    parts: [verse.1, refrain.9]
parts:
  - type: verse
    contents:
    - type: lyrics
      number: 1
      content: one
"#;
        let error = import_from_yml_string(yml).unwrap_err().to_string();
        assert!(error.contains("refrain.9"), "unexpected error: {}", error);
    }

    #[test]
    fn test_lyrics_language_falls_back_to_the_song_default() {
        let yml = r#"
version: 0.1
title: Languages
default_language: de
parts:
  - type: verse
    contents:
    - type: lyrics
      number: 1
      content: Hallo
    - type: lyrics
      number: 2
      language: EN
      content: Hello
"#;
        let song = import_from_yml_string(yml).unwrap();
        // Language codes are normalised, so "EN" and "en" are the same.
        assert_eq!(song.available_languages(), ["de", "en"]);

        let verse2 = song.part(&"verse.2".parse().unwrap()).unwrap();
        assert_eq!(verse2.lyrics_for(Some("en"), Some("de")).unwrap().content, "Hello");
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
        assert_eq!(song.part_count_of_type(SongPartType::Verse), 1);
    }
}
