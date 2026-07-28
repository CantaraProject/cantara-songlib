//! ABC notation exporter — generates `.abc` files from a [`Song`].
//!
//! ABC is a text based music notation format that is widely supported by
//! renderers such as `abcm2ps`, `abc2svg` and `abcjs`. This module translates
//! the LilyPond flavoured voice content stored in a song into ABC, together
//! with the lyrics of every verse and the refrain.
//!
//! The conversion works in three stages:
//!
//! 1. [`parse_voice`] turns the LilyPond input into a list of [`MusicEvent`]s.
//!    This resolves `\relative` octaves, LilyPond's duration memory (a note
//!    without a duration inherits the previous one), accidentals (`fis`, `bes`,
//!    …), ties, slurs and bar lines.
//! 2. The events are grouped into *slots* — one slot per sung syllable. Notes
//!    that continue a tie or that sit inside a slur belong to the previous
//!    slot, so a slot count can be compared directly with a syllable count.
//! 3. The slots are split into lines along the lyric lines of the song and
//!    rendered as ABC, with one `w:` line per verse underneath each music line.
//!
//! Songs whose refrain carries its own melody get a separate, annotated
//! section after (or before, for refrain-first songs) the stanza.

use crate::song::{Song, SongPart, SongPartContent, SongPartContentType};

/// Configuration for ABC notation export output.
#[derive(Clone, PartialEq, Debug)]
pub struct AbcSettings {
    /// The unit note length (`L:` field), e.g. `"1/4"` or `"1/8"`.
    pub unit_note_length: String,
    /// Reserved for chord symbol output.
    ///
    /// Chord content is stored in LilyPond `\chordmode` syntax which this
    /// exporter does not parse yet, so the flag currently has no effect.
    pub include_chords: bool,
    /// Whether to include all verses or only the first one.
    pub include_all_verses: bool,
}

impl Default for AbcSettings {
    fn default() -> Self {
        AbcSettings {
            unit_note_length: "1/4".to_string(),
            include_chords: true,
            include_all_verses: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Rational numbers (note durations as a fraction of a whole note)
// ---------------------------------------------------------------------------

/// A non-negative rational number, always kept in lowest terms.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Frac {
    num: u32,
    den: u32,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

impl Frac {
    fn new(num: u32, den: u32) -> Frac {
        let den = if den == 0 { 1 } else { den };
        let g = gcd(num.max(1), den);
        Frac {
            num: num / g,
            den: den / g,
        }
    }

    fn zero() -> Frac {
        Frac { num: 0, den: 1 }
    }

    fn is_zero(self) -> bool {
        self.num == 0
    }

    /// `self / other`
    fn div(self, other: Frac) -> Frac {
        Frac::new(self.num * other.den, self.den * other.num)
    }

    fn add(self, other: Frac) -> Frac {
        Frac::new(
            self.num * other.den + other.num * self.den,
            self.den * other.den,
        )
    }

    /// `self - other`, saturating at zero.
    fn sub(self, other: Frac) -> Frac {
        let num = self.num * other.den;
        let subtrahend = other.num * self.den;
        if num <= subtrahend {
            Frac::zero()
        } else {
            Frac::new(num - subtrahend, self.den * other.den)
        }
    }

    /// `self >= other`
    fn at_least(self, other: Frac) -> bool {
        self.num * other.den >= other.num * self.den
    }
}

/// Parse a `"1/4"`-style unit note length. Falls back to a quarter note.
fn parse_unit_note_length(text: &str) -> Frac {
    let mut split = text.trim().split('/');
    let num = split.next().and_then(|s| s.trim().parse::<u32>().ok());
    let den = split.next().and_then(|s| s.trim().parse::<u32>().ok());
    match (num, den) {
        (Some(n), Some(d)) if n > 0 && d > 0 => Frac::new(n, d),
        (Some(n), None) if n > 0 => Frac::new(n, 1),
        _ => Frac::new(1, 4),
    }
}

/// Convert a LilyPond duration such as `4`, `2.` or `8..` into a fraction of a
/// whole note. An empty string means "reuse the previous duration" and yields
/// `None`.
fn parse_lilypond_duration(text: &str) -> Option<Frac> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let base: u32 = digits.parse().ok()?;
    let dots = text.chars().filter(|c| *c == '.').count() as u32;

    // A note with n dots lasts (2^(n+1) - 1) / 2^n of its undotted value.
    let dot_num = 2u32.pow(dots + 1) - 1;
    let dot_den = 2u32.pow(dots);
    Some(Frac::new(dot_num, base * dot_den))
}

// ---------------------------------------------------------------------------
// The music event model
// ---------------------------------------------------------------------------

/// A pitch in scientific notation: `step` 0..6 maps to c…b, `octave` 4 is the
/// octave of middle C.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Pitch {
    step: u8,
    alter: i8,
    octave: i32,
}

/// One musical event of a voice.
#[derive(Clone, Debug)]
enum MusicEvent {
    /// A single note or a chord (`pitches` holds more than one entry).
    Note {
        pitches: Vec<Pitch>,
        duration: Frac,
        /// The note is tied to the following one.
        tie: bool,
        /// A slur starts at this note.
        slur_start: bool,
        /// A slur ends at this note.
        slur_end: bool,
    },
    /// A rest. `visible` is false for LilyPond's `s` spacer rests.
    Rest { duration: Frac, visible: bool },
    /// A bar line, already in ABC spelling (`|`, `||`, `|]`, `:|`, …).
    Barline(String),
}

impl MusicEvent {
    fn is_note(&self) -> bool {
        matches!(self, MusicEvent::Note { .. })
    }
}

const STEP_LETTERS: [char; 7] = ['c', 'd', 'e', 'f', 'g', 'a', 'b'];

fn letter_to_step(c: char) -> Option<u8> {
    STEP_LETTERS.iter().position(|l| *l == c).map(|p| p as u8)
}

// ---------------------------------------------------------------------------
// LilyPond parsing
// ---------------------------------------------------------------------------

/// Parse the note name part of a LilyPond token (e.g. `fis`, `bes`, `ceses`).
/// Returns the diatonic step, the alteration and how many characters were used.
fn parse_note_name(token: &str) -> Option<(u8, i8, usize)> {
    let mut chars = token.chars();
    let first = chars.next()?;
    let step = letter_to_step(first.to_ascii_lowercase())?;

    let mut alter: i8 = 0;
    let mut consumed = first.len_utf8();
    let rest = &token[consumed..];
    let mut offset = 0usize;

    loop {
        let remainder = &rest[offset..];
        if remainder.starts_with("isis") {
            alter += 2;
            offset += 4;
        } else if remainder.starts_with("eses") {
            alter -= 2;
            offset += 4;
        } else if remainder.starts_with("is") {
            alter += 1;
            offset += 2;
        } else if remainder.starts_with("es") {
            alter -= 1;
            offset += 2;
        } else if remainder.starts_with('s') && (first == 'a' || first == 'e') && offset == 0 {
            // Dutch short forms: `as` = a flat, `es` = e flat.
            alter -= 1;
            offset += 1;
        } else {
            break;
        }
    }

    consumed += offset;
    Some((step, alter, consumed))
}

/// Resolve a LilyPond `\relative` pitch against the previous pitch.
///
/// LilyPond picks the octave that keeps the interval to the previous note at a
/// fourth or less; `'` and `,` shift the result by whole octaves.
fn resolve_relative_octave(previous: &Pitch, step: u8, octave_marks: i32) -> i32 {
    let previous_index = previous.octave * 7 + previous.step as i32;
    let mut octave = previous.octave;
    let mut index = octave * 7 + step as i32;

    while index - previous_index > 3 {
        octave -= 1;
        index -= 7;
    }
    while previous_index - index > 3 {
        octave += 1;
        index += 7;
    }

    octave + octave_marks
}

/// Translate a LilyPond `\bar "…"` argument into its ABC spelling.
fn lilypond_bar_to_abc(bar: &str) -> String {
    match bar {
        "|." => "|]".to_string(),
        "||" => "||".to_string(),
        ".|" => "[|".to_string(),
        ":|." | ":|" | ":|]" => ":|".to_string(),
        ".|:" | "|:" => "|:".to_string(),
        ":..:" | ":|.|:" | ":|:" => "::".to_string(),
        _ => "|".to_string(),
    }
}

/// Characters that end a note token.
fn is_token_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '|' | '(' | ')' | '<' | '>' | '\\' | '"' | '[' | ']')
}

/// Parse LilyPond voice content into a list of [`MusicEvent`]s.
///
/// The content is interpreted as if it were wrapped in `\relative c'`, which is
/// how the LilyPond exporter emits it.
fn parse_voice(content: &str) -> Vec<MusicEvent> {
    let chars: Vec<char> = content.chars().collect();
    let byte_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(chars.len() + 1);
        let mut acc = 0;
        for c in &chars {
            offsets.push(acc);
            acc += c.len_utf8();
        }
        offsets.push(acc);
        offsets
    };
    let _ = byte_offsets;

    let mut events: Vec<MusicEvent> = Vec::new();
    let mut index = 0usize;
    let mut last_duration = Frac::new(1, 4);
    // `\relative c'` starts from middle C.
    let mut previous_pitch = Pitch {
        step: 0,
        alter: 0,
        octave: 4,
    };

    /// Apply a flag to the most recently parsed note.
    fn mark_last_note(events: &mut [MusicEvent], apply: impl Fn(&mut MusicEvent)) {
        if let Some(event) = events.iter_mut().rev().find(|e| e.is_note()) {
            apply(event);
        }
    }

    while index < chars.len() {
        let current = chars[index];

        if current.is_whitespace() {
            index += 1;
            continue;
        }

        match current {
            '\\' => {
                index += 1;
                let start = index;
                while index < chars.len() && chars[index].is_ascii_alphabetic() {
                    index += 1;
                }
                let command: String = chars[start..index].iter().collect();

                if command == "bar" {
                    while index < chars.len() && chars[index].is_whitespace() {
                        index += 1;
                    }
                    if index < chars.len() && chars[index] == '"' {
                        index += 1;
                        let arg_start = index;
                        while index < chars.len() && chars[index] != '"' {
                            index += 1;
                        }
                        let argument: String = chars[arg_start..index].iter().collect();
                        index += 1; // closing quote
                        events.push(MusicEvent::Barline(lilypond_bar_to_abc(&argument)));
                    }
                } else if matches!(command.as_str(), "set" | "unset" | "override" | "revert") {
                    // These carry arguments we cannot use; skip the rest of the line.
                    while index < chars.len() && chars[index] != '\n' {
                        index += 1;
                    }
                }
                // Every other command (\breathe, \global, dynamics, …) is dropped.
            }
            '|' => {
                let mut count = 0;
                while index < chars.len() && chars[index] == '|' {
                    count += 1;
                    index += 1;
                }
                events.push(MusicEvent::Barline(
                    if count > 1 { "||" } else { "|" }.to_string(),
                ));
            }
            '(' => {
                mark_last_note(&mut events, |event| {
                    if let MusicEvent::Note { slur_start, .. } = event {
                        *slur_start = true;
                    }
                });
                index += 1;
            }
            ')' => {
                mark_last_note(&mut events, |event| {
                    if let MusicEvent::Note { slur_end, .. } = event {
                        *slur_end = true;
                    }
                });
                index += 1;
            }
            '~' => {
                mark_last_note(&mut events, |event| {
                    if let MusicEvent::Note { tie, .. } = event {
                        *tie = true;
                    }
                });
                index += 1;
            }
            '"' => {
                index += 1;
                while index < chars.len() && chars[index] != '"' {
                    index += 1;
                }
                index += 1;
            }
            '<' => {
                index += 1;
                let start = index;
                while index < chars.len() && chars[index] != '>' {
                    index += 1;
                }
                let inner: String = chars[start..index].iter().collect();
                index += 1; // closing '>'

                // The duration follows the closing bracket.
                let duration_start = index;
                while index < chars.len() && !is_token_delimiter(chars[index]) {
                    index += 1;
                }
                let suffix: String = chars[duration_start..index].iter().collect();

                let mut pitches = Vec::new();
                for note in inner.split_whitespace() {
                    if let Some(pitch) = parse_pitch_token(note, &mut previous_pitch) {
                        pitches.push(pitch);
                    }
                }
                if pitches.is_empty() {
                    continue;
                }

                let duration = parse_lilypond_duration(&suffix).unwrap_or(last_duration);
                last_duration = duration;
                events.push(MusicEvent::Note {
                    pitches,
                    duration,
                    tie: suffix.contains('~'),
                    slur_start: false,
                    slur_end: false,
                });
            }
            '[' | ']' => index += 1,
            c if c.is_ascii_alphabetic() => {
                let start = index;
                while index < chars.len() && !is_token_delimiter(chars[index]) {
                    index += 1;
                }
                let token: String = chars[start..index].iter().collect();

                if let Some(event) =
                    parse_note_or_rest(&token, &mut previous_pitch, &mut last_duration)
                {
                    events.push(event);
                }
            }
            _ => index += 1,
        }
    }

    events
}

/// Parse the pitch part of a token and advance the `\relative` reference.
fn parse_pitch_token(token: &str, previous_pitch: &mut Pitch) -> Option<Pitch> {
    let (step, alter, consumed) = parse_note_name(token)?;

    let mut octave_marks = 0i32;
    for c in token[consumed..].chars() {
        match c {
            '\'' => octave_marks += 1,
            ',' => octave_marks -= 1,
            _ => break,
        }
    }

    let octave = resolve_relative_octave(previous_pitch, step, octave_marks);
    let pitch = Pitch { step, alter, octave };
    *previous_pitch = pitch;
    Some(pitch)
}

/// Parse a whitespace-delimited LilyPond token into a note or a rest.
fn parse_note_or_rest(
    token: &str,
    previous_pitch: &mut Pitch,
    last_duration: &mut Frac,
) -> Option<MusicEvent> {
    let tie = token.ends_with('~');
    let token = token.trim_end_matches('~');
    if token.is_empty() {
        return None;
    }

    let first = token.chars().next()?;

    // Rests: `r` (rest), `R` (multi-measure rest), `s` (spacer).
    if matches!(first, 'r' | 'R' | 's') {
        let remainder = &token[first.len_utf8()..];
        let is_rest = remainder.is_empty()
            || remainder.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '*');
        if is_rest {
            let duration = parse_lilypond_duration(remainder).unwrap_or(*last_duration);
            *last_duration = duration;
            return Some(MusicEvent::Rest {
                duration,
                visible: first != 's',
            });
        }
    }

    let (step, alter, consumed) = parse_note_name(token)?;

    let mut octave_marks = 0i32;
    let mut rest_index = consumed;
    for c in token[consumed..].chars() {
        match c {
            '\'' => {
                octave_marks += 1;
                rest_index += c.len_utf8();
            }
            ',' => {
                octave_marks -= 1;
                rest_index += c.len_utf8();
            }
            _ => break,
        }
    }

    let octave = resolve_relative_octave(previous_pitch, step, octave_marks);
    let pitch = Pitch { step, alter, octave };
    *previous_pitch = pitch;

    let duration = parse_lilypond_duration(&token[rest_index..]).unwrap_or(*last_duration);
    *last_duration = duration;

    Some(MusicEvent::Note {
        pitches: vec![pitch],
        duration,
        tie,
        slur_start: false,
        slur_end: false,
    })
}

// ---------------------------------------------------------------------------
// Key signatures
// ---------------------------------------------------------------------------

/// Position of a tonic on the circle of fifths (naturals only).
fn natural_fifths(step: u8) -> i32 {
    match step {
        3 => -1, // f
        0 => 0,  // c
        4 => 1,  // g
        1 => 2,  // d
        5 => 3,  // a
        2 => 4,  // e
        6 => 5,  // b
        _ => 0,
    }
}

/// Offset a mode adds to the circle-of-fifths position of its tonic.
fn mode_fifths_offset(mode: &str) -> i32 {
    match mode {
        "minor" | "aeolian" | "m" => -3,
        "dorian" | "dor" => -2,
        "phrygian" | "phr" => -4,
        "lydian" | "lyd" => 1,
        "mixolydian" | "mix" => -1,
        "locrian" | "loc" => -5,
        _ => 0, // major / ionian
    }
}

/// Split a key string such as `"f major"`, `"bes minor"` or `"F# dorian"` into
/// the tonic's step, its alteration and the mode.
fn split_key(key_str: &str) -> Option<(u8, i8, String)> {
    let lowered = key_str.trim().to_lowercase();
    let mut tokens = lowered.split_whitespace();
    let tonic = tokens.next()?;
    let mut mode: String = tokens.collect::<Vec<&str>>().join(" ");

    let mut chars = tonic.chars();
    let letter = chars.next()?;
    let step = letter_to_step(letter)?;

    let suffix: String = chars.collect();
    let mut alter: i8 = 0;
    match suffix.as_str() {
        "" => {}
        "#" | "is" | "sharp" => alter = 1,
        "##" | "isis" | "x" => alter = 2,
        "b" | "es" | "flat" => alter = -1,
        "bb" | "eses" => alter = -2,
        "s" if letter == 'a' || letter == 'e' => alter = -1,
        _ => {}
    }

    // Allow "b flat major" / "f sharp minor" spellings.
    if alter == 0 {
        match mode.split_whitespace().next() {
            Some("flat") => {
                alter = -1;
                mode = mode.trim_start_matches("flat").trim().to_string();
            }
            Some("sharp") => {
                alter = 1;
                mode = mode.trim_start_matches("sharp").trim().to_string();
            }
            _ => {}
        }
    }

    if mode.is_empty() {
        mode = "major".to_string();
    }

    Some((step, alter, mode))
}

/// Number of sharps (positive) or flats (negative) of a key.
fn key_fifths(key_str: &str) -> i32 {
    match split_key(key_str) {
        Some((step, alter, mode)) => {
            natural_fifths(step) + alter as i32 * 7 + mode_fifths_offset(&mode)
        }
        None => 0,
    }
}

/// The alteration each diatonic step carries in a key with `fifths` accidentals.
fn key_alterations(fifths: i32) -> [i8; 7] {
    const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6]; // f c g d a e b
    const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3]; // b e a d g c f

    let mut alterations = [0i8; 7];
    if fifths > 0 {
        for step in SHARP_ORDER.iter().take(fifths.min(7) as usize) {
            alterations[*step] = 1;
        }
    } else if fifths < 0 {
        for step in FLAT_ORDER.iter().take((-fifths).min(7) as usize) {
            alterations[*step] = -1;
        }
    }
    alterations
}

/// Convert a key string from LilyPond format to the ABC `K:` field.
///
/// LilyPond: `"f major"`, `"d minor"`, `"bes major"` →
/// ABC: `"F"`, `"Dm"`, `"Bb"`.
pub fn convert_key_to_abc(key_str: &str) -> String {
    let (step, alter, mode) = match split_key(key_str) {
        Some(parts) => parts,
        None => return "C".to_string(),
    };

    let mut tonic = STEP_LETTERS[step as usize].to_ascii_uppercase().to_string();
    match alter {
        1 => tonic.push('#'),
        2 => tonic.push_str("##"),
        -1 => tonic.push('b'),
        -2 => tonic.push_str("bb"),
        _ => {}
    }

    match mode.as_str() {
        "major" | "ionian" => tonic,
        "minor" | "aeolian" => format!("{}m", tonic),
        "mixolydian" => format!("{}mix", tonic),
        "dorian" => format!("{}dor", tonic),
        "phrygian" => format!("{}phr", tonic),
        "lydian" => format!("{}lyd", tonic),
        "locrian" => format!("{}loc", tonic),
        _ => tonic,
    }
}

// ---------------------------------------------------------------------------
// ABC rendering of music events
// ---------------------------------------------------------------------------

/// Accidentals that are currently in force, keyed by (step, octave).
/// ABC resets them at every bar line.
#[derive(Default)]
struct BarAccidentals {
    entries: Vec<((u8, i32), i8)>,
}

impl BarAccidentals {
    fn get(&self, step: u8, octave: i32) -> Option<i8> {
        self.entries
            .iter()
            .find(|((s, o), _)| *s == step && *o == octave)
            .map(|(_, alter)| *alter)
    }

    fn set(&mut self, step: u8, octave: i32, alter: i8) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|((s, o), _)| *s == step && *o == octave)
        {
            entry.1 = alter;
        } else {
            self.entries.push(((step, octave), alter));
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Render a duration as an ABC length modifier relative to the unit note length.
fn render_duration(duration: Frac, unit: Frac) -> String {
    let ratio = duration.div(unit);
    match (ratio.num, ratio.den) {
        (1, 1) => String::new(),
        (n, 1) => n.to_string(),
        (1, 2) => "/".to_string(),
        (1, d) => format!("/{}", d),
        (n, d) => format!("{}/{}", n, d),
    }
}

/// Render a single pitch, emitting an accidental only where it is needed.
fn render_pitch(pitch: &Pitch, key: &[i8; 7], bar: &mut BarAccidentals) -> String {
    let effective = bar
        .get(pitch.step, pitch.octave)
        .unwrap_or(key[pitch.step as usize]);

    let mut result = String::new();
    if pitch.alter != effective {
        result.push_str(match pitch.alter {
            2 => "^^",
            1 => "^",
            0 => "=",
            -1 => "_",
            -2 => "__",
            _ => "",
        });
        bar.set(pitch.step, pitch.octave, pitch.alter);
    }

    let letter = STEP_LETTERS[pitch.step as usize];
    // ABC: octave 4 is uppercase, octave 5 lowercase, then ' / , per octave.
    if pitch.octave >= 5 {
        result.push(letter);
        for _ in 5..pitch.octave {
            result.push('\'');
        }
    } else {
        result.push(letter.to_ascii_uppercase());
        for _ in pitch.octave..4 {
            result.push(',');
        }
    }

    result
}

/// Render a slice of events into a single ABC music line.
fn render_events(
    events: &[MusicEvent],
    key: &[i8; 7],
    unit: Frac,
    bar: &mut BarAccidentals,
) -> String {
    let mut pieces: Vec<String> = Vec::new();

    for event in events {
        match event {
            MusicEvent::Barline(text) => {
                bar.clear();
                pieces.push(text.clone());
            }
            MusicEvent::Rest { duration, visible } => {
                let symbol = if *visible { 'z' } else { 'x' };
                pieces.push(format!("{}{}", symbol, render_duration(*duration, unit)));
            }
            MusicEvent::Note {
                pitches,
                duration,
                tie,
                slur_start,
                slur_end,
            } => {
                let mut text = String::new();
                if *slur_start {
                    text.push('(');
                }

                if pitches.len() == 1 {
                    text.push_str(&render_pitch(&pitches[0], key, bar));
                } else {
                    text.push('[');
                    for pitch in pitches {
                        text.push_str(&render_pitch(pitch, key, bar));
                    }
                    text.push(']');
                }

                text.push_str(&render_duration(*duration, unit));

                if *tie {
                    text.push('-');
                }
                if *slur_end {
                    text.push(')');
                }
                pieces.push(text);
            }
        }
    }

    pieces.join(" ")
}

// ---------------------------------------------------------------------------
// Syllable slots and lyrics
// ---------------------------------------------------------------------------

/// Determine, for every event, whether it starts a new sung syllable.
///
/// Notes that continue a tie or that sit inside a slur are melismas and stay
/// with the preceding syllable.
fn slot_flags(events: &[MusicEvent]) -> Vec<bool> {
    let mut flags = Vec::with_capacity(events.len());
    let mut in_slur = false;
    let mut tie_pending = false;

    for event in events {
        match event {
            MusicEvent::Note {
                tie,
                slur_start,
                slur_end,
                ..
            } => {
                flags.push(!tie_pending && !in_slur);
                tie_pending = *tie;
                if *slur_start {
                    in_slur = true;
                }
                if *slur_end {
                    in_slur = false;
                }
            }
            _ => flags.push(false),
        }
    }

    flags
}

/// One syllable of a lyrics line.
#[derive(Clone, Debug, PartialEq)]
struct Syllable {
    text: String,
    /// The syllable continues the previous word and is joined with a hyphen.
    joined: bool,
}

/// Escape the characters ABC treats as control characters inside a `w:` line.
fn escape_lyric_text(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        match c {
            '-' | '_' | '*' | '|' | '~' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Split a LilyPond lyrics line into syllables.
///
/// `--` (as its own token or inside a word) marks a syllable break, `_` is a
/// melisma extender and is passed through, and inline commands such as
/// `\set ignoreMelismata = ##t` are removed together with their arguments.
fn lyrics_line_to_syllables(line: &str) -> Vec<Syllable> {
    let mut syllables: Vec<Syllable> = Vec::new();
    let mut join_next = false;
    let mut words = line.split_whitespace().peekable();

    while let Some(word) = words.next() {
        if word.starts_with('\\') {
            // Drop the command, its target and an optional `= value`.
            words.next();
            if words.peek() == Some(&"=") {
                words.next();
                words.next();
            }
            continue;
        }

        if word == "--" {
            join_next = true;
            continue;
        }

        if word == "_" {
            syllables.push(Syllable {
                text: "_".to_string(),
                joined: false,
            });
            continue;
        }

        // A word may carry the syllable separator without spaces ("hei--lig").
        for (index, chunk) in word.split("--").enumerate() {
            if chunk.is_empty() {
                continue;
            }
            syllables.push(Syllable {
                text: escape_lyric_text(chunk),
                joined: join_next || index > 0,
            });
            join_next = false;
        }
        join_next = false;
    }

    syllables
}

/// Split a whole lyrics block into one syllable list per line, dropping lines
/// which contain no singable text.
fn lyrics_to_lines(content: &str) -> Vec<Vec<Syllable>> {
    content
        .lines()
        .map(lyrics_line_to_syllables)
        .filter(|line| !line.is_empty())
        .collect()
}

/// Render a list of syllables as the body of an ABC `w:` line.
fn render_syllables(syllables: &[Syllable]) -> String {
    let mut result = String::new();
    for (index, syllable) in syllables.iter().enumerate() {
        if index > 0 {
            result.push(if syllable.joined { '-' } else { ' ' });
        }
        result.push_str(&syllable.text);
    }
    result
}

// ---------------------------------------------------------------------------
// Section assembly
// ---------------------------------------------------------------------------

/// A musical section (the stanza or the refrain) together with its lyrics.
struct Section {
    /// Text printed above the first note, or `None` for the stanza.
    annotation: Option<String>,
    events: Vec<MusicEvent>,
    /// One entry per lyrics block: an optional verse label plus its syllables.
    lyrics: Vec<(Option<String>, Vec<Vec<Syllable>>)>,
}

/// Split a section's events into music lines along the reference lyrics lines.
///
/// Returns the event ranges of each line together with the number of syllable
/// slots it holds.
fn segment_events(events: &[MusicEvent], reference: &[Vec<Syllable>]) -> Vec<(usize, usize, usize)> {
    let flags = slot_flags(events);
    let total_slots = flags.iter().filter(|f| **f).count();

    // Without usable lyrics the whole section becomes one line.
    if reference.is_empty() || total_slots == 0 {
        return vec![(0, events.len(), total_slots)];
    }

    let mut segments = Vec::new();
    let mut index = 0usize;

    for (line_number, line) in reference.iter().enumerate() {
        if index >= events.len() {
            break;
        }

        let is_last_line = line_number + 1 == reference.len();
        let wanted = line.len();
        let start = index;
        let mut seen = 0usize;

        if is_last_line {
            // Everything that is left belongs to the final line.
            index = events.len();
            seen = flags[start..].iter().filter(|f| **f).count();
        } else {
            while index < events.len() && seen < wanted {
                if flags[index] {
                    seen += 1;
                }
                index += 1;
            }
            // Pull trailing bar lines and rests into this line.
            while index < events.len() && !flags[index] {
                index += 1;
            }
        }

        if index > start {
            segments.push((start, index, seen));
        }
    }

    if index < events.len() {
        let seen = flags[index..].iter().filter(|f| **f).count();
        segments.push((index, events.len(), seen));
    }

    segments
}

/// Render one section into ABC lines.
fn render_section(
    section: &Section,
    key: &[i8; 7],
    unit: Frac,
    bar: &mut BarAccidentals,
) -> String {
    // The first lyrics block defines where the music lines are broken.
    let reference: &[Vec<Syllable>] = section
        .lyrics
        .first()
        .map(|(_, lines)| lines.as_slice())
        .unwrap_or(&[]);

    let segments = segment_events(&section.events, reference);

    // Flatten every verse into a single syllable stream so that verses with a
    // different line layout still line up with the music.
    let mut streams: Vec<(Option<String>, Vec<Syllable>)> = section
        .lyrics
        .iter()
        .map(|(label, lines)| {
            (
                label.clone(),
                lines.iter().flatten().cloned().collect::<Vec<Syllable>>(),
            )
        })
        .collect();

    let mut output = String::new();
    let mut cursors = vec![0usize; streams.len()];

    for (segment_index, (start, end, slots)) in segments.iter().enumerate() {
        let mut line = render_events(&section.events[*start..*end], key, unit, bar);

        if segment_index == 0 {
            if let Some(annotation) = &section.annotation {
                line = format!("\"^{}\" {}", annotation, line);
            }
        }

        output.push_str(&line);
        output.push('\n');

        for (stream_index, (label, syllables)) in streams.iter_mut().enumerate() {
            let cursor = cursors[stream_index];
            if cursor >= syllables.len() && segment_index > 0 {
                continue;
            }

            let is_last_segment = segment_index + 1 == segments.len();
            // Never write more syllables than the line has notes — ABC would
            // drop the surplus and the text would slide out of alignment.
            let take = (*slots).min(syllables.len() - cursor);

            let mut chunk: Vec<Syllable> = syllables[cursor..cursor + take].to_vec();
            cursors[stream_index] = cursor + take;

            // A verse line shorter than the melody is padded with skip markers
            // so the following lines stay aligned.
            if !is_last_segment && chunk.len() < *slots {
                for _ in chunk.len()..*slots {
                    chunk.push(Syllable {
                        text: "*".to_string(),
                        joined: false,
                    });
                }
            }

            if chunk.is_empty() {
                continue;
            }

            // The verse number is bound to the first note with `~`.
            let prefix = if segment_index == 0 {
                label.clone().map(|l| format!("{}~", l)).unwrap_or_default()
            } else {
                String::new()
            };

            output.push_str(&format!("w:{}{}\n", prefix, render_syllables(&chunk)));
        }
    }

    // Text that has no notes left to sit on (a verse with more lines than the
    // melody has phrases) is kept as a comment rather than silently dropped or
    // pushed onto notes it does not belong to.
    for (stream_index, (label, syllables)) in streams.iter().enumerate() {
        let cursor = cursors[stream_index];
        if cursor >= syllables.len() {
            continue;
        }
        output.push_str(&format!(
            "% unaligned {}{}\n",
            label.clone().map(|l| format!("{} ", l)).unwrap_or_default(),
            render_syllables(&syllables[cursor..])
        ));
    }

    output
}

// ---------------------------------------------------------------------------
// Song → sections
// ---------------------------------------------------------------------------

/// Find voice content that is stored on the part itself (not inherited).
fn find_own_voice(part: &SongPart) -> Option<&SongPartContent> {
    part.contents.iter().find(|content| {
        matches!(
            content.voice_type,
            SongPartContentType::LeadVoice
                | SongPartContentType::SupranoVoice
                | SongPartContentType::AltoVoice
                | SongPartContentType::TenorVoice
                | SongPartContentType::BassVoice
        )
    })
}

/// Build the ABC header (everything up to and including the `K:` field).
fn build_header(song: &Song, settings: &AbcSettings) -> String {
    let mut header = String::new();
    header.push_str("X:1\n");
    header.push_str(&format!("T:{}\n", song.title));

    if let Some(author) = song.get_tag("author") {
        header.push_str(&format!("C:{}\n", author));
    }

    if let Some(time) = song.get_tag("time") {
        header.push_str(&format!("M:{}\n", time));
    } else {
        header.push_str("M:none\n");
    }

    header.push_str(&format!("L:{}\n", settings.unit_note_length));

    let key = song
        .get_tag("key")
        .map(|k| convert_key_to_abc(k))
        .unwrap_or_else(|| "C".to_string());
    header.push_str(&format!("K:{}\n", key));
    // A single-voice tune does not need a V: field, but declaring it keeps the
    // output usable as-is when further voices are added.
    header.push_str("V:1\n");

    header
}

/// Collect the stanza and refrain sections of a song in singing order.
fn build_sections(song: &Song, settings: &AbcSettings) -> Result<Vec<Section>, String> {
    use crate::song::SongPartType;

    let parts = song.get_unpacked_parts();

    let mut verse_parts: Vec<&SongPart> = parts
        .iter()
        .filter(|part| part.part_type == SongPartType::Verse)
        .collect();
    verse_parts.sort_by_key(|part| part.number);

    let refrain_parts: Vec<&SongPart> = parts
        .iter()
        .filter(|part| part.part_type.is_chorus_like())
        .collect();

    // The stanza melody; fall back to any part that carries a voice so that
    // refrain-only or single-block songs still export.
    let stanza_voice = verse_parts
        .iter()
        .find_map(|part| song.get_voice_for_part(part))
        .or_else(|| parts.iter().find_map(|part| song.get_voice_for_part(part)))
        .ok_or_else(|| "Song has no voice content for ABC export".to_string())?;

    let refrain_voice: Option<&SongPartContent> =
        refrain_parts.iter().find_map(|part| find_own_voice(part));

    // --- Verse lyrics ---
    let mut verse_lyrics: Vec<(Option<String>, Vec<Vec<Syllable>>)> = Vec::new();
    let mut verse_number = 1u32;
    for part in &verse_parts {
        for content in &part.contents {
            if content.voice_type.is_lyrics() {
                verse_lyrics.push((
                    Some(format!("{}.", verse_number)),
                    lyrics_to_lines(&content.content),
                ));
                verse_number += 1;
            }
        }
    }

    // Songs without verse parts (e.g. a lyrics-only refrain) still need lyrics.
    if verse_lyrics.is_empty() {
        for part in &parts {
            for content in &part.contents {
                if content.voice_type.is_lyrics() {
                    verse_lyrics.push((
                        Some(format!("{}.", verse_number)),
                        lyrics_to_lines(&content.content),
                    ));
                    verse_number += 1;
                }
            }
        }
    }

    if !settings.include_all_verses {
        verse_lyrics.truncate(1);
    }

    // --- Refrain lyrics ---
    let refrain_lyrics: Vec<Vec<Syllable>> = refrain_parts
        .iter()
        .find_map(|part| {
            part.contents
                .iter()
                .find(|content| content.voice_type.is_lyrics())
        })
        .map(|content| lyrics_to_lines(&content.content))
        .unwrap_or_default();

    let stanza_events = parse_voice(&stanza_voice.content);

    // A refrain without its own melody shares the stanza music; its text is
    // added as one more lyrics line underneath the stanza.
    if refrain_voice.is_none() {
        let mut lyrics = verse_lyrics;
        if !refrain_lyrics.is_empty() {
            lyrics.push((Some("R.".to_string()), refrain_lyrics));
        }
        return Ok(vec![Section {
            annotation: None,
            events: stanza_events,
            lyrics,
        }]);
    }

    let refrain_events = parse_voice(&refrain_voice.unwrap().content);

    let stanza_section = Section {
        annotation: Some("Stanza".to_string()),
        events: stanza_events,
        lyrics: verse_lyrics,
    };
    let refrain_section = Section {
        annotation: Some("Refrain".to_string()),
        events: refrain_events,
        lyrics: if refrain_lyrics.is_empty() {
            Vec::new()
        } else {
            vec![(None, refrain_lyrics)]
        },
    };

    let refrain_first = song
        .part_orders
        .first()
        .map_or(false, |order| order.is_refrain_first());

    Ok(if refrain_first {
        vec![refrain_section, stanza_section]
    } else {
        vec![stanza_section, refrain_section]
    })
}

/// Insert the bar lines the LilyPond source left out.
///
/// LilyPond treats `|` as an optional bar *check*, so hand written melodies
/// frequently omit some of them. In ABC bar lines are structural — a missing
/// one merges two measures — so every time a full measure has been filled and
/// no bar line follows, one is inserted.
///
/// The measure fill is carried across sections because a stanza that ends on an
/// incomplete bar is completed by the pickup of the refrain.
fn insert_missing_barlines(sections: &mut [Section], bar_length: Frac, initial_fill: Frac) {
    if bar_length.is_zero() {
        return;
    }

    let mut fill = initial_fill;
    let section_count = sections.len();

    for (section_index, section) in sections.iter_mut().enumerate() {
        let is_last_section = section_index + 1 == section_count;
        let events = std::mem::take(&mut section.events);
        let event_count = events.len();
        let mut rebarred: Vec<MusicEvent> = Vec::with_capacity(event_count);

        for (index, event) in events.into_iter().enumerate() {
            let duration = match &event {
                MusicEvent::Barline(_) => {
                    // Trust an explicit bar line and start a fresh measure.
                    fill = Frac::zero();
                    rebarred.push(event);
                    continue;
                }
                MusicEvent::Note { duration, .. } | MusicEvent::Rest { duration, .. } => *duration,
            };

            rebarred.push(event);

            fill = fill.add(duration);
            let mut crossed = false;
            while fill.at_least(bar_length) {
                fill = fill.sub(bar_length);
                crossed = true;
            }

            // Only a measure that ends exactly here needs a bar line.
            if !crossed || !fill.is_zero() {
                continue;
            }

            let is_final_event = is_last_section && index + 1 == event_count;
            let next_is_barline = matches!(rebarred.last(), Some(MusicEvent::Barline(_)));
            if is_final_event || next_is_barline {
                continue;
            }

            rebarred.push(MusicEvent::Barline("|".to_string()));
        }

        section.events = rebarred;
    }

    // A bar line inserted right before an explicit one would double up.
    for section in sections.iter_mut() {
        section.events.dedup_by(|a, b| {
            matches!((&*a, &*b), (MusicEvent::Barline(_), MusicEvent::Barline(second))
                if second == "|")
        });
    }
}

/// The length of one measure as a fraction of a whole note, from the `M:` tag.
fn meter_bar_length(song: &Song) -> Frac {
    match song.get_tag("time") {
        Some(time) => {
            let mut parts = time.trim().split('/');
            let beats: u32 = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            let unit: u32 = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            if beats == 0 || unit == 0 {
                Frac::zero()
            } else {
                Frac::new(beats, unit)
            }
        }
        None => Frac::zero(),
    }
}

/// How much of the first measure the anacrusis leaves empty.
fn initial_bar_fill(song: &Song, bar_length: Frac) -> Frac {
    match song.get_tag("partial").and_then(|p| p.trim().parse::<u32>().ok()) {
        Some(partial) if partial > 0 => bar_length.sub(Frac::new(1, partial)),
        _ => Frac::zero(),
    }
}

/// Append a closing bar line to the last section if it has none.
fn ensure_final_barline(sections: &mut [Section]) {
    if let Some(section) = sections.last_mut() {
        let has_final = matches!(
            section.events.last(),
            Some(MusicEvent::Barline(text)) if text == "|]"
        );
        if !has_final {
            // Drop a trailing plain bar line so we do not emit `| |]`.
            if matches!(section.events.last(), Some(MusicEvent::Barline(_))) {
                section.events.pop();
            }
            section.events.push(MusicEvent::Barline("|]".to_string()));
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate a complete ABC notation file from a [`Song`].
///
/// The result contains the header fields, the melody and one `w:` lyrics line
/// per verse aligned to the notes. If the refrain has its own melody it is
/// emitted as a separate, labelled section.
///
/// Returns an error if the song contains no voice content.
pub fn abc_from_song(song: &Song, settings: &AbcSettings) -> Result<String, String> {
    let mut output = build_header(song, settings);

    let unit = parse_unit_note_length(&settings.unit_note_length);
    let fifths = song.get_tag("key").map(|k| key_fifths(k)).unwrap_or(0);
    let key = key_alterations(fifths);

    let mut sections = build_sections(song, settings)?;
    let bar_length = meter_bar_length(song);
    insert_missing_barlines(&mut sections, bar_length, initial_bar_fill(song, bar_length));
    ensure_final_barline(&mut sections);

    let mut bar = BarAccidentals::default();
    for (index, section) in sections.iter().enumerate() {
        // A blank line would end the tune, so sections are separated by a
        // comment line instead.
        if index > 0 {
            output.push_str("%\n");
        }
        output.push_str(&render_section(section, &key, unit, &mut bar));
    }

    Ok(output)
}

/// Generate ABC notation with the whole melody on one line and the lyrics
/// stacked underneath it.
///
/// This is useful when the caller wants to re-flow the music themselves; the
/// syllable-to-note alignment is identical to [`abc_from_song`].
pub fn abc_from_song_separated(song: &Song, settings: &AbcSettings) -> Result<String, String> {
    let mut output = build_header(song, settings);

    let unit = parse_unit_note_length(&settings.unit_note_length);
    let fifths = song.get_tag("key").map(|k| key_fifths(k)).unwrap_or(0);
    let key = key_alterations(fifths);

    let mut sections = build_sections(song, settings)?;
    let bar_length = meter_bar_length(song);
    insert_missing_barlines(&mut sections, bar_length, initial_bar_fill(song, bar_length));
    ensure_final_barline(&mut sections);

    let mut bar = BarAccidentals::default();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            output.push_str("%\n");
        }

        let mut line = render_events(&section.events, &key, unit, &mut bar);
        if let Some(annotation) = &section.annotation {
            line = format!("\"^{}\" {}", annotation, line);
        }
        output.push_str(&line);
        output.push('\n');

        for (label, lines) in &section.lyrics {
            let syllables: Vec<Syllable> = lines.iter().flatten().cloned().collect();
            if syllables.is_empty() {
                continue;
            }
            let prefix = label.clone().map(|l| format!("{}~", l)).unwrap_or_default();
            output.push_str(&format!("w:{}{}\n", prefix, render_syllables(&syllables)));
        }
    }

    Ok(output)
}

/// Generate a standalone ABC tune for a single song part.
pub fn abc_part_from_song(
    song: &Song,
    part_number: usize,
    settings: &AbcSettings,
) -> Result<String, String> {
    let parts = song.get_unpacked_parts();

    let part = parts
        .get(part_number)
        .ok_or_else(|| format!("Part number {} out of range", part_number))?;

    let voice = song
        .get_voice_for_part(part)
        .ok_or_else(|| format!("Part {} has no voice content", part.id.get_id()))?;

    let unit = parse_unit_note_length(&settings.unit_note_length);
    let fifths = song.get_tag("key").map(|k| key_fifths(k)).unwrap_or(0);
    let key_alter = key_alterations(fifths);

    let mut output = String::new();
    output.push_str(&format!("X:{}\n", part_number + 1));
    output.push_str(&format!("T:{} - {}\n", song.title, part.id.get_id()));
    if let Some(author) = song.get_tag("author") {
        output.push_str(&format!("C:{}\n", author));
    }
    if let Some(time) = song.get_tag("time") {
        output.push_str(&format!("M:{}\n", time));
    } else {
        output.push_str("M:none\n");
    }
    output.push_str(&format!("L:{}\n", settings.unit_note_length));
    output.push_str(&format!(
        "K:{}\n",
        song.get_tag("key")
            .map(|k| convert_key_to_abc(k))
            .unwrap_or_else(|| "C".to_string())
    ));
    output.push_str("V:1\n");

    let lyrics: Vec<(Option<String>, Vec<Vec<Syllable>>)> = part
        .contents
        .iter()
        .filter(|content| content.voice_type.is_lyrics())
        .map(|content| (None, lyrics_to_lines(&content.content)))
        .collect();

    let mut sections = [Section {
        annotation: None,
        events: parse_voice(&voice.content),
        lyrics,
    }];
    let bar_length = meter_bar_length(song);
    insert_missing_barlines(&mut sections, bar_length, initial_bar_fill(song, bar_length));
    ensure_final_barline(&mut sections);

    let mut bar = BarAccidentals::default();
    output.push_str(&render_section(&sections[0], &key_alter, unit, &mut bar));

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::song_yml;
    use crate::song::{LyricLanguage, Song, SongPartContent, SongPartContentType, SongPartType};

    fn amazing_grace() -> Song {
        let content = std::fs::read_to_string("testfiles/Amazing Grace.song.yml").unwrap();
        song_yml::import_from_yml_string(&content).unwrap()
    }

    fn sei_nicht_stolz() -> Song {
        let content =
            std::fs::read_to_string("testfiles/Sei nicht stolz auf das, was du bist.song.yml")
                .unwrap();
        song_yml::import_from_yml_string(&content).unwrap()
    }

    // --- Header ---------------------------------------------------------

    #[test]
    fn test_abc_export_basic_header() {
        let abc = abc_from_song(&amazing_grace(), &AbcSettings::default()).unwrap();

        assert!(abc.contains("X:1"));
        assert!(abc.contains("T:Amazing Grace"));
        assert!(abc.contains("C:John Newton"));
        assert!(abc.contains("K:F"));
        assert!(abc.contains("M:3/4"));
        assert!(abc.contains("L:1/4"));
    }

    #[test]
    fn test_header_field_order() {
        let abc = abc_from_song(&amazing_grace(), &AbcSettings::default()).unwrap();
        let m = abc.find("M:").unwrap();
        let l = abc.find("L:").unwrap();
        let k = abc.find("K:").unwrap();
        assert!(m < l && l < k, "M: must precede L: which must precede K:");
    }

    // --- Key signatures -------------------------------------------------

    #[test]
    fn test_convert_key_to_abc() {
        assert_eq!(convert_key_to_abc("f major"), "F");
        assert_eq!(convert_key_to_abc("d minor"), "Dm");
        assert_eq!(convert_key_to_abc("C major"), "C");
        assert_eq!(convert_key_to_abc("G mixolydian"), "Gmix");
        assert_eq!(convert_key_to_abc("E dorian"), "Edor");
        assert_eq!(convert_key_to_abc("bes major"), "Bb");
        assert_eq!(convert_key_to_abc("fis minor"), "F#m");
    }

    #[test]
    fn test_key_fifths() {
        assert_eq!(key_fifths("c major"), 0);
        assert_eq!(key_fifths("d major"), 2);
        assert_eq!(key_fifths("f major"), -1);
        assert_eq!(key_fifths("bes major"), -2);
        assert_eq!(key_fifths("a minor"), 0);
        assert_eq!(key_fifths("d minor"), -1);
    }

    #[test]
    fn test_key_alterations() {
        // D major: F# and C#
        let alterations = key_alterations(2);
        assert_eq!(alterations[3], 1, "f should be sharp");
        assert_eq!(alterations[0], 1, "c should be sharp");
        assert_eq!(alterations[4], 0, "g stays natural");
    }

    // --- Durations ------------------------------------------------------

    #[test]
    fn test_parse_lilypond_duration() {
        assert_eq!(parse_lilypond_duration("4"), Some(Frac::new(1, 4)));
        assert_eq!(parse_lilypond_duration("8"), Some(Frac::new(1, 8)));
        assert_eq!(parse_lilypond_duration("2."), Some(Frac::new(3, 4)));
        assert_eq!(parse_lilypond_duration("4.."), Some(Frac::new(7, 16)));
        assert_eq!(parse_lilypond_duration(""), None);
    }

    #[test]
    fn test_render_duration_relative_to_unit() {
        let unit = Frac::new(1, 4);
        assert_eq!(render_duration(Frac::new(1, 4), unit), "");
        assert_eq!(render_duration(Frac::new(1, 2), unit), "2");
        assert_eq!(render_duration(Frac::new(1, 8), unit), "/");
        assert_eq!(render_duration(Frac::new(1, 16), unit), "/4");
        // A dotted half note is three quarter notes — never "2." in ABC.
        assert_eq!(render_duration(Frac::new(3, 4), unit), "3");
        // A dotted quarter note is one and a half quarter notes.
        assert_eq!(render_duration(Frac::new(3, 8), unit), "3/2");
    }

    #[test]
    fn test_duration_is_carried_over() {
        // In LilyPond a note without a duration reuses the previous one.
        let events = parse_voice("c8 d e f");
        let durations: Vec<Frac> = events
            .iter()
            .filter_map(|e| match e {
                MusicEvent::Note { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect();
        assert_eq!(durations, vec![Frac::new(1, 8); 4]);
    }

    // --- Pitches --------------------------------------------------------

    #[test]
    fn test_parse_note_name_accidentals() {
        assert_eq!(parse_note_name("c"), Some((0, 0, 1)));
        assert_eq!(parse_note_name("fis"), Some((3, 1, 3)));
        assert_eq!(parse_note_name("bes"), Some((6, -1, 3)));
        assert_eq!(parse_note_name("as"), Some((5, -1, 2)));
        assert_eq!(parse_note_name("es"), Some((2, -1, 2)));
        assert_eq!(parse_note_name("cisis"), Some((0, 2, 5)));
    }

    #[test]
    fn test_relative_octave_resolution() {
        // \relative c' { c d e f g a b c } stays within one ascending scale.
        let events = parse_voice("c4 d e f g a b c");
        let octaves: Vec<i32> = events
            .iter()
            .filter_map(|e| match e {
                MusicEvent::Note { pitches, .. } => Some(pitches[0].octave),
                _ => None,
            })
            .collect();
        assert_eq!(octaves, vec![4, 4, 4, 4, 4, 4, 4, 5]);
    }

    #[test]
    fn test_relative_octave_marks() {
        // The melody of "Sei nicht stolz": `a2.` is followed by `d,`, which has
        // to drop an octave below the d LilyPond would pick on its own.
        let events = parse_voice("d8 e fis a2. d,8 e");
        let pitches: Vec<Pitch> = events
            .iter()
            .filter_map(|e| match e {
                MusicEvent::Note { pitches, .. } => Some(pitches[0]),
                _ => None,
            })
            .collect();
        // \relative c' — d e fis all sit in the octave of middle C.
        assert_eq!(pitches[0].octave, 4);
        assert_eq!(pitches[3].octave, 4, "a after fis stays in the same octave");
        // Nearest d to a4 would be d5; the comma lowers it back to d4.
        assert_eq!(pitches[4].step, 1);
        assert_eq!(pitches[4].octave, 4);
    }

    #[test]
    fn test_no_lilypond_octave_marks_leak_into_output() {
        let abc = abc_from_song(&sei_nicht_stolz(), &AbcSettings::default()).unwrap();
        for line in abc.lines() {
            if line.starts_with("w:") || line.starts_with(|c: char| c.is_ascii_uppercase()) {
                continue;
            }
            assert!(
                !line.contains(",'"),
                "LilyPond octave syntax leaked into: {}",
                line
            );
        }
        // Lowercase note followed by a comma is not valid ABC.
        assert!(!abc.contains("d,"), "found LilyPond-style octave mark d,");
    }

    #[test]
    fn test_accidental_only_emitted_when_needed() {
        // D major has F# in the key signature, so `fis` needs no accidental.
        let mut song = Song::new("Key Test");
        song.add_tag("key", "d major");
        song.add_tag("time", "4/4");
        let part = song.add_part_of_type(SongPartType::Verse, Some(1));
        {
            let mut part = part.borrow_mut();
            part.add_content(SongPartContent {
                voice_type: SongPartContentType::LeadVoice,
                content: "fis4 f g a".to_string(),
            });
            part.add_content(SongPartContent {
                voice_type: SongPartContentType::Lyrics {
                    language: LyricLanguage::Default,
                },
                content: "one two three four".to_string(),
            });
        }

        let abc = abc_from_song(&song, &AbcSettings::default()).unwrap();
        let music = abc.lines().find(|l| l.starts_with('^') || l.contains('F')).unwrap();
        // fis -> F (from the key), f -> =F (natural sign needed).
        assert!(music.contains("=F"), "expected a natural sign in {}", music);
    }

    // --- Ties and slurs -------------------------------------------------

    #[test]
    fn test_tie_becomes_abc_dash() {
        let events = parse_voice("c2~ c2");
        match &events[0] {
            MusicEvent::Note { tie, .. } => assert!(*tie),
            other => panic!("expected a note, got {:?}", other),
        }
        let abc = render_events(
            &events,
            &key_alterations(0),
            Frac::new(1, 4),
            &mut BarAccidentals::default(),
        );
        assert_eq!(abc, "C2- C2");
    }

    #[test]
    fn test_slur_uses_abc_parentheses() {
        let events = parse_voice("c8( d)");
        let abc = render_events(
            &events,
            &key_alterations(0),
            Frac::new(1, 4),
            &mut BarAccidentals::default(),
        );
        // LilyPond puts `(` after the first note of the slur, ABC before it.
        assert_eq!(abc, "(C/ D/)");
    }

    #[test]
    fn test_slurred_and_tied_notes_do_not_consume_syllables() {
        let events = parse_voice("c4 d8( e) f4~ f4");
        let flags = slot_flags(&events);
        assert_eq!(flags.iter().filter(|f| **f).count(), 3);
    }

    // --- Bar lines and rests --------------------------------------------

    #[test]
    fn test_bar_lines_and_final_bar() {
        let events = parse_voice("c4 d4 | e4 f4 \\bar \"|.\"");
        let abc = render_events(
            &events,
            &key_alterations(0),
            Frac::new(1, 4),
            &mut BarAccidentals::default(),
        );
        assert_eq!(abc, "C D | E F |]");
    }

    #[test]
    fn test_rests() {
        let events = parse_voice("r4 r2 s4");
        let abc = render_events(
            &events,
            &key_alterations(0),
            Frac::new(1, 4),
            &mut BarAccidentals::default(),
        );
        assert_eq!(abc, "z z2 x");
    }

    #[test]
    fn test_lilypond_commands_are_dropped() {
        let events = parse_voice("c4 \\breathe d4 \\set foo = ##t\ne4");
        assert_eq!(events.iter().filter(|e| e.is_note()).count(), 3);
    }

    // --- Lyrics ---------------------------------------------------------

    #[test]
    fn test_syllable_splitting() {
        let syllables = lyrics_line_to_syllables("A -- ma -- zing grace");
        assert_eq!(syllables.len(), 4);
        assert_eq!(render_syllables(&syllables), "A-ma-zing grace");
    }

    #[test]
    fn test_syllable_splitting_without_spaces() {
        let syllables = lyrics_line_to_syllables("A--ma--zing grace");
        assert_eq!(render_syllables(&syllables), "A-ma-zing grace");
    }

    #[test]
    fn test_lyrics_commands_are_removed() {
        let syllables =
            lyrics_line_to_syllables("\\set ignoreMelismata = ##t Da -- rum \\unset ignoreMelismata");
        assert_eq!(render_syllables(&syllables), "Da-rum");
    }

    #[test]
    fn test_melisma_marker_survives() {
        let syllables = lyrics_line_to_syllables("weg -- ge -- tan. _");
        assert_eq!(render_syllables(&syllables), "weg-ge-tan. _");
    }

    #[test]
    fn test_every_verse_gets_its_own_w_line() {
        let abc = abc_from_song(&amazing_grace(), &AbcSettings::default()).unwrap();

        // Three verses × four lyric lines each.
        assert_eq!(abc.matches("w:").count(), 12);
        assert!(abc.contains("w:1.~A-ma-zing grace,"));
        assert!(abc.contains("w:2.~"));
        assert!(abc.contains("w:3.~"));
    }

    #[test]
    fn test_syllables_align_with_notes() {
        // Amazing Grace: every lyric line must match its music line exactly.
        let song = amazing_grace();
        let sections = build_sections(&song, &AbcSettings::default()).unwrap();
        let section = &sections[0];
        let reference: Vec<Vec<Syllable>> = section.lyrics[0].1.clone();
        let segments = segment_events(&section.events, &reference);

        assert_eq!(segments.len(), reference.len());
        for (index, (_, _, slots)) in segments.iter().enumerate() {
            assert_eq!(
                *slots,
                reference[index].len(),
                "line {} has {} notes but {} syllables",
                index + 1,
                slots,
                reference[index].len()
            );
        }
    }

    #[test]
    fn test_verse_number_only_on_first_line() {
        let abc = abc_from_song(&amazing_grace(), &AbcSettings::default()).unwrap();
        assert_eq!(abc.matches("w:1.~").count(), 1);
        assert_eq!(abc.matches("w:2.~").count(), 1);
    }

    #[test]
    fn test_only_first_verse_when_requested() {
        let settings = AbcSettings {
            include_all_verses: false,
            ..AbcSettings::default()
        };
        let abc = abc_from_song(&amazing_grace(), &settings).unwrap();
        assert_eq!(abc.matches("w:").count(), 4);
        assert!(abc.contains("w:1.~"));
        assert!(!abc.contains("w:2.~"));
    }

    #[test]
    fn test_custom_unit_note_length() {
        let settings = AbcSettings {
            unit_note_length: "1/8".to_string(),
            ..AbcSettings::default()
        };
        let abc = abc_from_song(&amazing_grace(), &settings).unwrap();
        assert!(abc.contains("L:1/8"));
        // With L:1/8 a quarter note is written as "2".
        assert!(abc.contains("C2"));
    }

    // --- Refrain --------------------------------------------------------

    #[test]
    fn test_refrain_with_own_melody_gets_its_own_section() {
        let abc = abc_from_song(&sei_nicht_stolz(), &AbcSettings::default()).unwrap();

        assert!(abc.contains("\"^Refrain\""), "refrain section missing:\n{}", abc);
        assert!(
            abc.contains("Denn wer sich rüh-men will,"),
            "refrain lyrics missing:\n{}",
            abc
        );
        // The refrain text must not be glued onto a stanza lyrics line.
        let stanza_line = abc
            .lines()
            .find(|l| l.starts_with("w:1.~"))
            .expect("first verse line");
        assert!(!stanza_line.contains("Denn wer sich"));
    }

    #[test]
    fn test_refrain_without_own_melody_is_extra_lyrics_line() {
        let yml = r#"
version: 0.1
title: Shared Melody
score:
  key: c major
  time: 4/4
parts:
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f
    - type: lyrics
      number: 1
      content: |
        one two three four
  - type: refrain
    contents:
    - type: lyrics
      number: 1
      content: |
        five six se -- ven
"#;
        let song = song_yml::import_from_yml_string(yml).unwrap();
        let abc = abc_from_song(&song, &AbcSettings::default()).unwrap();

        assert!(abc.contains("w:1.~one two three four"));
        assert!(abc.contains("w:R.~five six se-ven"));
        assert!(!abc.contains("\"^Refrain\""));
    }

    #[test]
    fn test_refrain_first_song_puts_refrain_first() {
        let yml = r#"
version: 0.1
title: Refrain First
score:
  key: c major
  time: 4/4
orders:
  - refrain-stanza-refrain
parts:
  - type: stanza
    contents:
    - type: voice
      number: 1
      content: |
        c4 d e f
    - type: lyrics
      number: 1
      content: |
        one two three four
  - type: refrain
    contents:
    - type: voice
      number: 1
      content: |
        g4 a b c
    - type: lyrics
      number: 1
      content: |
        five six se -- ven eight
"#;
        let song = song_yml::import_from_yml_string(yml).unwrap();
        let abc = abc_from_song(&song, &AbcSettings::default()).unwrap();

        let refrain_position = abc.find("\"^Refrain\"").expect("refrain section");
        let stanza_position = abc.find("\"^Stanza\"").expect("stanza section");
        assert!(refrain_position < stanza_position);
    }

    // --- Errors and misc ------------------------------------------------

    #[test]
    fn test_abc_export_no_voice_error() {
        let mut song = Song::new("Test No Voice");
        let part = song.add_part_of_type(SongPartType::Verse, Some(1));
        {
            let mut part = part.borrow_mut();
            part.add_content(SongPartContent {
                voice_type: SongPartContentType::Lyrics {
                    language: LyricLanguage::Default,
                },
                content: "Test lyrics".to_string(),
            });
        }

        let result = abc_from_song(&song, &AbcSettings::default());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no voice content"));
    }

    #[test]
    fn test_abc_part_export() {
        let abc = abc_part_from_song(&amazing_grace(), 0, &AbcSettings::default()).unwrap();
        assert!(abc.contains("X:1"));
        assert!(abc.contains("Amazing Grace"));
        assert!(abc.contains("w:"));
    }

    #[test]
    fn test_abc_separated_export() {
        let abc = abc_from_song_separated(&amazing_grace(), &AbcSettings::default()).unwrap();
        assert!(abc.contains("T:Amazing Grace"));
        assert!(abc.contains("w:1.~A-ma-zing"));
    }

    #[test]
    fn test_tune_ends_with_final_barline() {
        let abc = abc_from_song(&amazing_grace(), &AbcSettings::default()).unwrap();
        assert!(abc.contains("|]"), "missing final bar line:\n{}", abc);
    }
}
