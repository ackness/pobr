//! Optional common player facts in existing ordinary scalar constants.
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::native_player::{NATIVE_GENERIC_OT, NativeMetadataFile};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::super::BranchCodecArgs;
use super::super::stat_set_codec::policy::hex;
use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::native_facts::runtime::player as source;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    pub manifest_sha256: String,
    pub source_lock_sha256: String,
    pub bindings: Vec<Binding>,
    pub statement: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub file: String,
    pub target_pointer: String,
    pub source: Source,
    pub additional_divisors: Vec<f64>,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Source {
    GenericStat { id: String },
    CharacterConsensus { field: String },
}
impl Policy {
    pub(super) fn validate<'a>(
        &'a self,
        files: &BTreeSet<&str>,
        destinations: &mut BTreeSet<(&'a String, &'a String)>,
    ) -> Result<(), String> {
        hex(&self.manifest_sha256, 64, "player manifest SHA256")?;
        hex(&self.source_lock_sha256, 64, "player lock SHA256")?;
        if self.bindings.is_empty() || self.statement.trim().is_empty() {
            return Err("empty player field policy".into());
        }
        for binding in &self.bindings {
            if !matches!(
                binding.file.as_str(),
                "character_constants.json" | "game_constants.json"
            ) || !files.contains(binding.file.as_str())
                || !destinations.insert((&binding.file, &binding.target_pointer))
                || !binding.target_pointer.starts_with('/')
                || binding
                    .additional_divisors
                    .iter()
                    .any(|v| !v.is_finite() || *v == 0.0)
            {
                return Err("invalid/duplicate player field binding".into());
            }
            match &binding.source {
                Source::GenericStat { id } if id.is_empty() => {
                    return Err("empty player stat ID".into());
                }
                Source::CharacterConsensus { field }
                    if !matches!(field.as_str(), "BaseMaxLife" | "BaseMaxMana") =>
                {
                    return Err("unsupported common character column".into());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub(super) struct Applied {
    pub files: BTreeMap<String, Vec<u8>>,
    pub receipt: Value,
}
pub(super) fn apply(
    input: &VerifiedFacts,
    args: &BranchCodecArgs,
    policy: &Policy,
    documents: &mut BTreeMap<String, Value>,
    shapes: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Applied, String> {
    if args.sha256 != policy.manifest_sha256 {
        return Err("player constants manifest pin differs".into());
    }
    let player = source::verify(
        &source::Args {
            root: args.path.clone(),
            manifest_sha256: args.sha256.clone(),
            source_lock_sha256: policy.source_lock_sha256.clone(),
        },
        input,
    )?;
    let metadata = player
        .context
        .metadata
        .files
        .get(NATIVE_GENERIC_OT)
        .ok_or("missing generic player metadata")?;
    let records = project(input, metadata, policy, documents, shapes)?;
    Ok(Applied {
        files: player
            .context
            .source_archive
            .into_iter()
            .map(|(name, bytes)| (format!("player-{name}"), bytes.into_bytes()))
            .collect(),
        receipt: json!({"policy":policy,"records":records,"source_manifest":player.manifest,
            "scope":"Common ordinary scalar field interpretation only. Original generic OT root remains independent of AO inheritance; all unconsumed source facts remain archived, not declared effectless."}),
    })
}

pub(super) fn project(
    input: &VerifiedFacts,
    metadata: &NativeMetadataFile,
    policy: &Policy,
    documents: &mut BTreeMap<String, Value>,
    shapes: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<Value>, String> {
    if metadata.path != NATIVE_GENERIC_OT {
        return Err("wrong generic player metadata root".into());
    }
    let tables = Tables::selected(input, &super::super::unarmed::TABLES)?;
    let mut records = Vec::new();
    for (position, binding) in policy.bindings.iter().enumerate() {
        if !shapes
            .get(&binding.file)
            .is_some_and(|s| s.contains(&binding.target_pointer))
        {
            return Err("unknown/non-numeric player destination".into());
        }
        let (value, evidence) = match &binding.source {
            Source::GenericStat { id } => {
                let assignments: Vec<_> = metadata
                    .assignments
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.key_raw == *id && a.plain_stats_scope)
                    .collect();
                if assignments.len() != 1 {
                    return Err(format!("missing/ambiguous common player stat: {id}"));
                }
                let (index, assignment) = assignments[0];
                let value = assignment
                    .value_raw
                    .trim()
                    .parse::<i32>()
                    .map_err(|_| "common player stat requires a signed integer literal")?;
                (
                    value,
                    json!({"path":NATIVE_GENERIC_OT,"assignment":index,"source":assignment,"stat_id":id}),
                )
            }
            Source::CharacterConsensus { field } => {
                for (name, kind) in [("Id", "string"), (field.as_str(), "i32")] {
                    super::super::convert::column(
                        input,
                        "Characters",
                        &[name],
                        &json!({"name":name,"type":kind,"array":false,"interval":false,"references":null}),
                    )?;
                }
                let mut ids = BTreeSet::new();
                let mut common = None;
                let mut rows = Vec::new();
                for row in 0..tables.len("Characters") {
                    let raw = tables.row("Characters", row)?;
                    let id = super::super::convert::text(&raw["Id"])?;
                    if id.is_empty() || !ids.insert(id) {
                        return Err("empty/duplicate character consensus identity".into());
                    }
                    let value = super::integer(&raw[field], field)?;
                    if common.is_some_and(|previous| previous != value) {
                        return Err(format!(
                            "character disagreement cannot fit shared scalar: {field}"
                        ));
                    }
                    common = Some(value);
                    rows.push(json!({"row":row,"id":id,"value":value,"pointer":format!("/rows/{row}/values/{field}")}));
                }
                (
                    common.ok_or("empty character consensus")?,
                    json!({"payload":"tables/Characters.json","field":field,"rows":rows,"rule":"Every target row must agree; no selected-class fallback or average"}),
                )
            }
        };
        let (normalized, steps) = super::normalize(value, 1, &binding.additional_divisors)?;
        let document = documents
            .get_mut(&binding.file)
            .ok_or("missing player destination document")?;
        let (parent, leaf) = super::leaf_parent(document, &binding.target_pointer)?;
        let old = parent
            .get(&leaf)
            .cloned()
            .ok_or("player destination leaf is absent")?;
        if !old.as_f64().is_some_and(f64::is_finite) {
            return Err("player destination must be finite numeric leaf".into());
        }
        parent.insert(leaf, json!(normalized));
        records.push(json!({"position":position,"binding":binding,"source":evidence,"raw_value":value,
            "normalization_steps":steps,"output":normalized,"legacy_value":old,"old_equality_is_not_admission":true}));
    }
    Ok(records)
}
