//! Target-owned support gating on an explicitly curated granted-effect/minion sidecar.

mod policy;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::actors::GrantedEffectMinionsDef;
use serde_json::{Value, json};

use super::stat_set_codec::read_pinned;
use super::totem_varieties::RowRanges;
use crate::native_facts::reader::{self, Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 2] = ["GrantedEffects", "MonsterVarieties"];
const FILE: &str = "granted_effect_minions.json";
const SCHEMA: &str = "granted_effect_minions/v1";
const OWNED: &str = "ignore_minion_types";

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub source_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub protected_parents: [PathBuf; 2],
    pub receipt: Value,
}

pub(super) fn convert(input: &VerifiedFacts, args: &super::CodecArgs) -> Result<Converted, String> {
    let pinned_policy = read_pinned(&args.policy, &args.policy_sha256, "minion-links policy")?;
    let policy: policy::Policy = serde_json::from_value(pinned_policy.document)
        .map_err(|e| format!("minion-links policy: {e}"))?;
    policy.validate(input)?;
    if policy.source.sha256 != args.source_sha256 {
        return Err("minion-links source pin differs from policy identity".into());
    }
    let legacy = read_pinned(&args.source, &args.source_sha256, "minion-links source")?;
    if legacy.document["_meta"] != policy.source.metadata {
        return Err("minion-links source metadata differs from policy identity".into());
    }
    let dto: GrantedEffectMinionsDef = serde_json::from_value(legacy.document.clone())
        .map_err(|e| format!("minion-links source: {e}"))?;
    let tables = Tables::selected(input, &TABLES)?;
    for (table, field, kind) in [
        ("GrantedEffects", "Id", "string"),
        ("GrantedEffects", "IgnoreMinionTypes", "bool"),
        ("MonsterVarieties", "Id", "string"),
    ] {
        super::convert::column(
            input,
            table,
            &[field],
            &json!({
                "name":field,"type":kind,"array":false,"interval":false,"references":null
            }),
        )?;
    }
    let effects = identities(&tables, "GrantedEffects")?;
    let varieties = identities(&tables, "MonsterVarieties")?;
    // All base effects are consumed for the outside-sidecar scope, not just curated rows.
    let flags: Vec<bool> = (0..tables.len("GrantedEffects"))
        .map(|row| {
            tables.row("GrantedEffects", row)?["IgnoreMinionTypes"]
                .as_bool()
                .ok_or_else(|| format!("GrantedEffects/{row}.IgnoreMinionTypes: bool required"))
        })
        .collect::<Result<_, String>>()?;
    let mut aliases = BTreeMap::new();
    let mut alias_usage = vec![0_usize; policy.alias_catalog.aliases.len()];
    for (policy_row, alias) in policy.alias_catalog.aliases.iter().enumerate() {
        let &variety_row = varieties.get(&alias.monster_variety_id).ok_or_else(|| {
            format!(
                "policy alias {}: missing target variety {}",
                alias.id, alias.monster_variety_id
            )
        })?;
        aliases.insert(alias.id.as_str(), (policy_row, variety_row));
    }
    let mut document = legacy.document.clone();
    let output_entries = document["entries"]
        .as_array_mut()
        .ok_or("entries must be an array")?;
    let mut ids = BTreeSet::new();
    let mut records = Vec::new();
    let mut reference_count = 0;
    let mut changed = Vec::new();
    for (source_row, (entry, output)) in dto.entries.iter().zip(output_entries).enumerate() {
        if entry.effect_id.is_empty() || !ids.insert(entry.effect_id.as_str()) {
            return Err("empty/duplicate minion-links effect_id".into());
        }
        let &effect_row = effects
            .get(&entry.effect_id)
            .ok_or_else(|| format!("missing target effect {}", entry.effect_id))?;
        let raw_flag = flags[effect_row];
        let raw_entry = &legacy.document["entries"][source_row];
        let mut occurrences = Vec::new();
        for (field, references) in [
            ("minion_list", &entry.minion_list),
            ("add_minion_list", &entry.add_minion_list),
        ] {
            for (position, id) in references.iter().enumerate() {
                let &(policy_row, variety_row) = aliases.get(id.as_str()).ok_or_else(|| {
                    format!(
                        "{}.{field}[{position}]: missing policy alias {id}",
                        entry.effect_id
                    )
                })?;
                alias_usage[policy_row] += 1;
                occurrences.push(json!({"field":field,"position":position,"alias_id":id,"policy_row":policy_row,
                    "source_catalog_row":policy.alias_catalog.aliases[policy_row].source_catalog_row,
                    "variety":{"source_row":variety_row,"id":policy.alias_catalog.aliases[policy_row].monster_variety_id}}));
            }
        }
        reference_count += occurrences.len();
        let object = output
            .as_object_mut()
            .ok_or("minion-links entry must be an object")?;
        object.remove(OWNED);
        if raw_flag {
            object.insert(OWNED.into(), json!(true));
        }
        let retained: Vec<_> = raw_entry.as_object().ok_or("source entry must be an object")?.iter()
            .filter(|(field, _)| field.as_str() != OWNED)
            .map(|(field, value)| Ok(json!({"field":field,"source_pointer":format!("/entries/{source_row}/{}",pointer_token(field)),
                "value_sha256":sha256_hex(&reader::json_bytes(value)?)})))
            .collect::<Result<_, String>>()?;
        if raw_flag != entry.ignore_minion_types {
            changed.push(json!({"source_row":source_row,"effect_id":entry.effect_id,
                "legacy_logical_value":entry.ignore_minion_types,"target":raw_flag}));
        }
        records.push(json!({"source_row":source_row,"source_pointer":format!("/entries/{source_row}"),"effect_id":entry.effect_id,
            "effect_source_row":effect_row,"source_entry_sha256":sha256_hex(&reader::json_bytes(raw_entry)?),
            "owned_field":{"field":OWNED,"table":"GrantedEffects","column":"IgnoreMinionTypes","raw":raw_flag,
                "present":raw_flag,"value":if raw_flag{Some(true)}else{None},
                "rule":"target true emitted; target false omitted (existing DTO default false), never legacy fallback",
                "legacy_present":raw_entry.get(OWNED).is_some(),"legacy_value":raw_entry.get(OWNED)},
            "reference_occurrences":occurrences,"retained_fields":retained,
            "output_entry_sha256":sha256_hex(&reader::json_bytes(output)?)}));
    }
    let alias_inventory: Vec<_> = policy.alias_catalog.aliases.iter().enumerate().map(|(policy_row, alias)| {
        json!({"policy_row":policy_row,"alias":alias,"variety_source_row":aliases[alias.id.as_str()].1,
            "reference_occurrences":alias_usage[policy_row],"unused":alias_usage[policy_row]==0})
    }).collect();
    let mut outside_rows = RowRanges::default();
    let mut outside_true = Vec::new();
    for (source_row, &value) in flags.iter().enumerate() {
        let id = tables.row("GrantedEffects", source_row)?["Id"]
            .as_str()
            .ok_or("invalid effect Id")?;
        if !ids.contains(id) {
            outside_rows.push(source_row);
            if value {
                outside_true.push(json!({"source_row":source_row,"id":id,"raw":true}));
            }
        }
    }
    let legacy_identity = json!({"file":"legacy-minion-links.json","bytes":legacy.bytes.len(),"sha256":sha256_hex(&legacy.bytes),
        "snapshot_version":policy.source.snapshot_version,"reference_path":policy.source.path,"metadata":policy.source.metadata});
    let policy_identity = json!({"file":"codec-policy.json","bytes":pinned_policy.bytes.len(),"sha256":sha256_hex(&pinned_policy.bytes)});
    let authoring = json!({"declaration":policy.alias_catalog.source_declaration,"file_read_by_producer":false,
        "file_hash_verified_by_producer":false,"scope":"policy-authoring declaration only; policy bytes and target alias identities are verified, the unprovided catalog is not"});
    let target = json!({"facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"source_identity":input.manifest["source_identity"],
        "owned_fields":[OWNED],"source_column":"GrantedEffects.IgnoreMinionTypes","rule":"emit true, omit false"});
    let compatibility = json!({"legacy_source":legacy_identity,"policy":policy_identity,"alias_authoring":authoring,
        "scope":"all entry membership/relationships/other fields and policy aliases remain fixed community curation; no DAT-derived summoning or equipment claim"});
    let metadata = document["_meta"]
        .as_object_mut()
        .ok_or("legacy metadata must be an object")?;
    for (key, value) in [
        ("target_fields", target.clone()),
        ("fixed_compatibility", compatibility.clone()),
        (
            "ordinary_producer",
            json!({"name":"pobr-data-adapter/ordinary-facts","domain":"granted-effect-minions",
            "status":"domain-candidate/not-adopted","original_metadata_scope":"original generator/vendor/regen fields describe archived relationships only; target boolean provenance is separate"}),
        ),
    ] {
        if metadata.insert(key.into(), value).is_some() {
            return Err(format!(
                "legacy metadata conflicts with generated provenance key {key}"
            ));
        }
    }
    let _: GrantedEffectMinionsDef = serde_json::from_value(document.clone())
        .map_err(|e| format!("converted minion-links DTO: {e}"))?;
    let receipt = json!({"entries":records.len(),"source_records":records,"reference_occurrences":reference_count,
        "alias_inventory":alias_inventory,"alias_authoring":authoring,"legacy_source":legacy_identity,"codec_policy":policy_identity,
        "target_fields":target,"fixed_compatibility":compatibility,"changed_target_booleans":changed,
        "outside_sidecar":{"rows":outside_rows,"true_effects":outside_true,
            "scope":"ordinary-skills supplies all base flags; no overlay entries generated for these effects, so existing base values remain authoritative"},
        "row_range_convention":"zero-based half-open [start,end_exclusive), in physical source order",
        "historical_source_evidence":policy.evidence,"compatibility_statement":policy.statement,
        "unconsumed_source":unmapped(input, &tables)?,
        "pending":"fixed community relationships/alias curation are not newly acquired summon/AI/equipment rules; complete target package, ordinary BD/Web, source approval and adoption remain separate"});
    Ok(Converted {
        overlay: BTreeMap::from([(FILE.into(), super::pretty_bytes(&document)?)]),
        source_bytes: legacy.bytes,
        policy_bytes: pinned_policy.bytes,
        protected_parents: [legacy.parent, pinned_policy.parent],
        receipt,
    })
}

fn identities(tables: &Tables<'_>, table: &str) -> Result<BTreeMap<String, usize>, String> {
    // Tables checks nonempty unique identities. Preserve exact source strings, not policy tokens.
    (0..tables.len(table))
        .map(|row| {
            let id = tables.row(table, row)?["Id"]
                .as_str()
                .ok_or("invalid source Id")?;
            Ok((id.into(), row))
        })
        .collect()
}

fn pointer_token(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    TABLES.iter().map(|table| {
        let used = if *table=="GrantedEffects" {vec!["Id","IgnoreMinionTypes"]} else {vec!["Id"]};
        let path = format!("tables/{table}.json");
        let columns:Vec<_> = input.documents[&path]["columns"].as_array().ok_or("missing columns")?.iter()
            .filter(|c| c["physical_fields"].as_array().is_some_and(|fields| fields.iter().any(|f| !used.iter().any(|u| f.as_str()==Some(*u))))).collect();
        Ok(json!({"table":table,"payload":path,"consumed_columns":used,"columns":columns,
            "source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "scope":"all identities and all effect booleans validated; every other column (including MinionType) remains unconsumed, not a substitute for authored relationships"}))
    }).collect()
}
