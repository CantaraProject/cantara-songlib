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