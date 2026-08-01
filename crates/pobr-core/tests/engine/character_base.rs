use pobr_core::{CalcConfig, CharacterBase, ModDb};
use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_data::prelude::*;

fn db_of(mods: Vec<pobr_core::Modifier>) -> ModDb {
    let mut db = ModDb::new();
    db.add_list(mods);
    db
}

/// Test constant set = Default fallback (matches `base/character_constants.json`
/// value-for-value).
fn constants() -> CharacterConstantsDef {
    CharacterConstantsDef::default()
}

#[test]
fn character_base_derives_life_mana_accuracy_from_level_and_attributes() {
    // PoB2 CalcSetup characterConstants (oracle-verified L99: Life 1204 / Mana 426):
    // life = 12*level + 16 + 2*Strength
    // mana = 4*level + 30 + 2*Intelligence
    // accuracy = 6*level - 6 + 6*Dexterity
    let base = CharacterBase {
        level: 1,
        strength: 15.0,
        dexterity: 7.0,
        intelligence: 7.0,
    };

    let db = db_of(base.modifiers(&constants()));
    let cfg = CalcConfig::new();

    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumLife")]),
        58.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumMana")]),
        48.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("Accuracy")]),
        42.0
    );
    // PoB2 characterConstants.base_evasion_rating = 7.
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("Evasion")]),
        7.0
    );
}

#[test]
fn character_base_scales_with_level() {
    let base = CharacterBase {
        level: 10,
        strength: 0.0,
        dexterity: 0.0,
        intelligence: 0.0,
    };

    let db = db_of(base.modifiers(&constants()));
    let cfg = CalcConfig::new();

    // 12*10 + 16 = 136, 4*10 + 30 = 70, 6*10 - 6 = 54.
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumLife")]),
        136.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumMana")]),
        70.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("Accuracy")]),
        54.0
    );
}

#[test]
fn character_base_modifiers_carry_character_base_source() {
    let base = CharacterBase {
        level: 1,
        strength: 1.0,
        dexterity: 1.0,
        intelligence: 1.0,
    };

    for modifier in base.modifiers(&constants()) {
        let origin = modifier
            .origin
            .as_ref()
            .expect("character base modifier carries an origin");
        assert_eq!(origin.source_id.kind, SourceKind::CharacterBase);
    }
}
