use pobr_build::{parse_build, resolve_main_skill_selection};

fn assert_selection(primary: &str, ordered: &[&str]) {
    let data = crate::test_data();
    for (index, expected) in ordered.iter().enumerate() {
        let ordinal = index + 1;
        let xml = format!(
            r#"<PathOfBuilding2><Build className="Witch" level="90" mainSocketGroup="1"/>
<Skills><Skill mainActiveSkill="{ordinal}"><Gem skillId="{primary}" level="20" skillPart="1"/></Skill></Skills>
</PathOfBuilding2>"#
        );
        let code = pobr_build::encode_pob_code(&xml).unwrap();
        let build = parse_build(&pobr_build::decode_pob_code(&code).unwrap()).unwrap();
        assert_eq!(build.socket_groups[0].main_active_skill, Some(ordinal));
        assert_eq!(
            resolve_main_skill_selection(&build, &data),
            Some((0, ordinal, expected.to_string()))
        );
    }
}

#[test]
fn crossbow_display_order_selects_shot_before_ammo() {
    assert_selection(
        "HighVelocityRoundsAmmoPlayer",
        &["HighVelocityRoundsPlayer", "HighVelocityRoundsAmmoPlayer"],
    );
}

#[test]
fn banner_display_order_selects_banner_before_reservation() {
    assert_selection(
        "WarBannerReservationPlayer",
        &["WarBannerPlayer", "WarBannerReservationPlayer"],
    );
}

#[test]
fn mirage_display_order_skips_support_and_selects_spawn_before_meta() {
    assert_selection(
        "MetaMirageArcherPlayer",
        &["MirageArcherSpawnPlayer", "MetaMirageArcherPlayer"],
    );
    assert_selection(
        "MetaMirageDeadeyePlayer",
        &["MirageDeadeyeSpawnPlayer", "MetaMirageDeadeyePlayer"],
    );
}

#[test]
fn selected_crossbow_shots_keep_the_ammo_reload_modifiers() {
    let data = crate::test_data();
    for (primary, reload, bolts) in [
        ("ExplosiveShotAmmoPlayer", 0.8 / 0.7, 1.0),
        ("SiegeCascadeAmmoPlayer", 0.8 / 0.7, 1.0),
        ("ShockburstRoundsAmmoPlayer", 0.8 / 0.8, 19.0),
        ("HailstormRoundsAmmoPlayer", 0.8 / 2.5, 50.0),
    ] {
        let build = crossbow(primary);
        let session = pobr_build::calculate_with_data_session(
            &build,
            &data,
            &pobr_build::DataOrchestratorOptions::default(),
        )
        .unwrap();
        let out = session.output();
        // active_skill_reload_speed_+%_final belongs to the ammo's reload
        // action; base_number_of_crossbow_bolts supplies its magazine.
        assert!(
            (out.reload_time - reload).abs() < 1e-9,
            "{primary}: {}",
            out.reload_time
        );
        let cycle = session.crossbow_reload().unwrap();
        assert_eq!(cycle.effective_bolt_count, bolts);
        assert!((out.action_rate - bolts / (bolts / out.firing_rate + reload)).abs() < 1e-9);
        assert!(out.dps > 0.0);
    }
}

#[test]
fn requiem_fires_without_a_magazine_or_reload() {
    let data = crate::test_data();
    let session = pobr_build::calculate_with_data_session(
        &crossbow("CrossbowRequiemAmmoPlayer"),
        &data,
        &pobr_build::DataOrchestratorOptions::default(),
    )
    .unwrap();
    // crossbow_skill_has_no_ammo: "Does not use Ammunition and can be
    // fired freely until the Requiem ends."
    assert!(session.output().firing_rate > 0.0);
    assert_eq!(session.output().action_rate, session.output().firing_rate);
    assert_eq!(session.output().reload_time, 0.0);
    assert!(session.crossbow_reload().is_none());
}

fn crossbow(primary: &str) -> pobr_build::Build {
    parse_build(&format!(
        r#"<PathOfBuilding2><Build className="Witch" level="90" mainSocketGroup="1"/>
<Skills><Skill mainActiveSkill="1"><Gem skillId="{primary}" level="20"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Makeshift Crossbow
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items></PathOfBuilding2>"#
    ))
    .unwrap()
}

#[test]
fn magma_spray_uses_its_fire_base_and_complete_groups_of_shield_defence() {
    let data = crate::test_data();
    let run = |armour: u32| {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2><Build level="90" className="Witch"/>
<Skills><Skill mainActiveSkill="2"><Gem skillId="MagmaBarrierPlayer" level="20"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Splintered Tower Shield
Armour: {armour}
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 2" itemId="1"/></ItemSet></Items></PathOfBuilding2>"#
        ))
        .unwrap();
        pobr_build::calculate_with_data_session(
            &build,
            &data,
            &pobr_build::DataOrchestratorOptions::default(),
        )
        .unwrap()
    };
    // off_hand_weapon_*_fire_damage = 103–155 at level 20;
    // off_hand_*_added_fire_damage_per_15_shield_armour = 5–7.
    let bare = run(0);
    assert!(bare.output().dps > 0.0);
    assert_eq!(run(14).output().total_hit_avg, bare.output().total_hit_avg);
    let ratio = run(15).output().total_hit_avg / bare.output().total_hit_avg;
    assert!((ratio - (129.0 + 6.0) / 129.0).abs() < 1e-9, "{ratio}");
}

#[test]
fn granted_periodic_effects_use_their_game_intervals() {
    let data = crate::test_data();
    for (primary, rate) in [
        ("ArchonOfChayulaPlayer", 5.0),
        ("TheStarsAnswerPlayer", 1000.0 / 300.0),
        ("WalkingCalamityPlayer", 1000.0 / 150.0),
    ] {
        let build = parse_build(&format!(r#"<PathOfBuilding2><Build className="Witch" level="90"/>
<Skills><Skill mainActiveSkill="2"><Gem skillId="{primary}" level="20"/></Skill></Skills></PathOfBuilding2>"#)).unwrap();
        for speed in [
            "0% increased Attack and Cast Speed",
            "100% increased Attack and Cast Speed",
        ] {
            let options = pobr_build::DataOrchestratorOptions {
                extra_modifier_texts: vec![speed.into()],
                ..Default::default()
            };
            let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
            // archon_of_chayula_tornado_attack_interval_ms is on the
            // tornado; walking_calamity_spawn_meteor_every_ms is on its parent.
            assert!(
                (session.output().action_rate - rate).abs() < 1e-8,
                "{primary}: {}",
                session.output().action_rate
            );
            assert!(session.output().dps > 0.0);
        }
    }
}
