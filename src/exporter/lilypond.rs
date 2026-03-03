//! LilyPond exporter — generates a complete standalone `.ly` file from a Song.

use crate::song::{Song, SongPartType};

/// Generate a complete LilyPond (.ly) file from a Song.
///
/// The output includes \version, \header, and \score blocks and can be compiled
/// directly with `lilypond`.
///
/// Returns an error if the song has no voice content to export.
pub fn lilypond_from_song(song: &Song) -> Result<String, String> {
    let mut output = String::new();

    // Version
    output.push_str("\\version \"2.24.0\"\n\n");

    // Header
    output.push_str("\\header {\n");
    output.push_str(&format!("  title = \"{}\"\n", song.title));
    if let Some(author) = song.get_tag("author") {
        output.push_str(&format!("  composer = \"{}\"\n", author));
    }
    output.push_str("}\n\n");

    // Find the voice content (from the first verse that has one, or via repetition chain)
    let parts = song.get_unpacked_parts();
    let voice_content = parts.iter().find_map(|part| song.get_voice_for_part(part));

    let voice_notes = match voice_content {
        Some(vc) => vc.content.clone(),
        None => return Err("Song has no voice content for LilyPond export".to_string()),
    };

    // Collect all lyrics in order (verses only, sorted by number)
    let mut verse_parts: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Verse)
        .collect();
    verse_parts.sort_by_key(|p| p.number);

    let mut lyrics_entries: Vec<String> = Vec::new();
    for part in &verse_parts {
        for content in &part.contents {
            if content.voice_type.is_lyrics() {
                lyrics_entries.push(content.content.clone());
            }
        }
    }

    // Also collect refrain/chorus lyrics if present
    let mut refrain_parts: Vec<_> = parts
        .iter()
        .filter(|p| p.part_type == SongPartType::Refrain || p.part_type == SongPartType::Chorus)
        .collect();
    refrain_parts.sort_by_key(|p| p.number);

    // Build the \score block
    output.push_str("\\score {\n");
    output.push_str("  <<\n");

    // Staff with notes
    output.push_str("    \\new Staff {\n");

    // Key signature
    if let Some(key_str) = song.get_tag("key") {
        if let Some(ly_key) = format_lilypond_key(key_str) {
            output.push_str(&format!("      {}\n", ly_key));
        }
    }

    // Time signature
    if let Some(time_str) = song.get_tag("time") {
        output.push_str(&format!("      \\time {}\n", time_str));
    }

    // Partial (anacrusis/pickup)
    if let Some(partial_str) = song.get_tag("partial") {
        output.push_str(&format!("      \\partial {}\n", partial_str));
    }

    // Notes
    output.push_str(&format!("      {}\n", voice_notes.trim()));
    output.push_str("    }\n");

    // Add lyrics for each verse
    for lyrics in &lyrics_entries {
        output.push_str("    \\addlyrics {\n");
        for line in lyrics.trim().lines() {
            output.push_str(&format!("      {}\n", line.trim()));
        }
        output.push_str("    }\n");
    }

    // Add refrain lyrics if present
    for part in &refrain_parts {
        for content in &part.contents {
            if content.voice_type.is_lyrics() {
                output.push_str("    \\addlyrics {\n");
                for line in content.content.trim().lines() {
                    output.push_str(&format!("      {}\n", line.trim()));
                }
                output.push_str("    }\n");
            }
        }
    }

    output.push_str("  >>\n");
    output.push_str("  \\layout { }\n");
    output.push_str("  \\midi { }\n");
    output.push_str("}\n");

    Ok(output)
}

/// Convert a human-readable key string (e.g. "f major") to LilyPond format (\key f \major).
fn format_lilypond_key(key_str: &str) -> Option<String> {
    let parts: Vec<&str> = key_str.trim().split_whitespace().collect();
    if parts.len() == 2 {
        let note = parts[0].to_lowercase();
        let mode = parts[1].to_lowercase();
        Some(format!("\\key {} \\{}", note, mode))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::song_yml;

    #[test]
    fn test_lilypond_export() {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        let song = song_yml::import_from_yml_string(&content).unwrap();

        let ly_output = lilypond_from_song(&song).unwrap();

        // Check version
        assert!(ly_output.contains("\\version \"2.24.0\""));
        // Check header
        assert!(ly_output.contains("title = \"Amazing Grace\""));
        assert!(ly_output.contains("composer = \"John Newton\""));
        // Check key/time/partial
        assert!(ly_output.contains("\\key f \\major"));
        assert!(ly_output.contains("\\time 3/4"));
        assert!(ly_output.contains("\\partial 4"));
        // Check notes are present
        assert!(ly_output.contains("c4 | f2 a8( f)"));
        // Check lyrics are present
        assert!(ly_output.contains("\\addlyrics"));
        assert!(ly_output.contains("A -- ma -- zing grace"));
        // Check layout and midi
        assert!(ly_output.contains("\\layout { }"));
        assert!(ly_output.contains("\\midi { }"));
    }

    #[test]
    fn test_format_lilypond_key() {
        assert_eq!(
            format_lilypond_key("f major"),
            Some("\\key f \\major".to_string())
        );
        assert_eq!(
            format_lilypond_key("d minor"),
            Some("\\key d \\minor".to_string())
        );
        assert_eq!(format_lilypond_key("invalid"), None);
    }
}
