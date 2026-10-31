//! A queryable native candidate must not reach legacy defaults or text rules.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_build::BuildData;
use pobr_gamedata::{GameData, LoadError};

#[test]
fn native_runtime_build_data_rejects_unadmitted_profile_before_domain_fallbacks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../pobr-gamedata/tests/fixtures/native_runtime");
    let manifest_bytes = std::fs::read(root.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let mut files: BTreeMap<_, _> = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(|file| (file.clone(), std::fs::read(root.join(file)).unwrap()))
        .collect();
    files.insert("manifest.json".into(), manifest_bytes);

    for data in [
        GameData::new(root),
        GameData::from_memory(files.clone()),
        GameData::from_memory_prechecked(files.clone()),
        GameData::from_memory_releasing(files, true),
    ] {
        data.validate_manifest()
            .expect("valid synthetic native envelope");
        data.identity()
            .expect("native byte identity available to the assembler");
        assert!(data.native_runtime().unwrap().is_some());
        // No game/character constants or parser catalog is present. An error naming
        // admission proves these cannot become apparently certified fallback values.
        let error = BuildData::load(&data).unwrap_err();
        assert!(
            matches!(error, LoadError::Integrity { ref path, ref message }
            if path.ends_with("base/native_runtime.json")
                && message.starts_with("native-profile-not-admitted:")
                && message.contains("constants and rule closure")
                && message.contains("dataset/save binding")
                && message.contains("stable branch admission")),
            "{error}"
        );
        // Rejection occurs before releasing any domain needed for assembler queries.
        assert!(data.passive_nodes().unwrap().is_empty());
    }
}
