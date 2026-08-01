//! Tests for the complex slide layout: notation stacked on top of any number of
//! languages.
//!
//! The guarantee these tests are built around is that **every row of a slide
//! covers the same passage of the song** — the notation spans exactly the
//! lyrics lines printed underneath it.

use cantara_songlib::exporter::abc::{AbcSettings, PartPhrases};
use cantara_songlib::exporter::slides::slides_from_song;
use cantara_songlib::importer::{import_song_from_file, song_yml};
use cantara_songlib::slides::{
    ComplexSlide, LanguageConfiguration, ShowMetaInformation, Slide, SlideContent, SlideElement,
    SlideRowKind, SlideSettings,
};
use cantara_songlib::song::Song;

/// A song with two verses, each in English and German, sharing one melody.
const BILINGUAL: &str = r#"
version: 0.1
title: Bilingual
default_language: en
score:
  key: c major
  time: 4/4
parts:
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f | g2 g2 | a4 g f e | d2 c2
    - type: lyrics
      number: 1
      language: en
      content: |
        A -- ma -- zing grace how sweet
        the sound that saved a wretch
    - type: lyrics
      number: 1
      language: de
      content: |
        Oh teu -- re Gna -- de wun -- der
        bar die mich er -- ret -- tet hat
    - type: lyrics
      number: 2
      language: en
      content: |
        Twas grace that taught my heart
        and grace my fears re -- lieved
    - type: lyrics
      number: 2
      language: de
      content: |
        Die Gna -- de lehr -- te mich zu
        fürch -- ten und nahm die Furcht
"#;

fn settings(elements: Vec<SlideElement>, max_lines: Option<usize>) -> SlideSettings {
    SlideSettings {
        title_slide: false,
        empty_last_slide: false,
        show_spoiler: true,
        max_lines,
        meta_syntax: String::new(),
        show_meta_information: ShowMetaInformation::none(),
        language: LanguageConfiguration::Complex(elements),
    }
}

fn complex_slides(slides: &[Slide]) -> Vec<&ComplexSlide> {
    slides
        .iter()
        .filter_map(|slide| match &slide.slide_content {
            SlideContent::Complex(complex) => Some(complex),
            _ => None,
        })
        .collect()
}

fn notation(elements: &[SlideElement]) -> Vec<SlideElement> {
    elements.to_vec()
}

fn lyrics_rows(slide: &ComplexSlide) -> Vec<(Option<String>, String)> {
    slide
        .rows
        .iter()
        .filter_map(|row| match &row.kind {
            SlideRowKind::Lyrics { language } => Some((language.clone(), row.content.clone())),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[test]
fn test_rows_appear_in_the_requested_order() {
    let song = song_yml::import_from_yml_string(BILINGUAL).unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
        SlideElement::Lyrics("de".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let slides = complex_slides(&slides);
    assert_eq!(slides.len(), 2, "one slide per verse");

    let first = slides[0];
    assert_eq!(first.rows.len(), 3);
    assert!(first.rows[0].is_notation());
    assert_eq!(
        lyrics_rows(first),
        [
            (Some("en".to_string()), "Amazing grace how sweet\nthe sound that saved a wretch".to_string()),
            (Some("de".to_string()), "Oh teure Gnade wunder\nbar die mich errettet hat".to_string()),
        ]
    );

    // Asking for the languages the other way round swaps the rows.
    let swapped = notation(&[
        SlideElement::Lyrics("de".to_string()),
        SlideElement::Lyrics("en".to_string()),
    ]);
    let slides = slides_from_song(&song, &settings(swapped, None));
    let slides = complex_slides(&slides);
    let languages: Vec<Option<String>> = lyrics_rows(slides[0])
        .into_iter()
        .map(|(language, _)| language)
        .collect();
    assert_eq!(languages, [Some("de".to_string()), Some("en".to_string())]);
}

#[test]
fn test_a_language_the_song_lacks_is_left_out() {
    let song = song_yml::import_from_yml_string(BILINGUAL).unwrap();
    let elements = notation(&[
        SlideElement::Lyrics("en".to_string()),
        SlideElement::Lyrics("fr".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let slides = complex_slides(&slides);

    // No empty French row — the row is simply absent.
    assert_eq!(lyrics_rows(slides[0]).len(), 1);
    assert_eq!(lyrics_rows(slides[0])[0].0, Some("en".to_string()));
}

// ---------------------------------------------------------------------------
// The notation matches the text
// ---------------------------------------------------------------------------

/// The heart of this slide type: for every slide, the notation has to carry
/// exactly as many syllables as the lyrics printed below it.
///
/// The count is taken from the song model rather than from the rendered text,
/// because the text on a slide has its `--` syllable markers stripped for
/// reading — "A -- ma -- zing" is shown as "Amazing" but is still sung on three
/// notes.
#[test]
fn test_notation_covers_exactly_the_lyrics_shown() {
    let song = song_yml::import_from_yml_string(BILINGUAL).unwrap();

    for max_lines in [None, Some(1), Some(2)] {
        let elements = notation(&[
            SlideElement::Notation,
            SlideElement::Lyrics("en".to_string()),
        ]);
        let slides = slides_from_song(&song, &settings(elements, max_lines));
        let slides = complex_slides(&slides);

        // What the notation of each slide claims to cover …
        let claimed: Vec<usize> = slides
            .iter()
            .map(|slide| {
                slide
                    .rows
                    .iter()
                    .find_map(|row| match row.kind {
                        SlideRowKind::Notation { syllables } => Some(syllables),
                        _ => None,
                    })
                    .expect("every slide has a notation row")
            })
            .collect();

        // … against the syllables of the lyrics lines it shows, counted
        // independently from the song's own text.
        let mut expected: Vec<usize> = Vec::new();
        for part in song.ordered_parts() {
            let per_line = syllables_per_line(part);
            let step = max_lines.unwrap_or(per_line.len()).max(1);
            let mut start = 0;
            while start < per_line.len() {
                let end = (start + step).min(per_line.len());
                expected.push(per_line[start..end].iter().sum());
                start = end;
            }
        }

        assert_eq!(
            claimed, expected,
            "max_lines={:?}: notation and lyrics disagree",
            max_lines
        );
    }
}

/// Syllables per lyrics line, straight from the song model.
///
/// `--` separates the syllables of one word, whether written with spaces
/// around it or glued on ("hei -- lig" and "hei--lig" both count as two).
fn syllables_per_line(part: &cantara_songlib::song::SongPart) -> Vec<usize> {
    let content = part.lyrics_for(None, None).expect("the part has lyrics");
    content
        .content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_whitespace()
                .filter(|word| *word != "--")
                .map(|word| word.split("--").filter(|part| !part.is_empty()).count())
                .sum()
        })
        .collect()
}

#[test]
fn test_wrapping_splits_notation_and_text_together() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
    ]);

    let unwrapped = slides_from_song(&song, &settings(elements.clone(), None));
    let wrapped = slides_from_song(&song, &settings(elements, Some(2)));

    // Three verses of four lines: one slide each, or two slides each.
    assert_eq!(complex_slides(&unwrapped).len(), 3);
    assert_eq!(complex_slides(&wrapped).len(), 6);

    for slide in complex_slides(&unwrapped) {
        assert_eq!(slide.line_count, 4);
    }
    for slide in complex_slides(&wrapped) {
        assert_eq!(slide.line_count, 2);
        assert_eq!(lyrics_rows(slide)[0].1.lines().count(), 2);
    }

    // The two halves of a verse must show different music.
    let first = &complex_slides(&wrapped)[0].rows[0].content;
    let second = &complex_slides(&wrapped)[1].rows[0].content;
    assert_ne!(first, second, "both halves got the same notation");
}

/// Each notation row is a complete ABC tune, so a renderer can take it as-is.
#[test]
fn test_notation_rows_are_standalone_abc_tunes() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, Some(2)));

    for slide in complex_slides(&slides) {
        let abc = &slide.rows[0].content;
        assert!(abc.starts_with("X:1\n"), "missing the reference number:\n{}", abc);
        assert!(abc.contains("M:3/4"), "missing the meter:\n{}", abc);
        assert!(abc.contains("L:1/4"), "missing the unit note length:\n{}", abc);
        assert!(abc.contains("K:F"), "missing the key:\n{}", abc);
        // A blank line would end the tune before the music started.
        assert!(!abc.trim_end().contains("\n\n"), "blank line inside:\n{}", abc);

        // Every music line has bar lines and is followed by its lyrics.
        let music_lines: Vec<&str> = abc
            .lines()
            .filter(|line| !line.starts_with("w:") && !is_header(line))
            .collect();
        assert!(!music_lines.is_empty(), "no music at all:\n{}", abc);
        for line in music_lines {
            assert!(line.contains('|'), "no bar lines in '{}':\n{}", line, abc);
        }
    }
}

/// Whether an ABC line is a header field such as `X:1` or `K:F`.
fn is_header(line: &str) -> bool {
    let mut chars = line.chars();
    matches!((chars.next(), chars.next()), (Some(letter), Some(':')) if letter.is_ascii_alphabetic())
}

// ---------------------------------------------------------------------------
// The words under the notes
// ---------------------------------------------------------------------------

/// The notation carries the lyrics of the first requested language, so the
/// staff can be read and sung on its own.
#[test]
fn test_notation_carries_the_first_language() {
    let song = song_yml::import_from_yml_string(BILINGUAL).unwrap();

    let english_first = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
        SlideElement::Lyrics("de".to_string()),
    ]);
    let slides = slides_from_song(&song, &settings(english_first, None));
    let abc = &complex_slides(&slides)[0].rows[0].content;
    assert!(
        abc.contains("w:A-ma-zing grace how sweet"),
        "the English lyrics are missing from the notation:\n{}",
        abc
    );
    assert!(!abc.contains("Gna-de"), "the German lyrics leaked in:\n{}", abc);

    // Asking for German first puts German under the notes instead.
    let german_first = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("de".to_string()),
        SlideElement::Lyrics("en".to_string()),
    ]);
    let slides = slides_from_song(&song, &settings(german_first, None));
    let abc = &complex_slides(&slides)[0].rows[0].content;
    assert!(
        abc.contains("w:Oh teu-re Gna-de wun-der"),
        "the German lyrics are missing from the notation:\n{}",
        abc
    );
    assert!(!abc.contains("A-ma-zing"), "the English lyrics leaked in:\n{}", abc);
}

/// The notation breaks where the text breaks: one system per lyrics line, and
/// each system is followed by its own words.
#[test]
fn test_notation_breaks_like_the_source_text() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();

    for max_lines in [None, Some(1), Some(2)] {
        let elements = notation(&[
            SlideElement::Notation,
            SlideElement::Lyrics("en".to_string()),
        ]);
        let slides = slides_from_song(&song, &settings(elements, max_lines));

        for slide in complex_slides(&slides) {
            let abc = &slide.rows[0].content;

            let music_lines = abc
                .lines()
                .filter(|line| !line.starts_with("w:") && !is_header(line))
                .count();
            let lyrics_lines = abc.lines().filter(|line| line.starts_with("w:")).count();

            assert_eq!(
                music_lines, slide.line_count,
                "max_lines={:?}: {} systems for {} lyrics lines:\n{}",
                max_lines, music_lines, slide.line_count, abc
            );
            assert_eq!(
                lyrics_lines, slide.line_count,
                "max_lines={:?}: every system needs its own words:\n{}",
                max_lines, abc
            );
        }
    }
}

/// The four lines of a verse produce four systems, matching the text rows.
#[test]
fn test_a_four_line_verse_gives_four_systems() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let slide = complex_slides(&slides)[0];

    assert_eq!(slide.line_count, 4);
    assert_eq!(
        slide.rows[0].content.lines().filter(|l| l.starts_with("w:")).count(),
        4
    );
    // The text row below shows the same four lines.
    assert_eq!(lyrics_rows(slide)[0].1.lines().count(), 4);
}

/// A translation with more syllables than the melody has notes must not be cut
/// mid-word. Every system carries its own `w:` line, so a surplus cannot push
/// the following words out of place.
#[test]
fn test_a_longer_translation_is_not_mangled() {
    // The German lines of the fixture are longer than the melody's phrases.
    let song = song_yml::import_from_yml_string(BILINGUAL).unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("de".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let abc = &complex_slides(&slides)[0].rows[0].content;

    assert!(
        abc.contains("w:Oh teu-re Gna-de wun-der"),
        "the last syllable was cut off:\n{}",
        abc
    );
    assert!(
        abc.contains("w:bar die mich er-ret-tet hat"),
        "the second line was cut off:\n{}",
        abc
    );
}

/// A shorter line is padded with ABC's skip marker so the remaining words stay
/// under the right notes.
#[test]
fn test_a_shorter_line_is_padded() {
    let yml = r#"
version: 0.1
title: Short Line
score:
  key: c major
  time: 4/4
parts:
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f | g4 a b c
    - type: lyrics
      number: 1
      content: |
        one two three four
        five six
"#;
    let song = song_yml::import_from_yml_string(yml).unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let abc = &complex_slides(&slides)[0].rows[0].content;

    // Four notes, two words — the two spare notes get skip markers.
    assert!(
        abc.contains("w:five six * *"),
        "the short line was not padded:\n{}",
        abc
    );
}

/// The words under the notes come from whatever text the first language slot
/// resolved to — including the unlabelled fallback of a song that states no
/// language.
#[test]
fn test_notation_carries_the_fallback_text() {
    let yml = r#"
version: 0.1
title: No Language Stated
score:
  key: c major
  time: 4/4
parts:
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f
    - type: lyrics
      number: 1
      content: |
        one two three four
"#;
    let song = song_yml::import_from_yml_string(yml).unwrap();
    // The song has no language information, so the first language slot picks
    // up its unlabelled text — and the notation is written for that text.
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("fr".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let slides = complex_slides(&slides);
    assert_eq!(slides.len(), 1);

    let abc = &slides[0].rows[0].content;
    assert!(
        abc.contains("w:one two three four"),
        "the fallback text is missing from the notation:\n{}",
        abc
    );
    // The text row reports "no language stated" rather than claiming French.
    assert_eq!(lyrics_rows(slides[0])[0].0, None);
}

// ---------------------------------------------------------------------------
// Songs without language information
// ---------------------------------------------------------------------------

/// A classic `.song` file states no language at all. Its text has to appear in
/// place of the first requested language rather than being dropped.
#[test]
fn test_song_without_languages_uses_the_first_requested_row() {
    let song = import_song_from_file("tests/data/Amazing Grace.song").unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
        SlideElement::Lyrics("de".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let slides = complex_slides(&slides);
    assert!(!slides.is_empty(), "the song produced no slides at all");

    let rows = lyrics_rows(slides[0]);
    // Exactly one lyrics row: the text is not repeated under the second
    // language, which would claim a translation that does not exist.
    assert_eq!(rows.len(), 1);
    // And it is reported as "no language stated" rather than as English.
    assert_eq!(rows[0].0, None);
    assert!(rows[0].1.contains("Amazing grace"));
}

/// The fallback belongs to the first requested *language*, which is not the
/// first row when notation comes first.
#[test]
fn test_fallback_ignores_a_leading_notation_row() {
    let song = import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli").unwrap();

    // Notation first, then the language. The CCLI file has no melody, so the
    // notation row drops out and only the lyrics remain.
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("de".to_string()),
    ]);
    let slides = slides_from_song(&song, &settings(elements, None));
    let slides = complex_slides(&slides);

    assert_eq!(slides.len(), 3, "three verses");
    for slide in &slides {
        assert!(
            slide.rows.iter().all(|row| !row.is_notation()),
            "a song without a melody must not get a notation row"
        );
        assert_eq!(lyrics_rows(slide).len(), 1);
        assert_eq!(lyrics_rows(slide)[0].0, None);
    }
}

// ---------------------------------------------------------------------------
// Spoilers and meta information
// ---------------------------------------------------------------------------

#[test]
fn test_spoiler_shows_the_next_slide_in_every_language() {
    let song = song_yml::import_from_yml_string(BILINGUAL).unwrap();
    let elements = notation(&[
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
        SlideElement::Lyrics("de".to_string()),
    ]);

    let slides = slides_from_song(&song, &settings(elements, None));
    let slides = complex_slides(&slides);

    // Both languages are previewed …
    assert_eq!(slides[0].spoiler.len(), 2);
    assert!(slides[0].spoiler[0].content.contains("Twas grace"));
    assert!(slides[0].spoiler[1].content.contains("Die Gnade"));
    // … as text; a spoiler never repeats the notation.
    assert!(slides[0].spoiler.iter().all(|row| !row.is_notation()));

    // The last slide has nothing to preview.
    assert!(slides[1].spoiler.is_empty());
}

#[test]
fn test_spoiler_can_be_switched_off() {
    let song = song_yml::import_from_yml_string(BILINGUAL).unwrap();
    let elements = notation(&[SlideElement::Lyrics("en".to_string())]);

    let mut config = settings(elements, None);
    config.show_spoiler = false;

    for slide in complex_slides(&slides_from_song(&song, &config)) {
        assert!(slide.spoiler.is_empty());
    }
}

/// Meta information is placed by the same rules as on a simple slide.
#[test]
fn test_meta_information_follows_the_usual_settings() {
    let mut song = song_yml::import_from_yml_string(BILINGUAL).unwrap();
    song.set_tag("author", "Someone");

    let elements = notation(&[SlideElement::Lyrics("en".to_string())]);
    let mut config = settings(elements, None);
    config.title_slide = true;
    config.meta_syntax = "{{title}} ({{author}})".to_string();

    // slides: 0 = title, 1 and 2 = the verses.
    let cases = [
        (ShowMetaInformation::none(), vec![]),
        (ShowMetaInformation::title_slide(), vec![0]),
        (ShowMetaInformation::first_slide(), vec![1]),
        (ShowMetaInformation::last_slide(), vec![2]),
        (ShowMetaInformation::all(), vec![0, 1, 2]),
    ];

    for (show, expected) in cases {
        config.show_meta_information = show;
        let carrying: Vec<usize> = slides_from_song(&song, &config)
            .iter()
            .enumerate()
            .filter(|(_, slide)| slide.has_meta_text())
            .map(|(index, _)| index)
            .collect();
        assert_eq!(carrying, expected, "wrong slides for {:?}", show);
    }
}

// ---------------------------------------------------------------------------
// The excerpt API the slides are built on
// ---------------------------------------------------------------------------

#[test]
fn test_phrases_follow_the_lyrics_lines() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
    let verse = song.part(&"verse.1".parse().unwrap()).unwrap();

    let phrases = PartPhrases::of(&song, verse, &AbcSettings::default()).unwrap();

    assert_eq!(phrases.len(), 4);
    // "Amazing grace, How sweet the sound" — eight syllables, and so on.
    assert_eq!(
        (0..phrases.len()).map(|i| phrases.syllables(i)).collect::<Vec<_>>(),
        [8, 6, 8, 6]
    );
    assert_eq!(phrases.syllables_in(0..2), 14);
    assert_eq!(phrases.syllables_in(0..4), 28);
}

/// A verse that only references another verse's melody still has phrases.
#[test]
fn test_phrases_follow_a_shared_melody() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
    let second = song.part(&"verse.2".parse().unwrap()).unwrap();

    assert!(second.own_voice().is_none(), "verse 2 has no melody of its own");

    let phrases = PartPhrases::of(&song, second, &AbcSettings::default()).unwrap();
    assert_eq!(phrases.len(), 4);
}

#[test]
fn test_excerpt_range_is_checked() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
    let verse = song.part(&"verse.1".parse().unwrap()).unwrap();
    let phrases = PartPhrases::of(&song, verse, &AbcSettings::default()).unwrap();

    assert!(phrases.excerpt(0..4).is_some());
    assert!(phrases.excerpt(0..0).is_none(), "an empty range has no music");
    assert!(phrases.excerpt(0..99).is_none(), "out of range");
    assert!(phrases.excerpt(99..100).is_none(), "out of range");
}

#[test]
fn test_no_melody_means_no_phrases() {
    let song: Song = import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli").unwrap();
    let verse = song.part_at(0).unwrap();

    assert!(PartPhrases::of(&song, verse, &AbcSettings::default()).is_none());
}
