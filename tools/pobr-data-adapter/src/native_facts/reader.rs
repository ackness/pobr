//! Selected physical facts for ordinary domain producers; no runtime/profile projection.

use std::path::Path;

use super::{Args, Profile, trust};

pub(crate) use super::convert::Tables;
pub(crate) use super::trust::VerifiedInput as VerifiedFacts;

/// Authenticate the existing v4 envelope, lock and selected payload bytes before parsing.
/// Unselected payloads are not read or certified. The caller supplies the external pin.
pub(crate) fn open(
    input: &Path,
    expected_manifest_sha256: &str,
    version: &str,
    tables: &[&str],
) -> Result<VerifiedFacts, String> {
    let facts = trust::verify_selection(
        &Args {
            input: input.into(),
            expected_manifest_sha256: expected_manifest_sha256.into(),
            source_lock: input.join("source-lock.json"),
            out: Default::default(),
            profile: Profile::ItemsActorsBc,
        },
        Some(tables),
    )?;
    trust::require(
        facts.manifest["source_identity"]["game_patch"] == version,
        "target version differs from facts game_patch",
    )?;
    Ok(facts)
}

/// Serialize receipts deterministically with the existing canonical source-lock encoding.
pub(crate) fn json_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, String> {
    super::json_bytes(value)
}

/// Refuse existing outputs, including dangling symlinks.
pub(crate) fn require_absent(path: &Path) -> Result<(), String> {
    trust::require_absent(path)
}

/// Reuse the envelope decoder for pinned auxiliary policy JSON, including duplicate-key rejection.
pub(crate) fn strict_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    trust::strict_json(bytes)
}
