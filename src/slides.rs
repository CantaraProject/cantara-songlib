//! Here the logic for the slides is implemented

use serde::{Deserialize, Serialize};

use crate::importer::SongFile;
use crate::song::Song;

// A Presentation Chapter (mostly representing a song) which should be displayed
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PresentationChapter {
    /// The slides
    pub slides: Vec<Slide>,
    /// The linked entity -> most likely the song which was the source where the Presentation came from. Other entities might be imported later.
    pub linked_entity: LinkedEntity,
}

impl PresentationChapter {
    pub fn new(slides: Vec<Slide>, linked_entity: LinkedEntity) -> Self {
        PresentationChapter {
            slides,
            linked_entity,
        }
    }
}

/// Any source where slides can come from (now just a song, other sources might follow later)
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum LinkedEntity {
    /// A song as source for the presentation (the song has to be given as an argument)
    Song(Song),
    /// Just a Title which is given (e.g. if the presentation has been imported directly)
    Title(String),
    SongFile(SongFile),
}

/// The enum which contains all possible contents of a slide
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum SlideContent {
    SingleLanguageMainContent(SingleLanguageMainContentSlide),
    Title(TitleSlide),
    MultiLanguageMainContent(MultiLanguageMainContentSlide),
    /// Notation and any number of languages stacked on one slide
    Complex(ComplexSlide),
    SimplePicture(SimplePictureSlide),
    Empty(EmptySlide),
    /// A slide that displays a single page from a PDF document
    PdfPage(PdfPageSlide),
}

/// A struct which represents a presented slide
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Slide {
    pub slide_content: SlideContent,
    pub linked_file: Option<SongFile>,
}

impl Slide {
    pub fn new_empty_slide(black_background: bool) -> Self {
        Slide {
            slide_content: SlideContent::Empty(EmptySlide { black_background }),
            linked_file: None,
        }
    }

    pub fn new_content_slide(
        main_text: String,
        spoiler_text: Option<String>,
        meta_text: Option<String>,
    ) -> Self {
        Slide {
            slide_content: SlideContent::SingleLanguageMainContent(
                SingleLanguageMainContentSlide::new(
                    main_text.trim().to_string(),
                    spoiler_text.map(|string| string.trim().to_string()),
                    meta_text.map(|string| string.trim().to_string()),
                ),
            ),
            linked_file: None,
        }
    }

    pub fn new_title_slide(title_text: String, meta_text: Option<String>) -> Self {
        Slide {
            slide_content: SlideContent::Title(TitleSlide {
                title_text: title_text.trim().to_string(),
                meta_text: meta_text.map(|string| string.trim().to_string()),
            }),
            linked_file: None,
        }
    }

    pub fn new_pdf_page_slide(pdf_path: String, page_number: u32) -> Self {
        Slide {
            slide_content: SlideContent::PdfPage(PdfPageSlide {
                pdf_path,
                page_number,
            }),
            linked_file: None,
        }
    }

    pub fn new_multi_language_content_slide(
        main_text_list: Vec<String>,
        spoiler_text_vector: Vec<String>,
        meta_text: Option<String>,
    ) -> Self {
        let trimmed_main: Vec<String> = main_text_list
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let trimmed_spoiler: Vec<String> = spoiler_text_vector
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let parsed_meta: Option<String> = match meta_text {
            Some(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        };
        Slide {
            slide_content: SlideContent::MultiLanguageMainContent(
                MultiLanguageMainContentSlide {
                    main_text_list: trimmed_main,
                    spoiler_text_vector: trimmed_spoiler,
                    meta_text: parsed_meta,
                },
            ),
            linked_file: None,
        }
    }

    /// A slide showing notation and/or several languages stacked on top of one
    /// another. See [`ComplexSlide`].
    pub fn new_complex_slide(
        rows: Vec<SlideRow>,
        spoiler: Vec<SlideRow>,
        meta_text: Option<String>,
        line_count: usize,
    ) -> Self {
        Slide {
            slide_content: SlideContent::Complex(ComplexSlide {
                rows,
                spoiler,
                meta_text: match meta_text {
                    Some(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
                    _ => None,
                },
                line_count,
            }),
            linked_file: None,
        }
    }

    pub fn with_song_file(self, linked_file: SongFile) -> Self {
        let mut cloned_self = self.clone();
        cloned_self.linked_file = Some(linked_file);

        cloned_self
    }

    pub fn has_spoiler(&self) -> bool {
        match &self.slide_content {
            SlideContent::SingleLanguageMainContent(single_language_main_content_slide) => {
                single_language_main_content_slide.spoiler_text.is_some()
            }
            SlideContent::Title(_) => false,
            SlideContent::MultiLanguageMainContent(multi_language_main_content_slide) => {
                !multi_language_main_content_slide
                    .spoiler_text_vector
                    .is_empty()
            }
            SlideContent::Complex(complex) => !complex.spoiler.is_empty(),
            SlideContent::SimplePicture(_) => false,
            SlideContent::Empty(_) => false,
            SlideContent::PdfPage(_) => false,
        }
    }

    pub fn has_meta_text(&self) -> bool {
        match &self.slide_content {
            SlideContent::SingleLanguageMainContent(single_language_main_content_slide) => {
                single_language_main_content_slide.meta_text.is_some()
            }
            SlideContent::Title(title_slide) => title_slide.meta_text.is_some(),
            SlideContent::MultiLanguageMainContent(multi_language_main_content_slide) => {
                multi_language_main_content_slide.meta_text.is_some()
            }
            SlideContent::Complex(complex) => complex.meta_text.is_some(),
            SlideContent::SimplePicture(_) => false,
            SlideContent::Empty(_) => false,
            SlideContent::PdfPage(_) => false,
        }
    }
}

/// A slide which consists of at least a Main Text, an optional Spoiler Text with the content of the next slide and an optional Meta Text with additional information.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SingleLanguageMainContentSlide {
    /// The mandatory main text which will be displayed
    main_text: String,
    /// A smaller spoiler text which is displayed below the main text if present. It can be used to spoil the next slide or to show a secondary block content.
    spoiler_text: Option<String>,
    /// Meta information which are displayed on the slide (mostly on the bottom corner)
    meta_text: Option<String>,
}

impl SingleLanguageMainContentSlide {
    fn new(main_text: String, spoiler_text: Option<String>, meta_text: Option<String>) -> Self {
        // We don't allow empty strings in spoiler_text or meta_text
        let parsed_spoiler_text: Option<String> = match spoiler_text {
            Some(str) => match str.trim() {
                "" => None,
                _ => Some(str),
            },
            None => None,
        };
        let parsed_meta_text: Option<String> = match meta_text {
            Some(str) => match str.trim() {
                "" => None,
                _ => Some(str),
            },
            None => None,
        };

        SingleLanguageMainContentSlide {
            main_text,
            spoiler_text: parsed_spoiler_text,
            meta_text: parsed_meta_text,
        }
    }

    pub fn spoiler_text(self) -> Option<String> {
        self.spoiler_text
    }

    pub fn main_text(self) -> String {
        self.main_text
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct MultiLanguageMainContentSlide {
    pub main_text_list: Vec<String>,
    pub spoiler_text_vector: Vec<String>,
    pub meta_text: Option<String>,
}

/// An empty slide which no text content to be displayed
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct EmptySlide {
    /// If true, the default background will be overridden by a back background image
    pub black_background: bool,
}

/// A title slide (mostly at the beginning of a new song)
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct TitleSlide {
    pub title_text: String,
    pub meta_text: Option<String>,
}

/// A slide containing of a simple picture
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SimplePictureSlide {
    picture_path: String,
}

/// A slide that displays a single page from a PDF document
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct PdfPageSlide {
    /// The path to the PDF file
    pub pdf_path: String,
    /// The page number to display (1-based)
    pub page_number: u32,
}

/// A slide that stacks several representations of the same passage: the
/// melody as notation and the lyrics in one or more languages.
///
/// Every row covers **the same passage of the song**. The notation row spans
/// exactly the lyrics lines that the text rows below it show, so the notes and
/// the words line up — that is the point of this slide type.
///
/// ```text
/// ┌──────────────────────────────────────────────┐
/// │ X:1 M:3/4 L:1/4 K:F  C | F2 (A/ F/) | A2 G … │  ← Notation row
/// │ Amazing grace, how sweet the sound            │  ← Lyrics row "en"
/// │ Oh teure Gnade wunderbar                      │  ← Lyrics row "de"
/// │                                               │
/// │ That saved a wretch like me.                  │  ← spoiler (text only)
/// └──────────────────────────────────────────────┘
/// ```
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct ComplexSlide {
    /// The rows, in the order the user asked for them.
    pub rows: Vec<SlideRow>,
    /// A preview of the next slide. Text only — notation is not repeated
    /// because a spoiler is meant to be small.
    pub spoiler: Vec<SlideRow>,
    /// Meta information, placed according to
    /// [`crate::slides::ShowMetaInformation`].
    pub meta_text: Option<String>,
    /// How many lyrics lines of the song this slide covers.
    ///
    /// Every row spans these same lines, which is what makes the notation match
    /// the text. Wrapping by [`SlideSettings::max_lines`] works on this count.
    pub line_count: usize,
}

impl ComplexSlide {
    /// The rows with nothing shown twice: the notation plus every lyrics row
    /// whose text is *not* already printed under the notes.
    ///
    /// Use this for a layout that shows the notation with its words and does
    /// not want the first language repeated underneath; use
    /// [`ComplexSlide::rows`] to lay out everything yourself.
    pub fn rows_without_repetition(&self) -> impl Iterator<Item = &SlideRow> {
        self.rows.iter().filter(|row| !row.redundant)
    }

    /// The notation row, if this slide has one.
    pub fn notation(&self) -> Option<&SlideRow> {
        self.rows.iter().find(|row| row.is_notation())
    }
}

/// One row of a [`ComplexSlide`].
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SlideRow {
    /// What this row shows.
    pub kind: SlideRowKind,
    /// The row's content: a complete ABC tune for a notation row, the lyrics
    /// lines joined by newlines for a lyrics row.
    pub content: String,
    /// Whether this row's text is already printed elsewhere on the same slide.
    ///
    /// The notation row carries the words of the first requested language under
    /// its notes, so the lyrics row for that language repeats them. It is still
    /// included — a frontend may well want to show the text again in a larger
    /// font for the congregation — but it is flagged so that a layout which
    /// would rather not repeat it can leave it out. See
    /// [`ComplexSlide::rows_without_repetition`].
    #[serde(default)]
    pub redundant: bool,
}

impl SlideRow {
    /// A notation row holding a standalone ABC tune.
    pub fn notation(abc: impl Into<String>, syllables: usize) -> SlideRow {
        SlideRow {
            kind: SlideRowKind::Notation { syllables },
            content: abc.into(),
            redundant: false,
        }
    }

    /// A lyrics row. `language` is `None` when the song carries no language
    /// information at all, which is the case for the classic `.song` format.
    pub fn lyrics(language: Option<String>, text: impl Into<String>) -> SlideRow {
        SlideRow {
            kind: SlideRowKind::Lyrics { language },
            content: text.into(),
            redundant: false,
        }
    }

    /// Mark this row as repeating text that the notation already shows.
    ///
    /// ```
    /// use cantara_songlib::slides::SlideRow;
    ///
    /// let row = SlideRow::lyrics(Some("en".to_string()), "Amazing grace");
    /// assert!(!row.redundant);
    /// assert!(row.also_shown_in_notation().redundant);
    /// ```
    pub fn also_shown_in_notation(mut self) -> SlideRow {
        self.redundant = true;
        self
    }

    /// Whether this row carries notation rather than text.
    pub fn is_notation(&self) -> bool {
        matches!(self.kind, SlideRowKind::Notation { .. })
    }
}

/// What a [`SlideRow`] shows.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum SlideRowKind {
    /// The melody as ABC notation.
    Notation {
        /// How many syllables the notation covers. A frontend can use this to
        /// check that the notes and the text below really do match up.
        syllables: usize,
    },
    /// Lyrics in a language, or unlabelled lyrics when `language` is `None`.
    Lyrics {
        /// The language code, e.g. `"en"`. `None` means the song stated no
        /// language — see [`LanguageConfiguration::Complex`].
        language: Option<String>,
    },
}

/// One row that a complex presentation should show.
///
/// ```
/// use cantara_songlib::slides::{LanguageConfiguration, SlideElement};
///
/// // "notation + english + german"
/// let layout = LanguageConfiguration::Complex(vec![
///     SlideElement::Notation,
///     SlideElement::Lyrics("en".to_string()),
///     SlideElement::Lyrics("de".to_string()),
/// ]);
/// # let _ = layout;
/// ```
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum SlideElement {
    /// The melody as ABC notation, covering exactly the lyrics shown below it.
    Notation,
    /// The lyrics in the given language.
    Lyrics(String),
}

impl SlideElement {
    /// Parse a row description such as `"notation"`, `"abc"`, `"noten"` or a
    /// language code.
    ///
    /// Anything that is not a notation keyword is taken to be a language code,
    /// which is what makes `--show notation,en,de` work.
    ///
    /// ```
    /// use cantara_songlib::slides::SlideElement;
    ///
    /// assert_eq!("Noten".parse(), Ok(SlideElement::Notation));
    /// assert_eq!("abc".parse(), Ok(SlideElement::Notation));
    /// assert_eq!("de".parse(), Ok(SlideElement::Lyrics("de".to_string())));
    /// ```
    pub fn parse(text: &str) -> SlideElement {
        match text.trim().to_lowercase().as_str() {
            "notation" | "notes" | "noten" | "abc" | "score" | "music" => SlideElement::Notation,
            language => SlideElement::Lyrics(language.to_string()),
        }
    }
}

impl std::str::FromStr for SlideElement {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SlideElement::parse(s))
    }
}

/// Configuration for which language(s) to display on presentation slides
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum LanguageConfiguration {
    /// Single language mode: display lyrics in one language.
    /// If `None`, use the song's `default_language` (or fall back to the first available lyrics).
    SingleLanguage(Option<String>),

    /// Multi-language mode: display lyrics in multiple languages on each slide.
    /// Languages are shown in the order specified. If the list is empty, all available languages are used.
    MultiLanguage(Vec<String>),

    /// Complex mode: stack notation and any number of languages on each slide,
    /// in the given order. Produces [`SlideContent::Complex`] slides.
    ///
    /// A song without any language information — a classic `.song` file, say —
    /// has its unlabelled lyrics shown in place of the **first** requested
    /// language. The remaining language rows are then left out rather than
    /// repeating the same text under a different heading.
    Complex(Vec<SlideElement>),
}

impl Default for LanguageConfiguration {
    fn default() -> Self {
        LanguageConfiguration::SingleLanguage(None)
    }
}

/// Struct for specifying the settings for creating presentation slides.
/// Importers or slide creators may use this as a generic way to specify the parameters for the slide creation process.
/// Not all settings have to be used by every importer or slide creator.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SlideSettings {
    /// Specifies whether a special title slide for the song should be generated
    pub title_slide: bool,

    /// Specifies whether a spoiler should be shown as a secondary block
    pub show_spoiler: bool,

    /// Specifies whether and how to display meta information
    pub show_meta_information: ShowMetaInformation,

    /// Specifies the meta information syntax as a handlebar template
    pub meta_syntax: String,

    /// Specifies whether an empty slide at the end of each song should be added
    pub empty_last_slide: bool,

    /// Specifies the maximum amount of lines of each block. If the number is higher, the slides will be wrapped into several ones. In case of `None` this is ignored.
    pub max_lines: Option<usize>,

    /// Specifies which language(s) to display on the slides
    pub language: LanguageConfiguration,
}

impl Default for SlideSettings {
    fn default() -> Self {
        SlideSettings {
            title_slide: true,
            meta_syntax: "".to_string(),
            show_meta_information: ShowMetaInformation::all(),
            empty_last_slide: true,
            show_spoiler: true,
            max_lines: None,
            language: LanguageConfiguration::default(),
        }
    }
}

/// Which slides of a song carry the meta information line.
///
/// The three positions are independent, so any combination is expressible —
/// including showing the metadata only on the title slide, which the previous
/// enum could not express.
///
/// ```
/// use cantara_songlib::slides::ShowMetaInformation;
///
/// // Named constructors for the usual combinations …
/// assert!(ShowMetaInformation::first_and_last_slide().on_first_slide());
/// assert!(!ShowMetaInformation::first_and_last_slide().on_title_slide());
///
/// // … or pick the positions individually.
/// let custom = ShowMetaInformation {
///     title_slide: true,
///     first_slide: false,
///     last_slide: true,
/// };
/// assert!(custom.on_title_slide());
/// ```
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub struct ShowMetaInformation {
    /// Show it on the song's title slide.
    pub title_slide: bool,
    /// Show it on the first content slide.
    pub first_slide: bool,
    /// Show it on the last content slide.
    pub last_slide: bool,
}

impl ShowMetaInformation {
    /// Show no meta information anywhere.
    pub fn none() -> Self {
        ShowMetaInformation::default()
    }

    /// Show it on the title slide only.
    pub fn title_slide() -> Self {
        ShowMetaInformation {
            title_slide: true,
            ..Self::none()
        }
    }

    /// Show it on the first content slide only.
    pub fn first_slide() -> Self {
        ShowMetaInformation {
            first_slide: true,
            ..Self::none()
        }
    }

    /// Show it on the last content slide only.
    pub fn last_slide() -> Self {
        ShowMetaInformation {
            last_slide: true,
            ..Self::none()
        }
    }

    /// Show it on the first and the last content slide.
    pub fn first_and_last_slide() -> Self {
        ShowMetaInformation {
            title_slide: false,
            first_slide: true,
            last_slide: true,
        }
    }

    /// Show it on the title slide and on the first and last content slide.
    pub fn all() -> Self {
        ShowMetaInformation {
            title_slide: true,
            first_slide: true,
            last_slide: true,
        }
    }

    /// Whether anything is shown at all.
    pub fn is_none(&self) -> bool {
        !self.title_slide && !self.first_slide && !self.last_slide
    }

    /// Whether the title slide shows the meta information.
    pub fn on_title_slide(&self) -> bool {
        self.title_slide
    }

    /// Whether the first content slide shows the meta information.
    pub fn on_first_slide(&self) -> bool {
        self.first_slide
    }

    /// Whether the last content slide shows the meta information.
    pub fn on_last_slide(&self) -> bool {
        self.last_slide
    }

    /// Whether the content slide at `index` out of `count` shows it.
    ///
    /// A song with a single content slide has that slide be both the first and
    /// the last, so it shows the metadata if either position is selected.
    ///
    /// ```
    /// use cantara_songlib::slides::ShowMetaInformation;
    ///
    /// let last_only = ShowMetaInformation::last_slide();
    /// assert!(!last_only.on_content_slide(0, 3));
    /// assert!(last_only.on_content_slide(2, 3));
    ///
    /// // The only slide of a song counts as the last one.
    /// assert!(last_only.on_content_slide(0, 1));
    /// ```
    pub fn on_content_slide(&self, index: usize, count: usize) -> bool {
        if count == 0 {
            return false;
        }
        (self.first_slide && index == 0) || (self.last_slide && index + 1 == count)
    }

    /// Read the positions from a bit mask, as the C interface passes them.
    ///
    /// Bit 0 is the first content slide, bit 1 the last, bit 2 the title slide.
    /// The values `0`–`3` therefore keep the meaning the previous enum gave
    /// them, so existing callers do not have to change.
    ///
    /// ```
    /// use cantara_songlib::slides::ShowMetaInformation;
    ///
    /// assert_eq!(ShowMetaInformation::from_bits(0), ShowMetaInformation::none());
    /// assert_eq!(ShowMetaInformation::from_bits(1), ShowMetaInformation::first_slide());
    /// assert_eq!(ShowMetaInformation::from_bits(2), ShowMetaInformation::last_slide());
    /// assert_eq!(ShowMetaInformation::from_bits(3), ShowMetaInformation::first_and_last_slide());
    /// assert_eq!(ShowMetaInformation::from_bits(4), ShowMetaInformation::title_slide());
    /// assert_eq!(ShowMetaInformation::from_bits(7), ShowMetaInformation::all());
    /// ```
    pub fn from_bits(bits: u8) -> Self {
        ShowMetaInformation {
            first_slide: bits & 0b001 != 0,
            last_slide: bits & 0b010 != 0,
            title_slide: bits & 0b100 != 0,
        }
    }

    /// The inverse of [`ShowMetaInformation::from_bits`].
    pub fn to_bits(&self) -> u8 {
        (self.first_slide as u8) | ((self.last_slide as u8) << 1) | ((self.title_slide as u8) << 2)
    }
}

/// This function wraps the blocks, so that the number of lines never exceeds maximum_lines.
/// The second block is optional and will be wrapped accordingly to the first one.
/// **Warning: This function will panic, if the length of a given secondary blocks are not equal to the length of the primary block**
///
/// # Arguments
/// - `blocks`: A `&mut Vec<Vec<Vec<String>>>` with all the blocks which should be wrapped
/// - `maximum_lines`: The number of maximum lines which a block may have
/// - `persistence`: Whether block brakes are to be preserved (recommended is true)
/// Panics if secondary_block is Some(s) but s.len() != primary_block.len()
/// # Returns
/// The modified blocks as `Vec<Vec<Vec<String>>>`
pub fn wrap_blocks(
    blocks: &Vec<Vec<Vec<String>>>,
    maximum_lines: usize,
    persistence: bool,
) -> Vec<Vec<Vec<String>>> {
    if blocks.is_empty() {
        return blocks.clone();
    }

    let first_block_length = blocks[0].len();
    if blocks.len() > 1 {
        for i in 1..blocks.len() {
            if blocks[i].len() != first_block_length {
                panic!("The length of every block has to be equal.")
            }
        }
    }

    let mut wrapped_blocks = blocks.clone();

    let mut block_index: usize = 0;
    let mut skip_next: bool = false;
    while block_index < wrapped_blocks[0].len() {
        #[cfg(test)]
        {
            eprintln!("DBG idx={}, lens={:?}", block_index, wrapped_blocks.iter().map(|b| b.len()).collect::<Vec<_>>());
        }
        if skip_next {
            skip_next = false;
            block_index += 1;
            continue;
        }
        if wrapped_blocks[0][block_index].len() > maximum_lines {
            // The first part takes as many lines as it is allowed to.
            let target_first_len = maximum_lines;

            // Determine whether we should insert a new block placeholder after the current one
            let has_next = wrapped_blocks[0].get(block_index + 1).is_some();
            let insert_new_block = !has_next || persistence || (!persistence && has_next);
            if insert_new_block {
                wrapped_blocks
                    .iter_mut()
                    .for_each(|block| block.insert(block_index + 1, vec![]));
            }

            // Determine destination index for moved lines
            // - If persistence is true or there was no next, move lines into the newly created block at index+1
            // - If persistence is false and a next block exists, move lines into the original next block which is now at index+2
            // In non-persistent mode with an existing next block we still insert
            // a placeholder at index+1, merge the overflow into it and append
            // the original next block afterwards.
            let destination_index = block_index + 1;

            let mut moved_line_count = 0;
            // Move lines starting at target_first_len until the first part has exactly target_first_len lines
            while wrapped_blocks[0][block_index].len() > target_first_len {
                let primary_line = wrapped_blocks[0][block_index].remove(target_first_len);
                wrapped_blocks[0][destination_index].insert(moved_line_count, primary_line);

                // Move corresponding lines in other parallel blocks if present
                for block in wrapped_blocks.iter_mut().skip(1) {
                    if target_first_len < block[block_index].len() {
                        let primary_line = block[block_index].remove(target_first_len);
                        block[destination_index].insert(moved_line_count, primary_line);
                    }
                }
                moved_line_count += 1;
            }

            // In non-persistent mode with an existing next block, append its content to the new block
            if !persistence && has_next {
                // For each parallel block group, append the original next block to the new destination block
                for block in wrapped_blocks.iter_mut() {
                    if block.len() > block_index + 2 {
                        // Move content out of the original next without removing the block (keep as empty to preserve count)
                        let original_next_content = std::mem::take(&mut block[block_index + 2]);
                        // Append preserving order
                        block[destination_index].extend(original_next_content);
                    }
                }
                return wrapped_blocks;
            }
        }
        block_index += 1;
    }
    wrapped_blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_empty_slide() {
        let slide = Slide::new_empty_slide(false);
        assert!(matches!(slide.slide_content, SlideContent::Empty(_)));
    }

    #[test]
    fn create_pdf_page_slide() {
        let slide = Slide::new_pdf_page_slide("/path/to/document.pdf".to_string(), 3);
        assert!(matches!(slide.slide_content, SlideContent::PdfPage(_)));
        if let SlideContent::PdfPage(pdf_slide) = &slide.slide_content {
            assert_eq!(pdf_slide.pdf_path, "/path/to/document.pdf");
            assert_eq!(pdf_slide.page_number, 3);
        }
        assert!(!slide.has_spoiler());
        assert!(!slide.has_meta_text());
    }

    #[test]
    fn pdf_page_slide_is_serde_ready() {
        let slide = Slide::new_pdf_page_slide("/path/to/file.pdf".to_string(), 1);
        let json = serde_json::to_string(&slide).expect("Serialization failed");
        let deserialized: Slide = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(slide, deserialized);
    }

    #[test]
    fn check_has_spoiler_function() {
        let slide_1 = Slide::new_content_slide("Test".to_string(), Some("Hallo".to_string()), None);
        assert!(slide_1.has_spoiler());

        let slide_2 = Slide::new_content_slide(
            "Test".to_string(),
            Some("".to_string()),
            Some("".to_string()),
        );
        assert!(!slide_2.has_spoiler());
    }

    #[test]
    fn test_wrap_blocks_function() {
        let example_blocks = vec![
            vec![
                vec![
                    "A1".to_string(),
                    "A2".to_string(),
                    "A3".to_string(),
                    "A4".to_string(),
                    "A5".to_string(),
                ],
                vec![
                    "A6".to_string(),
                    "A7".to_string(),
                    "A8".to_string(),
                    "A9".to_string(),
                    "A10".to_string(),
                ],
            ],
            vec![
                vec![
                    "B1".to_string(),
                    "B2".to_string(),
                    "B3".to_string(),
                    "B4".to_string(),
                ],
                vec![
                    "B5".to_string(),
                    "B6".to_string(),
                    "B7".to_string(),
                    "B8".to_string(),
                    "B9".to_string(),
                ],
            ],
        ];

        let wrapped_blocks = wrap_blocks(&example_blocks, 3, true);
        dbg!(&wrapped_blocks);
    }
    
    #[test]
    fn test_wrap_blocks_with_odd_lines() {
        // Test with odd number of lines (5)
        let blocks_with_odd_lines = vec![
            vec![
                vec![
                    "L1".to_string(),
                    "L2".to_string(),
                    "L3".to_string(),
                    "L4".to_string(),
                    "L5".to_string(),
                ],
            ],
        ];
        
        // Maximum lines is set to 3, which is less than the 5 lines in our block, so it should trigger splitting
        let wrapped_blocks = wrap_blocks(&blocks_with_odd_lines, 3, true);
        
        // For 5 lines with maximum_lines=3, we prefer a 3 | 2 split (larger first part)
        assert_eq!(wrapped_blocks[0][0].len(), 3);
        assert_eq!(wrapped_blocks[0][1].len(), 2);
        
        // Verify the actual content
        assert_eq!(wrapped_blocks[0][0], vec!["L1".to_string(), "L2".to_string(), "L3".to_string()]);
        assert_eq!(wrapped_blocks[0][1], vec!["L4".to_string(), "L5".to_string()]);
    }
    
    #[test]
    fn test_wrap_blocks_empty() {
        // Test with empty blocks
        let empty_blocks: Vec<Vec<Vec<String>>> = vec![];
        let wrapped_empty = wrap_blocks(&empty_blocks, 3, true);
        
        // Empty blocks should remain empty
        assert_eq!(wrapped_empty.len(), 0);
        
        // Test with blocks containing empty vectors
        let blocks_with_empty = vec![vec![vec![]]];
        let wrapped_with_empty = wrap_blocks(&blocks_with_empty, 3, true);
        
        // Should not change as there are no lines to wrap
        assert_eq!(wrapped_with_empty, blocks_with_empty);
    }
    
    #[test]
    fn test_wrap_blocks_exact_maximum() {
        // Test with blocks having exactly maximum_lines
        let blocks_exact = vec![
            vec![
                vec![
                    "A1".to_string(),
                    "A2".to_string(),
                    "A3".to_string(),
                ],
            ],
        ];
        
        let wrapped_exact = wrap_blocks(&blocks_exact, 3, true);
        
        // Should not change as the number of lines equals maximum_lines
        assert_eq!(wrapped_exact, blocks_exact);
        assert_eq!(wrapped_exact[0][0].len(), 3);
        assert_eq!(wrapped_exact[0].len(), 1); // No new block should be created
    }
    
    #[test]
    fn test_wrap_blocks_persistence() {
        // Create test blocks that need wrapping
        let test_blocks = vec![
            vec![
                vec![
                    "A1".to_string(),
                    "A2".to_string(),
                    "A3".to_string(),
                    "A4".to_string(),
                ],
                vec![
                    "B1".to_string(),
                    "B2".to_string(),
                ],
            ],
        ];
        
        // Test with persistence = true
        let wrapped_persistent = wrap_blocks(&test_blocks, 2, true);
        
        // Should insert a new block after the first one
        assert_eq!(wrapped_persistent[0].len(), 3);
        assert_eq!(wrapped_persistent[0][0], vec!["A1".to_string(), "A2".to_string()]);
        assert_eq!(wrapped_persistent[0][1], vec!["A3".to_string(), "A4".to_string()]);
        assert_eq!(wrapped_persistent[0][2], vec!["B1".to_string(), "B2".to_string()]);
        
        // Test with persistence = false and a block after the one being wrapped
        let test_blocks_with_next = vec![
            vec![
                vec![
                    "A1".to_string(),
                    "A2".to_string(),
                    "A3".to_string(),
                    "A4".to_string(),
                ],
                vec![
                    "B1".to_string(),
                    "B2".to_string(),
                ],
            ],
        ];
        
        let wrapped_non_persistent = wrap_blocks(&test_blocks_with_next, 2, false);
        
        // Should modify the existing next block
        assert_eq!(wrapped_non_persistent[0].len(), 3);
        assert_eq!(wrapped_non_persistent[0][0], vec!["A1".to_string(), "A2".to_string()]);
        assert_eq!(wrapped_non_persistent[0][1], vec!["A3".to_string(), "A4".to_string(), "B1".to_string(), "B2".to_string()]);
    }
    
    #[test]
    fn test_wrap_blocks_multiple_blocks() {
        // Test with multiple blocks that need wrapping
        let multiple_blocks = vec![
            vec![
                vec![
                    "A1".to_string(),
                    "A2".to_string(),
                    "A3".to_string(),
                    "A4".to_string(),
                ],
            ],
            vec![
                vec![
                    "B1".to_string(),
                    "B2".to_string(),
                    "B3".to_string(),
                    "B4".to_string(),
                ],
            ],
        ];
        
        let wrapped_multiple = wrap_blocks(&multiple_blocks, 2, true);
        
        // Both blocks should be wrapped
        assert_eq!(wrapped_multiple.len(), 2);
        assert_eq!(wrapped_multiple[0].len(), 2);
        assert_eq!(wrapped_multiple[1].len(), 2);
        
        // Check first block's content
        assert_eq!(wrapped_multiple[0][0], vec!["A1".to_string(), "A2".to_string()]);
        assert_eq!(wrapped_multiple[0][1], vec!["A3".to_string(), "A4".to_string()]);
        
        // Check second block's content
        assert_eq!(wrapped_multiple[1][0], vec!["B1".to_string(), "B2".to_string()]);
        assert_eq!(wrapped_multiple[1][1], vec!["B3".to_string(), "B4".to_string()]);
    }
    
    #[test]
    fn test_wrap_blocks_edge_cases() {
        // Test with maximum_lines = 1 (extreme case)
        let blocks_for_extreme = vec![
            vec![
                vec![
                    "A1".to_string(),
                    "A2".to_string(),
                    "A3".to_string(),
                ],
            ],
        ];
        
        let wrapped_extreme = wrap_blocks(&blocks_for_extreme, 1, true);
        
        // Should create 3 blocks with 1 line each
        assert_eq!(wrapped_extreme[0].len(), 3);
        assert_eq!(wrapped_extreme[0][0], vec!["A1".to_string()]);
        assert_eq!(wrapped_extreme[0][1], vec!["A2".to_string()]);
        assert_eq!(wrapped_extreme[0][2], vec!["A3".to_string()]);
    }
}
