use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;

fn session(lines: &[&str]) -> CalculationSession {
    let mut session = super::support::session(MinimalInput {
        base_life: 1000.0,
        base_mana: 500.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..Default::default()
    })
    .with_config(CalcConfig::spell().with_mode_effective(true));
    session.add_modifier_texts(lines).unwrap();
    session
}

#[test]
fn ailment_threshold_adds_arbitrary_es_rolls_before_increases() {
    let mut session = session(&[
        "+2000 to maximum Energy Shield",
        "Gain additional Ailment Threshold equal to 17% of maximum Energy Shield",
        "40% increased Elemental Ailment Threshold",
    ]);
    session.perform_minimal().unwrap();
    // Game keyword: half Life, plus 17% ES, then 40% increased threshold.
    // (1000 * 0.5 + 2000 * 0.17) * 1.4 = 1176.
    assert_eq!(session.output().energy_shield, 2000.0);
    assert!((display_value(session.output(), "AilmentThreshold") - 1176.0).abs() < 1e-9);
    assert!(session.unsupported_modifier_texts().is_empty());
    assert_eq!(session.detail_sources("AilmentThreshold")[0].rows.len(), 2);
}

#[test]
fn on_kill_recovery_adds_flat_and_percent_of_final_maximum_pools() {
    let mut session = session(&[
        "+2000 to maximum Energy Shield",
        "7 Life per Enemy Killed",
        "11 Mana per Enemy Killed",
        "13 Energy Shield per Enemy Killed",
        "Recover 2% of maximum Life on Kill",
        "Recover 3% of maximum Mana on Kill",
        "Recover 4% of maximum Energy Shield on Kill",
        "50% increased maximum Life",
        "100% increased maximum Mana",
    ]);
    session.perform_minimal().unwrap();
    // 7 + 1500*.02 = 37; 11 + 1000*.03 = 41; 13 + 2000*.04 = 93.
    for (id, expected) in [
        ("LifeOnKill", 37.0),
        ("ManaOnKill", 41.0),
        ("EnergyShieldOnKill", 93.0),
    ] {
        assert_eq!(display_value(session.output(), id), expected, "{id}");
        assert!(
            session
                .detail_sources(id)
                .iter()
                .any(|group| !group.rows.is_empty())
        );
    }
    assert!(
        session.unsupported_modifier_texts().is_empty(),
        "{:?}",
        session.unsupported_modifier_texts()
    );
}

#[test]
fn on_kill_recovery_respects_resource_gain_restrictions() {
    for restriction in [
        "CannotGainLife",
        "CannotRecoverLifeOutsideLeech",
        "CannotGainMana",
        "CannotGainEnergyShield",
    ] {
        let mut session = session(&[
            "+2000 to maximum Energy Shield",
            "Recover 2% of maximum Life on Kill",
            "Recover 3% of maximum Mana on Kill",
            "Recover 4% of maximum Energy Shield on Kill",
        ]);
        session.add_modifiers([Modifier::flag(restriction)]);
        session.perform_minimal().unwrap();
        let out = session.output();
        assert_eq!(
            out.life_on_kill,
            if restriction.contains("Life") {
                0.0
            } else {
                20.0
            }
        );
        assert_eq!(
            out.mana_on_kill,
            if restriction.contains("Mana") {
                0.0
            } else {
                15.0
            }
        );
        assert_eq!(
            out.energy_shield_on_kill,
            if restriction.contains("EnergyShield") {
                0.0
            } else {
                80.0
            }
        );
    }
}

#[test]
fn freeze_buildup_reads_the_game_text_modifier() {
    let db = {
        let mut session = session(&[
            "50% increased Freeze Buildup",
            "20% increased Immobilisation Buildup",
        ]);
        // Test the same traced query used by perform, with a 1000 poise threshold.
        session.add_modifiers([Modifier::number("FreezeBuildup", ModType::More, 20.0)]);
        session.mod_db().clone()
    };
    let (buildup, _) = pobr_core::calc::ailment::freeze_poise_buildup_traced(
        1000.0,
        &db,
        &CalcConfig::spell(),
        &mut pobr_core::TraceGraph::new(),
    );
    // 2.1 / 1000 * (1 + .5 + .2) * 1.2 * 100 = .4284% per cold damage.
    assert!((buildup - 0.4284).abs() < 1e-9);
}

#[test]
fn daze_chance_is_a_capped_hit_chance() {
    for (extra, expected) in [(15.0, 25.0), (150.0, 100.0)] {
        let mut session = session(&["10% chance to Daze on Hit"]);
        session.add_modifiers([Modifier::number("DazeChance", ModType::Base, extra)]);
        session.perform_minimal().unwrap();
        assert_eq!(display_value(session.output(), "DazeChance"), expected);
    }
}
