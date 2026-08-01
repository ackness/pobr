//! Modifier text parsing.
//!
//! - The data-driven scan engine ([`scan`] / [`compiled`] / [`forms`] /
//!   [`template`] / [`engine`]): consumes Track A's pre-delivered
//!   `overlay/mod_parser_rules.json` (schema
//!   `pobr_data::catalog::parser_rules`), replicating the semantics of
//!   vendor `ModParser.lua`'s `scan()` + `parseMod()`. The engine entry
//!   point [`engine::parse_mod_engine`] is the **only** parser (the legacy
//!   hand-written parser was removed once the transition completed) — the
//!   orchestrator always loads `mod_parser_rules.json` through pobr-gamedata,
//!   compiles [`CompiledParserRules`], and injects it into the session; when
//!   no rules are injected, [`ParseCtx`] returns whole-line Unsupported for
//!   every line (see the [`dispatch`] module doc).
//! - [`outcome`]: shared parser output types ([`ParseOutcome`] etc.).
//! - [`dispatch`]: the parse dispatch context [`ParseCtx`].
//! - [`canonical`]: canonical serialization of [`ParseOutcome`] (the
//!   comparison unit for precompile).

/// Shared parser output types.
pub mod outcome;

/// Parse dispatch context [`ParseCtx`].
pub mod dispatch;

pub mod canonical;
pub mod compiled;
pub mod engine;
pub mod forms;
pub mod scan;
pub mod template;

// Re-export the shared parser output types from `outcome` (keeps the
// caller-facing path `pobr_core::mod_parser::*` unchanged).
pub use dispatch::ParseCtx;
pub use outcome::{ParseError, ParseOutcome, ParseStatus, SpecialMatchMeta};

pub use canonical::{canonical_outcome, canonical_tags};
pub use compiled::{CompileError, CompiledParserRules};
pub use engine::{EngineDiag, parse_mod_engine, parse_mod_engine_diag};

// The `test-rules` feature is for downstream crates' integration tests;
// `cfg(test)` lets pobr-core's own unit tests get real rules too
// (serde_json comes in as a dev-dependency; the zero-I/O invariant only
// constrains production builds).
#[cfg(any(test, feature = "test-rules"))]
mod test_rules;
#[cfg(any(test, feature = "test-rules"))]
pub use test_rules::test_compiled_rules;
