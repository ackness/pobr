//! Schema for the four item-editing-state overlay tables:
//!
//! | Table | Path | Vendor source |
//! |---|---|---|
//! | `mod_scalability.json` | overlay/ | `Data/ModScalability.lua` (15037 lines, a bare `return {...}`) |
//! | `catalysts.json`       | overlay/ | three local table literals at `Classes/Item.lua:14-29` |
//! | `runes.json`           | overlay/ | `Data/ModRunes.lua` (a bare `return {...}`) |
//! | `uniques.json`         | overlay/ | `Data/Uniques/*.lua` (arrays of raw text blocks) + `Special/race` |
//!
//! All deterministically extracted by `sync-pob-catalog extract-lua --what
//! mod-scalability|catalysts|runes|uniques` (luajit executes vendor's
//! serialization; the output is byte-stable, `_meta` records the vendor
//! commit, and hand edits are forbidden). This module only defines the
//! serde shape, zero logic, zero I/O.
//!
//! Consumers:
//! - `mod_scalability` / `catalysts` → injected via RuleSet `ItemRules`
//!   into `pobr-core::apply_range`'s value-resolution engine;
//! - `runes` / `uniques` → pobr-item's editing state, loaded separately on
//!   demand, not part of ItemRules.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// mod_scalability — scalability + format-conversion table for `{range:x}` mod values

/// Scaling rule for one numeric slot in a mod template (corresponds to one
/// entry of vendor's entry array).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModScalabilitySlotDef {
    /// Whether this numeric slot can scale with `{range:x}` / a catalyst
    /// (`isScalable`).
    #[serde(default)]
    pub is_scalable: bool,
    /// Numeric-format conversion enum (`formats`, e.g.
    /// `divide_by_one_hundred` / `per_minute_to_per_second`; the consumer
    /// warns at load time for an unknown format).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<String>,
}

/// Scaling rule for one `#`-templated mod (vendor's key is the mod text
/// with its numbers replaced by `#`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModScalabilityEntryDef {
    /// The `#`-templated mod text (e.g. `# Armour per 2 Strength`).
    pub template: String,
    /// Scaling rules per numeric slot (in 1:1 order with the `#`
    /// occurrences in the template).
    pub slots: Vec<ModScalabilitySlotDef>,
}

/// Top level of `overlay/mod_scalability.json` (the consumer ignores `_meta`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModScalabilityDef {
    /// Entry list, ascending by `template`.
    pub entries: Vec<ModScalabilityEntryDef>,
}

// catalysts — catalyst quality-tag matching table

/// A single catalyst (vendor `Classes/Item.lua:14-29`'s three parallel
/// arrays merged by index).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalystDef {
    /// 1-based index (= the XML `Catalyst` attribute value; vendor
    /// `catalystList`'s index).
    pub id: u8,
    /// Name prefix (`catalystList`, e.g. `Carapace`).
    pub name: String,
    /// Descriptor word (`catalystDescriptorList`, e.g. `Defence`; used to
    /// reverse-look-up the id from an item's `Quality (<descriptor> Modifiers)` text).
    pub descriptor: String,
    /// The set of mod tags it matches (`catalystTags`; `getCatalystScalar`
    /// grants `(100+quality)/100` when
    /// `catalystTags[id] ∩ mod.modTags ≠ ∅`).
    pub tags: Vec<String>,
}

/// Top level of `overlay/catalysts.json` (the consumer ignores `_meta`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalystsDef {
    /// Catalyst list, ascending by `id` (12 entries).
    pub catalysts: Vec<CatalystDef>,
}

// runes — rune / soul core socketed-mod table

/// A rune's mod group for one slot class (vendor `ModRunes.lua`'s
/// second-level table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuneSlotDef {
    /// Category literal (`type`: `Rune` / `SoulCore`).
    pub kind: String,
    /// Rendered mod lines (vendor table's array part).
    pub lines: Vec<String>,
    /// Sort weight (`rank`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rank: Vec<f64>,
    /// The mods' statOrder (in the same order as `lines`; used when merging
    /// and sorting in the editing state).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stat_order: Vec<f64>,
}

/// A single rune / soul core (vendor's key is the rune name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuneDef {
    /// Rune name (e.g. `Desert Rune` / `Hayoxi's Soul Core of Heatproofing`).
    pub name: String,
    /// Slot class → mod group (keys like `weapon`/`helmet`/`boots`,
    /// BTreeMap to keep key order stable).
    pub slots: BTreeMap<String, RuneSlotDef>,
}

/// Top level of `overlay/runes.json` (the consumer ignores `_meta`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RunesDef {
    /// Rune list, ascending by `name`.
    pub runes: Vec<RuneDef>,
}

// uniques — unique-item raw text blocks + a pre-parsed index (two layers)

/// A single unique item (two layers: `raw` keeps vendor's original text
/// block byte-for-byte; the index columns only do minimal pre-parsing —
/// name/base/variants/league/source; parsing the mod template lines is
/// done at runtime by pobr-item).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniqueDef {
    /// Item name (line 1 of the raw block).
    pub name: String,
    /// Base name (line 2 of the raw block).
    pub base: String,
    /// The source file's domain (the `<item_type>` of vendor's
    /// `Data/Uniques/<item_type>.lua`, e.g. `amulet`; `Special/race` is
    /// recorded as `race`).
    pub item_type: String,
    /// Vendor's original text block (includes `Variant:`/`League:`/
    /// `{tags:...}` annotations, kept byte-for-byte).
    pub raw: String,
    /// `Variant:` line list (pre-parsed index; in appearance order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    /// `League:` line value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub league: Option<String>,
    /// `Source:` line value (the first line when there are several; see
    /// `raw` for the full content).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `Upgrade:` line value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<String>,
}

/// Top level of `overlay/uniques.json` (the consumer ignores `_meta`).
///
/// Scope note: `Data/Uniques/Special/Generated.lua` (procedurally
/// generated, depends on runtime itemMods) and `Special/New.lua` (an
/// unfinalized pool) are out of extraction scope — see the `_meta` note.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UniquesDef {
    /// Unique list, ascending by `(item_type, name, appearance order)`.
    pub uniques: Vec<UniqueDef>,
}
