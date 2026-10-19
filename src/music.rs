//! Music theory primitives shared by the notation exporters.
//!
//! Everything in here is notation agnostic: a [`Pitch`] knows its diatonic
//! step, its alteration and its octave, but nothing about how LilyPond or ABC
//! spell it. The exporters add that spelling, and the transposer
//! ([`crate::transpose`]) works on these types alone.
//!
//! The module also holds the small rational type the exporters use for note
//! durations ([`Frac`]) and the key signature arithmetic that both of them
//! need — the circle of fifths and the accidentals a key puts on each step.

use std::fmt;

// ---------------------------------------------------------------------------
// Rational numbers (note durations as a fraction of a whole note)
// ---------------------------------------------------------------------------

/// A non-negative rational number, always kept in lowest terms.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Frac {
    pub(crate) num: u32,
    pub(crate) den: u32,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

impl Frac {
    pub(crate) fn new(num: u32, den: u32) -> Frac {
        let den = if den == 0 { 1 } else { den };
        let g = gcd(num.max(1), den);
        Frac {
            num: num / g,
            den: den / g,
        }
    }

    pub(crate) fn zero() -> Frac {
        Frac { num: 0, den: 1 }
    }

    pub(crate) fn is_zero(self) -> bool {
        self.num == 0
    }

    /// `self / other`
    pub(crate) fn div(self, other: Frac) -> Frac {
        Frac::new(self.num * other.den, self.den * other.num)
    }

    pub(crate) fn add(self, other: Frac) -> Frac {
        Frac::new(
            self.num * other.den + other.num * self.den,
            self.den * other.den,
        )
    }

    /// `self - other`, saturating at zero.
    pub(crate) fn sub(self, other: Frac) -> Frac {
        let num = self.num * other.den;
        let subtrahend = other.num * self.den;
        if num <= subtrahend {
            Frac::zero()
        } else {
            Frac::new(num - subtrahend, self.den * other.den)
        }
    }

    /// `self >= other`
    pub(crate) fn at_least(self, other: Frac) -> bool {
        self.num * other.den >= other.num * self.den
    }
}

/// Convert a LilyPond duration such as `4`, `2.` or `8..` into a fraction of a
/// whole note. An empty string means "reuse the previous duration" and yields
/// `None`.
pub(crate) fn parse_lilypond_duration(text: &str) -> Option<Frac> {
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
// Pitches
// ---------------------------------------------------------------------------

/// The diatonic steps in order, `0` being c.
pub(crate) const STEP_LETTERS: [char; 7] = ['c', 'd', 'e', 'f', 'g', 'a', 'b'];

/// How many semitones each diatonic step sits above c.
const STEP_SEMITONES: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

pub(crate) fn letter_to_step(c: char) -> Option<u8> {
    STEP_LETTERS.iter().position(|l| *l == c).map(|p| p as u8)
}

/// A pitch in scientific notation: `step` 0..6 maps to c…b, `octave` 4 is the
/// octave of middle C.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pitch {
    pub(crate) step: u8,
    pub(crate) alter: i8,
    pub(crate) octave: i32,
}

impl Pitch {
    /// Build a pitch from its three components.
    pub(crate) fn new(step: u8, alter: i8, octave: i32) -> Pitch {
        Pitch {
            step,
            alter,
            octave,
        }
    }

    /// The pitch as a number of semitones above c0, so that two pitches can be
    /// compared and intervals between them measured.
    pub(crate) fn chroma(&self) -> i32 {
        STEP_SEMITONES[self.step as usize] + self.alter as i32 + 12 * self.octave
    }

    /// Spell this pitch the way LilyPond writes note names: Dutch names
    /// (`fis`, `bes`, `as`) followed by `'` or `,` octave marks relative to
    /// `reference_octave`.
    pub(crate) fn to_lilypond(self, reference_octave: i32) -> String {
        let mut name = note_name_to_lilypond(self.step, self.alter);
        let marks = self.octave - reference_octave;
        for _ in 0..marks {
            name.push('\'');
        }
        for _ in marks..0 {
            name.push(',');
        }
        name
    }

    /// Transpose by an interval, keeping the spelling the interval implies: a
    /// minor third up from `d` is `f`, not `eis`.
    pub(crate) fn transposed(&self, interval: Interval) -> Pitch {
        let total_step = self.step as i32 + interval.steps;
        let step = total_step.rem_euclid(7);
        let octave = self.octave + total_step.div_euclid(7);

        // The step and the octave are fixed by the interval's diatonic size;
        // the alteration takes up whatever is left to reach the target pitch.
        let target_chroma = self.chroma() + interval.semitones;
        let alter = target_chroma - (STEP_SEMITONES[step as usize] + 12 * octave);

        Pitch {
            step: step as u8,
            alter: alter as i8,
            octave,
        }
    }
}

/// Spell a step and an alteration as a LilyPond (Dutch) note name.
///
/// Flats take the long form LilyPond documents as canonical — `ees` and `aes`
/// rather than the `es`/`as` short forms it also accepts — so that a key
/// signature reads the way the LilyPond manual writes it.
pub(crate) fn note_name_to_lilypond(step: u8, alter: i8) -> String {
    let mut name = String::from(STEP_LETTERS[step as usize]);
    let suffix = if alter > 0 { "is" } else { "es" };
    for _ in 0..alter.abs() {
        name.push_str(suffix);
    }
    name
}

/// Parse the note name part of a LilyPond token (e.g. `fis`, `bes`, `ceses`).
/// Returns the diatonic step, the alteration and how many characters were used.
pub(crate) fn parse_note_name(token: &str) -> Option<(u8, i8, usize)> {
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

/// Read the `'` and `,` octave marks that follow a note name.
///
/// Returns the octave shift and how many characters it took.
pub(crate) fn parse_octave_marks(text: &str) -> (i32, usize) {
    let mut marks = 0i32;
    let mut consumed = 0usize;
    for c in text.chars() {
        match c {
            '\'' => {
                marks += 1;
                consumed += c.len_utf8();
            }
            ',' => {
                marks -= 1;
                consumed += c.len_utf8();
            }
            _ => break,
        }
    }
    (marks, consumed)
}

/// Resolve a LilyPond `\relative` pitch against the previous pitch.
///
/// LilyPond picks the octave that keeps the interval to the previous note at a
/// fourth or less; `'` and `,` shift the result by whole octaves.
pub(crate) fn resolve_relative_octave(previous: &Pitch, step: u8, octave_marks: i32) -> i32 {
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

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// A key signature: a tonic plus a mode.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Key {
    /// The tonic's diatonic step, `0` being c.
    pub(crate) step: u8,
    /// The tonic's alteration in semitones.
    pub(crate) alter: i8,
    /// The mode, lower case and spelled out: `"major"`, `"minor"`, `"dorian"`, …
    pub(crate) mode: String,
}

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
        "minor" | "aeolian" => -3,
        "dorian" => -2,
        "phrygian" => -4,
        "lydian" => 1,
        "mixolydian" => -1,
        "locrian" => -5,
        _ => 0, // major / ionian
    }
}

/// Expand the abbreviations the different formats use for a mode.
///
/// Returns `None` for a word that names no mode, which is how the key parser
/// tells `"Dm"` (d minor) from `"Dx"` (nothing it knows).
fn canonical_mode(text: &str) -> Option<&'static str> {
    Some(match text {
        "" | "major" | "maj" | "ionian" | "ion" => "major",
        "m" | "min" | "minor" | "aeolian" | "aeo" => "minor",
        "dor" | "dorian" => "dorian",
        "phr" | "phrygian" => "phrygian",
        "lyd" | "lydian" => "lydian",
        "mix" | "mixolydian" | "mixo" => "mixolydian",
        "loc" | "locrian" => "locrian",
        _ => return None,
    })
}

impl Key {
    /// Parse a key in any of the spellings the formats of this crate use.
    ///
    /// LilyPond writes `"bes major"` and `"fis minor"`, ABC writes `"Bb"` and
    /// `"F#m"`, and a person typing on the command line may write either, or
    /// `"b flat major"`. All of them are accepted; the mode may be attached to
    /// the tonic or stand as its own word, and an absent mode means major.
    ///
    /// ```
    /// use cantara_songlib::music::Key;
    /// assert_eq!(Key::parse("bes major").unwrap().to_lilypond(), "bes major");
    /// assert_eq!(Key::parse("Bb").unwrap().to_lilypond(), "bes major");
    /// assert_eq!(Key::parse("F#m").unwrap().to_lilypond(), "fis minor");
    /// assert_eq!(Key::parse("e flat minor").unwrap().to_lilypond(), "ees minor");
    /// ```
    pub fn parse(text: &str) -> Option<Key> {
        let lowered = text.trim().to_lowercase();
        let mut tokens = lowered.split_whitespace();
        let tonic = tokens.next()?;
        let remainder: Vec<&str> = tokens.collect();

        let mut chars = tonic.chars();
        let letter = chars.next()?;
        let step = letter_to_step(letter)?;
        let suffix: String = chars.collect();

        // The accidental may hang off the tonic (`bes`, `F#`) or be a word of
        // its own (`b flat`); whatever is left over has to name the mode.
        let (alter, mode_in_suffix) = split_accidental(letter, &suffix)?;
        let (alter, mode_words) = match (alter, remainder.first()) {
            (0, Some(&"flat")) if mode_in_suffix.is_empty() => (-1, &remainder[1..]),
            (0, Some(&"sharp")) if mode_in_suffix.is_empty() => (1, &remainder[1..]),
            _ => (alter, &remainder[..]),
        };

        // Either the tonic carries the mode or the words after it do, never both.
        let mode_text = if mode_in_suffix.is_empty() {
            mode_words.join(" ")
        } else if mode_words.is_empty() {
            mode_in_suffix.to_string()
        } else {
            return None;
        };

        Some(Key {
            step,
            alter,
            mode: canonical_mode(&mode_text)?.to_string(),
        })
    }

    /// The tonic as a pitch, in the octave below middle C — LilyPond's
    /// unmarked note name.
    pub(crate) fn tonic(&self) -> Pitch {
        Pitch::new(self.step, self.alter, 3)
    }

    /// Number of sharps (positive) or flats (negative) in the key signature.
    pub(crate) fn fifths(&self) -> i32 {
        natural_fifths(self.step) + self.alter as i32 * 7 + mode_fifths_offset(&self.mode)
    }

    /// The key in LilyPond spelling, e.g. `"ees major"`.
    pub fn to_lilypond(&self) -> String {
        format!("{} {}", note_name_to_lilypond(self.step, self.alter), self.mode)
    }

    /// The key as an ABC `K:` field, e.g. `"Eb"` or `"F#m"`.
    pub fn to_abc(&self) -> String {
        let mut tonic = STEP_LETTERS[self.step as usize]
            .to_ascii_uppercase()
            .to_string();
        match self.alter {
            1 => tonic.push('#'),
            2 => tonic.push_str("##"),
            -1 => tonic.push('b'),
            -2 => tonic.push_str("bb"),
            _ => {}
        }

        match self.mode.as_str() {
            "major" => tonic,
            "minor" => format!("{}m", tonic),
            "mixolydian" => format!("{}mix", tonic),
            "dorian" => format!("{}dor", tonic),
            "phrygian" => format!("{}phr", tonic),
            "lydian" => format!("{}lyd", tonic),
            "locrian" => format!("{}loc", tonic),
            _ => tonic,
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_lilypond())
    }
}

/// Split the accidental off a tonic's suffix, returning it and whatever is
/// left (which has to be the mode).
fn split_accidental(letter: char, suffix: &str) -> Option<(i8, &str)> {
    // Longest match first, so that `isis` is not read as `is` plus a mode.
    for (marker, alter) in [
        ("isis", 2),
        ("eses", -2),
        ("##", 2),
        ("bb", -2),
        ("is", 1),
        ("es", -1),
        ("#", 1),
        ("b", -1),
    ] {
        if let Some(rest) = suffix.strip_prefix(marker) {
            return Some((alter, rest));
        }
    }
    // `as` and `es` are LilyPond's short forms for a flat and e flat.
    if (letter == 'a' || letter == 'e')
        && let Some(rest) = suffix.strip_prefix('s')
    {
        return Some((-1, rest));
    }
    Some((0, suffix))
}

/// The alteration each diatonic step carries in a key with `fifths` accidentals.
pub(crate) fn key_alterations(fifths: i32) -> [i8; 7] {
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

// ---------------------------------------------------------------------------
// Intervals
// ---------------------------------------------------------------------------

/// Which way a transposition goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TransposeDirection {
    /// Raise the music to the next occurrence of the target key.
    Up,
    /// Lower the music to the previous occurrence of the target key.
    Down,
    /// Take whichever of the two moves the music least.
    #[default]
    Nearest,
}

impl fmt::Display for TransposeDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TransposeDirection::Up => "up",
            TransposeDirection::Down => "down",
            TransposeDirection::Nearest => "nearest",
        })
    }
}

/// A directed interval: how far to move on the staff and by how many semitones.
///
/// Both numbers are needed because they decide the spelling as well as the
/// sound. Three semitones up with two diatonic steps is a minor third — `d`
/// becomes `f` — while three semitones up with three steps is an augmented
/// second, which would write the same sound as `eis`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Interval {
    /// Diatonic steps, negative when the music goes down.
    pub(crate) steps: i32,
    /// Semitones, negative when the music goes down.
    pub(crate) semitones: i32,
}

impl Interval {
    /// The unison: transposing by it changes nothing.
    pub(crate) fn unison() -> Interval {
        Interval {
            steps: 0,
            semitones: 0,
        }
    }

    pub(crate) fn is_unison(&self) -> bool {
        self.steps == 0 && self.semitones == 0
    }

    /// How far the music moves, in semitones, regardless of direction.
    pub(crate) fn distance(&self) -> i32 {
        self.semitones.abs()
    }

    /// The interval that leads from `from` to `to` in the given direction.
    ///
    /// A key names a pitch class, not a pitch, so the same pair of keys gives
    /// two different intervals: `d major` → `f major` is a minor third up or a
    /// major sixth down. `direction` picks between them;
    /// [`TransposeDirection::Nearest`] takes the smaller move.
    pub(crate) fn between(from: &Key, to: &Key, direction: TransposeDirection) -> Interval {
        let up = Interval {
            steps: (to.step as i32 - from.step as i32).rem_euclid(7),
            semitones: (to.tonic().chroma() - from.tonic().chroma()).rem_euclid(12),
        };

        if up.is_unison() {
            // The same key: no direction moves the music anywhere.
            return Interval::unison();
        }

        // Going down to the same key is going up minus an octave.
        let down = Interval {
            steps: up.steps - 7,
            semitones: up.semitones - 12,
        };

        match direction {
            TransposeDirection::Up => up,
            TransposeDirection::Down => down,
            TransposeDirection::Nearest => {
                if down.distance() < up.distance() {
                    down
                } else {
                    up
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Pitches --------------------------------------------------------

    #[test]
    fn note_names_round_trip() {
        for name in ["c", "fis", "bes", "ees", "aes", "cisis", "geses", "b"] {
            let (step, alter, consumed) = parse_note_name(name).unwrap();
            assert_eq!(consumed, name.len(), "{} not fully consumed", name);
            assert_eq!(note_name_to_lilypond(step, alter), name);
        }
    }

    #[test]
    fn the_short_flat_names_are_read_and_written_long() {
        for (short, long) in [("as", "aes"), ("es", "ees"), ("ases", "aeses")] {
            let (step, alter, consumed) = parse_note_name(short).unwrap();
            assert_eq!(consumed, short.len(), "{} not fully consumed", short);
            assert_eq!(note_name_to_lilypond(step, alter), long);
        }
    }

    #[test]
    fn pitch_spelling_carries_octave_marks() {
        let pitch = Pitch::new(0, 0, 5);
        assert_eq!(pitch.to_lilypond(3), "c''");
        assert_eq!(pitch.to_lilypond(5), "c");
        assert_eq!(Pitch::new(3, 1, 2).to_lilypond(3), "fis,");
    }

    // --- Keys -----------------------------------------------------------

    #[test]
    fn keys_parse_from_every_spelling() {
        let cases = [
            ("d major", "d major"),
            ("D", "d major"),
            ("Dm", "d minor"),
            ("d minor", "d minor"),
            ("bes", "bes major"),
            ("Bb major", "bes major"),
            ("b flat minor", "bes minor"),
            ("F#m", "fis minor"),
            ("fis minor", "fis minor"),
            ("Eb", "ees major"),
            ("as major", "aes major"),
            ("e dorian", "e dorian"),
            ("Emix", "e mixolydian"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                Key::parse(input).map(|key| key.to_lilypond()),
                Some(expected.to_string()),
                "parsing {}",
                input
            );
        }
    }

    #[test]
    fn nonsense_keys_are_rejected() {
        assert_eq!(Key::parse(""), None);
        assert_eq!(Key::parse("H major"), None);
        assert_eq!(Key::parse("d wonky"), None);
        assert_eq!(Key::parse("Dm minor"), None);
    }

    #[test]
    fn keys_render_as_abc() {
        assert_eq!(Key::parse("f major").unwrap().to_abc(), "F");
        assert_eq!(Key::parse("d minor").unwrap().to_abc(), "Dm");
        assert_eq!(Key::parse("bes major").unwrap().to_abc(), "Bb");
        assert_eq!(Key::parse("fis minor").unwrap().to_abc(), "F#m");
        assert_eq!(Key::parse("e dorian").unwrap().to_abc(), "Edor");
    }

    #[test]
    fn key_fifths_follow_the_circle() {
        let fifths = |key: &str| Key::parse(key).unwrap().fifths();
        assert_eq!(fifths("c major"), 0);
        assert_eq!(fifths("d major"), 2);
        assert_eq!(fifths("f major"), -1);
        assert_eq!(fifths("bes major"), -2);
        assert_eq!(fifths("a minor"), 0);
        assert_eq!(fifths("d minor"), -1);
    }

    // --- Intervals ------------------------------------------------------

    #[test]
    fn interval_direction_picks_the_spelling() {
        let d = Key::parse("d major").unwrap();
        let f = Key::parse("f major").unwrap();

        let up = Interval::between(&d, &f, TransposeDirection::Up);
        assert_eq!(up, Interval { steps: 2, semitones: 3 });

        let down = Interval::between(&d, &f, TransposeDirection::Down);
        assert_eq!(down, Interval { steps: -5, semitones: -9 });

        // A minor third up is the shorter way round.
        assert_eq!(Interval::between(&d, &f, TransposeDirection::Nearest), up);
    }

    #[test]
    fn interval_to_the_same_key_is_a_unison() {
        let d = Key::parse("d major").unwrap();
        for direction in [
            TransposeDirection::Up,
            TransposeDirection::Down,
            TransposeDirection::Nearest,
        ] {
            assert!(Interval::between(&d, &d, direction).is_unison());
        }
    }

    #[test]
    fn transposing_keeps_the_interval_spelling() {
        let d = Key::parse("d major").unwrap();
        let f = Key::parse("f major").unwrap();
        let up = Interval::between(&d, &f, TransposeDirection::Up);

        // d' (the second line) becomes f', a minor third higher.
        let transposed = Pitch::new(1, 0, 4).transposed(up);
        assert_eq!(transposed, Pitch::new(3, 0, 4));

        // The leading note cis' becomes e', keeping the key's accidentals.
        assert_eq!(Pitch::new(0, 1, 4).transposed(up), Pitch::new(2, 0, 4));

        // Going down crosses the octave boundary.
        let down = Interval::between(&d, &f, TransposeDirection::Down);
        assert_eq!(Pitch::new(1, 0, 4).transposed(down), Pitch::new(3, 0, 3));
    }

    #[test]
    fn transposing_a_fifth_down_keeps_accidentals_in_range() {
        let g = Key::parse("g major").unwrap();
        let c = Key::parse("c major").unwrap();
        let down = Interval::between(&g, &c, TransposeDirection::Down);
        // fis' (the leading note of G) becomes b, the leading note of C.
        assert_eq!(Pitch::new(3, 1, 4).transposed(down), Pitch::new(6, 0, 3));
    }
}
