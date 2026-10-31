//! Physical quality facts through the existing ordinary effect-wide conversion.

mod policy;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;
use crate::skills::quality::{self, IndexedId, RawQuality};

pub(super) use policy::Args;
pub(super) const TABLES: [&str; 3] = ["GrantedEffects", "Stats", "GrantedEffectQualityStats"];
const FIELDS: [&str; 7] = [
    "GrantedEffect",
    "Stats",
    "StatsValuesPermille",
    "ApplyToStatSets",
    "AltStats",
    "AltStatValuesPermille",
    "AltApplyToStatSets",
];

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub receipt: Value,
    pub policy_bytes: Vec<u8>,
    pub policy_parent: PathBuf,
}

pub(super) fn convert(input: &VerifiedFacts, args: &Args) -> Result<Converted, String> {
    let pinned = policy::read(args, input)?;
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    let effects = identities(&tables, "GrantedEffects")?;
    let stats = identities(&tables, "Stats")?;
    let mut rows = Vec::new();
    let mut owners = Vec::new();
    let mut excluded = Vec::new();
    for source_row in 0..tables.len("GrantedEffectQualityStats") {
        let raw = tables.row("GrantedEffectQualityStats", source_row)?;
        let row: RawQuality = serde_json::from_value(raw.clone())
            .map_err(|e| format!("GrantedEffectQualityStats/{source_row}: {e}"))?;
        // The physical schema declares signed i32 scopes; the existing quality
        // metadata uses u32 positions. Zero is valid, null/negative is not a position.
        for scope in row.sets.iter().chain(&row.alt_sets) {
            i32::try_from(*scope)
                .map_err(|_| format!("quality/{source_row}: scope exceeds source i32"))?;
        }
        let id = effects.get(&row.effect).ok_or_else(|| {
            format!(
                "quality/{source_row}: dangling GrantedEffect {}",
                row.effect
            )
        })?;
        owners
            .push(json!({"effect_id":id, "source_row":source_row, "effect_source_row":row.effect}));
        if let Some(reason) = pinned.policy.compatibility.excluded_effects.get(id) {
            // Preserve excluded raw values as evidence, not as runtime quality.
            let facts: BTreeMap<_, _> = FIELDS.iter().map(|name| (*name, &raw[*name])).collect();
            excluded.push(json!({
                "effect_id":id, "source_row":source_row, "effect_source_row":row.effect,
                "reason":reason, "raw_fields":facts,
                "status":"compatibility-excluded; not a claim of no effect"
            }));
        }
        rows.push(row);
    }
    // Validates both channels, including excluded owners: no unresolved references,
    // duplicate owners or missing decisions can be hidden by policy filtering.
    let converted = quality::convert_rows(&effects, &stats, rows, &pinned.policy.compatibility)?;
    let inputs: BTreeMap<_, _> = TABLES
        .iter()
        .map(|table| {
            let path = format!("tables/{table}.json");
            let bytes = &input.bytes[&path];
            (
                path,
                json!({"bytes":bytes.len(), "sha256":sha256_hex(bytes)}),
            )
        })
        .collect();
    let doc = json!({
        "_meta": {
            "schema":"gem_quality_stats/v1",
            "generator":"pobr-data-adapter --ordinary-domain gem-quality",
            "poe_version":input.manifest["source_identity"]["game_patch"],
            "source_manifest_sha256":sha256_hex(&input.manifest_bytes),
            "quality_policy_sha256":pinned.sha256,
            "target_source":input.manifest["source_identity"],
            "inputs":inputs,
            "compatibility":pinned.policy.compatibility,
            "model_reference":pinned.policy.reference,
            "target_applicability":pinned.policy.target_applicability,
            "unapplied_stat_set_scopes":converted.scopes
        },
        "effects":converted.effects
    });
    let receipt = json!({
        "enabled_effects":converted.effects.len(),
        "source_quality_rows":tables.len("GrantedEffectQualityStats"),
        "quality_policy":{"file":"quality-policy.json", "bytes":pinned.bytes.len(), "sha256":pinned.sha256},
        "model_reference":pinned.policy.reference,
        "target_applicability":pinned.policy.target_applicability,
        "owners":owners,
        "excluded_effects":excluded,
        "unapplied_stat_set_scopes":converted.scopes,
        "unmapped_source_columns":unmapped(input, &tables)?,
        "mapping": {
            "identity":"GrantedEffect -> GrantedEffects.Id; Stats/AltStats -> Stats.Id via verified physical rows, not values._index, names or stable-ID sorting",
            "rates":"StatsValuesPermille/AltStatValuesPermille signed i32 / 1000.0; main then alt, preserving zero, negative values, order and repeated stats",
            "scope":"ApplyToStatSets/AltApplyToStatSets preserved unapplied under the existing effect-wide model policy; positions are not foreign keys",
            "classification":"complete policy enabled/excluded owner partition; IsSupport and gem membership do not filter quality",
            "consumer":"unchanged alt/Gemling gating and trunc(rate * quality); no AddTypes/AddMinionTypes or set application mechanism"
        },
        "pending":[
            "Unapplied scope positions and unconsumed columns remain target facts, not globally effectless.",
            "Other skill compatibility assets, minion mappings and complete ordinary runtime assembly/adoption remain outside this domain.",
            "Target applicability is reuse of the existing model policy for the pinned F; it is not independent game-rule proof or source acquisition approval."
        ]
    });
    Ok(Converted {
        overlay: BTreeMap::from([("gem_quality_stats.json".into(), super::pretty_bytes(&doc)?)]),
        receipt,
        policy_bytes: pinned.bytes,
        policy_parent: pinned.parent,
    })
}

fn identities(tables: &Tables<'_>, table: &str) -> Result<BTreeMap<usize, String>, String> {
    let rows = (0..tables.len(table))
        .map(|index| {
            let raw = tables.row(table, index)?;
            Ok(IndexedId {
                // Tables::selected has already checked _row == index in these bytes.
                index,
                id: raw["Id"].as_str().ok_or("invalid stable Id")?.into(),
            })
        })
        .collect::<Result<_, String>>()?;
    quality::index(rows, table)
}

fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for (table, name, kind, array, target) in [
        ("GrantedEffects", "Id", "string", false, None),
        ("Stats", "Id", "string", false, None),
        (
            "GrantedEffectQualityStats",
            "GrantedEffect",
            "foreignrow",
            false,
            Some("GrantedEffects"),
        ),
        (
            "GrantedEffectQualityStats",
            "Stats",
            "foreignrow",
            true,
            Some("Stats"),
        ),
        (
            "GrantedEffectQualityStats",
            "StatsValuesPermille",
            "i32",
            true,
            None,
        ),
        (
            "GrantedEffectQualityStats",
            "ApplyToStatSets",
            "i32",
            true,
            None,
        ),
        (
            "GrantedEffectQualityStats",
            "AltStats",
            "foreignrow",
            true,
            Some("Stats"),
        ),
        (
            "GrantedEffectQualityStats",
            "AltStatValuesPermille",
            "i32",
            true,
            None,
        ),
        (
            "GrantedEffectQualityStats",
            "AltApplyToStatSets",
            "i32",
            true,
            None,
        ),
    ] {
        super::convert::column(
            input,
            table,
            &[name],
            &json!({
                "name":name, "type":kind, "array":array, "interval":false,
                "references":target.map(|table| json!({"table":table}))
            }),
        )?;
    }
    Ok(())
}

fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for table in TABLES {
        let doc = &input.documents[&format!("tables/{table}.json")];
        let used: &[&str] = if table == "GrantedEffectQualityStats" {
            &FIELDS
        } else {
            &["Id"]
        };
        let columns: Vec<_> = doc["columns"]
            .as_array()
            .ok_or("missing columns")?
            .iter()
            .filter(|c| {
                c["physical_fields"].as_array().is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|f| !used.iter().any(|name| f.as_str() == Some(*name)))
                })
            })
            .collect();
        if columns.is_empty() {
            continue;
        }
        // Owner identities are listed once above. The original pinned table retains
        // all values, including additional-type arrays; do not duplicate whole tables.
        result.push(json!({
            "table":table, "payload":format!("tables/{table}.json"),
            "columns":columns, "source_row_range":{"start":0, "end_exclusive":tables.len(table)},
            "affected_ids":if table == "GrantedEffectQualityStats" {
                "conversion.owners maps every quality row to its effect ID"
            } else {
                "Id in each source row; conversion.owners identifies the quality-owner subset"
            },
            "status":"unconsumed; retained in pinned F, not inferred effectless"
        }));
    }
    Ok(result)
}
