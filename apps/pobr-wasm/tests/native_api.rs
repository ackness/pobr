//! Native WASM host boundary tests. Actual v2 calculation requires explicitly supplied inputs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pobr_build::native_document::{
    NativeActorPolicyPin, NativeActorSelection, NativeBuildRequest, NativeBuildSchema,
    NativeDatasetBinding, NativeEvaluationKind, NativeGemSelection,
};
use pobr_build::native_skill;
use pobr_build::{Build, BuildConfig, CharacterIdentity, SocketGroup};
use pobr_gamedata::{GameData, NativeData};
use serde_json::{Value, json};

fn v1_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/pobr-gamedata/tests/fixtures/native_runtime")
}

fn read_pack(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let bytes = std::fs::read(root.join("manifest.json")).expect("read explicit pack manifest");
    let manifest: Value = serde_json::from_slice(&bytes).expect("parse pack manifest");
    let mut files: BTreeMap<_, _> = manifest["files"]
        .as_object()
        .expect("manifest inventory")
        .keys()
        .map(|path| {
            (
                path.clone(),
                std::fs::read(root.join(path)).expect("read sealed pack file"),
            )
        })
        .collect();
    files.insert("manifest.json".into(), bytes);
    files
}

fn stage(files: &BTreeMap<String, Vec<u8>>) {
    for (index, (path, bytes)) in files.iter().enumerate() {
        if index.is_multiple_of(2) {
            pobr_wasm::stage_data_bytes(path, bytes.clone());
        } else {
            pobr_wasm::stage_data_file(path, std::str::from_utf8(bytes).expect("JSON bytes"));
        }
    }
}

fn v1_request() -> String {
    // A structurally valid request can bind queryable v1 data without admitting it.
    let data = GameData::new(v1_dir());
    let mut group = SocketGroup::new()
        .with_gem("Metadata/Items/Gems/SkillGemSpark")
        .with_active_skill("SparkPlayer", 1)
        .with_gem_skill_quality("SparkPlayer", 1, 0);
    group.active_gem_quality = Some(0);
    let request = NativeBuildRequest {
        schema: NativeBuildSchema::V1,
        dataset: NativeDatasetBinding::from_game_data(&data).expect("v1 native binding"),
        actor: NativeActorSelection {
            id: "Metadata/Characters/Int/IntFourb".into(),
            level: 1,
        },
        skill: NativeGemSelection {
            group_index: 0,
            gem_index: 0,
            gem_id: "Metadata/Items/Gems/SkillGemSpark".into(),
            effect_id: "SparkPlayer".into(),
            stat_set_id: "SparkPlayer".into(),
            level: 1,
            quality: 0,
        },
        actor_policy: NativeActorPolicyPin {
            id: "selected-player-generic-ot-v1".into(),
            version: 1,
            sha256: "f".repeat(64),
        },
        evaluation: NativeEvaluationKind::ConditionalSkillHitV1,
        build: Build::new()
            .with_character(CharacterIdentity {
                level: 1,
                class_name: "Display label".into(),
                ascendancy_name: String::new(),
            })
            .with_config(
                BuildConfig::new().with_campaign_progress(pobr_core::CampaignProgress::Act1),
            )
            .add_socket_group(group),
    };
    String::from_utf8(request.to_json().expect("complete native document")).expect("JSON UTF-8")
}

fn error(result: Result<String, String>, code: &str, message: &str) {
    let text = result.expect_err("request must fail");
    let value: Value = serde_json::from_str(&text).expect("structured API error");
    assert_eq!(value["code"], code, "{text}");
    assert!(
        value["message"].as_str().unwrap().contains(message),
        "expected {message:?}: {text}"
    );
}

#[test]
fn native_calculation_requires_explicit_native_initialization() {
    assert!(!pobr_wasm::is_data_ready());
    assert!(!pobr_wasm::is_native_data_ready());
    let request = v1_request();
    error(
        pobr_wasm::calculate_native_build_json(&request),
        "not_initialized",
        "native data not initialized",
    );
    assert!(pobr_wasm::init_native_staged_data().is_err());
    assert!(!pobr_wasm::is_native_data_ready());

    let ordinary_dir = pobr_gamedata::current_data_dir();
    pobr_wasm::init_data_from_dir(&ordinary_dir.to_string_lossy()).expect("ordinary data");
    assert!(pobr_wasm::is_data_ready());
    assert!(!pobr_wasm::is_native_data_ready());
    error(
        pobr_wasm::calculate_native_build_json(&request),
        "not_initialized",
        "native data not initialized",
    );
    assert!(pobr_wasm::init_native_data_from_dir(&ordinary_dir.to_string_lossy()).is_err());
    assert!(
        pobr_wasm::is_data_ready(),
        "failed native load preserves ordinary mode"
    );
    assert!(!pobr_wasm::is_native_data_ready());
}

#[test]
fn staged_native_v1_is_ready_but_never_calculable_or_ordinary_data() {
    stage(&read_pack(&v1_dir()));
    pobr_wasm::init_native_staged_data().expect("mixed byte/text staging of native v1");
    assert!(pobr_wasm::is_native_data_ready());
    assert!(!pobr_wasm::is_data_ready());
    assert!(pobr_wasm::state::build_data().is_err());
    assert!(pobr_wasm::state::game_data().is_err());
    let request = v1_request();
    error(
        pobr_wasm::calculate_native_build_json(&request),
        "bad_request",
        "exact v2",
    );
    error(
        pobr_wasm::calculate_build_json("{}"),
        "not_initialized",
        "game data not initialized",
    );
    error(
        pobr_wasm::gem_catalog_json(),
        "not_initialized",
        "game data not initialized",
    );
    assert!(
        pobr_wasm::calculate_json(r#"{"modifiers":["+50 to maximum life"]}"#).is_err(),
        "native mode must not supply ordinary parser rules"
    );

    let mut missing: Value = serde_json::from_str(&request).unwrap();
    missing["build"]["config"]
        .as_object_mut()
        .unwrap()
        .remove("enemy_tier");
    for invalid in [
        "<PathOfBuilding/>".to_string(),
        missing.to_string(),
        format!(
            r#"{{"sch\u0065ma":"pobr-native-build/v1",{}"#,
            &request[1..]
        ),
    ] {
        error(
            pobr_wasm::calculate_native_build_json(&invalid),
            "bad_request",
            "native build",
        );
    }
}

#[test]
fn failed_native_and_ordinary_initialization_preserve_the_native_catalogs() {
    pobr_wasm::init_native_data_from_dir(&v1_dir().to_string_lossy()).expect("native disk init");
    let request = v1_request();
    let before = pobr_wasm::calculate_native_build_json(&request).unwrap_err();
    let mut changed = read_pack(&v1_dir());
    changed.get_mut("base/skill_gems.json").unwrap().push(b' ');
    stage(&changed);
    let failed = pobr_wasm::init_native_staged_data().expect_err("changed sealed bytes");
    assert!(failed.contains("SHA-256 mismatch"), "{failed}");
    assert!(
        pobr_wasm::init_native_staged_data().is_err(),
        "staging was consumed"
    );

    // Both ordinary staged paths must keep their native-profile guard, even the
    // caller-verified path, and must leave the active native instance intact.
    for init in [
        pobr_wasm::init_staged_data,
        pobr_wasm::init_staged_data_verified,
    ] {
        stage(&read_pack(&v1_dir()));
        let failed = init().expect_err("ordinary native-profile guard");
        assert!(failed.contains("native-profile-not-admitted:"), "{failed}");
        assert!(pobr_wasm::is_native_data_ready());
        assert!(!pobr_wasm::is_data_ready());
        assert_eq!(
            pobr_wasm::calculate_native_build_json(&request).unwrap_err(),
            before
        );
    }
    assert!(pobr_wasm::init_native_data_from_dir("/nonexistent/pobr-native-pack").is_err());
    assert!(pobr_wasm::is_native_data_ready());
    assert_eq!(
        pobr_wasm::calculate_native_build_json(&request).unwrap_err(),
        before
    );
}

#[test]
#[ignore = "requires POBR_NATIVE_WASM_PACK and POBR_NATIVE_WASM_REQUEST for the exact actual v2 pack/document"]
fn supplied_actual_pack_saved_request_reopens_and_rechecks_every_call() {
    let pack = PathBuf::from(
        std::env::var_os("POBR_NATIVE_WASM_PACK").expect("supply the immutable actual v2 pack"),
    );
    let document = PathBuf::from(
        std::env::var_os("POBR_NATIVE_WASM_REQUEST").expect("supply the saved native request JSON"),
    );
    let saved = std::fs::read_to_string(&document).expect("read saved native request");
    let request = NativeBuildRequest::from_json(saved.as_bytes()).expect("strict saved request");
    let native = NativeData::load(&GameData::new(&pack)).expect("actual native catalogs");
    let expected = native_skill::calculate_native_skill(
        &native_skill::admit_native_skill(&native, &request).expect("actual request admitted"),
    )
    .expect("shared native calculation");
    let expected_json = serde_json::to_string(&expected).unwrap();
    let lightning = expected
        .damage
        .iter()
        .find(|d| d.damage_type == pobr_data::constants::DamageType::Lightning)
        .unwrap();
    assert_eq!(
        (lightning.non_critical.min, lightning.non_critical.max),
        (1.0, 10.0)
    );
    assert_eq!(expected.critical_probability, 0.09);
    assert_eq!(expected.critical_multiplier, 2.0);
    assert!((expected.conditional_weighted_mean - 5.995).abs() < 1e-10);
    assert_eq!(expected.analytical_base_use_ms, 700);
    assert_eq!(expected.resource_costs[0].resource, "Mana");
    assert_eq!(expected.resource_costs[0].amount_per_use, 5.0);
    assert_eq!(expected.total_projectiles, 4.0);
    assert_eq!(expected.projectile_lifetime_ms, 2000.0);

    pobr_wasm::init_native_data_from_dir(&pack.to_string_lossy()).expect("host native init");
    assert_eq!(
        pobr_wasm::calculate_native_build_json(&saved).unwrap(),
        expected_json
    );
    stage(&read_pack(&pack));
    pobr_wasm::init_native_staged_data().expect("native bytes init");
    let reopened = std::fs::read_to_string(&document).expect("reopen the same saved document");
    assert_eq!(
        pobr_wasm::calculate_native_build_json(&reopened).unwrap(),
        expected_json
    );

    for (pointer, value, message) in [
        (
            "/dataset/runtime_identity_sha256",
            json!(different_pin(&request.dataset.runtime_identity_sha256)),
            "runtime_identity_sha256",
        ),
        (
            "/dataset/assembly_key",
            json!(different_pin(&request.dataset.assembly_key)),
            "assembly_key",
        ),
        (
            "/actor_policy/sha256",
            json!(different_pin(&request.actor_policy.sha256)),
            "actor_policy",
        ),
        (
            "/skill/stat_set_id",
            json!("SparkColdInfusedPlayer"),
            "skill",
        ),
        (
            "/build/config/conditions",
            json!({"Moving":false}),
            "build.config",
        ),
    ] {
        let mut edited: Value = serde_json::from_str(&reopened).unwrap();
        *edited.pointer_mut(pointer).unwrap() = value;
        let edited_json = edited.to_string();
        NativeBuildRequest::from_json(edited_json.as_bytes()).expect("structurally valid edit");
        error(
            pobr_wasm::calculate_native_build_json(&edited_json),
            "bad_request",
            message,
        );
        assert_eq!(
            pobr_wasm::calculate_native_build_json(&reopened).unwrap(),
            expected_json
        );
    }
    assert_eq!(
        pobr_wasm::state::response_cache_hits(),
        0,
        "native evaluation is uncached"
    );

    let mut changed = read_pack(&pack);
    changed.get_mut("base/skill_gems.json").unwrap().push(b' ');
    stage(&changed);
    assert!(pobr_wasm::init_native_staged_data().is_err());
    assert_eq!(
        pobr_wasm::calculate_native_build_json(&reopened).unwrap(),
        expected_json
    );

    // Even the identical saved request must be rejected after a successful pack
    // switch; no response from the previous native snapshot may be reused.
    pobr_wasm::init_native_data_from_dir(&v1_dir().to_string_lossy()).expect("switch to v1");
    error(
        pobr_wasm::calculate_native_build_json(&reopened),
        "bad_request",
        "native dataset differs",
    );
    pobr_wasm::init_native_data_from_dir(&pack.to_string_lossy()).expect("switch back to v2");
    assert_eq!(
        pobr_wasm::calculate_native_build_json(&reopened).unwrap(),
        expected_json
    );
    error(
        pobr_wasm::calculate_build_json("{}"),
        "not_initialized",
        "game data not initialized",
    );
    error(
        pobr_wasm::calculate_build_json(&reopened),
        "bad_request",
        "invalid request json",
    );
}

fn different_pin(pin: &str) -> String {
    if pin == "f".repeat(64) {
        "a".repeat(64)
    } else {
        "f".repeat(64)
    }
}
