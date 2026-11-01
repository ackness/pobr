//! Target totem rows include every matched GrantedEffect alias. Existing
//! fallback rows retain their values; target-only aliases are not discarded.

use pobr_data::catalog::totem_varieties::TotemVarietiesDef;
use pobr_gamedata::{GameData, repo_data_root};

#[test]
fn target_rows_preserve_builtin_rows_and_include_source_aliases() {
    let loaded = GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
        .totem_varieties()
        .expect("totem_varieties should load");
    assert_eq!(loaded.totems.len(), 11);
    assert!(
        loaded
            .totems
            .windows(2)
            .all(|pair| pair[0].granted_effect < pair[1].granted_effect)
    );
    for expected in TotemVarietiesDef::default().totems {
        let actual = loaded
            .totems
            .iter()
            .find(|row| row.granted_effect == expected.granted_effect)
            .expect("every legacy effect remains source-bound");
        assert_eq!(actual, &expected);
    }
    let warrior = loaded
        .totems
        .iter()
        .find(|t| t.granted_effect == "AncestralWarriorTotemPlayer")
        .expect("Ancestral Warrior totem");
    assert_eq!(warrior.life_multiplier, 120);
}
