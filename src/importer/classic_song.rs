use std::fs::File;
use std::io::prelude::*;
use std::error::Error;
use regex::Regex;

use crate::importer::errors::CantaraImportNoContentError;
use crate::song::{LyricLanguage, Song, SongPart, SongPartContent, SongPartContentType, SongPartType};

use super::Importer;

pub struct CantaraSongFileImporter {
    filepath: String,
    contents: String,
}

impl CantaraSongFileImporter {
    pub fn new() -> Self {
        CantaraSongFileImporter { 
            filepath: String::new(),
            contents: String::new(),
        }
    }

    fn parse_block(&self, block: &str, song: Song) -> Result<Song, Box<dyn Error>> {        
        if block.is_empty() {
            return Ok(song);
        }

        let mut cloned_song: Song = song.clone();

        // If first letter is a #, then parse the tags
        if block.chars().next().unwrap() == '#' {
            let tags_regex = Regex::new(r"#(\w+):\s*(.+)$").unwrap();

            let _ = tags_regex.captures_iter(block).map(|capture: regex::Captures| {
                let tag: &str = capture.get(1).unwrap().as_str();
                let value: &str = capture.get(2).unwrap().as_str();
                cloned_song.add_tag(tag, value);
            });
            return Ok(cloned_song);
        }
        let song_part: &mut SongPart = cloned_song.add_part_of_type(SongPartType::Verse, None);

        {
            let lyric_language: LyricLanguage = LyricLanguage::Default;
            let lyrics_content: SongPartContent = SongPartContent {
                voice_type: SongPartContentType::Lyrics { language: lyric_language },
                content: block.to_string(),
            };

            let _ = &mut song_part.add_content(lyrics_content);
        }
        
        Ok(cloned_song)
    }

    fn load_content(&mut self) -> Result<(), Box<dyn Error>> {
        let mut file = File::open(&self.filepath)?;
        let mut file_content = String::new();
        file.read_to_string(&mut file_content)?;
        self.contents = file_content;
        Ok(())
    }
}

impl Importer for CantaraSongFileImporter {

    fn from_path(self, filepath: &str) -> Self {
        
        let filepath: String = filepath.to_string();
        
        CantaraSongFileImporter {
            filepath: filepath,
            ..self
        }      
    }

    fn from_content(self, contents: String) -> Self {
        CantaraSongFileImporter {
            contents: contents.trim().to_string(),
            ..self
        }
    }

    fn import_song(&self) -> Result<Song, Box<dyn Error>> {
        if self.contents.is_empty() {
            return Err(
                Box::new(
                    CantaraImportNoContentError { }
                )
            );
        } 
        // Get the title either from the content or the filename
        let title_regex = Regex::new(r"#title:\s*(.+?)$").unwrap();

        let title: &str = match title_regex.captures(&self.contents).unwrap().get(1) {
            Some(title) => {
                let title: &str = title.as_str();
                title
            },
            None => {
                let title: &str = self.filepath.split("/").last().unwrap();
                let title: &str = title.split(".").next().unwrap();
                title
            }
        };

        let song: Song = Song::new(title);
        
        // Parse the blocks
        let parts_iterator: std::str::Split<&str> = self.contents.split("\n\n");
        let parts: Vec<&str> = parts_iterator.collect();
        let song = parts.iter().fold(song, |song, part| {
            self.parse_block(part, song).unwrap()
        });
        Ok(song)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_import_song() {
        let binding = CantaraSongFileImporter::new();
        let importer = binding
            .from_content(String::from("#title: Test Song"));
        let song = importer.import_song().unwrap();
        assert_eq!(song.title, "Test Song");
    }
}