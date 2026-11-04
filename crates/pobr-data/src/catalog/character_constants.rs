//! Character base-constants domain schema
//! (`base/character_constants.json`, level/attribute-derived constants).
//!
//! The numeric constants were migrated out of `pobr-core::character`
//! (**the formula logic still lives in Rust** — this table only carries the
//! numbers). Only values pobr doesn't already have are extracted from
//! vendor PoB2, with the vendor file:line noted in each field's doc (vendor
//! commit `2df5a74`, see `vendor/.pob2-version.txt`).
//!
//! Source cross-reference:
//! - consumer: `crates/pobr-core/src/ingest/character.rs` (the formulas);
//! - vendor: `src/Data/Misc.lua:140`'s `data.characterConstants` table +
//!   `src/Modules/CalcSetup.lua:615-622`'s character-base section +
//!   `src/Modules/CalcPerform.lua:420-443`'s attribute-derivation section +
//!   `src/Modules/Data.lua:174`'s `AccuracyPerDexBase`.
//!
//! Scope note: resistance caps / charge caps / base crit damage etc. also
//! live under vendor's `data.characterConstants`, but they're assigned to
//! `game_constants.json`'s character section instead, and aren't stored
//! here — to avoid defining them twice across two tables.
//!
//! TODO (recorded only, not handled by this task): this table is listed
//! under `overlay/` by the pre-registered `manifest.json`, which registers
//! `character_constants` in the `base` section instead. This implementation
//! goes with the manifest and stores it under `base/`; the ownership
//! discrepancy is left for a later decision.

use serde::{Deserialize, Serialize};

/// Character level/attribute-derived constants (a single-object domain —
/// the whole file is one JSON object of this struct).
///
/// Consumed by `pobr-core::character` (the `CharacterBase` derivation formulas):
/// - inherent life = `base_life_constant + life_per_level*level + life_per_strength*Str`
/// - inherent mana = `base_mana_constant + mana_per_level*level + mana_per_intelligence*Int`
/// - inherent accuracy = `base_accuracy_constant + accuracy_per_level*level + accuracy_per_dexterity*Dex`
/// - inherent evasion = `base_evasion`
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CharacterConstantsDef {
    /// Inherent life's constant term (formerly the pobr constant
    /// `BASE_LIFE_CONSTANT = 16`; vendor `CalcSetup.lua:615`'s Multiplier
    /// semantics, `base = 16`).
    pub base_life_constant: f64,
    /// Life per level (formerly the pobr constant `LIFE_PER_LEVEL = 12`; vendor
    /// `Data/Misc.lua:152`'s `life_per_level`).
    pub life_per_level: f64,
    /// Life per 1 point of strength (formerly the pobr constant
    /// `LIFE_PER_STRENGTH = 2`; vendor `CalcPerform.lua:429`'s literal
    /// `output.Str * 2`).
    pub life_per_strength: f64,
    /// Inherent mana's constant term (formerly the pobr constant
    /// `BASE_MANA_CONSTANT = 30`; vendor `CalcSetup.lua:616`'s `base = 30`).
    pub base_mana_constant: f64,
    /// Mana per level (formerly the pobr constant `MANA_PER_LEVEL = 4`; vendor
    /// `Data/Misc.lua:153`'s `mana_per_level`).
    pub mana_per_level: f64,
    /// Mana per 1 point of intelligence (formerly the pobr constant
    /// `MANA_PER_INTELLIGENCE = 2`; vendor `CalcPerform.lua:440`'s literal
    /// `output.Int * 2`).
    pub mana_per_intelligence: f64,
    /// Inherent accuracy's constant term (formerly the pobr constant
    /// `BASE_ACCURACY_CONSTANT = -6`; vendor `CalcSetup.lua:622`'s
    /// `base = -data.characterConstants["accuracy_rating_per_level"]`).
    pub base_accuracy_constant: f64,
    /// Accuracy per level (formerly the pobr constant `ACCURACY_PER_LEVEL = 6`;
    /// vendor `Data/Misc.lua:154`'s `accuracy_rating_per_level`).
    pub accuracy_per_level: f64,
    /// Accuracy per 1 point of dexterity (formerly the pobr constant
    /// `ACCURACY_PER_DEXTERITY = 6`; vendor `Modules/Data.lua:174`'s
    /// `AccuracyPerDexBase = 6`).
    pub accuracy_per_dexterity: f64,
    /// Inherent base evasion (formerly the pobr constant `BASE_EVASION = 7`;
    /// vendor `Data/Misc.lua:151`'s `base_evasion_rating`).
    pub base_evasion: f64,
    /// Strength per level (vendor-only: `Data/Misc.lua:157`'s
    /// `strength_per_level = 0`).
    pub strength_per_level: f64,
    /// Dexterity per level (vendor-only: `Data/Misc.lua:158`'s
    /// `dexterity_per_level = 0`).
    pub dexterity_per_level: f64,
    /// Intelligence per level (vendor-only: `Data/Misc.lua:159`'s
    /// `intelligence_per_level = 0`).
    pub intelligence_per_level: f64,
    /// Inherent Runic Ward regeneration rate, % of ward per minute (vendor
    /// `Data/Misc.lua:129`'s `BaseWardRegenerationPercentPerMinute = 300`).
    /// Defaulted so snapshots written before the field existed keep loading.
    #[serde(default = "default_ward_regeneration_per_minute_pct")]
    pub ward_regeneration_per_minute_pct: f64,
}

fn default_ward_regeneration_per_minute_pct() -> f64 {
    300.0
}

// Default = fallback values
//
// Semantics: `Default` is "the fallback constant set used when no GameData
// is injected", and must be value-equal field by field to
// `data/<version>/base/character_constants.json`.
//
// The shipped table is the source of truth; pobr-gamedata's
// `default_matches_shipped_character_constants` test keeps this fallback
// equal to it. The vendor-only per-level fields cite their vendor source.

impl Default for CharacterConstantsDef {
    fn default() -> Self {
        Self {
            base_life_constant: 16.0,
            life_per_level: 12.0,
            life_per_strength: 2.0,
            base_mana_constant: 30.0,
            mana_per_level: 4.0,
            mana_per_intelligence: 2.0,
            base_accuracy_constant: -6.0,
            accuracy_per_level: 6.0,
            accuracy_per_dexterity: 6.0,
            base_evasion: 7.0,
            // vendor-only (Data/Misc.lua:157-159, all per-level attributes are currently 0).
            strength_per_level: 0.0,
            dexterity_per_level: 0.0,
            intelligence_per_level: 0.0,
            // vendor-only (Misc.lua:129).
            ward_regeneration_per_minute_pct: default_ward_regeneration_per_minute_pct(),
        }
    }
}
