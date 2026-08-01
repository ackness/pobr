//! Generic stat floor/ceiling resolution (`StatBoundary`).
//!
//! Applies a `pobr_data::BoundarySpec` (floor / default_max / hard_cap) to an
//! uncapped stat value, producing: the clamped final value, the effective max,
//! over-cap (how much of the uncapped value was wasted above the max), and
//! missing (how far below the max the final value still is). Resistances,
//! maximum resistances, and any other bounded stat share this primitive.
//!
//! Formulas (kept in sync with `offence::resolve_resistance`):
//! - `effective_max = min(default_max + max_bonus, hard_cap)` (either side is
//!   skipped when `None`)
//! - `final = clamp(uncapped, floor, effective_max)` (floor / max leave that
//!   side unconstrained when `None`)
//! - `over_cap = max(uncapped - effective_max, 0)`
//! - `missing = max(effective_max - final, 0)`

use pobr_data::prelude::*;

use super::round;

/// Result of resolving a stat's floor/ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatBoundary {
    /// The raw, unclamped value.
    pub uncapped: f64,
    /// The effective max (default_max + max_bonus, constrained by hard_cap);
    /// `f64::INFINITY` when there is no max constraint.
    pub max: f64,
    /// The clamped final value.
    pub final_value: f64,
    /// How much of `uncapped` was wasted above the max (>= 0).
    pub over_cap: f64,
    /// How far below the max the final value still is (>= 0); 0 when there
    /// is no max constraint.
    pub missing: f64,
}

/// Resolves the floor/ceiling of `uncapped` per `spec`. `max_bonus` is the sum
/// of modifiers that raise `default_max` (e.g. `+5% to maximum Fire Resistance`).
pub fn stat_boundary(uncapped: f64, max_bonus: f64, spec: &BoundarySpec) -> StatBoundary {
    let effective_max = spec.default_max.map(|default_max| {
        let raised = default_max + max_bonus;
        match spec.hard_cap {
            Some(hard) => raised.min(hard),
            None => raised,
        }
    });

    let final_value = {
        let mut value = uncapped;
        if let Some(floor) = spec.floor {
            value = value.max(floor);
        }
        if let Some(max) = effective_max {
            value = value.min(max);
        }
        value
    };

    let (max_out, over_cap, missing) = match effective_max {
        Some(max) => (
            round(max),
            round((uncapped - max).max(0.0)),
            round((max - final_value).max(0.0)),
        ),
        None => (f64::INFINITY, 0.0, 0.0),
    };

    StatBoundary {
        uncapped: round(uncapped),
        max: max_out,
        final_value: round(final_value),
        over_cap,
        missing,
    }
}
