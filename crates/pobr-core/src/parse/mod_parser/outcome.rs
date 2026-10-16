//! Shared parser output types ([`ParseOutcome`] / [`ParseStatus`] /
//! [`SpecialMatchMeta`] / [`ParseError`]) — used by both legacy and the
//! data-driven engine.
//!
//! Extracted out of `legacy.rs` (a prerequisite for D-T8's legacy removal):
//! the engine's (`engine.rs` / `canonical.rs`) output types shouldn't depend
//! on the legacy module, so they were split into their own module; these
//! types stay alongside the engine after legacy is removed.

use std::fmt;

use crate::Modifier;

/// Parse status of a single modifier line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    /// At least the structure was recognized (may still produce 0 mods,
    /// e.g. a pure-recognition entry).
    Parsed,
    /// No rule matched — the whole line is unsupported.
    Unsupported,
}

/// The result of parsing a single line: the mod list + status + unparsed
/// leftover text + special-match metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutcome {
    /// The parsed-out modifiers.
    pub mods: Vec<Modifier>,
    /// Parse status.
    pub status: ParseStatus,
    /// Leftover text that wasn't consumed (used for diagnostics / coverage
    /// reports).
    pub unparsed: Option<String>,
    /// Metadata for a special-modifier rule hit (§2.3). `None` means the
    /// line went through the general parsing path; `Some` means
    /// [`crate::rules::SpecialModRules`] produced it from a whole-line match
    /// (entry_id + verified, passed through for attribution and parity
    /// reports).
    pub special_meta: Option<SpecialMatchMeta>,
}

impl ParseOutcome {
    /// Whether the parser read the whole line. A [`ParseStatus::Parsed`] outcome
    /// that still carries leftover text ([`Self::unparsed`]) understood only part of
    /// it, and the part it did read can mean something else than the line says:
    /// "1% more Life Flask Recovery" reads as a maximum Life modifier with
    /// "Flask Recovery" left over, "50% of Chaos damage you prevent when Hit
    /// Recouped as Life" as +50 Chaos damage.
    pub fn is_complete(&self) -> bool {
        self.status == ParseStatus::Parsed && self.unparsed.is_none()
    }

    /// The modifiers an ingest may apply for `line`, or the text it reports as
    /// unsupported when it applies nothing: the line itself for a partial parse,
    /// the parser's own remainder for an unsupported one. Every ingest path
    /// (items, flasks and charms, passive nodes, gems, free modifier texts) goes
    /// through this, so a partly read line never reaches the ModDb.
    pub fn into_applicable(self, line: &str) -> Result<Vec<Modifier>, String> {
        if self.is_complete() {
            return Ok(self.mods);
        }
        Err(match self.status {
            ParseStatus::Unsupported => self.unparsed.unwrap_or_else(|| line.to_string()),
            ParseStatus::Parsed => line.to_string(),
        })
    }
}

/// Metadata for a special-channel match ([`ParseOutcome::special_meta`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialMatchMeta {
    /// The stable id of the matched special entry.
    pub entry_id: String,
    /// Whether this entry has been verified against the oracle
    /// (`verified:false` entries are listed separately in parity reports).
    pub verified: bool,
    /// The entry is registered as changing nothing a calculation reports
    /// (batch `S0`): its empty result is a recognised line, not a dropped one.
    pub no_effect: bool,
}

/// A parse failure (keeps the input and the reason, for the caller to
/// diagnose).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The original input text.
    pub input: String,
    /// Failure reason.
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to parse modifier {:?}: {}",
            self.input, self.reason
        )
    }
}

impl std::error::Error for ParseError {}
