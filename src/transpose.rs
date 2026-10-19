//! Transposing a song into another key.
//!
//! A song is transposed by naming the key it should end up in and which way to
//! go — [`TransposeDirection::Up`] to raise it, [`TransposeDirection::Down`] to
//! lower it, or [`TransposeDirection::Nearest`] to move it as little as
//! possible. The target key alone is ambiguous: `d major` → `f major` is a
//! minor third up or a major sixth down, and for a congregation that is the
//! difference between a comfortable melody and an unsingable one.
//!
//! Everything the song carries is moved together — every voice, the chord line
//! and the key signature — so the result is a complete song in the new key
//! that every exporter can use:
//!
//! ```
//! use cantara_songlib::importer::song_yml;
//! use cantara_songlib::music::TransposeDirection;
//! use cantara_songlib::transpose::transpose_song;
//!
//! let yml = r#"
//! version: 0.1
//! title: Example
//! score:
//!   key: d major
//!   time: 4/4
//! parts:
//!   - type: stanza
//!     contents:
//!     - type: chords
//!       number: 1
//!       content: d2 a2
//!     - type: voice
//!       number: 1
//!       content: d'4 e fis g
//! "#;
//! let song = song_yml::import_from_yml_string(yml).unwrap();
//!
//! let lowered = transpose_song(&song, "bes major", TransposeDirection::Down).unwrap();
//! assert_eq!(lowered.score.key.as_deref(), Some("bes major"));
//! ```
//!
//! The spelling follows the key the song lands in rather than the notes it
//! came from: transposing out of `d major` into `f major` turns `cis` into
//! `e`, not into `fes`.

use crate::chords::transpose_chordmode;
use crate::music::{
    Interval, Key, parse_note_name, parse_octave_marks, Pitch, resolve_relative_octave,
    TransposeDirection,
};
use crate::song::{Song, SongPartContentType};

/// The octave of `\relative c'`, the wrapper every voice of a song is written
/// in: middle C.
const RELATIVE_REFERENCE: Pitch = Pitch {
    step: 0,
    alter: 0,
    octave: 4,
};

/// Transpose a song into `target_key`.
///
/// The key is accepted in any of the spellings the formats of this crate use —
/// `"ees major"`, `"Eb"`, `"e flat major"`, `"F#m"` — see [`Key::parse`].
///
/// Returns an error when the song has no key of its own (there is nothing to
/// transpose *from*) or when the target key cannot be read.
pub fn transpose_song(
    song: &Song,
    target_key: &str,
    direction: TransposeDirection,
) -> Result<Song, String> {
    let target = Key::parse(target_key)
        .ok_or_else(|| format!("'{}' is not a key I can read", target_key))?;

    let source_text = song.score.key.as_deref().ok_or_else(|| {
        "the song does not say which key it is in, so it cannot be transposed".to_string()
    })?;
    let source = Key::parse(source_text)
        .ok_or_else(|| format!("the song's key '{}' is not one I can read", source_text))?;

    let interval = Interval::between(&source, &target, direction);

    let mut transposed = song.clone();
    transposed.score.key = Some(target.to_lilypond());

    for part in transposed.parts_mut() {
        for content in part.contents.iter_mut() {
            if content.content_type.is_voice()
                || matches!(
                    content.content_type,
                    SongPartContentType::Instrumental | SongPartContentType::Solo
                )
            {
                content.content = transpose_voice(&content.content, interval);
            } else if matches!(content.content_type, SongPartContentType::Chords) {
                content.content = transpose_chordmode(&content.content, interval);
            }
        }
    }

    Ok(transposed)
}

/// Characters that end a note token — the same set the ABC exporter uses.
fn is_token_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '|' | '(' | ')' | '<' | '>' | '\\' | '"' | '[' | ']' | '{' | '}')
}

/// Where a `\relative` melody has got to, on both sides of the transposition.
///
/// The octave marks a note needs depend on the note before it, and that note
/// is a different one after transposing — so the input and the output each
/// need their own reference pitch.
struct RelativeCursor {
    input: Pitch,
    output: Pitch,
}

impl RelativeCursor {
    fn new() -> RelativeCursor {
        RelativeCursor {
            input: RELATIVE_REFERENCE,
            output: RELATIVE_REFERENCE,
        }
    }

    /// Resolve one written note against the input melody, transpose it and
    /// spell the result for the output melody.
    fn transpose_note(&mut self, step: u8, alter: i8, marks: i32, interval: Interval) -> String {
        let octave = resolve_relative_octave(&self.input, step, marks);
        let written = Pitch::new(step, alter, octave);
        self.input = written;

        let transposed = written.transposed(interval);
        // Spell the note against the octave LilyPond would infer for it here,
        // which is what turns the octave it needs into `'` and `,` marks.
        let inferred = resolve_relative_octave(&self.output, transposed.step, 0);
        self.output = transposed;
        transposed.to_lilypond(inferred)
    }
}

/// Commands whose argument is a note name and must not be read as one.
const COMMANDS_WITH_PITCH_ARGUMENT: [&str; 2] = ["key", "transposition"];

/// Transpose LilyPond voice content, leaving everything that is not a pitch
/// exactly as it was.
///
/// The content is read as if it were wrapped in `\relative c'`, which is how
/// the LilyPond exporter emits it and how the `.song.yml` format stores it.
/// Slurs, ties, durations, bar lines, breath marks, line breaks and comments
/// all come through untouched, so a transposed melody stays as readable as the
/// one it was written from:
///
/// ```text
/// a'2 fis4.( g8 ) | a2 d,     up a minor third becomes
/// c''2 a4.( bes8 ) | c2 f,
/// ```
pub(crate) fn transpose_voice(content: &str, interval: Interval) -> String {
    if interval.is_unison() {
        return content.to_string();
    }

    let bytes = content.as_bytes();
    let mut output = String::with_capacity(content.len());
    let mut copied = 0usize;
    let mut index = 0usize;
    let mut cursor = RelativeCursor::new();
    // Set while a `<…>` chord is being read: the first note of the chord is
    // what the note after the chord is measured against, so it is remembered
    // until the chord closes.
    let mut chord_first: Option<Option<(Pitch, Pitch)>> = None;

    while index < content.len() {
        let rest = &content[index..];
        let current = bytes[index] as char;

        match current {
            '%' => index += rest.find('\n').unwrap_or(rest.len()),
            '"' => {
                // A string argument (`\bar "|."`, a markup) holds no pitches.
                let end = rest[1..].find('"').map(|offset| index + offset + 2);
                index = end.unwrap_or(content.len());
            }
            '\\' => {
                let name_length = rest[1..]
                    .find(|c: char| !c.is_ascii_alphabetic())
                    .unwrap_or(rest.len() - 1);
                let command = &rest[1..1 + name_length];
                index += 1 + name_length.max(1);

                if matches!(command, "set" | "unset" | "override" | "revert") {
                    // Their arguments run to the end of the line and may look
                    // like note names (`\set Staff.instrumentName = …`).
                    index += content[index..].find('\n').unwrap_or(content.len() - index);
                } else if COMMANDS_WITH_PITCH_ARGUMENT.contains(&command) {
                    // `\key d \major`: the tonic is not part of the melody.
                    let remainder = &content[index..];
                    let leading = remainder.len() - remainder.trim_start().len();
                    let argument = remainder[leading..]
                        .find(|c: char| is_token_delimiter(c))
                        .unwrap_or(remainder.len() - leading);
                    index += leading + argument;
                }
            }
            '<' => {
                chord_first = Some(None);
                index += 1;
            }
            '>' => {
                if let Some(Some((first_input, first_output))) = chord_first {
                    cursor.input = first_input;
                    cursor.output = first_output;
                }
                chord_first = None;
                index += 1;
            }
            c if c.is_ascii_alphabetic() => {
                let token_length = rest
                    .find(|c: char| is_token_delimiter(c))
                    .unwrap_or(rest.len());
                let token = &rest[..token_length];
                let start = index;
                index += token_length;

                // A rest (`r`, `R`, `s`) carries no pitch, and a word that is
                // no note name at all (`treble`, `volta`) is left alone.
                if token.starts_with(['r', 'R', 's']) || parse_note_name(token).is_none() {
                    continue;
                }

                let (step, alter, name_length) = parse_note_name(token).unwrap();
                let (marks, mark_length) = parse_octave_marks(&token[name_length..]);

                let name = cursor.transpose_note(step, alter, marks, interval);
                // LilyPond measures the note after a chord against the chord's
                // *first* note, so that note has to survive the rest of the
                // chord.
                if matches!(chord_first, Some(None)) {
                    chord_first = Some(Some((cursor.input, cursor.output)));
                }

                output.push_str(&content[copied..start]);
                output.push_str(&name);
                output.push_str(&token[name_length + mark_length..]);
                copied = index;
            }
            _ => index += 1,
        }
    }

    output.push_str(&content[copied..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::song_yml;

    const TOCHTER_ZION_VOICE: &str = "a'2 fis4.( g8 ) | a2 d, | e8( fis g a g4 ) fis | e1 \\breathe |";

    fn interval(from: &str, to: &str, direction: TransposeDirection) -> Interval {
        Interval::between(
            &Key::parse(from).unwrap(),
            &Key::parse(to).unwrap(),
            direction,
        )
    }

    // --- Voices ---------------------------------------------------------

    #[test]
    fn a_melody_keeps_everything_but_its_pitches() {
        let up = interval("d major", "f major", TransposeDirection::Up);
        assert_eq!(
            transpose_voice(TOCHTER_ZION_VOICE, up),
            "c'2 a4.( bes8 ) | c2 f, | g8( a bes c bes4 ) a | g1 \\breathe |"
        );
    }

    #[test]
    fn transposing_down_keeps_the_melody_in_its_octave() {
        let down = interval("d major", "c major", TransposeDirection::Down);
        assert_eq!(
            transpose_voice("d'4 e fis g", down),
            "c'4 d e f"
        );
    }

    #[test]
    fn accidentals_follow_the_target_key() {
        // The leading note of D major is cis; in F major it is e.
        let up = interval("d major", "f major", TransposeDirection::Up);
        assert_eq!(transpose_voice("cis1 d", up), "e1 f");
        // Out of D into E flat the same note becomes d.
        let up_flat = interval("d major", "ees major", TransposeDirection::Up);
        assert_eq!(transpose_voice("cis1 d", up_flat), "d1 ees");
    }

    #[test]
    fn transposing_twice_is_the_same_as_not_at_all() {
        let up = interval("d major", "f major", TransposeDirection::Up);
        let down = interval("f major", "d major", TransposeDirection::Down);
        let there_and_back = transpose_voice(&transpose_voice(TOCHTER_ZION_VOICE, up), down);
        assert_eq!(there_and_back, TOCHTER_ZION_VOICE);
    }

    #[test]
    fn a_unison_leaves_the_text_byte_for_byte() {
        let same = interval("d major", "d major", TransposeDirection::Nearest);
        assert_eq!(transpose_voice(TOCHTER_ZION_VOICE, same), TOCHTER_ZION_VOICE);
    }

    #[test]
    fn rests_commands_and_strings_are_left_alone() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        assert_eq!(
            transpose_voice("\\clef treble c4 r4 s4 e2 \\bar \"|.\"", up),
            "\\clef treble d4 r4 s4 fis2 \\bar \"|.\""
        );
    }

    #[test]
    fn a_key_change_inside_a_melody_is_not_read_as_a_note() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        // The `d` after `\key` is the tonic, not the first note.
        assert_eq!(
            transpose_voice("\\key d \\major c4 d", up),
            "\\key d \\major d4 e"
        );
    }

    #[test]
    fn a_chord_is_measured_from_its_first_note() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        assert_eq!(transpose_voice("<c e g>2 c2", up), "<d fis a>2 d2");
    }

    // --- Whole songs ----------------------------------------------------

    fn tochter_zion() -> Song {
        let content = std::fs::read_to_string("tests/data/Tochter Zion.song.yml").unwrap();
        song_yml::import_from_yml_string(&content).unwrap()
    }

    #[test]
    fn a_song_moves_key_melody_and_chords_together() {
        let song = tochter_zion();
        let transposed = transpose_song(&song, "f major", TransposeDirection::Up).unwrap();

        assert_eq!(transposed.score.key.as_deref(), Some("f major"));

        let part = transposed.part(&"verse.1".parse().unwrap()).unwrap();
        let voice = part
            .contents
            .iter()
            .find(|content| content.content_type.is_voice())
            .unwrap();
        assert!(
            voice.content.starts_with("c'2 a4.( bes8 )"),
            "melody was not transposed: {}",
            &voice.content[..40]
        );

        let chords = part
            .contents
            .iter()
            .find(|content| matches!(content.content_type, SongPartContentType::Chords))
            .unwrap();
        assert!(
            chords.content.starts_with("f1 | f | c2. f4"),
            "chords were not transposed: {}",
            &chords.content[..20]
        );
    }

    #[test]
    fn the_lyrics_are_never_touched() {
        let song = tochter_zion();
        let transposed = transpose_song(&song, "bes major", TransposeDirection::Down).unwrap();

        let lyrics = |song: &Song| {
            song.part(&"verse.1".parse().unwrap())
                .unwrap()
                .contents
                .iter()
                .filter(|content| content.content_type.is_lyrics())
                .map(|content| content.content.clone())
                .collect::<Vec<String>>()
        };
        assert_eq!(lyrics(&transposed), lyrics(&song));
    }

    #[test]
    fn an_unreadable_or_missing_key_is_reported() {
        let song = tochter_zion();
        assert!(transpose_song(&song, "H major", TransposeDirection::Up).is_err());

        let mut keyless = song.clone();
        keyless.score.key = None;
        assert!(transpose_song(&keyless, "f major", TransposeDirection::Up).is_err());
    }
}
