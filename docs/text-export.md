# Text and markup export

Two things are often wanted from a song that are neither slides nor sheet music:
the bare lyrics as a text file, and the lyrics wrapped in markup to paste into a
chat or a document. Both are the same job — write the verses in singing order —
so both come from one exporter, differing only in the
[Handlebars](https://handlebarsjs.com/) template they use.

```bash
cantara-songlib "Amazing Grace.song.yml" text
cantara-songlib "Amazing Grace.song.yml" text --format markdown
cantara-songlib "Amazing Grace.song.yml" text --format telegram
```

## The built-in formats

### `plain` — the simple text export

The title, a blank line, then every part in singing order:

```text
Amazing Grace

Amazing grace, How sweet the sound
That saved a wretch like me.
I once was lost, but now I'm found,
Was blind, but now I see.

Twas grace that taught my heart to fear,
…
```

Nothing from the source format survives: no LilyPond syllable markers, no ABC
fields, no section headings.

### `markdown`

```markdown
# Amazing Grace
*John Newton*

Amazing grace, How sweet the sound
…
```

The author line is left out when the song has none, rather than leaving empty
emphasis marks behind.

### `telegram`

```text
**Amazing Grace**

Amazing grace, How sweet the sound
…
```

## Singing order

The parts come out in the order they are sung, which is what
`Song::ordered_parts` produces — so a refrain is written after every verse, not
once at the end:

```rust
use cantara_songlib::exporter::text::{text_from_song, TextSettings};
use cantara_songlib::importer::import_song_from_file;

let song = import_song_from_file("tests/data/O What A Savior That He Died For Me.song")?;
let text = text_from_song(&song, &TextSettings::default())?;

// Four verses, so the chorus appears four times.
assert_eq!(text.matches("Verily, verily,\" message ever new!").count(), 4);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Which language

`TextSettings::language` picks the language. Left at `None` it uses the song's
default language and falls back to whatever lyrics a part has, so a song with no
language information at all exports just as well as one with several.

```rust
use cantara_songlib::exporter::text::{text_from_song, TextSettings};
use cantara_songlib::importer::song_yml;

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
let song = song_yml::import_from_yml_string(yml)?;

let german = TextSettings { language: Some("de".to_string()), ..TextSettings::default() };
assert!(text_from_song(&song, &german)?.contains("Hallo Welt"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Your own template

`--template` takes a Handlebars template; in Rust it is `TextFormat::Custom`.

```bash
cantara-songlib song.yml text --template '## {{title}}
{{#each parts}}
**{{label}}**
{{text}}
{{/each}}'
```

### Variables

| Variable | Contents |
|----------|----------|
| `title` | the song's title |
| `language` | the language the lyrics were taken from, if the song stated one |
| `author`, `copyright`, `ccli_song_number`, … | every tag of the song |
| `parts` | the parts in singing order |

Each entry of `parts` offers:

| Field | Contents |
|-------|----------|
| `text` | the lyrics, ready to read |
| `label` | the heading the source gave it, e.g. `"Vers 1"`; the id when it gave none |
| `id` | `"verse.1"` |
| `kind` | `"verse"`, `"refrain"`, … |
| `number` | the part's number |
| `language` | the language this text is in, if stated |
| `position` | 1-based position in the singing order |
| `first`, `last` | flags for the first and last part |

`first` and `last` are what you need for separators without a trailing one:

```rust
use cantara_songlib::exporter::text::{text_from_song, TextFormat, TextSettings};
use cantara_songlib::importer::import_song_from_file;

let song = import_song_from_file("tests/data/Amazing Grace.song.yml")?;
let settings = TextSettings::with_format(TextFormat::Custom(
    "{{#each parts}}{{number}}{{#unless last}} · {{/unless}}{{/each}}".to_string(),
));

assert_eq!(text_from_song(&song, &settings)?, "1 · 2 · 3");
# Ok::<(), Box<dyn std::error::Error>>(())
```

A malformed template is reported rather than quietly producing nothing:

```rust
# use cantara_songlib::exporter::text::{text_from_song, TextFormat, TextSettings};
# use cantara_songlib::importer::import_song_from_file;
# let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
let broken = TextSettings::with_format(TextFormat::Custom("{{#if title}}oops".to_string()));
assert!(text_from_song(&song, &broken).is_err());
```

## Several songs at once

Every command accepts more than one file:

```bash
cantara-songlib a.song.yml b.ccli c.song text --format markdown
```

In Rust, `text_from_songs` renders them all with the same template and joins
them with `TextSettings::song_separator` (a blank line by default):

```rust
use cantara_songlib::exporter::text::{text_from_songs, TextFormat, TextSettings};
use cantara_songlib::importer::import_song_from_file;

let songs = [
    import_song_from_file("tests/data/Amazing Grace.song.yml")?,
    import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli")?,
];

let settings = TextSettings {
    format: TextFormat::Markdown,
    song_separator: Some("---".to_string()),
    ..TextSettings::default()
};

let document = text_from_songs(&songs, &settings)?;
assert!(document.contains("# Amazing Grace"));
assert!(document.contains("\n---\n"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Several songs as slides

The same works for a presentation. Each song becomes a `PresentationChapter`
that keeps a link back to the song it was built from:

```rust
use cantara_songlib::exporter::slides::chapters_from_songs;
use cantara_songlib::importer::import_song_from_file;
use cantara_songlib::slides::{LinkedEntity, SlideSettings};

let songs = [
    import_song_from_file("tests/data/Amazing Grace.song.yml")?,
    import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli")?,
];

let chapters = chapters_from_songs(&songs, &SlideSettings::default());
assert_eq!(chapters.len(), 2);

match &chapters[1].linked_entity {
    LinkedEntity::Song(song) => assert!(song.title.starts_with("Weiß ich")),
    other => panic!("unexpected link: {:?}", other),
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`slides_from_songs` gives the same slides as a flat run when the consumer does
not need the songs kept apart.

On the command line, more than one file makes `presentation` emit chapters
instead of a flat slide list; `--chapters` forces that shape for a single file
too:

```bash
cantara-songlib a.song.yml b.ccli presentation      # → chapters
cantara-songlib a.song.yml presentation --chapters  # → one chapter
cantara-songlib a.song.yml presentation             # → slides
```

The music formats (`lilypond`, `abc`) take exactly one file and say so if given
more.
