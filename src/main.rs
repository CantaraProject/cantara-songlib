//! This crate also provides a very small wrapper cli for directly converting and parsing song files.

use cantara_songlib::exporter;
use cantara_songlib::exporter::abc::AbcSettings;
use cantara_songlib::exporter::lilypond::LilypondSettings;
use cantara_songlib::importer::import_song_from_file;
use cantara_songlib::slides::{
    LanguageConfiguration, ShowMetaInformation, SlideElement, SlideSettings,
};
use cantara_songlib::templating::MetaTemplate;
use cantara_songlib::slides_from_file;

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

/// Where the meta information line may appear, as selectable on the command
/// line. Maps onto [`ShowMetaInformation`].
#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
enum MetaPosition {
    /// On the song's title slide
    Title,
    /// On the first content slide
    First,
    /// On the last content slide
    Last,
}

impl MetaPosition {
    /// Fold the selected positions into the library's settings type.
    fn to_settings(positions: &[MetaPosition]) -> ShowMetaInformation {
        let mut show = ShowMetaInformation::none();
        for position in positions {
            match position {
                MetaPosition::Title => show.title_slide = true,
                MetaPosition::First => show.first_slide = true,
                MetaPosition::Last => show.last_slide = true,
            }
        }
        show
    }
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

        /// Complex layout: stack notation and any number of languages on each
        /// slide, in the given order. Use "notation" (or "abc") for the melody
        /// and a language code for lyrics, e.g. --show notation,en,de
        #[arg(long, value_name = "ROWS", value_delimiter = ',')]
        show: Vec<String>,

        /// Maximum number of lyrics lines per slide; longer parts are wrapped
        #[arg(long, value_name = "N")]
        max_lines: Option<usize>,

        /// Handlebars template for the meta information line, e.g.
        /// "{{title}} ({{author}})". Available variables are the song's tags
        /// plus "title". Empty means no meta information.
        #[arg(long, value_name = "TEMPLATE", default_value = "")]
        meta_syntax: String,

        /// Where to show the meta information. Repeat the flag or separate the
        /// values with a comma, e.g. --show-meta title,first.
        #[arg(long, value_name = "WHERE", value_delimiter = ',', value_enum)]
        show_meta: Vec<MetaPosition>,

        /// Omit the separate title slide at the beginning of the song
        #[arg(long)]
        no_title_slide: bool,
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

    /// Generates an ABC notation (.abc) music file
    Abc {
        /// Unit note length for the output (default: "1/4")
        #[arg(short, long, default_value = "1/4")]
        unit_note_length: String,

        /// Include chord symbols above the staff
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        include_chords: bool,

        /// Include all verses in the output (if false, only first verse is included)
        #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
        all_verses: bool,
    },
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
            show,
            max_lines,
            meta_syntax,
            show_meta,
            no_title_slide,
        } => {
            let lang_config = if !show.is_empty() {
                // --show wins: it is the most specific of the three.
                LanguageConfiguration::Complex(
                    show.iter().map(|row| SlideElement::parse(row)).collect(),
                )
            } else if let Some(multi) = multi_language {
                // --multi-language was passed
                let langs: Vec<String> = match multi {
                    Some(s) => s.split(',').map(|l| l.trim().to_string()).collect(),
                    None => vec![],
                };
                LanguageConfiguration::MultiLanguage(langs)
            } else {
                LanguageConfiguration::SingleLanguage(language.clone())
            };

            // Fail early and clearly on a broken template rather than
            // silently producing slides without any metadata.
            MetaTemplate::parse(meta_syntax)
                .map_err(|error| format!("invalid --meta-syntax template: {}", error))?;

            let settings = SlideSettings {
                language: lang_config,
                max_lines: *max_lines,
                title_slide: !no_title_slide,
                meta_syntax: meta_syntax.clone(),
                show_meta_information: MetaPosition::to_settings(show_meta),
                ..SlideSettings::default()
            };

            let slides = slides_from_file(&file, &settings)?;
            println!("{}", serde_json::to_string_pretty(&slides)?);
        }

        Commands::Lilypond {
            paper_size,
            indent,
        } => {
            let song = import_song_from_file(&file)?;
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

        Commands::Abc {
            unit_note_length,
            include_chords,
            all_verses,
        } => {
            let song = import_song_from_file(&file)?;
            let settings = AbcSettings {
                unit_note_length: unit_note_length.clone(),
                include_chords: *include_chords,
                include_all_verses: *all_verses,
            };
            match exporter::abc::abc_from_song(&song, &settings) {
                Ok(abc_output) => println!("{}", abc_output),
                Err(e) => return Err(e.into()),
            }
        }
    }

    Ok(())
}
