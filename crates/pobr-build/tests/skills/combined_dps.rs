//! Combined damage remains damage per second for skills whose game tooltip
//! uses average damage (`base_skill_show_average_damage_instead_of_dps`).

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::OutputTable;

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
    let data = crate::test_data();
    let innate = calculate(&data, "DetonateLivingPlayer", &[]);
    let explicit = calculate(&data, "DetonateLivingPlayer", &["Culling Strike"]);
    // spells_have_culling_strike: "Your Spells have Culling Strike".
    assert!(innate.culling_dps > 0.0);
    assert!((innate.combined_dps - explicit.combined_dps).abs() < 1e-6);
    assert!((innate.combined_dps - innate.dps - innate.culling_dps).abs() < 1e-6);
}

#[test]
fn culling_strike_ii_applies_only_to_compatible_selected_hits_against_rare_or_unique_enemies() {
    let data = crate::test_data();
    let calculate =
        |skill: &str, support_enabled: bool, main_group: u32, enemy: &str, effective: bool| {
            let build = pobr_build::parse_build(&format!(
            r#"<PathOfBuilding2><Build level="90" className="Monk" mainSocketGroup="{main_group}"/>
<Skills>
<Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/>
<Gem skillId="SupportCullingStrikePlayerTwo" level="1" enabled="{support_enabled}"/></Skill>
<Skill enabled="true"><Gem skillId="MeleeQuarterstaffPlayer" level="20" enabled="true"/></Skill>
</Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Wrapped Quarterstaff
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config>{enemy}</Config></PathOfBuilding2>"#
        ))
        .unwrap();
            pobr_build::calculate_with_data_session(
                &build,
                &data,
                &DataOrchestratorOptions {
                    mode_effective: effective,
                    ..DataOrchestratorOptions::against_reference_enemy()
                },
            )
            .unwrap()
        };
    let unique = r#"<Input name="enemyIsBoss" string="Pinnacle"/>"#;
    for (enemy, threshold) in [
        (r#"<Input name="enemyIsBoss" string="None"/>"#, 0.0),
        (
            r#"<Input name="enemyIsBoss" string="None"/><Input name="conditionEnemyRareOrUnique" boolean="true"/>"#,
            10.0,
        ),
        (unique, 5.0),
    ] {
        let plain = calculate("MeleeQuarterstaffPlayer", false, 1, enemy, true);
        let supported = calculate("MeleeQuarterstaffPlayer", true, 1, enemy, true);
        let output = supported.output();
        assert!(output.dps > 0.0);
        assert_eq!(output.dps, plain.output().dps);
        // The innate grant does not activate II's separate increased
        // threshold after a cull: no such event is supplied by this build.
        let multiplier = 100.0 / (100.0 - threshold);
        assert!(
            (output.cull_multiplier - multiplier).abs() < 1e-9,
            "threshold {threshold}: multiplier {}, conditions {:?}",
            output.cull_multiplier,
            supported.config().conditions
        );
        assert!((output.combined_dps - plain.output().combined_dps * multiplier).abs() < 1e-6);
        assert!(
            (output.culling_dps - plain.output().combined_dps * (multiplier - 1.0)).abs() < 1e-6
        );
        let panel = calculate("MeleeQuarterstaffPlayer", true, 1, enemy, false);
        assert_eq!(panel.output().cull_multiplier, 1.0);
        assert_eq!(panel.output().culling_dps, 0.0);
        let grants = supported.mods_named("CanCull");
        assert_eq!(grants.len(), 1);
        let origin = grants[0].origin.as_ref().unwrap();
        assert_eq!(
            origin.source_id.kind,
            pobr_data::source::SourceKind::SupportGem
        );
        assert_eq!(
            origin.source_id.id,
            "SupportCullingStrikePlayerTwo.support_culling_strike_vs_rare_or_unique_enemy"
        );
    }
    for (skill, enabled, main_group) in [
        ("FireballPlayer", true, 1),
        ("MeleeQuarterstaffPlayer", false, 1),
        ("MeleeQuarterstaffPlayer", true, 2),
        ("TempestBellPlayer", true, 1),
    ] {
        let session = calculate(skill, enabled, main_group, unique, true);
        assert_eq!(
            session.output().culling_dps,
            0.0,
            "{skill} / group {main_group}"
        );
    }
}

#[test]
fn average_damage_tooltip_does_not_remove_rate_or_damage_over_time() {
    let data = crate::test_data();
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
