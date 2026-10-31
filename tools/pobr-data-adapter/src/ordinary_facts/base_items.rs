//! Physical base-item facts joined into the existing ordinary item DTOs.

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

use std::collections::BTreeMap;

use pobr_data::catalog::{ArmourBaseStats, BaseItemDef, UtilityBaseStats, WeaponBaseStats};
use serde_json::{Value, json};

use super::convert::{column, resolve, resolve_list, text, u32_value};
use crate::native_facts::reader::{Tables, VerifiedFacts};

pub(super) const TABLES: [&str; 11] = [
    "BaseItemTypes",
    "ItemClasses",
    "Tags",
    "Mods",
    "WeaponTypes",
    "ArmourTypes",
    "ShieldTypes",
    "ItemSpirit",
    "AttributeRequirements",
    "Flasks",
    "ComponentCharges",
];

const FIELDS: [(&str, &[&str]); 11] = [
    (
        "BaseItemTypes",
        &[
            "Id",
            "Name",
            "ItemClass",
            "DropLevel",
            "Width",
            "Height",
            "Tags",
            "Implicit_Mods",
            "ModDomain",
        ],
    ),
    ("ItemClasses", &["Id"]),
    ("Tags", &["Id"]),
    ("Mods", &["Id"]),
    (
        "WeaponTypes",
        &[
            "BaseItemType",
            "DamageMin",
            "DamageMax",
            "Speed",
            "CritChance",
            "RangeMax",
            "ReloadTime",
        ],
    ),
    (
        "ArmourTypes",
        &[
            "BaseItemType",
            "Armour",
            "Evasion",
            "EnergyShield",
            "Ward",
            "IncreasedMovementSpeed",
        ],
    ),
    ("ShieldTypes", &["BaseItemType", "Block"]),
    ("ItemSpirit", &["BaseItemType", "SpiritGranted"]),
    (
        "AttributeRequirements",
        &["BaseItemType", "ReqStr", "ReqDex", "ReqInt"],
    ),
    (
        "Flasks",
        &["BaseItemType", "RecoveryTime", "LifePerUse", "ManaPerUse"],
    ),
    (
        "ComponentCharges",
        &["BaseItemTypesKey", "MaxCharges", "PerCharge"],
    ),
];

pub(super) struct Converted {
    pub bases: Vec<BaseItemDef>,
    pub receipt: Value,
}

pub(super) fn convert(input: &VerifiedFacts) -> Result<Converted, String> {
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    let weapons = components(&tables, "WeaponTypes", |row| {
        let min = number(row, "DamageMin")?;
        let max = number(row, "DamageMax")?;
        if min > max {
            return Err(format!("bad damage range {min}..{max}"));
        }
        let reload = number(row, "ReloadTime")?;
        Ok(WeaponBaseStats {
            physical_min: min,
            physical_max: max,
            speed_ms: number(row, "Speed")?,
            crit_chance: number(row, "CritChance")?,
            range: number(row, "RangeMax")?,
            reload_time_ms: (reload != 0).then_some(reload),
        })
    })?;
    let mut armours = components(&tables, "ArmourTypes", |row| {
        let movement = row["IncreasedMovementSpeed"]
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .ok_or("IncreasedMovementSpeed must be signed i32")?;
        Ok(ArmourBaseStats {
            armour: number(row, "Armour")?,
            evasion: number(row, "Evasion")?,
            energy_shield: number(row, "EnergyShield")?,
            ward: number(row, "Ward")?,
            block_chance: None,
            movement_penalty: (movement != 0).then(|| -f64::from(movement) / 10000.0),
        })
    })?;
    let shields = components(&tables, "ShieldTypes", |row| number(row, "Block"))?;
    for (base, block) in &shields {
        // All 203 shield rows in the pinned F have ArmourTypes. A future unmatched
        // row cannot prove zero defences, so it must not manufacture an armour shell.
        let armour = armours
            .get_mut(base)
            .ok_or_else(|| format!("ShieldTypes: BaseItemTypes/{base} has no ArmourTypes row"))?;
        armour.block_chance = Some(f64::from(*block));
    }
    let spirits = components(&tables, "ItemSpirit", |row| number(row, "SpiritGranted"))?;
    let requirements = components(&tables, "AttributeRequirements", |row| {
        Ok([
            number(row, "ReqStr")?,
            number(row, "ReqDex")?,
            number(row, "ReqInt")?,
        ])
    })?;
    let (utility, utility_evidence) = utilities(&tables)?;
    let mut bases = Vec::new();
    let mut filtered = Vec::new();
    let mut missing_charm_buff = Vec::new();
    let mut absent_requirements = 0;
    for source_row in 0..tables.len("BaseItemTypes") {
        let raw = tables.row("BaseItemTypes", source_row)?;
        let base = (|| {
            let id = text(&raw["Id"])?;
            // The existing RawBaseItem contract requires a string. F has no null
            // names; null must fail rather than becoming a fabricated empty name.
            let name = text(&raw["Name"]).map_err(|e| format!("Name: {e}"))?;
            let item_class = resolve(&tables, "ItemClasses", &raw["ItemClass"], "Id")?;
            let [req_str, req_dex, req_int] =
                requirements.get(&source_row).copied().unwrap_or_else(|| {
                    absent_requirements += 1;
                    [0; 3] // Existing BaseItemDef semantics: no requirement row means no base requirement.
                });
            Ok(BaseItemDef {
                id: id.into(),
                name: name.into(),
                item_class,
                drop_level: number(raw, "DropLevel")?,
                width: u8::try_from(number(raw, "Width")?).map_err(|_| "Width exceeds u8")?,
                height: u8::try_from(number(raw, "Height")?).map_err(|_| "Height exceeds u8")?,
                tags: resolve_list(&tables, "Tags", &raw["Tags"])
                    .map_err(|e| format!("Tags: {e}"))?,
                implicits: resolve_list(&tables, "Mods", &raw["Implicit_Mods"])
                    .map_err(|e| format!("Implicit_Mods: {e}"))?,
                mod_domain: number(raw, "ModDomain")?,
                weapon: weapons.get(&source_row).cloned(),
                armour: armours.get(&source_row).cloned(),
                spirit: spirits.get(&source_row).copied(),
                utility: utility.get(&source_row).cloned(),
                charm_buff: Vec::new(),
                req_str,
                req_dex,
                req_int,
            })
        })()
        .map_err(|error: String| format!("BaseItemTypes/{source_row} ({}): {error}", raw["Id"]))?;
        let excluded = crate::is_placeholder(&base.name);
        if base.item_class == "UtilityFlask" {
            missing_charm_buff.push(json!({
                "id":base.id, "source_row":source_row, "item_class":base.item_class,
                "included":!excluded, "field":"charm_buff",
                "status":"missing-compatible-inherent-effect-description",
                "source":"tables/BaseItemTypes.json", "reason":"No direct mapping to ordinary buff text in this slice; empty does not mean effectless."
            }));
        }
        if excluded {
            filtered.push(json!({"id":base.id,"source_row":source_row,"name":base.name}));
        } else {
            bases.push(base);
        }
    }
    bases.sort_by(|a, b| a.id.cmp(&b.id));
    let unmapped: BTreeMap<_, _> = FIELDS
        .iter()
        .map(|(table, used)| {
            let payload = format!("tables/{table}.json");
            let columns: Vec<_> = input.documents[&payload]["physical_fields"]
                .as_array()
                .ok_or("missing physical fields")?
                .iter()
                .filter(|field| field.as_str().is_some_and(|name| !used.contains(&name)))
                .cloned()
                .collect();
            Ok((
                *table,
                json!({"payload":payload,"columns":columns,
            "source_rows":{"start":0,"end_exclusive":tables.len(table)},
            "retained":"pinned source values; not applied to this DTO"}),
            ))
        })
        .collect::<Result<_, String>>()?;
    let receipt = json!({
        "base_items":bases.len(), "source_base_items":tables.len("BaseItemTypes"),
        "filtered_placeholders":filtered, "missing_charm_buff":missing_charm_buff,
        "bases_without_requirement_row":absent_requirements,
        "utility":utility_evidence, "unmapped_fields":unmapped,
        "source_row_reference":"tables/{table}.json#/rows/{source_row}/values",
        "mapping": {
            "base":"BaseItemTypes.Id/Name/DropLevel/Width/Height/ModDomain -> existing fields; ItemClass -> ItemClasses.Id; Tags -> Tags.Id; Implicit_Mods -> Mods.Id; all used values/joins checked before placeholder filtering",
            "weapon":"WeaponTypes.BaseItemType joins physical base row; DamageMin/Max, Speed(ms), CritChance(raw), RangeMax copied; ReloadTime(ms) > 0 -> Some, 0 -> None",
            "armour":"ArmourTypes.BaseItemType joins physical base row; Armour/Evasion/EnergyShield/Ward copied; nonzero IncreasedMovementSpeed -> -raw/10000, zero -> None",
            "shield":"ShieldTypes.BaseItemType joins the existing armour row; Block raw percent -> Some(block_chance), including zero; missing armour is an error",
            "spirit":"ItemSpirit.BaseItemType joins physical base row; SpiritGranted -> Some(spirit), including zero; no row -> None",
            "requirements":"AttributeRequirements.BaseItemType joins physical base row; ReqStr/ReqDex/ReqInt copied; no row -> zero base requirements, existing malformed row -> error",
            "utility":"Flasks.BaseItemType joins physical base row; ComponentCharges.BaseItemTypesKey joins BaseItemTypes.Id (string); RecoveryTime/10 seconds, LifePerUse/ManaPerUse, MaxCharges/PerCharge copied; no Flasks row -> None; missing charges -> error; secondary modes never replace primary values",
            "charm_buff":"Empty pending compatible inherent-effect text, never inferred from vendor, another patch, names or raw buff references",
            "placeholder":"Existing is_placeholder(Name): empty or [DNT/[UNUSED/[OLD marker; null Name is an error"
        }
    });
    Ok(Converted { bases, receipt })
}

fn number(row: &Value, field: &str) -> Result<u32, String> {
    u32_value(&row[field]).map_err(|error| format!("{field}: {error}"))
}

/// Component row order has no relationship to BaseItemTypes physical row order.
fn components<T>(
    tables: &Tables<'_>,
    table: &str,
    convert: impl Fn(&Value) -> Result<T, String>,
) -> Result<BTreeMap<usize, T>, String> {
    let mut values = BTreeMap::new();
    for source_row in 0..tables.len(table) {
        let raw = tables.row(table, source_row)?;
        let base = base_reference(tables, &raw["BaseItemType"])
            .map_err(|error| format!("{table}/{source_row}: {error}"))?;
        let value = convert(raw).map_err(|error| format!("{table}/{source_row}: {error}"))?;
        if values.insert(base, value).is_some() {
            return Err(format!(
                "{table}/{source_row}: duplicate BaseItemTypes/{base} join"
            ));
        }
    }
    Ok(values)
}

fn base_reference(tables: &Tables<'_>, value: &Value) -> Result<usize, String> {
    let base = value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("invalid BaseItemTypes physical reference: {value}"))?;
    tables.row("BaseItemTypes", base)?;
    Ok(base)
}

fn utilities(tables: &Tables<'_>) -> Result<(BTreeMap<usize, UtilityBaseStats>, Value), String> {
    let mut base_ids = BTreeMap::new();
    for row in 0..tables.len("BaseItemTypes") {
        base_ids.insert(text(&tables.row("BaseItemTypes", row)?["Id"])?, row);
    }
    let mut charges = BTreeMap::new();
    let mut unmapped = Vec::new();
    let mut differences = Vec::new();
    for source_row in 0..tables.len("ComponentCharges") {
        let raw = tables.row("ComponentCharges", source_row)?;
        let id = text(&raw["BaseItemTypesKey"])?;
        let base = *base_ids.get(id).ok_or_else(|| {
            format!("ComponentCharges/{source_row}: unresolved BaseItemTypes.Id {id}")
        })?;
        let value = (
            source_row,
            number(raw, "MaxCharges")?,
            number(raw, "PerCharge")?,
        );
        if charges.insert(base, value).is_some() {
            return Err(format!(
                "ComponentCharges/{source_row}: duplicate BaseItemTypes.Id {id}"
            ));
        }
        utility_remainder(
            raw,
            "ComponentCharges",
            source_row,
            id,
            &mut unmapped,
            &mut differences,
        )?;
    }
    let mut consumed = std::collections::BTreeSet::new();
    let utility = components(tables, "Flasks", |raw| {
        let base = base_reference(tables, &raw["BaseItemType"])?;
        let (_, maximum, per_use) = charges
            .get(&base)
            .copied()
            .ok_or_else(|| format!("BaseItemTypes/{base}: missing ComponentCharges row"))?;
        Ok(UtilityBaseStats {
            duration: f64::from(number(raw, "RecoveryTime")?) / 10.0,
            life: f64::from(number(raw, "LifePerUse")?),
            mana: f64::from(number(raw, "ManaPerUse")?),
            charges_max: f64::from(maximum),
            charges_used: f64::from(per_use),
        })
    })?;
    for source_row in 0..tables.len("Flasks") {
        let raw = tables.row("Flasks", source_row)?;
        let base = base_reference(tables, &raw["BaseItemType"])?;
        consumed.insert(base);
        let id = text(&tables.row("BaseItemTypes", base)?["Id"])?;
        utility_remainder(
            raw,
            "Flasks",
            source_row,
            id,
            &mut unmapped,
            &mut differences,
        )?;
    }
    let unpaired: Vec<_> = charges
        .iter()
        .filter(|(base, _)| !consumed.contains(*base))
        .map(|(base, (source_row, maximum, per_use))| {
            Ok(
                json!({"base_source_row":base,"id":tables.row("BaseItemTypes",*base)?["Id"],
                "payload":"tables/ComponentCharges.json","source_row":source_row,
                "max_charges":maximum,"per_charge":per_use}),
            )
        })
        .collect::<Result<_, String>>()?;
    Ok((
        utility,
        json!({"unmapped_rows":unmapped,"secondary_mode_differences":differences,
        "charges_without_flask_row":unpaired}),
    ))
}

fn utility_remainder(
    raw: &Value,
    table: &str,
    source_row: usize,
    id: &str,
    unmapped: &mut Vec<Value>,
    differences: &mut Vec<Value>,
) -> Result<(), String> {
    let used = FIELDS
        .iter()
        .find(|(name, _)| *name == table)
        .ok_or("unknown utility table")?
        .1;
    let values: BTreeMap<_, _> = raw
        .as_object()
        .ok_or("invalid utility row")?
        .iter()
        .filter(|(field, _)| !used.contains(&field.as_str()))
        .collect();
    unmapped.push(json!({"id":id,"payload":format!("tables/{table}.json"),"source_row":source_row,"values":values}));
    for &primary in if table == "Flasks" {
        &["RecoveryTime"][..]
    } else {
        &["MaxCharges", "PerCharge"][..]
    } {
        let secondary = format!("{primary}2");
        if let Some(value) = raw.get(&secondary)
            && value != &raw[primary]
        {
            differences.push(json!({"id":id,"payload":format!("tables/{table}.json"),"source_row":source_row,
                "primary_field":primary,"primary":raw[primary],"secondary_field":secondary,"secondary":value}));
        }
    }
    Ok(())
}

fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for (table, fields) in FIELDS {
        for &field in fields {
            let (kind, array, target) = match (table, field) {
                (_, "Id" | "Name") => ("string", false, None),
                (_, "BaseItemType") => {
                    ("foreignrow", false, Some(json!({"table":"BaseItemTypes"})))
                }
                (_, "BaseItemTypesKey") => (
                    "string",
                    false,
                    Some(json!({"table":"BaseItemTypes","column":"Id"})),
                ),
                (_, "ItemClass") => ("foreignrow", false, Some(json!({"table":"ItemClasses"}))),
                (_, "Tags") => ("foreignrow", true, Some(json!({"table":"Tags"}))),
                (_, "Implicit_Mods") => ("foreignrow", true, Some(json!({"table":"Mods"}))),
                (_, "ModDomain") => ("enumrow", false, Some(json!({"table":"ModDomains"}))),
                _ => ("i32", false, None),
            };
            column(
                input,
                table,
                &[field],
                &json!({"name":field,"type":kind,"array":array,"interval":false,"references":target}),
            )?;
        }
    }
    Ok(())
}
