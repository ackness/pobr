//! Fixed values for a spell in a Cast on Critical, from a build code through
//! the production parser to the result.
//!
//! The fixture (`fixtures/coc_cast_on_crit.xml`) is a level 80 Sorceress with
//! a Wooden Club and one skill group: Armour Breaker, Cast on Critical and
//! Fireball, the Fireball selected. The game would not socket an attack in a
//! meta gem; a build code can list one there, and it is then a skill the
//! character uses itself, whose critical hits fill the gem.
//!
//! The rules and where they come from are in `pobr_core::calc::energy`, and
//! `skills::energy_triggers` tests them against the game's own numbers. This
//! suite pins what one build gives, so that a change to any step between the
//! build code and the trigger rate shows: the source skill's own numbers,
//! what the gem holds, what a critical hit gives it, and the rate. The
//! numbers are small because the build is: a level 10 attack with a Wooden
//! Club against a Pinnacle boss needs thousands of critical hits per Fireball.
//!
//! When a deliberate change moves a constant, update it to the value in the
//! failure message (the convention of `golden_regression.rs`).

use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::GameData;

const COC_XML: &str = include_str!("../fixtures/coc_cast_on_crit.xml");

/// Fireball's base cast time is 1.2 seconds: 10 Energy per 0.1 seconds.
const GOLDEN_MAXIMUM_ENERGY: f64 = 120.0;
/// What one critical hit of Armour Breaker gives the gem: 15.84 damage of the
/// boss's Ailment Threshold of 71303, times its Power of 20, with the 27%
/// increased Energy of a level 10 gem.
const GOLDEN_ENERGY_PER_CRITICAL_HIT: f64 = 0.005642624;
/// Fireballs per second: Armour Breaker's critical hits per second over the
/// whole number of them that fills the gem.
const GOLDEN_SKILL_TRIGGER_RATE: f64 = 0.000002024;
/// With sufficient Energy per event, each critical hit can trigger once:
/// 1.15942029 uses/s × 0.742558559 hit chance × 0.05 crit chance.
const GOLDEN_TRIGGER_RATE_CAP: f64 = 0.043046873;
/// The Fireball's average hit at the rate above.
const GOLDEN_DPS: f64 = 0.000159187;
/// The Fireball's own numbers.
const GOLDEN_MAIN_CRIT_CHANCE: f64 = 0.07;
const GOLDEN_MAIN_HIT_CHANCE: f64 = 1.0;
const GOLDEN_MAIN_TOTAL_HIT_AVG: f64 = 78.645;

/// The Fireball cast by hand: 1.2 seconds a cast.
const GOLDEN_OWN_ACTION_RATE: f64 = 0.833333333;
const GOLDEN_OWN_DPS: f64 = 65.537499974;

/// Armour Breaker with the Wooden Club, as when it is the selected skill.
const GOLDEN_SRC_EFFECTIVE_ACTION_RATE: f64 = 1.15942029;
const GOLDEN_SRC_HIT_CHANCE: f64 = 0.742558559;
const GOLDEN_SRC_CRIT_CHANCE: f64 = 0.05;

/// Relative tolerance for float fields (same value as golden_regression.rs).
const RELATIVE_TOL: f64 = 1e-6;

fn assert_near_float(label: &str, expected: f64, actual: f64) {
    if expected == 0.0 {
        assert!(
            actual.abs() < RELATIVE_TOL,
            "{label}: expected 0, got {actual}"
        );
        return;
    }
    let relative = ((actual - expected) / expected).abs();
    assert!(
        relative < RELATIVE_TOL,
        "{label}: expected {expected}, got {actual}, relative error {relative} exceeds {RELATIVE_TOL}"
    );
}

fn load_build_data() -> BuildData {
    let data = GameData::new(pobr_gamedata::current_data_dir());
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn parse_coc_build() -> Build {
    parse_build(COC_XML).expect("parse coc fixture")
}

/// The Fireball: what the gem holds and gains, and how often it is triggered.
#[test]
fn coc_golden_triggered_skill_output() {
    let data = load_build_data();
    let build = parse_coc_build();
    let out = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
        .expect("coc calculate");

    assert_eq!(
        out.energy_trigger_source.as_deref(),
        Some("ArmourBreakerPlayer")
    );
    assert_near_float(
        "energy_trigger_maximum",
        GOLDEN_MAXIMUM_ENERGY,
        out.energy_trigger_maximum,
    );
    assert_near_float(
        "energy_per_trigger_event",
        GOLDEN_ENERGY_PER_CRITICAL_HIT,
        out.energy_per_trigger_event,
    );
    assert_near_float(
        "skill_trigger_rate",
        GOLDEN_SKILL_TRIGGER_RATE,
        out.skill_trigger_rate,
    );
    assert_near_float(
        "effective_action_rate",
        GOLDEN_SKILL_TRIGGER_RATE,
        out.effective_action_rate,
    );
    assert_near_float(
        "trigger_rate_cap",
        GOLDEN_TRIGGER_RATE_CAP,
        out.trigger_rate_cap,
    );
    assert_near_float("dps", GOLDEN_DPS, out.dps);
    assert_near_float("crit_chance", GOLDEN_MAIN_CRIT_CHANCE, out.crit_chance);
    assert_near_float("hit_chance", GOLDEN_MAIN_HIT_CHANCE, out.hit_chance);
    assert_near_float(
        "total_hit_avg",
        GOLDEN_MAIN_TOTAL_HIT_AVG,
        out.total_hit_avg,
    );
}

/// The skill that fills the gem, selected as the main skill: the numbers the
/// calculation of the Fireball takes from it.
#[test]
fn coc_golden_trigger_source_stats() {
    let data = load_build_data();
    let mut build = parse_coc_build();
    build.socket_groups[0].main_active_skill = Some(1);
    let out = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
        .expect("source calculate");

    assert_near_float(
        "src effective_action_rate",
        GOLDEN_SRC_EFFECTIVE_ACTION_RATE,
        out.effective_action_rate,
    );
    assert_near_float("src hit_chance", GOLDEN_SRC_HIT_CHANCE, out.hit_chance);
    assert_near_float("src crit_chance", GOLDEN_SRC_CRIT_CHANCE, out.crit_chance);
}

/// The constants above hold together: the rate is the source's critical hits
/// per second over the whole number of them that fills the gem.
#[test]
fn coc_trigger_rate_is_critical_hits_over_hits_to_fill() {
    let critical_hits_per_second =
        GOLDEN_SRC_EFFECTIVE_ACTION_RATE * GOLDEN_SRC_HIT_CHANCE * GOLDEN_SRC_CRIT_CHANCE;
    let hits_to_fill = (GOLDEN_MAXIMUM_ENERGY / GOLDEN_ENERGY_PER_CRITICAL_HIT).ceil();
    let expected = critical_hits_per_second / hits_to_fill;
    // The constants are themselves rounded to nine decimal places.
    assert!(
        (expected - GOLDEN_SKILL_TRIGGER_RATE).abs() < 1e-9,
        "{expected} != golden {GOLDEN_SKILL_TRIGGER_RATE}"
    );
}

/// Off, the gem is left out and the Fireball is cast at its own rate.
#[test]
fn coc_golden_spell_at_its_own_rate() {
    let data = load_build_data();
    let build = parse_coc_build();
    let options = DataOrchestratorOptions {
        energy_triggers: false,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &options).expect("coc calculate");
    assert_eq!(out.skill_trigger_rate, 0.0);
    assert_near_float("action_rate", GOLDEN_OWN_ACTION_RATE, out.action_rate);
    assert_near_float("dps", GOLDEN_OWN_DPS, out.dps);
}

/// Two calculations give the same result: the calculation of the source skill
/// brings no order dependence.
#[test]
fn coc_golden_is_deterministic() {
    let data = load_build_data();
    let build = parse_coc_build();
    let opts = DataOrchestratorOptions::default();
    let out1 = calculate_with_data(&build, &data, &opts).expect("first calc");
    let out2 = calculate_with_data(&build, &data, &opts).expect("second calc");
    assert_eq!(out1, out2);
}
