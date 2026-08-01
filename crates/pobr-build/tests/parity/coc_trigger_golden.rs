//! Golden fixture for the CoC (Cast on Critical Strike) trigger chain.
//!
//! T5 already shipped directional assertions (calc_orchestrator unit tests:
//! recognition hits / attack-speed -> trigger-rate monotonicity); this suite
//! adds **fixed-value golden**: a PoB2 XML fixture
//! (`fixtures/coc_cast_on_crit.xml`: Wooden Club + attack source Armour
//! Breaker + the MetaCastOnCritPlayer trigger + a triggered Fireball in the
//! same group, main skill = Fireball) run through the production parser
//! `parse_build` -> `calculate_with_data` end-to-end, pinning down the current
//! values of the trigger panel and the source-stats chain, so future
//! refactors can't silently change trigger output.
//!
//! Chain reference (vendor `Modules/CalcTriggers.lua`):
//! - Recognition: the `MetaCastOnCritPlayer` entry in
//!   `overlay/trigger_configs.json` (`triggered_by`, `trigger_on_crit = true`,
//!   source predicate = Attack; :1089-1092);
//! - Source rate: the sub-calculation takes the source skill's post-calc
//!   effective rate (GlobalCache-equivalent, :74-86);
//! - Trigger chance: source hit × source crit folded in (trigger_on_crit,
//!   :716-770); the CoC entry has no cooldown override and the trigger gem has
//!   no cooldown data, so the `trigger_rate_cap` panel stays 0 (no cooldown
//!   cap) and trigger rate = source rate × source hit × source crit (the pure
//!   source-rate-driven branch).
//!
//! **Vendor oracle comparison finding (2026-06, vendor 0.18 checkout)**:
//! running `tools/pob2-oracle` against the same fixture **can't produce a
//! TriggerRate** — vendor's own CoC chain is broken:
//! `mainSkill.triggeredBy.grantedEffect.name = "SupportMetaCastOnCritPlayer"`,
//! but the configTable name lookup (`CalcTriggers.lua:1452-1455`) doesn't
//! match `"cast on critical strike"` (the old PoE1 name key), so the trigger
//! handler never runs and `skillData.triggered` gets cleared (matches
//! DeepWiki's known "needs an entire overhaul" state). The **intermediate
//! values** that can be compared (source-skill stats, oracle
//! mainActiveSkill=1) have been checked:
//! - source rate: vendor `Speed = 1.16` (rounded to 2 display digits) vs PoBR
//!   `1.15942029` ✓;
//! - source base crit: vendor `PreEffectiveCritChance = 5` vs PoBR `0.05` ✓;
//! - source hit: vendor `77%` vs PoBR `74.2558559%` — this is the existing
//!   accuracy-formula parity gap (already tracked by ninja_parity), not a
//!   trigger-chain deviation.
//!
//! Baseline updates: when a behaviour-change commit alters the `GOLDEN_*`
//! constants below, update them to the actual values from the failure message
//! (same convention as `golden_regression.rs`).

use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, repo_data_root};

/// CoC build fixture: Weapon 1 = Wooden Club (the source attack's weapon
/// base), no passives; group = [Armour Breaker (attack source), Cast on
/// Critical (trigger), Fireball (triggered)], main skill = Fireball
/// (mainActiveSkill=3, the non-support ordinal within the group).
const COC_XML: &str = include_str!("../fixtures/coc_cast_on_crit.xml");

// Golden baseline (established when this suite first landed, worktree baseline 761ebb5)

/// Actual trigger rate (times/sec) = source rate × source hit × source crit
/// (trigger_on_crit folded in): `1.15942029 × 0.742558559 × 0.05`.
const GOLDEN_SKILL_TRIGGER_RATE: f64 = 0.043046873;
/// Trigger rate cap: CoC has no cooldown data -> the panel stays 0.
const GOLDEN_TRIGGER_RATE_CAP: f64 = 0.0;
/// Triggered Fireball's DPS (currently = hit_avg × its own cast rate; once
/// trigger-rate -> DPS wiring lands, this baseline updates with that
/// behaviour commit).
const GOLDEN_DPS: f64 = 65.537499974;
/// Triggered Fireball's main-skill stats.
const GOLDEN_MAIN_CRIT_CHANCE: f64 = 0.07;
const GOLDEN_MAIN_HIT_CHANCE: f64 = 1.0;
const GOLDEN_MAIN_ACTION_RATE: f64 = 0.833333333;
const GOLDEN_MAIN_TOTAL_HIT_AVG: f64 = 78.645;

/// Source skill's (Armour Breaker @ Wooden Club) stats: the source for the
/// sub-calculation's injected
/// `TriggerSourceRate`/`TriggerSourceHitChance`/`TriggerSourceCritChance`.
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
    // Pins the data version being checked against the golden values (decoupled from the active DATA_VERSION); see pobr_data::GOLDEN_PARITY_DATA_VERSION.
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn parse_coc_build() -> Build {
    parse_build(COC_XML).expect("parse coc fixture")
}

/// End-to-end golden: fixture XML -> production parser -> data-orchestrated
/// calc, pinning the trigger panel plus the triggered main-skill output.
#[test]
fn coc_golden_triggered_skill_output() {
    let data = load_build_data();
    let build = parse_coc_build();
    let out = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
        .expect("coc calculate");

    assert_near_float(
        "skill_trigger_rate",
        GOLDEN_SKILL_TRIGGER_RATE,
        out.skill_trigger_rate,
    );
    assert_near_float(
        "trigger_rate_cap",
        GOLDEN_TRIGGER_RATE_CAP,
        out.trigger_rate_cap,
    );
    assert_near_float("dps", GOLDEN_DPS, out.dps);
    assert_near_float("crit_chance", GOLDEN_MAIN_CRIT_CHANCE, out.crit_chance);
    assert_near_float("hit_chance", GOLDEN_MAIN_HIT_CHANCE, out.hit_chance);
    assert_near_float("action_rate", GOLDEN_MAIN_ACTION_RATE, out.action_rate);
    assert_near_float(
        "total_hit_avg",
        GOLDEN_MAIN_TOTAL_HIT_AVG,
        out.total_hit_avg,
    );
}

/// Source-skill golden (an intermediate value for TriggerRate): switches the
/// group's main skill to Armour Breaker, i.e. the source sub-calculation's
/// stats (the vendor oracle comparison points: Speed 1.16 / PreEffectiveCrit 5%).
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

/// Trigger-chain identity: `skill_trigger_rate = source rate × source hit ×
/// source crit` (CoC's pure source-rate-driven branch + trigger_on_crit
/// folded in, in perform's `fill_trigger`). This is a structural relationship
/// between the golden constants — any silent change to a step in the chain
/// would break both this assertion and the two golden sets above.
#[test]
fn coc_trigger_rate_is_source_chain_product() {
    let expected =
        GOLDEN_SRC_EFFECTIVE_ACTION_RATE * GOLDEN_SRC_HIT_CHANCE * GOLDEN_SRC_CRIT_CHANCE;
    // fill_trigger rounds at the end (9 internal decimal digits): the golden constants themselves are already the rounded values.
    assert!(
        (expected - GOLDEN_SKILL_TRIGGER_RATE).abs() < 1e-8,
        "trigger chain product {expected} != golden {GOLDEN_SKILL_TRIGGER_RATE}"
    );
}

/// Determinism: two calculations produce identical output (the trigger sub-calculation introduces no non-determinism).
#[test]
fn coc_golden_is_deterministic() {
    let data = load_build_data();
    let build = parse_coc_build();
    let opts = DataOrchestratorOptions::default();
    let out1 = calculate_with_data(&build, &data, &opts).expect("first calc");
    let out2 = calculate_with_data(&build, &data, &opts).expect("second calc");
    assert_eq!(
        out1.skill_trigger_rate, out2.skill_trigger_rate,
        "skill_trigger_rate non-deterministic"
    );
    assert_eq!(out1.dps, out2.dps, "dps non-deterministic");
    assert_eq!(
        out1.trigger_rate_cap, out2.trigger_rate_cap,
        "trigger_rate_cap non-deterministic"
    );
}
