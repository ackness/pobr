//! Integration tests: the four-way evade split + the stun system (13-G9 / 13-G12).
//!
//! Every expected value is hand-computed from the PoB2 formulas, with each case
//! annotated with the `vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua` line
//! numbers it's pinned to.

use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::defence::{EvadeSuite, calc_evade_suite};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const EPS: f64 = 1e-9;

fn db_with(mods: Vec<Modifier>) -> ModDb {
    let mut db = ModDb::new();
    db.add_list(mods);
    db
}

/// Default four-way entry point: no enemy HitChance multiplier (=1.0), no CannotBeEvaded.
fn suite(db: &ModDb, evasion: f64, accuracy: f64) -> EvadeSuite {
    calc_evade_suite(db, &CalcConfig::default(), evasion, accuracy, 1.0, false)
}

// The four-way evade split (CalcDefence.lua:1394-1456)

/// Baseline: Evasion=4000, enemyAccuracy=1000.
/// monsterHitChance (:40-46) = round((1 − 0.95×4000/(4000+4000))×100) = round(52.5) = 53.
/// EvadeChance (:1437) = 100 − (53 − 0)×1 = 47; all four types share this value → no
/// split, so the composite equals melee = 47.
#[test]
fn evade_baseline_uniform_47() {
    let s = suite(&ModDb::new(), 4000.0, 1000.0);
    assert!(
        (s.evade_chance - 47.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 47.0).abs() < EPS);
    assert!((s.projectile - 47.0).abs() < EPS);
    assert!((s.spell - 47.0).abs() < EPS);
    assert!((s.spell_projectile - 47.0).abs() < EPS);
}

/// ΣBASE EvadeChance goes inside the parentheses (:1437): +10 BASE → 100 − (53 − 10) = 57.
#[test]
fn evade_base_evade_chance_additive_inside() {
    let db = db_with(vec![Modifier::number("EvadeChance", ModType::Base, 10.0)]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!(
        (s.evade_chance - 57.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
}

/// Each of the four types has its own INC multiplier (:1438): `MeleeEvadeChance` INC
/// 20% only scales melee: melee = 47 × 1.2 = 56.4 (the min 95 / max 0 clamp doesn't
/// trigger); the other three types stay at 47. Since melee now differs from the rest,
/// this splits (:1443-1445), and the composite keeps the standalone formula value 47.
#[test]
fn evade_melee_inc_splits_and_scales_only_melee() {
    let db = db_with(vec![Modifier::number(
        "MeleeEvadeChance",
        ModType::Inc,
        20.0,
    )]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!((s.melee - 56.4).abs() < EPS, "got {}", s.melee);
    assert!((s.projectile - 47.0).abs() < EPS);
    assert!((s.spell - 47.0).abs() < EPS);
    assert!((s.spell_projectile - 47.0).abs() < EPS);
    // Split: the composite falls back to the standalone formula value (not melee).
    assert!(
        (s.evade_chance - 47.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
}

/// The `SpellProjectileEvadeChance` multiplier name set includes ProjectileEvadeChance
/// (:1441): `ProjectileEvadeChance` INC 20% scales both projectile and spell_projectile.
#[test]
fn evade_projectile_inc_applies_to_spell_projectile_too() {
    let db = db_with(vec![Modifier::number(
        "ProjectileEvadeChance",
        ModType::Inc,
        20.0,
    )]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!((s.projectile - 56.4).abs() < EPS);
    assert!((s.spell_projectile - 56.4).abs() < EPS);
    assert!((s.spell - 47.0).abs() < EPS);
    // melee(47) == spell(47) → no split (the :1443 condition requires all four to differ) → composite = melee.
    assert!((s.evade_chance - 47.0).abs() < EPS);
}

/// `MeleeEvasion` changes the effective evasion value fed into the melee slot
/// (:1394-1397): INC 100% → MeleeEvasion = 8000; mhc = round((1 − 0.95×8000/(8000+4000))×100)
/// = round(36.666…) = 37 → melee = 100 − 37 = 63; the other three stay at 47.
#[test]
fn evade_melee_evasion_rating_scales_hit_chance_input() {
    let db = db_with(vec![Modifier::number("MeleeEvasion", ModType::Inc, 100.0)]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!((s.melee - 63.0).abs() < EPS, "got {}", s.melee);
    assert!((s.projectile - 47.0).abs() < EPS);
}

/// Cap at 95 (:1436/:1449, game_constants `evade_chance_cap`):
/// Evasion=100000, acc=1000 → mhc = round((1 − 0.95×100000/104000)×100) = round(8.65…) = 9;
/// +10 BASE → 100 − (9 − 10) = 101 → min(95). Each of the four types applies the same
/// min(evadeMax, …) clamp.
#[test]
fn evade_capped_at_95() {
    let db = db_with(vec![Modifier::number("EvadeChance", ModType::Base, 10.0)]);
    let s = suite(&db, 100000.0, 1000.0);
    assert!(
        (s.evade_chance - 95.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 95.0).abs() < EPS);
}

/// `EvadeChanceMax` Override tightens the upper bound (:1436; W0.1 vendor MAX → Override).
#[test]
fn evade_max_override_lowers_cap() {
    let db = db_with(vec![Modifier::number(
        "EvadeChanceMax",
        ModType::Override,
        30.0,
    )]);
    let s = suite(&db, 4000.0, 1000.0);
    // Before clamping this would be 47 → the Override forces it to 30.
    assert!(
        (s.evade_chance - 30.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 30.0).abs() < EPS);
}

/// `CannotEvade` → all four types zero (:1421-1426).
#[test]
fn evade_cannot_evade_zeroes_all() {
    let db = db_with(vec![Modifier::flag("CannotEvade")]);
    let s = suite(&db, 4000.0, 1000.0);
    assert_eq!(s, EvadeSuite::default());
}

/// The enemy's `CannotBeEvaded` flag (:1421) → all four types zero.
#[test]
fn evade_enemy_cannot_be_evaded_zeroes_all() {
    let s = calc_evade_suite(
        &ModDb::new(),
        &CalcConfig::default(),
        4000.0,
        1000.0,
        1.0,
        true,
    );
    assert_eq!(s, EvadeSuite::default());
}

/// `AlwaysEvade` ("Attacks cannot Hit you") → all four types at 100 (:1427-1433).
#[test]
fn evade_always_evade_all_100() {
    let db = db_with(vec![Modifier::flag("AlwaysEvade")]);
    let s = suite(&db, 0.0, 1000.0);
    assert!((s.evade_chance - 100.0).abs() < EPS);
    assert!((s.melee - 100.0).abs() < EPS);
    assert!((s.spell_projectile - 100.0).abs() < EPS);
}

/// `UnluckyEvade` → each value becomes x²/100 (:1450-1456): 47 → 22.09.
#[test]
fn evade_unlucky_squares_each_value() {
    let db = db_with(vec![Modifier::flag("UnluckyEvade")]);
    let s = suite(&db, 4000.0, 1000.0);
    assert!(
        (s.evade_chance - 22.09).abs() < EPS,
        "got {}",
        s.evade_chance
    );
    assert!((s.melee - 22.09).abs() < EPS);
}

/// The enemy's HitChance multiplier enters the formula (:1437 `× hitChance`):
/// enemy +100% HitChance INC → mult 2.0 → 100 − 53×2 = −6; each of the four types
/// clamps at 0 (:1438 `m_max(0, …)`); no split occurs, so composite = melee = 0.
#[test]
fn evade_enemy_hit_chance_mult_can_floor_types_to_zero() {
    let s = calc_evade_suite(
        &ModDb::new(),
        &CalcConfig::default(),
        4000.0,
        1000.0,
        2.0,
        false,
    );
    assert!((s.melee - 0.0).abs() < EPS, "got {}", s.melee);
    assert!((s.evade_chance - 0.0).abs() < EPS);
}

/// Zero evasion on a bare panel: monsterHitChance(0, acc) = 100 (:44, numerator is 0) → everything zero.
#[test]
fn evade_zero_evasion_is_zero() {
    let s = suite(&ModDb::new(), 0.0, 1000.0);
    assert_eq!(s, EvadeSuite::default());
}

/// Enemy accuracy 0 with nonzero evasion: mhc = round((1 − 0.95)×100) = 5 (the floor clamp) → evade = 95.
#[test]
fn evade_zero_enemy_accuracy_hits_floor_5() {
    let s = suite(&ModDb::new(), 4000.0, 0.0);
    assert!(
        (s.evade_chance - 95.0).abs() < EPS,
        "got {}",
        s.evade_chance
    );
}

/// End-to-end via perform: fill_evade_stun writes the four-way split into OutputTable
/// (a single call in perform.rs). Player Evasion=4000 (fed as base), enemy accuracy 1000
/// → all four at 47.
#[test]
fn perform_fills_evade_suite_into_output() {
    let base = ActorBaseStats {
        life: 1000.0,
        evasion: 4000.0,
        ..ActorBaseStats::default()
    };
    let mut env = Env::new(Actor::new(1, base));
    env.enemy.base.accuracy = 1000.0;
    perform(&mut env).unwrap();

    assert!((env.player.output.evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.melee_evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.projectile_evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.spell_evade_chance - 47.0).abs() < EPS);
    assert!((env.player.output.spell_projectile_evade_chance - 47.0).abs() < EPS);
}

// The stun system (CalcDefence.lua:2525-2643) + the avoid_stun ES condition fix (:2554-2557)

use pobr_core::calc::defence::calc_avoidance;
use pobr_core::calc::stun::{StunInputs, calc_stun, calc_stun_threshold};

fn stun_inputs(life: f64) -> StunInputs {
    StunInputs {
        life,
        ..StunInputs::default()
    }
}

/// The threshold defaults to output.Life (:2542-2543): life 1000 → 1000.
#[test]
fn stun_threshold_defaults_to_life() {
    let t = calc_stun_threshold(&ModDb::new(), &CalcConfig::default(), &stun_inputs(1000.0));
    assert!((t - 1000.0).abs() < EPS, "got {t}");
}

/// Switching to an ES-based threshold (:2529-2532): flag + StunThresholdEnergyShieldPercent
/// 50, ES 2000 → 2000×50/100 = 1000 (life is ignored).
#[test]
fn stun_threshold_es_based() {
    let db = db_with(vec![
        Modifier::flag("StunThresholdBasedOnEnergyShieldInsteadOfLife"),
        Modifier::number("StunThresholdEnergyShieldPercent", ModType::Base, 50.0),
    ]);
    let inp = StunInputs {
        life: 5000.0,
        energy_shield: 2000.0,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 1000.0).abs() < EPS, "got {t}");
}

/// Switching to a Mana-based threshold (:2533-2536): flag + StunThresholdManaPercent 100, mana 800 → 800.
#[test]
fn stun_threshold_mana_based() {
    let db = db_with(vec![
        Modifier::flag("StunThresholdBasedOnManaInsteadOfLife"),
        Modifier::number("StunThresholdManaPercent", ModType::Base, 100.0),
    ]);
    let inp = StunInputs {
        life: 5000.0,
        mana: 800.0,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 800.0).abs() < EPS, "got {t}");
}

/// The Chaos Inoculation branch (:2537-2539): the threshold uses "pre-CI Life" (the
/// flat base plus ΣBASE MaximumLife), not the post-CI output.Life(=1).
#[test]
fn stun_threshold_ci_uses_pre_ci_life() {
    let db = db_with(vec![Modifier::number("MaximumLife", ModType::Base, 900.0)]);
    let inp = StunInputs {
        life: 1.0, // the post-CI pool
        life_base_flat: 100.0,
        chaos_inoculation: true,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 1000.0).abs() < EPS, "got {t}");
}

/// AddESToStunThreshold (:2544-2548): life 1000 + ES 2000×50% → 2000.
#[test]
fn stun_threshold_add_es() {
    let db = db_with(vec![
        Modifier::flag("AddESToStunThreshold"),
        Modifier::number("ESToStunThresholdPercent", ModType::Base, 50.0),
    ]);
    let inp = StunInputs {
        life: 1000.0,
        energy_shield: 2000.0,
        ..StunInputs::default()
    };
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &inp);
    assert!((t - 2000.0).abs() < EPS, "got {t}");
}

/// BASE/INC/MORE aggregation (:2549-2552): (1000 + 200) × 1.3 × 2 (a MORE 100 "doubles the
/// threshold" mod) = 3120.
#[test]
fn stun_threshold_aggregation() {
    let db = db_with(vec![
        Modifier::number("StunThreshold", ModType::Base, 200.0),
        Modifier::number("StunThreshold", ModType::Inc, 30.0),
        Modifier::number("StunThreshold", ModType::More, 100.0),
    ]);
    let t = calc_stun_threshold(&db, &CalcConfig::default(), &stun_inputs(1000.0));
    assert!((t - 3120.0).abs() < EPS, "got {t}");
}

/// Duration rounds up to whole server ticks (:2594): base 0.5s / tick 0.033 = 15.15… → ceil 16 ticks → 0.528s.
#[test]
fn stun_duration_rounds_up_to_server_tick() {
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &stun_inputs(1000.0));
    assert!(
        (r.stun_duration - 0.528).abs() < EPS,
        "got {}",
        r.stun_duration
    );
}

/// Duration INC (:2591/:2594): StunDuration +100% → 1.0/0.033 = 30.3 → 31 ticks → 1.023s.
#[test]
fn stun_duration_scales_with_inc() {
    let db = db_with(vec![Modifier::number("StunDuration", ModType::Inc, 100.0)]);
    let r = calc_stun(&db, &CalcConfig::default(), &stun_inputs(1000.0));
    assert!(
        (r.stun_duration - 1.023).abs() < EPS,
        "got {}",
        r.stun_duration
    );
}

/// Recovery shortens the duration (:2593-2594, the denominator): StunRecovery +100% → 0.25/0.033 = 7.57 → 8 ticks → 0.264s.
#[test]
fn stun_duration_shortened_by_recovery() {
    let db = db_with(vec![Modifier::number("StunRecovery", ModType::Inc, 100.0)]);
    let r = calc_stun(&db, &CalcConfig::default(), &stun_inputs(1000.0));
    assert!(
        (r.stun_duration - 0.264).abs() < EPS,
        "got {}",
        r.stun_duration
    );
}

/// Avoidance ≥100% (e.g. StunImmune) → duration 0, chance 0 (:2584-2586).
#[test]
fn stun_avoid_100_zeroes_duration_and_chance() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 1000.0,
        physical_taken_hit: 1000.0,
        avoid_stun: 100.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert_eq!(r.stun_duration, 0.0);
    assert_eq!(r.self_stun_chance, 0.0);
}

/// Chance to be stunned (:2617-2624): taken 1000 / phys 1000 / threshold 1000 →
/// eff = (1000 + 1000×0.25) × 1.0 = 1250; base = min(200×1250/1000, 100) = 100 > 20
/// → SelfStunChance = 100 × (100−0)/100 = 100.
#[test]
fn stun_chance_full_hit_is_100() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 1000.0,
        physical_taken_hit: 1000.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert!(
        (r.self_stun_chance - 100.0).abs() < EPS,
        "got {}",
        r.self_stun_chance
    );
}

/// The MinStunChanceNeeded(20) threshold (:2624): taken 50 / phys 50 → eff 62.5 →
/// base = 200×62.5/1000 = 12.5 < 20 → falls to 0.
#[test]
fn stun_chance_below_min_needed_is_zero() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 50.0,
        physical_taken_hit: 50.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert_eq!(r.self_stun_chance, 0.0);
}

/// Chance is scaled by (1 − avoidance) (:2624): avoid_stun 50 → 100 × 50/100 = 50.
#[test]
fn stun_chance_scaled_by_not_avoid() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 1000.0,
        physical_taken_hit: 1000.0,
        avoid_stun: 50.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert!(
        (r.self_stun_chance - 50.0).abs() < EPS,
        "got {}",
        r.self_stun_chance
    );
}

/// Physical damage is weighted ×0.25 (:2617): taken 100 (all physical) → eff = 125 →
/// base = 200×125/1000 = 25 > 20 → 25.
#[test]
fn stun_chance_physical_weighted_quarter() {
    let inp = StunInputs {
        life: 1000.0,
        total_taken_hit: 100.0,
        physical_taken_hit: 100.0,
        ..StunInputs::default()
    };
    let r = calc_stun(&ModDb::new(), &CalcConfig::default(), &inp);
    assert!(
        (r.self_stun_chance - 25.0).abs() < EPS,
        "got {}",
        r.self_stun_chance
    );
}

// The avoid_stun ES condition fix (CalcDefence.lua:2554-2557)

/// ES > totalTakenHit and not EB → notAvoid ×0.5 → an implicit 50% avoidance.
#[test]
fn avoid_stun_es_above_taken_hit_halves() {
    let r = calc_avoidance(&ModDb::new(), &CalcConfig::default(), 2000.0, 1000.0, false);
    assert_eq!(r.avoid_stun, 50.0);
}

/// ES ≤ totalTakenHit → no halving (this is the fix for the old "ES > 0 always halves" behaviour).
#[test]
fn avoid_stun_es_below_taken_hit_no_halving() {
    let r = calc_avoidance(&ModDb::new(), &CalcConfig::default(), 500.0, 1000.0, false);
    assert_eq!(r.avoid_stun, 0.0);
}

/// EB (EnergyShieldProtectsMana) → no halving even when ES > takenHit (:2555).
#[test]
fn avoid_stun_eb_disables_es_halving() {
    let r = calc_avoidance(&ModDb::new(), &CalcConfig::default(), 2000.0, 1000.0, true);
    assert_eq!(r.avoid_stun, 0.0);
}

/// End-to-end via perform: the three stun output fields (totalTakenHit currently uses
/// the real pool-deduction pipeline value, per Track E "swap in real values once F is
/// wired").
///
/// A bare Env (no setup_enemy → zero incoming damage): threshold stays at 1000; zero
/// effective stun damage → SelfStunChance 0 (neutral); duration still follows the
/// given-stunned formula, 0.528s (:2584-2595).
#[test]
fn perform_fills_stun_into_output() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = Env::new(Actor::new(1, base));
    perform(&mut env).unwrap();

    assert!((env.player.output.stun_threshold - 1000.0).abs() < EPS);
    assert!((env.player.output.self_stun_chance - 0.0).abs() < EPS);
    assert!((env.player.output.stun_duration - 0.528).abs() < EPS);
}

/// End-to-end via perform (with an enemy): a real totalTakenHit drives SelfStunChance.
/// life 1000, no mitigation: a single Pinnacle@82 hit deals 4246 total / 965 physical →
/// eff = 4246 + 965×0.25 = 4487.25 → base = min(200×4487.25/1000, 100) = 100
/// (:2617/:2623); avoid 0 → SelfStunChance 100.
#[test]
fn perform_fills_stun_with_enemy_taken_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = Env::new(Actor::new(1, base));
    pobr_core::calc::setup_enemy(&mut env, 82, pobr_data::monster::EnemyTier::Pinnacle);
    perform(&mut env).unwrap();

    assert!((env.player.output.stun_threshold - 1000.0).abs() < EPS);
    assert!((env.player.output.self_stun_chance - 100.0).abs() < EPS);
    assert!((env.player.output.stun_duration - 0.528).abs() < EPS);
}

// Resistance floor at −200 (`CalcDefence.lua:886`/`:919`, `Data.lua:180`)

use pobr_core::calc::{MinimalInput, calculate_minimal};

/// Deeply negative resist hits the floor: base −300 → final = max(min(−300, 75), −200) = −200.
#[test]
fn resistance_floored_at_minus_200() {
    let input = MinimalInput {
        base_life: 100.0,
        base_fire_resistance: -300.0,
        ..MinimalInput::default()
    };
    let out = calculate_minimal(&ModDb::new(), &CalcConfig::default(), &input);
    assert_eq!(out.fire_resistance, -200.0);
}

/// A negative resist within the floor is unaffected: base −60 → final = −60 (the fix leaves the normal path unchanged).
#[test]
fn resistance_above_floor_unchanged() {
    let input = MinimalInput {
        base_life: 100.0,
        base_cold_resistance: -60.0,
        ..MinimalInput::default()
    };
    let out = calculate_minimal(&ModDb::new(), &CalcConfig::default(), &input);
    assert_eq!(out.cold_resistance, -60.0);
}
