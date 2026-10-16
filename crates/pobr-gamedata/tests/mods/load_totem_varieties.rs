//! `overlay-common/totem_varieties.json` load test: the file equals the
//! built-in rows taken from the game tables `SkillTotemVariations`,
//! `ActiveSkills.SkillTotemId` and `MonsterVarieties.LifeMultiplier`.

use pobr_data::catalog::totem_varieties::TotemVarietiesDef;
use pobr_gamedata::{GameData, repo_data_root};

#[test]
fn json_matches_builtin_fallback() {
    let loaded = GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
        .totem_varieties()
        .expect("totem_varieties should load");
    assert_eq!(loaded, TotemVarietiesDef::default());
    let warrior = loaded
        .totems
        .iter()
        .find(|t| t.granted_effect == "AncestralWarriorTotemPlayer")
        .expect("Ancestral Warrior totem");
    assert_eq!(warrior.life_multiplier, 120);
}
