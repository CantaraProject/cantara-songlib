//! This module contains functions to import songs from the classic Cantara song format.
//! The Cantara song format is a simple text format that is used to write songs in plain text files.
//! You can find a documentation here: <https://www.cantara.app/tutorial/where-to-get-the-songs/index.html#the-song-file-format>

use std::collections::HashMap;
use std::error::Error;
use std::sync::OnceLock;

extern crate regex;
use regex::{Regex,RegexBuilder};

use crate::importer::errors::CantaraImportNoContentError;
use crate::song::{
    LyricLanguage, PartOrder, PartOrderName, PartOrderRule, Song, SongPartContent, SongPartId,
    SongPartType,
};

use crate::slides::*;
use crate::templating::MetaTemplate;

use crate::importer::metadata::*;

/// Read the metadata block of a classic `.song` file into the song.
///
/// A block starting with `#` holds `#tag: value` lines rather than lyrics.
fn parse_metadata(block: &str, song: &mut Song) {
    // Compile the regex only once.
    let tags_regex = {
        static TAGS_REGEX: OnceLock<Regex> = OnceLock::new();
        TAGS_REGEX.get_or_init(|| {
            RegexBuilder::new(r"\s*#(\w+):\s*(.+)$")
                .multi_line(true)
                .build()
                .unwrap()
        })
    };

    for capture in tags_regex.captures_iter(block) {
        let tag = capture.get(1).unwrap().as_str().to_lowercase();
        let value = capture.get(2).unwrap().as_str();
        song.set_tag(&tag, value);
        if tag == "title" {
            song.title = value.to_string();
        }
    }
}

/// One run of consecutive blocks that belong together, as it appears in the
/// file.
struct Run {
    /// The blocks of this run, in file order.
    blocks: Vec<String>,
    /// Whether every block of the run is a repeated one, i.e. a refrain.
    refrain: bool,
}

/// Split the block sequence into runs of refrain blocks and runs of other
/// blocks.
///
/// A block that occurs more than once in the file is a refrain block; anything
/// else is verse material. Consecutive blocks of the same kind form one run, so
/// that a refrain written as two paragraphs comes out as a single refrain and
/// the blocks between two refrains come out as a single verse.
///
/// Grouping only means something relative to a refrain. A song without one is
/// nothing but a list of verses — gathering those up would turn the three
/// verses of "Amazing Grace" into one — so there every block stays its own
/// part.
fn split_into_runs(blocks: &[String]) -> Vec<Run> {
    let mut occurrences: HashMap<&str, usize> = HashMap::new();
    for block in blocks {
        *occurrences.entry(block.as_str()).or_insert(0) += 1;
    }
    let is_refrain = |block: &String| occurrences[block.as_str()] > 1;
    let has_refrain = blocks.iter().any(is_refrain);

    let mut runs: Vec<Run> = Vec::new();
    for block in blocks {
        let refrain = is_refrain(block);
        match runs.last_mut() {
            // Verses are only gathered up when there is a refrain to gather
            // them between; refrain blocks always are.
            Some(run) if run.refrain == refrain && (refrain || has_refrain) => {
                run.blocks.push(block.clone())
            }
            _ => runs.push(Run {
                blocks: vec![block.clone()],
                refrain,
            }),
        }
    }

    runs
}

/// The length of the shortest piece `run` is a whole repetition of.
///
/// A refrain written out twice in a row is one run of blocks, and nothing
/// outside it says where the seam is. Its own period does: `A B A B` repeats
/// after two blocks, so the refrain is `A B` sung twice rather than a single
/// refrain of four blocks.
fn repeating_period(run: &[String]) -> usize {
    (1..=run.len())
        .find(|period| {
            run.len().is_multiple_of(*period)
                && run
                    .iter()
                    .enumerate()
                    .all(|(index, block)| *block == run[index % period])
        })
        .unwrap_or(run.len())
}

/// Match a refrain run against the refrains already seen.
///
/// A song that closes by singing its refrain twice produces a run of two
/// refrain blocks where the earlier runs held one. Matching greedily against
/// the longest refrain already known splits such a run back into repeats of it,
/// instead of inventing a second refrain that holds the text twice. When
/// nothing is known yet — the run *is* the first occurrence — the run's own
/// period does the same job.
fn split_refrain_run<'a>(run: &'a [String], known: &[Vec<String>]) -> Vec<&'a [String]> {
    let mut pieces: Vec<&[String]> = Vec::new();
    let mut rest = run;

    while !rest.is_empty() {
        let matched = known
            .iter()
            .filter(|refrain| rest.starts_with(refrain))
            .map(|refrain| refrain.len())
            .max()
            .unwrap_or_else(|| repeating_period(rest));

        pieces.push(&rest[..matched]);
        rest = &rest[matched..];
    }

    pieces
}

/// Import a song written in the classic Cantara `.song` format.
///
/// The format states no structure of its own: blocks are separated by blank
/// lines and nothing says which one is the refrain. A block that occurs more
/// than once is therefore taken to be the refrain, and consecutive blocks of
/// the same kind form one part — so a refrain written as two paragraphs, or a
/// verse written as two, comes out as a single part rather than several.
///
/// **The singing order is never rearranged.** The file states it explicitly by
/// listing the blocks, so the order is read off the file rather than guessed. A
/// refrain that is only sung after every second verse stays that way, and a
/// refrain the file never repeats is not inserted anywhere.
pub fn import_song(content: &str) -> Result<Song, Box<dyn Error>> {
    if content.is_empty() {
        return Err(Box::new(CantaraImportNoContentError {}));
    }

    // Get the title either from the content or the filename
    let title: String = match get_title_from_file_content(content) {
        Some(title_string) => title_string,
        None => "".to_string()
    };

    let mut song: Song = Song::new(&title);

    // --- Split the file into blocks, keeping their order ---

    let mut lyric_blocks: Vec<String> = Vec::new();
    let mut block = String::new();
    let finish = |block: &mut String, song: &mut Song, blocks: &mut Vec<String>| {
        let text = block.trim().to_string();
        block.clear();
        if text.is_empty() {
            return;
        }
        if text.starts_with('#') {
            parse_metadata(&text, song);
        } else {
            blocks.push(text);
        }
    };

    for line in content.trim().lines() {
        if line.trim().is_empty() {
            finish(&mut block, &mut song, &mut lyric_blocks);
        } else {
            block.push_str(line.trim());
            block.push('\n');
        }
    }
    finish(&mut block, &mut song, &mut lyric_blocks);

    // --- Turn the runs into parts, remembering the order they are sung in ---

    let runs = split_into_runs(&lyric_blocks);

    // The refrains seen so far, each as its sequence of blocks.
    let mut refrains: Vec<Vec<String>> = Vec::new();
    let mut order: Vec<SongPartId> = Vec::new();

    for run in &runs {
        if !run.refrain {
            let id = song.add_part_of_type(SongPartType::Verse, None);
            song.part_mut(&id)
                .unwrap()
                .add_content(SongPartContent::lyrics(
                    LyricLanguage::Default,
                    run.blocks.join("\n"),
                ));
            order.push(id);
            continue;
        }

        for piece in split_refrain_run(&run.blocks, &refrains) {
            let id = match refrains.iter().position(|refrain| refrain == piece) {
                // Sung again: the text is stored once and referred to twice.
                Some(index) => SongPartId::new(SongPartType::Chorus, index as u32 + 1),
                None => {
                    refrains.push(piece.to_vec());
                    let id = song.add_part_of_type(SongPartType::Chorus, None);
                    song.part_mut(&id)
                        .unwrap()
                        .add_content(SongPartContent::lyrics(
                            LyricLanguage::Default,
                            piece.join("\n"),
                        ));
                    id
                }
            };
            order.push(id);
        }
    }

    // --- Record the order ---

    song.part_orders.push(order_for(&song, order));

    Ok(song)
}

/// Describe a sung order, preferring a standard rule when it says the same
/// thing.
///
/// A rule keeps the file small and readable, but only if it reproduces the
/// order exactly. Rather than reasoning about when that holds, the candidate
/// rules are applied and their result compared with the order actually read
/// from the file; anything that does not match is written out part by part.
fn order_for(song: &Song, order: Vec<SongPartId>) -> PartOrder {
    for rule in [
        PartOrderRule::VerseRefrainBridgeRefrain,
        PartOrderRule::RefrainVerseBridgeRefrain,
    ] {
        let candidate = PartOrder::new(PartOrderName::Default, rule);
        if candidate.to_part_ids(song) == order {
            return candidate;
        }
    }

    PartOrder::new(PartOrderName::Default, PartOrderRule::Custom(order))
}

/// Generates slides from a classic song content which is provided as &str
/// 
/// # Arguments
/// - `content`: The content of the classic song file given as a &str
/// - `presentation_settings`: A PresentationSettings struct which provides all settings for the creation of presentation slides
/// - `backup_title`: The title (String) which will be used if no #title - tag is specified in the content. This is most likely coming from the filename.
/// 
/// # Returns
/// A `Vec<Slide>` with the slides. This can be integrated into a PresentationChapter and a Presentation.
pub fn slides_from_classic_song(
    content: &str,
    slide_settings: &SlideSettings,
    backup_title: String) -> Vec<Slide> {
    
    /// Defines the current parsing state (which area is to be parsed)
    enum WritingArea {
        // The main block
        MainBlock,
        // The SecondarybBlock
        SecondaryBlock
    }
    
    // The emptyness of the line before (in the loop)
    let mut empty_line = false;
    // A new block has been started (in the iteration before)
    let mut start_block_flag = true;
    // The current block is a meta block
    let mut meta_block_flag = false;
    // All (main) blocks
    let mut blocks: Vec<Vec<String>> = vec![];
    // All secondary blocks. There will be always as many secondary blocks as there are primary blocks. 
    // Empty String equals None
    let mut secondary_blocks: Vec<Vec<String>> = vec![];
    
    // The current string of the block (used in the algorithm below)
    let mut cur_block_string: String = "".to_string();
    // The current string of the second block (used in the algorithm below)
    let mut cur_secundary_block_string: String = "".to_string();
    
    // The metadata of the song
    let mut metadata: HashMap<String, String> = HashMap::new();
    // Which block is currently written to (Main Block/Secondary Block)
    let mut writing_area: WritingArea = WritingArea::MainBlock;
    
    // A sub function for handling a block (putting it at the right position)
    // As this code is used twice in the code, it is outsourced into this function
    fn handle_block(metadata: &mut HashMap<String, String>, 
        meta_block_flag: &bool, 
        backup_title: &str,
        cur_block_string: &str,
        cur_secundary_block_string: &str,
        blocks: &mut Vec<Vec<String>>, 
        secondary_blocks: &mut Vec<Vec<String>>
        ) {
        match meta_block_flag {
                true => { 
                    parse_metadata_block(cur_block_string)
                    .iter()
                    .for_each(|(key, value)| {
                        metadata.insert(key.clone(), value.clone());
                    }); 
                    if metadata.get("title").is_none() {
                        metadata.insert("title".to_string(), backup_title.to_string());
                    }
                },
                false => { 
                    if !cur_block_string.trim().is_empty() {
                        blocks.push(
                            cur_block_string.lines()
                            .map(|str| str.to_string()).collect()
                        );
                        secondary_blocks.push(
                            cur_secundary_block_string.lines()
                            .map(|str| str.to_string()).collect()
                        );
                    }
                },
            }
    }
                
    for line in content.trim().lines() {
        if empty_line { start_block_flag = true };
        
        if start_block_flag && !line.is_empty() {
            meta_block_flag = line.starts_with('#');
            start_block_flag = false;
        }
        
        if line.trim().is_empty() {
            empty_line = true;
            writing_area = WritingArea::MainBlock;
            
            // Skip anything below if the line is empty as well
            if cur_block_string.is_empty() {
                continue;
            }

            handle_block(&mut metadata, 
                &meta_block_flag, 
                &backup_title, 
                &cur_block_string, 
                &cur_secundary_block_string, 
                &mut blocks, 
                &mut secondary_blocks
            );
            
            cur_block_string = "".to_string();
            cur_secundary_block_string = "".to_string();
            
        }
        // The --- delimiter starts a secondary block in a stanza
        else if line.trim() == "---" {
            writing_area = WritingArea::SecondaryBlock;
        }
        else {
            match writing_area {
                // Only separate a line from an existing one. Prepending the
                // newline unconditionally left every block with a leading
                // empty line, which `wrap_blocks` counts against `max_lines`
                // even though `Slide::new_content_slide` trims it away again.
                WritingArea::MainBlock => {
                    if !cur_block_string.is_empty() {
                        cur_block_string.push('\n');
                    }
                    cur_block_string.push_str(line);
                },
                WritingArea::SecondaryBlock => {
                    if !cur_secundary_block_string.is_empty() {
                        cur_secundary_block_string.push('\n');
                    }
                    cur_secundary_block_string.push_str(line);
                }
            }
            
        }
    }
    handle_block(&mut metadata, 
        &meta_block_flag, 
        &backup_title, 
        &cur_block_string, 
        &cur_secundary_block_string, 
        &mut blocks, 
        &mut secondary_blocks
    );

    if let Some(max_lines) = slide_settings.max_lines {
        let wrapped = wrap_blocks(&[blocks, secondary_blocks], max_lines, true);
        blocks = wrapped.first().cloned().unwrap_or_default();
        secondary_blocks = wrapped.get(1).cloned().unwrap_or_default();
    }

    // Create the Presentation

    let mut slides: Vec<Slide> = vec![];

    // The title has to be in the metadata before the template is rendered,
    // otherwise a template using {{title}} would come out blank for a file
    // without a #title tag.
    metadata
        .entry("title".to_string())
        .or_insert_with(|| backup_title.clone());

    // Compile the template once and render it once; the result is then placed
    // on whichever slides the settings ask for.
    let meta_text: Option<String> = if slide_settings.show_meta_information.is_none() {
        None
    } else {
        MetaTemplate::parse(&slide_settings.meta_syntax)
            .ok()
            .and_then(|template| template.render(&metadata))
    };

    if slide_settings.title_slide {
        let displayed_meta_text = match slide_settings.show_meta_information.on_title_slide() {
            true => meta_text.clone(),
            false => None,
        };

        slides.push(
            Slide::new_title_slide(
                metadata.get("title").unwrap().into(),
                displayed_meta_text
            )
        )
    }

    let count = blocks.len();
    for (index, block) in blocks.iter().enumerate() {
        let displayed_meta_text = match slide_settings
            .show_meta_information
            .on_content_slide(index, count)
        {
            true => meta_text.clone(),
            false => None,
        };
        
        // A stanza's secondary block (everything after a `---`) is a second
        // language, not a preview. It shares the spoiler field with the preview
        // of the next stanza, but it is content in its own right, so
        // `show_spoiler` does not suppress it — that setting only governs the
        // preview shown for stanzas that have no secondary block.
        let secondary_block = secondary_blocks.get(index).unwrap();
        let secondary_text = if !secondary_block.is_empty() {
            Some(secondary_block.join("\n"))
        } else if slide_settings.show_spoiler {
            blocks.get(index + 1).map(|next_block| next_block.join("\n"))
        } else {
            None
        };

        slides.push(Slide::new_content_slide(
            block.join("\n"),
            secondary_text,
            displayed_meta_text,
        ));
    }
    
    if slide_settings.empty_last_slide {
        slides.push(
            Slide::new_empty_slide(false)    
        );
    }
    
    slides

}

#[cfg(test)]
mod test {
    use crate::importer::import_song_from_file;

    use super::*;

    #[test]
    fn test_import_song() {
        let content: String = String::from("#title: Test Song");
        let song = import_song(&content).unwrap();
        assert_eq!(song.title, "Test Song");
    }

    #[test]
    fn test_import_song_with_tags() {
        let content: String = String::from(
            "#title: Test Song
            #author: Test Author
            #key: C"
        );
        let song = import_song(&content).unwrap();
        assert_eq!(song.title, "Test Song");
        assert_eq!(song.tag("author").unwrap(), "Test Author");
        assert_eq!(song.tag("key").unwrap(), "C");
    }

    #[test]
    fn test_import_song_with_verse() {
        let content: String = 
            "#title: Test Song
            
            This is a verse
            
            And a refrain
            
            The second verse
            
            And a refrain"
            .to_string();
        let song = import_song(&content).unwrap();
        assert_eq!(song.part_count_of_type(SongPartType::Verse), 2);
    }

    /// The part ids of a song, in storage order.
    fn part_ids(song: &Song) -> Vec<String> {
        song.parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect()
    }

    /// A song that opens with its refrain used to lose `verse.1`: the opening
    /// block was numbered 1 as a verse and then promoted to `chorus.1`, so the
    /// verses started at 2 and nothing referred to a first one.
    #[test]
    fn test_a_song_starting_with_the_refrain_numbers_its_verses_from_one() {
        let content = "#title: Alles was ihr tut

Alles was ihr tut, gescheh in Liebe,

Die Liebe ist sanftmütig und gütig.

Alles was ihr tut, gescheh in Liebe,

Jesus hat uns stets gelehrt zu lieben.

Alles was ihr tut, gescheh in Liebe,

Wir lieben nun weil Gott zuerst geliebt.";

        let song = import_song(content).unwrap();

        assert_eq!(
            part_ids(&song),
            ["chorus.1", "verse.1", "verse.2", "verse.3"]
        );
        assert!(
            song.part(&"verse.1".parse().unwrap()).is_some(),
            "a song always has a first verse"
        );
    }

    /// The ordinary case must keep the numbering it already had.
    #[test]
    fn test_a_song_starting_with_a_verse_keeps_its_numbering() {
        let content = "#title: Test Song

This is a verse

And a refrain

The second verse

And a refrain";

        let song = import_song(content).unwrap();

        assert_eq!(part_ids(&song), ["verse.1", "chorus.1", "verse.2"]);
    }

    /// Renumbering happens before the singing order is guessed, so the order
    /// has to name parts that actually exist.
    #[test]
    fn test_the_guessed_order_refers_to_existing_parts() {
        let content = "#title: Refrain First

The refrain

A verse

The refrain

Another verse";

        let song = import_song(content).unwrap();

        for part in song.part_orders.first().unwrap().to_parts(&song) {
            let id = part.id();
            assert!(
                song.part(&id).is_some(),
                "the order names {} which the song does not have",
                id
            );
        }
    }

    #[test]
    fn test_file_amazing_grace() {
        let song: Song = import_song_from_file("tests/data/Amazing Grace.song").unwrap();
        assert_eq!(song.title, "Amazing Grace");
        assert_eq!(song.tag("author").unwrap(), "John Newton");
        assert_eq!(song.part_count_of_type(SongPartType::Verse), 3)
    }

    #[test]
    fn test_song_with_refrain() {
        let song: Song = import_song_from_file("tests/data/O What A Savior That He Died For Me.song").unwrap();
        assert_eq!(song.title, "O What A Savior That He Died For Me");
        assert_eq!(song.part_count_of_type(SongPartType::Verse), 4);
        assert_eq!(song.part_count_of_type(SongPartType::Chorus), 1);
        dbg!(song);
    }
    
    /// The classic format states no order, so it has to be guessed — otherwise
    /// `Song::ordered_parts` would hand out the blocks in storage order and the
    /// refrain would appear only once.
    #[test]
    fn test_a_guessed_singing_order_is_added() {
        let song: Song =
            import_song_from_file("tests/data/O What A Savior That He Died For Me.song").unwrap();

        assert_eq!(song.part_orders.len(), 1);

        let sung: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();

        // Four verses, the chorus after each of them.
        assert_eq!(sung.len(), 8);
        assert_eq!(
            sung.iter().filter(|id| id.starts_with("chorus")).count(),
            4
        );
        assert_eq!(sung[0], "verse.1");
        assert_eq!(sung[1], "chorus.1");
    }

    /// A song without a refrain is simply sung through.
    #[test]
    fn test_order_without_a_refrain() {
        let song: Song = import_song_from_file("tests/data/Amazing Grace.song").unwrap();

        let sung: Vec<String> = song
            .ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect();
        assert_eq!(sung, ["verse.1", "verse.2", "verse.3"]);
    }

    // --- The order is read off the file, never rearranged ----------------

    /// The parts in singing order.
    fn sung(song: &Song) -> Vec<String> {
        song.ordered_parts()
            .iter()
            .map(|part| part.id().to_string())
            .collect()
    }

    /// The text a song is actually sung as, block by block.
    fn sung_text(song: &Song) -> Vec<String> {
        song.ordered_parts()
            .iter()
            .flat_map(|part| {
                part.contents
                    .iter()
                    .filter(|c| c.content_type.is_lyrics())
                    .map(|c| c.content.trim().to_string())
            })
            .collect()
    }

    /// The heart of it: a classic file states its order by listing the blocks,
    /// so the import must reproduce that list and never rearrange it.
    ///
    /// This song sings its refrain after every *second* verse block. The order
    /// used to be rebuilt from the rule "refrain after every verse", which both
    /// moved the refrain and dropped its second half.
    #[test]
    fn test_a_refrain_after_every_second_block_is_not_moved() {
        let content = "#title: Vivek

Verse one, first half.

Verse one, second half.

Refrain, first half.

Refrain, second half.

Verse two, first half.

Verse two, second half.

Refrain, first half.

Refrain, second half.";

        let song = import_song(content).unwrap();

        // Two blocks between the refrains are one verse; the two-block refrain
        // is one refrain.
        assert_eq!(sung(&song), ["verse.1", "chorus.1", "verse.2", "chorus.1"]);
        assert_eq!(
            sung_text(&song),
            [
                "Verse one, first half.\nVerse one, second half.",
                "Refrain, first half.\nRefrain, second half.",
                "Verse two, first half.\nVerse two, second half.",
                "Refrain, first half.\nRefrain, second half.",
            ]
        );
    }

    /// Nothing may be inserted that the file does not sing: a refrain that only
    /// comes at the end stays at the end.
    #[test]
    fn test_a_refrain_is_never_inserted_where_the_file_has_none() {
        let content = "#title: Late Refrain

Verse one.

The refrain.

Verse two.

Verse three.

The refrain.";

        let song = import_song(content).unwrap();

        assert_eq!(
            sung(&song),
            ["verse.1", "chorus.1", "verse.2", "chorus.1"],
            "verses two and three are one run between the refrains"
        );
    }

    /// With the refrain after every verse the plain rule says the same thing,
    /// so it is used instead of spelling the order out.
    #[test]
    fn test_a_regular_song_uses_the_standard_rule() {
        let content = "#title: Regular

Verse one.

The refrain.

Verse two.

The refrain.";

        let song = import_song(content).unwrap();

        assert_eq!(
            song.part_orders[0].rule(),
            &crate::song::PartOrderRule::VerseRefrainBridgeRefrain
        );
        assert_eq!(sung(&song), ["verse.1", "chorus.1", "verse.2", "chorus.1"]);
    }

    /// An irregular song gets its order written out part by part. This one ends
    /// on a verse; the plain rule would sing the refrain once more.
    #[test]
    fn test_an_irregular_song_gets_a_custom_order() {
        let content = "#title: Irregular

Verse one.

The refrain.

Verse two.

The refrain.

Verse three.";

        let song = import_song(content).unwrap();

        assert!(
            matches!(
                song.part_orders[0].rule(),
                crate::song::PartOrderRule::Custom(_)
            ),
            "expected an explicit order, got {:?}",
            song.part_orders[0].rule()
        );
        assert_eq!(
            sung(&song),
            ["verse.1", "chorus.1", "verse.2", "chorus.1", "verse.3"],
            "the song must not gain a closing refrain"
        );
    }

    /// A refrain written as two paragraphs is one refrain, not two. Splitting
    /// it used to lose the second half entirely.
    #[test]
    fn test_a_two_block_refrain_is_a_single_part() {
        let content = "#title: Two Block Refrain

Verse one.

Refrain A.

Refrain B.

Verse two.

Refrain A.

Refrain B.";

        let song = import_song(content).unwrap();

        assert_eq!(song.part_count_of_type(SongPartType::Chorus), 1);
        assert!(
            sung_text(&song).contains(&"Refrain A.\nRefrain B.".to_string()),
            "{:?}",
            sung_text(&song)
        );
        // Both halves are sung, both times.
        let all = sung_text(&song).join("\n");
        assert_eq!(all.matches("Refrain B.").count(), 2, "{}", all);
    }

    /// Without a refrain there is nothing to group verses between, so every
    /// block stays a verse of its own — three verses, not one long one.
    #[test]
    fn test_verses_are_not_merged_without_a_refrain() {
        let song: Song = import_song_from_file("tests/data/Amazing Grace.song").unwrap();

        assert_eq!(song.part_count_of_type(SongPartType::Verse), 3);
    }

    /// A refrain sung twice in a row is one run of blocks with nothing outside
    /// it to mark the seam. Its own period supplies it: `A B A B` is `A B`
    /// twice, not one refrain of four blocks.
    #[test]
    fn test_a_refrain_repeated_back_to_back_is_split() {
        let content = "#title: Doubled

Refrain A.

Refrain B.

Refrain A.

Refrain B.

A verse.

Refrain A.

Refrain B.";

        let song = import_song(content).unwrap();

        assert_eq!(song.part_count_of_type(SongPartType::Chorus), 1);
        assert_eq!(
            sung(&song),
            ["chorus.1", "chorus.1", "verse.1", "chorus.1"]
        );
    }

    #[test]
    fn generate_slides() {
        let testfile = std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();
        
        let presentation_settings = SlideSettings {
            title_slide: true,
            meta_syntax: "{{title}} ({{author}})".to_string(),
            show_meta_information: ShowMetaInformation::all(),
            empty_last_slide: true,
            show_spoiler: true ,
            max_lines: Some(10),
            language: crate::slides::LanguageConfiguration::default(),
        };
        
        let slides: Vec<Slide> = slides_from_classic_song(
            &testfile, 
            &presentation_settings,
            "Verily, Verily".to_string()
        );
        
        assert!(!slides.is_empty());
        
        dbg!(slides);
    }

    #[test]
    fn test_metadata_displayed_correctly() {
        let testfile = std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();
        
        let mut presentation_settings = SlideSettings {
            title_slide: false,
            meta_syntax: "{{title}} ({{author}})".to_string(),
            show_meta_information: ShowMetaInformation::none(),
            empty_last_slide: true,
            show_spoiler: true,
            max_lines: None,
            language: crate::slides::LanguageConfiguration::default(),
        };

        let slides: Vec<Slide> = slides_from_classic_song(
            &testfile, 
            &presentation_settings,
            "Verily, Verily".to_string()
        );

        slides.iter().for_each(|slide| assert!(!slide.has_meta_text()));

        // With no title slide, the first content slide is slides[0]. This used
        // to be checked against slides[1], which enshrined an off-by-one in the
        // position test.
        presentation_settings.show_meta_information = ShowMetaInformation::first_slide();

        let slides: Vec<Slide> = slides_from_classic_song(
            &testfile,
            &presentation_settings,
            "Verily, Verily".to_string()
        );

        assert!(slides[0].has_meta_text(), "the first slide should carry it");
        for slide in &slides[1..] {
            assert!(!slide.has_meta_text(), "only the first slide should carry it");
        }
    }

    /// The metadata goes on the last content slide, not on the trailing empty
    /// slide that `empty_last_slide` appends.
    #[test]
    fn test_metadata_on_the_last_slide() {
        let testfile =
            std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();

        let settings = SlideSettings {
            title_slide: false,
            meta_syntax: "{{title}} ({{author}})".to_string(),
            show_meta_information: ShowMetaInformation::last_slide(),
            empty_last_slide: true,
            show_spoiler: true,
            max_lines: None,
            language: crate::slides::LanguageConfiguration::default(),
        };

        let slides = slides_from_classic_song(&testfile, &settings, "Verily".to_string());

        let carrying: Vec<usize> = slides
            .iter()
            .enumerate()
            .filter(|(_, slide)| slide.has_meta_text())
            .map(|(index, _)| index)
            .collect();

        // The last content slide is the one before the appended empty slide.
        assert_eq!(carrying, [slides.len() - 2]);
    }

    /// The title slide is a position of its own: asking for the metadata only
    /// on the content slides must leave the title slide clean.
    #[test]
    fn test_title_slide_is_a_separate_position() {
        let testfile =
            std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();

        let mut settings = SlideSettings {
            title_slide: true,
            meta_syntax: "{{title}} ({{author}})".to_string(),
            show_meta_information: ShowMetaInformation::first_slide(),
            empty_last_slide: false,
            show_spoiler: true,
            max_lines: None,
            language: crate::slides::LanguageConfiguration::default(),
        };

        let slides = slides_from_classic_song(&testfile, &settings, "Verily".to_string());
        assert!(!slides[0].has_meta_text(), "the title slide was not asked for");
        assert!(slides[1].has_meta_text(), "the first content slide was");

        settings.show_meta_information = ShowMetaInformation::title_slide();
        let slides = slides_from_classic_song(&testfile, &settings, "Verily".to_string());
        assert!(slides[0].has_meta_text(), "the title slide was asked for");
        for slide in &slides[1..] {
            assert!(!slide.has_meta_text(), "no content slide was");
        }
    }

    /// A file without a `#title` tag falls back to the file name, and the
    /// template has to see that fallback rather than an empty title.
    #[test]
    fn test_title_fallback_reaches_the_template() {
        let testfile =
            std::fs::read_to_string("tests/data/What a friend we have in Jesus.song").unwrap();

        let settings = SlideSettings {
            title_slide: true,
            meta_syntax: "{{title}}".to_string(),
            show_meta_information: ShowMetaInformation::title_slide(),
            empty_last_slide: false,
            show_spoiler: false,
            max_lines: None,
            language: crate::slides::LanguageConfiguration::default(),
        };

        let slides = slides_from_classic_song(
            &testfile,
            &settings,
            "What a friend we have in Jesus".to_string(),
        );

        let rendered = serde_json::to_string(&slides[0]).unwrap();
        assert!(
            rendered.contains("What a friend we have in Jesus"),
            "the fallback title is missing from the meta line: {}",
            rendered
        );
    }

    /// Line counts of the content slides, in order.
    fn main_text_line_counts(slides: &[Slide]) -> Vec<usize> {
        slides
            .iter()
            .filter_map(|slide| match &slide.slide_content {
                SlideContent::SingleLanguageMainContent(content) => {
                    Some(content.clone().main_text().lines().count())
                }
                _ => None,
            })
            .collect()
    }

    fn wrapping_settings(max_lines: Option<usize>) -> SlideSettings {
        SlideSettings {
            title_slide: false,
            meta_syntax: "".to_string(),
            show_meta_information: ShowMetaInformation::none(),
            empty_last_slide: false,
            show_spoiler: false,
            max_lines,
            language: crate::slides::LanguageConfiguration::default(),
        }
    }

    /// `max_lines` has to count the lines that actually reach the slide.
    ///
    /// The blocks used to be built with a leading empty line, which
    /// `wrap_blocks` counted against the limit even though
    /// `Slide::new_content_slide` trims it away again. That cost every block's
    /// first chunk one line: these four-line verses came out as 2+2 under a
    /// limit of three instead of filling the limit at 3+1.
    #[test]
    fn test_max_lines_is_not_spent_on_a_leading_empty_line() {
        let testfile =
            std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();

        // The song is eight blocks of four lines each.
        let unwrapped = slides_from_classic_song(
            &testfile,
            &wrapping_settings(None),
            "Verily".to_string(),
        );
        assert_eq!(main_text_line_counts(&unwrapped), vec![4; 8]);

        let wrapped = slides_from_classic_song(
            &testfile,
            &wrapping_settings(Some(3)),
            "Verily".to_string(),
        );
        assert_eq!(
            main_text_line_counts(&wrapped),
            [3, 1].repeat(8),
            "a four-line block under a limit of three must fill the first slide"
        );
    }

    /// A block that is exactly `max_lines` long must not be split at all.
    /// The phantom leading line used to push it one over the limit.
    #[test]
    fn test_block_of_exactly_max_lines_is_not_wrapped() {
        let testfile =
            std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();

        let slides = slides_from_classic_song(
            &testfile,
            &wrapping_settings(Some(4)),
            "Verily".to_string(),
        );

        assert_eq!(main_text_line_counts(&slides), vec![4; 8]);
    }

    /// Main text paired with its secondary block, for the content slides.
    fn main_and_secondary(slides: &[Slide]) -> Vec<(Vec<String>, Vec<String>)> {
        slides
            .iter()
            .filter_map(|slide| match &slide.slide_content {
                SlideContent::SingleLanguageMainContent(content) => {
                    let lines = |text: String| {
                        text.lines().map(|line| line.to_string()).collect::<Vec<_>>()
                    };
                    Some((
                        lines(content.clone().main_text()),
                        content.clone().spoiler_text().map(lines).unwrap_or_default(),
                    ))
                }
                _ => None,
            })
            .collect()
    }

    /// The `---` delimiter puts a second language into a secondary block, which
    /// `wrap_blocks` wraps in parallel with the main block. Both used to carry
    /// the leading empty line, so both lost a line off their first chunk.
    ///
    /// This also pins the alignment: after wrapping, each slide's translation
    /// still has to be the translation of the main text on that same slide.
    #[test]
    fn test_secondary_blocks_wrap_in_step_with_the_main_block() {
        let testfile = std::fs::read_to_string("tests/data/Bilingual Test Song.song").unwrap();

        let slides = slides_from_classic_song(
            &testfile,
            &wrapping_settings(Some(3)),
            "Bilingual Test Song".to_string(),
        );

        assert_eq!(
            main_and_secondary(&slides),
            vec![
                (
                    vec![
                        "Main verse one line one".to_string(),
                        "Main verse one line two".to_string(),
                        "Main verse one line three".to_string(),
                    ],
                    vec![
                        "Neben Strophe eins Zeile eins".to_string(),
                        "Neben Strophe eins Zeile zwei".to_string(),
                        "Neben Strophe eins Zeile drei".to_string(),
                    ],
                ),
                (
                    vec!["Main verse one line four".to_string()],
                    vec!["Neben Strophe eins Zeile vier".to_string()],
                ),
                (
                    vec![
                        "Main verse two line one".to_string(),
                        "Main verse two line two".to_string(),
                        "Main verse two line three".to_string(),
                    ],
                    vec![
                        "Neben Strophe zwei Zeile eins".to_string(),
                        "Neben Strophe zwei Zeile zwei".to_string(),
                        "Neben Strophe zwei Zeile drei".to_string(),
                    ],
                ),
                (
                    vec!["Main verse two line four".to_string()],
                    vec!["Neben Strophe zwei Zeile vier".to_string()],
                ),
            ],
        );
    }

    /// Unwrapped, a `---` song is one slide per stanza with the full
    /// translation attached — the secondary block must not gain a blank line.
    #[test]
    fn test_secondary_block_is_not_wrapped_without_max_lines() {
        let testfile = std::fs::read_to_string("tests/data/Bilingual Test Song.song").unwrap();

        let slides = slides_from_classic_song(
            &testfile,
            &wrapping_settings(None),
            "Bilingual Test Song".to_string(),
        );

        let pairs = main_and_secondary(&slides);
        assert_eq!(pairs.len(), 2, "one slide per stanza");
        for (main, secondary) in pairs {
            assert_eq!(main.len(), 4);
            assert_eq!(secondary.len(), 4);
        }
    }

    /// The spoiler texts of the content slides, in order.
    fn spoilers(slides: &[Slide]) -> Vec<Option<String>> {
        slides
            .iter()
            .filter_map(|slide| match &slide.slide_content {
                SlideContent::SingleLanguageMainContent(content) => {
                    Some(content.clone().spoiler_text())
                }
                _ => None,
            })
            .collect()
    }

    fn spoiler_settings(show_spoiler: bool) -> SlideSettings {
        SlideSettings {
            show_spoiler,
            ..wrapping_settings(None)
        }
    }

    /// `show_spoiler` governs the preview of the *next* stanza. A second
    /// language behind `---` is content, so it survives `show_spoiler: false`
    /// even though it travels in the same field.
    #[test]
    fn test_show_spoiler_does_not_suppress_the_second_language() {
        let testfile = std::fs::read_to_string("tests/data/Bilingual Test Song.song").unwrap();

        let slides = slides_from_classic_song(
            &testfile,
            &spoiler_settings(false),
            "Bilingual Test Song".to_string(),
        );

        for spoiler in spoilers(&slides) {
            let spoiler = spoiler.expect("the translation is not a spoiler and must stay");
            assert!(
                spoiler.starts_with("Neben Strophe"),
                "expected the translation, got {:?}",
                spoiler
            );
        }
    }

    /// Without a secondary block the spoiler is a preview of the next stanza,
    /// and that is exactly what `show_spoiler: false` has to switch off. This
    /// setting was previously ignored in this code path.
    #[test]
    fn test_show_spoiler_suppresses_the_preview_of_the_next_stanza() {
        let testfile =
            std::fs::read_to_string("tests/data/O What A Savior That He Died For Me.song").unwrap();

        let shown = spoilers(&slides_from_classic_song(
            &testfile,
            &spoiler_settings(true),
            "Verily".to_string(),
        ));
        // Every stanza but the last previews its successor.
        assert!(
            shown[..shown.len() - 1].iter().all(|spoiler| spoiler.is_some()),
            "previews are on: {:?}",
            shown
        );
        assert_eq!(shown.last().unwrap(), &None, "the last stanza has no successor");

        let hidden = spoilers(&slides_from_classic_song(
            &testfile,
            &spoiler_settings(false),
            "Verily".to_string(),
        ));
        assert!(
            hidden.iter().all(|spoiler| spoiler.is_none()),
            "previews are off: {:?}",
            hidden
        );
    }

}
