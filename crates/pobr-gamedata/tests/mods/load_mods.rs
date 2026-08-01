use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

#[test]
fn manifest_lists_mods_and_stats_domains() {
    let manifest = game_data().manifest().expect("manifest should load");
    assert!(manifest.domains.base.iter().any(|d| d == "mods"));
    assert!(manifest.domains.base.iter().any(|d| d == "stats"));
}

#[test]
fn stats_registry_loads_with_stable_ids() {
    let stats = game_data().stats().expect("stats should load");
    assert!(
        stats.len() > 10000,
        "the stat registry should have tens of thousands of entries, got {}",
        stats.len()
    );

    let str_stat = stats
        .iter()
        .find(|s| s.id == "additional_strength")
        .expect("additional_strength stat should exist");
    assert!(!str_stat.is_local);
    assert_eq!(str_stat.semantic, Some(3));
}

#[test]
fn stats_sorted_by_id_for_stable_diffs() {
    let stats = game_data().stats().unwrap();
    let mut sorted = stats.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(stats, sorted, "stats.json should be sorted by id");
}

#[test]
fn mods_load_with_resolved_stat_foreign_keys() {
    let mods = game_data().mods().expect("mods should load");
    assert!(
        mods.len() > 10000,
        "should have tens of thousands of mods, got {}",
        mods.len()
    );

    let brute = mods
        .iter()
        .find(|m| m.id == "Strength1")
        .expect("Strength1 mod should exist");
    assert_eq!(brute.name.as_deref(), Some("of the Brute"));
    assert_eq!(brute.domain, 1);
    assert_eq!(brute.generation_type, Some(2));
    assert_eq!(brute.level, 1);

    // The Stat foreign key is already resolved to a stable string stat id
    // (not an integer index), and the roll range is already merged in.
    assert_eq!(brute.stats.len(), 1);
    let slot = &brute.stats[0];
    assert_eq!(slot.stat_id, "additional_strength");
    assert_eq!(slot.min, 5);
    assert_eq!(slot.max, 8);
}

#[test]
fn every_mod_stat_id_resolves_against_registry() {
    let stats = game_data().stats().unwrap();
    let registry: std::collections::HashSet<&str> = stats.iter().map(|s| s.id.as_str()).collect();
    let mods = game_data().mods().unwrap();

    // Spot check: every mod's stat foreign key must resolve against the
    // registry (no dangling integer-index leftovers).
    for m in &mods {
        for s in &m.stats {
            assert!(
                registry.contains(s.stat_id.as_str()),
                "mod {}'s stat_id {} is not in the stat registry",
                m.id,
                s.stat_id
            );
        }
    }
}

#[test]
fn mods_sorted_by_id_for_stable_diffs() {
    let mods = game_data().mods().unwrap();
    let mut sorted = mods.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(mods, sorted, "mods.json should be sorted by id");
}

#[test]
fn traditional_chinese_mod_names_available() {
    let names = game_data()
        .mod_names("zh-TW")
        .expect("zh-TW mod name sidecar should load");
    assert!(
        names.len() > 1000,
        "should have thousands of localized mod names, got {}",
        names.len()
    );
    // Strength1 = "of the Brute" → "野蠻之" (the zh-TW export switched from
    // the "之X" to the "X之" word order across the board in 0.5.4; the
    // pipeline faithfully transcribes the Mods table's Name column)
    assert_eq!(names.get("Strength1").map(String::as_str), Some("野蠻之"));
}
