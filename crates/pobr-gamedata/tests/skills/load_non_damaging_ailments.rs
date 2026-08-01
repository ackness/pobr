//! `base/non_damaging_ailments.json` load tests.
//!
//! Migration invariant: for any value pobr's existing Rust already has a
//! source of truth for, the JSON must be value-equal to it — chill/shock
//! bounds are checked against `pobr_data::monster` and
//! `pobr_data::constants`'s pub consts; a damaging ailment's DoT type is
//! checked against `AilmentType::damage_type()`. Fields pobr doesn't have
//! (vendor-only) are spot-checked against hardcoded expected values per
//! vendor PoB2 line numbers.

use pobr_data::constants::{AilmentType, DamageType, SHOCK_MIN_EFFECT};
use pobr_data::monster::{
    BASE_SHOCK_MAGNITUDE, CHILL_MAX_EFFECT, CHILL_MIN_EFFECT, SHOCK_MAX_EFFECT,
};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// The chill/shock bounds are value-equal to pobr's Rust source-of-truth
/// consts (the core assertion for the migration invariant).
#[test]
fn chill_and_shock_bounds_match_rust_source_constants() {
    let table = game_data()
        .non_damaging_ailments()
        .expect("non_damaging_ailments should load");

    let chill = table.ailments.get("Chill").expect("Chill should exist");
    // Source of truth: pobr-data/src/monster.rs's CHILL_MIN_EFFECT=30 / CHILL_MAX_EFFECT=50.
    assert_eq!(chill.default, Some(CHILL_MIN_EFFECT));
    assert_eq!(chill.min, CHILL_MIN_EFFECT);
    assert_eq!(chill.max, CHILL_MAX_EFFECT);

    let shock = table.ailments.get("Shock").expect("Shock should exist");
    // Source of truth: pobr-data/src/monster.rs's BASE_SHOCK_MAGNITUDE=20 /
    // SHOCK_MAX_EFFECT=100; pobr-data/src/constants.rs's SHOCK_MIN_EFFECT=20
    // (the two consts share a value, both are sources of truth).
    assert_eq!(shock.default, Some(BASE_SHOCK_MAGNITUDE));
    assert_eq!(shock.min, BASE_SHOCK_MAGNITUDE);
    assert_eq!(shock.min, SHOCK_MIN_EFFECT);
    assert_eq!(shock.max, SHOCK_MAX_EFFECT);
}

/// Spot-checks vendor-only fields (pobr has no source of truth for these,
/// expected values hardcoded, vendor line numbers referenced).
#[test]
fn vendor_only_fields_match_pob2_data_lua() {
    let table = game_data().non_damaging_ailments().unwrap();
    assert_eq!(
        table.ailments.len(),
        3,
        "nonDamagingAilment has exactly three entries"
    );

    // Modules/Data.lua:348 (the Chill row) + Data/Misc.lua:91's BaseChillDuration=8.
    let chill = &table.ailments["Chill"];
    assert_eq!(chill.associated_type, DamageType::Cold);
    assert!(!chill.alt);
    assert_eq!(chill.precision, 0);
    assert_eq!(chill.duration, 8.0);

    // Modules/Data.lua:349 (the Freeze row, default=nil/min=0.3/max=3/precision=2)
    // + Data/Misc.lua:56's FreezeDuration=4.
    let freeze = &table.ailments["Freeze"];
    assert_eq!(freeze.associated_type, DamageType::Cold);
    assert!(!freeze.alt);
    assert_eq!(freeze.default, None);
    assert_eq!(freeze.min, 0.3);
    assert_eq!(freeze.max, 3.0);
    assert_eq!(freeze.precision, 2);
    assert_eq!(freeze.duration, 4.0);

    // Modules/Data.lua:350 (the Shock row) + Data/Misc.lua:93's BaseShockDuration=8.
    let shock = &table.ailments["Shock"];
    assert_eq!(shock.associated_type, DamageType::Lightning);
    assert!(!shock.alt);
    assert_eq!(shock.precision, 0);
    assert_eq!(shock.duration, 8.0);
}

/// buildupTypes matches vendor (Modules/Data.lua:353-376, vendor-only).
#[test]
fn buildup_types_match_pob2_data_lua() {
    let table = game_data().non_damaging_ailments().unwrap();
    assert_eq!(table.buildup_types.len(), 4);

    // Data.lua:354-357 Electrocute / :372-375 Pin: ScalesFrom is empty.
    assert!(table.buildup_types["Electrocute"].scales_from.is_empty());
    assert!(table.buildup_types["Pin"].scales_from.is_empty());
    // Data.lua:358-362 Freeze: Cold only.
    assert_eq!(
        table.buildup_types["Freeze"].scales_from,
        vec![DamageType::Cold]
    );
    // Data.lua:363-371 HeavyStun: all five damage types (canonical order).
    assert_eq!(
        table.buildup_types["HeavyStun"].scales_from,
        vec![
            DamageType::Physical,
            DamageType::Fire,
            DamageType::Cold,
            DamageType::Lightning,
            DamageType::Chaos,
        ]
    );
}

/// defaultAilmentDamageTypes: the DoT damage type is value-equal to the
/// Rust source of truth `AilmentType::damage_type()`
/// (pobr-data/src/constants.rs:124-134); ScalesFrom is vendor-only
/// (Modules/Data.lua:378-410).
#[test]
fn default_ailment_damage_types_match_rust_source_and_vendor() {
    let table = game_data().non_damaging_ailments().unwrap();
    assert_eq!(table.default_ailment_damage_types.len(), 5);

    // Damaging ailments: damage_type is value-equal to the Rust source of truth.
    for (key, ailment) in [
        ("Bleed", AilmentType::Bleed),
        ("Poison", AilmentType::Poison),
        ("Ignite", AilmentType::Ignite),
    ] {
        assert_eq!(
            table.default_ailment_damage_types[key].damage_type,
            ailment.damage_type(),
            "{key}'s DoT damage type should match AilmentType::damage_type()"
        );
    }
    // Non-damaging ailments have no DoT type (vendor's row has no
    // DamageType field; the source of truth likewise returns None).
    for (key, ailment) in [("Shock", AilmentType::Shock), ("Chill", AilmentType::Chill)] {
        assert_eq!(table.default_ailment_damage_types[key].damage_type, None);
        assert_eq!(ailment.damage_type(), None);
    }

    // ScalesFrom spot checks (vendor-only): Data.lua:386-392 Poison's dual
    // physical+chaos source, :400-404 Shock is lightning only, :405-409
    // Chill is cold only.
    assert_eq!(
        table.default_ailment_damage_types["Poison"].scales_from,
        vec![DamageType::Physical, DamageType::Chaos]
    );
    assert_eq!(
        table.default_ailment_damage_types["Shock"].scales_from,
        vec![DamageType::Lightning]
    );
    assert_eq!(
        table.default_ailment_damage_types["Chill"].scales_from,
        vec![DamageType::Cold]
    );
}

/// The committed JSON is byte-identical to a serde-pretty round trip (the
/// reproducibility rule: no hand edits, regenerating gives a zero byte-diff).
#[test]
fn committed_json_is_serde_pretty_roundtrip_stable() {
    let path = repo_data_root()
        .join(version())
        .join("base/non_damaging_ailments.json");
    let committed = std::fs::read_to_string(&path).expect("failed to read the committed JSON");
    let table = game_data().non_damaging_ailments().unwrap();
    let regenerated = serde_json::to_string_pretty(&table).expect("serialize");
    assert_eq!(
        committed.trim_end(),
        regenerated,
        "non_damaging_ailments.json should be byte-identical to serde pretty output (including key order)"
    );
}
