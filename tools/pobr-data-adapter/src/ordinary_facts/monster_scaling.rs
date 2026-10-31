//! Original per-level monster arrays, supplied entirely by authenticated target facts.

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::BTreeMap;

use pobr_data::catalog::monster_scaling::{MONSTER_SCALING_TABLE_LEN, MonsterScalingTable};
use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 1] = ["DefaultMonsterStats"];
const TABLE: &str = TABLES[0];
const PAYLOAD: &str = "tables/DefaultMonsterStats.json";
const INTEGERS: [(&str, &str); 7] = [
    ("Accuracy", "accuracy"),
    ("Evasion", "evasion"),
    ("Armour", "armour"),
    ("MonsterLife", "life"),
    ("AllyLife", "ally_life"),
    ("AilmentThreshold", "ailment_threshold"),
    ("PoiseThreshold", "poise_threshold"),
];
const FLOATS: [(&str, &str); 2] = [("Damage", "damage"), ("MinionDamage", "ally_damage")];

pub(super) struct Converted {
    pub base: BTreeMap<String, Vec<u8>>,
    pub receipt: Value,
}

struct Level<'a> {
    source_row: usize,
    raw: &'a Value,
    integers: [u32; 7],
    damage: f64,
    ally_damage: f64,
}

pub(super) fn convert(input: &VerifiedFacts) -> Result<Converted, String> {
    let tables = Tables::selected(input, &TABLES)?;
    for (field, kind) in std::iter::once(("DisplayLevel", "string"))
        .chain(INTEGERS.iter().map(|(field, _)| (*field, "i32")))
        .chain(FLOATS.iter().map(|(field, _)| (*field, "f32")))
    {
        super::convert::column(
            input,
            TABLE,
            &[field],
            &json!({
                "name":field,"type":kind,"array":false,"interval":false,"references":null
            }),
        )?;
    }
    let mut levels = BTreeMap::new();
    for source_row in 0..tables.len(TABLE) {
        let raw = tables.row(TABLE, source_row)?;
        let parsed = (|| {
            let level = parse_level(&raw["DisplayLevel"])?;
            let mut integers = [0; 7];
            for (index, (field, _)) in INTEGERS.iter().enumerate() {
                integers[index] = integer(&raw[*field]).map_err(|e| format!("{field}: {e}"))?;
            }
            Ok::<_, String>((
                level,
                Level {
                    source_row,
                    raw,
                    integers,
                    damage: damage(&raw["Damage"]).map_err(|e| format!("Damage: {e}"))?,
                    ally_damage: damage(&raw["MinionDamage"])
                        .map_err(|e| format!("MinionDamage: {e}"))?,
                },
            ))
        })()
        .map_err(|e| format!("{TABLE}/{source_row}: {e}"))?;
        let (level, row) = parsed;
        if levels.insert(level, row).is_some() {
            return Err(format!("duplicate DisplayLevel: {level}"));
        }
    }
    // Range + uniqueness + this cardinality proves the complete level set.
    if levels.len() != MONSTER_SCALING_TABLE_LEN {
        return Err(format!(
            "incomplete DisplayLevel set: expected 1..={MONSTER_SCALING_TABLE_LEN}, got {} levels",
            levels.len()
        ));
    }
    // Every element comes from a validated row. Never fill holes from Default or zeros.
    let data = MonsterScalingTable {
        accuracy: levels.values().map(|r| r.integers[0]).collect(),
        evasion: levels.values().map(|r| r.integers[1]).collect(),
        armour: levels.values().map(|r| r.integers[2]).collect(),
        life: levels.values().map(|r| r.integers[3]).collect(),
        ally_life: levels.values().map(|r| r.integers[4]).collect(),
        damage: levels.values().map(|r| r.damage).collect(),
        ally_damage: levels.values().map(|r| r.ally_damage).collect(),
        ailment_threshold: levels.values().map(|r| r.integers[5]).collect(),
        poise_threshold: levels.values().map(|r| r.integers[6]).collect(),
    };
    let records: Vec<_> = levels.iter().map(|(level, r)| {
        let raw: BTreeMap<_, _> = INTEGERS.iter().chain(FLOATS.iter())
            .map(|(field, _)| (*field, &r.raw[*field])).collect();
        let output: BTreeMap<_, _> = INTEGERS.iter().enumerate()
            .map(|(i, (_, field))| (*field, json!(r.integers[i])))
            .chain([("damage", json!(r.damage)), ("ally_damage", json!(r.ally_damage))]).collect();
        json!({"level":level,"source_row":r.source_row,"display_level":r.raw["DisplayLevel"],
            "output_index":level-1,"raw":raw,"output":output,
            "normalizations":[
                {"source_field":"Damage","output_field":"damage","raw":r.raw["Damage"],"normalized":r.damage},
                {"source_field":"MinionDamage","output_field":"ally_damage","raw":r.raw["MinionDamage"],"normalized":r.ally_damage}
            ]})
    }).collect();
    let mut document = serde_json::to_value(data).map_err(|e| e.to_string())?;
    document["_meta"] = json!({
        "schema":"monster-scaling/v1","generator":"pobr-data-adapter ordinary-facts monster-scaling",
        "poe_version":input.manifest["source_identity"]["game_patch"],"source_identity":input.manifest["source_identity"],
        "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),
        "inputs":{PAYLOAD:sha256_hex(&input.bytes[PAYLOAD])}
    });
    let unused: Vec<_> = input.documents[PAYLOAD]["columns"]
        .as_array()
        .ok_or("missing columns")?
        .iter()
        .filter(|column| {
            column["physical_fields"].as_array().is_some_and(|fields| {
                fields.iter().any(|f| {
                    f.as_str() != Some("DisplayLevel")
                        && !INTEGERS
                            .iter()
                            .chain(FLOATS.iter())
                            .any(|(field, _)| f.as_str() == Some(*field))
                })
            })
        })
        .collect();
    let mapping: BTreeMap<_, _> = INTEGERS.into_iter().chain(FLOATS).collect();
    Ok(Converted {
        base: BTreeMap::from([(
            "monster_scaling.json".into(),
            super::pretty_bytes(&document)?,
        )]),
        receipt: json!({
            "levels":levels.len(),"mapping":mapping,"source_records":records,
            "level_identity":"DisplayLevel ASCII decimal integer, complete unique 1..=MONSTER_SCALING_TABLE_LEN; output index level-1, independent of physical source order",
            "integer_conversion":"declared i32 exact JSON integer, checked nonnegative u32; no truncation/default/clamp",
            "float_normalization":"Damage/MinionDamage finite numeric input within f32 range, retained as f64 without a bitwise f32-spelling requirement; (value * 100).round() / 100, nearest 0.01 with ties away from zero; signed values retained",
            "unconsumed_source":[{"table":TABLE,"payload":PAYLOAD,"columns":unused,
                "source_row_range":{"start":0,"end_exclusive":tables.len(TABLE)},
                "identity":"DisplayLevel plus physical row in source_records",
                "status":"unapplied source facts, not effectless"}],
            "pending":"minionGemLevel-to-monsterLevel mapping, full minion catalogs, other prepared assets, complete target runtime and ordinary BD/Web/accuracy acceptance; no hiddenDamageFixup calculation or source approval"
        }),
    })
}

fn parse_level(value: &Value) -> Result<usize, String> {
    let text = value
        .as_str()
        .ok_or("DisplayLevel: ASCII decimal string required")?;
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("DisplayLevel: ASCII decimal string required".into());
    }
    let level = text
        .parse::<usize>()
        .map_err(|_| "DisplayLevel: integer overflow")?;
    if !(1..=MONSTER_SCALING_TABLE_LEN).contains(&level) {
        return Err(format!(
            "DisplayLevel: outside 1..={MONSTER_SCALING_TABLE_LEN}"
        ));
    }
    Ok(level)
}

fn integer(value: &Value) -> Result<u32, String> {
    let raw = value
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or("exact signed i32 required")?;
    u32::try_from(raw).map_err(|_| "nonnegative u32 required".into())
}

fn damage(value: &Value) -> Result<f64, String> {
    let raw = value.as_f64().ok_or("finite f32-range number required")?;
    if !raw.is_finite() || raw.abs() > f64::from(f32::MAX) {
        return Err("finite f32-range number required".into());
    }
    // f32 bounds keep this multiplication finite in f64. Do not cast to f32 first:
    // valid short JSON decimal spellings need not equal a widened f32 bit pattern.
    Ok((raw * 100.0).round() / 100.0)
}
