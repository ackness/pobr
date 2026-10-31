//! Target skin-zero totem objects, with unresolved source selectors kept explicit.

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::totem_varieties::{TotemVarietiesDef, TotemVarietyDef};
use serde::Serialize;
use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;

pub(super) const TABLES: [&str; 6] = [
    "GrantedEffects",
    "ActiveSkills",
    "ActiveSkillType",
    "SkillTotemVariations",
    "MonsterVarieties",
    "MonsterTypes",
];

pub(super) struct Converted {
    pub overlay: BTreeMap<String, Vec<u8>>,
    pub receipt: Value,
}

#[derive(Serialize)]
struct Reference {
    source_row: usize,
    id: String,
}

struct Active {
    selector: u32,
    tags: Vec<Reference>,
}

#[derive(Serialize)]
struct Variation {
    source_row: usize,
    selector: u32,
    skin_id: i32,
    variety: Reference,
    monster_type: Reference,
    life_multiplier: i32,
    armour: i32,
    evasion: i32,
    energy_shield_from_life: i32,
}

/// Compact source locations for large unmatched/null partitions, in physical order.
#[derive(Default, Serialize)]
struct RowRanges {
    count: usize,
    ranges: Vec<[usize; 2]>,
}

impl RowRanges {
    fn push(&mut self, row: usize) {
        self.count += 1;
        if let Some(last) = self.ranges.last_mut().filter(|last| last[1] == row) {
            last[1] = row + 1;
        } else {
            self.ranges.push([row, row + 1]);
        }
    }
}

pub(super) fn convert(input: &VerifiedFacts) -> Result<Converted, String> {
    let tables = Tables::selected(input, &TABLES)?;
    check_columns(input)?;
    // Validate consumed signed physical values and references even on unused rows.
    // Unsigned DTO constraints apply only to emitted objects, not unrelated facts.
    for row in 0..tables.len("MonsterTypes") {
        for field in ["Armour", "Evasion", "EnergyShieldFromLife"] {
            signed(tables.row("MonsterTypes", row)?, field)
                .map_err(|e| format!("MonsterTypes/{row}: {e}"))?;
        }
    }
    for row in 0..tables.len("MonsterVarieties") {
        let values = tables.row("MonsterVarieties", row)?;
        signed(values, "LifeMultiplier").map_err(|e| format!("MonsterVarieties/{row}: {e}"))?;
        resolve(&tables, "MonsterTypes", &values["MonsterType"])
            .map_err(|e| format!("MonsterVarieties/{row}.MonsterType: {e}"))?;
    }
    let mut variations = Vec::new();
    let mut skin_zero = BTreeMap::new();
    for source_row in 0..tables.len("SkillTotemVariations") {
        let values = tables.row("SkillTotemVariations", source_row)?;
        let variation = (|| {
            let selector = unsigned(&values["SkillTotemsKey"], "SkillTotemsKey selector")?;
            let skin_id = signed(values, "TotemSkinId")?;
            let variety = resolve(&tables, "MonsterVarieties", &values["MonsterVarietiesKey"])?;
            let variety_values = tables.row("MonsterVarieties", variety.source_row)?;
            let monster_type = resolve(&tables, "MonsterTypes", &variety_values["MonsterType"])?;
            let type_values = tables.row("MonsterTypes", monster_type.source_row)?;
            Ok::<_, String>(Variation {
                source_row,
                selector,
                skin_id,
                variety,
                monster_type,
                life_multiplier: signed(variety_values, "LifeMultiplier")?,
                armour: signed(type_values, "Armour")?,
                evasion: signed(type_values, "Evasion")?,
                energy_shield_from_life: signed(type_values, "EnergyShieldFromLife")?,
            })
        })()
        .map_err(|e| format!("SkillTotemVariations/{source_row}: {e}"))?;
        if variation.skin_id == 0 && skin_zero.insert(variation.selector, source_row).is_some() {
            return Err(format!(
                "ambiguous skin-zero variations for selector {}",
                variation.selector
            ));
        }
        variations.push(variation);
    }
    let mut actives = Vec::new();
    for row in 0..tables.len("ActiveSkills") {
        let values = tables.row("ActiveSkills", row)?;
        let selector = unsigned(&values["SkillTotemId"], "SkillTotemId selector")
            .map_err(|e| format!("ActiveSkills/{row}: {e}"))?;
        let tags = values["ActiveSkillTypes"]
            .as_array()
            .ok_or_else(|| format!("ActiveSkills/{row}.ActiveSkillTypes: array required"))?
            .iter()
            .enumerate()
            .map(|(position, v)| {
                resolve(&tables, "ActiveSkillType", v)
                    .map_err(|e| format!("ActiveSkills/{row}.ActiveSkillTypes[{position}]: {e}"))
            })
            .collect::<Result<_, _>>()?;
        actives.push(Active { selector, tags });
    }

    let mut matched = Vec::new();
    let mut unresolved = Vec::new();
    let mut unmatched: BTreeMap<u32, RowRanges> = BTreeMap::new();
    let mut null_active = RowRanges::default();
    let mut used_variations = BTreeSet::new();
    for row in 0..tables.len("GrantedEffects") {
        let values = tables.row("GrantedEffects", row)?;
        if values["ActiveSkill"].is_null() {
            null_active.push(row);
            continue;
        }
        let active_ref = resolve(&tables, "ActiveSkills", &values["ActiveSkill"])
            .map_err(|e| format!("GrantedEffects/{row}.ActiveSkill: {e}"))?;
        let active = &actives[active_ref.source_row];
        let effect_id = values["Id"].as_str().ok_or("invalid effect Id")?;
        let tags: Vec<_> = active
            .tags
            .iter()
            .enumerate()
            .map(|(position, target)| json!({"position":position,"target":target}))
            .collect();
        let source = json!({
            "effect":{"source_row":row,"id":effect_id},"active_skill":active_ref,
            "selector":active.selector,"active_skill_types":tags
        });
        let Some(&variation_row) = skin_zero.get(&active.selector) else {
            unmatched.entry(active.selector).or_default().push(row);
            // The tag is diagnostic only. It never excludes a matched object.
            if active.tags.iter().any(|tag| tag.id == "SummonsTotem") {
                unresolved.push(json!({"source":source,"reason":"no skin-zero source variation; no inferred variety or numeric fallback","status":"unresolved; not effectless"}));
            }
            continue;
        };
        let variation = &variations[variation_row];
        let dto_value = |field: &str, value: i32| {
            u32::try_from(value).map_err(|_| {
                format!(
                    "GrantedEffects/{row} ({effect_id}): \
                     selected {field} is outside u32 range: {value}"
                )
            })
        };
        let entry = TotemVarietyDef {
            granted_effect: effect_id.into(),
            active_skill: active_ref.id.clone(),
            skill_totem_id: active.selector,
            monster_variety: variation.variety.id.clone(),
            life_multiplier: dto_value("LifeMultiplier", variation.life_multiplier)?,
            armour: dto_value("Armour", variation.armour)?,
            evasion: dto_value("Evasion", variation.evasion)?,
            energy_shield_from_life: dto_value(
                "EnergyShieldFromLife",
                variation.energy_shield_from_life,
            )?,
        };
        used_variations.insert(variation_row);
        matched.push((entry, json!({"source":source,"variation":variation})));
    }
    // GrantedEffects identity was checked by Tables; aliases sharing an active row
    // retain their own effect key and are all emitted.
    matched.sort_by(|a, b| a.0.granted_effect.cmp(&b.0.granted_effect));
    let (totems, records): (Vec<_>, Vec<_>) = matched.into_iter().unzip();
    let data = TotemVarietiesDef { totems };
    let variation_records: Vec<_> = variations
        .iter()
        .map(|v| {
            let status = if v.skin_id != 0 {
                "other-skin-unapplied"
            } else if used_variations.contains(&v.source_row) {
                "matched-skin-zero"
            } else {
                "unreferenced-skin-zero"
            };
            json!({"source":v,"status":status})
        })
        .collect();
    let inputs: BTreeMap<_, _> = TABLES
        .iter()
        .map(|table| {
            let path = format!("tables/{table}.json");
            let hash = sha256_hex(&input.bytes[&path]);
            (path, hash)
        })
        .collect();
    let document = json!({
        "_meta":{"schema":"totem-varieties/v1","generator":"pobr-data-adapter ordinary-facts totem-varieties",
            "poe_version":input.manifest["source_identity"]["game_patch"],"source_identity":input.manifest["source_identity"],
            "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"inputs":inputs},
        "totems":data.totems
    });
    let receipt = json!({
        "totems":data.totems.len(),"source_records":records,"unresolved_summons_totem":unresolved,
        "unmatched_selectors":unmatched,"null_active_effects":null_active,
        "summary_locations":{"table":"GrantedEffects","identity":"Id","ranges":"zero-based [start,end_exclusive) physical row ranges; unmatched includes separately detailed tagged unresolved effects"},
        "variation_records":variation_records,
        "selector_join":{"namespace":"SkillTotems","columns":[
            {"table":"ActiveSkills","column":"SkillTotemId","type":"enumrow"},
            {"table":"SkillTotemVariations","column":"SkillTotemsKey","type":"enumrow"}],
            "semantics":"opaque value equality only; declared external namespace content is not read, fabricated or certified; selector is not a variation row index"},
        "mapping":{
            "effect":"GrantedEffects.Id; nullable ActiveSkill physical FK -> ActiveSkills.Id; ActiveSkills.GrantedEffect is not used as a reverse link",
            "object":"unique selector match with TotemSkinId == 0 -> MonsterVarietiesKey -> MonsterVarieties.MonsterType",
            "numbers":"LifeMultiplier/Armour/Evasion/EnergyShieldFromLife in original raw percentage units; signed source validated, selected values checked into u32; no unit conversion/default/growth rule",
            "tags":"ordered ActiveSkillTypes references retained, duplicates preserved; SummonsTotem used only for unresolved diagnostics, never matched-row admission"
        },
        "unconsumed_source":unmapped(input,&tables)?,
        "pending":"unresolved object identities and other skins are not effectless; monster resistances use the separate domain; minion/totem catalogs and level rules, full target package and ordinary BD/Web acceptance remain separate"
    });
    Ok(Converted {
        overlay: BTreeMap::from([(
            "totem_varieties.json".into(),
            super::pretty_bytes(&document)?,
        )]),
        receipt,
    })
}

fn signed(values: &Value, field: &str) -> Result<i32, String> {
    values[field]
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| format!("{field}: signed i32 required"))
}

fn unsigned(value: &Value, field: &str) -> Result<u32, String> {
    value
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("{field}: unsigned u32 required"))
}

fn resolve(tables: &Tables<'_>, table: &str, value: &Value) -> Result<Reference, String> {
    let source_row = value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("required physical reference must be usize")?;
    let id = tables.row(table, source_row)?["Id"]
        .as_str()
        .ok_or("invalid stable Id")?;
    Ok(Reference {
        source_row,
        id: id.into(),
    })
}

fn used(table: &str) -> &'static [&'static str] {
    match table {
        "GrantedEffects" => &["Id", "ActiveSkill"],
        "ActiveSkills" => &["Id", "SkillTotemId", "ActiveSkillTypes"],
        "SkillTotemVariations" => &["SkillTotemsKey", "TotemSkinId", "MonsterVarietiesKey"],
        "MonsterVarieties" => &["Id", "MonsterType", "LifeMultiplier"],
        "MonsterTypes" => &["Id", "Armour", "Evasion", "EnergyShieldFromLife"],
        _ => &["Id"],
    }
}

fn check_columns(input: &VerifiedFacts) -> Result<(), String> {
    for table in TABLES {
        for &field in used(table) {
            let (kind, array, target) = match field {
                "Id" => ("string", false, None),
                "ActiveSkill" => ("foreignrow", false, Some("ActiveSkills")),
                "ActiveSkillTypes" => ("foreignrow", true, Some("ActiveSkillType")),
                "SkillTotemId" | "SkillTotemsKey" => ("enumrow", false, Some("SkillTotems")),
                "MonsterVarietiesKey" => ("foreignrow", false, Some("MonsterVarieties")),
                "MonsterType" => ("foreignrow", false, Some("MonsterTypes")),
                _ => ("i32", false, None),
            };
            super::convert::column(
                input,
                table,
                &[field],
                &json!({"name":field,"type":kind,"array":array,"interval":false,"references":target.map(|t|json!({"table":t}))}),
            )?;
        }
    }
    Ok(())
}

fn unmapped(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    for table in TABLES {
        let doc = &input.documents[&format!("tables/{table}.json")];
        let columns: Vec<_> = doc["columns"]
            .as_array()
            .ok_or("missing columns")?
            .iter()
            .filter(|c| {
                c["physical_fields"].as_array().is_some_and(|fields| {
                    fields
                        .iter()
                        .any(|f| !used(table).iter().any(|name| f.as_str() == Some(*name)))
                })
            })
            .collect();
        result.push(json!({"table":table,"payload":format!("tables/{table}.json"),"columns":columns,
            "source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "identity":"Id in each pinned physical row, except SkillTotemVariations uses physical row and opaque selector/skin; source_records/variation_records and compressed effect summaries locate the joined subset",
            "status":"remaining columns and unreferenced/other-skin facts are not applied, not declared effectless"}));
    }
    Ok(result)
}
