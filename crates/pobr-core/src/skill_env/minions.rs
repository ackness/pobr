//! Minion wiring semantics (engine-semantics layer): summoning-gem recognition and
//! minion assembly.
//!
//! Moved from `pobr-build`'s `skill/minions.rs`: collects `Minions deal/have …` mods
//! (equipment + extra), wraps them into `MinionModifierEntry`, and spawns each
//! summoned minion into the session's minion ModDb — driven by lookup traits +
//! `SocketGroupView` + a `MinionSession` write surface.

use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::calc::minion::{AttributeInfusion, MinionModifierEntry};
use crate::rules::skill_stats;
use crate::rules::umbral_souls::{self, Soul};
use crate::skill_env::lookup::{
    EffectLookup, MinionLookup, ParserRulesLookup, SocketGroupView, StatMapLookup, StatSetLookup,
};
use crate::skill_env::resolve::{
    GemPropertyLookup, support_granted_gem_levels, support_stat_set_index,
};
use crate::skill_env::support::judge_group_supports;
use crate::skill_env::{GemInput, SupportCandidate};

/// The session write surface [`spawn_minions`] needs: the minion quantity limit's
/// player-side BASE sum, the player's flags, the per-minion spawn sink and the
/// player sink for what replaces a minion.
///
/// `pobr_core::calc::CalculationSession` implements this; the trait exists so minion
/// wiring doesn't depend on the concrete session type.
pub trait MinionSession {
    /// `Sum("BASE", nil, name)` over the player db (the minion limit stat source).
    fn base_sum(&self, name: &str) -> f64;

    /// Whether the player has the flag `name`.
    fn has_flag(&self, name: &str) -> bool;

    /// Gives the player modifiers granted instead of a minion.
    fn add_player_modifiers(&mut self, modifiers: Vec<crate::Modifier>);

    /// Spawns one minion from its catalog definition (see
    /// [`crate::calc::CalculationSession::add_minion_from_def`]).
    #[allow(clippy::too_many_arguments)]
    fn add_minion_from_def(
        &mut self,
        def: &pobr_data::minion::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: AttributeInfusion,
        origin: crate::calc::MinionOrigin,
    );
}

impl MinionSession for crate::calc::CalculationSession {
    fn base_sum(&self, name: &str) -> f64 {
        crate::calc::CalculationSession::base_sum(self, name)
    }

    fn has_flag(&self, name: &str) -> bool {
        crate::calc::CalculationSession::has_flag(self, name)
    }

    fn add_player_modifiers(&mut self, modifiers: Vec<crate::Modifier>) {
        self.add_modifiers(modifiers);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_minion_from_def(
        &mut self,
        def: &pobr_data::minion::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<MinionModifierEntry>,
        ally_buff_mods: Vec<crate::Modifier>,
        infusion: AttributeInfusion,
        origin: crate::calc::MinionOrigin,
    ) {
        crate::calc::CalculationSession::add_minion_from_def(
            self,
            def,
            gem_level,
            limit,
            minion_modifiers,
            ally_buff_mods,
            infusion,
            origin,
        );
    }
}

/// The lookup surface [`spawn_minions`] needs.
pub trait MinionEnv:
    MinionLookup + EffectLookup + StatSetLookup + StatMapLookup + ParserRulesLookup + GemPropertyLookup
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
    // Umbral Well: Skeletal Minions become Souls on the character instead.
    let umbral_well = session.has_flag(umbral_souls::UMBRAL_WELL);

    groups.for_each_enabled_group(&mut |group| {
        // This group's each **active skill** gem.
        // Prefers `gems` (the real import path, includes both active + support);
        // falls back to `active_skill_id` when empty (the builder/test path's
        // with_active_skill, the same fallback source as resolve_main_skill).
        let candidates: Vec<GemInput> = if group.gems.is_empty() {
            group
                .active_skill_id
                .map(|id| {
                    vec![GemInput {
                        skill_id: id.to_string(),
                        gem_level: group.active_gem_level,
                        quality: 0,
                        stat_set_index: None,
                        corrupted: false,
                        minion: None,
                    }]
                })
                .unwrap_or_default()
        } else {
            group.gems.to_vec()
        };
        for GemInput {
            skill_id,
            gem_level,
            quality,
            stat_set_index,
            corrupted,
            minion: chosen,
        } in candidates
        {
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
            let minion_ids = summoned_minions(data, &skill_id, chosen.as_deref());
            if minion_ids.is_empty() {
                continue;
            }
            if umbral_well
                && let Some(soul) = Soul::of_skill(&skill_id)
                && let Some(magnitude) =
                    soul_magnitude(data, &skill_id, gem_level, quality, stat_set_index)
            {
                for minion_id in minion_ids {
                    if !seen.insert(minion_id.clone()) {
                        continue;
                    }
                    let souls = data
                        .minion_def(minion_id)
                        .map_or(1, |def| minion_limit(&*session, def));
                    let origin = ModifierSource::new(SourceId::new(
                        SourceKind::Buff,
                        format!("buff.{}", soul.buff),
                    ))
                    .with_raw_text(format!("{souls} Umbral Soul ({skill_id})"));
                    session.add_player_modifiers(
                        soul.modifiers(magnitude, souls)
                            .into_iter()
                            .map(|modifier| modifier.with_origin(origin.clone()))
                            .collect(),
                    );
                }
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
                    corrupted,
                ))
                .saturating_add(support_granted_gem_levels(&judgement, group.gems, data));
            // A skill that says how many minions it summons ("Summons {0}
            // Hyenas") creates that many of each kind; any other creates as
            // many as the kind's limit.
            let own_count = own_minion_count(
                data,
                &skill_id,
                effective_gem_level,
                quality,
                stat_set_index,
            );
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
                // The limit only affects `Multiplier:SummonedMinion` (per-minion mods +
                // DPS aggregation count), not a single minion's life/defence.
                let limit = own_count.unwrap_or_else(|| minion_limit(&*session, def));
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
                    crate::calc::MinionOrigin {
                        summoned_by: skill_id.clone(),
                        is_companion,
                    },
                );
            }
        }
    });
}

/// The minions the skill `skill_id` summons. A Spectre's or a tamed beast's
/// gem summons the monster the player chose for it (`chosen`, PoB's `<Gem
/// skillMinion>`), as does a skill with no minions of its own; any other
/// skill summons its own minions, whichever one the player shows.
fn summoned_minions<'a>(
    data: &'a dyn MinionEnv,
    skill_id: &str,
    chosen: Option<&'a str>,
) -> Vec<&'a String> {
    let own = data.effect_minion_list(skill_id);
    let picks_a_monster = data.effect(skill_id).is_some_and(|effect| {
        matches!(
            effect.active_skill.as_deref(),
            Some("summon_beast" | "summon_spectre")
        )
    });
    match chosen {
        Some(id) if (picks_a_monster || own.is_empty()) && data.minion_def(id).is_some() => {
            let def = data.minion_def(id).expect("checked");
            vec![&def.id]
        }
        _ => own.iter().collect(),
    }
}

/// How much one Umbral Soul of the minions of the skill `skill_id` grants,
/// when its minions can become Souls.
fn soul_magnitude(
    data: &dyn MinionEnv,
    skill_id: &str,
    level: u32,
    quality: u32,
    set_index: Option<u32>,
) -> Option<f64> {
    let magnitude: f64 = data
        .effect_stats(skill_id, level, quality, set_index)
        .all()
        .filter(|ds| ds.stat == umbral_souls::SOUL_MAGNITUDE)
        .map(|ds| ds.value)
        .sum();
    (magnitude > 0.0).then_some(magnitude)
}

/// How many minions of each kind the skill `skill_id` creates at `level`,
/// when one of its stats says ([`skill_stats::is_summoned_minion_count`]).
fn own_minion_count(
    data: &dyn MinionEnv,
    skill_id: &str,
    level: u32,
    quality: u32,
    set_index: Option<u32>,
) -> Option<u32> {
    let mut count = None;
    for ds in data.effect_stats(skill_id, level, quality, set_index).all() {
        if skill_stats::is_summoned_minion_count(&ds.stat) {
            *count.get_or_insert(0.0) += ds.value;
        }
    }
    count.map(|total: f64| total.max(0.0) as u32)
}

/// How many of the minion `def` its skill creates: the player's BASE sum of
/// the minion's limit stat (e.g. `ActiveZombieLimit`), at least one.
fn minion_limit(session: &dyn MinionSession, def: &pobr_data::minion::MinionDef) -> u32 {
    let limit_stat = def.limit.to_pob2_str();
    if limit_stat.is_empty() {
        return 1;
    }
    let summed = session.base_sum(limit_stat);
    if summed >= 1.0 { summed as u32 } else { 1 }
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
    let cannot_be_supported = data.effect(skill_id).is_some_and(|e| e.cannot_be_supported);

    let candidates: Vec<SupportCandidate<'_>> = group
        .gems
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            std::iter::once(g.skill_id.as_str())
                .chain(
                    data.additional_effects(&g.skill_id)
                        .iter()
                        .map(String::as_str),
                )
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
