use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn calculate(data: &BuildData, inputs: &str, extra: &[&str], reference: bool) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" quality="0" enabled="true"/></Skill></Skills>
<Config>{inputs}</Config></PathOfBuilding2>"#
    );
    let build = parse_build(&xml).unwrap();
    let mut options = DataOrchestratorOptions {
        extra_modifier_texts: extra.iter().map(|text| text.to_string()).collect(),
        ..Default::default()
    };
    if reference {
        options = options.on_reference_basis();
    }
    calculate_with_data(&build, data, &options).unwrap()
}

#[test]
fn configured_player_shock_reads_its_effect_reduction_in_game_mode() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap();
    let inputs = r#"<Input name="conditionShocked" boolean="true"/>"#;
    let reduced = ["40% reduced effect of Shock on you"];
    // Retain the imported config's existing 15% magnitude: 15 * .6 = 9%.
    // Stormwalker's game stat is a 40% reduction, not 40 percentage points.
    for (extra, reference, expected) in [
        (&[][..], false, 1.15),
        (&reduced[..], false, 1.09),
        (&reduced[..], true, 1.15),
    ] {
        let output = calculate(&data, inputs, extra, reference);
        assert!((output.taken_multi_physical - expected).abs() < 1e-9);
    }
}

#[test]
fn configured_chill_reaches_action_speed_only_when_active_and_supplied() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap();
    let reduction = ["40% reduced Effect of Chill on you"];
    let plain = calculate(&data, "", &reduction, false);
    for (state, speed) in [(true, 0.7), (false, 1.0)] {
        let inputs = format!(
            r#"<Input name="conditionChilled" boolean="{state}"/>
<Input name="conditionChilledEffect" number="50"/>"#
        );
        let output = calculate(&data, &inputs, &reduction, false);
        assert!((output.action_rate / plain.action_rate - speed).abs() < 1e-5);
        assert_eq!(output.effective_movement_speed_mod, speed);
    }
    let unspecified = calculate(
        &data,
        r#"<Input name="conditionChilled" boolean="true"/>"#,
        &reduction,
        false,
    );
    assert_eq!(unspecified.action_rate, plain.action_rate);
}

#[test]
fn self_duration_support_buffs_reach_the_display_views() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap();
    let xml = r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" quality="0" enabled="true"/></Skill>
<Skill enabled="true"><Gem skillId="ArcticArmourPlayer" level="20" quality="0" enabled="true"/>
<Gem skillId="SupportCoolheadedPlayer" level="1" quality="0" enabled="true"/>
<Gem skillId="SupportWarmbloodedPlayer" level="1" quality="0" enabled="true"/>
<Gem skillId="SupportStrongHeartedPlayer" level="1" quality="0" enabled="true"/>
</Skill></Skills></PathOfBuilding2>"#;
    let build = parse_build(xml).unwrap();
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec!["20% reduced Duration of Ailments on You".into()],
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &options).unwrap();
    // Game granted_effect_stat_sets: each support_*_protection_spirit_cost_*
    // duration_on_self_+%_final is -50: (100 - 20) * 0.5 = 40% of base.
    for id in [
        "SelfIgniteDuration",
        "SelfFreezeDuration",
        "SelfShockDuration",
    ] {
        assert_eq!(pobr_core::display_value(&out, id), 40.0, "{id}");
    }
    for id in [
        "SelfBleedDuration",
        "SelfPoisonDuration",
        "SelfChillDuration",
    ] {
        assert_eq!(pobr_core::display_value(&out, id), 80.0, "{id}");
    }
}

#[test]
fn freeze_poison_and_blind_self_views_read_their_game_stats() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap();
    let plain = calculate(&data, "", &[], false);
    for id in ["SelfFreezeEffect", "SelfPoisonEffect", "SelfBlindDuration"] {
        assert_eq!(pobr_core::display_value(&plain, id), 100.0, "{id}");
    }
    // non_damaging_ailment_effect_+%_on_self: 100 - 35 = 65% Freeze effect.
    // unaffected_by_damaging_ailments: Poison magnitude on you is 0.
    // debuff_time_passed_+% 10: Blind is a debuff, 100 / 1.1 of its duration.
    let out = calculate(
        &data,
        "",
        &[
            "35% reduced Effect of Non-Damaging Ailments on you",
            "Unaffected by Damaging Ailments",
            "Debuffs on you expire 10% faster",
        ],
        false,
    );
    assert_eq!(pobr_core::display_value(&out, "SelfFreezeEffect"), 65.0);
    assert_eq!(pobr_core::display_value(&out, "SelfPoisonEffect"), 0.0);
    assert!((pobr_core::display_value(&out, "SelfBlindDuration") - 100.0 / 1.1).abs() < 1e-9);
}

#[test]
fn inflicted_shock_and_chill_durations_reconstruct_from_their_detail() {
    use pobr_build::diagnose;
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap();
    for (skill, stat, line) in [
        (
            "SparkPlayer",
            "ShockDuration",
            "20% increased Shock Duration",
        ),
        (
            "FrostboltPlayer",
            "ChillDuration",
            "20% increased Chill Duration",
        ),
    ] {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" quality="0" enabled="true"/></Skill></Skills>
</PathOfBuilding2>"#
        ))
        .unwrap();
        let mut options = DataOrchestratorOptions::against_reference_enemy();
        let plain = diagnose(&build, &data, &options).unwrap();
        // Game keyword: Shock and Chill last 8 seconds on non-players.
        assert_eq!(
            pobr_core::display_value(plain.session.output(), stat),
            8.0,
            "{stat}"
        );
        options.extra_modifier_texts = vec![line.into()];
        let result = diagnose(&build, &data, &options).unwrap();
        assert!(result.session.unsupported_modifier_texts().is_empty());
        let value = pobr_core::display_value(result.session.output(), stat);
        // 8 * (1 + 20 / 100) = 9.6, before any enemy debuff expiration.
        let detail = result.session.derived_detail(stat).unwrap();
        let input = |id| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
        assert_eq!(input("BaseAilmentDuration"), 8.0);
        assert_eq!(input("AilmentDurationIncreased"), 20.0);
        let rebuilt = input("BaseAilmentDuration")
            * (1.0 + input("AilmentDurationIncreased") / 100.0)
            * input("AilmentDurationMore")
            * input("EnemyDebuffDuration");
        assert!(
            (rebuilt - value).abs() < 1e-9,
            "{stat}: {rebuilt} vs {value}"
        );
        assert!((value - 9.6 * input("EnemyDebuffDuration")).abs() < 1e-9);
    }
}
