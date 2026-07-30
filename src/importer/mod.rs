//! The importer module contains functions for importing songs from different formats.
//! Specific submodules are used for different file formats.

/// This module contains defined errors which may occur during the import process.
pub mod errors;

pub mod cssf;

pub mod classic_song;

pub mod song_yml;

pub mod ccli;

pub mod metadata;

use errors::CantaraFileDoesNotExistError;
use serde::{Deserialize, Serialize};

use crate::filetypes::FileType;
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
/// Read a song file, choosing the importer by the file's name.
///
/// This is the one place that maps a file format onto an importer; the command
/// line wrapper and the C interface both go through it, so a new format only
/// has to be registered here.
///
/// | Extension | Format |
/// |-----------|--------|
/// | `.song` | the classic Cantara text format |
/// | `.song.yml`, `.song.yaml`, `.yml`, `.yaml` | the YAML song format |
/// | `.ccli` | a CCLI SongSelect export |
/// | `.cssf` | Cantara Structured Song Format (under construction) |
///
/// A song whose file contains no title of its own is titled after the file.
///
/// # Errors
/// [`errors::CantaraImportUnknownFileExtensionError`] if the extension is not
/// one of the above, or the underlying I/O or parsing error.
pub fn import_song_from_file(file_path: impl AsRef<Path>) -> Result<Song, Box<dyn Error>> {
    let path = file_path.as_ref();

    let file_type = FileType::from_path(path).ok_or_else(|| {
        Box::new(errors::CantaraImportUnknownFileExtensionError {
            file_extension: path
                .extension()
                .and_then(OsStr::to_str)
                .unwrap_or("")
                .to_string(),
        })
    })?;

    let content = std::fs::read_to_string(path)?;

    let mut song = match file_type {
        FileType::SongYaml => song_yml::import_from_yml_string(&content)?,
        FileType::ClassicSongFile => classic_song::import_song(&content)?,
        FileType::CCLISongselectFile => ccli::import_from_ccli_string(&content)?,
        FileType::CSSF => cssf::import_input_string(
            content,
            path.file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("")
                .to_string(),
        )?,
    };

    // Formats that carry no title fall back to the file name.
    if song.title.trim().is_empty() {
        if let Some(stem) = path.file_stem().and_then(OsStr::to_str) {
            song.title = stem.to_string();
        }
    }

    Ok(song)
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
        let song = import_song_from_file("tests/data/Amazing Grace.song").unwrap();
        assert_eq!(song.title, "Amazing Grace");
    }

    #[test]
    /// This test tests a song import from a file without a title tag.
    /// The title is derived from the filename.
    fn test_import_song_without_title_tag_from_file() {
        let song = import_song_from_file("tests/data/What a friend we have in Jesus.song").unwrap();
        assert_eq!(song.title, "What a friend we have in Jesus");
    }

    #[test]
    /// This test tests a song import from a file with an unknown file extension.
    /// The function should return an error.
    /// The error should be of type CantaraImportUnknownFileExtensionError.
    fn test_import_song_with_unknown_file_extension_from_file() {
        let result = import_song_from_file("tests/data/What a friend we have in Jesus.txt");
        assert!(result.is_err());
        let error: Box<dyn Error> = result.err().unwrap();
        assert_eq!(error.to_string(), "Unknown file extension: txt");
    }

    #[test]
    fn test_create_songfile_which_does_not_exist() {
        let result = SongFile::new("tests/data/A Non Existing File.txt");
        assert_eq!(result.unwrap_err(), CantaraFileDoesNotExistError);
    }
}
