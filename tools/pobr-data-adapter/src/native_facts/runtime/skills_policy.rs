//! Closed source-channel policy for ordinary primary projectile spell projections.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::super::trust::{VerifiedInput, require};
use super::inputs::{ids, index, row, rows};

pub(super) fn static_fields(
    input: &VerifiedInput,
    gem: &Value,
    gem_effect: &Value,
    effect: &Value,
    active: &Value,
    set: &Value,
) -> Result<(), String> {
    require(
        active["IsGem"] == true
            && active["IsManuallyCasted"] == true
            && active["WeaponRequirements"].is_null()
            && active["ShapeShiftForm"].is_null()
            && effect["MinionType"].is_null()
            && effect["CooldownNotRecoverDuringActive"] == false
            && gem["IsVaalVariant"] == false
            && gem_effect["Consumed_Mod"].is_null(),
        "selected source requires an unsupported actor/use interpretation",
    )?;
    require(
        effect["CastTime"]
            .as_i64()
            .is_some_and(|n| n > 0 && i32::try_from(n).is_ok()),
        "selected base cast time must be positive milliseconds",
    )?;
    for key in [
        "AllowedActiveSkillTypes",
        "AddedActiveSkillTypes",
        "ExcludedActiveSkillTypes",
        "AddedMinionActiveSkillTypes",
        "SupportWeaponRestrictions",
    ] {
        require(
            effect[key] == json!([]),
            &format!("unsupported effect channel {key}"),
        )?;
    }
    for key in ["CannotBeSupported", "SupportsGemsOnly", "IgnoreMinionTypes"] {
        require(
            effect[key] == false,
            &format!("unsupported effect channel {key}"),
        )?;
    }
    require(
        active["MinionActiveSkillTypes"] == json!([]),
        "unsupported minion skill types",
    )?;
    let skill_types = checked_ids(input, "ActiveSkillType", &active["ActiveSkillTypes"])?;
    for required in ["Spell", "Projectile", "ProjectilesFromUser", "Damage"] {
        require(
            skill_types.iter().any(|id| id == required),
            "missing ordinary projectile spell classification",
        )?;
    }
    for id in &skill_types {
        require(
            matches!(
                id.as_str(),
                "Spell"
                    | "Projectile"
                    | "ProjectilesFromUser"
                    | "Damage"
                    | "Duration"
                    | "Trappable"
                    | "Totemable"
                    | "Mineable"
                    | "Multicastable"
                    | "Triggerable"
                    | "Lightning"
                    | "Fire"
                    | "Cold"
                    | "Area"
                    | "AreaSpell"
                    | "Unleashable"
                    | "Invokable"
                    | "UsableWhileMoving"
            ),
            &format!("unexplained active skill type: {id}"),
        )?;
    }
    let elements = skill_types
        .iter()
        .filter(|s| matches!(s.as_str(), "Fire" | "Cold" | "Lightning"))
        .collect::<Vec<_>>();
    require(elements.len() == 1, "ambiguous primary spell element")?;
    let tags = checked_ids(input, "GemTags", &gem_effect["GemTags"])?;
    let element_tag = elements[0].to_lowercase();
    for required in [
        "spell",
        "projectile",
        "grants_active_skill",
        element_tag.as_str(),
    ] {
        require(
            tags.iter().any(|id| id == required),
            "gem/active source classification disagreement",
        )?;
    }
    for id in &tags {
        require(
            matches!(
                id.as_str(),
                "intelligence"
                    | "grants_active_skill"
                    | "spell"
                    | "projectile"
                    | "fire"
                    | "cold"
                    | "lightning"
                    | "duration"
                    | "area"
                    | "repeatable"
            ),
            "unexplained selected gem tag",
        )?;
        if matches!(id.as_str(), "fire" | "cold" | "lightning") {
            require(id == &element_tag, "conflicting gem element")?;
        }
    }
    for id in checked_ids(input, "Stats", &set["ConstantStats"])? {
        require(
            matches!(
                id.as_str(),
                "base_skill_effect_duration"
                    | "spark_pathing_width_bounds"
                    | "damage_hit_effect_index"
                    | "movement_speed_+%_final_while_performing_action"
                    | "movement_speed_acceleration_+%_per_second_while_performing_action"
                    | "movement_speed_while_performing_action_locked_duration_%"
                    | "spark_min_time_between_target_clearing_ms"
                    | "projectile_maximum_range_override"
            ),
            &format!("unexplained constant stat channel: {id}"),
        )?;
    }
    let implicit = checked_ids(input, "Stats", &set["ImplicitStats"])?;
    require(
        implicit.iter().any(|id| id == "base_is_projectile"),
        "missing primary projectile marker",
    )?;
    for id in implicit {
        // Each known channel is retained in the typed catalog. The consumer supplies
        // its per-stat disposition; no unknown implicit becomes generic geometry.
        require(
            matches!(
                id.as_str(),
                "base_is_projectile"
                    | "projectiles_should_collide_with_flying_monsters"
                    | "projectile_uses_contact_position"
                    | "projectile_uses_contact_direction"
                    | "check_for_targets_between_initiator_and_projectile_source"
                    | "can_perform_skill_while_moving"
                    | "maintain_projectile_direction_when_using_contact_position"
                    | "should_use_additive_aiming_animation"
                    | "active_skill_consumes_a_fire_infusion"
                    | "precise_cursor_targeting_uses_contact_point_height_offset"
                    | "always_pierce"
            ),
            &format!("unexplained implicit stat channel: {id}"),
        )?;
    }
    Ok(())
}

pub(super) fn level_fields(
    input: &VerifiedInput,
    level: &Value,
    set_level: &Value,
    active: &Value,
) -> Result<(), String> {
    for value in [&level["ActorLevel"], &set_level["ActorLevel"]] {
        require(
            value.as_f64().is_some_and(f64::is_finite),
            "nonfinite actor scaling coordinate",
        )?;
    }
    for key in [
        "Cooldown",
        "AttackTime",
        "AttackSpeedMultiplier",
        "Reservation",
        "StoredUses",
        "SoulGainPreventionDuration",
        "VaalSouls",
        "VaalStoredUses",
        "PvPDamageMultiplier",
    ] {
        require(
            i32_value(&level[key])? == 0,
            &format!("nonneutral selected level channel: {key}"),
        )?;
    }
    for key in ["CostMultiplier", "EffectOnPlayer"] {
        require(
            i32_value(&level[key])? == 100,
            &format!("nonneutral selected level channel: {key}"),
        )?;
    }
    for value in level["CostAmounts"].as_array().ok_or("missing costs")? {
        require(i32_value(value)? >= 0, "negative source Mana cost")?;
    }
    // Categorical cooldown metadata is retained, not normalized to a duration.
    i32_value(&level["CooldownGroup"])?;
    index(&level["CooldownBypassType"])?;
    require(
        i32_value(&set_level["BaseMultiplier"])? == 0,
        "unexplained base damage multiplier",
    )?;
    let crit = i32_value(&set_level["SpellCritChance"])?;
    require(
        (0..=10000).contains(&crit),
        "invalid source critical chance",
    )?;
    let effect_refs = set_level["GrantedEffects"]
        .as_array()
        .ok_or("missing effect references")?;
    let mut seen = BTreeSet::new();
    for reference in effect_refs {
        let physical = index(reference)?;
        row(input, "GrantedEffects", physical)?;
        require(seen.insert(physical), "duplicate stat-set effect reference")?;
    }
    let types = ids(input, "ActiveSkillType", &active["ActiveSkillTypes"])?;
    let element = types
        .iter()
        .find(|s| matches!(s.as_str(), "Fire" | "Cold" | "Lightning"))
        .ok_or("missing source element")?
        .to_lowercase();
    let float_ids = checked_ids(input, "Stats", &set_level["FloatStats"])?;
    let expected = [
        format!("spell_minimum_base_{element}_damage"),
        format!("spell_maximum_base_{element}_damage"),
    ];
    require(
        float_ids.len() == 2 && expected.iter().all(|id| float_ids.contains(id)),
        "incomplete/mismatched source damage channels",
    )?;
    let values = set_level["BaseResolvedValues"]
        .as_array()
        .ok_or("missing resolved values")?;
    let mut min_max = [0; 2];
    for (i, id) in expected.iter().enumerate() {
        let position = float_ids
            .iter()
            .position(|candidate| candidate == id)
            .ok_or("missing damage stat")?;
        min_max[i] = i32_value(values.get(position).ok_or("missing paired damage value")?)?;
    }
    require(
        min_max[0] >= 0 && min_max[0] <= min_max[1],
        "invalid source damage range",
    )?;
    for id in checked_ids(input, "Stats", &set_level["AdditionalStats"])? {
        require(
            matches!(
                id.as_str(),
                "base_number_of_projectiles" | "active_skill_chill_effect_+%_final"
            ),
            &format!("unexplained additional stat channel: {id}"),
        )?;
    }
    for value in set_level["FloatStatsValues"]
        .as_array()
        .ok_or("missing coefficients")?
    {
        require(
            value.as_f64().is_some_and(f64::is_finite),
            "nonfinite float coefficient",
        )?;
    }
    let interpolation_bases = set_level["InterpolationBases"]
        .as_array()
        .ok_or("missing interpolation bases")?;
    require(
        interpolation_bases.len() == float_ids.len(),
        "interpolation base length mismatch",
    )?;
    for value in interpolation_bases.iter().chain(
        set_level["StatInterpolations"]
            .as_array()
            .ok_or("missing interpolations")?,
    ) {
        // InterpolationBases are foreign rows; StatInterpolations are enum rows.
        // They remain references/categories, with no interpolation performed here.
        index(value)?;
    }
    Ok(())
}

fn i32_value(value: &Value) -> Result<i32, String> {
    value
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| format!("expected physical signed i32, got {value}"))
}

fn checked_ids(input: &VerifiedInput, table: &str, values: &Value) -> Result<Vec<String>, String> {
    let ids = ids(input, table, values)?;
    let mut seen = BTreeSet::new();
    for id in &ids {
        require(
            !id.is_empty() && seen.insert(id),
            "empty/duplicate selected categorical ID",
        )?;
        require(
            rows(input, table)?
                .iter()
                .filter(|row| row["values"]["Id"] == *id)
                .count()
                == 1,
            "aliased categorical stable ID",
        )?;
    }
    Ok(ids)
}
