//! Preserve a pinned legacy codec verbatim, verifying target raw ownership separately.

pub(super) mod policy;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use pobr_data::catalog::StatSetLabelsDef;
use serde::Serialize;
use serde_json::{Value, json};

use crate::native_facts::reader::{self, Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 2] = ["GrantedEffects", "GrantedEffectStatSets"];

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub policy_bytes: Vec<u8>,
    pub protected_parents: [PathBuf; 2],
    pub receipt: Value,
}

pub(super) struct Pinned {
    pub bytes: Vec<u8>,
    pub document: Value,
    pub parent: PathBuf,
}

pub(super) fn read_pinned(path: &Path, expected: &str, name: &str) -> Result<Pinned, String> {
    policy::hex(expected, 64, name)?;
    let path = path.canonicalize().map_err(|e| format!("{name}: {e}"))?;
    let bytes = fs::read(&path).map_err(|e| format!("{name}: {e}"))?;
    if sha256_hex(&bytes) != expected {
        return Err(format!("{name} SHA256 mismatch (checked before parsing)"));
    }
    let document = reader::strict_json(&bytes).map_err(|e| format!("{name}: {e}"))?;
    Ok(Pinned {
        bytes,
        document,
        parent: path.parent().ok_or("codec input has no parent")?.into(),
    })
}

#[derive(Serialize)]
struct SetRef {
    source_row: usize,
    set_id: String,
}

#[derive(Serialize)]
pub(super) struct Owner {
    effect_source_row: usize,
    primary: SetRef,
    additional: Vec<SetRef>,
}

impl Owner {
    pub(super) fn primary_source_row(&self) -> usize {
        self.primary.source_row
    }

    pub(super) fn contains(&self, set_id: &str) -> bool {
        self.primary.set_id == set_id || self.additional.iter().any(|s| s.set_id == set_id)
    }
}

pub(super) fn convert(input: &VerifiedFacts, args: &super::CodecArgs) -> Result<Converted, String> {
    let policy_input = read_pinned(&args.policy, &args.policy_sha256, "codec policy")?;
    let policy: policy::Policy =
        serde_json::from_value(policy_input.document).map_err(|e| format!("codec policy: {e}"))?;
    policy.validate(input)?;
    if policy.source.sha256 != args.source_sha256 {
        return Err("codec source pin differs from policy source identity".into());
    }
    let source = read_pinned(&args.source, &args.source_sha256, "codec source")?;
    policy::validate_metadata(&source.document["_meta"])?;
    if source.document["_meta"] != policy.source.metadata {
        return Err("codec source metadata differs from policy legacy identity".into());
    }
    let codec: StatSetLabelsDef =
        serde_json::from_value(source.document).map_err(|e| format!("codec source: {e}"))?;
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    let owners = owners(&tables)?;
    let mut exceptions: BTreeMap<_, _> = policy
        .raw_ownership_exceptions
        .iter()
        .map(|e| (e.key.clone(), e))
        .collect();
    let mut index_keys = BTreeSet::new();
    let mut set_keys = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut records = Vec::new();
    let mut exception_records = Vec::new();
    let mut referenced_owners = BTreeMap::new();
    for (source_row, row) in codec.labels.iter().enumerate() {
        let key = policy::Key {
            skill: row.skill.clone(),
            set_id: row.set_id.clone(),
            set_index: row.set_index,
        };
        key.checked()?;
        if !index_keys.insert((&row.skill, row.set_index))
            || !set_keys.insert((&row.skill, &row.set_id))
        {
            return Err(format!(
                "codec row {source_row}: duplicate (skill,index) or (skill,set_id)"
            ));
        }
        let owner = owners
            .get(&row.skill)
            .ok_or_else(|| format!("codec row {source_row}: unknown target owner {}", row.skill))?;
        referenced_owners.insert(&row.skill, owner);
        let classification = if owner.contains(&row.set_id) {
            "raw-owned"
        } else {
            let exception = exceptions.remove(&key).ok_or_else(|| {
                format!(
                    "codec row {source_row}: unlisted raw ownership exception for {}/{}/{}",
                    row.skill, row.set_id, row.set_index
                )
            })?;
            exception_records.push(json!({
                "key":key, "codec_source_row":source_row, "current_owner":owner,
                "reason":exception.reason, "action":"preserve unchanged; no remapping or branch application"
            }));
            "policy-exception"
        };
        records
            .push(json!({"key":key,"codec_source_row":source_row,"classification":classification}));
        keys.insert(key);
    }
    if !exceptions.is_empty() {
        return Err(
            "unused/stale codec ownership exception (missing record or now raw-owned)".into(),
        );
    }
    // These are declared comparison results, not a runtime binding calculation.
    let evidence = &policy.evidence;
    if evidence
        .bound_before_prepared_merges_in_both
        .checked_add(evidence.unbound_before_prepared_merges_in_both.len())
        != Some(codec.labels.len())
        || evidence
            .unbound_before_prepared_merges_in_both
            .iter()
            .any(|key| !keys.contains(key))
    {
        return Err("historical codec binding evidence does not describe this snapshot".into());
    }
    let receipt = json!({
        "codec_records":codec.labels.len(), "codec_owners":referenced_owners.len(),
        "raw_owned_records":codec.labels.len()-exception_records.len(),
        "exception_records":exception_records.len(),
        "empty_labels":codec.labels.iter().filter(|r| r.label.is_empty()).count(),
        "legacy_source":{
            "file":"overlay/stat_set_labels.json", "bytes":source.bytes.len(), "sha256":sha256_hex(&source.bytes),
            "snapshot_version":policy.source.snapshot_version,"reference_path":policy.source.path,
            "metadata":policy.source.metadata,"status":"legacy compatibility input; not target game acquisition"
        },
        "codec_policy":{"file":"codec-policy.json","bytes":policy_input.bytes.len(),"sha256":sha256_hex(&policy_input.bytes)},
        "target_membership":{
            "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),
            "source_identity":input.manifest["source_identity"],
            "owners":referenced_owners, "records":records,"exceptions":exception_records,
            "scope":"raw primary/additional stable-ID ownership only; not prepared runtime or imported BD execution"
        },
        "historical_binding_evidence":evidence,
        "compatibility_statement":policy.statement,
        "unmapped_source_columns":unmapped(input, &tables)?,
        "preservation":"All codec bytes, metadata, record order, labels and positive export indices are copied unchanged; no raw-order index inference or remapping.",
        "pending":[
            "Historical pre-merge unbound records are retained; source-check evidence is not prepared skill_overrides/minion binding or full runtime execution.",
            "Existing native-label priority, stable (effect,set) binding and conservative primary fallback are unchanged; compatibility with every newer exporter is not certified.",
            "Acquisition remains draft; no target numeric values, source approval, complete runtime package or ordinary BD/Web adoption are claimed."
        ]
    });
    Ok(Converted {
        overlay: BTreeMap::from([("stat_set_labels.json".into(), source.bytes)]),
        policy_bytes: policy_input.bytes,
        protected_parents: [source.parent, policy_input.parent],
        receipt,
    })
}

fn owners(tables: &Tables<'_>) -> Result<BTreeMap<String, Owner>, String> {
    let mut result = BTreeMap::new();
    for effect_source_row in 0..tables.len("GrantedEffects") {
        let row = tables.row("GrantedEffects", effect_source_row)?;
        let owner = read_owner(tables, effect_source_row)?;
        // The shared reader validates unique stable Id and physical _row. Additional
        // references are ordered occurrences and deliberately have no uniqueness rule.
        result.insert(row["Id"].as_str().ok_or("invalid owner Id")?.into(), owner);
    }
    Ok(result)
}

/// Parse the ordered physical references of one owner; callers choose their used subset.
pub(super) fn read_owner(tables: &Tables<'_>, effect_source_row: usize) -> Result<Owner, String> {
    let row = tables.row("GrantedEffects", effect_source_row)?;
    let primary = resolve(tables, &row["StatSet"])
        .map_err(|e| format!("GrantedEffects/{effect_source_row}.StatSet: {e}"))?;
    let additional = row["AdditionalStatSets"]
        .as_array()
        .ok_or_else(|| {
            format!("GrantedEffects/{effect_source_row}.AdditionalStatSets: array required")
        })?
        .iter()
        .map(|v| resolve(tables, v))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("GrantedEffects/{effect_source_row}.AdditionalStatSets: {e}"))?;
    Ok(Owner {
        effect_source_row,
        primary,
        additional,
    })
}

fn resolve(tables: &Tables<'_>, value: &Value) -> Result<SetRef, String> {
    let source_row = value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("required stat-set physical reference must be usize")?;
    let row = tables.row("GrantedEffectStatSets", source_row)?;
    Ok(SetRef {
        source_row,
        set_id: row["Id"].as_str().ok_or("invalid set Id")?.into(),
    })
}

pub(super) fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for (table, field, kind, array, target) in [
        ("GrantedEffects", "Id", "string", false, None),
        ("GrantedEffectStatSets", "Id", "string", false, None),
        (
            "GrantedEffects",
            "StatSet",
            "foreignrow",
            false,
            Some("GrantedEffectStatSets"),
        ),
        (
            "GrantedEffects",
            "AdditionalStatSets",
            "foreignrow",
            true,
            Some("GrantedEffectStatSets"),
        ),
    ] {
        super::convert::column(
            input,
            table,
            &[field],
            &json!({
                "name":field,"type":kind,"array":array,"interval":false,
                "references":target.map(|table| json!({"table":table}))
            }),
        )?;
    }
    Ok(())
}

fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for table in TABLES {
        let used: &[&str] = if table == "GrantedEffects" {
            &["Id", "StatSet", "AdditionalStatSets"]
        } else {
            &["Id"]
        };
        let doc = &input.documents[&format!("tables/{table}.json")];
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
        result.push(json!({
            "table":table,"payload":format!("tables/{table}.json"),"columns":columns,
            "source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "identity":"Id in each pinned physical row; target_membership.owners locates the codec owner subset and its ordered set references",
            "status":"not consumed by this compatibility domain; no numerical or label replacement"
        }));
    }
    Ok(result)
}
