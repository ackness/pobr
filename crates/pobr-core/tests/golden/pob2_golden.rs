//! PoB2 test suite port (golden parity).
//!
//! Ports the inputs/expected values of **isolated-mechanic** cases from vendor PoB2's
//! `spec/System/Test*_spec.lua` into PoBR calculation assertions: each case pins down
//! one mechanic (crit multiplier, crit-average formula, damage-increase aggregation,
//! max hit taken, ...), with expected values taken from PoB2's tests (as literals).
//! Target: within 10% of PoB2 (pure-formula cases require exactness).
//!
//! Source mapping is noted per case. When adding a new mechanic, prefer adding a
//! golden case here first (easier to debug than a real ninja build).

use crate::support::parse_mod;
use pobr_core::calc::{CalculationSession, MinimalInput, calculate_minimal};
use pobr_core::mod_parser::ParseStatus;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

/// Relative error < 10% (the PoB2 parity target). Falls back to absolute error when
/// `golden` is 0.
fn within_10pct(actual: f64, golden: f64) -> bool {
    if golden == 0.0 {
        actual.abs() < 1e-6
    } else {
        ((actual - golden) / golden).abs() < 0.10
    }
}

/// Parses PoB mod text and injects it into db (used to reproduce PoB tests'
/// customMods / item mods).
fn add_text(db: &mut ModDb, text: &str) {
    let outcome = parse_mod(text).unwrap_or_else(|e| panic!("parse {text:?}: {e}"));
    if outcome.status == ParseStatus::Parsed {
        db.add_list(outcome.mods);
    } else {
        panic!("unsupported modifier in golden test: {text:?}");
    }
}

fn input_base_hit(min: f64, max: f64) -> MinimalInput {
    MinimalInput {
        base_life: 1000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: min,
        base_hit_max: max,
        base_action_rate: 1.0,
    }
}

/// PoB2 TestAttacks "creates an item and has the correct crit multi":
/// base crit multiplier 2.0 + "25% increased Critical Damage Bonus" -> 2.25.
#[test]
fn crit_multiplier_base_plus_increase() {
    let mut db = ModDb::new();
    add_text(&mut db, "25% increased Critical Damage Bonus");
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(1.0, 1.0));
    assert!(
        (out.crit_multiplier - 2.25).abs() < 1e-6,
        "crit_multiplier = {} (expected 2.25)",
        out.crit_multiplier
    );
}

/// PoB2 TestAttacks "correctly calculates critical hit damage with static values":
/// base hit 1, crit chance 10%, multiplier 2 -> average hit = (1-0.1)*1 + 0.1*1*2 = 1.1.
#[test]
fn crit_average_static() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(1.0, 1.0));
    assert!(
        (out.crit_chance - 0.10).abs() < 1e-6,
        "crit_chance = {} (expected 0.10)",
        out.crit_chance
    );
    assert!(
        (out.total_hit_avg - 1.1).abs() < 1e-6,
        "total_hit_avg = {} (expected 1.1)",
        out.total_hit_avg
    );
}

/// PoB2 TestAttacks "does not force critical hits when critical hit chance is zero":
/// crit chance 0 -> crit chance 0, average hit = base (1).
#[test]
fn no_crit_when_zero_chance() {
    let db = ModDb::new();
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(1.0, 1.0));
    assert_eq!(out.crit_chance, 0.0);
    assert!(
        (out.total_hit_avg - 1.0).abs() < 1e-6,
        "total_hit_avg = {} (expected 1.0)",
        out.total_hit_avg
    );
}

/// PoB2 TestAttacks "correctly converts spell damage per stat to attack damage"
/// (excerpt): "10% increased attack damage" -> attack-domain Damage INC = 10.
#[test]
fn attack_damage_increase_aggregation() {
    let mut db = ModDb::new();
    add_text(&mut db, "10% increased attack damage");
    add_text(&mut db, "10% increased spell damage");
    let attack = CalcConfig::attack();
    let inc = db.sum(ModType::Inc, &attack, &[ModName::from("AttackDamage")]);
    assert!(
        (inc - 10.0).abs() < 1e-6,
        "attack Damage INC = {inc} (expected 10)"
    );
}

/// PoB2 TestDefence "no armour max hits" (baseline): default character Life 60,
/// all four resistances -60%, no armour -> physical max hit taken = Life 60;
/// elemental/chaos = 60/(1+0.6) = 37.5 (PoB2 rounds to 38).
/// Verifies PoBR's `max_hit_for_type = pool/(1-res/100)` matches PoB2.
#[test]
fn defence_no_armour_max_hits() {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    // Chaos resistance isn't in MinimalInput, so inject -60% separately.
    session.add_modifiers([Modifier::number("ChaosResistance", ModType::Base, -60.0)]);
    session.perform_minimal();
    let o = session.output();

    assert!(
        within_10pct(o.physical_max_hit, 60.0),
        "physical_max_hit = {} (PoB2 60)",
        o.physical_max_hit
    );
    for (name, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
        ("chaos", o.chaos_max_hit),
    ] {
        assert!(within_10pct(v, 38.0), "{name}_max_hit = {v} (PoB2 38)");
    }
}

/// PoB2 TestDefence "no armour max hits" (+200 res / +200% physical DR):
/// elemental/chaos res -60+200=140 -> capped at 75% -> takes 0.25 -> 60/0.25 = 240;
/// physical +200% additional DR -> DR capped at 90% -> takes 0.1 -> 60/0.1 = 600.
/// Verifies PoBR's resistance cap (75) and physical DR cap (90%) match PoB2.
#[test]
fn defence_capped_res_and_pdr() {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    session.add_modifiers([
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("FireResistance", ModType::Base, 200.0),
        Modifier::number("ColdResistance", ModType::Base, 200.0),
        Modifier::number("LightningResistance", ModType::Base, 200.0),
        Modifier::number("ChaosResistance", ModType::Base, 200.0),
        Modifier::number("PhysicalDamageReduction", ModType::Base, 200.0),
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 600.0),
        "physical_max_hit = {} (PoB2 600)",
        o.physical_max_hit
    );
    for (name, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
        ("chaos", o.chaos_max_hit),
    ] {
        assert!(within_10pct(v, 240.0), "{name}_max_hit = {v} (PoB2 240)");
    }
}

/// PoB2 TestAttacks "correctly adds damage with oracle forced outcome" (inevitable):
/// base 1, crit 10%, multiplier 2, "inevitable critical hits" -> average hit
/// = 1 + (2-1)*(1*0.1 + 0.7*0.9*0.1 + 0.4*0.9^2*0.1 + 0.1*0.9^3*0.1) = 1.20269.
#[test]
fn crit_inevitable_forced_outcome() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    db.add_mod(Modifier::flag("InevitableCriticalHits"));
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let out = calculate_minimal(&db, &cfg, &input_base_hit(1.0, 1.0));
    let s = 1.0 * 0.1 + 0.7 * 0.9 * 0.1 + 0.4 * 0.9_f64.powi(2) * 0.1 + 0.1 * 0.9_f64.powi(3) * 0.1;
    let expected = 1.0 * (1.0 + (2.0 - 1.0) * s);
    assert!(
        within_10pct(out.total_hit_avg, expected),
        "inevitable total_hit_avg = {} (PoB2 {:.5})",
        out.total_hit_avg,
        expected
    );
}

/// PoB2 TestAttacks "correctly calculates forced outcome with bifurcated critical hits":
/// base 1, crit 10%, multiplier 2, bifurcate+inevitable -> average hit
/// = 1 + (2-1)*(2*0.1*(1 + 0.7*(1-0.1)^2 + 0.4*((1-0.1)^2)^2 + 0.1*((1-0.1)^2)^3)).
#[test]
fn crit_bifurcate_inevitable() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    db.add_mod(Modifier::flag("BifurcateCrit"));
    db.add_mod(Modifier::flag("InevitableCriticalHits"));
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let out = calculate_minimal(&db, &cfg, &input_base_hit(1.0, 1.0));
    let q = (1.0_f64 - 0.1).powi(2);
    let expected =
        1.0 + (2.0 - 1.0) * (2.0 * 0.1 * (1.0 + 0.7 * q + 0.4 * q.powi(2) + 0.1 * q.powi(3)));
    assert!(
        within_10pct(out.total_hit_avg, expected),
        "bifurcate+inevitable total_hit_avg = {} (PoB2 {:.5})",
        out.total_hit_avg,
        expected
    );
}

/// PoB2 TestDefence "armoured max hits": Life 1000, resistances -60%, armour 10000,
/// no PDR. Physical max hit taken must be **self-consistent** (armour DR depends on
/// hit size): H satisfies H*taken(H)=pool -> H^2-1000H-10^6=0 -> H~=1618 (PoB2's
/// PhysicalMaximumHitTaken); elemental = 1000/1.6 = 625.
#[test]
fn defence_armoured_max_hits() {
    let input = MinimalInput {
        base_life: 1000.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    session.add_modifiers([
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("Armour", ModType::Base, 10000.0),
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 1618.0),
        "physical_max_hit = {} (PoB2 1618)",
        o.physical_max_hit
    );
    for (name, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
        ("chaos", o.chaos_max_hit),
    ] {
        assert!(within_10pct(v, 625.0), "{name}_max_hit = {v} (PoB2 625)");
    }
}

/// Shared helper: no armour, Life 60, all four resistances at -60% baseline + custom
/// defence mods -> (phys, fire, cold, lightning, chaos) max hit taken.
fn defence_max_hits(mods: &[(&str, ModType, f64)]) -> (f64, f64, f64, f64, f64) {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    let mut list = vec![Modifier::number("ChaosResistance", ModType::Base, -60.0)];
    for (n, t, v) in mods {
        list.push(Modifier::number(*n, *t, *v));
    }
    session.add_modifiers(list);
    session.perform_minimal();
    let o = session.output();
    (
        o.physical_max_hit,
        o.fire_max_hit,
        o.cold_max_hit,
        o.lightning_max_hit,
        o.chaos_max_hit,
    )
}

/// PoB2 TestDefence "no armour max hits" (+200 res / +200 max res / 50% reduced
/// damage taken): elemental res 140 -> hard-capped at 90 -> takes 0.1, then
/// x(1-0.5) reduced -> 60/0.05 = 1200; physical 60/0.5 = 120.
/// Verifies the max-resistance hard cap (90) + player-side DamageTaken (reduced=INC<0).
#[test]
fn defence_max_res_and_reduced_taken() {
    let (p, f, c, l, ch) = defence_max_hits(&[
        ("FireResistance", ModType::Base, 200.0),
        ("ColdResistance", ModType::Base, 200.0),
        ("LightningResistance", ModType::Base, 200.0),
        ("ChaosResistance", ModType::Base, 200.0),
        ("MaximumAllElementalResistances", ModType::Base, 200.0),
        ("MaximumChaosResistance", ModType::Base, 200.0),
        ("DamageTaken", ModType::Inc, -50.0),
    ]);
    assert!(within_10pct(p, 120.0), "physical = {p} (PoB2 120)");
    for (n, v) in [("fire", f), ("cold", c), ("lightning", l), ("chaos", ch)] {
        assert!(within_10pct(v, 1200.0), "{n} = {v} (PoB2 1200)");
    }
}

/// PoB2 TestDefence "no armour max hits" (+ stacking another 50% less damage taken):
/// dt = (1-0.5) reduced x 0.5 less = 0.25 -> elemental 60/(0.1*0.25)=2400; physical
/// 60/0.25 = 240. Verifies DamageTaken less=MORE multiplies with reduced.
#[test]
fn defence_reduced_and_less_taken() {
    let (p, f, c, l, ch) = defence_max_hits(&[
        ("FireResistance", ModType::Base, 200.0),
        ("ColdResistance", ModType::Base, 200.0),
        ("LightningResistance", ModType::Base, 200.0),
        ("ChaosResistance", ModType::Base, 200.0),
        ("MaximumAllElementalResistances", ModType::Base, 200.0),
        ("MaximumChaosResistance", ModType::Base, 200.0),
        ("DamageTaken", ModType::Inc, -50.0),
        ("DamageTaken", ModType::More, -50.0),
    ]);
    assert!(within_10pct(p, 240.0), "physical = {p} (PoB2 240)");
    for (n, v) in [("fire", f), ("cold", c), ("lightning", l), ("chaos", ch)] {
        assert!(within_10pct(v, 2400.0), "{n} = {v} (PoB2 2400)");
    }
}

/// PoB2 TestDefence "no armour max hits" (ES pool + fully stacked DR): Life 60 + ES
/// 60 (pool 120), max-res hard cap 90, reduced 50% + less 50% + nearby 20% less
/// (dt = 0.5x0.5x0.8 = 0.2). Elemental = 120/(0.1x0.2) = 6000; physical = 120/0.2 =
/// 600; chaos pool = life + 0.5xES = 90 -> 90/(0.1x0.2) = 4500. Verifies the ES pool
/// + chaos half-ES + multiple stacked DamageTaken multipliers.
#[test]
fn defence_es_pool_and_chaos_half() {
    let input = MinimalInput {
        base_life: 60.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    session.add_modifiers([
        Modifier::number("EnergyShield", ModType::Base, 60.0),
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("FireResistance", ModType::Base, 200.0),
        Modifier::number("ColdResistance", ModType::Base, 200.0),
        Modifier::number("LightningResistance", ModType::Base, 200.0),
        Modifier::number("ChaosResistance", ModType::Base, 200.0),
        Modifier::number("MaximumAllElementalResistances", ModType::Base, 200.0),
        Modifier::number("MaximumChaosResistance", ModType::Base, 200.0),
        Modifier::number("DamageTaken", ModType::Inc, -50.0), // 50% reduced
        Modifier::number("DamageTaken", ModType::More, -50.0), // 50% less
        Modifier::number("DamageTaken", ModType::More, -20.0), // nearby enemies 20% less
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 600.0),
        "physical = {} (PoB2 600)",
        o.physical_max_hit
    );
    for (n, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
    ] {
        assert!(within_10pct(v, 6000.0), "{n} = {v} (PoB2 6000)");
    }
    assert!(
        within_10pct(o.chaos_max_hit, 4500.0),
        "chaos = {} (PoB2 4500, pool=life+0.5*ES)",
        o.chaos_max_hit
    );
}

/// Damage conversion chain (matching PoB2's processDamageConversion): base physical
/// 100, "100% of Physical Damage Converted to Fire Damage" -> physical component 0,
/// fire component 100 (conserved, just retyped).
#[test]
fn conversion_physical_to_fire_full() {
    let mut db = ModDb::new();
    add_text(&mut db, "100% of Physical Damage Converted to Fire Damage");
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(100.0, 100.0));
    let comp = |t: DamageType| {
        out.damage_components
            .iter()
            .filter(|c| c.damage_type == t)
            .map(|c| c.avg())
            .sum::<f64>()
    };
    assert!(comp(DamageType::Physical) < 1.0, "physical should be ~0");
    assert!(
        within_10pct(comp(DamageType::Fire), 100.0),
        "fire = {} (expected ~100, converted)",
        comp(DamageType::Fire)
    );
}

/// gain-as-extra (source not deducted): base physical 100, "Gain 50% of Physical
/// Damage as Extra Fire Damage" -> physical 100 (retained) + fire 50 (extra) =
/// total 150.
#[test]
fn gain_as_extra_physical_to_fire() {
    let mut db = ModDb::new();
    add_text(&mut db, "Gain 50% of Physical Damage as Extra Fire Damage");
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input_base_hit(100.0, 100.0));
    assert!(
        within_10pct(out.total_hit_avg, 150.0),
        "total = {} (expected ~150: phys 100 + gained fire 50)",
        out.total_hit_avg
    );
}

/// PoB2 TestDefence "armoured max hits" (Armour applies to elements instead of
/// Physical): Life 1000 + Armour 10000 + elemental res 0 (physical gets no armour,
/// fire/cold/lightning are redirected to armour instead) + chaos res -60.
/// Physical = 1000 (= pool, no armour); fire/cold/lightning use the physical
/// armour-DR formula = 1618 (ArmourRatio 10: H(1-A/(A+10H))=pool); chaos =
/// 1000/1.6 = 625. Verifies the armour-applies-to-element redirect + elements
/// going through armour DR.
#[test]
fn defence_armour_applies_to_element() {
    let input = MinimalInput {
        base_life: 1000.0,
        base_mana: 50.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    // The "Armour applies to ... instead of Physical Damage" text isn't covered by
    // the current engine rules -- inject its data expansion directly
    // (ModParser.lua:2519-2524).
    session.add_modifiers([
        Modifier::number("Armour", ModType::Base, 10000.0),
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("ArmourAppliesToFireDamageTaken", ModType::Base, 100.0),
        Modifier::number("ArmourAppliesToColdDamageTaken", ModType::Base, 100.0),
        Modifier::number("ArmourAppliesToLightningDamageTaken", ModType::Base, 100.0),
        Modifier::flag("ArmourDoesNotApplyToPhysicalDamageTaken"),
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 1000.0),
        "physical = {} (PoB2 1000, redirected away from armour)",
        o.physical_max_hit
    );
    for (n, v) in [
        ("fire", o.fire_max_hit),
        ("cold", o.cold_max_hit),
        ("lightning", o.lightning_max_hit),
    ] {
        assert!(
            within_10pct(v, 1618.0),
            "{n} = {v} (PoB2 armour-on-element, ArmourRatio 10 → 1618)"
        );
    }
    assert!(
        within_10pct(o.chaos_max_hit, 625.0),
        "chaos = {} (PoB2 625, res -60)",
        o.chaos_max_hit
    );
}

/// PoB2 TestDefence "armoured max hits" (enemy physical overwhelm): Life 1000 +
/// Armour 1e9 (armour DR capped at 90%) + enemyPhysicalOverwhelm 15 -> physical DR
/// 90%-15% = 75% -> takes 0.25 -> physical max hit taken = 1000/0.25 = 4000.
/// Elemental/chaos res -60 -> 625. Verifies overwhelm reduces physical DR.
#[test]
fn defence_physical_overwhelm() {
    let input = MinimalInput {
        base_life: 1000.0,
        base_mana: 50.0,
        base_fire_resistance: -60.0,
        base_cold_resistance: -60.0,
        base_lightning_resistance: -60.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 0.0,
        base_hit_max: 0.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input).with_config(CalcConfig::attack());
    session.add_modifiers([
        Modifier::number("Armour", ModType::Base, 1.0e9),
        Modifier::number("ChaosResistance", ModType::Base, -60.0),
        Modifier::number("EnemyPhysicalOverwhelm", ModType::Base, 15.0),
    ]);
    session.perform_minimal();
    let o = session.output();
    assert!(
        within_10pct(o.physical_max_hit, 4000.0),
        "physical = {} (PoB2 4000: 90% DR - 15% overwhelm = 75% → /0.25)",
        o.physical_max_hit
    );
    assert!(
        within_10pct(o.fire_max_hit, 625.0),
        "fire = {}",
        o.fire_max_hit
    );
    assert!(
        within_10pct(o.chaos_max_hit, 625.0),
        "chaos = {}",
        o.chaos_max_hit
    );
}

/// PoB2 TestSkills "cost efficiency modifiers" (Ball Lightning L1 mana cost = 9):
/// cost efficiency is applied **after** inc/more (rounding), by **dividing** by
/// `1 + efficiency/100`, and the result is not rounded; the generic `Cost
/// Efficiency` and `Mana Cost Efficiency` stack additively. Verifies PoBR's
/// calc_mana_cost cost-efficiency semantics.
#[test]
fn cost_efficiency_modifiers() {
    use pobr_core::calc::skill_mechanics::calc_mana_cost;
    let cfg = CalcConfig::attack();
    let cost = |texts: &[&str]| -> f64 {
        let mut db = ModDb::new();
        for t in texts {
            add_text(&mut db, t);
        }
        calc_mana_cost(&db, &cfg, 9.0).final_cost
    };
    assert!((cost(&[]) - 9.0).abs() < 1e-6, "base cost = {}", cost(&[]));
    assert!(
        (cost(&["50% increased Mana Cost Efficiency"]) - 6.0).abs() < 1e-6,
        "50% mana eff → {} (PoB2 6 = 9/1.5)",
        cost(&["50% increased Mana Cost Efficiency"])
    );
    assert!(
        (cost(&["25% increased Cost Efficiency"]) - 7.2).abs() < 1e-3,
        "25% generic eff → {} (PoB2 7.2 = 9/1.25)",
        cost(&["25% increased Cost Efficiency"])
    );
    assert!(
        (cost(&[
            "25% increased Cost Efficiency",
            "25% increased Mana Cost Efficiency"
        ]) - 6.0)
            .abs()
            < 1e-6,
        "25%+25% eff → 6 (additive, 9/1.5)"
    );
    let inc_eff = cost(&[
        "50% increased Mana Cost",
        "50% increased Mana Cost Efficiency",
    ]);
    assert!(
        (inc_eff - 8.6667).abs() < 0.1,
        "50% inc + 50% eff → {inc_eff} (PoB2 8.67 = floor(9×1.5)/1.5)"
    );
}

/// Per-charge scaling (PoB2's Multiplier tag): the mod "N% increased/more Damage
/// per <X> Charge" scales by the current number of charge stacks. cfg.multiplier
/// stores the current stack count (isomorphic to PoB2's
/// `modDB.multipliers["PowerCharge"]`). Verifies PoBR's Modifier Multiplier tag
/// takes effect proportionally to stack count in sum/more aggregation.
#[test]
fn per_charge_scaling() {
    // 8% increased per power charge x 3 stacks = 24% INC.
    let mut db = ModDb::new();
    add_text(&mut db, "8% increased Damage per Power Charge");
    let cfg = CalcConfig::attack().with_multiplier("PowerCharge", 3.0);
    let inc = db.sum(ModType::Inc, &cfg, &[ModName::from("Damage")]);
    assert!(
        (inc - 24.0).abs() < 1e-6,
        "3 power charges → Damage INC {inc} (expect 24)"
    );
    // 0 stacks -> 0 INC.
    let cfg0 = CalcConfig::attack();
    let inc0 = db.sum(ModType::Inc, &cfg0, &[ModName::from("Damage")]);
    assert!(
        inc0.abs() < 1e-6,
        "0 charges → Damage INC {inc0} (expect 0)"
    );

    // 10% more per frenzy charge x 3 = x1.3.
    let mut db2 = ModDb::new();
    add_text(&mut db2, "10% more Damage per Frenzy Charge");
    let cfg3 = CalcConfig::attack().with_multiplier("FrenzyCharge", 3.0);
    let more = db2.more(&cfg3, &[ModName::from("Damage")]);
    assert!(
        (more - 1.3).abs() < 1e-6,
        "3 frenzy charges → Damage more {more} (expect 1.3)"
    );
}
