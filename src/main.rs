use cantara_songlib::importer;

use importer::{ get_song_from_file_as_json };

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1) {
        Some(filepath) => {
            println!("{}", 
                match get_song_from_file_as_json(&filepath) {
                    Ok(parsed_json) => parsed_json,
                    Err(error) => error.to_string()
                }        
            )
        },
        None => { println!("You need to specify a filepath!") }
    }
}