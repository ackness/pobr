//! Full build calculation and independently evaluated skill metrics.
//! A selected skill controls the detailed panel, not whether other skills can calculate.

use std::collections::BTreeMap;

use pobr_build::{Build, BuildData};
use pobr_core::calc::CalculationSession;
use serde::Serialize;

use super::request::{
    CalculateBuildRequest, apply_request_overrides, orchestrator_options, parse_build_from_request,
    run_session_for_build,
};
use crate::state;

// 0.2 + 0.3 calculate_build_json (the full display_catalog plus breakdown)

/// The aggregated ModNames the breakdown covers (PoB2's always-present
/// sidebar stats; derived values like TotalDPS have no single aggregation
/// name, so they're not listed here — see them via the attribution endpoint instead).
const BREAKDOWN_MOD_NAMES: &[&str] = &[
    "Life",
    "Mana",
    "EnergyShield",
    "Spirit",
    "Armour",
    "Evasion",
    "FireResist",
    "ColdResist",
    "LightningResist",
    "ChaosResist",
    "Speed",
    "CritChance",
    "CritMultiplier",
    "Accuracy",
    "MovementSpeed",
];

#[derive(Debug, Serialize)]
struct BreakdownModJson {
    /// `BASE` / `INC` / `MORE` / `FLAG` / `OVERRIDE` / `LIST`.
    mod_type: &'static str,
    /// The numeric view (`null` for Flag/Text mod lines). For an active
    /// BASE/INC/MORE line this is the applied number (per-stat scaling and
    /// shared caps included); an inactive line keeps its stored number.
    value: Option<f64>,
    /// Whether the inspected skill's calculation counts this line. `false`
    /// means its condition, skill type or weapon requirement is not met.
    active: bool,
    /// The mod line's raw text (the source it was parsed from).
    source_text: Option<String>,
    /// The attribution source category (`SourceKind`'s Debug name, e.g. `PassiveNode` / `ItemAffix`).
    origin_kind: Option<String>,
    /// The attribution source's stable id (node id / item slot / gem id).
    origin_id: Option<String>,
    /// The source slot (for equipment mod lines).
    slot: Option<String>,
}

#[derive(Debug, Serialize)]
struct BreakdownJson {
    /// The active BASE mod-line total (no class/base injection).
    base_total: f64,
    /// The active INC mod-line total (in percentage points).
    inc_total: f64,
    /// The product of the active MORE mod lines, as a multiplier (`1.0` when there are none).
    more_total: f64,
    /// The per-mod-line source list, inactive lines included.
    mods: Vec<BreakdownModJson>,
}

fn breakdown_for(session: &CalculationSession, name: &str) -> Option<BreakdownJson> {
    let rows = session.evaluated_mods_named(name);
    if rows.is_empty() {
        return None;
    }
    let mut entries: Vec<BreakdownModJson> = rows
        .iter()
        .map(|row| {
            let m = row.modifier;
            BreakdownModJson {
                mod_type: m.mod_type.as_trace_label(),
                value: row.value,
                active: row.active,
                source_text: m.source.clone(),
                origin_kind: m.origin.as_ref().map(|o| format!("{:?}", o.source_id.kind)),
                origin_id: m.origin.as_ref().map(|o| o.source_id.id.clone()),
                slot: m.origin.as_ref().and_then(|o| o.slot.clone()),
            }
        })
        .collect();
    // Establish a fixed order: ModDb iteration order is affected by the
    // underlying HashMap instance, so different data backends can produce
    // different orderings; the output is sorted by (type, origin, mod-line
    // text, value) to keep the contract byte-deterministic and the UI display stable.
    entries.sort_by(|a, b| {
        (a.mod_type, &a.origin_kind, &a.origin_id, &a.source_text)
            .cmp(&(b.mod_type, &b.origin_kind, &b.origin_id, &b.source_text))
            .then(
                a.value
                    .partial_cmp(&b.value)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(b.active.cmp(&a.active))
    });
    let (base_total, inc_total, more_total) = session.named_totals(name);
    Some(BreakdownJson {
        // An empty float sum is `-0.0`; adding zero keeps it out of the JSON.
        base_total: base_total + 0.0,
        inc_total: inc_total + 0.0,
        more_total,
        mods: entries,
    })
}

/// One damage-type component of the main skill's hit damage (a non-crit
/// leg, player-side, before enemy damage reduction — matches PoB2's Calcs
/// page damage-breakdown basis; `avg` is used for share display).
#[derive(Debug, Serialize)]
struct HitDamagePartJson {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    damage_type: String,
    min: f64,
    max: f64,
    /// `(min + max) / 2`.
    avg: f64,
}

/// The main skill's identity (the skill the engine's calculation actually
/// centers on) plus its damage breakdown (the counterpart to PoB2's Main
/// Skill sidebar plus its Calcs damage-breakdown area). Returned with every
/// recalculation — updates instantly on any equipment/passive change.
#[derive(Debug, Serialize)]
struct MainSkillJson {
    /// The selected skill group (0-based, aligned with the request's `socket_groups`).
    group_index: usize,
    /// PoB's 1-based ordinal among all active effects, including support-granted skills.
    active_skill_index: usize,
    /// That group's main skill's granted effect id.
    skill_id: String,
    /// The hit-damage components, split by damage type.
    hit_damage: Vec<HitDamagePartJson>,
    /// Hit DPS (`TotalDPS`).
    hit_dps: f64,
    /// The total of every damage-over-time source (`TotalDotDPS`).
    dot_dps: f64,
    /// Combined DPS (`CombinedDPS`).
    combined_dps: f64,
}

fn main_skill_json(
    build: &Build,
    data: &BuildData,
    output: &pobr_core::calc::OutputTable,
) -> Option<MainSkillJson> {
    let (group_index, active_skill_index, skill_id) =
        pobr_build::resolve_main_skill_selection(build, data)?;
    let hit_damage = output
        .damage_components
        .iter()
        .filter(|c| c.kind == pobr_data::prelude::DamageKind::Hit)
        .map(|c| HitDamagePartJson {
            damage_type: format!("{:?}", c.damage_type),
            min: c.min,
            max: c.max,
            avg: (c.min + c.max) / 2.0,
        })
        .collect();
    Some(MainSkillJson {
        group_index,
        active_skill_index,
        skill_id,
        hit_damage,
        hit_dps: output.dps,
        dot_dps: output.total_dot_dps,
        combined_dps: output.combined_dps,
    })
}

#[derive(Debug, Serialize)]
struct CalculateBuildResponse {
    /// The full set of display_catalog Computed fields (id/value/category).
    stats: Vec<pobr_data::display_stat::DisplayStatValue>,
    /// Unparsed modifier text and confirmed source-specific calculation limits,
    /// preserved verbatim for display and baseline-versus-edit comparison.
    unsupported_modifiers: Vec<String>,
    /// The mod-line breakdown for aggregated stats (keyed by ModName, see [`BREAKDOWN_MOD_NAMES`]).
    breakdowns: BTreeMap<String, BreakdownJson>,
    /// The main skill's identity plus damage breakdown (`null` = the build has no resolvable damage main skill).
    main_skill: Option<MainSkillJson>,
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
    let mut build = parse_build_from_request(&req)?;
    let item_errors = apply_request_overrides(&mut build, &req, &data)?;
    let session = run_session_for_build(&build, &req)?;
    let stats = pobr_core::extract_display_values(session.output());
    let breakdowns = BREAKDOWN_MOD_NAMES
        .iter()
        .filter_map(|name| breakdown_for(&session, name).map(|b| (name.to_string(), b)))
        .collect();
    let response = CalculateBuildResponse {
        stats,
        unsupported_modifiers: session.unsupported_modifier_texts().to_vec(),
        breakdowns,
        main_skill: main_skill_json(&build, &data, session.output()),
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
    let mut build = parse_build_from_request(&req)?;
    let item_errors = apply_request_overrides(&mut build, &req, &data)?;
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
