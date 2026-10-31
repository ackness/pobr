//! Physical Stats/Mods columns to the unchanged ordinary catalog DTOs.

use std::collections::BTreeSet;

use pobr_data::catalog::{ModDef, ModStat, SpawnWeight, StatDef};
use serde::Serialize;
use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};

pub(super) const TABLES: [&str; 5] = ["Stats", "Mods", "ModType", "ModFamily", "Tags"];
const STAT_SLOTS: usize = 8;

#[derive(Serialize)]
pub(super) struct OmittedMod {
    id: String,
    source_row: usize,
}

#[derive(Serialize)]
pub(super) struct UnassignedRange {
    id: String,
    source_row: usize,
    slot: usize,
    min: i64,
    max: i64,
}

pub(super) struct Converted {
    pub stats: Vec<StatDef>,
    pub mods: Vec<ModDef>,
    pub filtered_shells: Vec<OmittedMod>,
    pub unassigned_ranges: Vec<UnassignedRange>,
}

pub(super) fn convert(input: &VerifiedFacts) -> Result<Converted, String> {
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    let mut stats = Vec::new();
    for index in 0..tables.len("Stats") {
        let row = tables.row("Stats", index)?;
        let stat = (|| {
            Ok(StatDef {
                id: text(&row["Id"])?.into(),
                is_local: row["IsLocal"].as_bool().ok_or("IsLocal must be boolean")?,
                semantic: optional_u32(&row["Semantic"])?,
                category: optional_u32(&row["Category"])?,
            })
        })()
        .map_err(|error: String| format!("Stats/{index}: {error}"))?;
        stats.push(stat);
    }
    // ModType has Name, not Id. It is a group label and never a Families lookup.
    let mut names = BTreeSet::new();
    for index in 0..tables.len("ModType") {
        let name = text(&tables.row("ModType", index)?["Name"])?;
        if name.is_empty() || !names.insert(name) {
            return Err(format!("ModType/{index}: empty or duplicate group Name"));
        }
    }
    let mut result = Converted {
        stats,
        mods: Vec::new(),
        filtered_shells: Vec::new(),
        unassigned_ranges: Vec::new(),
    };
    for index in 0..tables.len("Mods") {
        let row = tables.row("Mods", index)?;
        let converted = convert_mod(&tables, row, index, &mut result.unassigned_ranges)
            .map_err(|error| format!("Mods/{index} ({}): {error}", row["Id"]))?;
        // Match the ordinary converter's shell filter, after validating every used field.
        if converted.name.is_none() && converted.stats.is_empty() {
            result.filtered_shells.push(OmittedMod {
                id: converted.id,
                source_row: index,
            });
        } else {
            result.mods.push(converted);
        }
    }
    result.stats.sort_by(|a, b| a.id.cmp(&b.id));
    result.mods.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

fn convert_mod(
    tables: &Tables<'_>,
    row: &Value,
    source_row: usize,
    unassigned: &mut Vec<UnassignedRange>,
) -> Result<ModDef, String> {
    let id = text(&row["Id"])?;
    let mut stats = Vec::new();
    for slot in 1..=STAT_SLOTS {
        let field = format!("Stat{slot}");
        let min = row[format!("{field}Min")]
            .as_i64()
            .ok_or_else(|| format!("{field}Min must be i64"))?;
        let max = row[format!("{field}Max")]
            .as_i64()
            .ok_or_else(|| format!("{field}Max must be i64"))?;
        if min > max {
            return Err(format!("{field}: bad range {min}..{max}"));
        }
        if row[&field].is_null() {
            // The physical source contains unused nonzero endpoints. They do not identify a
            // stat; preserve them in the receipt instead of assigning them to Stats row zero.
            if min != 0 || max != 0 {
                unassigned.push(UnassignedRange {
                    id: id.into(),
                    source_row,
                    slot,
                    min,
                    max,
                });
            }
        } else {
            stats.push(ModStat {
                stat_id: resolve(tables, "Stats", &row[&field], "Id")
                    .map_err(|error| format!("{field}: {error}"))?,
                min,
                max,
            });
        }
    }
    let mod_type = optional_u32(&row["ModType"])?;
    let group = if mod_type.is_some() {
        Some(resolve(tables, "ModType", &row["ModType"], "Name")?)
    } else {
        None
    };
    let families = if row["Families"].is_null() {
        None
    } else {
        Some(
            resolve_list(tables, "ModFamily", &row["Families"])
                .map_err(|error| format!("Families: {error}"))?,
        )
    };
    let tags =
        resolve_list(tables, "Tags", &row["Tags"]).map_err(|error| format!("Tags: {error}"))?;
    let weight_tags = array(&row["SpawnWeight_Tags"])?;
    let weights = array(&row["SpawnWeight_Values"])?;
    if weight_tags.len() != weights.len() {
        return Err("SpawnWeight_Tags/Values lengths differ".into());
    }
    let spawn_weights = weight_tags
        .iter()
        .zip(weights)
        .map(|(tag, weight)| {
            Ok(SpawnWeight {
                tag: resolve(tables, "Tags", tag, "Id")?,
                weight: u32_value(weight)?,
            })
        })
        .collect::<Result<_, String>>()
        .map_err(|error| format!("SpawnWeight: {error}"))?;
    let name = match &row["Name"] {
        Value::Null => None,
        value => Some(text(value)?)
            .filter(|name| !name.is_empty())
            .map(String::from),
    };
    Ok(ModDef {
        id: id.into(),
        name,
        mod_type,
        domain: u32_value(&row["Domain"]).map_err(|error| format!("Domain: {error}"))?,
        generation_type: optional_u32(&row["GenerationType"])
            .map_err(|error| format!("GenerationType: {error}"))?,
        level: u32_value(&row["Level"]).map_err(|error| format!("Level: {error}"))?,
        stats,
        tags,
        group,
        families,
        spawn_weights,
    })
}

fn resolve(tables: &Tables<'_>, table: &str, row: &Value, key: &str) -> Result<String, String> {
    let index = row
        .as_u64()
        .and_then(|i| usize::try_from(i).ok())
        .ok_or_else(|| format!("invalid {table} physical reference: {row}"))?;
    let value = text(&tables.row(table, index)?[key])?;
    if value.is_empty() {
        return Err(format!("empty {table}/{index}/{key}"));
    }
    Ok(value.into())
}

fn resolve_list(tables: &Tables<'_>, table: &str, values: &Value) -> Result<Vec<String>, String> {
    array(values)?
        .iter()
        .map(|value| resolve(tables, table, value, "Id"))
        .collect()
}

fn text(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| "expected string".into())
}

fn array(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "expected array".into())
}

fn u32_value(value: &Value) -> Result<u32, String> {
    value
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("value cannot be represented as u32: {value}"))
}

fn optional_u32(value: &Value) -> Result<Option<u32>, String> {
    if value.is_null() {
        Ok(None)
    } else {
        u32_value(value).map(Some)
    }
}

fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for table in ["Stats", "Mods", "ModFamily", "Tags"] {
        column(
            input,
            table,
            &["Id"],
            &json!({"name":"Id", "type":"string", "array":false, "interval":false, "references":null}),
        )?;
    }
    column(
        input,
        "ModType",
        &["Name"],
        &json!({"name":"Name", "type":"string", "array":false, "interval":false, "references":null}),
    )?;
    for (table, name, kind, target) in [
        ("Stats", "IsLocal", "bool", None),
        ("Stats", "Semantic", "enumrow", Some("StatSemantics")),
        (
            "Stats",
            "Category",
            "foreignrow",
            Some("PassiveSkillStatCategories"),
        ),
        ("Mods", "Name", "string", None),
        ("Mods", "Level", "i32", None),
        ("Mods", "Domain", "enumrow", Some("ModDomains")),
        (
            "Mods",
            "GenerationType",
            "enumrow",
            Some("ModGenerationType"),
        ),
        ("Mods", "ModType", "foreignrow", Some("ModType")),
    ] {
        column(
            input,
            table,
            &[name],
            &json!({"name":name, "type":kind, "array":false, "interval":false,
            "references":target.map(|table| json!({"table":table}))}),
        )?;
    }
    for (name, kind, target) in [
        ("Families", "foreignrow", Some("ModFamily")),
        ("Tags", "foreignrow", Some("Tags")),
        ("SpawnWeight_Tags", "foreignrow", Some("Tags")),
        ("SpawnWeight_Values", "i32", None),
    ] {
        column(
            input,
            "Mods",
            &[name],
            &json!({"name":name, "type":kind, "array":true, "interval":false,
            "references":target.map(|table| json!({"table":table}))}),
        )?;
    }
    let mut slot_fields = BTreeSet::new();
    for slot in 1..=STAT_SLOTS {
        let stat = format!("Stat{slot}");
        column(
            input,
            "Mods",
            &[&stat],
            &json!({"name":stat, "type":"foreignrow", "array":false, "interval":false,
            "references":{"table":"Stats"}}),
        )?;
        let min = format!("{stat}Min");
        let max = format!("{stat}Max");
        column(
            input,
            "Mods",
            &[&min, &max],
            &json!({"name":format!("{stat}Value"), "type":"i32", "array":false,
            "interval":true, "references":null}),
        )?;
        for suffix in ["", "Min", "Max", "MinHardmode", "MaxHardmode"] {
            slot_fields.insert(format!("{stat}{suffix}"));
        }
    }
    for field in array(&input.documents["tables/Mods.json"]["physical_fields"])? {
        let field = text(field)?;
        if field
            .strip_prefix("Stat")
            .and_then(|s| s.bytes().next())
            .is_some_and(|b| b.is_ascii_digit())
            && !slot_fields.contains(field)
        {
            return Err(format!("unhandled Mods stat-slot column: {field}"));
        }
    }
    Ok(())
}

/// Assert the physical column contract, including foreign-key destination and interval expansion.
fn column(
    input: &VerifiedFacts,
    table: &str,
    fields: &[&str],
    schema: &Value,
) -> Result<(), String> {
    let doc = &input.documents[&format!("tables/{table}.json")];
    let expected = json!(fields);
    let found = array(&doc["columns"])?
        .iter()
        .find(|column| column["physical_fields"] == expected)
        .ok_or_else(|| format!("{table}: missing column {fields:?}"))?;
    for (key, expected) in schema.as_object().ok_or("invalid column contract")? {
        if found["schema"].get(key) != Some(expected) {
            return Err(format!("{table} {fields:?}: source schema {key} mismatch"));
        }
    }
    Ok(())
}
