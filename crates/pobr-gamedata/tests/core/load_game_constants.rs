//! `base/game_constants.json` load tests:
//! - pobr-source-of-truth fields are asserted value-equal to
//!   `pobr_data::constants` (a migration invariant);
//! - vendor-only fields are spot-checked against hardcoded expected
//!   values, with the vendor file:line referenced.

use pobr_data::catalog::game_constants::GameConstantsDef;
use pobr_data::constants::{
    ARMOUR_RATIO, BLOCK_CHANCE_CAP, DEFAULT_MAX_RESISTANCE, DOT_DPS_CAP, GameConstants,
    HARD_MAX_RESISTANCE, PLAYER_BASE_CRIT_DAMAGE_BONUS, RESIST_FLOOR, SERVER_TICK_SECONDS,
    SHOCK_MIN_EFFECT,
};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> GameConstantsDef {
    GameData::new(repo_data_root().join(version()))
        .game_constants()
        .expect("game_constants 可加载")
}

/// pobr's source of truth (constants.rs's top-level constants) is value-equal.
#[test]
fn migrated_top_level_constants_match_rust_source() {
    let gc = load();
    assert_eq!(
        gc.character.base_maximum_all_resistances_pct,
        DEFAULT_MAX_RESISTANCE
    );
    assert_eq!(
        gc.character.base_critical_hit_damage_bonus,
        PLAYER_BASE_CRIT_DAMAGE_BONUS
    );
    assert_eq!(gc.game.resist_hard_cap, HARD_MAX_RESISTANCE);
    assert_eq!(gc.game.resist_floor, RESIST_FLOOR);
    assert_eq!(gc.game.server_tick_seconds, SERVER_TICK_SECONDS);
    assert_eq!(gc.game.armour_ratio, ARMOUR_RATIO);
    assert_eq!(gc.game.block_chance_cap, BLOCK_CHANCE_CAP);
    assert_eq!(gc.game.dot_dps_cap, DOT_DPS_CAP);
    assert_eq!(gc.game.shock_min_effect, SHOCK_MIN_EFFECT);
}

/// pobr's source of truth (`GameConstants::poe2()`'s default set) is value-equal.
#[test]
fn migrated_poe2_defaults_match_rust_source() {
    let gc = load();
    let rust = GameConstants::poe2();
    assert_eq!(gc.game.resist_hard_cap, rust.resist_hard_cap);
    assert_eq!(gc.game.resist_floor, rust.resist_floor);
    assert_eq!(gc.game.server_tick_seconds, rust.server_tick_seconds);
    assert_eq!(gc.game.armour_ratio, rust.armour_ratio);
    assert_eq!(gc.game.bleed_base_fraction, rust.bleed_base_fraction);
    assert_eq!(gc.game.bleed_base_duration, rust.bleed_base_duration);
    assert_eq!(gc.game.ignite_base_fraction, rust.ignite_base_fraction);
    assert_eq!(gc.game.ignite_base_duration, rust.ignite_base_duration);
    assert_eq!(gc.game.poison_base_fraction, rust.poison_base_fraction);
    assert_eq!(gc.game.poison_base_duration, rust.poison_base_duration);
    assert_eq!(gc.game.shock_default_effect, rust.shock_default_effect);
    assert_eq!(
        gc.character.base_critical_hit_damage_bonus,
        rust.player_base_crit_damage_bonus
    );
    assert_eq!(gc.game.block_chance_cap, rust.block_chance_cap);
    assert_eq!(
        gc.character.base_maximum_all_resistances_pct,
        rust.resist_default_max
    );
}

/// Spot-checks vendor-only fields (expected values hardcoded, sources noted per line).
#[test]
fn vendor_only_values_pinned_to_pob2_source() {
    let gc = load();

    // The character section: vendor Data/Misc.lua (exported from Character.ot).
    // Misc.lua:146 maximum_physical_damage_reduction_% = 90
    assert_eq!(gc.character.maximum_physical_damage_reduction_pct, 90.0);
    // Misc.lua:143 energy_shield_recharge_rate_per_minute_% = 750
    assert_eq!(
        gc.character.energy_shield_recharge_rate_per_minute_pct,
        750.0
    );
    // Modules/Data.lua:197 EnergyShieldRechargeDelay = 4
    assert_eq!(gc.character.energy_shield_recharge_delay_seconds, 4.0);
    // Misc.lua:144 character_inherent_mana_regeneration_rate_per_minute_% = 240
    assert_eq!(gc.character.mana_regeneration_rate_per_minute_pct, 240.0);

    // The monster section: vendor Data/Misc.lua (exported from Monster.ot).
    // Misc.lua:248 base_maximum_all_resistances_% = 75
    assert_eq!(gc.monster.base_maximum_all_resistances_pct, 75.0);
    // Misc.lua:247 maximum_physical_damage_reduction_% = 75
    assert_eq!(gc.monster.maximum_physical_damage_reduction_pct, 75.0);
    // Misc.lua:250 base_critical_hit_damage_bonus = 30
    assert_eq!(gc.monster.base_critical_hit_damage_bonus, 30.0);
    // Misc.lua:258 / :259 stun multipliers 33 / 100
    assert_eq!(gc.monster.melee_hit_stun_multiplier_pct, 33.0);
    assert_eq!(gc.monster.physical_hit_stun_multiplier_pct, 100.0);

    // The game section: the evade/deflect/suppression/avoid cap family.
    // Misc.lua:110 DefaultMaxEvadeChancePercent = 95 (Data.lua:182 EvadeChanceCap)
    assert_eq!(gc.game.evade_chance_cap, 95.0);
    // Modules/Data.lua:183 DeflectionChanceCap = 95
    assert_eq!(gc.game.deflection_chance_cap, 95.0);
    // Misc.lua:111 BasePercentDamageDeflected = 40 (Data.lua:188 DeflectEffect)
    assert_eq!(gc.game.deflect_effect, 40.0);
    // Modules/Data.lua:184 / :189 DodgeChanceCap / AvoidChanceCap = 75
    assert_eq!(gc.game.dodge_chance_cap, 75.0);
    assert_eq!(gc.game.avoid_chance_cap, 75.0);
    // Modules/Data.lua:186-187 SuppressionChanceCap = 100 / SuppressionEffect = 50
    assert_eq!(gc.game.suppression_chance_cap, 100.0);
    assert_eq!(gc.game.suppression_effect, 50.0);

    // The game section: accuracy falloff (Modules/Data.lua:190-192).
    assert_eq!(gc.game.accuracy_falloff_start, 20.0);
    assert_eq!(gc.game.accuracy_falloff_end, 90.0);
    assert_eq!(gc.game.max_accuracy_range_penalty, 90.0);

    // The game section: Chill (Misc.lua:76-77).
    assert_eq!(gc.game.chill_max_effect, 50.0);
    assert_eq!(gc.game.chill_effect_multiplier, 100.0);

    // The game section: low-pool threshold / leech (Data.lua:175 / :201, Misc.lua:130).
    assert_eq!(gc.game.low_pool_threshold, 0.35);
    assert_eq!(gc.game.leech_rate_base, 0.02);
    assert_eq!(gc.game.effective_max_damage_for_leech, 40000.0);

    // The game section: culling strike thresholds (Misc.lua:104-107: 35/20/10/5).
    assert_eq!(gc.game.culling_strike_normal_threshold, 35.0);
    assert_eq!(gc.game.culling_strike_magic_threshold, 20.0);
    assert_eq!(gc.game.culling_strike_rare_threshold, 10.0);
    assert_eq!(gc.game.culling_strike_unique_threshold, 5.0);

    // The game section: the full stun family.
    // Modules/Data.lua:217-219 MinStunChanceNeeded=20 / StunBaseMult=200 / StunBaseDuration=0.5
    assert_eq!(gc.game.min_stun_chance_needed, 20.0);
    assert_eq!(gc.game.stun_base_mult, 200.0);
    assert_eq!(gc.game.stun_base_duration_seconds, 0.5);
    // Misc.lua:44-47 light stun (monster/player): 15/58, 15/44
    assert_eq!(gc.game.light_stun_minimum_chance, 15.0);
    assert_eq!(gc.game.light_stun_ratio_scale, 58.0);
    assert_eq!(gc.game.light_stun_minimum_chance_player, 15.0);
    assert_eq!(gc.game.light_stun_ratio_scale_player, 44.0);
    // Misc.lua:48-53 heavy stun (monster/player): 0.58/500/16.5, 0.65/100/10
    assert_eq!(gc.game.heavy_stun_damage_scale, 0.58);
    assert_eq!(gc.game.heavy_stun_threshold_modifier, 500.0);
    assert_eq!(gc.game.heavy_stun_modifier_duration, 16.5);
    assert_eq!(gc.game.heavy_stun_damage_scale_player, 0.65);
    assert_eq!(gc.game.heavy_stun_threshold_modifier_player, 100.0);
    assert_eq!(gc.game.heavy_stun_modifier_duration_player, 10.0);

    // The game section: negative-armour damage-bonus cap (Modules/Data.lua:194).
    assert_eq!(gc.game.neg_armour_dmg_bonus_cap, 100.0);
}

///  Locks in the EHP loop magic numbers + normal-monster DPS multiplier
/// value-for-value (vendor Modules/Data.lua:228 / :235 / :237 / :239).
#[test]
fn m2_ehp_calc_constants_pinned_to_pob2_source() {
    let gc = load();

    // Modules/Data.lua:237 ehpCalcMaxDamage = 100000000
    assert_eq!(gc.game.ehp_calc_max_damage, 100_000_000.0);
    // Modules/Data.lua:239 ehpCalcMaxIterationsToCalc = 50
    assert_eq!(gc.game.ehp_calc_max_iterations, 50.0);
    // Modules/Data.lua:235 ehpCalcSpeedUp = 8
    assert_eq!(gc.game.ehp_calc_speed_up, 8.0);
    // Modules/Data.lua:228 normalEnemyDPSMult = 1 / 4.40 (bit-for-bit
    // equal under IEEE754)
    assert_eq!(gc.game.normal_enemy_dps_mult, 1.0 / 4.40);
}

///  Locks in the max-hit conversion smoothing iteration count (vendor
/// Modules/Data.lua:241's maxHitSmoothingPasses = 8, consumed by
/// CalcDefence.lua:3669).
#[test]
fn m2_max_hit_smoothing_passes_pinned_to_pob2_source() {
    let gc = load();

    assert_eq!(gc.game.max_hit_smoothing_passes, 8.0);
}

///  Locks in the Block panel family's constant — base block chance cap
/// 50% (vendor Data/Misc.lua:147's
/// `object_inherent_base_maximum_block_%_from_ot`, injected as
/// `BaseBlockChanceMax` BASE by CalcSetup.lua:28).
#[test]
fn m2_block_constants_pinned_to_pob2_source() {
    let gc = load();
    assert_eq!(gc.game.base_block_chance_max, 50.0);
    // Data.lua:185 BlockChanceCap = 90 (an existing field, also checked
    // here for the consumer's clamp boundary).
    assert_eq!(gc.game.block_chance_cap, 90.0);
}

/// Checks vendor `data.misc`'s derived convention: an ailment baseline
/// fraction = PercentPerMinute/60/100, and the JSON agrees with vendor's
/// derived value (Misc.lua:86-88 → 900/1200/1200).
#[test]
fn ailment_fractions_consistent_with_vendor_per_minute_form() {
    let gc = load();
    assert_eq!(gc.game.bleed_base_fraction, 900.0 / 60.0 / 100.0);
    assert_eq!(gc.game.ignite_base_fraction, 1200.0 / 60.0 / 100.0);
    assert_eq!(gc.game.poison_base_fraction, 1200.0 / 60.0 / 100.0);
    // Shock's scale: pobr uses the decimal 0.2, vendor's BaseShockMagnitude
    // uses the percent-scale 20.
    assert_eq!(
        gc.game.shock_default_effect,
        gc.game.shock_min_effect / 100.0
    );
}

/// Fallback invariant: `GameConstantsDef::default()` (the fallback
/// constant set used when there's no GameData) is **value-equal to the
/// stored JSON across the whole structure** — guaranteeing the "injected"
/// and "fallback" calc paths produce the same output (the structural lock
/// behind the migration invariant).
#[test]
fn default_fallback_equals_loaded_json_exactly() {
    assert_eq!(load(), GameConstantsDef::default());
}
