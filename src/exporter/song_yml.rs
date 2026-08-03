//! `.song.yml` exporter — writes a [`Song`] back out in the YAML song format.
//!
//! This is the counterpart of [`crate::importer::song_yml`] and is meant to
//! round trip: importing the result has to give the same song back. The only
//! deliberate differences to a hand-written file are cosmetic — key order and
//! the choice between the spellings a part type accepts (`verse` vs `stanza`).
//!
//! # Regrouping
//!
//! The importer *splits* one YAML part that carries several numbered lyrics
//! blocks into one [`crate::song::SongPart`] per verse, linked by
//! [`crate::song::SongPart::is_repetition_of`] so that the melody is stored
//! once. Writing them back out as separate parts would be lossless but would
//! duplicate the melody and no longer resemble the input, so the exporter
//! reverses the split: parts that repeat a common first part of the same type
//! are merged back into a single YAML part with one `lyrics` entry per verse.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::song::{
    LyricLanguage, PartOrderName, PartOrderRule, Song, SongPart, SongPartContent,
    SongPartContentType, SongPartId,
};

/// The version written into every exported file.
const FORMAT_VERSION: f64 = 0.1;

// --- Serialisation structs ---
//
// These mirror the importer's deserialisation structs. They are kept separate
// because writing has needs reading does not: a stable key order, and leaving
// empty fields out instead of emitting `null`.

#[derive(Serialize)]
struct YmlFile {
    version: f64,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_language: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    tags: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<YmlScore>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    orders: Vec<YmlOrder>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parts: Vec<YmlPart>,
}

#[derive(Serialize)]
struct YmlScore {
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    partial: Option<u32>,
}

impl YmlScore {
    fn is_empty(&self) -> bool {
        self.key.is_none() && self.time.is_none() && self.partial.is_none()
    }
}

#[derive(Serialize)]
struct YmlPart {
    #[serde(rename = "type")]
    part_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    contents: Vec<YmlContent>,
}

#[derive(Serialize)]
struct YmlContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    content: String,
}

#[derive(Serialize)]
#[serde(untagged)]
enum YmlOrder {
    Standard(String),
    Custom { name: String, parts: Vec<String> },
}

// --- Mapping helpers ---

/// The `type:` keyword for a content entry, as the importer reads it back.
fn content_type_keyword(content_type: &SongPartContentType) -> &'static str {
    match content_type {
        SongPartContentType::LeadVoice => "voice",
        SongPartContentType::SupranoVoice => "soprano",
        SongPartContentType::AltoVoice => "alto",
        SongPartContentType::TenorVoice => "tenor",
        SongPartContentType::BassVoice => "bass",
        SongPartContentType::Instrumental => "instrumental",
        SongPartContentType::Solo => "solo",
        SongPartContentType::Chords => "chords",
        SongPartContentType::Lyrics { .. } => "lyrics",
    }
}

/// Give a block scalar a trailing newline so that YAML writes it as `|` rather
/// than `|-`. The importer trims the content either way; this only keeps the
/// output looking like the hand-written files.
fn block(content: &str) -> String {
    let trimmed = content.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    format!("{}\n", trimmed)
}

/// The `language:` field for a lyrics entry, or `None` when it can be left out.
///
/// An entry without a language is read back as the song's default language, so
/// stating it again would be noise — but only when it really is the default.
fn language_field(language: &LyricLanguage, default_language: Option<&str>) -> Option<String> {
    match language {
        LyricLanguage::Default => None,
        LyricLanguage::Specific(code) => match default_language {
            Some(default) if default.eq_ignore_ascii_case(code) => None,
            _ => Some(code.clone()),
        },
    }
}

/// Turn one content entry into its YAML form.
fn content_entry(
    content: &SongPartContent,
    number: Option<u32>,
    default_language: Option<&str>,
) -> YmlContent {
    let (number, language) = match &content.content_type {
        // Only lyrics are numbered: the number is what ties the verses of one
        // part together and orders them.
        SongPartContentType::Lyrics { language } => {
            (number, language_field(language, default_language))
        }
        _ => (None, None),
    };

    YmlContent {
        content_type: content_type_keyword(&content.content_type).to_string(),
        number,
        language,
        content: block(&content.content),
    }
}

/// Group the parts back into the shape a YAML file stores them in.
///
/// Returns one entry per YAML part: the part that owns the music, followed by
/// the parts that repeat it. See the module documentation for why.
fn regroup_parts(song: &Song) -> Vec<Vec<&SongPart>> {
    let mut groups: Vec<Vec<&SongPart>> = Vec::new();
    let mut group_of: BTreeMap<SongPartId, usize> = BTreeMap::new();

    for part in song.parts() {
        // A part merges into the group of the part it repeats — but only if
        // that part plays the same role. A refrain borrowing a verse's melody
        // is still a refrain and needs its own YAML part.
        let target = part.is_repetition_of.and_then(|reference| {
            let same_type = song
                .part(&reference)
                .is_some_and(|other| other.part_type == part.part_type);
            same_type.then(|| group_of.get(&reference).copied()).flatten()
        });

        match target {
            Some(index) => {
                groups[index].push(part);
                group_of.insert(part.id(), index);
            }
            None => {
                groups.push(vec![part]);
                group_of.insert(part.id(), groups.len() - 1);
            }
        }
    }

    groups
}

/// Build the YAML part for one group.
fn part_entry(group: &[&SongPart], default_language: Option<&str>) -> YmlPart {
    let first = group[0];
    let mut contents: Vec<YmlContent> = Vec::new();

    // Music first, then chords, then the text — the order a reader expects and
    // the one the importer is indifferent to.
    for content in &first.contents {
        if content.content_type.is_voice() {
            contents.push(content_entry(content, None, default_language));
        }
    }
    for content in &first.contents {
        if matches!(content.content_type, SongPartContentType::Chords) {
            contents.push(content_entry(content, None, default_language));
        }
    }
    for part in group {
        for content in &part.contents {
            if content.content_type.is_lyrics() {
                contents.push(content_entry(content, Some(part.number), default_language));
            }
        }
    }

    YmlPart {
        part_type: first.part_type.as_str().to_string(),
        name: first.label.clone(),
        contents,
    }
}

/// Map the singing orders back onto the format's `orders:` entries.
fn order_entries(song: &Song) -> Vec<YmlOrder> {
    song.part_orders
        .iter()
        .map(|order| match order.rule() {
            PartOrderRule::VerseRefrainBridgeRefrain => {
                YmlOrder::Standard("verse-refrain-verse".to_string())
            }
            PartOrderRule::RefrainVerseBridgeRefrain => {
                YmlOrder::Standard("refrain-verse-refrain".to_string())
            }
            PartOrderRule::Custom(ids) => YmlOrder::Custom {
                name: match &order.name {
                    PartOrderName::Default => "default".to_string(),
                    PartOrderName::Custom(name) => name.clone(),
                },
                parts: ids.iter().map(|id| id.to_string()).collect(),
            },
        })
        .collect()
}

/// Export a song as the contents of a `.song.yml` file.
///
/// ```
/// use cantara_songlib::exporter::song_yml::song_yml_from_song;
/// use cantara_songlib::importer::song_yml::import_from_yml_string;
///
/// let original = std::fs::read_to_string("tests/data/Amazing Grace.song.yml").unwrap();
/// let song = import_from_yml_string(&original).unwrap();
///
/// let exported = song_yml_from_song(&song).unwrap();
/// let reimported = import_from_yml_string(&exported).unwrap();
///
/// assert_eq!(reimported.title, song.title);
/// assert_eq!(reimported.parts().len(), song.parts().len());
/// ```
pub fn song_yml_from_song(song: &Song) -> Result<String, String> {
    let default_language = song.default_language.as_deref();

    let score = YmlScore {
        key: song.score.key.clone(),
        time: song.score.time.clone(),
        partial: song.score.partial,
    };

    let file = YmlFile {
        version: FORMAT_VERSION,
        title: song.title.clone(),
        default_language: song.default_language.clone(),
        tags: song.tags().clone(),
        score: (!score.is_empty()).then_some(score),
        orders: order_entries(song),
        parts: regroup_parts(song)
            .iter()
            .map(|group| part_entry(group, default_language))
            .collect(),
    };

    serde_yaml::to_string(&file).map_err(|error| format!("could not write the song: {}", error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::song_yml::import_from_yml_string;
    use crate::song::{SongPartType, SongPartContent};

    fn amazing_grace() -> Song {
        let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml").unwrap();
        import_from_yml_string(&content).unwrap()
    }

    fn sei_nicht_stolz() -> Song {
        let content =
            std::fs::read_to_string("tests/data/Sei nicht stolz auf das, was du bist.song.yml")
                .unwrap();
        import_from_yml_string(&content).unwrap()
    }

    /// The song that comes back out of an export has to be the song that went
    /// in — this is the property the whole module exists for.
    fn assert_round_trips(song: &Song) {
        let exported = song_yml_from_song(song).unwrap();
        let back = import_from_yml_string(&exported)
            .unwrap_or_else(|error| panic!("the export does not parse: {}\n{}", error, exported));

        assert_eq!(back.title, song.title, "{}", exported);
        assert_eq!(back.default_language, song.default_language, "{}", exported);
        assert_eq!(back.tags(), song.tags(), "{}", exported);
        assert_eq!(back.score.key, song.score.key, "{}", exported);
        assert_eq!(back.score.time, song.score.time, "{}", exported);
        assert_eq!(back.score.partial, song.score.partial, "{}", exported);

        let ids = |s: &Song| -> Vec<String> {
            s.parts().iter().map(|p| p.id().to_string()).collect()
        };
        assert_eq!(ids(&back), ids(song), "{}", exported);

        for (before, after) in song.parts().iter().zip(back.parts()) {
            assert_eq!(after.label, before.label, "{}", exported);
            assert_eq!(
                after.is_repetition_of, before.is_repetition_of,
                "the repetition link of {} changed\n{}",
                before.id(),
                exported
            );

            let contents = |part: &SongPart| -> Vec<(String, String)> {
                let mut listed: Vec<(String, String)> = part
                    .contents
                    .iter()
                    .map(|c| {
                        (
                            format!("{:?}", c.content_type),
                            c.content.trim().to_string(),
                        )
                    })
                    .collect();
                // The exporter fixes a reading order; the model does not.
                listed.sort();
                listed
            };
            assert_eq!(
                contents(after),
                contents(before),
                "the contents of {} changed\n{}",
                before.id(),
                exported
            );
        }
    }

    #[test]
    fn test_amazing_grace_round_trips() {
        assert_round_trips(&amazing_grace());
    }

    /// This one has two part types, a refrain with its own melody, three verses
    /// sharing one, a `partial:` and a `name:` on every part.
    #[test]
    fn test_sei_nicht_stolz_round_trips() {
        assert_round_trips(&sei_nicht_stolz());
    }

    /// A song read from the classic format has no yml history at all.
    #[test]
    fn test_a_classic_song_round_trips() {
        let content =
            std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();
        let song = crate::importer::classic_song::import_song(&content).unwrap();

        assert_round_trips(&song);
    }

    /// The verses of one part are written back as numbered lyrics of a single
    /// part rather than as separate parts, so the melody stays stored once.
    #[test]
    fn test_verses_sharing_a_melody_become_one_part() {
        let exported = song_yml_from_song(&amazing_grace()).unwrap();

        assert_eq!(
            exported.matches("- type: verse").count(),
            1,
            "the three verses were not merged back into one part:\n{}",
            exported
        );
        assert_eq!(
            exported.matches("type: lyrics").count(),
            3,
            "one lyrics entry per verse was expected:\n{}",
            exported
        );
        // The melody belongs to the group, not to each verse.
        assert_eq!(
            exported.matches("type: voice").count(),
            1,
            "the melody was duplicated:\n{}",
            exported
        );
    }

    /// A refrain that merely borrows the verses' melody is still a part of its
    /// own — merging it in would lose its role.
    #[test]
    fn test_a_refrain_repeating_a_verse_stays_its_own_part() {
        let mut song = Song::new("Shared Melody");
        let verse = song.add_part_of_type(SongPartType::Verse, Some(1));
        song.part_mut(&verse)
            .unwrap()
            .add_content(SongPartContent::new(
                SongPartContentType::LeadVoice,
                "c4 d e f",
            ));
        let refrain = song.add_part_of_type(SongPartType::Refrain, Some(1));
        song.part_mut(&refrain).unwrap().is_repetition_of = Some(verse);
        song.part_mut(&refrain)
            .unwrap()
            .add_content(SongPartContent::lyrics(LyricLanguage::Default, "refrain"));

        let exported = song_yml_from_song(&song).unwrap();

        assert!(exported.contains("- type: verse"), "{}", exported);
        assert!(exported.contains("- type: refrain"), "{}", exported);
    }

    /// Lyrics in the song's default language need no `language:` — the importer
    /// fills it in. A second language does need one.
    #[test]
    fn test_the_default_language_is_left_out() {
        let mut song = Song::new("Two Languages");
        song.default_language = Some("de".to_string());
        let id = song.add_part_of_type(SongPartType::Verse, Some(1));
        let part = song.part_mut(&id).unwrap();
        part.add_content(SongPartContent::lyrics(
            LyricLanguage::specific("de"),
            "deutsch",
        ));
        part.add_content(SongPartContent::lyrics(
            LyricLanguage::specific("en"),
            "english",
        ));

        let exported = song_yml_from_song(&song).unwrap();

        assert_eq!(
            exported.matches("language:").count(),
            2,
            "expected only `default_language` and the English entry:\n{}",
            exported
        );
        assert!(exported.contains("language: en"), "{}", exported);
    }

    /// Multi-line content is written as a block scalar, the way the format is
    /// written by hand — a quoted string with `\n` escapes would parse but be
    /// unreadable and unmergeable.
    #[test]
    fn test_multiline_content_is_a_block_scalar() {
        let exported = song_yml_from_song(&sei_nicht_stolz()).unwrap();

        assert!(exported.contains("content: |"), "{}", exported);
        assert!(
            !exported.contains("\\n"),
            "content was escaped instead of blocked:\n{}",
            exported
        );
    }

    /// An empty song still produces a valid file rather than a broken one.
    #[test]
    fn test_an_empty_song_is_still_valid_yaml() {
        let song = Song::new("Nothing Here");
        let exported = song_yml_from_song(&song).unwrap();
        let back = import_from_yml_string(&exported).unwrap();

        assert_eq!(back.title, "Nothing Here");
        assert!(back.parts().is_empty());
        // Absent blocks are left out rather than written as `null`.
        assert!(!exported.contains("null"), "{}", exported);
    }

    /// The `name:` of a part is its heading in the source file. It used to be
    /// read and thrown away, which made every round trip lose it.
    #[test]
    fn test_the_part_heading_survives_the_round_trip() {
        let song = sei_nicht_stolz();
        assert_eq!(
            song.part(&"verse.1".parse().unwrap()).unwrap().label,
            Some("Strophe".to_string()),
            "the importer dropped the heading"
        );

        let exported = song_yml_from_song(&song).unwrap();
        assert!(exported.contains("name: Strophe"), "{}", exported);
        assert!(exported.contains("name: Refrain"), "{}", exported);

        let back = import_from_yml_string(&exported).unwrap();
        assert_eq!(
            back.part(&"verse.1".parse().unwrap()).unwrap().label,
            Some("Strophe".to_string())
        );
    }

    /// Exporting twice has to give the same bytes — otherwise the format is not
    /// safe to keep in version control.
    #[test]
    fn test_the_export_is_stable() {
        let song = sei_nicht_stolz();
        let once = song_yml_from_song(&song).unwrap();
        let twice = song_yml_from_song(&import_from_yml_string(&once).unwrap()).unwrap();

        assert_eq!(once, twice, "a second export changed the file");
    }

    /// A custom order names parts by id and has to survive the trip.
    #[test]
    fn test_a_custom_order_round_trips() {
        let yml = r#"
version: 0.1
title: Custom Order
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
      content: chorus
orders:
  - name: short
    parts: [verse.1, refrain.1, verse.1]
"#;
        let song = import_from_yml_string(yml).unwrap();
        let exported = song_yml_from_song(&song).unwrap();
        let back = import_from_yml_string(&exported).unwrap();

        assert_eq!(back.part_orders, song.part_orders, "{}", exported);
        assert!(exported.contains("name: short"), "{}", exported);
    }
}
