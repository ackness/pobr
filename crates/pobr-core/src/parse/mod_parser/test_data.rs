//! Shared runtime data discovery for tests and benchmarks only.

use std::path::PathBuf;

/// Resolve the active test snapshot without embedding data/CURRENT into a crate.
/// Keep the environment precedence aligned with pobr-gamedata's runtime loader.
pub fn test_data_dir() -> PathBuf {
    let root = std::env::var_os("POBR_DATA_ROOT")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data"));
    let version = std::env::var("POBR_DATA_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string(root.join("CURRENT"))
                .ok()
                .and_then(|text| text.lines().next().map(str::to_owned))
                .filter(|v| !v.trim().is_empty())
        })
        .unwrap_or_else(|| panic!("no data version: set POBR_DATA_VERSION or write data/CURRENT"));
    root.join(version.trim())
}

/// The imported per-level monster table (`base/monster_scaling.json`) for
/// tests that compute enemies or minions without loading `GameData`: the
/// table has no Rust copy and `RuntimeConstants::default()` leaves it empty.
#[allow(dead_code)]
pub fn test_monster_scaling() -> pobr_data::catalog::monster_scaling::MonsterScalingTable {
    let path = test_data_dir().join("base/monster_scaling.json");
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e} (run `mise run data:prepare`)", path.display()));
    serde_json::from_str(&json).expect("deserialize base/monster_scaling.json")
}

/// `RuntimeConstants::default()` with the imported per-level monster table
/// ([`test_monster_scaling`]) for tests that compute enemies or minions.
#[allow(dead_code)]
pub fn test_constants() -> pobr_data::catalog::RuntimeConstants {
    pobr_data::catalog::RuntimeConstants {
        monster_scaling: test_monster_scaling(),
        ..Default::default()
    }
}
