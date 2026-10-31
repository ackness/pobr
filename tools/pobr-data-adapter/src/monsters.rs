//! `MonsterVarieties.MonsterType` -> `MonsterTypes.MonsterResistances`
//! -> the normal-rarity `Fire1` / `Cold1` / `Lightning1` / `Chaos1` values.

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{MonsterResistanceDef, MonsterResistancesDef};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct RawVariety {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "MonsterType")]
    pub monster_type: usize,
}

#[derive(Deserialize)]
pub(crate) struct RawMonsterType {
    #[serde(rename = "MonsterResistances")]
    pub resistances: Vec<usize>,
}

#[derive(Deserialize)]
pub(crate) struct RawResistance {
    #[serde(rename = "Fire1")]
    fire: Vec<i32>,
    #[serde(rename = "Cold1")]
    cold: Vec<i32>,
    #[serde(rename = "Lightning1")]
    lightning: Vec<i32>,
    #[serde(rename = "Chaos1")]
    chaos: Vec<i32>,
}

fn adapt(en: &Path) -> Result<MonsterResistancesDef, String> {
    let varieties: Vec<RawVariety> = crate::read_json(&en.join("MonsterVarieties.json"))?;
    let types: Vec<RawMonsterType> = crate::read_json(&en.join("MonsterTypes.json"))?;
    let raw: Vec<RawResistance> = crate::read_json(&en.join("MonsterResistances.json"))?;
    adapt_rows(varieties, &types, raw)
}

/// Join parsed rows from either the original raw loader or authenticated physical facts.
pub(crate) fn adapt_rows(
    varieties: Vec<RawVariety>,
    types: &[RawMonsterType],
    raw: Vec<RawResistance>,
) -> Result<MonsterResistancesDef, String> {
    let resistances: Vec<MonsterResistanceDef> = raw
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            // The pinned 0.5 table has one value per column. Do not
            // silently discard a future level-dependent array.
            let value = |column: &str, values: Vec<i32>| match values.as_slice() {
                [value] => Ok(*value),
                _ => Err(format!(
                    "MonsterResistances row {index} {column}: expected one base value, got {}",
                    values.len()
                )),
            };
            Ok(MonsterResistanceDef {
                fire: value("Fire1", row.fire)?,
                cold: value("Cold1", row.cold)?,
                lightning: value("Lightning1", row.lightning)?,
                chaos: value("Chaos1", row.chaos)?,
            })
        })
        .collect::<Result<_, String>>()?;
    let mut monsters = BTreeMap::new();
    for variety in varieties {
        let monster_type = types.get(variety.monster_type).ok_or_else(|| {
            format!(
                "{}: missing MonsterTypes row {}",
                variety.id, variety.monster_type
            )
        })?;
        let mut combined = MonsterResistanceDef::default();
        // A type can link separate elemental resistances/vulnerabilities:
        // MudGolem links Fire +30, Cold +30 and Lightning -30. Keep all
        // entries. No resistance records means no base resistance.
        for &index in &monster_type.resistances {
            let resistance = resistances
                .get(index)
                .ok_or_else(|| format!("{}: missing MonsterResistances row {index}", variety.id))?;
            let add = |column: &str, sum: i32, value: i32| {
                sum.checked_add(value).ok_or_else(|| {
                    format!(
                        "{}: {column} i32 sum overflow at MonsterResistances row {index}",
                        variety.id
                    )
                })
            };
            combined.fire = add("Fire1", combined.fire, resistance.fire)?;
            combined.cold = add("Cold1", combined.cold, resistance.cold)?;
            combined.lightning = add("Lightning1", combined.lightning, resistance.lightning)?;
            combined.chaos = add("Chaos1", combined.chaos, resistance.chaos)?;
        }
        if monsters.insert(variety.id.clone(), combined).is_some() {
            return Err(format!("duplicate MonsterVarieties id: {}", variety.id));
        }
    }
    Ok(MonsterResistancesDef { monsters })
}

pub(crate) fn write(raw: &Path, base: &Path, patch: &str) -> Result<(), String> {
    let data = adapt(&raw.join("English"))?;
    let mut inputs = BTreeMap::new();
    for table in ["MonsterVarieties", "MonsterTypes", "MonsterResistances"] {
        let name = format!("English/{table}.json");
        let bytes = std::fs::read(raw.join(&name)).map_err(|e| format!("{name}: {e}"))?;
        inputs.insert(name, crate::raw_source::sha256_hex(&bytes));
    }
    let receipt = std::fs::read(raw.join("export-source.json")).map_err(|e| e.to_string())?;
    crate::write_pretty(
        &base.join("monster_resistances.json"),
        &serde_json::json!({
            "_meta": {
                "schema": "monster-resistances/v1",
                "generator": "pobr-data-adapter",
                "poe_version": patch,
                "source_receipt_sha256": crate::raw_source::sha256_hex(&receipt),
                "inputs": inputs,
            },
            "monsters": data.monsters,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn joins_all_type_resistances_and_preserves_vulnerabilities() {
        let dir =
            std::env::temp_dir().join(format!("pobr-monster-resistances-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, rows) in [
            (
                "MonsterVarieties",
                json!([
                    {"Id": "Metadata/Monsters/MudGolem/MudGolem", "MonsterType": 1},
                    {"Id": "NoResistances", "MonsterType": 0},
                    {"Id": "Metadata/Monsters/Skeletons/PlayerSummoned/SkeletonArsonistPlayerSummoned", "MonsterType": 2}
                ]),
            ),
            (
                "MonsterTypes",
                json!([
                    {"MonsterResistances": []},
                    {"MonsterResistances": [2, 0, 3]},
                    {"MonsterResistances": [1]}
                ]),
            ),
            (
                "MonsterResistances",
                json!([
                    {"Fire1": [0], "Cold1": [30], "Lightning1": [0], "Chaos1": [0]},
                    {"Fire1": [50], "Cold1": [0], "Lightning1": [0], "Chaos1": [0]},
                    {"Fire1": [30], "Cold1": [0], "Lightning1": [0], "Chaos1": [0]},
                    {"Fire1": [0], "Cold1": [0], "Lightning1": [-30], "Chaos1": [0]}
                ]),
            ),
        ] {
            std::fs::write(dir.join(format!("{file}.json")), rows.to_string()).unwrap();
        }
        let data = adapt(&dir).unwrap();
        assert_eq!(
            data.monsters["Metadata/Monsters/MudGolem/MudGolem"],
            MonsterResistanceDef {
                fire: 30,
                cold: 30,
                lightning: -30,
                chaos: 0,
            }
        );
        assert_eq!(
            data.monsters["NoResistances"],
            MonsterResistanceDef::default()
        );
        assert_eq!(data.monsters["Metadata/Monsters/Skeletons/PlayerSummoned/SkeletonArsonistPlayerSummoned"].fire, 50);

        let path = dir.join("MonsterResistances.json");
        let mut rows: serde_json::Value = crate::read_json(&path).unwrap();
        rows[0]["Cold1"] = json!([30, 50]);
        std::fs::write(path, rows.to_string()).unwrap();
        assert!(
            adapt(&dir)
                .unwrap_err()
                .contains("Cold1: expected one base value")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
