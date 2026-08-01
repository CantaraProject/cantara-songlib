# The song data model

Everything in this crate revolves around one type: `Song`. Importers produce
one, exporters consume one. The model knows about *structure* and *content*, but
nothing about any particular file format.

```text
Song
├── title
├── default_language          which language unlabelled lyrics are in
├── score                     key / time signature / anacrusis
├── tags                      free-form metadata (author, copyright, …)
├── parts: Vec<SongPart>      every distinct block, stored exactly once
│   └── SongPart
│       ├── part_type         Verse | Refrain | Chorus | Bridge | …
│       ├── number            distinguishes parts of the same type
│       ├── contents          melodies, chord lines, lyrics per language
│       └── is_repetition_of  Option<SongPartId>
└── part_orders: Vec<PartOrder>   how the parts are strung together
```

## Parts are owned, references are ids

A `Song` owns its parts in a plain `Vec`. Anything that needs to *refer* to a
part — a custom singing order, or a verse that reuses another verse's melody —
stores a `SongPartId` instead of a pointer.

That has three consequences worth knowing:

* **Serialisation round-trips.** A part appears once in the JSON/YAML output no
  matter how often it is referenced, and reading the file back gives you the
  same song.
* **`Song` is `Send + Sync`.** It can be moved between threads, which matters
  for the GUI application that consumes this crate.
* **No runtime borrow errors.** Access goes through `song.part(&id)` and
  `song.part_mut(&id)`, checked by the compiler.

## Part ids

An id is a value made of a type and a number, rendered as `verse.1`,
`refrain.2`, and so on.

```rust
use cantara_songlib::song::{SongPartId, SongPartType};

let id: SongPartId = "Stanza.2".parse().unwrap();
assert_eq!(id.part_type, SongPartType::Verse);
assert_eq!(id.number, 2);
assert_eq!(id.to_string(), "verse.2");
```

Parsing is case-insensitive and understands the aliases the supported formats
use, so `Stanza.1`, `stanza.1` and `verse.1` all name the same part. Formatting
always produces the canonical lower-case spelling. The whole string has to be an
id — no leading or trailing text is accepted.

Ids are unique within a song. `Song::add_part` reports a
`SongError::DuplicatePartId` if you try to break that;
`Song::add_part_of_type` sidesteps it by picking the next free number.

Because a part's id is *derived* from its type and number, it can never drift
out of sync with the part it names:

```rust
use cantara_songlib::song::{SongPart, SongPartId, SongPartType};

let mut part = SongPart::new(SongPartId::new(SongPartType::Verse, 1));
assert_eq!(part.id().to_string(), "verse.1");

part.part_type = SongPartType::Refrain;
assert_eq!(part.id().to_string(), "refrain.1");   // no separate update step
```

## Content

A part holds every voice, chord line and language variant of its lyrics side by
side. Which of them to use is the exporter's decision.

```rust
use cantara_songlib::song::*;

let mut part = SongPart::new(SongPartId::new(SongPartType::Verse, 1));
part.add_content(SongPartContent::new(SongPartContentType::LeadVoice, "c4 d e f"));
part.add_content(SongPartContent::lyrics(LyricLanguage::specific("en"), "Hello"));
part.add_content(SongPartContent::lyrics(LyricLanguage::specific("de"), "Hallo"));
```

`SongPartContentType::is_voice()` tells melodies apart from lyrics and chords,
which is what the sheet-music exporters key off.

### Repeated melodies

Verse 2 usually shares verse 1's melody. Rather than copying the notes, the
later verse points at the earlier one:

```rust
# use cantara_songlib::song::*;
# let mut song = Song::new("Example");
# let first = song.add_part_of_type(SongPartType::Verse, None);
# song.part_mut(&first).unwrap().add_content(
#     SongPartContent::new(SongPartContentType::LeadVoice, "c4 d e f"));
let second = song.add_part_of_type(SongPartType::Verse, None);
song.part_mut(&second).unwrap().is_repetition_of = Some(first);

// Resolving follows the chain and is safe against cycles and dangling ids.
let part = song.part(&second).unwrap();
assert_eq!(song.voice_for_part(part).unwrap().content, "c4 d e f");
```

`Song::voice_for_part` walks `is_repetition_of` until it finds a melody. A
broken or circular chain yields `None` rather than looping forever.

## Multiple languages

Lyrics carry a `LyricLanguage`, either `Default` (no language stated — see
`Song::default_language`) or `Specific("en")`. Codes are normalised to lower
case on construction, so `"EN"` and `"en"` compare equal.

Use `SongPart::lyrics_for` rather than searching the content list yourself; it
implements the fallback chain:

1. the requested language, counting a `Default` block when the song's default
   language *is* that language;
2. the song's default language;
3. an unlabelled block;
4. whatever lyrics the part has.

```rust
# use cantara_songlib::song::*;
# let mut part = SongPart::new(SongPartId::new(SongPartType::Verse, 1));
# part.add_content(SongPartContent::lyrics(LyricLanguage::specific("en"), "Hello"));
# part.add_content(SongPartContent::lyrics(LyricLanguage::specific("de"), "Hallo"));
assert_eq!(part.lyrics_for(Some("de"), None).unwrap().content, "Hallo");
// An unknown language falls back instead of returning nothing.
assert_eq!(part.lyrics_for(Some("fr"), Some("en")).unwrap().content, "Hello");
```

`Song::available_languages()` lists every language that appears on a `Specific`
block, sorted and deduplicated.

## Metadata

`Song::tags` is free-form key/value metadata for humans: author, copyright, a
bible reference. It is a `BTreeMap`, so serialising the same song twice produces
byte-identical output.

Notation settings live apart from it, in `Song::score`:

| Field     | Meaning                                               |
|-----------|-------------------------------------------------------|
| `key`     | key signature in LilyPond spelling, e.g. `"f major"`  |
| `time`    | time signature, e.g. `"3/4"`                          |
| `partial` | anacrusis as a note denominator; `4` = quarter pickup |

They are separate because they are not metadata: they have a fixed meaning and
the sheet-music exporters depend on them.

## Orders

A song can be sung in more than one way, so `part_orders` is a list. The first
entry is the default, and `Song::ordered_parts()` uses it.

| Rule                          | Meaning                                                        |
|-------------------------------|----------------------------------------------------------------|
| `VerseRefrainBridgeRefrain`   | verse, refrain, verse, refrain, …, bridge, final refrain        |
| `RefrainVerseBridgeRefrain`   | as above, but the refrain opens the song                        |
| `Custom(Vec<SongPartId>)`     | an explicit list; a part may appear more than once              |

The two named rules resolve parts by *type*, not by their position in the part
list. That matters because the formats disagree: a classic `.song` file
interleaves the refrain between the verses, while a `.song.yml` file lists all
verses first and the refrain last. Both produce the same singing order.

`Chorus` and `Refrain` are the same thing musically — the classic format uses
one name and the YAML format the other. `SongPartType::is_chorus_like()` and
`Song::chorus_like_parts()` exist so that structural code never has to care
which one it is looking at.

```rust
# use cantara_songlib::song::*;
# let mut song = Song::new("Example");
# for text in ["one", "two"] {
#     let id = song.add_part_of_type(SongPartType::Verse, None);
#     song.part_mut(&id).unwrap().add_content(
#         SongPartContent::lyrics(LyricLanguage::Default, text));
# }
# let id = song.add_part_of_type(SongPartType::Refrain, None);
# song.part_mut(&id).unwrap().add_content(
#     SongPartContent::lyrics(LyricLanguage::Default, "ref"));
song.add_guessed_part_order();

let sung: Vec<String> = song.ordered_parts().iter().map(|p| p.id().to_string()).collect();
assert_eq!(sung, ["verse.1", "refrain.1", "verse.2", "refrain.1"]);
```

`Song`: https://docs.rs/cantara-songlib/latest/cantara_songlib/song/struct.Song.html
