//! M4-T2 W-B2：MH/OH hand pass 集成测试。
//!
//! 重点 = I3 单手等价性（蓝图 §2 W-B2 测试计划①：单 HandSource 输出与
//! 「武器基底折进 MinimalInput 后单跑」**逐值相等**——这是双 pass 回退开关的
//! 正确性证明）+ 双持合成 fixture 手算对拍（②）+ doubleHits 不除 2（④）。
//! vendor 参照 CalcOffence.lua:2369-2449（passList）/ :2451-2545（combineStat）。

use pobr_core::calc::{
    HandSource, MinimalInput, WeaponBase, calculate_minimal_vs_enemy, combine_mode_for,
    run_hand_passes,
};
use pobr_core::{CalcConfig, CombineMode, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

fn attack_db() -> ModDb {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0));
    db.add_mod(Modifier::number("AttackSpeed", ModType::Inc, 20.0));
    db.add_mod(Modifier::number("Accuracy", ModType::Base, 500.0));
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        30.0,
    ));
    db.add_mod(Modifier::number("MaximumLife", ModType::Base, 40.0));
    db
}

fn base_input() -> MinimalInput {
    MinimalInput {
        base_life: 100.0,
        base_mana: 50.0,
        base_accuracy: 100.0,
        enemy_evasion: 400.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    }
}

/// I3-空：`passes = []` 与直接调用 `calculate_minimal_vs_enemy` 逐值相等，
/// per-hand 子表为 None（非攻击技能单 "Skill" pass）。
#[test]
fn empty_passes_passthrough_is_identity() {
    let db = attack_db();
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();

    let direct = calculate_minimal_vs_enemy(&db, &enemy, &cfg, &input);
    let via_pass = run_hand_passes(&db, &enemy, &cfg, &[], &input, false);

    assert_eq!(via_pass.combined, direct, "空 passes 必须逐值直通");
    assert!(via_pass.main_hand.is_none());
    assert!(via_pass.off_hand.is_none());
}

/// I3-单手：单 MainHand HandSource 与「武器折进 input 后单跑」逐值相等
/// （vendor `not bothWeaponAttack` → 全部 stat OR 直通，:2453）。
#[test]
fn single_hand_source_equals_legacy_input_fold_per_value() {
    let db = attack_db();
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();

    let weapon = WeaponBase {
        hit_min: 12.0,
        hit_max: 30.0,
        attack_rate: Some(1.4),
        crit_chance: 5.0,
    };

    // 旧折算口径：直接把武器基底写进 MinimalInput 再单跑。
    let mut folded = input;
    folded.base_hit_min += weapon.hit_min;
    folded.base_hit_max += weapon.hit_max;
    folded.base_action_rate = 1.4;
    let legacy = calculate_minimal_vs_enemy(&db, &enemy, &cfg, &folded);

    let out = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    );

    // 逐值相等（MinimalOutput derive PartialEq，含 breakdown 与 damage_components）。
    assert_eq!(out.combined, legacy, "单手 OR 直通必须与旧折算逐值相等");
    let mh = out.main_hand.expect("MainHand 子表");
    assert!(out.off_hand.is_none());
    assert_eq!(mh.total_dps, legacy.dps);
    assert_eq!(mh.speed, legacy.action_rate);
    assert_eq!(mh.crit_chance, legacy.crit_chance);
    assert_eq!(mh.average_hit, legacy.total_hit_avg);
    assert_eq!(mh.damage_components, legacy.damage_components);
    // Stored 族 W-B3 落值前为空（HandOutput 冻结字段集占位）。
    assert!(mh.stored_combined_avg.is_empty());
}

/// 单手 + per-hand 条件词条：`MainHandAttack` 条件词条在 MainHand pass 内生效
/// （PoB2 weapon1Cfg 等价）。
#[test]
fn main_hand_condition_mod_applies_inside_main_hand_pass() {
    let mut db = attack_db();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("MainHandAttack", false)),
    );
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();
    let weapon = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
    };

    let without_condition = run_hand_passes(
        &attack_db(),
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    );
    let with_condition = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(weapon)],
        &input,
        false,
    );
    assert!(
        with_condition.combined.total_hit_avg > without_condition.combined.total_hit_avg,
        "MainHandAttack 条件词条必须在主手 pass 内生效"
    );
}

/// 双持合成 fixture（蓝图测试计划②）：MH 锤 + OH 剑、per-hand 词条只进对应手，
/// 合并值逐模式手算对拍（AVERAGE 命中 / HARMONICMEAN 速度 / DPS 伤害 / CRIT 暴击）。
#[test]
fn dual_wield_combines_per_vendor_modes_hand_calculated() {
    let mut db = attack_db();
    // per-hand 词条：只放大主手（条件路由——W-A1 武器位落地前的通道）。
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("MainHandAttack", false)),
    );
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();

    let mace = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
    };
    let sword = WeaponBase {
        hit_min: 30.0,
        hit_max: 40.0,
        attack_rate: Some(2.0),
        crit_chance: 5.0,
    };

    let out = run_hand_passes(
        &db,
        &enemy,
        &cfg,
        &[HandSource::main_hand(mace), HandSource::off_hand(sword)],
        &input,
        false,
    );
    let mh = out.main_hand.as_ref().expect("MH 子表");
    let oh = out.off_hand.as_ref().expect("OH 子表");

    // per-hand 词条路由：MH 腿吃 +100% inc（150%+100%），OH 腿只有 150%。
    // MH base avg = 15 × 2.5 = 37.5…（×暴击因子）；OH base avg = 35 × 1.5。
    assert!(
        mh.average_hit / 15.0 > oh.average_hit / 35.0,
        "MainHandAttack 词条只得进 MH 腿（MH={} OH={}）",
        mh.average_hit,
        oh.average_hit
    );

    // 合并手算（vendor :2451-2545）：
    let avg = |a: f64, b: f64| (a + b) / 2.0;
    let eps = 1e-9;
    // HitChance：AVERAGE（:3024）。
    assert!((out.combined.hit_chance - avg(mh.hit_chance, oh.hit_chance)).abs() < eps);
    // Speed：HARMONICMEAN（:3026）。
    let harmonic = 2.0 / (1.0 / mh.speed + 1.0 / oh.speed);
    assert!((out.combined.action_rate - harmonic).abs() < 1e-6);
    // AverageDamage / TotalDPS：DPS 非 doubleHits → (MH+OH)/2（:2541-2545）。
    assert!((out.combined.total_hit_avg - avg(mh.average_hit, oh.average_hit)).abs() < eps);
    assert!((out.combined.dps - avg(mh.total_dps, oh.total_dps)).abs() < eps);
    // CritChance：CRIT 非 doubleHits → (MH+OH)/2（:2459-2464；fraction 折算不变）。
    assert!((out.combined.crit_chance - avg(mh.crit_chance, oh.crit_chance)).abs() < eps);
    // CritMultiplier：AVERAGE（:4557）。
    assert!(
        (out.combined.crit_multiplier - avg(mh.crit_multiplier, oh.crit_multiplier)).abs() < eps
    );
    // 防御族不分手：与单跑一致。
    assert_eq!(out.combined.life, 140.0, "100+40，防御族与 hand pass 无关");
}

/// doubleHits（蓝图测试计划④）：`doubleHitsWhenDualWielding` 技能 DPS = MH + OH
/// 不除 2（:2541-2545）；CritChance 走 doubleHits 交叉项公式（:2459-2461）。
#[test]
fn double_hits_dps_is_sum_not_halved() {
    let db = attack_db();
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let input = base_input();
    let mace = WeaponBase {
        hit_min: 10.0,
        hit_max: 20.0,
        attack_rate: Some(1.0),
        crit_chance: 5.0,
    };
    let sword = WeaponBase {
        hit_min: 30.0,
        hit_max: 40.0,
        attack_rate: Some(2.0),
        crit_chance: 5.0,
    };
    let passes = [HandSource::main_hand(mace), HandSource::off_hand(sword)];

    let normal = run_hand_passes(&db, &enemy, &cfg, &passes, &input, false);
    let double = run_hand_passes(&db, &enemy, &cfg, &passes, &input, true);

    let mh = double.main_hand.as_ref().unwrap();
    let oh = double.off_hand.as_ref().unwrap();
    let eps = 1e-9;
    assert!((double.combined.dps - (mh.total_dps + oh.total_dps)).abs() < eps);
    assert!(
        (normal.combined.dps - (mh.total_dps + oh.total_dps) / 2.0).abs() < eps,
        "非 doubleHits 除 2"
    );
    // CritChance doubleHits：MH% + OH% − MH%×OH%/100（百分数空间）。
    let mh_pct = mh.crit_chance * 100.0;
    let oh_pct = oh.crit_chance * 100.0;
    let expected = (mh_pct + oh_pct - mh_pct * oh_pct / 100.0) / 100.0;
    assert!((double.combined.crit_chance - expected).abs() < 1e-9);
}

/// COMBINE_TABLE 锁定（vendor 调用面照抄；行号见 hand_pass.rs 表注释）。
#[test]
fn combine_table_modes_match_vendor_call_sites() {
    assert_eq!(
        combine_mode_for("HitChance", false),
        Some(CombineMode::Average)
    );
    assert_eq!(
        combine_mode_for("Speed", false),
        Some(CombineMode::HarmonicMean)
    );
    assert_eq!(combine_mode_for("HitSpeed", false), Some(CombineMode::Or));
    assert_eq!(
        combine_mode_for("CritChance", true),
        Some(CombineMode::Crit { double_hits: true })
    );
    assert_eq!(
        combine_mode_for("CritMultiplier", true),
        Some(CombineMode::Average)
    );
    assert_eq!(
        combine_mode_for("TotalDPS", true),
        Some(CombineMode::Dps { double_hits: true })
    );
    assert_eq!(
        combine_mode_for("AverageDamage", false),
        Some(CombineMode::Dps { double_hits: false })
    );
    assert_eq!(combine_mode_for("NotAStat", false), None);
}
