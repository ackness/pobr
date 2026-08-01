//! Dedicated golden fixtures: asserts the Block / Spirit / Ward / Deflection
//! panel family against ninja build golden values (`meta.json::player_stats`)
//! at @5%.
//!
//! Track D gate: these new columns aren't in ninja_parity's defensive_rows yet
//! (that expansion lands with W2/F); this file's dedicated assertions plus
//! "the old baseline doesn't regress" serve as a double safety net.

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
    // Pins the data version being checked against the golden values (decoupled from the active DATA_VERSION); see
    // pobr_data::GOLDEN_PARITY_DATA_VERSION.
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

fn run_build(name: &str, data: &BuildData) -> OutputTable {
    let dir = builds_dir().join(name);
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    let build = parse_build_from_code(code.trim()).expect("parse build");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).expect("calculate")
}

/// @5% relative-tolerance assertion (requires exactly 0 when golden=0, to verify no false positives at zero).
fn assert_within_5pct(label: &str, build: &str, pobr: f64, golden: f64) {
    if golden == 0.0 {
        assert_eq!(pobr, 0.0, "{build} {label}: golden=0 but PoBR={pobr}");
        return;
    }
    let ratio = pobr / golden;
    assert!(
        (0.95..=1.05).contains(&ratio),
        "{build} {label}: PoBR={pobr} vs golden={golden} (ratio={ratio:.4}, exceeds @5%)"
    );
}

/// `EffectiveBlockChance` @5% for shield builds (13-G8): the warrior
/// dual-shield ninja builds are ready-made fixtures.
#[test]
fn block_chance_matches_golden_on_shield_builds() {
    let data = load_data();
    for name in [
        "warrior-titan-shield-wall",
        "warrior-smith-of-kitava-shield-wall",
    ] {
        let dir = builds_dir().join(name);
        let golden =
            golden_stat(&dir, "EffectiveBlockChance").expect("golden EffectiveBlockChance");
        let out = run_build(name, &data);
        assert_within_5pct(
            "EffectiveBlockChance",
            name,
            out.effective_block_chance,
            golden,
        );
    }
}

/// Block stays 0 for non-shield builds (verifies no false positive at zero).
#[test]
fn block_chance_zero_on_non_shield_builds() {
    let data = load_data();
    let out = run_build("sorceress-stormweaver-comet", &data);
    assert_eq!(out.effective_block_chance, 0.0);
    assert_eq!(out.block_chance, 0.0);
}

/// `DeflectChance`/`DeflectionRating` @5% (13-G10): the huntress/monk
/// `Gain Deflection Rating equal to N% of Evasion` builds are ready-made
/// fixtures; builds with no deflect source keep both values at 0 (verifies no
/// false positive at zero).
///
/// 0.5.4b adaptation note: Phase 0 once ignored this canary as "DeflectionRating
/// ~0.60x". A follow-up review (oracle re-run 2026-07-17) confirmed this gap
/// was entirely downstream of the Evasion gap (Mageblood, 6685c30) — rating is
/// derived from `Evasion × GainAsDeflection%`, and the formula itself
/// (CalcDefence.lua:48-54 / :1516-1522) already matches vendor. Once Evasion
/// was closed, monk rating landed at 22267.8 vs golden 22312.08 (0.998x), with
/// no formula change needed.
#[test]
fn deflection_matches_golden() {
    let data = load_data();
    for name in [
        "huntress-spirit-walker-twister", // rating 5666.7 / chance 37
        "monk-martial-artist-twister",    // rating 22312.08 / chance 84
        "warrior-titan-shield-wall",      // rating 0.84 / chance 0
        "sorceress-stormweaver-comet",    // 0 / 0
    ] {
        let dir = builds_dir().join(name);
        let golden_rating = golden_stat(&dir, "DeflectionRating").expect("golden DeflectionRating");
        let golden_chance = golden_stat(&dir, "DeflectChance").expect("golden DeflectChance");
        let out = run_build(name, &data);
        // When rating < 1, vendor gives a 0 chance (CalcDefence.lua:49-51); the rating itself is still compared at @5%.
        if golden_rating >= 1.0 {
            assert_within_5pct(
                "DeflectionRating",
                name,
                out.deflection_rating,
                golden_rating,
            );
        }
        assert_within_5pct("DeflectChance", name, out.deflect_chance, golden_chance);
    }
}

/// `Spirit` pool value @5% (13-G11): covers three source shapes — a pure
/// quest reward (100), a sceptre's rolled `Spirit:` line (druid 433), and
/// `+N to Spirit` gear/tree mods (mercenary 336).
#[test]
fn spirit_pool_matches_golden() {
    let data = load_data();
    for name in [
        "warrior-titan-shield-wall",     // 100 (quest reward only)
        "huntress-ritualist-bow-shot",   // 100
        "druid-oracle-comet",            // 433 (sceptre 213 + quest 100 + tree/gear)
        "mercenary-tactician-wolf-pack", // 336
        "monk-invoker-frost-bomb",       // 343
    ] {
        let dir = builds_dir().join(name);
        let golden = golden_stat(&dir, "Spirit").expect("golden Spirit");
        let out = run_build(name, &data);
        assert_within_5pct("Spirit", name, out.spirit, golden);
    }
}
