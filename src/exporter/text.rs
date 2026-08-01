//! Export songs as plain text or as any markup you can describe with a
//! template.
//!
//! Two things are often wanted from a song that are neither slides nor sheet
//! music: the bare lyrics as a text file, and the lyrics wrapped in some markup
//! to paste into a chat or a document. Both are the same job — write the verses
//! in singing order — so both are done by the same exporter, differing only in
//! the [Handlebars](https://handlebarsjs.com/) template they use.
//!
//! ```
//! use cantara_songlib::exporter::text::{text_from_song, TextFormat, TextSettings};
//! use cantara_songlib::importer::import_song_from_file;
//!
//! let song = import_song_from_file("tests/data/Amazing Grace.song.yml")?;
//!
//! let plain = text_from_song(&song, &TextSettings::default())?;
//! assert!(plain.starts_with("Amazing Grace\n\nAmazing grace, How sweet the sound"));
//!
//! let markdown = text_from_song(&song, &TextSettings::with_format(TextFormat::Markdown))?;
//! assert!(markdown.starts_with("# Amazing Grace"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use handlebars::{Handlebars, TemplateError};
use serde_json::{json, Map, Value};

use crate::song::Song;
use crate::templating::variables_for_song;

/// The markup to wrap the lyrics in.
#[derive(Clone, PartialEq, Eq, Debug)]
#[derive(Default)]
pub enum TextFormat {
    /// The lyrics and nothing else: the title, a blank line, then every part in
    /// singing order separated by blank lines.
    #[default]
    Plain,
    /// Markdown: the title as a heading and the author in italics.
    Markdown,
    /// Telegram: the title in bold. Telegram's own markup, not Markdown.
    Telegram,
    /// Your own Handlebars template — see [`TextFormat::template`] for the
    /// variables it can use.
    Custom(String),
}


impl TextFormat {
    /// The Handlebars template this format renders with.
    ///
    /// The variables available are:
    ///
    /// | Variable | Contents |
    /// |----------|----------|
    /// | `title` | the song's title |
    /// | `language` | the language the lyrics were taken from, if the song stated one |
    /// | `author`, `copyright`, … | every tag of the song |
    /// | `parts` | the parts in singing order — see below |
    ///
    /// Each entry of `parts` offers `text` (the lyrics, ready to read),
    /// `label` (the heading the source gave it, e.g. `"Vers 1"`), `id`
    /// (`"verse.1"`), `kind` (`"verse"`), `number`, `position` (1-based) and
    /// the flags `first` and `last`.
    ///
    /// ```
    /// use cantara_songlib::exporter::text::TextFormat;
    ///
    /// assert!(TextFormat::Markdown.template().starts_with("# {{title}}"));
    /// assert_eq!(TextFormat::Custom("{{title}}".into()).template(), "{{title}}");
    /// ```
    pub fn template(&self) -> &str {
        match self {
            TextFormat::Plain => PLAIN_TEMPLATE,
            TextFormat::Markdown => MARKDOWN_TEMPLATE,
            TextFormat::Telegram => TELEGRAM_TEMPLATE,
            TextFormat::Custom(template) => template,
        }
    }

    /// Parse a format name: `"plain"`, `"markdown"`/`"md"` or `"telegram"`.
    ///
    /// ```
    /// use cantara_songlib::exporter::text::TextFormat;
    ///
    /// assert_eq!("md".parse(), Ok(TextFormat::Markdown));
    /// assert_eq!("Telegram".parse(), Ok(TextFormat::Telegram));
    /// assert!("nonsense".parse::<TextFormat>().is_err());
    /// ```
    pub fn parse(name: &str) -> Option<TextFormat> {
        match name.trim().to_lowercase().as_str() {
            "plain" | "text" | "txt" => Some(TextFormat::Plain),
            "markdown" | "md" => Some(TextFormat::Markdown),
            "telegram" => Some(TextFormat::Telegram),
            _ => None,
        }
    }
}

impl std::str::FromStr for TextFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TextFormat::parse(s).ok_or_else(|| {
            format!("unknown text format '{}' (expected plain, markdown or telegram)", s)
        })
    }
}

/// The title, then every part in singing order.
const PLAIN_TEMPLATE: &str = "\
{{title}}

{{#each parts}}
{{text}}

{{/each}}";

/// A Markdown heading, the author in italics, then the parts.
const MARKDOWN_TEMPLATE: &str = "\
# {{title}}
{{#if author}}
*{{author}}*
{{/if}}

{{#each parts}}
{{text}}

{{/each}}";

/// Telegram's markup: the title in bold, then the parts.
const TELEGRAM_TEMPLATE: &str = "\
**{{title}}**

{{#each parts}}
{{text}}

{{/each}}";

/// Configuration for the text export.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TextSettings {
    /// The markup to use.
    pub format: TextFormat,
    /// Which language to take the lyrics from.
    ///
    /// `None` uses the song's default language, falling back to whatever
    /// lyrics a part has — which is what makes a song with no language
    /// information export just as well as one with several.
    pub language: Option<String>,
    /// What to put between songs when exporting more than one.
    ///
    /// `None` uses a blank line.
    pub song_separator: Option<String>,
}

impl TextSettings {
    /// Settings using the given format and nothing else changed.
    pub fn with_format(format: TextFormat) -> TextSettings {
        TextSettings {
            format,
            ..TextSettings::default()
        }
    }

    /// The separator between songs.
    fn separator(&self) -> &str {
        self.song_separator.as_deref().unwrap_or("\n")
    }
}

/// Render one song.
///
/// The lyrics are taken in singing order — the order
/// [`Song::ordered_parts`] produces, so a refrain appears after every verse
/// just as it is sung.
///
/// # Errors
/// A [`TemplateError`] if a [`TextFormat::Custom`] template is malformed.
pub fn text_from_song(song: &Song, settings: &TextSettings) -> Result<String, TemplateError> {
    let registry = registry(settings)?;
    Ok(render(&registry, &context_for_song(song, settings)))
}

/// Render several songs into one document.
///
/// Every song is rendered with the same template and the results are joined by
/// [`TextSettings::song_separator`].
///
/// ```
/// use cantara_songlib::exporter::text::{text_from_songs, TextFormat, TextSettings};
/// use cantara_songlib::importer::import_song_from_file;
///
/// let songs = [
///     import_song_from_file("tests/data/Amazing Grace.song.yml")?,
///     import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli")?,
/// ];
///
/// let document = text_from_songs(&songs, &TextSettings::with_format(TextFormat::Markdown))?;
/// assert!(document.contains("# Amazing Grace"));
/// assert!(document.contains("# Weiß ich den Weg auch nicht (Pax Dei)"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Errors
/// A [`TemplateError`] if a [`TextFormat::Custom`] template is malformed.
pub fn text_from_songs(songs: &[Song], settings: &TextSettings) -> Result<String, TemplateError> {
    let registry = registry(settings)?;

    let rendered: Vec<String> = songs
        .iter()
        .map(|song| render(&registry, &context_for_song(song, settings)))
        .filter(|text| !text.is_empty())
        .collect();

    Ok(rendered.join(&format!("\n{}\n", settings.separator())))
}

/// A Handlebars registry with the template compiled once.
fn registry(settings: &TextSettings) -> Result<Handlebars<'static>, TemplateError> {
    let mut registry = Handlebars::new();
    // The output is text or plain-text markup, never HTML.
    registry.register_escape_fn(handlebars::no_escape);
    registry.set_strict_mode(false);
    registry.register_template_string(TEMPLATE_NAME, settings.format.template())?;
    Ok(registry)
}

const TEMPLATE_NAME: &str = "song";

/// Render one context, tidying up the trailing blank lines the loops leave.
fn render(registry: &Handlebars, context: &Value) -> String {
    registry
        .render(TEMPLATE_NAME, context)
        .map(|text| text.trim_end().to_string())
        .unwrap_or_default()
}

/// Build the variables for one song.
///
/// Scalars come from [`variables_for_song`], so a template can use the same
/// `{{title}}` and `{{author}}` as the meta information line. On top of that
/// sits `parts`, the lyrics in singing order.
fn context_for_song(song: &Song, settings: &TextSettings) -> Value {
    let mut context = Map::new();

    for (key, value) in variables_for_song(song) {
        context.insert(key, Value::String(value));
    }

    let mut parts: Vec<Value> = Vec::new();
    for part in song.ordered_parts() {
        let Some(content) =
            part.lyrics_for(settings.language.as_deref(), song.default_language.as_deref())
        else {
            // A part without lyrics — an instrumental interlude, say — has
            // nothing to write.
            continue;
        };

        parts.push(json!({
            "id": part.id().to_string(),
            "label": part.display_label(),
            "kind": part.part_type.to_string(),
            "number": part.number,
            "language": content_language(content),
            "text": crate::exporter::slides::lyrics_for_reading(&content.content),
        }));
    }

    // `position`, `first` and `last` need the final count, so they are filled
    // in once all the parts are known.
    let total = parts.len();
    for (index, part) in parts.iter_mut().enumerate() {
        let object = part.as_object_mut().expect("built as an object above");
        object.insert("position".to_string(), json!(index + 1));
        object.insert("first".to_string(), json!(index == 0));
        object.insert("last".to_string(), json!(index + 1 == total));
    }

    // The language actually used, so a template can print it.
    let language = parts
        .first()
        .and_then(|part| part.get("language"))
        .cloned()
        .unwrap_or(Value::Null);
    context.insert("language".to_string(), language);
    context.insert("parts".to_string(), Value::Array(parts));

    Value::Object(context)
}

/// The language code of a lyrics content, or `null` when it carries none.
fn content_language(content: &crate::song::SongPartContent) -> Value {
    match &content.content_type {
        crate::song::SongPartContentType::Lyrics { language } => match language.code() {
            Some(code) => Value::String(code.to_string()),
            None => Value::Null,
        },
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::{import_song_from_file, song_yml};

    fn amazing_grace() -> Song {
        import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap()
    }

    // --- Plain text ------------------------------------------------------

    #[test]
    fn test_plain_text_is_title_then_verses() {
        let text = text_from_song(&amazing_grace(), &TextSettings::default()).unwrap();

        assert_eq!(
            text,
            "Amazing Grace\n\
             \n\
             Amazing grace, How sweet the sound\n\
             That saved a wretch like me.\n\
             I once was lost, but now I'm found,\n\
             Was blind, but now I see.\n\
             \n\
             Twas grace that taught my heart to fear,\n\
             And grace my fears relieved.\n\
             How precious did that grace appear\n\
             The hour I first believed.\n\
             \n\
             Through many dangers, toils and snares\n\
             I have already come,\n\
             'Tis grace has brought me safe thus far\n\
             And grace will lead me home."
        );
    }

    /// LilyPond syllable markers are for singing, not for reading.
    #[test]
    fn test_lilypond_markup_is_stripped() {
        let song = import_song_from_file("tests/data/Sei nicht stolz auf das, was du bist.song.yml")
            .unwrap();
        let text = text_from_song(&song, &TextSettings::default()).unwrap();

        assert!(text.contains("denn nur Gott gut und heilig ist."));
        assert!(!text.contains("hei -- lig"));
        assert!(!text.contains("\\set"));
    }

    /// The parts come out in singing order, so the refrain repeats.
    #[test]
    fn test_singing_order_is_used() {
        let song = import_song_from_file("tests/data/Sei nicht stolz auf das, was du bist.song.yml")
            .unwrap();
        let text = text_from_song(&song, &TextSettings::default()).unwrap();

        assert_eq!(
            text.matches("Denn wer sich rühmen will,").count(),
            3,
            "the refrain should follow each of the three verses:\n{}",
            text
        );
        // And it really alternates rather than being appended at the end.
        let first_verse = text.find("Sei nicht stolz").unwrap();
        let first_refrain = text.find("Denn wer sich").unwrap();
        let second_verse = text.find("Menschen suchen").unwrap();
        assert!(first_verse < first_refrain && first_refrain < second_verse);
    }

    // --- Markup ----------------------------------------------------------

    #[test]
    fn test_markdown() {
        let text =
            text_from_song(&amazing_grace(), &TextSettings::with_format(TextFormat::Markdown))
                .unwrap();

        assert!(text.starts_with("# Amazing Grace\n*John Newton*\n"));
        assert!(text.contains("Amazing grace, How sweet the sound"));
    }

    #[test]
    fn test_markdown_without_an_author() {
        let mut song = amazing_grace();
        song.remove_tag("author");

        let text =
            text_from_song(&song, &TextSettings::with_format(TextFormat::Markdown)).unwrap();

        // No stray emphasis marks where the author would have been.
        assert!(text.starts_with("# Amazing Grace\n\nAmazing grace"), "{}", text);
        assert!(!text.contains("**"));
    }

    #[test]
    fn test_telegram() {
        let text =
            text_from_song(&amazing_grace(), &TextSettings::with_format(TextFormat::Telegram))
                .unwrap();

        assert!(text.starts_with("**Amazing Grace**\n\nAmazing grace"), "{}", text);
    }

    #[test]
    fn test_custom_template() {
        let settings = TextSettings::with_format(TextFormat::Custom(
            "{{title}} [{{language}}]\n{{#each parts}}{{position}}. {{label}}: {{text}}\n{{/each}}"
                .to_string(),
        ));

        let text = text_from_song(&amazing_grace(), &settings).unwrap();
        assert!(text.starts_with("Amazing Grace [en]"));
        assert!(text.contains("1. verse.1: Amazing grace"));
        assert!(text.contains("2. verse.2: Twas grace"));
    }

    #[test]
    fn test_malformed_custom_template_is_reported() {
        let settings = TextSettings::with_format(TextFormat::Custom(
            "{{#if title}}never closed".to_string(),
        ));
        assert!(text_from_song(&amazing_grace(), &settings).is_err());
    }

    #[test]
    fn test_format_parsing() {
        assert_eq!(TextFormat::parse("markdown"), Some(TextFormat::Markdown));
        assert_eq!(TextFormat::parse("MD"), Some(TextFormat::Markdown));
        assert_eq!(TextFormat::parse(" telegram "), Some(TextFormat::Telegram));
        assert_eq!(TextFormat::parse("plain"), Some(TextFormat::Plain));
        assert_eq!(TextFormat::parse("nope"), None);
    }

    // --- Languages -------------------------------------------------------

    #[test]
    fn test_the_requested_language_is_used() {
        let yml = r#"
version: 0.1
title: Two Languages
default_language: en
parts:
  - type: stanza
    contents:
    - type: lyrics
      number: 1
      language: en
      content: Hello world
    - type: lyrics
      number: 1
      language: de
      content: Hallo Welt
"#;
        let song = song_yml::import_from_yml_string(yml).unwrap();

        // Without a request the song's default language wins.
        let text = text_from_song(&song, &TextSettings::default()).unwrap();
        assert!(text.contains("Hello world"), "{}", text);

        let german = TextSettings {
            language: Some("de".to_string()),
            ..TextSettings::default()
        };
        let text = text_from_song(&song, &german).unwrap();
        assert!(text.contains("Hallo Welt"), "{}", text);
        assert!(!text.contains("Hello world"));
    }

    /// A song that states no language at all still exports.
    #[test]
    fn test_song_without_language_information() {
        let song = import_song_from_file("tests/data/Amazing Grace.song").unwrap();
        let text = text_from_song(&song, &TextSettings::default()).unwrap();

        assert!(text.starts_with("Amazing Grace\n\nAmazing grace"), "{}", text);
    }

    // --- Several songs ---------------------------------------------------

    #[test]
    fn test_several_songs_in_one_document() {
        let songs = [
            amazing_grace(),
            import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli").unwrap(),
        ];

        let text =
            text_from_songs(&songs, &TextSettings::with_format(TextFormat::Markdown)).unwrap();

        assert!(text.contains("# Amazing Grace"));
        assert!(text.contains("# Weiß ich den Weg auch nicht (Pax Dei)"));
        // The first song comes first.
        assert!(text.find("# Amazing Grace") < text.find("# Weiß ich"));
    }

    #[test]
    fn test_custom_song_separator() {
        let songs = [amazing_grace(), amazing_grace()];
        let settings = TextSettings {
            song_separator: Some("---".to_string()),
            ..TextSettings::default()
        };

        let text = text_from_songs(&songs, &settings).unwrap();
        assert_eq!(text.matches("\n---\n").count(), 1);
    }

    #[test]
    fn test_no_songs_gives_no_output() {
        assert_eq!(text_from_songs(&[], &TextSettings::default()).unwrap(), "");
    }
}
