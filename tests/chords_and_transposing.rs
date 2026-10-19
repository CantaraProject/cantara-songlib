//! Chord symbols in the notation exports, and transposing a whole song.
//!
//! The test song is "Tochter Zion, freue dich": D major, a quarter-note
//! pickup, one melody and a chord line covering all three of its systems.

use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings, PartPhrases};
use cantara_songlib::exporter::lilypond::{
    lilypond_from_song, lilypond_parts_from_song, lilypond_sequential_from_song, LilypondSettings,
};
use cantara_songlib::importer::song_yml;
use cantara_songlib::music::TransposeDirection;
use cantara_songlib::song::{Song, SongPartContentType};
use cantara_songlib::transpose::transpose_song;

fn tochter_zion() -> Song {
    let content = std::fs::read_to_string("tests/data/Tochter Zion.song.yml").unwrap();
    song_yml::import_from_yml_string(&content).unwrap()
}

fn amazing_grace() -> Song {
    let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml").unwrap();
    song_yml::import_from_yml_string(&content).unwrap()
}

// ---------------------------------------------------------------------------
// Chords in the ABC export
// ---------------------------------------------------------------------------

#[test]
fn abc_prints_the_chord_symbols_above_the_staff() {
    let abc = abc_from_song(&tochter_zion(), &AbcSettings::default()).unwrap();

    // The first bar is D, the third A with a D on its last quarter.
    assert!(
        abc.contains("\"D\"A2 (F3/2 G/)"),
        "the opening chord is missing:\n{}",
        abc
    );
    assert!(
        abc.contains("\"A\"(E/ F/ G/ A/ G) \"D\"F"),
        "a chord changing inside the bar is missing:\n{}",
        abc
    );
    // The minor and seventh chords of the sixth bar.
    assert!(abc.contains("\"Bm\""), "B minor is missing:\n{}", abc);
    assert!(abc.contains("\"D7\""), "D seventh is missing:\n{}", abc);
}

#[test]
fn a_held_chord_is_printed_once() {
    let abc = abc_from_song(&tochter_zion(), &AbcSettings::default()).unwrap();

    // Bars one and two are both D, so the second bar carries no symbol: the
    // bar line is followed by the note itself.
    assert!(
        abc.contains("| A2 D2 |"),
        "the held chord was printed again:\n{}",
        abc
    );
}

#[test]
fn chord_symbols_can_be_left_out() {
    let settings = AbcSettings {
        include_chords: false,
        ..AbcSettings::default()
    };
    let abc = abc_from_song(&tochter_zion(), &settings).unwrap();

    assert!(!abc.contains('"'), "an annotation survived:\n{}", abc);
}

#[test]
fn every_lyrics_line_still_matches_its_music_line() {
    // Chord annotations are not notes, so they must not shift the syllables.
    let with_chords = abc_from_song(&tochter_zion(), &AbcSettings::default()).unwrap();
    let without = abc_from_song(
        &tochter_zion(),
        &AbcSettings {
            include_chords: false,
            ..AbcSettings::default()
        },
    )
    .unwrap();

    let lyrics = |abc: &str| {
        abc.lines()
            .filter(|line| line.starts_with("w:"))
            .map(str::to_string)
            .collect::<Vec<String>>()
    };
    assert_eq!(lyrics(&with_chords), lyrics(&without));
    assert!(!lyrics(&with_chords).is_empty());
}

#[test]
fn a_song_without_chords_exports_as_before() {
    let abc = abc_from_song(&amazing_grace(), &AbcSettings::default()).unwrap();
    assert!(!abc.contains('"'), "unexpected annotation:\n{}", abc);
}

#[test]
fn a_phrase_excerpt_carries_the_chords_of_its_phrase() {
    let song = tochter_zion();
    let verse = song.part(&"verse.1".parse().unwrap()).unwrap();
    let phrases = PartPhrases::of(&song, verse, &AbcSettings::default()).unwrap();

    let first = phrases.excerpt(0..1).unwrap();
    assert!(first.contains("\"D\""), "no chord in the excerpt:\n{}", first);

    // The chord of a later phrase belongs to that phrase, not to the first.
    let third = phrases.excerpt(2..3).unwrap();
    assert!(third.contains("\"Bm\"") || third.contains("\"A\""), "{}", third);
    assert!(!first.contains("\"Bm\""), "{}", first);
}

// ---------------------------------------------------------------------------
// Chords in the LilyPond exports
// ---------------------------------------------------------------------------

#[test]
fn every_lilypond_export_prints_the_chords() {
    let song = tochter_zion();
    let settings = LilypondSettings::default();

    let paper = lilypond_from_song(&song, &settings).unwrap();
    let sequential = lilypond_sequential_from_song(&song, &settings).unwrap();
    let parts = lilypond_parts_from_song(&song, &settings).unwrap();

    let mut outputs = vec![("paper", paper), ("sequential", sequential)];
    outputs.extend(
        parts
            .into_iter()
            .map(|part| ("part", part.ly_content)),
    );

    for (name, output) in &outputs {
        assert!(
            output.contains("\\chordmode"),
            "{} export has no chord block:\n{}",
            name,
            output
        );
        assert!(
            output.contains("ChordNames"),
            "{} export never prints the chords:\n{}",
            name,
            output
        );
        assert!(
            output.contains("b2:m d2:7"),
            "{} export lost the chord line:\n{}",
            name,
            output
        );
    }
}

#[test]
fn lilypond_chords_can_be_left_out() {
    let song = tochter_zion();
    let settings = LilypondSettings {
        include_chords: false,
        ..LilypondSettings::default()
    };

    for output in [
        lilypond_from_song(&song, &settings).unwrap(),
        lilypond_sequential_from_song(&song, &settings).unwrap(),
        lilypond_parts_from_song(&song, &settings)
            .unwrap()
            .remove(0)
            .ly_content,
    ] {
        assert!(!output.contains("chordmode"), "{}", output);
        assert!(!output.contains("ChordNames"), "{}", output);
    }
}

// ---------------------------------------------------------------------------
// Transposing
// ---------------------------------------------------------------------------

/// The chord line and the melody of the first verse.
fn verse_contents(song: &Song) -> (String, String) {
    let part = song.part(&"verse.1".parse().unwrap()).unwrap();
    let find = |wanted: &dyn Fn(&SongPartContentType) -> bool| {
        part.contents
            .iter()
            .find(|content| wanted(&content.content_type))
            .map(|content| content.content.clone())
            .unwrap()
    };
    (
        find(&|t| matches!(t, SongPartContentType::Chords)),
        find(&|t: &SongPartContentType| t.is_voice()),
    )
}

#[test]
fn transposing_up_moves_key_melody_and_chords() {
    let song = tochter_zion();
    let higher = transpose_song(&song, "F", TransposeDirection::Up).unwrap();

    assert_eq!(higher.score.key.as_deref(), Some("f major"));

    let (chords, voice) = verse_contents(&higher);
    assert!(
        chords.starts_with("f1 | f | c2. f4 | c1 | f2. c4 | d2:m f2:7 | bes2 c | f1"),
        "chords: {}",
        chords
    );
    assert!(
        voice.starts_with("c'2 a4.( bes8 ) | c2 f,"),
        "melody: {}",
        voice
    );
}

#[test]
fn transposing_down_reaches_the_lower_key() {
    let song = tochter_zion();
    let lower = transpose_song(&song, "bes major", TransposeDirection::Down).unwrap();

    assert_eq!(lower.score.key.as_deref(), Some("bes major"));

    let abc = abc_from_song(&lower, &AbcSettings::default()).unwrap();
    assert!(abc.contains("K:Bb"), "{}", abc);
    assert!(abc.contains("\"Bb\""), "{}", abc);
    // A major sixth down from a'' is c', the C above middle C.
    assert!(abc.contains("\"Bb\"F2 (D3/2 E/)"), "{}", abc);
}

#[test]
fn the_direction_decides_which_way_the_music_goes() {
    let song = tochter_zion();

    let up = transpose_song(&song, "f major", TransposeDirection::Up).unwrap();
    let down = transpose_song(&song, "f major", TransposeDirection::Down).unwrap();
    let nearest = transpose_song(&song, "f major", TransposeDirection::Nearest).unwrap();

    // Both land in F major, but an octave apart.
    assert_eq!(up.score.key, down.score.key);
    assert_ne!(verse_contents(&up).1, verse_contents(&down).1);
    // A minor third up is shorter than a major sixth down.
    assert_eq!(verse_contents(&nearest).1, verse_contents(&up).1);
}

#[test]
fn there_and_back_again_is_the_song_it_started_as() {
    let song = tochter_zion();
    let there = transpose_song(&song, "ees major", TransposeDirection::Up).unwrap();
    let back = transpose_song(&there, "d major", TransposeDirection::Down).unwrap();

    assert_eq!(back, song);
}

#[test]
fn transposing_to_its_own_key_changes_nothing() {
    let song = tochter_zion();
    for direction in [
        TransposeDirection::Up,
        TransposeDirection::Down,
        TransposeDirection::Nearest,
    ] {
        assert_eq!(transpose_song(&song, "d major", direction).unwrap(), song);
    }
}

#[test]
fn a_song_without_a_key_cannot_be_transposed() {
    let mut song = tochter_zion();
    song.score.key = None;

    let error = transpose_song(&song, "f major", TransposeDirection::Up).unwrap_err();
    assert!(error.contains("key"), "{}", error);
}

#[test]
fn an_unreadable_target_key_is_reported() {
    let error = transpose_song(&tochter_zion(), "H major", TransposeDirection::Up).unwrap_err();
    assert!(error.contains("H major"), "{}", error);
}

#[test]
fn a_transposed_song_exports_to_every_format() {
    let song = transpose_song(&tochter_zion(), "ees major", TransposeDirection::Up).unwrap();

    let abc = abc_from_song(&song, &AbcSettings::default()).unwrap();
    assert!(abc.contains("K:Eb"), "{}", abc);

    let ly = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();
    assert!(ly.contains("\\key ees \\major"), "{}", ly);

    // The chord line and the key signature have to agree: in E flat major the
    // dominant is B flat, not A sharp.
    assert!(ly.contains("ees1 | ees"), "{}", ly);
}

#[test]
fn the_lyrics_and_the_metadata_survive_transposing() {
    let song = tochter_zion();
    let transposed = transpose_song(&song, "c major", TransposeDirection::Down).unwrap();

    assert_eq!(transposed.title, song.title);
    assert_eq!(transposed.tags(), song.tags());
    assert_eq!(transposed.score.time, song.score.time);
    assert_eq!(transposed.score.partial, song.score.partial);

    let lyrics = |song: &Song| {
        song.parts()
            .iter()
            .flat_map(|part| part.contents.iter())
            .filter(|content| content.content_type.is_lyrics())
            .map(|content| content.content.clone())
            .collect::<Vec<String>>()
    };
    assert_eq!(lyrics(&transposed), lyrics(&song));
    assert_eq!(lyrics(&transposed).len(), 3);
}
