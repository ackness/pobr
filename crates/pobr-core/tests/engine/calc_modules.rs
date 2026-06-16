use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::breakdown::BreakdownTable;
use pobr_core::calc::defence::{armour_reduction, calc_defence, hit_chance};
use pobr_core::calc::env::Env;
use pobr_core::calc::offence::{MinimalInput, calculate_minimal};
use pobr_core::calc::output::OutputTable;
use pobr_core::calc::perform::perform;
use pobr_core::{CalcConfig, ModDb};
use pobr_data::prelude::*;

#[test]
fn split_calc_modules_expose_the_expected_core_entrypoints() {
    let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
    perform(&mut env).unwrap();

    let output = calculate_minimal(
        &ModDb::new(),
        &CalcConfig::attack(),
        &MinimalInput::default(),
    );

    assert_eq!(OutputTable::default().dps, 0.0);
    assert!(BreakdownTable::default().is_empty());
    assert_eq!(hit_chance(0.0, 0.0), 1.0);
    assert_eq!(armour_reduction(0.0, 100.0), 0.0);
    assert_eq!(output.dps, 0.0);
}

#[test]
fn split_defence_module_calculates_defence_output_directly() {
    let mut actor = Actor::new(
        1,
        ActorBaseStats {
            armour: 100.0,
            evasion: 200.0,
            energy_shield: 300.0,
            ..ActorBaseStats::default()
        },
    );
    actor
        .mod_db
        .add_mod(pobr_core::Modifier::number("Armour", ModType::Inc, 50.0));

    let output = calc_defence(&mut actor, &CalcConfig::new(), 400.0);

    assert_eq!(output.armour, 150.0);
    assert_eq!(output.evasion, 200.0);
    assert_eq!(output.energy_shield, 300.0);
    assert_eq!(actor.output.armour, 150.0);
    assert!(actor.breakdown.contains("armour"));
}
