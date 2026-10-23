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
    command_takes_arguments, is_token_delimiter, parse_key_with_explicit_mode, parse_note_token,
    resolve_relative_octave, skip_command_arguments, Interval, Key, Pitch, TransposeDirection,
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
    let (target, mode_was_given) = parse_key_with_explicit_mode(target_key)
        .ok_or_else(|| format!("'{}' is not a key I can read", target_key))?;

    let source_text = song.score.key.as_deref().ok_or_else(|| {
        "the song does not say which key it is in, so it cannot be transposed".to_string()
    })?;
    let source = Key::parse(source_text)
        .ok_or_else(|| format!("the song's key '{}' is not one I can read", source_text))?;

    // Transposing shifts every note by the same interval, which leaves the
    // mode where it was: a major song stays major. A target that only names a
    // tonic therefore takes the song's own mode — `F` for a song in D minor
    // means F minor — while a target that insists on another mode is asking
    // for something else entirely and is better refused than answered with a
    // key signature that contradicts the notes under it.
    let target = match mode_was_given {
        true if target.mode != source.mode => {
            return Err(format!(
                "transposing moves every note by the same interval, so a song in {} \
                 cannot become one in {}; name a key in {} instead",
                source, target, source.mode
            ));
        }
        true => target,
        false => target.with_mode(&source.mode),
    };

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

/// Transpose the tonic of an inline `\key` change.
///
/// The key signature in the middle of a melody has to move with the notes
/// around it: `\key g \major` in a melody transposed a whole tone up is
/// `\key a \major`, or the signature would contradict the notes under it.
///
/// Returns where the tonic starts and what it becomes, where there was one to
/// read, together with the byte index just past the argument. The tonic is
/// written in absolute spelling and does not touch the `\relative` cursor — it
/// is a key, not a note of the melody.
fn transpose_key_argument(
    content: &str,
    from: usize,
    interval: Interval,
) -> (Option<(usize, String)>, usize) {
    let remainder = &content[from..];
    let leading = remainder.len() - remainder.trim_start().len();
    let start = from + leading;

    let token_end = content[start..]
        .find(|c: char| is_token_delimiter(c))
        .map(|offset| start + offset)
        .unwrap_or(content.len());
    let token = &content[start..token_end];

    match parse_note_token(token) {
        Some((step, alter, marks, consumed)) => {
            let tonic = Pitch::new(step, alter, KEY_REFERENCE_OCTAVE + marks)
                .transposed(interval)
                .to_lilypond(KEY_REFERENCE_OCTAVE);
            (
                Some((start, format!("{}{}", tonic, &token[consumed..]))),
                token_end,
            )
        }
        // Not a tonic we can read; leave it alone rather than guess.
        None => (None, token_end),
    }
}

/// The octave a key's tonic is written in. A key signature has no octave, so
/// the tonic is spelled without marks.
const KEY_REFERENCE_OCTAVE: i32 = 3;

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

    let mut output = String::with_capacity(content.len());
    let mut copied = 0usize;
    let mut index = 0usize;
    let mut cursor = RelativeCursor::new();
    // Set while a `<…>` chord is being read: the first note of the chord is
    // what the note after the chord is measured against, so it is remembered
    // until the chord closes.
    let mut chord_first: Option<Option<(Pitch, Pitch)>> = None;

    while let Some(current) = content[index..].chars().next() {
        let rest = &content[index..];
        // Anything this loop does not recognise is stepped over whole: a byte
        // at a time would land inside a multi-byte character and the next
        // slice would panic.
        let skip_one = index + current.len_utf8();

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
                index = skip_one + name_length;

                if command == "key" {
                    // An inline key change has to move with the music, or the
                    // local key signature contradicts the notes under it.
                    let (rewritten, after) = transpose_key_argument(content, index, interval);
                    if let Some((start, tonic)) = rewritten {
                        output.push_str(&content[copied..start]);
                        output.push_str(&tonic);
                        copied = after;
                    }
                    index = after;
                } else if command_takes_arguments(command) {
                    // `\clef bass`, `\set Staff.autoBeaming = ##f`: their
                    // arguments read like music without being music. Only the
                    // arguments are skipped — notes may follow on the same
                    // line.
                    index = skip_command_arguments(content, command, index);
                }
            }
            '<' => {
                chord_first = Some(None);
                index = skip_one;
            }
            '>' => {
                if let Some(Some((first_input, first_output))) = chord_first {
                    cursor.input = first_input;
                    cursor.output = first_output;
                }
                chord_first = None;
                index = skip_one;
            }
            c if c.is_ascii_alphabetic() => {
                let token_length = rest
                    .find(|c: char| is_token_delimiter(c))
                    .unwrap_or(rest.len());
                let token = &rest[..token_length];
                let start = index;
                index += token_length;

                // A rest (`r`, `R`, `s`) carries no pitch, and a word that is
                // not a note from end to end (`treble`, `volta`, `Allegro`) is
                // no note of this melody either.
                if token.starts_with(['r', 'R', 's']) {
                    continue;
                }
                let Some((step, alter, marks, consumed)) = parse_note_token(token) else {
                    continue;
                };

                let name = cursor.transpose_note(step, alter, marks, interval);
                // LilyPond measures the note after a chord against the chord's
                // *first* note, so that note has to survive the rest of the
                // chord.
                if matches!(chord_first, Some(None)) {
                    chord_first = Some(Some((cursor.input, cursor.output)));
                }

                output.push_str(&content[copied..start]);
                output.push_str(&name);
                output.push_str(&token[consumed..]);
                copied = index;
            }
            _ => index = skip_one,
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
    fn a_key_change_inside_a_melody_moves_with_it() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        // The `d` after `\key` is a tonic, not the first note of the melody —
        // but it still has to move, or the signature would contradict the
        // notes under it.
        assert_eq!(
            transpose_voice("\\key d \\major c4 d", up),
            "\\key e \\major d4 e"
        );
        // The tonic is spelled for the key it lands in, not as an accidental.
        let down = interval("c major", "bes major", TransposeDirection::Down);
        assert_eq!(
            transpose_voice("\\key g \\minor c4", down),
            "\\key f \\minor bes4"
        );
    }

    #[test]
    fn a_tie_without_spaces_separates_two_notes() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        // `~` binds the notes without a space, so it has to end the token as
        // well — otherwise the second note is never looked at and the tie ends
        // up joining two different pitches.
        assert_eq!(transpose_voice("c4~c4 e2~e2", up), "d4~d4 fis2~fis2");
    }

    #[test]
    fn a_word_that_merely_starts_like_a_note_is_left_alone() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        // `bass` starts with a note letter; writing `cisass` into the score
        // would make it invalid LilyPond.
        assert_eq!(
            transpose_voice("\\clef bass c4 d", up),
            "\\clef bass d4 e"
        );
        assert_eq!(
            transpose_voice("\\mark \\markup { Allegro } c4", up),
            "\\mark \\markup { Allegro } d4"
        );
        // Nor may such a word move the `\relative` cursor: the note after it
        // is measured from the note before it.
        assert_eq!(
            transpose_voice("c4 \\clef alto g4", up),
            "d4 \\clef alto a4"
        );
    }

    #[test]
    fn a_property_command_only_swallows_its_own_arguments() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        // The melody carries on on the same line, so only the property and its
        // value may be skipped.
        assert_eq!(
            transpose_voice("\\set Staff.autoBeaming = ##f c4 d e f", up),
            "\\set Staff.autoBeaming = ##f d4 e fis g"
        );
        // A Scheme value in parentheses is one argument, not several words.
        assert_eq!(
            transpose_voice("\\override Stem.direction = #'() c4", up),
            "\\override Stem.direction = #'() d4"
        );
        assert_eq!(
            transpose_voice("\\unset ignoreMelismata c4", up),
            "\\unset ignoreMelismata d4"
        );
    }

    #[test]
    fn text_outside_the_ascii_range_does_not_derail_the_scan() {
        let up = interval("c major", "d major", TransposeDirection::Up);
        // A non-breaking space between the notes, and an umlaut in a markup:
        // stepping over either one byte at a time would land inside the
        // character and panic on the next slice.
        assert_eq!(
            transpose_voice("c4\u{a0}e4", up),
            "d4\u{a0}fis4"
        );
        assert_eq!(
            transpose_voice("\\mark \\markup { Ä } c4", up),
            "\\mark \\markup { Ä } d4"
        );
        assert_eq!(
            transpose_voice("c4 % Überleitung\ne4", up),
            "d4 % Überleitung\nfis4"
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
    fn a_bare_tonic_keeps_the_song_in_its_own_mode() {
        let mut song = tochter_zion();
        song.score.key = Some("d minor".to_string());

        // "F" names a tonic and says nothing about the mode, so the song stays
        // minor instead of being relabelled as major.
        let transposed = transpose_song(&song, "F", TransposeDirection::Up).unwrap();
        assert_eq!(transposed.score.key.as_deref(), Some("f minor"));
    }

    #[test]
    fn a_target_mode_that_contradicts_the_song_is_refused() {
        let song = tochter_zion();
        let error = transpose_song(&song, "d minor", TransposeDirection::Nearest).unwrap_err();
        assert!(error.contains("d major"), "{}", error);
        assert!(error.contains("d minor"), "{}", error);

        // The same mode is fine, however it is spelled.
        assert!(transpose_song(&song, "F#m", TransposeDirection::Up).is_err());
        assert!(transpose_song(&song, "fis major", TransposeDirection::Up).is_ok());
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
