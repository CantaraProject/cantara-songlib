//! ABC Export Demo - Demonstrates exporting a song to ABC notation

use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::importer::song_yml;

fn main() {
    println!("=== ABC Notation Export Demo ===\n");
    
    // Load Amazing Grace from YAML
    let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml")
        .expect("Failed to read test file");
    
    let song = song_yml::import_from_yml_string(&content)
        .expect("Failed to parse YAML");
    
    println!("Loaded song: {}", song.title);
    println!("Author: {:?}", song.tag("author"));
    println!();
    
    // Export to ABC notation with default settings
    let abc_output = abc_from_song(&song, &AbcSettings::default())
        .expect("Failed to export to ABC");
    
    println!("--- ABC Output (Default Settings) ---");
    println!("{}", abc_output);
    
    // Export with custom settings
    let custom_settings = AbcSettings {
        unit_note_length: "1/8".to_string(),
        include_chords: false,
        include_all_verses: false,
    };
    
    let abc_custom = abc_from_song(&song, &custom_settings)
        .expect("Failed to export to ABC with custom settings");
    
    println!("\n--- ABC Output (Custom Settings: 1/8 note, first verse only) ---");
    println!("{}", abc_custom);
}
