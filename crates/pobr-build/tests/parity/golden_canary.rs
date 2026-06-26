//! Golden **canary**——跨伤害类型 / 防御层各取一个代表 build，对 PoB2 黄金值做
//! **宽容标定 sanity**。
//!
//! 定位（重要）：这是**额外**的"标定烟雾"锚点，**不替代** `ninja_parity` 的全量 parity
//! 门禁——后者仍是穷尽对照 + 回归基线。canary 只快速抓住"系数/基础值写错"这类**粗标定
//! 漂移**（逻辑不变量抓不到、又不该靠追精确数字暴露），故容差刻意宽：Life/防御 ±10–15%
//! （标定应吻合），DPS ±40%（只抓数量级漂移，容忍 offence 管线已知缺口）。
//!
//! 版本钉定 `GOLDEN_PARITY_DATA_VERSION`（见 parity.rs A 组契约 + doc 16 §5）。
//! 口径与 CLI `calculate-build` 默认一致（mode_effective / Pinnacle / enemy_level=0）。
//! DoT / chaos 的标定走 `skill_dot_golden`（DoT 管线偏差大，不塞进 canary）。

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
    // golden 钉定被校验的数据版本（与活动 DATA_VERSION 解耦）；见 pobr_data::GOLDEN_PARITY_DATA_VERSION。
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

/// ±tol 相对标定带（golden 必须有效 >0——canary 只选有有效 golden 值的 stat）。
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

// 宽容刻度：Life/防御 = 紧（标定应吻合）；DPS = 宽（只抓数量级漂移）。
const LIFE: f64 = 0.10;
const DEF: f64 = 0.15;
const DPS: f64 = 0.40;

/// 物理近战 / 护甲 + 格挡层。
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

/// 冰冷投射 / 闪避 + ES 层。
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

/// 冰冷法术 / 纯 ES（CI，Life=1 跳过）。
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

/// 闪避近战层。
#[test]
fn canary_evasion_melee() {
    let d = load_data();
    let n = "monk-martial-artist-twister";
    let o = run(n, &d);
    within(n, "Life", o.life, golden_stat(n, "Life"), LIFE);
    within(n, "Evasion", o.evasion, golden_stat(n, "Evasion"), DEF);
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}

/// 火焰法术 / 护甲层（CI，Life=1 跳过）。
#[test]
fn canary_fire_spell_armour() {
    let d = load_data();
    let n = "druid-oracle-ember-fusillade";
    let o = run(n, &d);
    within(n, "Armour", o.armour, golden_stat(n, "Armour"), DEF);
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}

/// 闪电 / 纯生命层。
#[test]
fn canary_lightning_life() {
    let d = load_data();
    let n = "witch-blood-mage-coiling-bolts";
    let o = run(n, &d);
    within(n, "Life", o.life, golden_stat(n, "Life"), LIFE);
    within(n, "DPS", o.dps, golden_stat(n, "TotalDPS"), DPS);
}
