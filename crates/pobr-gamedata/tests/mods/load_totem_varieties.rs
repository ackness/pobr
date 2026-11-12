//! Totem rows retain their source identities and usable monster parameters.

use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn target_rows_have_unique_effects_and_source_links() {
    let loaded = GameData::new(current_data_dir())
        .totem_varieties()
        .expect("totem_varieties should load");
    assert!(!loaded.totems.is_empty());
    assert!(
        loaded
            .totems
            .windows(2)
            .all(|pair| pair[0].granted_effect < pair[1].granted_effect)
    );
    for row in &loaded.totems {
        assert!(!row.granted_effect.is_empty());
        assert!(!row.active_skill.is_empty());
        assert!(row.monster_variety.starts_with("Metadata/Monsters/"));
        assert!(row.life_multiplier > 0);
    }
    for effect in [
        "ShockwaveTotemPlayer",
        "SummonMetaTotemBallistaPlayer",
        "AncestralWarriorTotemPlayer",
        "SiegeBallistaPlayer",
        "ArtilleryBallistaPlayer",
        "DarkEffigyPlayer",
        "SummonMetaTotemSpellTotemPlayer",
        "MetaMortarCannonPlayer",
    ] {
        assert!(
            loaded.totems.iter().any(|row| row.granted_effect == effect),
            "missing {effect}"
        );
    }
}
