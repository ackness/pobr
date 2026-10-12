//! Exact active-effect selection and independent per-skill calculation contracts.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_skill_metrics, calculate_with_data, parse_build_from_code,
    resolve_main_skill_selection,
};
use pobr_gamedata::{GameData, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(pobr_gamedata::data_version())))
        .expect("load current game data")
}

fn build() -> Build {
    Build::new().with_character(CharacterIdentity {
        level: 90,
        class_name: "Witch".into(),
        ascendancy_name: String::new(),
    })
}

#[test]
fn blood_mage_selects_support_granted_doom_blast() {
    let data = data();
    let build = parse_build_from_code(include_str!(
        "../../../../examples/wasm-benchmark/builds/ninja-2026-10-11-witch-blood-mage-impending-doom-add19135df93/code.txt"
    )).unwrap();
    assert_eq!(build.main_socket_group, Some(5));
    assert_eq!(build.socket_groups[4].main_active_skill, Some(2));
    assert_eq!(
        resolve_main_skill_selection(&build, &data),
        Some((4, 2, "DoomBlastPlayer".into())),
    );
    let options = DataOrchestratorOptions::default();
    let panel = calculate_with_data(&build, &data, &options).unwrap();
    assert!(panel.total_hit_avg > 0.0, "Doom Blast must have a damage payload");
    let report = calculate_skill_metrics(&build, &data, &options).unwrap();
    let doom = report.skills.iter()
        .find(|row| row.group_index == 4 && row.active_skill_index == 2).unwrap();
    assert_eq!(doom.skill_id, "DoomBlastPlayer");
    assert!(doom.error.is_none(), "{:?}", doom.error);
    assert_eq!(doom.output.as_ref().unwrap().total_hit_avg, panel.total_hit_avg);
    assert_eq!(doom.output.as_ref().unwrap().combined_dps, panel.combined_dps);
    assert!(report.skills.iter().any(|row|
        row.group_index == 4 && row.active_skill_index == 1
            && row.skill_id == "ElementalWeaknessPlayer"
    ));
}

#[test]
fn every_gem_instance_calculates_without_a_main_selection() {
    let data = data();
    let build = build().add_socket_group(
        SocketGroup::new()
            .with_gem_skill_quality("FireballPlayer", 12, 0)
            .with_gem_skill_quality("FireballPlayer", 20, 20),
    );
    assert_eq!(build.main_socket_group, None);
    let options = DataOrchestratorOptions::default();
    let report = calculate_skill_metrics(&build, &data, &options).unwrap();
    assert_eq!(report.skills.len(), 2);
    for (index, row) in report.skills.iter().enumerate() {
        assert_eq!(row.group_index, 0);
        assert_eq!(row.active_skill_index, index + 1);
        let mut selected = build.clone().with_main_socket_group(1);
        selected.socket_groups[0].main_active_skill = Some(index + 1);
        let single = calculate_with_data(&selected, &data, &options).unwrap();
        assert_eq!(row.output.as_ref().unwrap().total_hit_avg, single.total_hit_avg);
        assert_eq!(row.output.as_ref().unwrap().combined_dps, single.combined_dps);
    }
    assert!(report.skills[1].output.as_ref().unwrap().total_hit_avg
        > report.skills[0].output.as_ref().unwrap().total_hit_avg);
}

#[test]
fn duplicate_effect_instances_keep_the_selected_stat_set_mapping() {
    let data = data();
    let duplicate = build().add_socket_group(
        SocketGroup::new()
            .with_gem_skill_stat_set("EarthquakePlayer", 20, 1)
            .with_gem_skill_stat_set("EarthquakePlayer", 20, 2),
    );
    let single = build().add_socket_group(
        SocketGroup::new().with_gem_skill_stat_set("EarthquakePlayer", 20, 2),
    ).with_main_socket_group(1);
    let options = DataOrchestratorOptions {
        base_input: pobr_core::calc::MinimalInput {
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let report = calculate_skill_metrics(&duplicate, &data, &options).unwrap();
    assert_eq!(report.skills.len(), 2);
    let aftershock = report.skills.iter()
        .find(|row| row.group_index == 0 && row.active_skill_index == 2).unwrap();
    let expected = calculate_with_data(&single, &data, &options).unwrap();
    assert!(expected.total_hit_avg > 0.0);
    let actual = aftershock.output.as_ref().unwrap();
    assert_eq!(actual.damage_components, expected.damage_components);
    assert_eq!(actual.total_hit_avg, expected.total_hit_avg);
    assert_eq!(actual.combined_dps, expected.combined_dps);
}

#[test]
fn explicit_utility_and_zero_output_are_not_replaced_or_dropped() {
    let data = data();
    let build = build().add_socket_group(
        SocketGroup::new()
            .with_gem_skill("FireballPlayer", 20)
            .with_gem_skill("ElementalWeaknessPlayer", 20)
            .with_main_active_skill(2),
    ).with_main_socket_group(1);
    assert_eq!(resolve_main_skill_selection(&build, &data),
        Some((0, 2, "ElementalWeaknessPlayer".into())));
    let report = calculate_skill_metrics(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(report.skills.len(), 2);
    let utility = &report.skills[1];
    assert!(utility.error.is_none());
    assert_eq!(utility.output.as_ref().unwrap().combined_dps, 0.0);
    assert!(utility.output.as_ref().unwrap().action_rate > 0.0);
    assert!(report.skills[0].output.as_ref().unwrap().combined_dps > 0.0);
}

#[test]
fn unknown_effect_and_invalid_ordinals_never_borrow_another_skill() {
    let data = data();
    let mut build = build()
        .add_socket_group(SocketGroup::new().with_gem_skill("MissingEffectPlayer", 20))
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20))
        .with_main_socket_group(1);
    assert_eq!(resolve_main_skill_selection(&build, &data), None);
    let options = DataOrchestratorOptions::default();
    let report = calculate_skill_metrics(&build, &data, &options).unwrap();
    assert_eq!(report.skills.len(), 2);
    let missing = &report.skills[0];
    assert_eq!(missing.skill_id, "MissingEffectPlayer");
    assert!(missing.output.is_none());
    assert!(missing.error.as_ref().unwrap().contains("MissingEffectPlayer"));
    assert!(report.skills[1].output.as_ref().unwrap().combined_dps > 0.0);
    build.main_socket_group = Some(2);
    for ordinal in [0, 2, usize::MAX] {
        build.socket_groups[1].main_active_skill = Some(ordinal);
        assert_eq!(resolve_main_skill_selection(&build, &data), None);
    }
    build.main_socket_group = Some(999);
    assert_eq!(resolve_main_skill_selection(&build, &data), None);
}

#[test]
fn unavailable_level_data_does_not_abort_sibling_rows_or_renumber_groups() {
    let mut data = data();
    data.granted_effect_levels.remove("ElementalWeaknessPlayer");
    let build = build()
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20).with_enabled(false))
        .add_socket_group(SocketGroup::new()
            .with_gem_skill("ElementalWeaknessPlayer", 20)
            .with_gem_skill("FireballPlayer", 20));
    let report = calculate_skill_metrics(&build, &data, &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(report.skills.len(), 2);
    assert!(report.skills.iter().all(|row| row.group_index == 1));
    assert!(report.skills[0].output.is_none());
    assert!(report.skills[0].error.is_some());
    assert_eq!(report.skills[1].active_skill_index, 2);
    assert!(report.skills[1].output.as_ref().unwrap().combined_dps > 0.0);
}

#[test]
fn complete_effect_ordinals_preserve_provenance_and_skip_support_effects() {
    let mut data = data();
    data.gem_effects.get_mut("ViciousHexSupportPlayer").unwrap()
        .additional_granted_effect_ids.insert(0, "SupportFireMasteryPlayer".into());
    let gems = [
        ("", 1, 0, None),
        ("ElementalWeaknessPlayer", 20, 5, Some(1)),
        ("ViciousHexSupportPlayer", 2, 19, Some(2)),
        ("MissingEffectPlayer", 7, 3, None),
    ];
    let effects: Vec<_> = pobr_core::skill_env::group_active_effects(gems.into_iter(), &data)
        .collect();
    assert_eq!(effects.len(), 3);
    assert_eq!(effects[0].skill_id, "ElementalWeaknessPlayer");
    assert_eq!(effects[0].active_skill_index, 1);
    let secondary = &effects[1];
    assert_eq!(secondary.skill_id, "DoomBlastPlayer");
    assert_eq!(secondary.active_skill_index, 2);
    assert_eq!(secondary.gem_index, 2);
    assert_eq!(secondary.gem_level, 2);
    assert_eq!(secondary.quality, 19);
    assert_eq!(secondary.stat_set_index, Some(2));
    assert_eq!(effects[2].skill_id, "MissingEffectPlayer");
    assert_eq!(effects[2].active_skill_index, 3);
}
