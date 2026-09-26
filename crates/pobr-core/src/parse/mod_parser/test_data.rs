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
        .unwrap_or_else(|| pobr_data::GOLDEN_PARITY_DATA_VERSION.to_owned());
    root.join(version.trim())
}
