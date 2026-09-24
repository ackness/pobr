//! Support-group fixed point — the four-stage judgement + addSkillTypes convergence
//! (matching PoB2 `Modules/CalcActiveSkill.lua:179-210`, contract C2).
//!
//! This is the **engine-semantics** half of `pobr-build`'s `judge_group_supports`:
//! it takes pure data (gem candidates + effect lookups) and produces the compatible
//! list + final skill type set. The orchestrator's `judge_group_supports` is a thin
//! wrapper that assembles the inputs from `SocketGroup`/`BuildData`.

use std::collections::HashSet;

use pobr_data::catalog::GrantedEffectDef;

use crate::ingest::skill_source::{ActiveSkillJudgeInput, SupportJudgeInput, can_support};

/// A support gem candidate for the group-level judgement.
#[derive(Debug, Clone)]
pub struct SupportCandidate<'a> {
    /// This support's index into the host gem list (for tracing back to the gem).
    pub gem_index: usize,
    /// The support's granted effect id.
    pub effect_id: &'a str,
    /// The effect definition (pre-fetched by the caller).
    pub effect: &'a GrantedEffectDef,
}

/// The result of a group-level support-applicability judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSupportJudgement {
    /// Compatible supports (slot order preserved).
    pub compatible: Vec<CompatibleSupport>,
    /// The skill type set after the fixed point converges.
    pub final_skill_types: HashSet<String>,
}

/// A reference to one compatible support's effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibleSupport {
    /// This support's index into the host gem list.
    pub gem_index: usize,
    /// The support's granted effect id.
    pub effect_id: String,
}

/// Runs **support-applicability judgement + the addSkillTypes fixed point** on a
/// candidate list (matching PoB2 `Modules/CalcActiveSkill.lua:179-210`, contract C2).
///
/// 1. Seed: the active skill's `skill_types` set;
/// 2. pass1 (:182-191): each support in slot order goes through [`can_support`]'s
///    four-stage judgement — a compatible one merges its `add_skill_types` into the
///    set; an incompatible one goes into the rejected list;
/// 3. repeat-until fixed point (:193-208): rescans the rejected list until a pass
///    adds nothing new — guaranteeing the judgement result is independent of support
///    slot order;
/// 4. pass2 (:210-214): **fully re-judges** against the final type set to produce the
///    compatible list (a support pass1 accepted can be rejected here if it's hit by
///    an exclude from a type merged in later; its already-merged add types are kept,
///    matching PoB2's no-rollback behavior).
///
/// `active_cannot_be_supported` is the active effect's `cannotBeSupported` flag
/// (decision stage 1); `active_from_gem` is whether the active skill comes from a gem
/// (decision stage 2's `supportGemsOnly` gate).
pub fn judge_group_supports(
    active_skill_types: &HashSet<String>,
    active_cannot_be_supported: bool,
    active_from_gem: bool,
    candidates: &[SupportCandidate<'_>],
) -> GroupSupportJudgement {
    let mut skill_types = active_skill_types.clone();

    let judge = |effect: &GrantedEffectDef, types: &HashSet<String>| -> bool {
        can_support(
            &SupportJudgeInput {
                support_gems_only: effect.support_gems_only,
                exclude_skill_types: &effect.exclude_skill_types,
                require_skill_types: &effect.require_skill_types,
            },
            &ActiveSkillJudgeInput {
                cannot_be_supported: active_cannot_be_supported,
                from_gem: active_from_gem,
                skill_types: types,
            },
        )
    };
    let merge_add = |effect: &GrantedEffectDef, types: &mut HashSet<String>| {
        for t in &effect.add_skill_types {
            types.insert(t.clone());
        }
    };

    // pass1: a compatible support merges addSkillTypes; an incompatible one goes into the rejected list.
    let mut rejected: Vec<&SupportCandidate<'_>> = Vec::new();
    for cand in candidates {
        if judge(cand.effect, &skill_types) {
            merge_add(cand.effect, &mut skill_types);
        } else {
            rejected.push(cand);
        }
    }
    // repeat-until fixed point: rescans the rejected list until a pass adds nothing new.
    loop {
        let mut newly_accepted = false;
        let mut still_rejected = Vec::with_capacity(rejected.len());
        for cand in rejected {
            if judge(cand.effect, &skill_types) {
                newly_accepted = true;
                merge_add(cand.effect, &mut skill_types);
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
    let compatible: Vec<CompatibleSupport> = candidates
        .iter()
        .filter(|c| judge(c.effect, &skill_types))
        .map(|c| CompatibleSupport {
            gem_index: c.gem_index,
            effect_id: c.effect_id.to_string(),
        })
        .collect();
    GroupSupportJudgement {
        compatible,
        final_skill_types: skill_types,
    }
}
