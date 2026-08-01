//! Runtime version discovery + the user patch layer (wrapping up the
//! data/code separation).

use pobr_gamedata::{GameData, current_data_dir, data_version};

/// Runtime version discovery: falls back to the `data/CURRENT` marker when
/// there's no env var (in the repo, this equals DATA_VERSION);
/// `current_data_dir()` points at that version's directory.
#[test]
fn data_version_resolves_and_dir_matches() {
    let v = data_version();
    assert!(!v.trim().is_empty(), "data_version should not be empty");
    // The repo's data/CURRENT matches the compile-time DATA_VERSION (unchanged behavior).
    assert_eq!(v, pobr_gamedata::DATA_VERSION);
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
