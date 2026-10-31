//! Staging a data pack the way the browser does, as bytes, and the two ways of
//! initializing it: hashing every listed file here, or trusting a caller that did.

use pobr_gamedata::repo_data_root;

/// Every runtime JSON file of the active data version plus the shared overlay, under the
/// relative paths the browser stages them with.
fn pack() -> Vec<(String, Vec<u8>)> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        prefix: &str,
        out: &mut Vec<(String, Vec<u8>)>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, prefix, out);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((format!("{prefix}{relative}"), std::fs::read(&path).unwrap()));
            }
        }
    }
    let version = pobr_gamedata::current_data_dir();
    let mut files = Vec::new();
    walk(&version, &version, "", &mut files);
    let common = version.parent().unwrap().join("overlay-common");
    walk(&common, &common, "overlay-common/", &mut files);
    files
}

fn demo_request() -> String {
    let path =
        repo_data_root().join("../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt");
    let code = std::fs::read_to_string(path).unwrap();
    serde_json::json!({ "pob_code": code.trim(), "breakdowns": [] }).to_string()
}

fn init_native_v1() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/pobr-gamedata/tests/fixtures/native_runtime");
    pobr_wasm::init_native_data_from_dir(&dir.to_string_lossy()).expect("native v1 catalogs");
    assert!(pobr_wasm::is_native_data_ready());
    assert!(!pobr_wasm::is_data_ready());
}

#[test]
fn bytes_and_text_staging_and_both_inits_build_the_same_data() {
    let files = pack();
    let request = demo_request();

    // Text staging, every file hashed here: the reference.
    init_native_v1();
    for (path, bytes) in &files {
        pobr_wasm::stage_data_file(path, std::str::from_utf8(bytes).unwrap());
    }
    pobr_wasm::init_staged_data().expect("hashing init");
    assert!(!pobr_wasm::is_native_data_ready());
    let reference = pobr_wasm::calculate_build_json(&request).unwrap();

    // Byte staging, digests already compared by the caller. Identity (the digest the
    // response carries as provenance) and the calculation must come out the same.
    init_native_v1();
    let error = pobr_wasm::calculate_build_json(&request).expect_err("old response was cleared");
    let error: serde_json::Value = serde_json::from_str(&error).unwrap();
    assert_eq!(error["code"], "not_initialized");
    for (path, bytes) in &files {
        pobr_wasm::stage_data_bytes(path, bytes.clone());
    }
    pobr_wasm::init_staged_data_verified().expect("verified init");
    assert!(!pobr_wasm::is_native_data_ready());
    let verified = pobr_wasm::calculate_build_json(&request).unwrap();
    assert_eq!(reference, verified);
    assert!(reference.contains("data_sha256"));
}

#[test]
fn the_hashing_init_still_rejects_a_changed_file() {
    let mut files = pack();
    // Any file the manifest lists, changed by one byte of whitespace.
    let (_, bytes) = files
        .iter_mut()
        .find(|(path, _)| path == "base/monster_scaling.json")
        .expect("a listed file");
    bytes.push(b' ');
    for (path, bytes) in &files {
        pobr_wasm::stage_data_bytes(path, bytes.clone());
    }
    let error = pobr_wasm::init_staged_data().expect_err("a changed file must not load");
    assert!(error.contains("SHA-256 mismatch"), "{error}");
}
