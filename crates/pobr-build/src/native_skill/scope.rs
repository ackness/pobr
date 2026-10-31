use pobr_core::CampaignProgress;
use pobr_core::rules::config_interpreter::RawConfigInputs;
use pobr_data::build_config::BanditChoice;
use pobr_data::catalog::native_player::{NATIVE_PLAYER_ID, NativeActorPolicy, NativeModelStatus};
use pobr_data::catalog::native_runtime::{
    NATIVE_PLAYER_CONTRACT_VERSION, NativeInputProfile, NativeOutputProfile, NativeProfile,
};
use pobr_data::constants::DamageType;
use pobr_data::passive_tree::PassiveTreeSpec;
use pobr_gamedata::NativeData;

use crate::build::{Build, CharacterIdentity, GemSkillRef, SocketGroup, WeaponSet};
use crate::build_config::BuildConfig;
use crate::native_document::{NativeBuildRequest, NativeBuildSchema, NativeEvaluationKind};
use crate::party::Party;

use super::{NativeAdmissionError, source_error, unsupported};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Profile {
    NoAllocations,
    OrdinaryDamagePassives,
}

pub(super) const GEM_ID: &str = "Metadata/Items/Gems/SkillGemSpark";
pub(super) const EFFECT_ID: &str = "SparkPlayer";

fn require(condition: bool, path: &str, reason: &str) -> Result<(), NativeAdmissionError> {
    if condition {
        Ok(())
    } else {
        Err(unsupported(path, reason))
    }
}

pub(super) fn validate(
    data: &NativeData,
    request: &NativeBuildRequest,
) -> Result<Profile, NativeAdmissionError> {
    let contract = data.contract();
    let binding = &request.dataset;
    // Every saved identity field participates, independently of patch/display compatibility.
    for (matches, field) in [
        (binding.game == contract.game, "game"),
        (binding.realm == contract.realm, "realm"),
        (binding.game_patch == contract.game_patch, "game_patch"),
        (
            binding.tree_resource_sha256 == contract.tree.resource_sha256,
            "tree_resource_sha256",
        ),
        (
            binding.assembly_key == contract.assembly_key,
            "assembly_key",
        ),
        (
            binding.runtime_identity_sha256 == data.identity().sha256,
            "runtime_identity_sha256",
        ),
    ] {
        if !matches {
            return Err(NativeAdmissionError::DatasetMismatch { field });
        }
    }
    require(
        contract.schema_version == NATIVE_PLAYER_CONTRACT_VERSION
            && contract.player.is_some()
            && contract.output_profile == NativeOutputProfile::ConditionalSkillHitV1
            && contract.normalization.version == NATIVE_PLAYER_CONTRACT_VERSION
            && contract.rules.version == NATIVE_PLAYER_CONTRACT_VERSION,
        "dataset",
        "conditional hit requires the exact v2 player/input/output interpretation contract",
    )?;
    let player = data
        .player_context()
        .ok_or_else(|| source_error("player_context", "missing v2 player facts"))?;
    let policy = data
        .actor_binding()
        .ok_or_else(|| source_error("actor_binding", "missing v2 actor policy"))?;
    let profile = profile(contract, policy.policy)?;
    let policy_id = match profile {
        Profile::NoAllocations => "selected-player-generic-ot-v1",
        Profile::OrdinaryDamagePassives => "selected-player-generic-ot-ordinary-damage-passives-v1",
    };
    require(
        request.schema == NativeBuildSchema::V1
            && request.evaluation == NativeEvaluationKind::ConditionalSkillHitV1
            && request.evaluation == policy.evaluation
            && policy.schema_version == 1
            && policy.model_status == NativeModelStatus::ConditionalModel
            && request.actor_policy.id == policy_id
            && request.actor_policy.version == policy.schema_version
            && Some(request.actor_policy.sha256.as_str()) == data.actor_binding_sha256(),
        "actor_policy",
        "evaluation and exact selected profile policy/version1 artifact pin must agree",
    )?;
    require(
        request.actor.id == NATIVE_PLAYER_ID
            && request.actor.id == policy.actor_id
            && player.facts.character.source.id.as_deref() == Some(request.actor.id.as_str())
            && request.actor.level == 1
            && request.actor.level == policy.actor_level,
        "actor",
        "only the exact selected Character7 at level 1 is admitted",
    )?;
    if let Some(fact) = policy.dispositions.iter().find(|fact| {
        fact.relevant_to_conditional_hit
            && fact.disposition
                == pobr_data::catalog::native_player::NativeFactDispositionKind::Unresolved
    }) {
        return Err(source_error(
            format!("base/native_player_context.json#{}", fact.selector),
            &fact.reason,
        ));
    }
    require(
        request.skill.group_index == 0
            && request.skill.gem_index == 0
            && request.skill.gem_id == GEM_ID
            && request.skill.effect_id == EFFECT_ID
            && request.skill.stat_set_id == EFFECT_ID
            && request.skill.level == 1
            && request.skill.quality == 0,
        "skill",
        "only the stable primary SparkPlayer branch at exact level 1/quality 0 is admitted",
    )?;
    build(&request.build, request, profile)?;
    Ok(profile)
}

pub(super) fn profile(
    contract: &pobr_data::catalog::NativeRuntimeContract,
    policy: NativeActorPolicy,
) -> Result<Profile, NativeAdmissionError> {
    match (
        contract.input_profile,
        contract.normalization.profile,
        contract.rules.profile,
        policy,
    ) {
        (
            NativeInputProfile::SorceressSparkLevel1Q0V1,
            NativeProfile::SparkLevel1Q0,
            NativeProfile::SparkLevel1Q0,
            NativeActorPolicy::SelectedPlayerGenericOtV1,
        ) => Ok(Profile::NoAllocations),
        (
            NativeInputProfile::SorceressSparkLevel1Q0OrdinaryDamagePassivesV1,
            NativeProfile::SparkLevel1Q0OrdinaryDamagePassives,
            NativeProfile::SparkLevel1Q0OrdinaryDamagePassives,
            NativeActorPolicy::SelectedPlayerGenericOtOrdinaryDamagePassivesV1,
        ) => Ok(Profile::OrdinaryDamagePassives),
        _ => Err(unsupported(
            "dataset",
            "input, normalization, rules and actor policy must form an exact admitted tuple",
        )),
    }
}

pub(super) fn build(
    build: &Build,
    request: &NativeBuildRequest,
    profile: Profile,
) -> Result<(), NativeAdmissionError> {
    // No `..`: a new Build input must receive an explicit numerical-scope decision.
    let Build {
        character,
        view_mode: _,
        tree,
        tree_version: _,
        items,
        jewels,
        granted_socket_jewels,
        radius_jewels,
        utility_slots,
        inactive_utility_slots,
        limbs,
        weapon_set,
        other_weapons,
        other_weapon_set_nodes,
        socket_groups,
        main_socket_group,
        config,
        party,
    } = build;
    let CharacterIdentity {
        level,
        class_name: _,
        ascendancy_name,
    } = character;
    require(
        *level == request.actor.level && ascendancy_name.is_empty(),
        "build.character",
        "actor level must agree and ascendancy must be empty",
    )?;
    let PassiveTreeSpec {
        allocated_nodes,
        mastery_effects,
        attribute_overrides,
    } = tree;
    require(
        (profile == Profile::OrdinaryDamagePassives || allocated_nodes.is_empty())
            && mastery_effects.is_empty()
            && attribute_overrides.is_empty(),
        "build.tree",
        "only the new profile permits explicitly selected eligible ordinary damage passives; masteries/attribute overrides are excluded and starting choices grant nothing",
    )?;
    require(
        items.is_empty()
            && jewels.is_empty()
            && granted_socket_jewels.is_empty()
            && radius_jewels.is_empty()
            && utility_slots.is_empty()
            && inactive_utility_slots.is_empty()
            && limbs.is_empty()
            && other_weapons.is_empty()
            && other_weapon_set_nodes.is_none(),
        "build",
        "all equipment, utility, jewel, limb and alternate-allocation sources are excluded, including inactive sources",
    )?;
    let Party { members, exports } = party;
    require(
        members.is_empty() && exports.is_empty(),
        "build.party",
        "party inputs/exports are outside this admitted profile",
    )?;
    require(
        socket_groups.len() == 1 && matches!(main_socket_group, None | Some(1)),
        "build.socket_groups",
        "exactly one group is required, with main group absent or 1; disabled extras are excluded",
    )?;
    group(&socket_groups[0], *weapon_set, request)?;
    configuration(config)
}

fn group(
    group: &SocketGroup,
    active_set: WeaponSet,
    request: &NativeBuildRequest,
) -> Result<(), NativeAdmissionError> {
    let SocketGroup {
        include_in_full_dps,
        group_count,
        weapon_set,
        source,
        slot: _,
        enabled,
        gem_ids,
        active_skill_id,
        active_gem_level,
        active_gem_quality,
        gem_skills,
        gem_requirement_levels,
        main_active_skill,
        reservation_free_skill,
        granted_level,
    } = group;
    require(
        *enabled && !include_in_full_dps && group_count.is_none_or(|n| n == 1.0),
        "build.socket_groups/0",
        "group must be enabled with neutral Full DPS controls",
    )?;
    require(
        weapon_set.is_none_or(|number| number == active_set.number())
            && source.is_none()
            && reservation_free_skill.is_none()
            && granted_level.is_none(),
        "build.socket_groups/0",
        "weapon binding must match the active set; source/granted/reservation overrides are excluded",
    )?;
    require(
        gem_ids.len() == 1
            && gem_ids[0] == request.skill.gem_id
            && gem_skills.len() == 1
            && active_skill_id.as_deref() == Some(request.skill.effect_id.as_str())
            && *active_gem_level == Some(request.skill.level)
            && *active_gem_quality == Some(request.skill.quality)
            && matches!(main_active_skill, None | Some(1))
            && (gem_requirement_levels.is_empty() || gem_requirement_levels.as_slice() == [1]),
        "build.socket_groups/0",
        "one gem and all active mirrors must agree; ordinal selection and prepared requirement levels cannot select another branch",
    )?;
    gem(&gem_skills[0], request)
}

fn gem(gem: &GemSkillRef, request: &NativeBuildRequest) -> Result<(), NativeAdmissionError> {
    let GemSkillRef {
        enabled,
        count,
        enable_global_1,
        enable_global_2,
        skill_id,
        gem_level,
        global_level_bonus,
        quality,
        stat_set_index,
        name_spec,
        corrupted,
        minion,
        minion_skill,
    } = gem;
    require(
        *enabled
            && count.is_none_or(|n| n == 1.0)
            && enable_global_1.is_none()
            && enable_global_2.is_none()
            && *skill_id == request.skill.effect_id
            && *gem_level == request.skill.level
            && *quality == request.skill.quality
            && global_level_bonus.is_none()
            && stat_set_index.is_none()
            && name_spec.is_none()
            && !corrupted
            && minion.is_none()
            && minion_skill.is_none(),
        "build.socket_groups/0/gem_skills/0",
        "gem must be enabled, uncorrupted and neutral; global, vendor ordinal, name and minion selectors are excluded",
    )
}

fn configuration(config: &BuildConfig) -> Result<(), NativeAdmissionError> {
    let BuildConfig {
        is_attack,
        is_spell,
        damage_type,
        bandit,
        conditions,
        multipliers,
        global_modifier_texts,
        campaign_progress,
        enemy_tier,
        raw_inputs,
    } = config;
    let RawConfigInputs {
        values,
        placeholders,
    } = raw_inputs;
    // Classification is redundant compatibility state only. Core scope comes from source.
    require(
        matches!(
            (*is_attack, *is_spell, *damage_type),
            (false, false, None) | (false, true, Some(DamageType::Lightning))
        ),
        "build.config",
        "classification must be neutral or the exact agreeing spell/lightning mirror",
    )?;
    require(
        *bandit == BanditChoice::None
            && *campaign_progress == Some(CampaignProgress::Act1)
            && enemy_tier.is_none()
            && conditions.is_empty()
            && multipliers.is_empty()
            && global_modifier_texts.is_empty()
            && values.is_empty()
            && placeholders.is_empty(),
        "build.config",
        "explicit Act1/no quests is required; enemy and all config inputs, including false/zero raw placeholders, are excluded",
    )
}
