//! Skills a totem uses.
//!
//! A spell socketed in the Spell Totem gem is cast by the totem: the support
//! half of the gem adds the game's `UsedByTotem` type to it. What a line says
//! of totems is then said of that skill: "Spells Cast by Totems have 20%
//! increased Cast Speed", "25% increased Totem Damage". The same lines say
//! nothing of a spell the character casts.

use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, calculate_with_data, diagnose, parse_build,
};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

const SPELL_TOTEM: &str = r#"<Gem gemId="Metadata/Items/Gems/SkillGemSpellTotem" skillId="SummonMetaTotemSpellTotemPlayer" nameSpec="Spell Totem" variantId="MetaSpellTotem" enabled="true" level="20" quality="0"/>"#;

/// "Spells Cast by Totems have 20% increased Cast Speed" and "Attacks used by
/// Totems have 20% increased Attack Speed", as stats of a support.
const URGENT_TOTEMS: &str = r#"<Gem gemId="Metadata/Items/Gem/SupportGemAncestralUrgencyThree" skillId="SupportUrgentTotemsPlayerThree" nameSpec="Urgent Totems III" variantId="UrgentTotemsSupportThree" enabled="true" level="1" quality="0"/>"#;

/// Grim Pillars with `supports` in its group: cast by a Spell Totem when
/// `in_totem`, otherwise by the character.
fn build(in_totem: bool, supports: &str) -> Build {
    build_with(in_totem, supports, "")
}

/// The same build with `config` as the content of its `<Config>`.
fn build_with(in_totem: bool, supports: &str, config: &str) -> Build {
    let (totem, main) = if in_totem { (SPELL_TOTEM, 2) } else { ("", 1) };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="" treeVersion="0_5"/></Tree>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="{main}">
    {totem}
    <Gem gemId="Metadata/Items/Gems/SkillGemGrimPillars" skillId="GrimPillarsPlayer" nameSpec="Grim Pillars" variantId="GrimPillars" enabled="true" level="20" quality="0"/>
    {supports}
  </Skill>
</SkillSet></Skills>
<Config>{config}</Config>
</PathOfBuilding2>"#
    );
    parse_build(&xml).expect("parse synthetic build")
}

/// `(uses per second, average hit)` of the build's Grim Pillars with `lines`
/// among the character's modifiers.
fn pillars(data: &BuildData, build: &Build, lines: &[&str]) -> (f64, f64) {
    let options = DataOrchestratorOptions {
        extra_modifier_texts: lines.iter().map(|line| line.to_string()).collect(),
        ..DataOrchestratorOptions::default()
    };
    let skill = diagnose(build, data, &options)
        .expect("diagnose")
        .main_skill
        .expect("a main skill");
    assert_eq!(skill.skill_id, "GrimPillarsPlayer");
    let output = calculate_with_data(build, data, &options).expect("calculate");
    assert!(output.action_rate > 0.0 && output.total_hit_avg > 0.0);
    (output.action_rate, output.total_hit_avg)
}

fn assert_ratio(value: f64, base: f64, expected: f64) {
    assert!(
        (value / base - expected).abs() < 1e-9,
        "{value} / {base} = {}, expected {expected}",
        value / base
    );
}

#[test]
fn a_spell_totem_casts_its_spell_more_slowly_than_the_character() {
    let data = data();
    let (own_rate, own_hit) = pillars(&data, &build(false, ""), &[]);
    let (totem_rate, totem_hit) = pillars(&data, &build(true, ""), &[]);
    // `support_spell_totem_cast_speed_+%_final` of the gem's support half: -25.
    assert_ratio(totem_rate, own_rate, 0.75);
    assert_eq!(totem_hit, own_hit);
}

#[test]
fn cast_speed_of_spells_cast_by_totems_is_for_the_spell_a_totem_casts() {
    let data = data();
    let line = "Spells Cast by Totems have 20% increased Cast Speed";
    for in_totem in [true, false] {
        let build = build(in_totem, "");
        let (rate, hit) = pillars(&data, &build, &[]);
        let (with_line, hit_with_line) = pillars(&data, &build, &[line]);
        assert_ratio(with_line, rate, if in_totem { 1.2 } else { 1.0 });
        assert_eq!(hit_with_line, hit);
    }
}

#[test]
fn totem_damage_is_damage_of_the_skill_a_totem_uses() {
    let data = data();
    let line = "25% increased Totem Damage";
    for in_totem in [true, false] {
        let build = build(in_totem, "");
        let (rate, hit) = pillars(&data, &build, &[]);
        let (rate_with_line, with_line) = pillars(&data, &build, &[line]);
        assert_ratio(with_line, hit, if in_totem { 1.25 } else { 1.0 });
        assert_eq!(rate_with_line, rate);
    }
}

#[test]
fn urgent_totems_speeds_up_the_spell_the_totem_casts() {
    let data = data();
    let (rate, hit) = pillars(&data, &build(true, ""), &[]);
    let (urgent_rate, urgent_hit) = pillars(&data, &build(true, URGENT_TOTEMS), &[]);
    // `totem_skill_cast_speed_+%` of Urgent Totems III: 20.
    assert_ratio(urgent_rate, rate, 1.2);
    assert_eq!(urgent_hit, hit);
    // Its other half is for attacks; a spell gains the cast speed only, and
    // the line of a passive adds to it as increases do.
    let (both, _) = pillars(
        &data,
        &build(true, URGENT_TOTEMS),
        &["Spells Cast by Totems have 10% increased Cast Speed"],
    );
    assert_ratio(both, rate, 1.3);

    // The support applies to a totem's skill or to the skill that places the
    // totem, so it does nothing for a spell the character casts.
    let (own_rate, _) = pillars(&data, &build(false, ""), &[]);
    let (own_urgent_rate, _) = pillars(&data, &build(false, URGENT_TOTEMS), &[]);
    assert_eq!(own_urgent_rate, own_rate);
}

/// For a totem to be using a skill the character has summoned it.
#[test]
fn a_totem_in_use_has_been_summoned() {
    let data = data();
    let recently = "12% increased Cast Speed if you've summoned a Totem Recently";
    let per_totem = "Spells Cast by Totems have 3% increased Cast Speed per Summoned Totem";
    for in_totem in [true, false] {
        let build = build(in_totem, "");
        let (rate, _) = pillars(&data, &build, &[]);
        let (after_summoning, _) = pillars(&data, &build, &[recently]);
        assert_ratio(after_summoning, rate, if in_totem { 1.12 } else { 1.0 });
        // The totem casting the spell is one summoned totem.
        let (with_one, _) = pillars(&data, &build, &[per_totem]);
        assert_ratio(with_one, rate, if in_totem { 1.03 } else { 1.0 });
    }

    // How many more totems stand is the player's to say.
    let four = build_with(true, "", r#"<Input name="TotemsSummoned" number="4"/>"#);
    let (rate, _) = pillars(&data, &four, &[]);
    let (with_four, _) = pillars(&data, &four, &[per_totem]);
    assert_ratio(with_four, rate, 1.12);
}

/// A totem's cast is not the character's: "if you've Cast a Spell Recently"
/// says nothing while the totem does the casting.
#[test]
fn a_totems_cast_is_not_the_characters() {
    let data = data();
    let line = "20% increased Spell Damage if you've Cast a Spell Recently";
    for in_totem in [true, false] {
        let build = build(in_totem, "");
        let (_, hit) = pillars(&data, &build, &[]);
        let (_, with_line) = pillars(&data, &build, &[line]);
        assert_ratio(with_line, hit, if in_totem { 1.0 } else { 1.2 });
    }
}

#[test]
fn placement_reads_the_summoning_gems_cast_time() {
    let data = data();
    assert_eq!(
        data.granted_effects["SummonMetaTotemSpellTotemPlayer"].cast_time,
        Some(500)
    );
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec![
            "100% increased Totem Placement speed".into(),
            "100% increased Cast Speed".into(),
        ],
        ..Default::default()
    };
    let output = calculate_with_data(&build(true, ""), &data, &options).unwrap();
    assert_eq!(output.total_totem_placement_time, 0.25);
    assert!((output.mana_per_second_cost - output.mana_cost * 4.0).abs() < 1e-6);
    let own = calculate_with_data(&build(false, ""), &data, &options).unwrap();
    assert_eq!(own.total_totem_placement_time, 0.0);
}
