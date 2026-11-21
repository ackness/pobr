//! Raw curated-rule validation, including entries shadowed by a version override.
//! The effective merged rule set is checked by [`super::special_mods_gate`].

use pobr_core::rules::{HandlerRegistry, SpecialModRules};
use pobr_data::catalog::parser_rules::{SpecialModsDef, SpecialTemplateDef};

/// The two raw curated `special_mods` layers: the version-independent curation
/// layer `data/overlay-common/` plus the version layer `data/<ver>/overlay/`.
/// Their union covers every curated pattern, including entries shadowed by a
/// version override.
fn special_mods_paths() -> Vec<std::path::PathBuf> {
    let dir = crate::support::test_data_dir();
    vec![
        dir.join("../overlay-common/special_mods.json"),
        dir.join("overlay/special_mods.json"),
    ]
}

/// Missing layers are allowed for legacy packages; I/O and parse errors fail.
fn load_layers() -> Vec<(std::path::PathBuf, SpecialModsDef)> {
    let mut layers = Vec::new();
    for path in special_mods_paths() {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => panic!("{}: {e}", path.display()),
        };
        let doc = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        layers.push((path, doc));
    }
    layers
}

fn load_entries() -> Vec<SpecialTemplateDef> {
    load_layers()
        .into_iter()
        .flat_map(|(_, doc)| doc.entries)
        .collect()
}

/// Compile each raw layer separately: same-id cross-layer overrides are valid,
/// while malformed shadowed rules and duplicate ids within a layer still fail.
#[test]
fn raw_curated_layers_compile_clean() {
    let layers = load_layers();
    assert!(layers.iter().any(|(_, doc)| !doc.entries.is_empty()));
    crate::support::install_test_skill_types();
    let mut registry = HandlerRegistry::new();
    pobr_core::rules::register_special_handlers(&mut registry).expect("register special handlers");
    for (path, doc) in layers {
        let rules = SpecialModRules::compile(&doc.entries, &registry)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(rules.len(), doc.entries.len(), "{}", path.display());
    }
}

/// Numeric captures use a fixed set of shapes: `(\d+)` / `(\d+(?:\.\d+)?)` /
/// `([+-]\d+)` / `(\+?\d+)` / `([+-]?\d+(?:\.\d+)?)`. Any capture outside that set must be an
/// explicit closed set (no `\d`) — the DSL's hard boundary of "no open captures".
#[test]
fn captures_are_numeric_or_closed_sets() {
    const NUMERIC_FORMS: [&str; 5] = [
        r"(\d+)",
        r"(\d+(?:\.\d+)?)",
        r"([+-]\d+)",
        r"(\+?\d+)",
        r"([+-]?\d+(?:\.\d+)?)",
    ];
    for entry in &load_entries() {
        let id = &entry.id;
        let pattern = &entry.pattern;
        // Scan capture group by capture group (top-level parens, ignoring (?: non-capturing groups)
        let bytes = pattern.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'(' && !pattern[i..].starts_with("(?:") {
                // Find the balanced closing paren
                let mut depth = 1;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'(' {
                        depth += 1;
                    } else if bytes[j] == b')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                let group = &pattern[i..j];
                let is_numeric = NUMERIC_FORMS.contains(&group);
                assert!(
                    is_numeric || !group.contains("\\d"),
                    "{id}: capture group {group:?} is neither a standard numeric form nor a closed set"
                );
                i = j;
            } else {
                i += 1;
            }
        }
    }
}
