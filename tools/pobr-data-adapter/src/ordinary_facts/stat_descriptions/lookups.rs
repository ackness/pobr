//! Pinned physical joins for the data-dependent CSD formatters used in PoE2.
use super::super::passive_tree::render::LookupValues;
use super::*;
use crate::native_facts::reader::selected;
use crate::ordinary_facts::convert::{resolve, text};

const TABLES: [&str; 9] = [
    "BaseItemTypes",
    "ItemClasses",
    "PassiveKeystoneList",
    "PassiveSkills",
    "PassiveTreeExpansionJewelSizes",
    "SkillGems",
    "SkillGemsForUniqueStat",
    "UltimatumWagerTypes",
    "UniqueMagesLegacy",
];

pub(super) struct Lookups {
    pub values: LookupValues,
    pub receipt: Value,
}

pub(super) fn read(input: &selected::Input) -> Result<Lookups, String> {
    let tables = input.tables(&TABLES)?;
    for (table, field, kind, array, reference) in [
        ("BaseItemTypes", "Name", "string", false, None),
        ("ItemClasses", "Name", "string", false, None),
        ("PassiveKeystoneList", "DisplayText", "string", false, None),
        ("PassiveSkills", "Name", "string", false, None),
        ("PassiveSkills", "PassiveSkillGraphId", "u16", false, None),
        (
            "PassiveTreeExpansionJewelSizes",
            "Name",
            "string",
            false,
            None,
        ),
        (
            "SkillGems",
            "BaseItemType",
            "foreignrow",
            false,
            Some("BaseItemTypes"),
        ),
        ("SkillGemsForUniqueStat", "Index", "i32", false, None),
        (
            "SkillGemsForUniqueStat",
            "SkillGems",
            "foreignrow",
            true,
            Some("SkillGems"),
        ),
        ("UltimatumWagerTypes", "HASH16", "u16", false, None),
        ("UltimatumWagerTypes", "DisplayText", "string", false, None),
        ("UniqueMagesLegacy", "DisplayText", "string", false, None),
    ] {
        input.column(
            table,
            field,
            &json!({"name":field,"type":kind,"array":array,"interval":false,
            "references":reference.map(|table|json!({"table":table}))}),
        )?;
    }
    let mut values = LookupValues::new();
    let mut records = Vec::new();
    for (handler, table, field, start, key_field) in [
        ("mod_value_to_item_class", "ItemClasses", "Name", 0, None),
        (
            "passive_keystone_index",
            "PassiveKeystoneList",
            "DisplayText",
            1,
            None,
        ),
        (
            "tree_expansion_jewel_passive",
            "PassiveTreeExpansionJewelSizes",
            "Name",
            0,
            None,
        ),
        (
            "mages_legacy_index",
            "UniqueMagesLegacy",
            "DisplayText",
            1,
            None,
        ),
        (
            "ultimatum_wager_type_hash",
            "UltimatumWagerTypes",
            "DisplayText",
            0,
            Some("HASH16"),
        ),
        (
            "passive_hash",
            "PassiveSkills",
            "Name",
            0,
            Some("PassiveSkillGraphId"),
        ),
    ] {
        let mut mapping = BTreeMap::new();
        for row in 0..tables.len(table) {
            let raw = tables.row(table, row)?;
            let key = if let Some(field) = key_field {
                raw[field].as_i64().ok_or("invalid lookup integer key")?
            } else {
                i64::try_from(row).map_err(|_| "lookup row overflow")? + start
            };
            let raw_label = text(&raw[field])?;
            let label = pobr_core::mod_parser::engine::strip_display_markup(raw_label);
            if mapping.insert(key, label.clone()).is_some() {
                return Err(format!("duplicate {handler} lookup key {key}"));
            }
            let mut keys = vec![key];
            if handler == "passive_hash" && key >= 32768 {
                let signed = key
                    .checked_sub(65536)
                    .ok_or("signed passive hash overflow")?;
                if mapping.insert(signed, label.clone()).is_some() {
                    return Err("duplicate signed passive hash".into());
                }
                keys.push(signed);
            }
            records.push(
                json!({"handler":handler,"table":table,"row":row,"keys":keys,"raw_literal":raw_label,"literal":label,
                "source_row_sha256":sha256_hex(&reader::json_bytes(raw)?)}),
            );
        }
        values.insert(handler.into(), mapping);
    }
    let mut skills = BTreeMap::new();
    for row in 0..tables.len("SkillGemsForUniqueStat") {
        let raw = tables.row("SkillGemsForUniqueStat", row)?;
        let key = raw["Index"]
            .as_i64()
            .ok_or("invalid specific-skill Index")?;
        let mut labels = Vec::new();
        let mut joins = Vec::new();
        for reference in raw["SkillGems"]
            .as_array()
            .ok_or("invalid specific-skill array")?
        {
            let skill_row = reference
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or("invalid skill FK")?;
            let skill = tables.row("SkillGems", skill_row)?;
            let label = resolve(&tables, "BaseItemTypes", &skill["BaseItemType"], "Name")?;
            labels.push(label);
            joins.push(json!({"skill_row":skill_row,"base_item_row":skill["BaseItemType"]}));
        }
        let raw_label = labels.join(" and ");
        let label = pobr_core::mod_parser::engine::strip_display_markup(&raw_label);
        if skills.insert(key, label.clone()).is_some() {
            return Err("duplicate specific-skill Index".into());
        }
        records.push(json!({"handler":"specific_skill","table":"SkillGemsForUniqueStat","row":row,
            "keys":[key],"raw_literal":raw_label,"literal":label,"joins":joins,"source_row_sha256":sha256_hex(&reader::json_bytes(raw)?)}));
    }
    values.insert("specific_skill".into(), skills);
    Ok(Lookups {
        receipt: json!({"manifest_sha256":sha256_hex(&input.manifest_bytes),"manifest":input.manifest,
        "records":records,"values_sha256":sha256_hex(&reader::json_bytes(&serde_json::to_value(&values).map_err(|e|e.to_string())?)?),
        "policy":"Pinned PyPoE66d1 PoE2 relational formatter keys/fields; complete source key-to-literal maps. Empty/unknown literals reject rendering; no PoE1-only handler is inferred."}),
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Value {
        let specs = [
            ("BaseItemTypes", json!([{"Name":"Flame"},{"Name":"Frost"}])),
            ("ItemClasses", json!([{"Name":"Rings"}])),
            ("PassiveKeystoneList", json!([{"DisplayText":"[Bond]"}])),
            (
                "PassiveSkills",
                json!([{"Name":"Signed passive","PassiveSkillGraphId":32768},{"Name":"Other","PassiveSkillGraphId":1}]),
            ),
            ("PassiveTreeExpansionJewelSizes", json!([{"Name":"Small"}])),
            ("SkillGems", json!([{"BaseItemType":1},{"BaseItemType":0}])),
            (
                "SkillGemsForUniqueStat",
                json!([{"Index":7,"SkillGems":[1,0]}]),
            ),
            (
                "UltimatumWagerTypes",
                json!([{"HASH16":5,"DisplayText":"20 Orbs"}]),
            ),
            ("UniqueMagesLegacy", json!([{"DisplayText":"Amethyst"}])),
        ];
        let mut docs = serde_json::Map::new();
        for (table, rows) in specs {
            let fields = rows[0]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            let columns = fields.iter().enumerate().map(|(i,field)| {
                let (kind,array,target) = match (table,field.as_str()) {
                    ("SkillGems","BaseItemType") => ("foreignrow",false,Some("BaseItemTypes")),
                    ("SkillGemsForUniqueStat","SkillGems") => ("foreignrow",true,Some("SkillGems")),
                    (_,"Index") => ("i32",false,None),
                    (_,"HASH16"|"PassiveSkillGraphId") => ("u16",false,None),
                    _ => ("string",false,None),
                };
                json!({"source_column":i,"physical_fields":[field],"schema":{"name":field,"type":kind,
                    "array":array,"interval":false,"references":target.map(|table|json!({"table":table}))}})
            }).collect::<Vec<_>>();
            let rows = rows
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(row, v)| json!({"_row":row,"values":v}))
                .collect::<Vec<_>>();
            docs.insert(table.into(),json!({"columns":columns,"physical_fields":fields,
                "raw_resource":format!("Data/Balance/{table}.datc64"),"row_key":"_row","rows":rows,"table":table,
                "validation":{"rows":rows.len(),"warnings":[],"schema_row_bytes":1,"actual_row_bytes":1}}));
        }
        Value::Object(docs)
    }
    #[test]
    fn verified_physical_joins_keep_order_signed_keys_and_change_with_source() {
        let f = super::super::super::tests::Fixture::new();
        let mut source = source();
        let args = super::super::super::tests::supplement(&f, &source);
        let input = selected::open(&args.path, &args.sha256, &f.manifest).unwrap();
        let lookups = read(&input).unwrap();
        assert_eq!(lookups.values["passive_hash"][&-32768], "Signed passive");
        assert_eq!(lookups.values["passive_hash"][&32768], "Signed passive");
        assert_eq!(lookups.values["specific_skill"][&7], "Flame and Frost");
        assert_eq!(lookups.values["passive_keystone_index"][&1], "Bond");
        assert!(!lookups.values["passive_keystone_index"].contains_key(&0));
        source["BaseItemTypes"]["rows"][0]["values"]["Name"] = json!("Changed");
        let mut changed = input;
        changed.documents.insert(
            "tables/BaseItemTypes.json".into(),
            source["BaseItemTypes"].clone(),
        );
        let after = read(&changed).unwrap();
        assert_eq!(after.values["specific_skill"][&7], "Changed and Frost");
        assert_ne!(
            lookups.receipt["values_sha256"],
            after.receipt["values_sha256"]
        );
        changed.documents.get_mut("tables/SkillGems.json").unwrap()["rows"][0]["values"]["BaseItemType"] =
            json!(99);
        assert!(
            read(&changed)
                .err()
                .unwrap()
                .contains("unresolved physical row")
        );
    }
}
