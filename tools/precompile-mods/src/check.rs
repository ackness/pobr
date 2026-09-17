//! Strict validation of the complete effective special-rule snapshot.
//! Common entries are replaced by version entries with the same ID; patches
//! use GameData's recursive merge. Derived and vendor entries concatenate.
//! StatId remains open: syntax validation is not a calculation-support claim.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::de::DeserializeOwned;

use pobr_core::mod_parser::CompiledParserRules;
use pobr_core::rules::{HandlerRegistry, SpecialModRules, register_special_handlers};
use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef};
use pobr_gamedata::GameData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleOrigin {
    pub id: String,
    /// Input paths relative to the data root, in merge order.
    pub files: Vec<String>,
    pub replaced_files: Vec<String>,
    /// Recognized metadata that this interpreter does not emit as modifiers.
    pub non_effective_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub engine_version: String,
    pub data_version: String,
    pub scope: String,
    pub input_sha256: BTreeMap<String, String>,
    /// Runtime first-match order, after overrides and patches.
    pub effective_rules: Vec<RuleOrigin>,
}

/// Validate every schema-bearing overlay JSON under `data_dir`.
///
/// Returns `Err` with all collected problems joined by newlines (so one run
/// surfaces every issue, not just the first). `Ok(())` = all clear.
pub fn check(data_dir: &Path) -> Result<(), String> {
    inspect(data_dir).map(|_| ())
}

pub fn inspect(data_dir: &Path) -> Result<ValidationReport, String> {
    let mut errors: Vec<String> = Vec::new();
    let overlay = data_dir.join("overlay");
    let generated = data_dir.join("generated");

    // 1) mod_parser_rules.json — required. Deserialize + unknown-field scan.
    let rules_path = overlay.join("mod_parser_rules.json");
    let rules_doc: Option<ModParserRulesDoc> = if rules_path.is_file() {
        load_strict(&rules_path, data_dir, &mut errors)
    } else {
        errors.push(format!("{}: missing (required)", rules_path.display()));
        None
    };

    // Validate file schemas independently, then compile the runtime effective set.
    let overlay_common = data_dir
        .parent()
        .map(|p| p.join("overlay-common"))
        .unwrap_or_else(|| data_dir.join("overlay-common"));
    let mut source_by_id: BTreeMap<String, RuleOrigin> = BTreeMap::new();
    let mut input_sha256 = BTreeMap::new();
    record_input(&rules_path, data_dir, &mut input_sha256)?;
    record_patch(&rules_path, data_dir, &mut input_sha256)?;
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
        record_input(&path, data_dir, &mut input_sha256)?;
        let patch = record_patch(&path, data_dir, &mut input_sha256)?;
        let patch_ids: BTreeSet<String> = patch
            .as_ref()
            .map(|p| {
                let value: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(p).unwrap_or_default())
                        .unwrap_or_default();
                value["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e["id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(def) = load_strict::<SpecialModsDef>(&path, data_dir, &mut errors) {
            let mut ids = std::collections::BTreeSet::new();
            for entry in def.entries {
                if !ids.insert(entry.id.clone()) {
                    errors.push(format!(
                        "{}: duplicate special id {}",
                        path.display(),
                        entry.id
                    ));
                }
                let mut files = vec![relative(&path, data_dir)];
                if patch_ids.contains(&entry.id) {
                    files.push(relative(patch.as_ref().unwrap(), data_dir));
                }
                let replaced_files = source_by_id
                    .get(&entry.id)
                    .map(|old| old.files.clone())
                    .unwrap_or_default();
                let non_effective_fields = non_effective(&entry.mods, "mods");
                source_by_id.insert(
                    entry.id.clone(),
                    RuleOrigin {
                        id: entry.id,
                        files,
                        replaced_files,
                        non_effective_fields,
                    },
                );
            }
        }
    }

    // Use the runtime loader's common -> version -> patch merge. Generated
    // domains are concatenated, so their ID conflicts remain errors.
    let data = GameData::new(data_dir);
    let mut special_entries = Vec::new();
    for result in [
        data.special_mods(),
        data.special_derived(),
        data.special_vendor(),
    ] {
        match result {
            Ok(Some(def)) => special_entries.extend(def.entries),
            Ok(None) => {}
            Err(e) => errors.push(e.to_string()),
        }
    }
    let mut registry = HandlerRegistry::new();
    register_special_handlers(&mut registry).map_err(|e| e.to_string())?;
    for entry in &special_entries {
        if let Err(e) = SpecialModRules::compile(std::slice::from_ref(entry), &registry) {
            errors.push(format!(
                "{}: {e}",
                source_by_id.get(&entry.id).map_or_else(
                    || "effective rules".into(),
                    |origin| origin.files.join(" + ")
                )
            ));
        }
    }
    if errors.is_empty()
        && let Some(doc) = &rules_doc
    {
        match CompiledParserRules::compile_with_special(doc, &special_entries) {
            Err(e) => errors.push(format!("{}: compile failed: {e}", rules_path.display())),
            Ok(rules) => {
                for entry in &special_entries {
                    for example in &entry.examples {
                        if !pobr_core::mod_parser::engine::matching_special_entry_ids(
                            example, &rules,
                        )
                        .contains(&entry.id.as_str())
                        {
                            errors.push(format!(
                                "{}: example does not match its rule: {example:?}",
                                entry.id
                            ));
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(ValidationReport {
            engine_version: env!("CARGO_PKG_VERSION").into(),
            data_version: data_dir.file_name().unwrap_or_default().to_string_lossy().into(),
            scope: "Special-rule schemas, handlers, captures, operations and scopes; generic parser schema/compile. StatId is open; recognition is not calculation support. Overlap audit covers sampled source text only.".into(),
            input_sha256,
            effective_rules: special_entries.iter().map(|e| source_by_id[&e.id].clone()).collect(),
        })
    } else {
        Err(errors.join("\n"))
    }
}

fn relative(path: &Path, data_dir: &Path) -> String {
    path.strip_prefix(data_dir.parent().unwrap_or(data_dir))
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn record_input(
    path: &Path,
    data_dir: &Path,
    hashes: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    if path.is_file() {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        hashes.insert(
            relative(path, data_dir),
            format!("{:x}", Sha256::digest(bytes)),
        );
    }
    Ok(())
}

fn record_patch(
    path: &Path,
    data_dir: &Path,
    hashes: &mut BTreeMap<String, String>,
) -> Result<Option<std::path::PathBuf>, String> {
    let patch = path
        .strip_prefix(data_dir)
        .ok()
        .map(|rel| data_dir.join("patch").join(rel))
        .filter(|p| p.is_file());
    if let Some(path) = &patch {
        record_input(path, data_dir, hashes)?;
    }
    Ok(patch)
}

fn non_effective(
    mods: &[pobr_data::catalog::parser_rules::ModTemplateDef],
    path: &str,
) -> Vec<String> {
    use pobr_data::catalog::parser_rules::{TemplateNameDef, TemplateValueDef};
    let mut fields = Vec::new();
    for (i, m) in mods.iter().enumerate() {
        let path = format!("{path}[{i}]");
        if matches!(&m.name, TemplateNameDef::Literal(n) if n.is_empty()) {
            fields.push(format!("{path}: config-discovery marker"));
        }
        match &m.value {
            TemplateValueDef::List(_) => fields.push(format!(
                "{path}.value: structured LIST has no interpreter consumer"
            )),
            TemplateValueDef::Nested { mods } => {
                fields.extend(non_effective(mods, &format!("{path}.value.mods")))
            }
            _ => {}
        }
    }
    fields
}

// Scan raw files too: merging ID arrays must not conceal duplicate entries.
fn check_raw_ids(bytes: &[u8], path: &Path, errors: &mut Vec<String>) {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) {
        let mut ids = BTreeSet::new();
        for entry in value["entries"].as_array().into_iter().flatten() {
            if let Some(id) = entry["id"].as_str()
                && !ids.insert(id)
            {
                errors.push(format!("{}: duplicate special id {id}", path.display()));
            }
        }
    }
}

/// Deserialize `path` into `T`, appending any parse error and every unknown
/// field (via `serde_ignored`) to `errors`. `_meta` provenance headers are
/// intentional and never reported. Returns the value on success.
fn load_strict<T: DeserializeOwned>(
    path: &Path,
    data_dir: &Path,
    errors: &mut Vec<String>,
) -> Option<T> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            errors.push(format!("{}: read failed: {e}", path.display()));
            return None;
        }
    };
    check_raw_ids(&bytes, path, errors);
    let bytes = if let Ok(rel) = path.strip_prefix(data_dir) {
        let patch = data_dir.join("patch").join(rel);
        if patch.is_file() {
            let _: Option<T> = deserialize_strict(&bytes, path, errors);
            let mut merge = || -> Result<Vec<u8>, String> {
                let base = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                let patch_bytes = std::fs::read(&patch).map_err(|e| e.to_string())?;
                check_raw_ids(&patch_bytes, &patch, errors);
                let patch_value =
                    serde_json::from_slice(&patch_bytes).map_err(|e| e.to_string())?;
                let value = pobr_gamedata::merge(base, patch_value).map_err(|e| e.to_string())?;
                serde_json::to_vec(&value).map_err(|e| e.to_string())
            };
            match merge() {
                Ok(bytes) => bytes,
                Err(e) => {
                    errors.push(format!("{}: {e}", patch.display()));
                    return None;
                }
            }
        } else {
            bytes
        }
    } else {
        bytes
    };
    deserialize_strict(&bytes, path, errors)
}

fn deserialize_strict<T: DeserializeOwned>(
    bytes: &[u8],
    path: &Path,
    errors: &mut Vec<String>,
) -> Option<T> {
    let mut unknown = Vec::new();
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let result: Result<T, _> = serde_ignored::deserialize(&mut de, |p| {
        let p = p.to_string();
        // `_meta` is the generator/provenance header, absent from the consumer
        // schema on purpose — not a contributor mistake.
        if p != "_meta" && !p.starts_with("_meta.") {
            unknown.push(p);
        }
    });
    if let Err(e) = de.end() {
        errors.push(format!("{}: {e}", path.display()));
    }
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

    #[test]
    fn body_armour_crit_rule_has_effective_common_provenance() {
        for version in [
            pobr_data::DATA_VERSION,
            pobr_data::GOLDEN_PARITY_DATA_VERSION,
        ] {
            let data = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../data")
                .join(version);
            let report = inspect(&data).unwrap();
            let rules = crate::parsed::compile_parser_rules(&data).unwrap();
            let parsed = pobr_core::mod_parser::parse_mod_engine(
                "Body Armour grants Hits against you have 50% reduced Critical Damage Bonus",
                &rules,
            );
            let id = parsed.special_meta.unwrap().entry_id;
            assert_eq!(id, "body_armour_grants_reduced_crit_damage_bonus_100");
            let origin = report.effective_rules.iter().find(|r| r.id == id).unwrap();
            assert_eq!(origin.files, ["overlay-common/special_mods.json"]);
            assert!(origin.replaced_files.is_empty());
            assert!(origin.non_effective_fields.is_empty());
            assert_eq!(report.input_sha256[&origin.files[0]].len(), 64);
        }
    }

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

    #[test]
    fn effective_overrides_patches_removal_and_provenance_match_runtime() {
        use serde_json::json;
        let tmp = std::env::temp_dir().join(format!("pobr-rule-layers-{}", std::process::id()));
        let data = tmp.join("test");
        let write = |file: &str, value: serde_json::Value| {
            let path = tmp.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        };
        let entry = |id: &str, value: i64| json!({"id":id,"pattern":"gain life","batch":"test","examples":["Gain Life"],"mods":[{"name":"MaximumLife","type":"BASE","value":value}]});
        write(
            "test/overlay/mod_parser_rules.json",
            json!({"forms":[],"name_map":[],"flag_phrases":[],"pre_flags":[],"tag_phrases":[]}),
        );
        write("test/generated/special_vendor.json", json!({"entries":[]}));
        write(
            "overlay-common/special_mods.json",
            json!({"entries":[entry("shared",1),entry("later",2)]}),
        );
        write(
            "test/overlay/special_mods.json",
            json!({"entries":[entry("shared",3)]}),
        );
        write(
            "test/patch/overlay/special_mods.json",
            json!({"entries":[{"id":"shared","mods":[{"name":"MaximumLife","type":"BASE","value":4}]}]}),
        );
        let report = inspect(&data).unwrap();
        assert_eq!(
            report
                .effective_rules
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            ["shared", "later"]
        );
        let first = &report.effective_rules[0];
        assert_eq!(
            first.files,
            [
                "test/overlay/special_mods.json",
                "test/patch/overlay/special_mods.json"
            ]
        );
        assert_eq!(first.replaced_files, ["overlay-common/special_mods.json"]);
        assert_eq!(report.input_sha256.len(), 5);
        let rules = crate::parsed::compile_parser_rules(&data).unwrap();
        let parsed = pobr_core::mod_parser::parse_mod_engine("Gain [Life]", &rules);
        assert_eq!(parsed.mods[0].value.as_number(), Some(4.0));
        assert_eq!(parsed.special_meta.unwrap().entry_id, "shared");
        assert_eq!(
            pobr_core::mod_parser::engine::matching_special_entry_ids("Gain [Life]", &rules),
            ["shared", "later"]
        );

        // A patch typo and duplicate raw IDs must not disappear during merging.
        write(
            "test/patch/overlay/special_mods.json",
            json!({"entries":[{"id":"shared","mods":[{"name":"Damage","type":"INC","value":10,"tags":[{"type":"Condition","var":"Active","negate":true}]}]}]}),
        );
        assert!(check(&data).unwrap_err().contains("negate"));
        write(
            "test/patch/overlay/special_mods.json",
            json!({"entries":[{"id":"shared"},{"id":"shared"}]}),
        );
        assert!(
            check(&data)
                .unwrap_err()
                .contains("duplicate special id shared")
        );
        std::fs::remove_file(data.join("patch/overlay/special_mods.json")).unwrap();

        // Removing the version override exposes the common entry. Validate that
        // newly effective rule, even when it was previously shadowed.
        let mut invalid = entry("shared", 1);
        invalid["mods"][0]["flags"] = json!(["Typo"]);
        write(
            "overlay-common/special_mods.json",
            json!({"entries":[invalid]}),
        );
        check(&data).unwrap();
        write("test/overlay/special_mods.json", json!({"entries":[]}));
        assert!(check(&data).unwrap_err().contains("unknown flag Typo"));
        write(
            "overlay-common/special_mods.json",
            json!({"entries":[entry("shared",1)]}),
        );
        let restored = inspect(&data).unwrap();
        assert_eq!(
            restored.effective_rules[0].files,
            ["overlay-common/special_mods.json"]
        );
        assert_ne!(restored.input_sha256, report.input_sha256);
        write(
            "test/generated/special_vendor.json",
            json!({"entries":[entry("shared",5)]}),
        );
        assert!(check(&data).unwrap_err().contains("duplicate"));
        std::fs::remove_dir_all(tmp).unwrap();
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
