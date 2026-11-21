//! Debuffs the character puts on the enemy, and the configuration stating
//! one is there.
//!
//! - Exposure is a debuff the character inflicts: "Is the enemy Exposed to
//!   Fire?" holds only for a character that can inflict Fire Exposure.
//! - Crushed lowers the target's Physical Damage Reduction by 15, below zero
//!   too (the debuff's stat `base_additional_physical_damage_reduction_%`).
//!   Styrn's Ferocity: "Supported Skills Crush targets they Hit for 8
//!   seconds", whichever of the character's skills it supports.

use pobr_build::{
    Build, BuildConfig, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data,
};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

use crate::test_data as data;

fn item(base: &str, mods: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: mods.iter().map(|text| text.to_string()).collect(),
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: vec![],
    }
}

fn build(skill: &str) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 85,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_active_skill(skill, 16))
}

/// The DPS against the enemy, with its resistances and Armour.
fn dps(build: &Build, data: &BuildData, modifiers: &[&str]) -> f64 {
    let options = DataOrchestratorOptions {
        mode_effective: true,
        extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
        ..Default::default()
    };
    let output = calculate_with_data(build, data, &options).expect("calculate");
    assert!(output.dps > 0.0);
    output.dps
}

#[test]
fn the_configured_exposure_needs_a_character_that_inflicts_it() {
    let data = data();
    let fireball = |exposed: bool| {
        build("FireballPlayer")
            .with_config(BuildConfig::default().with_condition("EnemyFireExposure", exposed))
    };
    // Nothing inflicts Fire Exposure: the enemy is not Exposed.
    assert_eq!(
        dps(&fireball(true), &data, &[]),
        dps(&fireball(false), &data, &[])
    );
    // "Inflict Fire Exposure on Hit": it is.
    let inflicts = ["Inflict Fire Exposure on Hit"];
    let exposed = dps(&fireball(true), &data, &inflicts);
    let plain = dps(&fireball(false), &data, &inflicts);
    assert!(exposed > plain, "{exposed} > {plain}");
}

#[test]
fn styrns_ferocity_crushes_the_enemy_for_the_other_skills() {
    let data = data();
    let shield_wall = |crushes: bool| {
        let mut raise_shield = SocketGroup::new().with_gem_skill("ShieldBlockPlayer", 1);
        if crushes {
            raise_shield = raise_shield.with_gem_skill("SupportStyrnsFerocity", 1);
        }
        build("ShieldWallPlayer")
            .set_item(EquipmentSlot::Weapon1, item("Wooden Club", &[]))
            .set_item(EquipmentSlot::Weapon2, item("Splintered Tower Shield", &[]))
            .add_socket_group(raise_shield)
    };
    // Shield Wall's hit is Physical: 15 less Physical Damage Reduction.
    let crushed = dps(&shield_wall(true), &data, &[]);
    let plain = dps(&shield_wall(false), &data, &[]);
    assert!(crushed > plain * 1.1, "{crushed} > {plain}");
}

#[test]
fn weapon_restricted_exposure_debuffs_require_a_compatible_active_weapon() {
    let data = data();
    // Flameblast on a Staff with Oil Grenade enabled and a Crossbow only in the swap set:
    // Oil Grenade cannot fire in the Staff weapon set, so it must not apply Fire Exposure.
    let staff_only = build("FlameblastPlayer")
        .set_item(EquipmentSlot::Weapon1, item("Ashen Staff", &[]))
        .add_socket_group(SocketGroup::new().with_gem_skill("OilGrenadePlayer", 20));
    let mut staff_with_swap = staff_only.clone();
    staff_with_swap
        .other_weapons
        .insert(EquipmentSlot::Weapon1, item("Siege Crossbow", &[]));
    let no_oil =
        build("FlameblastPlayer").set_item(EquipmentSlot::Weapon1, item("Ashen Staff", &[]));
    assert_eq!(dps(&staff_only, &data, &[]), dps(&no_oil, &data, &[]));
    assert_eq!(dps(&staff_with_swap, &data, &[]), dps(&no_oil, &data, &[]));

    // When a Crossbow IS held in the active weapon set, Oil Grenade applies its Fire Exposure.
    let crossbow_plain = build("ExplosiveGrenadePlayer")
        .set_item(EquipmentSlot::Weapon1, item("Siege Crossbow", &[]));
    let crossbow_with_oil = crossbow_plain
        .clone()
        .add_socket_group(SocketGroup::new().with_gem_skill("OilGrenadePlayer", 20));
    assert!(
        dps(&crossbow_with_oil, &data, &[]) > dps(&crossbow_plain, &data, &[]) * 1.1,
        "Oil Grenade applies exposure when a Crossbow is active"
    );

    // Oil Barrage requires a Talisman: holding a Mace with no Talisman must not apply exposure.
    let mace_plain =
        build("FireballPlayer").set_item(EquipmentSlot::Weapon1, item("Wooden Club", &[]));
    let mace_with_oil_barrage = mace_plain
        .clone()
        .add_socket_group(SocketGroup::new().with_gem_skill("OilBarragePlayer", 20));
    assert_eq!(
        dps(&mace_with_oil_barrage, &data, &[]),
        dps(&mace_plain, &data, &[])
    );
}
