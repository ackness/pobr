//! Totem rows retain their source identities and usable monster parameters.

use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn target_rows_have_unique_effects_and_source_links() {
    let data = GameData::new(current_data_dir());
    let loaded = data.totem_varieties().expect("totem_varieties should load");
    let effects: std::collections::BTreeMap<_, _> = data
        .granted_effects()
        .unwrap()
        .into_iter()
        .map(|effect| (effect.id.clone(), effect))
        .collect();
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
        let effect = effects
            .get(&row.granted_effect)
            .expect("source granted effect");
        assert_eq!(
            effect.active_skill.as_deref(),
            Some(row.active_skill.as_str())
        );
    }
    let active_skills: std::collections::BTreeSet<_> = loaded
        .totems
        .iter()
        .map(|row| row.active_skill.as_str())
        .collect();
    let expected: std::collections::BTreeSet<_> = effects
        .values()
        .filter(|effect| {
            effect
                .active_skill
                .as_deref()
                .is_some_and(|id| active_skills.contains(id))
        })
        .map(|effect| effect.id.as_str())
        .collect();
    let actual: std::collections::BTreeSet<_> = loaded
        .totems
        .iter()
        .map(|row| row.granted_effect.as_str())
        .collect();
    assert_eq!(expected, actual, "every source alias is retained");
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
