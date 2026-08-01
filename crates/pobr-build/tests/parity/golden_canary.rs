//! Golden **canary** — takes one representative build per damage type /
//! defence layer and runs a **loose calibration sanity check** against PoB2
//! golden values.
//!
//! Purpose (important): this is an **extra** "calibration smoke" anchor, it
//! **doesn't replace** `ninja_parity`'s full parity gate — that's still the
//! exhaustive comparison + regression baseline. The canary only catches
//! **coarse calibration drift** like a mistyped coefficient/base value (the
//! kind of thing logic invariants can't catch, but that shouldn't need
//! precise-number chasing to surface either), so the tolerance is
//! deliberately loose: Life/defence ±10-15% (calibration should be close),
//! DPS ±40% (only catches order-of-magnitude drift, tolerating known offence
//! pipeline gaps).
//!
//! Pins `GOLDEN_PARITY_DATA_VERSION` (see parity.rs group A's contract + doc
//! 16 §5). Uses the same settings as the CLI `calculate-build` default
//! (mode_effective / Pinnacle / enemy_level=0). DoT/chaos calibration is
//! covered by `skill_dot_golden` instead (the DoT pipeline's deviation is too
//! large to fold into the canary).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn load_data() -> BuildData {
    // Pins the data version being checked against the golden values (decoupled from the active DATA_VERSION); see pobr_data::GOLDEN_PARITY_DATA_VERSION.
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
    BuildData::load(&data).expect("load BuildData")
}

fn golden_stat(name: &str, key: &str) -> f64 {
    let text =
        std::fs::read_to_string(builds_dir().join(name).join("meta.json")).expect("read meta.json");
    let sanitized = text
        .replace("-Infinity", "-1e308")
        .replace("Infinity", "1e308")
        .replace("NaN", "0");
    let json: serde_json::Value = serde_json::from_str(&sanitized).expect("parse meta.json");
    json.get("player_stats")
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| panic!("{name}: golden 缺 {key}"))
}

fn run(name: &str, data: &BuildData) -> OutputTable {
    let code =
        std::fs::read_to_string(builds_dir().join(name).join("code.txt")).expect("read code.txt");
    let build = parse_build_from_code(code.trim()).expect("parse build");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).expect("calculate")
}

/// ±tol relative calibration band (golden must be valid >0 -- the canary only picks stats with valid golden values).
fn within(name: &str, label: &str, pobr: f64, golden: f64, tol: f64) {
    assert!(
        golden > 0.0,
        "{name} {label}: golden={golden}（canary 应只选有效 golden 值的 stat）"
    );
    let r = pobr / golden;
    assert!(
        (1.0 - tol..=1.0 + tol).contains(&r),
        "{name} {label}: PoBR={pobr:.1} vs golden={golden:.1}（ratio {r:.3}，超 ±{:.0}% 标定带）",
        tol * 100.0
    );
}

// Loose scale: Life/defence = tight (calibration should be close); DPS = loose (only catches order-of-magnitude drift).
const LIFE: f64 = 0.10;
const DEF: f64 = 0.15;
const DPS: f64 = 0.40;

/// Physical melee / armour + block layer.
#[test]
fn canary_physical_armour_block() {
    let d = load_data();
    let n = "warrior-titan-shield-wall";
    let o = run(n, &d);
    within(n, "Life", o.life, golden_stat(n, "Life"), LIFE);
    within(n, "Armour", o.armour, golden_stat(n, "Armour"), DEF);
    within(
        n,
        "Block",
        o.effective_block_chance,
        golden_stat(n, "EffectiveBlockChance"),
        DEF,
    );
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}

/// Cold projectile / evasion + ES layer.
#[test]
fn canary_cold_projectile_evasion_es() {
    let d = load_data();
    let n = "ranger-pathfinder-ice-shot";
    let o = run(n, &d);
    within(n, "Life", o.life, golden_stat(n, "Life"), LIFE);
    within(n, "Evasion", o.evasion, golden_stat(n, "Evasion"), DEF);
    within(
        n,
        "ES",
        o.energy_shield,
        golden_stat(n, "EnergyShield"),
        DEF,
    );
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}

/// Cold spell / pure ES (CI, Life=1 skipped).
#[test]
fn canary_cold_spell_es() {
    let d = load_data();
    let n = "sorceress-disciple-of-varashta-comet";
    let o = run(n, &d);
    within(
        n,
        "ES",
        o.energy_shield,
        golden_stat(n, "EnergyShield"),
        DEF,
    );
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}

/// Evasion melee layer.
#[test]
fn canary_evasion_melee() {
    let d = load_data();
    let n = "monk-martial-artist-twister";
    let o = run(n, &d);
    within(n, "Life", o.life, golden_stat(n, "Life"), LIFE);
    within(n, "Evasion", o.evasion, golden_stat(n, "Evasion"), DEF);
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}

/// Fire spell / armour layer (CI, Life=1 skipped).
///
/// Phase 0 once `#[ignore]`d this as a "0.5.4b offence DPS adaptation gap
/// (~0.60x)"; #4's re-verification found ember-fusillade TotalDPS is already
/// 1.00x (548742 vs 549423 -- the gap-map entry had already been closed
/// incidentally by earlier mana-multiplier / Arcane Surge fixes, just without
/// re-checking), so it's un-ignored.
#[test]
fn canary_fire_spell_armour() {
    let d = load_data();
    let n = "druid-oracle-ember-fusillade";
    let o = run(n, &d);
    within(n, "Armour", o.armour, golden_stat(n, "Armour"), DEF);
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}

/// Lightning / pure life layer.
#[test]
fn canary_lightning_life() {
    let d = load_data();
    let n = "witch-blood-mage-coiling-bolts";
    let o = run(n, &d);
    within(n, "Life", o.life, golden_stat(n, "Life"), LIFE);
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}
