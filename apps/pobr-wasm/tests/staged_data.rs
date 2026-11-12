//! Staging a data pack the way the browser does, as bytes, and the two ways of
//! initializing it: hashing every listed file here, or trusting a caller that did.

use pobr_gamedata::repo_data_root;

#[path = "support/staged_pack.rs"]
mod staged_pack;

fn demo_request() -> String {
    let path =
        repo_data_root().join("../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt");
    let code = std::fs::read_to_string(path).unwrap();
    serde_json::json!({ "pob_code": code.trim(), "breakdowns": [] }).to_string()
}

#[test]
fn bytes_and_text_staging_and_both_inits_build_the_same_data() {
    let files = staged_pack::pack();
    let request = demo_request();

    // Text staging, every file hashed here: the reference.
    for (path, bytes) in files {
        pobr_wasm::stage_data_file(path, std::str::from_utf8(bytes).unwrap());
    }
    pobr_wasm::init_staged_data().expect("hashing init");
    let info: serde_json::Value =
        serde_json::from_str(&pobr_wasm::data_info_json().unwrap()).unwrap();
    let game = pobr_gamedata::GameData::new(pobr_gamedata::current_data_dir());
    assert_eq!(info["data_version"], game.manifest().unwrap().poe_version);
    assert_eq!(info["tree_version"], game.tree_version().unwrap());
    let locales: serde_json::Value =
        serde_json::from_str(&pobr_wasm::available_locales_json().unwrap()).unwrap();
    assert_eq!(info["locales"], locales);
    let reference = pobr_wasm::calculate_build_json(&request).unwrap();

    // Byte staging, digests already compared by the caller. Identity (the digest the
    // response carries as provenance) and the calculation must come out the same.
    staged_pack::init(None);
    let verified = pobr_wasm::calculate_build_json(&request).unwrap();
    assert_eq!(reference, verified);
    assert!(reference.contains("data_sha256"));
}

#[test]
fn the_hashing_init_still_rejects_a_changed_file() {
    let files = staged_pack::pack();
    // Any file the manifest lists, changed by one byte of whitespace.
    let (changed_path, _) = files
        .iter()
        .find(|(path, _)| path == "base/monster_scaling.json")
        .expect("a listed file");
    for (path, original) in files {
        let mut bytes = original.clone();
        if path == changed_path {
            bytes.push(b' ');
        }
        pobr_wasm::stage_data_bytes(path, bytes);
    }
    let error = pobr_wasm::init_staged_data().expect_err("a changed file must not load");
    assert!(error.contains("SHA-256 mismatch"), "{error}");
}

#[test]
fn invalid_tree_versions_cannot_initialize_or_replace_the_active_pack() {
    staged_pack::init(None);
    let before = pobr_wasm::data_info_json().unwrap();
    let manifest = staged_pack::pack()
        .iter()
        .find(|(path, _)| path == "manifest.json")
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest.1).unwrap();
    for init in [
        pobr_wasm::init_staged_data,
        pobr_wasm::init_staged_data_verified,
    ] {
        for code in ["unknown_tree_version", "tree_version_mismatch"] {
            let mut manifest = manifest.clone();
            if code == "unknown_tree_version" {
                manifest["poe_version"] = serde_json::json!("1.0.0.1");
            }
            for (path, bytes) in staged_pack::pack() {
                pobr_wasm::stage_data_bytes(
                    path,
                    if path == "manifest.json" {
                        serde_json::to_vec(&manifest).unwrap()
                    } else {
                        bytes.clone()
                    },
                );
            }
            if code == "tree_version_mismatch" {
                pobr_wasm::stage_data_file(
                    "patch/overlay/passive_jewels.json",
                    r#"{"tree_version":"wrong_tree"}"#,
                );
            }
            let error = init().unwrap_err();
            assert!(error.contains(code), "{error}");
            assert_eq!(pobr_wasm::data_info_json().unwrap(), before);
        }
    }
}
