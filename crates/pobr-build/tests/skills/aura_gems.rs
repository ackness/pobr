//! An aura gem is a skill gem like any other: it gains levels from "+N to
//! Level of all Skills" and its quality adds to the stat the gem grants.
//!
//! Purity of Ice (`PurityOfIcePlayer`) grants `base_skill_buff_cold_damage_
//! resistance_%_to_apply`: 40 at gem level 19, 42 at level 21, and 0.4 more
//! for each 1% of quality.

use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, data_version, repo_data_root};

/// A Fireball build, with a level 19 Purity of Ice of `quality` when given.
fn build(purity_quality: Option<u32>) -> Build {
    let purity = purity_quality.map_or(String::new(), |quality| {
        format!(
            r#"<Skill enabled="true" mainActiveSkill="1"><Gem gemId="Metadata/Items/Gems/SkillGemPurityOfIce" skillId="PurityOfIcePlayer" nameSpec="Purity of Ice" level="19" quality="{quality}" enabled="true"/></Skill>"#
        )
    });
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>
  </Skill>
  {purity}
</SkillSet></Skills>
</PathOfBuilding2>"#
    );
    parse_build(&xml).expect("parse synthetic build")
}

fn with_amulet(build: Build, line: &str) -> Build {
    build.set_item(
        EquipmentSlot::Amulet,
        Item {
            base: ItemBaseId::from("Jade Amulet"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec![line.into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        },
    )
}

fn cold_resistance(data: &BuildData, build: &Build) -> f64 {
    calculate_with_data(build, data, &DataOrchestratorOptions::default())
        .expect("calculate")
        .cold_resistance
}

#[test]
fn an_aura_gem_keeps_its_quality_and_gains_levels() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data");
    let without = cold_resistance(&data, &build(None));
    let granted = |build: &Build| cold_resistance(&data, build) - without;

    assert_eq!(granted(&build(Some(0))), 40.0);
    // 20% quality: 0.4 x 20 more of the same stat.
    assert_eq!(granted(&build(Some(20))), 48.0);
    // Two levels: 42 at level 21.
    let two_levels = "+2 to Level of all Skills";
    assert_eq!(
        cold_resistance(&data, &with_amulet(build(Some(0)), two_levels))
            - cold_resistance(&data, &with_amulet(build(None), two_levels)),
        42.0
    );
}
