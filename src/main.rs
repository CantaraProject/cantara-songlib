use cantara_songlib::importer;

use importer::{ get_song_from_file_as_json };

fn main() {
    
    println!("{}", get_song_from_file_as_json("testfiles/O What A Savior That He Died For Me.song").unwrap());
}