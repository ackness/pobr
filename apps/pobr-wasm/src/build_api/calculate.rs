//! Full build calculation and independently evaluated skill metrics.
//! A selected skill controls the detailed panel, not whether other skills can calculate.

use std::collections::BTreeMap;

use pobr_build::breakdown::{
    BREAKDOWN_STATS, MainSkillBreakdown, StatBreakdown, main_skill_breakdown, stat_breakdown,
};
use serde::Serialize;

use super::request::{
    CalculateBuildRequest, apply_request_overrides, import_build_from_request, orchestrator_options,
};
use crate::state;

// 0.2 + 0.3 calculate_build_json (the full display_catalog plus breakdown;
// the breakdown types and their JSON shape live in `pobr_build::breakdown`)

#[derive(Debug, Serialize)]
struct CalculateBuildResponse {
    /// Absent unless requested. Rows include counts and their allocated DPS.
    #[serde(skip_serializing_if = "Option::is_none")]
    full_dps: Option<pobr_core::calc::output::FullDpsOutput>,
    /// The full set of display_catalog Computed fields (id/value/category).
    stats: Vec<pobr_data::display_stat::DisplayStatValue>,
    /// Only requested by a client displaying configuration applicability.
    #[serde(skip_serializing_if = "Option::is_none")]
    config_context: Option<super::config_context::ConfigContext>,
    /// Unparsed modifier text and confirmed source-specific calculation limits,
    /// preserved verbatim for display and baseline-versus-edit comparison.
    unsupported_modifiers: Vec<String>,
    /// The mod-line breakdown for aggregated stats (keyed by display stat id, see [`BREAKDOWN_STATS`]).
    breakdowns: BTreeMap<String, StatBreakdown>,
    /// The main skill's identity plus damage breakdown (`null` = the build has no resolvable damage main skill).
    main_skill: Option<MainSkillBreakdown>,
    /// Degraded records for a single equipment/flask/jewel item's text
    /// failing to parse (that item is skipped, everything else still
    /// calculates; the frontend flags it red by slot). An empty array means
    /// everything parsed successfully.
    item_errors: Vec<super::request::SlotIssue>,
    tree_effects: pobr_build::jewel_tree::PassiveJewelState,
}

/// 0.2 + 0.3: full build calculation -> the full display_catalog key/values
/// plus breakdown plus main-skill damage decomposition.
///
/// Requires game data to be initialized first (the `init` family of entry points).
pub fn calculate_build_json(request_json: &str) -> Result<String, String> {
    let key = selection_cache_key(request_json, true);
    state::cached_response_keyed("calculate_build", key, || {
        calculate_build_impl(request_json).map_err(super::ApiError::into_json)
    })
}

/// A cache key that ignores skill-selection fields the calculation never reads,
/// so switching the inspected skill reuses earlier results.
///
/// Only the main group's `main_active_skill` selects an effect
/// (`resolve::selected_group_effect`); other groups' ordinals are dropped. With
/// `keep_main == false` (per-skill metrics, which evaluate every effect) the
/// main group itself is dropped too. A request without an explicit main group
/// keeps every ordinal: the fallback group is not known here.
fn selection_cache_key(request_json: &str, keep_main: bool) -> String {
    let Ok(serde_json::Value::Object(mut request)) = serde_json::from_str(request_json) else {
        return request_json.to_string();
    };
    let main = request
        .get("main_socket_group")
        .and_then(serde_json::Value::as_u64);
    if keep_main && main.is_none() {
        return request_json.to_string();
    }
    if !keep_main {
        request.remove("main_socket_group");
    }
    if let Some(serde_json::Value::Array(groups)) = request.get_mut("socket_groups") {
        for (index, group) in groups.iter_mut().enumerate() {
            if keep_main && main == Some(index as u64) {
                continue;
            }
            if let serde_json::Value::Object(group) = group {
                group.remove("main_active_skill");
            }
        }
    }
    serde_json::Value::Object(request).to_string()
}

fn calculate_build_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let (mut build, mut item_errors) = import_build_from_request(&req)?;
    item_errors.extend(apply_request_overrides(&mut build, &req, &data)?);
    let options = orchestrator_options(&req)?;
    let (mut session, config_context) = if req.config_context {
        let (session, prepared) =
            pobr_build::calc_orchestrator::calculate_with_data_session_tracked(
                &build, &data, &options,
            )
            .map_err(|e| format!("calculate: {e}"))?;
        let context = super::config_context::collect(&prepared, &data, &session);
        (session, Some(context))
    } else {
        let session = pobr_build::calculate_with_data_session(&build, &data, &options)
            .map_err(|e| format!("calculate: {e}"))?;
        (session, None)
    };
    if req.full_dps {
        // Inspecting a different effect changes the main panel, not which effects
        // Full DPS includes. Reuse the roll-up across those selection changes.
        let full = state::cached_response_keyed(
            "full_dps",
            selection_cache_key(request_json, false),
            || {
                let full = super::request::full_dps_for_build(&build, &data, &req)
                    .map_err(|e| e.into_json())?;
                serde_json::to_string(&full).map_err(|e| format!("serialize Full DPS: {e}"))
            },
        )?;
        session.set_full_dps(
            serde_json::from_str(&full).map_err(|e| format!("read cached Full DPS: {e}"))?,
        );
    }
    let stats = pobr_core::extract_display_values(session.output());
    let breakdowns = BREAKDOWN_STATS
        .iter()
        .filter_map(|name| stat_breakdown(&session, name).map(|b| (name.to_string(), b)))
        .collect();
    let response = CalculateBuildResponse {
        full_dps: session.output().full_dps.clone(),
        stats,
        config_context,
        unsupported_modifiers: session.unsupported_modifier_texts().to_vec(),
        breakdowns,
        main_skill: main_skill_breakdown(&build, &data, session.output()),
        item_errors,
        tree_effects: pobr_build::calc_orchestrator::passive_jewel_state(&build, &data),
    };
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}

#[derive(Debug, Serialize)]
struct SkillMetricValues {
    hit_dps: f64,
    dot_dps: f64,
    combined_dps: f64,
    average_hit: f64,
    action_rate: f64,
    mana_cost: f64,
    life_cost: f64,
    cooldown: f64,
}

#[derive(Debug, Serialize)]
struct SkillMetricsJson {
    group_index: usize,
    active_skill_index: usize,
    skill_id: String,
    status: &'static str,
    metrics: Option<SkillMetricValues>,
    unsupported_modifiers: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SkillMetricsResponse {
    skills: Vec<SkillMetricsJson>,
    item_errors: Vec<super::request::SlotIssue>,
}

/// Evaluate each active effect in its full build context, without requiring a primary
/// skill or summing mutually exclusive skills into a purported combat DPS total.
pub fn skill_metrics_json(request_json: &str) -> Result<String, String> {
    let key = selection_cache_key(request_json, false);
    state::cached_response_keyed("skill_metrics", key, || {
        skill_metrics_impl(request_json).map_err(super::ApiError::into_json)
    })
}

fn skill_metrics_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let (mut build, mut item_errors) = import_build_from_request(&req)?;
    item_errors.extend(apply_request_overrides(&mut build, &req, &data)?);
    let opts = orchestrator_options(&req)?;
    let report = pobr_build::calculate_skill_metrics(&build, &data, &opts)
        .map_err(|e| format!("calculate: {e}"))?;
    let response = SkillMetricsResponse {
        skills: report
            .skills
            .into_iter()
            .map(|skill| {
                let mut error = skill.error;
                let metrics = skill.output.and_then(|output| {
                    let values = [
                        output.dps,
                        output.total_dot_dps,
                        output.combined_dps,
                        output.total_hit_avg,
                        output.effective_action_rate,
                        output.mana_cost,
                        output.life_cost,
                        output.cooldown,
                    ];
                    if values.iter().any(|value| !value.is_finite()) {
                        error = Some("Skill calculation produced non-finite metrics".to_string());
                        return None;
                    }
                    Some(SkillMetricValues {
                        hit_dps: output.dps,
                        dot_dps: output.total_dot_dps,
                        combined_dps: output.combined_dps,
                        average_hit: output.total_hit_avg,
                        action_rate: output.effective_action_rate,
                        mana_cost: output.mana_cost,
                        life_cost: output.life_cost,
                        cooldown: output.cooldown,
                    })
                });
                SkillMetricsJson {
                    group_index: skill.group_index,
                    active_skill_index: skill.active_skill_index,
                    skill_id: skill.skill_id,
                    status: if metrics.is_some() {
                        "calculated"
                    } else {
                        "unavailable"
                    },
                    metrics,
                    unsupported_modifiers: skill.unsupported_modifiers,
                    error,
                }
            })
            .collect(),
        item_errors,
    };
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}
