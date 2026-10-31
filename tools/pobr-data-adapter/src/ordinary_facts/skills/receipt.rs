//! Source identities and omitted facts, without copying the per-level source tables.

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::SkillStatSetDef;
use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::skills::SkillsBundle;

use super::validation::{FIELDS, PhysicalSkills, index, list};

pub(super) fn build(
    input: &VerifiedFacts,
    source: &PhysicalSkills<'_>,
    bundle: &SkillsBundle,
    converted_sets: &[SkillStatSetDef],
) -> Result<Value, String> {
    let tables = &source.tables;
    let effect_levels = groups(tables, "GrantedEffectsPerLevel", "GrantedEffect")?;
    let set_levels = groups(tables, "GrantedEffectStatSetsPerLevel", "StatSet")?;
    let experience_levels = groups(tables, "ItemExperiencePerLevel", "ItemExperienceType")?;
    let sets_by_effect: BTreeMap<_, _> = converted_sets
        .iter()
        .map(|s| (s.effect_id.as_str(), s))
        .collect();
    let mut gem_sources = Vec::new();
    let mut excluded_gems = Vec::new();
    for row in 0..tables.len("SkillGems") {
        let raw = tables.row("SkillGems", row)?;
        let base_row = index(&raw["BaseItemType"])?;
        let base = tables.row("BaseItemTypes", base_row)?;
        let variant_row = index(&raw["GemEffects"][0])?;
        let variant = tables.row("GemEffects", variant_row)?;
        gem_sources.push(
            json!({"source_row":row,"id":base["Id"],"base_source_row":base_row,
            "gem_effect_source_row":variant_row,"variant_id":variant["Id"],
            "experience_type_source_row":raw["ItemExperienceType"]}),
        );
        if crate::is_placeholder(base["Name"].as_str().ok_or("invalid base Name")?) {
            excluded_gems.push(
                json!({"source_row":row,"id":base["Id"],"base_source_row":base_row,
                "name":base["Name"],"reason":"existing base-name placeholder rule"}),
            );
        }
    }
    let mut effect_sources = Vec::new();
    let mut repeated_set_references = Vec::new();
    let mut empty_stat_effects = Vec::new();
    let mut omitted_stat_levels = Vec::new();
    let mut zero_effect_levels = Vec::new();
    for row in 0..tables.len("GrantedEffects") {
        let raw = tables.row("GrantedEffects", row)?;
        let id = raw["Id"].as_str().ok_or("invalid effect Id")?;
        let main = index(&raw["StatSet"])?;
        let mut set_rows = vec![main];
        set_rows.extend(
            list(&raw["AdditionalStatSets"])?
                .iter()
                .map(index)
                .collect::<Result<Vec<_>, _>>()?,
        );
        let mut occurrences: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (position, &set_row) in set_rows.iter().enumerate() {
            occurrences.entry(set_row).or_default().push(position);
        }
        for (set_row, positions) in occurrences {
            if positions.len() > 1 {
                repeated_set_references.push(json!({"effect_id":id,"effect_source_row":row,
                    "owner_payload":"tables/GrantedEffects.json","target_payload":"tables/GrantedEffectStatSets.json",
                    "set_id":tables.row("GrantedEffectStatSets",set_row)?["Id"],
                    "set_source_row":set_row,"branch_positions":positions,
                    "primary_target":raw["StatSet"],"additional_targets":raw["AdditionalStatSets"],
                    "position_convention":"primary=0, additional=1.. in source order; not a vendor/import ordinal",
                    "policy":"ordered repeated references preserved by the original converter; not duplicate source table keys"}));
            }
        }
        effect_sources.push(json!({"source_row":row,"id":id,
            "active_skill_source_row":raw["ActiveSkill"],
            "stat_set_source_rows":set_rows,
            "level_source_rows":ranges(effect_levels.get(&row).map(Vec::as_slice).unwrap_or(&[]))}));
        let output = sets_by_effect.get(id);
        if output.is_none() {
            empty_stat_effects.push(json!({"source_row":row,"id":id,"stat_set_source_rows":set_rows,
                "reason":"all converted sets have no constants, implicit stats or retained levels; effect definition itself is retained"}));
        }
        for (position, &set_row) in set_rows.iter().enumerate() {
            let set = tables.row("GrantedEffectStatSets", set_row)?;
            let output_levels: BTreeSet<_> = output
                .and_then(|effect| effect.sets.get(position))
                .map(|set| {
                    set.levels
                        .iter()
                        .map(|level| u64::from(level.gem_level))
                        .collect()
                })
                .unwrap_or_default();
            let mut zero = Vec::new();
            let mut trivial = Vec::new();
            for &level_row in set_levels.get(&set_row).map(Vec::as_slice).unwrap_or(&[]) {
                let level = tables.row("GrantedEffectStatSetsPerLevel", level_row)?["GemLevel"]
                    .as_u64()
                    .ok_or("invalid GemLevel")?;
                if level == 0 {
                    zero.push(level_row);
                } else if !output_levels.contains(&level) {
                    trivial.push(level_row);
                }
            }
            for (reason, rows) in [
                ("level zero", zero),
                ("no stats and default merged multiplier", trivial),
            ] {
                if !rows.is_empty() {
                    omitted_stat_levels.push(json!({"effect_id":id,"effect_source_row":row,
                        "set_id":set["Id"],"set_source_row":set_row,"branch_position":position,
                        "reason":reason,"source_rows":ranges(&rows)}));
                }
            }
        }
        let mut zero_rows = Vec::new();
        for &r in effect_levels.get(&row).map(Vec::as_slice).unwrap_or(&[]) {
            if tables.row("GrantedEffectsPerLevel", r)?["Level"] == 0 {
                zero_rows.push(r);
            }
        }
        if !zero_rows.is_empty() {
            zero_effect_levels.push(
                json!({"effect_id":id,"source_rows":ranges(&zero_rows),"reason":"level zero"}),
            );
        }
    }
    let mut set_sources = Vec::new();
    for row in 0..tables.len("GrantedEffectStatSets") {
        let raw = tables.row("GrantedEffectStatSets", row)?;
        set_sources.push(
            json!({"source_row":row,"id":raw["Id"],"label_source_row":raw["Label"],
            "level_source_rows":ranges(set_levels.get(&row).map(Vec::as_slice).unwrap_or(&[]))}),
        );
    }
    let mut experience_sources = Vec::new();
    let mut requirement_normalizations = Vec::new();
    for row in 0..tables.len("ItemExperienceTypes") {
        let id = &tables.row("ItemExperienceTypes", row)?["Id"];
        let rows = experience_levels
            .get(&row)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        experience_sources.push(json!({"source_row":row,"id":id,"level_source_rows":ranges(rows)}));
        let mut sorted = Vec::new();
        for &r in rows {
            let raw = tables.row("ItemExperiencePerLevel", r)?;
            sorted.push((index(&raw["ItemCurrentLevel"])?, r, index(&raw["Level"])?));
        }
        sorted.sort_unstable();
        let mut previous = 0;
        for (level, r, value) in sorted {
            if level == 0 {
                requirement_normalizations
                    .push(json!({"type_id":id,"source_row":r,"reason":"item level zero excluded"}));
            } else {
                if value < previous {
                    requirement_normalizations
                        .push(json!({"type_id":id,"source_row":r,"raw":value,
                        "converted":previous,"reason":"existing nondecreasing requirement rule"}));
                }
                previous = previous.max(value);
            }
        }
    }
    let mut cost_normalizations = Vec::new();
    for row in 0..tables.len("CostTypes") {
        let raw = tables.row("CostTypes", row)?;
        if raw["Divisor"].as_i64().ok_or("invalid Divisor")? <= 0 {
            cost_normalizations.push(json!({"source_row":row,"id":raw["Id"],"raw":raw["Divisor"],
                "converted":1,"reason":"existing nonpositive divisor normalization"}));
        }
    }
    let mut unmapped = BTreeMap::new();
    for (table, fields) in FIELDS {
        let payload = format!("tables/{table}.json");
        let mut used: BTreeSet<_> = fields.split_whitespace().collect();
        if table == "GrantedEffectsPerLevel" && source.optional_level_multiplier {
            used.insert("BaseMultiplier");
        }
        let columns: Vec<_> = list(&input.documents[&payload]["physical_fields"])?
            .iter()
            .filter(|v| v.as_str().is_some_and(|f| !used.contains(f)))
            .cloned()
            .collect();
        unmapped.insert(
            table,
            json!({"payload":payload,"columns":columns,
            "source_rows":{"start":0,"end_exclusive":tables.len(table)},
            "affected_id_resolution":identity_resolution(table),
            "status":"retained in pinned source; not applied by this domain"}),
        );
    }
    Ok(json!({
        "skill_gems":bundle.gems.len(),"granted_effects":bundle.effects.len(),
        "granted_effect_level_rows":bundle.levels.values().map(Vec::len).sum::<usize>(),
        "granted_effect_stat_set_effects":converted_sets.len(),"cost_types":tables.len("CostTypes"),
        "provenance": {"gems":gem_sources,"effects":effect_sources,"stat_sets":set_sources,"experience_types":experience_sources,
            "row_ranges":"half-open ranges in ascending physical source order; no sort by gem level before positional pairing",
            "additional_set_merge":"effect.stat_set_source_rows[0] is primary; each following branch pairs its source level rows with primary source level rows by array position, including level-zero source rows; missing paired rows retain existing fallback",
            "branch_position":"ordinary output order only; never a vendor/import statSetIndex",
            "row_reference":"tables/{table}.json#/rows/{source_row}/values"},
        "excluded_gems":excluded_gems,"empty_stat_set_effects":empty_stat_effects,
        "repeated_set_references":repeated_set_references,
        "omitted_stat_set_levels":omitted_stat_levels,"omitted_effect_levels":zero_effect_levels,
        "requirement_normalizations":requirement_normalizations,"cost_normalizations":cost_normalizations,
        "unmapped_fields":unmapped,
        "direct_source_additions":[{"source":"GrantedEffects.IgnoreMinionTypes","field":"GrantedEffectDef.ignore_minion_types","affected_ids":"provenance.effects","raw_mode":"unchanged false/overlay behavior"}],
        "optional_columns":{"GrantedEffectsPerLevel.BaseMultiplier":{"present":source.optional_level_multiplier,"absent_behavior":"existing None fallback; stat-set BaseMultiplier remains directly converted"}},
        "mapping":{
            "converter":"shared skills::{gems,effects,levels,stat_sets}; physical _row supplies legacy _index only after Tables::selected validation",
            "gems":"base stable ID; GemEffects variant/primary/additional IDs and tags in source order; experience requirements sorted by item level, gap fill and nondecreasing rule unchanged",
            "effects":"active ID/name/types, support postfix streams and flags unchanged; primary stat-set and cost type indices retained; additional set IDs resolved in order",
            "levels":"existing cooldown/attack time/costs, signed speed multiplier, primary or first additional crit map and offhand override; multiplier/100/zero normalizations unchanged",
            "stat_sets":"all stats retained, constants/main+additional source-order positional merge, ignored first occurrence, flags, labels, multiplier and effectiveness fallbacks and fewer-than-five-row actor scaling unchanged",
            "cost_types":"physical index order, Id/PerMinute and positive Divisor; nonpositive Divisor normalized to 1 with source evidence",
            "absence":"only ActiveSkill, Label and DisplayedName allow null; present numeric facts are checked rather than defaulted or clamped; optional GPL BaseMultiplier can be absent/null"
        },
        "pending_compatibility":[
            {"fields":["vendor_set_index"],"affected_ids":"provenance.effects and provenance.stat_sets","reason":"import/export ordinal is not the raw set array position"},
            {"fields":["quality effect-vs-set applicability","gem_quality_stats"],"affected_ids":"provenance.gems and provenance.effects","reason":"quality scope not migrated; no quality sidecar or another patch's values"},
            {"fields":["skill_overrides/baseMods","target-patch stat-map applicability","skill_attack_speed_more","dot_flags","explode_corpse","level_requirement on effect level"],"affected_ids":"provenance.effects and provenance.stat_sets","reason":"compatible prepared assets pending; gem requirements are independently present on SkillGemDef; retaining a stat is not proof of mapper coverage"},
            {"fields":["minion_list","add_minion_list","minion_uses","minion_has_item_set"],"affected_ids":"provenance.effects","reason":"prepared minion/equipment mappings pending; empty/false does not certify absence of minion behavior"},
            {"fields":["translation","gem effect display ordering outside SkillGemDef"],"affected_ids":"provenance.gems and provenance.effects","reason":"no language/vendor sidecar in this domain"}
        ]
    }))
}

fn groups(
    tables: &Tables<'_>,
    table: &str,
    field: &str,
) -> Result<BTreeMap<usize, Vec<usize>>, String> {
    let mut out: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for row in 0..tables.len(table) {
        out.entry(index(&tables.row(table, row)?[field])?)
            .or_default()
            .push(row);
    }
    Ok(out)
}

fn ranges(rows: &[usize]) -> Vec<[usize; 2]> {
    let mut out: Vec<[usize; 2]> = Vec::new();
    for &row in rows {
        if let Some(last) = out.last_mut()
            && last[1] == row
        {
            last[1] = row + 1;
        } else {
            out.push([row, row + 1]);
        }
    }
    out
}

fn identity_resolution(table: &str) -> &'static str {
    match table {
        "SkillGems" | "BaseItemTypes" | "GemEffects" => {
            "provenance.gems supplies affected gem/variant IDs and base/variant source rows; unreferenced source rows retain their own Id"
        }
        "GrantedEffects" | "GrantedEffectsPerLevel" | "ActiveSkills" => {
            "provenance.effects supplies effect IDs, active source rows and level row ranges; unreferenced ActiveSkills retain their own Id"
        }
        "GrantedEffectStatSets" | "GrantedEffectStatSetsPerLevel" | "GrantedEffectLabels" => {
            "provenance.stat_sets supplies set IDs, label source rows and level row ranges; provenance.effects supplies all owning effect IDs in branch order"
        }
        "ItemExperienceTypes" | "ItemExperiencePerLevel" => {
            "provenance.experience_types supplies type IDs and level row ranges; provenance.gems supplies affected base IDs"
        }
        _ => {
            "identity-only dependency: Id at the pinned source row; references remain in the source and converted stable IDs/raw cost indices, no other semantics certified"
        }
    }
}
