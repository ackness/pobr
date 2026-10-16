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
