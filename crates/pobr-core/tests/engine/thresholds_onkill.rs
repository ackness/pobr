use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{CalcConfig, Modifier};
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
