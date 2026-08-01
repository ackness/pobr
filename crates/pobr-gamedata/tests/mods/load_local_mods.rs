//! `overlay/local_mods.json` load tests.
//!
//! Migration-invariant check: the JSON content must be **value-equal** to
//! the built-in fallback [`LocalModsDef::default`] (= a mirror of
//! `pobr-build::calc_orchestrator::is_weapon_local_mod`'s original
//! hardcoded enum) — both paths (reading the file / degrading on a
//! missing file) must behave identically, so the wiring switchover is
//! zero parity change.

use pobr_data::catalog::local_mods::LocalModsDef;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> LocalModsDef {
    GameData::new(repo_data_root().join(version()))
        .local_mods()
        .expect("local_mods should load")
}

/// The JSON is value-equal to the built-in fallback (the core assertion
/// for the migration invariant: the file path and the degraded path must
/// give the same whitelist).
#[test]
fn json_matches_builtin_fallback_mirror() {
    assert_eq!(load(), LocalModsDef::default());
}

/// The weapon whitelist is value-equal to the original
/// `is_weapon_local_mod` hardcoded enum (lowercase clean-text convention):
/// two increased suffixes + one adds-damage suffix.
#[test]
fn weapon_whitelist_matches_original_hardcode() {
    let def = load();
    assert_eq!(
        def.weapon.increased_suffixes,
        vec![
            "% increased physical damage".to_string(),
            "% increased attack speed".to_string(),
        ]
    );
    assert_eq!(
        def.weapon.adds_damage_suffixes,
        vec!["physical damage".to_string()]
    );
}

/// Whitelist entries must be all lowercase (matching `clean_item_text`'s
/// output convention, case-sensitive matching).
#[test]
fn whitelist_entries_are_lowercase() {
    let def = load();
    for entry in def
        .weapon
        .increased_suffixes
        .iter()
        .chain(&def.weapon.adds_damage_suffixes)
    {
        assert_eq!(
            entry,
            &entry.to_lowercase(),
            "whitelist entry {entry:?} contains uppercase characters"
        );
    }
}
