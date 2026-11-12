//! Local-mod rules use the clean-text suffix convention of their consumer.

use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn weapon_suffixes_are_nonempty_unique_and_lowercase() {
    let def = GameData::new(current_data_dir())
        .local_mods()
        .expect("local_mods should load");
    for suffixes in [
        &def.weapon.increased_suffixes,
        &def.weapon.adds_damage_suffixes,
    ] {
        assert!(!suffixes.is_empty());
        let mut seen = std::collections::BTreeSet::new();
        for entry in suffixes {
            assert!(!entry.is_empty());
            assert_eq!(entry, entry.trim());
            assert_eq!(entry, &entry.to_lowercase());
            assert!(seen.insert(entry), "duplicate suffix {entry:?}");
        }
    }
    assert!(
        def.weapon
            .increased_suffixes
            .iter()
            .all(|suffix| suffix.starts_with("% "))
    );
    assert!(
        def.weapon
            .adds_damage_suffixes
            .iter()
            .all(|suffix| suffix.ends_with(" damage"))
    );
}
