//! Calcs views of limits, speed and recovery multipliers, attribute
//! requirement multipliers and non-damaging ailment maximums. Each value is
//! read from the helper the calculation itself uses (or, for a value only
//! computed inside another stage, retained by that stage); nothing here
//! evaluates a mechanic a second time.
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
    pub action_speed_mod: f64,
    /// Movement speed before action speed is applied.
    pub movement_speed_mod: f64,
    pub light_radius_mod: f64,
    pub life_recovery_rate_mod: f64,
    pub mana_recovery_rate_mod: f64,
    pub energy_shield_recovery_rate_mod: f64,
    /// `LowPoolThreshold` as a percentage.
    pub low_life_percentage: f64,
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
    views.life_recovery_rate_mod =
        super::survivability::recoup_recovery_rate(db, cfg, RecoupResource::Life);
    views.mana_recovery_rate_mod =
        super::survivability::recoup_recovery_rate(db, cfg, RecoupResource::Mana);
    views.energy_shield_recovery_rate_mod =
        super::survivability::recoup_recovery_rate(db, cfg, RecoupResource::EnergyShield);
    views.low_life_percentage = low_pool_percentage(cfg);
    views.lowest_attribute = cfg.stat("LowestAttribute");
    views.global_gem_attribute_requirements = gem;
    views.global_item_attribute_requirements = item;
    views.global_weapon_attribute_requirements = weapon;
    views.applied = env.applied_ailments;
}

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
        "ActionSpeedMod" => v.action_speed_mod,
        "MovementSpeedMod" => v.movement_speed_mod,
        "LightRadiusMod" => v.light_radius_mod,
        "LifeRecoveryRateMod" => v.life_recovery_rate_mod,
        "ManaRecoveryRateMod" => v.mana_recovery_rate_mod,
        "EnergyShieldRecoveryRateMod" => v.energy_shield_recovery_rate_mod,
        // The radius in tenths of a metre; `PresenceRadiusMetres` holds the
        // same floored value in metres.
        "PresenceRadius" => (out.presence_radius_metres * 10.0).round(),
        "LowLifePercentage" => v.low_life_percentage,
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
