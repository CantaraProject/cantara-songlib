# Complex slides: notation and several languages

A complex slide stacks several views of the *same passage* of a song: the melody
as notation, and the lyrics in as many languages as you ask for.

```text
┌──────────────────────────────────────────────────────┐
│  ♪───────────────────────────────────────────         │
│    A-ma-zing grace, How sweet the sound               │   ← notation, one
│  ♪───────────────────────────────────────────         │     system per
│    That saved a wretch like me.                       │     lyrics line,
│  ♪───────────────────────────────────────────         │     words included
│    I once was lost, but now I'm found,                │
│  ♪───────────────────────────────────────────         │
│    Was blind, but now I see.                          │
│                                                        │
│  en:  Amazing grace, How sweet the sound …            │   ← lyrics row
│  de:  Oh teure Gnade wunderbar …                      │   ← lyrics row
│                                                        │
│  Twas grace that taught my heart to fear, …           │   ← spoiler
└──────────────────────────────────────────────────────┘
```

The notation row is a complete ABC tune:

```abc
X:1
M:3/4
L:1/4
K:F
C | F2 (A/ F/) | A2 G | F2 D | C2
w:A-ma-zing grace, How sweet the sound
C | F2 (A/ F/) | A2 G | c3- | c2
w:That saved a wretch like me.
A | c2 (A/ F/) | A2 G | F2 D | C2
w:I once was lost, but now I'm found,
C | F2 (A/ F/) | A2 G | F3 |]
w:Was blind, but now I see.
```

## Asking for a layout

```rust
use cantara_songlib::slides::{LanguageConfiguration, SlideElement, SlideSettings};

let settings = SlideSettings {
    language: LanguageConfiguration::Complex(vec![
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
        SlideElement::Lyrics("de".to_string()),
    ]),
    max_lines: Some(2),
    ..SlideSettings::default()
};
```

From the command line:

```bash
cantara-songlib "Amazing Grace.song.yml" presentation --show notation,en,de --max-lines 2
```

`--show` accepts `notation` (also `abc`, `noten`, `score`, `music`) for the
melody; anything else is taken as a language code. The rows appear in the order
given.

## The guarantee: notes match words

The notation row spans **exactly the lyrics lines printed below it**, and it
carries those words itself: one music system per lyrics line, each followed by
its own `w:` line. The notation therefore breaks where the song's text breaks,
and a slide showing four lines of text shows four systems.

The words under the notes are those of the **first requested language** — the
one the notation is written for. Asking for `notation,de,en` instead of
`notation,en,de` puts the German under the staff.

That is the point of this slide type, and it holds through wrapping:

```rust
use cantara_songlib::exporter::abc::{AbcSettings, PartPhrases};
use cantara_songlib::importer::import_song_from_file;

let song = import_song_from_file("tests/data/Amazing Grace.song.yml")?;
let verse = song.part(&"verse.1".parse().unwrap()).unwrap();
let phrases = PartPhrases::of(&song, verse, &AbcSettings::default()).unwrap();

// Four lyrics lines, so four phrases.
assert_eq!(phrases.len(), 4);
// "Amazing grace, How sweet the sound" is eight syllables …
assert_eq!(phrases.syllables(0), 8);
// … and the first two lines together are fourteen.
assert_eq!(phrases.syllables_in(0..2), 14);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The melody is cut along the lyrics lines by counting syllables against notes.
A note that continues a tie or sits inside a slur is a melisma and stays with
the preceding syllable, so the counts line up even where several notes are sung
on one syllable.

Each notation row is a **complete ABC tune** with its own `X:`, `M:`, `L:` and
`K:` fields, so it can be handed to an ABC renderer as-is. It carries no `T:`
title, which a renderer would print above the staff. Accidentals are resolved
against the key signature from scratch, which is right for an excerpt: a sharp
that an earlier bar established is written out again rather than assumed.

### When the words and the notes do not fit

A translation rarely has the same number of syllables as the original. Because
every system carries its own `w:` line, a mismatch stays local:

* **Fewer syllables than notes** — the line is padded with ABC's `*` skip
  marker, so the remaining words stay under the right notes.
* **More syllables than notes** — the line is written out in full. The surplus
  cannot push the next line out of place, and cutting it would show a mangled
  word.

`SlideRowKind::Notation { syllables }` reports how many syllables the row
covers, so a frontend can check the alignment itself.

## Text that is shown twice

The notation carries the words of the first requested language under its notes,
so the lyrics row for that language repeats them. Both rows are produced — a
layout may well want the text again in a size the congregation can read from the
back — but the repeat is flagged:

```rust
# use cantara_songlib::exporter::slides::slides_from_song;
# use cantara_songlib::importer::import_song_from_file;
# use cantara_songlib::slides::*;
# let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
# let settings = SlideSettings {
#     language: LanguageConfiguration::Complex(vec![
#         SlideElement::Notation,
#         SlideElement::Lyrics("en".to_string()),
#     ]),
#     title_slide: false, empty_last_slide: false,
#     ..SlideSettings::default()
# };
let slides = slides_from_song(&song, &settings);
let SlideContent::Complex(slide) = &slides[0].slide_content else { panic!() };

assert!(!slide.rows[0].redundant);   // the notation
assert!(slide.rows[1].redundant);    // its words, printed again

// A layout that does not want the repetition:
assert_eq!(slide.rows_without_repetition().count(), 1);
```

The flag follows the first requested language, so `--show notation,de,en` marks
the German row and leaves the English one alone. Nothing is flagged when there
is no notation row — because none was asked for, or because the song has no
melody — and spoiler rows are never flagged, since a spoiler carries no
notation.

## Wrapping

`max_lines` limits how many lyrics lines go on one slide, and the notation
follows:

| `max_lines` | Slides for a four-line verse | Systems per slide |
|-------------|------------------------------|-------------------|
| `None` | 1 | 4 |
| `Some(2)` | 2 | 2 |
| `Some(1)` | 4 | 1 |

The number of lines a part has comes from the longest of the requested
languages, so all rows of a slide are cut at the same place.

## Songs without language information

A classic `.song` file states no language at all. Its text is shown in place of
the **first requested language**:

```rust
use cantara_songlib::exporter::slides::slides_from_song;
use cantara_songlib::importer::import_song_from_file;
use cantara_songlib::slides::*;

let song = import_song_from_file("tests/data/Amazing Grace.song")?;
let settings = SlideSettings {
    language: LanguageConfiguration::Complex(vec![
        SlideElement::Notation,
        SlideElement::Lyrics("en".to_string()),
        SlideElement::Lyrics("de".to_string()),
    ]),
    title_slide: false,
    empty_last_slide: false,
    ..SlideSettings::default()
};

let slides = slides_from_song(&song, &settings);
let SlideContent::Complex(first) = &slides[0].slide_content else { panic!() };

// One lyrics row, not two: the text is not repeated under "de".
let lyrics: Vec<_> = first.rows.iter().filter(|row| !row.is_notation()).collect();
assert_eq!(lyrics.len(), 1);

// And it reports "no language stated" rather than claiming to be English.
assert_eq!(lyrics[0].kind, SlideRowKind::Lyrics { language: None });
# Ok::<(), Box<dyn std::error::Error>>(())
```

Two details follow from that:

* The fallback belongs to the first requested **language**, which is not the
  first row when notation comes first.
* Later languages do **not** fall back. Repeating the same text under a second
  heading would claim a translation that does not exist.

A language the song does not have simply gets no row — never an empty one.
A song with no melody, such as a CCLI SongSelect export, gets no notation row.

## Spoilers

The spoiler previews the next slide in **every** requested language:

```rust
# use cantara_songlib::exporter::slides::slides_from_song;
# use cantara_songlib::importer::import_song_from_file;
# use cantara_songlib::slides::*;
# let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
# let settings = SlideSettings {
#     language: LanguageConfiguration::Complex(vec![SlideElement::Lyrics("en".to_string())]),
#     title_slide: false, empty_last_slide: false, show_spoiler: true,
#     ..SlideSettings::default()
# };
let slides = slides_from_song(&song, &settings);
let SlideContent::Complex(first) = &slides[0].slide_content else { panic!() };

assert!(first.spoiler[0].content.contains("Twas grace"));
// A spoiler is text only — the notation is not repeated.
assert!(first.spoiler.iter().all(|row| !row.is_notation()));
```

Set `show_spoiler: false` to switch it off.

## Meta information

Handled exactly as on a simple slide: `meta_syntax` supplies the template and
`show_meta_information` decides which slides carry it. See
[meta-information.md](meta-information.md).

## Multiple languages in a YAML song

Lyrics entries sharing a `number` are the same verse in different languages and
end up on one part; different numbers are different verses:

```yaml
parts:
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: c4 d e f | g2 g2
    - type: lyrics
      number: 1          # verse 1, English
      language: en
      content: A -- ma -- zing grace how sweet
    - type: lyrics
      number: 1          # verse 1, German — same number, same verse
      language: de
      content: Oh teu -- re Gna -- de wun -- der
    - type: lyrics
      number: 2          # verse 2
      language: en
      content: Twas grace that taught my heart
```
