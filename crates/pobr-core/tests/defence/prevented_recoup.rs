use pobr_core::Modifier;
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_data::prelude::*;

fn run(life: f64, extra: Vec<Modifier>) -> CalculationSession {
    let mut session = super::support::session(MinimalInput {
        base_life: life,
        ..Default::default()
    });
    session
        .add_modifier_texts(&["5% of Physical Damage prevented Recouped as Life"])
        .unwrap();
    assert!(session.unsupported_modifier_texts().is_empty());
    session.add_modifiers([
        // Armour/(Armour + 10 * hit) = 6666.666.../16666.666... = 40%.
        Modifier::number("Armour", ModType::Base, 20_000.0 / 3.0),
    ]);
    session.add_modifiers(extra);
    session.add_enemy_modifiers([
        Modifier::number("EnemyPhysicalDamage", ModType::Base, 1000.0),
        Modifier::flag("NeverCrit"),
    ]);
    session.perform_minimal().unwrap();
    session
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn prevented_recoup_uses_reduction_and_life_timing_with_reconstructable_detail() {
    // Made to Last: 5%. A 1000 Physical hit, 40% armour reduction, prevents
    // 400 and deals 600. One lethal hit recoups 400 * 5% / 8 = 2.5 Life/s.
    let base = run(100.0, vec![]);
    close(base.ehp_terms().unwrap().physical_prevented, 400.0);
    close(base.output().life_recoup_rate, 2.5);

    for flag in [None, Some("4SecondLifeRecoup"), Some("4SecondRecoup")] {
        // Additional PDR 10% raises prevented damage to 500; ordinary Life
        // recoup contributes 500 * 20% as well. 50% recovery rate scales both:
        // (500 * 5% + 500 * 20%) / 8 * 1.5 = 23.4375/s (4s: 46.875).
        let mut mods = vec![
            Modifier::number("PhysicalDamageReduction", ModType::Base, 10.0),
            Modifier::number("LifeRecoup", ModType::Base, 20.0),
            Modifier::number("LifeRecoveryRate", ModType::Inc, 50.0),
        ];
        if let Some(flag) = flag {
            mods.push(Modifier::flag(flag));
        }
        let session = run(100.0, mods);
        close(
            session.output().life_recoup_rate,
            if flag.is_some() { 46.875 } else { 23.4375 },
        );
        let detail = session.derived_detail("LifeRecoupRate").unwrap();
        let input = |name: &str| detail.inputs.iter().find(|(id, _)| id == name).unwrap().1;
        close(input("PhysicalDamagePrevented"), 500.0);
        let rebuilt = (input("RecoupableDamage") * input("RecoupPercent")
            + input("PhysicalDamagePrevented") * input("PreventedRecoupPercent"))
            / 100.0
            / input("RecoupDuration")
            * input("RecoupRecovery");
        close(rebuilt, session.output().life_recoup_rate);
        assert!(
            detail
                .formula
                .contains("PhysicalDamagePrevented * PreventedRecoupPercent")
        );
    }

    // Accumulation follows actual top-level batches, not recursive probes.
    // Ten 600-damage hits deplete 6000 Life: 4000 prevented => 25 Life/s.
    let many = run(6000.0, vec![]);
    close(many.ehp_terms().unwrap().physical_prevented, 4000.0);
    close(many.output().life_recoup_rate, 25.0);
}

#[test]
fn prevented_recoup_excludes_damage_taken_multipliers_and_shifted_damage() {
    let less = run(
        100.0,
        vec![Modifier::number(
            "PhysicalDamageTaken",
            ModType::More,
            -50.0,
        )],
    );
    // 600 becomes 300 taken, but only the earlier 400 is prevented for recoup.
    close(less.ehp_terms().unwrap().taken_hit[0], 300.0);
    close(less.output().life_recoup_rate, 2.5);
    let shifted = run(
        100.0,
        vec![Modifier::number(
            "PhysicalDamageTakenAsFire",
            ModType::Base,
            100.0,
        )],
    );
    close(shifted.ehp_terms().unwrap().taken_hit[0], 0.0);
    close(shifted.output().life_recoup_rate, 0.0);
}

#[test]
fn prevented_recoup_excludes_blocked_and_deflected_hits() {
    for damage_on_block in [0.0, 20.0] {
        let blocked = run(
            100.0,
            vec![
                Modifier::number("ShieldBlockChance", ModType::Base, 40.0),
                Modifier::number("SpellBlockChance", ModType::Base, 40.0),
                Modifier::number("BlockEffect", ModType::Base, damage_on_block),
            ],
        );
        // 40% block: only the 60% unblocked hits contribute 400 prevented.
        // Taking 20% damage on block does not make those hits eligible.
        close(blocked.ehp_terms().unwrap().physical_prevented, 240.0);
        close(blocked.output().life_recoup_rate, 1.5);
    }
    let deflected = run(
        100.0,
        vec![Modifier::number("DeflectionRating", ModType::Base, 1e9)],
    );
    // At the game's 95% deflection cap, only 5% of hits qualify:
    // 400 * 5% = 20 prevented, 20 * 5% / 8 = 0.125 Life/s.
    close(deflected.output().deflect_chance, 95.0);
    close(deflected.ehp_terms().unwrap().physical_prevented, 20.0);
    close(deflected.output().life_recoup_rate, 0.125);

    // Evasion changes attack count, not the damage accumulated by landed hits.
    let evasion = run(100.0, vec![Modifier::number("Evasion", ModType::Base, 1e9)]);
    assert!(evasion.ehp_terms().unwrap().not_hit_chance > 0.0);
    close(evasion.output().life_recoup_rate, 2.5);
}

fn run_chaos(life: f64, mana: f64, extra: Vec<Modifier>) -> CalculationSession {
    let mut session = super::support::session(MinimalInput {
        base_life: life,
        base_mana: mana,
        ..Default::default()
    });
    // Forsaken Bangle's line, as the charm payload reads it: the same 50% as Life and as Mana.
    session
        .add_modifier_texts(&["50% of Chaos damage you prevent when Hit Recouped as Life and Mana"])
        .unwrap();
    assert!(session.unsupported_modifier_texts().is_empty());
    session.add_modifiers([
        // 40% Chaos resistance: a 1000 Chaos hit enters the stage as 1000 and leaves as 600.
        Modifier::number("ChaosResist", ModType::Base, 40.0),
    ]);
    session.add_modifiers(extra);
    session.add_enemy_modifiers([
        Modifier::number("EnemyChaosDamage", ModType::Base, 1000.0),
        Modifier::flag("NeverCrit"),
    ]);
    session.perform_minimal().unwrap();
    session
}

fn detail_input(session: &CalculationSession, field: &str, name: &str) -> f64 {
    let detail = session.derived_detail(field).unwrap();
    detail.inputs.iter().find(|(id, _)| id == name).unwrap().1
}

#[test]
fn chaos_prevented_recoup_returns_life_and_mana_over_their_own_timing() {
    // A 1000 Chaos hit at 40% resistance prevents 400 and deals 600, which kills 100 Life
    // in one hit. 50% of 400 = 200 recouped as Life over 8 s = 25 Life/s, and the same
    // 200 as Mana over 8 s = 25 Mana/s.
    let base = run_chaos(100.0, 0.0, vec![]);
    let terms = base.ehp_terms().unwrap();
    close(terms.chaos_prevented, 400.0);
    close(terms.physical_prevented, 0.0);
    close(base.output().life_recoup_rate, 25.0);
    close(base.output().mana_recoup_rate, 25.0);

    // Mana's own recovery rate and duration apply to Mana only: +50% Mana recovery rate
    // gives 25 * 1.5 = 37.5 Mana/s, a 4-second Mana recoup doubles the base to 50 Mana/s,
    // and Life stays 25 Life/s unless the generic 4-second flag also shortens Life's.
    let faster = run_chaos(
        100.0,
        0.0,
        vec![Modifier::number("ManaRecoveryRate", ModType::Inc, 50.0)],
    );
    close(faster.output().mana_recoup_rate, 37.5);
    close(faster.output().life_recoup_rate, 25.0);
    for flag in ["4SecondManaRecoup", "4SecondRecoup"] {
        let quick = run_chaos(100.0, 0.0, vec![Modifier::flag(flag)]);
        close(quick.output().mana_recoup_rate, 50.0);
        close(
            quick.output().life_recoup_rate,
            if flag == "4SecondRecoup" { 50.0 } else { 25.0 },
        );
    }

    // Both details rebuild their rates from the listed inputs.
    for (field, rate) in [
        ("LifeRecoupRate", base.output().life_recoup_rate),
        ("ManaRecoupRate", base.output().mana_recoup_rate),
    ] {
        let input = |name: &str| detail_input(&base, field, name);
        close(input("ChaosDamagePrevented"), 400.0);
        close(input("ChaosPreventedRecoupPercent"), 50.0);
        let mut total = input("RecoupableDamage") * input("RecoupPercent")
            + input("ChaosDamagePrevented") * input("ChaosPreventedRecoupPercent");
        if field == "LifeRecoupRate" {
            total += input("PhysicalDamagePrevented") * input("PreventedRecoupPercent");
        }
        close(
            total / 100.0 / input("RecoupDuration") * input("RecoupRecovery"),
            rate,
        );
        assert!(
            base.derived_detail(field)
                .unwrap()
                .formula
                .contains("ChaosDamagePrevented * ChaosPreventedRecoupPercent")
        );
    }
}

#[test]
fn chaos_prevented_recoup_counts_resistance_and_reduction_but_not_later_multipliers() {
    // Negative resistance enlarges the hit and prevents nothing: 1000 * (1 - 1.5) < 0 -> 0.
    let weak = run_chaos(
        100.0,
        0.0,
        vec![Modifier::number("ChaosResist", ModType::Base, -90.0)],
    );
    close(weak.ehp_terms().unwrap().chaos_prevented, 0.0);
    close(weak.output().life_recoup_rate, 0.0);
    close(weak.output().mana_recoup_rate, 0.0);

    // Damage taken multipliers come after the stage: 600 becomes 300 taken, still 400 prevented.
    let less = run_chaos(
        100.0,
        0.0,
        vec![Modifier::number("ChaosDamageTaken", ModType::More, -50.0)],
    );
    close(less.ehp_terms().unwrap().taken_hit[4], 300.0);
    close(less.ehp_terms().unwrap().chaos_prevented, 400.0);
    close(less.output().mana_recoup_rate, 25.0);

    // Hits taken as Fire are no longer Chaos hits: nothing enters the Chaos stage.
    let shifted = run_chaos(
        100.0,
        0.0,
        vec![Modifier::number(
            "ChaosDamageTakenAsFire",
            ModType::Base,
            100.0,
        )],
    );
    close(shifted.ehp_terms().unwrap().chaos_prevented, 0.0);
    close(shifted.output().mana_recoup_rate, 0.0);

    // 40% block: only the 60% unblocked hits contribute their 400 prevented, 240 in total;
    // 50% of 240 = 120 over 8 s = 15 per second for each resource.
    let blocked = run_chaos(
        100.0,
        0.0,
        vec![
            Modifier::number("ShieldBlockChance", ModType::Base, 40.0),
            Modifier::number("SpellBlockChance", ModType::Base, 40.0),
            Modifier::number("BlockEffect", ModType::Base, 0.0),
        ],
    );
    close(blocked.ehp_terms().unwrap().chaos_prevented, 240.0);
    close(blocked.output().life_recoup_rate, 15.0);
    close(blocked.output().mana_recoup_rate, 15.0);
}
