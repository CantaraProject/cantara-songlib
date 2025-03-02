//! This module contains helper functions for parsing meta data

use regex::{Regex,RegexBuilder};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Parses a meta data block and returns a key value hashmap
pub fn parse_metadata_block(block: &str) -> HashMap<String, String> {
    let mut metadata: HashMap<String, String> = HashMap::new();

    // With that we make sure that the regex is only compiled once.
    let tags_regex = { 
        static TAGS_REGEX: OnceLock<Regex> = OnceLock::new();
        TAGS_REGEX.get_or_init(|| {
            RegexBuilder::new(r"^\s*#(\w+):\s*(.+)$")
                .multi_line(true)
                .build()
                .unwrap()
        })
    };        

    tags_regex
        .captures_iter(block)
        .for_each(|capture: regex::Captures| {
            let tag: &str = capture.get(1).unwrap().as_str().trim();
            let value = capture.get(2).unwrap().as_str().trim().to_string();
            let tag_lowercase = tag.to_lowercase();
            
            metadata.insert(tag_lowercase, value);
        });

    metadata
}

pub mod tests {
    use super::*;

    #[test]
    fn test_metadata_parsing() {
        let metadata_block: &str = "#title: Test \n\
            #author: J.S. Bach";
        let metadata = parse_metadata_block(metadata_block);
        
        assert_eq!(metadata.len(), 2);
        assert_eq!(metadata.get("title").unwrap(), "Test");
        assert_eq!(metadata.get("author").unwrap(), "J.S. Bach");
    }
}