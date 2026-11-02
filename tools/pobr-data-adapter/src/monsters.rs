//! `MonsterVarieties.MonsterType` -> `MonsterTypes.MonsterResistances`
//! -> the normal-rarity `Fire1` / `Cold1` / `Lightning1` / `Chaos1` values.

use std::collections::BTreeMap;

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

/// Join parsed rows from authenticated physical facts.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn joins_all_type_resistances_and_preserves_vulnerabilities() {
        let tables: BTreeMap<&str, serde_json::Value> = BTreeMap::from([
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
        ]);
        let adapt = |tables: &BTreeMap<&str, serde_json::Value>| {
            let parse = |name: &str| tables[name].clone();
            adapt_rows(
                serde_json::from_value(parse("MonsterVarieties")).unwrap(),
                &serde_json::from_value::<Vec<RawMonsterType>>(parse("MonsterTypes")).unwrap(),
                serde_json::from_value(parse("MonsterResistances")).unwrap(),
            )
        };
        let data = adapt(&tables).unwrap();
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

        let mut changed = tables.clone();
        changed.get_mut("MonsterResistances").unwrap()[0]["Cold1"] = json!([30, 50]);
        assert!(
            adapt(&changed)
                .unwrap_err()
                .contains("Cold1: expected one base value")
        );
    }
}
