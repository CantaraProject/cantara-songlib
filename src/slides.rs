//! Here the logic for the slides is implemented

use std::cmp::{max, min};
use std::slice::range;
use serde::{Serialize, Deserialize};

use crate::importer::SongFile;


/// A Presentation which can be displayed
pub type Presentation = Vec<Slide>;

/// The enum which contains all possible contents of a slide
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum SlideContent {
    SingleLanguageMainContent(SingleLanguageMainContentSlide),
    Title(TitleSlide),
    MultiLanguageMainContent(MultiLanguageMainContentSlide),
    SimplePicture(SimplePictureSlide),
    Empty(EmptySlide),
}


/// This represents a slide which is presented
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Slide {
    pub slide_content: SlideContent,
    pub linked_file: Option<SongFile>
}

impl Slide {
    pub fn new_empty_slide(black_background: bool) -> Self {
        Slide {
            slide_content: SlideContent::Empty(
                EmptySlide {
                    black_background,
                }
            ),
            linked_file: None,
        }
    }

    pub fn new_content_slide(main_text: String, spoiler_text: Option<String>, meta_text: Option<String>) -> Self {
        Slide {
            slide_content: SlideContent::SingleLanguageMainContent(
                SingleLanguageMainContentSlide::new(
                    main_text,
                    spoiler_text,
                    meta_text
                )                   
            ),
            linked_file: None,
        }
    }

    pub fn new_title_slide(title_text: String, meta_text: Option<String>) -> Self {
        Slide {
            slide_content: SlideContent::Title(
                TitleSlide {
                    title_text,
                    meta_text
                }
            ),
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
            SlideContent::SingleLanguageMainContent(single_language_main_content_slide) => single_language_main_content_slide.spoiler_text.is_some(),
            SlideContent::Title(_) => false,
            SlideContent::MultiLanguageMainContent(multi_language_main_content_slide) => multi_language_main_content_slide.spoiler_text_vector.len() > 0,
            SlideContent::SimplePicture(_) => false,
            SlideContent::Empty(_) => false,
        }
    }
}

/// A slide which consists of at least a Main Text, an optional Spoiler Text with the content of the next slide and an optional Meta Text with additional information.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SingleLanguageMainContentSlide {
    /// The mandatory main text which is to display
    main_text: String,
    spoiler_text: Option<String>,
    meta_text: Option<String>,
}

impl SingleLanguageMainContentSlide {
    fn new(main_text: String, spoiler_text: Option<String>, meta_text: Option<String>) -> Self {
        // We don't allow empty strings in spoiler_text or meta_text
        let parsed_spoiler_text: Option<String> = match spoiler_text {
            Some(str) => match str.trim() {
                "" => None,
                _ => Some(str)
            },
            None => None,
        };
        let parsed_meta_text: Option<String> = match meta_text {
            Some(str) => match str.trim() {
                "" => None,
                _ => Some(str)
            },
            None => None,
        };

        SingleLanguageMainContentSlide {
            main_text,
            spoiler_text: parsed_spoiler_text,
            meta_text: parsed_meta_text
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
    pub meta_text: Option<String>
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


/// Struct for specifying the settings when creating presentation slides
pub struct SlideSettings {
    title_slide: bool,
    show_spoiler: bool,
    show_meta_information: ShowMetaInformation,
    meta_information_syntax: String,
    empty_slide_at_the_ending: bool,
}


/// Enum for specifing the settings for the showing of meta information
pub enum ShowMetaInformation {
    /// Don't show any meta information in the presentation
    None,
    /// Show the meta information at the first slide of a song (apart from the title slide)
    FirstSlide,
    /// Show the meta information at the last slide of a song (apart from an empty slide)
    LastSlide,
    /// Show the meta information on both the first and the last slide of a song
    FirstSlideAndLastSlide
}

/// A generic enum which can be used to define Presentation Settings for the **generation**
/// This concerns the content/structure of the presentation, not(!) the design
pub struct PresentationSettings {
    pub show_title_slide: bool,
    pub meta_syntax: String,
    pub meta_syntax_on_first_slide: bool,
    pub meta_syntax_on_last_slide: bool,
    pub empty_last_slide: bool,
    pub spoiler: bool
}

impl PresentationSettings {
    pub fn default() -> Self {
        PresentationSettings { 
            show_title_slide: true, 
            meta_syntax: "".to_string(),
            meta_syntax_on_first_slide: true, 
            meta_syntax_on_last_slide: true, 
            empty_last_slide: true, 
            spoiler: true 
        }
    }
}

/// This function wraps the blocks, so that the number of lines never exeeds maximum_lines.
/// The second block is optional and will be wrapped accordingly to the first one.
/// **Warning: This function will panic, if the length of a given `secondary_block` is not equal to the length of the primary block**
///
/// # Arguments
/// - `primary_blocks`: A &mut <Vec<Vec<String>> with the primary block which should be wrapped
/// - `secondary_blocks': An Option<&mut Vec<Vec<String>>> with the secondary block which should be wrapped.
/// Panics if secondary_block is Some(s) but s.len() != primary_block.len()
/// - `maximum_lines`: The number of maximum lines which should be in a block.
/// # Returns
/// Nothing, the changes will be written to `primary_block` and `secondary_block`.
pub fn wrap_blocks(primary_blocks: &mut Vec<Vec<String>>, secondary_blocks_option: Option<&mut Vec<Vec<String>>>, maximum_lines: usize) {
    match secondary_blocks_option {
        Some(secondary_blocks) => {
            if secondary_blocks.len() != primary_blocks.len() {
                panic!("Block length mismatch. Please read the documentation of wrap_blocks!");
            }

            let mut block_index: usize = 0;
            while block_index < primary_blocks.len() {
                if primary_blocks[block_index].len() > maximum_lines {
                    let splitter = min(maximum_lines, primary_blocks[block_index].len()/2);
                    let mut line_index = splitter;
                    while line_index < primary_blocks.len() {
                        let primary_line = primary_blocks[block_index].remove(line_index);
                        if primary_blocks.get(block_index +1).is_none() {
                            primary_blocks.push(vec![]);
                            secondary_blocks.push(vec![]);
                        }
                        primary_blocks[block_index +1].insert(line_index-splitter, primary_line);

                        // Here the secondary block will be moved if available
                        if secondary_blocks[block_index].get(line_index).is_some() {
                            let secondary_line = secondary_blocks[block_index].remove(line_index);
                            secondary_blocks[block_index+1].insert(line_index-splitter, secondary_line);
                        }
                    }
                }
                block_index += 1;
            }
        }
        None => {
            todo!("Implement")
        }
    }
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
    fn check_has_spoiler_function() {
        let slide_1 = Slide::new_content_slide("Test".to_string(), Some("Hallo".to_string()), None);
        assert_eq!(slide_1.has_spoiler(), true);

        let slide_2 = Slide::new_content_slide("Test".to_string(), Some("".to_string()), Some("".to_string()));
        assert_eq!(slide_2.has_spoiler(), false);
    }
}
