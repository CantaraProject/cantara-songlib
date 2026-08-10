//! A minimal base64 codec.
//!
//! The SongBeamer format stores two of its fields — `#Chords` and `#Comments` —
//! as base64. That is the only place in this crate that needs base64 at all,
//! which is not enough to justify pulling in a dependency, so the standard
//! alphabet is implemented here.
//!
//! ```
//! use cantara_songlib::base64::{decode, encode};
//!
//! assert_eq!(encode(b"G,0,C"), "RywwLEM=");
//! assert_eq!(decode("RywwLEM=").unwrap(), b"G,0,C");
//! ```

/// The standard alphabet (RFC 4648), padded with `=`.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode bytes as base64.
pub fn encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        // Pack the (up to three) bytes into one 24-bit group, then read it back
        // out in 6-bit steps.
        let mut group = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            group |= (*byte as u32) << (16 - 8 * index);
        }

        // A group of n bytes carries n + 1 meaningful characters; the rest is
        // padding.
        let characters = chunk.len() + 1;
        for index in 0..4 {
            if index < characters {
                let sextet = ((group >> (18 - 6 * index)) & 0b11_1111) as usize;
                output.push(ALPHABET[sextet] as char);
            } else {
                output.push('=');
            }
        }
    }

    output
}

/// Decode a base64 string.
///
/// Whitespace — which the SongBeamer files sometimes carry after a long value —
/// is ignored. Returns `None` for input that is not base64 at all, so that a
/// caller can fall back to treating the field as plain text.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let mut output: Vec<u8> = Vec::with_capacity(input.len() / 4 * 3);
    let mut group = 0u32;
    let mut sextets = 0usize;

    for character in input.chars() {
        if character.is_whitespace() {
            continue;
        }
        // Padding ends the data; anything after it is ignored.
        if character == '=' {
            break;
        }

        let value = sextet_value(character)?;
        group = (group << 6) | value as u32;
        sextets += 1;

        if sextets == 4 {
            output.push((group >> 16) as u8);
            output.push((group >> 8) as u8);
            output.push(group as u8);
            group = 0;
            sextets = 0;
        }
    }

    // A trailing partial group carries one or two bytes; a single leftover
    // sextet cannot encode anything and means the input was truncated.
    match sextets {
        0 => {}
        2 => output.push((group >> 4) as u8),
        3 => {
            output.push((group >> 10) as u8);
            output.push((group >> 2) as u8);
        }
        _ => return None,
    }

    Some(output)
}

/// The 6-bit value of one base64 character.
fn sextet_value(character: char) -> Option<u8> {
    match character {
        'A'..='Z' => Some(character as u8 - b'A'),
        'a'..='z' => Some(character as u8 - b'a' + 26),
        '0'..='9' => Some(character as u8 - b'0' + 52),
        '+' => Some(62),
        '/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip_of_every_padding_case() {
        for text in ["", "a", "ab", "abc", "abcd", "abcde", "abcdef"] {
            assert_eq!(
                decode(&encode(text.as_bytes())).unwrap(),
                text.as_bytes(),
                "failed for {:?}",
                text
            );
        }
    }

    #[test]
    fn test_known_vectors() {
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn test_binary_data_survives() {
        let bytes: Vec<u8> = (0u8..=255).collect();
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn test_whitespace_is_ignored() {
        assert_eq!(decode("Zm9v YmFy").unwrap(), b"foobar");
        assert_eq!(decode("Zm9v\nYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn test_non_base64_input_is_rejected() {
        // A plain-text comment that was never encoded must not decode silently.
        assert!(decode("Hallo Welt!").is_none());
        // A single leftover sextet cannot encode a byte.
        assert!(decode("A").is_none());
    }
}
