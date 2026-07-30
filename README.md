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

## Repository layout

```
src/
├── song.rs        the data model every importer and exporter shares
├── importer/      .song (classic), .song.yml, .ccli, .cssf readers
├── exporter/      slides, LilyPond, ABC writers
├── slides.rs      presentation slide types
├── templating.rs  metadata templating
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
```
