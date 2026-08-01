use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

#[test]
fn manifest_describes_committed_bundle() {
    let manifest = game_data().manifest().expect("manifest should load");
    assert_eq!(manifest.poe_version, version());
    assert_eq!(manifest.schema_version, 2);
    assert!(manifest.domains.base.iter().any(|d| d == "base_items"));
    assert!(manifest.languages.iter().any(|l| l == "zh-TW"));
}

#[test]
fn base_items_load_with_resolved_foreign_keys() {
    let bases = game_data().base_items().expect("base_items should load");
    assert!(
        bases.len() > 1000,
        "should have thousands of bases, got {}",
        bases.len()
    );

    let hatchet = bases
        .iter()
        .find(|b| b.name == "Dull Hatchet")
        .expect("Dull Hatchet base should exist");
    // The foreign key is already resolved to a stable string ID (not an integer index).
    assert_eq!(hatchet.item_class, "One Hand Axe");
    assert!(hatchet.id.starts_with("Metadata/Items/Weapons/"));
    assert!(hatchet.tags.iter().any(|t| t == "ezomyte_basetype"));

    // Placeholder entries (e.g. [DNT-UNUSED]) are already filtered out.
    assert!(
        !bases.iter().any(|b| b.name.contains("[DNT")),
        "should not contain dev placeholder entries"
    );
}

///  Crossbow reload time is merged into the weapon section via overlay
/// (vendor `Data/Bases/crossbow.lua`'s Makeshift Crossbow
/// `ReloadTimeBase = 0.8`); non-crossbow weapons stay `None`.
#[test]
fn crossbow_reload_time_merged_from_overlay() {
    let bases = game_data().base_items().expect("base_items should load");
    let crossbow = bases
        .iter()
        .find(|b| b.name == "Makeshift Crossbow")
        .expect("Makeshift Crossbow base should exist");
    let weapon = crossbow
        .weapon
        .as_ref()
        .expect("a crossbow must have a weapon section");
    assert_eq!(weapon.reload_time_ms, Some(800));
    assert!(
        weapon.physical_min > 0,
        "merge must not disturb existing weapon values"
    );

    let hatchet = bases.iter().find(|b| b.name == "Dull Hatchet").unwrap();
    assert_eq!(
        hatchet.weapon.as_ref().and_then(|w| w.reload_time_ms),
        None,
        "non-crossbow weapons have no reload"
    );
}

#[test]
fn base_items_sorted_by_id_for_stable_diffs() {
    let bases = game_data().base_items().unwrap();
    let mut sorted = bases.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(bases, sorted, "base_items.json should be sorted by id");
}

#[test]
fn traditional_chinese_names_available_for_localization() {
    let names = game_data()
        .base_item_names("zh-TW")
        .expect("zh-TW sidecar should load");
    assert!(names.len() > 1000);
    // 磨刀石 (the zh-TW name) = Blacksmith's Whetstone
    let whetstone_id = "Metadata/Items/Currency/CurrencyWeaponQuality";
    assert_eq!(names.get(whetstone_id).map(String::as_str), Some("磨刀石"));
}
