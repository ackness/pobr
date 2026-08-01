//! 专项 golden fixture：Block / Spirit / Ward / Deflection 面板族
//! 对 ninja build 黄金值（`meta.json::player_stats`）@5% 断言。
//!
//! Track D 门禁：新列尚未进 ninja_parity defensive_rows
//! （W2/F 才扩列），以本文件的专项断言 + 旧基线不倒退做双保险。

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
    // golden 钉定其被校验的数据版本（与活动 DATA_VERSION 解耦）；见
    // pobr_data::GOLDEN_PARITY_DATA_VERSION。
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

/// @5% 相对容差断言（golden=0 时要求恰为 0，verify 零值不误报）。
fn assert_within_5pct(label: &str, build: &str, pobr: f64, golden: f64) {
    if golden == 0.0 {
        assert_eq!(pobr, 0.0, "{build} {label}: golden=0 而 PoBR={pobr}");
        return;
    }
    let ratio = pobr / golden;
    assert!(
        (0.95..=1.05).contains(&ratio),
        "{build} {label}: PoBR={pobr} vs golden={golden}（ratio={ratio:.4}，超出 @5%）"
    );
}

/// 盾 build 的 `EffectiveBlockChance` @5%（13-G8）：warrior 双盾系 ninja build
/// 是现成 fixture。
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

/// 无盾 build 的格挡保持 0（verify 零值不误报）。
#[test]
fn block_chance_zero_on_non_shield_builds() {
    let data = load_data();
    let out = run_build("sorceress-stormweaver-comet", &data);
    assert_eq!(out.effective_block_chance, 0.0);
    assert_eq!(out.block_chance, 0.0);
}

/// `DeflectChance`/`DeflectionRating` @5%（13-G10）：huntress/monk 系
/// `Gain Deflection Rating equal to N% of Evasion` build 是现成 fixture；
/// 无 deflect 来源的 build 双值保持 0（verify 零值不误报）。
///
/// 0.5.4b 适配注：Phase 0 曾以「DeflectionRating ~0.60x」ignore 本 canary；
/// 复盘（oracle 复跑 2026-07-17）确认该缺口整体是 Evasion 缺口（Mageblood，
/// 6685c30）的下游——rating 派生自 `Evasion × GainAsDeflection%`，公式本身
/// （CalcDefence.lua:48-54 / :1516-1522）与 vendor 一致，Evasion 闭合后
/// monk rating 22267.8 vs golden 22312.08（0.998x），无需公式改动。
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
        // rating < 1 时 vendor 给 0 几率（CalcDefence.lua:49-51），rating 本身按 @5% 比对。
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

/// `Spirit` 池本值 @5%（13-G11）：覆盖纯任务奖励（100）、权杖 rolled `Spirit:`
/// 行（druid 433）、`+N to Spirit` 装备/树词条（mercenary 336）三类来源形态。
#[test]
fn spirit_pool_matches_golden() {
    let data = load_data();
    for name in [
        "warrior-titan-shield-wall",     // 100（仅任务奖励）
        "huntress-ritualist-bow-shot",   // 100
        "druid-oracle-comet",            // 433（权杖 213 + 任务 100 + 树/装备）
        "mercenary-tactician-wolf-pack", // 336
        "monk-invoker-frost-bomb",       // 343
    ] {
        let dir = builds_dir().join(name);
        let golden = golden_stat(&dir, "Spirit").expect("golden Spirit");
        let out = run_build(name, &data);
        assert_within_5pct("Spirit", name, out.spirit, golden);
    }
}
