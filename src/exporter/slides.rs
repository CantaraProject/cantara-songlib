//! Generic Song → Slides converter.
//! Generates presentation slides from any Song, regardless of the import format.
//! Supports both single-language and multi-language slide generation.

use std::collections::HashMap;

use crate::slides::{wrap_blocks, LanguageConfiguration, Slide, SlideSettings};
use crate::song::{LyricLanguage, Song, SongPartContentType};
use crate::templating::render_metadata;

/// Strip LilyPond syllable markers (`--`) from lyrics text for presentation display.
fn strip_lilypond_markers(text: &str) -> String {
    // Replace " -- " (syllable separator) with nothing, joining syllables
    let result = text.replace(" -- ", "");
    // Also handle cases where -- appears at line boundaries
    result.replace("-- ", "").replace(" --", "")
}

/// Find lyrics content for a single language in a song part's contents.
/// Tries to match the requested language first, then falls back to default/first lyrics.
fn find_lyrics_for_language<'a>(
    contents: &'a [crate::song::SongPartContent],
    language: &Option<String>,
    default_language: &Option<String>,
) -> Option<&'a crate::song::SongPartContent> {
    // If a specific language is requested, try to find it
    let target_lang = language.as_ref().or(default_language.as_ref());

    if let Some(lang) = target_lang {
        let found = contents.iter().find(|c| match &c.voice_type {
            SongPartContentType::Lyrics {
                language: LyricLanguage::Specific(l),
            } => l == lang,
            _ => false,
        });
        if found.is_some() {
            return found;
        }
    }

    // Fall back to first lyrics
    contents.iter().find(|c| c.voice_type.is_lyrics())
}

/// Find lyrics content for multiple languages in a song part's contents.
/// Returns one lyrics string per requested language, in the order specified.
fn find_lyrics_for_languages(
    contents: &[crate::song::SongPartContent],
    languages: &[String],
) -> Vec<String> {
    languages
        .iter()
        .filter_map(|lang| {
            contents
                .iter()
                .find(|c| match &c.voice_type {
                    SongPartContentType::Lyrics {
                        language: LyricLanguage::Specific(l),
                    } => l == lang,
                    _ => false,
                })
                .map(|c| strip_lilypond_markers(&c.content))
        })
        .collect()
}

/// Resolve which languages to use for multi-language mode.
/// If the requested list is empty, returns all languages available in the song.
fn resolve_multi_languages(song: &Song, requested: &[String]) -> Vec<String> {
    if requested.is_empty() {
        song.get_available_languages()
    } else {
        requested.to_vec()
    }
}

/// Build metadata text from song tags using the template in settings.
fn build_meta_text(song: &Song, settings: &SlideSettings) -> Option<String> {
    let mut metadata: HashMap<String, String> = song.get_tags().clone();
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

/// Get ordered parts for the song.
fn get_ordered_parts(
    song: &Song,
) -> Vec<std::rc::Rc<std::cell::RefCell<crate::song::SongPart>>> {
    if let Some(order) = song.part_orders.first() {
        order.to_parts(song)
    } else {
        song.get_unpacked_parts()
            .into_iter()
            .map(|p| std::rc::Rc::new(std::cell::RefCell::new(p)))
            .collect()
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
        slides.push(Slide::new_title_slide(song.title.clone(), meta_text.clone()));
    }

    let ordered_parts = get_ordered_parts(song);

    let mut blocks: Vec<Vec<String>> = Vec::new();
    for part_ref in &ordered_parts {
        let part = part_ref.borrow();
        let lyrics_content =
            find_lyrics_for_language(&part.contents, language, &song.default_language);
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

    let ordered_parts = get_ordered_parts(song);

    // Collect per-part multi-language blocks.
    // Each entry is a Vec<String> with one text block per language.
    let mut multi_blocks: Vec<Vec<String>> = Vec::new();

    for part_ref in &ordered_parts {
        let part = part_ref.borrow();
        let texts = find_lyrics_for_languages(&part.contents, &languages);
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
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
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
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
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
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
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
