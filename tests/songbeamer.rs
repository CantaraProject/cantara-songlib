//! Integration tests for the SongBeamer (`.sng`) importer and exporter.
//!
//! The unit tests next to the two modules cover the parsing rules on strings
//! made up in place. These tests work on files on disk instead — one per
//! encoding SongBeamer writes — and check the two things a caller actually
//! does: read a `.sng` file through the generic entry point, and write one that
//! SongBeamer can read back.

use std::path::Path;

use cantara_songlib::exporter::songbeamer::{sng_bytes_from_song, sng_from_song, SngExportSettings};
use cantara_songlib::importer::import_song_from_file;
use cantara_songlib::importer::songbeamer::{
    import_from_bytes, import_from_file_with, SngImportSettings,
};
use cantara_songlib::song::{LyricLanguage, Song, SongPartType};

/// The lyrics of a part, by id.
fn lyrics(song: &Song, id: &str) -> String {
    song.part(&id.parse().unwrap())
        .unwrap_or_else(|| panic!("the song has no part {}", id))
        .lyrics_for(None, song.default_language.as_deref())
        .unwrap_or_else(|| panic!("{} has no lyrics", id))
        .content
        .clone()
}

/// The parts in singing order.
fn sung(song: &Song) -> Vec<String> {
    song.ordered_parts()
        .iter()
        .map(|part| part.id().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Reading the sample files
// ---------------------------------------------------------------------------

/// A UTF-8 file with a byte order mark, read through the generic entry point —
/// the `.sng` extension alone has to be enough to pick the importer.
#[test]
fn test_a_utf8_file_is_imported_by_its_extension() {
    let song = import_song_from_file("tests/data/Amazing Grace.sng").unwrap();

    assert_eq!(song.title, "Amazing Grace");
    assert_eq!(song.tag("author").unwrap(), "John Newton");
    assert_eq!(song.tag("composer").unwrap(), "New Britain");
    assert_eq!(song.tag("copyright").unwrap(), "Public Domain");
    assert_eq!(song.tag("ccli_song_number").unwrap(), "22025");
    assert_eq!(song.score.key.as_deref(), Some("f major"));

    assert_eq!(song.part_count_of_type(SongPartType::Verse), 3);
    assert_eq!(sung(&song), ["verse.1", "verse.2", "verse.3"]);

    // The '--' inside the first verse is a slide break, not a new part.
    assert_eq!(
        lyrics(&song, "verse.1"),
        "Amazing grace! How sweet the sound\n\
         That saved a wretch like me!\n\n\
         I once was lost, but now am found;\n\
         Was blind, but now I see."
    );

    // The formatting tags of the second verse are not part of the text.
    assert!(lyrics(&song, "verse.2").starts_with("Twas grace that taught"));
    assert!(!lyrics(&song, "verse.2").contains('<'));

    // Font and background settings say nothing about the song.
    assert!(song.tag("font").is_none());
    assert!(song.tag("backgroundimage").is_none());
}

/// The encoding older SongBeamer versions write: Windows-1252 with no byte
/// order mark. Read as UTF-8 this file would fail outright.
#[test]
fn test_a_cp1252_file_keeps_its_umlauts() {
    let song = import_song_from_file("tests/data/Grosser Gott wir loben dich.sng").unwrap();

    assert_eq!(song.title, "Großer Gott, wir loben dich");
    assert_eq!(song.tag("songbook").unwrap(), "Evangelisches Gesangbuch");
    assert_eq!(song.tag("song_number").unwrap(), "331");
    assert_eq!(song.tag("comments").unwrap(), "Nur Strophe 1 und 3");

    assert!(lyrics(&song, "verse.1").contains("Stärke"));
    assert!(lyrics(&song, "other.1").contains("für die Gemeinde"));

    // The German marks are understood, and their wording is kept.
    let verse = song.part(&"verse.1".parse().unwrap()).unwrap();
    assert_eq!(verse.part_type, SongPartType::Verse);
    assert_eq!(verse.label.as_deref(), Some("Strophe 1"));
    assert_eq!(
        song.part(&"other.1".parse().unwrap()).unwrap().label.as_deref(),
        Some("Zwischenteil")
    );

    // '--' inside the refrain breaks a slide, not the part.
    assert_eq!(
        lyrics(&song, "refrain.1"),
        "Alles, was dich preisen kann,\n\nCherubim und Serafim"
    );

    assert_eq!(
        sung(&song),
        ["verse.1", "refrain.1", "verse.2", "other.1"],
        "the stated #VerseOrder was not followed"
    );
}

/// A UTF-16 file with two languages, which is what a translated song looks like
/// when it comes out of newer versions of the program.
#[test]
fn test_a_utf16_bilingual_file() {
    let settings = SngImportSettings {
        languages: vec!["de".to_string(), "en".to_string()],
    };
    let song =
        import_from_file_with(Path::new("tests/data/Bilingual Test.sng"), &settings).unwrap();

    assert_eq!(song.title, "Bilingual Test");
    assert_eq!(song.default_language.as_deref(), Some("de"));
    assert_eq!(song.available_languages(), ["de", "en"]);

    let verse = song.part(&"verse.1".parse().unwrap()).unwrap();
    assert_eq!(
        verse.lyrics_for(Some("de"), None).unwrap().content,
        "Zeile eins\nZeile zwei"
    );
    assert_eq!(
        verse.lyrics_for(Some("en"), None).unwrap().content,
        "line one\nline two"
    );

    let refrain = song.part(&"refrain.1".parse().unwrap()).unwrap();
    assert_eq!(refrain.lyrics_for(Some("de"), None).unwrap().content, "Kehrvers");
    assert_eq!(
        refrain.lyrics_for(Some("en"), None).unwrap().content,
        "Chorus line"
    );
}

/// Read without language codes the same file still has to keep its two
/// languages apart, since nothing in the file names them.
#[test]
fn test_a_bilingual_file_without_language_codes() {
    let song = import_song_from_file("tests/data/Bilingual Test.sng").unwrap();

    let verse = song.part(&"verse.1".parse().unwrap()).unwrap();
    assert_eq!(verse.all_lyrics().count(), 2);
    assert_eq!(
        verse.lyrics_in(&LyricLanguage::Default).unwrap().content,
        "Zeile eins\nZeile zwei"
    );
    assert_eq!(
        verse
            .lyrics_in(&LyricLanguage::specific("lang2"))
            .unwrap()
            .content,
        "line one\nline two"
    );
}

// ---------------------------------------------------------------------------
// Writing files
// ---------------------------------------------------------------------------

/// The property that matters: a file that goes through the model and back out
/// is the same song, right down to the singing order.
#[test]
fn test_the_sample_files_round_trip_through_the_exporter() {
    for path in [
        "tests/data/Amazing Grace.sng",
        "tests/data/Grosser Gott wir loben dich.sng",
    ] {
        let song = import_song_from_file(path).unwrap();
        let exported = sng_from_song(&song, &SngExportSettings::default());
        let back = cantara_songlib::importer::songbeamer::import_from_sng_string(&exported)
            .unwrap_or_else(|error| panic!("{} does not re-import: {}\n{}", path, error, exported));

        assert_eq!(back.title, song.title, "{}", path);
        assert_eq!(back.tags(), song.tags(), "{}", path);
        assert_eq!(sung(&back), sung(&song), "{}", path);

        for part in song.parts() {
            let after = back
                .part(&part.id())
                .unwrap_or_else(|| panic!("{} lost {}", path, part.id()));
            assert_eq!(after.label, part.label, "{} / {}", path, part.id());
            assert_eq!(after.contents, part.contents, "{} / {}", path, part.id());
        }
    }
}

/// Written to disk the file has to be readable again — with the byte order mark
/// SongBeamer relies on to pick the encoding.
#[test]
fn test_a_written_file_is_read_back_identically() {
    let song = import_song_from_file("tests/data/Grosser Gott wir loben dich.sng").unwrap();

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Exported.sng");
    std::fs::write(
        &path,
        sng_bytes_from_song(&song, &SngExportSettings::default()),
    )
    .unwrap();

    let raw = std::fs::read(&path).unwrap();
    assert_eq!(&raw[..3], &[0xEF, 0xBB, 0xBF], "the byte order mark is missing");
    assert!(
        raw.windows(2).any(|pair| pair == b"\r\n"),
        "the file does not use CRLF line endings"
    );

    let back = import_song_from_file(&path).unwrap();
    assert_eq!(back.title, "Großer Gott, wir loben dich");
    assert_eq!(sung(&back), sung(&song));
    assert_eq!(back.tags(), song.tags());
}

/// A song that came from another format has to export as a valid `.sng` too —
/// that is the point of having the exporter at all.
#[test]
fn test_a_song_from_another_format_exports() {
    let song = import_song_from_file("tests/data/Amazing Grace.song.yml").unwrap();
    let bytes = sng_bytes_from_song(&song, &SngExportSettings::default());
    let back = import_from_bytes(&bytes).unwrap();

    assert_eq!(back.title, song.title);
    assert_eq!(back.part_count(), song.part_count());

    // The YAML file carries a melody; SongBeamer has nowhere to put one, so the
    // lyrics have to arrive complete and the notation has to be gone.
    assert!(song.has_voice_content());
    assert!(!back.has_voice_content());
    for part in song.parts() {
        let before = part.lyrics_for(None, song.default_language.as_deref());
        if let Some(before) = before {
            let after = back.part(&part.id()).unwrap().lyrics_for(None, None).unwrap();
            // The exporter writes what an audience reads, so the LilyPond
            // syllable markup is gone but the words are all there.
            let words: Vec<&str> = before.content.split_whitespace().collect();
            for word in words.iter().filter(|word| **word != "--" && **word != "_") {
                let plain = word.trim_matches(|c: char| !c.is_alphanumeric());
                if plain.len() > 3 {
                    assert!(
                        after.content.contains(plain),
                        "'{}' is missing from the exported {}:\n{}",
                        plain,
                        part.id(),
                        after.content
                    );
                }
            }
        }
    }
}
