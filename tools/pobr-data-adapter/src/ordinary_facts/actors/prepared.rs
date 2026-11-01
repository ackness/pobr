//! Existing actor fields from target relationships and explicitly declared vocabulary.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::projection::Projected;
use crate::native_facts::reader::{Tables, VerifiedFacts, selected};
use crate::ordinary_facts::convert::column;
use crate::raw_source::sha256_hex;

pub(super) const FIELDS: [&str; 4] = [
    "monster_category",
    "weapon_type1",
    "weapon_type2",
    "base_damage_ignores_attack_speed",
];
const FLAG_MEANING: &str = "bb52-MonsterTypes-column8-Flag0-as-BaseDamageIgnoresAttackSpeed/v1";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    category_manifest_sha256: String,
    category_vocabulary: BTreeMap<String, String>,
    weapon_vocabulary: BTreeMap<String, String>,
    non_weapon_classes: Vec<String>,
    damage_flag_interpretation: String,
    community_spec_sha256: String,
}

impl Policy {
    pub fn validate(&self) -> Result<(), String> {
        super::super::stat_set_codec::policy::hex(
            &self.category_manifest_sha256,
            64,
            "category manifest",
        )?;
        let category: BTreeMap<String, String> = [
            ("eldritch", "Eldritch"),
            ("demon", "Demon"),
            ("construct", "Construct"),
            ("undead", "Undead"),
            ("beast", "Beast"),
            ("humanoid", "Humanoid"),
        ]
        .into_iter()
        .map(|(a, b)| (a.into(), b.into()))
        .collect();
        let mut weapon: BTreeMap<String, String> = [
            "Claw",
            "Dagger",
            "Wand",
            "One Hand Sword",
            "One Hand Axe",
            "One Hand Mace",
            "Crossbow",
            "Bow",
            "Staff",
            "Warstaff",
            "Two Hand Sword",
            "Two Hand Axe",
            "Two Hand Mace",
            "Shield",
            "Flail",
            "Spear",
            "Talisman",
        ]
        .into_iter()
        .map(|s| (s.into(), s.into()))
        .collect();
        weapon.extend([
            ("FishingRod".into(), "Fishing Rod".into()),
            ("Sceptre".into(), "One Hand Mace".into()),
            ("Unarmed".into(), "None".into()),
        ]);
        if self.category_vocabulary != category
            || self.weapon_vocabulary != weapon
            || self.non_weapon_classes != ["Quiver"]
            || self.damage_flag_interpretation != FLAG_MEANING
            || self.community_spec_sha256
                != "4df9f94a1acc8fa8f4e505db7fc8a281e3e2f1b35a2d969e4f99ef1880d5ed4b"
        {
            return Err("unsupported prepared actor vocabulary/flag interpretation".into());
        }
        Ok(())
    }
}

pub(super) fn project(
    input: &VerifiedFacts,
    tables: &Tables<'_>,
    extra: &selected::Input,
    variety_row: usize,
    policy: &Policy,
) -> Result<Projected, String> {
    if sha256_hex(&extra.manifest_bytes) != policy.category_manifest_sha256 {
        return Err("prepared actor category manifest differs from policy".into());
    }
    let categories = extra.tables(&["MonsterCategories"])?;
    for field in ["MainHand_ItemClass", "OffHand_ItemClass"] {
        column(
            input,
            "MonsterVarieties",
            &[field],
            &json!({"name":field,"type":"foreignrow","array":false,"interval":false,"references":{"table":"ItemClasses"}}),
        )?;
    }
    column(
        input,
        "ItemClasses",
        &["Id"],
        &json!({"name":"Id","type":"string","array":false,"interval":false,"references":null}),
    )?;
    column(
        input,
        "MonsterVarieties",
        &["MonsterCategory"],
        &json!({"name":"MonsterCategory","type":"foreignrow","array":false,"interval":false,"references":{"table":"MonsterCategories"}}),
    )?;
    column(
        input,
        "MonsterTypes",
        &["Flag0"],
        &json!({"name":null,"type":"bool","array":false,"interval":false,"references":null}),
    )?;
    let flags = input.documents["tables/MonsterTypes.json"]["columns"]
        .as_array()
        .ok_or("missing monster type columns")?;
    if !flags
        .iter()
        .any(|c| c["source_column"] == 8 && c["physical_fields"] == json!(["Flag0"]))
    {
        return Err(
            "MonsterTypes.Flag0 no longer occupies declared community boolean position".into(),
        );
    }
    extra.column("MonsterCategories","Tag",&json!({"name":"Tag","type":"foreignrow","array":false,"interval":false,"references":{"table":"Tags"}}))?;
    // Name is retained for display provenance, never used as an engine category ID.
    extra.column("MonsterCategories","Name",&json!({"name":"Name","type":"string","array":false,"interval":false,"references":null,"localized":true}))?;
    let mut category_rows = Vec::new();
    for row in 0..categories.len("MonsterCategories") {
        let raw = categories.row("MonsterCategories", row)?;
        let tag_row = reference(tables, "Tags", &raw["Tag"])?;
        let tag = tables.row("Tags", tag_row)?["Id"]
            .as_str()
            .ok_or("category tag Id must be string")?;
        if !raw["Name"].is_string() {
            return Err("category Name must retain its source string".into());
        }
        category_rows
            .push(json!({"source_row":row,"tag_source_row":tag_row,"tag_id":tag,"raw":raw}));
    }
    let variety = tables.row("MonsterVarieties", variety_row)?;
    let mut values = serde_json::Map::new();
    let mut weapons = Vec::new();
    for (field, output) in [
        ("MainHand_ItemClass", "weapon_type1"),
        ("OffHand_ItemClass", "weapon_type2"),
    ] {
        let mut evidence = json!({"field":field,"raw":variety[field],"output_field":output});
        if !variety[field].is_null() {
            let row = reference(tables, "ItemClasses", &variety[field])?;
            let id = tables.row("ItemClasses", row)?["Id"]
                .as_str()
                .ok_or("item class Id must be string")?;
            evidence["source_row"] = json!(row);
            evidence["id"] = json!(id);
            if let Some(name) = policy.weapon_vocabulary.get(id) {
                values.insert(output.into(), json!(name));
                evidence["status"] = json!("explicit-engine-weapon-vocabulary");
            } else if policy.non_weapon_classes.iter().any(|s| s == id) {
                evidence["status"] = json!("declared-non-weapon-class; raw identity retained");
            } else {
                return Err(format!(
                    "unmapped actor item class {id}; explicit vocabulary adaptation required"
                ));
            }
        } else {
            evidence["status"] = json!("source-absent");
        }
        weapons.push(evidence);
    }
    let category = if variety["MonsterCategory"].is_null() {
        json!({"status":"source-absent","raw":null})
    } else {
        let row = reference(
            &categories,
            "MonsterCategories",
            &variety["MonsterCategory"],
        )?;
        let evidence = &category_rows[row];
        let tag = evidence["tag_id"]
            .as_str()
            .ok_or("missing category tag Id")?;
        let output = policy
            .category_vocabulary
            .get(tag)
            .map(String::as_str)
            .unwrap_or(tag);
        values.insert("monster_category".into(), json!(output));
        json!({"status":if policy.category_vocabulary.contains_key(tag){"explicit-engine-category-vocabulary"}else{"unknown-stable-tag-retained-as-other"},
            "raw_reference":variety["MonsterCategory"],"source":evidence,"output":output})
    };
    let type_row = reference(tables, "MonsterTypes", &variety["MonsterType"])?;
    let flag = tables.row("MonsterTypes", type_row)?["Flag0"]
        .as_bool()
        .ok_or("MonsterTypes.Flag0 must be a boolean")?;
    if flag {
        values.insert("base_damage_ignores_attack_speed".into(), json!(true));
    }
    Ok(Projected {
        values,
        receipt: json!({"weapons":weapons,"category":category,"category_rows":category_rows,
        "category_manifest":extra.manifest,"category_manifest_sha256":policy.category_manifest_sha256,
        "damage_flag":{"table":"MonsterTypes","source_row":type_row,"raw_column":"Flag0","source_column":8,"raw":flag,
            "interpretation":policy.damage_flag_interpretation,"authority":"fixed-community-schema-interpretation","official_semantics_certified":false,
            "schema_source":"vendor/PathOfBuilding-PoE2/src/Export/spec.lua","community_spec_sha256":policy.community_spec_sha256},
        "scope":"Name/Icon/Unknown0 stay raw. Engine category/weapon strings are explicit representation conversion, not client field names. The boolean uses a separately disclosed community interpretation; no actor OT/AI/summoning rules are inferred."}),
    })
}

fn reference(tables: &Tables<'_>, table: &str, value: &Value) -> Result<usize, String> {
    let row = value
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("actor physical reference must be usize")?;
    tables.row(table, row)?;
    Ok(row)
}
