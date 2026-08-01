//! Scalar field diff between two [`OutputTable`]s.
//!
//! Used for questions like "how much did DPS/EHP change after swapping this item /
//! flipping this config option", and for regression tests that compare against a PoB
//! baseline within a tolerance. Only compares [`OutputTable`]'s scalar numeric fields
//! (not the `damage_components` vector; that gets its own dedicated comparison).

use pobr_core::calc::OutputTable;

/// Diff for a single scalar field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDiff {
    /// Stable field name (for display / sorting; display text goes through i18n).
    pub field: &'static str,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
    /// Relative change (`delta / before`). `None` when `before` is 0.
    pub relative: Option<f64>,
}

/// Comparison result for two outputs: only fields that changed beyond the tolerance.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputComparison {
    pub diffs: Vec<FieldDiff>,
}

impl OutputComparison {
    /// Whether nothing changed (no diffs beyond tolerance).
    pub fn is_unchanged(&self) -> bool {
        self.diffs.is_empty()
    }

    /// Looks up the diff for a given field.
    pub fn field(&self, name: &str) -> Option<&FieldDiff> {
        self.diffs.iter().find(|d| d.field == name)
    }
}

/// Default float comparison tolerance.
pub const DEFAULT_EPSILON: f64 = 1e-9;

/// Compares the scalar fields of two [`OutputTable`]s, collecting diffs beyond `epsilon`.
pub fn compare_outputs(
    before: &OutputTable,
    after: &OutputTable,
    epsilon: f64,
) -> OutputComparison {
    let fields: [(&'static str, f64, f64); 19] = [
        ("life", before.life, after.life),
        ("mana", before.mana, after.mana),
        ("armour", before.armour, after.armour),
        ("evasion", before.evasion, after.evasion),
        ("energy_shield", before.energy_shield, after.energy_shield),
        (
            "chance_to_be_hit",
            before.chance_to_be_hit,
            after.chance_to_be_hit,
        ),
        (
            "fire_resistance",
            before.fire_resistance,
            after.fire_resistance,
        ),
        (
            "cold_resistance",
            before.cold_resistance,
            after.cold_resistance,
        ),
        (
            "lightning_resistance",
            before.lightning_resistance,
            after.lightning_resistance,
        ),
        ("crit_chance", before.crit_chance, after.crit_chance),
        (
            "crit_multiplier",
            before.crit_multiplier,
            after.crit_multiplier,
        ),
        ("total_hit_avg", before.total_hit_avg, after.total_hit_avg),
        ("hit_chance", before.hit_chance, after.hit_chance),
        ("action_rate", before.action_rate, after.action_rate),
        ("dps", before.dps, after.dps),
        (
            "fire_resistance_over_cap",
            before.fire_resistance_over_cap,
            after.fire_resistance_over_cap,
        ),
        (
            "cold_resistance_over_cap",
            before.cold_resistance_over_cap,
            after.cold_resistance_over_cap,
        ),
        (
            "lightning_resistance_over_cap",
            before.lightning_resistance_over_cap,
            after.lightning_resistance_over_cap,
        ),
        (
            "max_fire_resistance",
            before.max_fire_resistance,
            after.max_fire_resistance,
        ),
    ];

    let mut diffs = Vec::new();
    for (name, b, a) in fields {
        let delta = a - b;
        if delta.abs() > epsilon {
            diffs.push(FieldDiff {
                field: name,
                before: b,
                after: a,
                delta,
                relative: relative_change(b, delta),
            });
        }
    }

    OutputComparison { diffs }
}

fn relative_change(before: f64, delta: f64) -> Option<f64> {
    if before.abs() <= f64::EPSILON {
        None
    } else {
        Some(delta / before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_outputs_unchanged() {
        let out = OutputTable {
            life: 100.0,
            dps: 5000.0,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&out, &out, DEFAULT_EPSILON);
        assert!(cmp.is_unchanged());
    }

    #[test]
    fn detects_dps_change() {
        let before = OutputTable {
            dps: 1000.0,
            ..OutputTable::default()
        };
        let after = OutputTable {
            dps: 1500.0,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&before, &after, DEFAULT_EPSILON);
        let diff = cmp.field("dps").expect("dps diff");
        assert_eq!(diff.delta, 500.0);
        assert_eq!(diff.relative, Some(0.5));
    }

    #[test]
    fn relative_none_when_before_zero() {
        let before = OutputTable::default();
        let after = OutputTable {
            life: 50.0,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&before, &after, DEFAULT_EPSILON);
        let diff = cmp.field("life").expect("life diff");
        assert_eq!(diff.relative, None);
    }

    #[test]
    fn within_epsilon_ignored() {
        let before = OutputTable {
            life: 100.0,
            ..OutputTable::default()
        };
        let after = OutputTable {
            life: 100.0 + 1e-12,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&before, &after, DEFAULT_EPSILON);
        assert!(cmp.is_unchanged());
    }
}
