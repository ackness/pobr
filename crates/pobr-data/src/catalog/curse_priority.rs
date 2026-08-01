//! Schema for `overlay/curse_priority.json` (`curse_priority/v1`).
//!
//! Vendor source: the plain data table `data.cursePriority` starting at
//! `Modules/Data.lua:274`, deterministically extracted by
//! `sync-pob-catalog extract-lua --what curse-priority` (luajit evaluates
//! the table literal; the output is byte-stable, `_meta` records the
//! vendor commit, hand edits are forbidden). Vendor's flat `k=v` table is
//! split into four sections by meaning, for easier lookup by the consumer.
//!
//! Consumer (`calc/buff_pass.rs`) is equivalent to PoB2's
//! `determineCursePriority` (CalcPerform.lua:454-485):
//! `priority = curse_base + min(socket_index, 8) × socket_priority_base
//!           + slot_weight(slot name with the " (Swap)" suffix stripped) + source_weight`,
//! where source_weight is [`CursePriorityDef::curse_from_aura`] for an aura
//! source, or [`CursePriorityDef::curse_from_equipment`] for an equipment
//! implicit source (Ring 2/3 fold back to Ring 1's weight). This module
//! only defines the serde shape, zero logic, zero I/O.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Current overlay document schema identifier (bumped when the field shape evolves).
pub const CURSE_PRIORITY_SCHEMA: &str = "curse_priority/v1";

/// Top level of `overlay/curse_priority.json` (the consumer ignores
/// `_meta`; on the production side the tool wraps it via a flattened
/// `CursePriorityDoc`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursePriorityDef {
    /// Per-curse base value (e.g. `"Temporal Chains"→1` …
    /// `"Poacher's Mark"→13`; on a same-slot conflict, the larger value
    /// wins over the smaller).
    pub curse_base: BTreeMap<String, i64>,
    /// Unit weight per socket index (`min(socket_index, 8) × this value`;
    /// vendor = 100).
    pub socket_priority_base: i64,
    /// Equipment slot-name weight (`"Weapon 1"→1000` … `"Ring 3"→10000`;
    /// the `" (Swap)"` suffix is stripped before lookup).
    pub slot_weights: BTreeMap<String, i64>,
    /// Weight for a curse sourced from an equipment implicit (vendor = 11000).
    pub curse_from_equipment: i64,
    /// Weight for a curse sourced from an aura (vendor = 20000, always the
    /// highest band).
    pub curse_from_aura: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CursePriorityDef {
        CursePriorityDef {
            curse_base: BTreeMap::from([("Temporal Chains".to_string(), 1)]),
            socket_priority_base: 100,
            slot_weights: BTreeMap::from([("Weapon 1".to_string(), 1000)]),
            curse_from_equipment: 11000,
            curse_from_aura: 20000,
        }
    }

    /// serde round trip: serialize → deserialize matches field for field.
    #[test]
    fn serde_round_trip() {
        let def = sample();
        let json = serde_json::to_string_pretty(&def).unwrap();
        let back: CursePriorityDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }

    /// The consumer tolerates unknown fields when loading (the overlay
    /// document's leading `_meta` is simply ignored).
    #[test]
    fn deserialize_ignores_meta() {
        let json = r#"{
            "_meta": {"schema": "curse_priority/v1"},
            "curse_base": {"Enfeeble": 2},
            "socket_priority_base": 100,
            "slot_weights": {"Amulet": 2000},
            "curse_from_equipment": 11000,
            "curse_from_aura": 20000
        }"#;
        let def: CursePriorityDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.curse_base["Enfeeble"], 2);
        assert_eq!(def.slot_weights["Amulet"], 2000);
        assert_eq!(def.curse_from_aura, 20000);
    }
}
