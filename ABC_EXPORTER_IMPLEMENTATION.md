# ABC Notation Exporter Implementation

## Overview

Successfully implemented an ABC notation exporter for the cantara-songlib project that converts `.song.yml` files to ABC notation format.

## Files Created/Modified

### New Files
1. **`src/exporter/abc.rs`** - Main ABC exporter module with:
   - `AbcSettings` struct for configuration
   - `abc_from_song()` - Main export function
   - `abc_part_from_song()` - Export individual parts
   - Pitch and duration conversion utilities
   - Voice content conversion from LilyPond to ABC
   - Lyrics formatting for ABC notation

2. **`examples/abc_export_demo.rs`** - Demonstration program showing ABC export functionality

### Modified Files
1. **`src/exporter/mod.rs`** - Added `pub mod abc;` to expose the new module

## Features Implemented

### Core Functionality
- ✅ Header generation (title, composer, key, meter, unit note length)
- ✅ Voice/staff notation conversion from LilyPond input
- ✅ Multiple verse support with `w:` lyric markers
- ✅ Key signature conversion (e.g., "f major" → "K:F", "d minor" → "K:Dm")
- ✅ Time signature support
- ✅ Configurable unit note length
- ✅ Bar line preservation
- ✅ Tie handling (`~` → `-`)
- ✅ Rest conversion (`r`, `R`, `s` → `z`)
- ✅ Chord symbol support (optional)
- ✅ Individual part export capability

### Configuration Options (`AbcSettings`)
```rust
pub struct AbcSettings {
    pub unit_note_length: String,      // Default: "1/4"
    pub include_chords: bool,          // Default: true
    pub include_all_verses: bool,      // Default: true
}
```

## Test Coverage

Implemented 17 comprehensive tests covering:

1. **Header Tests**
   - Basic header fields (X, T, C, K, M, L)
   - Different key signatures
   - Custom settings

2. **Voice Conversion Tests**
   - Simple note conversion
   - Notes with ties
   - Bar lines
   - Rest conversion

3. **Lyrics Tests**
   - Single verse export
   - Multiple verses
   - Case-insensitive matching

4. **Integration Tests**
   - Full song export (Amazing Grace)
   - Stanza-refrain structure (Sei nicht stolz)
   - File I/O integration
   - Part export

5. **Error Handling**
   - No voice content error case

All tests pass successfully:
```
test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Usage Example

```rust
use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::importer::song_yml;

// Load song from YAML
let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml")?;
let song = song_yml::import_from_yml_string(&content)?;

// Export with default settings
let abc_output = abc_from_song(&song, &AbcSettings::default())?;

// Export with custom settings
let custom_settings = AbcSettings {
    unit_note_length: "1/8".to_string(),
    include_chords: false,
    include_all_verses: false,
};
let abc_custom = abc_from_song(&song, &custom_settings)?;
```

## Sample Output

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

## Known Limitations & Future Improvements

1. **Pitch Conversion**: Currently handles basic pitches and octaves. Could be enhanced for:
   - Better accidental handling (LilyPond's "cis" → ABC's "^c")
   - Complex chord symbols
   
2. **Duration Normalization**: Some LilyPond articulations may pass through unchanged

3. **ABC-Specific Features**: Could add:
   - Decorations (!accent!, !staccato!)
   - Dynamic markings
   - Repeat bars
   - Volta brackets

4. **Melisma Support**: Better handling of multiple notes per syllable

## Architecture

The implementation follows the same pattern as the existing LilyPond exporter:
- Settings struct for configuration
- Main export function returning `Result<String, String>`
- Helper functions for conversion
- Comprehensive test suite
- Module exposed via `mod.rs`

## Dependencies

No additional dependencies required beyond existing ones:
- Uses standard library for string manipulation
- Integrates with existing `Song` data model
- Compatible with existing YAML importer
