//! Target primary multipliers plus explicitly retained community interpretation entries.

mod policy;
mod source;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::StatSetLabelsDef;
use pobr_data::catalog::skill_overrides::{SkillOverrideEntry, SkillOverridesDef};
use serde_json::{Value, json};

use super::skill_stat_map::policy::identity;
use super::stat_set_codec::{self, read_pinned};
use crate::native_facts::reader::{self, Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 5] = [
    "Stats",
    "GrantedEffects",
    "GrantedEffectStatSets",
    "GrantedEffectStatSetsPerLevel",
    "GrantedEffectsPerLevel",
];

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub source_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub branch_bytes: Vec<u8>,
    pub protected_parents: [PathBuf; 3],
    pub receipt: Value,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    args: &super::CodecArgs,
    branch: &super::BranchCodecArgs,
) -> Result<Converted, String> {
    let pinned_policy = read_pinned(&args.policy, &args.policy_sha256, "skill-overrides policy")?;
    let policy: policy::Policy = serde_json::from_value(pinned_policy.document)
        .map_err(|e| format!("skill-overrides policy: {e}"))?;
    policy.validate(input)?;
    if args.source_sha256 != policy.source.sha256 || branch.sha256 != policy.branch_codec.sha256 {
        return Err("overrides/branch codec pin differs from policy source identity".into());
    }
    let legacy = read_pinned(&args.source, &args.source_sha256, "skill-overrides source")?;
    let codec = read_pinned(&branch.path, &branch.sha256, "branch codec")?;
    for (document, expected, name) in [
        (&legacy.document, &policy.source, "overrides"),
        (&codec.document, &policy.branch_codec, "branch codec"),
    ] {
        if document["_meta"] != expected.metadata {
            return Err(format!(
                "{name} metadata differs from policy legacy identity"
            ));
        }
    }
    // The DTO checks consumer shape; retain raw Values so unknown authored fields survive.
    let dto: SkillOverridesDef = serde_json::from_value(legacy.document.clone())
        .map_err(|e| format!("skill-overrides source: {e}"))?;
    let labels: StatSetLabelsDef =
        serde_json::from_value(codec.document.clone()).map_err(|e| format!("branch codec: {e}"))?;
    let mut index_keys = BTreeMap::new();
    let mut set_keys = BTreeSet::new();
    for (source_row, label) in labels.labels.iter().enumerate() {
        identity(&label.skill, "codec skill Id")?;
        identity(&label.set_id, "codec set Id")?;
        if label.set_index == 0
            || index_keys
                .insert((label.skill.as_str(), label.set_index), (source_row, label))
                .is_some()
            || !set_keys.insert((&label.skill, &label.set_id))
        {
            return Err(format!(
                "branch codec row {source_row}: invalid/duplicate (skill,index) or (skill,set_id)"
            ));
        }
    }
    let tables = Tables::selected(input, &TABLES)?;
    let facts = source::read(input, &tables)?;
    let mut numeric_exceptions: BTreeMap<_, _> = policy
        .legacy_numeric_exceptions
        .iter()
        .map(|e| (e.source_row, e))
        .collect();
    let mut unbound: BTreeMap<_, _> = policy
        .unbound_retained_entries
        .iter()
        .map(|e| (e.source_row, e))
        .collect();
    let mut output_document = legacy.document.clone();
    let output_entries = output_document["overrides"]
        .as_array_mut()
        .ok_or("overrides must be an array")?;
    let mut keys = BTreeSet::new();
    let mut records = Vec::new();
    let mut target_numeric = 0;
    let mut retained = 0;
    let mut virtual_numeric = 0;
    for (source_row, (entry, output)) in dto.overrides.iter().zip(output_entries).enumerate() {
        let raw = &legacy.document["overrides"][source_row];
        let key = policy::Key::from(entry);
        key.validate()?;
        validate_shape(entry, raw)?;
        if !keys.insert(key.clone()) {
            return Err(format!(
                "duplicate override identity at source row {source_row}"
            ));
        }
        let effect_row = facts.effects.get(&entry.skill).copied();
        let binding = if entry.stat == "base_multiplier" {
            if let Some(effect_row) = effect_row {
                if numeric_exceptions.contains_key(&source_row) {
                    return Err("stale legacy numeric exception: target effect now exists".into());
                }
                let owner = stat_set_codec::read_owner(&tables, effect_row)?;
                let numbers = rebuild(
                    entry,
                    output,
                    effect_row,
                    owner.primary_source_row(),
                    &facts,
                )?;
                target_numeric += 1;
                json!({"status":"target-primary-numeric","target_owner":owner,"numbers":numbers})
            } else {
                let exception = numeric_exceptions
                    .remove(&source_row)
                    .filter(|e| e.key == key)
                    .ok_or_else(|| {
                        format!(
                            "unlisted legacy numeric exception at {source_row}: {}",
                            entry.skill
                        )
                    })?;
                virtual_numeric += 1;
                json!({"status":"legacy-only-numeric","target_owner":null,"policy_exception":exception,
                    "scope":"virtual community entry retained unchanged; not target numeric acquisition"})
            }
        } else {
            retained += 1;
            let index = entry.stat_set.ok_or("retained entry needs stat_set")?;
            let label = index_keys.get(&(entry.skill.as_str(), index));
            let owner = effect_row
                .map(|row| stat_set_codec::read_owner(&tables, row))
                .transpose()?;
            let branch_binding = if let (Some((codec_row, label)), Some(owner)) = (label, &owner) {
                if unbound.contains_key(&source_row) {
                    return Err("stale unbound retained exception: branch now matches".into());
                }
                if !owner.contains(&label.set_id) {
                    return Err(format!(
                        "retained entry {source_row}: codec set {} is not target raw-owned",
                        label.set_id
                    ));
                }
                json!({"status":"raw-owned","codec_source_row":codec_row,"codec_record":codec.document["labels"][*codec_row],"target_owner":owner})
            } else {
                let exception = unbound
                    .remove(&source_row)
                    .filter(|e| {
                        e.key == key
                            && e.codec_present == label.is_some()
                            && e.target_effect_present == effect_row.is_some()
                    })
                    .ok_or_else(|| {
                        format!(
                            "unlisted/changed unbound retained entry {source_row}: {}/{}",
                            entry.skill, index
                        )
                    })?;
                json!({"status":"policy-unbound","codec_record":null,"target_owner":owner,"policy_exception":exception,
                    "scope":"retained unchanged without target branch claim; no producer primary fallback or retargeting"})
            };
            let stat = entry
                .stat_id
                .as_ref()
                .map(|id| {
                    let row = facts.stats.get(id).ok_or_else(|| {
                        format!("implicit stat Id {id} absent from target Stats; policy conflict")
                    })?;
                    Ok::<_, String>(json!({"table":"Stats","source_row":row,"id":id}))
                })
                .transpose()?;
            json!({"status":"retained-community-interpretation","branch":branch_binding,"implicit_stat":stat,
                "scope":"authored raw JSON preserved; target identity checks are not mechanic correctness certification"})
        };
        records.push(json!({"source_row":source_row,"source_pointer":format!("/overrides/{source_row}"),"key":key,
            "source_entry_sha256":sha256_hex(&reader::json_bytes(raw)?),
            "output_entry_sha256":sha256_hex(&reader::json_bytes(output)?),
            "entry_value_unchanged":raw.eq(output),"binding":binding}));
    }
    if !numeric_exceptions.is_empty() || !unbound.is_empty() {
        return Err("unused/stale numeric or unbound retained exception".into());
    }
    let legacy_identity = json!({"file":"legacy-skill-overrides.json","bytes":legacy.bytes.len(),"sha256":sha256_hex(&legacy.bytes),
        "snapshot_version":policy.source.snapshot_version,"reference_path":policy.source.path,"metadata":policy.source.metadata});
    let codec_identity = json!({"file":"branch-codec.json","bytes":codec.bytes.len(),"sha256":sha256_hex(&codec.bytes),
        "snapshot_version":policy.branch_codec.snapshot_version,"reference_path":policy.branch_codec.path,"metadata":policy.branch_codec.metadata,
        "records":labels.labels.len(),"scope":"all codec identity keys checked; only retained entry-used branch ownership certified"});
    let policy_identity = json!({"file":"codec-policy.json","bytes":pinned_policy.bytes.len(),"sha256":sha256_hex(&pinned_policy.bytes)});
    let target_identity = json!({"facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"source_identity":input.manifest["source_identity"],
        "formula":"1.0 + raw as f64 / 10000.0","numeric_source":"GrantedEffects primary StatSet -> GrantedEffectStatSetsPerLevel.BaseMultiplier at each actual positive GemLevel",
        "scope":"only raw-owned base_multiplier numbers are target-derived; acquisition status remains unchanged"});
    output_document["_meta"] = json!({"schema":"skill_overrides/v1","generator":"pobr-data-adapter/ordinary-facts",
        "target_numeric":target_identity,"legacy_compatibility":{"source":legacy_identity,"branch_codec":codec_identity,"policy":policy_identity},
        "status":"domain-candidate; mixed target numeric facts and fixed community interpretation; not complete runtime or adopted"});
    // Recheck the emitted shape, without losing raw unknown fields through DTO serialization.
    let _: SkillOverridesDef = serde_json::from_value(output_document.clone())
        .map_err(|e| format!("converted skill-overrides: {e}"))?;
    let receipt = json!({"entries":records.len(),"target_numeric_entries":target_numeric,"legacy_only_numeric_entries":virtual_numeric,
        "retained_interpretation_entries":retained,"entry_records":records,
        "legacy_source":legacy_identity,"branch_codec":codec_identity,"codec_policy":policy_identity,"target_numeric":target_identity,
        "historical_source_evidence":policy.evidence,"compatibility_statement":policy.statement,
        "unconsumed_source":source::unmapped(input,&tables)?,
        "reference_occurrences":"owner additional refs are ordered physical occurrences, including duplicates; export indices come only from pinned codec records",
        "numeric_comparison":"legacy values are comparison/template shape only; f64 equality is reported separately and never gates target output; historical Decimal comparison is not serialized/f64 equality",
        "preservation":"authored order/identity/unknown fields retained; only target-owned numeric representation and top metadata replaced; original source/codec/policy bytes archived verbatim",
        "pending":"remaining prepared assets, target law correctness, full package, ordinary BD/Web/accuracy and adoption remain separate; no new actor scaling, interpolation, primary fallback, source approval or effectless claims"});
    Ok(Converted {
        overlay: BTreeMap::from([(
            "skill_overrides.json".into(),
            super::pretty_bytes(&output_document)?,
        )]),
        source_bytes: legacy.bytes,
        policy_bytes: pinned_policy.bytes,
        branch_bytes: codec.bytes,
        protected_parents: [legacy.parent, codec.parent, pinned_policy.parent],
        receipt,
    })
}

fn validate_shape(entry: &SkillOverrideEntry, raw: &Value) -> Result<(), String> {
    // Explicit null is not a usable value, index, ID or sequence. Unknown fields may be null.
    for field in ["value", "per_level", "stat_set", "stat_id"] {
        if raw.get(field).is_some_and(Value::is_null) {
            return Err(format!("override {field} cannot be null"));
        }
    }
    match entry.stat.as_str() {
        "implicit_stat" if entry.value.is_none() && entry.per_level.is_none() => Ok(()),
        "base_multiplier" => match (&entry.value, &entry.per_level) {
            (Some(value), None) if value.is_finite() => Ok(()),
            (None, Some(points)) if !points.is_empty() => {
                let mut levels = BTreeSet::new();
                for (level, value) in points {
                    if *level == 0 || !value.is_finite() || !levels.insert(*level) {
                        return Err(
                            "invalid/duplicate override per_level key or nonfinite value".into(),
                        );
                    }
                }
                Ok(())
            }
            _ => Err(
                "base_multiplier requires exactly one finite value or nonempty per_level".into(),
            ),
        },
        "explode_corpse" | "skill_attack_speed_more" | "dot_is_area"
            if entry.value.is_some_and(f64::is_finite) && entry.per_level.is_none() =>
        {
            Ok(())
        }
        _ => Err("invalid retained override value/per_level shape".into()),
    }
}

fn rebuild(
    entry: &SkillOverrideEntry,
    output: &mut Value,
    effect_row: usize,
    set_row: usize,
    facts: &source::Facts,
) -> Result<Value, String> {
    let source_levels = facts
        .set_levels
        .get(&set_row)
        .ok_or_else(|| format!("{}: missing primary stat-set level rows", entry.skill))?;
    let positive: Vec<_> = source_levels
        .iter()
        .filter(|(level, _)| **level > 0)
        .collect();
    let first = positive
        .first()
        .ok_or_else(|| format!("{}: no positive primary stat-set level rows", entry.skill))?;
    let constant = entry.value.is_some()
        && positive
            .iter()
            .all(|(_, row)| row.raw_multiplier == first.1.raw_multiplier);
    let multiplier = |raw: i32| 1.0 + f64::from(raw) / 10000.0;
    let points: Vec<_> = positive
        .iter()
        .map(|(level, row)| json!([level, multiplier(row.raw_multiplier)]))
        .collect();
    let object = output
        .as_object_mut()
        .ok_or("override entry must be object")?;
    object.remove("value");
    object.remove("per_level");
    if constant {
        object.insert("value".into(), json!(multiplier(first.1.raw_multiplier)));
    } else {
        object.insert("per_level".into(), json!(points));
    }
    let gpl = facts.effect_levels.get(&effect_row);
    let rows: Vec<_> = source_levels.iter().map(|(level,row)| {
        let paired = if *level > 0 {gpl.and_then(|rows|rows.get(level))}else{None};
        // This branch is evidence only and never supplies the emitted multiplier.
        let old = entry.value.or_else(||entry.per_level.as_ref().and_then(|points|points.iter().find(|(l,_)| i64::from(*l)==i64::from(*level)).map(|(_,v)|*v)));
        json!({"table":"GrantedEffectStatSetsPerLevel","source_row":row.source_row,"primary_set_source_row":set_row,
            "gem_level":level,"raw_base_multiplier":row.raw_multiplier,"computed_multiplier":multiplier(row.raw_multiplier),
            "emitted":*level>0,"gpl_pair":paired.map(|r|json!({"table":"GrantedEffectsPerLevel","source_row":r,"effect_source_row":effect_row,"level":level})),
            "pair_status":if *level<=0{"nonpositive-level-not-emitted"}else if paired.is_some(){"paired"}else{"no-effect-level-row; source retained, no synthesized runtime row"},
            "legacy_comparison":{"value":old,"f64_equal":old.map(|old|old==multiplier(row.raw_multiplier))}})
    }).collect();
    let gpl_rows: Vec<_> = gpl
        .into_iter()
        .flat_map(|rows| rows.iter())
        .map(|(level, row)| {
            json!({
        "source_row":row,"level":level,"positive":*level>0,
        "primary_set_level_source_row":source_levels.get(level).map(|r|r.source_row)})
        })
        .collect();
    Ok(
        json!({"formula":"1.0 + raw as f64 / 10000.0","primary_set_source_row":set_row,
        "legacy_shape":if entry.value.is_some(){"constant"}else{"per_level"},
        "output_shape":if constant{"constant"}else{"per_level"},"output_points":points,
        "source_level_rows":rows,"effect_level_rows":gpl_rows,
        "pairing_scope":"GPL is identity-only evidence; it never filters source points or supplies a level/value fallback; unchanged consumers skip runtime levels absent from base"}),
    )
}
