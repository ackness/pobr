use std::collections::BTreeSet;

use pobr_core::calc::conditional_hit::{ConditionalHitContext, ConditionalHitSource};
use pobr_core::calc::crit::ResolvedCriticalBase;
use pobr_core::skill_env::{self, StatMapCtx, StatMapMode};
use pobr_core::{ModValue, Modifier};
use pobr_data::catalog::native_player::{NATIVE_PLAYER_CONTEXT_PATH, NativeFactDispositionKind};
use pobr_data::catalog::{
    GrantedEffectDef, SkillDamageStat, SkillLevelDef, SkillStatSetLevel, StatSetDef,
};
use pobr_data::constants::DamageType;
use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};
use pobr_data::source::SourceKind;
use pobr_gamedata::NativeData;

use crate::native_document::NativeBuildRequest;

use super::{
    NativeAdmissionError, NativeDispositionCounts, NativeModifierEvidence, NativeResourceCost,
    NativeSkillFact, NativeSkillProvenance, modifier_evidence, reference, source_error,
};

const SETS: &str = "base/granted_effect_stat_sets.json";
const EFFECTS: &str = "base/granted_effects.json";
const LEVELS: &str = "base/granted_effect_levels.json";

pub(super) struct Resolved {
    pub context: ConditionalHitContext,
    pub critical_base: ResolvedCriticalBase,
    pub hit_modifiers: Vec<Modifier>,
    pub intrinsic_modifiers: Vec<NativeModifierEvidence>,
    pub base_use_ms: u32,
    pub costs: Vec<NativeResourceCost>,
    pub total_projectiles: Option<f64>,
    pub projectile_lifetime_ms: Option<f64>,
    pub provenance: NativeSkillProvenance,
    pub skill_facts: Vec<NativeSkillFact>,
    pub assumptions: Vec<String>,
}

fn require(condition: bool, path: &str, reason: &str) -> Result<(), NativeAdmissionError> {
    if condition {
        Ok(())
    } else {
        Err(source_error(path, reason))
    }
}

fn unique<'a, T: 'a>(
    mut rows: impl Iterator<Item = (usize, &'a T)>,
    path: &str,
) -> Result<(usize, &'a T), NativeAdmissionError> {
    let selected = rows
        .next()
        .ok_or_else(|| source_error(path, "exact selected row is absent"))?;
    require(
        rows.next().is_none(),
        path,
        "ambiguous duplicate selected stable ID or exact level",
    )?;
    Ok(selected)
}

fn exact_names(
    names: &[String],
    expected: &[&str],
    path: &str,
) -> Result<(), NativeAdmissionError> {
    let unique: BTreeSet<_> = names.iter().map(String::as_str).collect();
    require(
        unique.len() == names.len() && unique == expected.iter().copied().collect(),
        path,
        "selected categorical inventory is missing, duplicated or contains an unmodelled channel",
    )
}

pub(super) fn resolve(
    data: &NativeData,
    request: &NativeBuildRequest,
) -> Result<Resolved, NativeAdmissionError> {
    let skill = &request.skill;
    let historical = data.primary_spells().is_none();
    let selection = data
        .primary_spells()
        .and_then(|p| p.selections.iter().find(|s| s.gem_id == skill.gem_id));
    let (base_index, base) = unique(
        data.base_items()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.id == skill.gem_id),
        "base/base_items.json",
    )?;
    require(
        base.implicits.is_empty()
            && base.weapon.is_none()
            && base.armour.is_none()
            && base.spirit.is_none()
            && base.charm_buff.is_empty()
            && base.utility.is_none(),
        "base/base_items.json",
        "selected stable gem base cannot supply extra mechanical channels",
    )?;
    let (gem_index, gem) = unique(
        data.skill_gems()
            .iter()
            .enumerate()
            .filter(|(_, g)| g.id == skill.gem_id),
        "base/skill_gems.json",
    )?;
    require(
        !gem.is_support
            && gem.gem_type == Some(0)
            && gem.gem_effect_id.as_deref()
                == Some(selection.map_or("Spark", |s| s.gem_effect_id.as_str()))
            && gem.granted_effect_id.as_deref() == Some(skill.effect_id.as_str())
            && gem.additional_granted_effect_ids.is_empty(),
        "base/skill_gems.json",
        "gem must uniquely grant the selected active effect without additional effects",
    )?;
    if historical {
        exact_names(
            &gem.gem_tags,
            &[
                "intelligence",
                "grants_active_skill",
                "spell",
                "projectile",
                "lightning",
                "duration",
                "repeatable",
            ],
            "base/skill_gems.json/gem_tags",
        )?;
    }
    let (effect_index, effect) = unique(
        data.granted_effects()
            .iter()
            .enumerate()
            .filter(|(_, e)| e.id == skill.effect_id),
        EFFECTS,
    )?;
    let effect_path = format!("/{effect_index}");
    let base_use_ms = effect_scope(effect, historical)?;
    let element = element(effect)?;
    if !historical {
        gem_categories(&gem.gem_tags, element)?;
        require(
            selection.is_some_and(|s| {
                effect.active_skill.as_deref() == Some(s.active_skill_id.as_str())
                    && s.effect_id == skill.effect_id
                    && s.stat_set_id == skill.stat_set_id
                    && s.gem_levels.contains(&skill.level)
            }),
            EFFECTS,
            "sealed primary selection does not agree",
        )?;
    }
    let levels = data
        .granted_effect_levels()
        .get(&skill.effect_id)
        .ok_or_else(|| source_error(LEVELS, "selected effect level table is absent"))?;
    let (level_index, level) = unique(
        levels
            .iter()
            .enumerate()
            .filter(|(_, l)| l.level == skill.level),
        LEVELS,
    )?;
    let level_path = format!("/{}/{level_index}", escape(&skill.effect_id));
    let chance_fraction = level_scope(level)?;
    let (sets_index, sets) = unique(
        data.skill_stat_sets()
            .iter()
            .enumerate()
            .filter(|(_, s)| s.effect_id == skill.effect_id),
        SETS,
    )?;
    // Stable identity joins are independent of vendor order and physical FK row evidence.
    let (set_index, set) = unique(
        sets.sets
            .iter()
            .enumerate()
            .filter(|(_, s)| s.set_id == skill.stat_set_id),
        SETS,
    )?;
    let set_path = format!("/{sets_index}/sets/{set_index}");
    set_scope(set)?;
    let (stat_level_index, stat_level) = unique(
        set.levels
            .iter()
            .enumerate()
            .filter(|(_, l)| l.gem_level == skill.level),
        SETS,
    )?;
    let stat_level_path = format!("{set_path}/levels/{stat_level_index}");
    let SkillStatSetLevel {
        gem_level: _,
        damage_multiplier,
        stats,
        per_effectiveness,
    } = stat_level;
    require(
        *damage_multiplier == 1.0 && per_effectiveness.is_empty(),
        &stat_level_path,
        "resolved spell stats cannot also supply a multiplier or user effectiveness channel",
    )?;
    let SelectedStats {
        facts: skill_facts,
        hit: hit_modifiers,
        intrinsic: intrinsic_modifiers,
        count: total_projectiles,
        duration_ms: projectile_lifetime_ms,
    } = selected_stats(
        set,
        stats,
        &set_path,
        &stat_level_path,
        SelectionSemantics {
            effect_id: &effect.id,
            element,
            historical,
            // The shared duration stat has a certified lifetime role only for this
            // exact Spark primary identity; Frostbolt's duration retains no such role.
            spark_lifetime: skill.gem_id == super::scope::GEM_ID
                && effect.id == super::scope::EFFECT_ID
                && set.set_id == super::scope::EFFECT_ID,
        },
    )?;
    let costs = costs(data, effect, level, &effect_path, &level_path)?;
    let policy = data
        .actor_binding()
        .ok_or_else(|| source_error("actor_binding", "missing admitted actor binding"))?;
    let bonus = &policy.critical_bonus;
    // NativeData validated this scalar against the sole plain source assignment. Do not
    // substitute a character default or import unrelated metadata/starting choices.
    require(
        bonus.bonus_percent.is_finite() && bonus.bonus_percent >= 0.0,
        "base/native_actor_binding.json/critical_bonus",
        "critical bonus must be finite and nonnegative",
    )?;
    let critical_base = ResolvedCriticalBase {
        chance_fraction,
        damage_bonus_percent: bonus.bonus_percent,
    };
    let flags = skill_env::stat_set_hit_flags(
        skill_env::skill_type_flags(&effect.skill_types),
        &set.implicit_stats,
    );
    let context = ConditionalHitContext {
        source: ConditionalHitSource::Spell,
        delivery_flags: flags.without(ModFlags::SPELL).without(ModFlags::HIT),
        damage_keywords: match element {
            DamageType::Fire => KeywordFlags::FIRE,
            DamageType::Cold => KeywordFlags::COLD,
            DamageType::Lightning => KeywordFlags::LIGHTNING,
            _ => return Err(source_error(EFFECTS, "unsupported primary damage element")),
        },
    };
    let mut provenance = NativeSkillProvenance {
        contract: data.contract().clone(),
        selection_sources: vec![
            reference("base/base_items.json", format!("/{base_index}")),
            reference("base/skill_gems.json", format!("/{gem_index}")),
            reference(EFFECTS, &effect_path),
            reference(LEVELS, &level_path),
            reference(SETS, &set_path),
            reference(SETS, &stat_level_path),
            reference(NATIVE_PLAYER_CONTEXT_PATH, "/facts/character/source"),
            reference("base/native_actor_binding.json", ""),
        ],
        critical_chance_source: reference(LEVELS, format!("{level_path}/crit_chance")),
        critical_bonus_source: reference(
            NATIVE_PLAYER_CONTEXT_PATH,
            format!(
                "/metadata/files/{}/assignments/{}",
                escape(&bonus.source.path),
                bonus.source.assignment
            ),
        ),
        base_use_source: reference(EFFECTS, format!("{effect_path}/cast_time")),
        required_conditions: policy.required_conditions.clone(),
        excluded_sources: policy.excluded_sources.clone(),
        actor_dispositions_source: reference("base/native_actor_binding.json", "/dispositions"),
        actor_disposition_counts: NativeDispositionCounts::from_facts(&policy.dispositions),
    };
    if let Some(projection) = data.primary_spells() {
        let index = projection
            .selections
            .iter()
            .position(|s| s.gem_id == skill.gem_id)
            .ok_or_else(|| source_error("primary_spells", "selected inventory entry is absent"))?;
        provenance.selection_sources.push(reference(
            pobr_data::catalog::native_runtime::NATIVE_SKILL_PROJECTION_PATH,
            format!("/selections/{index}"),
        ));
    }
    let mut assumptions = policy.assumptions.clone();
    assumptions.extend([
        "Ordinary stationary self-cast in Act1 with no quests or excluded sources; conditional on one successful damaging hit.",
        "Pre-resolved damage is used once. Projectile total/lifetime and path/re-hit facts never multiply this hit.",
        "Gem requirement levels and compatibility/UI fields are retained by the request and are not requirement certification.",
    ].map(String::from));
    Ok(Resolved {
        context,
        critical_base,
        hit_modifiers,
        intrinsic_modifiers,
        base_use_ms,
        costs,
        total_projectiles,
        projectile_lifetime_ms,
        provenance,
        skill_facts,
        assumptions,
    })
}

fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn effect_scope(effect: &GrantedEffectDef, historical: bool) -> Result<u32, NativeAdmissionError> {
    let GrantedEffectDef {
        id: _,
        is_support,
        active_skill,
        displayed_name: _,
        cast_time,
        require_skill_types,
        add_skill_types,
        exclude_skill_types,
        cannot_be_supported: _,
        support_gems_only,
        stat_set,
        additional_stat_set_ids,
        cost_types: _,
        skill_types,
        minion_list,
        add_minion_list,
        minion_uses,
        minion_has_item_set,
        ignore_minion_types,
    } = effect;
    require(
        !is_support
            && (if historical {
                active_skill.as_deref() == Some("spark")
            } else {
                active_skill.as_deref().is_some_and(|s| !s.is_empty())
            })
            && stat_set.is_some()
            && require_skill_types.is_empty()
            && add_skill_types.is_empty()
            && exclude_skill_types.is_empty()
            && !support_gems_only
            && additional_stat_set_ids.is_empty()
            && minion_list.is_empty()
            && add_minion_list.is_empty()
            && minion_uses.is_empty()
            && !minion_has_item_set
            && !ignore_minion_types,
        EFFECTS,
        "selected effect has an absent native join or unsupported support/minion/additional branch channel",
    )?;
    if historical {
        exact_names(
            skill_types,
            &[
                "Spell",
                "Projectile",
                "ProjectilesFromUser",
                "Damage",
                "Duration",
                "Trappable",
                "Totemable",
                "Mineable",
                "Multicastable",
                "Triggerable",
                "Lightning",
                "Unleashable",
                "Invokable",
                "UsableWhileMoving",
            ],
            EFFECTS,
        )?;
    } else {
        let allowed = [
            "Spell",
            "Projectile",
            "ProjectilesFromUser",
            "Damage",
            "Duration",
            "Trappable",
            "Totemable",
            "Mineable",
            "Multicastable",
            "Triggerable",
            "Lightning",
            "Unleashable",
            "Invokable",
            "UsableWhileMoving",
            "Area",
            "Fire",
            "Cold",
            "AreaSpell",
        ];
        let unique: BTreeSet<_> = skill_types.iter().map(String::as_str).collect();
        require(
            unique.len() == skill_types.len()
                && unique.iter().all(|s| allowed.contains(s))
                && ["Spell", "Projectile", "ProjectilesFromUser", "Damage"]
                    .iter()
                    .all(|s| unique.contains(s)),
            EFFECTS,
            "unknown, duplicate or incomplete primary spell categorical inventory",
        )?;
    }
    cast_time.filter(|ms| *ms > 0).ok_or_else(|| {
        source_error(
            EFFECTS,
            "analytical base cast time must be explicit and positive",
        )
    })
}

fn element(effect: &GrantedEffectDef) -> Result<DamageType, NativeAdmissionError> {
    let selected: Vec<_> = [
        ("Fire", DamageType::Fire),
        ("Cold", DamageType::Cold),
        ("Lightning", DamageType::Lightning),
    ]
    .into_iter()
    .filter(|(id, _)| effect.skill_types.iter().any(|s| s.as_str() == *id))
    .collect();
    require(
        selected.len() == 1,
        EFFECTS,
        "one source categorical primary element is required",
    )?;
    Ok(selected[0].1)
}

fn damage_pair(element: DamageType) -> Result<(&'static str, &'static str), NativeAdmissionError> {
    match element {
        DamageType::Fire => Ok((
            "spell_minimum_base_fire_damage",
            "spell_maximum_base_fire_damage",
        )),
        DamageType::Cold => Ok((
            "spell_minimum_base_cold_damage",
            "spell_maximum_base_cold_damage",
        )),
        DamageType::Lightning => Ok((
            "spell_minimum_base_lightning_damage",
            "spell_maximum_base_lightning_damage",
        )),
        _ => Err(source_error(EFFECTS, "unsupported primary damage element")),
    }
}

fn gem_categories(tags: &[String], element: DamageType) -> Result<(), NativeAdmissionError> {
    let allowed = [
        "intelligence",
        "grants_active_skill",
        "spell",
        "projectile",
        "repeatable",
        "duration",
        "area",
        "fire",
        "cold",
        "lightning",
    ];
    let selected: BTreeSet<_> = tags.iter().map(String::as_str).collect();
    let element_tag = match element {
        DamageType::Fire => "fire",
        DamageType::Cold => "cold",
        DamageType::Lightning => "lightning",
        _ => return Err(source_error(EFFECTS, "unsupported primary damage element")),
    };
    require(
        selected.len() == tags.len()
            && selected.iter().all(|t| allowed.contains(t))
            && [
                "intelligence",
                "grants_active_skill",
                "spell",
                "projectile",
                "repeatable",
                element_tag,
            ]
            .iter()
            .all(|t| selected.contains(t))
            && ["fire", "cold", "lightning"]
                .iter()
                .filter(|t| selected.contains(**t))
                .count()
                == 1,
        "base/skill_gems.json/gem_tags",
        "unknown, duplicated or disagreeing primary gem categories",
    )
}

fn level_scope(level: &SkillLevelDef) -> Result<f64, NativeAdmissionError> {
    let SkillLevelDef {
        level: _,
        cooldown_ms,
        attack_time_ms,
        cost_amounts: _,
        attack_speed_multiplier,
        base_multiplier,
        crit_chance,
        mana_multiplier,
        spirit_reservation_flat,
        reservation_multiplier,
        stored_uses,
        level_requirement: _,
    } = level;
    require(
        cooldown_ms.is_none()
            && attack_time_ms.is_none()
            && attack_speed_multiplier.is_none()
            && base_multiplier.is_none_or(|n| n == 1.0)
            && mana_multiplier.is_none()
            && spirit_reservation_flat.is_none()
            && reservation_multiplier.is_none()
            && stored_uses.is_none(),
        LEVELS,
        "cooldown, attack, multiplier, reservation and storage channels are not admitted",
    )?;
    let chance =
        crit_chance.ok_or_else(|| source_error(LEVELS, "intrinsic critical chance is required"))?;
    require(
        chance.is_finite() && (0.0..=100.0).contains(&chance),
        LEVELS,
        "critical chance must be finite percentage points in 0..=100",
    )?;
    Ok(chance / 100.0)
}

fn set_scope(set: &StatSetDef) -> Result<(), NativeAdmissionError> {
    let StatSetDef {
        set_id: _,
        label: _,
        vendor_set_index,
        base_effectiveness,
        user_level_scaling,
        constant_stats: _,
        skill_attack_speed_more,
        dot_flags,
        explode_corpse,
        implicit_stats: _,
        levels: _,
    } = set;
    require(
        vendor_set_index.is_none()
            && base_effectiveness.is_finite()
            && *base_effectiveness > 0.0
            && user_level_scaling.is_none()
            && skill_attack_speed_more.is_none()
            && dot_flags.is_default()
            && !explode_corpse,
        SETS,
        "vendor ordinals, scaling, speed, DoT and corpse channels are not admitted; effectiveness is reference-only",
    )
}

fn costs(
    data: &NativeData,
    effect: &GrantedEffectDef,
    level: &SkillLevelDef,
    effect_path: &str,
    level_path: &str,
) -> Result<Vec<NativeResourceCost>, NativeAdmissionError> {
    require(
        effect.cost_types.len() == level.cost_amounts.len() && effect.cost_types.len() == 1,
        LEVELS,
        "one complete instantaneous resource/amount pair is required",
    )?;
    let index = usize::try_from(effect.cost_types[0])
        .map_err(|_| source_error(EFFECTS, "cost index is out of range"))?;
    let resource = data.cost_types().get(index).ok_or_else(|| {
        source_error(
            "base/cost_types.json",
            "cost type reference is absent; no type-zero fallback exists",
        )
    })?;
    require(
        data.cost_types()
            .iter()
            .filter(|row| row.id == resource.id)
            .count()
            == 1
            && resource.id == "Mana"
            && resource.divisor > 0
            && !resource.per_minute,
        "base/cost_types.json",
        "cost must uniquely resolve Mana with a positive divisor and instantaneous semantics",
    )?;
    Ok(vec![NativeResourceCost {
        resource: resource.id.clone(),
        amount_per_use: f64::from(level.cost_amounts[0]) / f64::from(resource.divisor),
        raw_amount: level.cost_amounts[0],
        divisor: resource.divisor,
        amount_source: reference(LEVELS, format!("{level_path}/cost_amounts/0")),
        type_source: reference(EFFECTS, format!("{effect_path}/cost_types/0")),
        resource_source: reference("base/cost_types.json", format!("/{index}")),
    }])
}

struct SelectedStats {
    facts: Vec<NativeSkillFact>,
    hit: Vec<Modifier>,
    intrinsic: Vec<NativeModifierEvidence>,
    count: Option<f64>,
    duration_ms: Option<f64>,
}

#[derive(Clone, Copy)]
struct SelectionSemantics<'a> {
    effect_id: &'a str,
    element: DamageType,
    historical: bool,
    spark_lifetime: bool,
}

fn selected_stats(
    set: &StatSetDef,
    stats: &[SkillDamageStat],
    set_path: &str,
    level_path: &str,
    semantics: SelectionSemantics<'_>,
) -> Result<SelectedStats, NativeAdmissionError> {
    let mut selected = SelectedStats {
        facts: Vec::new(),
        hit: Vec::new(),
        intrinsic: Vec::new(),
        count: None,
        duration_ms: None,
    };
    let mut seen = BTreeSet::new();
    for (channel, values, path) in [
        ("level", stats, format!("{level_path}/stats")),
        (
            "constant",
            set.constant_stats.as_slice(),
            format!("{set_path}/constant_stats"),
        ),
    ] {
        for (i, stat) in values.iter().enumerate() {
            require(
                seen.insert(stat.stat.as_str()) && stat.value.is_finite(),
                &path,
                "duplicate selected stat or nonfinite value",
            )?;
            selected.push(stat, channel, format!("{path}/{i}"), semantics)?;
        }
    }
    let expected = [
        "spell_minimum_base_lightning_damage",
        "spell_maximum_base_lightning_damage",
        "base_number_of_projectiles",
        "base_skill_effect_duration",
        "spark_pathing_width_bounds",
        "damage_hit_effect_index",
        "movement_speed_+%_final_while_performing_action",
        "movement_speed_acceleration_+%_per_second_while_performing_action",
        "movement_speed_while_performing_action_locked_duration_%",
        "spark_min_time_between_target_clearing_ms",
    ];
    if semantics.historical {
        require(
            seen == expected.into_iter().collect(),
            SETS,
            "selected explicit stat inventory is incomplete",
        )?;
        exact_names(
            &set.implicit_stats,
            &[
                "base_is_projectile",
                "projectiles_should_collide_with_flying_monsters",
                "projectile_uses_contact_position",
                "projectile_uses_contact_direction",
                "check_for_targets_between_initiator_and_projectile_source",
                "can_perform_skill_while_moving",
            ],
            SETS,
        )?;
    }
    for (i, stat) in set.implicit_stats.iter().enumerate() {
        require(
            seen.insert(stat),
            SETS,
            "duplicate selected implicit/explicit stat",
        )?;
        let (disposition, reason) = match stat.as_str() {
            "base_is_projectile" => (
                NativeFactDispositionKind::Consumed,
                "source implicit stat supplies the shared hit projectile flag",
            ),
            "can_perform_skill_while_moving" => (
                NativeFactDispositionKind::Inactive,
                "ordinary stationary self-cast excludes moving state",
            ),
            "active_skill_consumes_a_fire_infusion" => {
                require(
                    semantics.element == DamageType::Fire,
                    SETS,
                    "fire infusion requires fire classification",
                )?;
                (
                    NativeFactDispositionKind::Inactive,
                    "infusion is excluded; this primary hit assumes no infusion is consumed",
                )
            }
            "projectiles_should_collide_with_flying_monsters"
            | "projectile_uses_contact_position"
            | "projectile_uses_contact_direction"
            | "check_for_targets_between_initiator_and_projectile_source"
            | "maintain_projectile_direction_when_using_contact_position"
            | "should_use_additive_aiming_animation"
            | "precise_cursor_targeting_uses_contact_point_height_offset"
            | "always_pierce" => (
                NativeFactDispositionKind::OutsideOutputScope,
                "contact geometry does not supply a multiplier to one successful hit",
            ),
            _ => {
                return Err(source_error(
                    SETS,
                    format!("unknown selected implicit stat {stat}"),
                ));
            }
        };
        selected.facts.push(NativeSkillFact {
            stat: stat.clone(),
            value: 1.0,
            source: reference(SETS, format!("{set_path}/implicit_stats/{i}")),
            disposition,
            reason: reason.into(),
        });
    }
    let (minimum, maximum) = damage_pair(semantics.element)?;
    let value = |id| {
        selected
            .facts
            .iter()
            .find(|f| f.stat == id)
            .map(|f| f.value)
    };
    let min = value(minimum).ok_or_else(|| source_error(SETS, "minimum damage is absent"))?;
    let max = value(maximum).ok_or_else(|| source_error(SETS, "maximum damage is absent"))?;
    require(min <= max, SETS, "minimum damage exceeds maximum damage")?;
    require(
        set.implicit_stats.iter().any(|s| s == "base_is_projectile"),
        SETS,
        "primary projectile requires its explicit implicit marker",
    )?;
    require(
        !semantics.historical || (selected.count.is_some() && selected.duration_ms.is_some()),
        SETS,
        "historical v1 requires complete count and projectile lifetime",
    )?;
    require(
        selected.count.is_none_or(f64::is_finite)
            && selected.duration_ms.is_none_or(f64::is_finite),
        SETS,
        "nonfinite intrinsic output",
    )?;
    Ok(selected)
}

impl SelectedStats {
    fn push(
        &mut self,
        stat: &SkillDamageStat,
        channel: &str,
        path: String,
        semantics: SelectionSemantics<'_>,
    ) -> Result<(), NativeAdmissionError> {
        let (expected_channel, mapping) = match stat.stat.as_str() {
            "spell_minimum_base_fire_damage" => ("level", Some(("FireDamageMin", "hit"))),
            "spell_maximum_base_fire_damage" => ("level", Some(("FireDamageMax", "hit"))),
            "spell_minimum_base_cold_damage" => ("level", Some(("ColdDamageMin", "hit"))),
            "spell_maximum_base_cold_damage" => ("level", Some(("ColdDamageMax", "hit"))),
            "spell_minimum_base_lightning_damage" => ("level", Some(("LightningDamageMin", "hit"))),
            "spell_maximum_base_lightning_damage" => ("level", Some(("LightningDamageMax", "hit"))),
            "base_number_of_projectiles" => {
                ("level", Some(("ProjectileCount", "total_projectiles")))
            }
            "base_skill_effect_duration" => (
                "constant",
                semantics
                    .spark_lifetime
                    .then_some(("SkillDurationBase", "projectile_lifetime_ms")),
            ),
            "active_skill_chill_effect_+%_final" => ("level", None),
            "projectile_maximum_range_override"
            | "spark_pathing_width_bounds"
            | "damage_hit_effect_index"
            | "spark_min_time_between_target_clearing_ms"
            | "movement_speed_+%_final_while_performing_action"
            | "movement_speed_acceleration_+%_per_second_while_performing_action"
            | "movement_speed_while_performing_action_locked_duration_%" => ("constant", None),
            _ => {
                return Err(source_error(
                    &path,
                    format!("unknown selected stat {}", stat.stat),
                ));
            }
        };
        require(
            channel == expected_channel,
            &path,
            "selected stat occurs in an unmodelled source channel",
        )?;
        if stat.stat.starts_with("spell_") {
            let pair = damage_pair(semantics.element)?;
            require(
                stat.stat == pair.0 || stat.stat == pair.1,
                &path,
                "damage pair disagrees with categorical element",
            )?;
        }
        if stat.stat == "active_skill_chill_effect_+%_final" {
            require(
                semantics.element == DamageType::Cold,
                &path,
                "chill magnitude requires cold classification",
            )?;
        }
        let (disposition, reason) = if mapping.is_some() {
            (
                NativeFactDispositionKind::Consumed,
                "owned no-catalog stat mapping, inspected before numerical use",
            )
        } else if stat.stat.starts_with("movement_speed_") {
            (
                NativeFactDispositionKind::Inactive,
                "stationary profile excludes movement modifiers while performing the action",
            )
        } else if stat.stat == "active_skill_chill_effect_+%_final" {
            (
                NativeFactDispositionKind::OutsideOutputScope,
                "chill magnitude is retained outside hit damage; it is not hit MORE",
            )
        } else if stat.stat == "base_skill_effect_duration" {
            require(
                stat.value >= 0.0,
                &path,
                "raw skill duration must be nonnegative",
            )?;
            (
                NativeFactDispositionKind::OutsideOutputScope,
                "raw skill duration retained; its projectile lifetime role is unestablished",
            )
        } else {
            (
                NativeFactDispositionKind::OutsideOutputScope,
                "path, visual and re-hit facts do not supply a multiplier to one successful hit",
            )
        };
        let mut fact = NativeSkillFact {
            stat: stat.stat.clone(),
            value: stat.value,
            source: reference(SETS, path.clone()),
            disposition,
            reason: reason.into(),
        };
        if let Some((name, reader)) = mapping {
            require(
                stat.value >= 0.0 && (name.ends_with("DamageMin") || stat.value > 0.0),
                &path,
                "consumed damage/count/duration must be nonnegative, with positive maximum/count/duration",
            )?;
            if name == "ProjectileCount" {
                require(
                    stat.value.fract() == 0.0 && stat.value >= 1.0,
                    &path,
                    "total projectile count must be a positive integer",
                )?;
            }
            let mut records = Vec::new();
            let mapped = skill_env::mapped_stat_modifiers(
                &mut StatMapCtx {
                    catalog: None,
                    mode: StatMapMode::Compare,
                    records: Some(&mut records),
                },
                std::slice::from_ref(stat),
                SourceKind::SkillGem,
                "native.skill",
                semantics.effect_id,
                None,
            );
            // The mapper intentionally skips zero. A zero minimum is explicitly
            // accounted through the owned rule; an unknown/empty mapping cannot pass.
            if stat.value == 0.0 {
                let zero = pobr_core::rules::skill_stats::skill_modifiers(&stat.stat, stat.value)
                    .ok_or_else(|| source_error(&path, "zero stat has no owned mapping"))?;
                require(
                    name.ends_with("DamageMin")
                        && mapped.is_empty()
                        && records.is_empty()
                        && zero.len() == 1
                        && valid_mapping(&zero[0], name)
                        && zero[0].value.as_number() == Some(0.0),
                    &path,
                    "zero stat mapping is not the explicitly accounted minimum",
                )?;
                fact.reason = "owned zero minimum explicitly accounted; mapper elides zero, so no hit modifier is needed".into();
            } else {
                require(
                    records.len() == 1
                        && records[0].stat == stat.stat
                        && records[0].classification == "mapped"
                        && mapped.len() == 1
                        && valid_mapping(&mapped[0], name),
                    &path,
                    "owned mapping is absent, ambiguous or outside the admitted vocabulary",
                )?;
                let modifier = &mapped[0];
                require(
                    modifier.origin.as_ref().is_some_and(|origin| {
                        origin.source_id.kind == SourceKind::SkillGem
                            && origin.source_id.id == format!("native.skill.{}", stat.stat)
                            && origin.stat_id.as_ref() == Some(&modifier.name)
                            && origin.mod_type == Some(modifier.mod_type)
                            && origin.raw_text.is_some()
                    }),
                    &path,
                    "owned mapping lost source/stat/type/raw provenance",
                )?;
                let value = modifier
                    .value
                    .as_number()
                    .ok_or_else(|| source_error(&path, "owned mapping is not numeric"))?;
                require(
                    value.is_finite() && value > 0.0,
                    &path,
                    "owned mapped value must remain positive and finite",
                )?;
                if reader == "hit" {
                    self.hit.extend(mapped);
                } else {
                    let mut evidence =
                        modifier_evidence(modifier, &fact.stat, &fact.source, Vec::new(), true);
                    evidence.intrinsic_reader = Some(reader.into());
                    self.intrinsic.push(evidence);
                    if reader == "total_projectiles" {
                        self.count = Some(value);
                    } else {
                        self.duration_ms = Some(value * 1000.0);
                    }
                }
            }
        }
        self.facts.push(fact);
        Ok(())
    }
}

fn valid_mapping(modifier: &Modifier, name: &str) -> bool {
    modifier.name.as_str() == name
        && modifier.mod_type == ModType::Base
        && matches!(&modifier.value, ModValue::Number(value) if value.is_finite())
        && modifier.flags.is_empty()
        && modifier.keyword_flags == KeywordFlags::NONE
        && modifier.tags.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spark_semantics() -> SelectionSemantics<'static> {
        SelectionSemantics {
            effect_id: "SparkPlayer",
            element: DamageType::Lightning,
            historical: true,
            spark_lifetime: true,
        }
    }

    #[test]
    fn new_elemental_sources_reject_nonfinite_ranges_and_preserve_absent_intrinsics() {
        let fixture: serde_json::Value =
            serde_json::from_slice(include_bytes!("tests/skill-facts.json")).unwrap();
        let mut set: StatSetDef =
            serde_json::from_value(fixture["granted_effect_stat_sets"][0]["sets"][0].clone())
                .unwrap();
        set.constant_stats.clear();
        set.implicit_stats = vec!["base_is_projectile".into()];
        for (element, effect_id) in [
            (DamageType::Fire, "FireballPlayer"),
            (DamageType::Cold, "FrostboltPlayer"),
        ] {
            let semantics = SelectionSemantics {
                effect_id,
                element,
                historical: false,
                spark_lifetime: false,
            };
            let (min, max) = damage_pair(element).unwrap();
            let stats = vec![
                SkillDamageStat {
                    stat: min.into(),
                    value: 0.0,
                },
                SkillDamageStat {
                    stat: max.into(),
                    value: 19.0,
                },
            ];
            let selected = selected_stats(&set, &stats, "/0", "/0/levels/0", semantics).unwrap();
            assert_eq!(selected.count, None);
            assert_eq!(selected.duration_ms, None);
            for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                for index in [0, 1] {
                    let mut invalid = stats.clone();
                    invalid[index].value = bad;
                    assert!(
                        selected_stats(&set, &invalid, "/0", "/0/levels/0", semantics).is_err()
                    );
                }
            }
        }
    }

    #[test]
    fn nonfinite_selected_source_values_cannot_be_admitted() {
        let fixture: serde_json::Value =
            serde_json::from_slice(include_bytes!("tests/skill-facts.json")).unwrap();
        let set: StatSetDef =
            serde_json::from_value(fixture["granted_effect_stat_sets"][0]["sets"][0].clone())
                .unwrap();
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut stats = set.levels[0].stats.clone();
            stats[0].value = value;
            assert!(
                selected_stats(
                    &set,
                    &stats,
                    "/0/sets/0",
                    "/0/sets/0/levels/0",
                    spark_semantics()
                )
                .is_err(),
                "{value}"
            );
            let mut constants = set.clone();
            constants.constant_stats[0].value = value;
            assert!(
                selected_stats(
                    &constants,
                    &set.levels[0].stats,
                    "/0/sets/0",
                    "/0/sets/0/levels/0",
                    spark_semantics(),
                )
                .is_err(),
                "{value}"
            );
            let mut effectiveness = set.clone();
            effectiveness.base_effectiveness = value;
            assert!(set_scope(&effectiveness).is_err(), "{value}");
            let mut level: SkillLevelDef =
                serde_json::from_value(fixture["granted_effect_levels"]["SparkPlayer"][0].clone())
                    .unwrap();
            level.crit_chance = Some(value);
            assert!(level_scope(&level).is_err(), "{value}");
        }
    }
}
