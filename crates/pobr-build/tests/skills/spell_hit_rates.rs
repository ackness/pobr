//! Repeated spell hits come from the game's timing stats, not cast speed.
use pobr_build::{
    Build, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data_session,
};

use crate::test_data as data;

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

#[test]
fn selected_warcry_rate_uses_warcry_speed_and_explains_its_sources() {
    let effect = &data().granted_effects["SeismicCryPlayer"];
    assert_eq!(effect.cast_time, Some(800));
    assert!(effect.skill_types.iter().any(|kind| kind == "Warcry"));
    assert!(!effect.skill_types.iter().any(|kind| kind == "Spell"));

    for (extra, expected, increased) in [
        (vec![], 1.25, 0.0),
        // The frozen Warbringer's applied WarcrySpeed totals 868%, including
        // its scaled jewels and Mageblood's 30% Skill Speed. The generic
        // Skill Speed line fans out to WarcrySpeed and must count only once.
        (
            vec!["838% increased Warcry Speed", "30% increased Skill Speed"],
            12.1,
            868.0,
        ),
        (
            vec![
                "838% increased Warcry Speed",
                "30% increased Skill Speed",
                "900% increased Cast Speed",
                "900% increased Attack Speed",
                "20% more Warcry Speed",
                "20% increased Action Speed",
            ],
            17.424,
            868.0,
        ),
    ] {
        let options = DataOrchestratorOptions {
            extra_modifier_texts: std::iter::once("Ignore Warcry Cooldowns")
                .chain(extra)
                .map(str::to_owned)
                .collect(),
            ..Default::default()
        };
        let session =
            calculate_with_data_session(&build("SeismicCryPlayer"), data(), &options).unwrap();
        let output = session.output();
        assert_eq!(output.cooldown, 0.0);
        assert!((output.action_rate - expected).abs() < 1e-9);
        assert!((output.warcry_cast_time - 1.0 / expected).abs() < 1e-6);
        let breakdown = pobr_build::breakdown::stat_breakdown(&session, "ActionRate").unwrap();
        assert_eq!(breakdown.inc_total, increased);
        let formula = breakdown.formula.unwrap();
        assert_eq!(formula.steps[0].operand, 1.25);
        assert_eq!(formula.value, output.action_rate);
        assert_eq!(session.stat_mod_names("ActionRate"), ["WarcrySpeed".into()]);
        assert!(breakdown.mods.iter().all(|row| {
            row.source_text
                .as_deref()
                .is_none_or(|text| !text.contains("Cast Speed") && !text.contains("Attack Speed"))
        }));
    }
}

#[test]
fn selected_warcry_rate_keeps_cooldown_recovery_and_spell_speed_separate() {
    for (recovery, cooldown) in [(false, 8.019), (true, 4.026)] {
        let mut options = DataOrchestratorOptions {
            extra_modifier_texts: vec!["100% increased Warcry Speed".into()],
            ..Default::default()
        };
        if recovery {
            options
                .extra_modifier_texts
                .push("100% increased Warcry Cooldown Recovery Rate".into());
        }
        let session =
            calculate_with_data_session(&build("SeismicCryPlayer"), data(), &options).unwrap();
        assert!((session.output().cooldown - cooldown).abs() < 1e-9);
        assert!((session.output().action_rate - 1.0 / cooldown).abs() < 1e-9);
        assert_eq!(session.output().warcry_cast_time, 0.4);
        assert_eq!(
            session.stat_formula("ActionRate").unwrap().value,
            session.output().action_rate
        );
    }

    let plain = calculate_with_data_session(
        &build("FireballPlayer"),
        data(),
        &DataOrchestratorOptions::default(),
    )
    .unwrap();
    let modified = calculate_with_data_session(
        &build("FireballPlayer"),
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![
                "868% increased Warcry Speed".into(),
                "100% increased Cast Speed".into(),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert!((modified.output().action_rate - plain.output().action_rate * 2.0).abs() < 2e-9);
}
