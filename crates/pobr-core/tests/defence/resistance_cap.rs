use crate::support::parse_mod;
use pobr_core::calc::{MinimalInput, calculate_minimal};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

fn input_with_fire(base: f64) -> MinimalInput {
    MinimalInput {
        base_fire_resistance: base,
        ..MinimalInput::default()
    }
}

#[test]
fn resistance_below_max_is_uncapped_with_zero_over_cap() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 40.0));

    let output = calculate_minimal(&db, &CalcConfig::new(), &input_with_fire(0.0));

    assert_eq!(output.fire_resistance, 40.0);
    assert_eq!(output.max_fire_resistance, 75.0);
    assert_eq!(output.fire_resistance_over_cap, 0.0);
}

#[test]
fn resistance_above_default_max_caps_at_75_and_reports_over_cap() {
    let mut db = ModDb::new();
    // total = 90 → final 75, over 15.
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 90.0));

    let output = calculate_minimal(&db, &CalcConfig::new(), &input_with_fire(0.0));

    assert_eq!(output.fire_resistance, 75.0);
    assert_eq!(output.max_fire_resistance, 75.0);
    assert_eq!(output.fire_resistance_over_cap, 15.0);
}

#[test]
fn maximum_resistance_modifier_raises_cap() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 90.0));
    db.add_mod(Modifier::number(
        "MaximumFireResistance",
        ModType::Base,
        5.0,
    ));

    let output = calculate_minimal(&db, &CalcConfig::new(), &input_with_fire(0.0));

    // max = 75 + 5 = 80, total 90 → final 80, over 10.
    assert_eq!(output.max_fire_resistance, 80.0);
    assert_eq!(output.fire_resistance, 80.0);
    assert_eq!(output.fire_resistance_over_cap, 10.0);
}

#[test]
fn all_elemental_maximum_resistance_applies_to_every_element() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "MaximumAllElementalResistances",
        ModType::Base,
        4.0,
    ));

    let output = calculate_minimal(&db, &CalcConfig::new(), &MinimalInput::default());

    assert_eq!(output.max_fire_resistance, 79.0);
    assert_eq!(output.max_cold_resistance, 79.0);
    assert_eq!(output.max_lightning_resistance, 79.0);
}

#[test]
fn maximum_resistance_cannot_exceed_hard_cap_of_90() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "MaximumFireResistance",
        ModType::Base,
        30.0,
    ));
    db.add_mod(Modifier::number("FireResistance", ModType::Base, 100.0));

    let output = calculate_minimal(&db, &CalcConfig::new(), &input_with_fire(0.0));

    // 75 + 30 = 105, hard cap 90. total 100 → final 90, over 10.
    assert_eq!(output.max_fire_resistance, 90.0);
    assert_eq!(output.fire_resistance, 90.0);
    assert_eq!(output.fire_resistance_over_cap, 10.0);
}

#[test]
fn negative_resistance_has_no_floor_and_no_over_cap() {
    let output = calculate_minimal(&ModDb::new(), &CalcConfig::new(), &input_with_fire(-30.0));

    assert_eq!(output.fire_resistance, -30.0);
    assert_eq!(output.fire_resistance_over_cap, 0.0);
}

#[test]
fn parses_maximum_resistance_modifiers() {
    let fire = parse_mod("+5% to maximum Fire Resistance").unwrap();
    assert_eq!(fire.mods[0].name, ModName::from("MaximumFireResistance"));
    assert_eq!(fire.mods[0].mod_type, ModType::Base);
    assert_eq!(fire.mods[0].value.as_number(), Some(5.0));

    let all = parse_mod("+3% to all maximum Elemental Resistances").unwrap();
    assert_eq!(
        all.mods[0].name,
        ModName::from("MaximumAllElementalResistances")
    );
}

/// Smith of Kitava: "Modifiers to Fire Resistance also grant Cold and
/// Lightning Resistance at 50% of their value" and "Modifiers to Maximum Fire
/// Resistance also grant Maximum Cold and Lightning Resistance". The
/// elemental resistance penalty is not a modifier to Fire Resistance.
#[test]
fn fire_resistance_shares_with_cold_and_lightning() {
    let mut db = ModDb::new();
    for line in [
        "Modifiers to Fire Resistance also grant Cold and Lightning Resistance at 50% of their value",
        "Modifiers to Maximum Fire Resistance also grant Maximum Cold and Lightning Resistance",
        "+100% to Fire Resistance",
        "+5% to Maximum Fire Resistance",
        "+10% to Cold Resistance",
    ] {
        let parsed = parse_mod(line).unwrap();
        assert!(!parsed.mods.is_empty(), "{line}");
        for modifier in parsed.mods {
            db.add_mod(modifier);
        }
    }
    for modifier in pobr_core::ingest::campaign::CampaignProgress::Endgame.modifiers() {
        db.add_mod(modifier);
    }

    let output = calculate_minimal(&db, &CalcConfig::new(), &MinimalInput::default());

    // Fire: 100 − 60 = 40, below its maximum of 80.
    assert_eq!(output.fire_resistance, 40.0);
    // Cold: 10 + 50 − 60 = 0, Lightning: 50 − 60 = −10; both reach 80.
    assert_eq!(output.cold_resistance, 0.0);
    assert_eq!(output.lightning_resistance, -10.0);
    assert_eq!(output.max_cold_resistance, 80.0);
    assert_eq!(output.max_lightning_resistance, 80.0);
}

#[test]
fn fire_resistance_sharing_excludes_all_elemental_and_truncates_the_total() {
    let mut db = ModDb::new();
    for line in [
        "Modifiers to Fire Resistance also grant Cold and Lightning Resistance at 50% of their value",
        "+69% to Fire Resistance",
        "+69% to Fire Resistance",
        "+5% to Fire Resistance",
        "+5% to all Elemental Resistances",
        "-7% to all Elemental [Resistances]",
        "+10% to Cold Resistance",
    ] {
        for modifier in parse_mod(line).unwrap().mods {
            db.add_mod(modifier);
        }
    }
    let output = calculate_minimal(&db, &CalcConfig::new(), &MinimalInput::default());
    // Share (69 + 69 + 5) / 2 = 71.5, without rounding individual modifiers.
    // Cold: trunc(71.5 + 5 - 7 + 10) = 79, hence 4 above the 75 cap.
    // Lightning: trunc(71.5 + 5 - 7) = 69. All-elemental +/- is counted once.
    assert_eq!(output.cold_resistance_over_cap, 4.0);
    assert_eq!(output.lightning_resistance, 69.0);
    for modifier in pobr_core::ingest::campaign::CampaignProgress::Endgame.modifiers() {
        db.add_mod(modifier);
    }
    db.add_mod(Modifier::number(
        "LightningResistance",
        ModType::Base,
        -20.0,
    ));
    let output = calculate_minimal(&db, &CalcConfig::new(), &MinimalInput::default());
    // trunc(71.5 + 5 - 7 - 60 - 20) = -10, toward zero rather than floor.
    assert_eq!(output.lightning_resistance, -10.0);
}
