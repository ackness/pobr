//! Relinquish overrides already owned by target per-ID base facts.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::{BaseItemDef, BaseItemOverridesDef};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::native_facts::reader::{self, VerifiedFacts};
use crate::ordinary_facts::{CodecArgs, stat_set_codec::read_pinned};

const OWNED: [&str; 6] = [
    "req_str",
    "req_dex",
    "req_int",
    "block_chance",
    "spirit",
    "reload_time_ms",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    version: String,
    source_sha256: String,
    source_metadata: Value,
    delegated_fields: Vec<String>,
    #[serde(default)]
    tag_ownership: Option<TagOwnership>,
    statement: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TagOwnership {
    inheritance_manifest_sha256: String,
    interpretation_policy_id: String,
}

pub(in crate::ordinary_facts) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub source_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub protected_parents: [PathBuf; 2],
    pub receipt: Value,
}

pub(in crate::ordinary_facts) fn convert(
    input: &VerifiedFacts,
    bases: &[BaseItemDef],
    args: &CodecArgs,
) -> Result<Converted, String> {
    convert_with_inheritance(input, bases, args, None)
}

pub(in crate::ordinary_facts) fn convert_with_inheritance(
    input: &VerifiedFacts,
    bases: &[BaseItemDef],
    args: &CodecArgs,
    inheritance: Option<&super::inheritance::Converted>,
) -> Result<Converted, String> {
    let pinned = read_pinned(
        &args.policy,
        &args.policy_sha256,
        "base-item ownership policy",
    )?;
    let policy: Policy = serde_json::from_value(pinned.document).map_err(|e| e.to_string())?;
    let mut owned: Vec<&str> = OWNED.to_vec();
    let tags = match (&policy.tag_ownership, inheritance) {
        (None, None) if policy.schema == "pobr-base-item-field-ownership/v1" => false,
        (Some(p), Some(source))
            if policy.schema == "pobr-base-item-field-ownership/v2"
                && p.inheritance_manifest_sha256 == source.manifest_sha256
                && p.interpretation_policy_id == super::inheritance::POLICY =>
        {
            true
        }
        _ => return Err("base-item inheritance policy/source ownership mismatch".into()),
    };
    if tags {
        owned.push("tags");
    }
    if policy.version != input.manifest["source_identity"]["game_patch"]
        || policy.source_sha256 != args.source_sha256
        || policy.delegated_fields != owned
        || policy.statement.trim().is_empty()
    {
        return Err("base-item ownership policy identity/fields mismatch".into());
    }
    let source = read_pinned(
        &args.source,
        &args.source_sha256,
        "base-item compatibility source",
    )?;
    if source.document["_meta"] != policy.source_metadata {
        return Err("base-item compatibility metadata mismatch".into());
    }
    let dto: BaseItemOverridesDef =
        serde_json::from_value(source.document.clone()).map_err(|e| e.to_string())?;
    let source_rows: BTreeMap<&str, usize> = input.documents["tables/BaseItemTypes.json"]["rows"]
        .as_array()
        .ok_or("missing base source rows")?
        .iter()
        .enumerate()
        .filter_map(|(row, v)| v["values"]["Id"].as_str().map(|id| (id, row)))
        .collect();
    let mut by_name: BTreeMap<&str, Vec<&BaseItemDef>> = BTreeMap::new();
    for base in bases {
        by_name.entry(&base.name).or_default().push(base);
    }
    let mut document = source.document.clone();
    let output = document["overrides"]
        .as_array_mut()
        .ok_or("missing overrides")?;
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for (row, (entry, raw)) in dto.overrides.iter().zip(output).enumerate() {
        if entry.name.is_empty() || !seen.insert(&entry.name) {
            return Err("empty/duplicate base-item compatibility name".into());
        }
        let mut removed = BTreeMap::new();
        let mut targets = Vec::new();
        if let Some(matches) = by_name.get(entry.name.as_str()) {
            // Do not choose one numeric value by English name. Removing the
            // shared override lets each stable base ID retain its own facts.
            for field in OWNED {
                if let Some(value) = raw
                    .as_object_mut()
                    .ok_or("invalid override entry")?
                    .remove(field)
                {
                    removed.insert(field, value);
                }
            }
            for base in matches {
                targets.push(json!({"id":base.id,"name":base.name,"source_row":source_rows.get(base.id.as_str()).ok_or("target base has no source identity")?,
                    "req_str":base.req_str,"req_dex":base.req_dex,"req_int":base.req_int,
                    "block_chance":base.armour.as_ref().and_then(|a|a.block_chance),
                    "spirit":base.spirit,"reload_time_ms":base.weapon.as_ref().and_then(|w|w.reload_time_ms)}));
            }
            if let Some(inheritance) = inheritance {
                let covered = matches
                    .iter()
                    .filter(|base| inheritance.owned_ids.contains(&base.id))
                    .count();
                if covered != 0 && covered != matches.len() {
                    return Err(format!(
                        "base-item name {} mixes resolved and unresolved inherited tags",
                        entry.name
                    ));
                }
                if covered == matches.len()
                    && let Some(value) = raw
                        .as_object_mut()
                        .ok_or("invalid override entry")?
                        .remove("tags")
                {
                    removed.insert("tags", value);
                }
            }
        }
        let numeric_removed: BTreeMap<_, _> = OWNED
            .into_iter()
            .filter_map(|field| removed.get(field).map(|value| (field, value)))
            .collect();
        records.push(json!({"source_row":row,"name":entry.name,"source_pointer":format!("/overrides/{row}"),
            "mode":if targets.is_empty(){"unmatched-complete-compatibility"}else{"target-base-fields"},
            "removed_legacy_numeric_fields":numeric_removed,
            "removed_legacy_tags":removed.get("tags"),"target_bases":targets,
            "retained_entry_sha256":crate::raw_source::sha256_hex(&reader::json_bytes(raw)?)}));
    }
    Ok(Converted {
        overlay: BTreeMap::from([(
            "base_item_overrides.json".into(),
            reader::json_bytes(&document)?,
        )]),
        source_bytes: source.bytes,
        policy_bytes: pinned.bytes,
        protected_parents: [source.parent, pinned.parent],
        receipt: json!({"source_sha256":args.source_sha256,"policy_sha256":args.policy_sha256,
            "target_owned_fields":owned,"records":records,
            "ownership":"Target BaseItemDef supplies per-ID values and absence; legacy numeric overrides are removed for matched bases. Under v2 only, tags are removed when every same-name target has resolved inheritance; mixed ownership is rejected.",
            "compatibility":"All other JSON fields, unmatched entries and unresolved inherited tags remain explicit compatibility, including socket limits/subtype/charm effects. They are not relabeled as target facts."}),
    })
}
