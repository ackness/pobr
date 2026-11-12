//! Runtime version discovery + the user patch layer (wrapping up the
//! data/code separation).

use pobr_gamedata::{GameData, current_data_dir, data_version};

fn tree_data(version: &str, compatibility: Option<&str>) -> GameData {
    let mut files = std::collections::BTreeMap::from([(
        "manifest.json".into(),
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 2, "poe_version": version, "languages": [], "domains": []
        }))
        .unwrap(),
    )]);
    if let Some(label) = compatibility {
        files.insert(
            "overlay/passive_jewels.json".into(),
            serde_json::to_vec(&serde_json::json!({ "tree_version": label })).unwrap(),
        );
    }
    GameData::from_memory(files)
}

#[test]
fn tree_labels_cover_all_shipped_trees() {
    for (version, expected) in [
        ("0.1.0", "0_1"),
        ("0.2.0", "0_2"),
        ("0.3.0", "0_3"),
        ("0.4.0", "0_4"),
        ("0.5.5.5", "0_5"),
    ] {
        assert_eq!(tree_data(version, None).tree_version().unwrap(), expected);
        assert_eq!(
            tree_data(version, Some(expected)).tree_version().unwrap(),
            expected
        );
    }
    let game = GameData::new(current_data_dir());
    assert_eq!(
        game.tree_version().unwrap(),
        game.passive_jewels().unwrap().unwrap().tree_version
    );
    // Keep the explicit historical cases tied to the package inventory.
    let known = ["0_1", "0_2", "0_3", "0_4", "0_5"];
    for label in game.available_tree_versions() {
        assert!(
            known.contains(&label.as_str()),
            "missing table regression for {label}"
        );
    }
}

#[test]
fn unknown_game_versions_are_named_errors_even_with_a_compatibility_label() {
    for version in ["1.0.0.1", "0.10.0", "0.50.1", "invalid"] {
        for compatibility in [None, Some("0_5")] {
            let error = tree_data(version, compatibility)
                .tree_version()
                .unwrap_err();
            assert!(matches!(
                error,
                pobr_gamedata::LoadError::UnknownTreeVersion { .. }
            ));
            assert!(error.to_string().starts_with("unknown_tree_version:"));
        }
    }
}

#[test]
fn compatibility_disagreement_is_reported() {
    let error = tree_data("0.5.5.5", Some("0_4"))
        .tree_version()
        .unwrap_err();
    assert!(matches!(
        error,
        pobr_gamedata::LoadError::TreeVersionMismatch { .. }
    ));
    assert!(error.to_string().starts_with("tree_version_mismatch:"));
}

/// Runtime version discovery: the environment override precedes `data/CURRENT`;
/// `current_data_dir()` points at that version's directory.
#[test]
fn data_version_resolves_and_dir_matches() {
    let v = data_version();
    assert!(!v.trim().is_empty(), "data_version should not be empty");
    let expected = std::env::var("POBR_DATA_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            std::fs::read_to_string(pobr_gamedata::repo_data_root().join("CURRENT"))
                .expect("repository data/CURRENT")
                .lines()
                .next()
                .expect("nonempty data/CURRENT")
                .to_string()
        });
    assert_eq!(v, expected.trim());
    assert!(
        current_data_dir().ends_with(&v),
        "current_data_dir should end with the discovered version: {:?}",
        current_data_dir()
    );
}

/// The user patch layer: `patch/<relative path>` is layered on top of the
/// official data per the merge rules — same id overrides, new id
/// appends; no patch directory = pure official data.
#[test]
fn user_patch_layer_merges_over_base() {
    let tmp = std::env::temp_dir().join(format!("pobr-patch-test-{}", std::process::id()));
    let base = tmp.join("base");
    let patch_base = tmp.join("patch").join("base");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&patch_base).unwrap();

    // Official base: Mana(divisor=1) + Life(divisor=1)
    std::fs::write(
        base.join("cost_types.json"),
        r#"[{"id":"Mana","divisor":1},{"id":"Life","divisor":1}]"#,
    )
    .unwrap();

    // No patch: pure official.
    let plain = GameData::new(&tmp)
        .cost_types()
        .expect("cost_types without patch");
    assert_eq!(plain.len(), 2);
    assert_eq!(plain.iter().find(|c| c.id == "Mana").unwrap().divisor, 1);

    // User patch: overrides Mana's divisor + appends a custom resource, Custom.
    std::fs::write(
        patch_base.join("cost_types.json"),
        r#"[{"id":"Mana","divisor":99},{"id":"Custom","divisor":7}]"#,
    )
    .unwrap();

    let patched = GameData::new(&tmp)
        .cost_types()
        .expect("cost_types with patch");
    assert_eq!(
        patched.len(),
        3,
        "should be three entries: Mana/Life/Custom"
    );
    assert_eq!(
        patched.iter().find(|c| c.id == "Mana").unwrap().divisor,
        99,
        "patch should override Mana's divisor"
    );
    assert_eq!(
        patched.iter().find(|c| c.id == "Life").unwrap().divisor,
        1,
        "unpatched Life should be unchanged"
    );
    assert!(
        patched.iter().any(|c| c.id == "Custom" && c.divisor == 7),
        "patch should append the custom Custom entry"
    );

    std::fs::remove_dir_all(&tmp).ok();
}
