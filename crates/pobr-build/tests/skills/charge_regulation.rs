//! Charge Regulation: a buff for each kind of charge the character holds.
//!
//! The game's `ChargeRegulationPlayer` at gem level 20: "26% more Critical
//! Hit Chance while you have a Power Charge", "20% more Armour, Evasion and
//! Energy Shield while you have an Endurance Charge" and "25% increased Skill
//! Speed while you have a Frenzy Charge".

use pobr_build::{
    BuildData, DataOrchestratorOptions, calculate_with_data, calculate_with_data_session,
    parse_build,
};
use pobr_core::calc::OutputTable;

const FIREBALL: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>"#;
const CHARGE_REGULATION: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemChargeInfusion" skillId="ChargeRegulationPlayer" nameSpec="Charge Regulation" level="20" quality="0" enabled="true"/>"#;

use crate::test_data as data;

/// A Fireball caster with flat defences, with or without Charge Regulation,
/// using the charges named in `charges` (`usePowerCharges`, ...).
fn calculate(data: &BuildData, regulation: bool, charges: &[&str]) -> OutputTable {
    let buff = if regulation {
        format!(r#"<Skill enabled="true" mainActiveSkill="1">{CHARGE_REGULATION}</Skill>"#)
    } else {
        String::new()
    };
    let inputs: String = charges
        .iter()
        .map(|name| format!(r#"<Input name="{name}" boolean="true"/>"#))
        .collect();
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">{FIREBALL}</Skill>
  {buff}
</SkillSet></Skills>
<Config>{inputs}</Config>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        mode_effective: true,
        extra_modifier_texts: [
            "+1000 to Armour",
            "+1000 to Evasion Rating",
            "+1000 to maximum Energy Shield",
        ]
        .map(String::from)
        .to_vec(),
        ..Default::default()
    };
    calculate_with_data(&build, data, &options).expect("calculate")
}

fn ratio(with: f64, without: f64) -> f64 {
    assert!(without > 0.0);
    with / without
}

#[test]
fn endurance_charges_bring_more_armour_evasion_and_energy_shield() {
    let data = data();
    let charges = ["useEnduranceCharges"];
    let with = calculate(&data, true, &charges);
    let without = calculate(&data, false, &charges);
    for (name, with, without) in [
        ("armour", with.armour, without.armour),
        ("evasion", with.evasion, without.evasion),
        ("energy shield", with.energy_shield, without.energy_shield),
    ] {
        assert!(
            (ratio(with, without) - 1.20).abs() < 1e-3,
            "{name}: {with} / {without}"
        );
    }
    // The other two effects need their own charges.
    assert_eq!(with.crit_chance, without.crit_chance);
    assert_eq!(with.action_rate, without.action_rate);
}

#[test]
fn power_charges_bring_more_critical_hit_chance() {
    let data = data();
    let charges = ["usePowerCharges"];
    let with = calculate(&data, true, &charges);
    let without = calculate(&data, false, &charges);
    assert!((ratio(with.crit_chance, without.crit_chance) - 1.26).abs() < 1e-3);
    assert_eq!(with.armour, without.armour);
}

#[test]
fn frenzy_charges_bring_skill_speed() {
    let data = data();
    let charges = ["useFrenzyCharges"];
    let with = calculate(&data, true, &charges);
    let without = calculate(&data, false, &charges);
    // 25% increased on a caster with no other increase.
    assert!((ratio(with.action_rate, without.action_rate) - 1.25).abs() < 1e-3);
}

#[test]
fn without_charges_the_buff_grants_nothing() {
    let data = data();
    let with = calculate(&data, true, &[]);
    let without = calculate(&data, false, &[]);
    assert_eq!(with.armour, without.armour);
    assert_eq!(with.energy_shield, without.energy_shield);
    assert_eq!(with.crit_chance, without.crit_chance);
    assert_eq!(with.action_rate, without.action_rate);
}

#[test]
fn conduit_redirects_gains_and_preserves_minimum_charges() {
    let data = data();
    // Conduit (keystone_conduit, passive 33979) gives incoming charges to allies.
    // The game stat additional_critical_strike_chance_per_power_charge_permyriad
    // supplies the test's +1% Critical Hit Chance per Power Charge: three charges
    // add 3 percentage points, zero adds none, and minimum one retains 1 point.
    for minimum in [0, 1] {
        let calculate = |conduit: bool, resonance: bool, reference: bool| {
            let nodes = if conduit { "33979" } else { "" };
            let build = parse_build(&format!(
                r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="{nodes}"/></Tree>
<Skills><Skill enabled="true">{FIREBALL}</Skill></Skills>
<Config>
<Input name="usePowerCharges" boolean="true"/><Input name="overridePowerCharges" number="3"/>
<Input name="useFrenzyCharges" boolean="true"/><Input name="overrideFrenzyCharges" number="3"/>
<Input name="useEnduranceCharges" boolean="true"/><Input name="overrideEnduranceCharges" number="3"/>
</Config></PathOfBuilding2>"#
            ))
            .unwrap();
            let mut options = DataOrchestratorOptions {
                extra_modifier_texts: vec![
                    "+1% Critical Hit Chance per Power Charge".into(),
                    format!("+{minimum} to Minimum Power Charges"),
                    format!("+{minimum} to Minimum Frenzy Charges"),
                    format!("+{minimum} to Minimum Endurance Charges"),
                ],
                ..Default::default()
            };
            if resonance {
                options.extra_modifier_texts.extend([
                    "Gain Endurance Charges instead of Power Charges".into(),
                    "Gain Frenzy Charges instead of Endurance Charges".into(),
                    "Gain Power Charges instead of Frenzy Charges".into(),
                ]);
            }
            if reference {
                options = options.on_reference_basis();
            }
            calculate_with_data_session(&build, &data, &options).unwrap()
        };
        let baseline = calculate(false, false, false);
        let reference = calculate(true, false, true);
        assert_eq!(
            baseline.output().crit_chance,
            reference.output().crit_chance
        );
        for resonance in [false, true] {
            let conduit = calculate(true, resonance, false);
            assert!(conduit.unsupported_modifier_texts().is_empty());
            for stat in [
                "ChargePowerCurrent",
                "ChargeFrenzyCurrent",
                "ChargeEnduranceCurrent",
            ] {
                assert_eq!(pobr_core::display_value(baseline.output(), stat), 3.0);
                assert_eq!(pobr_core::display_value(reference.output(), stat), 3.0);
                assert_eq!(
                    pobr_core::display_value(conduit.output(), stat),
                    f64::from(minimum)
                );
                // The existing detail reads the resolved charge snapshot.
                let detail = conduit.derived_detail(stat).unwrap();
                assert_eq!(detail.formula, "max(ConfiguredCharges, MinimumCharges)");
                assert_eq!(
                    detail.inputs[0].1.max(detail.inputs[1].1),
                    f64::from(minimum)
                );
            }
            let lost_crit = baseline.output().crit_chance - conduit.output().crit_chance;
            assert!((lost_crit - f64::from(3 - minimum) * 0.01).abs() < 1e-9);
        }
    }
}
