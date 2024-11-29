//! The importer module contains functions for importing songs from different formats.
//! Specific submodules are used for different file formats.

/// This module contains defined errors which may occur during the import process.
mod errors;

/// This module contains functions for importing classic song files.
pub mod classic_song;

use errors::CantaraFileDoesNotExistError;
use serde::{Deserialize, Serialize};

use crate::song::Song;
use std::error::Error;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};


#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
/// This struct represents a song file of any type located sommewhere on the file system. It can then later be parsed and hold a parsed Song type or used to create a presentation directly.
pub struct SongFile {
    /// The parsed file_path of the song
    pub file_path: PathBuf,

    /// The parsing state
    pub parsing_state: SongFileParsingState,
}

impl SongFile {
    pub fn new(path: &str) -> Result<Self, CantaraFileDoesNotExistError> {
        match Path::new(&path).exists() {
            true => {
                let file_path = Path::new(path).to_path_buf();
                let parsing_state = SongFileParsingState::NotStarted;

                Ok(
                    SongFile {
                        file_path,
                        parsing_state
                    }
                )
            },
            false => {
                Err(
                    CantaraFileDoesNotExistError
                )
            }            
        }
  
    }
}


/// Represents the parsing state of a song file
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum SongFileParsingState {
    /// The file is known, but 
    NotStarted,
    FormatDetermined,
    ClassicSongNotParsed,
    ParsedCantaraSong(Song),
}

/// Imports a song from a file.
/// The function reads the content of the file and determines the file format by its extension.
/// Depending on the file extension, the function calls the appropriate import function.
/// The function returns a Song object.
/// If the file extension is unknown, the function returns an error.
/// # Arguments
/// * `file_path` - A string slice that holds the path to the file.
/// # Returns
/// A Result object that holds either a Song object or an error.
/// The error is of type `Box<dyn Error>`.
/// The error can be of type CantaraImportUnknownFileExtensionError.
/// # Example
/// ```
/// use cantara_songlib::importer::import_song_from_file;
/// let song = import_song_from_file("testfiles/Amazing Grace.song").unwrap();
/// assert_eq!(song.title, "Amazing Grace");
/// ```
pub fn import_song_from_file(file_path: &str) -> Result<Song, Box<dyn Error>> {
    let content_wraped = std::fs::read_to_string(file_path);
    if content_wraped.is_err() {
        return Err(Box::new(content_wraped.err().unwrap()));
    }
    let content: String = content_wraped.unwrap();

    let file_extension: &str = Path::new(file_path)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap();
    match file_extension {
        "song" => {
            let wraped_song: Result<Song, Box<dyn Error>> = classic_song::import_song(&content);
            if wraped_song.is_err() {
                return Result::Err(wraped_song.err().unwrap());
            }
            let mut song: Song = wraped_song.unwrap();
            if song.title.is_empty() {
                let title: &str = Path::new(file_path)
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap();
                song.title = title.to_string();
            }
            Ok(song)
        }
        _ => Err(Box::new(errors::CantaraImportUnknownFileExtensionError {
            file_extension: file_extension.to_string(),
        })),
    }
}


/// Loads a song from a filename and returns it as JSON object or gives back an error if there has been any error during the process
/// # Parameters
/// - `file_path`: a `&str` with the filepath of the file which is to load
/// # Returns
/// - a Result with the song if everything went well, or an error if an error occured.
pub fn get_song_from_file_as_json(file_path: &str) -> Result<String, Box<dyn Error>> {
    match import_song_from_file(file_path) {
        Ok(song) => {
            match serde_json::to_string_pretty(&song) {
                Ok(string) => Ok(string),
                Err(error) => Err(Box::new(error))
            }            
        }
        Err(error) => Err(error)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    /// This test tests a song import from a file with a title tag.
    fn test_import_song_with_title_tag_from_file() {
        let song = import_song_from_file("testfiles/Amazing Grace.song").unwrap();
        assert_eq!(song.title, "Amazing Grace");
    }

    #[test]
    /// This test tests a song import from a file without a title tag.
    /// The title is derived from the filename.
    fn test_import_song_without_title_tag_from_file() {
        let song = import_song_from_file("testfiles/What a friend we have in Jesus.song").unwrap();
        assert_eq!(song.title, "What a friend we have in Jesus");
    }

    #[test]
    /// This test tests a song import from a file with an unknown file extension.
    /// The function should return an error.
    /// The error should be of type CantaraImportUnknownFileExtensionError.
    fn test_import_song_with_unknown_file_extension_from_file() {
        let result = import_song_from_file("testfiles/What a friend we have in Jesus.txt");
        assert!(result.is_err());
        let error: Box<dyn Error> = result.err().unwrap();
        assert_eq!(error.to_string(), "Unknown file extension: txt");
    }

    #[test]
    fn test_create_songfile_which_does_not_exist() {
        let result = SongFile::new("testfiles/A Non Existing File.txt");
        assert_eq!(result.unwrap_err(), CantaraFileDoesNotExistError);
    }
}
