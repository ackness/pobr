//! Physical catalyst identities and tags through the ordinary catalog schema.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::CatalystDef;
use serde::Deserialize;
use serde_json::{Value, json};

use super::convert::{resolve, text};
use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{Tables, VerifiedFacts, selected};
use crate::raw_source::sha256_hex;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

pub(super) const TABLES: [&str; 4] = ["BaseItemTypes", "ItemClasses", "Stats", "Tags"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    version: String,
    import_ids: BTreeMap<String, u8>,
    statement: String,
}

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub policy_bytes: Vec<u8>,
    pub policy_parent: PathBuf,
    pub receipt: Value,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    extra: &selected::Input,
    args: &super::BranchCodecArgs,
) -> Result<Converted, String> {
    let pinned = read_pinned(&args.path, &args.sha256, "catalyst import codec policy")?;
    let policy: Policy = serde_json::from_value(pinned.document).map_err(|e| e.to_string())?;
    if policy.schema != "pobr-ordinary-catalyst-policy/v1"
        || policy.version != input.manifest["source_identity"]["game_patch"]
        || policy.statement.is_empty()
    {
        return Err("catalyst policy identity mismatch".into());
    }
    let ids: BTreeSet<_> = policy.import_ids.values().copied().collect();
    if ids.len() != policy.import_ids.len() || ids.contains(&0) {
        return Err("duplicate/zero catalyst import ID".into());
    }
    let base = Tables::selected(input, &TABLES)?;
    let tables = extra.tables(&["AlternateQualityTypes", "ModEffectStats"])?;
    for (table, field, kind, array, target) in [
        ("AlternateQualityTypes", "Id", "string", false, None),
        (
            "AlternateQualityTypes",
            "Description",
            "string",
            false,
            None,
        ),
        (
            "AlternateQualityTypes",
            "Item",
            "foreignrow",
            false,
            Some("BaseItemTypes"),
        ),
        (
            "AlternateQualityTypes",
            "ModEffectStat",
            "foreignrow",
            false,
            Some("ModEffectStats"),
        ),
        (
            "AlternateQualityTypes",
            "ItemClass",
            "foreignrow",
            true,
            Some("ItemClasses"),
        ),
        ("ModEffectStats", "Stat", "foreignrow", false, Some("Stats")),
        ("ModEffectStats", "Tags", "foreignrow", true, Some("Tags")),
        ("ModEffectStats", "ApplyToExplicit", "bool", false, None),
        ("ModEffectStats", "ApplyToImplicit", "bool", false, None),
        ("ModEffectStats", "Multiplier", "i32", false, None),
    ] {
        let doc = &extra.documents[&format!("tables/{table}.json")];
        let column = doc["columns"]
            .as_array()
            .ok_or("missing supplement columns")?
            .iter()
            .find(|c| c["physical_fields"] == json!([field]))
            .ok_or("missing catalyst column")?;
        if column["schema"]["type"] != kind
            || column["schema"]["array"] != array
            || column["schema"]["references"]
                != target.map(|t| json!({"table":t})).unwrap_or(Value::Null)
        {
            return Err("catalyst column schema mismatch".into());
        }
    }
    let (output, records) = project(&base, &tables, &policy)?;
    let receipt = json!({"supplement_manifest_sha256":sha256_hex(&extra.manifest_bytes),"supplement":extra.manifest,
        "policy_sha256":args.sha256,"codec_statement":policy.statement,"records":records,
        "ownership":"Source owns all names, descriptors, tags and currency relationships. XML ordinal IDs are versioned compatibility. Raw Multiplier/flags/class restrictions retained, not a newly inferred quality-scaling law."});
    Ok(Converted {
        overlay: BTreeMap::from([(
            "catalysts.json".into(),
            super::pretty_bytes(&json!({"_meta":{
        "generator":"pobr-data-adapter ordinary catalysts","facts_manifest_sha256":sha256_hex(&input.manifest_bytes),
        "supplement_manifest_sha256":sha256_hex(&extra.manifest_bytes),"policy_sha256":args.sha256},"catalysts":output}))?,
        )]),
        policy_bytes: pinned.bytes,
        policy_parent: pinned.parent,
        receipt,
    })
}

fn row_index(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("invalid physical reference".into())
}
fn project(
    base: &Tables<'_>,
    tables: &Tables<'_>,
    policy: &Policy,
) -> Result<(Vec<CatalystDef>, Vec<Value>), String> {
    let mut primary = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut output = Vec::new();
    for row in 0..tables.len("AlternateQualityTypes") {
        let raw = tables.row("AlternateQualityTypes", row)?;
        let stable = text(&raw["Id"])?;
        let Some(&id) = policy.import_ids.get(stable) else {
            continue;
        };
        used.insert(stable.to_owned());
        let name = resolve(base, "BaseItemTypes", &raw["Item"], "Name")?;
        let name = name
            .strip_suffix(" Catalyst")
            .filter(|n| !n.is_empty())
            .ok_or("unsupported catalyst name syntax")?;
        let descriptor = text(&raw["Description"])?;
        let descriptor = descriptor
            .strip_prefix("Quality (")
            .and_then(|v| v.strip_suffix(" Modifiers)"))
            .filter(|s| !s.is_empty())
            .ok_or("unsupported catalyst descriptor syntax")?;
        let effect_index = row_index(&raw["ModEffectStat"])?;
        let effect = tables.row("ModEffectStats", effect_index)?;
        let tags = effect["Tags"]
            .as_array()
            .ok_or("missing catalyst tags")?
            .iter()
            .map(|v| resolve(base, "Tags", v, "Id"))
            .collect::<Result<Vec<_>, _>>()?;
        if tags.is_empty() {
            return Err("catalyst effect has no tags".into());
        }
        // A changed scope needs an explicit consumer decision; it must not be
        // silently flattened into a different ordinary quality type.
        if effect["ApplyToExplicit"] != true
            || effect["ApplyToImplicit"] != true
            || !effect["Multiplier"].is_i64()
        {
            return Err("unsupported catalyst effect scope".into());
        }
        if primary
            .insert(effect_index, (id, descriptor.to_owned()))
            .is_some()
        {
            return Err("ambiguous catalyst primary effect".into());
        }
        output.push(CatalystDef {
            id,
            name: name.into(),
            descriptor: descriptor.into(),
            tags,
        });
    }
    if used.len() != policy.import_ids.len() {
        return Err("unused catalyst import identity".into());
    }
    let mut records = Vec::new();
    for row in 0..tables.len("AlternateQualityTypes") {
        let raw = tables.row("AlternateQualityTypes", row)?;
        let effect_index = row_index(&raw["ModEffectStat"])?;
        let (id, descriptor) = primary
            .get(&effect_index)
            .ok_or("new catalyst effect requires explicit import ID")?;
        if raw["Description"] != format!("Quality ({descriptor} Modifiers)") {
            return Err("catalyst variant descriptor differs".into());
        }
        let effect = tables.row("ModEffectStats", effect_index)?;
        let classes = raw["ItemClass"]
            .as_array()
            .ok_or("missing catalyst classes")?
            .iter()
            .map(|v| resolve(base, "ItemClasses", v, "Id"))
            .collect::<Result<Vec<_>, _>>()?;
        records.push(json!({"source_row":row,"source_id":raw["Id"],"ordinary_id":id,
            "item_id":resolve(base,"BaseItemTypes",&raw["Item"],"Id")?,"name":resolve(base,"BaseItemTypes",&raw["Item"],"Name")?,
            "raw":raw,"effect_row":effect_index,"effect_raw":effect,"stat_id":resolve(base,"Stats",&effect["Stat"],"Id")?,"item_classes":classes}));
    }
    output.sort_by_key(|c| c.id);
    Ok((output, records))
}
