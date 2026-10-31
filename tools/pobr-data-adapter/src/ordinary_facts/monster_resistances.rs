//! Authenticated monster facts through the original normal-rarity resistance join.

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::monsters::{self, RawMonsterType, RawResistance, RawVariety};
use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 3] = ["MonsterVarieties", "MonsterTypes", "MonsterResistances"];
const NORMAL_COLUMNS: [&str; 4] = ["Fire1", "Cold1", "Lightning1", "Chaos1"];

pub(super) struct Converted {
    pub base: BTreeMap<String, Vec<u8>>,
    pub receipt: Value,
}

pub(super) fn convert(input: &VerifiedFacts) -> Result<Converted, String> {
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    // Deserialize the original typed rows in memory, from verified values only.
    // No raw file loader, replacement parser or temporary raw directory is used.
    let varieties: Vec<RawVariety> = rows(&tables, "MonsterVarieties")?;
    let types: Vec<RawMonsterType> = rows(&tables, "MonsterTypes")?;
    let resistances: Vec<RawResistance> = rows(&tables, "MonsterResistances")?;
    // Validate references on unused types too; ordered occurrences are not keys.
    for (source_row, monster_type) in types.iter().enumerate() {
        for (position, &row) in monster_type.resistances.iter().enumerate() {
            tables.row("MonsterResistances", row).map_err(|e| {
                format!("MonsterTypes/{source_row}.MonsterResistances[{position}]: {e}")
            })?;
        }
    }
    let mut records = Vec::new();
    for (source_row, variety) in varieties.iter().enumerate() {
        let type_row = tables
            .row("MonsterTypes", variety.monster_type)
            .map_err(|e| format!("MonsterVarieties/{source_row}.MonsterType: {e}"))?;
        let mut occurrences = Vec::new();
        for (position, &row) in types[variety.monster_type].resistances.iter().enumerate() {
            let values = tables.row("MonsterResistances", row)?;
            // Preserve exact singleton arrays in the receipt; adapt_rows validates
            // their length and computes all sums before any output can be reserved.
            let normal: BTreeMap<_, _> = NORMAL_COLUMNS.iter().map(|&c| (c, &values[c])).collect();
            occurrences.push(json!({
                "position":position,"source_row":row,"id":values["Id"],"normal_values":normal
            }));
        }
        records.push(json!({
            "id":variety.id,"variety_source_row":source_row,
            "monster_type":{"source_row":variety.monster_type,"id":type_row["Id"]},
            "resistance_occurrences":occurrences
        }));
    }
    // This is the same join as the old raw wrapper, including singleton checks,
    // all ordered references, signed values and known zeros for empty lists.
    let data = monsters::adapt_rows(varieties, &types, resistances)?;
    let inputs: BTreeMap<_, _> = TABLES
        .iter()
        .map(|table| {
            let path = format!("tables/{table}.json");
            let hash = sha256_hex(&input.bytes[&path]);
            (path, hash)
        })
        .collect();
    let document = json!({
        "_meta":{
            "schema":"monster-resistances/v1",
            "generator":"pobr-data-adapter ordinary-facts monster-resistances",
            "poe_version":input.manifest["source_identity"]["game_patch"],
            "source_identity":input.manifest["source_identity"],
            "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"inputs":inputs
        },"monsters":data.monsters
    });
    let receipt = json!({
        "monsters":data.monsters.len(),"source_records":records,
        "mapping":{
            "identity":"MonsterVarieties.Id, no monster filtering; physical _row is a foreign key, not the stable ID or _index",
            "join":"MonsterVarieties.MonsterType -> MonsterTypes.MonsterResistances -> every ordered MonsterResistances reference occurrence",
            "normal_values":"Fire1/Cold1/Lightning1/Chaos1 must each contain one signed i32 value; sum in source reference order with checked i32 addition",
            "empty_list":"explicit empty MonsterResistances list produces known base zeros; null/missing/invalid facts are errors"
        },
        "unconsumed_source":unmapped(input,&tables)?,
        "scope":"normal-rarity base resistances only, in existing DTO percentage points; no new race/rarity/level or gameplay model",
        "pending":"minion/spectre catalog values, minionList/loadouts, totem catalog and other rarity/level semantics are not certified by this file; complete target package and ordinary BD/Web acceptance remain separate"
    });
    Ok(Converted {
        base: BTreeMap::from([(
            "monster_resistances.json".into(),
            super::pretty_bytes(&document)?,
        )]),
        receipt,
    })
}

fn rows<T: DeserializeOwned>(tables: &Tables<'_>, table: &str) -> Result<Vec<T>, String> {
    (0..tables.len(table))
        .map(|row| {
            serde_json::from_value(tables.row(table, row)?.clone())
                .map_err(|e| format!("{table}/{row}: {e}"))
        })
        .collect()
}

fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for table in TABLES {
        super::convert::column(
            input,
            table,
            &["Id"],
            &json!({
                "name":"Id","type":"string","array":false,"interval":false,"references":null
            }),
        )?;
    }
    for (table, field, array, target) in [
        ("MonsterVarieties", "MonsterType", false, "MonsterTypes"),
        (
            "MonsterTypes",
            "MonsterResistances",
            true,
            "MonsterResistances",
        ),
    ] {
        super::convert::column(
            input,
            table,
            &[field],
            &json!({
                "name":field,"type":"foreignrow","array":array,"interval":false,"references":{"table":target}
            }),
        )?;
    }
    for field in NORMAL_COLUMNS {
        super::convert::column(
            input,
            "MonsterResistances",
            &[field],
            &json!({
                "name":field,"type":"i32","array":true,"interval":false,"references":null
            }),
        )?;
    }
    Ok(())
}

fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for table in TABLES {
        let used: &[&str] = match table {
            "MonsterVarieties" => &["Id", "MonsterType"],
            "MonsterTypes" => &["Id", "MonsterResistances"],
            _ => &["Id", "Fire1", "Cold1", "Lightning1", "Chaos1"],
        };
        let doc = &input.documents[&format!("tables/{table}.json")];
        let columns: Vec<_> = doc["columns"]
            .as_array()
            .ok_or("missing columns")?
            .iter()
            .filter(|c| {
                c["physical_fields"].as_array().is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|f| !used.iter().any(|name| f.as_str() == Some(*name)))
                })
            })
            .collect();
        result.push(json!({
            "table":table,"payload":format!("tables/{table}.json"),"columns":columns,
            "source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "identity":"Id in each pinned physical row; source_records locates every output variety, type and ordered resistance occurrence",
            "status":"other columns/rarities/flags/level semantics are not applied by this domain, not declared effectless"
        }));
    }
    Ok(result)
}
