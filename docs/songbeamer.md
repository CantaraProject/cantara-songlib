# SongBeamer import and export

[SongBeamer](https://www.songbeamer.de) is a widely used German presentation
program for churches. Its songs are plain text files with the extension `.sng`.
`src/importer/songbeamer.rs` reads them and `src/exporter/songbeamer.rs` writes
them, so a `.sng` file can be turned into slides, sheet music or Markdown, and
any song this crate can read can be handed to SongBeamer.

```rust,no_run
use cantara_songlib::importer::import_song_from_file;

let song = import_song_from_file("tests/data/Amazing Grace.sng")?;
assert_eq!(song.tag("author").unwrap(), "John Newton");
# Ok::<(), Box<dyn std::error::Error>>(())
```

From the command line:

```bash
cantara-songlib "tests/data/Amazing Grace.sng" presentation      # .sng → slides
cantara-songlib "tests/data/Amazing Grace.song.yml" songbeamer --output out.sng
```

## The format

```text
#LangCount=1                            ← header: one #Tag=value per line
#Title=Amazing Grace
#Author=John Newton
#(c)=Public Domain
#VerseOrder=Verse 1,Refrain,Verse 2     ← the singing order, if stated
---                                     ← the header ends; the body begins
Verse 1                                 ← a verse mark (optional)
Amazing grace! How sweet the sound
--                                      ← a slide break inside the same verse
I once was lost, but now am found;
---                                     ← the next block, i.e. the next part
Refrain
…
```

The format has no formal specification. This implementation follows the
[SongBeamer wiki](http://wiki.songbeamer.de/index.php?title=Song) for the
separators and verse marks, and OpenLP's `songbeamer.py` importer for the tag
list and the encoding rules.

### Separators

| Line | Meaning |
|------|---------|
| `---` | ends the block and starts a new [`SongPart`](data-model.md) |
| `--`  | ends the slide but not the part |

The data model has no notion of a slide, so a `--` becomes a **blank line**
inside the part's lyrics — which the slide exporter reads as a paragraph and
the SongBeamer exporter turns back into `--`.

### Verse marks

The first line of a block may name it: `Verse 2`, `Strophe 2`, `Refrain`,
`Chorus`, `Pre-Chorus`, `Bridge`, `Zwischenspiel`, `Coda`, `Teil 3`, … — one
keyword, optionally followed by a number, in German or English. A line that is
not a mark is lyrics, so a file that uses no marks at all still imports
completely: every block simply becomes a verse.

`$$M=Zwischenteil` is the format's custom mark. It names a section SongBeamer
has no keyword for; the importer keeps the wording as the part's label and
types the part as `Other`.

The number on the mark decides the part's number, so `Verse 3` becomes
`verse.3` whatever position the block is in.

### Metadata

Tags that describe *the song* are mapped onto this crate's names:

| SongBeamer | Song |
|------------|------|
| `#Title` | `title` |
| `#Author` | tag `author` |
| `#Melody` | tag `composer` |
| `#(c)` | tag `copyright` |
| `#NatCopyright` | tag `national_copyright` |
| `#CCLI` | tag `ccli_song_number` |
| `#Songbook=Book / 42` | tags `songbook` and `song_number` |
| `#ChurchSongID` | tag `church_song_id` |
| `#OTitle` | tag `original_title` |
| `#Bible`, `#Categories`, `#Keywords`, `#Translation`, `#Rights` | tags of the same name |
| `#Tempo`, `#Speed` | tag `tempo` |
| `#Comments` | tag `comments` (base64 in the file) |
| `#Chords` | tag `songbeamer_chords` (base64 in the file) |
| `#Key=F` | tag `key`, plus `score.key` in LilyPond spelling (`f major`) |
| `#VerseOrder` | the song's default [part order](data-model.md) |

Tags that describe SongBeamer's *presentation* — `#Font`, `#FontSize`,
`#BackgroundImage`, `#TextAlign`, … — are dropped. They mean nothing outside
the program, and keeping them would turn every song's metadata into a pile of
display settings.

The inline formatting tags a line may carry (`<b>`, `<i>`, `<color=…>`, …) are
stripped for the same reason. A `<` that is not one of them stays in the text.

### Encoding

SongBeamer accepts four encodings and tells them apart by the byte order mark:
UTF-8, UTF-16 little endian, UTF-16 big endian, and — with no BOM at all —
Windows-1252, which is what older versions wrote. Reading a `.sng` file as UTF-8
therefore either fails or mangles every umlaut.

Use the byte-level entry points; `import_song_from_file` already does:

```rust,no_run
use cantara_songlib::exporter::songbeamer::{sng_bytes_from_song, SngExportSettings};
use cantara_songlib::importer::songbeamer::import_from_bytes;

let song = import_from_bytes(&std::fs::read("tests/data/Amazing Grace.sng")?)?;

// Written back as UTF-8 with a BOM and CRLF line endings — what SongBeamer
// itself writes.
std::fs::write("out.sng", sng_bytes_from_song(&song, &SngExportSettings::default()))?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Several languages

With `#LangCount=2` the languages are interleaved **line by line** inside every
slide: first line the first language, second line its translation, and so on.
The importer splits them apart into one lyrics block per language and the
exporter interleaves them again.

Nothing in the file names its languages, so pass the codes in if you know them:

```rust,no_run
use cantara_songlib::importer::songbeamer::{import_from_file_with, SngImportSettings};
use std::path::Path;

let settings = SngImportSettings {
    languages: vec!["de".to_string(), "en".to_string()],
};
let song = import_from_file_with(Path::new("tests/data/Bilingual Test.sng"), &settings)?;

assert_eq!(song.available_languages(), ["de", "en"]);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Without them the first language becomes the unlabelled default and the others
are named `lang2`, `lang3`, … so that they still stay apart and can be renamed
later.

## What a round trip keeps, and what it loses

A `.sng` file that is imported and exported again comes back the same: title,
metadata, parts, marks, slide breaks and the stated singing order.

Going the other way — another format out as `.sng` — two things do not survive,
because the format has nowhere to put them:

* **Notation.** SongBeamer stores no melody. A song with a score keeps its
  lyrics and loses its music.
* **Headings that are not marks.** A part whose label is not a SongBeamer verse
  mark is written with the canonical mark for its type (`stanza` → `Verse 1`),
  since the type is what the singing order and every other exporter work from.
  Parts typed `Other` keep their wording through `$$M=`.

A guessed singing order is not written out at all: the rule-based orders are
this crate's idea of how a song is sung, and writing one into `#VerseOrder`
would turn a guess into a stated fact.
