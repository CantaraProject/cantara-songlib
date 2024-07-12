//! This module contains functions to import songs from the classic Cantara song format.
//! The Cantara song format is a simple text format that is used to write songs in plain text files.
//! You can find a documentation here: https://www.cantara.app/tutorial/where-to-get-the-songs/index.html#the-song-file-format

use std::error::Error;

extern crate regex;
use regex::Regex;

use crate::importer::errors::CantaraImportNoContentError;
use crate::song::{
    LyricLanguage, Song, SongPart, SongPartContent, SongPartContentType, SongPartType,
};

fn parse_block(block: &str, song: Song) -> Result<Song, Box<dyn Error>> {
    if block.is_empty() {
        return Ok(song);
    }

    let mut cloned_song: Song = song.clone();

    // If first letter is a #, then parse the tags
    if block.starts_with('#') {
        let tags_regex = Regex::new(r"#(\w+):\s*(.+)$").unwrap();

        let _ = tags_regex
            .captures_iter(block)
            .map(|capture: regex::Captures| {
                let tag: &str = capture.get(1).unwrap().as_str();
                let value: &str = capture.get(2).unwrap().as_str();
                cloned_song.add_tag(tag, value);
            });
        return Ok(cloned_song);
    }

    let song_part_reference = cloned_song.add_part_of_type(SongPartType::Verse, None);
    let mut song_part: std::cell::RefMut<SongPart> = song_part_reference.borrow_mut();

    {
        let lyric_language: LyricLanguage = LyricLanguage::Default;
        let lyrics_content: SongPartContent = SongPartContent {
            voice_type: SongPartContentType::Lyrics {
                language: lyric_language,
            },
            content: block.to_string(),
        };

        let _ = &mut song_part.add_content(lyrics_content);
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
    let title_regex = Regex::new(r"#title:\s*(.+?)$").unwrap();

    let title: &str = match title_regex.captures(content) {
        Some(title_captures) => title_captures.get(1).unwrap().as_str(),
        None => "",
    };

    let song: Song = Song::new(title);

    // Parse the blocks
    let parts_iterator: std::str::Split<&str> = content.split("\n\n");
    let parts: Vec<&str> = parts_iterator.collect();

    for (index, part) in parts.iter().enumerate() {
        // Determine if the same part has been repeated
        let mut part_index: usize = 0;
        let _ = parts[0..index].iter().map(|p| {
            if p == part {
                part_index += 1;
            }
        }); //Hier weitermachen
        if parts[0..index].contains(part) {
            part_index = parts[0..index].iter().filter(|&p| p == part).count();
        }
    }

    let song = parts
        .iter()
        .fold(song, |song, part| parse_block(part, song).unwrap());
    Ok(song)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_import_song() {
        let content: String = String::from("#title: Test Song");
        let song = import_song(&content).unwrap();
        assert_eq!(song.title, "Test Song");
    }
}
