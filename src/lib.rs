/*!
This library contains functions to import, parse and export song files of different formats.
It is used in the Cantara project for song import and generation of song slides and music sheets.

# Overview

Churches and other groups who want to sing together as a group often need to export songs to different formats.
While the musicians need the songs in a music-sheet like format, the audience most often is interested in the lyrics only.
The Cantara project tries to unify these requirements by providing a simple text format for songs which can be used to generate different output formats.
The song format is a simple and easy to read text format which can be used to write songs in plain text files.
The crate handles the import of these song files and provides a [`song::Song`] struct which can be used to generate different output formats.

# The pipeline

Everything goes through one type. An importer reads a file into a [`song::Song`],
and an exporter turns that song into an output format:

```text
.song     ─┐                          ┌─► slides   (JSON for the presentation)
.song.yml  ├─► importer ─► Song ─► exporter ─► LilyPond (.ly, SVG, PDF)
.ccli      │                          ├─► ABC      (.abc)
.cssf     ─┘                          └─► text     (plain, Markdown, Telegram, …)
```

Because the model sits in the middle, every input format gains every output
format. See the [`song`] module for how a song is represented — parts, singing
orders, several voices, multiple languages and metadata.

# Import formats

- The Cantara classic song format (lyrics only), see [`importer::classic_song`].
- The YAML song format (lyrics and scores), see [`importer::song_yml`].
- CCLI SongSelect exports (lyrics only), see [`importer::ccli`].
- The cssf song format (lyrics and scores), see [`importer::cssf`]. (under construction)

# Export formats

- Presentation slides, see [`exporter::slides`].
- LilyPond sheet music, see [`exporter::lilypond`].
- ABC notation, see [`exporter::abc`].
- Plain text and templated markup, see [`exporter::text`].

# Example

```
use cantara_songlib::importer::song_yml;
use cantara_songlib::exporter::lilypond::{lilypond_from_song, LilypondSettings};

let yml = r#"
version: 0.1
title: Example
score:
  key: c major
  time: 4/4
parts:
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: c4 d e f
    - type: lyrics
      number: 1
      content: one two three four
"#;

let song = song_yml::import_from_yml_string(yml).unwrap();
assert_eq!(song.title, "Example");

let sheet = lilypond_from_song(&song, &LilypondSettings::default()).unwrap();
assert!(sheet.contains("\\score"));
```
*/

use importer::classic_song::slides_from_classic_song;
use importer::errors::*;
use slides::{LanguageConfiguration, ShowMetaInformation, Slide, SlideSettings};
use std::error::Error;
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::{Path, PathBuf};


pub mod song;

/// Compiles the Rust examples in `docs/` as doc tests so that the prose
/// documentation cannot drift away from the API without the build noticing.
///
/// This type exists only while running doc tests and is not part of the API.
#[cfg(doctest)]
#[doc = include_str!("../docs/data-model.md")]
#[doc = include_str!("../docs/abc-export.md")]
#[doc = include_str!("../docs/ccli-import.md")]
#[doc = include_str!("../docs/meta-information.md")]
#[doc = include_str!("../docs/complex-slides.md")]
#[doc = include_str!("../docs/text-export.md")]
pub struct DocumentationExamples;

pub mod importer;

/// The filetypes which are supported as input/output
pub mod filetypes;

/// The handling of song presentation slides
pub mod slides;

pub mod templating;

pub mod exporter;

/// Extern library call function for creating a presentation from a given input file
/// 
/// # Parameters
/// - `c_file_path`: The absolute path of the file as a `*const c_char`
/// - `c_title_slide`: A C boolean integer which determins whether to show a separate title slide (0 = false, 1 => true)
/// - `c_show_spoiler`: A C boolean integer which determins whether a designated spoiler is shown 
/// - `c_show_meta_information`: A C integer used as a bit mask for where the meta information is shown: bit 0 = first content slide, bit 1 = last content slide, bit 2 = title slide. So 0 => nowhere, 1 => first slide, 2 => last slide, 3 => first and last, 4 => title slide only, 7 => everywhere.
/// - `c_meta_syntax`: A `*const c_char` which contains the syntax of the shown meta data (if none is desired, give an empty string)
/// - `c_empty_last_slides`: A C boolean integer which determins whether an empty last slide should be appended to every song (0 => false, 1 => true)
/// - `c_max_lines`: A c_int with the max number of lines after which the slide is wrapped. If 0 is given, no slide wrap will take place,
///
/// # Returns
/// The slides as a `*const c_char`.
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
    let title_slide: bool = match c_title_slide {
        1 => true,
        _ => false
    };
    let show_spoiler: bool = match c_show_spoiler {
        1 => true,
        _ => false
    };
    // A bit mask: bit 0 = first content slide, bit 1 = last, bit 2 = title
    // slide. Values 0-3 keep the meaning they had before the title slide
    // became selectable, so existing callers are unaffected.
    let show_meta_information = ShowMetaInformation::from_bits(c_show_meta_information as u8);
    
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
        max_lines,
        language: LanguageConfiguration::default(),
    };
    
    match create_presentation_from_file(file_path, slide_settings) {
        Ok(v) => rust_string_to_c_char(serde_json::to_string(&v).unwrap()).unwrap(),
        Err(err) => rust_string_to_c_char(err.to_string()).unwrap(),
    }
}

/// Create presentation slides from a song file, whatever format it is in.
///
/// The format is detected from the file name — see
/// [`importer::import_song_from_file`] for the list. Classic `.song` files take
/// a dedicated path because that format encodes the presentation order in the
/// file itself, including the spoiler blocks that the generic converter cannot
/// reconstruct from a [`song::Song`].
///
/// # Errors
/// [`importer::errors::CantaraFileDoesNotExistError`] if the path does not
/// exist, or whatever the importer reports.
pub fn slides_from_file(
    file_path: impl AsRef<Path>,
    slide_settings: &SlideSettings,
) -> Result<Vec<Slide>, Box<dyn Error>> {
    let file_path = file_path.as_ref();

    if !file_path.exists() {
        return Err(Box::new(CantaraFileDoesNotExistError));
    }

    let file_type = filetypes::FileType::from_path(file_path).ok_or_else(|| {
        Box::new(CantaraImportUnknownFileExtensionError {
            file_extension: file_path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("")
                .to_string(),
        })
    })?;

    if file_type == filetypes::FileType::ClassicSongFile {
        let content = std::fs::read_to_string(file_path)?;
        let backup_title = file_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        return Ok(slides_from_classic_song(&content, slide_settings, backup_title));
    }

    let song = importer::import_song_from_file(file_path)?;
    Ok(exporter::slides::slides_from_song(&song, slide_settings))
}

/// Create a presentation from a file and return the slides or an error if something went wrong.
///
/// Kept as the name the C interface uses; prefer [`slides_from_file`], which
/// accepts any `AsRef<Path>`.
pub fn create_presentation_from_file(
    file_path: PathBuf,
    slide_settings: SlideSettings,
) -> Result<Vec<Slide>, Box<dyn Error>> {
    slides_from_file(file_path, &slide_settings)
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
    use std::path::{Path, PathBuf};

    use crate::{create_presentation_from_file, slides::SlideSettings};

    use super::song::Song;

    #[test]
    fn create_example_song() {
        let song: Song = Song::new("Test Song");
        assert_eq!(song.title, "Test Song");
        assert_eq!(song.part_count(), 0);
        assert_eq!(song.parts().len(), 0)
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
