//! Target item grant flags under an explicit, pinned community interpretation.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::item_granted_skills::ItemGrantedSkillsDef;
use serde::Deserialize;
use serde_json::{Value, json};

use super::convert::{column, resolve, text};
use super::stat_set_codec::{policy::hex, read_pinned};
use crate::native_facts::reader::{Tables, VerifiedFacts, selected};
use crate::raw_source::sha256_hex;

mod links;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
pub(super) const TABLES: [&str; 3] = ["ItemInherentSkills", "BaseItemTypes", "SkillGems"];
pub(super) const LINK_TABLES: [&str; 6] = [
    "ItemInherentSkills",
    "BaseItemTypes",
    "SkillGems",
    "Mods",
    "GemEffects",
    "GrantedEffects",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunitySchema {
    repository: String,
    revision: String,
    files: BTreeMap<String, String>,
    spec_table: String,
    spec_field: u32,
    spec_type: String,
    spec_name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    version: String,
    source_sha256: String,
    raw_schema_revision: String,
    raw_field: String,
    raw_source_column: u32,
    interpreted_semantic: String,
    community_schema: CommunitySchema,
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
    args: &super::CodecArgs,
    extra: Option<&selected::Input>,
) -> Result<Converted, String> {
    let pinned = read_pinned(
        &args.policy,
        &args.policy_sha256,
        "item-grant interpretation policy",
    )?;
    let policy: Policy =
        serde_json::from_value(pinned.document.clone()).map_err(|e| e.to_string())?;
    validate_policy(input, args, &policy)?;
    let old = read_pinned(
        &args.source,
        &args.source_sha256,
        "legacy item grant compatibility",
    )?;
    let (mut document, records) = project(input, &old.document)?;
    let links = extra
        .map(|extra| links::project(input, extra, &mut document))
        .transpose()?;
    let legacy_meta = document
        .as_object_mut()
        .ok_or("item grants must be an object")?
        .remove("_meta");
    document["_meta"] = json!({"schema":"item_granted_skills/v1","generator":"pobr-data-adapter ordinary item-grants",
        "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"policy_sha256":args.policy_sha256,
        "reservation_free_bases":"Target IsWeapon flags interpreted as NoReservation only under the pinned community policy; not official field semantics.",
        "other_fields_source_sha256":args.source_sha256,"legacy_metadata_present":legacy_meta.is_some(),"legacy_metadata":legacy_meta});
    let mut receipt = json!({"records":records,"interpretation_policy":pinned.document,
        "legacy_source_sha256":args.source_sha256,"retained_fields":"Every legacy field except reservation_free_bases; original _meta is nested intact, and original bytes are archived.",
        "semantic_boundary":"The raw schema and its IsWeapon name remain unchanged. Community schema names column three NoReservation. This is an explicit compatibility interpretation, not independent official semantic approval or a unique/mod/roll relationship proof."});
    if let Some(links) = links {
        document["_meta"]["variant_links"] = json!({
            "supplement_manifest_sha256":links["supplement_manifest_sha256"],
            "target_fields":["skill_name","granted_effect_id"],
            "compatibility_fields":"variant membership, game_mod_id selectors, item/base names, exported aliases and all unknown fields"});
        receipt["retained_fields"] = json!(
            "Every legacy field except reservation_free_bases and the two target-owned variant fields skill_name/granted_effect_id; original _meta and bytes retained."
        );
        receipt["variant_links"] = links;
    }
    serde_json::from_value::<ItemGrantedSkillsDef>(document.clone()).map_err(|e| e.to_string())?;
    Ok(Converted {
        bytes: super::pretty_bytes(&document)?,
        sidecars: BTreeMap::from([
            ("legacy-item-grants.json".into(), old.bytes),
            ("codec-policy.json".into(), pinned.bytes),
        ]),
        protected_parents: vec![old.parent, pinned.parent],
        receipt,
    })
}

fn validate_policy(
    input: &VerifiedFacts,
    args: &super::CodecArgs,
    p: &Policy,
) -> Result<(), String> {
    let c = &p.community_schema;
    if p.schema != "pobr-ordinary-item-grants-policy/v1"
        || p.version != input.manifest["source_identity"]["game_patch"]
        || p.source_sha256 != args.source_sha256
        || p.raw_schema_revision != input.manifest["schema"]["revision"]
        || p.raw_field != "IsWeapon"
        || p.raw_source_column != 2
        || p.interpreted_semantic != "NoReservation"
        || p.statement.trim().is_empty()
        || c.repository != "https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2"
        || c.spec_table != "iteminherentskills"
        || c.spec_field != 3
        || c.spec_type != "Bool"
        || c.spec_name != "NoReservation"
    {
        return Err("item grant policy identity/interpretation mismatch".into());
    }
    hex(&c.revision, 40, "community revision")?;
    let required = BTreeSet::from([
        "src/Export/spec.lua",
        "src/Export/Scripts/bases.lua",
        "src/Data/Bases/amulet.lua",
    ]);
    if c.files.keys().map(String::as_str).collect::<BTreeSet<_>>() != required {
        return Err("item grant community evidence inventory mismatch".into());
    }
    for hash in c.files.values() {
        hex(hash, 64, "community evidence SHA256")?;
    }
    Ok(())
}

fn project(input: &VerifiedFacts, old: &Value) -> Result<(Value, Vec<Value>), String> {
    serde_json::from_value::<ItemGrantedSkillsDef>(old.clone()).map_err(|e| e.to_string())?;
    let tables = Tables::selected(input, &TABLES)?;
    for (table, field, kind, array, reference) in [
        (
            "ItemInherentSkills",
            "BaseItemType",
            "foreignrow",
            false,
            Some("BaseItemTypes"),
        ),
        (
            "ItemInherentSkills",
            "SkillsGranted",
            "foreignrow",
            true,
            Some("SkillGems"),
        ),
        ("ItemInherentSkills", "IsWeapon", "bool", false, None),
        ("BaseItemTypes", "Id", "string", false, None),
        ("BaseItemTypes", "Name", "string", false, None),
        (
            "SkillGems",
            "BaseItemType",
            "foreignrow",
            false,
            Some("BaseItemTypes"),
        ),
    ] {
        column(
            input,
            table,
            &[field],
            &json!({"name":field,"type":kind,"array":array,"interval":false,"references":reference.map(|t|json!({"table":t}))}),
        )?;
    }
    let columns = &input.documents["tables/ItemInherentSkills.json"]["columns"];
    if columns[2]["source_column"] != 2 || columns[2]["physical_fields"] != json!(["IsWeapon"]) {
        return Err("item grant boolean is not the declared third physical column".into());
    }
    let mut flags = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut records = Vec::new();
    for row in 0..tables.len("ItemInherentSkills") {
        let raw = tables.row("ItemInherentSkills", row)?;
        let id = resolve(&tables, "BaseItemTypes", &raw["BaseItemType"], "Id")?;
        let name = resolve(&tables, "BaseItemTypes", &raw["BaseItemType"], "Name")?;
        let flag = raw["IsWeapon"]
            .as_bool()
            .ok_or("item grant flag must be bool")?;
        if id.is_empty() || flags.insert(id.clone(), flag).is_some() {
            return Err("empty/duplicate inherent item identity".into());
        }
        let skills=raw["SkillsGranted"].as_array().ok_or("granted skills must be an array")?.iter().map(|index|{
            let row=index.as_u64().and_then(|v|usize::try_from(v).ok()).ok_or("invalid skill reference")?;
            let skill=tables.row("SkillGems",row)?;
            Ok(json!({"row":row,"base_item_id":resolve(&tables,"BaseItemTypes",&skill["BaseItemType"],"Id")?}))
        }).collect::<Result<Vec<_>,String>>()?;
        if flag {
            if name.is_empty() {
                return Err("empty reservation-free base name".into());
            }
            names.insert(name.to_owned());
        }
        records.push(json!({"row":row,"base_item_row":raw["BaseItemType"],"base_item_id":id,"base_item_name":name,
            "raw_IsWeapon":flag,"interpreted_NoReservation":flag,"skills":skills,"raw":raw}));
    }
    // A name-only ordinary consumer cannot distinguish equal names with different flags.
    for row in 0..tables.len("BaseItemTypes") {
        let base = tables.row("BaseItemTypes", row)?;
        if names.contains(text(&base["Name"])?) && flags.get(text(&base["Id"])?) != Some(&true) {
            return Err("ambiguous reservation ownership for shared base name".into());
        }
    }
    let mut document = old.clone();
    document
        .as_object_mut()
        .ok_or("item grants must be object")?
        .insert("reservation_free_bases".into(), json!(names));
    Ok((document, records))
}
