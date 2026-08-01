//! `gen-stat-id-map`: feeds each entry of Section A's
//! `overlay/stat_descriptions.json` (stat_id -> canonical text) through
//! `parse_mod_engine`, baking the result into `overlay/stat_id_map.json` (the
//! modifier templates for the "stat_id -> Modifier, second channel" pipeline).
//!
//! Unlike the luajit extraction targets: this command **doesn't run
//! luajit** — it purely consumes two already-generated overlays
//! (stat_descriptions + mod_parser_rules) and derives its output offline
//! through the engine. It needs regenerating whenever the engine changes
//! (the artifact evolves with engine capability).
//!
//! Responsibility split: the template keeps **structure**
//! (name/mod_type/tags/flags) and **coefficient** (the parsed value at V=1)
//! in separate fields; the tag string goes through
//! `pobr_core::mod_parser::canonical_tags` as the single serialization
//! source. All text lines for a given stat_id must parse successfully for it
//! to land in `mapped`; otherwise the whole entry goes to `unsupported`
//! (falls back to text / special channels). Sorting / byte-stability is
//! guaranteed by BTreeMap + serde_json.

use std::fs;
use std::io;
use std::path::Path;

use pobr_core::mod_parser::{CompiledParserRules, ParseStatus, canonical_tags, parse_mod_engine};
use pobr_core::{ModValue, Modifier};
use pobr_data::catalog::parser_rules::ModParserRulesDoc;
use pobr_data::catalog::stat_id_map::{ScopeStatIdMap, StatIdMapDef, StatIdModTemplate};
use serde::{Deserialize, Serialize};

use crate::extract_lua::OverlayMeta;
use crate::extract_stat_descriptions::StatDescriptionsDoc;

/// Current overlay document schema identifier.
pub const STAT_ID_MAP_SCHEMA: &str = "stat_id_map/v1";

/// The full overlay document (`_meta` header plus the flattened [`StatIdMapDef`]).
#[derive(Debug, Serialize, Deserialize)]
pub struct StatIdMapDoc {
    /// Header metadata (serialized as `_meta`).
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The mapping body (segmented by scope).
    #[serde(flatten)]
    pub def: StatIdMapDef,
}

/// Convert a single [`Modifier`] into a serializable template (structure and coefficient split into separate fields).
fn template(m: &Modifier) -> StatIdModTemplate {
    let (coefficient, value_kind) = match &m.value {
        ModValue::Number(n) => (Some(*n), None),
        ModValue::Bool(b) => (None, Some(format!("flag:{b}"))),
        ModValue::Text(s) => (None, Some(format!("text:{s}"))),
        ModValue::NestedMods(_) => (None, Some("nested".to_string())),
    };
    StatIdModTemplate {
        name: m.name.as_str().to_string(),
        mod_type: format!("{:?}", m.mod_type),
        coefficient,
        value_kind,
        tags: canonical_tags(&m.tags),
        flags: m.flags.bits(),
        keyword_flags: m.keyword_flags.bits(),
    }
}

/// Run the generation, returning the final (byte-stable) JSON text.
pub fn run_gen_stat_id_map(overlay_dir: &Path, out_for_meta: Option<String>) -> io::Result<String> {
    // Source 1: stat_descriptions.json (its _meta, including vendor commit, is passed through).
    let descs_text = fs::read_to_string(overlay_dir.join("stat_descriptions.json"))?;
    let descs: StatDescriptionsDoc = serde_json::from_str(&descs_text).map_err(io::Error::other)?;

    // Source 2: mod_parser_rules.json (serde ignores _meta, compiled into engine rules).
    let rules_text = fs::read_to_string(overlay_dir.join("mod_parser_rules.json"))?;
    let rules_doc: ModParserRulesDoc =
        serde_json::from_str(&rules_text).map_err(io::Error::other)?;
    let rules = CompiledParserRules::compile(&rules_doc)
        .map_err(|e| io::Error::other(format!("failed to compile mod_parser_rules: {e:?}")))?;

    let def = build_map(&descs.def, &rules);
    let meta = build_meta(&descs.meta, out_for_meta);

    let doc = StatIdMapDoc { meta, def };
    let mut json = serde_json::to_string_pretty(&doc).map_err(io::Error::other)?;
    json.push('\n');
    Ok(json)
}

/// Parse text lines into modifier templates per scope / stat_id, assembling [`StatIdMapDef`].
///
/// All lines for a given stat_id must be Parsed and each produce a non-empty
/// mod for it to land in `mapped`; if any line is Unsupported / empty, the
/// whole entry goes to `unsupported` (conservative: better to fall back than to compute wrong values).
pub fn build_map(
    descs: &pobr_data::catalog::stat_descriptions::StatDescriptionsDef,
    rules: &CompiledParserRules,
) -> StatIdMapDef {
    let mut def = StatIdMapDef::default();
    for (scope_name, scope) in &descs.scopes {
        let mut out_scope = ScopeStatIdMap::default();
        for (stat_id, lines) in &scope.single {
            let mut templates = Vec::new();
            let mut all_ok = true;
            for line in lines {
                let outcome = parse_mod_engine(line, rules);
                if outcome.status != ParseStatus::Parsed || outcome.mods.is_empty() {
                    all_ok = false;
                    break;
                }
                templates.extend(outcome.mods.iter().map(template));
            }
            if all_ok && !templates.is_empty() {
                out_scope.mapped.insert(stat_id.clone(), templates);
            } else {
                out_scope.unsupported.insert(stat_id.clone());
            }
        }
        def.scopes.insert(scope_name.clone(), out_scope);
    }
    def
}

/// Build `_meta`: the vendor commit is passed through from source
/// stat_descriptions' `_meta` (this table is derived from that extraction);
/// extracted_files records the two source overlays; regen_command records this command.
fn build_meta(source_meta: &OverlayMeta, out_for_meta: Option<String>) -> OverlayMeta {
    let mut regen =
        "cargo run -p sync-pob-catalog -- gen-stat-id-map --overlay-dir data/<version>/overlay"
            .to_string();
    if let Some(out) = &out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    OverlayMeta {
        schema: STAT_ID_MAP_SCHEMA.to_string(),
        generator: "sync-pob-catalog gen-stat-id-map".to_string(),
        vendor: source_meta.vendor.clone(),
        vendor_commit: source_meta.vendor_commit.clone(),
        vendor_commit_subject: source_meta.vendor_commit_subject.clone(),
        extracted_files: vec![
            "overlay/stat_descriptions.json".to_string(),
            "overlay/mod_parser_rules.json".to_string(),
        ],
        regen_command: regen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::stat_descriptions::{ScopeDescriptions, StatDescriptionsDef};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn descs_with(scope: &str, entries: &[(&str, &str)]) -> StatDescriptionsDef {
        let mut single = BTreeMap::new();
        for (id, text) in entries {
            single.insert(id.to_string(), vec![text.to_string()]);
        }
        let mut scopes = BTreeMap::new();
        scopes.insert(
            scope.to_string(),
            ScopeDescriptions {
                single,
                ..Default::default()
            },
        );
        StatDescriptionsDef { scopes }
    }

    /// Load the real overlay rules (no test-rules feature needed; same approach as stat_desc_parse_rate).
    fn real_rules() -> CompiledParserRules {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/4.5.0.3.4/overlay/mod_parser_rules.json");
        let doc: ModParserRulesDoc =
            serde_json::from_str(&fs::read_to_string(path).expect("read rules")).expect("doc");
        CompiledParserRules::compile(&doc).expect("compile")
    }

    /// Parseable text -> mapped; unparseable -> unsupported (conservatively falls back for the whole entry).
    #[test]
    fn maps_parseable_and_quarantines_rest() {
        let rules = real_rules();
        let descs = descs_with(
            "stat_descriptions",
            &[
                ("additional_strength", "+1 to Strength"),
                ("gibberish_stat", "Walk the Paths Not Taken"),
            ],
        );
        let def = build_map(&descs, &rules);
        let scope = &def.scopes["stat_descriptions"];
        // strength must land in mapped (a standard, generic mod).
        assert!(scope.mapped.contains_key("additional_strength"));
        let tmpl = &scope.mapped["additional_strength"][0];
        assert_eq!(tmpl.name, "Strength");
        assert_eq!(tmpl.mod_type, "Base");
        // gibberish must land in unsupported (no rule matches it).
        assert!(scope.unsupported.contains("gibberish_stat"));
    }

    /// Template coefficient: a numeric mod at V=1 has coefficient = 1.0.
    #[test]
    fn number_template_coefficient_is_one() {
        let m = Modifier::number("Strength", pobr_data::prelude::ModType::Base, 1.0);
        let t = template(&m);
        assert_eq!(t.name, "Strength");
        assert_eq!(t.mod_type, "Base");
        assert_eq!(t.coefficient, Some(1.0));
        assert!(t.value_kind.is_none());
    }
}
