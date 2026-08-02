//! Generic Song → Slides converter.
//! Generates presentation slides from any Song, regardless of the import format.
//! Supports both single-language and multi-language slide generation.

use crate::exporter::abc::{AbcSettings, PartPhrases};
use crate::slides::{
    wrap_blocks, LanguageConfiguration, LinkedEntity, PresentationChapter, Slide, SlideElement,
    SlideRow, SlideSettings,
};
use crate::song::{LyricLanguage, Song, SongPart};
use crate::templating::MetaTemplate;

/// Turn stored lyrics into text a human should read.
///
/// The song model keeps lyrics as they are sung, with LilyPond's syllable and
/// melisma markup. Anything shown to an audience — a slide or a text export —
/// wants that removed.
pub fn lyrics_for_reading(text: &str) -> String {
    strip_lilypond_markers(text)
}

/// Strip LilyPond lyric markup from lyrics text for presentation display.
///
/// Removes syllable separators (`--`), melisma placeholders (`_`), the `[…]`
/// brackets that mark a region where melismata are ignored, and inline commands
/// together with their arguments (e.g. `\set ignoreMelismata = ##t`,
/// `\unset ignoreMelismata`) — none of which are meant to be seen by the
/// audience.
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
        // `[…]` marks a region where melismata are ignored. The brackets are
        // markup, not lyrics, and may hug the words they enclose.
        let word = word.trim_start_matches('[').trim_end_matches(']');
        if word.is_empty() {
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
        let wrapped = wrap_blocks(std::slice::from_ref(&blocks), max_lines, true);
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

// ---------------------------------------------------------------------------
// Complex slides: notation plus any number of languages
// ---------------------------------------------------------------------------

/// The position of each element among the *language* elements.
///
/// The fallback for a song without language information applies to the first
/// requested **language**, which is not the first row when notation comes
/// first. `None` marks a notation element.
fn language_positions(elements: &[SlideElement]) -> Vec<Option<usize>> {
    let mut next = 0;
    elements
        .iter()
        .map(|element| match element {
            SlideElement::Notation => None,
            SlideElement::Lyrics(_) => {
                let position = next;
                next += 1;
                Some(position)
            }
        })
        .collect()
}

/// One chunk of a song part: the lyrics lines it covers, per requested element.
struct Chunk<'song> {
    part: &'song SongPart,
    /// Which of the part's lyrics lines this chunk covers.
    lines: std::ops::Range<usize>,
    /// The text of each requested element, `None` where the song has nothing.
    /// Parallel to the requested elements; the notation entry is always `None`.
    texts: Vec<Option<String>>,
    /// The lyrics that go under the notes, as the song stores them — `--`
    /// syllable markers intact, because those are what the notation needs to
    /// tell a syllable from a word. This is the text of the first requested
    /// language, which is the one the notation is written for.
    notation_lyrics: Option<String>,
    /// Which requested element supplied [`Chunk::notation_lyrics`]. That row
    /// repeats what the notation already shows and is flagged accordingly.
    notation_slot: Option<usize>,
}

/// Generate [`SlideContent::Complex`] slides.
///
/// Each part of the song is cut into chunks of at most
/// [`SlideSettings::max_lines`] lyrics lines, and every chunk becomes one
/// slide whose rows all cover exactly those lines.
fn generate_complex_slides(
    song: &Song,
    settings: &SlideSettings,
    elements: &[SlideElement],
) -> Vec<Slide> {
    let mut slides: Vec<Slide> = Vec::new();
    let meta_text = build_meta_text(song, settings);

    if settings.title_slide {
        slides.push(Slide::new_title_slide(
            song.title.clone(),
            meta_for_title_slide(&meta_text, settings),
        ));
    }

    let positions = language_positions(elements);
    let chunks = build_chunks(song, settings, elements, &positions);
    let abc_settings = AbcSettings::default();

    let count = chunks.len();
    for (index, chunk) in chunks.iter().enumerate() {
        let mut rows: Vec<SlideRow> = Vec::new();

        // Whether the notation made it onto the slide decides whether the row
        // it took its words from is a repetition.
        let mut notation_shown = false;

        for (slot, element) in elements.iter().enumerate() {
            match element {
                SlideElement::Notation => {
                    if let Some(row) = notation_row(song, chunk, &abc_settings) {
                        notation_shown = true;
                        rows.push(row);
                    }
                }
                SlideElement::Lyrics(language) => {
                    if let Some(text) = &chunk.texts[slot] {
                        let position = positions[slot].unwrap_or(0);
                        let row = SlideRow::lyrics(
                            language_label(song, chunk.part, language, position),
                            text.clone(),
                        );
                        rows.push(if notation_shown && chunk.notation_slot == Some(slot) {
                            row.also_shown_in_notation()
                        } else {
                            row
                        });
                    }
                }
            }
        }

        // A chunk that produced nothing at all would be a blank slide.
        if rows.is_empty() {
            continue;
        }

        // The spoiler previews the next chunk, text only.
        let spoiler: Vec<SlideRow> = if settings.show_spoiler {
            chunks
                .get(index + 1)
                .map(|next| text_rows(song, next, elements, &positions))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        slides.push(Slide::new_complex_slide(
            rows,
            spoiler,
            meta_for_position(&meta_text, settings, index, count),
            chunk.lines.len(),
        ));
    }

    if settings.empty_last_slide {
        slides.push(Slide::new_empty_slide(false));
    }

    slides
}

/// Cut every part of the song into chunks of at most `max_lines` lyrics lines.
///
/// The number of lines a part has is taken from the first requested language
/// that the part actually carries, so all rows of a slide are cut at the same
/// place.
fn build_chunks<'song>(
    song: &'song Song,
    settings: &SlideSettings,
    elements: &[SlideElement],
    positions: &[Option<usize>],
) -> Vec<Chunk<'song>> {
    let mut chunks: Vec<Chunk> = Vec::new();

    for part in song.ordered_parts() {
        // The lyrics of every requested language, as lines.
        let per_element: Vec<Option<LyricsLines>> = elements
            .iter()
            .enumerate()
            .map(|(slot, element)| match element {
                SlideElement::Notation => None,
                SlideElement::Lyrics(language) => {
                    lyrics_lines(song, part, language, positions[slot].unwrap_or(0))
                }
            })
            .collect();

        // How many lines the part has: the longest of the requested languages.
        let line_count = per_element
            .iter()
            .flatten()
            .map(|lyrics| lyrics.raw.len())
            .max()
            .unwrap_or(0);

        if line_count == 0 {
            continue;
        }

        // The notation is written for the first requested language that the
        // part actually has.
        let notation_slot = per_element
            .iter()
            .position(|lyrics| lyrics.is_some());
        let notation_source = notation_slot.and_then(|slot| per_element[slot].as_ref());

        let step = settings.max_lines.unwrap_or(line_count).max(1);

        let mut start = 0;
        while start < line_count {
            let end = (start + step).min(line_count);

            let texts = per_element
                .iter()
                .map(|lyrics| {
                    lyrics.as_ref().and_then(|lyrics| {
                        let slice = &lyrics.display
                            [start.min(lyrics.display.len())..end.min(lyrics.display.len())];
                        if slice.is_empty() {
                            None
                        } else {
                            Some(slice.join("\n"))
                        }
                    })
                })
                .collect();

            chunks.push(Chunk {
                part,
                lines: start..end,
                texts,
                notation_lyrics: notation_source.map(|lyrics| lyrics.raw.join("\n")),
                notation_slot,
            });
            start = end;
        }
    }

    chunks
}

/// The lyrics of a part in one requested language, in two shapes.
struct LyricsLines {
    /// As the song stores them, `--` syllable markers intact. The notation
    /// needs these to place one syllable per note.
    raw: Vec<String>,
    /// Cleaned up for reading on a slide.
    display: Vec<String>,
}

/// The lyrics of a part in one requested language.
///
/// The **first** requested language falls back to the song's unlabelled lyrics,
/// which is what makes a classic `.song` file — a format with no language
/// information at all — show its text under the first heading. Later languages
/// do not fall back; repeating the same text under a second heading would be
/// misleading.
fn lyrics_lines(
    song: &Song,
    part: &SongPart,
    language: &str,
    position: usize,
) -> Option<LyricsLines> {
    let content = part
        .lyrics_in(&LyricLanguage::specific(language))
        .or_else(|| {
            // `LyricLanguage::Default` counts as the song's default language.
            part.all_lyrics()
                .find(|(candidate, _)| {
                    candidate.matches(language, song.default_language.as_deref())
                })
                .map(|(_, content)| content)
        })
        .or_else(|| {
            if position == 0 {
                part.lyrics_in(&LyricLanguage::Default)
            } else {
                None
            }
        })?;

    // Both shapes are derived line by line from the same source, so they stay
    // the same length and line *i* of one is line *i* of the other.
    let mut raw = Vec::new();
    let mut display = Vec::new();
    for line in content.content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        raw.push(line.trim().to_string());
        display.push(strip_lilypond_markers(line));
    }

    if raw.is_empty() {
        None
    } else {
        Some(LyricsLines { raw, display })
    }
}

/// The language to report for a row.
///
/// `None` when the text came from the song's unlabelled lyrics, so a frontend
/// can tell "this is English" from "this song never said".
fn language_label(
    song: &Song,
    part: &SongPart,
    language: &str,
    position: usize,
) -> Option<String> {
    let stated = part
        .all_lyrics()
        .any(|(candidate, _)| candidate.matches(language, song.default_language.as_deref()));

    if stated {
        Some(language.to_string())
    } else if position == 0 {
        None
    } else {
        Some(language.to_string())
    }
}

/// The notation row for a chunk: the melody of exactly its lyrics lines.
fn notation_row(song: &Song, chunk: &Chunk, settings: &AbcSettings) -> Option<SlideRow> {
    let phrases = PartPhrases::of(song, chunk.part, settings)?;

    // The melody is split along the part's own lyrics, so a chunk that runs
    // past the last phrase is clamped rather than dropped.
    let lines = chunk.lines.start.min(phrases.len())..chunk.lines.end.min(phrases.len());

    // The words of the first requested language go under the notes; without
    // any lyrics the notation is still emitted, just bare.
    let abc = match &chunk.notation_lyrics {
        Some(lyrics) => phrases.excerpt_with_lyrics(lines.clone(), lyrics)?,
        None => phrases.excerpt(lines.clone())?,
    };

    Some(SlideRow::notation(abc, phrases.syllables_in(lines)))
}

/// The text rows of a chunk, used for the spoiler.
fn text_rows(
    song: &Song,
    chunk: &Chunk,
    elements: &[SlideElement],
    positions: &[Option<usize>],
) -> Vec<SlideRow> {
    elements
        .iter()
        .enumerate()
        .filter_map(|(slot, element)| match element {
            SlideElement::Notation => None,
            SlideElement::Lyrics(language) => chunk.texts[slot].as_ref().map(|text| {
                SlideRow::lyrics(
                    language_label(song, chunk.part, language, positions[slot].unwrap_or(0)),
                    text.clone(),
                )
            }),
        })
        .collect()
}

/// Generate presentation slides from a Song struct.
///
/// This is the generic converter that works with any Song, whether it was
/// imported from .song, .song.yml, .cssf, or constructed programmatically.
///
/// The `LanguageConfiguration` in `SlideSettings` picks the layout: one
/// language, several languages side by side, or the complex layout that stacks
/// notation and any number of languages.
pub fn slides_from_song(song: &Song, settings: &SlideSettings) -> Vec<Slide> {
    match &settings.language {
        LanguageConfiguration::SingleLanguage(lang) => {
            generate_single_language_slides(song, settings, lang)
        }
        LanguageConfiguration::MultiLanguage(langs) => {
            generate_multi_language_slides(song, settings, langs)
        }
        LanguageConfiguration::Complex(elements) => {
            generate_complex_slides(song, settings, elements)
        }
    }
}

// ---------------------------------------------------------------------------
// Several songs
// ---------------------------------------------------------------------------

/// Build one [`PresentationChapter`] per song.
///
/// A chapter groups the slides of one song and keeps a link back to the song
/// they came from, which is what lets a presentation jump between songs and
/// show where it is.
///
/// ```
/// use cantara_songlib::exporter::slides::chapters_from_songs;
/// use cantara_songlib::importer::import_song_from_file;
/// use cantara_songlib::slides::{LinkedEntity, SlideSettings};
///
/// let songs = [
///     import_song_from_file("tests/data/Amazing Grace.song.yml")?,
///     import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli")?,
/// ];
///
/// let chapters = chapters_from_songs(&songs, &SlideSettings::default());
/// assert_eq!(chapters.len(), 2);
/// assert!(!chapters[0].slides.is_empty());
///
/// match &chapters[0].linked_entity {
///     LinkedEntity::Song(song) => assert_eq!(song.title, "Amazing Grace"),
///     other => panic!("unexpected link: {:?}", other),
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn chapters_from_songs(songs: &[Song], settings: &SlideSettings) -> Vec<PresentationChapter> {
    songs
        .iter()
        .map(|song| {
            PresentationChapter::new(
                slides_from_song(song, settings),
                LinkedEntity::Song(song.clone()),
            )
        })
        .collect()
}

/// The slides of several songs, one after another.
///
/// Use this when the consumer wants a flat run of slides;
/// [`chapters_from_songs`] keeps the songs apart.
pub fn slides_from_songs(songs: &[Song], settings: &SlideSettings) -> Vec<Slide> {
    songs
        .iter()
        .flat_map(|song| slides_from_song(song, settings))
        .collect()
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

    /// The `[…]` region markers are markup for the engraver, not words. An
    /// audience must never see them on a slide.
    #[test]
    fn test_ignore_melismata_brackets_are_stripped() {
        assert_eq!(
            strip_lilypond_markers("[al le vier] hier"),
            "al le vier hier"
        );
        assert_eq!(strip_lilypond_markers("one [two] three"), "one two three");
        // Brackets standing on their own leave no empty word behind.
        assert_eq!(strip_lilypond_markers("[ one ]"), "one");
    }

    /// The command form is dropped with its arguments, as before.
    #[test]
    fn test_ignore_melismata_commands_are_stripped() {
        assert_eq!(
            strip_lilypond_markers(
                "\\set ignoreMelismata = ##t one two \\unset ignoreMelismata three"
            ),
            "one two three"
        );
    }
}
