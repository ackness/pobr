//! Calcs views of limits, speed and recovery multipliers, attribute
//! requirement multipliers and non-damaging ailment maximums. Each value is
//! read from the helper the calculation itself uses (or, for a value only
//! computed inside another stage, retained by that stage); nothing here
//! evaluates a mechanic a second time.
use pobr_data::prelude::{ModName, ModType, SkillTypes};

use super::{
    Env, OutputTable,
    ailment_apply::{AppliedAilment, AppliedAilments},
    session::detail::DerivedDetail,
    survivability::RecoupResource,
};

/// Values that the calculation resolves for other purposes and the sidebar
/// only displays. Multipliers are neutral at 1; the default (all zero) is the
/// table before any calculation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LimitViews {
    /// Usable Charm slots (`charm_slots`).
    pub charm_limit: f64,
    /// The player's own curse and mark limits. `OutputTable::enemy_curse_limit`
    /// is the largest limit among all curse casters, so it can be higher.
    pub enemy_curse_limit: f64,
    pub enemy_mark_limit: f64,
    /// How many runes the selected Hazard skill can have placed at once
    /// (`rune_hazard_base_number_of_runes_allowed`, Conductive Runes: 20); 0
    /// for any other skill.
    pub hazard_rune_limit: f64,
    pub action_speed_mod: f64,
    /// Movement speed before action speed is applied.
    pub movement_speed_mod: f64,
    pub light_radius_mod: f64,
    /// Flat Freeze Threshold additions (`FreezeThreshold` BASE, including the share of maximum Energy
    /// Shield). The multiplier and modeled total are displayed separately.
    pub freeze_threshold_added: f64,
    pub freeze_threshold_mod: f64,
    /// The total: the Ailment Threshold plus what the build adds, times the Freeze Threshold modifiers. The game
    /// computes the chance to Freeze against the target's Ailment Threshold, which is the base taken here; the
    /// order of the flat additions and the increases is the Ailment Threshold's own. No in-game measurement.
    pub freeze_threshold: f64,
    pub life_recovery_rate_mod: f64,
    pub mana_recovery_rate_mod: f64,
    pub energy_shield_recovery_rate_mod: f64,
    /// `LowPoolThreshold` as a percentage.
    pub low_life_percentage: f64,
    /// The share of maximum Life and Mana that is not permanently reserved
    /// (`survivability::cancellable_reservation_percent`), and the permanent
    /// percentage it is derived from.
    pub life_cancellable_reservation: f64,
    pub mana_cancellable_reservation: f64,
    pub life_permanent_reserved: f64,
    pub mana_permanent_reserved: f64,
    /// `FullLifePercentage`: the share of maximum Life at or above which the
    /// character counts as on Full Life, and the override that set it.
    pub full_life_percentage: f64,
    pub full_life_override: Option<f64>,
    /// `FullManaPercentage`: the same threshold for Full Mana (Idle Hands).
    pub full_mana_percentage: f64,
    pub full_mana_override: Option<f64>,
    /// The generic skill duration modifier, from the same terms the skill
    /// duration uses (`skill_mechanics::duration_modifier_terms`).
    pub duration_mod: f64,
    pub duration_increased: f64,
    pub duration_more: f64,
    /// How many enemies the selected Strike skill targets (0 for any other
    /// skill) and the additional targets it was granted.
    pub strike_targets: f64,
    pub strike_skill: bool,
    pub additional_strike_targets: f64,
    pub lowest_attribute: f64,
    pub global_gem_attribute_requirements: f64,
    pub global_item_attribute_requirements: f64,
    pub global_weapon_attribute_requirements: f64,
    /// Chill and Shock as resolved by the enemy-ailment stage.
    pub applied: AppliedAilments,
}

/// Ids whose detail is a derived formula (listed in `DERIVED_STATS`); the
/// others are modifier queries named in `source_names`.
pub(crate) fn has_derived_detail(stat: &str) -> bool {
    matches!(
        stat,
        "LowLifePercentage"
            | "LifeCancellableReservation"
            | "ManaCancellableReservation"
            | "FullLifePercentage"
            | "FullManaPercentage"
            | "DurationMod"
            | "FreezeThreshold"
            | "StrikeTargets"
            | "LowestAttribute"
            | "MaximumChill"
            | "MaximumShock"
            | "CurrentChill"
            | "CurrentShock"
    )
}

/// Stores the values that are not computed by the sidebar itself.
pub(super) fn fill(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let [gem, item, weapon] = super::skill_mechanics::global_requirement_modifiers(db, cfg);
    let views = &mut env.player.output.limit_views;
    views.charm_limit = super::env_finalize::charm_slots(db, cfg);
    views.enemy_curse_limit = super::buff_pass::player_curse_limit(db, cfg);
    views.enemy_mark_limit = super::buff_pass::player_mark_limit(db, cfg);
    views.hazard_rune_limit = db
        .sum(ModType::Base, cfg, &[ModName::from("HazardRuneLimit")])
        .max(0.0);
    views.life_recovery_rate_mod =
        super::survivability::recoup_recovery_rate(db, cfg, RecoupResource::Life);
    views.mana_recovery_rate_mod =
        super::survivability::recoup_recovery_rate(db, cfg, RecoupResource::Mana);
    views.energy_shield_recovery_rate_mod =
        super::survivability::recoup_recovery_rate(db, cfg, RecoupResource::EnergyShield);
    views.low_life_percentage = low_pool_percentage(cfg);
    views.life_permanent_reserved =
        super::survivability::permanent_reserved_percent(db, cfg, "Life");
    views.mana_permanent_reserved =
        super::survivability::permanent_reserved_percent(db, cfg, "Mana");
    views.life_cancellable_reservation =
        super::survivability::cancellable_reservation_percent(views.life_permanent_reserved);
    views.mana_cancellable_reservation =
        super::survivability::cancellable_reservation_percent(views.mana_permanent_reserved);
    views.full_life_override = db.override_(cfg, "FullLifePercentage");
    views.full_life_percentage = views
        .full_life_override
        .unwrap_or(DEFAULT_FULL_LIFE_PERCENTAGE);
    views.full_mana_override = db.override_(cfg, "FullManaPercentage");
    views.full_mana_percentage = views
        .full_mana_override
        .unwrap_or(DEFAULT_FULL_LIFE_PERCENTAGE);
    let duration =
        super::skill_mechanics::duration_modifier_terms(db, cfg, Some("PrimaryDuration"));
    views.duration_increased = duration.increased;
    views.duration_more = duration.more;
    views.duration_mod = duration.value();
    views.strike_skill = cfg
        .skill_types
        .intersects(SkillTypes::from_pob2_name("MeleeSingleTarget").unwrap());
    views.additional_strike_targets = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("AdditionalStrikeTarget")],
    );
    views.strike_targets = if views.strike_skill {
        1.0 + views.additional_strike_targets
    } else {
        0.0
    };
    views.lowest_attribute = cfg.stat("LowestAttribute");
    views.global_gem_attribute_requirements = gem;
    views.global_item_attribute_requirements = item;
    views.global_weapon_attribute_requirements = weapon;
    views.applied = env.applied_ailments;
}

/// Full Life means no Life is missing: the character is at 100% of maximum
/// Life unless a `full_life_threshold_%_override` lowers the bar. The game
/// exports no constant for it and there is no in-game measurement. Full Mana
/// uses the same default (`full_mana_threshold_%_override` lowers it).
const DEFAULT_FULL_LIFE_PERCENTAGE: f64 = 100.0;

/// The game's `LowPoolThreshold` as a percentage, the same constant that
/// decides the automatic Low Life condition.
fn low_pool_percentage(cfg: &crate::CalcConfig) -> f64 {
    cfg.constants.game().low_pool_threshold * 100.0
}

pub(crate) fn display_value(out: &OutputTable, stat: &str) -> Option<f64> {
    let v = &out.limit_views;
    Some(match stat {
        "CharmLimit" => v.charm_limit,
        "EnemyCurseLimit" => v.enemy_curse_limit,
        "EnemyMarkLimit" => v.enemy_mark_limit,
        "ActiveHazardLimit" => v.hazard_rune_limit,
        "ActionSpeedMod" => v.action_speed_mod,
        "MovementSpeedMod" => v.movement_speed_mod,
        "LightRadiusMod" => v.light_radius_mod,
        "FreezeThresholdAdded" => v.freeze_threshold_added,
        "FreezeThresholdMod" => v.freeze_threshold_mod,
        "FreezeThreshold" => v.freeze_threshold,
        "LifeRecoveryRateMod" => v.life_recovery_rate_mod,
        "ManaRecoveryRateMod" => v.mana_recovery_rate_mod,
        "EnergyShieldRecoveryRateMod" => v.energy_shield_recovery_rate_mod,
        // The radius in tenths of a metre; `PresenceRadiusMetres` holds the
        // same floored value in metres.
        "PresenceRadius" => (out.presence_radius_metres * 10.0).round(),
        "LowLifePercentage" => v.low_life_percentage,
        "LifeCancellableReservation" => v.life_cancellable_reservation,
        "ManaCancellableReservation" => v.mana_cancellable_reservation,
        "FullLifePercentage" => v.full_life_percentage,
        "FullManaPercentage" => v.full_mana_percentage,
        "DurationMod" => v.duration_mod,
        "StrikeTargets" => v.strike_targets,
        "LowestAttribute" => v.lowest_attribute,
        "GlobalGemAttributeRequirements" => v.global_gem_attribute_requirements,
        "GlobalItemAttributeRequirements" => v.global_item_attribute_requirements,
        "GlobalWeaponAttributeRequirements" => v.global_weapon_attribute_requirements,
        "MaximumChill" => v.applied.chill.maximum,
        "MaximumShock" => v.applied.shock.maximum,
        "CurrentChill" => v.applied.chill.current,
        "CurrentShock" => v.applied.shock.current,
        _ => return None,
    })
}

/// The override replaces the default threshold outright, so the formula is
/// the chosen threshold; the context says which it was.
fn full_pool_detail(pool: &str, value: f64, overridden: Option<f64>) -> DerivedDetail {
    DerivedDetail {
        formula: format!("Full{pool}Threshold"),
        inputs: vec![(format!("Full{pool}Threshold"), value)],
        context: vec![
            (
                format!("DefaultFull{pool}Threshold"),
                DEFAULT_FULL_LIFE_PERCENTAGE,
            ),
            (
                format!("Full{pool}ThresholdOverridden"),
                f64::from(u8::from(overridden.is_some())),
            ),
        ],
    }
}

fn maximum_detail(name: &str, applied: &AppliedAilment) -> DerivedDetail {
    let terms = applied.terms;
    match terms.override_value {
        Some(value) => DerivedDetail {
            formula: format!("{name}MaxOverride"),
            inputs: vec![(format!("{name}MaxOverride"), value)],
            context: Vec::new(),
        },
        None => DerivedDetail {
            formula: format!("GameMaximum + {name}Max"),
            inputs: vec![
                ("GameMaximum".into(), terms.game_maximum),
                (format!("{name}Max"), terms.bonus),
            ],
            context: Vec::new(),
        },
    }
}

fn current_detail(applied: &AppliedAilment) -> DerivedDetail {
    if !applied.applied {
        // No source applies the ailment, or it is already applied.
        return DerivedDetail {
            formula: "NotApplied".into(),
            inputs: vec![("NotApplied".into(), 0.0)],
            context: vec![
                ("EnemyAilmentMagnitude".into(), applied.enemy_value),
                ("MaximumMagnitude".into(), applied.maximum),
            ],
        };
    }
    DerivedDetail {
        formula: "floor(min(max(AilmentSourceMagnitude, EnemyAilmentMagnitude), MaximumMagnitude))"
            .into(),
        inputs: vec![
            ("AilmentSourceMagnitude".into(), applied.source),
            ("EnemyAilmentMagnitude".into(), applied.enemy_value),
            ("MaximumMagnitude".into(), applied.maximum),
        ],
        context: Vec::new(),
    }
}

pub(crate) fn detail(env: &Env, stat: &str) -> Option<DerivedDetail> {
    let out = &env.player.output;
    let (formula, inputs): (&str, Vec<(String, f64)>) = match stat {
        "LowLifePercentage" => (
            "LowPoolThreshold * 100",
            vec![(
                "LowPoolThreshold".into(),
                env.cfg.constants.game().low_pool_threshold,
            )],
        ),
        "LifeCancellableReservation" => (
            "max(100 - ExtraLifeReserved, 0)",
            vec![(
                "ExtraLifeReserved".into(),
                out.limit_views.life_permanent_reserved,
            )],
        ),
        "ManaCancellableReservation" => (
            "max(100 - ExtraManaReserved, 0)",
            vec![(
                "ExtraManaReserved".into(),
                out.limit_views.mana_permanent_reserved,
            )],
        ),
        "FullLifePercentage" => {
            let views = &out.limit_views;
            return Some(full_pool_detail(
                "Life",
                views.full_life_percentage,
                views.full_life_override,
            ));
        }
        "FullManaPercentage" => {
            let views = &out.limit_views;
            return Some(full_pool_detail(
                "Mana",
                views.full_mana_percentage,
                views.full_mana_override,
            ));
        }
        "FreezeThreshold" => (
            "(AilmentThreshold + FreezeThresholdAdded) * FreezeThresholdMod",
            vec![
                ("AilmentThreshold".into(), out.ailment_threshold),
                (
                    "FreezeThresholdAdded".into(),
                    out.limit_views.freeze_threshold_added,
                ),
                (
                    "FreezeThresholdMod".into(),
                    out.limit_views.freeze_threshold_mod,
                ),
            ],
        ),
        "DurationMod" => (
            "max((1 + DurationIncreased / 100) * DurationMore, 0)",
            vec![
                (
                    "DurationIncreased".into(),
                    out.limit_views.duration_increased,
                ),
                ("DurationMore".into(), out.limit_views.duration_more),
            ],
        ),
        "StrikeTargets" => (
            "StrikeSkill * (1 + AdditionalStrikeTarget)",
            vec![
                (
                    "StrikeSkill".into(),
                    f64::from(u8::from(out.limit_views.strike_skill)),
                ),
                (
                    "AdditionalStrikeTarget".into(),
                    out.limit_views.additional_strike_targets,
                ),
            ],
        ),
        "LowestAttribute" => (
            "min(Strength, Dexterity, Intelligence)",
            vec![
                ("Strength".into(), out.strength),
                ("Dexterity".into(), out.dexterity),
                ("Intelligence".into(), out.intelligence),
            ],
        ),
        "MaximumChill" => return Some(maximum_detail("Chill", &out.limit_views.applied.chill)),
        "MaximumShock" => return Some(maximum_detail("Shock", &out.limit_views.applied.shock)),
        "CurrentChill" => return Some(current_detail(&out.limit_views.applied.chill)),
        "CurrentShock" => return Some(current_detail(&out.limit_views.applied.shock)),
        _ => return None,
    };
    Some(DerivedDetail {
        formula: formula.into(),
        inputs,
        context: Vec::new(),
    })
}
