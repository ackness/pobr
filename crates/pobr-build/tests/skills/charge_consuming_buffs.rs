//! Buffs that scale by the charges their skill consumes.
//!
//! Pinnacle of Power (the game's `PinnacleOfPowerPlayer`, constant stat
//! `elemental_power_elemental_damage_+%_final_per_power_charge = 15`): "15%
//! more Elemental Damage per Power Charge Consumed". It consumes every Power
//! Charge the character has, so a character using Power Charges gains 15% per
//! charge, and one without charges gains nothing.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};

const FIREBALL: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>"#;
const BONESTORM: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemBonestorm" skillId="BonestormPlayer" nameSpec="Bonestorm" level="20" quality="0" enabled="true"/>"#;
const PINNACLE: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemPinnacleOfPower" skillId="PinnacleOfPowerPlayer" nameSpec="Pinnacle of Power" level="20" quality="0" enabled="true"/>"#;
/// "Benefits from consuming Charges for Supported Skill have 20% chance to be
/// doubled" (`chance_%_to_double_effect_of_removing_charges = 20`).
const HEIGHTENED_CHARGES: &str = r#"<Gem gemId="Metadata/Items/Gem/SupportGemTwofold" skillId="SupportHeightenedChargesPlayer" nameSpec="Heightened Charges" level="1" quality="0" enabled="true"/>"#;

/// The average hit of `main_gem` beside a socket group holding `buff_gems`.
fn average_hit(
    data: &BuildData,
    main_gem: &str,
    buff_gems: &[&str],
    use_power_charges: bool,
    modifiers: &[&str],
) -> f64 {
    let buff_group = if buff_gems.is_empty() {
        String::new()
    } else {
        format!(
            r#"<Skill enabled="true" mainActiveSkill="1">{}</Skill>"#,
            buff_gems.concat()
        )
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">{main_gem}</Skill>
  {buff_group}
</SkillSet></Skills>
<Config><Input name="usePowerCharges" boolean="{use_power_charges}"/></Config>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        mode_effective: true,
        extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
        ..Default::default()
    };
    let output = calculate_with_data(&build, data, &options).expect("calculate");
    assert!(output.total_hit_avg > 0.0, "{main_gem} deals no damage");
    output.total_hit_avg
}

use crate::test_data as data;

/// How much the buff of `buff_gems` multiplies the average hit by.
fn gain_from(
    data: &BuildData,
    main_gem: &str,
    buff_gems: &[&str],
    use_power_charges: bool,
    modifiers: &[&str],
) -> f64 {
    average_hit(data, main_gem, buff_gems, use_power_charges, modifiers)
        / average_hit(data, main_gem, &[], use_power_charges, modifiers)
}

/// How much Pinnacle of Power multiplies the average hit by.
fn gain(data: &BuildData, main_gem: &str, use_power_charges: bool, modifiers: &[&str]) -> f64 {
    gain_from(data, main_gem, &[PINNACLE], use_power_charges, modifiers)
}

#[test]
fn pinnacle_of_power_grants_more_elemental_damage_per_power_charge() {
    let data = data();
    // Three Power Charges by default.
    assert!((gain(&data, FIREBALL, true, &[]) - 1.45).abs() < 1e-9);
    let four = ["+1 to Maximum Power Charges"];
    assert!((gain(&data, FIREBALL, true, &four) - 1.60).abs() < 1e-9);
}

#[test]
fn pinnacle_of_power_without_power_charges_has_nothing_to_consume() {
    let data = data();
    assert!((gain(&data, FIREBALL, false, &[]) - 1.0).abs() < 1e-9);
}

/// "more Elemental Damage" is by the type of the damage, not of the skill: a
/// physical spell gains nothing, and the fire damage it is given gains in full.
#[test]
fn pinnacle_of_power_scales_elemental_damage_whatever_the_skill() {
    let data = data();
    assert!((gain(&data, BONESTORM, true, &[]) - 1.0).abs() < 1e-9);
    // Equal parts physical and fire: (1 + 1.45) / 2.
    let as_fire = ["Gain 100% of Damage as Extra Fire Damage"];
    assert!((gain(&data, BONESTORM, true, &as_fire) - 1.225).abs() < 1e-9);
}

/// Heightened Charges gives the benefit of each consumed charge a 20% chance
/// to be doubled: on average 15% x 1.2 per charge. It supports the skill that
/// consumes the charges, so it does nothing from another socket group.
#[test]
fn a_chance_to_double_consumed_charge_benefits_applies_to_the_supported_skill() {
    let data = data();
    let supported = gain_from(&data, FIREBALL, &[PINNACLE, HEIGHTENED_CHARGES], true, &[]);
    assert!((supported - 1.54).abs() < 1e-9, "{supported}");

    let beside_the_main_skill = average_hit(
        &data,
        &[FIREBALL, HEIGHTENED_CHARGES].concat(),
        &[PINNACLE],
        true,
        &[],
    ) / average_hit(&data, FIREBALL, &[], true, &[]);
    assert!(
        (beside_the_main_skill - 1.45).abs() < 1e-9,
        "{beside_the_main_skill}"
    );
}
