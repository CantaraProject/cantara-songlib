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

impl fmt::Display for Frac {
    /// As LilyPond and ABC write a fraction: `3/4`, or just `4` for a whole
    /// number.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.den {
            1 => write!(f, "{}", self.num),
            den => write!(f, "{}/{}", self.num, den),
        }
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

/// Characters a note token may carry after its name and octave marks:
/// durations (`4`, `2.`, `4*2/3`), accidental reminders (`!`, `?`) and
/// articulation markers that are not commands.
fn is_note_suffix_char(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '.' | '*' | '/' | '!' | '?' | '-' | '_' | '^' | '+')
}

/// Read a token that has to be a note from end to end.
///
/// [`parse_note_name`] recognises a *prefix*, which is all a note needs — but
/// it also matches the beginning of any word that happens to start with a note
/// letter. `\clef bass` would become a `b` followed by the leftover `ass`,
/// and transposing it would write `cisass` into the score. A note is therefore
/// only accepted when everything after its name and octave marks could belong
/// to a note as well, which no word of prose can manage.
///
/// Returns the step, the alteration, the octave marks and how many bytes the
/// name and the marks took, so that a caller can keep the suffix as it is.
pub(crate) fn parse_note_token(token: &str) -> Option<(u8, i8, i32, usize)> {
    let (step, alter, name_length) = parse_note_name(token)?;
    let (marks, mark_length) = parse_octave_marks(&token[name_length..]);
    let consumed = name_length + mark_length;

    if !token[consumed..].chars().all(is_note_suffix_char) {
        return None;
    }

    Some((step, alter, marks, consumed))
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
// Scanning LilyPond source
// ---------------------------------------------------------------------------

/// Characters that end a note token.
///
/// A tie binds two notes without needing a space between them, so `~` ends a
/// token as well — otherwise `c4~c4` would be read as a single note and the
/// second one would never be looked at.
pub(crate) fn is_token_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '|' | '(' | ')' | '<' | '>' | '\\' | '"' | '[' | ']' | '{' | '}' | '~' | '%'
        )
}

/// The commands that set a property, together with the arguments they take.
///
/// These are the ones whose arguments read like music without being music:
/// `\set Staff.autoBeaming = ##f` holds an `a` and a `b` that are no more
/// notes than the `e` in "tempo" is.
const PROPERTY_COMMANDS: [&str; 5] = ["set", "unset", "override", "revert", "tweak"];

/// The commands that take a single word which is not a note of the melody: a
/// clef name, a rehearsal mark, a tempo indication, a repeat kind, the tonic
/// of a key signature.
const WORD_ARGUMENT_COMMANDS: [&str; 9] = [
    "clef",
    "mark",
    "tempo",
    "time",
    "partial",
    "repeat",
    "tuplet",
    "transposition",
    "key",
];

/// Whether a command's arguments have to be skipped rather than read as music,
/// and how far — see [`skip_command_arguments`].
pub(crate) fn command_takes_arguments(command: &str) -> bool {
    PROPERTY_COMMANDS.contains(&command) || WORD_ARGUMENT_COMMANDS.contains(&command)
}

/// Skip the arguments of a command that carries no music, returning the byte
/// index just past them.
///
/// Only the arguments are skipped, never the rest of the line: music may
/// follow a property on the same line, and `\set Staff.autoBeaming = ##f c4 d`
/// still has to transpose its four notes.
///
/// * A property command takes a path, optionally `=` and a value. A Scheme
///   value may be parenthesised (`#'()`, `#(list 1 2)`), so parentheses are
///   balanced rather than counted as one word.
/// * Every other command in the list takes a single word.
pub(crate) fn skip_command_arguments(content: &str, command: &str, from: usize) -> usize {
    let mut index = skip_argument_word(content, from);

    if PROPERTY_COMMANDS.contains(&command) {
        let after_path = skip_whitespace(content, index);
        if content[after_path..].starts_with('=') {
            index = skip_argument_word(content, after_path + 1);
        }
    }

    index
}

fn skip_whitespace(content: &str, from: usize) -> usize {
    let remainder = &content[from..];
    from + (remainder.len() - remainder.trim_start().len())
}

/// Skip one whitespace-separated argument, keeping parentheses balanced so
/// that a Scheme expression counts as a single word.
fn skip_argument_word(content: &str, from: usize) -> usize {
    let mut index = skip_whitespace(content, from);
    let mut depth = 0usize;

    for c in content[index..].chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 && (c.is_whitespace() || c == '%') => break,
            _ => {}
        }
        index += c.len_utf8();
        if depth == 0 && c == ')' {
            break;
        }
    }

    index
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
        parse_key_with_explicit_mode(text).map(|(key, _)| key)
    }

    /// The same key in another mode.
    pub(crate) fn with_mode(&self, mode: &str) -> Key {
        Key {
            step: self.step,
            alter: self.alter,
            mode: mode.to_string(),
        }
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

/// Parse a key, reporting whether its mode was spelled out.
///
/// `"F"` names a tonic and leaves the mode open, while `"f major"` and `"Dm"`
/// state it. The difference matters to the transposer: a bare tonic takes the
/// mode of the song being transposed, so `--transpose F` works for a song in
/// D minor as well as one in D major.
pub(crate) fn parse_key_with_explicit_mode(text: &str) -> Option<(Key, bool)> {
    let lowered = text.trim().to_lowercase();
    let mut tokens = lowered.split_whitespace();
    let tonic = tokens.next()?;
    let remainder: Vec<&str> = tokens.collect();

    let mut chars = tonic.chars();
    let letter = chars.next()?;
    let step = letter_to_step(letter)?;
    let suffix: String = chars.collect();

    // The accidental may hang off the tonic (`bes`, `F#`) or be a word of its
    // own (`b flat`); whatever is left over has to name the mode.
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

    let key = Key {
        step,
        alter,
        mode: canonical_mode(&mode_text)?.to_string(),
    };
    Some((key, !mode_text.is_empty()))
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
    pub(crate) fn is_unison(&self) -> bool {
        self.steps == 0 && self.semitones == 0
    }

    /// How far the music moves, in semitones, regardless of direction.
    pub(crate) fn distance(&self) -> i32 {
        self.semitones.abs()
    }

    /// Move this interval by whole octaves.
    ///
    /// An octave is seven diatonic steps and twelve semitones; shifting only
    /// one of them would describe an interval that does not exist, and a pitch
    /// transposed by it would come out spelled with a pile of accidentals.
    fn shifted(self, octaves: i32) -> Interval {
        Interval {
            steps: self.steps + 7 * octaves,
            semitones: self.semitones + 12 * octaves,
        }
    }

    /// The interval that leads from `from` to `to` in the given direction.
    ///
    /// A key names a pitch class, not a pitch, so the same pair of keys gives
    /// two different intervals: `d major` → `f major` is a minor third up or a
    /// major sixth down. `direction` picks between them;
    /// [`TransposeDirection::Nearest`] takes the smaller move.
    ///
    /// Two keys that name the same pitch — `ges major` and `fis major` — are a
    /// respelling rather than a move, so every direction leaves the pitches
    /// where they are and only changes how they are written.
    pub(crate) fn between(from: &Key, to: &Key, direction: TransposeDirection) -> Interval {
        // The interval to the target tonic written in the same octave as the
        // one the music starts from. It may point either way, and it is the
        // only place the two tonics are compared — the octave correction below
        // moves both components together, so the interval stays one that can
        // actually be spelled.
        let plain = Interval {
            steps: to.step as i32 - from.step as i32,
            semitones: to.tonic().chroma() - from.tonic().chroma(),
        };

        match direction {
            // A target that sounds the same as the source is a respelling: no
            // direction has anywhere to move it.
            _ if plain.semitones == 0 => plain,
            TransposeDirection::Up if plain.semitones < 0 => plain.shifted(1),
            TransposeDirection::Up => plain,
            TransposeDirection::Down if plain.semitones > 0 => plain.shifted(-1),
            TransposeDirection::Down => plain,
            TransposeDirection::Nearest => {
                let other = plain.shifted(if plain.semitones > 0 { -1 } else { 1 });
                if other.distance() < plain.distance() {
                    other
                } else {
                    plain
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
    fn an_octave_correction_moves_both_halves_of_an_interval() {
        // From C sharp major down to C major: the step count has to follow the
        // octave the semitones land in, or the result is spelled as a C with a
        // pile of sharps instead of a C natural.
        let cis = Key::parse("cis major").unwrap();
        let c = Key::parse("c major").unwrap();

        let down = Interval::between(&cis, &c, TransposeDirection::Down);
        assert_eq!(Pitch::new(0, 1, 4).transposed(down), Pitch::new(0, 0, 4));

        let up = Interval::between(&cis, &c, TransposeDirection::Up);
        assert_eq!(Pitch::new(0, 1, 4).transposed(up), Pitch::new(0, 0, 5));
    }

    #[test]
    fn two_spellings_of_the_same_key_only_respell_the_music() {
        // G flat major and F sharp major sound alike, so there is nothing to
        // move in either direction — only the spelling changes.
        let ges = Key::parse("ges major").unwrap();
        let fis = Key::parse("fis major").unwrap();

        for direction in [
            TransposeDirection::Up,
            TransposeDirection::Down,
            TransposeDirection::Nearest,
        ] {
            let interval = Interval::between(&ges, &fis, direction);
            assert_eq!(interval.semitones, 0, "{} moved the music", direction);
            // ges' becomes fis', the same sound written differently.
            assert_eq!(
                Pitch::new(4, -1, 4).transposed(interval),
                Pitch::new(3, 1, 4),
                "{}",
                direction
            );
        }
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
