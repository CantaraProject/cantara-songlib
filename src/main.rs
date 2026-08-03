//! This crate also provides a very small wrapper cli for directly converting and parsing song files.

use cantara_songlib::exporter;
use cantara_songlib::exporter::slides::chapters_from_songs;
use cantara_songlib::exporter::text::{text_from_songs, TextFormat, TextSettings};
use cantara_songlib::exporter::abc::AbcSettings;
use cantara_songlib::exporter::lilypond::LilypondSettings;
use cantara_songlib::importer::import_song_from_file;
use cantara_songlib::slides::{
    LanguageConfiguration, ShowMetaInformation, SlideElement, SlideSettings,
};
use cantara_songlib::templating::MetaTemplate;
use cantara_songlib::slides_from_file;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(version, about)]
#[command(subcommand_precedence_over_arg = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// The song file(s) to read. The `text`, `presentation` and `abc` commands
    /// accept several; `lilypond` takes exactly one.
    #[arg(global = true)]
    files: Vec<PathBuf>,
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

        /// Group the output into chapters, one per song. Implied when more than
        /// one input file is given.
        #[arg(long)]
        chapters: bool,
    },

    /// Writes the lyrics as plain text or in a markup of your choosing
    Text {
        /// Output markup: plain, markdown or telegram
        #[arg(short, long, default_value = "plain")]
        format: TextFormat,

        /// A Handlebars template to use instead of a built-in format. See the
        /// documentation for the variables it can use.
        #[arg(long, value_name = "TEMPLATE", conflicts_with = "format")]
        template: Option<String>,

        /// Which language to take the lyrics from (e.g. "en", "de")
        #[arg(short, long)]
        language: Option<String>,

        /// What to put between songs when several are exported
        #[arg(long, value_name = "TEXT")]
        separator: Option<String>,
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

    /// Writes the song in the `.song.yml` format, e.g. to convert a classic
    /// `.song` file into it
    SongYml,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    /// Check that every input exists before doing any work.
    fn inputs(files: &[PathBuf]) -> Result<&[PathBuf], Box<dyn std::error::Error>> {
        if files.is_empty() {
            return Err("no input file was provided".into());
        }
        for file in files {
            if !file.is_file() {
                return Err(format!("'{}' is not a file", file.display()).into());
            }
        }
        Ok(files)
    }

    /// Warn about a song that carries neither notation nor lyrics.
    ///
    /// Such a file is empty, not broken — an export run over a whole song
    /// folder should say so and carry on rather than abort on the first one.
    /// Returns whether the song was empty.
    fn warn_if_empty(song: &cantara_songlib::song::Song, file: &Path) -> bool {
        let empty = !song.has_voice_content() && !song.has_lyrics_content();
        if empty {
            eprintln!(
                "warning: '{}' has neither notation nor lyrics — nothing to export",
                file.display()
            );
        }
        empty
    }

    /// Set the `X:` reference number of an ABC tune.
    ///
    /// Tunes in one ABC file have to be numbered apart; the exporter always
    /// emits `X:1` because it only ever sees a single song.
    fn renumber_tune(abc: &str, number: usize) -> String {
        match abc.split_once('\n') {
            Some((first, rest)) if first.starts_with("X:") => {
                format!("X:{}\n{}", number, rest)
            }
            _ => abc.to_string(),
        }
    }

    /// The single input file, for the commands that take exactly one.
    fn only(files: &[PathBuf]) -> Result<&PathBuf, Box<dyn std::error::Error>> {
        match files {
            [file] => Ok(file),
            _ => Err(format!(
                "this command takes exactly one input file, {} were given",
                files.len()
            )
            .into()),
        }
    }

    match &cli.command {
        Commands::Presentation {
            language,
            multi_language,
            chapters,
            show,
            max_lines,
            meta_syntax,
            show_meta,
            no_title_slide,
        } => {
            let files = inputs(&cli.files)?;
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

            // Several songs — or an explicit request — are grouped into
            // chapters so that a presentation can tell them apart.
            if files.len() > 1 || *chapters {
                let songs: Result<Vec<_>, _> = files.iter().map(import_song_from_file).collect();
                let chapters = chapters_from_songs(&songs?, &settings);
                println!("{}", serde_json::to_string_pretty(&chapters)?);
            } else {
                let slides = slides_from_file(only(files)?, &settings)?;
                println!("{}", serde_json::to_string_pretty(&slides)?);
            }
        }

        Commands::Text {
            format,
            template,
            language,
            separator,
        } => {
            let files = inputs(&cli.files)?;
            let settings = TextSettings {
                format: match template {
                    Some(template) => TextFormat::Custom(template.clone()),
                    None => format.clone(),
                },
                language: language.clone(),
                song_separator: separator.clone(),
            };

            let songs: Result<Vec<_>, _> = files.iter().map(import_song_from_file).collect();
            println!("{}", text_from_songs(&songs?, &settings)?);
        }

        Commands::Lilypond {
            paper_size,
            indent,
        } => {
            let file = only(inputs(&cli.files)?)?;
            let song = import_song_from_file(file)?;
            let settings = LilypondSettings {
                paper_size: paper_size.clone(),
                layout_indent: indent.clone(),
                ..LilypondSettings::default()
            };
            // A song with neither notation nor lyrics is nothing to engrave,
            // but it is not a broken input either.
            if warn_if_empty(&song, file) {
                return Ok(());
            }
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
            let files = inputs(&cli.files)?;
            let settings = AbcSettings {
                unit_note_length: unit_note_length.clone(),
                include_chords: *include_chords,
                include_all_verses: *all_verses,
            };

            // An ABC file holds a sequence of tunes, so several songs simply
            // follow one another — each with its own X: number.
            let mut tune_number = 0usize;
            for file in files {
                let song = import_song_from_file(file)?;
                if warn_if_empty(&song, file) {
                    continue;
                }
                let abc_output = exporter::abc::abc_from_song(&song, &settings)?;
                tune_number += 1;
                // The trailing newline of `println!` doubles as the blank line
                // the ABC standard wants between two tunes.
                println!("{}", renumber_tune(&abc_output, tune_number));
            }
        }

        Commands::SongYml => {
            // One `.song.yml` file holds exactly one song.
            let file = only(inputs(&cli.files)?)?;
            let song = import_song_from_file(file)?;
            print!("{}", exporter::song_yml::song_yml_from_song(&song)?);
        }
    }

    Ok(())
}
