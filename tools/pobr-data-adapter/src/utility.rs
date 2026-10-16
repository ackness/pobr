//! Official flask/charm bases: Flasks joins BaseItemTypes by row index and
//! ComponentCharges by the base item's stable Id. RecoveryTime is in tenths of a second.
use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::UtilityBaseStats;
use serde::Deserialize;

#[derive(Deserialize)]
struct RawFlask {
    #[serde(rename = "BaseItemType")]
    base: usize,
    #[serde(rename = "LifePerUse")]
    life: u32,
    #[serde(rename = "ManaPerUse")]
    mana: u32,
    #[serde(rename = "RecoveryTime")]
    recovery_time: u32,
}

#[derive(Deserialize)]
struct RawCharges {
    // This is a string FK to BaseItemTypes.Id, not a row index.
    #[serde(rename = "BaseItemTypesKey")]
    base_id: String,
    #[serde(rename = "MaxCharges")]
    maximum: u32,
    #[serde(rename = "PerCharge")]
    per_use: u32,
}

pub(crate) fn adapt(en: &Path) -> Result<BTreeMap<usize, UtilityBaseStats>, String> {
    let bases = crate::id_lookup(&crate::read_json::<Vec<crate::RawIndexed>>(
        &en.join("BaseItemTypes.json"),
    )?);
    let flasks: Vec<RawFlask> = crate::read_json(&en.join("Flasks.json"))?;
    let raw_charges: Vec<RawCharges> = crate::read_json(&en.join("ComponentCharges.json"))?;
    let mut charges = BTreeMap::new();
    for row in raw_charges {
        let id = row.base_id.clone();
        if charges.insert(id.clone(), row).is_some() {
            return Err(format!("ComponentCharges: duplicate base id {id}"));
        }
    }
    let mut utility = BTreeMap::new();
    for flask in flasks {
        let id = crate::resolve(&bases, flask.base)
            .ok_or_else(|| format!("Flasks: missing BaseItemTypes row {}", flask.base))?;
        let charge = charges
            .get(&id)
            .ok_or_else(|| format!("Flasks {id}: missing ComponentCharges row"))?;
        let value = UtilityBaseStats {
            duration: f64::from(flask.recovery_time) / 10.0,
            charges_max: f64::from(charge.maximum),
            charges_used: f64::from(charge.per_use),
            life: f64::from(flask.life),
            mana: f64::from(flask.mana),
        };
        if utility.insert(flask.base, value).is_some() {
            return Err(format!("Flasks: duplicate base id {id}"));
        }
    }
    Ok(utility)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn joins_stable_charge_ids_and_converts_flask_and_charm_deciseconds() {
        let dir = std::env::temp_dir().join(format!("pobr-utility-tables-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let life = "Metadata/Items/Flasks/FourFlaskLife9";
        let charm = "Metadata/Items/Flasks/FourCharm8";
        for (table, rows) in [
            (
                "BaseItemTypes",
                json!([
                    {"_index": 0, "Id": life}, {"_index": 1, "Id": charm}
                ]),
            ),
            (
                "Flasks",
                json!([
                    {"BaseItemType": 0, "LifePerUse": 920, "ManaPerUse": 0, "RecoveryTime": 30},
                    {"BaseItemType": 1, "LifePerUse": 0, "ManaPerUse": 0, "RecoveryTime": 40}
                ]),
            ),
            // Deliberately reverse the rows: this join is by stable Id.
            (
                "ComponentCharges",
                json!([
                    {"BaseItemTypesKey": charm, "MaxCharges": 40, "PerCharge": 20},
                    {"BaseItemTypesKey": life, "MaxCharges": 75, "PerCharge": 10}
                ]),
            ),
        ] {
            std::fs::write(dir.join(format!("{table}.json")), rows.to_string()).unwrap();
        }
        let values = adapt(&dir).unwrap();
        assert_eq!(
            values[&0],
            UtilityBaseStats {
                duration: 3.0,
                charges_max: 75.0,
                charges_used: 10.0,
                life: 920.0,
                mana: 0.0,
            }
        );
        assert_eq!(
            values[&1],
            UtilityBaseStats {
                duration: 4.0,
                charges_max: 40.0,
                charges_used: 20.0,
                life: 0.0,
                mana: 0.0,
            }
        );
        std::fs::write(dir.join("ComponentCharges.json"), "[]").unwrap();
        assert!(
            adapt(&dir)
                .unwrap_err()
                .contains("missing ComponentCharges row")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
