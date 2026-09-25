//! triggers — trigger fixed point + support-applicability judgement (pure migration from calc_orchestrator, no logic change).

use pobr_core::Modifier;

use super::super::DataOrchestratorOptions;
use super::super::calculate_with_context;
use super::super::context::CalculationContext;
use crate::build::{Build, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};

/// The source skill's full sub-calculation statistics (contract 4's transport
/// surface): resolves `source_gem` to its `(group_index, gem_index)` and delegates to
/// the [`OrchestratorSubCalc`] channel (the same sub-calculation
/// `trigger_modifiers` runs for its source rate).
///
/// Guards: source = the triggered skill itself (a cycle) → `None`; a trigger-source
/// context → `None` (one level of depth); sub-calculation failure → `None`.
#[cfg(test)]
pub(crate) fn trigger_source_stats(
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    source_gem: &crate::build::GemSkillRef,
    main_skill_id: &str,
) -> Option<pobr_core::calc::TriggerSourceStats> {
    if source_gem.skill_id == main_skill_id || context.is_trigger_source {
        return None;
    }
    let group_index = build
        .enabled_socket_groups()
        .position(|g| std::ptr::eq(g, group))?;
    let gem_index = group
        .gem_skills
        .iter()
        .position(|g| std::ptr::eq(g, source_gem))?;
    let mut sub = OrchestratorSubCalc {
        context,
        build,
        data,
        options,
    };
    pobr_core::skill_env::TriggerSubCalc::source_stats(&mut sub, group_index, gem_index)
}

/// Whether `main_skill_id` or another gem in `group` matches a `trigger_configs`
/// entry (the meta-shell `Triggered` backfill in `prepare.rs`; the same four-level
/// recognition `trigger_modifiers` runs).
pub(crate) fn recognize_trigger_config(
    data: &BuildData,
    group: &SocketGroup,
    main_skill_id: &str,
) -> bool {
    let gems = group.gem_inputs();
    let eg = group.enabled_group(&gems);
    pobr_core::skill_env::recognize_trigger_config(data, &eg, main_skill_id).is_some()
}

/// The orchestrator's [`pobr_core::skill_env::TriggerSubCalc`] implementation: clones
/// the build, points `main_socket_group`/`main_active_skill` at the source gem, and
/// runs a one-level-deep `calculate_with_context` (trigger relations stripped inside
/// via `context.trigger_source()`).
struct OrchestratorSubCalc<'a> {
    context: &'a mut CalculationContext,
    build: &'a Build,
    data: &'a BuildData,
    options: &'a DataOrchestratorOptions,
}

impl pobr_core::skill_env::TriggerSubCalc for OrchestratorSubCalc<'_> {
    fn source_stats(
        &mut self,
        group_index: usize,
        gem_index: usize,
    ) -> Option<pobr_core::calc::TriggerSourceStats> {
        let group = self.build.enabled_socket_groups().nth(group_index)?;
        let source_gem = group.gem_skills.get(gem_index)?;
        // The source gem's 1-based ordinal in the group's **non-support** sequence
        // (the selection key of pick_group_main_skill).
        let active_pos = group
            .gem_skills
            .iter()
            .filter(|g| {
                !self
                    .data
                    .granted_effects
                    .get(&g.skill_id)
                    .map(|e| e.is_support)
                    .unwrap_or(false)
            })
            .position(|g| std::ptr::eq(g, source_gem))?
            + 1;

        let mut sub_build = self.build.clone();
        sub_build.main_socket_group = Some(group_index + 1);
        sub_build.socket_groups[group_index].main_active_skill = Some(active_pos);

        let mut child = self.context.trigger_source();
        let result = calculate_with_context(&sub_build, self.data, self.options, &mut child);
        self.context.compare_records.extend(child.compare_records);
        let session = result.ok()?;
        let out = session.output();
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
/// (`:716-770`). Sub-calculation guards: stripped inside a trigger-source context (this function's
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
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &ResolvedSkillLevel,
    group: &SocketGroup,
    main_skill_id: &str,
) -> Vec<Modifier> {
    // `None` (a detached/test group not in the build's socket_groups) = the
    // sub-calculation channel is unavailable; cooldown/flag mods still inject, the
    // source rate falls back to base `1/use_time` semantics.
    let group_index = build
        .enabled_socket_groups()
        .position(|g| std::ptr::eq(g, group));
    let gems = group.gem_inputs();
    let eg = group.enabled_group(&gems);
    let bonuses = super::resolve::gem_property_bonuses(build, data);
    let build_cfg = build.config.to_calc_config();
    let is_trigger_source = context.is_trigger_source;
    let mut sub = OrchestratorSubCalc {
        context,
        build,
        data,
        options,
    };
    let mut ctx = pobr_core::skill_env::TriggerCtx {
        groups: build,
        equipment: build,
        sub_calc: &mut sub,
        condition: &|name| build_cfg.condition(name),
        is_trigger_source,
        class_name: &build.character.class_name,
    };
    pobr_core::skill_env::trigger_modifiers(
        &mut ctx,
        data,
        &bonuses,
        main_skill,
        &eg,
        group_index,
        main_skill_id,
    )
}

// End of trigger section

#[cfg(test)]
mod support_judgement_tests {
    //! T3.5 unit tests for group-level support judgement + the addSkillTypes fixed
    //! point (matching PoB2 `Modules/CalcActiveSkill.lua:179-210`).

    use super::super::super::BuildData;
    use super::super::super::test_context;
    use super::super::mods::support_modifiers;
    use crate::build::SocketGroup;
    use crate::support::judge_group_supports;
    use pobr_core::skill_env::GroupSupportJudgement;
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
        judge_group_supports(&group, &data, "MainSpell", group.from_gem())
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
            "AB and BA socket order should produce the same compatibility list"
        );
        assert_eq!(ab.final_skill_types, ba.final_skill_types);
        assert_eq!(ab.compatible.len(), 2, "both supports should be compatible");
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
        assert!(
            j.compatible.is_empty(),
            "require Melee should reject a spell"
        );
        assert!(
            !j.final_skill_types.contains("Area"),
            "a rejected support's addSkillTypes must not be merged in"
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
                "a support hit by exclude from the final type set should be rejected (order {order:?})"
            );
            assert!(
                j.final_skill_types.contains("Minion"),
                "already-merged types don't roll back"
            );
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
        let mods = support_modifiers(&mut test_context(&data), &group, &data, "MainSpell");
        assert!(
            mods.is_empty(),
            "a rejected support must not inject any modifier"
        );
    }
}
