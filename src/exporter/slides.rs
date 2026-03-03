//! Generic Song → Slides converter.
//! Generates presentation slides from any Song, regardless of the import format.

use std::collections::HashMap;

use crate::slides::{wrap_blocks, Slide, SlideSettings};
use crate::song::{Song, SongPartContentType};
use crate::templating::render_metadata;

/// Strip LilyPond syllable markers (`--`) from lyrics text for presentation display.
fn strip_lilypond_markers(text: &str) -> String {
    // Replace " -- " (syllable separator) with nothing, joining syllables
    let result = text.replace(" -- ", "");
    // Also handle cases where -- appears at line boundaries
    result.replace("-- ", "").replace(" --", "")
}

/// Generate presentation slides from a Song struct.
///
/// This is the generic converter that works with any Song, whether it was
/// imported from .song, .song.yml, .cssf, or constructed programmatically.
pub fn slides_from_song(song: &Song, settings: &SlideSettings) -> Vec<Slide> {
    let mut slides: Vec<Slide> = Vec::new();

    // Build metadata HashMap for template rendering
    let mut metadata: HashMap<String, String> = song.get_tags().clone();
    metadata.insert("title".to_string(), song.title.clone());

    // Render meta text
    let meta_text_rendering_result = render_metadata(&settings.meta_syntax, &metadata);
    let meta_text: Option<String> = match meta_text_rendering_result {
        Ok(ref s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };

    // Title slide
    if settings.title_slide {
        let displayed_meta = if meta_text.is_some() {
            meta_text.clone()
        } else {
            None
        };
        slides.push(Slide::new_title_slide(song.title.clone(), displayed_meta));
    }

    // Get ordered parts using the first part order (or guess one)
    let ordered_parts = if let Some(order) = song.part_orders.first() {
        order.to_parts(song)
    } else {
        // Fallback: just use parts in their natural order
        let mut parts = Vec::new();
        for part in song.get_unpacked_parts() {
            parts.push(std::rc::Rc::new(std::cell::RefCell::new(part)));
        }
        parts
    };

    // Extract lyrics blocks from ordered parts
    let mut blocks: Vec<Vec<String>> = Vec::new();

    for part_ref in &ordered_parts {
        let part = part_ref.borrow();

        // Find lyrics content — prefer matching default_language, fall back to first lyrics
        let lyrics_content = part
            .contents
            .iter()
            .find(|c| match &c.voice_type {
                SongPartContentType::Lyrics { language } => match language {
                    crate::song::LyricLanguage::Specific(lang) => {
                        song.default_language.as_deref() == Some(lang.as_str())
                    }
                    _ => false,
                },
                _ => false,
            })
            .or_else(|| {
                part.contents
                    .iter()
                    .find(|c| c.voice_type.is_lyrics())
            });

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

    // Create content slides
    let count = blocks.len();
    for (index, block) in blocks.iter().enumerate() {
        // Determine meta text display based on settings
        let displayed_meta = if meta_text.is_some() {
            let is_first_content = index == 0;
            let is_last_content = index == count - 1;
            if (settings.show_meta_information.on_first_slide() && is_first_content)
                || (settings.show_meta_information.on_last_slide() && is_last_content)
            {
                meta_text.clone()
            } else {
                None
            }
        } else {
            None
        };

        // Spoiler: next block's content
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

    // Empty last slide
    if settings.empty_last_slide {
        slides.push(Slide::new_empty_slide(false));
    }

    slides
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
    fn test_lilypond_markers_stripped() {
        let input = "A -- ma -- zing grace, How sweet the sound";
        let result = strip_lilypond_markers(input);
        assert_eq!(result, "Amazing grace, How sweet the sound");
    }
}
