//! Shared support-group fixed point for calculation and candidate validation.

use std::collections::HashSet;

use crate::{BuildData, SocketGroup};
use pobr_core::skill_env::{CompatibleSupport, GroupSupportJudgement, SupportCandidate};

/// Runs **support-applicability judgement + the addSkillTypes fixed point** on a socket
/// group (matching PoB2 `Modules/CalcActiveSkill.lua:179-210`, contract C2).
///
/// This is the orchestrator's thin wrapper around [`pobr_core::skill_env::judge_group_supports`]:
/// it assembles the candidate list from `SocketGroup`/`BuildData` and delegates the
/// four-stage judgement + fixed-point convergence to the engine-semantics layer.
///
/// Contract C2 note: this signature carries one extra parameter, `active_skill_id`,
/// compared to the prototype — PoB2's judgement targets a **single active skill** (in a
/// meta group, the first non-support slot might be a meta shell rather than the real
/// main skill picked by `resolve_main_skill`), and since the caller already holds the
/// resolution result, passing it in avoids re-deriving or mis-deriving it here.
pub fn judge_group_supports(
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
    from_gem: bool,
) -> GroupSupportJudgement {
    let active_effect = data.granted_effects.get(active_skill_id);
    let active_skill_types: HashSet<String> = active_effect
        .map(|e| e.skill_types.iter().cloned().collect())
        .unwrap_or_default();
    let cannot_be_supported = active_effect.is_some_and(|e| e.cannot_be_supported);

    // In-group support candidates (slot order preserved): among each gem's primary
    // granted effect + additional granted effects (the `gem_effects` foreign key),
    // whichever are `is_support` — vendor routes each effect in grantedEffectList by its
    // support flag, and a meta gem's (Blasphemy) support half lives in the additional
    // slot (SupportBlasphemyPlayer, carrying skill-local segments like `CurseEffect MORE`).
    // Active / unknown effects don't participate in the judgement.
    let candidates: Vec<SupportCandidate<'_>> = group
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
                .filter_map(move |id| {
                    data.granted_effects
                        .get(id)
                        .filter(|e| e.is_support)
                        .map(|effect| SupportCandidate {
                            gem_index: i,
                            effect_id: id,
                            effect,
                        })
                })
        })
        .collect();

    pobr_core::skill_env::judge_group_supports(
        &active_skill_types,
        cannot_be_supported,
        from_gem,
        &candidates,
    )
}

/// This support effect's statSet selection: the gem instance's `statSetIndex` is
/// only meaningful for the **primary effect**; an additionally-granted support half
/// uses the default set (vendor's additional effects share the gemInstance but the
/// set selection doesn't carry across effects).
pub fn support_stat_set_index(sup: &CompatibleSupport, group: &SocketGroup) -> Option<u32> {
    let gem = &group.gem_skills[sup.gem_index];
    (gem.skill_id == sup.effect_id)
        .then_some(gem.stat_set_index)
        .flatten()
}

/// Tests each explicit support against all active skills in the group. A support
/// must be accepted by at least one active skill. Search budgets, family conflicts,
/// lineage copy limits and level requirements belong to the caller.
/// Unknown effects are rejected rather than guessed compatible.
pub fn support_group_compatible(group: &SocketGroup, data: &BuildData) -> bool {
    if group
        .gem_skills
        .iter()
        .any(|gem| !data.granted_effects.contains_key(&gem.skill_id))
    {
        return false;
    }
    let active: Vec<_> = group
        .gem_skills
        .iter()
        .filter(|gem| !data.granted_effects[&gem.skill_id].is_support)
        .collect();
    if active.is_empty() {
        return false;
    }
    let accepted: std::collections::HashSet<_> = active
        .iter()
        .flat_map(|gem| {
            judge_group_supports(group, data, &gem.skill_id, group.from_gem()).compatible
        })
        .map(|support| support.gem_index)
        .collect();
    group.gem_skills.iter().enumerate().all(|(index, gem)| {
        !data.granted_effects[&gem.skill_id].is_support || accepted.contains(&index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data() -> BuildData {
        let mut data = BuildData::empty();
        for value in [
            json!({"id":"Spell", "is_support":false, "skill_types":["Spell"]}),
            json!({"id":"Attack", "is_support":false, "skill_types":["Attack"]}),
            json!({"id":"Enabler", "is_support":true, "require_skill_types":["Spell"], "add_skill_types":["Minion"]}),
            json!({"id":"NeedsMinion", "is_support":true, "require_skill_types":["Minion"]}),
            json!({"id":"NoMinion", "is_support":true, "exclude_skill_types":["Minion"]}),
            json!({"id":"GemOnly", "is_support":true, "support_gems_only":true}),
            json!({"id":"AttackOnly", "is_support":true, "require_skill_types":["Attack"]}),
        ] {
            let effect: pobr_data::catalog::GrantedEffectDef =
                serde_json::from_value(value).unwrap();
            data.granted_effects.insert(effect.id.clone(), effect);
        }
        data
    }

    #[test]
    fn group_gate_uses_fixed_point_and_final_exclusions() {
        let data = data();
        for supports in [["NeedsMinion", "Enabler"], ["Enabler", "NeedsMinion"]] {
            let group = SocketGroup::new()
                .with_gem_skill("Spell", 1)
                .with_gem_skill(supports[0], 1)
                .with_gem_skill(supports[1], 1);
            assert!(support_group_compatible(&group, &data));
            let rejected = group.with_gem_skill("NoMinion", 1);
            assert!(!support_group_compatible(&rejected, &data));
        }
        assert!(!support_group_compatible(
            &SocketGroup::new()
                .with_gem_skill("Spell", 1)
                .with_gem_skill("NeedsMinion", 1),
            &data
        ));
    }

    #[test]
    fn item_provenance_reaches_calculation_and_group_gate() {
        let data = data();
        let mut group = SocketGroup::new()
            .with_gem_skill("Spell", 1)
            .with_gem_skill("GemOnly", 1);
        assert!(support_group_compatible(&group, &data));
        assert_eq!(
            judge_group_supports(&group, &data, "Spell", true)
                .compatible
                .len(),
            1
        );
        group.source = Some("Item:weapon1".into());
        assert!(!group.from_gem());
        assert!(!support_group_compatible(&group, &data));
        assert!(
            judge_group_supports(&group, &data, "Spell", group.from_gem())
                .compatible
                .is_empty()
        );
    }

    #[test]
    fn supports_may_target_different_active_skills_but_unknowns_never_pass() {
        let data = data();
        let spell = SocketGroup::new()
            .with_gem_skill("Spell", 1)
            .with_gem_skill("AttackOnly", 1);
        assert!(!support_group_compatible(&spell, &data));
        assert!(support_group_compatible(
            &spell.with_gem_skill("Attack", 1),
            &data
        ));
        assert!(!support_group_compatible(&SocketGroup::new(), &data));
        assert!(!support_group_compatible(
            &SocketGroup::new()
                .with_gem_skill("Spell", 1)
                .with_gem_skill("Unknown", 1),
            &data
        ));
    }
}
