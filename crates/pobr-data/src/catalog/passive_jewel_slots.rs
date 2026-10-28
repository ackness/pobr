//! The game's jewel socket slots, curated from the official `PassiveJewelSlots` table.
//! Provenance lives in `overlay-common/passive_jewel_slots.json`.

use serde::{Deserialize, Serialize};

/// One row of `PassiveJewelSlots`, resolved to the passive tree node that is its socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveJewelSlotDef {
    /// Row index in the table. A WeGame share names the socket `PassiveJewel_<index>`.
    pub index: u32,
    /// `PassiveSkills.Id` of the socket, the id of its passive tree node.
    pub passive_id: String,
    #[serde(default)]
    pub passive_skill_graph_id: u32,
    #[serde(default)]
    pub sinister_jewel_socket: bool,
}

/// Top level of `overlay-common/passive_jewel_slots.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassiveJewelSlotsDef {
    #[serde(default)]
    pub slots: Vec<PassiveJewelSlotDef>,
}

impl PassiveJewelSlotsDef {
    /// The node id of the socket a WeGame `PassiveJewel_<index>` names.
    pub fn passive_id_for_wegame_socket(&self, socket: &str) -> Option<&str> {
        let index: u32 = socket.strip_prefix("PassiveJewel_")?.parse().ok()?;
        self.slots
            .iter()
            .find(|slot| slot.index == index)
            .map(|slot| slot.passive_id.as_str())
    }
}
