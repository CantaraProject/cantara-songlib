# Transposing

A song is transposed by naming the key it should end up in and which way to
go. Both are needed: the target key alone is ambiguous. From `d major` to
`f major` is a minor third up or a major sixth down, and for a congregation
that is the difference between a comfortable melody and an unsingable one.

```bash
cantara-songlib "tests/data/Tochter Zion.song.yml" lilypond \
    --transpose F --transpose-direction up

cantara-songlib "tests/data/Tochter Zion.song.yml" abc \
    --transpose "Bb" --transpose-direction down
```

`--transpose` is a global option, so every command takes it — sheet music,
slides, plain text and `.sng` export all come out in the same key. Without
`--transpose-direction` the music moves as little as possible (`nearest`).

| `--transpose-direction` | From `d major` to `f major` |
|-------------------------|-----------------------------|
| `up`                    | a minor third up            |
| `down`                  | a major sixth down          |
| `nearest` (default)     | a minor third up — the shorter way |

The key may be written in any of the spellings the formats of this crate use:
`"f major"`, `"F"`, `"Dm"`, `"bes"`, `"Bb major"`, `"e flat minor"`,
`"F#m"`, `"E dorian"`.

## The mode stays as it is

Transposing moves every note by the same interval, which leaves the mode where
it was: a song in a minor key stays minor. A target that names only a tonic
therefore takes the song's own mode — `--transpose F` turns a song in D minor
into F minor, not F major — while a target that insists on another mode is
refused rather than answered with a key signature that contradicts the notes
underneath it:

```text
$ cantara-songlib song.song.yml abc --transpose "d minor"     # song is in d major
Error: "cannot transpose 'song.song.yml': transposing moves every note by the
       same interval, so a song in d major cannot become one in d minor; name a
       key in major instead"
```

Two keys that name the same pitch, such as `ges major` and `fis major`, are a
respelling rather than a move: the notes stay where they are in every direction
and only their spelling changes.

## From Rust

```rust
use cantara_songlib::importer::song_yml;
use cantara_songlib::music::TransposeDirection;
use cantara_songlib::transpose::transpose_song;

let content = std::fs::read_to_string("tests/data/Tochter Zion.song.yml")?;
let song = song_yml::import_from_yml_string(&content)?;
assert_eq!(song.score.key.as_deref(), Some("d major"));

let higher = transpose_song(&song, "F", TransposeDirection::Up)?;
assert_eq!(higher.score.key.as_deref(), Some("f major"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The result is a complete `Song` in the new key, so every exporter can be used
on it as usual. The original is left untouched.

`transpose_song` reports an error when the song does not say which key it is in
— there is nothing to transpose *from* — or when the target key cannot be read.

## What moves

Everything that sounds: every voice of every part, the chord line and the key
signature. Lyrics, metadata, part order and the layout of the notation are left
exactly as they were.

The spelling follows the key the music lands in rather than the notes it came
from. Transposing out of `d major` into `f major` turns the leading note `cis`
into `e`, not into `fes`; into `ees major` the same note becomes `d`. Durations,
slurs, ties, bar lines, breath marks, line breaks and comments come through
unchanged, so a transposed melody reads like the one it was written from:

```text
a'2 fis4.( g8 ) | a2 d, | e8( fis g a g4 ) fis | e1 \breathe |
c''2 a4.( bes8 ) | c2 f, | g8( a bes c bes4 ) a | g1 \breathe |
```

Chord lines keep their layout in the same way, bar lines and all:

```text
d1 | d | a2. d4 | b2:m d2:7 | g2 a | d1
f1 | f | c2. f4 | d2:m f2:7 | bes2 c | f1
```

## Transposing twice

Transposing is a plain interval shift, so going somewhere and coming back
returns the music it started from:

```rust
use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::importer::song_yml;
use cantara_songlib::music::TransposeDirection;
use cantara_songlib::transpose::transpose_song;

let content = std::fs::read_to_string("tests/data/Tochter Zion.song.yml")?;
let song = song_yml::import_from_yml_string(&content)?;

let there = transpose_song(&song, "f major", TransposeDirection::Up)?;
let back = transpose_song(&there, "d major", TransposeDirection::Down)?;

let abc = |song| abc_from_song(song, &AbcSettings::default());
assert_eq!(abc(&back)?, abc(&song)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```
