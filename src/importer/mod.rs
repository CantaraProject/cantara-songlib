//! The importer module contains functions for importing songs from different formats.
//! Specific submodules are used for different file formats.

/// This module contains defined errors which may occur during the import process.
mod errors;

/// This module contains functions for importing classic song files.
mod classic_song;

use crate::song::Song;
use std::error::Error;
use std::ffi::OsStr;
use std::path::Path;

/// Imports a song from a file.
/// The function reads the content of the file and determines the file format by its extension.
/// Depending on the file extension, the function calls the appropriate import function.
/// The function returns a Song object.
/// If the file extension is unknown, the function returns an error.
/// # Arguments
/// * `file_path` - A string slice that holds the path to the file.
/// # Returns
/// A Result object that holds either a Song object or an error.
/// The error is of type Box<dyn Error>.
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
}
