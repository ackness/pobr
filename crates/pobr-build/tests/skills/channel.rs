//! Channel cycles use the selected stat set, not an implicit perfect release.
use pobr_build::{
    Build, BuildConfig, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_core::calc::CalculationSession;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
}

fn calculate(
    data: &BuildData,
    skill: &str,
    set: u32,
    stages: Option<f64>,
    speed: bool,
) -> CalculationSession {
    let weapon = match skill {
        "SuperchargedSlamPlayer" => "Felled Greatclub",
        "PerfectStrikePlayer" => "Wooden Club",
        "GatheringStormPlayer" => "Wrapped Quarterstaff",
        _ => "Crude Bow",
    };
    let mut config = BuildConfig::new();
    if let Some(stages) = stages {
        config = config.with_multiplier("ChannelStages", stages);
    }
    let build = Build::new()
        .with_config(config)
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill_stat_set(skill, 20, set))
        .set_item(
            EquipmentSlot::Weapon1,
            Item {
                base: ItemBaseId::from(weapon),
                rarity: ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                socketed: vec![],
                parsed_stats: vec![],
            },
        );
    let options = DataOrchestratorOptions {
        extra_modifier_texts: if speed {
            vec![
                "100% increased Attack Speed".into(),
                "50% increased Action Speed".into(),
            ]
        } else {
            vec![]
        },
        ..Default::default()
    };
    calculate_with_data_session(&build, data, &options).unwrap()
}

#[test]
fn detonating_arrow_defaults_to_max_stages_and_shares_the_count_with_damage() {
    let data = data();
    let default = calculate(&data, "DetonatingArrowPlayer", 1, None, false);
    let one = calculate(&data, "DetonatingArrowPlayer", 1, Some(1.0), false);
    let bounded = calculate(&data, "DetonatingArrowPlayer", 1, Some(20.0), false);
    let cycle = default.channel().unwrap();
    assert_eq!(cycle.stages, 4.0);
    assert!(cycle.default_stages);
    assert!((cycle.cycle_time - 4.0 * cycle.attack_time).abs() < 1e-9);
    assert_eq!(default.output().dps, bounded.output().dps);
    let fire = |session: &CalculationSession| {
        session
            .output()
            .damage_components
            .iter()
            .find(|part| part.damage_type == pobr_data::prelude::DamageType::Fire)
            .unwrap()
            .avg()
    };
    assert!((fire(&default) / fire(&one) - 4.0).abs() < 1e-6);
    assert!((default.output().action_rate * 4.0 - one.output().action_rate).abs() < 1e-6);
    let formula = default.stat_formula("ActionRate").unwrap();
    for op in [
        "channel_maximum_stages",
        "channel_default_stages",
        "channel_stages",
        "channel_time",
        "channel_release",
    ] {
        assert!(
            formula.steps.iter().any(|step| step.op.as_str() == op),
            "{op}"
        );
    }
}

#[test]
fn supercharged_slam_has_immutable_attack_time_and_one_aftershock_per_stage() {
    let data = data();
    let full = calculate(&data, "SuperchargedSlamPlayer", 1, None, false);
    let fast = calculate(&data, "SuperchargedSlamPlayer", 1, None, true);
    let one = calculate(&data, "SuperchargedSlamPlayer", 1, Some(1.0), false);
    let aftershocks = calculate(&data, "SuperchargedSlamPlayer", 2, None, false);
    assert_eq!(full.output().action_rate, fast.output().action_rate);
    let cycle = full.channel().unwrap();
    assert_eq!(cycle.attack_time, 0.85);
    assert!((cycle.cycle_time - 3.55).abs() < 1e-9);
    assert!((full.output().total_hit_avg / one.output().total_hit_avg - 2.2 / 1.4).abs() < 1e-6);
    // Same per-hit damage: impact + 3 Aftershocks versus 3 Aftershocks alone.
    assert!((full.output().total_hit_avg - aftershocks.output().total_hit_avg).abs() < 1e-6);
    assert!((full.output().dps / aftershocks.output().dps - 4.0 / 3.0).abs() < 1e-6);
}

#[test]
fn perfect_timing_keeps_selected_damage_and_uses_the_documented_fallback_time() {
    let data = data();
    for (skill, start, release) in [
        ("PerfectStrikePlayer", 0.5, 0.35),
        ("GatheringStormPlayer", 0.5, 0.85),
        ("SnipePlayer", 0.16, 1.33),
    ] {
        let normal = calculate(&data, skill, 1, None, false);
        let perfect = calculate(&data, skill, 2, None, false);
        let cycle = normal.channel().unwrap();
        assert_eq!(cycle, perfect.channel().unwrap());
        assert!(
            (cycle.start_lock - start * cycle.attack_time).abs() < 1e-9,
            "{skill}"
        );
        assert!(
            (cycle.release_time - release * cycle.attack_time).abs() < 1e-9,
            "{skill}"
        );
        assert!(
            perfect.output().total_hit_avg > normal.output().total_hit_avg,
            "{skill}"
        );
        assert!((normal.output().action_rate - 1.0 / cycle.cycle_time).abs() < 1e-6);
    }
}
