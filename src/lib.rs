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
use importer::import_song_from_file;
use exporter::abc::AbcSettings;
use exporter::lilypond::LilypondSettings;
use exporter::text::{text_from_song, TextFormat, TextSettings};
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
#[doc = include_str!("../docs/c-api.md")]
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
#[unsafe(no_mangle)]
pub extern "C" fn create_presentation_from_file_c(
    c_file_path: *const c_char,
    c_title_slide: c_int,
    c_show_spoiler: c_int,
    c_show_meta_information: c_int,
    c_meta_syntax: *const c_char,
    c_empty_last_side: c_int,
    c_max_lines: c_int
) -> *const c_char {
    let file_path: PathBuf = match c_string_to_rust(c_file_path) {
        Ok(path) => PathBuf::from(path),
        Err(error) => return rust_string_to_c_char(error),
    };
    let title_slide: bool = c_title_slide == 1;
    let show_spoiler: bool = c_show_spoiler == 1;
    // A bit mask: bit 0 = first content slide, bit 1 = last, bit 2 = title
    // slide. Values 0-3 keep the meaning they had before the title slide
    // became selectable, so existing callers are unaffected.
    let show_meta_information = ShowMetaInformation::from_bits(c_show_meta_information as u8);
    
    let meta_syntax = match c_string_to_rust(c_meta_syntax) {
        Ok(syntax) => syntax,
        Err(error) => return rust_string_to_c_char(error),
    };
    
    let empty_last_slide: bool = c_empty_last_side == 1;
    
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
        Ok(v) => match serde_json::to_string(&v) {
            Ok(slides_json) => rust_string_to_c_char(slides_json),
            Err(error) => rust_string_to_c_char(error.to_string()),
        },
        Err(err) => rust_string_to_c_char(err.to_string()),
    }
}

/// Loads a song from a file and returns its JSON representation.
#[unsafe(no_mangle)]
pub extern "C" fn get_song_from_file_as_json_c(c_file_path: *const c_char) -> *const c_char {
    let file_path = match c_string_to_rust(c_file_path) {
        Ok(path) => path,
        Err(error) => return rust_string_to_c_char(error),
    };

    match importer::get_song_from_file_as_json(&file_path) {
        Ok(song_json) => rust_string_to_c_char(song_json),
        Err(error) => rust_string_to_c_char(error.to_string()),
    }
}

/// Exports a song as text from a supported song file.
#[unsafe(no_mangle)]
pub extern "C" fn create_text_from_file_c(
    c_file_path: *const c_char,
    c_format: *const c_char,
    c_template: *const c_char,
    c_language: *const c_char,
    c_separator: *const c_char,
) -> *const c_char {
    let file_path = match c_string_to_rust(c_file_path) {
        Ok(path) => path,
        Err(error) => return rust_string_to_c_char(error),
    };

    let format_name = match c_string_to_rust(c_format) {
        Ok(format) => format,
        Err(error) => return rust_string_to_c_char(error),
    };

    let template = match c_optional_string_to_option(c_template) {
        Ok(value) => value,
        Err(error) => return rust_string_to_c_char(error),
    };

    let language = match c_optional_string_to_option(c_language) {
        Ok(value) => value,
        Err(error) => return rust_string_to_c_char(error),
    };

    let separator = match c_optional_string_to_option(c_separator) {
        Ok(value) => value,
        Err(error) => return rust_string_to_c_char(error),
    };

    let format = match template {
        Some(template) => TextFormat::Custom(template),
        None => match TextFormat::parse(&format_name) {
            Some(format) => format,
            None => {
                return rust_string_to_c_char(format!(
                    "unknown text format '{}' (expected plain, markdown or telegram)",
                    format_name
                ));
            }
        },
    };

    let settings = TextSettings {
        format,
        language,
        song_separator: separator,
    };

    let song = match import_song_from_file(&file_path) {
        Ok(song) => song,
        Err(error) => return rust_string_to_c_char(error.to_string()),
    };

    match text_from_song(&song, &settings) {
        Ok(text) => rust_string_to_c_char(text),
        Err(error) => rust_string_to_c_char(error.to_string()),
    }
}

/// Exports a song as LilyPond source from a supported song file.
#[unsafe(no_mangle)]
pub extern "C" fn create_lilypond_from_file_c(
    c_file_path: *const c_char,
    c_paper_size: *const c_char,
    c_layout_indent: *const c_char,
) -> *const c_char {
    let file_path = match c_string_to_rust(c_file_path) {
        Ok(path) => path,
        Err(error) => return rust_string_to_c_char(error),
    };
    let paper_size = match c_string_to_rust(c_paper_size) {
        Ok(size) => size,
        Err(error) => return rust_string_to_c_char(error),
    };
    let layout_indent = match c_string_to_rust(c_layout_indent) {
        Ok(indent) => indent,
        Err(error) => return rust_string_to_c_char(error),
    };

    let settings = LilypondSettings {
        paper_size,
        layout_indent,
        ..LilypondSettings::default()
    };

    let song = match import_song_from_file(&file_path) {
        Ok(song) => song,
        Err(error) => return rust_string_to_c_char(error.to_string()),
    };

    match exporter::lilypond::lilypond_from_song(&song, &settings) {
        Ok(output) => rust_string_to_c_char(output),
        Err(error) => rust_string_to_c_char(error),
    }
}

/// Exports a song as ABC notation from a supported song file.
#[unsafe(no_mangle)]
pub extern "C" fn create_abc_from_file_c(
    c_file_path: *const c_char,
    c_unit_note_length: *const c_char,
    c_include_chords: c_int,
    c_include_all_verses: c_int,
) -> *const c_char {
    let file_path = match c_string_to_rust(c_file_path) {
        Ok(path) => path,
        Err(error) => return rust_string_to_c_char(error),
    };
    let unit_note_length = match c_string_to_rust(c_unit_note_length) {
        Ok(length) => length,
        Err(error) => return rust_string_to_c_char(error),
    };

    let settings = AbcSettings {
        unit_note_length,
        include_chords: c_include_chords == 1,
        include_all_verses: c_include_all_verses == 1,
    };

    let song = match import_song_from_file(&file_path) {
        Ok(song) => song,
        Err(error) => return rust_string_to_c_char(error.to_string()),
    };

    match exporter::abc::abc_from_song(&song, &settings) {
        Ok(output) => rust_string_to_c_char(output),
        Err(error) => rust_string_to_c_char(error),
    }
}

/// Exports a song as `.song.yml` from a supported song file.
#[unsafe(no_mangle)]
pub extern "C" fn create_song_yml_from_file_c(c_file_path: *const c_char) -> *const c_char {
    let file_path = match c_string_to_rust(c_file_path) {
        Ok(path) => path,
        Err(error) => return rust_string_to_c_char(error),
    };

    let song = match import_song_from_file(&file_path) {
        Ok(song) => song,
        Err(error) => return rust_string_to_c_char(error.to_string()),
    };

    match exporter::song_yml::song_yml_from_song(&song) {
        Ok(output) => rust_string_to_c_char(output),
        Err(error) => rust_string_to_c_char(error),
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

fn c_string_to_rust(c_str: *const c_char) -> Result<String, String> {
    if c_str.is_null() {
        return Err("received null pointer where C string was expected".to_string());
    }

    unsafe {
        let cstr = CStr::from_ptr(c_str);
        cstr.to_str()
            .map(|s| s.to_string())
            .map_err(|_| "received invalid UTF-8 input".to_string())
    }
}

fn c_optional_string_to_option(c_str: *const c_char) -> Result<Option<String>, String> {
    if c_str.is_null() {
        return Ok(None);
    }
    let value = c_string_to_rust(c_str)?;
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn rust_string_to_c_char(rust_str: String) -> *const c_char {
    match CString::new(rust_str) {
        Ok(c_string) => c_string.into_raw() as *const c_char,
        Err(_) => std::ptr::null(),
    }
}

/// Frees a C string returned by this library.
#[unsafe(no_mangle)]
pub extern "C" fn free_c_string(c_str: *mut c_char) {
    if c_str.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(c_str));
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
