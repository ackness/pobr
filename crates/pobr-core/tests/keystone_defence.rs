//! M2 Track C 集成测试：keystone_registry + CI 接线 + 防御资源转换矩阵。
//!
//! C-1：`DefenceKeystones::from_db` 契约（一次性快照、词条名即接口）。
//! 蓝图 m2-defence §2 Track C / §3.3 契约 2：E/D/B/F 以参数消费本结构，
//! 禁止各 track 散读 keystone flag。

use pobr_core::{CalcConfig, DefenceKeystones, ModDb, Modifier};
use pobr_data::prelude::*;

/// 从词条文本经 mod_parser 解析后驱动注册表（端到端：文本 → flag → 快照）。
///
/// `Maximum Life is 1`（Chaos Inoculation 节点词条）→ `ChaosInoculation` flag
/// （mod_parser keystone special 段）；`Converts all Energy Shield to Mana`
/// （Eldritch Battery 型）→ `EnergyShieldConvertToMana` BASE 100 → 全转换开关。
#[test]
fn keystones_from_parsed_mod_texts() {
    // Arrange
    let mut db = ModDb::new();
    for text in ["Maximum Life is 1", "Converts all Energy Shield to Mana"] {
        let outcome = pobr_core::mod_parser::parse_mod(text).expect("解析失败");
        db.add_list(outcome.mods);
    }
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.chaos_inoculation, "CI 词条应驱动 chaos_inoculation");
    assert!(
        ks.eldritch_battery_es_to_mana,
        "全转换词条应驱动 eldritch_battery_es_to_mana"
    );
    // 未出现的 keystone 保持关闭。
    assert!(!ks.unbreakable);
    assert!(!ks.energy_shield_to_ward);
}

/// W0.1 词条 `Energy Shield protects Mana instead of Life`（EB flag，
/// ModParser.lua:2439）端到端驱动 `energy_shield_protects_mana`。
#[test]
fn eb_flag_from_parsed_text() {
    // Arrange
    let mut db = ModDb::new();
    let outcome = pobr_core::mod_parser::parse_mod("Energy Shield protects Mana instead of Life")
        .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.energy_shield_protects_mana);
    assert!(
        !ks.eldritch_battery_es_to_mana,
        "EB flag 不等于 ES→Mana 全转换"
    );
}

/// IronReflexes 词条（`Converts all Evasion Rating to Armour`，ModParser.lua:2343）
/// 同时产出 flag（联动用）与 `EvasionConvertToArmour` BASE 100（矩阵数据通道）。
#[test]
fn iron_reflexes_flag_and_matrix_data_coexist() {
    // Arrange
    let mut db = ModDb::new();
    let outcome = pobr_core::mod_parser::parse_mod("Converts all Evasion Rating to Armour")
        .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);
    let conv = db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("EvasionConvertToArmour")],
    );

    // Assert：flag 仅供 Unbreakable 联动；数据展开走转换矩阵（BASE 100）。
    assert!(ks.iron_reflexes);
    assert_eq!(conv, 100.0);
}

/// 快照语义：直接注入 flag Modifier 的最小路径（树 ingest 等非文本来源）。
#[test]
fn keystones_from_injected_flags() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        Modifier::flag("Unbreakable"),
        Modifier::flag("DoubleBodyArmourDefence"),
        Modifier::flag("WardNotBreak"),
        Modifier::flag("EternalLife"),
        Modifier::flag("BloodMagic"),
    ]);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.unbreakable);
    assert!(ks.double_body_armour_defence);
    assert!(ks.ward_not_break);
    assert!(ks.eternal_life);
    assert!(ks.blood_magic);
    assert!(!ks.chaos_inoculation);
}

// ─────────────────────────────────────────────────────────────────
// C-2：CI 接线（perform.rs EhpOptions.chaos_inoculation 不再写死 false）
// ─────────────────────────────────────────────────────────────────

/// CI build 端到端：`Maximum Life is 1` → Life=1 + ChaosInoculation flag →
/// EHP 走 ES 池、混沌 max hit = ∞。
///
/// vendor 依据：CalcDefence.lua:85（flag 读出）/:120-123（Life=1）；CI 免疫混沌
/// （agent-docs/active-defences.md §五 Keystone 表；EhpOptions::chaos_inoculation 语义）。
/// 修复前（C-2 之前）该 flag 在 perform 写死 false：chaos_max_hit 为有限值
/// （1 + 500×0.5 = 251 的混沌池），EHP 池仍按 Life 口径。
#[test]
fn ci_build_ehp_uses_es_pool_and_chaos_immunity() {
    use pobr_core::calc::{CalculationSession, MinimalInput};

    // Arrange：1000 基础生命 + 500 ES + CI。
    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 2.0,
    };
    let mut session = CalculationSession::new(input);
    session
        .add_modifier_texts(["+500 to maximum Energy Shield", "Maximum Life is 1"])
        .unwrap();

    // Act
    session.perform_minimal();
    let output = session.output().clone();

    // Assert：CI → Life Override 1；混沌免疫；火 max hit = ES 池 / (1−0%) = 500。
    assert_eq!(output.life, 1.0, "CI 应把最大生命覆盖为 1");
    assert_eq!(output.energy_shield, 500.0);
    assert!(
        output.chaos_max_hit.is_infinite(),
        "CI 应使混沌 max hit = ∞（实际 {}）",
        output.chaos_max_hit
    );
    assert_eq!(output.fire_max_hit, 500.0, "CI 下元素池 = ES（500）");
    assert!(output.total_ehp.is_finite());
}
