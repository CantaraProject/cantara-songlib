# Cantara-Songlib
![GitHub branch check runs](https://img.shields.io/github/check-runs/CantaraProject/cantara-songlib/master)

A reimplementation of the Cantara Backend in Rust.

This crate provides functionality for parsing songs in different formats (lyrics, scores and different languages) and export them as a song presentation or music sheet.
It is part of the effort to rewrite [Cantara](https://github.com/reckel-jm/cantara) in Rust.

> [!NOTE]
> While this crate is already usable, some functions are yet to be implemented. Please expect breaking changes.

## Documentation

| Document | Contents |
|----------|----------|
| [docs/data-model.md](docs/data-model.md) | The `Song` model: parts, orders, voices, multiple languages, metadata |
| [docs/lilypond-export.md](docs/lilypond-export.md) | Sheet music export and rendering |
| [docs/abc-export.md](docs/abc-export.md) | ABC notation export |
| [docs/ccli-import.md](docs/ccli-import.md) | Reading CCLI SongSelect exports in any language |
| [docs/meta-information.md](docs/meta-information.md) | The meta information line on slides: template and placement |
| [docs/complex-slides.md](docs/complex-slides.md) | Slides stacking notation and several languages |
| [docs/text-export.md](docs/text-export.md) | Plain text, Markdown/Telegram markup, and exporting several songs |
| [docs/c-api.md](docs/c-api.md) | C entrypoints matching the CLI export flows |

## Repository layout

```
src/
├── song.rs        the data model every importer and exporter shares
├── importer/      .song (classic), .song.yml, .ccli, .cssf readers
├── exporter/      slides, LilyPond, ABC, text writers
├── slides.rs      presentation slide types
├── templating.rs  the meta information template
├── lib.rs         library entry point and C FFI
└── main.rs        the command line wrapper
tests/
├── data/          song files used by the tests and examples
└── *.rs           integration tests
examples/          runnable demonstrations (cargo run --example …)
docs/              prose documentation
```

## Command line

```bash
cargo run -- "tests/data/Amazing Grace.song.yml" presentation   # slides as JSON
cargo run -- "tests/data/Amazing Grace.song.yml" lilypond       # sheet music
cargo run -- "tests/data/Amazing Grace.song.yml" abc            # ABC notation
cargo run -- "tests/data/Amazing Grace.song.yml" text           # plain lyrics
cargo run -- a.song.yml b.ccli text --format markdown           # several songs
```
