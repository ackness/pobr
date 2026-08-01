//! Generates the keystone-derived special table.
//!
//! Deterministically derives `generated/special_derived.json` (schema =
//! `special_derived/v1`, the same structure as `special_mods/v1`) from the
//! keystone nodes in `data/<patch>/base/passive_tree.json`: each keystone
//! name becomes one whole-line-anchored special entry that produces a
//! `Keystone LIST` mod (literal value = the node's canonical name, kept in
//! proper case — matching field-for-field with
//! `pobr-core::mod_parser::parse_keystone_grant`'s generic path, avoiding a
//! case mismatch; the keystone's actual mods get expanded via
//! `env_finalize`'s `merge_keystones` lookup table).
//!
//! Cross-referenced against vendor `ModParser.lua:6151-6158`: each name in
//! `data.keystones` is registered as a whole-line specialModList key. This
//! step treats passive_tree.json's keystone nodes as the full set (a
//! superset is harmless — it just recognizes a few extra lines; any discrepancy is noted in `_meta`).
//!
//! **byte-stable discipline**: entries are sorted lexicographically by
//! keystone name, and serialization goes through the uniform pretty writer
//! ([`crate::write_pretty`]); rerunning with the same input produces zero byte-diff (covered by regen-check).
//!
//! **Downstream note**: when this step's output migrates into
//! `tools/precompile-mods`, the keystone segment must stay byte-equivalent.

use std::path::PathBuf;

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};
use serde::Serialize;

use crate::write_pretty;

pub struct SpecialDerivedArgs {
    /// Path to the already-stored `passive_tree.json` (a node array).
    pub tree_json: PathBuf,
    /// The data root (`data/`); output goes to `<out>/<patch>/generated/special_derived.json`.
    pub out: PathBuf,
    pub patch: String,
}

// -- Output schema (serialization side; same structure as pobr-data's
//    SpecialModsDef, defined independently to avoid coupling the pure-data crate to a serialization dependency) --

#[derive(Serialize)]
struct DerivedDoc {
    #[serde(rename = "_meta")]
    meta: DerivedMeta,
    entries: Vec<DerivedEntry>,
}

#[derive(Serialize)]
struct DerivedMeta {
    schema: &'static str,
    generator: &'static str,
    note: String,
}

#[derive(Serialize)]
struct DerivedEntry {
    id: String,
    pattern: String,
    mods: Vec<DerivedMod>,
    verified: bool,
    batch: &'static str,
    source_note: String,
}

#[derive(Serialize)]
struct DerivedMod {
    name: &'static str,
    #[serde(rename = "type")]
    mod_type: &'static str,
    value: String,
}

/// Snake-cases a keystone name into a stable id (matching the existing special_mods.json id style).
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_us = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    out.trim_matches('_').to_string()
}

/// Escapes regex metacharacters (keystone names contain `'`/spaces/etc.; whole-line anchoring is added by the interpreter with `^...$`).
fn regex_escape_lower(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    for c in lower.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn run(args: SpecialDerivedArgs) -> Result<String, String> {
    let bytes = std::fs::read(&args.tree_json)
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_json.display()))?;
    let nodes: Vec<PassiveNodeDef> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("解析 {} 失败：{e}", args.tree_json.display()))?;

    let mut entries: Vec<DerivedEntry> = nodes
        .iter()
        .filter(|n| n.kind == PassiveNodeKind::Keystone)
        .filter_map(|n| n.name.as_deref())
        .map(|name| DerivedEntry {
            id: format!("keystone_{}", slug(name)),
            // The whole line, lowercased and escaped; the interpreter wraps
            // it in ^...$ at compile time (cross-referenced against vendor :6155-6158).
            pattern: regex_escape_lower(name),
            mods: vec![DerivedMod {
                name: "Keystone",
                mod_type: "LIST",
                // A proper-case literal value — matches the generic parse_keystone_grant field-for-field.
                value: name.to_string(),
            }],
            verified: false,
            batch: "S0",
            source_note: format!("passive_tree keystone「{name}」（C-1 派生）"),
        })
        .collect();

    // byte-stable: sorted lexicographically by id.
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let count = entries.len();

    let doc = DerivedDoc {
        meta: DerivedMeta {
            schema: "special_derived/v1",
            generator: "pobr-data-adapter --emit-special-derived",
            note: format!(
                "{count} 个 keystone（passive_tree kind=Keystone）派生；\
                 对照 vendor ModParser.lua:6151-6158 data.keystones（超集无害）"
            ),
        },
        entries,
    };

    let path = args
        .out
        .join(&args.patch)
        .join("generated")
        .join("special_derived.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建 {} 失败：{e}", parent.display()))?;
    }
    write_pretty(&path, &doc)?;
    Ok(format!(
        "special_derived: {count} keystone 条目 → {}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normalizes() {
        assert_eq!(slug("Unwavering Stance"), "unwavering_stance");
        assert_eq!(slug("Zealot's Oath"), "zealot_s_oath");
        assert_eq!(slug("Mind Over Matter"), "mind_over_matter");
    }

    #[test]
    fn escape_lowers_and_escapes() {
        assert_eq!(regex_escape_lower("Zealot's Oath"), "zealot's oath");
        assert_eq!(regex_escape_lower("Eldritch Battery"), "eldritch battery");
    }
}
