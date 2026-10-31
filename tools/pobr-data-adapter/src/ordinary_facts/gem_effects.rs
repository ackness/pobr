//! Target gem links, with separately pinned legacy display and owner precedence.

mod policy;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::{GemEffectDef, GemEffectsDef};
use serde::Serialize;
use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 5] = [
    "BaseItemTypes",
    "SkillGems",
    "GemEffects",
    "GrantedEffects",
    "GrantedEffectStatSets",
];

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub source_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub protected_parents: [PathBuf; 2],
    pub receipt: Value,
}

#[derive(Serialize)]
struct Reference {
    source_row: usize,
    id: String,
}

struct Variant {
    primary: Option<Reference>,
    additional: Vec<Reference>,
}

struct Target {
    gems: BTreeMap<String, GemEffectDef>,
    sources: BTreeMap<String, Value>,
    exclusions: Vec<Value>,
}

pub(super) fn convert(input: &VerifiedFacts, args: &super::CodecArgs) -> Result<Converted, String> {
    // Share the established pin-before-parse auxiliary reader, not a second envelope.
    let policy_input =
        super::stat_set_codec::read_pinned(&args.policy, &args.policy_sha256, "codec policy")?;
    let policy: policy::Policy = serde_json::from_value(policy_input.document)
        .map_err(|e| format!("gem-effects policy: {e}"))?;
    policy.validate(input)?;
    if policy.source.sha256 != args.source_sha256 {
        return Err("codec source pin differs from policy source identity".into());
    }
    let source =
        super::stat_set_codec::read_pinned(&args.source, &args.source_sha256, "codec source")?;
    policy::validate_metadata(&source.document["_meta"])?;
    if source.document["_meta"] != policy.source.metadata {
        return Err("codec source metadata differs from policy legacy identity".into());
    }
    let codec: GemEffectsDef =
        serde_json::from_value(source.document).map_err(|e| format!("gem-effects codec: {e}"))?;
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    let mut target = target(&tables)?;
    let mut legacy_gems = BTreeSet::new();
    let mut legacy_owners = BTreeMap::new();
    let mut matches = Vec::new();
    for (codec_source_row, old) in codec.gems.iter().enumerate() {
        if old.gem_id.is_empty() || old.variant_id.is_empty() || old.granted_effect_id.is_empty() {
            return Err(format!("codec row {codec_source_row}: empty link identity"));
        }
        if !legacy_gems.insert(&old.gem_id)
            || legacy_owners
                .insert(old.granted_effect_id.clone(), old.gem_id.clone())
                .is_some()
        {
            return Err(format!(
                "codec row {codec_source_row}: duplicate gem/primary identity"
            ));
        }
        let new = target.gems.get_mut(&old.gem_id).ok_or_else(|| {
            format!(
                "codec row {codec_source_row}: gem {} absent from target normal gems",
                old.gem_id
            )
        })?;
        for (field, equal) in [
            ("variant_id", old.variant_id == new.variant_id),
            (
                "granted_effect_id",
                old.granted_effect_id == new.granted_effect_id,
            ),
            (
                "additional_granted_effect_ids",
                old.additional_granted_effect_ids == new.additional_granted_effect_ids,
            ),
            (
                "additional_stat_set_ids",
                old.additional_stat_set_ids == new.additional_stat_set_ids,
            ),
        ] {
            if !equal {
                return Err(format!(
                    "codec row {codec_source_row} ({}): target tuple mismatch: {field}",
                    old.gem_id
                ));
            }
        }
        let mut display_positions = BTreeSet::new();
        for &position in &old.granted_effect_display_order {
            // Partial orders are valid. Only duplicate/out-of-range positions fail.
            if position > new.additional_granted_effect_ids.len()
                || !display_positions.insert(position)
            {
                return Err(format!(
                    "codec row {codec_source_row}: invalid display order position {position}"
                ));
            }
        }
        new.granted_effect_display_order
            .clone_from(&old.granted_effect_display_order);
        matches.push(json!({
            "gem_id":old.gem_id, "codec_source_row":codec_source_row,
            "target_tuple":"all four relationship fields match exactly, including ordered occurrences",
            "granted_effect_display_order":old.granted_effect_display_order,
            "effect_count":1 + new.additional_granted_effect_ids.len()
        }));
    }

    // This ordering preserves BuildData's existing base-first + overlay-last-wins
    // selection while retaining every gem for GameData's by-gem lookup.
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (gem_id, gem) in &target.gems {
        groups
            .entry(gem.granted_effect_id.clone())
            .or_default()
            .push(gem_id.clone());
    }
    let mut preferred = BTreeMap::new();
    let mut collisions = Vec::new();
    for (primary, members) in &groups {
        let legacy = legacy_owners.get(primary);
        let owner = legacy.unwrap_or(&members[0]);
        preferred.insert(primary.clone(), owner.clone());
        if members.len() > 1 {
            collisions.push(json!({
                "primary":primary,"members":members,"preferred":owner,
                "reason":if legacy.is_some() {"legacy codec owner"} else {"first gem_id in existing base sort"}
            }));
        }
    }
    let mut gems: Vec<_> = target.gems.into_values().collect();
    gems.sort_by(|a, b| {
        let a_preferred = preferred[&a.granted_effect_id] == a.gem_id;
        let b_preferred = preferred[&b.granted_effect_id] == b.gem_id;
        (a_preferred, &a.gem_id).cmp(&(b_preferred, &b.gem_id))
    });
    let records: Vec<_> = gems
        .iter()
        .enumerate()
        .map(|(output_row, gem)| {
            json!({
                "output_row":output_row,"gem_id":gem.gem_id,"source":target.sources[&gem.gem_id],
                "preferred_primary_owner":preferred[&gem.granted_effect_id] == gem.gem_id
            })
        })
        .collect();
    let legacy_source = json!({
        "file":"legacy-gem-effects.json", "sha256":sha256_hex(&source.bytes),"bytes":source.bytes.len(),
        "snapshot_version":policy.source.snapshot_version,"reference_path":policy.source.path,
        "metadata":policy.source.metadata,
        "status":"legacy display/owner compatibility input; not target acquisition or numeric data"
    });
    let overlay = json!({
        "_meta":{
            "schema":"gem_effects/v1", "generator":"pobr-data-adapter ordinary-facts gem-effects",
            "game_patch":input.manifest["source_identity"]["game_patch"],
            "source_identity":input.manifest["source_identity"],
            "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),
            "relationships":"target physical facts; ordered references preserved",
            "legacy_codec":legacy_source,"codec_policy_sha256":sha256_hex(&policy_input.bytes),
            "owner_precedence":policy.owner_precedence
        },"gems":gems
    });
    let receipt = json!({
        "gems":gems.len(),"new_gems_without_legacy_codec":gems.len()-codec.gems.len(),
        "legacy_source":legacy_source,
        "codec_policy":{"file":"codec-policy.json","bytes":policy_input.bytes.len(),"sha256":sha256_hex(&policy_input.bytes)},
        "legacy_tuple_matches":matches,"source_records":records,"filtered_placeholders":target.exclusions,
        "owner_precedence":policy.owner_precedence,"shared_primary_owners":collisions,
        "mapping":{
            "gem_id":"SkillGems.BaseItemType -> BaseItemTypes.Id; same is_placeholder(Name) and nullable no-base exclusion",
            "variant_id":"single SkillGems.GemEffects -> GemEffects.Id",
            "granted_effect_id":"GemEffects.GrantedEffect -> GrantedEffects.Id",
            "additional_granted_effect_ids":"GemEffects.AdditionalGrantedEffects -> ordered GrantedEffects.Id occurrences",
            "additional_stat_set_ids":"primary GrantedEffects.AdditionalStatSets -> ordered GrantedEffectStatSets.Id occurrences",
            "granted_effect_display_order":"pinned legacy codec only; partial distinct in-range zero-based order retained, empty keeps consumer fallback"
        },
        "reference_occurrences":"positions below are zero-based positions within their named physical reference array; source_row is the validated _row, never an Id or _index value",
        "unconsumed_source":unmapped(input, &tables)?,
        "historical_source_comparison":policy.evidence,"compatibility_statement":policy.statement,
        "pending":"complete target package, prepared assets, full ordinary BD/Web selection and later-exporter compatibility are not established; unapplied mechanics are not effectless"
    });
    Ok(Converted {
        overlay: BTreeMap::from([("gem_effects.json".into(), super::pretty_bytes(&overlay)?)]),
        source_bytes: source.bytes,
        policy_bytes: policy_input.bytes,
        protected_parents: [source.parent, policy_input.parent],
        receipt,
    })
}

fn target(tables: &Tables<'_>) -> Result<Target, String> {
    // Validate every consumed field/reference, including unused variants and excluded gems.
    for row in 0..tables.len("BaseItemTypes") {
        tables.row("BaseItemTypes", row)?["Name"]
            .as_str()
            .ok_or_else(|| format!("BaseItemTypes/{row}.Name: string required"))?;
    }
    let mut sets = Vec::new();
    for row in 0..tables.len("GrantedEffects") {
        sets.push(refs(
            tables,
            "GrantedEffects",
            row,
            "AdditionalStatSets",
            "GrantedEffectStatSets",
        )?);
    }
    let mut variants = Vec::new();
    for row in 0..tables.len("GemEffects") {
        let values = tables.row("GemEffects", row)?;
        let primary = if values["GrantedEffect"].is_null() {
            None
        } else {
            Some(
                resolve(tables, "GrantedEffects", &values["GrantedEffect"])
                    .map_err(|e| format!("GemEffects/{row}.GrantedEffect: {e}"))?,
            )
        };
        variants.push(Variant {
            primary,
            additional: refs(
                tables,
                "GemEffects",
                row,
                "AdditionalGrantedEffects",
                "GrantedEffects",
            )?,
        });
    }
    let mut result = Target {
        gems: BTreeMap::new(),
        sources: BTreeMap::new(),
        exclusions: Vec::new(),
    };
    let mut base_keys = BTreeSet::new();
    for row in 0..tables.len("SkillGems") {
        let values = tables.row("SkillGems", row)?;
        let gem_variants = refs(tables, "SkillGems", row, "GemEffects", "GemEffects")?;
        if gem_variants.len() > 1 {
            return Err(format!(
                "SkillGems/{row}: multiple GemEffects; the gem definition holds one"
            ));
        }
        let base = if values["BaseItemType"].is_null() {
            None
        } else {
            Some(
                resolve(tables, "BaseItemTypes", &values["BaseItemType"])
                    .map_err(|e| format!("SkillGems/{row}.BaseItemType: {e}"))?,
            )
        };
        let Some(base) = base else {
            result.exclusions.push(json!({"skill_gem_source_row":row,"reason":"null BaseItemType dev placeholder","gem_effects":occurrences(&gem_variants)}));
            continue;
        };
        if !base_keys.insert(base.source_row) {
            return Err(format!(
                "SkillGems/{row}: duplicate base gem identity {}",
                base.id
            ));
        }
        let name = tables.row("BaseItemTypes", base.source_row)?["Name"]
            .as_str()
            .ok_or("invalid base Name")?;
        if crate::is_placeholder(name) {
            result.exclusions.push(json!({"skill_gem_source_row":row,"base":base,"name":name,"reason":"existing is_placeholder(Name)","gem_effects":occurrences(&gem_variants)}));
            continue;
        }
        let variant = gem_variants.first().ok_or_else(|| {
            format!("SkillGems/{row}: normal gem requires one GemEffects reference")
        })?;
        let link = &variants[variant.source_row];
        let primary = link
            .primary
            .as_ref()
            .ok_or_else(|| format!("SkillGems/{row}: normal gem requires primary GrantedEffect"))?;
        let additional_sets = &sets[primary.source_row];
        result.sources.insert(
            base.id.clone(),
            json!({
                "skill_gem_source_row":row,"base":base,
                "gem_effects":[{"position":0,"target":variant}],"primary":primary,
                "additional_granted_effects":occurrences(&link.additional),
                "additional_stat_sets":occurrences(additional_sets)
            }),
        );
        result.gems.insert(
            base.id.clone(),
            GemEffectDef {
                gem_id: base.id,
                variant_id: variant.id.clone(),
                granted_effect_id: primary.id.clone(),
                additional_granted_effect_ids: link
                    .additional
                    .iter()
                    .map(|r| r.id.clone())
                    .collect(),
                additional_stat_set_ids: additional_sets.iter().map(|r| r.id.clone()).collect(),
                granted_effect_display_order: Vec::new(),
            },
        );
    }
    Ok(result)
}

fn resolve(tables: &Tables<'_>, table: &str, value: &Value) -> Result<Reference, String> {
    let source_row = value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("physical reference must be usize (not null, negative or fractional)")?;
    let id = tables.row(table, source_row)?["Id"]
        .as_str()
        .ok_or("invalid stable Id")?;
    Ok(Reference {
        source_row,
        id: id.into(),
    })
}

fn refs(
    tables: &Tables<'_>,
    owner: &str,
    row: usize,
    field: &str,
    target: &str,
) -> Result<Vec<Reference>, String> {
    tables.row(owner, row)?[field]
        .as_array()
        .ok_or_else(|| format!("{owner}/{row}.{field}: array required"))?
        .iter()
        .enumerate()
        .map(|(position, v)| {
            resolve(tables, target, v)
                .map_err(|e| format!("{owner}/{row}.{field}[{position}]: {e}"))
        })
        .collect()
}

fn occurrences(references: &[Reference]) -> Vec<Value> {
    references
        .iter()
        .enumerate()
        .map(|(position, target)| json!({"position":position,"target":target}))
        .collect()
}

fn used(table: &str) -> &'static [&'static str] {
    match table {
        "BaseItemTypes" => &["Id", "Name"],
        "SkillGems" => &["BaseItemType", "GemEffects"],
        "GemEffects" => &["Id", "GrantedEffect", "AdditionalGrantedEffects"],
        "GrantedEffects" => &["Id", "AdditionalStatSets"],
        _ => &["Id"],
    }
}

fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for table in TABLES {
        for &field in used(table) {
            let (kind, array, target) = match field {
                "BaseItemType" => ("foreignrow", false, Some("BaseItemTypes")),
                "GemEffects" => ("foreignrow", true, Some("GemEffects")),
                "GrantedEffect" => ("foreignrow", false, Some("GrantedEffects")),
                "AdditionalGrantedEffects" => ("foreignrow", true, Some("GrantedEffects")),
                "AdditionalStatSets" => ("foreignrow", true, Some("GrantedEffectStatSets")),
                _ => ("string", false, None),
            };
            super::convert::column(
                input,
                table,
                &[field],
                &json!({
                    "name":field,"type":kind,"array":array,"interval":false,
                    "references":target.map(|table|json!({"table":table}))
                }),
            )?;
        }
    }
    Ok(())
}

fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for table in TABLES {
        let doc = &input.documents[&format!("tables/{table}.json")];
        let columns: Vec<_> = doc["columns"]
            .as_array()
            .ok_or("missing columns")?
            .iter()
            .filter(|c| {
                c["physical_fields"].as_array().is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|f| !used(table).iter().any(|name| f.as_str() == Some(*name)))
                })
            })
            .collect();
        result.push(json!({
            "table":table,"payload":format!("tables/{table}.json"),"columns":columns,
            "source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "identity":"Id in each pinned physical row; SkillGems identifies its BaseItemType; source_records/exclusions locate consumed output links",
            "status":"remaining fields not consumed by this link domain; no inference that unapplied mechanics are effectless"
        }));
    }
    Ok(result)
}
