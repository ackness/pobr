//! Spirit reservation of the skills that items grant.
//!
//! The Lament, Portent and Absent Amulets grant a skill and waive the Spirit it
//! reserves as a gem: the game's table of item-granted skills
//! (`ItemInherentSkills`) flags those bases `NoReservation`. Any other item
//! that grants a skill leaves its reservation as it is.
//!
//! Archmage reserves 100 Spirit. Its quality gives 0.5% Reservation Efficiency
//! per 1%, so a gem with 20% quality reserves 100 / 1.10 = 91.

use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, WeaponSet, calculate_with_data, parse_build,
    resolve_main_skill_selection,
};
use pobr_data::item::{EquipmentSlot, ItemRarity};

use crate::test_data as data;

fn archmage(level: u32, quality: u32, corrupted: bool) -> String {
    format!(
        r#"<Gem gemId="Metadata/Items/Gem/SkillGemArchmage" skillId="ArchmagePlayer" nameSpec="Archmage" level="{level}" quality="{quality}" corrupted="{corrupted}" enabled="true"/>"#
    )
}

/// An ordinary skill group holding one Archmage gem.
fn gem(level: u32, quality: u32, corrupted: bool) -> String {
    format!(
        r#"<Skill enabled="true">{}</Skill>"#,
        archmage(level, quality, corrupted)
    )
}

/// The amulet's own group, as a build code writes it.
fn granted(base: &str) -> String {
    format!(
        r#"<Skill enabled="true" source="Item:1:Test Beads, {base}" slot="Amulet">{}</Skill>"#,
        archmage(20, 0, false)
    )
}

/// The Spirit reserved by a Sorceress who wears an amulet of `base` granting
/// Level 20 Archmage and has `groups` as her skill groups.
fn reserved(data: &BuildData, base: &str, groups: &[String]) -> f64 {
    let groups = groups.concat();
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">{groups}</SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Beads
{base}
Implicits: 1
Grants Skill: Level 20 Archmage
</Item><ItemSet id="1"><Slot name="Amulet" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    calculate_with_data(&build, data, &DataOrchestratorOptions::default())
        .expect("calculate")
        .spirit_reserved
}

/// A base renamed by a data update would silently lose the rule.
#[test]
fn every_listed_base_is_a_base_item() {
    let data = data();
    assert!(!data.reservation_free_grant_bases.is_empty());
    for base in &data.reservation_free_grant_bases {
        assert!(data.base_items.contains_key(base), "{base}");
    }
}

#[test]
fn the_three_amulets_grant_their_skill_without_reservation() {
    let data = data();
    for base in ["Lament Amulet", "Portent Amulet", "Absent Amulet"] {
        assert_eq!(reserved(&data, base, &[]), 0.0, "{base}");
        assert_eq!(reserved(&data, base, &[granted(base)]), 0.0, "{base}");
    }
    // Any other item's granted skill reserves what the skill reserves.
    assert_eq!(reserved(&data, "Gold Amulet", &[]), 100.0);
}

/// A character imported from the game lists a granted skill a second time, as
/// an ordinary group that holds its supports. It is one skill.
#[test]
fn the_group_that_lists_the_granted_skill_is_that_skill() {
    let data = data();
    let groups = [gem(20, 0, false), granted("Absent Amulet")];
    assert_eq!(reserved(&data, "Absent Amulet", &groups), 0.0);
    let groups = [gem(20, 0, false), granted("Gold Amulet")];
    assert_eq!(reserved(&data, "Gold Amulet", &groups), 100.0);
}

#[test]
fn a_gem_of_the_same_skill_still_reserves() {
    let data = data();
    let amulet = "Absent Amulet";
    let listed = gem(20, 0, false);
    let own = gem(20, 20, true);
    // Whichever comes first.
    let groups = [listed.clone(), granted(amulet), own.clone()];
    assert_eq!(reserved(&data, amulet, &groups), 91.0);
    let groups = [own.clone(), listed, granted(amulet)];
    assert_eq!(reserved(&data, amulet, &groups), 91.0);
    // And when the granted skill is listed only as the amulet's group.
    assert_eq!(reserved(&data, amulet, &[granted(amulet), own]), 91.0);
}

/// The granted skill has the level the item states, no quality and is not
/// corrupted. A group that differs is a gem.
#[test]
fn a_group_that_differs_from_the_grant_is_a_gem() {
    let data = data();
    let amulet = "Absent Amulet";
    for other in [gem(19, 0, false), gem(20, 0, true), gem(20, 1, false)] {
        let groups = [other, granted(amulet)];
        assert!(reserved(&data, amulet, &groups) >= 99.0);
    }
}

#[test]
fn only_one_group_is_the_granted_skill() {
    let data = data();
    let amulet = "Absent Amulet";
    let groups = [gem(20, 0, false), gem(20, 0, false), granted(amulet)];
    assert_eq!(reserved(&data, amulet, &groups), 100.0);
}

const VARIANTS: [(&str, &str, &str, &str, &str); 4] = [
    (
        "The Coming Calamity",
        "Heroic Armour",
        "Herald of Ash",
        "HeraldOfAshPlayer",
        "UniqueHeraldOfAshPlayer",
    ),
    (
        "The Coming Calamity",
        "Heroic Armour",
        "Herald of Ice",
        "HeraldOfIcePlayer",
        "UniqueHeraldOfIcePlayer",
    ),
    (
        "The Coming Calamity",
        "Heroic Armour",
        "Herald of Thunder",
        "HeraldOfThunderPlayer",
        "UniqueHeraldOfThunderPlayer",
    ),
    (
        "Sands of Silk",
        "Shrouded Vest",
        "Blink",
        "BlinkReservationPlayer",
        "BlinkSandPlayer",
    ),
];

fn skill_gem(id: &str, level: u32, quality: u32, corrupted: bool) -> String {
    let gem_id = &data().gem_effects[id].gem_id;
    format!(
        r#"<Gem gemId="{gem_id}" skillId="{id}" level="{level}" quality="{quality}" corrupted="{corrupted}" enabled="true"/>"#
    )
}

fn skill_group(attributes: &str, gems: &str) -> String {
    format!(r#"<Skill enabled="true" {attributes}>{gems}</Skill>"#)
}

fn item_group(item: &str, base: &str, id: &str, level: u32) -> String {
    skill_group(
        &format!(r#"source="Item:1:{item}, {base}" slot="Body Armour""#),
        &skill_gem(id, level, 0, false),
    )
}

fn unique_build(item: &str, base: &str, grants: &str, groups: &[String]) -> Build {
    let count = grants.lines().count();
    let groups = groups.concat();
    parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="100" className="Sorceress" mainSocketGroup="1"/>
<Skills><SkillSet id="1">{groups}</SkillSet></Skills>
<Items><Item id="1">Rarity: UNIQUE
{item}
{base}
Implicits: {count}
{grants}
</Item><ItemSet id="1"><Slot name="Body Armour" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#,
    ))
    .expect("parse unique grant")
}

fn reservation(build: &Build) -> f64 {
    calculate_with_data(build, data(), &DataOrchestratorOptions::default())
        .expect("calculate unique grant")
        .spirit_reserved
}

/// Game ModGrantedSkills rows 9/10/11/1 link these mods to the distinct
/// effects. Native Heralds have zero base reservation, not a -100% multiplier:
/// the Precision I support still reserves 10. Sand Blink has three uses.
#[test]
fn native_and_exported_item_variants_keep_support_costs_and_import_identity() {
    for (item, base, name, exported, native) in VARIANTS {
        for own_id in [exported, native] {
            for listed_id in [exported, native] {
                let mut gems = skill_gem(listed_id, 20, 0, false);
                if name != "Blink" {
                    gems.push_str(&skill_gem("SupportPrecisionPlayer", 1, 0, false));
                }
                let build = unique_build(
                    item,
                    base,
                    &format!("Grants Skill: Level 20 {name}"),
                    &[item_group(item, base, own_id, 20), skill_group("", &gems)],
                );
                let original_groups = build.socket_groups.clone();
                let original_text = build.items[&EquipmentSlot::BodyArmour]
                    .implicit_texts
                    .clone();
                assert_eq!(
                    resolve_main_skill_selection(&build, data()),
                    Some((1, 1, native.into())),
                    "{item}: {own_id}, {listed_id}"
                );
                let output =
                    calculate_with_data(&build, data(), &DataOrchestratorOptions::default())
                        .unwrap();
                assert_eq!(
                    output.spirit_reserved,
                    if name == "Blink" { 0.0 } else { 10.0 }
                );
                if name == "Blink" {
                    assert_eq!(output.cooldown_stored_uses, 3);
                }
                assert_eq!(build.socket_groups, original_groups);
                assert_eq!(
                    build.items[&EquipmentSlot::BodyArmour].implicit_texts,
                    original_text
                );
            }
        }
    }
}

#[test]
fn known_unique_source_synthesizes_missing_variants_without_claiming_manual_gems() {
    let build = unique_build(
        "The Coming Calamity",
        "Heroic Armour",
        "Grants Skill: Level 20 Herald of Ash\nGrants Skill: Level 20 Herald of Ice\nGrants Skill: Level 20 Herald of Thunder",
        &[
            item_group(
                "The Coming Calamity",
                "Heroic Armour",
                "HeraldOfAshPlayer",
                20,
            ),
            skill_group("", &skill_gem("HeraldOfIcePlayer", 20, 0, false)),
        ],
    );
    // Only Ash has exported attachment metadata. The synthesized Ice group
    // does not establish that the separate manual Ice gem belongs to it.
    assert_eq!(reservation(&build), 30.0);
    assert_eq!(
        resolve_main_skill_selection(&build, data()),
        Some((0, 1, "UniqueHeraldOfAshPlayer".into()))
    );
}

#[test]
fn a_synthesized_native_item_source_does_not_claim_an_ordinary_manual_gem() {
    let build = unique_build(
        "Sands of Silk",
        "Shrouded Vest",
        "Grants Skill: Level 20 Blink",
        &[
            skill_group(
                r#"source="Item" slot="Body Armour""#,
                &skill_gem("BlinkSandPlayer", 20, 0, false),
            ),
            skill_group("", &skill_gem("BlinkReservationPlayer", 20, 0, false)),
        ],
    );
    assert_eq!(reservation(&build), 60.0);
    assert_eq!(
        resolve_main_skill_selection(&build, data()),
        Some((0, 1, "BlinkSandPlayer".into()))
    );
}

#[test]
fn runemastered_sands_keeps_a_corrupted_quality_blink_in_either_weapon_set() {
    for base in ["Shrouded Vest", "Runemastered Shrouded Vest"] {
        for set in [WeaponSet::First, WeaponSet::Second] {
            let mut build = unique_build(
                "Sands of Silk",
                base,
                "Grants Skill: Level 18 Blink",
                &[
                    skill_group("", &skill_gem("BlinkReservationPlayer", 20, 20, true)),
                    skill_group("", &skill_gem("BlinkReservationPlayer", 18, 0, false)),
                    item_group("Sands of Silk", base, "BlinkReservationPlayer", 18),
                ],
            );
            build.weapon_set = set;
            build.main_socket_group = Some(3);
            assert_eq!(reservation(&build), 60.0, "{base} {set:?}");
            assert_eq!(
                resolve_main_skill_selection(&build, data()),
                Some((1, 1, "BlinkSandPlayer".into()))
            );
            build.main_socket_group = Some(1);
            assert_eq!(
                resolve_main_skill_selection(&build, data()),
                Some((0, 1, "BlinkReservationPlayer".into()))
            );
        }
    }
}

#[test]
fn a_disabled_listed_variant_suppresses_the_item_copy_and_its_support_cost() {
    let mut build = unique_build(
        "The Coming Calamity",
        "Runemastered Heroic Armour",
        "Grants Skill: Level 20 Herald of Ash",
        &[
            item_group(
                "The Coming Calamity",
                "Runemastered Heroic Armour",
                "HeraldOfAshPlayer",
                20,
            ),
            skill_group(
                "",
                &(skill_gem("HeraldOfAshPlayer", 20, 0, false)
                    + &skill_gem("SupportPrecisionPlayer", 1, 0, false)),
            ),
        ],
    );
    assert_eq!(reservation(&build), 10.0);
    build.socket_groups[1].enabled = false;
    assert_eq!(reservation(&build), 0.0);
    assert!(!build.socket_groups[1].enabled);
}

#[test]
fn only_one_matching_ordinary_group_becomes_the_unique_variant() {
    let build = unique_build(
        "The Coming Calamity",
        "Heroic Armour",
        "Grants Skill: Level 20 Herald of Ice",
        &[
            skill_group("", &skill_gem("HeraldOfIcePlayer", 20, 0, false)),
            item_group(
                "The Coming Calamity",
                "Heroic Armour",
                "HeraldOfIcePlayer",
                20,
            ),
            skill_group("", &skill_gem("HeraldOfIcePlayer", 20, 0, false)),
        ],
    );
    assert_eq!(reservation(&build), 30.0);
}

#[test]
fn a_shared_display_name_without_matching_item_evidence_stays_ordinary() {
    let original = unique_build(
        "Sands of Silk",
        "Shrouded Vest",
        "Grants Skill: Level 20 Blink",
        &[item_group(
            "Sands of Silk",
            "Shrouded Vest",
            "BlinkReservationPlayer",
            20,
        )],
    );
    for mismatch in ["name", "base", "slot", "rarity", "no_source"] {
        let mut build = original.clone();
        match mismatch {
            "name" => {
                build.socket_groups[0].source = Some("Item:1:Other Unique, Shrouded Vest".into())
            }
            "base" => {
                build
                    .items
                    .get_mut(&EquipmentSlot::BodyArmour)
                    .unwrap()
                    .base = "Heroic Armour".into()
            }
            "slot" => build.socket_groups[0].slot = Some("Helmet".into()),
            "rarity" => {
                build
                    .items
                    .get_mut(&EquipmentSlot::BodyArmour)
                    .unwrap()
                    .rarity = ItemRarity::Rare
            }
            "no_source" => build.socket_groups[0].source = None,
            _ => unreachable!(),
        }
        assert_eq!(reservation(&build), 60.0, "{mismatch}");
        assert_eq!(
            resolve_main_skill_selection(&build, data()),
            Some((0, 1, "BlinkReservationPlayer".into())),
            "{mismatch}"
        );
    }
}

#[test]
fn sands_variant_remaps_selected_and_following_active_effects() {
    for listed in [false, true] {
        for another_gem in [false, true] {
            let mut gems = skill_gem("BlinkReservationPlayer", 20, 0, false);
            if another_gem {
                gems.push_str(&skill_gem("FireballPlayer", 20, 0, false));
            }
            let mut groups = Vec::new();
            if listed {
                groups.push(item_group(
                    "Sands of Silk",
                    "Shrouded Vest",
                    "BlinkReservationPlayer",
                    20,
                ));
                groups.push(skill_group("", &gems));
            } else {
                groups.push(skill_group(
                    r#"source="Item:1:Sands of Silk, Shrouded Vest" slot="Body Armour""#,
                    &gems,
                ));
            }
            let mut build = unique_build(
                "Sands of Silk",
                "Shrouded Vest",
                "Grants Skill: Level 20 Blink",
                &groups,
            );
            let selected_group = usize::from(listed);
            // The old list is reservation, Blink, then optionally Fireball.
            // Both first-gem effects become Sand Blink. A later gem shifts
            // down one position but must remain the selected effect.
            for selected in 1..=if another_gem { 3 } else { 2 } {
                build.socket_groups[selected_group].main_active_skill = Some(selected);
                let original = build.socket_groups.clone();
                let (ordinal, effect) = if selected == 3 {
                    (2, "FireballPlayer")
                } else {
                    (1, "BlinkSandPlayer")
                };
                assert_eq!(
                    resolve_main_skill_selection(&build, data()),
                    Some((selected_group, ordinal, effect.into())),
                    "listed={listed}, another_gem={another_gem}, selected={selected}"
                );
                assert_eq!(build.socket_groups, original);
            }
            // An invalid explicit selection must not be changed to a valid one.
            build.socket_groups[selected_group].main_active_skill = Some(4);
            assert_eq!(resolve_main_skill_selection(&build, data()), None);
        }
    }
}
