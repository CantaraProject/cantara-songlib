/*!
This library contains functions to import, parse and export song files of different formats.
It is used in the Cantara project for song import and generation of song slides and music sheets.

# Overview

Churches and other groups who want to sing together as a group often need to export songs to different formats.
While the musicians need the songs in a music-sheet like format, the audience most often is interested in the lyrics only.
The Cantara project tries to unify these requirements by providing a simple text format for songs which can be used to generate different output formats.
The song format is a simple and easy to read text format which can be used to write songs in plain text files.
The crate handles the import of these song files and provides a Song struct which can be used to generate different output formats.
Due to legacy reasons, the crate also supports the import of songs from other formats.
At the moment, the following import formats are going to be supported:
- The Cantara classic song format (lyrics only), see [`crate::importer::classic-song`] module.
- The cssf song format (lyrics and scores) (under construction)
- the CCLI song format (lyrics only) (under construction)
*/

use importer::classic_song::slides_from_classic_song;
use importer::errors::*;
use slides::{Slide, SlideSettings};
use std::error::Error;
use std::ffi::{c_char, c_int};
use std::path::PathBuf;

/// - The `song` module contains the data structures needed for songs and its methods for managing and interpreting song data.
pub mod song;

/// - The `importer` module contains functions for importing songs from different formats.
pub mod importer;

/// The filetypes which are supported as input/output
pub mod filetypes;

/// The handling of song presentation slides
pub mod slides;

/// Templates which define the creation of slides and the insertion of data
pub mod templating;

#[no_mangle]
pub extern "C" fn create_presentation_from_file_c(
    file_path: *const c_char,
    title_slide: c_int,
    show_spoiler: c_int,
    show_meta_information: c_int,
    meta_syntax: *const c_char,
    empty_last_side: c_int,
    max_lines: c_int,
) {
    // TODO: Implement wrapper here
}

/// Create a presentation from a file and return the slides or an error if something went wrong
pub fn create_presentation_from_file(
    file_path: PathBuf,
    slide_settings: SlideSettings,
) -> Result<Vec<Slide>, Box<dyn Error>> {
    if !file_path.exists() {
        return Err(Box::new(CantaraFileDoesNotExistError));
    }

    if file_path.extension() == Some(std::ffi::OsStr::new("song")) {
        let file_content = std::fs::read_to_string(&file_path).unwrap();
        let slides = slides_from_classic_song(
            &file_content,
            &slide_settings,
            file_path.file_stem().unwrap().to_str().unwrap().to_string(),
        );

        return Ok(slides);
    }

    Err(Box::new(CantaraImportUnknownFileExtensionError {
        file_extension: "unknown".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{create_presentation_from_file, slides::SlideSettings};

    use super::song::Song;

    #[test]
    fn create_example_song() {
        let song: Song = Song::new("Test Song");
        assert_eq!(song.title, "Test Song");
        assert_eq!(song.get_total_part_count(), 0);
        assert_eq!(song.get_unpacked_parts().len(), 0)
    }

    #[test]
    fn test_file_does_not_exist_error() {
        let file_path: PathBuf = "Ich existiere nicht.song".into();
        let slide_settings: SlideSettings = SlideSettings::default();

        assert!(create_presentation_from_file(file_path, slide_settings).is_err())
    }
}
