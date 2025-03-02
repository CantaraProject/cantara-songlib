//! This module handles the loading of cssf (Cantara structured song file) files.
//! If the import is successful, t will return a Song struct with the imported song and also handles errors if they occur during the import process.

use std::error::Error;

use crate::{CantaraImportParsingError, ParsingErrorType};
use crate::{song::Song, CantaraImportNoContentError};

use crate::importer::metadata::parse_metadata_block;

/// Call this function to import a string which contains data in the cssf format (most likely coming from a file which has been read before)
/// 
/// # Parameters
/// - `import_string`: The string which should be parsed and converted into a Song struct
/// - `file_name`: the file name the string is coming from (see Note)
/// 
/// # Return
/// - a Song struct with the imported 
/// 
/// # Note
/// In case the title is not specified as a tag in the cssf string, it will be extracted from the given filename. Otherwise, the given filename is not used. If you are sure that the cssf string contains the title, you could leave `file_name` empty.
pub fn import_input_string(import_string: String, file_name: String) -> Result<Song, Box<dyn Error>> {
    let import_string = import_string.trim();

    if import_string.is_empty() {
        return Err(Box::new(CantaraImportNoContentError));
    };

    let mut flag_first_block = true;
    let mut flag_first_line = true;
    let mut flag_is_metadata_block = false;

    let mut cur_block: String = "".to_string();

    // Create Song Struct
    let mut song: Song = Song::new("");

    for (num, line) in import_string.split("\n").enumerate() {
        if line.is_empty() {
            if !cur_block.is_empty() {
                if flag_is_metadata_block {
                    for (key, value) in parse_metadata_block(&cur_block) {
                        song.add_tag(&key, &value);
                    }
                }
            } else {
                // TODO: Implement parsing

            }
            cur_block = "".to_string();
            flag_first_line = true;
            flag_first_block = false;
            
            continue;
        }
        
        if flag_first_block && flag_first_line {
            if line.contains(":") {
                flag_is_metadata_block = true;
            }
        }

        if flag_first_line && !flag_is_metadata_block { 
            if !line.starts_with("#") {
                return Err(
                    Box::new(
                        CantaraImportParsingError {
                            line: num,
                            error_type: ParsingErrorType::BlockNeedsToStartWithCategorization
                        }
                    )
                );
            }
        }

        if flag_first_line {
            flag_first_line = false;
        }

        if line.trim().is_empty() {
            flag_first_line = true;
        }

        cur_block = cur_block + line + "\n";
    }

    Ok(song)
}