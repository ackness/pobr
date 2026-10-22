//! Bear form: a skill of the game types `Shapeshift` and `Bear` puts its user
//! in the form, and "While in Bear form, you gain: +10 to Armour per level;
//! 30% of Armour also applies to Elemental Damage Types". Lines for "Bear
//! Skills" are for the skills of that type.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::OutputTable;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::prelude::DamageType;

use crate::test_data as data;

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

/// The `kind` damage of the average non-critical hit.
fn hit(out: &OutputTable, kind: DamageType) -> f64 {
    out.damage_components
        .iter()
        .filter(|component| component.damage_type == kind)
        .map(|component| component.avg())
        .sum()
}

/// "Bear Skills Convert 80% of Physical Damage to Fire Damage" (the game stat
/// `non_skill_physical_damage_%_to_convert_to_fire_with_bear_skills`) turns
/// four fifths of a Bear skill's Physical damage into Fire, and does nothing
/// for other skills.
#[test]
fn bear_skills_convert_physical_damage_to_fire() {
    let data = data();
    let line = ["Bear Skills Convert 80% of Physical Damage to Fire Damage"];
    let maul = calculate(&data, "BearMaulPlayer", &line);
    let physical = hit(&maul, DamageType::Physical);
    let fire = hit(&maul, DamageType::Fire);
    assert!(
        (fire / (physical + fire) - 0.8).abs() < 1e-9,
        "{physical} Physical, {fire} Fire"
    );

    let chop = calculate(&data, "AxeChopPlayer", &line);
    let plain = calculate(&data, "AxeChopPlayer", &[]);
    assert_eq!(chop.damage_components, plain.damage_components);
    assert_eq!(hit(&chop, DamageType::Fire), 0.0);
}
