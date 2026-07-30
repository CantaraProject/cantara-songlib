# CCLI SongSelect import

[CCLI SongSelect](https://songselect.ccli.com) lets a licence holder download
the lyrics of a song as plain text. `src/importer/ccli.rs` reads those files
(here given the extension `.ccli`).

```rust,no_run
use cantara_songlib::importer::import_song_from_file;

let song = import_song_from_file("tests/data/Weiß ich den Weg auch nicht.ccli")?;
assert_eq!(song.tag("ccli_song_number").unwrap(), "5973691");
# Ok::<(), Box<dyn std::error::Error>>(())
```

or from the command line:

```bash
cantara-songlib "tests/data/Weiß ich den Weg auch nicht.ccli" presentation
```

## The format

```text
Weiß ich den Weg auch nicht (Pax Dei)   ← title block
                                        ← blank line
Vers 1                                  ← section heading
Weiß ich den Weg auch nicht, du weißt…  ← lyrics
das macht die Seele still und friedevoll.

Vers 2
…

CCLI-Liednummer 5973691                 ← trailer starts here
Hedwig Von Redern | John Bacchus Dykes
© Words: Public Domain
Music: Public Domain
CCLI-Lizenznummer 0000000
```

## Reading it in any language

SongSelect translates the file it hands out. The same song comes as
`Vers 1 / Refrain / CCLI-Liednummer` in German, `Verse 1 / Chorus / CCLI Song #`
in English, `Verso 1 / Coro / Número de Canción CCLI` in Spanish. The importer
therefore relies on the things that stay put:

| Signal | Why it is language-independent |
|--------|--------------------------------|
| The title is the first block | Position, not wording |
| Each following block is one section, its first line the heading | Position, not wording |
| The trailer starts at the first line mentioning `CCLI` | `CCLI` is a brand name and is never translated |
| `©` opens the copyright block | Punctuation, not a word |
| `\|` separates co-authors | Punctuation, not a word |
| A trailing number is the section number | Digits are written the same everywhere |

Because the *structure* is recovered without vocabulary, a song imports
completely even in a language the importer has never seen. Nothing is dropped.

### The one step that needs vocabulary

Only deciding whether a heading means "verse" or "chorus" needs to know words,
and that step is allowed to fail. `classify_heading` compares the heading
against a table covering the Latin-script languages SongSelect publishes in,
plus a few common CJK headings:

| Type | Recognised headings (excerpt) |
|------|-------------------------------|
| Verse | verse, vers, strophe, strofe, strofa, verso, estrofa, couplet, zwrotka, sloka, versszak, säkeistö, 主歌, 절 |
| Chorus | chorus, refrain, refrein, refrão, refren, refräng, omkvæd, coro, estribillo, ritornello, kertosäe, 副歌, 후렴, サビ |
| Pre-chorus | pre-chorus, pré-refrain, vorrefrain, pré-refrão, precoro, voorrefrein |
| Bridge | bridge, brücke, brug, brygga, bro, silta, puente, ponte, pont, most, híd, 桥段 |
| Intro / Outro / Interlude | intro, einleitung, vorspiel · outro, ending, schluss, coda · interlude, zwischenspiel, mellanspel |

Comparison happens on a normalised form — lower-cased, accents folded to ASCII,
separators removed — so one entry covers `Pre-Chorus`, `pre chorus` and
`PreChorus`, and `Brücke` matches whether or not the umlaut survived the
keyboard it was typed on.

**A heading that is not in the table still imports.** It produces a part typed
`SongPartType::Other` whose `label` holds the original wording:

```rust
use cantara_songlib::importer::ccli::import_from_ccli_string;
use cantara_songlib::song::SongPartType;

let song = import_from_ccli_string(
    "Lagu\n\nBagian Pertama\nbaris satu\n\nCCLI Song # 42\nPenulis\n",
).unwrap();

let part = song.part_at(0).unwrap();
assert_eq!(part.part_type, SongPartType::Other);
assert_eq!(part.label.as_deref(), Some("Bagian Pertama"));
```

The lyrics, the metadata and the section boundaries are all still correct; only
the automatic verse/refrain ordering is unavailable, because the importer cannot
know which block is the refrain. Adding a language means adding entries to the
`HEADINGS` table — nothing else in the importer changes.

## What ends up in the song

| Song field | Source |
|------------|--------|
| `title` | first line of the file |
| `tags["subtitle"]` | any further lines of the title block |
| parts | one per section block, in file order |
| `SongPart::label` | the section heading, verbatim |
| `tags["ccli_song_number"]` | first `CCLI …` line of the trailer |
| `tags["ccli_license_number"]` | last `CCLI …` line, when there is more than one |
| `tags["author"]` | the trailer line(s) before the copyright, `\|` replaced by `, ` |
| `tags["copyright"]` | from the `©` line to the end of the trailer |
| `part_orders` | guessed from the section types |

`default_language` is deliberately left unset. The heading table cannot identify
the language reliably — `Vers` is German, Dutch, Swedish, Norwegian and Danish;
`Refrain` is used in English too — and a wrong language code is worse than none,
because it would make the multi-language slide export pick the wrong text.

## Limits

* **No music.** SongSelect's text export has lyrics only, so the LilyPond and
  ABC exporters reject a CCLI song with *"Song has no voice content"*. Use it
  for presentation slides.
* **No chords.** The ChordPro-style export SongSelect also offers is a different
  format and is not read by this importer.
* **The title is kept verbatim**, including a parenthesised suffix such as
  `(Pax Dei)`. That suffix is the album or artist in some exports and part of
  the song's name in others, and guessing wrong would corrupt the title.
