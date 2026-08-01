//! env_finalize stage 6, end to end: session → perform → output.
//!
//! Per-buff numeric values are already pinned by the `rules::buff_expander` unit tests
//! (PoB2 formula + floor behaviour). This file verifies the **wiring**: buff definitions
//! enter Env via `set_buff_definitions`, the whole stage is gated by `cfg.mode_combat`,
//! expanded mods are written back into the player modDB and participate in aggregation,
//! and `conditions_set` writes `cfg.conditions` so condition-tagged modifiers activate.
//!
//! Key invariant: with `mode_combat` false (the default), injecting the definitions or
//! not must leave every output value unchanged.

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

/// A minimal Onslaught definition (vendor :539-570 basic shape: Speed INC 2e Attack +
/// MovementSpeed INC e, where e = floor(10 × (1 + Σ INC/100))).
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

/// mode_combat=true + the flag set → Onslaught expands (effect=10 → Speed INC 20)
/// and feeds into attack-speed aggregation: action_rate 2.0 × 1.20 = 2.4.
#[test]
fn onslaught_expands_through_perform() {
    let mut session = session_with_onslaught(true);
    let out = session.perform_minimal();
    assert_eq!(out.action_rate, 2.4);
}

/// B3 numeric anchor (end-to-end semantics): OnslaughtEffect 23% + BuffEffectOnSelf 10%
/// → effect = floor(10×1.33) = 13 → Speed INC 26 → action_rate 2.52.
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

/// Key invariant: with mode_combat false (default or explicit), injecting the definitions or not leaves every output value unchanged.
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

/// The trigger flag isn't set → no expansion happens (even with mode_combat=true).
#[test]
fn flag_unset_yields_no_expansion() {
    let mut session =
        CalculationSession::new(input()).with_config(CalcConfig::attack().with_mode_combat(true));
    session.set_buff_definitions(vec![onslaught_def()]);
    let out = session.perform_minimal();
    assert_eq!(out.action_rate, 2.0);
}

/// The conditions_set channel: a buff's attached conditions write into cfg.conditions,
/// activating condition-tagged modifiers (`ModTag::Condition`) within the same perform
/// pass.
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

/// Idempotency guard: repeated perform calls on the same session must not double-count buff expansion.
#[test]
fn repeated_perform_does_not_double_count() {
    let mut session = session_with_onslaught(true);
    let first = session.perform_minimal();
    let second = session.perform_minimal();
    assert_eq!(first.action_rate, 2.4);
    assert_eq!(second.action_rate, 2.4);
}
