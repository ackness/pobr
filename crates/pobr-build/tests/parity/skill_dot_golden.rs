//! Skill DoT golden fixture: an essence-drain build (a ready-made pure-DoT
//! build in the corpus) run end-to-end through real data orchestration and
//! compared against the PoB2 golden `TotalDot` (meta.json::player_stats, the
//! same Lua-side export used by tools/pob2-oracle).
//!
//! Current coverage and known gaps (recorded in the acceptance report; see the
//! commit message / skill_dot.rs module doc for details):
//! - The `<Type>Dot` base-value chain (statmap
//!   `base_<type>_damage_to_deal_per_minute / 60`) ✅ works — `skill_total_dot`
//!   is non-zero for the first time.
//! - The PoB2 full-bit table (permanently switched on): this build's TotalDot
//!   **hits golden value-for-value (ratio = 1.0000)** — Swift Affliction's
//!   `Damage MORE (Dot)` reaches the bucket via the DOT bit.
//! - dotIsSpell data gap: the ingested `.dat` doesn't include the value-less
//!   boolean stat (`spell_damage_modifiers_apply_to_skill_dot`), so the Spell
//!   bit is conservatively stripped; this build hitting value-for-value shows
//!   this gap doesn't manifest in this corpus, so it's left for a build that
//!   actually consumes dotIsSpell to verify.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

fn build_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .join(name)
        .canonicalize()
        .expect("build dir exists")
}

fn load_data() -> BuildData {
    // Pins the data version being checked against the golden values (decoupled from the active DATA_VERSION); see pobr_data::GOLDEN_PARITY_DATA_VERSION.
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
    BuildData::load(&data).expect("load BuildData")
}

fn golden_stat(dir: &Path, key: &str) -> Option<f64> {
    let text = std::fs::read_to_string(dir.join("meta.json")).expect("read meta.json");
    let sanitized = text
        .replace("-Infinity", "-1e308")
        .replace("Infinity", "1e308")
        .replace("NaN", "0");
    let json: serde_json::Value = serde_json::from_str(&sanitized).expect("parse meta.json");
    json.get("player_stats")?.get(key)?.as_f64()
}

fn run_build(dir: &Path, data: &BuildData) -> pobr_core::calc::OutputTable {
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    let build = parse_build_from_code(code.trim()).expect("parse build");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true, // PoB2's main panel (golden export) is always EFFECTIVE
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).expect("calculate")
}

/// essence-drain: the skill DoT chain is live + compared against the PoB2 oracle's `TotalDot` (a regression pin).
#[test]
fn essence_drain_skill_dot_vs_pob2_golden() {
    let dir = build_dir("sorceress-chronomancer-essence-drain");
    let data = load_data();
    let out = run_build(&dir, &data);
    let golden_total_dot = golden_stat(&dir, "TotalDot").expect("golden TotalDot");

    assert!(
        out.skill_total_dot > 0.0,
        "the DoT chain must be live (<Type>Dot base → instance → TotalDot): {out:#?}"
    );
    let ratio = out.skill_total_dot / golden_total_dot;
    println!(
        "essence-drain TotalDot: pobr={:.2} golden={:.2} ratio={:.4}",
        out.skill_total_dot, golden_total_dot, ratio
    );

    // Regression pin: with the PoB2 full-bit table permanently switched on, the DOT bit
    // reaches the bucket, and Swift Affliction's `Damage MORE (Dot)` hits dotCfg via the DOT bit --
    // measured ratio = 1.0000, pinned at a 5% match gate.
    let pin = 0.95..=1.05;
    assert!(
        pin.contains(&ratio),
        "TotalDot's ratio to golden is outside the pinned range {pin:?}: ratio={ratio:.4} (pobr={} golden={})",
        out.skill_total_dot,
        golden_total_dot
    );

    // Self-check for the combined family: WithDotDPS = TotalDPS + TotalDot; CombinedDPS >= WithDotDPS.
    assert!(
        (out.with_dot_dps - (out.dps + out.skill_total_dot)).abs() < 1e-6,
        "WithDotDPS identity is violated: {out:#?}"
    );
    assert!(
        out.total_dot_dps >= out.skill_total_dot,
        "TotalDotDPS must be ≥ the skill's TotalDot: {out:#?}"
    );
}

/// A non-DoT build (comet): skill DoT contract fields stay neutrally zero
/// (skill dot doesn't fire spuriously), and the combined-family fields satisfy
/// the composition identity.
#[test]
fn non_dot_build_keeps_skill_dot_neutral() {
    let dir = build_dir("sorceress-stormweaver-comet");
    let data = load_data();
    let out = run_build(&dir, &data);
    assert_eq!(
        out.skill_dot_instance, 0.0,
        "a non-DoT skill must not produce a skill DoT instance: {out:#?}"
    );
    assert_eq!(out.skill_total_dot, 0.0);
    assert_eq!(
        out.with_dot_dps, 0.0,
        "no skill dot: WithDotDPS stays neutral"
    );
    // CombinedDPS = TotalDPS + TotalDotDPS (ailment DoT still folds into the combined family).
    assert!(
        (out.combined_dps - (out.dps + out.total_dot_dps)).abs() < 1e-6,
        "CombinedDPS identity is violated: {out:#?}"
    );
}
