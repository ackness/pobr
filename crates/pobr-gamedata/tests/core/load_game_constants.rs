//! Loaded game constants obey the units and ranges used by engine formulas.
//! Package balance values can evolve independently of no-data defaults.

use pobr_data::catalog::game_constants::GameConstantsDef;
use pobr_gamedata::{GameData, current_data_dir};

fn load() -> GameConstantsDef {
    GameData::new(current_data_dir())
        .game_constants()
        .expect("game_constants should load")
}

#[test]
fn all_sections_contain_finite_numeric_constants() {
    let value = serde_json::to_value(load()).unwrap();
    for section in ["character", "monster", "game"] {
        let fields = value[section].as_object().expect("constant section");
        assert!(!fields.is_empty());
        for (name, value) in fields {
            assert!(
                value.as_f64().is_some_and(f64::is_finite),
                "{section}.{name}"
            );
        }
    }
}

#[test]
fn caps_and_fractions_have_their_documented_units() {
    let c = load();
    for cap in [
        c.character.base_maximum_all_resistances_pct,
        c.character.maximum_physical_damage_reduction_pct,
        c.monster.base_maximum_all_resistances_pct,
        c.monster.maximum_physical_damage_reduction_pct,
        c.game.resist_hard_cap,
        c.game.block_chance_cap,
        c.game.base_block_chance_max,
        c.game.evade_chance_cap,
        c.game.deflection_chance_cap,
        c.game.deflect_effect,
        c.game.dodge_chance_cap,
        c.game.avoid_chance_cap,
        c.game.suppression_chance_cap,
        c.game.suppression_effect,
        c.game.max_accuracy_range_penalty,
    ] {
        assert!((0.0..=100.0).contains(&cap), "percentage {cap}");
    }
    assert!(c.game.resist_floor <= c.game.resist_hard_cap);
    assert!(c.game.base_block_chance_max <= c.game.block_chance_cap);
    assert!(c.game.accuracy_falloff_start < c.game.accuracy_falloff_end);
    for fraction in [
        c.game.low_pool_threshold,
        c.game.leech_rate_base,
        c.game.buff_expiration_slow_cap,
    ] {
        assert!(fraction > 0.0 && fraction <= 1.0, "fraction {fraction}");
    }
    assert!(c.game.server_tick_seconds > 0.0 && c.game.server_tick_seconds < 1.0);
    assert!(c.game.armour_ratio > 0.0);
}

#[test]
fn ailment_and_recharge_parameters_are_usable() {
    let c = load();
    for value in [
        c.game.bleed_base_fraction,
        c.game.ignite_base_fraction,
        c.game.poison_base_fraction,
        c.game.bleed_base_duration,
        c.game.ignite_base_duration,
        c.game.poison_base_duration,
        c.game.shock_base_duration,
        c.game.chill_base_duration,
        c.game.freeze_base_duration,
        c.game.pin_base_duration,
        c.game.pin_damage_scale,
        c.game.heavy_stun_damage_scale,
        c.game.heavy_stun_damage_scale_player,
        c.game.stun_base_duration_seconds,
        c.character.energy_shield_recharge_delay_seconds,
        c.character.energy_shield_recharge_rate_per_minute_pct,
    ] {
        assert!(
            value.is_finite() && value > 0.0,
            "positive parameter {value}"
        );
    }
    assert!(c.game.shock_min_effect >= 0.0);
    assert_eq!(c.game.shock_default_effect, c.game.shock_min_effect / 100.0);
}

#[test]
fn ehp_iteration_limits_are_positive_integers() {
    let c = load();
    for count in [
        c.game.ehp_calc_max_iterations,
        c.game.max_hit_smoothing_passes,
    ] {
        assert!(
            count > 0.0 && count.fract() == 0.0,
            "iteration count {count}"
        );
    }
    for value in [
        c.game.ehp_calc_max_damage,
        c.game.ehp_calc_speed_up,
        c.game.normal_enemy_dps_mult,
        c.game.dot_dps_cap,
    ] {
        assert!(value.is_finite() && value > 0.0, "positive bound {value}");
    }
}
