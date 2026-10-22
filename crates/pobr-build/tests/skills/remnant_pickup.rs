//! Remnant collector recovery does not inherit creator-owned duplicate effects.
use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build,
};
use pobr_core::calc::{
    CalculationSession, CompletedCalculationSession, MinimalInput,
    remnant_pickup::{CONFIG_INPUTS, STATS},
};
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;

use crate::test_data as data;
fn build(skill: &str, nodes: &str, config: &str) -> Build {
    parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Sorceress" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="{nodes}" treeVersion="0_5"/></Tree>
<Skills><Skill mainActiveSkill="1"><Gem skillId="{skill}" level="20"/></Skill></Skills>
<Config>{config}</Config></PathOfBuilding2>"#
    ))
    .unwrap()
}
fn run(b: &Build, d: &BuildData) -> CompletedCalculationSession {
    calculate_with_data_session(b, d, &DataOrchestratorOptions::default()).unwrap()
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() < 1e-7 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    );
}
fn value(s: &CalculationSession, id: &str) -> f64 {
    display_value(s.output(), id)
}
fn native(cfg: CalcConfig, mods: Vec<Modifier>) -> CalculationSession {
    let mut s = CalculationSession::new(MinimalInput {
        base_mana: 1000.0,
        base_life: 1000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        ..Default::default()
    })
    .with_config(cfg);
    s.add_modifiers(mods);
    s.track_modifier_reads();
    s.perform_minimal().unwrap();
    s
}
fn sources() -> Vec<Modifier> {
    vec![
        Modifier::number("RemnantManaRecoveryPercent", ModType::Base, 3.0),
        Modifier::number("CreatedRemnantDuplicateChance", ModType::Base, 15.0),
    ]
}

#[test]
fn remnant_pickup_current_passives_separate_collector_recovery_and_creator_effects() {
    let d = data();
    let input = r#"<Input name="remnantPickupsPerSecond" number="2"/><Input name="createdRemnantPickupsPerSecond" number="4"/>"#;
    let arcane = run(&build("FireballPlayer", "46124", input), &d);
    let both = run(&build("FireballPlayer", "46124,40985", input), &d);
    assert!(
        both.unsupported_modifier_texts().is_empty(),
        "{:?}",
        both.unsupported_modifier_texts()
    );
    close(value(&both, "RemnantManaRecoveryPercent"), 3.0);
    close(value(&both, "CreatedRemnantDuplicateChance"), 15.0);
    close(
        value(&both, "RemnantManaRecoverablePerPickup"),
        both.output().mana * 0.03,
    );
    close(
        value(&both, "RemnantManaRecoverablePerSecond"),
        both.output().mana * 0.06,
    );
    close(
        value(&both, "RemnantManaRecoverablePerSecond"),
        value(&arcane, "RemnantManaRecoverablePerSecond"),
    );
    close(value(&both, "CreatedRemnantEffectGrantsPerPickup"), 1.15);
    close(value(&both, "CreatedRemnantEffectGrantsPerSecond"), 4.6);
    close(
        value(&both, "CreatedRemnantPotentialEffectGrantsPerUse"),
        0.0,
    ); // Fireball creates none.
    close(both.output().dps, arcane.output().dps);
    close(both.output().mana_regen, arcane.output().mana_regen);
    for id in STATS {
        let detail = pobr_build::breakdown::detail::stat_detail(&both, id).unwrap();
        if id.ends_with("Percent") || id.ends_with("Chance") {
            assert!(
                detail
                    .sources
                    .iter()
                    .any(|s| s.base_total > 0.0 && !s.mods.is_empty()),
                "{id}"
            );
        } else {
            assert!(detail.derived.is_some(), "{id}");
        }
        assert!(
            pobr_core::display_catalog()
                .iter()
                .find(|d| d.id.as_str() == *id)
                .unwrap()
                .pob_key
                .is_none()
        );
    }
    let reference = calculate_with_data_session(
        &build("FireballPlayer", "46124,40985", input),
        &d,
        &DataOrchestratorOptions::default().on_reference_basis(),
    )
    .unwrap();
    for id in STATS {
        close(value(&reference, id), 0.0);
    }
}

#[test]
fn remnant_pickup_rates_require_explicit_frequency_and_recovery_respects_missing_mana() {
    let cfg = CalcConfig::spell().with_mode_combat(true);
    let no_rates = native(cfg.clone(), sources());
    for (_, key) in CONFIG_INPUTS {
        assert!(no_rates.read_variables().multipliers.contains(*key));
    }
    close(value(&no_rates, "RemnantManaRecoverablePerPickup"), 30.0);
    close(value(&no_rates, "RemnantManaRecoverablePerSecond"), 0.0);
    close(value(&no_rates, "CreatedRemnantEffectGrantsPerSecond"), 0.0);
    for (missing, expected) in [(0.0, 0.0), (5.0, 5.0), (100.0, 30.0)] {
        let s = native(
            cfg.clone()
                .with_multiplier("RemnantPickupMissingMana", missing)
                .with_multiplier("RemnantPickupsPerSecond", 2.0),
            sources(),
        );
        close(value(&s, "RemnantManaRecoverablePerPickup"), expected);
        close(value(&s, "RemnantManaRecoverablePerSecond"), expected * 2.0);
    }
    for (blocker, expected) in [("CannotGainMana", 0.0), ("NoManaRegen", 30.0)] {
        let mut mods = sources();
        mods.push(Modifier::flag(blocker));
        let s = native(
            cfg.clone().with_multiplier("RemnantPickupsPerSecond", 2.0),
            mods,
        );
        close(value(&s, "RemnantManaRecoverablePerPickup"), expected);
        close(value(&s, "RemnantManaRecoverablePerSecond"), expected * 2.0);
        let detail = s.derived_detail("RemnantManaRecoverablePerPickup").unwrap();
        close(
            detail
                .inputs
                .iter()
                .find(|(k, _)| k == "RemnantCanGainMana")
                .unwrap()
                .1,
            f64::from(expected > 0.0),
        );
    }
    let full = native(
        cfg.clone()
            .with_condition("FullMana", true)
            .with_multiplier("RemnantPickupMissingMana", 100.0),
        sources(),
    );
    close(value(&full, "RemnantManaRecoverablePerPickup"), 0.0);
    let mut mods = sources();
    mods.push(Modifier::flag("Condition:FullMana"));
    close(
        value(
            &native(cfg.clone(), mods),
            "RemnantManaRecoverablePerPickup",
        ),
        0.0,
    );
    let own_only = native(
        cfg.clone()
            .with_multiplier("CreatedRemnantPickupsPerSecond", 4.0),
        sources(),
    );
    close(value(&own_only, "RemnantManaRecoverablePerSecond"), 0.0);
    close(value(&own_only, "CreatedRemnantEffectGrantsPerSecond"), 4.6);
    let other_creator = native(
        cfg.with_multiplier("RemnantPickupsPerSecond", 2.0),
        sources(),
    );
    close(
        value(&other_creator, "CreatedRemnantEffectGrantsPerSecond"),
        0.0,
    );
}

#[test]
fn remnant_pickup_reuses_generation_without_collecting_or_changing_configured_flame_stacks() {
    let remnants = SkillTypes::from_pob2_name("GeneratesRemnants").unwrap();
    let cfg = CalcConfig::spell()
        .with_mode_combat(true)
        .with_skill_types(remnants);
    let mut mods = sources();
    mods.push(Modifier::number(
        "AdditionalRemnantChance",
        ModType::Base,
        156.0,
    ));
    let s = native(cfg.clone(), mods);
    close(value(&s, "RemnantsPerUse"), 2.56);
    close(
        value(&s, "CreatedRemnantPotentialEffectGrantsPerUse"),
        2.56 * 1.15,
    );
    close(value(&s, "RemnantManaRecoverablePerSecond"), 0.0);
    close(value(&s, "CreatedRemnantEffectGrantsPerSecond"), 0.0);
    let capped = native(
        cfg,
        vec![Modifier::number(
            "CreatedRemnantDuplicateChance",
            ModType::Base,
            200.0,
        )],
    );
    close(value(&capped, "CreatedRemnantDuplicateChance"), 100.0);
    close(value(&capped, "CreatedRemnantEffectGrantsPerPickup"), 2.0);
    let d = data();
    let input = r#"<Input name="purpleFlameStacks" number="4"/>"#;
    let baseline = run(&build("IntoTheBreachPlayer", "", input), &d);
    let modified = run(&build("IntoTheBreachPlayer", "40985", input), &d);
    close(modified.output().dps, baseline.output().dps);
    assert_eq!(
        modified.config().multipliers.get("PurpleFlamesCount"),
        baseline.config().multipliers.get("PurpleFlamesCount")
    );
}

#[test]
fn remnant_pickup_xml_json_native_off_values_retire_imported_inputs() {
    let d = data();
    let xml = build(
        "FireballPlayer",
        "46124,40985",
        r#"<Input name="remnantPickupsPerSecond" number="2"/><Input name="createdRemnantPickupsPerSecond" number="3"/><Input name="remnantPickupMissingMana" number="5"/>"#,
    );
    let expected = run(&xml, &d);
    let mut b = build("FireballPlayer", "46124,40985", "");
    for ((_, name), n) in CONFIG_INPUTS.iter().zip([2.0, 3.0, 5.0]) {
        b.config.multipliers.insert((*name).into(), n);
    }
    let native = run(&b, &d);
    for id in STATS {
        close(value(&native, id), value(&expected, id));
    }
    for raw in [
        ConfigInputValue::Number(0.0),
        ConfigInputValue::Bool(false),
        ConfigInputValue::Text("0".into()),
        ConfigInputValue::Text("invalid".into()),
        ConfigInputValue::Number(-1.0),
    ] {
        let mut off = b.clone();
        for (input, _) in CONFIG_INPUTS {
            off.config
                .raw_inputs
                .values
                .insert((*input).into(), raw.clone());
        }
        let s = run(&off, &d);
        close(value(&s, "RemnantManaRecoverablePerPickup"), 0.0);
        close(value(&s, "RemnantManaRecoverablePerSecond"), 0.0);
        close(value(&s, "CreatedRemnantEffectGrantsPerSecond"), 0.0);
    }
    let xml_off = run(
        &build(
            "FireballPlayer",
            "46124,40985",
            r#"<Input name="remnantPickupsPerSecond" boolean="false"/><Input name="createdRemnantPickupsPerSecond" number="0"/><Input name="remnantPickupMissingMana" string="0"/>"#,
        ),
        &d,
    );
    close(value(&xml_off, "RemnantManaRecoverablePerPickup"), 0.0);
    close(value(&xml_off, "RemnantManaRecoverablePerSecond"), 0.0);
    close(value(&xml_off, "CreatedRemnantEffectGrantsPerSecond"), 0.0);
    b.config.conditions.insert("FullMana".into(), true);
    close(value(&run(&b, &d), "RemnantManaRecoverablePerPickup"), 0.0);
    b.config
        .raw_inputs
        .values
        .insert("conditionFullMana".into(), ConfigInputValue::Bool(false));
    close(value(&run(&b, &d), "RemnantManaRecoverablePerPickup"), 5.0);
}

#[test]
fn remnant_pickup_no_source_noncombat_and_details_keep_event_scope() {
    let cfg = CalcConfig::spell()
        .with_mode_combat(true)
        .with_multiplier("RemnantPickupsPerSecond", 2.0)
        .with_multiplier("CreatedRemnantPickupsPerSecond", 3.0);
    let no_source = native(
        cfg.clone(),
        vec![Modifier::number("RemnantEffect", ModType::Inc, 100.0)],
    );
    for id in STATS {
        close(value(&no_source, id), 0.0);
    }
    assert!(
        !no_source
            .read_modifier_names()
            .iter()
            .any(|name| name.as_str() == "RemnantEffect")
    );
    for (_, key) in CONFIG_INPUTS {
        assert!(no_source.read_variables().multipliers.contains(*key));
    }
    let off = native(cfg.with_mode_combat(false), sources());
    for id in &STATS[1..] {
        if *id != "CreatedRemnantDuplicateChance" {
            close(value(&off, id), 0.0);
        }
    }
    let d = data();
    let baseline = run(&build("SkeletalWarriorPlayer", "", ""), &d);
    let modified = run(&build("SkeletalWarriorPlayer", "46124,40985", ""), &d);
    assert_eq!(baseline.output().minions, modified.output().minions);
    let detail = modified
        .derived_detail("RemnantManaRecoverablePerPickup")
        .unwrap();
    let input = |id: &str| detail.inputs.iter().find(|(key, _)| key == id).unwrap().1;
    close(
        value(&modified, "RemnantManaRecoverablePerPickup"),
        input("RemnantCombatEnabled")
            * input("RemnantCanGainMana")
            * (input("MaximumMana") * input("RemnantManaRecoveryPercent") / 100.0)
                .min(input("RemnantMissingManaCap")),
    );
    close(input("RemnantMissingManaSpecified"), 0.0);
}

#[test]
fn remnant_pickup_reservation_and_finite_rate_boundaries() {
    let cfg = CalcConfig::spell()
        .with_mode_combat(true)
        .with_multiplier("RemnantPickupMissingMana", 1000.0);
    for (reserved, expected) in [(990.0, 10.0), (1000.0, 0.0)] {
        let mut mods = sources();
        mods.push(Modifier::number("ManaReserved", ModType::Base, reserved));
        let s = native(cfg.clone(), mods);
        close(s.output().mana, 1000.0);
        close(value(&s, "RemnantManaRecoverablePerPickup"), expected);
    }
    let s = native(
        cfg.with_multiplier("RemnantPickupsPerSecond", f64::MAX)
            .with_multiplier("CreatedRemnantPickupsPerSecond", f64::MAX),
        sources(),
    );
    close(value(&s, "RemnantManaRecoverablePerSecond"), f64::MAX);
    close(value(&s, "CreatedRemnantEffectGrantsPerSecond"), f64::MAX);
    for id in [
        "RemnantManaRecoverablePerSecond",
        "CreatedRemnantEffectGrantsPerSecond",
    ] {
        let d = s.derived_detail(id).unwrap();
        close(
            d.inputs
                .iter()
                .find(|(key, _)| key == "RemnantFiniteLimit")
                .unwrap()
                .1,
            f64::MAX,
        );
        assert!(d.formula.starts_with("min("));
    }
}
