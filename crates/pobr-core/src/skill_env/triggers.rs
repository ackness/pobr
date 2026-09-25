//! Trigger-chain semantics (engine-semantics layer): data-driven trigger recognition
//! + the built-in trigger path + source-rate injection.
//!
//! Moved from `pobr-build`'s `skill/triggers.rs`. The source skill's post-calculation
//! statistics are fetched through the [`TriggerSubCalc`] callback (the orchestrator
//! implements it by running a one-level-deep `calculate_with_context` sub-calculation),
//! so the recognition/injection semantics don't depend on the orchestrator.

use pobr_data::item::EquipmentSlot;

use crate::Modifier;
use crate::calc::TriggerSourceStats;
use crate::skill_env::lookup::{
    BaseItemLookup, EquipmentView, SocketGroupView, TriggerConfigLookup, WeaponTypeLookup,
};
use crate::skill_env::mods::{is_damage_skill, mk_trigger_flag, mk_trigger_mod};
use crate::skill_env::resolve::{ResolvedSkillLevel, SkillLevelLookup, resolve_skill_level};
use crate::skill_env::weapon::{WeaponContributionLookup, weapon_contribution};
use crate::skill_env::{EnabledGroup, GemInput};

/// The lookup surface the trigger path needs.
pub trait TriggerEnv:
    TriggerConfigLookup
    + crate::skill_env::BuffEnv
    + SkillLevelLookup
    + WeaponContributionLookup
    + SourceCondLookup
{
    /// Upcast to the buff-env view (trait objects can't be re-sliced).
    fn as_buff_env(&self) -> &dyn crate::skill_env::BuffEnv;
    /// Upcast to the source-cond view.
    fn as_source_cond(&self) -> &dyn SourceCondLookup;
    /// Upcast to the gem-property view.
    fn as_gem_property(&self) -> &dyn crate::skill_env::GemPropertyLookup;
    /// Upcast to the skill-level view.
    fn as_skill_level(&self) -> &dyn SkillLevelLookup;
    /// Upcast to the weapon-contribution view.
    fn as_weapon(&self) -> &dyn WeaponContributionLookup;
}
impl<T> TriggerEnv for T
where
    T: TriggerConfigLookup
        + crate::skill_env::BuffEnv
        + SkillLevelLookup
        + WeaponContributionLookup
        + SourceCondLookup,
{
    fn as_buff_env(&self) -> &dyn crate::skill_env::BuffEnv {
        self
    }
    fn as_source_cond(&self) -> &dyn SourceCondLookup {
        self
    }
    fn as_gem_property(&self) -> &dyn crate::skill_env::GemPropertyLookup {
        self
    }
    fn as_skill_level(&self) -> &dyn SkillLevelLookup {
        self
    }
    fn as_weapon(&self) -> &dyn WeaponContributionLookup {
        self
    }
}

/// The sub-calculation surface for fetching a source skill's post-calculation
/// statistics (a minimal equivalent of PoB2's GlobalCache,
/// `CalcTriggers.lua:74-86`'s `cachedData[uuid].HitSpeed or Speed`).
///
/// The orchestrator implements this by cloning the build, pointing
/// `main_socket_group`/`main_active_skill` at the source gem, and running a
/// one-level-deep `calculate_with_context` (trigger relations stripped inside).
pub trait TriggerSubCalc {
    /// Runs the source skill's sub-calculation, returning its effective action rate +
    /// hit/crit chances. `group_index` = the source group's index in the enabled-group
    /// enumeration; `gem_index` = the source gem's index in the group's gem list (the
    /// implementor maps it back to the concrete group + derives the non-support
    /// ordinal `pick_group_main_skill` selects by). Returns `None` when the
    /// sub-calculation is unavailable (cycle / recursion guard / data gap) — the
    /// caller falls back to the base `1/use_time`.
    fn source_stats(&mut self, group_index: usize, gem_index: usize) -> Option<TriggerSourceStats>;
}

/// The trigger-chain injection surface: groups + equipment + the sub-calculation
/// callback + the condition gate (vendor's `modDB:Flag(nil, "Condition:X")` for
/// `requires_condition` entries).
pub struct TriggerCtx<'a, S: TriggerSubCalc> {
    /// Enabled socket groups.
    pub groups: &'a dyn SocketGroupView,
    /// Equipped items (the main-hand weapon's type bits feed `source_skill_cond`'s
    /// ModFlag predicate).
    pub equipment: &'a dyn EquipmentView,
    /// The source-skill sub-calculation callback.
    pub sub_calc: &'a mut S,
    /// Whether a `requires_condition` gate is satisfied (the build config's condition
    /// flags; evaluated by the caller, which owns `Build.config`).
    pub condition: &'a dyn Fn(&str) -> bool,
    /// Whether we're already inside a trigger-source sub-calculation (the recursion
    /// guard: no trigger relation is recognized/injected one level deep).
    pub is_trigger_source: bool,
    /// The build's class name (the unarmed contribution's lookup key when an attack
    /// skill has no weapon equipped — `data.unarmedWeaponData[classId]`).
    pub class_name: &'a str,
}

/// The result of data-driven recognition: the matched trigger config + the trigger
/// gem's index in the group (`None` when the main skill itself matched a skill-kind key).
pub struct RecognizedTrigger<'a> {
    /// The matched trigger config entry.
    pub config: &'a pobr_data::catalog::TriggerConfigDef,
    /// The trigger gem's index into the group's gem list (a meta/support gem; `None`
    /// when the main skill itself matched).
    pub trigger_gem_index: Option<usize>,
}

/// The main skill's trigger-chain modifiers (build-layer wiring for findings
/// 03-01/03-02/03-06; expanded since).
///
/// Two recognition paths (data-driven first, then built-in triggers; returns as soon
/// as one matches):
///
/// 1. **Data-driven recognition (`overlay/trigger_configs.json`)**: PoBR's projection
///    of vendor `CalcTriggers.lua:1452-1455`'s four-level key lookup — an entry's
///    `match_effect_ids` match either a **gem in the group** (a triggeredBy relation)
///    or the **main skill itself** (a skill key). On a match, injects per the entry's
///    declarative facts: trigger cooldown (override value > trigger gem's own
///    cooldown > triggered skill's cooldown), `TriggerRateCapOverride`, a global
///    marker, source-skill predicate matching + sub-calculation statistics.
/// 2. **Built-in triggers** (`skill_types` includes `Triggered`/`InbuiltTrigger`,
///    matching PoB2's `isTriggered`): injects the triggered skill's cooldown +
///    in-group source rate.
///
/// **Deferred**: ① fetching the `trigger_chance_stat`/`source_rate_stat` stat values;
/// ② handler entries' real logic; ③ per-skill cooldown rotation for multiple
/// triggered skills; ④ cross-call sub-calculation caching.
pub fn trigger_modifiers<S: TriggerSubCalc>(
    ctx: &mut TriggerCtx<'_, S>,
    data: &dyn TriggerEnv,
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
    main_skill: &ResolvedSkillLevel,
    group: &EnabledGroup<'_>,
    group_index: Option<usize>,
    main_skill_id: &str,
) -> Vec<Modifier> {
    // Recursion guard: no trigger relation is recognized/injected within a source
    // skill's sub-calculation env (one level of depth stripped).
    if ctx.is_trigger_source {
        return Vec::new();
    }

    // — Path 1: data-driven recognition (returns as soon as it matches, including
    // "recognized but the gate isn't satisfied → empty").
    if let Some(mods) = config_trigger_modifiers(
        ctx,
        data,
        gem_level_bonuses,
        main_skill,
        group,
        group_index,
        main_skill_id,
    ) {
        return mods;
    }

    // — Path 2: built-in trigger (matching PoB2's isTriggered: skillTypes includes
    // Triggered or InbuiltTrigger).
    let Some(effect) = data.effect(main_skill_id) else {
        return Vec::new();
    };
    let is_triggered = effect
        .skill_types
        .iter()
        .any(|t| t == "Triggered" || t == "InbuiltTrigger");
    if !is_triggered {
        return Vec::new();
    }

    let mut mods = Vec::new();

    // Triggered skill's cooldown → trigger cooldown + triggered cooldown BASE (same
    // value; without separate trigger-gem cooldown data, PoB2's
    // `actionCooldown = max(triggerCD, triggeredCD)` degenerates to this single cooldown).
    if let Some(cd) = main_skill.cooldown_s
        && cd > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggeredSkillCooldown",
            cd,
            "triggered skill base cooldown",
        ));
        mods.push(mk_trigger_mod(
            "TriggerCooldownBase",
            cd,
            "trigger base cooldown",
        ));
    }

    // The in-group trigger source skill → sub-calculation statistics →
    // TriggerSourceRate + source hit folded in.
    if let Some(stats) = in_group_trigger_source_stats(
        ctx,
        data,
        gem_level_bonuses,
        group,
        group_index,
        main_skill_id,
    ) {
        push_source_stat_mods(&mut mods, &stats, true, false);
    }

    mods
}

/// Source statistics injection (contract 4's transport surface): rate is always
/// injected; hit/crit are injected as percentages per the chain's semantics.
pub fn push_source_stat_mods(
    mods: &mut Vec<Modifier>,
    stats: &TriggerSourceStats,
    fold_hit: bool,
    fold_crit: bool,
) {
    mods.push(mk_trigger_mod(
        "TriggerSourceRate",
        stats.action_rate,
        "trigger source effective rate (sub-calculated)",
    ));
    if fold_hit && stats.hit_chance > 0.0 {
        mods.push(mk_trigger_mod(
            "TriggerSourceHitChance",
            stats.hit_chance * 100.0,
            "trigger source hit chance",
        ));
    }
    if fold_crit && stats.crit_chance > 0.0 {
        mods.push(mk_trigger_mod(
            "TriggerSourceCritChance",
            stats.crit_chance * 100.0,
            "trigger source crit chance",
        ));
    }
}

/// Data-driven trigger wiring: on a recognition match, returns the injected mods
/// (`Some(vec![])` = recognized but the `requires_condition` gate isn't satisfied,
/// matching vendor's disable); returns `None` on no match (falls through to path 2).
fn config_trigger_modifiers<S: TriggerSubCalc>(
    ctx: &mut TriggerCtx<'_, S>,
    data: &dyn TriggerEnv,
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
    main_skill: &ResolvedSkillLevel,
    group: &EnabledGroup<'_>,
    group_index: Option<usize>,
    main_skill_id: &str,
) -> Option<Vec<Modifier>> {
    let recognized = recognize_trigger_config(data, group, main_skill_id)?;
    let config = recognized.config;

    // The requires_condition gate (matching vendor's `modDB:Flag(nil, "Condition:X")`,
    // e.g. The Hidden Blade needs Phasing, Cast on Melee Kill needs KilledRecently;
    // when unsatisfied, vendor sets disable / degrades to self-cast).
    if let Some(cond_name) = &config.requires_condition
        && !(ctx.condition)(cond_name)
    {
        return Some(Vec::new());
    }

    let mut mods = vec![mk_trigger_flag(
        "SkillIsTriggered",
        "trigger relation recognized (trigger_configs)",
    )];

    // Triggered skill's cooldown.
    if let Some(cd) = main_skill.cooldown_s
        && cd > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggeredSkillCooldown",
            cd,
            "triggered skill base cooldown",
        ));
    }
    // Trigger cooldown: entry override value > the trigger gem's own cooldown > the
    // triggered skill's cooldown.
    let trigger_gem_cd = recognized.trigger_gem_index.and_then(|idx| {
        let gem = &group.gems[idx];
        resolve_source_level(ctx, data, gem_level_bonuses, group, gem)
            .and_then(|resolved| resolved.cooldown_s)
    });
    if let Some(cd) = config
        .cooldown_override_s
        .or(trigger_gem_cd)
        .or(main_skill.cooldown_s)
        && cd > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggerCooldownBase",
            cd,
            "trigger base cooldown",
        ));
    }
    // Rate cap override (matching vendor's `skillData.triggerRateCapOverride`, e.g.
    // Hidden Blade's 2/s).
    if let Some(cap) = config.trigger_rate_cap_override
        && cap > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggerRateCapOverride",
            cap,
            "trigger rate cap override",
        ));
    }

    // global / source = self: doesn't depend on a source skill's rate (matching
    // vendor's `EffectiveSourceRate = TriggerRateCap`).
    if config.global_trigger || config.source_is_self {
        mods.push(mk_trigger_flag(
            "TriggerSourceGlobal",
            "global trigger (source rate = rate cap)",
        ));
        return Some(mods);
    }

    if config.trigger_on_crit {
        mods.push(mk_trigger_flag(
            "TriggerOnCrit",
            "trigger chance folds source crit chance",
        ));
    }

    // Source skill: the group's non-triggered damaging skill matching the restricted
    // predicate (the one with the highest base rate, matching PoB2's findTriggerSkill
    // highest-APS) → sub-calculation fetches post-calculation statistics; falls back
    // to the base `1/use_time` when the sub-calculation is unavailable.
    if let Some((source_idx, base_rate)) = find_trigger_source_gem(
        ctx,
        data,
        gem_level_bonuses,
        group,
        main_skill_id,
        &recognized,
    ) {
        let stats =
            source_stats_for(ctx, group_index, group, source_idx, main_skill_id).or_else(|| {
                Some(TriggerSourceStats {
                    action_rate: base_rate,
                    ..Default::default()
                })
            });
        if let Some(stats) = stats {
            // The triggerOnUse chain doesn't fold hit/crit (matching vendor :721's
            // `not config.triggerOnUse`).
            let fold_hit = !config.trigger_on_use;
            let fold_crit = config.trigger_on_crit;
            push_source_stat_mods(&mut mods, &stats, fold_hit, fold_crit);
        }
    }

    Some(mods)
}

/// Recognizes a trigger relation (PoBR's projection of the four-level key, keyed by
/// `match_effect_ids`): checks the main skill itself first (a skill-kind key), then
/// scans the rest of the group's gems (a triggeredBy / unique trigger).
pub fn recognize_trigger_config<'a>(
    data: &'a dyn TriggerEnv,
    group: &'a EnabledGroup<'a>,
    main_skill_id: &str,
) -> Option<RecognizedTrigger<'a>> {
    if let Some(config) = data.trigger_config(main_skill_id) {
        return Some(RecognizedTrigger {
            config,
            trigger_gem_index: None,
        });
    }
    for (idx, gem) in group.gems.iter().enumerate() {
        if gem.skill_id == main_skill_id {
            continue;
        }
        if let Some(config) = data.trigger_config(&gem.skill_id) {
            return Some(RecognizedTrigger {
                config,
                trigger_gem_index: Some(idx),
            });
        }
    }
    None
}

/// Selects the trigger source gem within the group (matching vendor's
/// `findTriggerSkill` same-socket semantics + the restricted predicate filter):
/// non-support, non-triggered, a damaging skill, ≠ the main skill, ≠ the trigger gem,
/// and passes `source_skill_cond`; among multiple candidates, takes the one with the
/// highest base rate (`1/use_time`). Returns `(gem_index, base_rate)`.
fn find_trigger_source_gem<S: TriggerSubCalc>(
    ctx: &mut TriggerCtx<'_, S>,
    data: &dyn TriggerEnv,
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
    group: &EnabledGroup<'_>,
    main_skill_id: &str,
    recognized: &RecognizedTrigger<'_>,
) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (idx, gem) in group.gems.iter().enumerate() {
        if gem.skill_id == main_skill_id {
            continue;
        }
        if let Some(trigger_idx) = recognized.trigger_gem_index
            && idx == trigger_idx
        {
            continue;
        }
        let Some(effect) = data.effect(&gem.skill_id) else {
            continue;
        };
        if effect.is_support
            || effect
                .skill_types
                .iter()
                .any(|t| t == "Triggered" || t == "InbuiltTrigger")
            || !is_damage_skill(effect)
        {
            continue;
        }
        if let Some(cond) = &recognized.config.source_skill_cond
            && !source_cond_matches(
                ctx.equipment,
                data.as_source_cond(),
                &effect.skill_types,
                cond,
            )
        {
            continue;
        }
        let Some(rate) = base_rate_of(ctx, data, gem_level_bonuses, group, gem) else {
            continue;
        };
        if best.is_none_or(|(_, b)| rate > b) {
            best = Some((idx, rate));
        }
    }
    best
}

/// Evaluates the restricted predicate (three fields: any_skill_types / all_mod_flags /
/// not_skill_types). Mod flags are approximated by the main-hand weapon's type bits
/// (`weapon_types` table's flag + one_hand) — the skill cfg flags' weapon bits are
/// themselves derived from the main-hand weapon (matching vendor's skillCfg.flags source).
/// The lookup surface [`source_cond_matches`] needs.
pub trait SourceCondLookup: BaseItemLookup + WeaponTypeLookup {}
impl<T: BaseItemLookup + WeaponTypeLookup> SourceCondLookup for T {}

pub fn source_cond_matches(
    equipment: &dyn EquipmentView,
    data: &dyn SourceCondLookup,
    skill_types: &[String],
    cond: &pobr_data::catalog::TriggerSkillCondDef,
) -> bool {
    if !cond.any_skill_types.is_empty()
        && !cond.any_skill_types.iter().any(|t| skill_types.contains(t))
    {
        return false;
    }
    if cond.not_skill_types.iter().any(|t| skill_types.contains(t)) {
        return false;
    }
    if !cond.all_mod_flags.is_empty() {
        let Some(weapon) = equipment
            .item(EquipmentSlot::Weapon1)
            .and_then(|item| data.base_item(&item.base.to_string()))
            .and_then(|def| data.weapon_type_info(&def.item_class))
        else {
            return false;
        };
        for flag in &cond.all_mod_flags {
            let matched = match flag.as_str() {
                "Weapon1H" => weapon.one_hand,
                "Weapon2H" => !weapon.one_hand,
                other => weapon.flag == other,
            };
            if !matched {
                return false;
            }
        }
    }
    true
}

/// Resolves a gem's effective level + parameters (gem level + global `+N to Level`
/// bonuses + support-granted levels → `resolve_skill_level`).
fn resolve_source_level<S: TriggerSubCalc>(
    ctx: &mut TriggerCtx<'_, S>,
    data: &dyn TriggerEnv,
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
    group: &EnabledGroup<'_>,
    gem: &GemInput,
) -> Option<ResolvedSkillLevel> {
    let judgement =
        crate::skill_env::buff_specs::group_judgement(group, data.as_buff_env(), &gem.skill_id);
    let bonus = crate::skill_env::additional_gem_levels(
        gem_level_bonuses,
        data.as_gem_property(),
        &gem.skill_id,
    )
    .saturating_add(crate::skill_env::support_granted_gem_levels(
        &judgement,
        group.gems,
        data.as_buff_env(),
    ));
    let _ = ctx;
    resolve_skill_level(
        data.as_skill_level(),
        &gem.skill_id,
        gem.gem_level.saturating_add(bonus),
        gem.stat_set_index,
    )
}

/// The source gem's base rate (used both as the sub-calculation fallback and the
/// candidate sort key): `1/use_time`; when an attack skill has no use_time of its own,
/// takes the weapon base attack speed (including attackSpeedMultiplier, sourced the
/// same way as the main assembly path `weapon_contribution` — vendor's attack source
/// rate is determined by the weapon in the first place).
fn base_rate_of<S: TriggerSubCalc>(
    ctx: &mut TriggerCtx<'_, S>,
    data: &dyn TriggerEnv,
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
    group: &EnabledGroup<'_>,
    gem: &GemInput,
) -> Option<f64> {
    let resolved = resolve_source_level(ctx, data, gem_level_bonuses, group, gem)?;
    if let Some(use_time) = resolved.use_time_s
        && use_time > 0.0
    {
        return Some(1.0 / use_time);
    }
    let weapon = weapon_contribution(
        ctx.equipment,
        data.as_weapon(),
        ctx.class_name,
        &gem.skill_id,
        &resolved,
    )?;
    if weapon.attack_rate <= 0.0 {
        return None;
    }
    let asm = resolved
        .attack_speed_multiplier
        .map_or(1.0, |m| 1.0 + m / 100.0);
    Some(weapon.attack_rate * asm)
}

/// The source skill's full sub-calculation statistics: delegates to the
/// [`TriggerSubCalc`] callback (the orchestrator clones the build + points the
/// selection at the source gem + runs a one-level-deep `calculate_with_context`).
/// `source_idx` = the source gem's index in `group.gems`.
fn source_stats_for<S: TriggerSubCalc>(
    ctx: &mut TriggerCtx<'_, S>,
    group_index: Option<usize>,
    group: &EnabledGroup<'_>,
    source_idx: usize,
    main_skill_id: &str,
) -> Option<TriggerSourceStats> {
    let source_gem = &group.gems[source_idx];
    if source_gem.skill_id == main_skill_id {
        // Trigger cycle (the source skill is also the triggered skill): falls back to
        // base use_time semantics.
        return None;
    }
    if ctx.is_trigger_source {
        return None;
    }
    // `group_index` = the group's index in the build's enabled-group enumeration;
    // `None` (a detached/test group not in the build) = no sub-calculation (the caller
    // falls back to the base `1/use_time`).
    let group_index = group_index?;
    // The implementor maps `source_idx` back to the concrete group's gem and derives
    // the non-support ordinal `pick_group_main_skill` selects by (it owns `BuildData`).
    ctx.sub_calc.source_stats(group_index, source_idx)
}

/// Built-in trigger's in-group source skill statistics: selects the highest-base-rate
/// candidate per the existing candidate rule (non-support, non-triggered, damaging
/// skill, ≠ main skill), then fetches **post-calculation** statistics via
/// sub-calculation; falls back to the base `1/use_time` when the sub-calculation is
/// unavailable.
fn in_group_trigger_source_stats<S: TriggerSubCalc>(
    ctx: &mut TriggerCtx<'_, S>,
    data: &dyn TriggerEnv,
    gem_level_bonuses: &[crate::skill_env::GemPropertyBonus],
    group: &EnabledGroup<'_>,
    group_index: Option<usize>,
    main_skill_id: &str,
) -> Option<TriggerSourceStats> {
    let mut best: Option<(usize, f64)> = None;
    for (idx, gem) in group.gems.iter().enumerate() {
        if gem.skill_id == main_skill_id {
            continue;
        }
        let Some(effect) = data.effect(&gem.skill_id) else {
            continue;
        };
        if effect.is_support
            || effect
                .skill_types
                .iter()
                .any(|t| t == "Triggered" || t == "InbuiltTrigger")
            || !is_damage_skill(effect)
        {
            continue;
        }
        let Some(rate) = base_rate_of(ctx, data, gem_level_bonuses, group, gem) else {
            continue;
        };
        if best.is_none_or(|(_, b)| rate > b) {
            best = Some((idx, rate));
        }
    }
    let (source_idx, base_rate) = best?;
    Some(
        source_stats_for(ctx, group_index, group, source_idx, main_skill_id).unwrap_or(
            TriggerSourceStats {
                action_rate: base_rate,
                ..Default::default()
            },
        ),
    )
}
