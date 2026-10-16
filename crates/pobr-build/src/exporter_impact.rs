//! Isolated effects of rules the reference exporter lacks or handles differently.
//! Other reference-basis models stay fixed, so their differences are not charged
//! to a rule. Absent rules are enabled one at a time; counts are not additive
//! when several rules affect the same row. Different rules remain enabled in
//! both modes and are listed with zero mode-change rows.

use pobr_core::calc::CalculationSession;
use pobr_data::source::ExporterCoverage;
use serde::Serialize;

use crate::{
    Build, BuildData, BuildError, DataOrchestratorOptions, ImportReport,
    calculate_with_data_session, compare_build_to_reference,
};

#[derive(Debug, Clone, Serialize)]
pub struct ExporterRuleImpact {
    pub rule_id: String,
    pub exporter: ExporterCoverage,
    pub game_source: String,
    /// Changed comparable rows, at numerical tolerance, not agreement tolerance.
    pub changed_rows: usize,
}

/// `reference` must be calculated with `options.clone().on_reference_basis()`.
/// Returns no build identity: holdout callers aggregate counts only.
pub fn exporter_rule_impacts(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    import: &ImportReport,
    reference: &CalculationSession,
) -> Result<Vec<ExporterRuleImpact>, BuildError> {
    let rows = compare_build_to_reference(build, data, reference.output(), import);
    reference
        .exporter_rules()
        .map(|rule| {
            let changed_rows = if rule.exporter == ExporterCoverage::Absent {
                let mut options = options.clone().on_reference_basis();
                options
                    .exporter_rule_overrides
                    .insert(rule.id.clone(), true);
                let with_rule = calculate_with_data_session(build, data, &options)?;
                compare_build_to_reference(build, data, with_rule.output(), import)
                    .iter()
                    .filter(|row| {
                        rows.iter()
                            .find(|base| base.id == row.id)
                            .is_some_and(|base| {
                                (row.pobr - base.pobr).abs() > 1e-8 * base.pobr.abs().max(1.0)
                            })
                    })
                    .count()
            } else {
                0
            };
            Ok(ExporterRuleImpact {
                rule_id: rule.id.clone(),
                exporter: rule.exporter,
                game_source: rule.game_source.clone(),
                changed_rows,
            })
        })
        .collect()
}
