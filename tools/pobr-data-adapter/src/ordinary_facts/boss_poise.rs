//! Target poise values for the existing, explicitly curated enemy presets.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::boss_poise_thresholds::BossPoiseThresholdsDef;
use serde::Deserialize;
use serde_json::{Value, json};

use super::convert::{column, text};
use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

pub(super) const TABLES: [&str; 1] = ["MonsterVarieties"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    version: String,
    map_id_contains: String,
    pinnacle_id: String,
    uber_uses_pinnacle: bool,
    statement: String,
}

pub(super) struct Converted {
    pub bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub policy_parent: PathBuf,
    pub receipt: Value,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    args: &super::BranchCodecArgs,
) -> Result<Converted, String> {
    let pinned = read_pinned(&args.path, &args.sha256, "boss poise preset policy")?;
    let policy: Policy = serde_json::from_value(pinned.document).map_err(|e| e.to_string())?;
    if policy.schema != "pobr-ordinary-boss-poise-policy/v1"
        || policy.version != input.manifest["source_identity"]["game_patch"]
        || policy.map_id_contains.is_empty()
        || policy.pinnacle_id.is_empty()
        || !policy.uber_uses_pinnacle
        || policy.statement.trim().is_empty()
    {
        return Err("invalid boss poise policy identity/selection".into());
    }
    let (output, selection) = project(input, &policy)?;
    let meta = json!({"generator":"pobr-data-adapter ordinary boss-poise",
        "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"policy_sha256":args.sha256,
        "selection_statement":policy.statement});
    let mut document = serde_json::to_value(output).map_err(|e| e.to_string())?;
    document["_meta"] = meta;
    Ok(Converted {
        bytes: super::pretty_bytes(&document)?,
        policy_bytes: pinned.bytes,
        policy_parent: pinned.parent,
        receipt: json!({"selection":selection,"policy_sha256":args.sha256,
            "statement":policy.statement,"units":"Raw source percent, unchanged; the existing consumer supplies unit conversion and rarity modifiers.",
            "scope":"Curated generic enemy preset selectors are model compatibility. These target-derived values do not certify every boss, pinnacle or Uber as identical. No numeric defaults or old-value equality gate."}),
    })
}

fn project(
    input: &VerifiedFacts,
    policy: &Policy,
) -> Result<(BossPoiseThresholdsDef, Value), String> {
    let tables = Tables::selected(input, &TABLES)?;
    for (field, kind) in [
        ("Id", "string"),
        ("BossHealthBar", "bool"),
        ("PoiseThreshold", "i32"),
    ] {
        column(
            input,
            TABLES[0],
            &[field],
            &json!({"name":field,"type":kind,"array":false,"interval":false,"references":null}),
        )?;
    }
    let mut ids = BTreeSet::new();
    let mut frequencies = BTreeMap::<u32, usize>::new();
    let mut map_rows = Vec::new();
    let mut pinnacle = None;
    for row in 0..tables.len(TABLES[0]) {
        let raw = tables.row(TABLES[0], row)?;
        let id = text(&raw["Id"])?;
        if id.is_empty() || !ids.insert(id) {
            return Err("empty/duplicate monster identity".into());
        }
        let boss = raw["BossHealthBar"]
            .as_bool()
            .ok_or("invalid boss-health-bar flag")?;
        let map = boss && id.contains(&policy.map_id_contains);
        if !map && id != policy.pinnacle_id {
            continue;
        }
        let value = raw["PoiseThreshold"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("selected poise must fit nonnegative source i32")?;
        let evidence = json!({"row":row,"id":id,"boss_health_bar":boss,"value":value,
            "pointer":format!("/rows/{row}/values/PoiseThreshold")});
        if map {
            *frequencies.entry(value).or_default() += 1;
            map_rows.push(evidence.clone());
        }
        if id == policy.pinnacle_id {
            pinnacle = Some((value, evidence));
        }
    }
    let max = frequencies
        .values()
        .max()
        .copied()
        .ok_or("empty MAP boss selection")?;
    let winners: Vec<_> = frequencies
        .iter()
        .filter(|(_, n)| **n == max)
        .map(|(v, _)| *v)
        .collect();
    if winners.len() != 1 {
        return Err("ambiguous MAP boss mode".into());
    }
    let (pinnacle, evidence) = pinnacle.ok_or("missing pinnacle identity")?;
    Ok((
        BossPoiseThresholdsDef {
            boss: winners[0],
            pinnacle,
            uber: pinnacle,
        },
        json!({"map_rows":map_rows,"frequencies":frequencies,"mode":winners[0],"pinnacle":evidence,
            "uber":"Explicit policy alias of pinnacle, not an independently observed Uber monster", "total_source_rows":tables.len(TABLES[0])}),
    ))
}
