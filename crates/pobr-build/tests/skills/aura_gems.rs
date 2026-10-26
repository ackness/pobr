//! An aura gem is a skill gem like any other: it gains levels from "+N to
//! Level of all Skills" and its quality adds to the stat the gem grants.
//!
//! Purity of Ice (`PurityOfIcePlayer`) grants `base_skill_buff_cold_damage_
//! resistance_%_to_apply`: 40 at gem level 19, 42 at level 21, and 0.4 more
//! for each 1% of quality.

use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

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
            socketed: Vec::new(),
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
    let data = crate::test_data();
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

#[test]
fn item_grant_uses_the_listed_groups_supports_and_enabled_state() {
    let data = crate::test_data();
    let build = parse_build(
        r#"<PathOfBuilding2><Build level="100" className="Witch" mainSocketGroup="1"/>
<Items><Item id="1">Rarity: RARE
Test Sceptre
Stoic Sceptre
Implicits: 1
Grants Skill: Level 20 Discipline
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Skills>
<Skill enabled="true" slot="Weapon 1" source="Item:1:Test Sceptre, Stoic Sceptre"><Gem skillId="DisciplinePlayer" level="20" enabled="true"/></Skill>
<Skill enabled="true"><Gem skillId="DisciplinePlayer" level="20" enabled="true"/><Gem skillId="SupportUhtredExodusPlayer" level="1" enabled="true"/></Skill>
</Skills></PathOfBuilding2>"#,
    )
    .unwrap();
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec!["+200 to Intelligence".into(), "+100 to Strength".into()],
        ..Default::default()
    };
    let output = calculate_with_data(&build, data, &options).unwrap();
    // The supported Discipline runs at 23 and grants 439 total ES. The
    // earlier item-source copy must not hide it with its unsupported level 20.
    assert_eq!(output.energy_shield, 439.0);
    let selection = pobr_build::resolve_main_skill_selection(&build, data).unwrap();
    assert_eq!(selection, (1, 1, "DisciplinePlayer".into()));

    let mut disabled = build;
    disabled.socket_groups[1].enabled = false;
    let output = calculate_with_data(&disabled, data, &options).unwrap();
    assert_eq!(output.energy_shield, 0.0);
}

/// A buff has the name the game shows for its skill (`Sniper's Mark`, which
/// the curse priority table knows; `Arctic Armour`, whose skill is
/// `new_new_arctic_armour`). A name with a placeholder, a Companion's
/// "Companion: {0}", falls back to the skill's own name.
#[test]
fn a_buff_has_the_name_the_game_shows() {
    let data = crate::test_data();
    let name = |skill| pobr_core::skill_env::buff_skill_name(data, skill);
    assert_eq!(name("SnipersMarkPlayer"), "Sniper's Mark");
    assert_eq!(name("ArcticArmourPlayer"), "Arctic Armour");
    assert_eq!(name("SummonBeastPlayer"), "Summon Beast");
}

#[test]
fn alchemists_boon_quality_speed_requires_the_corresponding_active_flask() {
    use pobr_build::calculate_with_data_session;
    use pobr_data::passive_tree::NodeId;
    use pobr_data::prelude::{ModName, ModType};

    let data = crate::test_data();
    for (life_active, mana_active) in [(false, false), (true, false), (false, true)] {
        let xml = format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Mercenary" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill>
<Skill enabled="true"><Gem skillId="AlchemistsBoonPlayer" level="20" quality="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1">
<Item id="1">Rarity: NORMAL
Gargantuan Life Flask
Implicits: 0</Item>
<Item id="2">Rarity: NORMAL
Gargantuan Mana Flask
Implicits: 0</Item>
<ItemSet id="1"><Slot name="Flask 1" itemId="1" active="{life_active}"/>
<Slot name="Flask 2" itemId="2" active="{mana_active}"/></ItemSet></Items>
</PathOfBuilding2>"#
        );
        let mut build = parse_build(&xml).unwrap();
        // The speed is the additional quality effect, not ordinary quality.
        for additional_quality in [false, true] {
            build.tree.allocated_nodes = if additional_quality {
                vec![NodeId(14429)]
            } else {
                Vec::new()
            };
            let session = calculate_with_data_session(
                &build,
                data,
                &DataOrchestratorOptions::against_reference_enemy(),
            )
            .unwrap();
            assert_eq!(session.config().condition("UsingLifeFlask"), life_active);
            assert_eq!(session.config().condition("UsingManaFlask"), mana_active);
            for (name, active) in [("AttackSpeed", life_active), ("CastSpeed", mana_active)] {
                let value =
                    session
                        .mod_db()
                        .sum(ModType::Inc, session.config(), &[ModName::from(name)]);
                assert_eq!(
                    value,
                    if active && additional_quality {
                        20.0
                    } else {
                        0.0
                    }
                );
            }
        }
    }
}
