use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::importer::song_yml;

fn main() -> Result<(), String> {
    let content = std::fs::read_to_string("testfiles/Sei nicht stolz auf das, was du bist.song.yml")
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    let song = song_yml::import_from_yml_string(&content)
        .map_err(|e| format!("Failed to parse song: {}", e))?;
    
    let abc_output = abc_from_song(&song, &AbcSettings::default())?;
    
    println!("{}", abc_output);
    Ok(())
}
