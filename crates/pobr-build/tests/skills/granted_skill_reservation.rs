//! Spirit reservation of the skills that items grant.
//!
//! The Lament, Portent and Absent Amulets grant a skill and waive the Spirit it
//! reserves as a gem: the game's table of item-granted skills
//! (`ItemInherentSkills`) flags those bases `NoReservation`. Any other item
//! that grants a skill leaves its reservation as it is.
//!
//! Archmage reserves 100 Spirit. Its quality gives 0.5% Reservation Efficiency
//! per 1%, so a gem with 20% quality reserves 100 / 1.10 = 91.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

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
