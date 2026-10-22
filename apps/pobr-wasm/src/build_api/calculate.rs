//! Full build calculation and independently evaluated skill metrics.
//! A selected skill controls the detailed panel, not whether other skills can calculate.

use std::collections::BTreeMap;

use pobr_build::breakdown::detail::{
    DetailKind, StatDetail, detail_descriptors, minion_detail_descriptors, minion_stat_detail,
    stat_detail,
};
use pobr_build::breakdown::{
    BREAKDOWN_STATS, BreakdownActor, MainSkillBreakdown, main_skill_breakdown,
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
    provenance: pobr_build::provenance::CalculationProvenance,
    minions: Vec<MinionOutputJson>,
    main_minion_index: Option<usize>,
    /// Absent unless requested. Rows include counts and their allocated DPS.
    #[serde(skip_serializing_if = "Option::is_none")]
    full_dps: Option<pobr_core::calc::output::FullDpsOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    energy_trigger: Option<pobr_core::calc::energy::EnergyTriggerTerms>,
    /// The full set of display_catalog Computed fields (id/value/category).
    stats: Vec<pobr_data::display_stat::DisplayStatValue>,
    /// Only requested by a client displaying configuration applicability.
    #[serde(skip_serializing_if = "Option::is_none")]
    config_context: Option<super::config_context::ConfigContext>,
    /// Unparsed modifier text and confirmed source-specific calculation limits,
    /// preserved verbatim for display and baseline-versus-edit comparison.
    unsupported_modifiers: Vec<String>,
    /// The mod-line breakdown for aggregated stats (keyed by display stat id, see [`BREAKDOWN_STATS`]).
    breakdowns: BTreeMap<String, StatDetail>,
    breakdown_descriptors: BTreeMap<String, DetailKind>,
    /// The main skill's identity plus damage breakdown (`null` = the build has no resolvable damage main skill).
    main_skill: Option<MainSkillBreakdown>,
    /// Degraded records for a single equipment/flask/jewel item's text
    /// failing to parse (that item is skipped, everything else still
    /// calculates; the frontend flags it red by slot). An empty array means
    /// everything parsed successfully.
    item_errors: Vec<super::request::SlotIssue>,
    tree_effects: pobr_build::jewel_tree::PassiveJewelState,
}

#[derive(Debug, Serialize)]
struct MinionOutputJson {
    index: usize,
    minion_id: String,
    name: String,
    summoned_by: String,
    skill_id: String,
    level: u32,
    dps: f64,
    life: f64,
    armour: f64,
    evasion: f64,
    energy_shield: f64,
    stats: BTreeMap<String, f64>,
    breakdowns: BTreeMap<String, StatDetail>,
    breakdown_descriptors: BTreeMap<String, DetailKind>,
}

fn minion_outputs(
    session: &pobr_core::calc::CalculationSession,
    data: &pobr_build::BuildData,
    req: &CalculateBuildRequest,
) -> Vec<MinionOutputJson> {
    session
        .output()
        .minions
        .iter()
        .enumerate()
        .map(|(index, minion)| {
            let descriptors = minion_detail_descriptors();
            let selected = match req.breakdown_actor {
                Some(BreakdownActor::Minion { index: selected }) => selected == index,
                _ => req.breakdowns.is_none() && req.breakdown_actor.is_none(),
            };
            let requested: Vec<&str> = if selected {
                req.breakdowns.as_ref().map_or_else(
                    || descriptors.keys().map(String::as_str).collect(),
                    |ids| ids.iter().map(String::as_str).collect(),
                )
            } else {
                vec![]
            };
            let breakdowns =
                collect_breakdowns(requested, |stat| minion_stat_detail(session, index, stat));
            // Values come from the completed actor output, never from producing
            // unrequested explanation formulas merely to fill the stat table.
            let stats = session
                .minion_output(index)
                .map(|output| {
                    descriptors
                        .keys()
                        .map(|id| {
                            (
                                id.clone(),
                                pobr_core::display_catalog::display_value(output, id),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            MinionOutputJson {
                index,
                minion_id: minion.minion_id.clone(),
                name: data
                    .minion_def(&minion.minion_id)
                    .map(|def| def.name.clone())
                    .unwrap_or_default(),
                summoned_by: minion.summoned_by.clone(),
                skill_id: minion.skill_id.clone(),
                level: minion.level,
                dps: minion.dps,
                life: minion.life,
                armour: minion.armour,
                evasion: minion.evasion,
                energy_shield: minion.energy_shield,
                stats,
                breakdowns,
                breakdown_descriptors: descriptors,
            }
        })
        .collect()
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
        request.remove("breakdowns");
        request.remove("breakdown_actor");
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
        session = session.with_full_dps(
            serde_json::from_str(&full).map_err(|e| format!("read cached Full DPS: {e}"))?,
        );
    }
    let stats = pobr_core::extract_display_values(session.output());
    let requested: Vec<&str> = req.breakdowns.as_ref().map_or_else(
        || BREAKDOWN_STATS.to_vec(),
        |ids| ids.iter().map(String::as_str).collect(),
    );
    let descriptors = detail_descriptors();
    if let Some(unknown) = requested.iter().find(|id| !descriptors.contains_key(**id)) {
        return Err(super::ApiError::bad_request(format!(
            "unknown breakdown: {unknown}"
        )));
    }
    // Evaluate only selected ids, with duplicates collapsed before any query.
    if let Some(BreakdownActor::Minion { index }) = req.breakdown_actor
        && index >= session.minion_count()
    {
        return Err(super::ApiError::bad_request(format!(
            "unknown breakdown actor: minion {index}"
        )));
    }
    let breakdowns = if matches!(req.breakdown_actor, Some(BreakdownActor::Minion { .. })) {
        BTreeMap::new()
    } else {
        collect_breakdowns(requested, |id| stat_detail(&session, id))
    };
    let main_minion_index =
        pobr_build::reference_check::main_skill_minion(&build, &data, session.output()).and_then(
            |main| {
                session
                    .output()
                    .minions
                    .iter()
                    .position(|minion| std::ptr::eq(main, minion))
            },
        );
    let response = CalculateBuildResponse {
        provenance: pobr_build::provenance::CalculationProvenance::from_data(
            state::game_data()
                .map_err(super::ApiError::not_initialized)?
                .as_ref(),
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|e| format!("data identity: {e}"))?,
        minions: minion_outputs(&session, &data, &req),
        main_minion_index,
        full_dps: session.output().full_dps.clone(),
        energy_trigger: session.output().energy_trigger.clone(),
        stats,
        config_context,
        unsupported_modifiers: session.unsupported_modifier_texts().to_vec(),
        breakdowns,
        breakdown_descriptors: descriptors,
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

/// Selection happens before the producer runs: empty and duplicate requests
/// must not cause detail queries whose output is later discarded.
fn collect_breakdowns<'a, T>(
    ids: impl IntoIterator<Item = &'a str>,
    mut compute: impl FnMut(&str) -> Option<T>,
) -> BTreeMap<String, T> {
    ids.into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|id| compute(id).map(|detail| (id.to_string(), detail)))
        .collect()
}

#[cfg(test)]
mod detail_request_tests {
    use super::*;
    #[test]
    fn unrequested_details_are_never_computed() {
        let mut called = Vec::new();
        let empty = collect_breakdowns([], |id| {
            called.push(id.to_string());
            Some(1)
        });
        assert!(empty.is_empty());
        assert!(called.is_empty());
        let selected = collect_breakdowns(["ManaCost", "Life", "ManaCost"], |id| {
            called.push(id.to_string());
            Some(1)
        });
        assert_eq!(called, ["Life", "ManaCost"]);
        assert_eq!(selected.len(), 2);
    }
}
