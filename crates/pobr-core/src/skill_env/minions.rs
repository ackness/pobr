//! Minion wiring semantics (engine-semantics layer): summoning-gem recognition and
//! minion assembly.
//!
//! Moved from `pobr-build`'s `skill/minions.rs`: collects `Minions deal/have …` mods
//! (equipment + extra), wraps them into `MinionModifierEntry`, and spawns each
//! summoned minion into the session's minion ModDb — driven by lookup traits +
//! `SocketGroupView` + a `MinionSession` write surface.

use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::calc::minion::{AttributeInfusion, MinionModifierEntry};
use crate::skill_env::lookup::{
    EffectLookup, MinionLookup, ParserRulesLookup, SocketGroupView, StatMapLookup,
    StatSetLookup,
};
use crate::skill_env::resolve::{
    GemPropertyLookup, support_granted_gem_levels, support_stat_set_index,
};
use crate::skill_env::support::judge_group_supports;
use crate::skill_env::SupportCandidate;

/// The session write surface [`spawn_minions`] needs: the minion quantity limit's
/// player-side BASE sum + the per-minion spawn sink.
///
/// `pobr_core::calc::CalculationSession` implements this; the trait exists so minion
/// wiring doesn't depend on the concrete session type.
pub trait MinionSession {
    /// `Sum("BASE", nil, name)` over the player db (the minion limit stat source).
    fn base_sum(&self, name: &str) -> f64;

    /// Spawns one minion from its catalog definition (see
    /// [`crate::calc::CalculationSession::add_minion_from_def`]).
    fn add_minion_from_def(
        &mut self,
        def: &pobr_data::minion::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: AttributeInfusion,
        is_companion: bool,
    );
}

impl MinionSession for crate::calc::CalculationSession {
    fn base_sum(&self, name: &str) -> f64 {
        crate::calc::CalculationSession::base_sum(self, name)
    }

    fn add_minion_from_def(
        &mut self,
        def: &pobr_data::minion::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: AttributeInfusion,
        is_companion: bool,
    ) {
        crate::calc::CalculationSession::add_minion_from_def(
            self,
            def,
            gem_level,
            limit,
            minion_modifiers,
            ally_buff_mods,
            infusion,
            is_companion,
        );
    }
}

/// The lookup surface [`spawn_minions`] needs.
pub trait MinionEnv:
    MinionLookup
    + EffectLookup
    + StatSetLookup
    + StatMapLookup
    + ParserRulesLookup
    + GemPropertyLookup
{
}
impl<T> MinionEnv for T where
    T: MinionLookup
        + EffectLookup
        + StatSetLookup
        + StatMapLookup
        + ParserRulesLookup
        + GemPropertyLookup
{
}

/// Wires every summoned minion into the session (B3 + #12 companion/support payload).
///
/// - `item_texts` = every equipped item's enchant/implicit/explicit lines (the
///   `Minions deal/have …` scan surface; the caller collects them via
///   `collect_item_texts`), chained with `extra_texts`;
/// - `gem_level_bonuses` = the build's GemProperty Level bonuses (the
///   `additional_gem_levels` input, assembled once by the caller via
///   [`crate::skill_env::gem_property_bonuses`]).
///
/// Semantics: the mods don't participate in the player's own aggregation in the main
/// flow (the engine produces a `MinionModifier` LIST mod, and LIST doesn't participate
/// in sum/more/flag) — they only enter the minion ModDb here. Each line is run through
/// `parse_mod_engine`, and `extract_minion_modifier_entries` extracts the
/// `MinionModifier` wrapper from the output. Missing rules (an old data pack) = no
/// parser → produces no minion mods (matching the global "rules not injected →
/// everything Unsupported" semantics).
pub fn spawn_minions(
    session: &mut dyn MinionSession,
    groups: &dyn SocketGroupView,
    data: &dyn MinionEnv,
    item_texts: &[String],
    extra_texts: &[String],
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
) {
    use std::collections::BTreeSet;

    let mut minion_modifiers: Vec<MinionModifierEntry> = Vec::new();
    if let Some(rules) = data.parser_rules() {
        for text in item_texts.iter().chain(extra_texts.iter()) {
            let outcome = crate::mod_parser::parse_mod_engine(text, rules);
            minion_modifiers.extend(crate::calc::minion::extract_minion_modifier_entries(
                &outcome.mods,
            ));
        }
    }

    // Deduplication: the same minion id (referenced by the same minion from different
    // skills/groups) is only wired in once, to avoid double-counting.
    let mut seen: BTreeSet<String> = BTreeSet::new();

    groups.for_each_enabled_group(&mut |group| {
        // This group's each **active skill** gem's (granted effect id, gem level).
        // Prefers `gems` (the real import path, includes both active + support);
        // falls back to `active_skill_id` when empty (the builder/test path's
        // with_active_skill, the same fallback source as resolve_main_skill).
        let candidates: Vec<(String, u32)> = if group.gems.is_empty() {
            group
                .active_skill_id
                .map(|id| vec![(id.to_string(), group.active_gem_level)])
                .unwrap_or_default()
        } else {
            group
                .gems
                .iter()
                .map(|g| (g.skill_id.clone(), g.gem_level))
                .collect()
        };
        for (skill_id, gem_level) in candidates {
            // A support itself doesn't summon — its addMinionList appends to the active
            // skill's minion_list, which is deferred; the first version only wires up
            // the active skill's minion_list.
            let is_support = data
                .effect(&skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false);
            if is_support {
                continue;
            }
            let minion_ids = data.effect_minion_list(&skill_id);
            if minion_ids.is_empty() {
                continue;
            }
            // Minion level uses the **effective gem level** (matching vendor's
            // `data.minionLevelTable[activeEffect.level]`, CalcActiveSkill.lua:948 —
            // activeEffect.level includes applyGemMods's `+N to Level of all <X>
            // Skills` and levels granted by supports). wolf-pack's "+4 to Level of all
            // Minion Skills": gem 18 → 22 → monster level 44 (pinned by oracle; before
            // the fix, 36 → life 1013 vs 2262).
            let judgement = group_judgement(&group, data, &skill_id);
            let effective_gem_level = gem_level
                .saturating_add(crate::skill_env::additional_gem_levels(
                    gem_level_bonuses,
                    data,
                    &skill_id,
                ))
                .saturating_add(support_granted_gem_levels(&judgement, group.gems, data));
            // (#12 companion) Companion determination: the granted skill has
            // `SkillType.Companion` and not `MinionsAreUndamagable` (matching vendor
            // CalcPerform.lua:3365-3367's includeSkill predicate) → this skill's
            // minions count toward `TotalCompanionLife`.
            let is_companion = data.effect(&skill_id).is_some_and(|e| {
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
            if let Some(catalog) = data.stat_map_catalog() {
                use crate::rules::stat_map_engine::map_minion_life_stat;
                for sup in &judgement.compatible {
                    let sup_gem = &group.gems[sup.gem_index];
                    let set_index = support_stat_set_index(sup, group.gems);
                    // Quality passed as 0, matching support_modifiers's semantics (supports have no quality table entries).
                    let stats =
                        data.effect_stats(&sup.effect_id, sup_gem.gem_level, 0, set_index);
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
    });
}

/// Runs the group support judgement for `skill_id` over an [`EnabledGroup`] (the
/// engine-semantics counterpart of `pobr-build`'s `judge_group_supports` wrapper):
/// assembles candidates from the group's gems + additional granted effects.
fn group_judgement(
    group: &crate::skill_env::EnabledGroup<'_>,
    data: &dyn MinionEnv,
    skill_id: &str,
) -> crate::skill_env::GroupSupportJudgement {
    use std::collections::HashSet;

    let active_skill_types: HashSet<String> = data
        .effect(skill_id)
        .map(|e| e.skill_types.iter().cloned().collect())
        .unwrap_or_default();
    let cannot_be_supported = data
        .effect(skill_id)
        .is_some_and(|e| e.cannot_be_supported);

    let candidates: Vec<SupportCandidate<'_>> = group
        .gems
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            std::iter::once(g.skill_id.as_str())
                .chain(data.additional_effects(&g.skill_id).iter().map(String::as_str))
                .filter_map(move |id| {
                    data.effect(id)
                        .filter(|e| e.is_support)
                        .map(|effect| SupportCandidate {
                            gem_index: i,
                            effect_id: id,
                            effect,
                        })
                })
        })
        .collect();

    judge_group_supports(
        &active_skill_types,
        cannot_be_supported,
        group.from_gem,
        &candidates,
    )
}
