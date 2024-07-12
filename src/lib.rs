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
- The Cantara classic song format (lyrics only), see [`classic_song`] module.
- The cssf song format (lyrics and scores), see cssf_song module. (under construction)
- the CCLI song format (lyrics only), see ccli_song module. (under construction)
*/
/// - The `song` module contains the data structures needed for songs and its methods for managing and interpreting song data.
pub mod song;

/// - The `importer` module contains functions for importing songs from different formats.
pub mod importer;

use song::Song;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_example_song() {
        let song = Song::new("Test Song");
        assert_eq!(song.title, "Test Song");
        assert_eq!(song.get_total_part_count(), 0);
        assert_eq!(song.get_unpacked_parts().len(), 0)
    }
}
