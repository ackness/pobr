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

use pobr_core::calc::{CalcError, CalculationSession, MinimalInput};
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
    let out = session.perform_minimal().expect("perform");
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
    let out = session.perform_minimal().expect("perform");
    assert_eq!(out.action_rate, 2.0 * 1.26);
}

/// Key invariant: with mode_combat false (default or explicit), injecting the definitions or not leaves every output value unchanged.
#[test]
fn mode_combat_false_is_value_identical() {
    let mut with_defs = session_with_onslaught(false);
    let out_with_defs = with_defs.perform_minimal().expect("perform");

    let mut without_defs =
        CalculationSession::new(input()).with_config(CalcConfig::attack().with_mode_combat(false));
    without_defs.add_modifiers([Modifier::flag("Onslaught").with_source("test grant")]);
    let out_without_defs = without_defs.perform_minimal().expect("perform");

    assert_eq!(out_with_defs.action_rate, 2.0);
    assert_eq!(out_with_defs, out_without_defs);
}

/// The trigger flag isn't set → no expansion happens (even with mode_combat=true).
#[test]
fn flag_unset_yields_no_expansion() {
    let mut session =
        CalculationSession::new(input()).with_config(CalcConfig::attack().with_mode_combat(true));
    session.set_buff_definitions(vec![onslaught_def()]);
    let out = session.perform_minimal().expect("perform");
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
    let out = session.perform_minimal().expect("perform");
    assert_eq!(
        out.action_rate, 2.2,
        "the HerEmbrace condition should be set and activate the mod"
    );
}

/// Rejected repeat execution must preserve the expanded buff and its output.
#[test]
fn repeated_perform_does_not_double_count() {
    let mut session = session_with_onslaught(true);
    let first = session.perform_minimal().expect("perform");
    let modifier_count = session.all_mods().len();
    assert_eq!(session.perform_minimal(), Err(CalcError::AlreadyPerformed));
    assert_eq!(first.action_rate, 2.4);
    assert_eq!(session.output().action_rate, 2.4);
    assert_eq!(session.all_mods().len(), modifier_count);
}

/// Stage 5 buff-granted keystones and Stage 6 misc buffs can add or activate
/// EnemyModifier entries; the post-Stage-6 forwarding pass delivers them
/// without duplicating entries already forwarded in earlier stages.
#[test]
fn post_stage6_forwards_enemy_modifiers_from_keystones_and_misc_buffs() {
    use pobr_core::ModValue;
    use pobr_core::calc::env_finalize::env_finalize;
    use pobr_core::calc::{Actor, ActorBaseStats, BuffKind, BuffSpec, Env};

    let enemy_mod = |amount: f64| {
        Modifier::new(
            "EnemyModifier",
            ModType::List,
            ModValue::NestedMods(vec![
                Modifier::number("DamageTaken", ModType::Inc, amount)
                    .with_tag(ModTag::condition("Effective", false)),
            ]),
        )
    };

    let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
    env.cfg = CalcConfig::attack()
        .with_mode_combat(true)
        .with_mode_buffs(true)
        .with_mode_effective(true);

    // Stage 2 baseline EnemyModifier (must not be duplicated by later passes).
    env.player.mod_db.add_mod(enemy_mod(10.0));

    // Stage 4 buff grants a keystone whose Stage 5 expansion adds an EnemyModifier.
    env.buff_skills.push(BuffSpec {
        name: "KeystoneGrant".to_string(),
        kind: BuffKind::Buff,
        skill_id: "KeystoneGrantBuff".to_string(),
        mods: vec![Modifier::new(
            "Keystone",
            ModType::List,
            ModValue::Text("TestEnemyKeystone".to_string()),
        )],
        magnitude: 1.0,
        slot: None,
        socket_index: 1,
        is_mark: false,
        ignore_curse_limit: false,
        local_effect_inc: 0.0,
        local_effect_more: 1.0,
        skill_types: pobr_data::skill::SkillTypes::NONE,
    });
    env.keystone_mods
        .insert("TestEnemyKeystone".to_string(), vec![enemy_mod(20.0)]);

    // Stage 6 misc buff sets a condition activating a gated EnemyModifier.
    env.player.mod_db.add_mod(Modifier::flag("Stage6Trigger"));
    env.player
        .mod_db
        .add_mod(enemy_mod(30.0).with_tag(ModTag::condition("Stage6Active", false)));
    env.buff_definitions.push(BuffDef {
        id: "Stage6Trigger".to_string(),
        trigger_flag: "Stage6Trigger".to_string(),
        mode_gate: BuffModeGate::Combat,
        effect: None,
        mods: Vec::new(),
        conditions_set: vec!["Stage6Active".to_string()],
        handler_id: None,
        verified: true,
        vendor_ref: vendor_ref(),
        notes: None,
    });

    env_finalize(&mut env);

    assert_eq!(
        env.enemy
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
        60.0,
        "10 (Stage 2) + 20 (Stage 5 keystone) + 30 (Stage 6 condition), with no duplicates"
    );
}
