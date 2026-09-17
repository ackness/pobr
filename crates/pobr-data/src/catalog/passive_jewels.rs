//! Data extracted from PoB2 for passive allocation and deterministic jewel transformations.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PassiveJewelData {
    pub tree_version: String,
    /// Class name -> stable tree node ID, not a version-specific numeric node ID.
    pub class_starts: BTreeMap<String, String>,
    /// Canonical modifier text -> one-based index in the injected radius table.
    pub ring_sizes: BTreeMap<String, usize>,
    pub conquerors: BTreeMap<String, JewelConqueror>,
    pub nodes: BTreeMap<String, JewelPassive>,
    pub families: BTreeMap<String, JewelConquestFamily>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JewelConqueror {
    pub family: String,
    pub keystone: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JewelConquestFamily {
    pub small_replacement: Option<String>,
    #[serde(default)]
    pub attribute_additions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JewelPassive {
    pub name: String,
    pub stats: Vec<String>,
}
