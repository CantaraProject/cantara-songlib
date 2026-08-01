//! Export one song to every supported format.
//!
//! Run with:
//! ```text
//! cargo run --example export_formats
//! ```

use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::exporter::lilypond::{lilypond_from_song, LilypondSettings};
use cantara_songlib::exporter::slides::slides_from_song;
use cantara_songlib::importer::song_yml;
use cantara_songlib::slides::{
    LanguageConfiguration, SlideContent, SlideElement, SlideRowKind, SlideSettings,
};
use cantara_songlib::song::Song;

const SONG_FILE: &str = "tests/data/Sei nicht stolz auf das, was du bist.song.yml";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(SONG_FILE)?;
    let song = song_yml::import_from_yml_string(&content)?;

    describe(&song);

    println!("\n=== Slides ===");
    let slides = slides_from_song(&song, &SlideSettings::default());
    for (index, slide) in slides.iter().enumerate() {
        println!("  slide {}: {:?}", index + 1, slide.slide_content);
    }

    println!("\n=== Complex slides: notation + lyrics, two lines each ===");
    let complex = SlideSettings {
        language: LanguageConfiguration::Complex(vec![
            SlideElement::Notation,
            SlideElement::Lyrics("de".to_string()),
        ]),
        max_lines: Some(2),
        title_slide: false,
        empty_last_slide: false,
        ..SlideSettings::default()
    };
    for (index, slide) in slides_from_song(&song, &complex).iter().enumerate() {
        let SlideContent::Complex(slide) = &slide.slide_content else {
            continue;
        };
        println!("  slide {} — {} lyrics lines", index + 1, slide.line_count);
        for row in &slide.rows {
            match &row.kind {
                SlideRowKind::Notation { syllables } => println!(
                    "    notation ({} syllables): {}",
                    syllables,
                    row.content.lines().last().unwrap_or_default()
                ),
                SlideRowKind::Lyrics { language } => println!(
                    "    {}: {}",
                    language.as_deref().unwrap_or("(no language stated)"),
                    row.content.replace('\n', " / ")
                ),
            }
        }
    }

    println!("\n=== LilyPond ===");
    println!("{}", lilypond_from_song(&song, &LilypondSettings::default())?);

    println!("\n=== ABC ===");
    println!("{}", abc_from_song(&song, &AbcSettings::default())?);

    Ok(())
}

/// Print what the importer made of the file.
fn describe(song: &Song) {
    println!("=== {} ===", song.title);
    for (key, value) in song.tags() {
        println!("  {}: {}", key, value);
    }
    if !song.score.is_empty() {
        println!("  score: {:?}", song.score);
    }
    println!("  languages: {:?}", song.available_languages());

    println!("  parts:");
    for part in song.parts() {
        let kinds: Vec<String> = part
            .contents
            .iter()
            .map(|content| content.content_type.to_string())
            .collect();
        println!("    {} — {}", part.id(), kinds.join(", "));
    }

    let order: Vec<String> = song
        .ordered_parts()
        .iter()
        .map(|part| part.id().to_string())
        .collect();
    println!("  sung as: {}", order.join(" → "));
}
