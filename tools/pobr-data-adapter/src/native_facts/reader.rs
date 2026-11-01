//! Selected physical facts for ordinary domain producers; no runtime/profile projection.

use std::path::Path;

use super::{Args, Profile, trust};

pub(crate) mod selected;

pub(crate) use super::convert::Tables;
pub(crate) use super::model::{Graph, InstanceInput};
pub(crate) use super::trust::VerifiedInput as VerifiedFacts;

/// Read only the two additional authenticated tree projections, never all domains.
pub(crate) fn read_tree(input: &Path, facts: &mut VerifiedFacts) -> Result<(), String> {
    let mut authenticated = Vec::new();
    for path in ["graph.json", "passives.json"] {
        let bytes = trust::read_regular(&input.join(path))?;
        let fp = &facts.manifest["files"][path];
        trust::require(
            fp["bytes"] == bytes.len() && fp["sha256"] == crate::raw_source::sha256_hex(&bytes),
            &format!("tree payload fingerprint mismatch: {path}"),
        )?;
        authenticated.push((path, bytes));
    }
    for (path, bytes) in authenticated {
        facts.documents.insert(path.into(), strict_json(&bytes)?);
        facts.bytes.insert(path.into(), bytes);
    }
    Ok(())
}

/// Reuse the physical identity/topology checks of the existing domain converter.
pub(crate) fn tree(facts: &VerifiedFacts) -> Result<(Graph, Vec<InstanceInput>), String> {
    let passives: super::model::PassiveInput =
        serde_json::from_value(facts.documents["passives.json"].clone())
            .map_err(|e| e.to_string())?;
    let graph =
        serde_json::from_value(facts.documents["graph.json"].clone()).map_err(|e| e.to_string())?;
    let tables = Tables::selected(facts, &["PassiveSkills", "PassiveSkillTrees"])?;
    let graph = super::convert::normalize_graph(graph, &passives, &tables, facts)?;
    Ok((graph, passives.default_graph_instances))
}

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
