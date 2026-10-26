//! Imported item names retain their affixes; calculations resolve the game base.

use pobr_build::{DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::skill_env::weapon_item_contribution;
use pobr_data::item::EquipmentSlot;

use crate::test_data;

fn imported(slot: &str, item: &str, skill: &str) -> pobr_build::Build {
    parse_build(&format!(
        r#"<PathOfBuilding2><Build level="90" className="Witch"/>
<Skills><Skill><Gem skillId="{skill}" level="20"/></Skill></Skills>
<Items><Item id="1">{item}</Item><ItemSet id="1"><Slot name="{slot}" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
    ))
    .unwrap()
}

#[test]
fn manifest_weapon_copies_a_magic_structured_hammer_and_its_local_damage() {
    let data = test_data();
    let magic_text = "Rarity: MAGIC\nSquire's Structured Hammer\nQuality: 20\nImplicits: 1\n40% chance to Daze on Hit\n16% increased Physical Damage\n+20 to Accuracy Rating";
    let magic = imported("Weapon 1", magic_text, "ManifestWeaponPlayer");
    let canonical = imported(
        "Weapon 1",
        &magic_text.replace("Squire's Structured Hammer", "Structured Hammer"),
        "ManifestWeaponPlayer",
    );
    let weapon = &magic.items[&EquipmentSlot::Weapon1];
    let local = weapon_item_contribution(weapon, data).unwrap();
    let base = data.weapon_base("Structured Hammer").unwrap();
    assert!((local.phys_min - f64::from(base.physical_min) * 1.16 * 1.20).abs() < 1e-9);
    assert!((local.phys_max - f64::from(base.physical_max) * 1.16 * 1.20).abs() < 1e-9);
    let run =
        |build: &_| calculate_with_data(build, data, &DataOrchestratorOptions::default()).unwrap();
    let actual = run(&magic);
    let expected = run(&canonical);
    assert_eq!(actual.minions.len(), 1);
    assert!(actual.minions[0].dps > 0.0);
    assert_eq!(actual.minions[0].dps, expected.minions[0].dps);
    assert_eq!(weapon.base.to_string(), "Squire's Structured Hammer");
    assert_eq!(
        weapon.modifier_texts,
        canonical.items[&EquipmentSlot::Weapon1].modifier_texts
    );
}

#[test]
fn base_resolution_keeps_exact_names_and_excludes_false_suffix_matches() {
    let data = test_data();
    for (name, expected) in [
        ("Squire's Structured Hammer", "Structured Hammer"),
        ("Chilled Warden Bow of the Mongoose", "Warden Bow"),
        ("Emerald Ring of the Fox", "Emerald Ring"),
        ("Rotund Emerald Ring of the Ruby", "Emerald Ring"),
        ("Exceptional Ornate Plate", "Ornate Plate"),
        ("Fists of Stone of the Fox", "Fists of Stone"),
        (
            "Runeforged Fists of Stone of the Fox",
            "Runeforged Fists of Stone",
        ),
        (
            "Rotund Runeforged Fists of Stone of the Fox",
            "Runeforged Fists of Stone",
        ),
    ] {
        assert_eq!(data.base_item(name).unwrap().name, expected);
    }
    for name in [
        "Unknown Weapon of the Structured Hammer",
        "Unknown Weapon of Structured Hammer",
        "UnrecognisedStructured Hammer",
        "Squire's Structured Hammer Replica",
        "Squire's Structured Hammer of ",
        "Unknown Gloves of Fists of Stone",
    ] {
        assert!(data.base_item(name).is_none(), "{name}");
    }
    let mut custom = data.clone();
    let mut exact = data.base_items["Ruby"].clone();
    exact.name = "Ruby of the Structured Hammer".into();
    custom.base_items.insert(exact.name.clone(), exact);
    assert_eq!(
        custom
            .base_item("Ruby of the Structured Hammer")
            .unwrap()
            .item_class,
        "Jewel"
    );
}

#[test]
fn decorated_base_with_of_preserves_per_level_defence() {
    let data = test_data();
    let text = "Rarity: MAGIC\nFists of Stone of the Fox\nEnergy Shield: 100\nImplicits: 1\nHas +1 to maximum Energy Shield per player level";
    let mut magic = imported("Gloves", text, "FireballPlayer");
    let mut canonical = imported(
        "Gloves",
        &text.replace("Fists of Stone of the Fox", "Fists of Stone"),
        "FireballPlayer",
    );
    magic.character.level = 100;
    canonical.character.level = 100;
    let run =
        |build: &_| calculate_with_data(build, data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(run(&canonical).energy_shield, 100.0);
    assert_eq!(run(&magic).energy_shield, 100.0);
}

#[test]
fn magic_armour_uses_local_defences_once_and_the_base_movement_penalty() {
    let data = test_data();
    let text = "Rarity: MAGIC\nEphemeral Hawker's Jacket of Recuperation\nEvasion: 389\nEnergy Shield: 120\nQuality: 20\nImplicits: 0\n64% increased Evasion and Energy Shield\n21.9 Life Regeneration per second";
    let magic = imported("Body Armour", text, "FireballPlayer");
    let canonical = imported(
        "Body Armour",
        &text.replace(
            "Ephemeral Hawker's Jacket of Recuperation",
            "Hawker's Jacket",
        ),
        "FireballPlayer",
    );
    let run =
        |build: &_| calculate_with_data(build, data, &DataOrchestratorOptions::default()).unwrap();
    let actual = run(&magic);
    let expected = run(&canonical);
    assert!(expected.evasion > 0.0 && expected.energy_shield > 0.0);
    assert_eq!(actual.evasion, expected.evasion);
    assert_eq!(actual.energy_shield, expected.energy_shield);
    assert_eq!(
        actual.effective_movement_speed_mod,
        expected.effective_movement_speed_mod
    );
    assert_eq!(
        magic.items[&EquipmentSlot::BodyArmour].base.to_string(),
        "Ephemeral Hawker's Jacket of Recuperation"
    );
}

#[test]
fn exceptional_normal_armour_uses_existing_base_and_explicit_quality_only() {
    let data = test_data();
    let text = "Rarity: NORMAL\nExceptional Ornate Plate\nArmour: 595\nQuality: 20\nSockets: S S S S\nImplicits: 1\nRegenerate 2.45% of maximum Life per second";
    let exceptional = imported("Body Armour", text, "FireballPlayer");
    let canonical = imported(
        "Body Armour",
        &text.replace("Exceptional Ornate Plate", "Ornate Plate"),
        "FireballPlayer",
    );
    let run =
        |build: &_| calculate_with_data(build, data, &DataOrchestratorOptions::default()).unwrap();
    let actual = run(&exceptional);
    let expected = run(&canonical);
    assert_eq!(actual.armour, expected.armour);
    assert_eq!(
        actual.effective_movement_speed_mod,
        expected.effective_movement_speed_mod
    );
    let item = &exceptional.items[&EquipmentSlot::BodyArmour];
    assert_eq!(item.base.to_string(), "Exceptional Ornate Plate");
    assert_eq!(item.quality, 20);
}
