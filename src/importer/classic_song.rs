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

use super::SongFile;

fn parse_metadata_block(block: &str) -> HashMap<String, String> {
    let mut metadata: HashMap<String, String> = HashMap::new();

    // With that we make sure that the regex is only compiled once.
    let tags_regex = { 
        static TAGS_REGEX: OnceLock<Regex> = OnceLock::new();
        TAGS_REGEX.get_or_init(|| {
            RegexBuilder::new(r"^\s*#(\w+):\s*(.+)$")
                .multi_line(true)
                .build()
                .unwrap()
        })
    };        

    tags_regex
        .captures_iter(block)
        .for_each(|capture: regex::Captures| {
            let tag: &str = capture.get(1).unwrap().as_str().trim();
            let value = capture.get(2).unwrap().as_str().trim().to_string();
            let tag_lowercase = tag.to_lowercase();
            
            metadata.insert(tag_lowercase, value);
        });

    metadata
}

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

    // Make sure that the regex is only compiled once.
    let title_regex: &Regex = {
        static TITLE_REGEX: OnceLock<Regex> = OnceLock::new();
        TITLE_REGEX.get_or_init(|| {
            RegexBuilder::new(r"\s*#title:\s*(.+?)$")
                .multi_line(true)
                .build()
                .unwrap()
        })
    };

    // Get the title either from the content or the filename
    let title: &str = match title_regex.captures(content) {
        Some(title_captures) => title_captures.get(1).unwrap().as_str(),
        None => "",
    };

    let mut song: Song = Song::new(title);

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

fn presentation_from_classic_song(
        content: &str, 
        presentation_settings: PresentationSettings) {
    
    // The emptyness of the line before (in the loop)
    let mut empty_line = false;
    let mut start_block_flag = true;
    let mut meta_block_flag = false;
    let mut blocks: Vec<Vec<String>> = vec![];
    let mut cur_block_string: String = "".to_string();
    let mut metadata: HashMap<String, String> = HashMap::new();

    for line in content.trim().lines() {
        if empty_line { start_block_flag = true };
        
        if start_block_flag {
            meta_block_flag = match line.chars().next().unwrap() {
                '#' => true,
                _   => false,
            };
            start_block_flag = false;
        }
        
        if line.trim().is_empty() {
            empty_line = true;
            
            // Skip anything below if the line is empty as well
            if cur_block_string.is_empty() {
                continue;
            }

            match meta_block_flag {
                true => { 
                    parse_metadata_block(&cur_block_string)
                    .iter()
                    .for_each(|(key, value)| {
                        metadata.insert(key.clone(), value.clone());
                    }); 

                },
                false => { 
                    if !cur_block_string.trim().is_empty() {
                        blocks.push(
                            cur_block_string.lines()
                            .map(|str| str.to_string()).collect()
                        );
                    }
                },
            }
            cur_block_string = "".to_string();
        }
        else {
            cur_block_string.push_str("\n");
            cur_block_string.push_str(line);
        }
    }

    // TODO: Implement word wrap feature
    

    // Create the Presentation
    let slides: Presentation = vec![];

    if presentation_settings.show_title_slide {
        // TODO: Implement the meta tag stuff...
        //slides.push(
        //    Slide::new_title_slide(title_text, meta_text)
        //);
    }

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
    fn test_metadata_parsing() {
        let metadata_block: &str = "#title: Test \n\
            #author: J.S. Bach";
        let metadata = parse_metadata_block(metadata_block);
        
        assert_eq!(metadata.len(), 2);
        assert_eq!(metadata.get("title").unwrap(), "Test");
        assert_eq!(metadata.get("author").unwrap(), "J.S. Bach");
    }

}
