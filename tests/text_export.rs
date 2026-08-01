//! End-to-end tests for the text export and for exporting several songs at once.
//!
//! The unit tests inside `src/exporter/text.rs` cover the rendering itself;
//! these go through the public entry points and check the pieces that only
//! appear when songs are combined.

use cantara_songlib::exporter::slides::{chapters_from_songs, slides_from_songs};
use cantara_songlib::exporter::text::{text_from_song, text_from_songs, TextFormat, TextSettings};
use cantara_songlib::importer::import_song_from_file;
use cantara_songlib::slides::{LinkedEntity, SlideSettings};
use cantara_songlib::song::Song;

const GRACE: &str = "tests/data/Amazing Grace.song.yml";
const CCLI: &str = "tests/data/Weiß ich den Weg auch nicht.ccli";
const CLASSIC: &str = "tests/data/O What A Savior That He Died For Me.song";

fn songs() -> Vec<Song> {
    [GRACE, CCLI]
        .iter()
        .map(|path| import_song_from_file(path).expect("import"))
        .collect()
}

// ---------------------------------------------------------------------------
// Simple text
// ---------------------------------------------------------------------------

/// The plain format is the lyrics of the first language in singing order and
/// nothing else — no markup left over from the source format.
#[test]
fn test_plain_text_carries_no_markup() {
    for path in [GRACE, CCLI, CLASSIC] {
        let song = import_song_from_file(path).unwrap();
        let text = text_from_song(&song, &TextSettings::default()).unwrap();

        assert!(text.starts_with(&song.title), "{}: no title:\n{}", path, text);

        for artefact in ["--", "\\set", "\\unset", "##t", "_ ", "w:", "X:1"] {
            assert!(
                !text.contains(artefact),
                "{}: '{}' leaked into the text:\n{}",
                path,
                artefact,
                text
            );
        }
    }
}

/// Every part of the singing order shows up, refrains included and repeated.
#[test]
fn test_singing_order_including_repeats() {
    let song = import_song_from_file(CLASSIC).unwrap();
    let text = text_from_song(&song, &TextSettings::default()).unwrap();

    // Four verses, each followed by the chorus.
    assert_eq!(text.matches("Verily, verily, I say unto you;").count(), 4);
    assert_eq!(text.matches("Oh, what a Saviour that He died for me!").count(), 1);
}

// ---------------------------------------------------------------------------
// Markup
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_and_telegram_differ_only_in_the_heading() {
    let song = import_song_from_file(GRACE).unwrap();

    let markdown =
        text_from_song(&song, &TextSettings::with_format(TextFormat::Markdown)).unwrap();
    let telegram =
        text_from_song(&song, &TextSettings::with_format(TextFormat::Telegram)).unwrap();

    assert!(markdown.starts_with("# Amazing Grace"));
    assert!(telegram.starts_with("**Amazing Grace**"));

    // The lyrics are the same in both.
    let body = |text: &str| {
        text.lines()
            .skip_while(|line| !line.starts_with("Amazing grace"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(body(&markdown), body(&telegram));
}

/// A template gets the parts in singing order with their labels.
#[test]
fn test_custom_template_sees_the_parts() {
    let song = import_song_from_file("tests/data/Sei nicht stolz auf das, was du bist.song.yml")
        .unwrap();

    let settings = TextSettings::with_format(TextFormat::Custom(
        "{{#each parts}}{{position}}:{{id}}{{#unless last}},{{/unless}}{{/each}}".to_string(),
    ));

    let text = text_from_song(&song, &settings).unwrap();
    assert_eq!(
        text,
        "1:verse.1,2:refrain.1,3:verse.2,4:refrain.1,5:verse.3,6:refrain.1"
    );
}

/// The `first` and `last` flags let a template put separators between parts
/// without a trailing one.
#[test]
fn test_first_and_last_flags() {
    let song = import_song_from_file(GRACE).unwrap();
    let settings = TextSettings::with_format(TextFormat::Custom(
        "{{#each parts}}{{#if first}}[{{/if}}{{number}}{{#if last}}]{{else}}-{{/if}}{{/each}}"
            .to_string(),
    ));

    assert_eq!(text_from_song(&song, &settings).unwrap(), "[1-2-3]");
}

// ---------------------------------------------------------------------------
// Several songs
// ---------------------------------------------------------------------------

#[test]
fn test_several_songs_into_one_document() {
    let text =
        text_from_songs(&songs(), &TextSettings::with_format(TextFormat::Markdown)).unwrap();

    let headings: Vec<&str> = text.lines().filter(|line| line.starts_with("# ")).collect();
    assert_eq!(
        headings,
        ["# Amazing Grace", "# Weiß ich den Weg auch nicht (Pax Dei)"]
    );
}

#[test]
fn test_song_separator() {
    let settings = TextSettings {
        song_separator: Some("· · ·".to_string()),
        ..TextSettings::default()
    };

    let text = text_from_songs(&songs(), &settings).unwrap();
    // One separator between two songs, none at either end.
    assert_eq!(text.matches("· · ·").count(), 1);
    assert!(!text.starts_with("· · ·"));
    assert!(!text.trim_end().ends_with("· · ·"));
}

// ---------------------------------------------------------------------------
// Chapters
// ---------------------------------------------------------------------------

/// Several songs become one chapter each, and every chapter keeps a link back
/// to the song it was built from.
#[test]
fn test_chapters_keep_their_songs() {
    let songs = songs();
    let chapters = chapters_from_songs(&songs, &SlideSettings::default());

    assert_eq!(chapters.len(), 2);

    for (chapter, song) in chapters.iter().zip(songs.iter()) {
        assert!(!chapter.slides.is_empty());
        match &chapter.linked_entity {
            LinkedEntity::Song(linked) => assert_eq!(linked, song),
            other => panic!("expected a song link, got {:?}", other),
        }
    }
}

/// Flattening the chapters gives the same slides in the same order.
#[test]
fn test_flat_slides_match_the_chapters() {
    let songs = songs();
    let settings = SlideSettings::default();

    let flat = slides_from_songs(&songs, &settings);
    let from_chapters: Vec<_> = chapters_from_songs(&songs, &settings)
        .into_iter()
        .flat_map(|chapter| chapter.slides)
        .collect();

    assert_eq!(flat, from_chapters);
}

#[test]
fn test_chapters_round_trip_through_json() {
    let chapters = chapters_from_songs(&songs(), &SlideSettings::default());

    let json = serde_json::to_string(&chapters).expect("serialise");
    let restored: Vec<cantara_songlib::slides::PresentationChapter> =
        serde_json::from_str(&json).expect("deserialise");

    assert_eq!(restored, chapters);
}

#[test]
fn test_no_songs_gives_no_chapters() {
    assert!(chapters_from_songs(&[], &SlideSettings::default()).is_empty());
    assert!(slides_from_songs(&[], &SlideSettings::default()).is_empty());
    assert_eq!(text_from_songs(&[], &TextSettings::default()).unwrap(), "");
}
