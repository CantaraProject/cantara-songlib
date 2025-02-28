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
At the moment, the following import formats are supported:
- The Cantara classic song format (lyrics only), see [`crate::importer::classic-song`] module.
- The cssf song format (lyrics and scores), see cssf_song module. (under construction)
- the CCLI song format (lyrics only), see ccli_song module. (under construction)
*/

use importer::classic_song::slides_from_classic_song;
use importer::errors::*;
use slides::{ShowMetaInformation, Slide, SlideSettings};
use std::error::Error;
use std::ffi::{c_char, c_int, CStr, CString};
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

/// Extern library call function for creating a presentation from a given input file
/// 
/// # Parameters
/// - `c_file_path`: The absolute path of the file as a `*const c_char`
/// - `c_title_slide`: A C boolean integer which determins whether to show a separate title slide (0 = false, 1 => true)
/// - `c_show_spoiler`: A C boolean integer which determins whether a designated spoiler is shown 
/// - `c_show_meta_information`: A C integer which determins whether meta informtion is shown (0 => None, 1 => Show on first slide, 2 => Show on last slide, 3 => Show on first slide and last slide)
/// - `c_meta_syntax`: A `*const c_char` which contains the syntax of the shown meta data (if none is desired, give an empty string)
/// - `c_empty_last_slides`: A C boolean integer which determins whether an empty last slide should be appended to every song (0 => false, 1 => true)
/// - `c_max_lines`: A c_int with the max number of lines after which the slide is wrapped. If 0 is given, no slide wrap will take place,
///
/// # Returns
/// The slides as a `*const c_char`.
#[no_mangle]
pub extern "C" fn create_presentation_from_file_c(
    c_file_path: *const c_char,
    c_title_slide: c_int,
    c_show_spoiler: c_int,
    c_show_meta_information: c_int,
    c_meta_syntax: *const c_char,
    c_empty_last_side: c_int,
    c_max_lines: c_int
) -> *const c_char {
    let file_path: PathBuf = PathBuf::from(c_string_to_rust(c_file_path).unwrap());
    let title_slide: bool = match c_title_slide as i32 {
        1 => true,
        _ => false
    };
    let show_spoiler: bool = match c_show_spoiler as i32 {
        1 => true,
        _ => false
    };
    let show_meta_information: ShowMetaInformation = match c_show_meta_information {
        1 => ShowMetaInformation::FirstSlide,
        2 => ShowMetaInformation::LastSlide,        
        3 => ShowMetaInformation::FirstSlideAndLastSlide,
        _ => ShowMetaInformation::None,        
    };
    
    let meta_syntax = c_string_to_rust(c_meta_syntax).unwrap();
    
    let empty_last_slide: bool = match c_empty_last_side {
        1 => true,
        _ => false
    };
    
    let max_lines: Option<usize> = match c_max_lines as usize {
        0 => None,
        _ => Some(c_max_lines as usize)
    };
    
    let slide_settings: SlideSettings = SlideSettings {
        title_slide,
        show_spoiler,
        show_meta_information,
        meta_syntax,
        empty_last_slide,
        max_lines
    };
    
    match create_presentation_from_file(file_path, slide_settings) {
        Ok(v) => rust_string_to_c_char(serde_json::to_string(&v).unwrap()).unwrap(),
        Err(err) => rust_string_to_c_char(err.to_string()).unwrap(),
    }
}

/// Create a presentation from a file and return the slides or an error if something went wrong
pub fn create_presentation_from_file(file_path: PathBuf, slide_settings: SlideSettings) -> Result<Vec<Slide>, Box<dyn Error>> {
    if !file_path.exists() {
        return Err(
            Box::new(CantaraFileDoesNotExistError)
        )
    }

    if file_path.extension() == Some(std::ffi::OsStr::new("song")) {

        let file_content = std::fs::read_to_string(&file_path).unwrap();
        let slides = slides_from_classic_song(
            &file_content,
            &slide_settings,
            file_path.file_stem().unwrap().to_str().unwrap().to_string()
        );

        return Ok(slides);
    }

    Err(
        Box::new(
            CantaraImportUnknownFileExtensionError {
                file_extension: "unknown".to_string()
            }
        )
    )
}

fn c_string_to_rust(c_str: *const c_char) -> Option<String> {
    if c_str.is_null() {
        return None; // Handle null pointer
    }

    // Unsafe block, da wir mit rohen Zeigern arbeiten
    unsafe {
        // Konvertiere *const char zu &CStr
        let cstr = CStr::from_ptr(c_str);
        
        // Konvertiere zu Rust-String (bei ungültigem UTF-8 gibt es Fehlerbehandlung)
        cstr.to_str()
            .map(|s| s.to_string()) // Erfolgreiche Konvertierung zu String
            .ok() // Fehlerbehandlung: None bei ungültigem UTF-8
    }
}

fn rust_string_to_c_char(rust_str: String) -> Option<*const c_char> {
    // Konvertiere Rust-String in CString
    match CString::new(rust_str) {
        Ok(c_string) => {
            // Extrahiere den *const c_char Zeiger
            let c_ptr = c_string.as_ptr();
            // Wichtig: c_string bleibt hier im Scope, damit der Zeiger gültig bleibt
            std::mem::forget(c_string); // Optional: Verhindert Drop, wenn der Zeiger länger leben soll
            Some(c_ptr)
        }
        Err(_) => {
            // Fehler: String enthält interne Nullbytes
            None
        }
    }
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
        
        assert!(
            create_presentation_from_file(file_path, slide_settings)
            .is_err()
        )
    }
}
