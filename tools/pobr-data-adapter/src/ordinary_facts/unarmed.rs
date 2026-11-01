//! Target character weapon facts in the existing ordinary unarmed DTO.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::UnarmedWeaponDef;
use pobr_data::catalog::native_player::{NATIVE_GENERIC_OT, NativeMetadataFile};
use serde::Deserialize;
use serde_json::{Value, json};

use super::convert::{column, text};
use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::native_facts::runtime::player;
use crate::raw_source::sha256_hex;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

// The shared player-source verifier also authenticates its existing physical joins.
pub(super) const TABLES: [&str; 4] = [
    "Characters",
    "Stats",
    "PassiveSkills",
    "ClassPassiveSkillOverrides",
];
const CRIT: &str = "unarmed_base_critical_strike_chance";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    version: String,
    source_sha256: String,
    player_manifest_sha256: String,
    player_source_lock_sha256: String,
    legacy_bindings: BTreeMap<String, String>,
    legacy_only: BTreeMap<String, Value>,
    statement: String,
}

pub(super) struct Converted {
    pub bytes: Vec<u8>,
    pub sidecars: BTreeMap<String, Vec<u8>>,
    pub protected_parents: Vec<PathBuf>,
    pub receipt: Value,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    codec: &super::CodecArgs,
    context: &super::BranchCodecArgs,
) -> Result<Converted, String> {
    let pinned = read_pinned(&codec.policy, &codec.policy_sha256, "unarmed policy")?;
    let p: Policy = serde_json::from_value(pinned.document).map_err(|e| e.to_string())?;
    if p.schema != "pobr-ordinary-unarmed-policy/v1"
        || p.version != input.manifest["source_identity"]["game_patch"]
        || p.source_sha256 != codec.source_sha256
        || p.player_manifest_sha256 != context.sha256
        || p.statement.trim().is_empty()
    {
        return Err("unarmed policy identity mismatch".into());
    }
    let old = read_pinned(&codec.source, &codec.source_sha256, "legacy unarmed data")?;
    let player = player::verify(
        &player::Args {
            root: context.path.clone(),
            manifest_sha256: context.sha256.clone(),
            source_lock_sha256: p.player_source_lock_sha256.clone(),
        },
        input,
    )?;
    let metadata = player
        .context
        .metadata
        .files
        .get(NATIVE_GENERIC_OT)
        .ok_or("missing generic character OT")?;
    let (output, records, crit) = project(input, &old.document, &p, metadata)?;
    let mut sidecars: BTreeMap<_, _> = player
        .context
        .source_archive
        .into_iter()
        .map(|(name, bytes)| (format!("player-{name}"), bytes.into_bytes()))
        .collect();
    sidecars.insert("legacy-unarmed.json".into(), old.bytes);
    sidecars.insert("codec-policy.json".into(), pinned.bytes);
    Ok(Converted {
        bytes: super::pretty_bytes(&output)?,
        receipt: json!({"records":records,"critical_strike":crit,"policy_sha256":codec.policy_sha256,
            "legacy_source_sha256":codec.source_sha256,"player_manifest_sha256":context.sha256,
            "player_source_lock_sha256":p.player_source_lock_sha256,"player_source":player.manifest,
            "field_interpretation":p.statement,"retained_historical_entries":p.legacy_only,
            "unconsumed_character_fields":"All non-owned columns remain at the pinned Characters payload; they are not effectless or projected into this DTO.",
            "scope":"Ordinary unarmed fields only, not native runtime/admission or a general actor-stat merge. Original source acquisition status is unchanged."}),
        sidecars,
        protected_parents: vec![old.parent, pinned.parent, context.path.clone()],
    })
}

fn project(
    input: &VerifiedFacts,
    old: &Value,
    p: &Policy,
    metadata: &NativeMetadataFile,
) -> Result<(Value, Vec<Value>, Value), String> {
    let tables = Tables::selected(input, &TABLES)?;
    for (field, kind) in [
        ("Id", "string"),
        ("Name", "string"),
        ("IntegerId", "i32"),
        ("WeaponSpeed", "i32"),
        ("MinDamage", "i32"),
        ("MaxDamage", "i32"),
    ] {
        column(
            input,
            "Characters",
            &[field],
            &json!({"name":field,"type":kind,"array":false,"interval":false,"references":null}),
        )?;
    }
    if metadata.path != NATIVE_GENERIC_OT {
        return Err("wrong unarmed generic metadata root".into());
    }
    let assignments: Vec<_> = metadata
        .assignments
        .iter()
        .enumerate()
        .filter(|(_, a)| a.key_raw == CRIT && a.plain_stats_scope)
        .collect();
    if assignments.len() != 1 {
        return Err("missing/ambiguous unarmed critical strike assignment".into());
    }
    let (assignment, raw_crit) = assignments[0];
    let crit = raw_crit
        .value_raw
        .trim()
        .parse::<u32>()
        .map_err(|_| "unarmed crit requires an exact nonnegative integer literal")?;
    let crit_fraction = f64::from(crit) / 10000.0;
    let critical = json!({"path":NATIVE_GENERIC_OT,"assignment":assignment,"source":raw_crit,
        "stat_id":CRIT,"raw_integer":crit,"divisor":10000,"output_fraction":crit_fraction,
        "interpretation":"Explicit shared ordinary unarmed-base field; no AO inheritance or generic metadata stat merge is inferred."});
    let old_rows = old
        .as_array()
        .ok_or("legacy unarmed data must be an array")?;
    let mut old_names = BTreeSet::new();
    let mut bound = BTreeMap::new();
    let mut historical = Vec::new();
    for (index, row) in old_rows.iter().enumerate() {
        let typed: UnarmedWeaponDef =
            serde_json::from_value(row.clone()).map_err(|e| e.to_string())?;
        if typed.class_name.is_empty() || !old_names.insert(typed.class_name.clone()) {
            return Err("empty/duplicate legacy unarmed name".into());
        }
        match (
            p.legacy_bindings.get(&typed.class_name),
            p.legacy_only.get(&typed.class_name),
        ) {
            (Some(id), None) if !id.is_empty() => {
                if bound.insert(id.clone(), (index, row)).is_some() {
                    return Err("duplicate unarmed legacy stable-ID binding".into());
                }
            }
            (None, Some(exact)) if exact == row => historical.push((index, row.clone())),
            _ => return Err("legacy unarmed entry has no exact single policy disposition".into()),
        }
    }
    if old_names.len() != p.legacy_bindings.len() + p.legacy_only.len() {
        return Err("unused unarmed legacy declaration".into());
    }
    let mut output = Vec::new();
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut integers = BTreeSet::new();
    for row in 0..tables.len("Characters") {
        let raw = tables.row("Characters", row)?;
        let id = text(&raw["Id"])?;
        let name = text(&raw["Name"])?;
        let integer = unsigned(&raw["IntegerId"])?;
        if id.is_empty()
            || name.is_empty()
            || !ids.insert(id)
            || !names.insert(name)
            || !integers.insert(integer)
        {
            return Err("empty/duplicate target unarmed identity/name/class ID".into());
        }
        let speed = unsigned(&raw["WeaponSpeed"])?;
        let min = unsigned(&raw["MinDamage"])?;
        let max = unsigned(&raw["MaxDamage"])?;
        if speed == 0 || min > max {
            return Err("unarmed attack time must be nonzero and damage ordered".into());
        }
        let previous = bound.remove(id);
        let mut entry = previous
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| json!({"weapon_type":"None"}));
        entry["class_id"] = json!(integer);
        entry["class_name"] = json!(name);
        entry["attack_rate"] = json!(1000.0 / f64::from(speed));
        entry["crit_chance"] = json!(crit_fraction);
        entry["physical_min"] = json!(f64::from(min));
        entry["physical_max"] = json!(f64::from(max));
        // The type vocabulary and unknown old fields stay explicit compatibility.
        let _: UnarmedWeaponDef =
            serde_json::from_value(entry.clone()).map_err(|e| e.to_string())?;
        records.push(json!({"source_row":row,"stable_id":id,"source_pointer":format!("/rows/{row}/values"),
            "source_payload_sha256":sha256_hex(&input.bytes["tables/Characters.json"]),
            "source_fields":{"Name":name,"IntegerId":integer,"WeaponSpeed":speed,"MinDamage":min,"MaxDamage":max},
            "output":entry,"legacy_row":previous.map(|(i,_)|i),"attack_rate_normalization":"1000 / WeaponSpeed milliseconds, unrounded f64"}));
        output.push(entry);
    }
    if !bound.is_empty() {
        return Err("legacy-bound target unarmed character is missing".into());
    }
    for (index, row) in historical {
        if names.contains(text(&row["class_name"])?) {
            return Err("historical unarmed name now exists in target; update policy".into());
        }
        records.push(
            json!({"mode":"complete-historical-compatibility","legacy_row":index,"output":row}),
        );
        output.push(row);
    }
    Ok((json!(output), records, critical))
}

fn unsigned(v: &Value) -> Result<u32, String> {
    let n = v
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or("Characters requires exact i32 fields")?;
    u32::try_from(n).map_err(|_| "negative unarmed source field".into())
}
