use std::collections::HashMap;

use crate::mod_parser::{ParseError, ParseOutcome, parse_mod};

#[derive(Debug, Clone, Default)]
pub struct ModCache {
    entries: HashMap<String, ParseOutcome>,
}

impl ModCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, text: &str) -> Option<&ParseOutcome> {
        self.entries.get(&normalize_cache_key(text))
    }

    pub fn parse_or_insert(&mut self, text: &str) -> Result<ParseOutcome, ParseError> {
        let key = normalize_cache_key(text);
        if let Some(outcome) = self.entries.get(&key) {
            return Ok(outcome.clone());
        }

        let outcome = parse_mod(text)?;
        self.entries.insert(key, outcome.clone());
        Ok(outcome)
    }
}

fn normalize_cache_key(text: &str) -> String {
    text.trim().to_ascii_lowercase()
}
