# ABC Notation Export - CLI Usage Guide

## Overview

The cantara-songlib CLI now supports exporting songs to ABC notation format via the `abc` subcommand.

## Basic Usage

```bash
cantara-songlib abc <input-file>
```

### Examples

**Export with default settings:**
```bash
./target/debug/cantara-songlib abc testfiles/Amazing\ Grace.song.yml
```

**Export with custom unit note length:**
```bash
./target/debug/cantara-songlib abc testfiles/Amazing\ Grace.song.yml --unit-note-length 1/8
```

**Export first verse only:**
```bash
./target/debug/cantara-songlib abc testfiles/Amazing\ Grace.song.yml --all-verses false
```

**Disable chord symbols:**
```bash
./target/debug/cantara-songlib abc testfiles/Amazing\ Grace.song.yml --include-chords false
```

**Combine multiple options:**
```bash
./target/debug/cantara-songlib abc testfiles/Amazing\ Grace.song.yml \
  --unit-note-length 1/8 \
  --include-chords false \
  --all-verses false
```

## Command-Line Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--unit-note-length` | `-u` | `1/4` | Unit note length for the output (e.g., "1/4", "1/8") |
| `--include-chords` | - | `true` | Include chord symbols above the staff (`true` or `false`) |
| `--all-verses` | - | `true` | Include all verses in output (`true` for all, `false` for first verse only) |

## Supported Input Formats

The CLI automatically detects the input file format:

- **`.song.yml` / `.song.yaml`** - YAML-based song format
- **`.yml` / `.yaml`** - Generic YAML files
- **`.song`** - Classic Cantara song format

## Output

The ABC notation is printed to **stdout**, so you can:

**Save to a file:**
```bash
./target/debug/cantara-songlib abc testfiles/Amazing\ Grace.song.yml > output.abc
```

**Pipe to another tool:**
```bash
./target/debug/cantara-songlib abc testfiles/Amazing\ Grace.song.yml | abcm2ps - | gv
```

## Help

View help for the ABC subcommand:
```bash
./target/debug/cantara-songlib abc --help
```

View general CLI help:
```bash
./target/debug/cantara-songlib --help
```

## Example Output

```abc
X:1
T:Amazing Grace
C:John Newton
K:F
M:3/4
L:1/4

V:1
c4 | f2 a8 f | a2 g4 | f2 d4 | c2 c4 | ...
w:
A -- ma -- zing grace, How sweet the sound...
w:
Twas grace that taught my heart to fear...
w:
Through ma -- ny dan -- gers, toils and snares...
```

## Exit Codes

- **0** - Success
- **1** - Error (file not found, invalid format, export failed, etc.)
