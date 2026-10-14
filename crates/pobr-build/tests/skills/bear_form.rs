//! Bear form: a skill of the game types `Shapeshift` and `Bear` puts its user
//! in the form, and "While in Bear form, you gain: +10 to Armour per level;
//! 30% of Armour also applies to Elemental Damage Types".

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::OutputTable;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// A Talisman without implicit modifiers.
fn talisman() -> Item {
    Item {
        base: ItemBaseId::from("Wildwood Talisman"),
        rarity: ItemRarity::Normal,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec![],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: vec![],
    }
}

/// A level 90 Warrior using `skill` with a Talisman, with `lines` among the
/// character's modifiers.
fn calculate(data: &BuildData, skill: &str, lines: &[&str]) -> OutputTable {
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_active_skill(skill, 1))
        .set_item(EquipmentSlot::Weapon1, talisman());
    let options = DataOrchestratorOptions {
        extra_modifier_texts: lines.iter().map(|line| line.to_string()).collect(),
        ..DataOrchestratorOptions::default()
    };
    calculate_with_data(&build, data, &options).expect("calculate")
}

#[test]
fn a_bear_skill_gives_the_armour_of_bear_form() {
    let data = data();
    let maul = calculate(&data, "BearMaulPlayer", &[]);
    let chop = calculate(&data, "AxeChopPlayer", &[]);
    // +10 to Armour per level, at level 90.
    assert_eq!(maul.armour - chop.armour, 900.0);
    // The same Armour without the form: only the form's Armour also applies
    // to Elemental damage.
    let armoured = calculate(&data, "AxeChopPlayer", &["+900 to Armour"]);
    assert_eq!(armoured.armour, maul.armour);
    assert!(
        maul.fire_max_hit > armoured.fire_max_hit,
        "{} vs {}",
        maul.fire_max_hit,
        armoured.fire_max_hit
    );
}
