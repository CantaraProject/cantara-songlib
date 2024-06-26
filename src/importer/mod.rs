#[path = "../song.rs"]
mod classic_song;

use std::error::Error;
use std::fmt;
use crate::song::Song;

pub trait Importer {
    fn from_path(&mut self, path: &str) -> Result<&mut Self, Box<dyn Error>>;
    fn from_content(&mut self, content: &str) -> &mut Self;
    fn import_song(&self) -> Result<Song, Box<dyn Error>>;
}

#[derive(Debug, Clone)]
pub struct CantaraImportNoContentError;

impl fmt::Display for CantaraImportNoContentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There is no content to import")
    }
}