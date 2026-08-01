use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeId(pub u32);

/// Which effect the player picked on a mastery node.
///
/// A mastery node offers several effects and the player takes one. This is the
/// `<MasteryEffect skill="N" effect="M">` element of a PoB2 build code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterySelection {
    /// The chosen mod line, verbatim English. Matches one entry of the node's
    /// `PassiveNodeDef::stats`, and is handed to `pobr-core::mod_parser`.
    pub effect_text: String,
}

/// Which attribute the player picked on a `+5 to any Attribute` node.
///
/// Round-trips through `<Overrides><AttributeOverride strNodes/dexNodes/intNodes>`
/// in the build code (`PassiveSpec.lua::SwitchAttributeNode`). An unpicked node
/// grants nothing at all — PoB2's ModParser turns `+N to any attribute` into an
/// empty mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeChoice {
    Strength,
    Dexterity,
    Intelligence,
}

/// Everything the player has chosen on the passive tree.
#[derive(Debug, Clone, Default)]
pub struct PassiveTreeSpec {
    pub allocated_nodes: Vec<NodeId>,
    /// Effect picked on each allocated mastery node. A mastery node missing
    /// from this map contributes nothing — picking is what unlocks it.
    pub mastery_effects: HashMap<NodeId, MasterySelection>,
    /// Attribute picked on each `+5 to any Attribute` node. Same rule: a node
    /// missing from the map grants nothing.
    pub attribute_overrides: HashMap<NodeId, AttributeChoice>,
}
