use std::collections::HashMap;

use crate::mod_parser::{ParseCtx, ParseError, ParseOutcome};

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

    /// Returns the cached result on a hit; on a miss, parses via `ctx`
    /// (goes through the data-driven engine when rules are injected; whole
    /// line becomes Unsupported when `ctx` is empty — see [`ParseCtx::parse`]).
    ///
    /// **The cache key doesn't include `ctx`**: callers must ensure `ctx`
    /// stays consistent across the lifetime of a given `ModCache` instance
    /// (the orchestration layer's session holds a single `parser_rules` per
    /// build, satisfying this). Mixing different `ctx` values would let the
    /// first parse result served get reused from the cache regardless.
    pub fn parse_or_insert_with_ctx(
        &mut self,
        text: &str,
        ctx: ParseCtx<'_>,
    ) -> Result<ParseOutcome, ParseError> {
        let key = normalize_cache_key(text);
        if let Some(outcome) = self.entries.get(&key) {
            return Ok(outcome.clone());
        }

        let outcome = ctx.parse(text)?;
        self.entries.insert(key, outcome.clone());
        Ok(outcome)
    }
}

fn normalize_cache_key(text: &str) -> String {
    text.trim().to_ascii_lowercase()
}
