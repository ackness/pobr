//! Fixed community skill interpretation bytes, with separate target identity evidence.

pub(super) mod policy;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::StatSetLabelsDef;
use pobr_data::catalog::stat_map::{SkillStatMapDef, StatMapEntry};
use serde_json::{Value, json};

use super::stat_set_codec::{self, read_pinned};
use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 3] = ["Stats", "GrantedEffects", "GrantedEffectStatSets"];

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
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
    let pinned_policy = read_pinned(&args.policy, &args.policy_sha256, "skill-stat-map policy")?;
    let policy: policy::Policy = serde_json::from_value(pinned_policy.document)
        .map_err(|e| format!("skill-stat-map policy: {e}"))?;
    policy.validate(input)?;
    if args.source_sha256 != policy.source.sha256 || branch.sha256 != policy.branch_codec.sha256 {
        return Err("map/branch codec pin differs from policy source identity".into());
    }
    let source = read_pinned(&args.source, &args.source_sha256, "skill-stat-map source")?;
    let codec = read_pinned(&branch.path, &branch.sha256, "branch codec")?;
    for (document, expected, name) in [
        (&source.document, &policy.source, "map"),
        (&codec.document, &policy.branch_codec, "branch codec"),
    ] {
        if document["_meta"] != expected.metadata {
            return Err(format!(
                "{name} metadata differs from policy legacy identity"
            ));
        }
    }
    // Deserialize for original consumer shape compatibility only. Output always uses
    // the untouched bytes, including fields serde does not understand.
    let map: SkillStatMapDef = serde_json::from_value(source.document.clone())
        .map_err(|e| format!("skill-stat-map source: {e}"))?;
    let labels: StatSetLabelsDef =
        serde_json::from_value(codec.document.clone()).map_err(|e| format!("branch codec: {e}"))?;
    let mut index_keys = BTreeMap::new();
    let mut set_keys = BTreeSet::new();
    for (source_row, row) in labels.labels.iter().enumerate() {
        policy::identity(&row.skill, "codec skill Id")?;
        policy::identity(&row.set_id, "codec set Id")?;
        if row.set_index == 0
            || index_keys
                .insert((row.skill.as_str(), row.set_index), (source_row, row))
                .is_some()
            || !set_keys.insert((&row.skill, &row.set_id))
        {
            return Err(format!(
                "branch codec row {source_row}: invalid/duplicate (skill,index) or (skill,set_id)"
            ));
        }
    }
    let tables = Tables::selected(input, &TABLES)?;
    stat_set_codec::check_columns(input)?;
    super::convert::column(
        input,
        "Stats",
        &["Id"],
        &json!({"name":"Id","type":"string","array":false,"interval":false,"references":null}),
    )?;
    let stats = identities(&tables, "Stats")?;
    let effects = identities(&tables, "GrantedEffects")?;
    let _sets = identities(&tables, "GrantedEffectStatSets")?;
    let mut absent = BTreeSet::new();
    let global = stat_records(
        &map.global,
        &source.document["global"],
        "/global",
        &stats,
        &mut absent,
    )?;
    let mut scopes = Vec::new();
    let mut unbound: BTreeMap<_, _> = policy
        .unbound_scopes
        .iter()
        .map(|e| ((e.skill.as_str(), e.set_index), e))
        .collect();
    let mut scoped_entries = 0;
    let mut bound_count = 0;
    for (skill, sets) in &map.per_stat_set {
        policy::identity(skill, "map scope skill Id")?;
        for (index, entries) in sets {
            let set_index = policy::index(index)?;
            let key = (skill.as_str(), set_index);
            let records = stat_records(
                entries,
                &source.document["per_stat_set"][skill][index],
                &format!("/per_stat_set/{}/{}", pointer_token(skill), index),
                &stats,
                &mut absent,
            )?;
            scoped_entries += records.len();
            let binding = match (index_keys.get(&key), effects.get(skill)) {
                (Some((codec_row, label)), Some(&effect_row)) => {
                    if unbound.contains_key(&key) {
                        return Err(format!(
                            "stale unbound scope exception: {skill}/{index} now matches"
                        ));
                    }
                    let owner = stat_set_codec::read_owner(&tables, effect_row)?;
                    if !owner.contains(&label.set_id) {
                        return Err(format!(
                            "scope {skill}/{index}: codec set {} is not target raw-owned",
                            label.set_id
                        ));
                    }
                    bound_count += 1;
                    json!({"status":"raw-owned","codec_source_row":codec_row,"codec_record":codec.document["labels"][*codec_row],
                        "target_owner":owner,"target_effect_id":skill})
                }
                (None, None) => {
                    let exception = unbound
                        .remove(&key)
                        .ok_or_else(|| format!("unlisted unbound scope: {skill}/{index}"))?;
                    json!({"status":"policy-unbound","codec_present":false,"target_effect_present":false,
                        "target_owner":null,"codec_record":null,"policy_exception":exception})
                }
                (codec_row, effect_row) => {
                    return Err(format!(
                        "scope {skill}/{index}: incomplete codec/target binding (codec_present={}, target_effect_present={})",
                        codec_row.is_some(),
                        effect_row.is_some()
                    ));
                }
            };
            scopes.push(json!({"skill":skill,"set_index":set_index,"source_index_key":index,"binding":binding,"stats":records}));
        }
    }
    if !unbound.is_empty() {
        return Err("unused/stale unbound scope exception".into());
    }
    let expected_absent: BTreeSet<_> = policy.absent_target_stat_ids.iter().cloned().collect();
    if absent != expected_absent {
        return Err(format!(
            "absent stat exceptions mismatch: unlisted {:?}; stale {:?}",
            absent.difference(&expected_absent).collect::<Vec<_>>(),
            expected_absent.difference(&absent).collect::<Vec<_>>()
        ));
    }
    let unextractable: Vec<_> = global
        .iter()
        .chain(
            scopes
                .iter()
                .flat_map(|s| s["stats"].as_array().into_iter().flatten()),
        )
        .filter(|r| r["unextractable"] == true)
        .cloned()
        .collect();
    let receipt = json!({
        "global_entries":global.len(),"scoped_entries":scoped_entries,"scope_count":scopes.len(),
        "raw_owned_scopes":bound_count,"unbound_scopes":scopes.len()-bound_count,
        "global_records":global,"scope_records":scopes,"absent_target_stat_ids":absent,
        "unextractable_records":unextractable,
        "legacy_source":{"file":"overlay/skill_stat_map.json","bytes":source.bytes.len(),"sha256":sha256_hex(&source.bytes),
            "snapshot_version":policy.source.snapshot_version,"reference_path":policy.source.path,"metadata":policy.source.metadata,
            "status":"fixed community interpretation; not target game-number acquisition or gameplay-law certification"},
        "branch_codec":{"file":"branch-codec.json","bytes":codec.bytes.len(),"sha256":sha256_hex(&codec.bytes),
            "snapshot_version":policy.branch_codec.snapshot_version,"reference_path":policy.branch_codec.path,"metadata":policy.branch_codec.metadata,
            "records":labels.labels.len(),"scope":"all codec keys validated; only map-used scope ownership certified; remaining archival records preserved without new membership claims"},
        "codec_policy":{"file":"codec-policy.json","bytes":pinned_policy.bytes.len(),"sha256":sha256_hex(&pinned_policy.bytes)},
        "target_identity":{"source_identity":input.manifest["source_identity"],"facts_manifest_sha256":sha256_hex(&input.manifest_bytes),
            "scope":"stat presence/absence and stable branch raw ownership only; no mechanic correctness or full runtime applicability"},
        "reference_occurrences":"target_owner.primary and additional are physical refs resolved to stable set IDs; additional array positions are zero-based ordered occurrences, including duplicates; export index comes only from codec_record",
        "historical_source_evidence":policy.evidence,"compatibility_statement":policy.statement,
        "unconsumed_source":unmapped(input,&tables)?,
        "preservation":"All map and branch codec bytes/meta/order/unknown/unsupported contents copied verbatim; no constructor evaluation, index inference, fallback, normalization, filtering, retargeting or diagnostic clearing",
        "pending":"source remains draft; target rule correctness, minion/tree/config/other prepared assets, complete package and ordinary BD/Web/accuracy/adoption remain separate"
    });
    Ok(Converted {
        overlay: BTreeMap::from([("skill_stat_map.json".into(), source.bytes)]),
        policy_bytes: pinned_policy.bytes,
        branch_bytes: codec.bytes,
        protected_parents: [source.parent, codec.parent, pinned_policy.parent],
        receipt,
    })
}

fn identities(tables: &Tables<'_>, table: &str) -> Result<BTreeMap<String, usize>, String> {
    (0..tables.len(table))
        .map(|row| {
            let id = tables.row(table, row)?["Id"]
                .as_str()
                .ok_or("invalid source Id")?;
            policy::identity(id, &format!("{table}/{row}.Id"))?;
            Ok((id.into(), row))
        })
        .collect()
}

fn stat_records(
    entries: &BTreeMap<String, StatMapEntry>,
    raw: &Value,
    path: &str,
    stats: &BTreeMap<String, usize>,
    absent: &mut BTreeSet<String>,
) -> Result<Vec<Value>, String> {
    entries.iter().map(|(id,entry)|{
        policy::identity(id,"map stat Id")?;
        let row=stats.get(id);
        if row.is_none(){absent.insert(id.clone());}
        Ok(json!({"stat_id":id,"source_pointer":format!("{path}/{}",pointer_token(id)),
            "target_stat":row.map(|row|json!({"table":"Stats","source_row":row,"id":id})),
            "status":if row.is_some(){"present"}else{"policy-absent; retained unchanged, not effectless"},
            "entry_sha256":sha256_hex(&super::reader::json_bytes(&raw[id])?),
            "unextractable":entry.unextractable,
            "unextractable_entry":if entry.unextractable {Some(&raw[id])}else{None}}))
    }).collect()
}

fn pointer_token(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    TABLES.iter().map(|table|{
        let used:&[&str]=if *table=="GrantedEffects"{&["Id","StatSet","AdditionalStatSets"]}else{&["Id"]};
        let path=format!("tables/{table}.json");
        let columns:Vec<_>=input.documents[&path]["columns"].as_array().ok_or("missing columns")?.iter()
            .filter(|c|c["physical_fields"].as_array().is_some_and(|fields|fields.iter().any(|f|!used.iter().any(|name|f.as_str()==Some(*name))))).collect();
        Ok(json!({"table":table,"payload":path,"columns":columns,"source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "identity":"Id in pinned physical rows; global_records/scope_records give map-used stat/owner references",
            "status":"remaining columns and unused owner references not certified/applied; not effectless"}))
    }).collect()
}
