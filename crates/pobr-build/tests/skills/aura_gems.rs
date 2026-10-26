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
fn planted_war_banner_native_speed_reaches_attacks_once_with_its_source() {
    use pobr_build::calculate_with_data_session;
    use pobr_data::source::ExporterCoverage;

    const STAT: &str = "base_skill_buff_banner_attack_speed_+%_to_apply";
    let data = crate::test_data();
    // This is a native positive stat of the planted effect, also present at
    // the other ordinary gem levels, not a bonus to the reservation effect.
    for level in [1, 18, 20] {
        let stats = data.effect_stats("WarBannerPlayer", level, 0, None);
        assert_eq!(stats.all().find(|s| s.stat == STAT).unwrap().value, 25.0);
    }
    let make = |main: &str, enabled: bool, planted: bool| {
        parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="90" className="Mercenary" mainSocketGroup="1"/>
<Skills><Skill><Gem skillId="{main}" level="20"/></Skill>
<Skill><Gem skillId="WolfPackPlayer" level="19"/></Skill>
<Skill enabled="{enabled}"><Gem skillId="WarBannerReservationPlayer" level="18"/>
<Gem skillId="SupportMagnifiedAreaPlayerTwo" level="1"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Wooden Club
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config><Input name="bannerPlanted" boolean="{planted}"/></Config>
</PathOfBuilding2>"#
        ))
        .unwrap()
    };
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec![
            "Banner Skills have 39% increased Aura Magnitudes".into(),
            "Aura Skills have 121% increased Magnitudes".into(),
        ],
        ..Default::default()
    };
    let calc = |build: &Build, reference: bool| {
        let opts = if reference {
            options.clone().on_reference_basis()
        } else {
            options.clone()
        };
        let session = calculate_with_data_session(build, data, &opts).unwrap();
        assert!(session.unsupported_modifier_texts().is_empty());
        session
    };
    for main in ["Melee1HMacePlayer", "FireballPlayer"] {
        let build = make(main, true, true);
        let game = calc(&build, false);
        let reference = calc(&build, true);
        // 25 * 1.39 AuraEffect * 2.21 Magnitude = 76.7975 -> 76.
        assert_eq!(game.stat_totals("AttackSpeed").1, 76.0);
        assert_eq!(game.minion_stat_totals(0, "AttackSpeed").1, 76.0);
        assert_eq!(
            game.minion_stat_formula(0, "ActionRate").unwrap().value,
            2.2
        );
        assert_eq!(
            reference
                .minion_stat_formula(0, "ActionRate")
                .unwrap()
                .value,
            1.25
        );
        if main == "Melee1HMacePlayer" {
            assert!(
                (game.output().action_rate / reference.output().action_rate - 1.76).abs() < 1e-9
            );
        } else {
            assert_eq!(game.output().action_rate, reference.output().action_rate);
            assert_eq!(game.stat_totals("CastSpeed").1, 0.0);
        }
        let player = game.mods_named("AttackSpeed");
        assert_eq!(player.len(), 1);
        let origin = player[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.id, format!("buff.WarBannerPlayer.{STAT}"));
        assert!(origin.raw_text.as_ref().unwrap().contains(STAT));
        let rule = player[0].exporter_rule.as_ref().unwrap();
        assert_eq!(rule.id, STAT);
        assert_eq!(rule.game_source, STAT);
        assert_eq!(rule.exporter, ExporterCoverage::Absent);
        let minion = game.minion_stat_mods(0, "AttackSpeed");
        assert_eq!(minion.len(), 1);
        assert!(minion[0].active);
        assert_eq!(minion[0].value, Some(76.0));
        assert_eq!(minion[0].modifier.origin.as_ref(), Some(origin));
        assert_eq!(minion[0].modifier.exporter_rule, player[0].exporter_rule);
        // Only the missing speed stat is omitted on the reference basis.
        assert!(reference.mods_named("AttackSpeed").is_empty());
        assert_eq!(reference.minion_stat_totals(0, "AttackSpeed").1, 0.0);
        for stat in ["Damage", "Accuracy"] {
            assert_eq!(
                game.minion_stat_totals(0, stat),
                reference.minion_stat_totals(0, stat)
            );
        }
        assert_eq!(reference.minion_stat_totals(0, "Damage").2, 1.76);
        assert_eq!(reference.minion_stat_totals(0, "Accuracy").1, 153.0);

        for inactive in [make(main, true, false), make(main, false, true)] {
            let session = calc(&inactive, false);
            assert_eq!(session.stat_totals("AttackSpeed").1, 0.0);
            assert_eq!(session.minion_stat_totals(0, "AttackSpeed").1, 0.0);
            assert_eq!(
                session.minion_stat_formula(0, "ActionRate").unwrap().value,
                1.25
            );
        }
        let mut unsupported = build.clone();
        // Blasphemy's aura penalties belong to supported curses; the same
        // group cannot make them scale a Banner's native speed grant.
        unsupported.socket_groups[2]
            .gem_skills
            .push(pobr_build::build::GemSkillRef {
                skill_id: "SupportBlasphemyPlayer".into(),
                gem_level: 20,
                ..Default::default()
            });
        assert_eq!(
            calc(&unsupported, false)
                .minion_stat_totals(0, "AttackSpeed")
                .1,
            76.0
        );
    }
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

#[test]
fn item_grant_matches_the_listed_weapon_set_before_redirecting_the_main_group() {
    let data = crate::test_data();
    let build = parse_build(
        r#"<PathOfBuilding2><Build level="100" className="Witch" mainSocketGroup="3"/>
<Items><Item id="1">Rarity: RARE
First Staff
Ashen Staff
Implicits: 1
Grants Skill: Level 20 Firebolt
</Item><Item id="2">Rarity: RARE
Second Staff
Ashen Staff
Implicits: 1
Grants Skill: Level 20 Firebolt
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 1 Swap" itemId="2"/></ItemSet></Items>
<Skills>
<Skill enabled="true" set1="false" set2="true"><Gem skillId="FireboltPlayer" level="20" enabled="true"/></Skill>
<Skill enabled="true" set1="true" set2="false"><Gem skillId="FireboltPlayer" level="20" enabled="true"/></Skill>
<Skill enabled="true" slot="Weapon 1" source="Item:1:First Staff, Ashen Staff"><Gem skillId="FireboltPlayer" level="20" enabled="true"/></Skill>
<Skill enabled="true" slot="Weapon 1 Swap" source="Item:2:Second Staff, Ashen Staff"><Gem skillId="FireboltPlayer" level="20" enabled="true"/></Skill>
</Skills></PathOfBuilding2>"#,
    )
    .unwrap();
    assert_eq!(build.weapon_set, pobr_build::WeaponSet::First);
    assert_eq!(
        pobr_build::resolve_main_skill_selection(&build, data),
        Some((1, 1, "FireboltPlayer".into())),
    );
    let mut second = build;
    second.main_socket_group = Some(4);
    second.weapon_set = pobr_build::WeaponSet::Second;
    std::mem::swap(&mut second.items, &mut second.other_weapons);
    assert_eq!(
        pobr_build::resolve_main_skill_selection(&second, data),
        Some((0, 1, "FireboltPlayer".into())),
    );
}

#[test]
fn item_grant_keeps_an_api_created_manual_gem_independent() {
    let data = crate::test_data();
    let build = Build::new()
        .add_socket_group(pobr_build::SocketGroup::new().with_gem_skill("CometPlayer", 20))
        .set_item(
            EquipmentSlot::Ring1,
            Item {
                base: "Sapphire Ring".into(),
                rarity: ItemRarity::Unique,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec!["Grants Skill: Level 20 Comet".into()],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                socketed: vec![],
                parsed_stats: vec![],
            },
        );
    let rows =
        pobr_build::calculate_skill_metrics(&build, data, &DataOrchestratorOptions::default())
            .unwrap();
    assert_eq!(
        rows.skills
            .iter()
            .filter(|s| s.skill_id == "CometPlayer")
            .count(),
        2
    );
    let diagnosis =
        pobr_build::diagnose(&build, data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(
        diagnosis.skills[0].gem_level, 20,
        "manual gem is not downgraded as a grant"
    );
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
