/// This library contains functions to import, parse and export song files of different formats.

/// The library is structured as follows:

/// - The `song` module contains the `Song` struct and its methods for managing and interpreting song data.
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
