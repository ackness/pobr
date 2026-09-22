//! Execution state owned by one calculation, including trigger sub-calculations.

use super::{
    BuildData, DataOrchestratorOptions, StatMapCatalog, StatMapCompareRecord, StatMapMode,
};
use std::sync::Arc;

pub(super) struct CalculationContext {
    pub mode: StatMapMode,
    pub catalog: Option<Arc<StatMapCatalog>>,
    pub compare_records: Vec<StatMapCompareRecord>,
    pub is_trigger_source: bool,
}

impl CalculationContext {
    pub fn new(data: &BuildData, options: &DataOrchestratorOptions) -> Self {
        Self {
            mode: options.stat_map_mode,
            catalog: options
                .stat_map_catalog
                .clone()
                .or_else(|| data.stat_map_catalog.clone()),
            compare_records: Vec::new(),
            is_trigger_source: false,
        }
    }

    pub fn trigger_source(&self) -> Self {
        Self {
            mode: self.mode,
            catalog: self.catalog.clone(),
            compare_records: Vec::new(),
            is_trigger_source: true,
        }
    }
}
