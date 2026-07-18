//! `--check`: validate the hand-editable overlay JSON that contributors touch.
//!
//! Motivation: `overlay/mod_parser_rules.json` and `overlay/special_mods.json`
//! are consumed at app startup by `CompiledParserRules::compile_with_special`
//! (see `pobr-build::build_data`). A malformed edit fails in three ways:
//!
//! 1. **Type / syntax error** — surfaces as a `LoadError::Parse` only when the
//!    app actually loads the version, far from the edit.
//! 2. **Unknown / misspelled field** — `serde` silently drops it (the structs
//!    use `#[serde(default)]`, no `deny_unknown_fields`), so the rule quietly
//!    loses its effect with no error at all (silent degradation).
//! 3. **Bad Lua pattern / duplicate special `id` / unregistered `handler_id`**
//!    — surfaces as a compile error, again only at app startup.
//!
//! `--check` runs the same deserialize + compile a contributor's edit will hit
//! at runtime, plus `serde_ignored` unknown-field detection, standalone and
//! fast, and exits non-zero on any problem.
//!
//! Not checked (by design): `ModName` / `StatId` spelling. `StatId` is an open
//! `String` newtype with no closed registry — a misspelled stat name produces a
//! modifier that simply aggregates to nothing. There is nothing to validate it
//! against; the end-to-end parity tests are the backstop for that class.

use std::path::Path;

use serde::de::DeserializeOwned;

use pobr_core::mod_parser::CompiledParserRules;
use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef};

/// Validate every schema-bearing overlay JSON under `data_dir`.
///
/// Returns `Err` with all collected problems joined by newlines (so one run
/// surfaces every issue, not just the first). `Ok(())` = all clear.
pub fn check(data_dir: &Path) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();
    let overlay = data_dir.join("overlay");
    let generated = data_dir.join("generated");

    // 1) mod_parser_rules.json — required. Deserialize + unknown-field scan.
    let rules_path = overlay.join("mod_parser_rules.json");
    let rules_doc: Option<ModParserRulesDoc> = if rules_path.is_file() {
        load_strict(&rules_path, &mut errors)
    } else {
        errors.push(format!("{}: missing (required)", rules_path.display()));
        None
    };

    // 2) special_mods entries come from three sources, all concatenated for the
    //    compile step below (matches the runtime `GameData::load_ruleset` set):
    //    - overlay-common/special_mods.json (version-independent curated, P1-3) +
    //      overlay/special_mods.json (version-specific) — both optional;
    //    - generated/special_derived.json — optional;
    //    - generated/special_vendor.json — required (the 4.5.4.3 upgrade shipped
    //      without it because regen-all.sh lacked the step and nothing failed;
    //      a missing file must be loud, not a silent skip).
    let overlay_common = data_dir
        .parent()
        .map(|p| p.join("overlay-common"))
        .unwrap_or_else(|| data_dir.join("overlay-common"));
    let mut special_entries = Vec::new();
    for (path, required) in [
        (overlay_common.join("special_mods.json"), false),
        (overlay.join("special_mods.json"), false),
        (generated.join("special_derived.json"), false),
        (generated.join("special_vendor.json"), true),
    ] {
        if !path.is_file() {
            if required {
                errors.push(format!(
                    "{}: missing (required; regenerate with `cargo run -p sync-pob-catalog -- \
                     extract-lua --what special-mods ...`, see pipeline/regen-all.sh)",
                    path.display()
                ));
            }
            continue;
        }
        if let Some(def) = load_strict::<SpecialModsDef>(&path, &mut errors) {
            special_entries.extend(def.entries);
        }
    }

    // 3) Compile — catches Lua-pattern syntax errors, duplicate special `id`s,
    //    and `handler_id`s with no registered handler (fail-fast inside
    //    `compile_with_special`). Only runs if the rules doc deserialized.
    if let Some(doc) = &rules_doc
        && let Err(e) = CompiledParserRules::compile_with_special(doc, &special_entries)
    {
        errors.push(format!("{}: compile failed: {e}", rules_path.display()));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// Deserialize `path` into `T`, appending any parse error and every unknown
/// field (via `serde_ignored`) to `errors`. `_meta` provenance headers are
/// intentional and never reported. Returns the value on success.
fn load_strict<T: DeserializeOwned>(path: &Path, errors: &mut Vec<String>) -> Option<T> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            errors.push(format!("{}: read failed: {e}", path.display()));
            return None;
        }
    };
    let mut unknown = Vec::new();
    let mut de = serde_json::Deserializer::from_slice(&bytes);
    let result: Result<T, _> = serde_ignored::deserialize(&mut de, |p| {
        let p = p.to_string();
        // `_meta` is the generator/provenance header, absent from the consumer
        // schema on purpose — not a contributor mistake.
        if !p.starts_with("_meta") {
            unknown.push(p);
        }
    });
    for u in unknown {
        errors.push(format!(
            "{}: unknown field `{u}` (typo? silently ignored at runtime)",
            path.display()
        ));
    }
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push(format!("{}: {e}", path.display()));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed repo data passes `--check` (deserialize + compile clean)
    /// for the active and parity-golden versions. Versions come from the
    /// `pobr_data` constants (auto-advance on data bumps, no literal to
    /// re-pin); stale/experimental dirs under `data/` are out of contract.
    #[test]
    fn repo_overlay_passes_check() {
        let mut versions = vec![
            pobr_data::DATA_VERSION,
            pobr_data::GOLDEN_PARITY_DATA_VERSION,
        ];
        versions.dedup();
        for version in versions {
            let data_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../data")
                .join(version);
            if !data_dir.join("overlay/mod_parser_rules.json").is_file() {
                eprintln!("SKIP: repo data dir {version} not present");
                continue;
            }
            check(&data_dir)
                .unwrap_or_else(|e| panic!("committed {version} JSON should pass --check: {e}"));
        }
    }

    /// A type error, an unknown field, and a bad Lua pattern are each reported;
    /// a clean doc passes. Exercises all three failure classes on a temp copy.
    #[test]
    fn detects_type_unknown_and_pattern_errors() {
        let tmp = std::env::temp_dir().join(format!("pobr-check-test-{}", std::process::id()));
        let overlay = tmp.join("overlay");
        let generated = tmp.join("generated");
        std::fs::create_dir_all(&overlay).unwrap();
        std::fs::create_dir_all(&generated).unwrap();
        let rules = overlay.join("mod_parser_rules.json");

        // Clean minimal doc, but no generated/special_vendor.json yet: the
        // missing required file must be reported, not silently skipped.
        std::fs::write(
            &rules,
            r#"{"_meta":{"schema":"x"},"forms":[{"pattern":"^(%d+)%% increased","form":"INC"}],
               "name_map":[],"flag_phrases":[],"pre_flags":[],"tag_phrases":[]}"#,
        )
        .unwrap();
        let err = check(&tmp).unwrap_err();
        assert!(
            err.contains("special_vendor.json") && err.contains("missing"),
            "missing special_vendor.json should be flagged: {err}"
        );

        // With an (empty-entries) special_vendor.json present, the doc passes.
        std::fs::write(
            generated.join("special_vendor.json"),
            r#"{"_meta":{"schema":"special_mods/v1"},"entries":[]}"#,
        )
        .unwrap();
        assert!(check(&tmp).is_ok(), "clean doc should pass");

        // Unknown field: `forms[].from` (typo of `form`) is silently dropped by
        // serde — serde_ignored must flag it.
        std::fs::write(
            &rules,
            r#"{"forms":[{"pattern":"^x","form":"INC","from":"INC"}],
               "name_map":[],"flag_phrases":[],"pre_flags":[],"tag_phrases":[]}"#,
        )
        .unwrap();
        let err = check(&tmp).unwrap_err();
        assert!(
            err.contains("unknown field"),
            "should flag unknown field: {err}"
        );

        // Type error: `forms` must be an array.
        std::fs::write(&rules, r#"{"forms":"nope","name_map":[]}"#).unwrap();
        assert!(check(&tmp).is_err(), "type error should fail");

        // Bad Lua pattern: unbalanced `%` class escape rejected by compile.
        std::fs::write(
            &rules,
            r#"{"forms":[{"pattern":"(%","form":"INC"}],
               "name_map":[],"flag_phrases":[],"pre_flags":[],"tag_phrases":[]}"#,
        )
        .unwrap();
        assert!(check(&tmp).is_err(), "bad pattern should fail compile");

        std::fs::remove_dir_all(&tmp).ok();
    }
}
