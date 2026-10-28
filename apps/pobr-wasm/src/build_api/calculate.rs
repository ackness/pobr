//! Full build calculation and independently evaluated skill metrics.
//! A selected skill controls the detailed panel, not whether other skills can calculate.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use pobr_build::breakdown::detail::{
    DetailKind, StatDetail, detail_descriptors_ref, minion_detail_descriptors, minion_stat_detail,
    stat_detail,
};
use pobr_build::breakdown::{
    BREAKDOWN_STATS, BreakdownActor, MainSkillBreakdown, main_skill_breakdown_in_view,
};
use pobr_core::calc::CompletedCalculationSession;
use serde::Serialize;

use super::request::{
    CalculateBuildRequest, apply_request_overrides, import_build_from_request, orchestrator_options,
};
use crate::state;

// 0.2 + 0.3 calculate_build_json (the full display_catalog plus breakdown;
// the breakdown types and their JSON shape live in `pobr_build::breakdown`)

/// What a full calculation says regardless of which breakdowns are asked for: the
/// completed session (the breakdown queries run against it), the values read from
/// it, and the facts about the build it calculated. A request that differs from an
/// earlier one only in its breakdown selection reuses this and produces just the
/// breakdowns it asks for.
struct CalculationSnapshot {
    session: CompletedCalculationSession,
    provenance: pobr_build::provenance::CalculationProvenance,
    main_minion_index: Option<usize>,
    stats: Vec<pobr_data::display_stat::DisplayStatValue>,
    config_context: Option<super::config_context::ConfigContext>,
    unsupported_modifiers: Vec<String>,
    main_skill: Option<MainSkillBreakdown>,
    item_errors: Vec<super::request::SlotIssue>,
    tree_effects: pobr_build::jewel_tree::PassiveJewelState,
}

#[derive(Debug, Serialize)]
struct CalculateBuildResponse<'a> {
    provenance: &'a pobr_build::provenance::CalculationProvenance,
    minions: Vec<MinionOutputJson>,
    main_minion_index: Option<usize>,
    /// Absent unless requested. Rows include counts and their allocated DPS.
    #[serde(skip_serializing_if = "Option::is_none")]
    full_dps: Option<&'a pobr_core::calc::output::FullDpsOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    energy_trigger: Option<&'a pobr_core::calc::energy::EnergyTriggerTerms>,
    /// The full set of display_catalog Computed fields (id/value/category).
    stats: &'a [pobr_data::display_stat::DisplayStatValue],
    /// Only requested by a client displaying configuration applicability.
    #[serde(skip_serializing_if = "Option::is_none")]
    config_context: Option<&'a super::config_context::ConfigContext>,
    /// Unparsed modifier text and confirmed source-specific calculation limits,
    /// preserved verbatim for display and baseline-versus-edit comparison.
    unsupported_modifiers: &'a [String],
    /// The mod-line breakdown for aggregated stats (keyed by display stat id, see [`BREAKDOWN_STATS`]).
    breakdowns: BTreeMap<String, StatDetail>,
    /// Present only when the request asks for it (`include_descriptors`); the table is
    /// otherwise available once from [`breakdown_descriptors_json`].
    #[serde(skip_serializing_if = "Option::is_none")]
    breakdown_descriptors: Option<&'static BTreeMap<String, DetailKind>>,
    /// The main skill's identity plus damage breakdown (`null` = the build has no resolvable damage main skill).
    main_skill: Option<&'a MainSkillBreakdown>,
    /// Degraded records for a single equipment/flask/jewel item's text
    /// failing to parse (that item is skipped, everything else still
    /// calculates; the frontend flags it red by slot). An empty array means
    /// everything parsed successfully.
    item_errors: &'a [super::request::SlotIssue],
    tree_effects: &'a pobr_build::jewel_tree::PassiveJewelState,
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

/// The kind of explanation each display stat has (`steps`, `sources`, `derived`, `none`).
/// It depends only on the engine build, not on the data or a build, so it is available
/// before any data is loaded and a client needs it once. Calculation responses carry it
/// only on request (`include_descriptors`).
pub fn breakdown_descriptors_json() -> Result<String, String> {
    serde_json::to_string(detail_descriptors_ref()).map_err(|e| format!("serialize: {e}"))
}

/// 0.2 + 0.3: full build calculation -> the full display_catalog key/values
/// plus breakdown plus main-skill damage decomposition.
///
/// Requires game data to be initialized first (the `init` family of entry points).
pub fn calculate_build_json(request_json: &str) -> Result<String, String> {
    let key = selection_cache_key(request_json, true, false);
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
///
/// `ignore_detail_selection` also drops which breakdowns and which actor's are
/// asked for, and whether the descriptor table is wanted: those choose what is
/// explained or sent, never what is calculated.
fn selection_cache_key(
    request_json: &str,
    keep_main: bool,
    ignore_detail_selection: bool,
) -> String {
    /// Request fields that shape the response without changing the calculation.
    const DETAIL_FIELDS: [&str; 3] = ["breakdowns", "breakdown_actor", "include_descriptors"];
    let Ok(serde_json::Value::Object(mut request)) = serde_json::from_str(request_json) else {
        return request_json.to_string();
    };
    // A request with no detail fields to drop stays keyed by its exact text, as
    // before: benchmarks rely on a different spelling of the same request being a
    // different key.
    let drops_detail = ignore_detail_selection
        && DETAIL_FIELDS
            .iter()
            .any(|field| request.contains_key(*field));
    if drops_detail {
        for field in DETAIL_FIELDS {
            request.remove(field);
        }
    }
    let main = request
        .get("main_socket_group")
        .and_then(serde_json::Value::as_u64);
    if keep_main && main.is_none() {
        return if drops_detail {
            serde_json::Value::Object(request).to_string()
        } else {
            request_json.to_string()
        };
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

/// The most recent calculations, newest last. Opening a stat's explanation asks for
/// the same calculation again with one more breakdown; keeping the last two lets that
/// follow-up (and a return to the previous build) skip the calculation. A session
/// holds the character's modifier database, so the count stays small.
const SNAPSHOT_CAP: usize = 2;

thread_local! {
    static SNAPSHOTS: RefCell<VecDeque<(String, Rc<CalculationSnapshot>)>> =
        const { RefCell::new(VecDeque::new()) };
}

/// Drops the retained calculations; results depend on the game data version.
pub(crate) fn clear_snapshots() {
    SNAPSHOTS.with_borrow_mut(VecDeque::clear);
}

fn cached_snapshot(
    key: String,
    calculate: impl FnOnce() -> Result<CalculationSnapshot, super::ApiError>,
) -> Result<Rc<CalculationSnapshot>, super::ApiError> {
    let hit = SNAPSHOTS.with_borrow_mut(|snapshots| {
        let position = snapshots.iter().position(|(cached, _)| *cached == key)?;
        let entry = snapshots.remove(position)?;
        let snapshot = entry.1.clone();
        snapshots.push_back(entry);
        Some(snapshot)
    });
    if let Some(snapshot) = hit {
        return Ok(snapshot);
    }
    let snapshot = Rc::new(calculate()?);
    SNAPSHOTS.with_borrow_mut(|snapshots| {
        if snapshots.len() >= SNAPSHOT_CAP {
            snapshots.pop_front();
        }
        snapshots.push_back((key, snapshot.clone()));
    });
    Ok(snapshot)
}

/// Imports the request's build, calculates it and gathers everything the response
/// reads from the result except the breakdowns.
fn calculate_snapshot(
    request_json: &str,
    req: &CalculateBuildRequest,
) -> Result<CalculationSnapshot, super::ApiError> {
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let (mut build, mut item_errors) = import_build_from_request(req)?;
    item_errors.extend(apply_request_overrides(&mut build, req, &data)?);
    let options = orchestrator_options(req)?;
    let (mut session, config_context, prepared) = if req.config_context {
        let (session, prepared) =
            pobr_build::calc_orchestrator::calculate_with_data_session_tracked(
                &build, &data, &options,
            )
            .map_err(|e| format!("calculate: {e}"))?;
        let context = super::config_context::collect(&prepared, &data, &session);
        (session, Some(context), std::borrow::Cow::Owned(prepared))
    } else {
        let (session, prepared) =
            pobr_build::calculate_with_data_session_prepared(&build, &data, &options)
                .map_err(|e| format!("calculate: {e}"))?;
        (session, None, prepared)
    };
    if req.full_dps {
        // Inspecting a different effect changes the main panel, not which effects
        // Full DPS includes. Reuse the roll-up across those selection changes.
        let full = state::cached_response_keyed(
            "full_dps",
            selection_cache_key(request_json, false, false),
            || {
                let full = super::request::full_dps_for_build(&build, &data, req)
                    .map_err(|e| e.into_json())?;
                serde_json::to_string(&full).map_err(|e| format!("serialize Full DPS: {e}"))
            },
        )?;
        session = session.with_full_dps(
            serde_json::from_str(&full).map_err(|e| format!("read cached Full DPS: {e}"))?,
        );
    }
    let stats = pobr_core::extract_display_values(session.output());
    // The facts below are read from the view the calculation used, rather than from
    // a build prepared again for each of them.
    let main_minion_index =
        pobr_build::main_skill_minion_in_view(&prepared, &data, session.output()).and_then(
            |main| {
                session
                    .output()
                    .minions
                    .iter()
                    .position(|minion| std::ptr::eq(main, minion))
            },
        );
    let provenance = pobr_build::provenance::CalculationProvenance::from_data(
        state::game_data()
            .map_err(super::ApiError::not_initialized)?
            .as_ref(),
        env!("CARGO_PKG_VERSION"),
    )
    .map_err(|e| format!("data identity: {e}"))?;
    Ok(CalculationSnapshot {
        provenance,
        main_minion_index,
        stats,
        config_context,
        unsupported_modifiers: session.unsupported_modifier_texts().to_vec(),
        main_skill: main_skill_breakdown_in_view(&prepared, &data, session.output()),
        item_errors,
        tree_effects: pobr_build::calc_orchestrator::passive_jewel_state_in_view(&prepared, &data),
        session,
    })
}

fn calculate_build_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let snapshot = cached_snapshot(selection_cache_key(request_json, true, true), || {
        calculate_snapshot(request_json, &req)
    })?;
    let session = &snapshot.session;
    let requested: Vec<&str> = req.breakdowns.as_ref().map_or_else(
        || BREAKDOWN_STATS.to_vec(),
        |ids| ids.iter().map(String::as_str).collect(),
    );
    let descriptors = detail_descriptors_ref();
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
        collect_breakdowns(requested, |id| stat_detail(session, id))
    };
    let response = CalculateBuildResponse {
        provenance: &snapshot.provenance,
        minions: minion_outputs(session, &data, &req),
        main_minion_index: snapshot.main_minion_index,
        full_dps: session.output().full_dps.as_ref(),
        energy_trigger: session.output().energy_trigger.as_ref(),
        stats: &snapshot.stats,
        config_context: snapshot.config_context.as_ref(),
        unsupported_modifiers: &snapshot.unsupported_modifiers,
        breakdowns,
        breakdown_descriptors: req.include_descriptors.then_some(descriptors),
        main_skill: snapshot.main_skill.as_ref(),
        item_errors: &snapshot.item_errors,
        tree_effects: &snapshot.tree_effects,
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
    let key = selection_cache_key(request_json, false, false);
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

    #[test]
    fn the_snapshot_key_ignores_only_which_breakdowns_are_asked_for() {
        let base = r#"{"pob_code":"x","main_socket_group":1}"#;
        let with_details = r#"{"pob_code":"x","main_socket_group":1,"breakdowns":["Life"],"breakdown_actor":{"kind":"player"}}"#;
        let edited = r#"{"pob_code":"y","main_socket_group":1,"breakdowns":["Life"]}"#;
        let key = |json: &str| selection_cache_key(json, true, true);
        assert_eq!(key(base), key(with_details));
        assert_ne!(key(base), key(edited));
        // Without a main group the whole request is the key, minus the detail choice.
        let no_main = r#"{"pob_code":"x"}"#;
        let no_main_details = r#"{"pob_code":"x","breakdowns":[]}"#;
        assert_eq!(key(no_main), key(no_main_details));
        // The exact-response key still tells those requests apart.
        assert_ne!(
            selection_cache_key(no_main, true, false),
            selection_cache_key(no_main_details, true, false)
        );
    }
}
