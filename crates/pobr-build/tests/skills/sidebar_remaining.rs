use pobr_build::{DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;

use crate::test_data as data;

fn calculate(skills: &str, modifiers: &[&str]) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2><Build level="80" className="Witch" mainSocketGroup="1"/><Skills>{skills}</Skills></PathOfBuilding2>"#
    );
    let build = parse_build(&xml).unwrap();
    calculate_with_data(
        &build,
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
            mode_effective: true,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn on_kill_sidebar_attributes_totem_hit_kills_to_the_totem() {
    for (skill, expected) in [("FireballPlayer", 13.0), ("ShockwaveTotemPlayer", 0.0)] {
        let out = calculate(
            &format!(r#"<Skill mainActiveSkill="1"><Gem skillId="{skill}" level="20"/></Skill>"#),
            &["13 Life per Enemy Killed"],
        );
        // Totems use their own skills and have their own Life; the player's
        // 13 Life per kill is recovered only for the player's own kill.
        assert_eq!(out.life_on_kill, expected, "{skill}");
    }
}

#[test]
fn movement_reads_game_penalty_and_presence_ignores_skill_area() {
    let skill = r#"<Skill mainActiveSkill="1"><Gem skillId="FireballPlayer" level="20"/></Skill>"#;
    let base = calculate(skill, &[]);
    let modified = calculate(
        skill,
        &[
            "30% increased Movement Speed",
            "125% increased Presence Area of Effect",
            "Presence Radius is doubled",
            "900% increased Area of Effect",
        ],
    );
    assert_eq!(base.presence_radius_metres, 4.0);
    assert!((base.movement_speed_while_using_skill - 0.3).abs() < 1e-9);
    assert_eq!(modified.presence_radius_metres, 12.0);
    assert_eq!(modified.effective_movement_speed_mod, 1.3);
    assert!((modified.movement_speed_while_using_skill - 0.39).abs() < 1e-9);
}

#[test]
fn blood_price_reduces_enemy_life_to_remove_without_changing_hits() {
    let skill = r#"<Skill mainActiveSkill="1"><Gem skillId="FireballPlayer" level="20"/></Skill>"#;
    let base = calculate(skill, &[]);
    let reserved = calculate(
        skill,
        &["Enemies in your Presence have at least 10% of Life Reserved"],
    );
    assert_eq!(base.total_hit_avg, reserved.total_hit_avg);
    assert!(reserved.reservation_dps > 0.0);
    assert!((reserved.combined_dps / base.combined_dps - 1.0 / 0.9).abs() < 1e-5);
}

#[test]
fn minion_limit_is_the_selected_summoning_skills_limit() {
    let zombie =
        r#"<Skill mainActiveSkill="1"><Gem skillId="RaiseZombiePlayer" level="20"/></Skill>"#;
    let base = calculate(zombie, &[]);
    assert!(base.active_minion_limit > 0.0);
    let other = calculate(
        &format!(
            r#"<Skill mainActiveSkill="1"><Gem skillId="FireballPlayer" level="20"/></Skill>{zombie}"#
        ),
        &[],
    );
    assert_eq!(other.active_minion_limit, 0.0);
}

#[test]
fn curse_effect_display_reads_curse_magnitude_modifiers() {
    let plain = calculate(
        r#"<Skill mainActiveSkill="1"><Gem skillId="DespairPlayer" level="20" quality="0"/></Skill>"#,
        &[],
    );
    let increased = calculate(
        r#"<Skill mainActiveSkill="1"><Gem skillId="DespairPlayer" level="20" quality="0"/></Skill>"#,
        &["40% increased Curse Magnitudes"],
    );
    assert!(plain.curse_effect_mod > 0.0);
    assert!((increased.curse_effect_mod / plain.curse_effect_mod - 1.4).abs() < 1e-9);
}

#[test]
fn ignited_ground_is_a_view_of_its_ignite() {
    let ground = calculate(
        r#"<Skill mainActiveSkill="1"><Gem skillId="BallLightningPlayer" level="20" statSetIndex="3"/></Skill>"#,
        &[],
    );
    assert!(ground.ignite_dps > 0.0);
    assert_eq!(ground.burning_ground_dps, ground.ignite_dps);
}

#[test]
fn profane_decay_reads_its_own_chaos_damage_over_time() {
    let skill = r#"<Skill mainActiveSkill="1"><Gem skillId="TriggeredDecayingHexPlayer" level="20"/></Skill>"#;
    let base = calculate(skill, &[]);
    let boosted = calculate(skill, &["100% increased Chaos Damage"]);
    assert!(base.decay_dps > 0.0);
    assert_eq!(base.decay_dps, base.skill_total_dot);
    assert!((boosted.decay_dps / base.decay_dps - 2.0).abs() < 1e-6);
}

#[test]
fn lingering_mirage_damage_belongs_to_the_clone() {
    let clone = calculate(
        r#"<Skill mainActiveSkill="3"><Gem skillId="MetaMirageArcherPlayer" level="20"/><Gem skillId="OilGrenadePlayer" level="20" statSetIndex="2"/></Skill>"#,
        &["Adds 100 to 100 Fire Damage to Attacks"],
    );
    assert!(clone.mirage_dps > 0.0);
    assert!(clone.mirage_burning_ground_dps > 0.0);
    // The meta gem's built-in support supplies skill_disabled_unless_cloned.
    assert_eq!(clone.mirage_dps, clone.combined_dps);
    assert_eq!(clone.mirage_burning_ground_dps, clone.burning_ground_dps);
}

#[test]
fn armour_base_penalty_multiplies_movement_but_can_be_ignored() {
    let xml = r#"<PathOfBuilding2><Build level="80" className="Witch"/><Items><Item id="1">Rarity: NORMAL
Rusted Cuirass
--------
Armour: 45
--------
Item Level: 80</Item><ItemSet id="1"><Slot name="Body Armour" itemId="1"/></ItemSet></Items></PathOfBuilding2>"#;
    let build = parse_build(xml).unwrap();
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec!["30% increased Movement Speed".into()],
        ..Default::default()
    };
    let penalty = calculate_with_data(&build, data(), &options).unwrap();
    assert_eq!(
        penalty.effective_movement_speed_mod, 1.235,
        "items={:?}",
        build.items
    );
    let ignored = calculate_with_data(
        &build,
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![
                "30% increased Movement Speed".into(),
                "Ignore all Movement Penalties from Armour".into(),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ignored.effective_movement_speed_mod, 1.3);
}

#[test]
fn planted_defiance_banner_grants_its_movement_buff() {
    let template = r#"<PathOfBuilding2><Build level="80" className="Witch" mainSocketGroup="1"/><Skills><Skill mainActiveSkill="1"><Gem skillId="FireballPlayer" level="20"/></Skill><Skill mainActiveSkill="1"><Gem skillId="DefianceBannerReservationPlayer" level="20"/></Skill></Skills><Config><Input name="bannerPlanted" boolean="PLANTED"/></Config></PathOfBuilding2>"#;
    let calculate = |planted| {
        let build = parse_build(&template.replace("PLANTED", planted)).unwrap();
        calculate_with_data(&build, data(), &DataOrchestratorOptions::default()).unwrap()
    };
    let carried = calculate("false");
    let planted = calculate("true");
    assert_eq!(carried.effective_movement_speed_mod, 1.0);
    assert!(planted.effective_movement_speed_mod > 1.0);
}

#[test]
fn wolf_pack_limit_uses_its_level_dependent_native_pack_size() {
    for (level, expected) in [(1, 3.0), (18, 5.0), (20, 6.0)] {
        let out = calculate(
            &format!(
                r#"<Skill mainActiveSkill="1"><Gem skillId="WolfPackPlayer" level="{level}" skillMinion="WolfMinion"/></Skill>"#
            ),
            &[],
        );
        assert_eq!(out.active_minion_limit, expected, "gem level {level}");
    }
}
