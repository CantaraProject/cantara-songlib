//! End-to-end tests for CCLI SongSelect imports.
//!
//! The unit tests inside `src/importer/ccli.rs` cover the parser itself; these
//! go through the public entry points instead, so they also check that `.ccli`
//! is wired into the format dispatcher and reaches every exporter.

use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::exporter::lilypond::{lilypond_from_song, LilypondSettings};
use cantara_songlib::importer::import_song_from_file;
use cantara_songlib::slides::{ShowMetaInformation, SlideSettings};
use cantara_songlib::slides_from_file;
use cantara_songlib::song::SongPartType;

const GERMAN: &str = "tests/data/Weiß ich den Weg auch nicht.ccli";
const GENERIC: &str = "tests/data/ExampleCCLISong1.ccli";

#[test]
fn test_dispatcher_recognises_the_ccli_extension() {
    let song = import_song_from_file(GERMAN).expect("the .ccli extension should be dispatched");

    assert_eq!(song.title, "Weiß ich den Weg auch nicht (Pax Dei)");
    assert_eq!(song.part_count_of_type(SongPartType::Verse), 3);
}

#[test]
fn test_slides_from_a_ccli_file() {
    let settings = SlideSettings {
        title_slide: true,
        empty_last_slide: false,
        ..SlideSettings::default()
    };

    let slides = slides_from_file(GERMAN, &settings).expect("slides");

    // One title slide plus one slide per verse.
    assert_eq!(slides.len(), 4);
}

/// The metadata read out of the trailer has to reach the slide templating.
#[test]
fn test_trailer_metadata_reaches_the_slides() {
    let settings = SlideSettings {
        title_slide: true,
        meta_syntax: "{{title}} — {{author}} (CCLI {{ccli_song_number}})".to_string(),
        show_meta_information: ShowMetaInformation::FirstSlide,
        ..SlideSettings::default()
    };

    let slides = slides_from_file(GERMAN, &settings).expect("slides");
    let rendered = serde_json::to_string(&slides).expect("serialisable");

    assert!(
        rendered.contains("Hedwig Von Redern, John Bacchus Dykes"),
        "the authors from the trailer are missing:\n{}",
        rendered
    );
    assert!(
        rendered.contains("CCLI 5973691"),
        "the CCLI song number is missing:\n{}",
        rendered
    );
}

/// A CCLI export carries lyrics but no music, so the sheet-music exporters have
/// to refuse it with a clear message instead of panicking or emitting an empty
/// score.
#[test]
fn test_sheet_music_export_reports_the_missing_melody() {
    let song = import_song_from_file(GENERIC).expect("import");

    let lilypond = lilypond_from_song(&song, &LilypondSettings::default());
    assert!(lilypond.is_err());
    assert!(lilypond.unwrap_err().contains("no voice content"));

    let abc = abc_from_song(&song, &AbcSettings::default());
    assert!(abc.is_err());
    assert!(abc.unwrap_err().contains("no voice content"));
}

/// The singing order is derived from the section headings, so a pre-chorus ends
/// up between the verse and the chorus and both repeat for every verse.
#[test]
fn test_singing_order_from_headings() {
    let song = import_song_from_file(GENERIC).expect("import");

    let sung: Vec<String> = song
        .ordered_parts()
        .iter()
        .map(|part| part.display_label())
        .collect();

    assert_eq!(
        sung,
        [
            "Vers 1",
            "Pre-Chorus",
            "Chorus",
            "Vers 2",
            "Pre-Chorus",
            "Chorus"
        ]
    );
}

/// A song read from a `.ccli` file has to survive being written out and read
/// back, which is what the presentation frontend does with it.
#[test]
fn test_song_round_trips_through_json() {
    let song = import_song_from_file(GERMAN).expect("import");

    let json = serde_json::to_string(&song).expect("serialise");
    let restored: cantara_songlib::song::Song = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(restored, song);
    // The original headings are part of the model, so they survive too.
    assert_eq!(
        restored.part_at(0).unwrap().label.as_deref(),
        Some("Vers 1")
    );
}
