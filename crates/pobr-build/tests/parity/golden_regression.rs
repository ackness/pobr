//! Empty-build calculation and codec regression checks, not numeric PoB parity.
//!
//! Real build codes supply character identity only. One calculation checks the
//! neutral output fields; separate cases cover injected modifiers and determinism.
//! Gear/passive/skill parity is checked by the real-build suites.

use pobr_build::{
    Build, CharacterIdentity, OrchestratorOptions, calculate, decode_pob_code, parse_build_header,
};
use pobr_core::calc::MinimalInput;

/// A real PoB2 ninja Deadeye build code.
const DEADEYE_CODE: &str = include_str!("../../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

/// A real PoB2 ninja Martial Artist build code.
const MARTIAL_ARTIST_CODE: &str =
    include_str!("../../../../examples/demo-bd-test/ninja-bd-marial-artist.txt");

// Tolerance helpers

/// Integer field tolerance: |delta| < 0.5 (allows rounding deviation).
const INTEGER_TOL: f64 = 0.5;
/// Float field relative tolerance: 1e-6 (one part per million).
const RELATIVE_TOL: f64 = 1e-6;

fn assert_near_int(label: &str, expected: f64, actual: f64) {
    let delta = (actual - expected).abs();
    assert!(
        delta < INTEGER_TOL,
        "{label}: expected {expected}, got {actual}, delta {delta} exceeds integer tolerance {INTEGER_TOL}"
    );
}

fn assert_near_float(label: &str, expected: f64, actual: f64) {
    // If the expected value is 0, fall back to the integer tolerance.
    if expected.abs() < f64::EPSILON {
        let delta = actual.abs();
        assert!(
            delta < INTEGER_TOL,
            "{label}: expected ~0, got {actual}, exceeds tolerance"
        );
        return;
    }
    let relative = (actual - expected).abs() / expected.abs();
    assert!(
        relative < RELATIVE_TOL,
        "{label}: expected {expected:.6}, got {actual:.6}, relative error {relative:.2e} exceeds {RELATIVE_TOL:.2e}"
    );
}

// Constructs a Build from a build code (minimal path)

fn build_from_code(code: &str) -> Build {
    let xml = decode_pob_code(code.trim()).expect("decode build code");
    let header = parse_build_header(&xml).expect("parse build header");

    Build::new().with_character(CharacterIdentity {
        level: header.identity.level,
        class_name: header.identity.class_name.clone(),
        ascendancy_name: header.identity.ascendancy_name.clone(),
    })
}

fn default_opts() -> OrchestratorOptions {
    OrchestratorOptions {
        base_input: MinimalInput::default(),
        extra_modifier_texts: vec![],
    }
}

/// Expected character level after decoding the Deadeye build (from XML `<Build level="...">`).
const DEADEYE_EXPECTED_LEVEL: u32 = 98;

/// Expected class name after decoding the Deadeye build.
const DEADEYE_EXPECTED_CLASS: &str = "Ranger";

/// Empty Build (no gear) + empty MinimalInput -> neutral defaults: all zero / default defence.
const DEADEYE_NEUTRAL_LIFE: f64 = 0.0;
const DEADEYE_NEUTRAL_MANA: f64 = 0.0;
const DEADEYE_NEUTRAL_FIRE_RES: f64 = 0.0;
const DEADEYE_NEUTRAL_COLD_RES: f64 = 0.0;
const DEADEYE_NEUTRAL_LIGHTNING_RES: f64 = 0.0;
const DEADEYE_NEUTRAL_DPS: f64 = 0.0;

// Tests: Deadeye build end-to-end golden

#[test]
fn deadeye_golden_decode_and_identity() {
    // Stage 1: build code -> XML -> header parsing.
    let xml = decode_pob_code(DEADEYE_CODE.trim()).expect("decode");
    let header = parse_build_header(&xml).expect("parse header");

    assert_eq!(
        header.identity.level, DEADEYE_EXPECTED_LEVEL,
        "Deadeye level mismatch: XML level changed or wrong fixture"
    );
    assert_eq!(
        header.identity.class_name, DEADEYE_EXPECTED_CLASS,
        "Deadeye class mismatch"
    );
    assert!(
        header.identity.ascendancy_name.contains("Deadeye"),
        "expected Deadeye ascendancy, got: {}",
        header.identity.ascendancy_name
    );
}

#[test]
fn deadeye_golden_calc_baseline() {
    // Stage 2: Build construction + CalcOrchestrator -> OutputTable baseline.
    // No gear / no extra modifiers, so output should equal the empty MinimalInput defaults.
    let build = build_from_code(DEADEYE_CODE);
    let opts = default_opts();
    let out = calculate(&build, &opts).expect("calculate");

    assert_near_int("life", DEADEYE_NEUTRAL_LIFE, out.life);
    assert_near_int("mana", DEADEYE_NEUTRAL_MANA, out.mana);
    assert_near_float(
        "fire_resistance",
        DEADEYE_NEUTRAL_FIRE_RES,
        out.fire_resistance,
    );
    assert_near_float(
        "cold_resistance",
        DEADEYE_NEUTRAL_COLD_RES,
        out.cold_resistance,
    );
    assert_near_float(
        "lightning_resistance",
        DEADEYE_NEUTRAL_LIGHTNING_RES,
        out.lightning_resistance,
    );
    assert_near_float("dps", DEADEYE_NEUTRAL_DPS, out.dps);

    // Crit chance: no base-crit mod -> 0.0.
    assert_near_float("crit_chance", 0.0, out.crit_chance);
    // Crit multiplier: PoE2's base value 2.0 (PLAYER_BASE_CRIT_DAMAGE_BONUS = 100%).
    assert_near_float(
        "crit_multiplier",
        DEADEYE_NEUTRAL_CRIT_MULTIPLIER,
        out.crit_multiplier,
    );
    // Pre-effective-mode crit chance: no base -> 0.0.
    assert_near_float(
        "pre_effective_crit_chance",
        0.0,
        out.pre_effective_crit_chance,
    );

    // ES recharge: no mods -> recharge rate 0, delay 0, per-second 0.
    assert_near_float("es_recharge_rate", 0.0, out.es_recharge_rate);
    assert_near_float("es_recharge_delay", 0.0, out.es_recharge_delay);
    assert_near_float("es_recharge_per_second", 0.0, out.es_recharge_per_second);

    // Avoidance: no mods -> all 0.
    assert_near_float(
        "avoid_all_damage_from_hits",
        0.0,
        out.avoid_all_damage_from_hits,
    );
    assert_near_float("avoid_ignite", 0.0, out.avoid_ignite);
    assert_near_float("avoid_shock", 0.0, out.avoid_shock);
    assert_near_float("avoid_chill", 0.0, out.avoid_chill);
    assert_near_float("avoid_freeze", 0.0, out.avoid_freeze);
    assert_near_float("avoid_poison", 0.0, out.avoid_poison);
    assert_near_float("avoid_bleeding", 0.0, out.avoid_bleeding);
    assert_near_float("avoid_stun", 0.0, out.avoid_stun);

    // Damage-taken multipliers are neutral: default 1.0.
    assert_near_float("taken_multi_physical", 1.0, out.taken_multi_physical);
    assert_near_float("taken_multi_fire", 1.0, out.taken_multi_fire);
    assert_near_float("taken_multi_cold", 1.0, out.taken_multi_cold);
    assert_near_float("taken_multi_lightning", 1.0, out.taken_multi_lightning);
    assert_near_float("taken_multi_chaos", 1.0, out.taken_multi_chaos);

    // Extra crit damage reduction / enemy crit effect (neutral).
    assert_near_float(
        "crit_extra_damage_reduction",
        0.0,
        out.crit_extra_damage_reduction,
    );
    assert_near_float("enemy_crit_effect", 1.0, out.enemy_crit_effect);

    // Charges: no mods -> current/maximum both 0.
    assert_eq!(out.charge_power_current, 0.0, "charge_power_current");
    assert_eq!(out.charge_power_maximum, 0, "charge_power_maximum");
    assert_eq!(out.charge_frenzy_current, 0.0, "charge_frenzy_current");
    assert_eq!(out.charge_frenzy_maximum, 0, "charge_frenzy_maximum");
    assert_eq!(
        out.charge_endurance_current, 0.0,
        "charge_endurance_current"
    );
    assert_eq!(out.charge_endurance_maximum, 0, "charge_endurance_maximum");

    // Leech rates: no mods -> 0.
    assert_near_float("life_leech_rate", 0.0, out.life_leech_rate);
    assert_near_float("mana_leech_rate", 0.0, out.mana_leech_rate);
    assert_near_float("es_leech_rate", 0.0, out.es_leech_rate);

    // Recoup: no mods -> 0.
    assert_near_float("life_recoup_rate", 0.0, out.life_recoup_rate);
    assert_near_float("es_recoup_rate", 0.0, out.es_recoup_rate);

    assert_near_float("chill_effect", 0.0, out.chill_effect);
    assert_near_float("freeze_buildup_pct", 0.0, out.freeze_buildup_pct);
    assert_near_float("electrocute_buildup_pct", 0.0, out.electrocute_buildup_pct);

    // Stacked DPS / active stacks: no hit-producing mods -> 0.
    assert_near_float("bleed_stacked_dps", 0.0, out.bleed_stacked_dps);
    assert_near_float("bleed_active_stacks", 0.0, out.bleed_active_stacks);
    assert_near_float("poison_stacked_dps", 0.0, out.poison_stacked_dps);
    assert_near_float("poison_active_stacks", 0.0, out.poison_active_stacks);

    // Skill mechanics: no base mods -> 0.
    assert_near_float("aoe_radius", 0.0, out.aoe_radius);
    // Area scaling is a multiplier, neutral at 1 even without a base radius.
    assert_near_float("aoe_area_mod", 1.0, out.aoe_area_mod);
    assert_near_float("projectile_count", 0.0, out.projectile_count);
    assert_near_float("cooldown", 0.0, out.cooldown);
    assert_eq!(out.cooldown_stored_uses, 0, "cooldown_stored_uses");
    assert_near_float("mana_cost", 0.0, out.mana_cost);
    assert_near_float("life_cost", 0.0, out.life_cost);
    assert_near_float("spirit_reserved", 0.0, out.spirit_reserved);

    // Trigger: no trigger mods -> 0.
    assert_near_float("trigger_rate_cap", 0.0, out.trigger_rate_cap);
    assert_near_float("skill_trigger_rate", 0.0, out.skill_trigger_rate);

    assert_near_float("bleed_dps", 0.0, out.bleed_dps);
    assert_near_float("ignite_dps", 0.0, out.ignite_dps);
    assert_near_float("poison_dps", 0.0, out.poison_dps);
    assert_near_float("shock_effect", 0.0, out.shock_effect);

    assert_near_float("action_rate", 0.0, out.action_rate);
    assert_near_float("effective_action_rate", 0.0, out.effective_action_rate);
    // hit_chance: base_accuracy=0 + enemy_evasion=0 -> formula returns 1.0 (100%).
    assert_near_float("hit_chance", DEADEYE_NEUTRAL_HIT_CHANCE, out.hit_chance);
}

#[test]
fn deadeye_golden_calc_with_life_modifier() {
    // Stage 3: injects a known mod, verifying its effect is correctly folded into the calculation.
    // Injects "+1000 to maximum Life" -> expects life to increase by 1000.
    let build = build_from_code(DEADEYE_CODE);
    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 500.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec!["+1000 to maximum Life".to_string()],
    };
    let out = calculate(&build, &opts).expect("calculate with modifier");

    // base_life 500 + modifier +1000 = 1500
    assert_near_int("life_with_modifier", 1500.0, out.life);
}

#[test]
fn deadeye_golden_snapshot_is_deterministic() {
    // Stage 4: two identical calls produce identical results (determinism guarantee).
    let build = build_from_code(DEADEYE_CODE);
    let opts = default_opts();

    let out1 = calculate(&build, &opts).expect("first calc");
    let out2 = calculate(&build, &opts).expect("second calc");

    assert_eq!(out1.life, out2.life, "life non-deterministic");
    assert_eq!(out1.dps, out2.dps, "dps non-deterministic");
    assert_eq!(
        out1.fire_resistance, out2.fire_resistance,
        "fire_res non-deterministic"
    );
}

// Martial Artist golden baseline (a second fixture, verifying non-Ranger class decoding works)

#[test]
fn martial_artist_golden_decode_and_calc() {
    let xml = decode_pob_code(MARTIAL_ARTIST_CODE.trim()).expect("decode martial artist code");
    // parse_build_header only accepts a PathOfBuilding2 root; successful parsing confirms it's a PoE2 document.
    let header = parse_build_header(&xml).expect("parse header");

    // level should be within a sane range.
    assert!(
        header.identity.level > 0 && header.identity.level <= 100,
        "level out of range: {}",
        header.identity.level
    );

    // Build + CalcOrchestrator run without error.
    let build = Build::new().with_character(CharacterIdentity {
        level: header.identity.level,
        class_name: header.identity.class_name.clone(),
        ascendancy_name: header.identity.ascendancy_name.clone(),
    });

    let out = calculate(&build, &default_opts()).expect("martial artist calc");
    // Empty Build with no mods -> output is all zero (default).
    assert_near_int("martial_artist_life", 0.0, out.life);
}

// Empty-input constants (PoE2 base crit damage bonus and guaranteed hit without evasion).
const DEADEYE_NEUTRAL_CRIT_MULTIPLIER: f64 = 2.0;

const DEADEYE_NEUTRAL_HIT_CHANCE: f64 = 1.0;

/// Wave 2 ES mod injection: verifies an ES mod is correctly parsed and injected into the calc pipeline.
///
/// Note: the current CalcOrchestrator returns results via the
/// MinimalOutput -> OutputTable path, which only keeps MinimalOutput's core
/// fields (life/mana/resistance/crit/hit); defence extension fields like
/// energy_shield / es_recharge fall back to OutputTable::default() on this
/// path. This test verifies: the pipeline doesn't crash + the life injection
/// path works + default fields match expectations. Full ES-calculation unit
/// tests live at the pobr-core layer (defense.rs / session.rs).
#[test]
fn energy_shield_modifier_preserves_minimal_output_contract() {
    let build = build_from_code(DEADEYE_CODE);
    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 500.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec![
            "+500 to maximum Energy Shield".to_string(),
            "+1000 to maximum Life".to_string(),
        ],
    };
    // The pipeline doesn't crash.
    let out = calculate(&build, &opts).expect("calc with es modifier should not panic");

    // life = base(500) + modifier(+1000) = 1500 (kept correctly by the MinimalOutput path).
    assert_near_int("life_with_modifiers", 1500.0, out.life);

    // The current orchestrator path falls back energy_shield to OutputTable::default() = 0.0.
    // This assertion is a regression lock against silent rewrites (if the orchestrator
    // is ever changed to return all fields, this assertion should be updated to 500.0).
    assert_near_int(
        "energy_shield_via_orchestrator_path",
        0.0,
        out.energy_shield,
    );
}

/// Wave 1/2 resistance + defence field determinism as a whole (two calls agree).
///
/// Only injects known-parseable mods; avoids triggering a ParseError (unsupported mod text would return Err).
#[test]
fn injected_calculation_is_deterministic() {
    let build = build_from_code(DEADEYE_CODE);
    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 1000.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec![
            "+300 to maximum Energy Shield".to_string(),
            "+50% to fire resistance".to_string(),
        ],
    };

    let out1 = calculate(&build, &opts).expect("first calc");
    let out2 = calculate(&build, &opts).expect("second calc");

    // Core defence fields.
    assert_eq!(out1.life, out2.life, "life non-deterministic");
    assert_eq!(
        out1.fire_resistance, out2.fire_resistance,
        "fire_resistance"
    );
    assert_eq!(
        out1.es_recharge_per_second, out2.es_recharge_per_second,
        "es_recharge_per_second"
    );
    // Wave 2 charges/ailment/skill fields.
    assert_eq!(
        out1.charge_power_maximum, out2.charge_power_maximum,
        "charge_power_max"
    );
    assert_eq!(out1.chill_effect, out2.chill_effect, "chill_effect");
    assert_eq!(out1.aoe_radius, out2.aoe_radius, "aoe_radius");
    assert_eq!(
        out1.trigger_rate_cap, out2.trigger_rate_cap,
        "trigger_rate_cap"
    );
    assert_eq!(
        out1.bleed_stacked_dps, out2.bleed_stacked_dps,
        "bleed_stacked_dps"
    );
}
