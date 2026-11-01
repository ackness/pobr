//! Source-owned effect identities within explicitly retained item/export aliases.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts, selected};
use crate::ordinary_facts::convert::{column, resolve, text};
use crate::raw_source::sha256_hex;

fn row(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("invalid item skill reference".into())
}

pub(super) fn project(
    input: &VerifiedFacts,
    extra: &selected::Input,
    document: &mut Value,
) -> Result<Value, String> {
    let base = Tables::selected(input, &super::LINK_TABLES)?;
    let links = extra.tables(&["ModGrantedSkills"])?;
    for (field, target) in [("Mod", "Mods"), ("Skill", "SkillGems")] {
        extra.column(
            "ModGrantedSkills",
            field,
            &json!({"name":field,"type":"foreignrow",
            "array":false,"interval":false,"references":{"table":target}}),
        )?;
    }
    for (table, field, kind, array, target) in [
        ("Mods", "Id", "string", false, None),
        (
            "SkillGems",
            "GemEffects",
            "foreignrow",
            true,
            Some("GemEffects"),
        ),
        ("GemEffects", "Id", "string", false, None),
        (
            "GemEffects",
            "GrantedEffect",
            "foreignrow",
            false,
            Some("GrantedEffects"),
        ),
        ("GrantedEffects", "Id", "string", false, None),
    ] {
        column(
            input,
            table,
            &[field],
            &json!({"name":field,"type":kind,"array":array,
            "interval":false,"references":target.map(|t|json!({"table":t}))}),
        )?;
    }
    let mut records = Vec::new();
    let mut by_mod = BTreeMap::<String, Vec<usize>>::new();
    for index in 0..links.len("ModGrantedSkills") {
        let raw = links.row("ModGrantedSkills", index)?;
        let mod_id = resolve(&base, "Mods", &raw["Mod"], "Id")?;
        let skill_row = row(&raw["Skill"])?;
        let skill = base.row("SkillGems", skill_row)?;
        let name = resolve(&base, "BaseItemTypes", &skill["BaseItemType"], "Name")?;
        let gem_id = resolve(&base, "BaseItemTypes", &skill["BaseItemType"], "Id")?;
        let effects = skill["GemEffects"].as_array().ok_or("invalid gem effect list")?
            .iter().map(|reference| {
                let effect_row = row(reference)?;
                let effect = base.row("GemEffects", effect_row)?;
                Ok(json!({"gem_effect_row":effect_row,"gem_effect_id":text(&effect["Id"])?,
                    "granted_effect_row":row(&effect["GrantedEffect"])?,
                    "granted_effect_id":resolve(&base,"GrantedEffects",&effect["GrantedEffect"],"Id")?}))
            }).collect::<Result<Vec<_>,String>>()?;
        by_mod.entry(mod_id.clone()).or_default().push(index);
        records.push(json!({"source_row":index,"raw":raw,"game_mod_id":mod_id,
            "skill_row":skill_row,"skill_base_row":skill["BaseItemType"],"skill_base_id":gem_id,
            "skill_name":name,"effects":effects}));
    }
    let mut used = BTreeSet::new();
    let mut bindings = Vec::new();
    if let Some(variants) = document
        .as_object_mut()
        .ok_or("invalid item grants object")?
        .get_mut("variants")
    {
        for (index, variant) in variants
            .as_array_mut()
            .ok_or("invalid item variants")?
            .iter_mut()
            .enumerate()
        {
            let id = text(&variant["game_mod_id"])?;
            let sources = by_mod
                .get(id)
                .ok_or_else(|| format!("missing item grant for {id}"))?;
            if sources.len() != 1 {
                return Err(format!("ambiguous item grant rows for {id}"));
            }
            let source = sources[0];
            let record = &records[source];
            let effects = record["effects"]
                .as_array()
                .ok_or("invalid recorded effects")?;
            if effects.len() != 1 || text(&record["skill_name"])?.is_empty() {
                return Err(format!(
                    "item grant {id} needs exactly one named primary effect"
                ));
            }
            bindings.push(json!({"variant_index":index,"game_mod_id":id,"source_row":source}));
            variant["skill_name"] = record["skill_name"].clone();
            variant["granted_effect_id"] = effects[0]["granted_effect_id"].clone();
            used.insert(source);
        }
    }
    let unbound: Vec<_> = (0..records.len()).filter(|r| !used.contains(r)).collect();
    Ok(
        json!({"records":records,"bindings":bindings,"unbound_source_rows":unbound,
        "supplement_manifest_sha256":sha256_hex(&extra.manifest_bytes),"supplement":extra.manifest,
        "ownership":"Only skill_name and granted_effect_id of retained variants are replaced by target joins selected by game_mod_id. All item/base names, exported effect aliases and unknown fields retain the pinned legacy source. Unbound source links do not prove item ownership or roll relationships."}),
    )
}
