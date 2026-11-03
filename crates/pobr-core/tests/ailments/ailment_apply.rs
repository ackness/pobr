//! Integration tests for the non-damaging-ailment application loop:
//! - end-to-end: enemy `ShockVal` 20 → effective DPS ×1.20 (via the DamageTaken chain
//!   consumed by offence's `mode_effective`);
//! - override and Val take the max; Maximum clamp + precision truncation (prec=0 → integer
//!   floor), value by value;
//! - a second application is a no-op once `Condition:Already<X>` is set;
//! - empty-spin compatibility (no source mods → env values unchanged);
//! - Chill ActionSpeed / Bonechill branch / magnitude scaling / incremental Multiplier updates.
//!
//! Formula source: vendor CalcPerform.lua:3076-3180 (line numbers read against 0.18.0).

use pobr_core::calc::ailment_apply::apply_nondamaging_ailments;
use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

/// Standard attack Env: 100-200 physical, 1/s, full hit chance (enemy has no evasion),
/// effective-DPS semantics. Shaped like `enemy_mod_db.rs`'s `attack_input`: baseline DPS = 150.
fn effective_env() -> Env {
    let base = ActorBaseStats {
        life: 100.0,
        hit_min: 100.0,
        hit_max: 200.0,
        action_rate: 1.0,
        ..Default::default()
    };
    let mut env = Env::new(Actor::new(80, base));
    env.cfg = CalcConfig::attack()
        .with_damage_type(DamageType::Physical)
        .with_mode_effective(true);
    env
}

/// Inject an enemy condition flag (the minimal shape equivalent to config `conditionEnemyShocked`).
fn enemy_condition(env: &mut Env, condition: &str) {
    env.enemy
        .mod_db
        .add_mod(Modifier::flag(format!("Condition:{condition}")).with_source("Config"));
}

/// The INC aggregation of a given name in the enemy db, under the given cfg.
fn enemy_inc(env: &Env, name: &str) -> f64 {
    env.enemy
        .mod_db
        .sum(ModType::Inc, &env.cfg, &[ModName::from(name)])
}

/// The BASE aggregation of a given name in the enemy db, under the given cfg.
fn enemy_base(env: &Env, name: &str) -> f64 {
    env.enemy
        .mod_db
        .sum(ModType::Base, &env.cfg, &[ModName::from(name)])
}

fn selected_shock_env() -> Env {
    let mut env = effective_env();
    for condition in ["Shocked", "ShockedConfig"] {
        env.enemy.mod_db.add_mod(
            Modifier::flag(format!("Condition:{condition}"))
                .with_tag(pobr_core::ModTag::condition("Effective", false)),
        );
    }
    env
}

#[test]
fn selected_shock_uses_game_default_and_character_magnitude_without_a_hit_estimate() {
    // Game keyword Shock / non_damaging_ailments.Shock.default: 20%.
    // Power Conduction: "25% increased Magnitude of Shock you inflict".
    for (increased, expected) in [(0.0, 20.0), (25.0, 25.0)] {
        let mut env = selected_shock_env();
        env.cfg.conditions.insert("Shocked".into(), false);
        env.player.mod_db.add_mod(Modifier::number(
            "EnemyShockMagnitude",
            ModType::Inc,
            increased,
        ));
        env.player.mod_db.add_mod(
            Modifier::number("Damage", ModType::Inc, 100.0)
                .with_tag(pobr_core::ModTag::condition("Shocked", false)),
        );
        // The selected enemy state may come from another skill, even if the
        // main skill has Elemental Focus. Do not invent chance or uptime.
        env.player.mod_db.add_mod(Modifier::flag("CannotShock"));
        perform(&mut env).expect("selected shock calculation");
        assert_eq!(
            pobr_core::display_catalog::display_value(&env.player.output, "CurrentShock"),
            expected
        );
        assert!((env.player.output.dps - 150.0 * (1.0 + expected / 100.0)).abs() < 1e-9);
        assert_eq!(env.player.output.shock_effect, 0.0, "no hit-derived shock");
        assert_eq!(enemy_inc(&env, "DamageTaken"), expected);
        assert!(env.cfg.condition("EnemyShocked"));
        assert!(!env.cfg.condition("Shocked"), "the player is not Shocked");
    }
}

#[test]
fn selected_shock_does_not_replace_or_scale_an_explicit_final_magnitude() {
    for value in [0.0, 10.0, 33.7, 120.0] {
        let mut env = selected_shock_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 100.0));
        env.enemy
            .mod_db
            .add_mod(Modifier::number("SelfShockMagnitude", ModType::Inc, 100.0));
        // ShockVal represents a supplied final magnitude, including an ally's
        // shock. Character magnitude cannot change it or establish a floor.
        env.enemy
            .mod_db
            .add_mod(Modifier::number("ShockVal", ModType::Base, value));
        apply_nondamaging_ailments(&mut env);
        assert_eq!(enemy_inc(&env, "DamageTaken"), value.min(100.0).floor());
    }
}

#[test]
fn selected_shock_preserves_guaranteed_sources_and_does_not_stack_duplicates() {
    for (name, expected) in [
        ("ShockBase", 12.0),
        ("ShockMinimum", 25.0),
        ("ShockOverride", 10.0),
    ] {
        let mut env = selected_shock_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 25.0));
        for _ in 0..2 {
            env.player
                .mod_db
                .add_mod(Modifier::number(name, ModType::Base, 10.0));
        }
        apply_nondamaging_ailments(&mut env);
        assert_eq!(
            enemy_inc(&env, "DamageTaken"),
            expected,
            "{name}: strongest Base/Override, summed Minimum, no default floor"
        );
        let count = env.enemy.mod_db.iter_mods().count();
        apply_nondamaging_ailments(&mut env);
        assert_eq!(env.enemy.mod_db.iter_mods().count(), count);
        assert_eq!(enemy_inc(&env, "DamageTaken"), expected);
    }
}

#[test]
fn selected_shock_is_not_inferred_from_recent_events_stacks_or_player_shock() {
    let mut env = effective_env();
    env.cfg.conditions.insert("EnemyShocked".into(), false);
    env.cfg
        .conditions
        .insert("ShockedEnemyRecently".into(), true);
    env.cfg.conditions.insert("Shocked".into(), true);
    env.cfg.multipliers.insert("ShockStack".into(), 2.0);
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockChance", ModType::Base, 100.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 25.0));
    apply_nondamaging_ailments(&mut env);
    assert_eq!(enemy_inc(&env, "DamageTaken"), 0.0);
    assert_eq!(env.enemy.mod_db.iter_mods().count(), 0);

    // Nor does a generic target predicate identify who inflicted it. It
    // needs a selected snapshot or an actual magnitude source.
    enemy_condition(&mut env, "Shocked");
    apply_nondamaging_ailments(&mut env);
    assert_eq!(enemy_inc(&env, "DamageTaken"), 0.0);
}

#[test]
fn selected_shock_respects_effective_mode_and_native_selected_state() {
    let mut panel = selected_shock_env();
    panel.cfg.conditions.insert("EnemyShocked".into(), true);
    panel.cfg.mode_effective = false;
    perform(&mut panel).expect("panel calculation");
    assert!(
        !panel
            .enemy
            .mod_db
            .flag(&panel.cfg, "Condition:AlreadyShocked")
    );
    assert_eq!(panel.player.output.dps, 150.0);
    assert_eq!(enemy_inc(&panel, "DamageTaken"), 0.0);

    let mut native = effective_env();
    native.cfg.conditions.insert("EnemyShocked".into(), true);
    perform(&mut native).expect("native selected state");
    assert_eq!(
        pobr_core::display_catalog::display_value(&native.player.output, "CurrentShock"),
        20.0
    );
    assert_eq!(native.player.output.dps, 180.0);
    let count = native.enemy.mod_db.iter_mods().count();
    apply_nondamaging_ailments(&mut native);
    assert_eq!(native.enemy.mod_db.iter_mods().count(), count);
    assert_eq!(enemy_inc(&native, "DamageTaken"), 20.0);
}

#[test]
fn selected_shock_scales_target_magnitude_and_clamps_each_side_before_multiplying() {
    for (attacker, target, expected) in [(25.0, -20.0, 20.0), (-200.0, -200.0, 0.0)] {
        let mut env = selected_shock_env();
        env.player.mod_db.add_mod(Modifier::number(
            "EnemyShockMagnitude",
            ModType::Inc,
            attacker,
        ));
        env.enemy
            .mod_db
            .add_mod(Modifier::number("SelfShockMagnitude", ModType::Inc, target));
        apply_nondamaging_ailments(&mut env);
        assert_eq!(enemy_inc(&env, "DamageTaken"), expected);
    }
}

// 1. End-to-end: enemy ShockVal 20 -> effective DPS x1.20

#[test]
fn shock_val_20_raises_effective_dps_by_20_percent() {
    // Baseline: no shock.
    let mut base_env = effective_env();
    perform(&mut base_env).expect("baseline perform");
    let base_dps = base_env.player.output.dps;
    assert!(
        (base_dps - 150.0).abs() < 1e-9,
        "baseline 150 average x 1/s x 100% hit chance, got {base_dps}"
    );

    // Shocked: the config shape = enemy `Condition:Shocked` flag + enemy `ShockVal` BASE 20.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0).with_source("Shock"));
    perform(&mut env).expect("shocked perform");

    assert!(
        (env.player.output.dps - base_dps * 1.20).abs() < 1e-9,
        "ShockVal 20 → DamageTaken INC 20 → DPS ×1.20：expect {}, got {}",
        base_dps * 1.20,
        env.player.output.dps
    );
    // Panel semantics are unaffected (the DamageTaken consumer chain is gated by mode_effective).
    let mut panel = effective_env();
    panel.cfg.mode_effective = false;
    enemy_condition(&mut panel, "Shocked");
    panel
        .enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0).with_source("Shock"));
    perform(&mut panel).expect("panel perform");
    assert!(
        (panel.player.output.dps - base_dps).abs() < 1e-9,
        "panel scope is not affected by enemy DamageTaken: expect {base_dps}, got {}",
        panel.player.output.dps
    );
}

// 2. override and Val take the max (CalcPerform.lua:3164's m_max(override, SumVal))

#[test]
fn override_and_val_take_max() {
    // override(30) > Val(20) -> 30.
    let mut env = effective_env();
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockOverride", ModType::Base, 30.0).with_source("Skitterbots"));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 30.0).abs() < 1e-9,
        "override 30 wins"
    );
    // An Override source -> sets Condition:Shocked (vendor :3136-3138) + bridges into cfg.
    assert!(
        env.enemy.mod_db.flag(&env.cfg, "Condition:Shocked"),
        "an Override source sets enemy Condition:Shocked"
    );
    assert!(
        env.cfg.condition("EnemyShocked"),
        "condition bridges to the enemy's config namespace"
    );
    assert!(!env.cfg.condition("Shocked"), "not an incoming Shock");

    // Val(40) > override(30) -> 40.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 40.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockOverride", ModType::Base, 30.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 40.0).abs() < 1e-9,
        "Val 40 wins"
    );
}

// 3. Maximum clamp + precision truncation (prec=0 -> integer floor), value by value

#[test]
fn maximum_clamp_and_precision_floor() {
    // Shock cap = non_damaging_ailments.json Shock.max = 100: 150 -> 100.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 150.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 100.0).abs() < 1e-9,
        "Shock clamps to the data max=100"
    );

    // Chill cap = 50 (cfg.constants.game().chill_max_effect): 80 -> 50.
    let mut env = effective_env();
    enemy_condition(&mut env, "Chilled");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ChillVal", ModType::Base, 80.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "ActionSpeed") - (-50.0)).abs() < 1e-9,
        "Chill clamps to the data max=50, ActionSpeed INC -50"
    );

    // Precision truncation: prec=0 -> floor (33.7 -> 33).
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 33.7));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 33.0).abs() < 1e-9,
        "prec=0 integer floor: 33.7 -> 33"
    );

    // A `<X>Max` BASE mod raises the cap: max = 100 + 20 -> ShockVal 110 -> 110.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 110.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMax", ModType::Base, 20.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 110.0).abs() < 1e-9,
        "ShockMax BASE +20 raises the cap to 120"
    );

    // A `<X>Max` Override directly overrides the cap: override 60 -> ShockVal 110 -> 60.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 110.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMax", ModType::Override, 60.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 60.0).abs() < 1e-9,
        "ShockMax Override=60 takes priority over the data max"
    );
}

// 4. A second application is a no-op once Already is set (guards against double application by minions, :3130/:3168)

#[test]
fn second_application_is_noop_after_already_flag() {
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));

    apply_nondamaging_ailments(&mut env);
    let taken_after_first = enemy_inc(&env, "DamageTaken");
    let mods_after_first = env.enemy.mod_db.iter_mods().count();
    assert!(
        env.enemy.mod_db.flag(&env.cfg, "Condition:AlreadyShocked"),
        "the first application sets Condition:AlreadyShocked"
    );

    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - taken_after_first).abs() < 1e-12,
        "a second application doesn't stack DamageTaken"
    );
    assert_eq!(
        env.enemy.mod_db.iter_mods().count(),
        mods_after_first,
        "a second application writes no new mod"
    );
}

// 5. Empty-spin compatibility: no source mods -> env values unchanged

#[test]
fn empty_spin_leaves_env_unchanged() {
    let mut env = effective_env();
    let player_mods = env.player.mod_db.iter_mods().count();
    let enemy_mods = env.enemy.mod_db.iter_mods().count();
    let conditions = env.cfg.conditions.clone();

    apply_nondamaging_ailments(&mut env);

    assert_eq!(env.player.mod_db.iter_mods().count(), player_mods);
    assert_eq!(env.enemy.mod_db.iter_mods().count(), enemy_mods);
    assert_eq!(
        env.cfg.conditions, conditions,
        "cfg.conditions is left untouched"
    );
}

// 6. Magnitude scaling: Base/Minimum multiply by Enemy<X>Magnitude/AilmentMagnitude, Override doesn't

#[test]
fn magnitude_scales_base_and_minimum_but_not_override() {
    // ShockBase 20 x (1 + 50%) = 30.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockBase", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 50.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 30.0).abs() < 1e-9,
        "ShockBase is affected by EnemyShockMagnitude INC: 20x1.5=30"
    );

    // ShockOverride 20 doesn't multiply by magnitude -> 20.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockOverride", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 50.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 20.0).abs() < 1e-9,
        "an Override source is not multiplied by magnitude"
    );

    // The enemy side's SelfShockMagnitude also participates (:3146): 20 x 1.5(skill) x 1.1(enemy) = 33.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockBase", ModType::Base, 20.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("EnemyShockMagnitude", ModType::Inc, 50.0));
    env.enemy
        .mod_db
        .add_mod(Modifier::number("SelfShockMagnitude", ModType::Inc, 10.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 33.0).abs() < 1e-9,
        "skill x enemy both-side magnitude: 20x1.5x1.1=33"
    );

    // Minimum accumulates (:3148-3150): ShockMinimum 10+15 -> 25, higher than either single value -> 25.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMinimum", ModType::Base, 10.0));
    env.player
        .mod_db
        .add_mod(Modifier::number("ShockMinimum", ModType::Base, 15.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "DamageTaken") - 25.0).abs() < 1e-9,
        "Minimum accumulates to 25 as the floor"
    );
}

// 7. Chill: negative ActionSpeed + the Bonechill branch

#[test]
fn chill_writes_negative_action_speed_and_bonechill() {
    // No Bonechill: only ActionSpeed INC -num.
    let mut env = effective_env();
    enemy_condition(&mut env, "Chilled");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ChillVal", ModType::Base, 25.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "ActionSpeed") - (-25.0)).abs() < 1e-9,
        "Chill → ActionSpeed INC -25"
    );
    assert!(
        enemy_inc(&env, "ColdDamageTaken").abs() < 1e-12,
        "without HasBonechill, ColdDamageTaken is not written"
    );

    // HasBonechill + enemy ChillVal > 0 -> ColdDamageTaken INC num (:3092-3094).
    let mut env = effective_env();
    enemy_condition(&mut env, "Chilled");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ChillVal", ModType::Base, 25.0));
    env.player
        .mod_db
        .add_mod(Modifier::flag("HasBonechill").with_source("Bonechill Support"));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_inc(&env, "ColdDamageTaken") - 25.0).abs() < 1e-9,
        "Bonechill → ColdDamageTaken INC 25"
    );
}

// 8. Incremental updates to Multiplier:ChillEffect/ShockEffect (:3173-3180)

#[test]
fn effect_multiplier_updates_incrementally() {
    // No existing multiplier: top up to Current.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_base(&env, "Multiplier:ShockEffect") - 20.0).abs() < 1e-9,
        "Multiplier:ShockEffect tops up to 20"
    );

    // Existing 5 -> +15 increment, total 20.
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    env.enemy.mod_db.add_mod(Modifier::number(
        "Multiplier:ShockEffect",
        ModType::Base,
        5.0,
    ));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_base(&env, "Multiplier:ShockEffect") - 20.0).abs() < 1e-9,
        "existing 5 + increment 15 = 20"
    );

    // Existing 30 >= Current 20 -> no update (stays at 30).
    let mut env = effective_env();
    enemy_condition(&mut env, "Shocked");
    env.enemy
        .mod_db
        .add_mod(Modifier::number("ShockVal", ModType::Base, 20.0));
    env.enemy.mod_db.add_mod(Modifier::number(
        "Multiplier:ShockEffect",
        ModType::Base,
        30.0,
    ));
    apply_nondamaging_ailments(&mut env);
    assert!(
        (enemy_base(&env, "Multiplier:ShockEffect") - 30.0).abs() < 1e-9,
        "existing 30 >= 20, no update"
    );
}
