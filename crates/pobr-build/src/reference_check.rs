//! Compares a calculation with reference values recorded under PoB2 output
//! names: the `<PlayerStat>` values a build code carries
//! ([`ImportReport::reference_stats`](crate::ImportReport)) or an oracle dump.
//!
//! The pairs come from [`display_catalog`], so a stat is comparable exactly
//! when the catalog names its PoB2 counterpart. A reference is another
//! tool's result, not the game's: a row that differs says the two tools
//! disagree, not which one is right.

use std::collections::BTreeMap;

use pobr_core::calc::OutputTable;
use pobr_core::display_catalog::{display_catalog, display_value};
use pobr_data::display_stat::ParityStatus;

/// One stat both sides report.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceRow {
    /// Display stat id, as in [`display_catalog`].
    pub id: String,
    /// The reference's name for it.
    pub pob_key: String,
    pub pobr: f64,
    pub reference: f64,
}

impl ReferenceRow {
    /// `pobr / reference`. `None` when the reference is zero or either side is
    /// not finite, where a ratio says nothing.
    pub fn ratio(&self) -> Option<f64> {
        (self.reference != 0.0 && self.reference.is_finite() && self.pobr.is_finite())
            .then(|| self.pobr / self.reference)
    }

    /// Whether the two values agree within `tolerance` of the reference.
    /// Below 1 the tolerance is absolute, so rounding near zero is not a miss.
    pub fn agrees(&self, tolerance: f64) -> bool {
        if self.pobr == self.reference {
            return true;
        }
        self.pobr.is_finite()
            && self.reference.is_finite()
            && (self.pobr - self.reference).abs() <= tolerance * self.reference.abs().max(1.0)
    }
}

/// Catalog pairs whose two sides are defined differently, so comparing them
/// measures the definitions rather than the calculation.
const NOT_COMPARABLE: &[&str] = &[
    // PoBR's value is the hit before the enemy's mitigation and chance to
    // hit; PoB2's `AverageHit` includes both.
    "TotalHitAvg",
    // Same PoB2 key as `ActionRate`; listed once.
    "EffectiveActionRate",
];

/// Every computed catalog stat the reference also reports, in catalog order.
pub fn compare_to_reference(
    output: &OutputTable,
    reference: &BTreeMap<String, f64>,
) -> Vec<ReferenceRow> {
    display_catalog()
        .into_iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .filter(|def| !NOT_COMPARABLE.contains(&def.id.as_str()))
        .filter_map(|def| {
            let pob_key = def.pob_key?;
            let reference = *reference.get(&pob_key)?;
            Some(ReferenceRow {
                pobr: display_value(output, def.id.as_str()),
                id: def.id.as_str().to_string(),
                pob_key,
                reference,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pobr: f64, reference: f64) -> ReferenceRow {
        ReferenceRow {
            id: "Life".into(),
            pob_key: "Life".into(),
            pobr,
            reference,
        }
    }

    #[test]
    fn agreement_is_relative_above_one_and_absolute_below() {
        assert!(row(1823.85, 1824.0).agrees(0.05));
        assert!(!row(6418.5, 12818.0).agrees(0.05));
        assert!(row(0.0, 0.04).agrees(0.05));
        assert!(!row(0.0, 0.5).agrees(0.05));
        assert!(row(f64::INFINITY, f64::INFINITY).agrees(0.05));
        assert!(!row(100.0, f64::INFINITY).agrees(0.05));
    }

    #[test]
    fn ratio_is_absent_where_it_says_nothing() {
        assert_eq!(row(50.0, 100.0).ratio(), Some(0.5));
        assert_eq!(row(5.0, 0.0).ratio(), None);
        assert_eq!(row(5.0, f64::INFINITY).ratio(), None);
    }

    #[test]
    fn only_stats_the_reference_reports_are_compared() {
        let output = OutputTable {
            life: 1000.0,
            mana: 500.0,
            ..Default::default()
        };
        let reference = BTreeMap::from([("Life".to_string(), 1010.0)]);
        let rows = compare_to_reference(&output, &reference);
        assert_eq!(rows, [row(1000.0, 1010.0)]);
    }
}
