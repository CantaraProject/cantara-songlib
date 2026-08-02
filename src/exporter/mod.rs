//! The exporter module contains functions for exporting songs to different output formats.

pub mod slides;

pub mod lilypond;

pub mod abc;

pub mod text;

use crate::song::{Song, SongPartType};

/// One block of readable lyrics: a label such as `"1."` or `"Refrain"` and the
/// lines of text below it.
pub(crate) struct LyricsBlock {
    pub label: String,
    pub lines: Vec<String>,
}

/// The lyrics of a song as readable text, one block per part that has any.
///
/// The notation exporters fall back to this when a song carries no music:
/// there is nothing to engrave, but the text is still worth typesetting. The
/// LilyPond syllable markup (`--`, `_`, `\set …`) is stripped, so the result
/// reads the way an audience would see it rather than the way it is sung.
pub(crate) fn readable_lyrics_blocks(song: &Song) -> Vec<LyricsBlock> {
    let mut blocks = Vec::new();
    let mut verse_number = 1u32;

    for part in song.parts() {
        for content in &part.contents {
            if !content.content_type.is_lyrics() {
                continue;
            }

            let lines: Vec<String> = slides::lyrics_for_reading(&content.content)
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect();

            if lines.is_empty() {
                continue;
            }

            // Verses are numbered through; everything else is named after its
            // part type, which is what a reader needs to follow the order.
            let label = match part.part_type {
                SongPartType::Verse => {
                    let label = format!("{}.", verse_number);
                    verse_number += 1;
                    label
                }
                SongPartType::Refrain | SongPartType::Chorus => "Refrain".to_string(),
                other => {
                    let name = other.as_str();
                    let mut chars = name.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        None => name.to_string(),
                    }
                }
            };

            blocks.push(LyricsBlock { label, lines });
        }
    }

    blocks
}
