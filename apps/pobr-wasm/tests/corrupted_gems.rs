//! Whether a gem is corrupted, and the monster a minion gem summons, survive
//! decoding, the structured request and export, so "all Corrupted Skill Gems"
//! modifiers and a Companion's Spirit reservation reach the calculation
//! whichever way a build arrives.

use serde_json::{Value, json};

fn init() {
    pobr_wasm::init_data_from_dir(&pobr_gamedata::current_data_dir().to_string_lossy()).unwrap();
}

fn decode(code: &str) -> Value {
    serde_json::from_str(&pobr_wasm::decode_build_json(code).unwrap()).unwrap()
}

fn calculate(request: &Value) -> Value {
    serde_json::from_str(&pobr_wasm::calculate_build_json(&request.to_string()).unwrap()).unwrap()
}

/// A Fireball at `level`, with the gem's `corrupted` state when given, on a
/// character wearing "+1 to Level of all Corrupted Skill Gems".
fn request(level: u32, corrupted: Option<bool>) -> Value {
    let mut gem = json!({ "skill_id": "FireballPlayer", "level": level, "quality": 0 });
    if let Some(corrupted) = corrupted {
        gem["corrupted"] = json!(corrupted);
    }
    json!({
        "character": { "level": 85, "class_name": "Sorceress" },
        "main_socket_group": 0,
        "socket_groups": [{ "enabled": true, "gems": [gem] }],
        "items": [{
            "slot": "amulet",
            "text": "Rarity: RARE\nTest Amulet\nJade Amulet\nImplicits: 0\n+1 to Level of all Corrupted Skill Gems",
        }],
    })
}

/// The numbers of a result: a result also names its inputs, which differ.
fn output(request: &Value) -> Value {
    let stats = calculate(request)["stats"].clone();
    assert!(stats.is_array(), "{stats}");
    stats
}

#[test]
fn a_request_states_whether_a_gem_is_corrupted() {
    init();
    let clean = output(&request(20, None));
    assert_eq!(output(&request(20, Some(false))), clean);
    // The corrupted gem gains the level: the same numbers as a level 21 gem
    // the bonus does not reach.
    assert_eq!(output(&request(20, Some(true))), output(&request(21, None)));
    assert_ne!(output(&request(20, Some(true))), clean);
}

#[test]
fn corruption_survives_export_and_decode() {
    init();
    let code = pobr_wasm::encode_build_json(&request(20, Some(true)).to_string()).unwrap();
    let xml = pobr_build::decode_pob_code(&code).unwrap();
    assert!(xml.contains(r#"corrupted="true""#), "{xml}");
    let decoded = decode(&code);
    assert_eq!(
        decoded["socket_groups"][0]["gems"][0],
        json!({ "skill_id": "FireballPlayer", "level": 20, "quality": 0, "corrupted": true })
    );

    // A gem that is not corrupted is written and read as before.
    let code = pobr_wasm::encode_build_json(&request(20, None).to_string()).unwrap();
    assert!(
        !pobr_build::decode_pob_code(&code)
            .unwrap()
            .contains("corrupted")
    );
    assert_eq!(
        decode(&code)["socket_groups"][0]["gems"][0],
        json!({ "skill_id": "FireballPlayer", "level": 20, "quality": 0 })
    );
}

/// A Companion summoning `minion` when given, on a character with 100 Spirit.
fn companion(minion: Option<&str>) -> Value {
    let mut gem = json!({ "skill_id": "SummonBeastPlayer", "level": 20, "quality": 0 });
    if let Some(minion) = minion {
        gem["minion"] = json!(minion);
    }
    json!({
        "character": { "level": 85, "class_name": "Huntress" },
        "main_socket_group": 0,
        "socket_groups": [{ "enabled": true, "gems": [gem] }],
        "items": [{
            "slot": "amulet",
            "text": "Rarity: RARE\nTest Amulet\nJade Amulet\nImplicits: 0\n+100 to Spirit",
        }],
    })
}

#[test]
fn a_chosen_minion_survives_export_and_decode() {
    init();
    let quadrilla = "Metadata/Monsters/Quadrilla/Quadrilla";
    // The beast sets the Spirit the Companion reserves.
    assert_ne!(
        output(&companion(Some(quadrilla))),
        output(&companion(None))
    );

    let code = pobr_wasm::encode_build_json(&companion(Some(quadrilla)).to_string()).unwrap();
    let xml = pobr_build::decode_pob_code(&code).unwrap();
    assert!(
        xml.contains(&format!(r#"skillMinion="{quadrilla}""#)),
        "{xml}"
    );
    assert_eq!(
        decode(&code)["socket_groups"][0]["gems"][0],
        json!({ "skill_id": "SummonBeastPlayer", "level": 20, "quality": 0, "minion": quadrilla })
    );
}
