//! A table staged to release what its loaders have parsed lets go of the bytes, and nothing
//! else about the data changes: identity, inventory and every other file read the same.

use std::collections::BTreeMap;

use pobr_gamedata::GameData;
use serde_json::json;
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn pack() -> BTreeMap<String, Vec<u8>> {
    let costs = br#"[{"id":"Mana","divisor":1}]"#.to_vec();
    let stats = b"[]".to_vec();
    let manifest = json!({
        "schema_version": 3, "poe_version": "1.2.3", "languages": [],
        "domains": { "base": ["stats", "cost_types"], "overlay": [], "generated": [] },
        "files": { "base/cost_types.json": hash(&costs), "base/stats.json": hash(&stats) }
    });
    BTreeMap::from([
        (
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        ),
        ("base/cost_types.json".into(), costs),
        ("base/stats.json".into(), stats),
    ])
}

#[test]
fn a_released_file_is_not_read_again_and_the_rest_of_the_pack_is_unchanged() {
    let keeping = GameData::from_memory(pack());
    let releasing = GameData::from_memory_releasing(pack(), false);
    assert_eq!(releasing.cost_types().unwrap()[0].divisor, 1);
    releasing.release_domain("cost_types.json");
    // Another file of the pack, and the pack's identity, are as they were.
    assert!(releasing.stats().unwrap().is_empty());
    assert_eq!(releasing.identity().unwrap(), keeping.identity().unwrap());
    // The released one is a failure, never a file that is merely absent.
    let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| releasing.cost_types()));
    match read {
        Err(_) => assert!(cfg!(debug_assertions), "only a debug build panics"),
        Ok(result) => {
            let error = result
                .expect_err("a released file cannot be read")
                .to_string();
            assert!(error.contains("released"), "{error}");
        }
    }
    // A table that was not staged to release keeps its files, whatever is asked of it.
    keeping.release_domain("cost_types.json");
    assert_eq!(keeping.cost_types().unwrap()[0].divisor, 1);
}

#[test]
fn identity_is_settled_before_a_file_is_released() {
    // The identity digest reads every file; asked for only after a release, it must
    // still be the pack's.
    let expected = GameData::from_memory(pack()).identity().unwrap().clone();
    for prechecked in [false, true] {
        let releasing = GameData::from_memory_releasing(pack(), prechecked);
        releasing.release_domain("stats.json");
        releasing.release_domain("cost_types.json");
        assert_eq!(
            releasing.identity().unwrap(),
            &expected,
            "prechecked: {prechecked}"
        );
    }
}
