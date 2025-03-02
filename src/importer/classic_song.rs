//! This module contains functions to import songs from the classic Cantara song format.
//! The Cantara song format is a simple text format that is used to write songs in plain text files.
//! You can find a documentation here: <https://www.cantara.app/tutorial/where-to-get-the-songs/index.html#the-song-file-format>

use std::collections::HashMap;
use std::error::Error;
use std::{cell::RefCell, rc::Rc};
use std::sync::OnceLock;

extern crate regex;
use regex::{Regex,RegexBuilder};

use crate::importer::errors::CantaraImportNoContentError;
use crate::song::{
    LyricLanguage, 
    Song, 
    SongPart, 
    SongPartContent, 
    SongPartContentType, 
    SongPartType, 
};

use crate::slides::*;
use crate::templating::render_metadata;

use crate::importer::metadata::*;

fn parse_block(block: &str, song: Song) -> Result<Song, Box<dyn Error>> {
    if block.is_empty() {
        return Ok(song);
    }

    let mut cloned_song: Song = song.clone();

    // If first letter is a #, then parse the tags
    if block.starts_with('#') {
        
        // With that we make sure that the regex is only compiled once.
        let tags_regex = { 
            static TAGS_REGEX: OnceLock<Regex> = OnceLock::new();
            TAGS_REGEX.get_or_init(|| {
                RegexBuilder::new(r"\s*#(\w+):\s*(.+)$")
                    .multi_line(true)
                    .build()
                    .unwrap()
            })
        };        

        tags_regex
            .captures_iter(block)
            .for_each(|capture: regex::Captures| {
                let tag: &str = capture.get(1).unwrap().as_str();
                let value: &str = capture.get(2).unwrap().as_str();
                let tag_lowercase = tag.to_lowercase();
                cloned_song.add_tag(tag_lowercase.as_str(), value);
                if tag_lowercase == "title" {
                    cloned_song.title = value.to_string();
                }
            });
        return Ok(cloned_song);
    }

    // We will find first whether the content is already in the song, if yes, we have most likely a chorus.
    // If not, we will add a new verse.
    // If the content is already in the song, we will change the part type to chorus and add the content as a new chorus part.
    
    let content_vector = song.find_content_in_part(block);
    let (part_type, part_reference) = match content_vector.len() {
        0 => (SongPartType::Verse, None),
        _ => (SongPartType::Chorus, Some(content_vector.last().unwrap().clone())),
    };

    let lyric_language: LyricLanguage = LyricLanguage::Default;
    let lyrics_content: SongPartContent = SongPartContent {
        voice_type: SongPartContentType::Lyrics {
            language: lyric_language,
        },
        content: block.to_string(),
    };
    
    if part_reference.is_none() {    
        let song_part_reference: Rc<RefCell<SongPart>> = cloned_song.add_part_of_type(part_type, None);

        let mut song_part: std::cell::RefMut<SongPart> = song_part_reference.borrow_mut();
        let _ = &mut song_part.add_content(lyrics_content);
        song_part.set_repition(part_reference);
    } else {
        let unwrapped_reference = part_reference.unwrap();
        let mut previous_song_part: std::cell::RefMut<SongPart> = unwrapped_reference.borrow_mut();
        {
            let _ = &mut previous_song_part.set_type(SongPartType::Chorus);
        }
        previous_song_part.number = 1;
        let _ = &mut previous_song_part.update_id();
    }

    Ok(cloned_song)
}

/// Imports a song from a str which contains the song in the Cantara classic song format.
/// The function reads the content of the str and returns a result with a Song or an error.
/// The function guesses the part types (Refrain/Chorus, Verse, Bridge, etc.) based on the content and
/// keeps the song order which is provided.
pub fn import_song(content: &str) -> Result<Song, Box<dyn Error>> {
    if content.is_empty() {
        return Err(Box::new(CantaraImportNoContentError {}));
    }

    // Get the title either from the content or the filename
    let title: String = match get_title_from_file_content(content) {
        Some(title_string) => title_string,
        None => "".to_string()
    };

    let mut song: Song = Song::new(&title);

    let mut part: String = String::new();
    // Parse the blocks
    for line in content.trim().lines(){
        match line.trim() {
            "" => {
                if part.is_empty() {
                    continue;
                }
                song = parse_block(&part, song.clone()).unwrap();
                part.clear();
            }
            _ => {
                part.push_str(line.trim());
                part.push('\n');
            }
        }
    }
    if !(part.is_empty()) {
        song = parse_block(&part, song.clone()).unwrap();
        part.clear();
    }
    
    Ok(song)
}

/// Generates slides from a classic song content which is provided as &str
/// 
/// # Arguments
/// - `content`: The content of the classic song file given as a &str
/// - `presentation_settings`: A PresentationSettings struct which provides all settings for the creation of presentation slides
/// - `backup_title`: The title (String) which will be used if no #title - tag is specified in the content. This is most likely coming from the filename.
/// 
/// # Returns
/// A Vec<Slide> with the slides. This can be integrated into a PresentationChapter and a Presentation.
pub fn slides_from_classic_song(
    content: &str,
    slide_settings: &SlideSettings,
    backup_title: String) -> Vec<Slide> {
    
    /// Defines the current parsing state (which area is to be parsed)
    enum WritingArea {
        // The main block
        MainBlock,
        // The SecondarybBlock
        SecondaryBlock
    }
    
    // The emptyness of the line before (in the loop)
    let mut empty_line = false;
    // A new block has been started (in the iteration before)
    let mut start_block_flag = true;
    // The current block is a meta block
    let mut meta_block_flag = false;
    // All (main) blocks
    let mut blocks: Vec<Vec<String>> = vec![];
    // All secondary blocks. There will be always as many secondary blocks as there are primary blocks. 
    // Empty String equals None
    let mut secondary_blocks: Vec<Vec<String>> = vec![];
    
    // The current string of the block (used in the algorithm below)
    let mut cur_block_string: String = "".to_string();
    // The current string of the second block (used in the algorithm below)
    let mut cur_secundary_block_string: String = "".to_string();
    
    // The metadata of the song
    let mut metadata: HashMap<String, String> = HashMap::new();
    // Which block is currently written to (Main Block/Secondary Block)
    let mut writing_area: WritingArea = WritingArea::MainBlock;
    
    // A sub function for handling a block (putting it at the right position)
    // As this code is used twice in the code, it is outsourced into this function
    fn handle_block(metadata: &mut HashMap<String, String>, 
        meta_block_flag: &bool, 
        backup_title: &String,
        cur_block_string: &String, 
        cur_secundary_block_string: &String, 
        blocks: &mut Vec<Vec<String>>, 
        secondary_blocks: &mut Vec<Vec<String>>
        ) {
        match meta_block_flag {
                true => { 
                    parse_metadata_block(cur_block_string)
                    .iter()
                    .for_each(|(key, value)| {
                        metadata.insert(key.clone(), value.clone());
                    }); 
                    if metadata.get("title").is_none() {
                        metadata.insert("title".to_string(), backup_title.clone());
                    }
                },
                false => { 
                    if !cur_block_string.trim().is_empty() {
                        blocks.push(
                            cur_block_string.lines()
                            .map(|str| str.to_string()).collect()
                        );
                        secondary_blocks.push(
                            cur_secundary_block_string.lines()
                            .map(|str| str.to_string()).collect()
                        );
                    }
                },
            }
    }
                
    for line in content.trim().lines() {
        if empty_line { start_block_flag = true };
        
        if start_block_flag && !line.is_empty() {
            meta_block_flag = match line.chars().next().unwrap() {
                '#' => true,
                _   => false,
            };
            start_block_flag = false;
        }
        
        if line.trim().is_empty() {
            empty_line = true;
            writing_area = WritingArea::MainBlock;
            
            // Skip anything below if the line is empty as well
            if cur_block_string.is_empty() {
                continue;
            }

            handle_block(&mut metadata, 
                &meta_block_flag, 
                &backup_title, 
                &cur_block_string, 
                &cur_secundary_block_string, 
                &mut blocks, 
                &mut secondary_blocks
            );
            
            cur_block_string = "".to_string();
            cur_secundary_block_string = "".to_string();
            
        }
        // The --- delimiter starts a secondary block in a stanza
        else if line.trim() == "---" {
            writing_area = WritingArea::SecondaryBlock;
        }
        else {
            match writing_area {
                WritingArea::MainBlock => {
                    cur_block_string.push('\n');
                    cur_block_string.push_str(line);
                },
                WritingArea::SecondaryBlock => {
                    cur_secundary_block_string.push('\n');
                    cur_secundary_block_string.push_str(line);
                }
            }
            
        }
    }
    handle_block(&mut metadata, 
        &meta_block_flag, 
        &backup_title, 
        &cur_block_string, 
        &cur_secundary_block_string, 
        &mut blocks, 
        &mut secondary_blocks
    );

    if slide_settings.max_lines.is_some() {
        let wrapped_blocks_output: Vec<Vec<Vec<String>>> = wrap_blocks(&vec![blocks, secondary_blocks], slide_settings.max_lines.unwrap(), true);
        blocks = wrapped_blocks_output.first().unwrap().clone();
        secondary_blocks = wrapped_blocks_output.get(1).unwrap().clone();
    }

    // Create the Presentation
    
    let mut slides: Vec<Slide> = vec![];

    let meta_text_rendering_result = render_metadata(
        &slide_settings.meta_syntax,
        &metadata
    );
    let mut meta_text: String = "".to_string();

    let meta_text_showable: bool = match meta_text_rendering_result {
        Ok(str) => {
            meta_text = str.clone();
            !str.is_empty()
        },
        Err(_) => false,
    };

    // Make sure that the meta tag title is available (bugfix...)
    if metadata.get("title").is_none() {
        metadata.insert("title".to_string(), backup_title.clone());
    }

    if slide_settings.title_slide {
        let displayed_meta_text = match meta_text_showable {
            true => Some(meta_text.clone()),
            false => None,
        };
        
        slides.push(
            Slide::new_title_slide(
                metadata.get("title").unwrap().into(),                                          
                displayed_meta_text
            )
        )
    }
    
    let count = blocks.len();
    for (index, block) in blocks.iter().enumerate() {
        let displayed_meta_text = match meta_text_showable && (slide_settings.show_meta_information.on_first_slide() && index == 1) || (slide_settings.show_meta_information.on_last_slide() && index == count -1) {
            true => Some(meta_text.clone()),
            false => None,
        };
        
        let secondary_block = secondary_blocks.get(index).unwrap();
        if secondary_block.is_empty() {
            match blocks.get(index+1) {
                Some(next_block) => {
                    slides.push(
                        Slide::new_content_slide(block.join("\n"), Some(next_block.join("\n")), displayed_meta_text)
                    )       
                },
                None => {
                    slides.push(
                        Slide::new_content_slide(
                            block.join("\n"), None, 
                            displayed_meta_text
                        )
                    )
                }
            }
        } else {
            slides.push(
                Slide::new_content_slide(block.join("\n"),
                    Some(secondary_block.join("\n")), 
                    displayed_meta_text
                )
            );
        }
    }
    
    if slide_settings.empty_last_slide {
        slides.push(
            Slide::new_empty_slide(false)    
        );
    }
    
    slides

}

#[cfg(test)]
mod test {
    use crate::importer::import_song_from_file;

    use super::*;

    #[test]
    fn test_import_song() {
        let content: String = String::from("#title: Test Song");
        let song = import_song(&content).unwrap();
        assert_eq!(song.title, "Test Song");
    }

    #[test]
    fn test_import_song_with_tags() {
        let content: String = String::from(
            "#title: Test Song
            #author: Test Author
            #key: C"
        );
        let song = import_song(&content).unwrap();
        assert_eq!(song.title, "Test Song");
        assert_eq!(song.get_tag("author").unwrap(), "Test Author");
        assert_eq!(song.get_tag("key").unwrap(), "C");
    }

    #[test]
    fn test_import_song_with_verse() {
        let content: String = 
            "#title: Test Song
            
            This is a verse
            
            And a refrain
            
            The second verse
            
            And a refrain"
            .to_string();
        let song = import_song(&content).unwrap();
        assert_eq!(song.get_part_count(SongPartType::Verse), 2);
    }

    #[test]
    fn test_file_amazing_grace() {
        let song: Song = import_song_from_file("testfiles/Amazing Grace.song").unwrap();
        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.get_tag("author").unwrap(), "John Newton");
        assert_eq!(song.get_part_count(SongPartType::Verse), 3)
    }

    #[test]
    fn test_song_with_refrain() {
        let song: Song = import_song_from_file("testfiles/O What A Savior That He Died For Me.song").unwrap();
        assert_eq!(song.title, "O What A Savior That He Died For Me");
        assert_eq!(song.get_part_count(SongPartType::Verse), 4);
        assert_eq!(song.get_part_count(SongPartType::Chorus), 1);
        dbg!(song);
    }
    
    #[test]
    fn generate_slides() {
        let testfile = std::fs::read_to_string("testfiles/O What A Savior That He Died For Me.song").unwrap();
        
        let presentation_settings = SlideSettings { 
            title_slide: true,
            meta_syntax: "{{title}} ({{author}})".to_string(), 
            show_meta_information: ShowMetaInformation::FirstSlideAndLastSlide, 
            empty_last_slide: true, 
            show_spoiler: true ,
            max_lines: Some(10),
        };
        
        let slides: Vec<Slide> = slides_from_classic_song(
            &testfile, 
            &presentation_settings,
            "Verily, Verily".to_string()
        );
        
        assert!(!slides.is_empty());
        
        dbg!(slides);
    }

    #[test]
    fn test_metadata_displayed_correctly() {
        let testfile = std::fs::read_to_string("testfiles/O What A Savior That He Died For Me.song").unwrap();
        
        let mut presentation_settings   = SlideSettings { 
            title_slide: false,
            meta_syntax: "{{title}} ({{author}})".to_string(), 
            show_meta_information: ShowMetaInformation::None,
            empty_last_slide: true, 
            show_spoiler: true,
            max_lines: None,
        };

        let slides: Vec<Slide> = slides_from_classic_song(
            &testfile, 
            &presentation_settings,
            "Verily, Verily".to_string()
        );

        slides.iter().for_each(|slide| assert!(!slide.has_meta_text()));

        presentation_settings.show_meta_information = ShowMetaInformation::FirstSlide;
        
        let slides: Vec<Slide> = slides_from_classic_song(
            &testfile, 
            &presentation_settings,
            "Verily, Verily".to_string()
        );

        assert!(slides.get(1).unwrap().has_meta_text());
    }

}
