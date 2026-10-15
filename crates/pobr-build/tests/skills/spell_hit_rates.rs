//! Repeated spell hits come from the game's timing stats, not cast speed.
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_gamedata::{GameData, data_version, repo_data_root};
use std::sync::OnceLock;

fn data() -> &'static BuildData {
    static DATA: OnceLock<BuildData> = OnceLock::new();
    DATA.get_or_init(|| {
        BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
    })
}

fn build(skill: &str) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_active_skill(skill, 20))
}

#[test]
fn periodic_skills_use_their_own_hit_intervals() {
    for (skill, rate) in [
        ("BallLightningPlayer", 1000.0 / 200.0),
        ("FirestormPlayer", 1000.0 / 200.0),
        ("IcestormPlayer", 1000.0 / 150.0),
        ("ThunderstormPlayer", 1000.0 / 500.0),
        ("SolarOrbPlayer", 1000.0 / 1400.0),
    ] {
        for cast_speed in [false, true] {
            let options = DataOrchestratorOptions {
                extra_modifier_texts: if cast_speed {
                    vec!["100% increased Cast Speed".into()]
                } else {
                    vec![]
                },
                ..Default::default()
            };
            let session = calculate_with_data_session(&build(skill), data(), &options).unwrap();
            assert!(
                (session.output().action_rate - rate).abs() < 1e-6,
                "{skill}: {} != {rate}",
                session.output().action_rate
            );
            let formula = session.stat_formula("ActionRate").unwrap();
            assert_eq!(formula.value, session.output().action_rate);
        }
    }
}

#[test]
fn solar_orb_duration_limits_uptime_only_on_the_game_basis() {
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec!["99% reduced Skill Effect Duration".into()],
        ..Default::default()
    };
    let game = calculate_with_data_session(&build("SolarOrbPlayer"), data(), &options).unwrap();
    // 10 s * 1% duration, cast every 0.8 s: one eighth uptime.
    assert!(
        (game.output().action_rate - 0.125 / 1.4).abs() < 1e-6,
        "{}",
        game.output().action_rate
    );
    let reference = calculate_with_data_session(
        &build("SolarOrbPlayer"),
        data(),
        &options.on_reference_basis(),
    )
    .unwrap();
    assert!((reference.output().action_rate - 1.0 / 1.4).abs() < 1e-6);
}

#[test]
fn flameblast_and_snap_respect_cooldowns_and_bonestorm_stays_unchanged() {
    for (skill, rate) in [
        ("FlameblastPlayer", 1.0 / 10.032),
        ("SnapPlayer", 1.0 / 4.026),
        ("BonestormPlayer", 8.0),
    ] {
        let session =
            calculate_with_data_session(&build(skill), data(), &DataOrchestratorOptions::default())
                .unwrap();
        assert!(
            (session.output().action_rate - rate).abs() < 1e-6,
            "{skill}: {}",
            session.output().action_rate
        );
        assert!(session.output().total_hit_avg > 0.0);
    }
    let flame = calculate_with_data_session(
        &build("FlameblastPlayer"),
        data(),
        &DataOrchestratorOptions::default(),
    )
    .unwrap();
    // Level 20: 244-367 fire damage, 75% more at the first stage,
    // 8% critical chance and the character's 100% extra critical damage.
    assert!(
        (flame.output().total_hit_avg - (244.0 + 367.0) / 2.0 * 1.75 * 1.08).abs() < 1e-6,
        "{}",
        flame.output().total_hit_avg
    );
}

#[test]
fn flameblast_channel_display_excludes_its_release_and_cooldown() {
    let session = calculate_with_data_session(
        &build("FlameblastPlayer"),
        data(),
        &DataOrchestratorOptions::default(),
    )
    .unwrap();
    // base_minimum_channel_time_ms is 490; the 600-ms release and cooldown
    // belong to the use cycle, not the time spent channelling.
    assert_eq!(session.output().channel_time, 0.49);
    assert!(session.output().cooldown > session.output().channel_time);
}

#[test]
fn non_spell_casts_share_cast_and_skill_speed_without_attack_speed() {
    let skill = "AncientGiftsPlayer";
    let effect = &data().granted_effects[skill];
    assert_eq!(effect.cast_time, Some(1000));
    assert!(
        !effect
            .skill_types
            .iter()
            .any(|t| t == "Spell" || t == "Attack")
    );
    // Wildwood's Gifts uses a 1000 ms cast. The body's 7% and jewel's 3%
    // Cast Speed add to Skill Speed (12% increased, 10% reduced): 1.12/s.
    // Attack Speed cannot apply to this non-attack buff.
    for (cast_speed, expected) in [(false, 1.02), (true, 1.12)] {
        let mut lines = vec![
            "12% increased Skill Speed".into(),
            "10% reduced Skill Speed".into(),
            "100% increased Attack Speed".into(),
        ];
        if cast_speed {
            lines.extend([
                "7% increased Cast Speed".into(),
                "3% increased Cast Speed".into(),
            ]);
        }
        let options = DataOrchestratorOptions {
            extra_modifier_texts: lines,
            ..Default::default()
        };
        let session = calculate_with_data_session(&build(skill), data(), &options).unwrap();
        assert_eq!(session.output().action_rate, expected);
        assert_eq!(session.stat_formula("ActionRate").unwrap().value, expected);
    }
}
