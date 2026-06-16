use pobr_core::{CalcConfig, ModDb, Modifier, TraceGraph};
use pobr_data::prelude::*;

fn db_with(mods: Vec<Modifier>) -> ModDb {
    let mut db = ModDb::new();
    db.add_list(mods);
    db
}

#[test]
fn more_traced_records_product_and_value_matches_more() {
    let db = db_with(vec![
        Modifier::number("Damage", ModType::More, 50.0),
        Modifier::number("Damage", ModType::More, 20.0),
    ]);
    let cfg = CalcConfig::attack();
    let names = [ModName::from("Damage")];
    let mut trace = TraceGraph::new();

    let traced = db.more_traced(&cfg, &names, &mut trace, "Damage MORE");

    assert_eq!(traced.value, db.more(&cfg, &names));
    assert!((traced.value - 1.5 * 1.2).abs() < 1e-9);
    // one product node + one input node per contribution.
    assert_eq!(trace.nodes().len(), 3);
}

#[test]
fn flag_traced_returns_flag_state_and_records_node() {
    let db = db_with(vec![Modifier::flag("Onslaught")]);
    let cfg = CalcConfig::attack();
    let mut trace = TraceGraph::new();

    let active = db.flag_traced(&cfg, ModName::from("Onslaught"), &mut trace, "Onslaught");

    assert!(active);
    assert!(!trace.nodes().is_empty());
}

#[test]
fn override_traced_picks_last_written_value() {
    let db = db_with(vec![
        Modifier::number("CritChance", ModType::Override, 10.0),
        Modifier::number("CritChance", ModType::Override, 100.0),
    ]);
    let cfg = CalcConfig::attack();
    let mut trace = TraceGraph::new();

    let (value, _node) = db.override_traced(
        &cfg,
        ModName::from("CritChance"),
        &mut trace,
        "Crit override",
    );

    assert_eq!(value, Some(100.0));
}

#[test]
fn iter_mods_yields_every_modifier() {
    let db = db_with(vec![
        Modifier::number("Damage", ModType::Inc, 10.0),
        Modifier::number("Life", ModType::Base, 50.0),
        Modifier::number("Damage", ModType::More, 20.0),
    ]);

    assert_eq!(db.iter_mods().count(), 3);
}
