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

impl CantaraImportParsingError {
    fn error_text(&self) -> String {
        format!("Cantara Parsing Error in line {}:\n  {}",
            self.line.to_string(),
            self.error_type.to_string()
        )
    }
}

impl fmt::Display for CantaraImportParsingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There file does not exist")
    }
}

impl std::error::Error for CantaraImportParsingError {
    fn description(&self) -> &str {
        "There file does not exist"
    }
}

#[derive(Debug, Clone)]
pub enum ParsingErrorType {
    BlockNeedsToStartWithCategorization,
    MetaDataNotCorrect,
}

impl ParsingErrorType {
    pub fn to_string(&self) -> String {
        match self {
            &ParsingErrorType::BlockNeedsToStartWithCategorization => "The block has to start with a categorization (e.g. #stanza.1".to_string(),
            &ParsingErrorType::MetaDataNotCorrect => "The meta data is not correct, the syntax should be \"#key: value\"".to_string()
        }
    }
    
}