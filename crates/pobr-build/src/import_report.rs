//! What an import could not carry into a [`Build`], and the reference values
//! a build code records about itself.
//!
//! A [`Build`] is calculation input only (`snapshot::same_build` projects every
//! field), so these travel beside it. Nothing here changes a result.

use std::collections::BTreeMap;

use crate::build::Build;

/// A parsed build with the report of its import.
#[derive(Debug, Clone)]
pub struct ImportedBuild {
    pub build: Build,
    pub report: ImportReport,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    /// `<Item>` blocks the item parser rejected. The build has no item where
    /// one of these was used, so its numbers are those of a character missing
    /// that gear.
    pub skipped_items: Vec<SkippedItem>,
    /// Slots of the active item set that hold an item and that the engine has
    /// no model for. Whatever those items give is absent from the result.
    pub unmodelled_slots: Vec<UnmodelledSlot>,
    /// `<PlayerStat>` values the exporting tool wrote into the code, by PoB2
    /// output name. They are that tool's result at export time, under its own
    /// game data: a reference to compare against, not the game's numbers.
    pub reference_stats: BTreeMap<String, f64>,
}

impl ImportReport {
    /// Rejected items the active sets use. A rejected item that only sits in
    /// the item library changes no result.
    pub fn missing_items(&self) -> impl Iterator<Item = &SkippedItem> {
        self.skipped_items
            .iter()
            .filter(|item| !item.used_in.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmodelledSlot {
    /// The PoB slot name, as the build wrote it.
    pub slot: String,
    /// The `<Item id>` in the build XML.
    pub item_id: u32,
    /// The item's base, for display. Empty when the block did not parse.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedItem {
    /// The `<Item id>` in the build XML.
    pub item_id: u32,
    /// Where the active item set and passive spec use it: an equipment or limb
    /// slot id (`bodyarmour`, `arm1`), a PoB flask or jewel slot name, or
    /// `socket:<node>`.
    pub used_in: Vec<String>,
    /// The item's name lines, for display. Empty when the block has none.
    pub name: String,
    pub reason: String,
}
