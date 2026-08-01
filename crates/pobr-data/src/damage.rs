//! Ailment and debuff results.
//!
//! These are what `pobr-core` produces, but they live here so that `pobr-build`
//! and other callers can hold a calc result without depending on `pobr-core`.
//!
//! `DamageComponent` — the per-component damage bucket — deliberately is *not*
//! here; it belongs to `pobr-core::calc`, and defining it twice would give two
//! types with the same name.

use serde::{Deserialize, Serialize};

use crate::constants::{AilmentType, DamageSource};

/// One ailment currently applied.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AilmentInstance {
    pub ailment: AilmentType,
    /// Damage per second; zero for the non-damaging ailments.
    pub magnitude_dps: f64,
    /// How long it lasts, in seconds.
    pub duration_secs: f64,
    /// Which damage component of the hit inflicted it.
    #[serde(default)]
    pub source_component: Option<DamageSource>,
    /// Whether this damage over time bypasses energy shield.
    #[serde(default)]
    pub bypasses_es: bool,
}

impl AilmentInstance {
    /// Total damage over the ailment's lifetime.
    pub fn total_damage(&self) -> f64 {
        self.magnitude_dps * self.duration_secs
    }
}

/// A stacking debuff — Corrupted Blood and friends, which are not bleeds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebuffInstance {
    pub label: String,
    pub current_stacks: u8,
    pub max_stacks: u8,
    pub dps_per_stack: f64,
    pub duration_secs: f64,
}

impl DebuffInstance {
    /// Damage per second from the stacks currently up, capped at `max_stacks`.
    pub fn total_dps(&self) -> f64 {
        self.dps_per_stack * f64::from(self.current_stacks.min(self.max_stacks))
    }
}
