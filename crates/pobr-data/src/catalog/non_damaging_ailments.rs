//! Non-damaging ailment domain schema (`base/non_damaging_ailments.json`).
//!
//! Corresponds to three vendor tables in PoB2
//! (`vendor/PathOfBuilding-PoE2/src/Modules/Data.lua`):
//! - `data.nonDamagingAilment` (Data.lua:347-351) — the
//!   default/min/max/precision/duration for chill/freeze/shock
//!   (gameConstants references already resolved to literal values);
//! - `data.buildupTypes` (Data.lua:353-376) — the damage-type scaling
//!   source for accumulating ailments;
//! - `data.defaultAilmentDamageTypes` (Data.lua:378-410) — each ailment's
//!   default damage-scaling source plus the DoT damage type for damaging
//!   ailments.
//!
//! Values are value-equal to pobr's existing Rust source of truth (a
//! migration invariant): chill/shock bounds correspond to
//! `pobr_data::monster`'s
//! `CHILL_MIN_EFFECT`/`CHILL_MAX_EFFECT`/`BASE_SHOCK_MAGNITUDE`/`SHOCK_MAX_EFFECT`
//! and `pobr_data::constants::SHOCK_MIN_EFFECT`; a damaging ailment's
//! `damage_type` corresponds to
//! `pobr_data::constants::AilmentType::damage_type()`. Fields pobr doesn't
//! have (associated_type/alt/precision/duration, freeze's bounds,
//! scales_from) are extracted from vendor, with the line number noted in
//! each field's doc. The calc formulas still live in
//! `pobr-core::calc::ailment` — this table only migrates the numbers (not
//! wired up yet, to be consumed in W3).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::constants::DamageType;

/// Top-level structure of `base/non_damaging_ailments.json`.
///
/// The keys of all three section maps are vendor's canonical
/// ailment/buildup names (e.g.
/// `Chill`/`Freeze`/`Shock`/`Electrocute`/`HeavyStun`/`Pin`/`Bleed`/`Poison`/`Ignite`);
/// `BTreeMap` keeps the serialized key order stable (diff-friendly,
/// reproducible).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NonDamagingAilmentsDef {
    /// Non-damaging ailment magnitude bounds (`data.nonDamagingAilment`, Data.lua:347-351).
    pub ailments: BTreeMap<String, NonDamagingAilmentDef>,
    /// Scaling source for accumulating ailments (`data.buildupTypes`, Data.lua:353-376).
    pub buildup_types: BTreeMap<String, BuildupTypeDef>,
    /// Each ailment's default damage-scaling source / DoT damage type
    /// (`data.defaultAilmentDamageTypes`, Data.lua:378-410).
    pub default_ailment_damage_types: BTreeMap<String, AilmentDamageTypeDef>,
}

/// The magnitude bounds for a single non-damaging ailment (one row of
/// `data.nonDamagingAilment`).
///
/// Magnitude units: Chill/Shock are percentages (% reduced action speed /
/// % increased damage taken), Freeze is seconds (a duration tier).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NonDamagingAilmentDef {
    /// The associated elemental damage type (Chill/Freeze→Cold,
    /// Shock→Lightning; Data.lua:348-350).
    pub associated_type: DamageType,
    /// Whether this is an alternative ailment (vendor's three entries are
    /// all `false`, Data.lua:348-350).
    pub alt: bool,
    /// Default applied magnitude. Chill=30 (source of truth
    /// `monster::CHILL_MIN_EFFECT`), Shock=20 (source of truth
    /// `monster::BASE_SHOCK_MAGNITUDE`/`constants::SHOCK_MIN_EFFECT`);
    /// Freeze has no default (vendor's `default = nil`, Data.lua:349).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<f64>,
    /// Minimum effective magnitude (below this, it isn't applied at all).
    /// Chill=30 / Shock=20 as above; Freeze=0.3 seconds (vendor-only, Data.lua:349).
    pub min: f64,
    /// Magnitude cap. Chill=50 (source of truth `monster::CHILL_MAX_EFFECT`),
    /// Shock=100 (source of truth `monster::SHOCK_MAX_EFFECT`);
    /// Freeze=3 seconds (vendor-only, Data.lua:349).
    pub max: f64,
    /// Display precision (decimal places; Chill/Shock=0, Freeze=2;
    /// vendor-only, Data.lua:348-350).
    pub precision: u8,
    /// Base duration (seconds; vendor-only, gameConstants references
    /// already resolved): Chill=8 (`BaseChillDuration`, Data/Misc.lua:91),
    /// Freeze=4 (`FreezeDuration`, Data/Misc.lua:56), Shock=8
    /// (`BaseShockDuration`, Data/Misc.lua:93).
    pub duration: f64,
}

/// The damage-type scaling source for an accumulating ailment (buildup)
/// (one row of `data.buildupTypes`, vendor-only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildupTypeDef {
    /// The damage types that contribute to accumulation (vendor's
    /// `ScalesFrom` set, in PoE's canonical damage-type order
    /// Physical/Fire/Cold/Lightning/Chaos). An empty array means it
    /// doesn't accumulate from hit damage type at all (Electrocute/Pin are
    /// driven directly by a skill stat instead, Data.lua:354-357, 372-375).
    pub scales_from: Vec<DamageType>,
}

/// An ailment's default damage-scaling source / DoT damage type (one row
/// of `data.defaultAilmentDamageTypes`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AilmentDamageTypeDef {
    /// The hit damage types that feed the magnitude calculation by default
    /// (vendor's `ScalesFrom`, in canonical damage-type order;
    /// Data.lua:380-409).
    pub scales_from: Vec<DamageType>,
    /// The DoT damage type for a damaging ailment (Bleed→Physical /
    /// Poison→Chaos / Ignite→Fire; source of truth
    /// `constants::AilmentType::damage_type()`); `None` for non-damaging
    /// ailments (Shock/Chill — vendor's row has no `DamageType` field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_type: Option<DamageType>,
}
