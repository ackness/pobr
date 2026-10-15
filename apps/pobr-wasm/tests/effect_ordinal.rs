use serde_json::{Value, json};

#[test]
fn display_ordinals_survive_import_materialization_and_export() {
    let dir = pobr_gamedata::current_data_dir();
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).unwrap();
    for (primary, secondary) in [
        ("HighVelocityRoundsAmmoPlayer", "HighVelocityRoundsPlayer"),
        ("WarBannerReservationPlayer", "WarBannerPlayer"),
        ("MetaMirageArcherPlayer", "MirageArcherSpawnPlayer"),
        ("MetaMirageDeadeyePlayer", "MirageDeadeyeSpawnPlayer"),
    ] {
        for (index, expected) in [secondary, primary].iter().enumerate() {
            let ordinal = index + 1;
            let xml = format!(
                r#"<PathOfBuilding2><Build className="Witch" level="90" mainSocketGroup="1"/>
<Skills><Skill mainActiveSkill="{ordinal}"><Gem skillId="{primary}" level="20"/></Skill></Skills>
</PathOfBuilding2>"#
            );
            let code = pobr_build::encode_pob_code(&xml).unwrap();
            let mut decoded: Value =
                serde_json::from_str(&pobr_wasm::decode_build_json(&code).unwrap()).unwrap();
            decoded["socket_groups"][0]
                .as_object_mut()
                .unwrap()
                .remove("active_skill_id");
            let request = json!({
                "character": decoded["character"],
                "socket_groups": decoded["socket_groups"],
                "main_socket_group": decoded["main_socket_group"],
            });
            let exported = pobr_wasm::encode_build_json(&request.to_string()).unwrap();
            let decoded: Value =
                serde_json::from_str(&pobr_wasm::decode_build_json(&exported).unwrap()).unwrap();
            assert_eq!(decoded["socket_groups"][0]["main_active_skill"], ordinal);
            let calculated: Value = serde_json::from_str(
                &pobr_wasm::calculate_build_json(&json!({"pob_code": exported}).to_string())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(calculated["main_skill"]["skill_id"], *expected);
        }
    }
}
