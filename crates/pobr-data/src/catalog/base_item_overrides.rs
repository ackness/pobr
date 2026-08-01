//! Base item override-value overlay domain schema
//! (`overlay/base_item_overrides.json`).
//!
//! Data source: vendor PoB2 `Data/Bases/*.lua` (shield / sceptre etc.) — the
//! bundle for the corresponding GGG `.dat` tables (`ShieldTypes`'s `Block`
//! column, `ItemSpirit`'s `SpiritGranted` column) has been pruned from the
//! CDN at the pinned patch, so the `.dat` route is unavailable; per the
//! dual-route decision from open questions 1/2, this falls back to vendor
//! extraction. Deterministically extracted by
//! `sync-pob-catalog extract-bases` (schema id `base_item_overrides/v1`,
//! shaped the same as the `skill_overrides` channel).
//!
//! Consumer: `pobr-gamedata` merges this table onto [`super::BaseItemDef`]
//! by **English canonical name** while loading `base/base_items.json`
//! (merge semantics and unit tests in
//! `pobr-gamedata::domains::base_item_overrides`). This module only
//! defines the serde shape, zero logic.

use serde::{Deserialize, Serialize};

/// A single base override entry (vendor's `itemBases["<name>"]`'s
/// `armour.BlockChance` / `spirit`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseItemOverrideEntry {
    /// The base's English canonical name (= vendor's `itemBases` key =
    /// `BaseItemDef::name`).
    pub name: String,
    /// A shield base's block chance (%; vendor's `armour.BlockChance`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_chance: Option<f64>,
    /// The base's inherent Spirit (vendor's `spirit`, e.g. 100 for a sceptre).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spirit: Option<u32>,
    /// Crossbow reload time (milliseconds; vendor's
    /// `weapon.ReloadTimeBase` seconds value ×1000, ultimately sourced from
    /// `WeaponTypes.ReloadTime` — the vendor-extraction fallback while the
    /// local `.dat` snapshot is missing it; the consumer writes it into
    /// [`super::WeaponBaseStats::reload_time_ms`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_time_ms: Option<u32>,
    /// A charm base's inherent buff mod (vendor
    /// `Data/Bases/flask.lua`'s `charm.buff`, e.g. a Ruby Charm's
    /// `"+25% to Fire Resistance"`); the consumer writes it into
    /// [`super::BaseItemDef::charm_buff`]. No GGG `.dat` column for this —
    /// the vendor-extraction fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charm_buff: Option<Vec<String>>,
    /// The base's full tag set (vendor's `itemBases[name].tags` — the
    /// merged product of GGG `.it` metadata's inheritance chain). The
    /// `.dat`'s `BaseItemTypes.Tags` only has the leaf tag (e.g.
    /// `dex_int_armour`) and is missing category tags like
    /// `body_armour`/`armour`, but mod spawn-weight checks (tier
    /// reverse-lookup) need the full set. The consumer merges this as a
    /// **union** with the base tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// The base's attribute requirements (vendor's
    /// `req = { str/dex/int }`; the consumer writes it into
    /// [`super::BaseItemDef::req_str`] etc., the data source for the
    /// equipment-requirement snapshot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_str: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_dex: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req_int: Option<u32>,
}

/// Top level of `overlay/base_item_overrides.json` (from the consumer's
/// perspective: the `_meta` header is provenance info, ignored by default
/// via serde along with other unknown fields; the consumer just takes the
/// `overrides` list).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseItemOverridesDef {
    /// Override list, ascending by `name`.
    pub overrides: Vec<BaseItemOverrideEntry>,
}
