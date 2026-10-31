//! Native document rejection must precede data loading and compatibility imports.

use pobr_build::native_document::MAX_NATIVE_BUILD_BYTES;
use pobr_cli::{CliError, native::calculate_native_document};
use pobr_gamedata::GameData;

#[test]
fn native_rejects_non_native_input_before_loading_data() {
    // An empty in-memory dataset cannot fall back to repository files or CURRENT.
    let data = GameData::from_memory(Default::default());
    for bytes in [
        b"".as_slice(),
        b"<PathOfBuilding><Build level=\"1\"/></PathOfBuilding>",
        b"eJwPoBBuildCode",
        br#"{}"#,
        br#"{"schema":"pobr-native-build/v2"}"#,
        br#"{"schema":"pobr-native-build/v1","unknown":"<PathOfBuilding/>"}"#,
        br#"{"schema":"pobr-native-build/v1","schema":"pobr-native-build/v1"}"#,
        br#"{"schema":"pobr-native-build/v1","\u0073chema":"pobr-native-build/v1"}"#,
        br#"{"schema":"pobr-native-build/v1"} trailing"#,
    ] {
        let error = calculate_native_document(&data, bytes)
            .expect_err("invalid native documents must not reach data loading or XML import");
        assert!(
            matches!(error, CliError::NativeDocument(_)),
            "input {:?}: {error}",
            String::from_utf8_lossy(bytes),
        );
    }
}

#[test]
fn native_rejects_oversized_input_before_json_or_data_loading() {
    let data = GameData::from_memory(Default::default());
    let bytes = vec![b' '; MAX_NATIVE_BUILD_BYTES + 1];
    let error = calculate_native_document(&data, &bytes)
        .expect_err("oversized native documents must fail before decoding or data loading");
    assert!(matches!(error, CliError::NativeDocument(_)));
    assert!(error.to_string().contains("exceeds the byte limit"));
}

/// Exercises the real command in fresh processes; no source paths or digests are guessed.
#[test]
#[ignore = "requires POBR_NATIVE_PASSIVE_PACK and POBR_NATIVE_PASSIVE_REQUEST bound to an assembled new-profile candidate"]
fn supplied_actual_passive_saved_edits_run_in_fresh_cli_processes() {
    use std::path::PathBuf;
    use std::process::Command;

    use pobr_build::native_document::NativeBuildRequest;
    use pobr_data::passive_tree::NodeId;
    use serde_json::Value;

    let pack = PathBuf::from(
        std::env::var_os("POBR_NATIVE_PASSIVE_PACK").expect("supply new-profile pack"),
    );
    let saved = PathBuf::from(
        std::env::var_os("POBR_NATIVE_PASSIVE_REQUEST").expect("supply bound baseline document"),
    );
    let request = NativeBuildRequest::from_json(&std::fs::read(saved).unwrap()).unwrap();
    assert!(request.build.tree.allocated_nodes.is_empty());
    assert_eq!(
        request.actor_policy.id,
        "selected-player-generic-ot-ordinary-damage-passives-v1"
    );
    let data = GameData::new(&pack);
    let directory =
        std::env::temp_dir().join(format!("pobr-cli-native-passives-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let document = directory.join("saved.json");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_pobr"))
            .arg("calculate-native")
            .arg("--data-dir")
            .arg(&pack)
            .arg("--file")
            .arg(&document)
            .output()
            .expect("launch actual CLI")
    };
    let mut baseline = None;
    for (ids, mean) in [
        (vec![], 5.995),
        (vec![4739], 6.5945),
        (vec![4739, 18845], 7.194),
        (vec![4739, 20649, 22821, 33053], 8.393),
        (vec![], 5.995),
    ] {
        let mut edited = request.clone();
        edited.build.tree.allocated_nodes = ids.iter().copied().map(NodeId).collect();
        std::fs::write(&document, edited.to_json().unwrap()).unwrap();
        let reopened = std::fs::read(&document).unwrap();
        let expected: Value =
            serde_json::from_str(&calculate_native_document(&data, &reopened).unwrap()).unwrap();
        for _ in 0..2 {
            let output = run();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(actual, expected, "fresh CLI/shared result equality");
            assert!((actual["conditional_weighted_mean"].as_f64().unwrap() - mean).abs() < 1e-10);
            assert_eq!(actual["critical_probability"], 0.09);
            assert_eq!(actual["critical_multiplier"], 2.0);
            assert_eq!(actual["analytical_base_use_ms"], 700);
            assert_eq!(actual["resource_costs"][0]["amount_per_use"], 5.0);
            assert_eq!(actual["total_projectiles"], 4.0);
            assert_eq!(actual["projectile_lifetime_ms"], 2000.0);
            let nodes: Vec<_> = actual["modifiers"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["runtime_source"]["artifact"] == "base/passive_tree.json")
                .collect();
            assert_eq!(nodes.len(), ids.len());
            assert!(
                nodes
                    .iter()
                    .all(|m| m["counted"] == true && m["origin"]["native_stat"].is_object())
            );
            if ids.is_empty() {
                if let Some(before) = &baseline {
                    assert_eq!(&actual, before, "remove/reopen restores baseline");
                } else {
                    baseline = Some(actual);
                }
            }
        }
    }
    for ids in [vec![4739, 4739], vec![4739, u32::MAX]] {
        let mut invalid = request.clone();
        invalid.build.tree.allocated_nodes = ids.into_iter().map(NodeId).collect();
        std::fs::write(&document, invalid.to_json().unwrap()).unwrap();
        assert!(!run().status.success(), "invalid edit fails as a whole");
        std::fs::write(&document, request.to_json().unwrap()).unwrap();
        let output = run();
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            baseline.clone().unwrap()
        );
    }
    let mut stale = request;
    stale.actor_policy.id = "selected-player-generic-ot-v1".into();
    std::fs::write(&document, stale.to_json().unwrap()).unwrap();
    assert!(
        !run().status.success(),
        "old policy cannot widen into new profile"
    );
}
