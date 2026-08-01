//! Integration tests: the Block / Spirit / Ward / Deflection panel family +
//! reservation efficiency.
//!
//! Expected values are all hand-computed from the PoB2 formula; comments cite
//! CalcDefence.lua line numbers (each formula test carries a vendor line number
//! plus a hand-computed expectation).

use pobr_core::{CalcConfig, ModDb, calc};
use pobr_data::prelude::*;

/// Mod text -> ModDb (through mod_parser; the W0.1 mod coverage table is the contract).
fn db_from_texts(texts: &[&str]) -> ModDb {
    let mut db = ModDb::new();
    for text in texts {
        let outcome = crate::support::parse_mod(text)
            .unwrap_or_else(|e| panic!("failed to parse `{text}`: {e:?}"));
        assert!(
            !outcome.mods.is_empty(),
            "`{text}` should parse into a mod (Unsupported silently drops coverage)"
        );
        db.add_list(outcome.mods);
    }
    db
}

/// Directly injects a numeric mod (build-layer injection semantics, e.g. ShieldBlockChance).
fn add_base(db: &mut ModDb, name: &str, value: f64) {
    db.add_list([pobr_core::Modifier::number(name, ModType::Base, value)]);
}

// Block (CalcDefence.lua:961-1058)

/// With no mods at all: block chance 0, cap = the character's innate BaseBlockChanceMax 50
/// (Misc.lua:147; CalcSetup.lua:28).
#[test]
fn block_defaults_without_sources() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert_eq!(block.block_chance, 0.0);
    assert_eq!(block.block_chance_max, 50.0);
    assert_eq!(block.spell_block_chance_max, 50.0);
    assert_eq!(block.effective_block_chance, 0.0);
    assert_eq!(
        block.block_effect_taken_pct, 0.0,
        "default is full block (0% damage taken)"
    );
}

/// Shield base + the inc multiplier: `(26 + 0) x 1.07 = 27.82` (CalcDefence.lua:989-991;
/// the golden shape from warrior-titan-shield-wall: Tawhoan Tower Shield base 26 +
/// 7% inc from the tree).
#[test]
fn block_shield_base_times_increased() {
    let mut db = db_from_texts(&["7% increased Block chance"]);
    add_base(&mut db, "ShieldBlockChance", 26.0);
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert!(
        (block.block_chance - 27.82).abs() < 1e-9,
        "(26+0)x1.07 = 27.82, got {}",
        block.block_chance
    );
    assert_eq!(
        block.effective_block_chance, 27.82,
        "without the luck flag, the effective value equals the raw value"
    );
}

/// The BlockChanceMax system (:961-965): cap = 50 innate + sum of BASE BlockChanceMax
/// mods; the total is capped when it exceeds this; BlockChanceCap 90 is the hard ceiling.
#[test]
fn block_capped_by_max_chain() {
    let mut db = db_from_texts(&["+5% to Maximum Block Chance"]);
    add_base(&mut db, "ShieldBlockChance", 30.0);
    add_base(&mut db, "BlockChance", 40.0); // 30+40=70 > 55
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert_eq!(block.block_chance_max, 55.0, "50 innate + 5 from the mod");
    assert_eq!(block.block_chance, 55.0, "70 clamped by the cap 55");
}

/// Spell block (:1003-1014): has its own independent BASE/multipliers; with the
/// `SpellBlockChanceIsBlockChance` flag (ModParser.lua:3027) it equals attack block.
#[test]
fn spell_block_independent_and_flag_unified() {
    // Independent path: a 20% spell block mod -> spell=20, attack unaffected.
    let db = db_from_texts(&["20% chance to Block Spell Damage"]);
    let cfg = CalcConfig::new();
    let block = calc::calc_block(&db, &cfg);
    assert_eq!(block.spell_block_chance, 20.0);
    assert_eq!(block.block_chance, 0.0);

    // Flag path: spell block = attack block. The mod text (ModParser.lua:3027)
    // isn't covered by the current dataset/engine rules — the flag is injected
    // directly (its real-world production source is engine-rule data).
    let mut db = ModDb::new();
    db.add_list([pobr_core::Modifier::flag("SpellBlockChanceIsBlockChance")]);
    add_base(&mut db, "ShieldBlockChance", 26.0);
    let block = calc::calc_block(&db, &cfg);
    assert_eq!(block.spell_block_chance, block.block_chance);
    assert_eq!(block.spell_block_chance, 26.0);
}

/// Block-effect-taken (:1054-1058 / ModParser.lua:2479): `You take 30% of Damage from
/// Blocked Hits` -> a 30% damage-taken share.
#[test]
fn block_effect_taken_share() {
    // `You take 30% of Damage from Blocked Hits` (ModParser.lua:2479) isn't covered
    // by the current engine rules — injected directly as BlockEffect BASE (the calc
    // consumption side is unchanged).
    let mut db = ModDb::new();
    add_base(&mut db, "BlockEffect", 30.0);
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert_eq!(block.block_effect_taken_pct, 30.0);
}

// The Spirit pool (CalcDefence.lua:73-126)

/// Pool formula (:87-95): `(sum of BASE x (1-conv) + extra) x (1+inc) x more`, rounded.
/// Hand calc: (100+30) x 1.20 = 156.
#[test]
fn spirit_pool_base_times_increased() {
    let mut db = db_from_texts(&["+30 to Spirit"]);
    add_base(&mut db, "Spirit", 100.0); // quest-reward / item-level injection semantics
    db.add_list([pobr_core::Modifier::number("Spirit", ModType::Inc, 20.0)]);
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_spirit_pool(&db, &cfg), 156.0);
}

/// With no sources, it floors at 1 (:95 `m_max(round(...), 1)`, matching Life/Mana).
#[test]
fn spirit_pool_floor_is_one() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_spirit_pool(&db, &cfg), 1.0);
}

/// Conversion shrinkage (:92): `SpiritConvertToEnergyShield` 30 -> 100x0.7 = 70.
#[test]
fn spirit_pool_conversion_reduces_base() {
    let mut db = ModDb::new();
    add_base(&mut db, "Spirit", 100.0);
    add_base(&mut db, "SpiritConvertToEnergyShield", 30.0);
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_spirit_pool(&db, &cfg), 70.0);
}

// The Ward pool (CalcDefence.lua:1144-1296)

/// Aggregate formula (:1158/:1286): `sum of BASE Ward x (1 + sum of inc(Ward,Defences)/100) x more`.
/// Hand calc: (34+241+50) x 1.20 = 390.
#[test]
fn ward_base_times_increased() {
    let mut db = db_from_texts(&["+50 to Ward", "20% increased Ward"]);
    add_base(&mut db, "Ward", 34.0); // item-level injection semantics (a rolled `Ward:` line)
    add_base(&mut db, "Ward", 241.0);
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_ward(&db, &cfg, false), 390.0);
}

/// The `EnergyShieldToWard` keystone (:1162-1163): ES's inc is lent to Ward —
/// EnergyShield is added to the set of inc names consulted. Hand calc:
/// 100 x (1 + (20+30)/100) = 150.
#[test]
fn ward_es_to_ward_borrows_es_increases() {
    let mut db = db_from_texts(&["20% increased Ward"]);
    add_base(&mut db, "Ward", 100.0);
    db.add_list([pobr_core::Modifier::number(
        "EnergyShield",
        ModType::Inc,
        30.0,
    )]);
    let cfg = CalcConfig::new();

    assert_eq!(
        calc::calc_ward(&db, &cfg, false),
        120.0,
        "without the keystone, nothing is borrowed"
    );
    assert_eq!(
        calc::calc_ward(&db, &cfg, true),
        150.0,
        "the keystone borrows the ES inc"
    );
}

/// With no Ward sources -> 0 (verifying the zero case isn't misreported).
#[test]
fn ward_zero_without_sources() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    assert_eq!(calc::calc_ward(&db, &cfg, false), 0.0);
}

// Deflection (CalcDefence.lua:48-54 / :1487-1506)

/// Hand calc of the `deflectChance` formula (:48-54): rating=5000, acc=2000 ->
/// notDeflect = 2000/(2000+600)x150-50 = 65.3846..., chance = 100-round(65) = 35.
#[test]
fn deflection_chance_formula() {
    let mut db = ModDb::new();
    add_base(&mut db, "DeflectionRating", 5000.0);
    let cfg = CalcConfig::new();

    let d = calc::calc_deflection(&db, &cfg, 0.0, 0.0, 2000.0);

    assert_eq!(d.rating, 5000.0);
    assert_eq!(d.chance, 35.0);
    assert_eq!(d.effect_pct, 40.0, "base DeflectEffect 40 (Misc.lua:111)");
}

/// GainAs composition (:1490): rating = 0 + (evasion x 30% + armour x 20%) x (1+inc);
/// hand calc: (10000x0.30 + 5000x0.20) x 1.10 = 4400. inc only applies to the GainAs part.
#[test]
fn deflection_rating_gain_as_with_increased() {
    let mut db = db_from_texts(&[
        "Gain Deflection Rating equal to 30% of Evasion Rating",
        "Gain Deflection Rating equal to 20% of Armour",
        "10% increased Deflection Rating",
    ]);
    add_base(&mut db, "DeflectionRating", 100.0);
    let cfg = CalcConfig::new();

    let d = calc::calc_deflection(&db, &cfg, 5000.0, 10000.0, 2000.0);

    // 100 (bare BASE, unaffected by multipliers) + 4000x1.1 = 4500.
    assert_eq!(d.rating, 4500.0);
}

/// rating < 1 -> 0 chance (:49-51); DeflectIsLucky's power formula (:1492-1495).
#[test]
fn deflection_zero_and_lucky() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    assert_eq!(
        calc::calc_deflection(&db, &cfg, 0.0, 0.0, 2000.0).chance,
        0.0
    );

    // lucky: 35% -> (1-0.65^2)x100 = 57.75. The `Chance to Deflect is Lucky` text
    // isn't covered by the current engine rules — the DeflectIsLucky flag is
    // injected directly.
    let mut db = ModDb::new();
    db.add_list([pobr_core::Modifier::flag("DeflectIsLucky")]);
    add_base(&mut db, "DeflectionRating", 5000.0);
    let d = calc::calc_deflection(&db, &cfg, 0.0, 0.0, 2000.0);
    assert!((d.chance - 57.75).abs() < 1e-9, "got {}", d.chance);
}

// Reservation Efficiency (CalcDefence.lua:172-350)

/// The efficiency division (:240-241/:249-258): `reserved = (flat + pool x pct) x mult /
/// (1+eff/100) / effMore`. Hand calc: pool 1000, 50% reserved, 20% efficiency ->
/// 500 / 1.2 = 416.666....
#[test]
fn reservation_efficiency_divides() {
    let r = calc::reservation_with_efficiency(1000.0, 0.0, 50.0, 1.0, 20.0, 1.0);
    assert!(
        (r.reserved - 500.0 / 1.2).abs() < 1e-6,
        "got {}",
        r.reserved
    );

    // Negative efficiency (reduced efficiency) -> more is reserved: 500 / 0.8 = 625.
    let r = calc::reservation_with_efficiency(1000.0, 0.0, 50.0, 1.0, -20.0, 1.0);
    assert_eq!(r.reserved, 625.0);

    // Efficiency -100% (divisor 0) -> diverges, clamped to the full pool (a degenerate
    // edge case outside vendor's :251 more>0 guard).
    let r = calc::reservation_with_efficiency(1000.0, 100.0, 0.0, 1.0, -100.0, 1.0);
    assert_eq!(r.reserved, 1000.0);
}

/// ReservationMultiplier (:197 `floor(more, 4)`): mult 1.3 -> reservation x1.3;
/// floored to 4 decimal places (1.23456 -> 1.2345).
#[test]
fn reservation_multiplier_floor4() {
    let r = calc::reservation_with_efficiency(1000.0, 100.0, 0.0, 1.3, 0.0, 1.0);
    assert_eq!(r.reserved, 130.0);

    let r = calc::reservation_with_efficiency(10000.0, 10000.0, 0.0, 1.23456, 0.0, 1.0);
    // floor(1.23456, 4) = 1.2345 -> 10000x1.2345 = 12345 -> clamped to the pool of 10000.
    assert_eq!(r.reserved, 10000.0);
    let r = calc::reservation_with_efficiency(100000.0, 10000.0, 0.0, 1.23456, 0.0, 1.0);
    assert_eq!(r.reserved, 12345.0);
}

/// Spirit reservation efficiency is **not** applied a second time on the aggregate
/// side: efficiency is per-skill semantics (vendor CalcDefence.lua:240-243 computes
/// it per skillCfg); PoBR applies it at the injection side instead —
/// `spirit_reservation_modifiers` already divides by it before injecting each
/// SkillSpiritReservationBase — so `calc_spirit_reservation` no longer divides the
/// aggregate total by efficiency (the old implementation's global division would
/// have double-applied it alongside the injection-side division).
#[test]
fn spirit_reservation_efficiency_not_applied_twice_at_aggregate() {
    let db = db_from_texts(&["25% increased Spirit Reservation Efficiency"]);
    let cfg = CalcConfig::new();

    let r = calc::skill_mechanics::calc_spirit_reservation(&db, &cfg, 100.0);

    assert_eq!(r.final_cost, 100.0);
}
