use pobr_core::{CalcConfig, CharacterBase, ModDb};
use pobr_data::prelude::*;

fn db_of(mods: Vec<pobr_core::Modifier>) -> ModDb {
    let mut db = ModDb::new();
    db.add_list(mods);
    db
}

#[test]
fn character_base_derives_life_mana_accuracy_from_level_and_attributes() {
    // PoE2 0.5.0 base formulas (agent-docs/attributes.md):
    // life = 28 + 12*level + 2*Strength
    // mana = 34 + 4*level + 2*Intelligence
    // accuracy = 6*level + 6*Dexterity
    let base = CharacterBase {
        level: 1,
        strength: 15.0,
        dexterity: 7.0,
        intelligence: 7.0,
    };

    let db = db_of(base.modifiers());
    let cfg = CalcConfig::new();

    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumLife")]),
        70.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumMana")]),
        52.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("Accuracy")]),
        48.0
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

    let db = db_of(base.modifiers());
    let cfg = CalcConfig::new();

    // 28 + 12*10 = 148, 34 + 4*10 = 74, 6*10 = 60.
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumLife")]),
        148.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("MaximumMana")]),
        74.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("Accuracy")]),
        60.0
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

    for modifier in base.modifiers() {
        let origin = modifier
            .origin
            .as_ref()
            .expect("character base modifier carries an origin");
        assert_eq!(origin.source_id.kind, SourceKind::CharacterBase);
    }
}
