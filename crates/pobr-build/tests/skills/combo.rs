//! Combo belongs to the receiving skill; its own use speed is not a gain rate.
use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, calculate_with_data_session, diagnose, parse_build,
};
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_core::{
    CalcConfig, Modifier,
    calc::{CalculationSession, MinimalInput, combo},
    display_catalog,
};
use pobr_data::prelude::ModType;
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
}

fn build(skill: &str, support: &str, config: &str, nodes: &str) -> Build {
    let support = if support.is_empty() {
        String::new()
    } else {
        format!(r#"<Gem skillId="{support}" level="1" enabled="true"/>"#)
    };
    parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Monk" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="{nodes}"/></Tree>
<Skills><Skill enabled="true" mainActiveSkill="1"><Gem skillId="{skill}" level="20" enabled="true"/>{support}</Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Wrapped Quarterstaff
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config>{config}</Config></PathOfBuilding2>"#)).unwrap()
}

fn calculate(data: &BuildData, build: &Build, lines: &[&str]) -> CalculationSession {
    calculate_with_data_session(
        build,
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: lines.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn value(session: &CalculationSession, stat: &str) -> f64 {
    display_catalog::display_value(session.output(), stat)
}

#[test]
fn combo_global_gain_is_additive_once_per_eligible_event_and_has_no_dps_effect() {
    let data = data();
    let build = build("TempestBellPlayer", "", "", "");
    let base = calculate(&data, &build, &[]);
    let with = calculate(
        &data,
        &build,
        &[
            "When you gain Combo, gain an additional Combo",
            "50% chance to build an additional Combo on Hit",
            "20% Chance to build an additional Combo on Hit",
        ],
    );
    assert!(with.unsupported_modifier_texts().is_empty());
    assert_eq!(value(&base, "ComboMaximum"), 4.0);
    assert_eq!(value(&base, "ComboGainPerEligibleEvent"), 1.0);
    assert!((value(&with, "ComboGainPerEligibleEvent") - 2.7).abs() < 1e-12);
    assert_eq!(with.output().dps, base.output().dps);
    assert_eq!(with.output().action_rate, base.output().action_rate);
    assert_eq!(with.output().total_hit_avg, base.output().total_hit_avg);
    let detail = with.derived_detail("ComboGainPerEligibleEvent").unwrap();
    let input = |id: &str| detail.inputs.iter().find(|(key, _)| key == id).unwrap().1;
    let cap = input("ComboCounterMaximum");
    let additional = input("ComboGuaranteedAdditional");
    let p = input("ComboExtraProbability");
    let rebuilt = input("ComboGainEnabled")
        * ((1.0 - p) * cap.min(1.0 + additional) + p * cap.min(2.0 + additional));
    assert_eq!(rebuilt, value(&with, "ComboGainPerEligibleEvent"));
    assert!(
        detail
            .context
            .iter()
            .any(|(key, _)| key == "ComboSameBoundWeapon")
    );
    for id in combo::STATS {
        let def = display_catalog::display_catalog()
            .into_iter()
            .find(|def| def.id.as_str() == *id)
            .unwrap();
        assert!(def.pob_key.is_none());
        assert!(pobr_core::calc::session::detail::DERIVED_STATS.contains(id));
        assert!(with.derived_detail(id).is_some());
        assert_eq!(
            pobr_build::breakdown::detail::detail_kind(id),
            pobr_build::breakdown::detail::DetailKind::Derived
        );
    }
}

#[test]
fn combo_receiving_skill_capacity_comes_from_current_game_stats_and_support_gating() {
    let data = data();
    for (skill, support, expected) in [
        ("TempestBellPlayer", "", 4.0),
        ("MantraOfDestructionPlayer", "", 8.0),
        ("TempestFlurryPlayer", "SupportComboFinisherPlayer", 3.0),
        ("TempestFlurryPlayer", "SupportComboFinisherPlayerTwo", 5.0),
        ("TempestFlurryPlayer", "SupportCulminationPlayer", 10.0),
        ("TempestFlurryPlayer", "SupportCulminationPlayerTwo", 10.0),
        ("TempestFlurryPlayer", "SupportVruunsInevitability", 10.0),
        ("FireballPlayer", "SupportCulminationPlayer", 0.0),
        ("TempestFlurryPlayer", "SupportPracticedComboPlayer", 0.0),
        ("TempestFlurryPlayer", "", 0.0),
    ] {
        let result = calculate(&data, &build(skill, support, "", ""), &[]);
        assert_eq!(
            value(&result, "ComboMaximum"),
            expected,
            "{skill} {support}"
        );
        assert_eq!(
            value(&result, "ComboGainPerEligibleEvent"),
            f64::from(expected > 0.0),
            "{skill} {support}"
        );
    }
    let passive = calculate(&data, &build("TempestBellPlayer", "", "", "41751"), &[]);
    assert_eq!(value(&passive, "ComboGainPerEligibleEvent"), 2.0);
}

#[test]
fn combo_probability_is_bounded_and_each_outcome_respects_the_counter_cap() {
    for (capacity, additional, chance, expected) in [
        (4.0, 1.0, 50.0, 2.5),
        (4.0, 1.0, 150.0, 3.0),
        (4.0, 1.0, -20.0, 2.0),
        (4.0, 3.0, 50.0, 4.0),
        (1.0, 1.0, 50.0, 1.0),
        (4.0, -1.0, 0.0, 1.0),
    ] {
        let mut session = CalculationSession::new(MinimalInput::default())
            .with_config(CalcConfig::default().with_mode_combat(true));
        session.add_modifiers([
            Modifier::number(combo::MAXIMUM, ModType::Base, capacity),
            Modifier::number(combo::ADDITIONAL, ModType::Base, additional),
            Modifier::number(combo::CHANCE, ModType::Base, chance),
        ]);
        session.perform_minimal().unwrap();
        assert_eq!(value(&session, "ComboGainPerEligibleEvent"), expected);
    }
}

#[test]
fn combo_missing_disabled_and_noncombat_receivers_do_not_generate() {
    let data = data();
    let mut disabled = build("TempestBellPlayer", "", "", "");
    disabled.socket_groups[0].enabled = false;
    let missing = Build::new();
    for build in [&missing, &disabled] {
        let result = calculate(
            &data,
            build,
            &["50% chance to build an additional Combo on Hit"],
        );
        assert_eq!(value(&result, "ComboMaximum"), 0.0);
        assert_eq!(value(&result, "ComboGainPerEligibleEvent"), 0.0);
    }
    let mut session = CalculationSession::new(MinimalInput::default())
        .with_config(CalcConfig::default().with_mode_combat(false));
    session.add_modifiers(
        combo::skill_modifiers("active_skill_required_number_of_combo_stacks", 4.0).unwrap(),
    );
    session.add_modifiers([Modifier::number(combo::ADDITIONAL, ModType::Base, 1.0)]);
    session.perform_minimal().unwrap();
    assert_eq!(value(&session, "ComboGainPerEligibleEvent"), 0.0);
}

#[test]
fn combo_finality_snapshot_and_explicit_false_retire_imported_state() {
    let data = data();
    let plain = build("TempestBellPlayer", "", "", "");
    for enabled in [false, true] {
        let mut native = plain.clone();
        native.config.conditions.insert("Finality".into(), enabled);
        let xml = build(
            "TempestBellPlayer",
            "",
            &format!(r#"<Input name="conditionFinality" boolean="{enabled}"/>"#),
            "",
        );
        // JSON requests feed the same typed raw input map as XML imports.
        let mut raw = plain.clone();
        raw.config.conditions.insert("Finality".into(), !enabled);
        raw.config
            .raw_inputs
            .values
            .insert("conditionFinality".into(), ConfigInputValue::Bool(enabled));
        for candidate in [&native, &xml, &raw] {
            let result = calculate(&data, candidate, &[]);
            assert_eq!(
                value(&result, "ComboGainPerEligibleEvent"),
                if enabled { 0.0 } else { 1.0 }
            );
            assert_eq!(result.config().condition("Finality"), enabled);
        }
    }
    for off in [
        ConfigInputValue::Text("off".into()),
        ConfigInputValue::Number(0.0),
    ] {
        let mut raw = plain.clone();
        raw.config.conditions.insert("Finality".into(), true);
        raw.config
            .raw_inputs
            .values
            .insert("conditionFinality".into(), off);
        assert_eq!(
            value(&calculate(&data, &raw, &[]), "ComboGainPerEligibleEvent"),
            1.0
        );
    }
    let mut no_catalog = data.clone();
    no_catalog.config_catalog = None;
    let mut false_edit = plain;
    false_edit.config.conditions.insert("Finality".into(), true);
    false_edit
        .config
        .raw_inputs
        .values
        .insert("conditionFinality".into(), ConfigInputValue::Bool(false));
    assert_eq!(
        value(
            &calculate(&no_catalog, &false_edit, &[]),
            "ComboGainPerEligibleEvent"
        ),
        1.0
    );
}

#[test]
fn combo_absent_rules_are_excluded_on_reference_basis_and_ledger_reads_are_truthful() {
    let data = data();
    let config = r#"<CustomModifierBlock enabled="true">50% chance to build an additional Combo on Hit</CustomModifierBlock>"#;
    for (skill, has_receiver) in [("TempestBellPlayer", true), ("TempestFlurryPlayer", false)] {
        let build = build(skill, "", config, "");
        let diagnosis = diagnose(&build, &data, &DataOrchestratorOptions::default()).unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        assert_eq!(usage.reads.contains_key(combo::CHANCE), has_receiver);
        assert_eq!(usage.condition_reads.contains("Finality"), has_receiver);
        let reference = calculate_with_data_session(
            &build,
            &data,
            &DataOrchestratorOptions::default().on_reference_basis(),
        )
        .unwrap();
        assert_eq!(value(&reference, "ComboMaximum"), 0.0);
        assert_eq!(value(&reference, "ComboGainPerEligibleEvent"), 0.0);
    }
    let build = build("TempestFlurryPlayer", "SupportCulminationPlayer", "", "");
    let reference = calculate_with_data_session(
        &build,
        &data,
        &DataOrchestratorOptions::default().on_reference_basis(),
    )
    .unwrap();
    assert_eq!(
        reference.mod_db().sum(
            ModType::Base,
            reference.config(),
            &["Multiplier:ComboStacksMax".into()]
        ),
        10.0
    );
}

#[test]
fn combo_explicit_maximum_is_not_added_to_requirement_and_proxies_are_inactive() {
    use pobr_data::prelude::SkillTypes;
    for (types, selected_finality, expected) in [
        (SkillTypes::NONE, None, 0.0),
        (SkillTypes::NONE, Some(false), 1.0),
        (SkillTypes::NONE, Some(true), 0.0),
        (SkillTypes::SUMMONS_TOTEM, Some(false), 0.0),
        (SkillTypes::MINION, Some(false), 0.0),
        (SkillTypes::TRIGGERED, Some(false), 0.0),
    ] {
        let mut cfg = CalcConfig::default()
            .with_mode_combat(true)
            .with_skill_types(types);
        if let Some(enabled) = selected_finality {
            cfg = cfg.with_condition("Finality", enabled);
        }
        let mut session = CalculationSession::new(MinimalInput::default()).with_config(cfg);
        for (stat, count) in [
            ("active_skill_required_number_of_combo_stacks", 4.0),
            ("skill_maximum_number_of_combo_stacks", 10.0),
        ] {
            session.add_modifiers(combo::skill_modifiers(stat, count).unwrap());
        }
        session.add_modifiers([Modifier::flag("Condition:Finality")]);
        session.perform_minimal().unwrap();
        assert_eq!(value(&session, "ComboMaximum"), 10.0);
        assert_eq!(value(&session, "ComboGainPerEligibleEvent"), expected);
    }
}
