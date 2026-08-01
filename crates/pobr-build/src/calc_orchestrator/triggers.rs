//! triggers — trigger fixed point + support-applicability judgement (pure migration from calc_orchestrator, no logic change).

use super::*;

// Trigger section

thread_local! {
    /// Trigger sub-calculation recursion depth:
    /// while a source skill's sub-calculation is in progress (>0), [`trigger_modifiers`]
    /// bails out entirely — the sub-calc's env forcibly strips trigger relations (one
    /// level of depth), preventing infinite recursion from triggers that reference each
    /// other in a cycle.
    static TRIGGER_SUBCALC_DEPTH: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// RAII depth guard (panic-safe: Drop restores the count).
pub(crate) struct TriggerDepthGuard;

impl TriggerDepthGuard {
    pub(crate) fn enter() -> Self {
        TRIGGER_SUBCALC_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
        TriggerDepthGuard
    }
}

impl Drop for TriggerDepthGuard {
    fn drop(&mut self) {
        TRIGGER_SUBCALC_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// The result of data-driven recognition: the matched trigger config + the trigger gem
/// (a meta/support gem in the group; `None` when the main skill itself matched a
/// skill-kind key).
pub(crate) struct RecognizedTrigger<'a> {
    config: &'a pobr_data::catalog::TriggerConfigDef,
    trigger_gem: Option<&'a crate::build::GemSkillRef>,
}

/// The main skill's trigger-chain modifiers (build-layer wiring for findings
/// 03-01/03-02/03-06; expanded since).
///
/// Two recognition paths (data-driven first, then built-in triggers; returns as soon as one matches):
///
/// 1. **Data-driven recognition (`overlay/trigger_configs.json`)**: PoBR's projection of
///    vendor `CalcTriggers.lua:1452-1455`'s four-level key lookup — an entry's
///    `match_effect_ids` (PoE2 granted effect ids) match either a **gem in the group**
///    (a triggeredBy relation, e.g. `MetaCastOnCritPlayer`) or the **main skill itself**
///    (a skill key). On a match, injects per the entry's declarative facts: trigger
///    cooldown (override value > trigger gem's own cooldown > triggered skill's
///    cooldown), `TriggerRateCapOverride`, a global marker, source-skill predicate
///    matching + sub-calculation statistics.
/// 2. **Built-in triggers** (`skill_types` includes `Triggered`/`InbuiltTrigger`,
///    matching PoB2's `isTriggered`: an auto-triggered skill built into an item/ascendancy):
///    injects the triggered skill's cooldown + in-group source rate.
///
/// **Source rate**: the source skill runs one full [`calculate_with_data`]
/// sub-calculation (a minimal equivalent of PoB2's GlobalCache,
/// `CalcTriggers.lua:74-86`'s `cachedData[uuid].HitSpeed or Speed`), taking its
/// **post-calculation** effective action rate and injecting it as `TriggerSourceRate`
/// BASE — a CoC build stacking attack speed sees its source rate grow with the attack
/// speed multiplier zone. Hit/crit is injected via `TriggerSourceHitChance`/
/// `TriggerSourceCritChance` BASE (as a percentage); perform's `fill_trigger` builds a
/// [`pobr_core::calc::TriggerSourceStats`] (contract 4) and folds it into trigger chance
/// (`:716-770`). Sub-calculation guards: stripped whenever depth >0 (this function's
/// top-level early return), and source = the triggered skill itself (a cycle) falls back
/// to the base `1/use_time`.
///
/// **Deferred**: ① fetching the `trigger_chance_stat`/`source_rate_stat` stat values
/// (the values live in the build mod domain, injection source not wired up yet); ②
/// handler entries' real logic (the registry is pending, count monitored to stay <100);
/// ③ per-skill cooldown rotation for multiple triggered skills (needs the full gem-link
/// list); ④ cross-call sub-calculation caching — the existing `CalcCache` only wraps
/// text-only `calculate`, extending it to a `(build hash, skill id)` key is pending a
/// cache-layer overhaul (within a single calculation, a sub-calc only runs once, so the
/// hot path is currently manageable).
pub(crate) fn trigger_modifiers(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &ResolvedSkillLevel,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Vec<Modifier> {
    // Recursion guard: no trigger relation is recognized/injected within a source
    // skill's sub-calculation env (one level of depth stripped).
    if TRIGGER_SUBCALC_DEPTH.with(|d| d.get()) > 0 {
        return Vec::new();
    }

    // — Path 1: data-driven recognition (returns as soon as it matches, including "recognized but the gate isn't satisfied → empty").
    if let Some(mods) =
        config_trigger_modifiers(build, data, options, main_skill, group, main_skill_id)
    {
        return mods;
    }

    // — Path 2: built-in trigger (matching PoB2's isTriggered: skillTypes includes Triggered or InbuiltTrigger).
    let Some(effect) = data.granted_effects.get(main_skill_id) else {
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

    // The in-group trigger source skill → sub-calculation statistics → TriggerSourceRate
    // (post-calculation attack speed) + source hit folded in. Nothing is injected when
    // there's no candidate in the group — fill_trigger falls back to the main skill's
    // rate (a placeholder semantics).
    if let Some(stats) = in_group_trigger_source_stats(build, data, options, group, main_skill_id) {
        push_source_stat_mods(
            &mut mods, &stats, /* fold_hit */ true, /* fold_crit */ false,
        );
    }

    mods
}

/// Builds a trigger BASE mod (SkillGem attribution, id prefix `trigger.`).
pub(crate) fn mk_trigger_mod(stat: &str, value: f64, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{stat}"),
    ))
    .with_raw_text(label);
    Modifier::number(stat, ModType::Base, value).with_origin(origin)
}

/// Builds a trigger FLAG mod (SkillGem attribution).
pub(crate) fn mk_trigger_flag(name: &str, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{name}"),
    ))
    .with_raw_text(label);
    Modifier::flag(name).with_origin(origin)
}

/// Source statistics injection (contract 4's transport surface): rate is always
/// injected; hit/crit are injected as percentages per the chain's semantics (`fold_hit`
/// = the default handler folds hit for anything other than triggerOnUse; `fold_crit` =
/// the CoC chain).
pub(crate) fn push_source_stat_mods(
    mods: &mut Vec<Modifier>,
    stats: &pobr_core::calc::TriggerSourceStats,
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
/// matching vendor's disable — the trigger panel stays at 0 and **does not fall through**
/// to the built-in trigger path); returns `None` on no match (falls through to path 2).
pub(crate) fn config_trigger_modifiers(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &ResolvedSkillLevel,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Option<Vec<Modifier>> {
    let recognized = recognize_trigger_config(data, group, main_skill_id)?;
    let config = recognized.config;

    // The requires_condition gate (matching vendor's `modDB:Flag(nil, "Condition:X")`,
    // e.g. The Hidden Blade needs Phasing, Cast on Melee Kill needs KilledRecently;
    // when unsatisfied, vendor sets disable / degrades to self-cast).
    if let Some(cond_name) = &config.requires_condition {
        let build_cfg = build.config.to_calc_config();
        if !build_cfg.condition(cond_name) {
            return Some(Vec::new());
        }
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
    // Trigger cooldown: entry override value (matching vendor's `skillData.cooldown = N`)
    // > the trigger gem's own cooldown (`triggeredBy.grantedEffect.levels[lvl].cooldown`)
    // > the triggered skill's cooldown.
    let trigger_gem_cd = recognized.trigger_gem.and_then(|gem| {
        resolve_skill_level_with_gem_bonus(
            build,
            data,
            &gem.skill_id,
            gem.gem_level,
            gem.stat_set_index,
        )
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
    // Rate cap override (matching vendor's `skillData.triggerRateCapOverride`, e.g. Hidden Blade's 2/s).
    if let Some(cap) = config.trigger_rate_cap_override
        && cap > 0.0
    {
        mods.push(mk_trigger_mod(
            "TriggerRateCapOverride",
            cap,
            "trigger rate cap override",
        ));
    }

    // global / source = self: doesn't depend on a source skill's rate (matching vendor's `EffectiveSourceRate = TriggerRateCap`).
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
    // highest-APS) → sub-calculation fetches post-calculation statistics; falls back to
    // the base `1/use_time` when the sub-calculation is unavailable (recursion
    // guard/cycle/failure).
    if let Some(source_gem) =
        find_trigger_source_gem(build, data, group, main_skill_id, &recognized)
    {
        let stats = trigger_source_stats(build, data, options, group, source_gem, main_skill_id)
            .or_else(|| {
                base_rate_of(build, data, source_gem).map(|rate| {
                    pobr_core::calc::TriggerSourceStats {
                        action_rate: rate,
                        ..Default::default()
                    }
                })
            });
        if let Some(stats) = stats {
            // The triggerOnUse chain doesn't fold hit/crit (matching vendor :721's `not config.triggerOnUse`).
            let fold_hit = !config.trigger_on_use;
            let fold_crit = config.trigger_on_crit;
            push_source_stat_mods(&mut mods, &stats, fold_hit, fold_crit);
        }
    }

    Some(mods)
}

/// Recognizes a trigger relation (PoBR's projection of the four-level key, keyed by
/// `match_effect_ids`): checks the main skill itself first (a skill-kind key, e.g.
/// Tempest Shield), then scans the rest of the group's gems (a triggeredBy / unique
/// trigger, e.g. `MetaCastOnCritPlayer`).
pub(crate) fn recognize_trigger_config<'a>(
    data: &'a BuildData,
    group: &'a SocketGroup,
    main_skill_id: &str,
) -> Option<RecognizedTrigger<'a>> {
    if data.trigger_configs.is_empty() {
        return None;
    }
    if let Some(config) = data.trigger_configs.get(main_skill_id) {
        return Some(RecognizedTrigger {
            config,
            trigger_gem: None,
        });
    }
    for gem in &group.gem_skills {
        if gem.skill_id == main_skill_id {
            continue;
        }
        if let Some(config) = data.trigger_configs.get(&gem.skill_id) {
            return Some(RecognizedTrigger {
                config,
                trigger_gem: Some(gem),
            });
        }
    }
    None
}

/// Selects the trigger source gem within the group (matching vendor's `findTriggerSkill`
/// same-socket semantics + the restricted predicate filter): non-support, non-triggered,
/// a damaging skill, ≠ the main skill, ≠ the trigger gem, and passes
/// `source_skill_cond`; among multiple candidates, takes the one with the highest base
/// rate (`1/use_time`).
pub(crate) fn find_trigger_source_gem<'b>(
    build: &Build,
    data: &BuildData,
    group: &'b SocketGroup,
    main_skill_id: &str,
    recognized: &RecognizedTrigger<'_>,
) -> Option<&'b crate::build::GemSkillRef> {
    let mut best: Option<(&crate::build::GemSkillRef, f64)> = None;
    for gem in &group.gem_skills {
        if gem.skill_id == main_skill_id {
            continue;
        }
        if let Some(trigger_gem) = recognized.trigger_gem
            && gem.skill_id == trigger_gem.skill_id
        {
            continue;
        }
        let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
            continue;
        };
        if effect.is_support
            || effect
                .skill_types
                .iter()
                .any(|t| t == "Triggered" || t == "InbuiltTrigger")
            || !is_damage_skill(data, &gem.skill_id)
        {
            continue;
        }
        if let Some(cond) = &recognized.config.source_skill_cond
            && !source_cond_matches(build, data, &effect.skill_types, cond)
        {
            continue;
        }
        let Some(rate) = base_rate_of(build, data, gem) else {
            continue;
        };
        if best.is_none_or(|(_, b)| rate > b) {
            best = Some((gem, rate));
        }
    }
    best.map(|(gem, _)| gem)
}

/// Evaluates the restricted predicate (three fields: any_skill_types / all_mod_flags /
/// not_skill_types). Mod flags are approximated by the main-hand weapon's type bits
/// (`weapon_types` table's flag + one_hand) — the skill cfg flags' weapon bits are
/// themselves derived from the main-hand weapon (matching vendor's skillCfg.flags source).
pub(crate) fn source_cond_matches(
    build: &Build,
    data: &BuildData,
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
        let Some(weapon) = build
            .items
            .get(&EquipmentSlot::Weapon1)
            .and_then(|item| data.base_items.get(&item.base.to_string()))
            .and_then(|def| weapon_type_info(data, &def.item_class))
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

/// The source gem's base rate (used both as the sub-calculation fallback and the
/// candidate sort key): `1/use_time`; when an attack skill has no use_time of its own,
/// takes the weapon base attack speed (including attackSpeedMultiplier, sourced the same
/// way as the main assembly path `weapon_contribution` — vendor's attack source rate is
/// determined by the weapon in the first place).
pub(crate) fn base_rate_of(
    build: &Build,
    data: &BuildData,
    gem: &crate::build::GemSkillRef,
) -> Option<f64> {
    let resolved = resolve_skill_level_with_gem_bonus(
        build,
        data,
        &gem.skill_id,
        gem.gem_level,
        gem.stat_set_index,
    )?;
    if let Some(use_time) = resolved.use_time_s
        && use_time > 0.0
    {
        return Some(1.0 / use_time);
    }
    let weapon = weapon_contribution(build, data, &gem.skill_id, &resolved)?;
    if weapon.attack_rate <= 0.0 {
        return None;
    }
    let asm = resolved
        .attack_speed_multiplier
        .map_or(1.0, |m| 1.0 + m / 100.0);
    Some(weapon.attack_rate * asm)
}

/// The source skill's full sub-calculation (a minimal equivalent of PoB2's GlobalCache):
/// same build / same group, swaps the active skill for the source gem
/// (`main_active_skill` points to its ordinal in the group's non-support list), runs a
/// full [`calculate_with_data`] to get `{effective_action_rate, hit_chance, crit_chance}`.
///
/// Guards:
/// - **Cycle detection**: source = the triggered skill itself → `None` (the caller falls back to base `1/use_time`);
/// - **One level of depth**: depth ≥1 returns `None` directly (deep trigger relations
///   are already stripped at the top of [`trigger_modifiers`]; this is a redundant guard
///   for direct calls);
/// - Sub-calculation failure (a data gap etc.) → `None`, doesn't amplify the error.
pub(crate) fn trigger_source_stats(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    source_gem: &crate::build::GemSkillRef,
    main_skill_id: &str,
) -> Option<pobr_core::calc::TriggerSourceStats> {
    if source_gem.skill_id == main_skill_id {
        // Trigger cycle (the source skill is also the triggered skill): falls back to base use_time semantics.
        return None;
    }
    if TRIGGER_SUBCALC_DEPTH.with(|d| d.get()) >= 1 {
        return None;
    }

    let group_idx = build
        .socket_groups
        .iter()
        .position(|g| std::ptr::eq(g, group))?;
    // The source gem's 1-based ordinal in the group's **non-support** sequence (the selection key of pick_group_main_skill).
    let active_pos = group
        .gem_skills
        .iter()
        .filter(|g| {
            !data
                .granted_effects
                .get(&g.skill_id)
                .map(|e| e.is_support)
                .unwrap_or(false)
        })
        .position(|g| std::ptr::eq(g, source_gem))?
        + 1;

    let mut sub_build = build.clone();
    sub_build.main_socket_group = Some(group_idx + 1);
    sub_build.socket_groups[group_idx].main_active_skill = Some(active_pos);

    let _guard = TriggerDepthGuard::enter();
    let out = calculate_with_data(&sub_build, data, options).ok()?;
    let action_rate = if out.effective_action_rate > 0.0 {
        out.effective_action_rate
    } else {
        out.action_rate
    };
    if action_rate <= 0.0 {
        return None;
    }
    Some(pobr_core::calc::TriggerSourceStats {
        action_rate,
        hit_chance: out.hit_chance,
        crit_chance: out.crit_chance,
    })
}

/// Built-in trigger's in-group source skill statistics: selects the highest-base-rate
/// candidate per the existing candidate rule (non-support, non-triggered, damaging
/// skill, ≠ main skill), then fetches **post-calculation** statistics via
/// sub-calculation; falls back to the base `1/use_time` when the sub-calculation is
/// unavailable (the legacy semantics from before 14-G2 was fixed, kept as the fallback surface).
pub(crate) fn in_group_trigger_source_stats(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Option<pobr_core::calc::TriggerSourceStats> {
    let mut best: Option<(&crate::build::GemSkillRef, f64)> = None;
    for gem in &group.gem_skills {
        if gem.skill_id == main_skill_id {
            continue;
        }
        let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
            continue;
        };
        if effect.is_support
            || effect
                .skill_types
                .iter()
                .any(|t| t == "Triggered" || t == "InbuiltTrigger")
            || !is_damage_skill(data, &gem.skill_id)
        {
            continue;
        }
        let Some(rate) = base_rate_of(build, data, gem) else {
            continue;
        };
        if best.is_none_or(|(_, b)| rate > b) {
            best = Some((gem, rate));
        }
    }
    let (source_gem, base_rate) = best?;
    Some(
        trigger_source_stats(build, data, options, group, source_gem, main_skill_id).unwrap_or(
            pobr_core::calc::TriggerSourceStats {
                action_rate: base_rate,
                ..Default::default()
            },
        ),
    )
}

// End of trigger section

/// The result of a group-level support-applicability judgement.
///
/// `compatible` is the list of support effect references that **passed PoB2's four-stage
/// judgement** (slot order preserved); `final_skill_types` is the active skill's type
/// set after the addSkillTypes fixed point converges (seeded from the active effect's
/// `skill_types`, merged with every compatible support's `add_skill_types`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupSupportJudgement {
    /// Compatible supports (slot order; includes the support half of an additionally-granted effect, see [`CompatibleSupport`]).
    pub(crate) compatible: Vec<CompatibleSupport>,
    /// The skill type set after the fixed point converges.
    pub(crate) final_skill_types: std::collections::HashSet<String>,
}

/// A reference to one compatible support's effect: level/quality/statSet index are
/// taken from the host gem instance (`gem_index`), while stat fetching uses
/// `effect_id` — for a normal support these come from the same source (effect_id = the
/// gem's primary effect); for a meta gem (Blasphemy), the primary effect is an active
/// skill, and the support half lives in an additional granted effect slot (the
/// `gem_effects` foreign key `additionalGrantedEffectId1..N`; vendor routes each effect
/// in grantedEffectList by its `support` flag, assembled in CalcSetup.lua's gemList).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompatibleSupport {
    /// This support's index into the host gem in `group.gem_skills`.
    pub(crate) gem_index: usize,
    /// The support's granted effect id (the fetch key for stat / manaMultiplier / set_key).
    pub(crate) effect_id: String,
}

impl CompatibleSupport {
    /// This support effect's statSet selection: the gem instance's `statSetIndex` is
    /// only meaningful for the **primary effect**; an additionally-granted support half
    /// uses the default set (vendor's additional effects share the gemInstance but the
    /// set selection doesn't carry across effects).
    pub(crate) fn stat_set_index(&self, group: &SocketGroup) -> Option<u32> {
        let gem = &group.gem_skills[self.gem_index];
        (gem.skill_id == self.effect_id)
            .then_some(gem.stat_set_index)
            .flatten()
    }
}

/// Runs **support-applicability judgement + the addSkillTypes fixed point** on a socket
/// group (matching PoB2 `Modules/CalcActiveSkill.lua:179-210`, contract C2):
///
/// 1. Seed: the active skill's (`active_skill_id`) effect's `skill_types` set;
/// 2. pass1 (:182-191): each support in slot order goes through
///    [`pobr_core::skill_source::can_support`]'s four-stage judgement — a compatible one
///    merges its `add_skill_types` (a plain token list, not an expression) into the set;
///    an incompatible one goes into the rejected list;
/// 3. repeat-until fixed point (:193-208): rescans the rejected list until a pass adds
///    nothing new — guaranteeing the judgement result is independent of support slot
///    order (a BA arrangement of "A adds a type, B requires that type" also converges);
/// 4. pass2 (:210-214): **fully re-judges** against the final type set to produce the
///    compatible list (matching PoB2: a support pass1 accepted can be rejected here if
///    it's hit by an exclude from a type merged in later; its already-merged add types
///    are kept, matching PoB2's no-rollback behavior).
///
/// Contract C2 note: this signature carries one extra parameter, `active_skill_id`,
/// compared to the prototype — PoB2's judgement targets a **single active skill** (in a
/// meta group, the first non-support slot might be a meta shell rather than the real
/// main skill picked by `resolve_main_skill`), and since the caller already holds the
/// resolution result, passing it in avoids re-deriving or mis-deriving it here.
pub(crate) fn judge_group_supports(
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
) -> GroupSupportJudgement {
    use pobr_core::skill_source::{ActiveSkillJudgeInput, SupportJudgeInput, can_support};
    use std::collections::HashSet;

    let active_effect = data.granted_effects.get(active_skill_id);
    let mut skill_types: HashSet<String> = active_effect
        .map(|e| e.skill_types.iter().cloned().collect())
        .unwrap_or_default();
    let cannot_be_supported = active_effect.is_some_and(|e| e.cannot_be_supported);

    // In-group support candidates (slot order preserved): among each gem's primary
    // granted effect + additional granted effects (the `gem_effects` foreign key),
    // whichever are `is_support` — vendor routes each effect in grantedEffectList by its
    // support flag, and a meta gem's (Blasphemy) support half lives in the additional
    // slot (SupportBlasphemyPlayer, carrying skill-local segments like `CurseEffect MORE`).
    // Active / unknown effects don't participate in the judgement.
    let support_candidates: Vec<(usize, &str)> = group
        .gem_skills
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            std::iter::once(g.skill_id.as_str())
                .chain(
                    data.gem_effects
                        .get(&g.skill_id)
                        .into_iter()
                        .flat_map(|l| l.additional_granted_effect_ids.iter().map(String::as_str)),
                )
                .filter(|id| data.granted_effects.get(*id).is_some_and(|e| e.is_support))
                .map(move |id| (i, id))
        })
        .collect();

    // Four-stage judgement (matching CalcTools.lua:84-110): cannotBeSupported →
    // supportGemsOnly → exclude expression → require expression (empty = accept). A
    // skill in a socket group is always gem-granted (from_gem=true); the fromItem
    // special case and the minionTypes secondary set are deferred.
    let judge = |effect_id: &str, types: &HashSet<String>| -> bool {
        data.granted_effects.get(effect_id).is_some_and(|effect| {
            can_support(
                &SupportJudgeInput {
                    support_gems_only: effect.support_gems_only,
                    exclude_skill_types: &effect.exclude_skill_types,
                    require_skill_types: &effect.require_skill_types,
                },
                &ActiveSkillJudgeInput {
                    cannot_be_supported,
                    from_gem: true,
                    skill_types: types,
                },
            )
        })
    };
    let merge_add = |effect_id: &str, types: &mut HashSet<String>| {
        if let Some(effect) = data.granted_effects.get(effect_id) {
            for t in &effect.add_skill_types {
                types.insert(t.clone());
            }
        }
    };

    // pass1: a compatible support merges addSkillTypes; an incompatible one goes into the rejected list.
    let mut rejected: Vec<&(usize, &str)> = Vec::new();
    for cand in &support_candidates {
        if judge(cand.1, &skill_types) {
            merge_add(cand.1, &mut skill_types);
        } else {
            rejected.push(cand);
        }
    }
    // repeat-until fixed point: rescans the rejected list until a pass adds nothing new.
    loop {
        let mut newly_accepted = false;
        let mut still_rejected = Vec::with_capacity(rejected.len());
        for cand in rejected {
            if judge(cand.1, &skill_types) {
                newly_accepted = true;
                merge_add(cand.1, &mut skill_types);
            } else {
                still_rejected.push(cand);
            }
        }
        rejected = still_rejected;
        if !newly_accepted {
            break;
        }
    }
    // pass2: fully re-judge against the final type set.
    let compatible: Vec<CompatibleSupport> = support_candidates
        .iter()
        .filter(|(_, id)| judge(id, &skill_types))
        .map(|&(i, id)| CompatibleSupport {
            gem_index: i,
            effect_id: id.to_string(),
        })
        .collect();
    GroupSupportJudgement {
        compatible,
        final_skill_types: skill_types,
    }
}

/// Maps the **compatible support gems'** per-level stats in the main skill's group
/// through [`map_skill_stat`] into SupportGem-attributed modifiers, injected into the
/// supported skill (e.g. "added lightning damage" → `LightningDamageMin/Max` BASE,
/// "more damage" → `Damage` MORE).
///
/// Before injection, [`judge_group_supports`] produces the compatible list: **a rejected
/// support doesn't participate at all** (neither its numeric values nor its
/// manaMultiplier applies, matching PoB2's `CalcActiveSkill.lua:210-214` semantics of
/// only putting compatible supports into effectList).
///
/// The current scope is **global** (correct semantics under a single-main-skill build:
/// every support's multiplier applies to the one skill being calculated); per-skill tag
/// isolation for multiple main skills (applying only to the supported skill) is deferred
/// until the flag system is wired up. The active main skill's own damage is already
/// injected by [`skill_base_modifiers`]; this only handles supports.
pub(crate) fn support_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
) -> Vec<Modifier> {
    let judgement = judge_group_supports(group, data, active_skill_id);
    let mut mods = Vec::new();
    for sup in &judgement.compatible {
        let gem = &group.gem_skills[sup.gem_index];
        let set_index = sup.stat_set_index(group);
        // TODO(T1, add after rebasing post-T3.6 merge): change the quality argument to
        // gem.quality — supports have no quality table entries (PoB2 skips them at
        // export), so this segment is currently always empty and passing 0 is
        // equivalent to passing gem.quality.
        let stats = data.effect_stats(&sup.effect_id, gem.gem_level, 0, set_index);
        // A support's set_key is taken from its own selected set (per-set overrides are
        // located by the support's effect id). Note: vendor doesn't pass a statSet for
        // support effects (CalcActiveSkill.lua:130 does a full merge across all sets) —
        // the full merge for a multi-set support's additional sets is a current gap.
        let set_key = data.selected_set_key(&sup.effect_id, set_index);
        mods.extend(mapped_stat_modifiers(
            &stats.base,
            SourceKind::SupportGem,
            &sup.effect_id,
            &sup.effect_id,
            set_key.as_deref(),
        ));
        // A compatible support's per-level cost multiplier → `SupportManaMultiplier`
        // MORE (matching PoB2's `CalcActiveSkill.lua:689-691`:
        // `NewMod("SupportManaMultiplier","MORE", level.manaMultiplier, modSource)`).
        // Only injected for the **compatible list** — a rejected support's multiplier
        // doesn't apply, matching PoB2's rejection. Consumed by
        // `skill_mechanics::calc_skill_cost` (the multipliers are chained and truncated
        // to 4 decimal places, then applied to base cost before the inc/more chain).
        if let Some(mm) = data
            .granted_effect_levels
            .get(&sup.effect_id)
            .and_then(|rows| {
                rows.iter()
                    .rfind(|r| r.level <= gem.gem_level)
                    .or(rows.first())
            })
            .and_then(|row| row.mana_multiplier)
            .filter(|&v| v != 0.0)
        {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SupportGem,
                format!("support.{}.manaMultiplier", sup.effect_id),
            ))
            .with_raw_text(format!("support {} cost multiplier {mm}%", sup.effect_id));
            mods.push(
                Modifier::number("SupportManaMultiplier", ModType::More, mm).with_origin(origin),
            );
        }
    }
    mods
}

#[cfg(test)]
mod support_judgement_tests {
    //! T3.5 unit tests for group-level support judgement + the addSkillTypes fixed
    //! point (matching PoB2 `Modules/CalcActiveSkill.lua:179-210`).

    use super::{BuildData, GroupSupportJudgement, judge_group_supports, support_modifiers};
    use crate::build::SocketGroup;
    use std::collections::HashMap;

    /// Constructs a minimal GrantedEffectDef (judgement-relevant fields configurable, rest default).
    fn effect(
        id: &str,
        is_support: bool,
        skill_types: &[&str],
        require: &[&str],
        add: &[&str],
        exclude: &[&str],
        cannot_be_supported: bool,
    ) -> pobr_data::catalog::GrantedEffectDef {
        let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        pobr_data::catalog::GrantedEffectDef {
            id: id.into(),
            is_support,
            active_skill: (!is_support).then(|| id.to_string()),
            cast_time: Some(1000),
            require_skill_types: v(require),
            add_skill_types: v(add),
            exclude_skill_types: v(exclude),
            cannot_be_supported,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
            skill_types: v(skill_types),
        }
    }

    /// Data + assembly: one active plus several supports in a given order.
    fn judge(
        effects: &[pobr_data::catalog::GrantedEffectDef],
        gem_order: &[&str],
    ) -> GroupSupportJudgement {
        let mut granted_effects = HashMap::new();
        for e in effects {
            granted_effects.insert(e.id.clone(), e.clone());
        }
        let data = BuildData {
            granted_effects,
            ..BuildData::empty()
        };
        let mut group = SocketGroup::new();
        for id in gem_order {
            group = group.with_gem_skill(*id, 20);
        }
        judge_group_supports(&group, &data, "MainSpell")
    }

    /// Converts the compatible list back to effect ids (for assertion readability).
    fn compatible_ids(j: &GroupSupportJudgement, _gem_order: &[&str]) -> Vec<String> {
        j.compatible.iter().map(|c| c.effect_id.clone()).collect()
    }

    /// The fixed point is independent of slot order (CalcActiveSkill.lua:193-208): "A
    /// adds Triggered, B requires Triggered" produces the same judgement result under
    /// both AB and BA slot orders (B is accepted either way).
    #[test]
    fn fixed_point_is_slot_order_independent() {
        let effects = vec![
            effect(
                "MainSpell",
                false,
                &["Spell", "Damage"],
                &[],
                &[],
                &[],
                false,
            ),
            effect("SupAdd", true, &[], &[], &["Triggered"], &[], false),
            effect("SupNeed", true, &[], &["Triggered"], &[], &[], false),
        ];
        let ab = judge(&effects, &["MainSpell", "SupAdd", "SupNeed"]);
        let ba = judge(&effects, &["MainSpell", "SupNeed", "SupAdd"]);

        assert_eq!(
            compatible_ids(&ab, &["MainSpell", "SupAdd", "SupNeed"])
                .iter()
                .collect::<std::collections::BTreeSet<_>>(),
            compatible_ids(&ba, &["MainSpell", "SupNeed", "SupAdd"])
                .iter()
                .collect::<std::collections::BTreeSet<_>>(),
            "AB 与 BA 插槽顺序的兼容名单应一致"
        );
        assert_eq!(ab.final_skill_types, ba.final_skill_types);
        assert_eq!(ab.compatible.len(), 2, "两个 support 都应兼容");
        assert!(ab.final_skill_types.contains("Triggered"));
    }

    /// An incompatible support is rejected, and its addSkillTypes are **not merged**
    /// into the set (CalcActiveSkill.lua:182-191 only merges compatible ones).
    #[test]
    fn rejected_support_does_not_merge_add_types() {
        let effects = vec![
            effect(
                "MainSpell",
                false,
                &["Spell", "Damage"],
                &[],
                &[],
                &[],
                false,
            ),
            effect("SupMelee", true, &[], &["Melee"], &["Area"], &[], false),
        ];
        let j = judge(&effects, &["MainSpell", "SupMelee"]);
        assert!(j.compatible.is_empty(), "require Melee 对法术应被拒");
        assert!(
            !j.final_skill_types.contains("Area"),
            "被拒 support 的 addSkillTypes 不得并入"
        );
    }

    /// Active effect cannotBeSupported → every support is rejected (the first stage of the four-stage judgement, `CalcTools.lua:86-88`).
    #[test]
    fn cannot_be_supported_rejects_everything() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], true),
            effect("SupAny", true, &[], &[], &[], &[], false),
        ];
        let j = judge(&effects, &["MainSpell", "SupAny"]);
        assert!(j.compatible.is_empty());
    }

    /// pass2's final re-judgement (CalcActiveSkill.lua:210-214): a support pass1
    /// accepted ends up rejected if it's hit by an exclude from a type merged in later;
    /// already-merged add types are kept (matching PoB2's no-rollback behavior). Both
    /// slot orders produce the same result.
    #[test]
    fn pass2_rejudges_against_final_type_set() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], false),
            effect("SupExcl", true, &[], &[], &[], &["Minion"], false),
            effect("SupAddMinion", true, &[], &[], &["Minion"], &[], false),
        ];
        for order in [
            ["MainSpell", "SupExcl", "SupAddMinion"],
            ["MainSpell", "SupAddMinion", "SupExcl"],
        ] {
            let j = judge(&effects, &order);
            assert_eq!(
                compatible_ids(&j, &order),
                vec!["SupAddMinion"],
                "exclude 被终态集合命中的 support 应被拒（顺序 {order:?}）"
            );
            assert!(j.final_skill_types.contains("Minion"), "已并入类型不回滚");
        }
    }

    /// An all-empty-gated support (no require/exclude) is always compatible (empty require = accept).
    #[test]
    fn empty_gating_always_compatible() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], false),
            effect("SupPlain", true, &[], &[], &[], &[], false),
        ];
        let j = judge(&effects, &["MainSpell", "SupPlain"]);
        assert_eq!(j.compatible.len(), 1);
    }

    /// T3.6 injection side: a rejected support's stats **produce no modifier at all**
    /// (none of its numeric values apply). Compatibility is judged by
    /// judge_group_supports; this uses empty stat data, only verifying the list-filter
    /// path doesn't panic and produces nothing (see tests/support_gating.rs for the
    /// end-to-end assertion of numeric injection).
    #[test]
    fn support_modifiers_skips_rejected_supports() {
        let effects = vec![
            effect("MainSpell", false, &["Spell"], &[], &[], &[], false),
            effect("SupMelee", true, &[], &["Melee"], &[], &[], false),
        ];
        let mut granted_effects = HashMap::new();
        for e in &effects {
            granted_effects.insert(e.id.clone(), e.clone());
        }
        let data = BuildData {
            granted_effects,
            ..BuildData::empty()
        };
        let group = SocketGroup::new()
            .with_gem_skill("MainSpell", 20)
            .with_gem_skill("SupMelee", 20);
        let mods = support_modifiers(&group, &data, "MainSpell");
        assert!(mods.is_empty(), "被拒 support 不得注入任何 modifier");
    }
}
