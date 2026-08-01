//! Generic Song → Slides converter.
//! Generates presentation slides from any Song, regardless of the import format.
//! Supports both single-language and multi-language slide generation.

use crate::slides::{wrap_blocks, LanguageConfiguration, Slide, SlideSettings};
use crate::song::{LyricLanguage, Song, SongPart};
use crate::templating::MetaTemplate;

/// Strip LilyPond lyric markup from lyrics text for presentation display.
///
/// Removes syllable separators (`--`), melisma placeholders (`_`) and inline
/// commands together with their arguments (e.g.
/// `\set ignoreMelismata = ##t`, `\unset ignoreMelismata`), none of which are
/// meant to be seen by the audience.
fn strip_lilypond_markers(text: &str) -> String {
    // Replace " -- " (syllable separator) with nothing, joining syllables
    let result = text.replace(" -- ", "");
    // Also handle cases where -- appears at line boundaries
    let result = result.replace("-- ", "").replace(" --", "");

    result
        .lines()
        .map(strip_lilypond_markers_in_line)
        .collect::<Vec<String>>()
        .join("\n")
}

/// Remove LilyPond commands and melisma markers from a single lyrics line.
fn strip_lilypond_markers_in_line(line: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut words = line.split_whitespace().peekable();

    while let Some(word) = words.next() {
        if word.starts_with('\\') {
            // A command such as `\set ignoreMelismata = ##t` — drop the command
            // name, its target and, if present, the `= value` assignment.
            words.next();
            if words.peek() == Some(&"=") {
                words.next();
                words.next();
            }
            continue;
        }
        // `_` is LilyPond's melisma extender and carries no text.
        if word == "_" {
            continue;
        }
        kept.push(word);
    }

    kept.join(" ")
}

/// Lyrics of a part in several languages, one entry per requested language and
/// in the order they were requested.
///
/// Languages the part has no text for are skipped rather than padded, so a
/// slide never shows an empty block.
fn find_lyrics_for_languages(part: &SongPart, languages: &[String]) -> Vec<String> {
    languages
        .iter()
        .filter_map(|language| part.lyrics_in(&LyricLanguage::specific(language)))
        .map(|content| strip_lilypond_markers(&content.content))
        .collect()
}

/// Resolve which languages to use for multi-language mode.
/// If the requested list is empty, returns all languages available in the song.
fn resolve_multi_languages(song: &Song, requested: &[String]) -> Vec<String> {
    if requested.is_empty() {
        song.available_languages()
    } else {
        requested.to_vec()
    }
}

/// The meta information line for a song, or `None` when there is nothing to
/// show.
///
/// The template is compiled once per song and reused for every slide. A
/// malformed template yields no metadata rather than aborting the export — the
/// caller can compile it with [`MetaTemplate::parse`] beforehand to be told
/// about the mistake.
fn build_meta_text(song: &Song, settings: &SlideSettings) -> Option<String> {
    if settings.show_meta_information.is_none() {
        return None;
    }
    MetaTemplate::parse(&settings.meta_syntax)
        .ok()?
        .render_song(song)
}

/// The meta text for a content slide, honouring where it is meant to appear.
fn meta_for_position(
    meta_text: &Option<String>,
    settings: &SlideSettings,
    index: usize,
    count: usize,
) -> Option<String> {
    if settings.show_meta_information.on_content_slide(index, count) {
        meta_text.clone()
    } else {
        None
    }
}

/// The meta text for the title slide, honouring the setting.
fn meta_for_title_slide(meta_text: &Option<String>, settings: &SlideSettings) -> Option<String> {
    if settings.show_meta_information.on_title_slide() {
        meta_text.clone()
    } else {
        None
    }
}

/// Generate single-language presentation slides from a Song.
fn generate_single_language_slides(
    song: &Song,
    settings: &SlideSettings,
    language: &Option<String>,
) -> Vec<Slide> {
    let mut slides: Vec<Slide> = Vec::new();
    let meta_text = build_meta_text(song, settings);

    if settings.title_slide {
        slides.push(Slide::new_title_slide(
            song.title.clone(),
            meta_for_title_slide(&meta_text, settings),
        ));
    }

    let ordered_parts = song.ordered_parts();

    let mut blocks: Vec<Vec<String>> = Vec::new();
    for part in &ordered_parts {
        let lyrics_content =
            part.lyrics_for(language.as_deref(), song.default_language.as_deref());
        if let Some(content) = lyrics_content {
            let cleaned = strip_lilypond_markers(&content.content);
            let lines: Vec<String> = cleaned.lines().map(|l| l.to_string()).collect();
            if !lines.is_empty() {
                blocks.push(lines);
            }
        }
    }

    // Apply wrapping if max_lines is set
    if let Some(max_lines) = settings.max_lines {
        let wrapped = wrap_blocks(&vec![blocks.clone()], max_lines, true);
        if let Some(first) = wrapped.first() {
            blocks = first.clone();
        }
    }

    let count = blocks.len();
    for (index, block) in blocks.iter().enumerate() {
        let displayed_meta = meta_for_position(&meta_text, settings, index, count);
        let spoiler = if settings.show_spoiler {
            blocks.get(index + 1).map(|next| next.join("\n"))
        } else {
            None
        };
        slides.push(Slide::new_content_slide(
            block.join("\n"),
            spoiler,
            displayed_meta,
        ));
    }

    if settings.empty_last_slide {
        slides.push(Slide::new_empty_slide(false));
    }

    slides
}

/// Generate multi-language presentation slides from a Song.
/// Each slide contains the same song part's lyrics in multiple languages.
fn generate_multi_language_slides(
    song: &Song,
    settings: &SlideSettings,
    requested_languages: &[String],
) -> Vec<Slide> {
    let mut slides: Vec<Slide> = Vec::new();
    let meta_text = build_meta_text(song, settings);
    let languages = resolve_multi_languages(song, requested_languages);

    if languages.is_empty() {
        // No languages found — fall back to single-language mode
        return generate_single_language_slides(song, settings, &None);
    }

    if settings.title_slide {
        slides.push(Slide::new_title_slide(
            song.title.clone(),
            meta_for_title_slide(&meta_text, settings),
        ));
    }

    let ordered_parts = song.ordered_parts();

    // Collect per-part multi-language blocks.
    // Each entry is a Vec<String> with one text block per language.
    let mut multi_blocks: Vec<Vec<String>> = Vec::new();

    for part in &ordered_parts {
        let texts = find_lyrics_for_languages(part, &languages);
        if !texts.is_empty() {
            multi_blocks.push(texts);
        }
    }

    let count = multi_blocks.len();
    for (index, block_texts) in multi_blocks.iter().enumerate() {
        let displayed_meta = meta_for_position(&meta_text, settings, index, count);

        let spoiler = if settings.show_spoiler {
            multi_blocks.get(index + 1).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };

        slides.push(Slide::new_multi_language_content_slide(
            block_texts.clone(),
            spoiler,
            displayed_meta,
        ));
    }

    if settings.empty_last_slide {
        slides.push(Slide::new_empty_slide(false));
    }

    slides
}

/// Generate presentation slides from a Song struct.
///
/// This is the generic converter that works with any Song, whether it was
/// imported from .song, .song.yml, .cssf, or constructed programmatically.
///
/// The `LanguageConfiguration` in `SlideSettings` controls whether
/// single-language or multi-language slides are generated.
pub fn slides_from_song(song: &Song, settings: &SlideSettings) -> Vec<Slide> {
    match &settings.language {
        LanguageConfiguration::SingleLanguage(lang) => {
            generate_single_language_slides(song, settings, lang)
        }
        LanguageConfiguration::MultiLanguage(langs) => {
            generate_multi_language_slides(song, settings, langs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::song_yml;
    use crate::slides::{ShowMetaInformation, SlideContent};

    #[test]
    fn test_slides_from_yml_song() {
        let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = SlideSettings {
            title_slide: true,
            show_spoiler: true,
            show_meta_information: ShowMetaInformation::none(),
            meta_syntax: "".to_string(),
            empty_last_slide: true,
            max_lines: None,
            language: LanguageConfiguration::default(),
        };

        let slides = slides_from_song(&song, &settings);

        // Title slide + 3 verse slides + empty last slide = 5
        assert_eq!(slides.len(), 5);
        assert!(matches!(slides[0].slide_content, SlideContent::Title(_)));
        assert!(matches!(
            slides[1].slide_content,
            SlideContent::SingleLanguageMainContent(_)
        ));
        assert!(matches!(
            slides[4].slide_content,
            SlideContent::Empty(_)
        ));
    }

    #[test]
    fn test_single_language_specific() {
        let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = SlideSettings {
            title_slide: false,
            show_spoiler: false,
            show_meta_information: ShowMetaInformation::none(),
            meta_syntax: "".to_string(),
            empty_last_slide: false,
            max_lines: None,
            language: LanguageConfiguration::SingleLanguage(Some("en".to_string())),
        };

        let slides = slides_from_song(&song, &settings);
        assert!(!slides.is_empty());
        for slide in &slides {
            assert!(matches!(
                slide.slide_content,
                SlideContent::SingleLanguageMainContent(_)
            ));
        }
    }

    #[test]
    fn test_multi_language_all() {
        let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let settings = SlideSettings {
            title_slide: false,
            show_spoiler: false,
            show_meta_information: ShowMetaInformation::none(),
            meta_syntax: "".to_string(),
            empty_last_slide: false,
            max_lines: None,
            language: LanguageConfiguration::MultiLanguage(vec![]),
        };

        let slides = slides_from_song(&song, &settings);

        // The test file only has "en" as explicit language.
        // Parts with only one language available will still produce slides.
        assert!(!slides.is_empty());
    }

    // --- Meta information -----------------------------------------------

    /// Build a three-verse song with metadata for the position tests.
    fn song_with_metadata() -> Song {
        let mut song = Song::new("Amazing Grace");
        song.set_tag("author", "John Newton");
        for text in ["verse one", "verse two", "verse three"] {
            let id = song.add_part_of_type(crate::song::SongPartType::Verse, None);
            song.part_mut(&id)
                .unwrap()
                .add_content(crate::song::SongPartContent::lyrics(
                    LyricLanguage::Default,
                    text,
                ));
        }
        song.add_guessed_part_order();
        song
    }

    fn settings_with(show: ShowMetaInformation) -> SlideSettings {
        SlideSettings {
            title_slide: true,
            meta_syntax: "{{title}} ({{author}})".to_string(),
            show_meta_information: show,
            empty_last_slide: false,
            show_spoiler: false,
            max_lines: None,
            language: LanguageConfiguration::default(),
        }
    }

    /// The indices of the slides that carry a meta line.
    fn slides_with_meta(song: &Song, settings: &SlideSettings) -> Vec<usize> {
        slides_from_song(song, settings)
            .iter()
            .enumerate()
            .filter(|(_, slide)| slide.has_meta_text())
            .map(|(index, _)| index)
            .collect()
    }

    #[test]
    fn test_meta_appears_only_where_asked_for() {
        let song = song_with_metadata();
        // slides: 0 = title, 1..=3 = the three verses.
        let cases = [
            (ShowMetaInformation::none(), vec![]),
            (ShowMetaInformation::title_slide(), vec![0]),
            (ShowMetaInformation::first_slide(), vec![1]),
            (ShowMetaInformation::last_slide(), vec![3]),
            (ShowMetaInformation::first_and_last_slide(), vec![1, 3]),
            (ShowMetaInformation::all(), vec![0, 1, 3]),
        ];

        for (show, expected) in cases {
            assert_eq!(
                slides_with_meta(&song, &settings_with(show)),
                expected,
                "wrong slides for {:?}",
                show
            );
        }
    }

    #[test]
    fn test_meta_text_is_rendered_from_the_template() {
        let song = song_with_metadata();
        let slides = slides_from_song(&song, &settings_with(ShowMetaInformation::title_slide()));

        let rendered = serde_json::to_string(&slides[0]).unwrap();
        assert!(
            rendered.contains("Amazing Grace (John Newton)"),
            "the template was not rendered: {}",
            rendered
        );
    }

    /// A song with a single content slide has that slide be both the first and
    /// the last, and the meta line must not be duplicated onto it twice.
    #[test]
    fn test_single_content_slide_is_both_first_and_last() {
        let mut song = Song::new("One Block");
        song.set_tag("author", "Someone");
        let id = song.add_part_of_type(crate::song::SongPartType::Verse, None);
        song.part_mut(&id)
            .unwrap()
            .add_content(crate::song::SongPartContent::lyrics(
                LyricLanguage::Default,
                "the only verse",
            ));
        song.add_guessed_part_order();

        for show in [
            ShowMetaInformation::first_slide(),
            ShowMetaInformation::last_slide(),
            ShowMetaInformation::first_and_last_slide(),
        ] {
            assert_eq!(slides_with_meta(&song, &settings_with(show)), [1]);
        }
    }

    /// An empty template means no meta line, whatever the positions say.
    #[test]
    fn test_blank_template_shows_nothing() {
        let song = song_with_metadata();
        let settings = SlideSettings {
            meta_syntax: String::new(),
            ..settings_with(ShowMetaInformation::all())
        };
        assert!(slides_with_meta(&song, &settings).is_empty());
    }

    /// A template whose placeholders the song has no values for produces an
    /// empty line, which should be left off rather than shown blank.
    #[test]
    fn test_template_without_values_shows_nothing() {
        let mut song = song_with_metadata();
        song.remove_tag("author");

        let settings = SlideSettings {
            meta_syntax: "{{author}}".to_string(),
            ..settings_with(ShowMetaInformation::all())
        };
        assert!(slides_with_meta(&song, &settings).is_empty());
    }

    /// A malformed template must not abort the export; the slides come out
    /// without metadata. The command line checks the template up front so the
    /// user still gets told.
    #[test]
    fn test_malformed_template_does_not_break_the_export() {
        let song = song_with_metadata();
        let settings = SlideSettings {
            meta_syntax: "{{#if author}}never closed".to_string(),
            ..settings_with(ShowMetaInformation::all())
        };

        let slides = slides_from_song(&song, &settings);
        assert_eq!(slides.len(), 4, "the slides themselves should still be there");
        assert!(slides_with_meta(&song, &settings).is_empty());
    }

    /// Multi-language mode places the meta line by the same rules.
    #[test]
    fn test_meta_in_multi_language_mode() {
        let mut song = Song::new("Two Languages");
        song.set_tag("author", "Someone");
        for texts in [("one", "eins"), ("two", "zwei")] {
            let id = song.add_part_of_type(crate::song::SongPartType::Verse, None);
            let part = song.part_mut(&id).unwrap();
            part.add_content(crate::song::SongPartContent::lyrics(
                LyricLanguage::specific("en"),
                texts.0,
            ));
            part.add_content(crate::song::SongPartContent::lyrics(
                LyricLanguage::specific("de"),
                texts.1,
            ));
        }
        song.add_guessed_part_order();

        let settings = SlideSettings {
            language: LanguageConfiguration::MultiLanguage(vec![]),
            ..settings_with(ShowMetaInformation::all())
        };

        // slides: 0 = title, 1 = verse one, 2 = verse two.
        assert_eq!(slides_with_meta(&song, &settings), [0, 1, 2]);
    }

    #[test]
    fn test_lilypond_markers_stripped() {
        let input = "A -- ma -- zing grace, How sweet the sound";
        let result = strip_lilypond_markers(input);
        assert_eq!(result, "Amazing grace, How sweet the sound");
    }
}
