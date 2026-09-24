//! Shared support-group fixed point for calculation and candidate validation.

use crate::{BuildData, SocketGroup};

/// The result of a group-level support-applicability judgement.
///
/// `compatible` is the list of support effect references that **passed PoB2's four-stage
/// judgement** (slot order preserved); `final_skill_types` is the active skill's type
/// set after the addSkillTypes fixed point converges (seeded from the active effect's
/// `skill_types`, merged with every compatible support's `add_skill_types`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSupportJudgement {
    /// Compatible supports (slot order; includes the support half of an additionally-granted effect, see [`CompatibleSupport`]).
    pub compatible: Vec<CompatibleSupport>,
    /// The skill type set after the fixed point converges.
    pub final_skill_types: std::collections::HashSet<String>,
}

/// A reference to one compatible support's effect: level/quality/statSet index are
/// taken from the host gem instance (`gem_index`), while stat fetching uses
/// `effect_id` — for a normal support these come from the same source (effect_id = the
/// gem's primary effect); for a meta gem (Blasphemy), the primary effect is an active
/// skill, and the support half lives in an additional granted effect slot (the
/// `gem_effects` foreign key `additionalGrantedEffectId1..N`; vendor routes each effect
/// in grantedEffectList by its `support` flag, assembled in CalcSetup.lua's gemList).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibleSupport {
    /// This support's index into the host gem in `group.gem_skills`.
    pub gem_index: usize,
    /// The support's granted effect id (the fetch key for stat / manaMultiplier / set_key).
    pub effect_id: String,
}

impl CompatibleSupport {
    /// This support effect's statSet selection: the gem instance's `statSetIndex` is
    /// only meaningful for the **primary effect**; an additionally-granted support half
    /// uses the default set (vendor's additional effects share the gemInstance but the
    /// set selection doesn't carry across effects).
    pub fn stat_set_index(&self, group: &SocketGroup) -> Option<u32> {
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
pub fn judge_group_supports(
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
    from_gem: bool,
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
    // skill carries its source provenance explicitly; the minionTypes secondary
    // set remains deferred.
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
                    from_gem,
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
