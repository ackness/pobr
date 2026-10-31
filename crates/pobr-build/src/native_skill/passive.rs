//! Resolve explicitly selected graph instances without legacy tree or text ingestion.

use std::collections::{BTreeMap, BTreeSet};

use pobr_core::Modifier;
use pobr_core::ingest::passive::{self, AllocatedNode, NativePassiveInput};
use pobr_data::catalog::{NativeRuntimeContract, PassiveNodeDef, PassiveNodeKind};
use pobr_data::modifier::{KeywordFlags, ModType};
use pobr_data::passive_tree::NodeId;
use pobr_data::source::SourceKind;
use pobr_gamedata::NativeData;
use serde_json::Value;

use super::{
    ModifierFactBinding, NativeAdmissionError, NativeArtifactReference, reference, source_error,
    unsupported,
};

const TREE: &str = "base/passive_tree.json";
const RESOURCES: [&str; 2] = ["tables/PassiveSkills.json", "tables/Stats.json"];

#[derive(Debug)]
pub(super) struct Selected {
    pub modifier: Modifier,
    pub binding: ModifierFactBinding,
    pub selection_source: NativeArtifactReference,
}

pub(super) fn resolve(
    data: &NativeData,
    requested: &[NodeId],
) -> Result<Vec<Selected>, NativeAdmissionError> {
    let context = data
        .player_context()
        .ok_or_else(|| source_error(TREE, "missing exact tree-parent source inventory"))?;
    let bytes = context
        .source_archive
        .get("parent-manifest.json")
        .ok_or_else(|| source_error(TREE, "missing sealed parent manifest"))?;
    // NativeData already verified these retained bytes against the exact parent pin.
    // Only resource identities are read here; no raw table I/O or source decoding runs.
    let parent: Value = serde_json::from_str(bytes)
        .map_err(|error| source_error(TREE, format!("invalid parent inventory: {error}")))?;
    let hash = |resource: &str| {
        parent["files"][resource]["sha256"]
            .as_str()
            .ok_or_else(|| source_error(TREE, format!("missing parent resource {resource}")))
    };
    let hashes = [hash(RESOURCES[0])?, hash(RESOURCES[1])?];
    resolve_nodes(data.passive_nodes(), data.contract(), requested, hashes)
}

// The loader independently binds the full sealed reference index to these packets.
// Keeping this numerical admission separate also permits portable synthetic tests.
pub(super) fn resolve_nodes(
    nodes: &[PassiveNodeDef],
    contract: &NativeRuntimeContract,
    requested: &[NodeId],
    resource_hashes: [&str; 2],
) -> Result<Vec<Selected>, NativeAdmissionError> {
    let mut seen = BTreeSet::new();
    for id in requested {
        if !seen.insert(id.0) {
            return Err(unsupported(
                "build.tree.allocated_nodes",
                format!("duplicate requested graph ID {}", id.0),
            ));
        }
    }
    seen.clear();
    for (index, node) in nodes.iter().enumerate() {
        if !seen.insert(node.skill) {
            return Err(source_error(
                format!("{TREE}#/{index}"),
                format!("ambiguous duplicate catalog graph ID {}", node.skill),
            ));
        }
    }
    let graph: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.skill, (index, node)))
        .collect();
    let mut selected_ids: Vec<_> = requested.iter().map(|id| id.0).collect();
    selected_ids.sort_unstable();
    // Sorting affects numerical addition/evidence only; the owned request is untouched.
    selected_ids
        .into_iter()
        .map(|id| {
            let &(index, node) = graph.get(&id).ok_or_else(|| {
                unsupported(
                    "build.tree.allocated_nodes",
                    format!("unknown graph ID {id}"),
                )
            })?;
            select(node, index, contract, resource_hashes)
        })
        .collect()
}

fn select(
    node: &PassiveNodeDef,
    index: usize,
    contract: &NativeRuntimeContract,
    resource_hashes: [&str; 2],
) -> Result<Selected, NativeAdmissionError> {
    let path = format!("{TREE}#/{index}/native_stats");
    let reject = |reason: &str| {
        source_error(
            &path,
            format!("graph {} definition {}: {reason}", node.skill, node.id),
        )
    };
    if node.kind != PassiveNodeKind::Normal
        || node.ascendancy_id.is_some()
        || node.unlock_constraint.is_some()
        || !node.variants.is_empty()
        || node.apply_to_armour
    {
        return Err(reject(
            "transformed, choice, ascendancy or non-ordinary node",
        ));
    }
    let packet = node
        .native_stats
        .as_ref()
        .ok_or_else(|| reject("native packet absent; text fallback is forbidden"))?;
    if packet.data_version != contract.game_patch
        || packet.tree_version.as_deref() != Some(contract.tree.compatibility_tag.as_str())
        || packet.node_id != node.skill
        || packet.node_slug != node.id
        || node.id.is_empty()
        || packet.modifier_texts != node.stats
    {
        return Err(reject(
            "packet patch/tree tag/graph/definition/text binding mismatch",
        ));
    }
    let [stat] = packet.stats.as_slice() else {
        return Err(reject(
            "complete single-stat packet required; text fallback is forbidden",
        ));
    };
    let provenance = &stat.provenance;
    if provenance.game_patch != contract.game_patch
        || provenance.source_manifest_sha256 != contract.tree.source_manifest_sha256
        || provenance.passive_id != node.id
        || provenance.references.len() != 2
    {
        return Err(reject(
            "stat patch/exact tree-parent/definition/reference binding mismatch",
        ));
    }
    let pointers = [
        format!("/rows/{}/values/Stat1Value", provenance.passive_row),
        format!("/rows/{}/values/Id", provenance.stat_row),
    ];
    for (i, resource) in RESOURCES.iter().enumerate() {
        let source = &provenance.references[i];
        if source.resource != *resource
            || source.pointer != pointers[i]
            || source.sha256 != resource_hashes[i]
            || source.sha256.len() != 64
            || !source
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(reject(&format!(
                "stat {} has invalid exact source reference {resource}#{}",
                stat.id, pointers[i]
            )));
        }
    }
    let allocated = AllocatedNode {
        node_id: NodeId(node.skill),
        ascendancy: false,
        modifier_texts: node.stats.clone(),
        native: Some(NativePassiveInput {
            packet: packet.clone(),
            ineligible: None,
        }),
    };
    let modifier = match passive::select_native_passive(&allocated) {
        Some(Ok(modifier)) => modifier,
        Some(Err(diagnostic)) => {
            return Err(reject(&format!(
                "stat {} at {}#{} rejected by owned selector: {:?}; text fallback is forbidden",
                stat.id,
                provenance.references[0].resource,
                provenance.references[0].pointer,
                diagnostic.reason,
            )));
        }
        None => {
            return Err(reject(
                "owned selector returned no native modifier; text fallback is forbidden",
            ));
        }
    };
    if modifier.mod_type != ModType::Inc
        || !matches!(
            modifier.name.as_str(),
            "PhysicalDamage"
                | "FireDamage"
                | "ColdDamage"
                | "LightningDamage"
                | "ChaosDamage"
                | "ElementalDamage"
                | "SpellDamage"
                | "ProjectileDamage"
        )
        || !modifier.value.as_number().is_some_and(f64::is_finite)
        || !modifier.flags.is_empty()
        || modifier.keyword_flags != KeywordFlags::NONE
        || !modifier.tags.is_empty()
        || !modifier.origin.as_ref().is_some_and(|origin| {
            origin.source_id.kind == SourceKind::PassiveNode
                && origin.source_id.id == format!("node.{}", node.skill)
                && origin.native_stat.as_ref() == Some(stat)
                && origin.stat_id.as_ref() == Some(&modifier.name)
                && origin.mod_type == Some(modifier.mod_type)
        })
    {
        return Err(reject(&format!(
            "stat {} is outside numeric tag/flag-free damage INC vocabulary",
            stat.id
        )));
    }
    let binding = ModifierFactBinding::new(
        &modifier,
        &stat.id,
        reference(TREE, format!("/{index}/native_stats/stats/0")),
    )?;
    Ok(Selected {
        modifier,
        binding,
        selection_source: reference(TREE, format!("/{index}")),
    })
}
