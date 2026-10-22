//! "Attacks using your Weapons have Added Physical Damage equal to 25% of the
//! Accuracy Rating on the Weapon" (the Amazon's Penetrate). The rating on a
//! weapon is what that item's own "+N to Accuracy Rating" lines total: 400 on
//! the bow adds 100 to both ends of its attacks, like "Adds 100 to 100
//! Physical Damage to Attacks".

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

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
        socketed: Vec::new(),
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
    let data = crate::test_data();
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

#[test]
fn melee_weapon_spell_damage_uses_the_equipped_weapon_class() {
    let data = crate::test_data();
    for (base, melee) in [
        ("Crude Bow", false),
        ("Ashen Staff", false),
        ("Wrapped Quarterstaff", true),
        ("Wooden Club", true),
    ] {
        assert!(
            data.base_items.contains_key(base),
            "missing game base {base}"
        );
        let build = Build::new()
            .add_socket_group(SocketGroup::new().with_active_skill("FireballPlayer", 20))
            .set_item(EquipmentSlot::Weapon1, item(base, &[]));
        let plain =
            calculate_with_data(&build, &data, &DataOrchestratorOptions::default()).unwrap();
        let opts = DataOrchestratorOptions {
            extra_modifier_texts: vec![
                "40% increased Spell Damage while wielding a Melee Weapon".into(),
            ],
            ..Default::default()
        };
        let boosted = calculate_with_data(&build, &data, &opts).unwrap();
        // spell_damage_+%_while_wielding_melee_weapon applies only while wielded.
        assert!(
            (boosted.total_hit_avg / plain.total_hit_avg - if melee { 1.4 } else { 1.0 }).abs()
                < 1e-9,
            "{base}"
        );
    }
}
