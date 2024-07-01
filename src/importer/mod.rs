mod errors;
mod classic_song;
use std::error::Error;

use crate::song::Song;

pub trait Importer {
    fn from_path(&mut self, path: &str) -> Result<&mut Self, Box<dyn Error>>;
    fn from_content(&mut self, content: String) -> &mut Self;
    fn import_song(&self) -> Result<Song, Box<dyn Error>>;
}