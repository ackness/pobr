use std::collections::HashMap;

use super::BuildData;
use pobr_data::catalog::PassiveNodeDef;

impl BuildData {
    /// Selects the passive node table by the build's `<Spec treeVersion>`: uses the
    /// matching historical tree version (`base/passive_trees/<v>.json` already
    /// extracted) if one exists, otherwise (current default version / not extracted /
    /// not annotated) falls back to the default tree [`Self::passive_nodes`] — PoBR's
    /// counterpart to PoB2's multi-version TreeData. Radius geometry uses the selected
    /// tree as well; nodes without coordinates cannot contribute radius effects.
    pub fn passive_nodes_for(&self, tree_version: Option<&str>) -> &HashMap<u32, PassiveNodeDef> {
        tree_version
            .and_then(|v| self.versioned_passive_nodes.get(v))
            .unwrap_or(&self.passive_nodes)
    }
}

impl pobr_core::skill_env::PassiveNodeLookup for BuildData {
    fn passive_node(&self, node_id: u32) -> Option<&PassiveNodeDef> {
        self.passive_nodes.passive_node(node_id)
    }

    fn notable_by_name(&self, name: &str) -> Option<&PassiveNodeDef> {
        self.passive_nodes.notable_by_name(name)
    }
}
