//! The ledger of a build's lines ([`pobr_build::usage::line_usage`]) for the
//! request `calculate_build_json` takes: every line of the build's items,
//! limbs, flasks, jewels, allocated nodes and config with what the
//! calculation did with it, and the inputs it dropped.

use pobr_build::usage::LineUsage;
use serde::Serialize;

use super::request::{
    CalculateBuildRequest, SlotIssue, apply_request_overrides, import_build_from_request,
    orchestrator_options,
};
use crate::state;

#[derive(Debug, Serialize)]
struct LineUsageResponse {
    usage: LineUsage,
    /// The items whose text could not be read, as `calculate_build_json`
    /// reports them: the ledger is of a character without them.
    item_errors: Vec<SlotIssue>,
}

/// What the calculation of a `calculate_build_json` request does with each
/// line of its build: applied (and the breakdown stats it reaches), left
/// out and why, never read, read before any modifier is stored, recognised
/// with no effect or not modelled, or not parsed; beside them the gem stats
/// the engine could not apply and the conditions and multipliers nothing set.
///
/// The calculation behind it tracks every read, so it costs more than
/// `calculate_build_json` and is not cached: ask for it when the ledger is
/// wanted, not on every edit.
pub fn line_usage_json(request_json: &str) -> Result<String, String> {
    line_usage_impl(request_json).map_err(super::ApiError::into_json)
}

fn line_usage_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let (mut build, mut item_errors) = import_build_from_request(&req)?;
    item_errors.extend(apply_request_overrides(&mut build, &req, &data)?);
    let options = orchestrator_options(&req)?;
    let diagnosis =
        pobr_build::diagnose(&build, &data, &options).map_err(|e| format!("diagnose: {e}"))?;
    let response = LineUsageResponse {
        usage: pobr_build::usage::line_usage(&diagnosis, &build, &data),
        item_errors,
    };
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}
