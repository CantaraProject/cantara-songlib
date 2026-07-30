# ABC notation export

[ABC](https://abcnotation.com/) is a text-based music notation format supported
by `abcm2ps`, `abc2svg`, `abcjs` and others. The exporter in
`src/exporter/abc.rs` turns a `Song` into a complete ABC tune: melody, bar
lines, and one lyrics line per verse aligned to the notes.

## From the command line

```bash
cantara-songlib "tests/data/Amazing Grace.song.yml" abc
```

| Option                | Default | Meaning                                        |
|-----------------------|---------|------------------------------------------------|
| `-u`, `--unit-note-length` | `1/4` | The `L:` field. `1/8` makes quarter notes `2`. |
| `--all-verses`        | `true`  | Set to `false` to export only the first verse.  |
| `--include-chords`    | `true`  | Reserved — see *Not supported yet* below.       |

## From Rust

```rust,no_run
use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::importer::song_yml;

let content = std::fs::read_to_string("tests/data/Amazing Grace.song.yml")?;
let song = song_yml::import_from_yml_string(&content)?;
let abc = abc_from_song(&song, &AbcSettings::default())?;
println!("{}", abc);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Three entry points are available:

* `abc_from_song` — the normal export: music broken into lines, with the
  matching `w:` lyrics lines underneath each one.
* `abc_from_song_separated` — the whole melody on one line, lyrics stacked
  below. Useful when the caller wants to re-flow the music itself.
* `abc_part_from_song` — a standalone tune for a single part.

## How the conversion works

The voice content stored in a song is written in LilyPond syntax, so the
exporter has to translate it. It does that in three stages.

**1 — Parse.** The LilyPond input becomes a list of musical events. This
resolves everything LilyPond leaves implicit:

| LilyPond                | ABC        | Note                                             |
|-------------------------|------------|--------------------------------------------------|
| `\relative c'` octaves  | `C`, `c`, `c'` | Octave marks are resolved to absolute pitches. |
| `c8 d e`                | `C/ D/ E/` | A note without a duration reuses the previous one. |
| `a2.`                   | `A3`       | Dotted values become multiples of `L:`, never `2.`. |
| `fis`, `bes`, `as`      | `^F`, `_B`, `_A` | Accidentals are emitted only where the key signature does not already supply them. |
| `c2~ c2`                | `C2- C2`   | Ties.                                            |
| `a8( f)`                | `(A/ F/)`  | ABC puts the parenthesis *before* the first note. |
| `r4`, `s4`              | `z`, `x`   | Rests and spacer rests.                          |
| `\bar "|."`             | `|]`       | Final bar line.                                  |
| `\breathe`, `\set …`    | —          | Dropped.                                          |

**2 — Group into syllable slots.** A note that continues a tie, or sits inside
a slur, is a melisma and belongs to the preceding syllable. Counting slots this
way makes them directly comparable with a syllable count.

**3 — Lay out.** The music is split into lines along the lyric lines of the
first verse, and every verse gets its own `w:` line underneath.

## Things worth knowing

**Bar lines are inserted where the source omits them.** LilyPond treats `|` as
an optional bar *check*, so hand-written melodies often leave some out. In ABC a
bar line is structural — a missing one merges two measures — so the exporter
fills them in from the meter, respecting the anacrusis given by `score.partial`.

**Blank lines are never emitted inside a tune.** In ABC an empty line ends the
tune; everything after it is ignored. Sections are separated by a `%` comment
line instead.

**Refrains.** A refrain with its own melody becomes a separate section, marked
with the ABC annotation `"^Refrain"` above the first note. A refrain that shares
the verse melody is added as one more lyrics line labelled `R.`. Refrain-first
songs put the refrain section first.

**Lyrics that do not fit.** If a verse has more lyric lines than the melody has
phrases, the surplus is written out as a `% unaligned …` comment rather than
being pushed onto notes it does not belong to. That keeps the remaining text
aligned instead of letting it slide.

## Example output

```abc
X:1
T:Amazing Grace
C:John Newton
M:3/4
L:1/4
K:F
V:1
C | F2 (A/ F/) | A2 G | F2 D | C2
w:1.~A-ma-zing grace, How sweet the sound
w:2.~Twas grace that taught my heart to fear,
w:3.~Through ma-ny dan-gers, toils and snares
C | F2 (A/ F/) | A2 G | c3- | c2
w:That saved a wretch like me.
w:And grace my fears re-lieved.
w:I have al-rea-dy come,
```

## Not supported yet

`AbcSettings::include_chords` has no effect. Chord content is stored in
LilyPond `\chordmode` syntax, which the exporter does not parse. The flag is
kept so that the API does not change when support is added.
