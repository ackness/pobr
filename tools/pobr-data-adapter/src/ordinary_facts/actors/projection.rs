//! The archived actor-field recipe, applied exclusively to verified target values.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};

pub(super) const OWNED_FIELDS: [&str; 22] = [
    "name",
    "monster_tags",
    "life",
    "damage",
    "damage_spread",
    "attack_time",
    "attack_range",
    "crit_chance",
    "base_movement_speed",
    "armour",
    "evasion",
    "energy_shield",
    "fire_resist",
    "cold_resist",
    "lightning_resist",
    "chaos_resist",
    "companion_fire_resist",
    "companion_cold_resist",
    "companion_lightning_resist",
    "companion_chaos_resist",
    "spectre_reservation",
    "companion_reservation",
];
const VARIETY_FIELDS: [&str; 11] = [
    "Id",
    "Name",
    "Tags",
    "MonsterType",
    "LifeMultiplier",
    "DamageMultiplier",
    "AttackSpeed",
    "MaximumAttackDistance",
    "MovementSpeed",
    "AttackCrit",
    "ExperienceMultiplier",
];
const TYPE_FIELDS: [&str; 6] = [
    "Id",
    "DamageSpread",
    "Armour",
    "Evasion",
    "EnergyShieldFromLife",
    "MonsterResistances",
];
const RESISTANCES: [(&str, &str); 8] = [
    ("Fire1", "fire_resist"),
    ("Cold1", "cold_resist"),
    ("Lightning1", "lightning_resist"),
    ("Chaos1", "chaos_resist"),
    ("Fire3", "companion_fire_resist"),
    ("Cold3", "companion_cold_resist"),
    ("Lightning3", "companion_lightning_resist"),
    ("Chaos3", "companion_chaos_resist"),
];

pub(super) struct Projected {
    pub values: Map<String, Value>,
    pub receipt: Value,
}

pub(super) fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for table in super::TABLES {
        column(input, table, "Id", "string", false, None)?;
    }
    column(input, "MonsterVarieties", "Name", "string", false, None)?;
    column(input, "MonsterVarieties", "AttackCrit", "f32", false, None)?;
    for field in [
        "LifeMultiplier",
        "DamageMultiplier",
        "AttackSpeed",
        "MaximumAttackDistance",
        "MovementSpeed",
        "ExperienceMultiplier",
    ] {
        column(input, "MonsterVarieties", field, "i32", false, None)?;
    }
    for field in ["DamageSpread", "Armour", "Evasion", "EnergyShieldFromLife"] {
        column(input, "MonsterTypes", field, "i32", false, None)?;
    }
    for (field, _) in RESISTANCES {
        column(input, "MonsterResistances", field, "i32", true, None)?;
    }
    for (table, field, array, target) in [
        ("MonsterVarieties", "MonsterType", false, "MonsterTypes"),
        ("MonsterVarieties", "Tags", true, "Tags"),
        (
            "MonsterTypes",
            "MonsterResistances",
            true,
            "MonsterResistances",
        ),
    ] {
        column(input, table, field, "foreignrow", array, Some(target))?;
    }
    for (field, description) in [
        ("AttackSpeed", "in ms"),
        ("LifeMultiplier", "in percent"),
        ("DamageMultiplier", "in percent"),
        ("ExperienceMultiplier", "in percent"),
    ] {
        super::super::convert::column(
            input,
            "MonsterVarieties",
            &[field],
            &json!({"description":description}),
        )?;
    }
    Ok(())
}

fn column(
    input: &VerifiedFacts,
    table: &str,
    field: &str,
    kind: &str,
    array: bool,
    target: Option<&str>,
) -> Result<(), String> {
    super::super::convert::column(
        input,
        table,
        &[field],
        &json!({"name":field,"type":kind,"array":array,"interval":false,
        "references":target.map(|table|json!({"table":table}))}),
    )
}

pub(super) fn identities(
    tables: &Tables<'_>,
    table: &str,
) -> Result<BTreeMap<String, usize>, String> {
    (0..tables.len(table))
        .map(|source_row| {
            let id = tables.row(table, source_row)?["Id"]
                .as_str()
                .ok_or("invalid source Id")?;
            // Tables already enforces nonempty, unique IDs. These are exact source
            // identities, including any whitespace, not canonical policy tokens.
            Ok((id.into(), source_row))
        })
        .collect()
}

pub(super) fn project(tables: &Tables<'_>, variety_row: usize) -> Result<Projected, String> {
    let variety = tables.row("MonsterVarieties", variety_row)?;
    let type_row = reference(tables, "MonsterTypes", &variety["MonsterType"])?;
    let kind = tables.row("MonsterTypes", type_row)?;
    let mut values = Map::new();
    let name = variety["Name"]
        .as_str()
        .ok_or("Name: string required; empty is allowed")?;
    values.insert("name".into(), json!(name));
    let mut tags = Vec::new();
    let mut tag_occurrences = Vec::new();
    for (position, value) in array(&variety["Tags"], "Tags")?.iter().enumerate() {
        let source_row = reference(tables, "Tags", value)?;
        let id = &tables.row("Tags", source_row)?["Id"];
        tags.push(id.clone());
        tag_occurrences.push(json!({"position":position,"source_row":source_row,"id":id}));
    }
    values.insert("monster_tags".into(), json!(tags));
    for (field, output, divisor) in [
        ("LifeMultiplier", "life", 100.0),
        ("DamageMultiplier", "damage", 100.0),
        ("AttackSpeed", "attack_time", 1000.0),
        ("MaximumAttackDistance", "attack_range", 1.0),
        ("MovementSpeed", "base_movement_speed", 1.0),
    ] {
        values.insert(
            output.into(),
            json!(f64::from(integer(&variety[field], field)?) / divisor),
        );
    }
    values.insert(
        "damage_spread".into(),
        json!(f64::from(integer(&kind["DamageSpread"], "DamageSpread")?) / 100.0),
    );
    for (field, output) in [
        ("Armour", "armour"),
        ("Evasion", "evasion"),
        ("EnergyShieldFromLife", "energy_shield"),
    ] {
        let raw = integer(&kind[field], field)?;
        if raw != 0 {
            values.insert(output.into(), json!(f64::from(raw) / 100.0));
        }
    }
    let crit = variety["AttackCrit"]
        .as_f64()
        .filter(|v| v.is_finite() && v.abs() <= f64::from(f32::MAX))
        .ok_or("AttackCrit: finite numeric value within f32 range required")?;
    let percentage = crit * 100.0;
    // Archived round(value, 2) uses floor, including negative halfway values.
    let rounded = (percentage * 100.0 + 0.5).floor() / 100.0;
    values.insert("crit_chance".into(), json!(rounded));
    let xp = integer(&variety["ExperienceMultiplier"], "ExperienceMultiplier")?;
    if xp < 0 {
        return Err("ExperienceMultiplier: nonnegative value required for sqrt/pow".into());
    }
    let x = f64::from(xp) / 100.0;
    let spectre = (x.powf(0.75) * 50.0).floor();
    let companion = ((x.sqrt() * 100.0).floor() / 100.0) * 30.0;
    if !spectre.is_finite() || !companion.is_finite() || !rounded.is_finite() {
        return Err("nonfinite projected actor value".into());
    }
    values.insert("spectre_reservation".into(), json!(spectre));
    values.insert("companion_reservation".into(), json!(companion));
    let refs = array(&kind["MonsterResistances"], "MonsterResistances")?;
    if refs.is_empty() {
        return Err("MonsterResistances: nonempty reference list required".into());
    }
    let mut resistances = Vec::new();
    for (position, value) in refs.iter().enumerate() {
        let source_row = reference(tables, "MonsterResistances", value)?;
        let row = tables.row("MonsterResistances", source_row)?;
        let mut arrays = BTreeMap::new();
        for (field, output) in RESISTANCES {
            let elements = array(&row[field], field)?;
            for value in elements {
                integer(value, field)?;
            }
            if position == 0 {
                let first = elements.first().ok_or_else(|| {
                    format!("{field}: selected resistance array must be nonempty")
                })?;
                values.insert(output.into(), json!(f64::from(integer(first, field)?)));
            }
            arrays.insert(field,json!({"raw":elements,"selected_element_index":if position==0{Some(0)}else{None},
                "unapplied_element_range":{"start":usize::from(position==0),"end_exclusive":elements.len()}}));
        }
        resistances.push(json!({"position":position,"source_row":source_row,"id":row["Id"],"selected":position==0,"arrays":arrays}));
    }
    let raw_variety: BTreeMap<_, _> = VARIETY_FIELDS
        .iter()
        .map(|field| (*field, &variety[*field]))
        .collect();
    let raw_type: BTreeMap<_, _> = TYPE_FIELDS
        .iter()
        .map(|field| (*field, &kind[*field]))
        .collect();
    Ok(Projected {
        values,
        receipt: json!({
            "variety":{"source_row":variety_row,"id":variety["Id"],"raw":raw_variety},
            "monster_type":{"source_row":type_row,"id":kind["Id"],"raw":raw_type},
            "tag_occurrences":tag_occurrences,"resistance_occurrences":resistances,
            "resistance_selection":{"reference_position":0,"array_element_index":0,"strategy":"archived first reference / first element; further occurrences are unapplied in this projection, not effectless; separate normal resistance sum/injection unchanged"},
            "crit_normalization":{"raw":crit,"percentage":percentage,"output":rounded,"formula":"floor((raw * 100.0) * 100.0 + 0.5) / 100.0; negative ties towards positive infinity; no f32 recast"},
            "reservation":{"raw_percent":xp,"x":x,"spectre":spectre,"companion":companion,
                "spectre_formula":"floor(x.powf(0.75) * 50.0)","companion_formula":"(floor(x.sqrt() * 100.0) / 100.0) * 30.0"}
        }),
    })
}

fn integer(value: &Value, field: &str) -> Result<i32, String> {
    value
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| format!("{field}: exact i32 required"))
}
fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{field}: array required"))
}
fn reference(tables: &Tables<'_>, table: &str, value: &Value) -> Result<usize, String> {
    let row = value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| format!("{table}: required physical reference must be usize"))?;
    tables.row(table, row)?;
    Ok(row)
}

pub(super) fn mapping() -> Value {
    json!({"name":"MonsterVarieties.Name (empty string retained)","monster_tags":"MonsterVarieties.Tags -> ordered Tags.Id",
        "life":"MonsterVarieties.LifeMultiplier / 100.0","damage":"MonsterVarieties.DamageMultiplier / 100.0",
        "damage_spread":"MonsterTypes.DamageSpread / 100.0","attack_time":"MonsterVarieties.AttackSpeed / 1000.0 (ms to seconds)",
        "attack_range":"MonsterVarieties.MaximumAttackDistance unchanged","base_movement_speed":"MonsterVarieties.MovementSpeed unchanged",
        "crit_chance":"floor((MonsterVarieties.AttackCrit * 100.0) * 100.0 + 0.5) / 100.0",
        "armour":"MonsterTypes.Armour / 100.0; omit if raw zero","evasion":"MonsterTypes.Evasion / 100.0; omit if raw zero","energy_shield":"MonsterTypes.EnergyShieldFromLife / 100.0; omit if raw zero",
        "fire_resist":"MonsterTypes.MonsterResistances[0] -> Fire1[0]","cold_resist":"MonsterTypes.MonsterResistances[0] -> Cold1[0]",
        "lightning_resist":"MonsterTypes.MonsterResistances[0] -> Lightning1[0]","chaos_resist":"MonsterTypes.MonsterResistances[0] -> Chaos1[0]",
        "companion_fire_resist":"MonsterTypes.MonsterResistances[0] -> Fire3[0]","companion_cold_resist":"MonsterTypes.MonsterResistances[0] -> Cold3[0]",
        "companion_lightning_resist":"MonsterTypes.MonsterResistances[0] -> Lightning3[0]","companion_chaos_resist":"MonsterTypes.MonsterResistances[0] -> Chaos3[0]",
        "spectre_reservation":"floor((MonsterVarieties.ExperienceMultiplier / 100.0).powf(0.75) * 50.0)",
        "companion_reservation":"(floor((MonsterVarieties.ExperienceMultiplier / 100.0).sqrt() * 100.0) / 100.0) * 30.0"})
}

pub(super) fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    super::TABLES.iter().map(|table|{
        let used:Vec<&str>=match *table {
            "MonsterVarieties"=>VARIETY_FIELDS.to_vec(),"MonsterTypes"=>TYPE_FIELDS.to_vec(),
            "MonsterResistances"=>std::iter::once("Id").chain(RESISTANCES.iter().map(|(f,_)|*f)).collect(),_=>vec!["Id"],
        };
        let path=format!("tables/{table}.json");
        let columns:Vec<_>=input.documents[&path]["columns"].as_array().ok_or("missing columns")?.iter()
            .filter(|c|c["physical_fields"].as_array().is_some_and(|fields|fields.iter().any(|f|!used.iter().any(|u|f.as_str()==Some(*u))))).collect();
        Ok(json!({"table":table,"payload":path,"consumed_columns":used,"columns":columns,
            "source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "scope":"all table identities checked; only catalog-referenced rows/fields projected. Remaining fields, unselected creature relationships and rarity/level semantics not certified or declared effectless"}))
    }).collect()
}
