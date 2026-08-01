//! Local-mod whitelist domain schema (`overlay/local_mods.json`).
//!
//! pobr's source of truth (a migration invariant: the JSON is value-equal
//! to the hardcoded list here): the text enum from
//! `crates/pobr-build/src/calc_orchestrator.rs::is_weapon_local_mod` — on a
//! weapon slot, a match gets removed from the global modifier pool (it's
//! already counted in the weapon source's own multiplier band; leaving it
//! in the global pool would double-count it and wrongly fold it into the
//! additive bucket):
//! 1. clean text ending in `% increased physical damage`;
//! 2. clean text ending in `% increased attack speed`;
//! 3. the `adds N to M physical damage` shape (`parse_adds_physical`).
//!
//! Vendor comparison (discrepancies are recorded only, not fixed here —
//! see audits/rearchitecture-2026-06-10/16-items.md's "self-audit of
//! pobr's current mixed approach"): PoB2's locality check is
//! `src/Classes/Item.lua:1655-1682`'s `calcLocal`, a **structured rule**
//! (exact match on mod name + flag, keywordFlags == 0, no tag or only an
//! InSlot tag), not a text enum; the eventual direction is "a
//! local_mods.json whitelist + structured local resolution". For now the
//! existing text enum is turned into data as-is (unchanged values = no
//! parity change); the structured migration is left for a later wave.
//!
//! Note: clean text = the output of `clean_item_text` (strips `{...}`
//! markup, trims, and **lowercases**), so whitelist entries are always
//! lowercase English.

use serde::{Deserialize, Serialize};

/// Top level of `overlay/local_mods.json` (a single-object domain).
///
/// Currently only the weapon section; armour pieces' local defence
/// (`parse_local_defence_inc/flat`) is a structured form parse rather than
/// a whitelist enum, so it's not stored here (to be decided together with
/// the structured local-resolution work).
///
/// `Default` is the built-in fallback (recursively taking
/// [`WeaponLocalModsDef::default`]): a mirror value-equal to
/// `data/<ver>/overlay/local_mods.json`, the degradation path used when an
/// old data pack has no such overlay file; consistency is locked by a
/// pobr-gamedata load test.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModsDef {
    /// Weapon local-mod matching rules.
    pub weapon: WeaponLocalModsDef,
}

/// The weapon local-mod whitelist (the data shape of `is_weapon_local_mod`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponLocalModsDef {
    /// Clean-text suffixes that count as local on an `ends_with` match (lowercase).
    pub increased_suffixes: Vec<String>,
    /// Damage suffixes counted as local in the `adds N to M <suffix>` shape
    /// (lowercase, no leading space).
    pub adds_damage_suffixes: Vec<String>,
}

/// The built-in fallback value = the original hardcoded enum in
/// `is_weapon_local_mod` (the migration invariant's source of truth).
impl Default for WeaponLocalModsDef {
    fn default() -> Self {
        Self {
            increased_suffixes: vec![
                "% increased physical damage".to_string(),
                "% increased attack speed".to_string(),
            ],
            adds_damage_suffixes: vec!["physical damage".to_string()],
        }
    }
}
