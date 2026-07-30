//! Generic Song → Slides converter.
//! Generates presentation slides from any Song, regardless of the import format.
//! Supports both single-language and multi-language slide generation.

use std::collections::HashMap;

use crate::slides::{wrap_blocks, LanguageConfiguration, Slide, SlideSettings};
use crate::song::{LyricLanguage, Song, SongPart};
use crate::templating::render_metadata;

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

/// Build metadata text from song tags using the template in settings.
fn build_meta_text(song: &Song, settings: &SlideSettings) -> Option<String> {
    let mut metadata: HashMap<String, String> = song
        .tags()
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    metadata.insert("title".to_string(), song.title.clone());
    match render_metadata(&settings.meta_syntax, &metadata) {
        Ok(ref s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Determine whether meta text should be shown on a slide at the given position.
fn meta_for_position(
    meta_text: &Option<String>,
    settings: &SlideSettings,
    index: usize,
    count: usize,
) -> Option<String> {
    meta_text.as_ref().and_then(|_| {
        let is_first = index == 0;
        let is_last = index == count - 1;
        if (settings.show_meta_information.on_first_slide() && is_first)
            || (settings.show_meta_information.on_last_slide() && is_last)
        {
            meta_text.clone()
        } else {
            None
        }
    })
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
        slides.push(Slide::new_title_slide(song.title.clone(), meta_text.clone()));
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
        slides.push(Slide::new_title_slide(song.title.clone(), meta_text.clone()));
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
            show_meta_information: ShowMetaInformation::None,
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
            show_meta_information: ShowMetaInformation::None,
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
            show_meta_information: ShowMetaInformation::None,
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

    #[test]
    fn test_lilypond_markers_stripped() {
        let input = "A -- ma -- zing grace, How sweet the sound";
        let result = strip_lilypond_markers(input);
        assert_eq!(result, "Amazing grace, How sweet the sound");
    }
}
