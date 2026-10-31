//! Shared minion/spectre catalogs: target fields and separately disclosed curated membership.

mod policy;
mod projection;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::actors::MinionsDef;
use serde_json::{Value, json};

use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{self, Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 5] = [
    "MonsterVarieties",
    "MonsterTypes",
    "MonsterResistances",
    "Tags",
    "GrantedEffects",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Catalog {
    Minions,
    Spectres,
}

impl Catalog {
    fn name(self) -> &'static str {
        match self {
            Self::Minions => "minions",
            Self::Spectres => "spectres",
        }
    }
    fn file(self) -> &'static str {
        match self {
            Self::Minions => "minions.json",
            Self::Spectres => "spectres.json",
        }
    }
    fn schema(self) -> &'static str {
        match self {
            Self::Minions => "minions/v1",
            Self::Spectres => "spectres/v1",
        }
    }
    fn identity_field(self) -> &'static str {
        match self {
            Self::Minions => "monster_variety_id",
            Self::Spectres => "id",
        }
    }
}

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub source_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub protected_parents: [PathBuf; 2],
    pub receipt: Value,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    args: &super::CodecArgs,
    catalog: Catalog,
) -> Result<Converted, String> {
    let pinned_policy = read_pinned(&args.policy, &args.policy_sha256, "actor policy")?;
    let policy: policy::Policy =
        serde_json::from_value(pinned_policy.document).map_err(|e| format!("actor policy: {e}"))?;
    policy.validate(input, catalog)?;
    if policy.source.sha256 != args.source_sha256 {
        return Err("actor source pin differs from policy identity".into());
    }
    let legacy = read_pinned(&args.source, &args.source_sha256, "actor source")?;
    if legacy.document["_meta"] != policy.source.metadata {
        return Err("actor source metadata differs from policy legacy identity".into());
    }
    // Validate the existing consumer shape, while preserving all non-owned raw Values.
    let dto: MinionsDef = serde_json::from_value(legacy.document.clone())
        .map_err(|e| format!("actor source: {e}"))?;
    let tables = Tables::selected(input, &TABLES)?;
    projection::check_columns(input)?;
    let identities: BTreeMap<_, _> = TABLES
        .iter()
        .map(|&table| Ok((table, projection::identities(&tables, table)?)))
        .collect::<Result<_, String>>()?;
    let mut document = legacy.document.clone();
    let entries = document["minions"]
        .as_array_mut()
        .ok_or("minions must be an array")?;
    let mut ids = BTreeSet::new();
    let mut aliases: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut records = Vec::new();
    let mut projected_fields = 0;
    let mut skill_occurrences = 0;
    let mut numeric_differences = Vec::new();
    for (source_row, (entry, output)) in dto.minions.iter().zip(entries).enumerate() {
        if entry.id.is_empty() {
            return Err("empty catalog Id".into());
        }
        if !ids.insert(&entry.id) {
            return Err(format!("duplicate catalog Id {}", entry.id));
        }
        let raw = &legacy.document["minions"][source_row];
        let variety_id = raw[catalog.identity_field()]
            .as_str()
            .ok_or("required catalog variety identity must be a string")?;
        if catalog == Catalog::Spectres
            && raw
                .get("monster_variety_id")
                .is_some_and(|v| v.as_str() != Some(variety_id))
        {
            return Err("spectre monster_variety_id differs from id".into());
        }
        let &variety_row = identities["MonsterVarieties"]
            .get(variety_id)
            .ok_or_else(|| format!("catalog {}: missing target variety {variety_id}", entry.id))?;
        let projected = projection::project(&tables, variety_row)
            .map_err(|e| format!("catalog {} / MonsterVarieties/{variety_row}: {e}", entry.id))?;
        let mut skills = Vec::new();
        for (position, id) in entry.skill_list.iter().enumerate() {
            let &row = identities["GrantedEffects"].get(id).ok_or_else(|| {
                format!(
                    "catalog {} skill_list[{position}]: missing target skill {id}",
                    entry.id
                )
            })?;
            skills.push(
                json!({"position":position,"id":id,"source_row":row,"table":"GrantedEffects"}),
            );
        }
        skill_occurrences += skills.len();
        let mut field_records = Vec::new();
        let object = output
            .as_object_mut()
            .ok_or("catalog entry must be an object")?;
        for field in projection::OWNED_FIELDS {
            let value = projected.values.get(field);
            let old = raw.get(field);
            let numeric_equal = old
                .and_then(Value::as_f64)
                .zip(value.and_then(Value::as_f64))
                .map(|(a, b)| a == b);
            if numeric_equal == Some(false) {
                numeric_differences.push(json!({"source_row":source_row,"id":entry.id,"field":field,"legacy":old,"target":value}));
            }
            field_records.push(json!({"field":field,"present":value.is_some(),"output":value,
                "omission":if value.is_none(){Some("raw zero; existing optional-field omission, no default materialized")}else{None},
                "legacy_comparison":{"present":old.is_some(),"value":old,"json_value_equal":old==value,"f64_equal":numeric_equal}}));
            object.remove(field);
        }
        projected_fields += projected.values.len();
        object.extend(projected.values);
        let retained:Vec<_>=raw.as_object().ok_or("legacy entry must be object")?.iter()
            .filter(|(field,_)|!projection::OWNED_FIELDS.contains(&field.as_str()))
            .map(|(field,value)|Ok(json!({"field":field,"source_pointer":format!("/minions/{source_row}/{}",pointer_token(field)),
                "value_sha256":sha256_hex(&reader::json_bytes(value)?),"status":"retained unchanged fixed community input; not target acquisition or law certification"})))
            .collect::<Result<_,String>>()?;
        aliases
            .entry(variety_id.into())
            .or_default()
            .push(json!({"source_row":source_row,"id":entry.id}));
        records.push(json!({"source_row":source_row,"source_pointer":format!("/minions/{source_row}"),"id":entry.id,
            "variety_id":variety_id,"alias":entry.id!=variety_id,"projection":projected.receipt,"owned_fields":field_records,
            "retained_fields":retained,"skill_list_present":raw.get("skill_list").is_some(),"skill_occurrences":skills,
            "source_entry_sha256":sha256_hex(&reader::json_bytes(raw)?),"output_entry_sha256":sha256_hex(&reader::json_bytes(output)?)}));
    }
    let legacy_identity = json!({"file":"legacy-actors.json","bytes":legacy.bytes.len(),"sha256":sha256_hex(&legacy.bytes),
        "snapshot_version":policy.source.snapshot_version,"reference_path":policy.source.path,"metadata":policy.source.metadata,
        "scope":"curated catalog membership/aliases and all non-owned fields are fixed community inputs"});
    let policy_identity = json!({"file":"codec-policy.json","bytes":pinned_policy.bytes.len(),"sha256":sha256_hex(&pinned_policy.bytes)});
    let target = json!({"facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"source_identity":input.manifest["source_identity"],"owned_fields":projection::OWNED_FIELDS,
        "scope":"declared actor fields only; acquisition evidence/status unchanged"});
    let recipe = json!({"schema":policy.projection_schema,"references":policy.projection_references,
        "status":"fixed archived projection semantics, never implicitly read/executed or claimed as independent gameplay laws"});
    document["_meta"] = json!({"schema":catalog.schema(),"generator":"pobr-data-adapter/ordinary-facts",
        "target_fields":target,"projection_recipe":recipe,"legacy_compatibility":{"source":legacy_identity,"policy":policy_identity},
        "status":"domain-candidate; mixed target fields and fixed community interpretation; no complete runtime or adoption"});
    let _: MinionsDef = serde_json::from_value(document.clone())
        .map_err(|e| format!("converted actor catalog: {e}"))?;
    let receipt = json!({"catalog":catalog.name(),"entries":records.len(),"projected_field_occurrences":projected_fields,
        "retained_skill_occurrences":skill_occurrences,"source_records":records,"variety_aliases":aliases,
        "legacy_source":legacy_identity,"codec_policy":policy_identity,"target_fields":target,"projection_recipe":recipe,
        "mapping":projection::mapping(),"historical_source_evidence":policy.evidence,
        "historically_observed_retained_fields":policy.retained_compatibility_fields_observed,"compatibility_statement":policy.statement,
        "legacy_f64_differences":numeric_differences,
        "comparison_scope":"legacy equality never gates projection; per-field parsed JSON/f64 comparisons are not raw text equality, mathematical equivalence or BD acceptance; exact companion formula tails are preserved",
        "unconsumed_source":projection::unmapped(input,&tables)?,
        "preservation":"catalog IDs/order/aliases and every non-owned Value/absent-field shape retained; owned fields and top metadata replaced, original input/policy bytes archived",
        "pending":"no automatic creature admission, AI/law/whole minion-model certification; remaining links/minion-level/tree/config assets, full target runtime, ordinary BD/Web/accuracy/source approval/adoption remain separate"});
    Ok(Converted {
        overlay: BTreeMap::from([(catalog.file().into(), super::pretty_bytes(&document)?)]),
        source_bytes: legacy.bytes,
        policy_bytes: pinned_policy.bytes,
        protected_parents: [legacy.parent, pinned_policy.parent],
        receipt,
    })
}

fn pointer_token(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
