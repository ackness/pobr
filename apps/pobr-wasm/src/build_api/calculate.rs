//! Full build calculation: `calculate_build_json` (the full display_catalog
//! plus breakdown plus main-skill damage decomposition) and `full_dps_json`
//! (per-socket-group DPS plus the FullDPS summary).

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
    /// The numeric view (`null` for Flag/Text mod lines).
    value: Option<f64>,
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
    /// The BASE mod-line total (a direct sum, with no expression detail beyond class/base injection).
    base_total: f64,
    /// The INC mod-line total (in percentage points).
    inc_total: f64,
    /// The per-mod-line source list.
    mods: Vec<BreakdownModJson>,
}

fn breakdown_for(session: &CalculationSession, name: &str) -> Option<BreakdownJson> {
    let mods = session.mods_named(name);
    if mods.is_empty() {
        return None;
    }
    let mut base_total = 0.0;
    let mut inc_total = 0.0;
    let mut entries: Vec<BreakdownModJson> = mods
        .iter()
        .map(|m| {
            let value = m.value.as_number();
            match m.mod_type {
                pobr_data::modifier::ModType::Base => base_total += value.unwrap_or(0.0),
                pobr_data::modifier::ModType::Inc => inc_total += value.unwrap_or(0.0),
                _ => {}
            }
            BreakdownModJson {
                mod_type: m.mod_type.as_trace_label(),
                value,
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
    });
    Some(BreakdownJson {
        base_total,
        inc_total,
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
    let (group_index, skill_id) = pobr_build::resolve_main_skill_selection(build, data)?;
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
    /// Modifier text that couldn't be parsed (output directly to the frontend's hint area).
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
}

/// 0.2 + 0.3: full build calculation -> the full display_catalog key/values
/// plus breakdown plus main-skill damage decomposition.
///
/// Requires game data to be initialized first (the `init` family of entry points).
pub fn calculate_build_json(request_json: &str) -> Result<String, String> {
    state::cached_response("calculate_build", request_json, || {
        calculate_build_impl(request_json).map_err(super::ApiError::into_json)
    })
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
    };
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}

// full_dps_json (per-socket-group DPS plus the FullDPS summary)

#[derive(Debug, Serialize)]
struct SkillDpsJson {
    /// The skill group's index (0-based, aligned with socket_groups).
    group_index: usize,
    /// That group's active skill's granted effect id.
    skill_id: String,
    dps: f64,
}

#[derive(Debug, Serialize)]
struct FullDpsResponse {
    /// The sum of CombinedDPS across every enabled damage skill group.
    full_dps: f64,
    per_skill: Vec<SkillDpsJson>,
}

/// Per-socket-group DPS (the request shape is the same as [`CalculateBuildRequest`]).
///
/// Computation cost = `1 + the number of enabled damage groups` full
/// orchestration passes; feeds the click-triggered per-skill DPS panel, and
/// isn't called on every recalculation (the same pattern as attribution).
pub fn full_dps_json(request_json: &str) -> Result<String, String> {
    state::cached_response("full_dps", request_json, || {
        full_dps_impl(request_json).map_err(super::ApiError::into_json)
    })
}

fn full_dps_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let mut build = parse_build_from_request(&req)?;
    // Degraded records don't go into this response — the main panel's
    // calculate already reports the same item_errors.
    let _ = apply_request_overrides(&mut build, &req, &data)?;
    let opts = orchestrator_options(&req)?;
    let report = pobr_build::calculate_full_dps(&build, &data, &opts)
        .map_err(|e| format!("calculate: {e}"))?;
    let response = FullDpsResponse {
        full_dps: report.full_dps,
        per_skill: report
            .per_skill
            .into_iter()
            .map(|s| SkillDpsJson {
                group_index: s.group_index,
                skill_id: s.skill_id,
                dps: s.combined_dps,
            })
            .collect(),
    };
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}
