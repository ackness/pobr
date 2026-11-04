//! Reverie and Azmeri Brew: bounded flask recovery, without a use timeline.

use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::{catalog::UtilityBaseStats, prelude::*};

fn flask(life: bool, rarity: ItemRarity, lines: &[&str]) -> (Item, UtilityBaseStats) {
    (
        Item {
            base: if life {
                "Ultimate Life Flask"
            } else {
                "Ultimate Mana Flask"
            }
            .into(),
            rarity,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: lines.iter().map(|line| (*line).into()).collect(),
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            socketed: vec![],
            parsed_stats: vec![],
        },
        // Deliberately equal fixture amounts make the direction of modifiers visible.
        UtilityBaseStats {
            life: if life { 1000.0 } else { 0.0 },
            mana: if life { 0.0 } else { 1000.0 },
            duration: 4.0,
            charges_max: 75.0,
            charges_used: 10.0,
        },
    )
}

fn near(session: &CalculationSession, stat: &str, expected: f64) {
    let actual = session.output().utility_stats[stat];
    assert!(
        (actual - expected).abs() < 1e-8,
        "{stat}: {actual} != {expected}"
    );
}

#[test]
fn less_life_flask_recovery_does_not_reduce_maximum_life_or_mana_flasks() {
    let mut session = super::support::session(MinimalInput {
        base_life: 2000.0,
        ..Default::default()
    });
    for (slot, life) in [("Flask 1", true), ("Flask 2", false)] {
        let (item, base) = flask(life, ItemRarity::Magic, &[]);
        session.add_utility_base(slot, &item, &base);
        session.add_flask_charm(slot, &item);
    }
    session
        .add_modifier_texts(&["45% less Life Flask Recovery"])
        .unwrap();
    session.perform_minimal().unwrap();
    near(&session, "Flask1Recovery", 550.0);
    near(&session, "Flask2Recovery", 1000.0);
    assert_eq!(session.output().life, 2000.0);
    assert!(session.unsupported_modifier_texts().is_empty());
}

const CANNOT_USE: &str = "Cannot use Life Flasks";
const CONSTANT: &str = "Non-Unique Life Flasks apply their Effects constantly";
const NO_INSTANT: &str = "Recovery from Life Flasks cannot be Instant";
const SELF_ONLY: &str =
    "Recovery from your Life Flasks cannot be applied to anything other than you";

#[test]
fn constant_life_flask_effect_respects_rarity_selection_use_restriction_and_player_gate() {
    // Only the explicit constant rule can admit an unselected Life flask.
    for (rarity, selected, constant, cannot_use, combat, redirected, expected) in [
        (ItemRarity::Normal, false, true, true, true, false, 40.0),
        (ItemRarity::Magic, false, true, true, true, false, 40.0),
        (ItemRarity::Rare, true, true, true, true, false, 40.0),
        (ItemRarity::Unique, false, true, true, true, false, 0.0),
        (ItemRarity::Unique, true, true, true, true, false, 0.0),
        (ItemRarity::Unique, true, true, false, true, false, 40.0),
        (ItemRarity::Magic, false, false, false, true, false, 0.0),
        (ItemRarity::Magic, true, false, true, true, false, 0.0),
        (ItemRarity::Magic, true, false, false, true, false, 40.0),
        (ItemRarity::Magic, false, true, true, false, false, 0.0),
        (ItemRarity::Magic, false, true, true, true, true, 0.0),
    ] {
        let mut session = super::support::session(MinimalInput::default())
            .with_config(CalcConfig::default().with_mode_combat(combat));
        let (item, base) = flask(true, rarity, &["+40% to Fire Resistance during effect"]);
        session.add_utility_base("Flask 1", &item, &base);
        if selected {
            session.add_flask_charm("Flask 1", &item);
        } else {
            session.add_inactive_utility_properties("Flask 1", &item);
        }
        // Same slot infrastructure, but Mana flasks never gain Reverie's constant effect.
        let (mana, base) = flask(
            false,
            ItemRarity::Magic,
            &["+30% to Cold Resistance during effect"],
        );
        session.add_utility_base("Flask 2", &mana, &base);
        session.add_inactive_utility_properties("Flask 2", &mana);
        for (enabled, line) in [
            (constant, CONSTANT),
            (cannot_use, CANNOT_USE),
            (redirected, "Flasks do not apply to you"),
        ] {
            if enabled {
                session.add_modifier_texts(&[line]).unwrap();
            }
        }
        session.perform_minimal().unwrap();
        assert_eq!(
            session.base_sum("FireResistance"),
            expected,
            "{rarity:?}, selected={selected}, constant={constant}, cannot_use={cannot_use}, combat={combat}, redirected={redirected}"
        );
        assert_eq!(session.base_sum("ColdResistance"), 0.0);
        assert_eq!(session.config().condition("UsingLifeFlask"), expected > 0.0);
        near(&session, "Flask1Uses", if cannot_use { 0.0 } else { 7.0 });
        near(&session, "Flask2Uses", 7.0);
        // Properties are inspectable even when this flask cannot be used.
        near(&session, "Flask1Recovery", 1000.0);
        assert_eq!(session.output().life_regen, 0.0);
        assert!(session.unsupported_modifier_texts().is_empty());
    }
}

#[test]
fn azmeri_copies_original_recovery_once_with_the_receivers_rate_and_source_instant_rule() {
    // Both Azmeri directions together must not amplify each other or feed the ES copy.
    for no_instant in [false, true] {
        let mut session = super::support::session(MinimalInput::default());
        for (slot, life) in [("Flask 1", true), ("Flask 2", false)] {
            let (item, base) = flask(
                life,
                ItemRarity::Magic,
                &["50% of Recovery applied Instantly"],
            );
            session.add_utility_base(slot, &item, &base);
            session.add_inactive_utility_properties(slot, &item);
        }
        session
            .add_modifier_texts(&[
                "Life Flasks also recover Mana",
                "Mana Flasks also recover Life",
                "Life Recovery from Flasks also applies to Energy Shield",
                "45% less Life Flask Recovery",
                "20% of Flask Recovery applied Instantly",
                CANNOT_USE,
                CONSTANT,
                SELF_ONLY,
            ])
            .unwrap();
        if no_instant {
            session.add_modifier_texts(&[NO_INSTANT]).unwrap();
        }
        session.add_modifiers([
            Modifier::number("LifeRecoveryRate", ModType::Inc, 50.0),
            Modifier::number("ManaRecoveryRate", ModType::Inc, 25.0),
            Modifier::number("EnergyShieldRecoveryRate", ModType::Inc, 100.0),
        ]);
        session.perform_minimal().unwrap();
        near(
            &session,
            "Flask1Recovery",
            if no_instant { 825.0 } else { 632.5 },
        );
        near(
            &session,
            "Flask1InstantRecovery",
            if no_instant { 0.0 } else { 385.0 },
        );
        near(
            &session,
            "Flask1ManaRecovery",
            if no_instant { 687.5 } else { 591.25 },
        );
        near(
            &session,
            "Flask1ManaInstantRecovery",
            if no_instant { 0.0 } else { 385.0 },
        );
        near(
            &session,
            "Flask1ManaRecoveryPerSecond",
            if no_instant { 171.875 } else { 51.5625 },
        );
        near(
            &session,
            "Flask1EnergyShieldRecovery",
            if no_instant { 1100.0 } else { 715.0 },
        );
        near(&session, "Flask2Recovery", 1075.0);
        near(&session, "Flask2LifeRecovery", 1150.0);
        near(&session, "Flask2LifeInstantRecovery", 700.0);
        near(&session, "Flask2LifeRecoveryPerSecond", 112.5);
        near(&session, "Flask1LifeRecovery", 0.0);
        near(&session, "Flask2ManaRecovery", 0.0);
        near(&session, "Flask2EnergyShieldRecovery", 0.0);
        assert!(session.derived_detail("Flask1ManaRecovery").is_some());
        assert!(
            session
                .derived_detail("Flask2LifeRecoveryPerSecond")
                .is_some()
        );
        assert_eq!(session.output().life_regen, 0.0);
        assert_eq!(session.output().mana_regen, 0.0);
    }
}

#[test]
fn azmeri_receiver_restrictions_do_not_block_the_other_resource() {
    for (restriction, life_copy, mana_copy) in [
        ("CannotGainLife", 0.0, 1000.0),
        ("CannotRecoverLifeOutsideLeech", 0.0, 1000.0),
        ("CannotGainMana", 1000.0, 0.0),
    ] {
        let mut session = super::support::session(MinimalInput::default());
        for (slot, life) in [("Flask 1", true), ("Flask 2", false)] {
            let (item, base) = flask(
                life,
                ItemRarity::Magic,
                &["50% of Recovery applied Instantly"],
            );
            session.add_utility_base(slot, &item, &base);
            session.add_flask_charm(slot, &item);
        }
        session
            .add_modifier_texts(&["Life Flasks also recover Mana\nMana Flasks also recover Life"])
            .unwrap();
        session.add_modifiers([Modifier::flag(restriction)]);
        session.perform_minimal().unwrap();
        near(&session, "Flask1ManaRecovery", mana_copy);
        near(&session, "Flask2LifeRecovery", life_copy);
        near(&session, "Flask1ManaInstantRecovery", mana_copy * 0.5);
        near(&session, "Flask2LifeInstantRecovery", life_copy * 0.5);
    }
}

#[test]
fn self_only_and_whole_reverie_block_keep_explicit_unmodelled_diagnostics() {
    for text in [
        SELF_ONLY.to_string(),
        [CANNOT_USE, CONSTANT, NO_INSTANT, SELF_ONLY].join("\n"),
    ] {
        let parsed = super::support::parse_mod(&text).unwrap();
        assert!(parsed.mods.is_empty(), "must not partially apply {text}");
        let meta = parsed.special_meta.expect("explicit unmodelled rule");
        assert_eq!(meta.entry_id, "life_flask_recovery_only_to_you_unmodelled");
        assert!(
            !meta.no_effect,
            "recipient restriction must not be declared effectless"
        );
    }
}
