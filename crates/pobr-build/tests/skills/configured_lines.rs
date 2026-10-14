//! The lines a build's configuration adds: custom modifiers and quest rewards.
//!
//! A gem level or quality bonus counts wherever the character's lines are.
//! "+1 to Level of all Spell Skills" on an amulet raises Fireball. The same
//! line typed into the build's custom modifiers, or given to a calculation as
//! an extra modifier text, says the same thing about the character and raises
//! the gem the same way. The mod parser takes such a line as nothing to add,
//! so without this it parsed, was reported nowhere and changed nothing.
//!
//! A configured line that is neither a gem bonus nor parsed whole by a rule
//! is a dropped input and is reported as one, marked `Config:`.

use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, calculate_skill_metrics, calculate_with_data,
    diagnose, parse_build,
};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// A build with one Fireball gem carrying `gem_attributes`, the passive nodes
/// `nodes` and `config` as the content of its `<Config>`.
fn build(gem_attributes: &str, nodes: &str, config: &str) -> Build {
    build_of("Fireball", gem_attributes, nodes, config)
}

/// The same build with the gem of `skill` in place of Fireball.
fn build_of(skill: &str, gem_attributes: &str, nodes: &str, config: &str) -> Build {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="{nodes}" treeVersion="0_5"/></Tree>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGem{skill}" skillId="{skill}Player" nameSpec="{skill}" enabled="true" {gem_attributes}/>
  </Skill>
</SkillSet></Skills>
<Config>{config}</Config>
</PathOfBuilding2>"#
    );
    parse_build(&xml).expect("parse synthetic build")
}

/// A level 20 Fireball with `lines` as an enabled group of custom modifiers.
fn with_custom_modifiers(gem_attributes: &str, lines: &str) -> Build {
    build(
        gem_attributes,
        "",
        &format!(
            r#"<CustomModifierBlock title="Test" enabled="true">{lines}</CustomModifierBlock>"#
        ),
    )
}

const LEVEL_20: &str = r#"level="20" quality="0""#;

fn average_hit(data: &BuildData, build: &Build, options: &DataOrchestratorOptions) -> f64 {
    let output = calculate_with_data(build, data, options).expect("calculate");
    assert!(output.total_hit_avg > 0.0);
    output.total_hit_avg
}

/// The levels and quality the calculation gives the main skill beyond its gem.
fn bonus(data: &BuildData, build: &Build, options: &DataOrchestratorOptions) -> (u32, u32) {
    let skill = diagnose(build, data, options)
        .expect("diagnose")
        .main_skill
        .expect("a main skill");
    (skill.bonus_levels, skill.quality)
}

#[test]
fn a_level_bonus_in_the_custom_modifiers_raises_the_gems_it_names() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let hit = |build: &Build| average_hit(&data, build, &options);
    let level = |level: u32| hit(&build(&format!(r#"level="{level}" quality="0""#), "", ""));
    assert!(level(21) > level(20) && level(22) > level(21));

    let custom = |lines: &str| hit(&with_custom_modifiers(LEVEL_20, lines));
    assert_eq!(custom("+1 to Level of all Spell Skills"), level(21));
    assert_eq!(custom("+2 to Level of all Fire Skills"), level(22));
    // Each line counts, next to others that are not about gems.
    assert_eq!(
        custom(
            "+1 to Level of all Spell Skills\n+50 to maximum Life\n+1 to Level of all Projectile Skills"
        ),
        level(22)
    );
    // A bonus for skills Fireball is not leaves it alone.
    assert_eq!(custom("+3 to Level of all Minion Skills"), level(20));
    assert_eq!(custom("+3 to Level of all Cold Skills"), level(20));
    // Colour codes are not part of the line.
    assert_eq!(custom("^x7070FF+1 to Level of all Spell Skills"), level(21));

    let (levels, _) = bonus(
        &data,
        &with_custom_modifiers(LEVEL_20, "+2 to Level of all Spell Skills"),
        &options,
    );
    assert_eq!(levels, 2);
}

/// The game words the kind of a bonus two ways: "all Cold Skills" and "all
/// Nova Skill Gems". Grim Pillars is a Cold Nova spell.
#[test]
fn a_bonus_for_a_kind_of_skill_gems_raises_the_gems_of_that_kind() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let levels = |skill: &str, lines: &str| {
        let build = build_of(
            skill,
            LEVEL_20,
            "",
            &format!(
                r#"<CustomModifierBlock title="Test" enabled="true">{lines}</CustomModifierBlock>"#
            ),
        );
        bonus(&data, &build, &options).0
    };
    assert_eq!(
        levels("GrimPillars", "+1 to Level of all Nova Skill Gems"),
        1
    );
    assert_eq!(levels("GrimPillars", "+1 to Level of all Cold Skills"), 1);
    assert_eq!(
        levels(
            "GrimPillars",
            "+1 to Level of all Nova Skill Gems\n+2 to Level of all Cold Spell Skills"
        ),
        3
    );
    // Fireball is neither a Nova nor a skill that summons a totem.
    assert_eq!(levels("Fireball", "+1 to Level of all Nova Skill Gems"), 0);
    assert_eq!(levels("Fireball", "+1 to Level of all Totem Skill Gems"), 0);
    assert_eq!(levels("Fireball", "+1 to Level of all Elemental Skills"), 1);
    assert_eq!(
        levels("GrimPillars", "+1 to Level of all Elemental Skills"),
        1
    );
}

/// A kind is a kind of gem, which the gem's tags say (the game's `GemEffects`
/// table). Rain of Arrows fires arrows that fall instead of flying: its skill
/// is not of the game's `Projectile` type, and the gem is tagged `projectile`.
/// Shockwave Totem places a totem and is tagged for the Slam the totem uses
/// (`slam`, `melee`, `nova`), which is another skill of the same gem.
#[test]
fn a_gem_is_of_the_kinds_its_tags_say() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let levels = |skill: &str, lines: &str| {
        let build = build_of(
            skill,
            LEVEL_20,
            "",
            &format!(
                r#"<CustomModifierBlock title="Test" enabled="true">{lines}</CustomModifierBlock>"#
            ),
        );
        bonus(&data, &build, &options).0
    };
    assert_eq!(
        levels("RainOfArrows", "+3 to Level of all Projectile Skills"),
        3
    );
    assert_eq!(
        levels("RainOfArrows", "+1 to Level of all Attack Skills"),
        1
    );
    assert_eq!(levels("RainOfArrows", "+1 to Level of all Spell Skills"), 0);
    assert_eq!(
        levels("ShockwaveTotem", "+1 to Level of all Slam Skill Gems"),
        1
    );
    assert_eq!(
        levels("ShockwaveTotem", "+1 to Level of all Melee Skills"),
        1
    );
    assert_eq!(
        levels("ShockwaveTotem", "+1 to Level of all Totem Skill Gems"),
        1
    );
    assert_eq!(
        levels("ShockwaveTotem", "+1 to Level of all Nova Skill Gems"),
        1
    );
    assert_eq!(
        levels("ShockwaveTotem", "+1 to Level of all Projectile Skills"),
        0
    );
    // A spell that flies is a projectile as before; one that does not is not.
    assert_eq!(
        levels("Fireball", "+1 to Level of all Projectile Skills"),
        1
    );
    assert_eq!(
        levels("GrimPillars", "+1 to Level of all Projectile Skills"),
        0
    );
    // Every word of a kind is a tag the gem has to carry, and "Elemental"
    // is any of the three elements.
    assert_eq!(
        levels("Fireball", "+1 to Level of all Fire Spell Skills"),
        1
    );
    assert_eq!(
        levels("Fireball", "+1 to Level of all Cold Spell Skills"),
        0
    );
    assert_eq!(levels("Fireball", "+1 to Level of all Elemental Skills"), 1);
    assert_eq!(
        levels("RainOfArrows", "+1 to Level of all Elemental Skills"),
        0
    );
}

#[test]
fn a_bonus_for_corrupted_gems_in_the_custom_modifiers_tests_the_gem() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let line = "+1 to Level of all Corrupted Skill Gems";
    let levels = |gem: &str| bonus(&data, &with_custom_modifiers(gem, line), &options).0;
    assert_eq!(levels(r#"level="20" quality="0" corrupted="true""#), 1);
    assert_eq!(levels(LEVEL_20), 0);
}

#[test]
fn a_quality_bonus_in_the_custom_modifiers_adds_to_the_gems_quality() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let quality =
        |gem: &str, lines: &str| bonus(&data, &with_custom_modifiers(gem, lines), &options).1;
    assert_eq!(quality(LEVEL_20, ""), 0);
    assert_eq!(quality(LEVEL_20, "+8% to Quality of all Skills"), 8);
    assert_eq!(
        quality(r#"level="20" quality="12""#, "+8% to Quality of all Skills"),
        20
    );
}

#[test]
fn a_disabled_group_of_custom_modifiers_adds_nothing() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let disabled = build(
        LEVEL_20,
        "",
        r#"<CustomModifierBlock title="Later" enabled="false">+2 to Level of all Spell Skills</CustomModifierBlock>"#,
    );
    assert_eq!(bonus(&data, &disabled, &options), (0, 0));
    assert_eq!(
        average_hit(&data, &disabled, &options),
        average_hit(&data, &build(LEVEL_20, "", ""), &options)
    );
}

#[test]
fn a_bonus_given_as_an_extra_modifier_text_raises_gems_too() {
    let data = data();
    let plain = DataOrchestratorOptions::default();
    let extra = DataOrchestratorOptions {
        extra_modifier_texts: vec![
            "+1 to Level of all Spell Skills".into(),
            "+10% to Quality of all Skills".into(),
            "+50 to maximum Life".into(),
        ],
        ..DataOrchestratorOptions::default()
    };
    let gem = build(LEVEL_20, "", "");
    assert_eq!(bonus(&data, &gem, &plain), (0, 0));
    assert_eq!(bonus(&data, &gem, &extra), (1, 10));
    // The level is the one a level 21 gem with that quality has.
    let level_21 = build(r#"level="21" quality="10""#, "", "");
    let life_only = DataOrchestratorOptions {
        extra_modifier_texts: vec!["+50 to maximum Life".into()],
        ..DataOrchestratorOptions::default()
    };
    assert_eq!(
        average_hit(&data, &gem, &extra),
        average_hit(&data, &level_21, &life_only)
    );
}

/// The per-skill table calculates each skill of the build under the same
/// options, so it shows the same number as the main panel.
#[test]
fn the_per_skill_table_counts_an_extra_bonus_like_the_main_calculation() {
    let data = data();
    let extra = DataOrchestratorOptions {
        extra_modifier_texts: vec!["+1 to Level of all Spell Skills".into()],
        ..DataOrchestratorOptions::default()
    };
    let gem = build(LEVEL_20, "", "");
    let table = calculate_skill_metrics(&gem, &data, &extra).expect("skill metrics");
    let row = table
        .skills
        .iter()
        .find(|row| row.skill_id == "FireballPlayer")
        .expect("a Fireball row");
    let output = row.output.as_ref().expect("a calculated row");
    assert_eq!(output.total_hit_avg, average_hit(&data, &gem, &extra));
    assert!(output.total_hit_avg > average_hit(&data, &gem, &DataOrchestratorOptions::default()));
}

/// A gem bonus on a passive node is read as a gem bonus. No rule parses the
/// tree's wording of it, and it is not an input the calculation dropped,
/// whether the node is allocated on the tree or by an item, and whether or not
/// the bonus is for the skill in use.
#[test]
fn a_gem_bonus_on_a_passive_node_is_counted_and_not_reported_as_dropped() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let line_of = |node: u32| data.passive_nodes[&node].stats.join("\n");
    let reported = |build: &Build| {
        let diagnosis = diagnose(build, &data, &options).expect("diagnose");
        let levels = diagnosis.main_skill.expect("a main skill").bonus_levels;
        let lines: Vec<String> = diagnosis
            .unparsed_lines
            .into_iter()
            .filter(|line| line.contains("to Level of all"))
            .collect();
        (levels, lines)
    };

    // A node of the Gemling Legionnaire, allocated on the tree.
    assert_eq!(
        line_of(37397),
        "+2 to Level of all Skills with an [Intelligence] requirement"
    );
    assert_eq!(reported(&build(LEVEL_20, "37397", "")), (2, vec![]));

    // The notable Endless Blizzard, allocated by an amulet.
    assert_eq!(line_of(19955), "+1 to Level of all [Cold] Skills");
    let anointed = |build: Build| {
        build.set_item(
            EquipmentSlot::Amulet,
            Item {
                base: ItemBaseId::from("Jade Amulet"),
                rarity: ItemRarity::Rare,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec!["Allocates Endless Blizzard".into()],
                rolled_defence: RolledDefence::default(),
                socketed: Vec::new(),
                parsed_stats: vec![],
            },
        )
    };
    let comet = build_of("Comet", LEVEL_20, "", "");
    assert_eq!(reported(&comet), (0, vec![]));
    assert_eq!(reported(&anointed(comet)), (1, vec![]));
    assert_eq!(reported(&anointed(build(LEVEL_20, "", ""))), (0, vec![]));
}

/// "… with an Intelligence requirement" names the gems by what they require.
/// The parser alone reads the tail of that line as a change to an attribute
/// requirement; read as the gem bonus it is, it stores no such modifier.
#[test]
fn a_bonus_by_attribute_requirement_is_a_gem_bonus_in_every_channel() {
    let data = data();
    let line = "+2 to Level of all Skills with an Intelligence requirement";
    let stores_a_requirement = |diagnosis: &pobr_build::Diagnosis| {
        diagnosis
            .unread_modifiers
            .iter()
            .any(|modifier| modifier.name.contains("Requirement"))
    };

    let custom = diagnose(
        &with_custom_modifiers(LEVEL_20, line),
        &data,
        &DataOrchestratorOptions::default(),
    )
    .expect("diagnose");
    assert_eq!(
        custom.main_skill.as_ref().expect("main skill").bonus_levels,
        2
    );
    assert!(
        custom.unparsed_lines.is_empty(),
        "{:?}",
        custom.unparsed_lines
    );
    assert!(!stores_a_requirement(&custom));

    let extra = diagnose(
        &build(LEVEL_20, "", ""),
        &data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![line.into()],
            ..DataOrchestratorOptions::default()
        },
    )
    .expect("diagnose");
    assert_eq!(
        extra.main_skill.as_ref().expect("main skill").bonus_levels,
        2
    );
    assert!(
        extra.unparsed_lines.is_empty(),
        "{:?}",
        extra.unparsed_lines
    );
    assert!(!stores_a_requirement(&extra));

    // A Strength requirement is not one Fireball has.
    let other = with_custom_modifiers(
        LEVEL_20,
        "+2 to Level of all Skills with a Strength requirement",
    );
    assert_eq!(
        bonus(&data, &other, &DataOrchestratorOptions::default()).0,
        0
    );
}

#[test]
fn a_configured_line_no_rule_parses_is_reported_and_the_rest_still_count() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let life = |build: &Build| {
        calculate_with_data(build, &data, &options)
            .expect("calculate")
            .life
    };
    let lines = "Zorblax the unknowable 12%\n+50 to maximum Life\n+1 to Level of all Spell Skills";
    let configured = with_custom_modifiers(LEVEL_20, lines);
    let diagnosis = diagnose(&configured, &data, &options).expect("diagnose");
    assert_eq!(
        diagnosis.unparsed_lines,
        ["Config: Zorblax the unknowable 12%"]
    );
    assert_eq!(diagnosis.main_skill.expect("main skill").bonus_levels, 1);
    let plain = build(LEVEL_20, "", "");
    assert!(life(&configured) > life(&plain));
    assert_eq!(
        life(&configured),
        life(&with_custom_modifiers(LEVEL_20, "+50 to maximum Life"))
    );
}

/// A line can parse and still do nothing, waiting for something nothing in
/// the calculation sets. "while wielding a Melee Weapon" waits for
/// `UsingMeleeWeapon`, which no pass of the engine and no config option sets;
/// "while moving" waits for `Moving`, which a config option sets and this
/// build leaves off. "per non-Idol Augment in your Equipment" counts on the
/// player's side, where the build sets the count, and "while holding a
/// Shield" waits for what the orchestration sets from the off hand: neither
/// is missing anything.
#[test]
fn modifiers_waiting_for_what_nothing_sets_are_reported() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let lines = "12% increased Spell Damage while wielding a Melee Weapon\n\
                 20% increased Evasion Rating while moving\n\
                 +1% to all Elemental Resistances per non-Idol Augment in your Equipment\n\
                 Attack Skills deal 10% increased Damage while holding a Shield";
    let diagnosis =
        diagnose(&with_custom_modifiers(LEVEL_20, lines), &data, &options).expect("diagnose");
    assert!(
        diagnosis.unparsed_lines.is_empty(),
        "{:?}",
        diagnosis.unparsed_lines
    );

    let unset = |condition: &str| {
        diagnosis
            .unset_conditions
            .iter()
            .find(|entry| entry.condition == condition)
    };
    let from_line = |entry: &pobr_build::UnsetCondition, words: &str| {
        entry
            .modifiers
            .iter()
            .any(|modifier| modifier.source.contains(words))
    };
    let melee = unset("UsingMeleeWeapon").expect("UsingMeleeWeapon is reported");
    assert_eq!(melee.config_option, None);
    assert!(!melee.flagged);
    assert!(
        from_line(melee, "while wielding a Melee Weapon"),
        "{:?}",
        melee.modifiers
    );
    let moving = unset("Moving").expect("Moving is reported");
    assert_eq!(moving.config_option.as_deref(), Some("conditionMoving"));
    assert!(from_line(moving, "while moving"), "{:?}", moving.modifiers);
    assert!(unset("UsingShield").is_none());
    // What nothing sets comes before what the config leaves off.
    assert!(
        diagnosis
            .unset_conditions
            .windows(2)
            .all(|pair| pair[0].config_option.is_none() || pair[1].config_option.is_some())
    );
    // Set by the config, it is not reported.
    let moving_set = diagnose(
        &build(
            LEVEL_20,
            "",
            &format!(
                r#"<Input name="conditionMoving" boolean="true"/><CustomModifierBlock title="Test" enabled="true">{lines}</CustomModifierBlock>"#
            ),
        ),
        &data,
        &options,
    )
    .expect("diagnose");
    assert!(
        moving_set
            .unset_conditions
            .iter()
            .all(|entry| entry.condition != "Moving")
    );

    // Read on the player's side, a count the build sets is provided: the
    // build counts the Augments in its equipment, none here.
    assert!(
        diagnosis
            .unprovided_multipliers
            .iter()
            .all(|(variable, _)| variable != "NonIdolAugmentsInEquipment"),
        "{:?}",
        diagnosis.unprovided_multipliers
    );
}

/// A quest reward is a line of the configuration too. The option for taking
/// no reward is not a line, and "5% increased Experience Gain" (one of the
/// Seven Pillars) changes nothing a calculation reports and is a line the
/// parser knows: neither is an input the calculation dropped.
#[test]
fn a_quest_reward_is_reported_only_when_its_line_is_not_understood() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let unparsed = |config: &str| {
        diagnose(&build(LEVEL_20, "", config), &data, &options)
            .expect("diagnose")
            .unparsed_lines
    };
    let no_reward = r#"<Input name="questAct 3Venom CryptsVenom Draught" string="None"/>"#;
    assert_eq!(unparsed(no_reward), [""; 0]);
    // As a build code stores an option with several lines.
    let seven_pillars = "<Input name=\"questInterlude 2QimahSeven Pillars\" string=\"5% increased Experience Gain\n\t-5% to all Elemental Resistances\n\t3% reduced Movement Speed\"/>";
    assert_eq!(unparsed(seven_pillars), [""; 0]);
    let unknown = r#"<Input name="questAct 3Venom CryptsVenom Draught" string="Zorblax the unknowable 12%"/>"#;
    assert_eq!(unparsed(unknown), ["Config: Zorblax the unknowable 12%"]);
}

/// "Defend with 120% of Armour while not on Low Energy Shield" (the game
/// stat `defend_with_%_of_armour_while_not_on_low_energy_shield`): against
/// hits the Armour counts 120%, as "Defend with 120% of Armour" has it,
/// unless the character is on Low Energy Shield.
#[test]
fn armour_defends_more_while_not_on_low_energy_shield() {
    let data = data();
    let options = DataOrchestratorOptions::reference_conditions();
    let reduction = |lines: &str, config: &str| {
        let build = build(
            LEVEL_20,
            "",
            &format!(
                r#"{config}<CustomModifierBlock title="Test" enabled="true">+5000 to Armour
{lines}</CustomModifierBlock>"#
            ),
        );
        calculate_with_data(&build, &data, &options)
            .expect("calculate")
            .physical_damage_reduction
    };
    let line = "Defend with 120% of Armour while not on Low Energy Shield";
    let plain = reduction("", "");
    let defended = reduction("Defend with 120% of Armour", "");
    assert!(defended > plain, "{plain} -> {defended}");
    assert_eq!(reduction(line, ""), defended);
    let low = r#"<Input name="conditionLowEnergyShield" boolean="true"/>"#;
    assert_eq!(reduction(line, low), reduction("", low));
}

/// "Non-Channelling Spells cost an additional N% of your maximum Life" adds
/// that share of the maximum Life, rounded down, to the cost of a spell that
/// is not channelled. A line for attacks leaves a spell's cost alone.
#[test]
fn a_spell_costs_a_share_of_maximum_life() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let calculate = |skill: &str, lines: &str| {
        let build = build_of(
            skill,
            LEVEL_20,
            "",
            &format!(
                r#"<CustomModifierBlock title="Test" enabled="true">{lines}</CustomModifierBlock>"#
            ),
        );
        calculate_with_data(&build, &data, &options).expect("calculate")
    };
    let line = "Non-Channelling Spells cost an additional 6% of your maximum Life";
    let plain = calculate("Fireball", "");
    let fireball = calculate("Fireball", line);
    assert_eq!(plain.life_cost, 0.0);
    assert!(fireball.life > 0.0);
    assert_eq!(fireball.life_cost, (fireball.life * 0.06).floor());
    assert_eq!(fireball.mana_cost, plain.mana_cost);
    assert_eq!(calculate("Incinerate", line).life_cost, 0.0);
    let attacks = "Attacks cost an additional 6% of your maximum Mana";
    assert_eq!(calculate("Fireball", attacks).mana_cost, plain.mana_cost);
}
