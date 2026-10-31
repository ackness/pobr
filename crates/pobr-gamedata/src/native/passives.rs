//! Internal consistency of emitted packets and their sealed parent reference index.
//!
//! Raw context eligibility remains the assembler's whole-row comparison. This check
//! neither opens raw sources nor authenticates their acquisition or gameplay meaning.

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::native_player::NativePlayerContextDef;
use pobr_data::catalog::{NativeRuntimeContract, PassiveNodeDef, PassiveNodeKind};
use pobr_data::native_stat::NativePassiveContext;
use serde_json::Value;

use crate::{GameData, LoadError};

pub(super) fn validate(
    data: &GameData,
    contract: &NativeRuntimeContract,
    nodes: &[PassiveNodeDef],
    index: &Value,
    context: &NativePlayerContextDef,
) -> Result<(), LoadError> {
    let fail =
        |message: &str| super::invalid(data, format!("native passive reference index: {message}"));
    let require = |condition, message: &str| {
        if condition {
            Ok(())
        } else {
            Err(fail(message))
        }
    };
    let parent: Value = serde_json::from_str(
        context
            .source_archive
            .get("parent-manifest.json")
            .ok_or_else(|| fail("missing retained parent manifest"))?,
    )
    .map_err(|e| fail(&e.to_string()))?;
    require(
        index["format"] == "pobr-native-runtime-reference-index/v2"
            && index["archive"]["manifest_sha256"] == contract.tree.source_manifest_sha256
            && index["archive"]["files"].is_object()
            && index["archive"]["files"] == parent["files"],
        "archive must match the exact tree parent and its resource inventory",
    )?;
    let instances = index["graph_instances"]
        .as_array()
        .ok_or_else(|| fail("missing graph instances"))?;
    let definitions = index["passive_definitions"]
        .as_object()
        .ok_or_else(|| fail("missing passive definitions"))?;
    let mut graph = BTreeMap::new();
    for instance in instances {
        require(
            exact_keys(
                instance,
                &[
                    "graph_id",
                    "definition_id",
                    "group",
                    "radius_raw",
                    "position_raw",
                    "connections",
                    "splines",
                ],
            ),
            "unknown or missing graph instance field",
        )?;
        let id = instance["graph_id"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| fail("invalid graph ID"))?;
        let slug = instance["definition_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| fail("invalid graph definition ID"))?;
        require(graph.insert(id, slug).is_none(), "duplicate graph ID")?;
        require(definitions.contains_key(slug), "graph definition missing")?;
    }
    let mut ids = BTreeSet::new();
    for node in nodes {
        let node_fail =
            |message: &str| fail(&format!("node {} ({}): {message}", node.skill, node.id));
        let check = |condition, message: &str| {
            if condition {
                Ok(())
            } else {
                Err(node_fail(message))
            }
        };
        check(ids.insert(node.skill), "duplicate catalog graph ID")?;
        check(
            graph.get(&node.skill).copied() == Some(node.id.as_str()),
            "graph/definition mismatch",
        )?;
        let Some(packet) = &node.native_stats else {
            continue;
        };
        check(
            packet.node_id == node.skill
                && packet.node_slug == node.id
                && packet.data_version == contract.game_patch
                && packet.tree_version.as_deref() == Some(contract.tree.compatibility_tag.as_str())
                && packet.modifier_texts == node.stats
                && packet.context == NativePassiveContext::UnconditionalPlayer
                && node.kind == PassiveNodeKind::Normal
                && node.variants.is_empty()
                && node.ascendancy_id.is_none()
                && node.unlock_constraint.is_none()
                && !node.apply_to_armour,
            "packet identity, patch, tree tag or ordinary context mismatch",
        )?;
        let [stat] = packet.stats.as_slice() else {
            return Err(node_fail("expected one complete stat packet"));
        };
        let definition = &definitions[&node.id];
        check(
            exact_keys(
                definition,
                &["raw", "graph_id_raw", "in_default_graph", "stats"],
            ),
            "unknown or missing passive definition channel",
        )?;
        let raw_stats = definition["stats"]
            .as_array()
            .ok_or_else(|| node_fail("missing definition stats"))?;
        let [raw_stat] = raw_stats.as_slice() else {
            return Err(node_fail("expected one complete definition stat"));
        };
        check(
            exact_keys(
                raw_stat,
                &["stat_id", "raw_stat", "slot", "value_field", "value"],
            ),
            "unknown or missing stat channel",
        )?;
        let source = &stat.provenance;
        check(
            definition["graph_id_raw"].as_u64() == Some(u64::from(node.skill))
                && definition["in_default_graph"] == true
                && source.passive_id == node.id
                && source.game_patch == contract.game_patch
                && source.source_manifest_sha256 == contract.tree.source_manifest_sha256
                && raw_stat["stat_id"].as_str() == Some(stat.id.as_str())
                && raw_stat["value"].as_i64() == Some(stat.value)
                && raw_stat["slot"].as_u64() == Some(1)
                && raw_stat["value_field"] == "Stat1Value",
            "definition/stat identity, value, slot or source mismatch",
        )?;
        check(
            source.references.len() == 2,
            "expected both distinct source references",
        )?;
        for (raw, resource, row, field) in [
            (
                &definition["raw"],
                "tables/PassiveSkills.json",
                source.passive_row,
                "Stat1Value",
            ),
            (
                &raw_stat["raw_stat"],
                "tables/Stats.json",
                source.stat_row,
                "Id",
            ),
        ] {
            let row_pointer = format!("/rows/{row}/values");
            check(
                exact_keys(raw, &["payload", "pointer"])
                    && raw["payload"] == resource
                    && raw["pointer"] == row_pointer,
                "physical resource/row pointer mismatch",
            )?;
            let hash = index["archive"]["files"][resource]["sha256"]
                .as_str()
                .ok_or_else(|| node_fail("missing resource hash"))?;
            check(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
                "invalid resource hash",
            )?;
            let pointer = format!("{row_pointer}/{field}");
            check(
                source
                    .references
                    .iter()
                    .filter(|r| r.resource == resource && r.pointer == pointer && r.sha256 == hash)
                    .count()
                    == 1,
                "source reference resource/hash/pointer mismatch",
            )?;
        }
    }
    require(
        ids.len() == graph.len(),
        "catalog and index graph IDs differ",
    )
}

fn exact_keys(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
    })
}
