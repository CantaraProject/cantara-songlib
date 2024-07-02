mod errors;
mod classic_song;
use std::error::Error;

use crate::song::Song;

pub trait Importer {
    fn from_path(self, path: &str) -> Self;
    fn from_content(self, content: String) -> Self;
    fn import_song(&self) -> Result<Song, Box<dyn Error>>;
}