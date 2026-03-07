//! This crate also provides a very small wrapper cli for directly converting and parsing song files.

use cantara_songlib::exporter;
use cantara_songlib::exporter::lilypond::LilypondSettings;
use cantara_songlib::importer::classic_song::slides_from_classic_song;
use cantara_songlib::importer::song_yml;
use cantara_songlib::slides::{LanguageConfiguration, SlideSettings};

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// The input file which is to be used
    #[arg(global = true)]
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generates a presentation with presentation slides
    Presentation {
        /// Use a specific language for single-language slides (e.g. "en", "de")
        #[arg(short, long)]
        language: Option<String>,

        /// Enable multi-language slides. Optionally provide a comma-separated list
        /// of language codes in display order (e.g. "en,de,fr").
        /// If no languages are specified, all available languages are used.
        #[arg(short, long, value_name = "LANGS")]
        multi_language: Option<Option<String>>,
    },

    /// Generates a LilyPond (.ly) music sheet file
    Lilypond {
        /// Paper size for the output (default: "a4")
        #[arg(short, long, default_value = "a4")]
        paper_size: String,

        /// Layout indent setting (default: "#0")
        #[arg(short, long, default_value = "#0")]
        indent: String,
    },
}

/// Import a Song from the given file path.
fn import_song(file: &PathBuf) -> Result<cantara_songlib::song::Song, Box<dyn std::error::Error>> {
    let path_str = file.to_string_lossy();
    let is_song_yml = path_str.ends_with(".song.yml") || path_str.ends_with(".song.yaml");
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

    if is_song_yml || ext == "yml" || ext == "yaml" {
        let content = std::fs::read_to_string(file)?;
        Ok(song_yml::import_from_yml_string(&content)?)
    } else if ext == "song" {
        let content = std::fs::read_to_string(file)?;
        Ok(cantara_songlib::importer::classic_song::import_song(&content)?)
    } else {
        Err(format!("Unsupported file extension: {}", ext).into())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let file = cli.file.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "No input file was provided.")
    })?;

    if !file.is_file() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Input file is not a file or does not exist.",
        )));
    }

    match &cli.command {
        Commands::Presentation {
            language,
            multi_language,
        } => {
            let lang_config = if let Some(multi) = multi_language {
                // --multi-language was passed
                let langs: Vec<String> = match multi {
                    Some(s) => s.split(',').map(|l| l.trim().to_string()).collect(),
                    None => vec![],
                };
                LanguageConfiguration::MultiLanguage(langs)
            } else {
                LanguageConfiguration::SingleLanguage(language.clone())
            };

            let settings = SlideSettings {
                language: lang_config,
                ..SlideSettings::default()
            };

            let path_str = file.to_string_lossy();
            let is_song_yml = path_str.ends_with(".song.yml") || path_str.ends_with(".song.yaml");
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");

            if ext == "song" && !is_song_yml {
                // Classic .song format — direct slide generation
                let file_content = std::fs::read_to_string(&file)?;
                let slides = slides_from_classic_song(
                    &file_content,
                    &settings,
                    file.file_stem().unwrap().to_str().unwrap().to_string(),
                );
                let json = serde_json::to_string_pretty(&slides)?;
                println!("{}", json);
            } else {
                // Song-based pipeline (yml, yaml, etc.)
                let song = import_song(&file)?;
                let slides = exporter::slides::slides_from_song(&song, &settings);
                let json = serde_json::to_string_pretty(&slides)?;
                println!("{}", json);
            }
        }

        Commands::Lilypond {
            paper_size,
            indent,
        } => {
            let song = import_song(&file)?;
            let settings = LilypondSettings {
                paper_size: paper_size.clone(),
                layout_indent: indent.clone(),
                ..LilypondSettings::default()
            };
            match exporter::lilypond::lilypond_from_song(&song, &settings) {
                Ok(ly_output) => println!("{}", ly_output),
                Err(e) => return Err(e.into()),
            }
        }
    }

    Ok(())
}
