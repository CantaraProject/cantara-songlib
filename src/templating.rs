//! This module contains some functions for templating which will be called from several parts of the library

use std::collections::HashMap;
use std::string::String;

use handlebars::Handlebars;
use handlebars::RenderError;

/// This function parses metadata of a song file against a Handlebar template string and returns a string
pub fn render_metadata(
    template_string: &str, 
    metadata: &HashMap<String, String>) -> Result<String, RenderError> {
    let reg = Handlebars::new();
    reg.render_template(template_string, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_render_metadata() {
        let template_string: &str = "{{title}} ({{author}})";
        let mut metadata: HashMap<String, String> = HashMap::new();
        
        metadata.insert("title".to_string(), "Amazing Grace".to_string());
        metadata.insert("author".to_string(), "John Newton".to_string());
        
        assert_eq!(
            render_metadata(template_string, &metadata).unwrap(),
            "Amazing Grace (John Newton)"
        );
        
        let template_string = "{{title}} ({{nonexisting}})";
        
        assert_eq!(
            render_metadata(template_string, &metadata).unwrap(),
            "Amazing Grace ()"
        );
    }
}