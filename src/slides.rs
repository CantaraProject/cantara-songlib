//! Here the logic for the slides is implemented

use serde::{Deserialize, Serialize};
use std::cmp::min;

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
                    match spoiler_text {
                        Some(string) => Some(string.trim().to_string()),
                        None => None,
                    },
                    match meta_text {
                        Some(string) => Some(string.trim().to_string()),
                        None => None,
                    },
                ),
            ),
            linked_file: None,
        }
    }

    pub fn new_title_slide(title_text: String, meta_text: Option<String>) -> Self {
        Slide {
            slide_content: SlideContent::Title(TitleSlide {
                title_text: title_text.trim().to_string(),
                meta_text: match meta_text {
                    Some(string) => Some(string.trim().to_string()),
                    None => None,
                },
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
}

impl Default for SlideSettings {
    fn default() -> Self {
        SlideSettings {
            title_slide: true,
            meta_syntax: "".to_string(),
            show_meta_information: ShowMetaInformation::FirstSlideAndLastSlide,
            empty_last_slide: true,
            show_spoiler: true,
            max_lines: None,
        }
    }
}

/// Enum for specifing the settings for the showing of meta information
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum ShowMetaInformation {
    /// Don't show any meta information in the presentation
    None,
    /// Show the meta information at the first slide of a song (apart from the title slide)
    FirstSlide,
    /// Show the meta information at the last slide of a song (apart from an empty slide)
    LastSlide,
    /// Show the meta information on both the first and the last slide of a song
    FirstSlideAndLastSlide,
}

impl ShowMetaInformation {
    pub fn on_first_slide(&self) -> bool {
        match self {
            ShowMetaInformation::FirstSlide | ShowMetaInformation::FirstSlideAndLastSlide => true,
            _ => false,
        }
    }

    pub fn on_last_slide(&self) -> bool {
        match self {
            ShowMetaInformation::LastSlide | ShowMetaInformation::FirstSlideAndLastSlide => true,
            _ => false,
        }
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
            // Determine the desired size of the first part: balance roughly in half, but do not exceed maximum_lines
            let total_lines = wrapped_blocks[0][block_index].len();
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
            let merging_into_existing_next = !persistence && has_next;
            // In non-persistent mode with an existing next, we'll still insert a placeholder at index+1
            // and merge overflow into this new block, then append the original next block to it.
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
