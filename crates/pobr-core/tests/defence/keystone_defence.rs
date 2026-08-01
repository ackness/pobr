//! Integration tests: keystone_registry + CI wiring + defence resource conversion matrix.
//!
//! C-1: the `DefenceKeystones::from_db` contract (a one-shot snapshot, mod names as
//! the interface). Track C / §3.3 contract 2: E/D/B/F consume this struct as a
//! parameter — no track is allowed to read keystone flags ad hoc.

use pobr_core::{CalcConfig, DefenceKeystones, ModDb, Modifier};
use pobr_data::prelude::*;

/// Drives the registry from mod text parsed through mod_parser (end-to-end:
/// text -> flag -> snapshot).
///
/// `Maximum Life is 1` (the Chaos Inoculation node's mod) -> `ChaosInoculation` flag
/// (mod_parser's keystone special-case section); `Converts all Energy Shield to Mana`
/// (Eldritch Battery style) -> `EnergyShieldConvertToMana` BASE 100 -> full-conversion switch.
#[test]
fn keystones_from_parsed_mod_texts() {
    // Arrange
    let mut db = ModDb::new();
    for text in ["Maximum Life is 1", "Converts all Energy Shield to Mana"] {
        let outcome = crate::support::parse_mod(text).expect("parse failed");
        db.add_list(outcome.mods);
    }
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(
        ks.chaos_inoculation,
        "the CI mod should drive chaos_inoculation"
    );
    assert!(
        ks.eldritch_battery_es_to_mana,
        "a full-conversion mod should drive eldritch_battery_es_to_mana"
    );
    // Keystones that don't appear stay off.
    assert!(!ks.unbreakable);
    assert!(!ks.energy_shield_to_ward);
}

/// The EB flag (`EnergyShieldProtectsMana`, sourced from `Energy Shield protects Mana
/// instead of Life`, ModParser.lua:2439 — that text isn't covered by the current
/// engine rules/dataset, so the flag is injected directly) drives
/// `energy_shield_protects_mana`.
#[test]
fn eb_flag_from_injected_flag() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([Modifier::flag("EnergyShieldProtectsMana")]);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.energy_shield_protects_mana);
    assert!(
        !ks.eldritch_battery_es_to_mana,
        "the EB flag is not the same as a full ES->Mana conversion"
    );
}

/// IronReflexes (sourced from `Converts all Evasion Rating to Armour`,
/// ModParser.lua:2343 — text not covered by the current engine rules, injected
/// directly per the legacy expansion) carries both the flag (for cross-keystone
/// interactions) and `EvasionConvertToArmour` BASE 100 (the matrix data channel).
#[test]
fn iron_reflexes_flag_and_matrix_data_coexist() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        Modifier::flag("IronReflexes"),
        Modifier::number("EvasionConvertToArmour", ModType::Base, 100.0),
    ]);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);
    let conv = db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("EvasionConvertToArmour")],
    );

    // Assert: the flag only feeds the Unbreakable interaction; the numeric expansion
    // flows through the conversion matrix (BASE 100).
    assert!(ks.iron_reflexes);
    assert_eq!(conv, 100.0);
}

/// Snapshot semantics: the minimal path of directly-injected flag Modifiers
/// (non-text sources such as tree ingest).
#[test]
fn keystones_from_injected_flags() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        Modifier::flag("Unbreakable"),
        Modifier::flag("DoubleBodyArmourDefence"),
        Modifier::flag("WardNotBreak"),
        Modifier::flag("EternalLife"),
        Modifier::flag("BloodMagic"),
    ]);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.unbreakable);
    assert!(ks.double_body_armour_defence);
    assert!(ks.ward_not_break);
    assert!(ks.eternal_life);
    assert!(ks.blood_magic);
    assert!(!ks.chaos_inoculation);
}

// C-2: CI wiring (perform.rs's EhpOptions.chaos_inoculation is no longer hardcoded false)

/// CI build end-to-end: `Maximum Life is 1` -> Life=1 + ChaosInoculation flag ->
/// EHP draws from the ES pool, chaos max hit = infinity.
///
/// vendor basis: CalcDefence.lua:85 (flag read) /:120-123 (Life=1); CI grants chaos
/// immunity (agent-docs/active-defences.md §5 keystone table; EhpOptions::chaos_inoculation
/// semantics). Before this fix (pre C-2) the flag was hardcoded false in perform:
/// chaos_max_hit came out finite (a chaos pool of 1 + 500x0.5 = 251), and the EHP pool
/// still used the Life reading.
#[test]
fn ci_build_ehp_uses_es_pool_and_chaos_immunity() {
    use pobr_core::calc::MinimalInput;

    // Arrange: 1000 base life + 500 ES + CI.
    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 2.0,
    };
    let mut session = crate::support::session(input);
    session
        .add_modifier_texts(["+500 to maximum Energy Shield", "Maximum Life is 1"])
        .unwrap();

    // Act
    session.perform_minimal();
    let output = session.output().clone();

    // Assert: CI -> Life Override 1; chaos immunity; fire max hit = ES pool / (1-0%) = 500.
    assert_eq!(output.life, 1.0, "CI should override max life to 1");
    assert_eq!(output.energy_shield, 500.0);
    assert!(
        output.chaos_max_hit.is_infinite(),
        "CI should make chaos max hit = infinity (actual {})",
        output.chaos_max_hit
    );
    // After the F-3 switch, max hit follows PoB2's TotalHitPool: under CI that's
    // LifeRecoverable(1) + ES(500) = 501 (vendor :3540-3545, pool base includes Life 1;
    // the old reading of 500 was an ES-only-pool approximation).
    assert_eq!(
        output.fire_max_hit, 501.0,
        "under CI, the hit pool = Life(1) + ES(500)"
    );
    assert!(output.total_ehp.is_finite());
}

// C-3: the five-way defence resource conversion matrix + the Body Armour doubling flags
// vendor: CalcDefence.lua:1301-1390 (matrix), :1150-1290 / :806-808 (doubling flags)

use pobr_core::ModTag;
use pobr_core::calc::ActorBaseStats;
use pobr_core::calc::defence::calc_defence_resources;

/// A slot-scoped BASE mod (a test stand-in for a rolled item-level base value).
fn slot_base(name: &str, slot: &str, value: f64) -> Modifier {
    Modifier::number(name, ModType::Base, value).with_tag(ModTag::SlotName(slot.to_string()))
}

/// The matrix is an identity transform with no conversion mods / no keystones:
/// matches the old `scaled_defence_stat` formula bit for bit (the minimal witness
/// that the C-3 behavior commit leaves mod-free builds' values unchanged).
#[test]
fn matrix_is_identity_without_conversion_mods() {
    // Arrange: base 100 + bodyarmour slot base 200 + a global 50% increased Armour.
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 200.0),
        Modifier::number("Armour", ModType::Inc, 50.0),
    ]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        armour: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert: old formula total = 100x1.5 + 200x1.5 = 450; other resources untouched.
    assert_eq!(res.armour, 450.0);
    assert_eq!(res.evasion, 0.0);
    assert_eq!(res.energy_shield, 0.0);
    assert_eq!(res.extra_life, 0.0);
    assert_eq!(res.extra_mana, 0.0);
}

/// ConvertTo (defence -> defence): the slot base moves, slot by slot, into **the
/// target's matching slot bucket** and picks up the target's multipliers; the source
/// shrinks by (100-total)/100 (CalcDefence.lua:1340-1352).
#[test]
fn convert_to_moves_slot_base_into_target_slot_bucket() {
    // Arrange: bodyarmour Armour slot base 200 + a global 100% increased Evasion +
    // a conversion mod ("50% of Armour converted to Evasion Rating" isn't covered by
    // the current engine rules, so its numeric expansion is injected directly as
    // ArmourConvertToEvasion BASE 50).
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 200.0),
        Modifier::number("Evasion", ModType::Inc, 100.0),
        Modifier::number("ArmourConvertToEvasion", ModType::Base, 50.0),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert: armour keeps 200x0.5=100; the evasion slot bucket receives 100 x (1+100%) = 200.
    assert_eq!(res.armour, 100.0);
    assert_eq!(res.evasion, 200.0);
}

/// When ConvertTo rates sum to >100, they're normalized proportionally (capped at 100,
/// matching CalcDefence.lua:1315-1320's intent; vendor's own normalization loop is
/// dead code due to an `ipairs` misuse, so we implement the real normalization
/// per the design ruling).
#[test]
fn conversion_rates_over_100_are_normalised() {
    // Arrange: global armour base 100; Armour->Evasion 80 + Armour->ES 40 (total 120 -> x5/6).
    let mut db = ModDb::new();
    db.add_list([
        Modifier::number("ArmourConvertToEvasion", ModType::Base, 80.0),
        Modifier::number("ArmourConvertToEnergyShield", ModType::Base, 40.0),
    ]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        armour: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert: armour converts out entirely (total=100); evasion=100x(80x5/6)/100=66.67, ES=33.33.
    assert_eq!(res.armour, 0.0);
    assert!(
        (res.evasion - 200.0 / 3.0).abs() < 1e-6,
        "evasion={}",
        res.evasion
    );
    assert!(
        (res.energy_shield - 100.0 / 3.0).abs() < 1e-6,
        "es={}",
        res.energy_shield
    );
}

/// GainAs doesn't shrink the source (CalcDefence.lua:1336-1337: gainRate is added
/// to the rate, but totalConversion only counts ConvertTo -> the shrink factor is
/// unaffected by GainAs).
#[test]
fn gain_as_does_not_reduce_source() {
    // Arrange: global armour base 100 + ArmourGainAsEvasion 25.
    let mut db = ModDb::new();
    db.add_list([Modifier::number("ArmourGainAsEvasion", ModType::Base, 25.0)]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        armour: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert
    assert_eq!(res.armour, 100.0, "GainAs doesn't deduct the source");
    assert_eq!(res.evasion, 25.0);
}

/// defence -> non-defence (ES -> Mana, folding the existing es_to_mana_rate channel
/// into the matrix): slot base + global base convert into `extra_mana` at the given
/// rate (no rounding for defence sources, :1340-1355), with ES shrinking accordingly.
#[test]
fn es_to_mana_merged_into_matrix() {
    // Arrange: ES base 100 + bodyarmour slot base 400 + a partial conversion
    // ("30% of Maximum Energy Shield converted to Mana" isn't covered by the current
    // engine rules, so its numeric expansion is injected directly).
    let mut db = ModDb::new();
    db.add_list([
        slot_base("EnergyShield", "bodyarmour", 400.0),
        Modifier::number("EnergyShieldConvertToMana", ModType::Base, 30.0),
    ]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        energy_shield: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert: ES = (400+100)x0.7 = 350; extra_mana = (400+100)x0.3 = 150.
    assert_eq!(res.energy_shield, 350.0);
    assert_eq!(res.extra_mana, 150.0);
    assert_eq!(res.extra_life, 0.0);
}

/// A non-defence source (Life) -> a defence target: rounded up globally with ceil
/// (CalcDefence.lua:1364-1366); the source itself isn't deducted inside the matrix
/// (that's the doActorLifeManaSpirit domain, :73-126).
#[test]
fn non_defence_source_gain_uses_ceil() {
    // Arrange: base Life 1001 + "Gain 25% of Maximum Life as Extra Maximum Energy Shield".
    let mut db = ModDb::new();
    let outcome =
        crate::support::parse_mod("Gain 25% of Maximum Life as Extra Maximum Energy Shield")
            .expect("parse failed");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        life: 1001.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert: ceil(1001x0.25) = ceil(250.25) = 251.
    assert_eq!(res.energy_shield, 251.0);
}

/// Unbreakable: doubles the Body Armour slot's armour base (CalcDefence.lua:1217);
/// other slots are unaffected.
#[test]
fn unbreakable_doubles_body_armour_slot_armour() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 300.0),
        slot_base("Armour", "helmet", 100.0),
        Modifier::flag("Unbreakable"),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert: 300x2 + 100 = 700.
    assert_eq!(res.armour, 700.0);
}

/// DoubleBodyArmourDefence: doubles the Body Armour slot's armour/evasion/ES bases
/// alike (CalcDefence.lua:1189/:1214/:1232).
#[test]
fn double_body_armour_defence_doubles_all_three() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 100.0),
        slot_base("Evasion", "bodyarmour", 150.0),
        slot_base("EnergyShield", "bodyarmour", 200.0),
        Modifier::flag("DoubleBodyArmourDefence"),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert
    assert_eq!(res.armour, 200.0);
    assert_eq!(res.evasion, 300.0);
    assert_eq!(res.energy_shield, 400.0);
}

/// Unbreakable x IronReflexes interaction, end-to-end: the Body Armour slot's evasion
/// base doubles (:1235-1237 / :806-808), then converts entirely into armour via
/// `EvasionConvertToArmour` 100 (IronReflexes's numeric expansion).
#[test]
fn unbreakable_iron_reflexes_doubles_then_converts_evasion() {
    // Arrange: bodyarmour armour 100 + evasion 200 + Unbreakable + IronReflexes
    // (the mod text isn't covered by the current engine rules, so the flag + matrix
    // data are injected directly per the legacy expansion).
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 100.0),
        slot_base("Evasion", "bodyarmour", 200.0),
        Modifier::flag("Unbreakable"),
        Modifier::flag("IronReflexes"),
        Modifier::number("EvasionConvertToArmour", ModType::Base, 100.0),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert: armour slot base 100x2 (Unbreakable); evasion slot base 200x2
    // (the interaction) converts entirely into armour's bodyarmour slot bucket ->
    // armour = 200 + 400 = 600, evasion = 0.
    assert_eq!(res.armour, 600.0);
    assert_eq!(res.evasion, 0.0);
}

/// EnergyShieldToWard: gear ES slot bases no longer aggregate into ES (the conversion
/// to Ward is consumed by Track D, CalcDefence.lua:1192-1205); global flat ES from
/// non-gear sources is unaffected.
#[test]
fn energy_shield_to_ward_excludes_gear_es_bases() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        slot_base("EnergyShield", "bodyarmour", 300.0),
        Modifier::flag("EnergyShieldToWard"),
    ]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        energy_shield: 50.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert: gear's 300 doesn't aggregate; only the global 50 remains.
    assert_eq!(res.energy_shield, 50.0);
}

/// End-to-end (perform's injection path): a full ES->Mana conversion routes through
/// the matrix into MaximumMana BASE, zeroing out the ES panel (an equivalence
/// regression against the old es_to_mana_rate behavior).
#[test]
fn perform_injects_matrix_extra_mana() {
    use pobr_core::calc::MinimalInput;

    // Arrange: 100 base mana + 500 flat ES + a full-conversion mod.
    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 2.0,
    };
    let mut session = crate::support::session(input);
    session
        .add_modifier_texts([
            "+500 to maximum Energy Shield",
            "Converts all Energy Shield to Mana",
        ])
        .unwrap();

    // Act
    session.perform_minimal();
    let output = session.output().clone();

    // Assert: ES panel 0; Mana = 100 + 500 (the converted amount picks up Mana's
    // global multiplier, which is 1 here).
    assert_eq!(output.energy_shield, 0.0);
    assert_eq!(output.mana, 600.0);
}

/// The Total flat-add channel (vendor CalcDefence.lua:1331/:1394): `<Res>Total` BASE
/// (e.g. Discipline aura's `EnergyShieldTotal`) **adds straight to the final value
/// without going through inc/more**, unlike ordinary flat (which feeds base and picks
/// up inc). The old implementation mistakenly folded the Discipline buff into the
/// `EnergyShield` bucket where it picked up the global inc (the root cause of
/// essence-drain's ES being overcounted by 1.13x).
#[test]
fn total_channel_adds_flat_without_scaling() {
    let mut db = ModDb::new();
    db.add_list([
        Modifier::number("EnergyShield", ModType::Base, 100.0),
        Modifier::number("EnergyShield", ModType::Inc, 100.0),
        Modifier::number("EnergyShieldTotal", ModType::Base, 235.0),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // 100x(1+100%) + 235 (flat-added, no inc) = 435; the old misplaced reading would
    // have given (100+235)x2 = 670.
    assert_eq!(res.energy_shield, 435.0);
}

/// The Total channel propagates through conversions and shrinks accordingly
/// (vendor :1362-1366/:1388): with a 60% ES->Armour conversion, 60% of
/// EnergyShieldTotal flat-adds into Armour's final value (also without Armour's
/// inc), and the remaining 40% flat-adds to ES.
#[test]
fn total_channel_follows_conversion() {
    let mut db = ModDb::new();
    db.add_list([
        Modifier::number("EnergyShieldTotal", ModType::Base, 100.0),
        Modifier::number("EnergyShieldConvertToArmour", ModType::Base, 60.0),
        Modifier::number("Armour", ModType::Inc, 50.0),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Armour receives total 60 (flat-added, unaffected by its own 50% inc); ES keeps
    // the remaining 40.
    assert_eq!(res.armour, 60.0);
    assert_eq!(res.energy_shield, 40.0);
}
