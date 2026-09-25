//! Execution state owned by one calculation, including trigger sub-calculations.

use super::{
    BuildData, DataOrchestratorOptions, StatMapCatalog, StatMapCompareRecord, StatMapMode,
};
use crate::calc_cache::TriggerMemo;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub(super) struct CalculationContext {
    pub mode: StatMapMode,
    pub catalog: Option<Arc<StatMapCatalog>>,
    pub compare_records: Vec<StatMapCompareRecord>,
    pub is_trigger_source: bool,
    pub trigger_memo: Option<Rc<RefCell<TriggerMemo>>>,
}

impl CalculationContext {
    /// Borrows this context as the engine-semantics [`StatMapCtx`] (catalog + mode +
    /// the Compare-mode record sink), for delegating to `pobr_core::skill_env`
    /// functions without exposing `CalculationContext` to the engine layer.
    pub fn stat_map_ctx(&mut self) -> pobr_core::skill_env::StatMapCtx<'_> {
        pobr_core::skill_env::StatMapCtx {
            catalog: self.catalog.as_deref(),
            mode: self.mode,
            records: if self.mode == StatMapMode::Compare {
                Some(&mut self.compare_records)
            } else {
                None
            },
        }
    }

    pub fn new(data: &BuildData, options: &DataOrchestratorOptions) -> Self {
        Self {
            mode: options.stat_map_mode,
            catalog: options
                .stat_map_catalog
                .clone()
                .or_else(|| data.stat_map_catalog.clone()),
            compare_records: Vec::new(),
            is_trigger_source: false,
            trigger_memo: None,
        }
    }

    pub fn trigger_source(&self) -> Self {
        Self {
            mode: self.mode,
            catalog: self.catalog.clone(),
            compare_records: Vec::new(),
            is_trigger_source: true,
            trigger_memo: None,
        }
    }
}
