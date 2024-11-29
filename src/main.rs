use cantara_songlib::importer::classic_song::slides_from_classic_song;
use cantara_songlib::slides::PresentationSettings;

use std::path::PathBuf;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// The input file which is to be used
    #[arg(global=true)]
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generates a presentation with presentation slides
    Presentation
}

fn main() -> Result<(), std::io::Error> {
    let cli = Cli::parse();

    if cli.file.is_none() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "No input file was provided."));
    };

    let file = cli.file.unwrap();

    if !file.is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Input file is not a file or does not exist."));
    };

    match &cli.command {
        Commands::Presentation => {
            if file.extension() == Some(std::ffi::OsStr::new("song")) {
                let settings = PresentationSettings::default();

                let file_content = std::fs::read_to_string(&file).unwrap();
                let slides = slides_from_classic_song(
                    &file_content,
                    &settings,
                    file.file_stem().unwrap().to_str().unwrap().to_string()
                );
                println!("{:#?}", slides);
            } else {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "The file type is not supported."));
            }
        }
    }

    Ok(())
}