//! Cancellable reservation, duration modifier, Full Life threshold and Strike targets
//! on complete builds.

use pobr_build::calc_orchestrator::calculate_with_data_session_tracked;
use pobr_build::{
    DataOrchestratorOptions, calculate_with_data, calculate_with_data_session, parse_build,
    parse_build_from_code,
};
use pobr_core::calc::{CalculationSession, MinimalInput, OutputTable};
use pobr_core::display_value;
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::prelude::{DamageType, ModName, ModType};

use crate::test_data as data;

fn calculate(skills: &str, modifiers: &[&str]) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2><Build level="80" className="Witch" mainSocketGroup="1"/><Skills>{skills}</Skills></PathOfBuilding2>"#
    );
    let build = parse_build(&xml).unwrap();
    calculate_with_data(
        &build,
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
            mode_effective: true,
            ..Default::default()
        },
    )
    .unwrap()
}

const FIREBALL: &str =
    r#"<Skill mainActiveSkill="1"><Gem skillId="FireballPlayer" level="20"/></Skill>"#;

#[test]
fn a_permanent_life_reservation_is_reserved_but_not_cancellable() {
    let plain = calculate(FIREBALL, &[]);
    assert_eq!(display_value(&plain, "LifeCancellableReservation"), 100.0);
    assert_eq!(display_value(&plain, "ManaCancellableReservation"), 100.0);
    // "Reserves 25% of Life" is a permanent quarter of maximum Life, and
    // Reservation Efficiency does not change it.
    let reserved = calculate(
        FIREBALL,
        &[
            "Reserves 25% of Life",
            "50% increased Reservation Efficiency of Skills",
        ],
    );
    assert_eq!(display_value(&reserved, "LifeCancellableReservation"), 75.0);
    assert_eq!(
        display_value(&reserved, "ManaCancellableReservation"),
        100.0
    );
    let maximum = reserved.life_reserved + reserved.life_unreserved;
    assert!(
        (reserved.life_reserved - 0.25 * maximum).abs() < 1.0,
        "{} of {maximum}",
        reserved.life_reserved
    );
    assert!(plain.life_reserved < reserved.life_reserved);
}

#[test]
fn duration_mod_is_the_duration_calculations_modifier() {
    let plain = calculate(FIREBALL, &[]);
    assert_eq!(display_value(&plain, "DurationMod"), 1.0);
    let longer = calculate(FIREBALL, &["40% increased Skill Effect Duration"]);
    assert!((display_value(&longer, "DurationMod") - 1.4).abs() < 1e-9);
}

#[test]
fn ancestral_boost_reference_retains_supported_strike_targets() {
    // A spell is not a Strike.
    assert_eq!(
        display_value(&calculate(FIREBALL, &[]), "StrikeTargets"),
        0.0
    );
    // Invoker Flicker Strike (tuned build): the Strike hits one enemy and its
    // Ancestral Call support's boosted Strike targets 2 additional Enemies.
    let build = parse_build_from_code(include_str!(
        "../../../../examples/reference-builds/tuned/builds/ninja-2026-10-13-monk-invoker-flicker-strike-e3b8bfa3a005/code.txt"
    ))
    .unwrap();
    let out = calculate_with_data(&build, data(), &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(display_value(&out, "StrikeTargets"), 1.0);
    let out = calculate_with_data(
        &build,
        data(),
        &DataOrchestratorOptions::default().on_reference_basis(),
    )
    .unwrap();
    assert_eq!(display_value(&out, "StrikeTargets"), 3.0);
}

fn ancestral_snapshot(
    skill: &str,
    support: &str,
    cry: bool,
    inputs: &str,
    reference: bool,
) -> CalculationSession {
    let cry = if cry {
        r#"<Skill><Gem skillId="AncestralCryPlayer" level="20" quality="0"/></Skill>"#
    } else {
        ""
    };
    let xml = format!(
        r#"<PathOfBuilding2>
        <Build level="80" className="Warrior" mainSocketGroup="1"/>
        <Skills><Skill mainActiveSkill="1"><Gem skillId="{skill}" level="20"/>{support}</Skill>{cry}</Skills>
        <Config>{inputs}</Config></PathOfBuilding2>"#
    );
    let options = DataOrchestratorOptions {
        base_input: MinimalInput {
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 2.0,
            ..Default::default()
        },
        ..Default::default()
    };
    calculate_with_data_session(
        &parse_build(&xml).unwrap(),
        data(),
        &if reference {
            options.on_reference_basis()
        } else {
            options
        },
    )
    .unwrap()
}

const BOOSTED: &str = r#"<Input name="conditionAncestrallyBoosted" boolean="true"/>"#;
const CRY_ACTIVE: &str = r#"<Input name="conditionAncestralCryActive" boolean="true"/>"#;

#[test]
fn ancestral_boost_raw_config_edits_and_native_state_are_tracked() {
    let mut build = parse_build(
        r#"<PathOfBuilding2>
        <Build level="80" className="Warrior" mainSocketGroup="1"/>
        <Skills><Skill><Gem skillId="BoneshatterPlayer" level="20"/>
        <Gem skillId="SupportAncestralCallPlayer" level="1"/></Skill>
        <Skill><Gem skillId="AncestralCryPlayer" level="20" quality="20"/></Skill></Skills>
        <Config><Input name="conditionAncestrallyBoosted" boolean="true"/>
        <Input name="multiplierAncestralBoostElapsedSeconds" number="99"/></Config>
        </PathOfBuilding2>"#,
    )
    .unwrap();
    let options = DataOrchestratorOptions::default();
    // This is the same raw map JSON `config_inputs` writes. An edit must
    // replace the imported condition/timer rather than leave the old state.
    build.config.raw_inputs.values.insert(
        "conditionAncestrallyBoosted".into(),
        ConfigInputValue::Bool(false),
    );
    build.config.raw_inputs.values.insert(
        "conditionAncestralCryActive".into(),
        ConfigInputValue::Bool(true),
    );
    build.config.raw_inputs.values.insert(
        "multiplierAncestralBoostElapsedSeconds".into(),
        ConfigInputValue::Number(0.0),
    );
    let (off, _) = calculate_with_data_session_tracked(&build, data(), &options).unwrap();
    assert!(!off.config().condition("AncestrallyBoosted"));
    let reads = off.read_variables();
    for condition in ["AncestrallyBoosted", "AncestralCryActive"] {
        assert!(
            reads.conditions.contains(condition),
            "{condition} must be discoverable"
        );
    }
    assert!(reads.multipliers.contains("AncestralBoostElapsedSeconds"));
    build.config.raw_inputs.values.insert(
        "conditionAncestrallyBoosted".into(),
        ConfigInputValue::Bool(true),
    );
    let cry = calculate_with_data_session(&build, data(), &options).unwrap();
    assert!(cry.config().condition("AncestrallyBoosted"));
    assert_eq!(cry.base_sum("PhysicalDamageConvertToFire"), 100.0);
    assert_eq!(
        cry.mod_db()
            .more(cry.config(), &[ModName::from("FireDamage")]),
        1.0
    );
    // The fire bonus is the additional quality effect, enabled by Advanced
    // Thaumaturgy, not the ordinary duration quality on this Cry.
    build
        .tree
        .allocated_nodes
        .push(pobr_data::passive_tree::NodeId(14429));
    let extra_quality = calculate_with_data_session(&build, data(), &options).unwrap();
    assert_eq!(
        extra_quality
            .mod_db()
            .more(extra_quality.config(), &[ModName::from("FireDamage")]),
        1.1
    );
    build.tree.allocated_nodes.clear();
    build.config.raw_inputs.values.insert(
        "conditionAncestralCryActive".into(),
        ConfigInputValue::Bool(false),
    );
    let cooling = calculate_with_data_session(&build, data(), &options).unwrap();
    assert!(!cooling.config().condition("AncestrallyBoosted"));
    build.config.raw_inputs.values.insert(
        "multiplierAncestralBoostElapsedSeconds".into(),
        ConfigInputValue::Number(5.0),
    );
    assert!(
        calculate_with_data_session(&build, data(), &options)
            .unwrap()
            .config()
            .condition("AncestrallyBoosted")
    );

    // Native callers can supply the same state without an imported XML map.
    build.config.raw_inputs.values.clear();
    build
        .config
        .conditions
        .insert("AncestrallyBoosted".into(), true);
    build
        .config
        .multipliers
        .insert("AncestralBoostElapsedSeconds".into(), 4.0);
    assert!(
        !calculate_with_data_session(&build, data(), &options)
            .unwrap()
            .config()
            .condition("AncestrallyBoosted")
    );
    build
        .config
        .multipliers
        .remove("AncestralBoostElapsedSeconds");
    build.config.conditions.remove("AncestrallyBoosted");
    build
        .config
        .raw_inputs
        .values
        .insert("warcryMode".into(), ConfigInputValue::Text("MAX".into()));
    assert!(
        calculate_with_data_session(&build, data(), &options)
            .unwrap()
            .config()
            .condition("AncestrallyBoosted")
    );
}

#[test]
fn ancestral_boost_native_intervals_select_one_use_without_assuming_frequency() {
    for (skill, support, interval, cooldown_name) in [
        (
            "BoneshatterPlayer",
            "SupportAncestralCallPlayer",
            5.0,
            "AncestralCallCooldown",
        ),
        (
            "BoneshatterPlayer",
            "SupportAncestralCallPlayerTwo",
            3.0,
            "AncestralCallCooldown",
        ),
        (
            "BoneshatterPlayer",
            "SupportAncestralCallPlayerThree",
            1.0,
            "AncestralCallCooldown",
        ),
        (
            "EarthquakePlayer",
            "FistOfWarSupportPlayer",
            6.0,
            "FistOfWarCooldown",
        ),
        (
            "EarthquakePlayer",
            "FistOfWarSupportPlayerTwo",
            4.0,
            "FistOfWarCooldown",
        ),
        (
            "EarthquakePlayer",
            "FistOfWarSupportPlayerThree",
            14.0,
            "FistOfWarCooldown",
        ),
    ] {
        let gem = format!(r#"<Gem skillId="{support}" level="1"/>"#);
        let off = ancestral_snapshot(skill, &gem, false, "", false);
        assert_eq!(
            off.base_sum(cooldown_name),
            interval,
            "{support}: native milliseconds"
        );
        assert!(
            !off.config().condition("AncestrallyBoosted"),
            "{support}: no automatic uptime"
        );
        for (elapsed, expected) in [(interval - 0.001, false), (interval, true), (-1.0, false)] {
            let inputs = format!(
                r#"{BOOSTED}<Input name="multiplierAncestralBoostElapsedSeconds" number="{elapsed}"/>"#
            );
            let state = ancestral_snapshot(skill, &gem, false, &inputs, false);
            assert_eq!(
                state.config().condition("AncestrallyBoosted"),
                expected,
                "{support}: {elapsed}"
            );
            assert_eq!(
                state.output().action_rate,
                off.output().action_rate,
                "readiness does not change attack frequency"
            );
        }
        let on = ancestral_snapshot(skill, &gem, false, BOOSTED, false);
        assert!(
            on.config().condition("AncestrallyBoosted"),
            "{support}: explicit snapshot"
        );
        let disabled = gem.replace("level=", "enabled=\"false\" level=");
        assert!(
            !ancestral_snapshot(skill, &disabled, false, BOOSTED, false)
                .config()
                .condition("AncestrallyBoosted")
        );
    }
}

#[test]
fn ancestral_boost_strike_targets_do_not_multiply_damage_and_slam_effect_is_doubled_once() {
    let call = r#"<Gem skillId="SupportAncestralCallPlayer" level="1"/>"#;
    let plain = ancestral_snapshot("BoneshatterPlayer", "", false, "", false);
    let strike = ancestral_snapshot("BoneshatterPlayer", call, false, BOOSTED, false);
    assert_eq!(display_value(strike.output(), "StrikeTargets"), 3.0);
    assert_eq!(strike.output().total_hit_avg, plain.output().total_hit_avg);
    assert_eq!(strike.output().combined_dps, plain.output().combined_dps);
    let call_three = call.replace(
        "SupportAncestralCallPlayer",
        "SupportAncestralCallPlayerThree",
    );
    let third = ancestral_snapshot("BoneshatterPlayer", &call_three, false, BOOSTED, false);
    assert_eq!(display_value(third.output(), "StrikeTargets"), 3.0);
    assert!((third.output().total_hit_avg / plain.output().total_hit_avg - 0.7).abs() < 0.001);

    let plain = ancestral_snapshot("EarthquakePlayer", "", false, "", false);
    assert!(plain.output().total_hit_avg > 0.0);
    for (support, effect) in [
        ("FistOfWarSupportPlayer", 1.0),
        ("FistOfWarSupportPlayerThree", 2.0),
    ] {
        let gem = format!(r#"<Gem skillId="{support}" level="1"/>"#);
        let slam = ancestral_snapshot("EarthquakePlayer", &gem, false, BOOSTED, false);
        assert_eq!(slam.config().multiplier("AncestralBoostEffect"), effect);
        assert_eq!(display_value(slam.output(), "StrikeTargets"), 0.0);
        assert!(
            (slam.output().total_hit_avg / plain.output().total_hit_avg - (1.0 + 0.3 * effect))
                .abs()
                < 0.001
        );
        assert_eq!(
            slam.mod_db().sum(
                ModType::Inc,
                slam.config(),
                &[ModName::from("AreaOfEffect")]
            ),
            25.0 * effect
        );
        let reference = ancestral_snapshot("EarthquakePlayer", &gem, false, BOOSTED, true);
        assert_eq!(
            reference.output().total_hit_avg,
            plain.output().total_hit_avg
        );
    }
}

#[test]
fn ancestral_boost_cry_requires_an_active_source_and_never_boosts_triggered_or_proxy_attacks() {
    for skill in ["BoneshatterPlayer", "EarthquakePlayer"] {
        let plain = ancestral_snapshot(skill, "", false, "", false);
        for (cry, inputs) in [(false, BOOSTED), (false, CRY_ACTIVE), (true, "")] {
            let inactive = ancestral_snapshot(skill, "", cry, inputs, false);
            assert!(!inactive.config().condition("AncestrallyBoosted"));
            assert_eq!(
                inactive.output().total_hit_avg,
                plain.output().total_hit_avg
            );
        }
        let active = ancestral_snapshot(skill, "", true, CRY_ACTIVE, false);
        assert!(active.config().condition("AncestrallyBoosted"));
        assert_eq!(active.base_sum("PhysicalDamageConvertToFire"), 100.0);
        assert_eq!(active.config().multiplier("AncestralBoostEffect"), 1.0);
        assert!(
            active
                .output()
                .damage_components
                .iter()
                .all(|c| c.damage_type != DamageType::Physical || c.avg() == 0.0)
        );
        let reference = ancestral_snapshot(skill, "", true, CRY_ACTIVE, true);
        assert!(!reference.config().condition("AncestrallyBoosted"));
        assert_eq!(
            reference.output().total_hit_avg,
            plain.output().total_hit_avg
        );
    }
    let both = ancestral_snapshot(
        "BoneshatterPlayer",
        r#"<Gem skillId="SupportAncestralCallPlayerTwo" level="1"/>"#,
        true,
        &format!("{BOOSTED}{CRY_ACTIVE}"),
        false,
    );
    assert_eq!(
        display_value(both.output(), "StrikeTargets"),
        3.0,
        "Cry and Call grant one shared boost"
    );
    let off = ancestral_snapshot(
        "BoneshatterPlayer",
        "",
        true,
        &format!(r#"{CRY_ACTIVE}<Input name="conditionAncestrallyBoosted" boolean="false"/>"#),
        false,
    );
    assert!(!off.config().condition("AncestrallyBoosted"));
    assert_eq!(off.base_sum("PhysicalDamageConvertToFire"), 0.0);
    for skill in [
        "FireballPlayer",
        "AncestralCryShockwavePlayer",
        "ShockwaveTotemQuakePlayer",
    ] {
        let session = ancestral_snapshot(
            skill,
            r#"<Gem skillId="FistOfWarSupportPlayerThree" level="1"/>"#,
            true,
            &format!("{BOOSTED}{CRY_ACTIVE}"),
            false,
        );
        assert!(!session.config().condition("AncestrallyBoosted"), "{skill}");
        assert_eq!(session.config().multiplier("AncestralBoostEffect"), 0.0);
        assert_eq!(session.base_sum("PhysicalDamageConvertToFire"), 0.0);
    }
}
