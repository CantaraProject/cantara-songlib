use std::fs::File;
use std::io::prelude::*;
use std::error::Error;
use regex::Regex;

use super::{CantaraImportNoContentError, Importer};
use crate::song::Song;

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
}

impl Importer for CantaraSongFileImporter {

    fn from_path(&mut self, filepath: &str) -> Result<&mut Self, Box<dyn Error>>{
        self.filepath = filepath.to_string();
        match File::open(&self.filepath) {
            Ok(mut file) => {
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                self.contents = contents;
            },
            Err(e) => {
                return Err(Box::new(e));
            }
        }
        Ok(self)
    }

    fn from_content(&mut self, contents: &str) -> &mut Self {
        self.contents = contents.to_string();
        self
    }

    fn import_song(&self) -> Result<Song, Box<dyn Error>> {
        if self.contents.is_empty() {
            return Err(Box::new(CantaraImportNoContentError::new()));
        } 
        // Get the title either from the content or the filename
        let title_regex = Regex::new(r"#title:\s*(.+?)$").unwrap();
        let title = title_regex.captures(&self.contents).unwrap().get(1)
            .unwrap_or_else(|| self.filepath.as_str())
            .as_str();
        let song: Song = Song::new(title);
        Ok(song)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_import_song() {
        let mut importer = CantaraSongFileImporter::new()
            .from_content("#title: Test Song");
        let song = import_song().unwrap();
        assert_eq!(song.title, "Test Song");
    }
}