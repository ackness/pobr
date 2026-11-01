//! Historical fallback invariants use their declared external 0.5.5.4.2 pack.
//! Current source rows preserve source ordering and append explicit compatibility
//! entries; they need not equal the old rounded vendor table.

use pobr_gamedata::{GameData, repo_data_root};

fn historical_data() -> GameData {
    GameData::new(repo_data_root().join("0.5.5.4.2"))
}

/// The per-class expected values from pobr's source of truth
/// (`calc_orchestrator.rs::unarmed_contribution`):
/// `(phys_min, phys_max, attack_rate, crit_chance)`. phys_max is matched by
/// class_name (Warrior 8, Scion/Mercenary/Druid 6, everyone else 5); the
/// other three are the same for every class.
fn rust_source_expectation(class_name: &str) -> (f64, f64, f64, f64) {
    let phys_max = match class_name {
        "Warrior" => 8.0,
        "Scion" | "Mercenary" | "Druid" => 6.0,
        _ => 5.0, // Witch/Ranger/Sorceress/Huntress/Monk
    };
    (2.0, phys_max, 1.65, 0.05)
}

/// The full table is value-equal to pobr's existing Rust table (a
/// migration invariant: the values don't change within this commit).
#[test]
fn values_match_existing_rust_table() {
    let entries = historical_data()
        .unarmed_data()
        .expect("unarmed_data should load");
    assert_eq!(
        entries.len(),
        9,
        "vendor unarmedWeaponData has 9 class entries"
    );

    for e in &entries {
        let (min, max, rate, crit) = rust_source_expectation(&e.class_name);
        assert_eq!(e.physical_min, min, "{} physical_min", e.class_name);
        assert_eq!(e.physical_max, max, "{} physical_max", e.class_name);
        assert_eq!(e.attack_rate, rate, "{} attack_rate", e.class_name);
        // Serialized legacy fraction remains 0.05; the calc boundary
        // converts it to vendor's 5 percentage points.
        assert_eq!(e.crit_chance, crit, "{} crit_chance", e.class_name);
    }
}

/// Spot-checks vendor-only fields (vendor `src/Modules/Data.lua:627-635`:
/// the classId table key + the trailing class-name comment; `type = "None"`).
#[test]
fn vendor_only_fields_sampled() {
    let entries = historical_data().unarmed_data().unwrap();

    // The full classId → class-name set (Data.lua:627-635's trailing
    // comments; this historical vendor table omits ids 3/4/5).
    let expected_ids: &[(u32, &str)] = &[
        (0, "Scion"),
        (1, "Witch"),
        (2, "Ranger"),
        (6, "Warrior"),
        (7, "Sorceress"),
        (8, "Huntress"),
        (9, "Mercenary"),
        (10, "Monk"),
        (11, "Druid"),
    ];
    let actual: Vec<(u32, &str)> = entries
        .iter()
        .map(|e| (e.class_id, e.class_name.as_str()))
        .collect();
    assert_eq!(
        actual, expected_ids,
        "classId↔class name should match vendor and be ascending by class_id"
    );

    // Spot check: Warrior (Data.lua:630) has PhysicalMax = 8; type = "None".
    let warrior = entries.iter().find(|e| e.class_id == 6).unwrap();
    assert_eq!(warrior.class_name, "Warrior");
    assert_eq!(warrior.weapon_type, "None");
    assert_eq!(warrior.physical_max, 8.0);

    // Spot check: Witch (Data.lua:628) has PhysicalMax = 5, AttackRate = 1.65.
    let witch = entries.iter().find(|e| e.class_id == 1).unwrap();
    assert_eq!(witch.physical_max, 5.0);
    assert_eq!(witch.attack_rate, 1.65);

    // weapon_type is "None" across the whole table (every row has `type = "None"`).
    assert!(entries.iter().all(|e| e.weapon_type == "None"));
}

/// The array is ascending by class_id (a diff-friendly convention, matching
/// the other base tables' sort order).
#[test]
fn sorted_by_class_id_for_stable_diffs() {
    let entries = historical_data().unarmed_data().unwrap();
    let mut sorted = entries.clone();
    sorted.sort_by_key(|e| e.class_id);
    assert_eq!(
        entries, sorted,
        "unarmed_data.json should be sorted by class_id"
    );
}

/// The ordinary source projection keeps all twelve Characters rows before the
/// retained Scion entry. Its duplicate legacy class_id does not reorder source
/// rows or replace target Marauder; the consumer resolves by class name.
#[test]
fn target_source_order_precedes_explicit_legacy_compatibility() {
    let entries = GameData::new(pobr_gamedata::current_data_dir())
        .unarmed_data()
        .expect("current source unarmed data");
    let (legacy, source) = entries.split_last().expect("nonempty source projection");
    assert_eq!(source.len(), 12);
    assert_eq!(
        source
            .iter()
            .map(|entry| entry.class_id)
            .collect::<Vec<_>>(),
        (0..12).collect::<Vec<_>>()
    );
    assert_eq!(source[0].class_name, "Marauder");
    assert_eq!(legacy.class_name, "Scion");
    let historical = historical_data().unarmed_data().unwrap();
    assert_eq!(
        legacy,
        historical
            .iter()
            .find(|entry| entry.class_name == "Scion")
            .unwrap()
    );
    assert!(
        source
            .iter()
            .all(|entry| entry.attack_rate == 1000.0 / 606.0)
    );
}
