//! `base/unarmed_data.json` load tests: value-by-value comparison against
//! pobr's existing Rust source of truth,
//! `pobr-build::calc_orchestrator::unarmed_contribution` (a migration
//! invariant); vendor-only fields (class_id / weapon_type) are spot-checked
//! against vendor line numbers.

use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
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
    let entries = game_data()
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
        // pobr's current value is 0.05 (vendor's is the percentage 5; the
        // discrepancy is already recorded in the schema's TODO(parity),
        // not fixed by this task).
        assert_eq!(e.crit_chance, crit, "{} crit_chance", e.class_name);
    }
}

/// Spot-checks vendor-only fields (vendor `src/Modules/Data.lua:554-562`:
/// the classId table key + the trailing class-name comment; `type = "None"`).
#[test]
fn vendor_only_fields_sampled() {
    let entries = game_data().unarmed_data().unwrap();

    // The full classId → class-name set (Data.lua:554-562's trailing
    // comments; PoE2 skips ids 3/4/5, they don't exist).
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

    // Spot check: Warrior (Data.lua:557) has PhysicalMax = 8; type = "None".
    let warrior = entries.iter().find(|e| e.class_id == 6).unwrap();
    assert_eq!(warrior.class_name, "Warrior");
    assert_eq!(warrior.weapon_type, "None");
    assert_eq!(warrior.physical_max, 8.0);

    // Spot check: Witch (Data.lua:555) has PhysicalMax = 5, AttackRate = 1.65.
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
    let entries = game_data().unarmed_data().unwrap();
    let mut sorted = entries.clone();
    sorted.sort_by_key(|e| e.class_id);
    assert_eq!(
        entries, sorted,
        "unarmed_data.json should be sorted by class_id"
    );
}
