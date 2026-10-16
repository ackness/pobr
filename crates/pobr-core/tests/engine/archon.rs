use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;

fn run(active: bool, lines: &[&str]) -> CalculationSession {
    let mut session = super::support::session(MinimalInput::default()).with_config(
        CalcConfig::new()
            .with_mode_combat(true)
            .with_condition("AffectedByArchon", active),
    );
    session.add_modifier_texts(lines).unwrap();
    assert!(session.unsupported_modifier_texts().is_empty());
    session.perform_minimal().unwrap();
    session
}

#[test]
fn invigorating_archon_scales_its_grants_but_not_energising_recharge() {
    // Game passive_tree: Invigorating = 10% movement, +20% elemental resist;
    // Dominion = 50% reduced Archon effect; Energising = 20% faster recharge.
    let lines = [
        "Archon Buffs also grant 10% increased Movement Speed",
        "Archon Buffs also grant +20% to all Elemental Resistances",
        "50% reduced effect of Archon Buffs on you",
        "20% faster start of Energy Shield Recharge while affected by an Archon Buff",
        "+100 to maximum Energy Shield",
    ];
    let active = run(true, &lines);
    let inactive = run(false, &lines);
    assert!((display_value(active.output(), "EffectiveMovementSpeedMod") - 1.05).abs() < 1e-9);
    assert_eq!(
        display_value(inactive.output(), "EffectiveMovementSpeedMod"),
        1.0
    );
    for stat in ["FireResist", "ColdResist", "LightningResist"] {
        assert_eq!(display_value(active.output(), stat), 10.0);
        assert_eq!(display_value(inactive.output(), stat), 0.0);
    }
    // This conditional bonus is not itself granted by the Archon buff.
    assert!((display_value(active.output(), "EsRechargeDelay") - 4.0 / 1.2).abs() < 1e-8);
    assert_eq!(display_value(inactive.output(), "EsRechargeDelay"), 4.0);
}

#[test]
fn archon_timing_uses_game_keyword_bases_and_dominion_removes_recovery() {
    let base = run(false, &[]);
    assert_eq!(display_value(base.output(), "ArchonDuration"), 10.0);
    assert_eq!(display_value(base.output(), "ArchonRecoveryPeriod"), 20.0);
    // Energising Archon: +30% duration; Bhatair's Storm: 10% faster recovery.
    let timed = run(
        false,
        &[
            "30% increased Archon Buff duration",
            "Archon recovery period expires 10% faster",
        ],
    );
    assert_eq!(display_value(timed.output(), "ArchonDuration"), 13.0);
    assert!((display_value(timed.output(), "ArchonRecoveryPeriod") - 20.0 / 1.1).abs() < 1e-9);
    assert!(timed.derived_detail("ArchonDuration").is_some());
    assert!(timed.derived_detail("ArchonRecoveryPeriod").is_some());
    let dominion = run(
        true,
        &[
            "Archon Buffs have no recovery period after you lose one",
            "Archon recovery period expires 30% slower",
        ],
    );
    assert_eq!(
        display_value(dominion.output(), "ArchonRecoveryPeriod"),
        0.0
    );
}

#[test]
fn archon_effect_combines_with_buff_effect_and_command_chance_does_not_activate_it() {
    let mut session = super::support::session(MinimalInput::default()).with_config(
        CalcConfig::new()
            .with_mode_combat(true)
            .with_condition("AffectedByArchon", true),
    );
    session
        .add_modifier_texts(&[
            "Archon Buffs also grant 10% increased Movement Speed",
            "10% increased effect of Archon Buffs on you",
        ])
        .unwrap();
    session.add_modifiers(vec![Modifier::number(
        "BuffEffectOnSelf",
        ModType::Inc,
        20.0,
    )]);
    session.perform_minimal().unwrap();
    assert!((display_value(session.output(), "EffectiveMovementSpeedMod") - 1.13).abs() < 1e-9);
    let chance = run(
        false,
        &[
            "15% chance to gain Archon of Undeath when you use a Command skill",
            "Archon Buffs also grant 10% increased Movement Speed",
        ],
    );
    assert_eq!(
        display_value(chance.output(), "EffectiveMovementSpeedMod"),
        1.0
    );
}
