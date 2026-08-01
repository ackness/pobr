//! Load tests for the data prerequisites: `overlay/mod_scalability.json` /
//! `overlay/catalysts.json` / `overlay/runes.json` / `overlay/uniques.json`
//! (spot checks, vendor line numbers noted as the source, commit `2df5a74`).

use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// mod_scalability: entry-count order of magnitude + templates ascending +
/// spot checks on `"# Armour per 2 Strength"` (ModScalability.lua:10) and
/// `"#"` (:9, formats=passive_keystone_index).
#[test]
fn mod_scalability_samples() {
    let def = game_data()
        .mod_scalability()
        .unwrap()
        .expect("mod_scalability.json should be present");
    assert!(
        def.entries.len() >= 15000,
        "ModScalability.lua has ~15037 line entries (got {})",
        def.entries.len()
    );
    assert!(
        def.entries
            .windows(2)
            .all(|w| w[0].template < w[1].template),
        "templates should be strictly ascending (unique key)"
    );
    let find = |t: &str| {
        def.entries
            .iter()
            .find(|e| e.template == t)
            .unwrap_or_else(|| panic!("template {t:?} missing"))
    };
    let armour = find("# Armour per 2 Strength");
    assert_eq!(armour.slots.len(), 1);
    assert!(armour.slots[0].is_scalable);
    assert!(armour.slots[0].formats.is_empty());
    let keystone = find("#");
    assert_eq!(keystone.slots[0].formats, ["passive_keystone_index"]);
    // The isScalable=false path (ModScalability.lua:14's Avatar of Fire row)
    let unscalable = find("# Armour while you do not have Avatar of Fire");
    assert!(!unscalable.slots[0].is_scalable);
}

/// catalysts: 13 entries merged from parallel arrays
/// (Classes/Item.lua:14-29, 0.5.4 added Necrotic/Minion); the 3rd entry =
/// Carapace / Defence / {defences, armour, evasion, energyshield}.
#[test]
fn catalysts_thirteen_entries() {
    let def = game_data()
        .catalysts()
        .unwrap()
        .expect("catalysts.json should be present");
    assert_eq!(def.catalysts.len(), 13);
    for (i, c) in def.catalysts.iter().enumerate() {
        assert_eq!(c.id as usize, i + 1, "id should be 1-based and contiguous");
    }
    let third = &def.catalysts[2];
    assert_eq!(third.name, "Carapace");
    assert_eq!(third.descriptor, "Defence");
    assert_eq!(
        third.tags,
        ["defences", "armour", "evasion", "energyshield"]
    );
    // A spot check on the first/last entries (Item.lua:14-15)
    assert_eq!(def.catalysts[0].name, "Flesh");
    assert_eq!(def.catalysts[0].descriptor, "Life");
    assert_eq!(def.catalysts[12].name, "Necrotic");
    assert_eq!(def.catalysts[12].descriptor, "Minion");
}

/// runes: a spot check on Hayoxi's Soul Core's helmet row
/// (ModRunes.lua:5-13) and Desert Rune's weapon slot (:660-666).
#[test]
fn runes_samples() {
    let def = game_data()
        .runes()
        .unwrap()
        .expect("runes.json should be present");
    assert!(
        def.runes.len() >= 250,
        "rune entry count order of magnitude (measured 283)"
    );
    let find = |name: &str| {
        def.runes
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("rune {name:?} missing"))
    };
    let hayoxi = find("Hayoxi's Soul Core of Heatproofing");
    let helmet = &hayoxi.slots["helmet"];
    assert_eq!(helmet.kind, "SoulCore");
    assert_eq!(helmet.lines, ["+40% of Armour also applies to Cold Damage"]);
    assert_eq!(helmet.rank, [50.0]);
    let desert = find("Desert Rune");
    let weapon = &desert.slots["weapon"];
    assert_eq!(weapon.kind, "Rune");
    assert_eq!(
        weapon.lines,
        [
            "Adds 7 to 11 Fire Damage",
            "Bonded: 30% increased Ignite Magnitude"
        ]
    );
    // vendor 0.5.4 (ModRunes.lua:737-746): statOrder = {832, 1077}
    assert_eq!(weapon.stat_order, [832.0, 1077.0]);
}

/// uniques: two layers — The Anvil's (Data/Uniques/amulet.lua:5-15) raw
/// block kept byte-for-byte + the pre-parsed index (name/base/variants).
#[test]
fn uniques_double_layer() {
    let def = game_data()
        .uniques()
        .unwrap()
        .expect("uniques.json should be present");
    assert!(
        def.uniques.len() >= 350,
        "unique entry count order of magnitude (measured 392)"
    );
    let anvil = def
        .uniques
        .iter()
        .find(|u| u.name == "The Anvil")
        .expect("The Anvil should be present");
    assert_eq!(anvil.base, "Bloodstone Amulet");
    assert_eq!(anvil.item_type, "amulet");
    assert_eq!(anvil.variants, ["Pre 0.2.0", "Pre 0.4.0", "Current"]);
    // The raw layer keeps vendor's annotations byte-for-byte ({tags:...}
    // and (min-max) range shapes)
    assert!(anvil.raw.contains("{tags:life}+(30-40) to maximum Life"));
    assert!(anvil.raw.contains("Variant: Pre 0.2.0"));
    // The index layer is minimal: League/Source lines (stored when present)
    assert!(
        def.uniques.iter().any(|u| u.league.is_some()),
        "League line pre-parsing should be present"
    );
    assert!(
        def.uniques.iter().any(|u| u.source.is_some()),
        "Source line pre-parsing should be present"
    );
    // item_type covers vendor's main itemTypes domains
    for ty in ["amulet", "body", "helmet", "ring", "bow", "flask", "jewel"] {
        assert!(
            def.uniques.iter().any(|u| u.item_type == ty),
            "item_type {ty} missing"
        );
    }
}

/// Missing-table tolerance: in an empty directory, all four new domains
/// return Ok(None), no panic.
#[test]
fn missing_overlay_files_yield_none() {
    let dir = std::env::temp_dir().join(format!("pobr-pre-m5c-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data = GameData::new(&dir);
    assert!(data.mod_scalability().unwrap().is_none());
    assert!(data.catalysts().unwrap().is_none());
    assert!(data.runes().unwrap().is_none());
    assert!(data.uniques().unwrap().is_none());
}
