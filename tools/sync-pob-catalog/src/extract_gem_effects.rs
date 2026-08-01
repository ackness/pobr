//! `extract-lua --what gem-effects`: extracts vendor PoB2 `Data/Gems.lua`'s
//! gem -> granted-effect links into `data/<version>/overlay/gem_effects.json`
//! (a data-plane source and the data source for contract C5's
//! `SkillGemDef.granted_effect_id`).
//!
//! **Channel note**: originally planned to come from the `.dat` table
//! `GemEffects` via the adapter into `base/`, but the bundle containing that
//! table is no longer downloadable at the pinned patch 4.5.0.3.4 (verified —
//! see `_tablesUnavailableForPinnedPatch` in `pipeline/config.json`). Per the
//! owner's call to "let the producing tool define the layer," it's extracted
//! via extract-lua into **overlay/** instead. Vendor's `Data/Gems.lua` is
//! itself an export of that table (`Export/Scripts/skills.lua:898-925`), so
//! the extraction is a faithful transcription. If the `.dat` table channel
//! comes back, this should migrate back to `base/` (a byte-equivalent
//! migration commit).
//!
//! Responsibility split matches [`crate::extract_lua`]: the Lua bootstrap
//! script (`extract_gem_effects.lua`, embedded at compile time) only does
//! faithful extraction and emits JSONL; the Rust side handles sorting
//! (ascending gem_id), duplicate gem_id validation, and whole-document
//! serialization, guaranteeing **byte-stable** output on repeated runs with the same input.

use std::io;

use pobr_data::catalog::GemEffectDef;
use serde::{Deserialize, Serialize};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory)
const BOOTSTRAP_LUA: &str = include_str!("extract_gem_effects.lua");

/// Current overlay document schema identifier (bumped when fields evolve)
pub const GEM_EFFECTS_SCHEMA: &str = "gem_effects/v1";

/// One JSONL line emitted by the bootstrap script: one gem variant's effect link.
#[derive(Debug, Clone, Deserialize)]
pub struct GemEffectRow {
    /// The gem base id (vendor `gameId`).
    pub gem_id: String,
    /// The effect variant id (vendor `variantId`).
    pub variant_id: String,
    /// The id of the primary granted effect.
    pub granted_effect: String,
    /// Additional granted effects (in `additionalGrantedEffectId1..N` order).
    #[serde(default)]
    pub additional_granted_effects: Vec<String>,
    /// Additional statSets for the primary effect (in `additionalStatSet1..N` order).
    #[serde(default)]
    pub additional_stat_sets: Vec<String>,
}

/// The full overlay document (generation side; see
/// [`pobr_data::catalog::GemEffectsDef`] for the consumption-side schema —
/// matching serde shapes guard against field drift).
#[derive(Debug, Serialize, Deserialize)]
pub struct GemEffectsDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file)
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The gem -> effect link table, ascending by gem_id.
    pub gems: Vec<GemEffectDef>,
}

/// Run the extraction, returning the final (byte-stable) JSON text.
pub fn run_extract_gem_effects(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows: Vec<GemEffectRow> = invoke_luajit_jsonl(args, BOOTSTRAP_LUA)?;
    let meta = build_meta(args)?;
    assemble_gem_effects_document(meta, rows)
}

/// Assemble the final document: sort ascending by gem_id + validate no
/// duplicate gem_id (current data is 1 gem <-> 1 variant; a duplicate means
/// the vendor data shape changed, so this errors rather than silently
/// picking one) + serde_json serialization.
pub fn assemble_gem_effects_document(
    meta: OverlayMeta,
    rows: Vec<GemEffectRow>,
) -> io::Result<String> {
    let mut gems: Vec<GemEffectDef> = rows
        .into_iter()
        .map(|row| GemEffectDef {
            gem_id: row.gem_id,
            variant_id: row.variant_id,
            granted_effect_id: row.granted_effect,
            additional_granted_effect_ids: row.additional_granted_effects,
            additional_stat_set_ids: row.additional_stat_sets,
        })
        .collect();
    gems.sort_by(|a, b| a.gem_id.cmp(&b.gem_id));
    if let Some(dup) = gems.windows(2).find(|w| w[0].gem_id == w[1].gem_id) {
        return Err(io::Error::other(format!(
            "gem_effects extraction found duplicate gem_id `{}` (variants {} / {}) — vendor data shape has \
             changed and the consumer's 1:1 merge assumption no longer holds; schema needs adjusting first",
            dup[0].gem_id, dup[0].variant_id, dup[1].variant_id
        )));
    }
    let doc = GemEffectsDoc { meta, gems };
    let mut json = serde_json::to_string_pretty(&doc)
        .expect("gem effects document serialization should not fail");
    json.push('\n');
    Ok(json)
}

/// Build `_meta` (same convention as the shared layer: regen_command writes a canonical relative path).
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = "cargo run -p sync-pob-catalog -- extract-lua --what gem-effects \
         --vendor-root vendor/PathOfBuilding-PoE2/src"
        .to_string();
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: GEM_EFFECTS_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        // This target always reads a single file (--files in the shared call layer is just a placeholder, not used for lookup).
        extracted_files: vec!["Data/Gems.lua".to_string()],
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> OverlayMeta {
        OverlayMeta {
            schema: GEM_EFFECTS_SCHEMA.into(),
            generator: "test".into(),
            vendor: "PathOfBuilding-PoE2".into(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "subject".into(),
            extracted_files: vec!["Data/Gems.lua".into()],
            regen_command: "cargo run …".into(),
        }
    }

    fn row(gem: &str, variant: &str, granted: &str) -> GemEffectRow {
        GemEffectRow {
            gem_id: gem.into(),
            variant_id: variant.into(),
            granted_effect: granted.into(),
            additional_granted_effects: vec![],
            additional_stat_sets: vec![],
        }
    }

    /// Sort determinism: shuffled input -> gem_id ascending output; repeated runs with the same input are byte-identical.
    #[test]
    fn sorts_by_gem_id_and_is_byte_stable() {
        let rows = vec![row("B", "b", "BPlayer"), row("A", "a", "APlayer")];
        let one = assemble_gem_effects_document(meta(), rows).unwrap();
        let rows2 = vec![row("A", "a", "APlayer"), row("B", "b", "BPlayer")];
        let two = assemble_gem_effects_document(meta(), rows2).unwrap();
        assert_eq!(one, two);
        let a = one.find("\"gem_id\": \"A\"").unwrap();
        let b = one.find("\"gem_id\": \"B\"").unwrap();
        assert!(a < b);
    }

    /// Duplicate gem_id (vendor's 1:1 assumption broke) -> errors, doesn't silently swallow it.
    #[test]
    fn duplicate_gem_id_errors_out() {
        let rows = vec![row("A", "a1", "APlayer"), row("A", "a2", "APlayerTwo")];
        assert!(assemble_gem_effects_document(meta(), rows).is_err());
    }

    /// The consumption-side schema (GemEffectsDef) can read back the generation-side document (matching serde shapes guard against drift).
    #[test]
    fn consumer_schema_roundtrip() {
        let mut entry = row("A", "a", "APlayer");
        entry.additional_granted_effects = vec!["AExtraPlayer".into()];
        entry.additional_stat_sets = vec!["AOnFrostbolt".into()];
        let json = assemble_gem_effects_document(meta(), vec![entry]).unwrap();
        let def: pobr_data::catalog::GemEffectsDef = serde_json::from_str(&json).unwrap();
        assert_eq!(def.gems.len(), 1);
        assert_eq!(def.gems[0].granted_effect_id, "APlayer");
        assert_eq!(def.gems[0].additional_granted_effect_ids, ["AExtraPlayer"]);
        assert_eq!(def.gems[0].additional_stat_set_ids, ["AOnFrostbolt"]);
    }
}
