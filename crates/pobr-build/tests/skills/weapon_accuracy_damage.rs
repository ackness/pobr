//! "Attacks using your Weapons have Added Physical Damage equal to 25% of the
//! Accuracy Rating on the Weapon" (the Amazon's Penetrate). The rating on a
//! weapon is what that item's own "+N to Accuracy Rating" lines total: 400 on
//! the bow adds 100 to both ends of its attacks, like "Adds 100 to 100
//! Physical Damage to Attacks".

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, data_version, repo_data_root};

const PENETRATE: &str = "Attacks using your Weapons have Added Physical Damage equal to 25% of the Accuracy Rating on the Weapon";

fn item(base: &str, mods: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: mods.iter().map(|s| (*s).into()).collect(),
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    }
}

/// The average hit of a bow attack with `bow_mods` on the bow and
/// `amulet_mods` on the amulet.
fn average_hit(data: &BuildData, bow_mods: &[&str], amulet_mods: &[&str]) -> f64 {
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 85,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_active_skill("IceShotPlayer", 20))
        .set_item(EquipmentSlot::Weapon1, item("Heavy Bow", bow_mods))
        .set_item(EquipmentSlot::Amulet, item("Jade Amulet", amulet_mods));
    let output =
        calculate_with_data(&build, data, &DataOrchestratorOptions::default()).expect("calculate");
    assert!(output.total_hit_avg > 0.0);
    output.total_hit_avg
}

#[test]
fn a_quarter_of_the_accuracy_rating_on_the_weapon_is_added_as_physical_damage() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data");
    let accurate_bow = ["+400 to Accuracy Rating"];
    let plain = average_hit(&data, &accurate_bow, &[]);
    let with_penetrate = average_hit(&data, &accurate_bow, &[PENETRATE]);
    let with_flat = average_hit(
        &data,
        &accurate_bow,
        &["Adds 100 to 100 Physical Damage to Attacks"],
    );
    assert!(with_penetrate > plain);
    assert!(
        (with_penetrate - with_flat).abs() < 1e-6,
        "{with_penetrate} != {with_flat}"
    );

    // Accuracy from elsewhere is not on the weapon.
    let elsewhere = average_hit(&data, &[], &[PENETRATE, "+400 to Accuracy Rating"]);
    assert!((elsewhere - average_hit(&data, &[], &[])).abs() < 1e-6);
}
