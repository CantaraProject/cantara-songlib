//! Rendering the meta information line of a presentation.
//!
//! A presentation can show a line of metadata — typically the title, the author
//! and a copyright notice — on selected slides of a song. What that line looks
//! like is up to the user, who supplies a
//! [Handlebars](https://handlebarsjs.com/) template such as
//!
//! ```text
//! {{title}} ({{author}})
//! ```
//!
//! The template is compiled once per song into a [`MetaTemplate`] and then
//! rendered for each slide that is meant to show it. Which slides those are is
//! decided by [`crate::slides::ShowMetaInformation`].

use std::collections::BTreeMap;

use handlebars::{Handlebars, TemplateError};
use serde::Serialize;

use crate::song::Song;

/// The name the template is registered under. Never visible to the user.
const TEMPLATE_NAME: &str = "meta";

/// A compiled meta information template.
///
/// Compiling once and rendering many times means a syntax error is reported
/// when the template is supplied rather than silently swallowed on every slide.
///
/// ```
/// use cantara_songlib::templating::MetaTemplate;
/// use cantara_songlib::song::Song;
///
/// let mut song = Song::new("Amazing Grace");
/// song.set_tag("author", "John Newton");
///
/// let template = MetaTemplate::parse("{{title}} ({{author}})").unwrap();
/// assert_eq!(template.render_song(&song).unwrap(), "Amazing Grace (John Newton)");
/// ```
pub struct MetaTemplate {
    registry: Handlebars<'static>,
    /// True when the source was blank, i.e. the user wants no meta line at all.
    blank: bool,
}

impl MetaTemplate {
    /// Compile a template.
    ///
    /// An empty or whitespace-only template is valid and means "show no meta
    /// information"; [`MetaTemplate::render`] then always yields `None`.
    ///
    /// # Errors
    /// A [`TemplateError`] if the template is malformed, e.g. an unclosed
    /// `{{#if}}` block. Reporting this is the point of compiling up front:
    /// the user gets told about the typo instead of quietly seeing no metadata.
    ///
    /// ```
    /// use cantara_songlib::templating::MetaTemplate;
    /// assert!(MetaTemplate::parse("{{title}}").is_ok());
    /// assert!(MetaTemplate::parse("{{#if author}}unclosed").is_err());
    /// ```
    pub fn parse(source: &str) -> Result<MetaTemplate, TemplateError> {
        let mut registry = Handlebars::new();

        // Slides are plain text, not HTML. Without this a song called
        // "Rock & Roll" would be shown as "Rock &amp; Roll".
        registry.register_escape_fn(handlebars::no_escape);
        // An unknown placeholder renders as nothing rather than failing, so a
        // template mentioning {{author}} still works for a song without one.
        registry.set_strict_mode(false);

        registry.register_template_string(TEMPLATE_NAME, source)?;

        Ok(MetaTemplate {
            registry,
            blank: source.trim().is_empty(),
        })
    }

    /// Whether this template can never produce any output.
    pub fn is_blank(&self) -> bool {
        self.blank
    }

    /// Render the template against a set of variables.
    ///
    /// Any serialisable value works as the variable source — a `BTreeMap`, a
    /// `HashMap` or a struct of your own — because that is all Handlebars
    /// needs.
    ///
    /// Returns `None` when the result is empty — a template of `"{{author}}"`
    /// for a song without an author has nothing to show, and an empty meta line
    /// should not take up room on the slide.
    ///
    /// ```
    /// use cantara_songlib::templating::MetaTemplate;
    /// use std::collections::BTreeMap;
    ///
    /// let mut variables = BTreeMap::new();
    /// variables.insert("author".to_string(), String::new());
    ///
    /// let template = MetaTemplate::parse("{{author}}").unwrap();
    /// assert_eq!(template.render(&variables), None);
    /// ```
    pub fn render<V: Serialize>(&self, variables: &V) -> Option<String> {
        if self.blank {
            return None;
        }

        match self.registry.render(TEMPLATE_NAME, variables) {
            Ok(rendered) => {
                let trimmed = rendered.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            // Compilation succeeded, so a render failure can only come from a
            // helper misbehaving. Treat it like an empty result.
            Err(_) => None,
        }
    }

    /// Render the template for a song.
    ///
    /// The variables available to the template are every tag of the song plus
    /// `title`. See [`variables_for_song`].
    pub fn render_song(&self, song: &Song) -> Option<String> {
        self.render(&variables_for_song(song))
    }
}

/// The variables a meta template can use for a given song.
///
/// These are the song's tags — whatever the importer found, e.g. `author`,
/// `copyright`, `ccli_song_number` — plus `title`, which always reflects
/// [`Song::title`] and therefore wins over a tag of the same name.
///
/// ```
/// use cantara_songlib::templating::variables_for_song;
/// use cantara_songlib::song::Song;
///
/// let mut song = Song::new("Amazing Grace");
/// song.set_tag("author", "John Newton");
///
/// let variables = variables_for_song(&song);
/// assert_eq!(variables["title"], "Amazing Grace");
/// assert_eq!(variables["author"], "John Newton");
/// ```
pub fn variables_for_song(song: &Song) -> BTreeMap<String, String> {
    let mut variables = song.tags().clone();
    variables.insert("title".to_string(), song.title.clone());
    variables
}

/// Compile and render a template in one step.
///
/// Convenient for a one-off; prefer [`MetaTemplate`] when rendering the same
/// template more than once, which is what the slide exporters do.
///
/// # Errors
/// A [`TemplateError`] if the template is malformed.
pub fn render_metadata<V: Serialize>(
    template_string: &str,
    metadata: &V,
) -> Result<String, TemplateError> {
    Ok(MetaTemplate::parse(template_string)?
        .render(metadata)
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variables(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn test_render_metadata() {
        let data = variables(&[("title", "Amazing Grace"), ("author", "John Newton")]);

        assert_eq!(
            render_metadata("{{title}} ({{author}})", &data).unwrap(),
            "Amazing Grace (John Newton)"
        );
    }

    /// An unknown placeholder must not blow up the whole line.
    #[test]
    fn test_unknown_placeholder_renders_as_nothing() {
        let data = variables(&[("title", "Amazing Grace")]);
        assert_eq!(
            render_metadata("{{title}} ({{nonexisting}})", &data).unwrap(),
            "Amazing Grace ()"
        );
    }

    /// Slides are plain text — HTML escaping would corrupt titles.
    #[test]
    fn test_text_is_not_html_escaped() {
        let data = variables(&[("title", "Rock & Roll"), ("author", "O'Brien <arr.>")]);
        assert_eq!(
            render_metadata("{{title}} — {{author}}", &data).unwrap(),
            "Rock & Roll — O'Brien <arr.>"
        );
    }

    #[test]
    fn test_malformed_template_is_reported() {
        let error = MetaTemplate::parse("{{#if author}}never closed");
        assert!(error.is_err(), "an unclosed block should not compile");
    }

    #[test]
    fn test_blank_template_renders_nothing() {
        let data = variables(&[("title", "Amazing Grace")]);

        for source in ["", "   ", "\n"] {
            let template = MetaTemplate::parse(source).unwrap();
            assert!(template.is_blank());
            assert_eq!(template.render(&data), None);
        }
    }

    /// A template whose placeholders are all missing has nothing to show, and
    /// an empty meta line should not be put on a slide.
    #[test]
    fn test_template_without_any_content_yields_none() {
        let data = variables(&[("title", "Amazing Grace")]);
        let template = MetaTemplate::parse("{{author}}").unwrap();
        assert_eq!(template.render(&data), None);

        // Surrounding whitespace does not count as content either.
        let template = MetaTemplate::parse("  {{author}}  ").unwrap();
        assert_eq!(template.render(&data), None);
    }

    #[test]
    fn test_conditionals_work() {
        let template =
            MetaTemplate::parse("{{title}}{{#if author}} ({{author}}){{/if}}").unwrap();

        assert_eq!(
            template.render(&variables(&[("title", "A"), ("author", "B")])),
            Some("A (B)".to_string())
        );
        assert_eq!(
            template.render(&variables(&[("title", "A")])),
            Some("A".to_string())
        );
    }

    #[test]
    fn test_variables_for_song() {
        let mut song = Song::new("The Title");
        song.set_tag("author", "The Author");
        // A tag called "title" must not shadow the song's real title.
        song.set_tag("title", "Wrong");

        let variables = variables_for_song(&song);
        assert_eq!(variables["title"], "The Title");
        assert_eq!(variables["author"], "The Author");
    }

    #[test]
    fn test_render_song() {
        let mut song = Song::new("Weiß ich den Weg auch nicht");
        song.set_tag("author", "Hedwig Von Redern");
        song.set_tag("ccli_song_number", "5973691");

        let template = MetaTemplate::parse("{{title}} — {{author}} (CCLI {{ccli_song_number}})")
            .unwrap();
        assert_eq!(
            template.render_song(&song).unwrap(),
            "Weiß ich den Weg auch nicht — Hedwig Von Redern (CCLI 5973691)"
        );
    }
}
