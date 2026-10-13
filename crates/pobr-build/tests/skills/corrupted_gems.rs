//! Modifiers to "all Corrupted Skill Gems" test the state of the gem itself.
//!
//! A corrupted gem already carries what corruption did to its own level. A
//! line such as "+1 to Level of all Corrupted Skill Gems" raises it once more,
//! and leaves a gem that is not corrupted alone.

use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, data_version, repo_data_root};

/// A build with one Fireball gem carrying `gem_attributes`.
fn build(gem_attributes: &str) -> Build {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" quality="0" enabled="true" {gem_attributes}/>
  </Skill>
</SkillSet></Skills>
</PathOfBuilding2>"#
    );
    parse_build(&xml).expect("parse synthetic build")
}

/// The same build wearing an amulet with "+1 to Level of all Corrupted Skill Gems".
fn with_the_bonus(build: Build) -> Build {
    build.set_item(
        EquipmentSlot::Amulet,
        Item {
            base: ItemBaseId::from("Jade Amulet"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec!["+1 to Level of all Corrupted Skill Gems".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        },
    )
}

fn average_hit(data: &BuildData, build: &Build) -> f64 {
    let output =
        calculate_with_data(build, data, &DataOrchestratorOptions::default()).expect("calculate");
    assert!(output.total_hit_avg > 0.0);
    output.total_hit_avg
}

#[test]
fn a_build_code_keeps_whether_a_gem_is_corrupted() {
    let corrupted = |attributes: &str| build(attributes).socket_groups[0].gem_skills[0].corrupted;
    assert!(corrupted(r#"level="20" corrupted="true""#));
    // PoB2 writes the literal "nil" for a gem that is not corrupted.
    assert!(!corrupted(r#"level="20" corrupted="nil""#));
    assert!(!corrupted(r#"level="20""#));
}

#[test]
fn a_level_bonus_for_corrupted_skill_gems_applies_to_a_corrupted_gem_only() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data");
    let clean = r#"level="20""#;
    let corrupted = r#"level="20" corrupted="true""#;
    let level_20 = average_hit(&data, &build(clean));
    let level_21 = average_hit(&data, &build(r#"level="21""#));
    assert!(level_21 > level_20);

    // Corrupted: one level higher, the same as a level 21 gem.
    assert_eq!(
        average_hit(&data, &with_the_bonus(build(corrupted))),
        level_21
    );
    // Corruption alone changes nothing; nor does the bonus on a clean gem.
    assert_eq!(average_hit(&data, &build(corrupted)), level_20);
    assert_eq!(average_hit(&data, &with_the_bonus(build(clean))), level_20);
}
