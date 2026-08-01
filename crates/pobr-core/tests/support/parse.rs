//! Shared test support: loads the real parser rules from the repo's data directory
//! (compiled once, reused by every test), providing the engine-backed `parse_mod` / `ctx`.
//!
//! Since the legacy hand-written parser was removed, tests exercise the same
//! data-driven engine as production. This module shares its loading logic with
//! `pobr-core`'s `test-rules` feature helper, but exists standalone because a crate's
//! own integration tests can't enable that crate's own feature (dev-deps already
//! pull in serde_json).

use std::path::PathBuf;
use std::sync::LazyLock;

use pobr_core::mod_parser::{
    CompiledParserRules, ParseCtx, ParseError, ParseOutcome, parse_mod_engine,
};
use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef, SpecialTemplateDef};

/// The data version directory (two levels up from `crates/pobr-core` to the repo root).
fn data_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("data")
        .join(pobr_data::data_version())
}

/// Loads a special_mods data file (missing file → empty; the caller handles fallback).
fn load_special(rel: &str) -> Vec<SpecialTemplateDef> {
    let path = data_root().join(rel);
    let Ok(json) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let def: SpecialModsDef = serde_json::from_str(&json).expect("deserialize special_mods");
    def.entries
}

static RULES: LazyLock<std::sync::Arc<CompiledParserRules>> = LazyLock::new(|| {
    let path = data_root().join("overlay/mod_parser_rules.json");
    let json = std::fs::read_to_string(&path).expect("read mod_parser_rules.json");
    let doc: ModParserRulesDoc = serde_json::from_str(&json).expect("deserialize the rule table");
    // special_mods has two layers (same order as pobr-gamedata's `load_ruleset`): the
    // version-agnostic curated layer `data/overlay-common/` (relative to the version
    // directory as `../overlay-common/`) forms the base, the version layer overrides /
    // appends by id, then derived / vendor are appended.
    let mut special = load_special("../overlay-common/special_mods.json");
    for v in load_special("overlay/special_mods.json") {
        match special.iter_mut().find(|e| e.id == v.id) {
            Some(slot) => *slot = v,
            None => special.push(v),
        }
    }
    special.extend(load_special("generated/special_derived.json"));
    special.extend(load_special("generated/special_vendor.json"));
    std::sync::Arc::new(
        CompiledParserRules::compile_with_special(&doc, &special).expect("compile the rule table"),
    )
});

/// The real rule set (the six parser-rule tables + the special channel).
#[allow(dead_code)]
pub fn rules() -> &'static CompiledParserRules {
    &RULES
}

/// Shared `Arc` of the real rule set (for injecting via `CalculationSession::set_parser_rules`).
#[allow(dead_code)]
pub fn rules_arc() -> std::sync::Arc<CompiledParserRules> {
    RULES.clone()
}

/// A [`pobr_core::calc::CalculationSession`] wired up with the real engine rules.
#[allow(dead_code)]
pub fn session(input: pobr_core::calc::MinimalInput) -> pobr_core::calc::CalculationSession {
    let mut s = pobr_core::calc::CalculationSession::new(input);
    s.set_parser_rules(rules_arc());
    s
}

/// A parse context wired up with the real rules.
#[allow(dead_code)]
pub fn ctx() -> ParseCtx<'static> {
    ParseCtx::with_engine(&RULES)
}

/// Engine-backed single-line parse (signature matches the historical
/// `pobr_core::mod_parser::parse_mod`; the engine never returns `Err` — unrecognized
/// text becomes a whole-line Unsupported).
#[allow(dead_code)]
pub fn parse_mod(text: &str) -> Result<ParseOutcome, ParseError> {
    Ok(parse_mod_engine(text, &RULES))
}
