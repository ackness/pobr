//! env_finalize 阶段 6端到端：session → perform → 输出。
//!
//! 逐 buff 数值已由 `rules::buff_expander` 单测锚定（PoB2 公式 + floor 行为），
//! 本文件验证**接线**：buff 定义经 `set_buff_definitions` 入 Env，
//! `cfg.mode_combat` 门控整段，展开 mods 写回 player modDB 并参与聚合，
//! `conditions_set` 写 `cfg.conditions` 供条件型词条生效。
//!
//! 关键不变式：`mode_combat` 默认 false → 注入与否输出逐值不变。

use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{CalcConfig, ModTag, Modifier};
use pobr_data::catalog::buffs::{
    BuffDef, BuffEffectFormula, BuffModTemplate, BuffModValue, BuffModeGate, Rounding, VendorRef,
};
use pobr_data::prelude::*;

fn input() -> MinimalInput {
    MinimalInput {
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
    }
}

fn vendor_ref() -> VendorRef {
    VendorRef {
        file: "Modules/CalcPerform.lua".to_string(),
        line_start: 540,
        line_end: 571,
        segment_hash: "fnv1a64:0000000000000000".to_string(),
    }
}

/// Onslaught 最小定义（vendor :539-570 基本形：Speed INC 2e Attack +
/// MovementSpeed INC e，e = floor(10 × (1 + Σ INC/100))）。
fn onslaught_def() -> BuffDef {
    BuffDef {
        id: "Onslaught".to_string(),
        trigger_flag: "Onslaught".to_string(),
        mode_gate: BuffModeGate::Combat,
        effect: Some(BuffEffectFormula {
            base: 10.0,
            inc_stats: vec![
                "OnslaughtEffect".to_string(),
                "BuffEffectOnSelf".to_string(),
            ],
            more_stats: Vec::new(),
            rounding: Rounding::Floor,
            min: None,
            max: None,
        }),
        mods: vec![
            BuffModTemplate {
                name: "Speed".to_string(),
                mod_type: "INC".to_string(),
                value: BuffModValue::PerEffect { coeff: 2.0 },
                flags: vec!["Attack".to_string()],
                tags: Vec::new(),
            },
            BuffModTemplate {
                name: "MovementSpeed".to_string(),
                mod_type: "INC".to_string(),
                value: BuffModValue::PerEffect { coeff: 1.0 },
                flags: Vec::new(),
                tags: Vec::new(),
            },
        ],
        conditions_set: Vec::new(),
        handler_id: None,
        verified: true,
        vendor_ref: vendor_ref(),
        notes: None,
    }
}

fn session_with_onslaught(mode_combat: bool) -> CalculationSession {
    let mut session = CalculationSession::new(input())
        .with_config(CalcConfig::attack().with_mode_combat(mode_combat));
    session.add_modifiers([Modifier::flag("Onslaught").with_source("test grant")]);
    session.set_buff_definitions(vec![onslaught_def()]);
    session
}

/// mode_combat=true + flag 置位 → Onslaught 展开（effect=10 → Speed INC 20）
/// 参与攻速聚合：action_rate 2.0 × 1.20 = 2.4。
#[test]
fn onslaught_expands_through_perform() {
    let mut session = session_with_onslaught(true);
    let out = session.perform_minimal();
    assert_eq!(out.action_rate, 2.4);
}

/// B3 数值锚点（端到端口径）：OnslaughtEffect 23% + BuffEffectOnSelf 10%
/// → effect = floor(10×1.33) = 13 → Speed INC 26 → action_rate 2.52。
#[test]
fn onslaught_effect_scaling_floor_end_to_end() {
    let mut session = session_with_onslaught(true);
    session.add_modifiers([
        Modifier::number("OnslaughtEffect", ModType::Inc, 23.0),
        Modifier::number("BuffEffectOnSelf", ModType::Inc, 10.0),
    ]);
    let out = session.perform_minimal();
    assert_eq!(out.action_rate, 2.0 * 1.26);
}

/// 关键不变式：mode_combat 默认/显式 false → 定义注入与否输出逐值不变。
#[test]
fn mode_combat_false_is_value_identical() {
    let mut with_defs = session_with_onslaught(false);
    let out_with_defs = with_defs.perform_minimal();

    let mut without_defs =
        CalculationSession::new(input()).with_config(CalcConfig::attack().with_mode_combat(false));
    without_defs.add_modifiers([Modifier::flag("Onslaught").with_source("test grant")]);
    let out_without_defs = without_defs.perform_minimal();

    assert_eq!(out_with_defs.action_rate, 2.0);
    assert_eq!(out_with_defs, out_without_defs);
}

/// trigger flag 未置位 → 零展开（mode_combat=true 也不触发）。
#[test]
fn flag_unset_yields_no_expansion() {
    let mut session =
        CalculationSession::new(input()).with_config(CalcConfig::attack().with_mode_combat(true));
    session.set_buff_definitions(vec![onslaught_def()]);
    let out = session.perform_minimal();
    assert_eq!(out.action_rate, 2.0);
}

/// conditions_set 通道：buff 附带条件写 cfg.conditions，使条件型词条
/// （`ModTag::Condition`）在同一次 perform 内生效。
#[test]
fn conditions_set_activates_condition_tagged_mods() {
    let def = BuffDef {
        id: "HerEmbrace".to_string(),
        trigger_flag: "HerEmbrace".to_string(),
        mode_gate: BuffModeGate::Combat,
        effect: None,
        mods: Vec::new(),
        conditions_set: vec!["HerEmbrace".to_string()],
        handler_id: None,
        verified: true,
        vendor_ref: vendor_ref(),
        notes: None,
    };
    let mut session =
        CalculationSession::new(input()).with_config(CalcConfig::attack().with_mode_combat(true));
    session.add_modifiers([
        Modifier::flag("HerEmbrace").with_source("test grant"),
        Modifier::number("AttackSpeed", ModType::Inc, 10.0)
            .with_tag(ModTag::condition("HerEmbrace", false)),
    ]);
    session.set_buff_definitions(vec![def]);
    let out = session.perform_minimal();
    assert_eq!(out.action_rate, 2.2, "HerEmbrace 条件应被置位并激活该词条");
}

/// 幂等护栏：同一 session 重复 perform，buff 展开不重复计入。
#[test]
fn repeated_perform_does_not_double_count() {
    let mut session = session_with_onslaught(true);
    let first = session.perform_minimal();
    let second = session.perform_minimal();
    assert_eq!(first.action_rate, 2.4);
    assert_eq!(second.action_rate, 2.4);
}
