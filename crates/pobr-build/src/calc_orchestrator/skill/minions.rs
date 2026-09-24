//! skill/minions — summoning-gem recognition and minion wiring.

use pobr_core::calc::CalculationSession;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use super::super::collect::collect_item_texts;
use super::super::skill::resolve::{additional_gem_levels, support_granted_gem_levels};
use crate::build::Build;
use crate::build_data::BuildData;
use pobr_core::calc::minion::AttributeInfusion;

pub(crate) fn spawn_minions(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    extra_texts: &[String],
) {
    use pobr_core::calc::minion::MinionModifierEntry;
    use std::collections::BTreeSet;

    // B3: collects `Minions deal/have …` mods (equipment + extra), wrapped into
    // MinionModifierEntry. These mods don't participate in the player's own aggregation
    // in the main flow (the engine produces a `MinionModifier` LIST mod, and LIST
    // doesn't participate in sum/more/flag) — they only enter the minion ModDb here.
    // Each line is run through `parse_mod_engine`, and `extract_minion_modifier_entries`
    // extracts the `MinionModifier` wrapper from the output. Missing rules (an old data
    // pack) = no parser → produces no minion mods (matching the global "rules not
    // injected → everything Unsupported" semantics).
    let mut minion_modifiers: Vec<MinionModifierEntry> = Vec::new();
    if let Some(rules) = data.parser_rules.as_deref() {
        for text in collect_item_texts(build).iter().chain(extra_texts.iter()) {
            let outcome = pobr_core::mod_parser::parse_mod_engine(text, rules);
            minion_modifiers.extend(pobr_core::calc::minion::extract_minion_modifier_entries(
                &outcome.mods,
            ));
        }
    }

    // Deduplication: the same minion id (referenced by the same minion from different
    // skills/groups) is only wired in once, to avoid double-counting.
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for group in build.enabled_socket_groups() {
        // This group's each **active skill** gem's (granted effect id, gem level).
        // Prefers `gem_skills` (the real import path, includes both active + support);
        // falls back to `active_skill_id` when empty (constructed by the builder/test
        // path's with_active_skill, the same fallback source as resolve_main_skill).
        let candidates: Vec<(&str, u32)> = if group.gem_skills.is_empty() {
            group
                .active_skill_id
                .as_deref()
                .map(|id| vec![(id, group.active_gem_level.unwrap_or(1))])
                .unwrap_or_default()
        } else {
            group
                .gem_skills
                .iter()
                .map(|g| (g.skill_id.as_str(), g.gem_level))
                .collect()
        };
        for (skill_id, gem_level) in candidates {
            // A support itself doesn't summon — its addMinionList appends to the active
            // skill's minion_list, which is deferred; the first version only wires up
            // the active skill's minion_list.
            let is_support = data
                .granted_effects
                .get(skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false);
            if is_support {
                continue;
            }
            let minion_ids = data.effect_minion_list(skill_id);
            if minion_ids.is_empty() {
                continue;
            }
            // Minion level uses the **effective gem level** (matching vendor's
            // `data.minionLevelTable[activeEffect.level]`, CalcActiveSkill.lua:948 —
            // activeEffect.level includes applyGemMods's `+N to Level of all <X>
            // Skills` and levels granted by supports). wolf-pack's "+4 to Level of all
            // Minion Skills": gem 18 → 22 → monster level 44 (pinned by oracle; before
            // the fix, 36 → life 1013 vs 2262).
            let effective_gem_level = gem_level
                .saturating_add(additional_gem_levels(build, data, skill_id))
                .saturating_add(support_granted_gem_levels(group, data, skill_id));
            // (#12 companion) Companion determination: the granted skill has
            // `SkillType.Companion` and not `MinionsAreUndamagable` (matching vendor
            // CalcPerform.lua:3365-3367's includeSkill predicate) → this skill's
            // minions count toward `TotalCompanionLife`.
            let is_companion = data.granted_effects.get(skill_id).is_some_and(|e| {
                e.skill_types.iter().any(|t| t == "Companion")
                    && !e.skill_types.iter().any(|t| t == "MinionsAreUndamagable")
            });
            // (#12) The minion-side payload of a compatible support in the same group
            // (vendor: a support statmap's `MinionModifier LIST` is merged into the
            // supported skill's skillModList → `addMinionModifiers` injects it into
            // **that skill's** minion modDB, in-group scope). Data channel =
            // `map_minion_life_stat` (the first batch only covers inner Life, e.g.
            // Loyalty's −30% more minion life).
            let mut group_minion_modifiers = minion_modifiers.clone();
            if let Some(catalog) = data.stat_map_catalog.as_deref() {
                use pobr_core::calc::minion::MinionModifierEntry;
                use pobr_core::rules::stat_map_engine::map_minion_life_stat;
                for sup in
                    crate::support::judge_group_supports(group, data, skill_id, group.from_gem())
                        .compatible
                {
                    let sup_gem = &group.gem_skills[sup.gem_index];
                    let set_index = (sup_gem.skill_id == sup.effect_id)
                        .then_some(sup_gem.stat_set_index)
                        .flatten();
                    // Quality passed as 0, matching support_modifiers's semantics (supports have no quality table entries).
                    let stats = data.effect_stats(&sup.effect_id, sup_gem.gem_level, 0, set_index);
                    let set_key = data.selected_set_key(&sup.effect_id, set_index);
                    for ds in stats.all() {
                        if ds.value == 0.0 {
                            continue;
                        }
                        for inner in map_minion_life_stat(
                            catalog,
                            &sup.effect_id,
                            set_key.as_deref(),
                            &ds.stat,
                            ds.value,
                        ) {
                            let origin = ModifierSource::new(SourceId::new(
                                SourceKind::SupportGem,
                                format!("minion.{}.{}", sup.effect_id, ds.stat),
                            ))
                            .with_raw_text(format!(
                                "minion {} {} ({})",
                                sup.effect_id, ds.stat, ds.value
                            ));
                            group_minion_modifiers.push(MinionModifierEntry {
                                inner: inner.with_origin(origin),
                                minion_type: None,
                            });
                        }
                    }
                }
            }
            for minion_id in minion_ids {
                if !seen.insert(minion_id.clone()) {
                    continue;
                }
                let Some(def) = data.minion_def(minion_id) else {
                    // minion_list references a minion not in the catalog (the foreign
                    // key is verified to have zero dangling references — defensive
                    // skip, theoretically unreachable).
                    continue;
                };
                // Quantity cap: the sum of the minion's limit stat's (e.g.
                // `ActiveZombieLimit`) player BASE; falls back to 1 when missing. The
                // limit only affects `Multiplier:SummonedMinion` (per-minion mods +
                // DPS aggregation count), not a single minion's life/defence.
                let limit_stat = def.limit.to_pob2_str();
                let limit = if limit_stat.is_empty() {
                    1
                } else {
                    let summed = session.base_sum(limit_stat);
                    if summed >= 1.0 { summed as u32 } else { 1 }
                };
                let def = def.clone();
                // `add_minion_from_def` internally maps gem level to monster level via
                // `minion_level_from_gem_level` (the default rule at
                // CalcActiveSkill.lua:948), so the effective gem level (including the
                // +N to Level bonus) is passed here and must not be pre-resolved
                // (otherwise the mapping would apply twice).
                session.add_minion_from_def(
                    &def,
                    effective_gem_level,
                    limit,
                    group_minion_modifiers.clone(),
                    Vec::new(),
                    AttributeInfusion::default(),
                    is_companion,
                );
            }
        }
    }
}
