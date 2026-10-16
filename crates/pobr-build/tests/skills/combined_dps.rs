//! Combined damage remains damage per second for skills whose game tooltip
//! uses average damage (`base_skill_show_average_damage_instead_of_dps`).

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, current_data_dir};

fn calculate(data: &BuildData, skill: &str, lines: &[&str]) -> OutputTable {
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20));
    calculate_with_data(
        &build,
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: lines.iter().map(|line| (*line).into()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn detonate_living_innate_culling_contributes_to_combined_dps() {
    let data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    let innate = calculate(&data, "DetonateLivingPlayer", &[]);
    let explicit = calculate(&data, "DetonateLivingPlayer", &["Culling Strike"]);
    // spells_have_culling_strike: "Your Spells have Culling Strike".
    assert!(innate.culling_dps > 0.0);
    assert!((innate.combined_dps - explicit.combined_dps).abs() < 1e-6);
    assert!((innate.combined_dps - innate.dps - innate.culling_dps).abs() < 1e-6);
}

#[test]
fn average_damage_tooltip_does_not_remove_rate_or_damage_over_time() {
    let data = BuildData::load(&GameData::new(current_data_dir())).unwrap();
    for skill in [
        "GrimPillarsPlayer",
        "LightningWarpPlayer",
        "VileDisruptionPlayer",
        "SkyfallPlayer",
        "IncineratePlayer",
        "LivingBombPlayer",
        "VolcanoPlayer",
        "RakePlayer",
    ] {
        let output = calculate(&data, skill, &[]);
        assert!(output.dps > 0.0, "{skill}");
        assert!(
            (output.combined_dps - output.dps - output.total_dot_dps).abs() < 1e-6,
            "{skill}: {output:?}"
        );
        if [
            "IncineratePlayer",
            "LivingBombPlayer",
            "VolcanoPlayer",
            "RakePlayer",
        ]
        .contains(&skill)
        {
            assert!(output.total_dot_dps > 0.0, "{skill}");
        }
    }
}
