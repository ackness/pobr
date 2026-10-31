//! PoBR-owned intermediate domain model; raw evidence stays in the sealed RePoE input.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::core_skills::{CoreSkills, CoreSummary, StableRef};

/// References are resolved only against the input manifest pinned in the candidate receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct InputRef {
    pub payload: String,
    pub pointer: String,
}

impl InputRef {
    pub fn row(table: &str, row: usize) -> Self {
        Self {
            payload: format!("tables/{table}.json"),
            pointer: format!("/rows/{row}/values"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Candidate {
    pub format: String,
    pub input_manifest_sha256: String,
    pub summary: Summary,
    // Maps provide stable-ID lookups; separate arrays retain physical definition order.
    pub passive_order: Vec<String>,
    pub passives: BTreeMap<String, Passive>,
    pub graph: Graph,
    pub instances: Vec<Instance>,
    pub class_relations: Vec<ClassRelation>,
    pub effect_order: Vec<String>,
    pub effects: BTreeMap<String, Effect>,
    pub stat_set_order: Vec<String>,
    pub stat_sets: BTreeMap<String, StatSet>,
    pub qualities: Vec<Quality>,
    pub source_diagnostics: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_skills: Option<CoreSkills<StableRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_summary: Option<CoreSummary>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Summary {
    pub passive_definitions: usize,
    pub graph_instances: usize,
    pub graph_stat_entries: usize,
    pub class_overrides: usize,
    pub effects: usize,
    pub qualities: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Passive {
    pub raw: InputRef,
    pub graph_id_raw: u64,
    pub in_default_graph: bool,
    pub stats: Vec<PassiveStat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PassiveStat {
    pub stat_id: String,
    pub raw_stat: InputRef,
    pub slot: usize,
    pub value_field: String,
    pub value: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Graph {
    pub tree_id: String,
    pub raw_tree: InputRef,
    pub raw: InputRef,
    pub header: Vec<u64>,
    pub skills_per_orbit_raw: Vec<u64>,
    pub roots: Vec<Root>,
    pub groups: Vec<Group>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Root {
    pub graph_id: u64,
    pub unknown: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Group {
    pub x_raw: f64,
    pub y_raw: f64,
    pub flag_raw: u64,
    pub unknown1: u64,
    pub unknown2: i64,
    pub node_order: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Instance {
    pub graph_id: u64,
    pub definition_id: String,
    pub group: usize,
    pub radius_raw: u64,
    pub position_raw: u64,
    pub connections: Vec<u64>,
    pub splines: Vec<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ClassRelation {
    pub raw: InputRef,
    pub character_id: String,
    pub raw_character: InputRef,
    pub base_definition_id: String,
    pub replacement_definition_id: String,
    pub base_graph_id: u64,
    pub replacement_graph_id: u64,
    pub base_in_default_graph: bool,
    pub replacement_in_default_graph: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Effect {
    pub raw: InputRef,
    pub ordered_stat_sets: Vec<StatSetLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct StatSetLink {
    pub stat_set_id: String,
    pub source_field: String,
    pub source_slot: Option<usize>,
    pub local_index_hypothesis: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StatSet {
    pub raw: InputRef,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Quality {
    pub raw: InputRef,
    pub effect_id: String,
    // Ordered sets are joined through effect_id; no scope application is inferred.
    pub main: QualityChannel,
    pub alternate: QualityChannel,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct QualityChannel {
    pub scope_raw: Vec<i64>,
    pub stats: Vec<QualityStat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct QualityStat {
    pub stat_id: String,
    pub raw_stat: InputRef,
    pub slot: usize,
    pub value_permille: i64,
}

// The v1 projection is deliberately strict; unknown physical metadata remains accessible
// through InputRef rather than being interpreted or duplicated in this candidate schema.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub row: usize,
    pub id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PassiveInput {
    pub definitions: Vec<DefinitionInput>,
    pub default_graph_instances: Vec<InstanceInput>,
    pub class_overrides: Vec<ClassInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DefinitionInput {
    pub definition: Identity,
    pub graph_id: u64,
    pub in_default_graph: bool,
    pub stats: Vec<PassiveStatInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PassiveStatInput {
    pub slot: usize,
    pub value_field: String,
    pub stat: Identity,
    pub value: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InstanceInput {
    pub definition: Identity,
    pub graph_id: u64,
    pub group: usize,
    pub radius: u64,
    pub position: u64,
    pub connections: Vec<u64>,
    pub splines: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClassInput {
    pub row: usize,
    pub character: Identity,
    pub base: Identity,
    pub replacement: Identity,
    pub base_graph_id: u64,
    pub replacement_graph_id: u64,
    pub base_in_default_graph: bool,
    pub replacement_in_default_graph: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SkillsInput {
    pub effects: Vec<EffectInput>,
    pub qualities: Vec<QualityInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectInput {
    pub effect: Identity,
    pub ordered_stat_sets: Vec<LinkInput>,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkInput {
    pub stat_set: Identity,
    pub source_field: String,
    pub source_slot: Option<usize>,
    pub local_index_hypothesis: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QualityInput {
    pub row: usize,
    pub effect: Identity,
    pub ordered_stat_sets: Vec<LinkInput>,
    pub main: ChannelInput,
    pub alternate: ChannelInput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChannelInput {
    pub scope_raw: Vec<i64>,
    pub stats: Vec<QualityStatInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QualityStatInput {
    pub slot: usize,
    pub stat: Identity,
    pub value_permille: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphInput {
    pub tree: Identity,
    pub raw_resource: String,
    pub header: Vec<u64>,
    pub skills_per_orbit: Vec<u64>,
    pub roots: Vec<Root>,
    pub groups: Vec<GroupInput>,
    pub parsed_bytes: u64,
    pub radius_semantics: String,
    pub warnings: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroupInput {
    pub group: usize,
    pub x: f64,
    pub y: f64,
    pub flag: u64,
    pub unknown1: u64,
    pub unknown2: i64,
    pub nodes: Vec<NodeInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NodeInput {
    pub graph_id: u64,
    pub radius: u64,
    pub position: u64,
    pub connections: Vec<u64>,
    pub splines: Vec<i64>,
}
