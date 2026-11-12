//! Character constants retain the shape and units needed by level derivation.

use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_gamedata::{GameData, current_data_dir};

fn load() -> CharacterConstantsDef {
    GameData::new(current_data_dir())
        .character_constants()
        .expect("character_constants should load")
}

#[test]
fn manifest_registers_character_constants_in_base() {
    let manifest = GameData::new(current_data_dir()).manifest().unwrap();
    assert!(
        manifest
            .domains
            .base
            .iter()
            .any(|d| d == "character_constants")
    );
}

#[test]
fn derivation_constants_are_finite_and_growth_is_nonnegative() {
    let c = load();
    let value = serde_json::to_value(c).unwrap();
    for (name, value) in value.as_object().unwrap() {
        assert!(value.as_f64().is_some_and(f64::is_finite), "{name}");
    }
    for growth in [
        c.life_per_level,
        c.life_per_strength,
        c.mana_per_level,
        c.mana_per_intelligence,
        c.accuracy_per_level,
        c.accuracy_per_dexterity,
        c.strength_per_level,
        c.dexterity_per_level,
        c.intelligence_per_level,
        c.base_evasion,
        c.ward_regeneration_per_minute_pct,
    ] {
        assert!(growth >= 0.0, "growth {growth}");
    }
    for (base, per_level) in [
        (c.base_life_constant, c.life_per_level),
        (c.base_mana_constant, c.mana_per_level),
        (c.base_accuracy_constant, c.accuracy_per_level),
    ] {
        assert!(base + per_level >= 0.0, "level-one base");
        assert!(base + 99.0 * per_level >= base + per_level, "level growth");
    }
}

#[test]
fn json_roundtrips_through_schema() {
    let c = load();
    let text = serde_json::to_string(&c).unwrap();
    let back: CharacterConstantsDef = serde_json::from_str(&text).unwrap();
    assert_eq!(c, back);
}
