//! `overlay/curse_priority.json` load tests.
//!
//! Spot checks against vendor `Modules/Data.lua:274-300`'s
//! `data.cursePriority` table (commit `2df5a74`); missing-table tolerance
//! (the consumer falls back).

use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// All four sections present + a spot check on vendor's values (Data.lua:275-300).
#[test]
fn curse_priority_sections_and_samples() {
    let def = game_data()
        .curse_priority()
        .expect("curse_priority should load")
        .expect("curse_priority.json should be present");

    // Per-curse base values: 13 entries as of writing (Temporal Chains=1 … Poacher's Mark=13)
    assert_eq!(def.curse_base.len(), 13);
    assert_eq!(def.curse_base["Temporal Chains"], 1);
    assert_eq!(def.curse_base["Despair"], 8);
    assert_eq!(def.curse_base["Poacher's Mark"], 13);

    // Unit weight per socket index
    assert_eq!(def.socket_priority_base, 100);

    // Equipment slot-name weight: 10 slots (Weapon 1=1000 … Ring 3=10000)
    assert_eq!(def.slot_weights.len(), 10);
    assert_eq!(def.slot_weights["Weapon 1"], 1000);
    assert_eq!(def.slot_weights["Body Armour"], 5000);
    assert_eq!(def.slot_weights["Ring 3"], 10000);

    // Source weights: aura is always the highest band
    assert_eq!(def.curse_from_equipment, 11000);
    assert_eq!(def.curse_from_aura, 20000);
    assert!(def.curse_from_aura > def.curse_from_equipment);
}

/// Missing-table tolerance: a version directory with no overlay table
/// returns `Ok(None)` rather than erroring.
#[test]
fn curse_priority_tolerates_missing_table() {
    let missing = GameData::new(repo_data_root().join("no-such-version"));
    assert!(
        missing
            .curse_priority()
            .expect("a missing table should be tolerated as Ok(None)")
            .is_none()
    );
}
