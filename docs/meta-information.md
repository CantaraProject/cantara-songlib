# Meta information on slides

A presentation can show a line of metadata — typically the title, the author and
a copyright notice — on selected slides of a song. Two settings control it, both
on `SlideSettings`:

| Setting | Meaning |
|---------|---------|
| `meta_syntax` | A [Handlebars](https://handlebarsjs.com/) template for the line |
| `show_meta_information` | Which slides it appears on |

```rust
use cantara_songlib::slides::{ShowMetaInformation, SlideSettings};

let settings = SlideSettings {
    meta_syntax: "{{title}} ({{author}})".to_string(),
    show_meta_information: ShowMetaInformation::first_and_last_slide(),
    ..SlideSettings::default()
};
```

From the command line:

```bash
cantara-songlib song.ccli presentation \
    --meta-syntax '{{title}} — {{author}}' \
    --show-meta title,last
```

## The template

The template is compiled once per song and then rendered for every slide that
shows it. Compiling up front means a syntax error is reported rather than
quietly producing empty metadata:

```rust
use cantara_songlib::templating::MetaTemplate;

assert!(MetaTemplate::parse("{{title}} ({{author}})").is_ok());
assert!(MetaTemplate::parse("{{#if author}}never closed").is_err());
```

The command line checks the template before doing any work, so a typo is
reported immediately.

### Available variables

Every tag the importer found, plus `title`:

| Variable | Comes from |
|----------|------------|
| `title` | `Song::title` — always, even when a tag of the same name exists |
| `author` | `#author:` in a `.song` file, the authors line of a CCLI export, `tags:` in YAML |
| `copyright` | the `©` block of a CCLI export |
| `ccli_song_number`, `ccli_license_number` | the trailer of a CCLI export |
| anything else | whatever tags the file carried |

An unknown placeholder renders as nothing, so one template works across a
library of songs whose files carry different metadata:

```rust
use cantara_songlib::templating::MetaTemplate;
use cantara_songlib::song::Song;

let song = Song::new("A Song Without An Author");
let template = MetaTemplate::parse("{{title}} ({{author}})").unwrap();
assert_eq!(template.render_song(&song).unwrap(), "A Song Without An Author ()");
```

Use a conditional when the punctuation should disappear too:

```rust
# use cantara_songlib::templating::MetaTemplate;
# use cantara_songlib::song::Song;
let template = MetaTemplate::parse("{{title}}{{#if author}} ({{author}}){{/if}}").unwrap();

let mut song = Song::new("Amazing Grace");
assert_eq!(template.render_song(&song).unwrap(), "Amazing Grace");

song.set_tag("author", "John Newton");
assert_eq!(template.render_song(&song).unwrap(), "Amazing Grace (John Newton)");
```

A template that renders to nothing at all produces no meta line, rather than an
empty one taking up room on the slide.

Output is **not** HTML-escaped — slides are plain text, so a song called
`Rock & Roll` shows up as `Rock & Roll` and not `Rock &amp; Roll`.

## Where it appears

`ShowMetaInformation` holds three independent flags:

```rust
use cantara_songlib::slides::ShowMetaInformation;

// Named constructors for the usual combinations
let show = ShowMetaInformation::first_and_last_slide();
assert!(show.on_first_slide() && show.on_last_slide());
assert!(!show.on_title_slide());

// Or set them individually
let show = ShowMetaInformation {
    title_slide: true,
    first_slide: false,
    last_slide: true,
};
assert!(show.on_title_slide());
```

| Constructor | Title slide | First content slide | Last content slide |
|-------------|:-----------:|:-------------------:|:------------------:|
| `none()` | | | |
| `title_slide()` | ● | | |
| `first_slide()` | | ● | |
| `last_slide()` | | | ● |
| `first_and_last_slide()` | | ● | ● |
| `all()` | ● | ● | ● |

Two details worth knowing:

* The **title slide is its own position**. Asking for the metadata on the
  content slides leaves the title slide clean, and the other way round.
* A song with a **single content slide** has that slide be both the first and
  the last, so it shows the metadata if either position is selected — once.

The trailing empty slide that `empty_last_slide` appends is not a content slide
and never carries metadata.

## From C

`create_presentation_from_file_c` takes the positions as a bit mask:

| Bit | Position |
|-----|----------|
| 0 (`1`) | first content slide |
| 1 (`2`) | last content slide |
| 2 (`4`) | title slide |

So `3` means first and last, `4` means the title slide only and `7` means
everywhere. Values `0`–`3` keep the meaning they had before the title slide
became selectable, so existing callers need no change.
