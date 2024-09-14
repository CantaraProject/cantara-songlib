//! This module contains functions to import songs from the classic Cantara song format.
//! The Cantara song format is a simple text format that is used to write songs in plain text files.
//! You can find a documentation here: https://www.cantara.app/tutorial/where-to-get-the-songs/index.html#the-song-file-format

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

fn parse_block(block: &str, song: Song) -> Result<Song, Box<dyn Error>> {
    if block.is_empty() {
        return Ok(song);
    }

    let mut cloned_song: Song = song.clone();

    // If first letter is a #, then parse the tags
    if block.starts_with('#') {
        dbg!("Parsing tags");
        
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
                dbg!((tag_lowercase.clone(), value));
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
        dbg!("Added part", song_part);
    } else {
        let unwrapped_reference = part_reference.unwrap();
        let mut previous_song_part: std::cell::RefMut<SongPart> = unwrapped_reference.borrow_mut();
        let _ = &mut previous_song_part.set_type(SongPartType::Chorus);
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
                dbg!("Clearing part", &part);
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

}
