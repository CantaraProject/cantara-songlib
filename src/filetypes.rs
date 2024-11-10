/// This enum contains entries for all supported file formats (as input and output)
pub enum FileType {
    ClassicSongFile,
    CSSF,
    CCLISongselectFile,
}

pub fn contains_song_structure(file_type: FileType) -> bool {
    match file_type {
        FileType::ClassicSongFile => false,
        FileType::CSSF => true,
        FileType::CCLISongselectFile => true,
    }
}

pub fn conatains_presentation_order(file_type: FileType) -> bool {
    match file_type {
        FileType::ClassicSongFile => true,
        FileType::CSSF => true,
        FileType::CCLISongselectFile => false,
    }
}

pub fn get_file_type_by_file_ending(ending: &str) -> Option<FileType> {
    match ending {
        ".cssf" => Some(FileType::CSSF),
        ".song" => Some(FileType::ClassicSongFile),
        ".ccli" => Some(FileType::CCLISongselectFile),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{FileType, contains_song_structure};

    #[test]
    fn test_contains_song_structure() {
        assert!(contains_song_structure(FileType::CCLISongselectFile));
        assert!(!contains_song_structure(FileType::ClassicSongFile));
        assert!(contains_song_structure(FileType::CSSF));
    }
}