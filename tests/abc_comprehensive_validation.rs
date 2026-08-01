//! Comprehensive ABC notation validation tests.
//!
//! These tests re-parse the generated ABC so that the output is checked against
//! the actual ABC grammar instead of against string patterns: bar lengths have
//! to match the meter, and every `w:` line has to carry exactly as many
//! syllables as the music line above it has singable notes.

use cantara_songlib::exporter::abc::{abc_from_song, AbcSettings};
use cantara_songlib::importer::song_yml;
use cantara_songlib::song::Song;

const TEST_FILES: [&str; 2] = [
    "tests/data/Amazing Grace.song.yml",
    "tests/data/Sei nicht stolz auf das, was du bist.song.yml",
];

fn load(path: &str) -> Song {
    let content =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {}: {}", path, e));
    song_yml::import_from_yml_string(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", path, e))
}

// ---------------------------------------------------------------------------
// A minimal ABC reader used to verify the generated output
// ---------------------------------------------------------------------------

/// A duration expressed in multiples of the unit note length, as a fraction.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Ratio {
    num: u64,
    den: u64,
}

impl Ratio {
    fn new(num: u64, den: u64) -> Ratio {
        fn gcd(a: u64, b: u64) -> u64 {
            if b == 0 {
                a.max(1)
            } else {
                gcd(b, a % b)
            }
        }
        let g = gcd(num.max(1), den.max(1));
        Ratio {
            num: num / g,
            den: den.max(1) / g,
        }
    }

    fn add(self, other: Ratio) -> Ratio {
        Ratio::new(self.num * other.den + other.num * self.den, self.den * other.den)
    }

    fn is_zero(self) -> bool {
        self.num == 0
    }
}

/// What a single music token contributes.
#[derive(Debug, PartialEq)]
enum Token {
    /// A note or rest with its duration; `singable` is false for rests and for
    /// notes that continue a tie or sit inside a slur.
    Sound { duration: Ratio, singable: bool },
    Barline,
}

/// Strip `"…"` annotations and decorations from a music line.
fn strip_annotations(line: &str) -> String {
    let mut result = String::new();
    let mut in_quotes = false;
    for c in line.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            _ if in_quotes => {}
            _ => result.push(c),
        }
    }
    result
}

/// Read the trailing length modifier of a note (`2`, `/`, `/4`, `3/2`, …).
fn read_duration(chars: &[char], index: &mut usize) -> Ratio {
    let mut num = String::new();
    while *index < chars.len() && chars[*index].is_ascii_digit() {
        num.push(chars[*index]);
        *index += 1;
    }

    let mut den = String::new();
    let mut slashes = 0u32;
    while *index < chars.len() && chars[*index] == '/' {
        slashes += 1;
        *index += 1;
        den.clear();
        while *index < chars.len() && chars[*index].is_ascii_digit() {
            den.push(chars[*index]);
            *index += 1;
        }
    }

    let numerator: u64 = num.parse().unwrap_or(1);
    let denominator: u64 = if !den.is_empty() {
        den.parse().unwrap_or(1)
    } else if slashes > 0 {
        2u64.pow(slashes)
    } else {
        1
    };

    Ratio::new(numerator, denominator)
}

/// Tokenize an ABC music line.
fn tokenize_music(line: &str) -> Vec<Token> {
    let chars: Vec<char> = strip_annotations(line).chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    let mut in_slur = false;
    let mut slur_opening = false;
    let mut tie_pending = false;

    while index < chars.len() {
        let c = chars[index];

        if c.is_whitespace() {
            index += 1;
            continue;
        }

        // Bar lines in every spelling this exporter can emit.
        if c == '|' || (c == ':' && chars.get(index + 1) == Some(&'|')) {
            while index < chars.len() && matches!(chars[index], '|' | ':' | ']' | '[') {
                index += 1;
            }
            tokens.push(Token::Barline);
            continue;
        }

        if c == '(' {
            // The first note of a slur still carries its own syllable; only the
            // notes after it are melismas.
            slur_opening = true;
            index += 1;
            continue;
        }

        if c == ')' {
            index += 1;
            continue;
        }

        // A chord counts as a single sound.
        if c == '[' {
            index += 1;
            while index < chars.len() && chars[index] != ']' {
                index += 1;
            }
            index += 1;
            let duration = read_duration(&chars, &mut index);
            let singable = !tie_pending && !in_slur;
            if slur_opening {
                in_slur = true;
                slur_opening = false;
            }
            tie_pending = chars.get(index) == Some(&'-');
            tokens.push(Token::Sound { duration, singable });
            continue;
        }

        // Accidentals precede the note letter.
        let mut is_note = false;
        let start = index;
        while index < chars.len() && matches!(chars[index], '^' | '_' | '=') {
            index += 1;
        }
        if index < chars.len() && chars[index].is_ascii_alphabetic() {
            let letter = chars[index];
            if letter.is_ascii_alphabetic() && "ABCDEFGabcdefgzx".contains(letter) {
                is_note = true;
                index += 1;
                while index < chars.len() && matches!(chars[index], ',' | '\'') {
                    index += 1;
                }
            }
        }

        if !is_note {
            index = start + 1;
            continue;
        }

        let letter = chars[start..index]
            .iter()
            .find(|c| c.is_ascii_alphabetic())
            .copied()
            .unwrap();
        let duration = read_duration(&chars, &mut index);
        let is_rest = letter == 'z' || letter == 'x';

        let singable = !is_rest && !tie_pending && !in_slur;
        if slur_opening {
            in_slur = true;
            slur_opening = false;
        }
        tie_pending = false;

        // A trailing '-' ties this note to the next one.
        if index < chars.len() && chars[index] == '-' {
            tie_pending = true;
            index += 1;
        }
        // A closing parenthesis ends the slur after this note.
        if index < chars.len() && chars[index] == ')' {
            in_slur = false;
            index += 1;
        }

        tokens.push(Token::Sound { duration, singable });
    }

    tokens
}

/// Number of syllables in a `w:` line. `*`, `_` and `-` are alignment markers.
fn count_syllables(line: &str) -> usize {
    let body = line.trim_start_matches("w:");
    let mut count = 0usize;
    for word in body.split_whitespace() {
        // Unescaped hyphens split a word into further syllables.
        let mut chars = word.chars().peekable();
        let mut current = String::new();
        let mut parts = 0usize;
        while let Some(c) = chars.next() {
            if c == '\\' {
                chars.next();
                current.push('x');
            } else if c == '-' {
                parts += 1;
                current.clear();
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() || parts == 0 {
            parts += 1;
        }
        count += parts;
    }
    count
}

/// Meter of the tune expressed in unit note lengths.
fn bar_length(abc: &str) -> Ratio {
    let meter = abc
        .lines()
        .find(|l| l.starts_with("M:"))
        .map(|l| l.trim_start_matches("M:").trim().to_string())
        .expect("no meter field");
    let unit = abc
        .lines()
        .find(|l| l.starts_with("L:"))
        .map(|l| l.trim_start_matches("L:").trim().to_string())
        .expect("no unit note length field");

    fn parse(text: &str) -> Ratio {
        let mut parts = text.split('/');
        let num: u64 = parts.next().unwrap().trim().parse().unwrap_or(1);
        let den: u64 = parts.next().unwrap_or("1").trim().parse().unwrap_or(1);
        Ratio::new(num, den)
    }

    let meter = parse(&meter);
    let unit = parse(&unit);
    // meter / unit
    Ratio::new(meter.num * unit.den, meter.den * unit.num)
}

/// Split the tune body into music lines with the `w:` lines belonging to them.
fn music_lines(abc: &str) -> Vec<(String, Vec<String>)> {
    let mut result: Vec<(String, Vec<String>)> = Vec::new();
    for line in abc.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('%') {
            continue;
        }
        if trimmed.starts_with("w:") {
            result
                .last_mut()
                .expect("lyrics line before any music line")
                .1
                .push(trimmed.to_string());
            continue;
        }
        // Header fields are `X:` style single letters followed by a colon.
        let is_header = trimmed.len() > 1
            && trimmed.chars().next().unwrap().is_ascii_alphabetic()
            && trimmed.chars().nth(1) == Some(':');
        if is_header {
            continue;
        }
        result.push((trimmed.to_string(), Vec::new()));
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_abc_header_format() {
    let abc = abc_from_song(&load(TEST_FILES[0]), &AbcSettings::default()).unwrap();
    let lines: Vec<&str> = abc.lines().collect();

    assert!(lines[0].starts_with("X:"));
    assert!(lines[1].starts_with("T:"));
    assert!(lines[2].starts_with("C:"));
    assert!(lines[3].starts_with("M:"));
    assert!(lines[4].starts_with("L:"));
    assert!(lines[5].starts_with("K:"));
    assert!(lines[6].starts_with("V:"));
}

#[test]
fn test_abc_voice_section() {
    let abc = abc_from_song(&load(TEST_FILES[0]), &AbcSettings::default()).unwrap();

    let key_position = abc.find("K:F").expect("key signature");
    let voice_position = abc.find("V:1").expect("voice declaration");
    assert!(voice_position > key_position);

    assert!(abc.contains('|'), "missing bar lines");
    assert!(abc.contains("|]"), "missing final bar line");
}

/// Every complete bar has to hold exactly one meter's worth of notes.
///
/// This is the strongest check on the LilyPond conversion: it only passes if
/// durations, dotted notes and the duration carry-over are all correct.
#[test]
fn test_bars_match_the_meter() {
    for path in TEST_FILES {
        let abc = abc_from_song(&load(path), &AbcSettings::default()).unwrap();
        let expected = bar_length(&abc);

        let mut bars: Vec<Ratio> = Vec::new();
        let mut current = Ratio::new(0, 1);
        for (music, _) in music_lines(&abc) {
            for token in tokenize_music(&music) {
                match token {
                    Token::Barline => {
                        bars.push(current);
                        current = Ratio::new(0, 1);
                    }
                    Token::Sound { duration, .. } => current = current.add(duration),
                }
            }
        }
        bars.push(current);
        bars.retain(|bar| !bar.is_zero());

        assert!(bars.len() > 4, "{}: suspiciously few bars", path);

        // The first bar may be an anacrusis and the last one may complete it,
        // so only the inner bars are checked against the meter.
        for (index, bar) in bars.iter().enumerate().skip(1).take(bars.len() - 2) {
            assert_eq!(
                *bar, expected,
                "{}: bar {} holds {:?} instead of {:?}\n{}",
                path,
                index + 1,
                bar,
                expected,
                abc
            );
        }
    }
}

/// Each `w:` line has to carry exactly as many syllables as its music line has
/// singable notes, otherwise the text drifts away from the melody.
#[test]
fn test_lyrics_align_with_notes() {
    for path in TEST_FILES {
        let abc = abc_from_song(&load(path), &AbcSettings::default()).unwrap();

        for (music, lyrics) in music_lines(&abc) {
            let singable = tokenize_music(&music)
                .iter()
                .filter(|token| matches!(token, Token::Sound { singable: true, .. }))
                .count();

            for lyric_line in &lyrics {
                let syllables = count_syllables(lyric_line);
                assert!(
                    syllables <= singable,
                    "{}: '{}' has {} syllables but only {} notes are available in '{}'",
                    path,
                    lyric_line,
                    syllables,
                    singable,
                    music
                );
            }
        }
    }
}

#[test]
fn test_every_music_line_has_lyrics() {
    for path in TEST_FILES {
        let abc = abc_from_song(&load(path), &AbcSettings::default()).unwrap();
        for (music, lyrics) in music_lines(&abc) {
            assert!(
                !lyrics.is_empty(),
                "{}: music line '{}' has no lyrics",
                path,
                music
            );
        }
    }
}

#[test]
fn test_abc_lyrics_format() {
    let abc = abc_from_song(&load(TEST_FILES[0]), &AbcSettings::default()).unwrap();

    // Three verses, four lines each — one `w:` line per verse and music line.
    let w_lines: Vec<&str> = abc.lines().filter(|l| l.starts_with("w:")).collect();
    assert_eq!(w_lines.len(), 12);

    // Verse numbers are bound to the first note with `~` and appear only once.
    assert_eq!(abc.matches("w:1.~").count(), 1);
    assert_eq!(abc.matches("w:2.~").count(), 1);
    assert_eq!(abc.matches("w:3.~").count(), 1);

    // ABC uses single hyphens between syllables.
    assert!(!abc.contains("--"), "lyrics must not contain LilyPond '--'");
    assert!(abc.contains("A-ma-zing"));
}

#[test]
fn test_slurs_and_brackets_are_balanced() {
    for path in TEST_FILES {
        let abc = abc_from_song(&load(path), &AbcSettings::default()).unwrap();
        let music: String = music_lines(&abc)
            .iter()
            .map(|(m, _)| strip_annotations(m).replace("|]", "|").replace("[|", "|"))
            .collect::<Vec<String>>()
            .join(" ");

        assert_eq!(
            music.matches('(').count(),
            music.matches(')').count(),
            "{}: unbalanced slur parentheses",
            path
        );
        assert_eq!(
            music.matches('[').count(),
            music.matches(']').count(),
            "{}: unbalanced chord brackets",
            path
        );
    }
}

#[test]
fn test_refrain_is_exported() {
    let abc = abc_from_song(&load(TEST_FILES[1]), &AbcSettings::default()).unwrap();

    assert!(
        abc.contains("\"^Refrain\""),
        "the refrain section is missing:\n{}",
        abc
    );
    assert!(
        abc.contains("Denn wer sich rüh-men will,"),
        "the refrain lyrics are missing:\n{}",
        abc
    );
}

#[test]
fn test_abc_settings_application() {
    let song = load(TEST_FILES[0]);

    for unit_length in ["1/4", "1/8", "1/2"] {
        let settings = AbcSettings {
            unit_note_length: unit_length.to_string(),
            ..AbcSettings::default()
        };
        let abc = abc_from_song(&song, &settings).unwrap();
        assert!(abc.contains(&format!("L:{}", unit_length)));

        // Whatever the unit is, the bars still have to add up.
        let expected = bar_length(&abc);
        let mut current = Ratio::new(0, 1);
        let mut bars = Vec::new();
        for (music, _) in music_lines(&abc) {
            for token in tokenize_music(&music) {
                match token {
                    Token::Barline => {
                        bars.push(current);
                        current = Ratio::new(0, 1);
                    }
                    Token::Sound { duration, .. } => current = current.add(duration),
                }
            }
        }
        bars.push(current);
        bars.retain(|bar| !bar.is_zero());
        for bar in bars.iter().skip(1).take(bars.len() - 2) {
            assert_eq!(*bar, expected, "L:{} produced a wrong bar length", unit_length);
        }
    }

    let first_only = AbcSettings {
        include_all_verses: false,
        ..AbcSettings::default()
    };
    let abc = abc_from_song(&song, &first_only).unwrap();
    assert_eq!(abc.lines().filter(|l| l.starts_with("w:")).count(), 4);
    assert!(!abc.contains("w:2.~"));
}

#[test]
fn test_abc_no_lilypond_artifacts() {
    for path in TEST_FILES {
        let abc = abc_from_song(&load(path), &AbcSettings::default()).unwrap();

        for pattern in [
            "\\breathe",
            "\\bar",
            "\\time",
            "\\key",
            "\\partial",
            "\\relative",
            "\\set",
            "\\unset",
            "##t",
            "ignoreMelismata",
        ] {
            assert!(
                !abc.contains(pattern),
                "{}: found LilyPond artifact {}",
                path,
                pattern
            );
        }

        // LilyPond octave marks on lowercase notes are not valid ABC.
        for (music, _) in music_lines(&abc) {
            for (index, c) in music.char_indices() {
                if c == ',' {
                    let previous = music[..index].chars().last().unwrap_or(' ');
                    assert!(
                        !previous.is_ascii_lowercase() || !"abcdefg".contains(previous),
                        "{}: lowercase octave mark in '{}'",
                        path,
                        music
                    );
                }
            }
        }
    }
}

#[test]
fn test_abc_multiple_songs_validation() {
    for path in TEST_FILES {
        let abc = abc_from_song(&load(path), &AbcSettings::default()).unwrap();

        assert!(abc.starts_with("X:"), "{}: missing X header", path);
        assert!(abc.contains("T:"), "{}: missing T header", path);
        assert!(abc.contains("K:"), "{}: missing K header", path);
        assert!(abc.contains("V:1"), "{}: missing voice", path);
        assert!(abc.contains("w:"), "{}: missing lyrics", path);
    }
}
