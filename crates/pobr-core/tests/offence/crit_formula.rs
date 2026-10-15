//! The displayed steps of crit chance and crit multiplier for the cases the
//! plain base / flat / increased / more chain does not cover: an increase on
//! the enemy, a skill that cannot crit, and crits without extra damage.

use pobr_core::calc::{CalculationSession, FormulaOp, MinimalInput, StatFormula};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

fn session(cfg: CalcConfig, player: Vec<Modifier>, enemy: Vec<Modifier>) -> CalculationSession {
    let mut session = CalculationSession::new(MinimalInput {
        base_life: 100.0,
        base_mana: 0.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 1.0,
    })
    .with_config(cfg);
    session.add_modifiers(player);
    session.add_enemy_modifiers(enemy);
    session.perform_minimal().expect("perform");
    session
}

fn ops(formula: &StatFormula) -> Vec<FormulaOp> {
    formula.steps.iter().map(|step| step.op).collect()
}

/// The enemy's increase adds to the player's: 10 × (1 + (50 + 30) / 100) = 18,
/// shown as the player's step (15) and the enemy's on top of it.
#[test]
fn enemy_increase_to_crit_chance_is_its_own_step() {
    let session = session(
        CalcConfig::spell().with_mode_effective(true),
        vec![
            Modifier::number("CriticalStrikeChance", ModType::Base, 10.0),
            Modifier::number("CriticalStrikeChance", ModType::Inc, 50.0),
        ],
        vec![Modifier::number("SelfCritChance", ModType::Inc, 30.0)],
    );
    let formula = session.stat_formula("CritChance").expect("formula");
    assert_eq!(
        ops(&formula),
        [
            FormulaOp::Flat,
            FormulaOp::Increased,
            FormulaOp::EnemyIncreaseAdded
        ]
    );
    let results: Vec<f64> = formula.steps.iter().map(|step| step.result).collect();
    assert!((results[1] - 15.0).abs() < 1e-9 && (results[2] - 18.0).abs() < 1e-9);
    assert!((session.output().crit_chance * 100.0 - 18.0).abs() < 1e-9);
    assert_eq!(formula.value, session.output().crit_chance * 100.0);
}

/// `NeverCrit` zeroes the chance and the multiplier whatever the lines say.
#[test]
fn a_skill_that_cannot_crit_says_so() {
    let session = session(
        CalcConfig::spell(),
        vec![
            Modifier::number("CriticalStrikeChance", ModType::Base, 10.0),
            Modifier::number("CriticalStrikeMultiplier", ModType::Base, 50.0),
            Modifier::flag("NeverCrit"),
        ],
        vec![],
    );
    for (stat, shown) in [
        ("CritChance", session.output().crit_chance),
        ("CritMultiplier", session.output().crit_multiplier),
    ] {
        let formula = session.stat_formula(stat).expect("formula");
        assert_eq!(ops(&formula), [FormulaOp::NeverCrit], "{stat}");
        assert_eq!((formula.value, shown), (0.0, 0.0), "{stat}");
    }
}

/// `NoCritMultiplier` leaves the chance alone and pins the multiplier at 1.
#[test]
fn crits_without_extra_damage_say_so() {
    let session = session(
        CalcConfig::spell(),
        vec![
            Modifier::number("CriticalStrikeChance", ModType::Base, 10.0),
            Modifier::number("CriticalStrikeMultiplier", ModType::Base, 50.0),
            Modifier::flag("NoCritMultiplier"),
        ],
        vec![],
    );
    let formula = session.stat_formula("CritMultiplier").expect("formula");
    assert_eq!(ops(&formula), [FormulaOp::NoCritBonus]);
    assert_eq!(
        (formula.value, session.output().crit_multiplier),
        (1.0, 1.0)
    );
    let chance = session.stat_formula("CritChance").expect("chance formula");
    assert_eq!(chance.value, 10.0);
}

/// "Chance to Hit with Attacks can exceed 100%" and "Gain additional Critical
/// Hit Chance equal to 25% of excess chance to Hit with Attacks": 3000
/// accuracy against 968 evasion is 3750 / 3290.4 = 114% to hit, so 14 in
/// excess and 3.5% more chance to crit on a base of 5%.
#[test]
fn excess_chance_to_hit_becomes_critical_hit_chance() {
    use pobr_core::ModTag;

    let crit_chance = |cfg: CalcConfig, can_exceed: bool| {
        let mut mods = vec![
            Modifier::number("Accuracy", ModType::Base, 3000.0),
            Modifier::number("CriticalStrikeChance", ModType::Base, 5.0),
            Modifier::number("CriticalStrikeChance", ModType::Base, 0.25)
                .with_tag(ModTag::multiplier("ExcessHitChance", 1.0, None)),
        ];
        if can_exceed {
            mods.push(Modifier::flag("Condition:HitChanceCanExceed100"));
        }
        let mut session = CalculationSession::new(MinimalInput {
            base_life: 100.0,
            base_mana: 0.0,
            base_fire_resistance: 0.0,
            base_cold_resistance: 0.0,
            base_lightning_resistance: 0.0,
            base_accuracy: 0.0,
            enemy_evasion: 968.0,
            base_hit_min: 100.0,
            base_hit_max: 200.0,
            base_action_rate: 1.0,
        })
        .with_config(cfg);
        session.add_modifiers(mods);
        session.perform_minimal().expect("perform").crit_chance
    };
    let close = |actual: f64, expected: f64| {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    };
    close(crit_chance(CalcConfig::attack(), true), 0.085);
    // The chance to hit of a character without it stops at 100%.
    close(crit_chance(CalcConfig::attack(), false), 0.05);
    // A spell makes no accuracy check: it has no chance to hit to exceed.
    close(crit_chance(CalcConfig::spell(), true), 0.05);
}
