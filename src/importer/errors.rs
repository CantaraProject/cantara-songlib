use std::fmt;

#[derive(Debug, Clone)]
pub struct CantaraImportNoContentError;

impl fmt::Display for CantaraImportNoContentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There is no content to import")
    }
}

impl std::error::Error for CantaraImportNoContentError {
    fn description(&self) -> &str {
        "There is no content to import"
    }
}

#[derive(Debug, Clone)]
pub struct CantaraImportUnknownFileExtensionError {
    pub file_extension: String,
}

impl fmt::Display for CantaraImportUnknownFileExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Unknown file extension: {}", self.file_extension)
    }
}

impl std::error::Error for CantaraImportUnknownFileExtensionError {
    fn description(&self) -> &str {
        "Unknown file extension"
    }
}

#[derive(Debug, Clone)]
pub struct CantaraImportUnknownBlockError {
    pub block: String,
}

impl fmt::Display for CantaraImportUnknownBlockError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Unknown block: {}", self.block)
    }
}

impl std::error::Error for CantaraImportUnknownBlockError {
    fn description(&self) -> &str {
        "Unknown block"
    }
}


#[derive(Debug, Clone, PartialEq)]
pub struct CantaraFileDoesNotExistError;

impl fmt::Display for CantaraFileDoesNotExistError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There file does not exist")
    }
}

impl std::error::Error for CantaraFileDoesNotExistError {
    fn description(&self) -> &str {
        "There file does not exist"
    }
}

#[derive(Debug, Clone)]
pub struct CantaraImportParsingError {
    pub line: usize,
    pub error_type: ParsingErrorType,
}

impl fmt::Display for CantaraImportParsingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "parsing error in line {}: {}",
            self.line, self.error_type
        )
    }
}

impl std::error::Error for CantaraImportParsingError {}

#[derive(Debug, Clone)]
pub enum ParsingErrorType {
    BlockNeedsToStartWithCategorization,
    MetaDataNotCorrect,
}

impl fmt::Display for ParsingErrorType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParsingErrorType::BlockNeedsToStartWithCategorization => {
                f.write_str("the block has to start with a categorization, e.g. '#stanza.1'")
            }
            ParsingErrorType::MetaDataNotCorrect => {
                f.write_str("malformed metadata, the syntax is '#key: value'")
            }
        }
    }
}

