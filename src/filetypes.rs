use std::path::Path;

/// This enum contains entries for all supported file formats (as input and output)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileType {
    ClassicSongFile,
    CSSF,
    SongYaml,
    CCLISongselectFile,
}

pub fn contains_song_structure(file_type: FileType) -> bool {
    match file_type {
        FileType::ClassicSongFile => false,
        FileType::CSSF => true,
        FileType::SongYaml => true,
        FileType::CCLISongselectFile => true,
    }
}

pub fn conatains_presentation_order(file_type: FileType) -> bool {
    match file_type {
        FileType::ClassicSongFile => true,
        FileType::CSSF => true,
        FileType::SongYaml => false,
        FileType::CCLISongselectFile => false,
    }
}

pub fn get_file_type_by_file_ending(ending: &str) -> Option<FileType> {
    // Accept the ending with or without its leading dot.
    match ending.trim_start_matches('.').to_lowercase().as_str() {
        "cssf" => Some(FileType::CSSF),
        "song" => Some(FileType::ClassicSongFile),
        "song.yml" | "song.yaml" | "yml" | "yaml" => Some(FileType::SongYaml),
        "ccli" => Some(FileType::CCLISongselectFile),
        _ => None,
    }
}

impl FileType {
    /// Detect the format of a song file from its name.
    ///
    /// Handles the double extension of `.song.yml` before falling back to the
    /// last extension, so that `Amazing Grace.song.yml` is recognised as YAML
    /// rather than as a classic `.song` file.
    ///
    /// ```
    /// use cantara_songlib::filetypes::FileType;
    /// use std::path::Path;
    ///
    /// assert_eq!(FileType::from_path(Path::new("a.song.yml")), Some(FileType::SongYaml));
    /// assert_eq!(FileType::from_path(Path::new("a.song")), Some(FileType::ClassicSongFile));
    /// assert_eq!(FileType::from_path(Path::new("a.ccli")), Some(FileType::CCLISongselectFile));
    /// assert_eq!(FileType::from_path(Path::new("a.txt")), None);
    /// ```
    pub fn from_path(path: &Path) -> Option<FileType> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();

        if name.ends_with(".song.yml") || name.ends_with(".song.yaml") {
            return Some(FileType::SongYaml);
        }

        let extension = path.extension()?.to_str()?;
        get_file_type_by_file_ending(extension)
    }
}

#[cfg(test)]
mod tests {
    use super::{contains_song_structure, FileType};
    use std::path::Path;

    #[test]
    fn test_contains_song_structure() {
        assert!(contains_song_structure(FileType::CCLISongselectFile));
        assert!(!contains_song_structure(FileType::ClassicSongFile));
        assert!(contains_song_structure(FileType::CSSF));
        assert!(contains_song_structure(FileType::SongYaml));
    }

    #[test]
    fn test_file_type_detection() {
        let cases = [
            ("Amazing Grace.song.yml", Some(FileType::SongYaml)),
            ("Amazing Grace.song.yaml", Some(FileType::SongYaml)),
            ("Amazing Grace.song", Some(FileType::ClassicSongFile)),
            ("Amazing Grace.cssf", Some(FileType::CSSF)),
            ("Weiß ich den Weg auch nicht.ccli", Some(FileType::CCLISongselectFile)),
            // Extensions are matched case-insensitively.
            ("SONG.CCLI", Some(FileType::CCLISongselectFile)),
            ("notes.txt", None),
            ("no extension", None),
        ];

        for (name, expected) in cases {
            assert_eq!(FileType::from_path(Path::new(name)), expected, "for {}", name);
        }
    }
}