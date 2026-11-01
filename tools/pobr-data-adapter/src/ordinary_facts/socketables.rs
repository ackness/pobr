//! Target socketable membership, restrictions and complete stat packets.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pobr_data::catalog::item_overlay::{RuneDef, RuneSlotDef, RunesDef};
use serde::Deserialize;
use serde_json::{Value, json};

use super::convert::{resolve, text};
use super::passive_tree::render::Renderer;
use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{self, Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

pub(super) const TABLES: [&str; 8] = [
    "BaseItemTypes",
    "SoulCores",
    "SoulCoreTypes",
    "SoulCoreStats",
    "SoulCoreStatCategories",
    "SoulCoreLimits",
    "Stats",
    "ItemClasses",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    version: String,
    source_sha256: String,
    descriptions_sha256: String,
    descriptions_manifest_sha256: String,
    categories: BTreeMap<String, Vec<String>>,
    category_item_classes: BTreeMap<String, Vec<String>>,
    limit_groups: BTreeMap<String, Option<String>>,
    compatibility_slot_aliases: BTreeMap<String, Vec<String>>,
    import_aliases: BTreeMap<String, Vec<String>>,
    excluded: BTreeMap<String, String>,
    compatibility: BTreeMap<String, Compatibility>,
    statement: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Compatibility {
    name: String,
    effects_sha256: String,
}

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub policy_bytes: Vec<u8>,
    pub source_bytes: Vec<u8>,
    pub protected_parents: Vec<PathBuf>,
    pub receipt: Value,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    args: &super::CodecArgs,
    descriptions: &Path,
) -> Result<Converted, String> {
    let policy_input = read_pinned(&args.policy, &args.policy_sha256, "socketable policy")?;
    let policy: Policy =
        serde_json::from_value(policy_input.document).map_err(|e| e.to_string())?;
    if policy.schema != "pobr-ordinary-socketables-policy/v1"
        || policy.version != input.manifest["source_identity"]["game_patch"]
        || policy.source_sha256 != args.source_sha256
        || policy.statement.trim().is_empty()
    {
        return Err("socketable policy identity mismatch".into());
    }
    let legacy = read_pinned(
        &args.source,
        &args.source_sha256,
        "socketable compatibility",
    )?;
    let csd = read_pinned(
        descriptions,
        &policy.descriptions_sha256,
        "socketable descriptions",
    )?;
    let manifest = read_pinned(
        &descriptions.with_file_name("manifest.json"),
        &policy.descriptions_manifest_sha256,
        "socketable description manifest",
    )?;
    if manifest.document["format"] != "repoe-native-descriptions/v1"
        || manifest.document["files"]["descriptions.json"]["sha256"] != policy.descriptions_sha256
        || manifest.document["files"]["descriptions.json"]["bytes"] != csd.bytes.len()
        || manifest.document["source_identity"]["game_patch"] != policy.version
    {
        return Err("socketable description source mismatch".into());
    }
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    let renderer = Renderer::global(&csd.document)?;
    let Projected {
        runes,
        records,
        excluded,
        aliases,
    } = project(&tables, &policy, &legacy.document, &renderer)?;
    let receipt = json!({"records":records,"excluded":excluded,"import_aliases":aliases,
        "target_fields":"membership, name, type, level, flags, limit, full ordered stat packets; global CSD rendered as a whole socketable",
        "compatibility":{"sha256":args.source_sha256,"metadata":legacy.document["_meta"],"statement":policy.statement},
        "policy_sha256":args.policy_sha256,
        "descriptions":{"sha256":policy.descriptions_sha256,"manifest_sha256":policy.descriptions_manifest_sha256,
            "source_identity":manifest.document["source_identity"]},
        "unconsumed":"SoulCores TierHigher/Description/ExtraDescription/Flag0/CanSocketInMartialArtistSlots; category Unknown0 and limit Text/Flag0 are retained in source, not new runtime laws. Display rank/stat_order are absent for target-rendered text; source packet order is retained."});
    let document = json!({"_meta":{"generator":"pobr-data-adapter ordinary socketables",
        "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"source_identity":input.manifest["source_identity"],
        "compatibility_source_sha256":args.source_sha256,"policy_sha256":args.policy_sha256},"runes":runes});
    Ok(Converted {
        overlay: BTreeMap::from([("runes.json".into(), super::pretty_bytes(&document)?)]),
        policy_bytes: policy_input.bytes,
        source_bytes: legacy.bytes,
        protected_parents: vec![
            policy_input.parent,
            legacy.parent,
            csd.parent,
            manifest.parent,
        ],
        receipt,
    })
}

fn index(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| format!("invalid physical reference: {value}"))
}
fn uint(value: &Value) -> Result<u32, String> {
    value
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("invalid unsigned field: {value}"))
}
fn flag(row: &Value, key: &str) -> Result<bool, String> {
    row[key]
        .as_bool()
        .ok_or_else(|| format!("missing boolean {key}"))
}
fn hash(value: &Value) -> Result<String, String> {
    Ok(sha256_hex(&reader::json_bytes(value)?))
}

fn packet(tables: &Tables<'_>, row: &Value, key: &str) -> Result<Vec<(String, i64)>, String> {
    let ids = row[key]
        .as_array()
        .ok_or_else(|| format!("missing {key}"))?;
    let field = format!("{key}Values");
    let values = row[&field]
        .as_array()
        .ok_or_else(|| format!("missing {field}"))?;
    if ids.len() != values.len() {
        return Err(format!("{key}/{field} cardinality mismatch"));
    }
    ids.iter()
        .zip(values)
        .map(|(id, value)| {
            Ok((
                resolve(tables, "Stats", id, "Id")?,
                value
                    .as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .map(i64::from)
                    .ok_or_else(|| format!("invalid signed {field} value"))?,
            ))
        })
        .collect()
}

fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    let fields = [
        ("BaseItemTypes", "Id", "string", false, None),
        ("BaseItemTypes", "Name", "string", false, None),
        ("Stats", "Id", "string", false, None),
        ("ItemClasses", "Id", "string", false, None),
        ("SoulCoreTypes", "Id", "string", false, None),
        ("SoulCoreLimits", "Id", "string", false, None),
        ("SoulCoreLimits", "Limit", "i32", false, None),
        ("SoulCoreStatCategories", "Id", "string", false, None),
        (
            "SoulCoreStatCategories",
            "TargetItemClasses",
            "foreignrow",
            true,
            Some("ItemClasses"),
        ),
        (
            "SoulCores",
            "BaseItemType",
            "foreignrow",
            false,
            Some("BaseItemTypes"),
        ),
        (
            "SoulCores",
            "Type",
            "foreignrow",
            false,
            Some("SoulCoreTypes"),
        ),
        (
            "SoulCores",
            "Limit",
            "foreignrow",
            false,
            Some("SoulCoreLimits"),
        ),
        ("SoulCores", "RequiredLevel", "i32", false, None),
        ("SoulCores", "IsSocketBound", "bool", false, None),
        ("SoulCores", "CanSocketInUniqueItems", "bool", false, None),
        ("SoulCores", "CanSocketInJewellery", "bool", false, None),
        (
            "SoulCores",
            "CanSocketInCorruptedSanctified",
            "bool",
            false,
            None,
        ),
        (
            "SoulCoreStats",
            "SoulCore",
            "foreignrow",
            false,
            Some("SoulCores"),
        ),
        (
            "SoulCoreStats",
            "StatCategory",
            "foreignrow",
            false,
            Some("SoulCoreStatCategories"),
        ),
        ("SoulCoreStats", "Stats", "foreignrow", true, Some("Stats")),
        ("SoulCoreStats", "StatsValues", "i32", true, None),
        (
            "SoulCoreStats",
            "BondedStats",
            "foreignrow",
            true,
            Some("Stats"),
        ),
        ("SoulCoreStats", "BondedStatsValues", "i32", true, None),
    ];
    for (table, name, kind, array, reference) in fields {
        super::convert::column(
            input,
            table,
            &[name],
            &json!({"name":name,"type":kind,
            "array":array,"interval":false,"references":reference.map(|t|json!({"table":t}))}),
        )?;
    }
    Ok(())
}

struct Projected {
    runes: Vec<RuneDef>,
    records: Vec<Value>,
    excluded: Vec<Value>,
    aliases: Vec<Value>,
}

fn project(
    tables: &Tables<'_>,
    policy: &Policy,
    legacy: &Value,
    renderer: &Renderer<'_>,
) -> Result<Projected, String> {
    let old: RunesDef = serde_json::from_value(legacy.clone()).map_err(|e| e.to_string())?;
    let mut legacy_by_name = BTreeMap::new();
    for rune in &old.runes {
        if legacy_by_name.insert(rune.name.as_str(), rune).is_some() {
            return Err("duplicate legacy socketable name".into());
        }
    }
    let mut effects = BTreeMap::<usize, Vec<(usize, Value)>>::new();
    for row in 0..tables.len("SoulCoreStats") {
        let raw = tables.row("SoulCoreStats", row)?;
        let core = index(&raw["SoulCore"])?;
        tables.row("SoulCores", core)?;
        let category = tables.row("SoulCoreStatCategories", index(&raw["StatCategory"])?)?;
        let id = text(&category["Id"])?;
        // Validate all source FK occurrences even for explicitly excluded records.
        let mut target_classes = Vec::new();
        for item in category["TargetItemClasses"]
            .as_array()
            .ok_or("missing target item classes")?
        {
            target_classes.push(resolve(tables, "ItemClasses", item, "Id")?);
        }
        if policy.category_item_classes.get(id) != Some(&target_classes) {
            return Err(format!(
                "socketable category item-class binding changed: {id}"
            ));
        }
        let normal = packet(tables, raw, "Stats")?;
        let bonded = packet(tables, raw, "BondedStats")?;
        effects
            .entry(core)
            .or_default()
            .push((row, json!({"category":id,"normal":normal,"bonded":bonded})));
    }
    let mut result = Vec::new();
    let mut records = Vec::new();
    let mut excluded = Vec::new();
    let mut aliases = Vec::new();
    let mut alias_runes = Vec::new();
    let mut alias_ids = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut exclusions_used = BTreeSet::new();
    for source_row in 0..tables.len("SoulCores") {
        let raw = tables.row("SoulCores", source_row)?;
        let id = resolve(tables, "BaseItemTypes", &raw["BaseItemType"], "Id")?;
        let name = resolve(tables, "BaseItemTypes", &raw["BaseItemType"], "Name")?;
        if !seen.insert(id.clone()) || !names.insert(name.clone()) {
            return Err(format!("duplicate socketable identity/name {id}"));
        }
        let packets = effects.get(&source_row).cloned().unwrap_or_default();
        let fact_packets: Vec<_> = packets.iter().map(|(_, p)| p.clone()).collect();
        let effects_hash = hash(&json!(fact_packets))?;
        if let Some(expected) = policy.excluded.get(&id) {
            if hash(&json!({"core":raw,"effects":fact_packets,"name":name}))? != *expected {
                return Err(format!("excluded socketable changed: {id}"));
            }
            exclusions_used.insert(id.clone());
            excluded.push(json!({"id":id,"source_row":source_row,"name":name,"reason":"explicit unused source entry","sha256":expected}));
            continue;
        }
        if packets.is_empty() {
            return Err(format!("socketable has no effect packet: {id}"));
        }
        let kind = resolve(tables, "SoulCoreTypes", &raw["Type"], "Id")?;
        let limit = if raw["Limit"].is_null() {
            None
        } else {
            let row = tables.row("SoulCoreLimits", index(&raw["Limit"])?)?;
            let id = text(&row["Id"])?;
            let group = policy
                .limit_groups
                .get(id)
                .ok_or_else(|| format!("unmapped socketable limit group {id}"))?;
            Some((uint(&row["Limit"])?, group.clone()))
        };
        let prototype = RuneSlotDef {
            kind,
            required_level: Some(uint(&raw["RequiredLevel"])?),
            limit: limit.as_ref().map(|x| x.0),
            limit_id: limit.and_then(|x| x.1),
            socket_bound: flag(raw, "IsSocketBound")?,
            can_socket_in_unique_items: flag(raw, "CanSocketInUniqueItems")?,
            can_socket_in_jewellery: flag(raw, "CanSocketInJewellery")?,
            can_socket_in_corrupted_sanctified: flag(raw, "CanSocketInCorruptedSanctified")?,
            lines: vec![],
            rank: vec![],
            stat_order: vec![],
        };
        let mut slots = BTreeMap::<String, RuneSlotDef>::new();
        let mut selected = Vec::new();
        let mut failed = Vec::new();
        for (row, p) in &packets {
            let category = text(&p["category"])?;
            let targets = policy
                .categories
                .get(category)
                .ok_or_else(|| format!("unmapped socketable category {category}"))?;
            if targets.is_empty()
                || targets.iter().any(|x| x.trim().is_empty())
                || targets.iter().collect::<BTreeSet<_>>().len() != targets.len()
            {
                return Err(format!("invalid socketable category mapping {category}"));
            }
            let mut lines = Vec::new();
            for (key, prefix) in [("normal", ""), ("bonded", "Bonded: ")] {
                let values: Vec<(String, i64)> =
                    serde_json::from_value(p[key].clone()).map_err(|e| e.to_string())?;
                match renderer.render(&values) {
                    Ok((text, evidence)) => {
                        lines.extend(text.into_iter().map(|line| {
                            format!(
                                "{prefix}{}",
                                pobr_core::mod_parser::engine::strip_display_markup(&line)
                            )
                        }));
                        selected.push(json!({"source_row":row,"packet":key,"prefix":prefix,"evidence":evidence}));
                    }
                    Err(error) => failed.push(json!({"source_row":row,"packet":key,"error":error})),
                }
            }
            for slot in targets {
                slots
                    .entry(slot.clone())
                    .or_insert_with(|| prototype.clone())
                    .lines
                    .extend(lines.clone());
            }
        }
        let mode = if failed.is_empty() {
            "target-csd"
        } else {
            let binding = policy
                .compatibility
                .get(&id)
                .ok_or_else(|| format!("unrenderable new socketable {id}: {failed:?}"))?;
            if effects_hash != binding.effects_sha256 {
                return Err(format!("unrenderable socketable effects changed: {id}"));
            }
            let prepared = legacy_by_name
                .get(binding.name.as_str())
                .ok_or_else(|| format!("missing socketable compatibility {}", binding.name))?;
            let mut prepared_slots = BTreeMap::new();
            for (key, value) in &prepared.slots {
                let targets = policy
                    .compatibility_slot_aliases
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| vec![key.clone()]);
                if targets.is_empty() {
                    return Err("empty compatibility slot alias".into());
                }
                for target in targets {
                    if prepared_slots.insert(target, value).is_some() {
                        return Err("overlapping compatibility slot aliases".into());
                    }
                }
            }
            if slots.keys().ne(prepared_slots.keys()) {
                return Err(format!("unrenderable socketable slot set changed: {id}"));
            }
            for (key, slot) in &mut slots {
                let old = prepared_slots[key];
                slot.lines.clone_from(&old.lines);
                slot.rank.clone_from(&old.rank);
                slot.stat_order.clone_from(&old.stat_order);
            }
            selected.clear(); // No partially target-rendered socketable is emitted.
            "whole-socketable-compatibility"
        };
        records.push(json!({"id":id,"source_row":source_row,"name":name,"mode":mode,
            "effects_sha256":effects_hash,"packets":packets,"selected_descriptions":selected,"unrenderable":failed}));
        if let Some(alias_names) = policy.import_aliases.get(&id) {
            alias_ids.insert(id.clone());
            if alias_names.is_empty() {
                return Err("empty socketable import aliases".into());
            }
            for alias in alias_names {
                if !legacy_by_name.contains_key(alias.as_str()) {
                    return Err(format!(
                        "socketable import alias has no historical identity: {alias}"
                    ));
                }
                aliases.push(json!({"id":id,"canonical_name":name,"historical_name":alias}));
                alias_runes.push(RuneDef {
                    name: alias.clone(),
                    slots: slots.clone(),
                });
            }
        }
        result.push(RuneDef { name, slots });
    }
    if exclusions_used.len() != policy.excluded.len() {
        return Err("unused socketable exclusion".into());
    }
    if alias_ids.len() != policy.import_aliases.len() {
        return Err("unused socketable import alias".into());
    }
    for alias in alias_runes {
        if !names.insert(alias.name.clone()) {
            return Err("duplicate socketable import alias/name".into());
        }
        result.push(alias);
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Projected {
        runes: result,
        records,
        excluded,
        aliases,
    })
}
