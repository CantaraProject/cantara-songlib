# Chords

A song keeps its chords in a content block of its own, next to the melody and
the lyrics of the part they belong to:

```yaml
parts:
  - type: stanza
    contents:
    - type: chords
      number: 1
      content: |
        d1 | d | a2. d4 | a1 | d2. a4 | b2:m d2:7 | g2 a | d1
    - type: voice
      number: 1
      content: |
        a'2 fis4.( g8 ) | a2 d, | e8( fis g a g4 ) fis | e1 |
    - type: lyrics
      number: 1
      content: |
        Toch -- ter Zi -- on, fre -- ue dich!
```

The notation is LilyPond's `\chordmode`: a root note in Dutch spelling (`d`,
`fis`, `bes`), a duration, and — after a colon — the quality.

| `\chordmode` | Chord symbol | |
|--------------|--------------|---|
| `d1`         | `D`          | a whole bar of D major |
| `b2:m`       | `Bm`         | a half bar of B minor |
| `d2:7`       | `D7`         | a dominant seventh |
| `c:maj7`     | `Cmaj7`      | `:maj` and `:maj7` mean the same thing |
| `e:m7`       | `Em7`        | |
| `g:dim`, `g:aug` | `Gdim`, `Gaug` | |
| `g:sus4`, `g:sus2` | `Gsus4`, `Gsus2` | `:sus` on its own is `sus4` |
| `a:m7.5-`    | `Am7b5`      | `-` lowers a degree, `+` raises it |
| `c:5.9`      | `Cadd9`      | a triad with a ninth added |
| `a:7/cis`    | `A7/C#`      | a bass note |

Durations work as they do in LilyPond: a chord without one lasts as long as the
chord before it, so `g2 a` is two half bars. Bar lines are a reading aid and
may be left out. A rest (`r`) says that no chord sounds.

The chord line runs in parallel with the melody of the same part, from the
first note onwards. A part that reuses another part's melody — verse 2 singing
verse 1's tune — is sung over that part's chords as well, however long the
chain of repetitions is.

## Where the chords show up

**LilyPond** prints them in a `ChordNames` context above the staff. That holds
for all three exports: the paper score, the sequential score with one block per
part sung, and the standalone per-part files the presentation uses for its
notation rows.

**ABC** writes each symbol as an annotation in front of the note it sounds
over, which is what every ABC renderer draws above the staff:

```abc
X:1
T:Tochter Zion, freue dich
M:4/4
L:1/4
K:D
V:1
"D"A2 (F3/2 G/) | A2 D2 | "A"(E/ F/ G/ A/ G) "D"F | "A"E4 |
w:1.~Toch-ter _ Zi-on, fre- _ _ _ _ ue dich!
```

A chord that merely repeats the one before it is not printed again — a held
chord is shown by leaving the space empty, the way a lead sheet does. A rest in
the chord line ends the chord, so the same chord after a rest is printed afresh.

Where the harmony changes under a note that is still sounding, the note is cut
in two and tied back together, so that the new chord stands where it starts:

```text
chords  c2 g2        melody  c'1        becomes  "C"c2- "G"c2
```

The two halves sound as the undivided note did, and because the second half
continues a tie it sings no syllable of its own — the words stay where they
were.

A single-phrase excerpt (`PartPhrases::excerpt`, which is what a presentation
slide shows) is a tune of its own: whoever reads it cannot see the phrases
before it, so the chord still sounding at its first note is printed again even
though the full export leaves a held chord unrepeated.

Chord symbols can be left out with `AbcSettings::include_chords`:

```rust
use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::importer::song_yml;

let content = std::fs::read_to_string("tests/data/Tochter Zion.song.yml")?;
let song = song_yml::import_from_yml_string(&content)?;

let with_chords = abc_from_song(&song, &AbcSettings::default())?;
assert!(with_chords.contains("\"Bm\""));

let settings = AbcSettings { include_chords: false, ..AbcSettings::default() };
let without = abc_from_song(&song, &settings)?;
assert!(!without.contains("\"Bm\""));
# Ok::<(), Box<dyn std::error::Error>>(())
```

On the command line that is `--include-chords false`.

## Transposing

Chords are transposed together with the melody — see
[docs/transposing.md](transposing.md).
