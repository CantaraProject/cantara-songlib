# LilyPond export

`src/exporter/lilypond.rs` turns a `Song` into a complete, standalone `.ly`
file, and can drive the LilyPond binary to render SVG or PDF.

## The three layouts

| Function                       | Output                                                                 |
|--------------------------------|------------------------------------------------------------------------|
| `lilypond_from_song`           | A hymn-book page: one melody staff with every verse below it as `\addlyrics`. |
| `lilypond_sequential_from_song`| One `\score` block per part in singing order (stanza 1 → refrain → stanza 2 → …). |
| `lilypond_parts_from_song`     | A separate, self-contained `.ly` file per part, for cropped per-part images. |

All three share the voice and lyrics definitions, so a melody is written out
once and referenced by name.

## From the command line

```bash
cantara-songlib "tests/data/Amazing Grace.song.yml" lilypond
```

| Option              | Default | Meaning                        |
|---------------------|---------|--------------------------------|
| `-p`, `--paper-size`| `a4`    | `\paper { #(set-paper-size …) }` |
| `-i`, `--indent`    | `#0`    | `\layout { indent = … }`         |

`LilypondSettings` additionally offers `staff_size` and a `font` setting
(`FontSetting::Specific { family }`) that are not exposed on the CLI.

## How the refrain is handled

When the refrain has a melody of its own, it becomes a **separate lyrics
variable** rather than being pasted into the first verse's text. That keeps each
block independently reusable:

```ly
verseOne = \lyricmode {
  \set stanza = "1."
  Sei nicht stolz auf das, was du bist,
  …
}

chorus = \lyricmode {
  Denn wer sich rüh -- men will,
  …
}

sopranoVoicePart = \new Staff { \sopranoVoiceStanza \sopranoVoiceRefrain }
\addlyrics { \verseOne \chorus }
\addlyrics { \verseTwo }
\addlyrics { \verseThree }
```

The `chorus` variable deliberately carries no `\set stanza`, because a refrain
has no verse number.

### Refrain-first songs

If the song's order starts with the refrain, the refrain melody comes first on
the staff. Simply swapping the references would place verse 2 underneath the
*refrain*. The later verses therefore have to skip past it:

```ly
chorusSkip = \lyricmode {
  \repeat unfold 7 { \skip 1 }
}

sopranoVoicePart = \new Staff { \sopranoVoiceRefrain \sopranoVoiceStanza }
\addlyrics { \chorus \verseOne }
\addlyrics { \chorusSkip \verseTwo }
\addlyrics { \chorusSkip \verseThree }
```

The repeat count is the refrain's syllable count: syllable separators (`--`),
extender lines (`__`) and inline commands such as `\set ignoreMelismata = ##t`
take no slot, while the blank syllable `_` does.

A refrain **without** its own melody shares the verse music instead, and is
added as a further `\addlyrics` line numbered `R1.`, `R2.`, …

## Rendering

`render_lilypond_to_svg` and `render_lilypond_to_pdf` shell out to the
`lilypond` binary and return the bytes. `render_song_parts_to_svg` renders every
part as a cropped SVG, which is what the presentation frontend uses.

These require LilyPond to be installed and on `PATH`; the error message says so
if it is not.
