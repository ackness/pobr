//! M4-I W-B2：双持（Weapon1 + Weapon2 均为武器基底）端到端测试。
//!
//! 覆盖编排装配（`dual_wield_off_hand_contribution` → 第二个 off-hand
//! `HandSource`）→ hand pass → combineStat 整链：
//! - 双持产出 MH/OH 两张子表，合并值按 vendor 模式（DPS=均值、Speed=调和平均，
//!   `CalcOffence.lua:2451-2545`）；
//! - Weapon2 局部词条折入 OH 武器源（不再泄漏进全局加法桶）；
//! - `with maces` 词条按 per-hand 武器位只进 MH 腿（vendor 口径；M4-I 切换
//!   commit 起唯一行为，legacy `UsingMace` 全局条件近似的两腿同吃锚点已随
//!   feature 退役）。
//!
//! 基底数值（`data/4.5.0.3.4/base/base_items.json`）：
//! Wooden Club（One Hand Mace）物理 6–10、690ms、暴击 5%；
//! Shortsword（One Hand Sword）物理 6–9、645ms、暴击 5%。

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn weapon(base_name: &str, mods: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base_name),
        rarity: ItemRarity::Normal,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: mods.iter().map(|s| s.to_string()).collect(),
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    }
}

/// MH 锤 + 可选 OH 武器的基本攻击 build（AxeChopPlayer 无武器限制建模，
/// 与 skill_damage_dps.rs 同款 harness）。
fn dual_build(off_hand: Option<Item>) -> Build {
    let mut b = Build::new()
        .with_character(CharacterIdentity {
            level: 50,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_active_skill("AxeChopPlayer", 1),
        )
        .set_item(EquipmentSlot::Weapon1, weapon("Wooden Club", &[]));
    if let Some(oh) = off_hand {
        b = b.set_item(EquipmentSlot::Weapon2, oh);
    }
    b
}

fn opts(extra: &[&str]) -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: extra.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

/// 双持装配 + combineStat：MH/OH 两张子表齐备，合并 DPS = (MH+OH)/2、
/// Speed = 调和平均（vendor :2541-2545 / :3026）；单手 build 无 OH 子表。
#[test]
fn dual_wield_produces_both_hands_and_combines_per_vendor_modes() {
    let data = load_build_data();

    let single = calculate_with_data(&dual_build(None), &data, &opts(&[])).expect("single calc");
    assert!(single.main_hand.is_some(), "单手攻击 build 须有 MH 子表");
    assert!(single.off_hand.is_none(), "无 Weapon2 武器不得产 OH pass");

    let dual = calculate_with_data(
        &dual_build(Some(weapon("Shortsword", &[]))),
        &data,
        &opts(&[]),
    )
    .expect("dual calc");
    let mh = dual.main_hand.as_ref().expect("双持 MH 子表");
    let oh = dual.off_hand.as_ref().expect("双持 OH 子表");

    // 两手基底不同（6–10@690ms vs 6–9@645ms）→ 两腿速率/击中不同。
    assert!(
        (mh.speed - oh.speed).abs() > 1e-6,
        "两手攻速基底不同：MH={} OH={}",
        mh.speed,
        oh.speed
    );
    // combineStat：DPS 均值（非 doubleHits）、Speed 调和平均。
    let eps = 1e-6;
    assert!(
        (dual.dps - (mh.total_dps + oh.total_dps) / 2.0).abs() < eps,
        "TotalDPS = (MH+OH)/2：{} vs ({}+{})/2",
        dual.dps,
        mh.total_dps,
        oh.total_dps
    );
    let harmonic = 2.0 / (1.0 / mh.speed + 1.0 / oh.speed);
    assert!(
        (dual.action_rate - harmonic).abs() < 1e-4,
        "Speed 调和平均：{} vs {harmonic}",
        dual.action_rate
    );
    // OH 腿吃到 Shortsword 基底（平均 7.5 < Wooden Club 8）→ 两腿击中均非零。
    assert!(mh.average_hit > 0.0 && oh.average_hit > 0.0);
}

/// Weapon2 局部词条折入 OH 武器源（独立乘区），不泄漏进全局加法桶：
/// OH 件局部「increased Physical Damage」只放大 OH 腿，MH 腿不动。
#[test]
fn offhand_local_phys_mod_scales_only_offhand_leg() {
    let data = load_build_data();

    let plain = calculate_with_data(
        &dual_build(Some(weapon("Shortsword", &[]))),
        &data,
        &opts(&[]),
    )
    .expect("plain calc");
    let boosted = calculate_with_data(
        &dual_build(Some(weapon(
            "Shortsword",
            &["40% increased Physical Damage"],
        ))),
        &data,
        &opts(&[]),
    )
    .expect("boosted calc");

    let (p_mh, p_oh) = (
        plain.main_hand.as_ref().unwrap(),
        plain.off_hand.as_ref().unwrap(),
    );
    let (b_mh, b_oh) = (
        boosted.main_hand.as_ref().unwrap(),
        boosted.off_hand.as_ref().unwrap(),
    );

    // MH 腿逐值不动（局部词条没进全局桶）。
    assert!(
        (b_mh.average_hit - p_mh.average_hit).abs() < 1e-9,
        "OH 局部词条不得影响 MH 腿：{} vs {}",
        b_mh.average_hit,
        p_mh.average_hit
    );
    // OH 腿按局部独立乘区 ×1.4。
    let ratio = b_oh.average_hit / p_oh.average_hit;
    assert!(
        (ratio - 1.4).abs() < 1e-6,
        "OH 局部 40% inc 应 ×1.4，实得 ×{ratio}"
    );
}

/// per-hand 武器位路由：`with maces` 词条只进 MH（锤）腿，OH（剑）腿攻速不动
/// （vendor weapon1Cfg/weapon2Cfg flags 按手隔离）。
#[test]
fn with_maces_mod_routes_to_main_hand_only_under_pob2_bits() {
    let data = load_build_data();
    let build = || dual_build(Some(weapon("Shortsword", &[])));

    let plain = calculate_with_data(&build(), &data, &opts(&[])).expect("plain calc");
    let routed = calculate_with_data(
        &build(),
        &data,
        &opts(&["20% increased attack speed with maces"]),
    )
    .expect("routed calc");

    let speed_ratio_mh =
        routed.main_hand.as_ref().unwrap().speed / plain.main_hand.as_ref().unwrap().speed;
    let speed_ratio_oh =
        routed.off_hand.as_ref().unwrap().speed / plain.off_hand.as_ref().unwrap().speed;
    assert!(
        (speed_ratio_mh - 1.2).abs() < 1e-6,
        "MH（锤）腿吃 with-maces 攻速：×{speed_ratio_mh}"
    );
    assert!(
        (speed_ratio_oh - 1.0).abs() < 1e-9,
        "OH（剑）腿不得吃 with-maces 攻速：×{speed_ratio_oh}"
    );
}

// ---------------------------------------------------------------------------
// Golden 基线（M4 §4.2 验收条目 2：双持异种武器 fixture 入 golden 门禁）
//
// 语料 18 build 无双持装备（Weapon2 全为 sceptre/focus/quiver/shield），故用
// 本文件的合成 fixture（Warrior L50 + AxeChopPlayer L1 + MH Wooden Club +
// OH Shortsword，真实入库数据）建基线。基线值 = 建立时（ModFlags 切换 commit
// 后）的实际输出；行为修复改变这些值时按 baseline 纪律以独立 commit 显式更新。
//
// 手算锚点：MH avg_hit = (6+10)/2 × 1.4(baseMultiplier) × 1.05(crit 5%×2.0)
// = 11.76；combined DPS = (MH+OH)/2（非 doubleHits）；Speed = 调和平均。
// ---------------------------------------------------------------------------

const GOLDEN_COMBINED_DPS: f64 = 17.068_250_757;
const GOLDEN_COMBINED_ACTION_RATE: f64 = 1.498_127_341;
const GOLDEN_COMBINED_HIT_AVG: f64 = 11.392_5;
const GOLDEN_MH_DPS: f64 = 17.043_478_257;
const GOLDEN_MH_SPEED: f64 = 1.449_275_362;
const GOLDEN_MH_AVG_HIT: f64 = 11.76;
const GOLDEN_OH_DPS: f64 = 17.093_023_257;
const GOLDEN_OH_SPEED: f64 = 1.550_387_597;
const GOLDEN_OH_AVG_HIT: f64 = 11.025;
const GOLDEN_CRIT_CHANCE: f64 = 0.05;

/// 双持 golden：合并值与 per-hand 子表逐字段锁定（相对容差 1e-6）。
#[test]
fn dual_wield_golden_baseline() {
    let near = |label: &str, expected: f64, actual: f64| {
        let rel = (actual - expected).abs() / expected.abs().max(1e-12);
        assert!(
            rel < 1e-6,
            "{label}: expected {expected}, got {actual}（相对误差 {rel:.2e}）"
        );
    };
    let data = load_build_data();
    let out = calculate_with_data(
        &dual_build(Some(weapon("Shortsword", &[]))),
        &data,
        &opts(&[]),
    )
    .expect("dual wield golden calc");
    let mh = out.main_hand.as_ref().expect("MH 子表");
    let oh = out.off_hand.as_ref().expect("OH 子表");

    near("combined.dps", GOLDEN_COMBINED_DPS, out.dps);
    near(
        "combined.action_rate",
        GOLDEN_COMBINED_ACTION_RATE,
        out.action_rate,
    );
    near(
        "combined.total_hit_avg",
        GOLDEN_COMBINED_HIT_AVG,
        out.total_hit_avg,
    );
    near("combined.crit_chance", GOLDEN_CRIT_CHANCE, out.crit_chance);
    near("mh.total_dps", GOLDEN_MH_DPS, mh.total_dps);
    near("mh.speed", GOLDEN_MH_SPEED, mh.speed);
    near("mh.average_hit", GOLDEN_MH_AVG_HIT, mh.average_hit);
    near("oh.total_dps", GOLDEN_OH_DPS, oh.total_dps);
    near("oh.speed", GOLDEN_OH_SPEED, oh.speed);
    near("oh.average_hit", GOLDEN_OH_AVG_HIT, oh.average_hit);
}
