//! Chord symbols: reading the LilyPond `\chordmode` notation a song stores,
//! and writing it back out as chord symbols for other formats.
//!
//! A song keeps its chords the way LilyPond writes them — `d1`, `b2:m`,
//! `e2:m7`, `a4:7/cis` — because that is the notation the `.song.yml` format
//! uses and LilyPond can typeset it unchanged. Every other exporter needs
//! something else: ABC wants a chord *symbol* (`Bm`, `Em7`, `A7/C#`) placed in
//! front of the note it sounds over, and the transposer needs to rewrite the
//! roots while leaving durations and chord qualities alone.
//!
//! Both of those work from [`parse_chordmode`], which turns a chord line into
//! a list of [`ChordItem`]s: what the chord is, how long it lasts and where it
//! sits in the source text.

use crate::music::{
    Frac, Interval, note_name_to_lilypond, parse_lilypond_duration, parse_note_name,
    parse_octave_marks, Pitch, STEP_LETTERS,
};

/// The octave an unmarked note name denotes in LilyPond's absolute mode: the
/// one below middle C. `\chordmode` is not wrapped in `\relative`, so `c` is
/// always this c.
const ABSOLUTE_REFERENCE_OCTAVE: i32 = 3;

/// A bass note written after a chord, i.e. LilyPond's `/e` (an inversion) or
/// `/+e` (an added bass note).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ChordBass {
    pub(crate) pitch: Pitch,
    /// `true` for `/+`, which adds the note instead of moving it to the bass.
    pub(crate) added: bool,
}

/// What a token of a chord line is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum ChordKind {
    /// A chord: a root plus LilyPond's quality modifiers.
    Chord {
        root: Pitch,
        /// The duration exactly as written, so that rewriting the line does
        /// not turn an inherited duration into an explicit one.
        duration_text: String,
        /// Everything after the `:`, e.g. `m`, `7`, `maj7`, `m7.5-`.
        modifiers: String,
        bass: Option<ChordBass>,
        /// The chord is tied to the following one (`~`).
        tie: bool,
    },
    /// A rest: no chord sounds here.
    Rest,
    /// A bar line. It takes no time but marks where a measure ends.
    Barline,
}

/// One token of a chord line, with the span it occupies in the source text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ChordItem {
    pub(crate) kind: ChordKind,
    /// How long the token sounds, as a fraction of a whole note.
    pub(crate) duration: Frac,
    /// Byte range of the token in the parsed content.
    pub(crate) span: (usize, usize),
}

/// Characters that may appear inside the modifier part of a chord.
fn is_modifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-' | '^')
}

/// Parse a chord line written in LilyPond `\chordmode` syntax.
///
/// Durations behave as they do in LilyPond: a chord without one lasts as long
/// as the chord before it. `\commands`, comments and anything else the chord
/// line carries are skipped — they neither sound nor take time.
pub(crate) fn parse_chordmode(content: &str) -> Vec<ChordItem> {
    let bytes = content.as_bytes();
    let mut items: Vec<ChordItem> = Vec::new();
    let mut index = 0usize;
    let mut last_duration = Frac::new(1, 4);

    while index < content.len() {
        let rest = &content[index..];
        let current = bytes[index] as char;

        if current.is_whitespace() {
            index += 1;
            continue;
        }

        match current {
            '%' => {
                // A comment runs to the end of the line.
                index += rest.find('\n').unwrap_or(rest.len());
            }
            '\\' => {
                // A command (`\global`, `\set …`) and its arguments on this
                // line carry no chord.
                let end = rest[1..]
                    .find(|c: char| !c.is_ascii_alphabetic())
                    .map(|offset| index + 1 + offset)
                    .unwrap_or(content.len());
                index = end.max(index + 1);
            }
            '|' => {
                let start = index;
                while index < content.len() && bytes[index] == b'|' {
                    index += 1;
                }
                items.push(ChordItem {
                    kind: ChordKind::Barline,
                    duration: Frac::zero(),
                    span: (start, index),
                });
            }
            c if c.is_ascii_alphabetic() => {
                let start = index;
                match parse_chord_token(rest, &mut last_duration) {
                    Some((kind, duration, consumed)) => {
                        index += consumed;
                        items.push(ChordItem {
                            kind,
                            duration,
                            span: (start, index),
                        });
                    }
                    None => index += 1,
                }
            }
            _ => index += 1,
        }
    }

    items
}

/// Parse one chord (or rest) token, returning it and how many bytes it took.
fn parse_chord_token(
    text: &str,
    last_duration: &mut Frac,
) -> Option<(ChordKind, Frac, usize)> {
    // A rest (`r`) or a spacer (`s`) means no chord sounds; both are followed
    // by nothing but an optional duration.
    let first = text.chars().next()?;
    if matches!(first, 'r' | 's' | 'R') {
        let digits: String = text[1..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let after = &text[1 + digits.len()..];
        if after.is_empty() || after.starts_with(|c: char| c.is_whitespace() || c == '|') {
            let duration = parse_lilypond_duration(&digits).unwrap_or(*last_duration);
            *last_duration = duration;
            return Some((ChordKind::Rest, duration, 1 + digits.len()));
        }
    }

    let (step, alter, mut consumed) = parse_note_name(text)?;
    let (marks, mark_length) = parse_octave_marks(&text[consumed..]);
    consumed += mark_length;

    let duration_text: String = text[consumed..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    consumed += duration_text.len();

    let mut tie = false;
    if text[consumed..].starts_with('~') {
        tie = true;
        consumed += 1;
    }

    let mut modifiers = String::new();
    if text[consumed..].starts_with(':') {
        consumed += 1;
        modifiers = text[consumed..].chars().take_while(|c| is_modifier_char(*c)).collect();
        consumed += modifiers.len();
    }

    let mut bass = None;
    if text[consumed..].starts_with('/') {
        let mut offset = consumed + 1;
        let added = text[offset..].starts_with('+');
        if added {
            offset += 1;
        }
        if let Some((bass_step, bass_alter, bass_length)) = parse_note_name(&text[offset..]) {
            offset += bass_length;
            let (bass_marks, bass_mark_length) = parse_octave_marks(&text[offset..]);
            offset += bass_mark_length;
            bass = Some(ChordBass {
                pitch: Pitch::new(
                    bass_step,
                    bass_alter,
                    ABSOLUTE_REFERENCE_OCTAVE + bass_marks,
                ),
                added,
            });
            consumed = offset;
        }
    }

    if !tie && text[consumed..].starts_with('~') {
        tie = true;
        consumed += 1;
    }

    let duration = parse_lilypond_duration(&duration_text).unwrap_or(*last_duration);
    *last_duration = duration;

    Some((
        ChordKind::Chord {
            root: Pitch::new(step, alter, ABSOLUTE_REFERENCE_OCTAVE + marks),
            duration_text,
            modifiers,
            bass,
            tie,
        },
        duration,
        consumed,
    ))
}

// ---------------------------------------------------------------------------
// Chord symbols
// ---------------------------------------------------------------------------

/// Spell a pitch as the root of a chord symbol: `D`, `F#`, `Bb`.
fn root_symbol(pitch: &Pitch) -> String {
    let mut symbol = STEP_LETTERS[pitch.step as usize]
        .to_ascii_uppercase()
        .to_string();
    let accidental = if pitch.alter > 0 { '#' } else { 'b' };
    for _ in 0..pitch.alter.abs() {
        symbol.push(accidental);
    }
    symbol
}

/// Translate the main quality of a LilyPond modifier list.
fn quality_symbol(text: &str) -> String {
    match text {
        // A bare `5` is LilyPond's way of spelling the plain triad.
        "" | "5" | "maj5" => String::new(),
        "m" | "min" | "m5" => "m".to_string(),
        "maj" => "maj7".to_string(),
        "sus" => "sus4".to_string(),
        "aug" => "aug".to_string(),
        "dim" => "dim".to_string(),
        _ => text.to_string(),
    }
}

/// Translate one alteration of a LilyPond modifier list: `5-` → `b5`.
///
/// A plain number adds that note to the chord without changing its quality,
/// which chord symbols write as `add9`.
fn alteration_symbol(text: &str) -> String {
    if let Some(degree) = text.strip_suffix('-') {
        format!("b{}", degree)
    } else if let Some(degree) = text.strip_suffix('+') {
        format!("#{}", degree)
    } else if text.chars().all(|c| c.is_ascii_digit()) {
        format!("add{}", text)
    } else {
        text.to_string()
    }
}

/// Translate a LilyPond modifier list into the suffix of a chord symbol.
///
/// ```text
/// m       → m        maj7    → maj7     m7.5-  → m7b5
/// 7       → 7        sus4    → sus4     7.9-   → 7b9
/// ```
///
/// Notes LilyPond *removes* from a chord (`13^11`) have no place in a chord
/// symbol, so they are dropped together with the `^`.
fn modifiers_to_symbol(modifiers: &str) -> String {
    let kept = modifiers.split('^').next().unwrap_or("");
    let mut symbol = String::new();
    for (index, part) in kept.split('.').enumerate() {
        if index == 0 {
            symbol.push_str(&quality_symbol(part));
        } else if !part.is_empty() {
            symbol.push_str(&alteration_symbol(part));
        }
    }
    symbol
}

/// The chord symbol of a chord token, e.g. `Bm`, `D7`, `A7/C#`.
///
/// Returns `None` for a rest or a bar line — neither names a chord.
pub(crate) fn chord_symbol(kind: &ChordKind) -> Option<String> {
    let ChordKind::Chord {
        root,
        modifiers,
        bass,
        ..
    } = kind
    else {
        return None;
    };

    let mut symbol = root_symbol(root);
    symbol.push_str(&modifiers_to_symbol(modifiers));
    if let Some(bass) = bass {
        symbol.push('/');
        symbol.push_str(&root_symbol(&bass.pitch));
    }
    Some(symbol)
}

/// Where a chord starts and what it is called.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct TimedChord {
    /// Distance from the start of the chord line, as a fraction of a whole note.
    pub(crate) start: Frac,
    /// The chord symbol to print, e.g. `Bm`.
    pub(crate) symbol: String,
}

/// Lay a chord line out in time, so that each symbol can be placed over the
/// note it sounds with.
///
/// A chord that merely repeats the one before it is dropped: printing the same
/// symbol over and over is noise, and both LilyPond and ABC renderers show a
/// held chord by leaving the space empty. A rest ends the chord, so the same
/// chord after a rest is printed again.
pub(crate) fn timed_chords(items: &[ChordItem]) -> Vec<TimedChord> {
    let mut chords: Vec<TimedChord> = Vec::new();
    let mut time = Frac::zero();
    let mut previous: Option<String> = None;

    for item in items {
        match &item.kind {
            ChordKind::Barline => continue,
            ChordKind::Rest => previous = None,
            kind @ ChordKind::Chord { .. } => {
                if let Some(symbol) = chord_symbol(kind) {
                    if previous.as_deref() != Some(symbol.as_str()) {
                        chords.push(TimedChord {
                            start: time,
                            symbol: symbol.clone(),
                        });
                    }
                    previous = Some(symbol);
                }
            }
        }
        time = time.add(item.duration);
    }

    chords
}

/// Read a chord line and lay it out in time in one step.
pub(crate) fn chords_from_content(content: &str) -> Vec<TimedChord> {
    timed_chords(&parse_chordmode(content))
}

// ---------------------------------------------------------------------------
// Transposition
// ---------------------------------------------------------------------------

/// Write a chord token back out in LilyPond `\chordmode` syntax.
///
/// The root is written as a plain note name. A chord line names chords, and in
/// which octave the root happens to be written changes neither the name nor
/// the symbol — so dropping the octave marks keeps the line readable instead
/// of pushing it up and down the octaves as it is transposed.
fn render_chordmode_token(kind: &ChordKind) -> Option<String> {
    let ChordKind::Chord {
        root,
        duration_text,
        modifiers,
        bass,
        tie,
    } = kind
    else {
        return None;
    };

    let mut text = note_name_to_lilypond(root.step, root.alter);
    text.push_str(duration_text);
    if !modifiers.is_empty() {
        text.push(':');
        text.push_str(modifiers);
    }
    if let Some(bass) = bass {
        text.push('/');
        if bass.added {
            text.push('+');
        }
        text.push_str(&note_name_to_lilypond(bass.pitch.step, bass.pitch.alter));
    }
    if *tie {
        text.push('~');
    }
    Some(text)
}

/// Transpose a chord line, keeping everything that is not a chord root intact.
///
/// Only the roots and the bass notes move; durations, chord qualities, bar
/// lines, line breaks and comments come out exactly as they went in, so the
/// transposed line is still as readable as the one the song was written with.
///
/// ```text
/// d1 | d | a2. d4 | b2:m d2:7      transposed up a minor third becomes
/// f1 | f | c2. f4 | d2:m f2:7
/// ```
pub(crate) fn transpose_chordmode(content: &str, interval: Interval) -> String {
    if interval.is_unison() {
        return content.to_string();
    }

    let mut result = String::with_capacity(content.len());
    let mut copied = 0usize;

    for item in parse_chordmode(content) {
        let ChordKind::Chord {
            root,
            duration_text,
            modifiers,
            bass,
            tie,
        } = &item.kind
        else {
            continue;
        };

        let transposed = ChordKind::Chord {
            root: root.transposed(interval),
            duration_text: duration_text.clone(),
            modifiers: modifiers.clone(),
            bass: bass.as_ref().map(|bass| ChordBass {
                pitch: bass.pitch.transposed(interval),
                added: bass.added,
            }),
            tie: *tie,
        };

        let (start, end) = item.span;
        result.push_str(&content[copied..start]);
        if let Some(text) = render_chordmode_token(&transposed) {
            result.push_str(&text);
        }
        copied = end;
    }

    result.push_str(&content[copied..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::{Key, TransposeDirection};

    /// The chord line of "Tochter Zion", first system.
    const TOCHTER_ZION: &str = "d1 | d | a2. d4 | a1 | d2. a4 | b2:m d2:7 | g2 a | d1";

    fn symbols(content: &str) -> Vec<String> {
        chords_from_content(content)
            .into_iter()
            .map(|chord| chord.symbol)
            .collect()
    }

    // --- Parsing --------------------------------------------------------

    #[test]
    fn chord_line_becomes_symbols() {
        assert_eq!(
            symbols(TOCHTER_ZION),
            vec!["D", "A", "D", "A", "D", "A", "Bm", "D7", "G", "A", "D"]
        );
    }

    #[test]
    fn repeated_chords_are_printed_once() {
        // The second `d` of `d1 | d` is the same chord held over the bar line.
        assert_eq!(symbols("d1 | d | a2 d2"), vec!["D", "A", "D"]);
        // A rest ends the chord, so the same chord after it is printed again.
        assert_eq!(symbols("d1 | r1 | d1"), vec!["D", "D"]);
    }

    #[test]
    fn durations_place_the_chords_in_time() {
        let chords = chords_from_content("d1 | a2. d4");
        let starts: Vec<Frac> = chords.iter().map(|chord| chord.start).collect();
        assert_eq!(
            starts,
            vec![Frac::new(0, 1), Frac::new(1, 1), Frac::new(7, 4)]
        );
    }

    #[test]
    fn a_chord_without_a_duration_inherits_the_previous_one() {
        let chords = chords_from_content("g2 a d2:m");
        assert_eq!(
            chords.iter().map(|c| c.start).collect::<Vec<Frac>>(),
            vec![Frac::new(0, 1), Frac::new(1, 2), Frac::new(1, 1)]
        );
    }

    #[test]
    fn commands_and_comments_carry_no_chord() {
        assert_eq!(symbols("\\global\n% the refrain\nd1 e:m"), vec!["D", "Em"]);
    }

    // --- Symbols --------------------------------------------------------

    #[test]
    fn qualities_become_chord_symbols() {
        let cases = [
            ("c", "C"),
            ("c:5", "C"),
            ("cis:m", "C#m"),
            ("bes:7", "Bb7"),
            ("c:maj7", "Cmaj7"),
            ("c:maj", "Cmaj7"),
            ("c:m7", "Cm7"),
            ("c:dim", "Cdim"),
            ("c:aug", "Caug"),
            ("c:sus4", "Csus4"),
            ("c:sus", "Csus4"),
            ("c:sus2", "Csus2"),
            ("c:6", "C6"),
            ("c:m7.5-", "Cm7b5"),
            ("c:7.9-", "C7b9"),
            ("c:5.9", "Cadd9"),
            ("c:13^11", "C13"),
        ];
        for (chordmode, expected) in cases {
            assert_eq!(symbols(chordmode), vec![expected], "for {}", chordmode);
        }
    }

    #[test]
    fn a_bass_note_is_kept() {
        assert_eq!(symbols("a2:7/cis"), vec!["A7/C#"]);
        assert_eq!(symbols("a2:7/+cis"), vec!["A7/C#"]);
    }

    // --- Transposition --------------------------------------------------

    fn interval(from: &str, to: &str, direction: TransposeDirection) -> Interval {
        Interval::between(
            &Key::parse(from).unwrap(),
            &Key::parse(to).unwrap(),
            direction,
        )
    }

    #[test]
    fn transposing_keeps_the_layout_of_the_line() {
        let up = interval("d major", "f major", TransposeDirection::Up);
        assert_eq!(
            transpose_chordmode(TOCHTER_ZION, up),
            "f1 | f | c2. f4 | c1 | f2. c4 | d2:m f2:7 | bes2 c | f1"
        );
    }

    #[test]
    fn transposing_keeps_qualities_bass_notes_and_ties() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        assert_eq!(
            transpose_chordmode("c1:maj7 a2:m7.5-/+e g4:7~ g4", up),
            "d1:maj7 b2:m7.5-/+fis a4:7~ a4"
        );
    }

    #[test]
    fn transposing_down_spells_the_key_it_lands_in() {
        let down = interval("d major", "bes major", TransposeDirection::Down);
        assert_eq!(transpose_chordmode("d1 a2 fis2:m", down), "bes1 f2 d2:m");
    }

    #[test]
    fn transposing_to_the_same_key_changes_nothing() {
        let same = interval("d major", "d major", TransposeDirection::Nearest);
        assert_eq!(transpose_chordmode(TOCHTER_ZION, same), TOCHTER_ZION);
    }
}
