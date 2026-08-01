//! Parse dispatch context [`ParseCtx`] — packages the optional data-driven
//! engine rules and threads them along the ingest chain (item / passive /
//! gem) to decide whether each modifier line can be parsed.
//!
//! Now that the transition is complete, the engine ([`parse_mod_engine`]) is
//! the only parser: `engine = Some` goes through the data-driven engine;
//! `engine = None` (no rules injected, e.g. an old data package or a
//! plain-text fallback) **has no legacy fallback anymore** — every line
//! comes back as whole-line
//! [`ParseStatus::Unsupported`](super::ParseStatus::Unsupported) (the
//! modifier has no effect but is collected into the unsupported report;
//! it's never silently dropped or miscalculated).
//!
//! [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine

use super::outcome::{ParseError, ParseOutcome, ParseStatus};

/// Parse dispatch context: an optional reference to the data-driven engine
/// rules, threaded along the ingest chain (item / passive / gem / flask).
///
/// The default ([`ParseCtx::none`]) means no rules injected: all text is
/// treated as Unsupported (see the module doc). Production paths
/// (orchestrator / wasm / CLI) always compile [`CompiledParserRules`] from
/// `mod_parser_rules.json` and inject it.
///
/// [`CompiledParserRules`]: crate::mod_parser::CompiledParserRules
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseCtx<'a> {
    /// Data-driven parser engine rules. When `Some`, [`ParseCtx::parse`]
    /// goes through [`parse_mod_engine`]; when `None`, every line returns
    /// Unsupported.
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    pub engine: Option<&'a crate::mod_parser::CompiledParserRules>,
}

impl<'a> ParseCtx<'a> {
    /// An empty context (no engine rules): any text parses to whole-line
    /// Unsupported.
    pub fn none() -> Self {
        Self::default()
    }

    /// A context carrying data-driven parser engine rules: afterward
    /// [`parse`](Self::parse) goes through [`parse_mod_engine`] (the special
    /// channel is already compiled into [`CompiledParserRules::special`]).
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`CompiledParserRules::special`]: crate::mod_parser::CompiledParserRules
    pub fn with_engine(engine: &'a crate::mod_parser::CompiledParserRules) -> Self {
        Self {
            engine: Some(engine),
        }
    }

    /// Parses one line of modifier text under this context.
    ///
    /// - `engine = Some` (the production path) -> [`parse_mod_engine`]
    ///   (data-driven; the engine returns Unsupported for input it can't
    ///   recognize, and never errors).
    /// - `engine = None` -> whole-line [`ParseStatus::Unsupported`] (empty
    ///   mods, `unparsed` = the original text). The modifier has no effect,
    ///   but it lands in the unsupported collection (visible to the session
    ///   / reports) rather than being silently swallowed as if parsed.
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    pub fn parse(&self, text: &str) -> Result<ParseOutcome, ParseError> {
        match self.engine {
            Some(engine) => Ok(crate::mod_parser::parse_mod_engine(text, engine)),
            None => Ok(ParseOutcome {
                mods: Vec::new(),
                status: ParseStatus::Unsupported,
                unparsed: Some(text.trim().to_string()),
                special_meta: None,
            }),
        }
    }
}
