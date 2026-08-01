//! Test-only rule-loading helper (-T8 A2, `feature = "test-rules"`).
//!
//! pobr-core itself is zero-I/O and doesn't depend on serde_json (per
//! CLAUDE.md: I/O is contained to pobr-gamedata alone). This module
//! **compiles only under the `test-rules` feature**, where it makes an
//! exception to pull in serde_json + fs and read the parser rule files from
//! the repo's `data/4.5.0.3.4/`, compiling them into [`CompiledParserRules`]
//! — so downstream integration tests (in separate crates) can grab the
//! rules when threading the A2 engine path, without each test duplicating
//! its own loading logic. Production builds never enable this feature, so
//! the zero-I/O invariant is unaffected.
//!
//! The loading logic matches `tests/parser_dual_run.rs::load_rules` (same
//! rules, same special-channel concatenation) — this is that logic
//! extracted for reuse.

use std::path::PathBuf;

use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef, SpecialTemplateDef};

use super::compiled::CompiledParserRules;

/// The repository root (two levels up from `crates/pobr-core`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The data version directory (currently `4.5.0.3.4`).
fn data_root() -> PathBuf {
    repo_root().join("data").join(pobr_data::data_version())
}

/// Loads one special_mods data file (a missing file yields empty, handled by
/// the concatenation on the caller side).
fn load_special(rel: &str) -> Vec<SpecialTemplateDef> {
    let path = data_root().join(rel);
    let Ok(json) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let def: SpecialModsDef = serde_json::from_str(&json).expect("反序列化 special_mods");
    def.entries
}

/// Loads and compiles a complete [`CompiledParserRules`] from the
/// repository's data directory (the six parser rule tables plus the special
/// channel concatenation), for downstream integration tests to thread the
/// A2 engine path.
///
/// Reads `overlay/mod_parser_rules.json` + `overlay/special_mods.json` +
/// `generated/special_derived.json` + `generated/special_vendor.json` (the
/// same three sources, in the same order, as pobr-gamedata's
/// `load_ruleset`; a missing special file concatenates as empty, and id
/// conflicts fail fast in
/// [`CompiledParserRules::compile_with_special`]). Missing or unparsable
/// files panic directly (this is a test environment; the repo's data
/// package is always present).
pub fn test_compiled_rules() -> CompiledParserRules {
    let path = data_root().join("overlay/mod_parser_rules.json");
    let json = std::fs::read_to_string(&path).expect("读取 mod_parser_rules.json");
    let doc: ModParserRulesDoc = serde_json::from_str(&json).expect("反序列化规则表");
    let mut special = load_special("overlay/special_mods.json");
    special.extend(load_special("generated/special_derived.json"));
    special.extend(load_special("generated/special_vendor.json"));
    CompiledParserRules::compile_with_special(&doc, &special).expect("编译规则表")
}
