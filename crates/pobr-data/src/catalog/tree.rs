//! Passive tree domain schema (`base/passive_tree.json` /
//! `base/passive_tree_meta.json`, sourced from GGG's official tree export
//! `data.json`).

use serde::{Deserialize, Serialize};

/// The kind of a passive node.
///
/// Derived from the boolean flags in GGG's official tree export
/// (`poe2-skilltree-export/data.json`): `isKeystone` / `isNotable` /
/// `isMastery` / `isJewelSocket` / `isAscendancyStart`; otherwise it's
/// [`PassiveNodeKind::Normal`] (a small passive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveNodeKind {
    /// A small passive (a plain stat node).
    Normal,
    /// A notable.
    Notable,
    /// A keystone.
    Keystone,
    /// A mastery node.
    Mastery,
    /// A jewel socket.
    JewelSocket,
    /// An ascendancy start node.
    AscendancyStart,
}

/// A per-class/ascendancy variant of an isSwitchable node (vendor
/// `tree.lua` node's `options.<Class>`).
///
/// PoB2 `PassiveSpec.lua:1251-1256`: when a node has `isSwitchable`, if
/// `options[curClassName]` (or failing that, `options[curAscendClassName]`)
/// exists, that option **wholly replaces** the node's mods (`ReplaceNode`
/// semantics — stats aren't merged). The variant itself carries its own
/// alias id (e.g. the Witch variant "Jagged Shards" = 64801), but tree
/// connections / the Build Code still reference the base node's `skill`
/// id — the alias is display/cross-reference only.
///
/// Only options that carry **their own `stats`** are stored (purely
/// cosmetic options inherit the base stats through Lua's `__index`
/// metatable and behave identically to the base version, so they aren't
/// stored).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveNodeVariant {
    /// The option key: a class name (e.g. `Witch`/`Druid`/`Huntress`) or an
    /// ascendancy name (e.g. `Abyssal Lich`).
    pub class: String,
    /// The variant's own node alias id (vendor `option.id`, e.g. 64801);
    /// for display/cross-reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_skill: Option<u32>,
    /// Variant name (English canonical, e.g. `Jagged Shards`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The variant's mods (wholly replaces the base `stats`, PoB's
    /// `ReplaceNode` semantics).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<String>,
}

/// A passive tree node definition (from the `nodes` of GGG's official tree
/// export `data.json`).
///
/// Calc internals only use stable IDs: `id` is GGG's string slug (e.g.
/// `passive_keystone_avatar_of_fire`), and `skill` is the numeric skill id
/// (the stable numeric key referenced by tree connections / the Build
/// Code). `stats` are the English mod-text lines the node grants (input for
/// PoB-compatible parsing). `connections` are the target `skill` ids of
/// this node's outgoing edges (the tree is undirected, so outgoing edges
/// alone are enough to reconstruct adjacency).
///
/// Note: this has floating-point `x`/`y` coordinate fields, so it doesn't
/// derive `Eq`/`Hash` (only `PartialEq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassiveNodeDef {
    /// Numeric skill id (GGG `nodes`'s map key / `skill` field). Referenced
    /// by tree connections and the Build Code.
    pub skill: u32,
    /// String slug (GGG's `id` field, e.g. `passive_keystone_avatar_of_fire`).
    pub id: String,
    /// Node name (English canonical).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Node kind.
    pub kind: PassiveNodeKind,
    /// The mod-text lines this node grants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stats: Vec<String>,
    /// The node's group (GGG's `group`, used for coordinates/layout;
    /// irrelevant to calc, kept for comparing against PoB2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// Ring number on its orbit (GGG's `orbit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit: Option<u32>,
    /// Angular slot on its orbit (GGG's `orbitIndex`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit_index: Option<u32>,
    /// The node's planar x coordinate (tree units). Derived from
    /// `group.(x,y)` plus the orbit radius/angle, matching PoB2
    /// `PassiveTree.lua`'s
    /// `node.x = group.x + sin(angle) * orbitRadii[orbit]`. The old catalog
    /// didn't have this field, so `#[serde(default)]` keeps it compatible;
    /// jewel-radius geometry calc depends on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    /// The node's planar y coordinate (tree units). Matches PoB2's
    /// `node.y = group.y - cos(angle) * orbitRadii[orbit]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    /// The `skill` ids of outgoing-edge target nodes (GGG's `out`, already
    /// converted from string keys to numbers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<u32>,
    /// The ascendancy this node belongs to (GGG's `ascendancyId`, e.g.
    /// `Warrior3`); `None` for main-tree nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascendancy_id: Option<String>,
    /// Per-class/ascendancy variants for an isSwitchable node (vendor
    /// `tree.lua`'s `options`, backfilled by
    /// `pobr-data-adapter --tree-variants`). The old catalog didn't have
    /// this field, so `#[serde(default)]` keeps it compatible; always empty
    /// for non-switchable nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<PassiveNodeVariant>,
    /// Marks a notable that Smith of Kitava's body-armour connection bonus
    /// counts (vendor `tree.lua`'s `applyToArmour`, backfilled by
    /// `pobr-data-adapter --tree-coords`): the count of allocated ones feeds
    /// `Multiplier:AllocatedConnectedNotable` (vendor
    /// CalcSetup.lua:840-841, consumed by Masterwork's "+200 to Armour for
    /// each Connected Notable …").
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub apply_to_armour: bool,
}

/// A summary of one class's ascendancy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveAscendancy {
    /// Stable ascendancy ID (matches a node's `ascendancy_id`, e.g. `Warrior3`).
    pub id: String,
    /// Ascendancy name (English canonical, e.g. `Smith of Kitava`).
    pub name: String,
}

/// A summary of one class (base attributes + ascendancy list).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveClass {
    /// Class name (English canonical, e.g. `Warrior`).
    pub name: String,
    /// Base strength / dexterity / intelligence.
    pub base_str: i32,
    pub base_dex: i32,
    pub base_int: i32,
    /// This class's ascendancy summaries (unnamed placeholder entries filtered out).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ascendancies: Vec<PassiveAscendancy>,
}

/// Metadata summary for the passive tree (classes / ascendancies / tree name).
///
/// Orbit radii / slots-per-ring (PoB's `constants`) aren't given as a
/// separate `constants` block in the current GGG PoE2 export (coordinates
/// are baked directly into each node's `x`/`y`), so this slice doesn't
/// store them — see the TODO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveTreeMeta {
    /// Tree identifier (GGG's `tree`, e.g. `Default`).
    pub tree: String,
    /// Class + ascendancy summaries (sorted by class name).
    pub classes: Vec<PassiveClass>,
}
