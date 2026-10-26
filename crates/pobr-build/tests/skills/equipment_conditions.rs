//! Static equipment predicates for Dance with Death (14226) and Tolerant Equipment (2843).

use pobr_build::{
    Build, CharacterIdentity, DataOrchestratorOptions, SocketGroup, WeaponSet,
    calculate_with_data_session, parse_build,
};
use pobr_core::calc::CompletedCalculationSession;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

use crate::test_data as data;

const SPEED: &str = "25% more Skill Speed while Off Hand is empty and you have\na [One-Handed] [MartialWeapon|Martial Weapon] equipped in your Main Hand";
const BLEED: &str =
    "Immune to [Bleeding] if Equipped Helmet has higher [Armour] than [Evasion] Rating";
const POISON: &str =
    "Immune to [Poison] if Equipped Helmet has higher [Evasion] Rating than [Armour]";

#[test]
fn damage_per_different_companion_counts_prepared_identities_in_presence() {
    const LINE: &str = "24% increased Damage per each different Companion in your Presence";
    let check = |build: &Build, expected: f64| {
        let plain = run(build, &[]);
        let boosted = run(build, &[LINE]);
        assert!(boosted.unsupported_modifier_texts().is_empty());
        assert_eq!(
            boosted.config().multiplier("DifferentCompanionsInPresence"),
            expected
        );
        assert_eq!(boosted.base_sum("Damage"), plain.base_sum("Damage"));
        let increase = boosted.mod_db().sum(
            pobr_data::modifier::ModType::Inc,
            boosted.config(),
            &[pobr_data::modifier::ModName::from("Damage")],
        ) - plain.mod_db().sum(
            pobr_data::modifier::ModType::Inc,
            plain.config(),
            &[pobr_data::modifier::ModName::from("Damage")],
        );
        assert_eq!(increase, 24.0 * expected);
        if expected == 0.0 {
            assert_eq!(boosted.output().dps, plain.output().dps);
        } else {
            assert!(boosted.output().dps > plain.output().dps);
        }
    };
    let bare = build("FireballPlayer");
    check(&bare, 0.0);
    let mut wolf = SocketGroup::new().with_gem_skill("WolfPackPlayer", 20);
    wolf.gem_skills[0].quality = 20; // Seven wolves are one different Companion.
    let one = bare.clone().add_socket_group(wolf.clone());
    check(&one, 1.0);
    check(&one.clone().add_socket_group(wolf.clone()), 1.0);
    check(
        &one.clone().add_socket_group(
            SocketGroup::new().with_gem_skill("SummonSkeletalWarriorsPlayer", 20),
        ),
        1.0,
    );
    check(
        &one.clone()
            .add_socket_group(SocketGroup::new().with_gem_skill("WildProtectorPlayer", 20)),
        2.0,
    );
    let mut inactive = one.clone();
    inactive.socket_groups[1].enabled = false;
    check(&inactive, 0.0);
    inactive = one.clone();
    inactive.socket_groups[1].gem_skills[0].enabled = false;
    check(&inactive, 0.0);
    for name in ["SummonedMinion", "MinionPresenceCount"] {
        let mut zero = one.clone();
        zero.config.multipliers.insert(name.into(), 0.0);
        check(&zero, 0.0);
    }
    let mut absent = one.clone();
    absent
        .config
        .conditions
        .insert("CompanionInPresence".into(), false);
    check(&absent, 0.0);
    let mut alternate = one.clone();
    alternate.socket_groups[1].weapon_set = Some(2);
    check(&alternate, 0.0);
    alternate.weapon_set = WeaponSet::Second;
    check(&alternate, 1.0);

    // A chosen beast's identity matters even when both use Tame Beast.
    let beast = |id: &str| {
        let mut group = SocketGroup::new().with_gem_skill("SummonBeastPlayer", 20);
        group.gem_skills[0].minion = Some(id.into());
        group
    };
    let a = beast("Metadata/Monsters/Quadrilla/Quadrilla");
    let b = beast("Metadata/Monsters/Werewolves/WerewolfPack1");
    check(
        &bare
            .clone()
            .add_socket_group(a.clone())
            .add_socket_group(a.clone()),
        1.0,
    );
    check(&bare.add_socket_group(a).add_socket_group(b), 2.0);
}

#[test]
fn companion_presence_default_requires_an_enabled_companion_skill() {
    for enabled in [None, Some(false), Some(true)] {
        for explicit in [None, Some(false), Some(true)] {
            let skill = enabled.map_or(String::new(), |enabled| format!(
                r#"<Skill enabled="{enabled}"><Gem skillId="WolfPackPlayer" level="20" enabled="true"/></Skill>"#,
            ));
            let input = explicit.map_or(String::new(), |value| {
                format!(r#"<Input name="companionInPresence" boolean="{value}"/>"#,)
            });
            let xml = format!(
                r#"<PathOfBuilding2><Build level="80" className="Druid" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill>{skill}</Skills>
<Config>{input}</Config></PathOfBuilding2>"#,
            );
            let build = parse_build(&xml).unwrap();
            assert_eq!(
                build.config.conditions.get("CompanionInPresence").copied(),
                explicit
            );
            let session = run(
                &build,
                &[
                    "+100 to maximum Energy Shield",
                    "40% increased Armour, Evasion and Energy Shield while your Companion is in your Presence",
                ],
            );
            let active = explicit.unwrap_or(enabled == Some(true));
            assert_eq!(session.config().condition("CompanionInPresence"), active);
            assert_eq!(
                session.output().energy_shield,
                if active { 140.0 } else { 100.0 }
            );
        }
    }
}

fn item(base: &str, mods: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: mods.iter().map(|line| (*line).into()).collect(),
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: vec![],
        parsed_stats: vec![],
    }
}

fn build(skill: &str) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_active_skill(skill, 20))
}

fn run(build: &Build, lines: &[&str]) -> CompletedCalculationSession {
    calculate_with_data_session(
        build,
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: lines.iter().map(|line| (*line).into()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn assert_speed(build: &Build, active: bool) {
    let plain = run(build, &[]);
    let bonus = run(build, &[SPEED]);
    assert!(bonus.unsupported_modifier_texts().is_empty());
    assert!(plain.output().action_rate > 0.0);
    let ratio = bonus.output().action_rate / plain.output().action_rate;
    assert!(
        (ratio - if active { 1.25 } else { 1.0 }).abs() < 1e-9,
        "{ratio}"
    );
}

#[test]
fn dance_with_death_requires_a_one_handed_martial_main_hand_and_empty_offhand() {
    // Skill speed also affects spells: test the equipment predicate independently
    // of the skill's weapon damage flags, then check an actual mace attack.
    for (weapon, eligible) in [
        ("Wooden Club", true),
        ("Hardwood Spear", true),
        ("Dull Hatchet", true),
        ("Shortsword", true),
        ("Splintered Flail", true),
        ("Crude Claw", true),
        ("Ethereal Blade", true),
        ("Withered Wand", false),
        ("Rattling Sceptre", false),
        ("Ashen Staff", false),
        ("Wrapped Quarterstaff", false),
        ("Changeling Talisman", false),
        ("Crude Bow", false),
        ("Makeshift Crossbow", false),
        ("Felled Greatclub", false),
        ("Splitting Greataxe", false),
        ("Corroded Longsword", false),
    ] {
        assert!(data().base_items.contains_key(weapon), "{weapon}");
        let build = build("FireballPlayer").set_item(EquipmentSlot::Weapon1, item(weapon, &[]));
        assert_speed(&build, eligible);
    }
    assert_speed(&build("FireballPlayer"), false);
    let main =
        build("MeleeMaceMacePlayer").set_item(EquipmentSlot::Weapon1, item("Wooden Club", &[]));
    assert_speed(&main, true);
    for offhand in [
        "Wooden Club",
        "Golden Shield",
        "Leather Buckler",
        "Twig Focus",
        "Broadhead Quiver",
        "Rattling Sceptre",
        "Unknown Offhand",
    ] {
        assert_speed(
            &main
                .clone()
                .set_item(EquipmentSlot::Weapon2, item(offhand, &[])),
            false,
        );
    }
    // A one-handed weapon in the other hand does not qualify the main hand.
    assert_speed(
        &build("FireballPlayer").set_item(EquipmentSlot::Weapon2, item("Wooden Club", &[])),
        false,
    );
    let giants_blood = build("MeleeMaceMacePlayer").set_item(
        EquipmentSlot::Weapon1,
        item(
            "Felled Greatclub",
            &["You can wield Two-Handed Axes, Maces and Swords in one hand"],
        ),
    );
    assert!(run(&giants_blood, &[]).has_flag("GiantsBlood"));
    assert_speed(&giants_blood, false);
}

#[test]
fn dance_with_death_uses_the_imported_skills_selected_weapon_set() {
    for second in [false, true] {
        let xml = format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Warrior" mainSocketGroup="1"/>
<Skills><Skill enabled="true" set1="{}" set2="{second}"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1">
<Item id="1">Rarity: RARE
Test Mace
Wooden Club
Implicits: 0
</Item>
<Item id="2">Rarity: RARE
Test Shield
Golden Shield
Implicits: 0
</Item>
<ItemSet id="1" useSecondWeaponSet="false">
<Slot name="Weapon 1" itemId="1"/>
<Slot name="Weapon 2" itemId="2"/>
<Slot name="Weapon 1 Swap" itemId="1"/>
</ItemSet></Items></PathOfBuilding2>"#,
            !second,
        );
        let build = parse_build(&xml).unwrap();
        assert_eq!(
            build.weapon_set,
            if second {
                WeaponSet::Second
            } else {
                WeaponSet::First
            }
        );
        assert_speed(&build, second);
    }
}

#[test]
fn helmet_immunities_compare_local_final_ratings_strictly() {
    for (helmet, bleed, poison) in [
        (None, false, false),
        (Some(item("Golden Visage", &[])), false, false),
        (
            Some(item(
                "Golden Visage",
                &["+100 to Armour", "+100 to Evasion Rating"],
            )),
            false,
            false,
        ),
        (
            Some(item("Golden Visage", &["+100 to Armour"])),
            true,
            false,
        ),
        (Some(item("Shabby Hood", &[])), false, true),
        (
            Some(item(
                "Shabby Hood",
                &["+10 to Armour", "100% increased Armour"],
            )),
            true,
            false,
        ),
    ] {
        let mut build = build("FireballPlayer");
        if let Some(mut helmet) = helmet {
            helmet.quality = 20;
            // Known game bases ignore stale imported display totals.
            helmet.rolled_defence.armour = Some(1.0);
            helmet.rolled_defence.evasion = Some(99999.0);
            build = build.set_item(EquipmentSlot::Helmet, helmet);
        }
        let plain = run(&build, &[]);
        assert!(!plain.has_flag("BleedImmune") && !plain.has_flag("PoisonImmune"));
        let result = run(&build, &[BLEED, POISON, "10000% increased Evasion Rating"]);
        assert!(result.unsupported_modifier_texts().is_empty());
        assert_eq!(result.has_flag("BleedImmune"), bleed);
        assert_eq!(result.has_flag("PoisonImmune"), poison);
        assert_eq!(
            result.output().avoid_bleeding,
            if bleed { 100.0 } else { 0.0 }
        );
        assert_eq!(
            result.output().avoid_poison,
            if poison { 100.0 } else { 0.0 }
        );
    }
}

#[test]
fn tolerant_equipment_grants_the_tuned_witchhunter_poison_immunity() {
    let code = include_str!(
        "../../../../examples/reference-builds/tuned/builds/ninja-2026-10-13-mercenary-witchhunter-crossbow-shot-084d3b2ad49a/code.txt"
    );
    let mut build = parse_build(&pobr_build::decode_pob_code(code).unwrap()).unwrap();
    assert_eq!(build.weapon_set, WeaponSet::Second);
    let immune = run(&build, &[]);
    assert_eq!(immune.output().avoid_poison, 100.0);
    assert!(immune.has_flag("PoisonImmune"));
    build.tree.allocated_nodes.retain(|node| node.0 != 2843);
    let without = run(&build, &[]);
    assert!(!without.has_flag("PoisonImmune"));
    assert_eq!(without.output().avoid_poison, 0.0);
    // Its Staunching Charm supplies Bleeding immunity independently.
    assert_eq!(
        immune.output().avoid_bleeding,
        without.output().avoid_bleeding
    );
}

#[test]
fn unmet_equipment_predicates_remain_inactive_without_missing_condition_leads() {
    let build = build("FireballPlayer").set_item(
        EquipmentSlot::Amulet,
        item("Gold Amulet", &[SPEED, BLEED, POISON]),
    );
    let diagnosis =
        pobr_build::diagnose(&build, data(), &DataOrchestratorOptions::default()).unwrap();
    let ledger = pobr_build::usage::line_usage(&diagnosis, &build, data());
    for condition in [
        "UsingOneHandedWeapon",
        "OffHandIsEmpty",
        "HelmetArmourHigherThanEvasion",
        "HelmetEvasionHigherThanArmour",
    ] {
        assert!(ledger.condition_states.contains(condition), "{condition}");
        assert!(
            !ledger
                .conditions
                .iter()
                .any(|entry| entry.condition == condition)
        );
    }
    for text in [SPEED, BLEED, POISON] {
        let row = ledger
            .lines
            .iter()
            .find(|line| line.text.as_deref() == Some(text))
            .unwrap();
        assert_eq!(
            row.status,
            pobr_build::usage::LineStatus::Inactive,
            "{row:?}"
        );
    }
}
