//! Withered on the enemy: 5% increased Chaos Damage taken per stack, at most
//! 10 stacks, for a character that can inflict it.
//!
//! The stack count is the player's statement (`multiplierWitheredStackCount`).
//! It counts when one of the character's skills, its Presence or one of its
//! minions can inflict Withered, and for nobody else.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};

/// A spell whose hit is Chaos damage only.
const HEXBLAST: &str = r#"<Gem gemId="Metadata/Items/Gems/SkillGemHexblast" skillId="HexblastPlayer" nameSpec="Hexblast" level="20" quality="0" enabled="true"/>"#;
const FIREBALL: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>"#;
/// "Every 1.42 seconds, inflicts Withered on enemies in your Presence".
const WITHERING_PRESENCE: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemWitheringPresence" skillId="WitheringPresencePlayer" nameSpec="Withering Presence" level="20" quality="0" enabled="true"/>"#;

const INFLICTS: &str = "25% chance to inflict Withered for 2 seconds on Hit";

use crate::test_data as data;

/// The DPS of `main_gem` against an enemy stated to hold `stacks` Withered,
/// for a character with `modifiers` and the gems of `other_group`.
fn dps(
    data: &BuildData,
    main_gem: &str,
    other_group: &str,
    stacks: u32,
    modifiers: &[&str],
) -> f64 {
    let other = if other_group.is_empty() {
        String::new()
    } else {
        format!(r#"<Skill enabled="true" mainActiveSkill="1">{other_group}</Skill>"#)
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">{main_gem}</Skill>
  {other}
</SkillSet></Skills>
<Config><Input name="multiplierWitheredStackCount" number="{stacks}"/></Config>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        mode_effective: true,
        extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
        ..Default::default()
    };
    let output = calculate_with_data(&build, data, &options).expect("calculate");
    assert!(output.dps > 0.0, "{main_gem} deals no damage");
    output.dps
}

/// How much `stacks` stated Withered multiply the DPS by.
fn gain(data: &BuildData, main_gem: &str, other: &str, stacks: u32, modifiers: &[&str]) -> f64 {
    dps(data, main_gem, other, stacks, modifiers) / dps(data, main_gem, other, 0, modifiers)
}

#[test]
fn each_stack_makes_the_enemy_take_five_percent_more_chaos_damage() {
    let data = data();
    assert!((gain(&data, HEXBLAST, "", 4, &[INFLICTS]) - 1.20).abs() < 1e-9);
    assert!((gain(&data, HEXBLAST, "", 10, &[INFLICTS]) - 1.50).abs() < 1e-9);
}

#[test]
fn stacks_beyond_ten_do_nothing() {
    let data = data();
    assert!((gain(&data, HEXBLAST, "", 15, &[INFLICTS]) - 1.50).abs() < 1e-9);
}

#[test]
fn stated_stacks_do_nothing_for_a_character_that_cannot_inflict_withered() {
    let data = data();
    assert!((gain(&data, HEXBLAST, "", 10, &[]) - 1.0).abs() < 1e-9);
}

/// The chance belongs to the minions, and this character has none.
#[test]
fn a_chance_for_minions_to_inflict_withered_needs_minions() {
    let data = data();
    let minions_only = ["Minions have 10% chance to inflict Withered on Hit"];
    assert!((gain(&data, HEXBLAST, "", 10, &minions_only) - 1.0).abs() < 1e-9);
}

/// Any of the character's skills can put the stacks on the enemy the
/// calculated skill hits.
#[test]
fn another_skill_of_the_build_can_inflict_the_withered() {
    let data = data();
    assert!((gain(&data, HEXBLAST, WITHERING_PRESENCE, 10, &[]) - 1.50).abs() < 1e-9);
}

/// "Spells have a 25% chance to inflict Withered" is about the spells.
#[test]
fn a_chance_for_spells_counts_for_a_spell() {
    let data = data();
    let spells = ["Spells have a 25% chance to inflict Withered for 4 seconds on Hit"];
    assert!((gain(&data, HEXBLAST, "", 10, &spells) - 1.50).abs() < 1e-9);
}

#[test]
fn withered_changes_chaos_damage_only() {
    let data = data();
    assert!((gain(&data, FIREBALL, "", 10, &[INFLICTS]) - 1.0).abs() < 1e-9);
}

/// 20% increased magnitude makes a stack 6%; 10% leaves it at a whole 5%.
#[test]
fn withered_magnitude_scales_each_stack() {
    let data = data();
    let with = |magnitude: &str| gain(&data, HEXBLAST, "", 10, &[INFLICTS, magnitude]);
    assert!((with("20% increased Withered Magnitude") - 1.60).abs() < 1e-9);
    assert!((with("10% increased Withered Magnitude") - 1.50).abs() < 1e-9);
}
