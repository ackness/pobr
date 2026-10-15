use pobr_build::{BuildData, parse_build, resolve_main_skill_selection};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn assert_selection(primary: &str, ordered: &[&str]) {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap();
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
