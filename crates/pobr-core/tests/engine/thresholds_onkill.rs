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
