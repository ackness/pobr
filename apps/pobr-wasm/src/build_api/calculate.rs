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

/// The displayed stats the breakdown covers (PoB2's always-present sidebar
/// stats). Each stat is evaluated over the modifier names its calculation
/// stage aggregates (`CalculationSession::stat_mod_names`). `TotalDPS` and
/// `TotalHitAvg` have no single aggregation: they carry steps only, and the
/// average hit a per-damage-type table (`damage`). `CombinedDPS` and
/// `TotalDotDPS` likewise carry steps only, the latter with the skill's own
/// damage over time per type (`skill_dot`).
const BREAKDOWN_MOD_NAMES: &[&str] = &[
    "CombinedDPS",
    "TotalDPS",
    "TotalHitAvg",
    "TotalDotDPS",
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
    "ActionRate",
    "CritChance",
    "CritMultiplier",
    "Accuracy",
    "HitChance",
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
    /// How the calculation gets from the starting value to the displayed
    /// stat (`null` = the steps are not available for this stat).
    formula: Option<FormulaJson>,
    /// `TotalHitAvg` only: how each damage type of the hit is built.
    #[serde(skip_serializing_if = "Option::is_none")]
    damage: Option<HitDamageTableJson>,
    /// `TotalDPS` only: the `enemy_mitigation` step per damage type.
    #[serde(skip_serializing_if = "Option::is_none")]
    mitigation: Option<Vec<HitMitigationRowJson>>,
    /// `TotalDotDPS` only: the `skill_dot` step per damage type.
    #[serde(skip_serializing_if = "Option::is_none")]
    skill_dot: Option<Vec<SkillDotRowJson>>,
}

fn mod_entries(rows: &[pobr_core::ModEvaluation<'_>]) -> Vec<BreakdownModJson> {
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
    entries
}

/// One damage type of the main skill's hit, before the enemy's mitigation.
#[derive(Debug, Serialize)]
struct HitDamageRowJson {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    damage_type: String,
    /// The range before increases: the weapon or skill base, added damage
    /// and what conversion and gain moved into this type.
    base_min: f64,
    base_max: f64,
    /// The INC total (in percentage points).
    increased: f64,
    /// The MORE product, as a multiplier.
    more: f64,
    /// MORE multipliers that apply to the minimum or the maximum only.
    more_min: f64,
    more_max: f64,
    /// The range of a non-critical hit:
    /// `base × (1 + increased / 100) × more × more_min|max × damage_effect`.
    min: f64,
    max: f64,
    /// The average non-critical hit (`(min + max) / 2` unless rolls are lucky).
    avg: f64,
    /// The average critical hit.
    crit_avg: f64,
    /// The INC / MORE lines that scale this type only (its own damage name
    /// and, for an element, elemental damage), inactive lines included.
    /// With `shared_mods`, the active INC values add up to `increased` and
    /// the active MORE values multiply to `more` (which the calculation
    /// rounds to two decimals per modifier name).
    mods: Vec<BreakdownModJson>,
}

#[derive(Debug, Serialize)]
struct HitDamageTableJson {
    rows: Vec<HitDamageRowJson>,
    /// The INC / MORE lines that scale every damage type of the hit
    /// (damage, attack / spell / projectile / area / melee damage and the
    /// skill's keyword damage names), inactive lines included.
    shared_mods: Vec<BreakdownModJson>,
    /// The double / triple damage multiplier every range includes (`1.0` without one).
    damage_effect: f64,
}

/// One damage type of the average hit against the configured enemy.
#[derive(Debug, Serialize)]
struct HitMitigationRowJson {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    damage_type: String,
    /// The type's share of the average hit (crits included), before the enemy.
    average: f64,
    /// The enemy's resistance before penetration (`null` for physical).
    resist: Option<f64>,
    /// What a non-critical hit is reduced by, in percent: the resistance
    /// after penetration, or for physical the reduction from the enemy's
    /// armour, flat reduction and Overwhelm. Negative means more damage.
    reduction: f64,
    /// The enemy's damage-taken multiplier for this type.
    taken: f64,
    /// `mitigated / average`. Equals `taken × (1 − reduction / 100)` unless
    /// critical hits are mitigated differently (armour depends on hit size).
    multiplier: f64,
    /// The share after the enemy. The rows add up to the result of the
    /// `enemy_mitigation` step.
    mitigated: f64,
}

/// One damage type of the skill's own damage over time (not an ailment).
#[derive(Debug, Serialize)]
struct SkillDotRowJson {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    damage_type: String,
    /// Damage per second before scaling, from the skill gem.
    base: f64,
    /// The INC total (in percentage points).
    increased: f64,
    /// The MORE product, as a multiplier.
    more: f64,
    /// The damage over time multiplier (in percentage points).
    dot_multiplier: f64,
    /// The aura effect multiplier (`1.0` unless the skill is an aura).
    aura: f64,
    /// The enemy's resistance and damage taken as one multiplier.
    enemy: f64,
    /// `base × (1 + increased / 100) × more × (1 + dot_multiplier / 100) × aura × enemy`.
    /// The rows add up to the operand of the `skill_dot` step.
    total: f64,
    /// The INC / MORE damage lines and the BASE damage over time multiplier
    /// lines this type reads, inactive lines included. The active INC values
    /// add up to `increased`, the active MORE values multiply to `more`
    /// (rounded to two decimals per modifier name by the calculation).
    mods: Vec<BreakdownModJson>,
}

#[derive(Debug, Serialize)]
struct FormulaStepJson {
    /// `base` / `flat` / `enemy_flat` / `converted` / `increased` / `more` / `added_time` /
    /// `action_speed` / `override` / `cap` / `cooldown_cap` /
    /// `server_tick_cap` / `floor` / `bonus_to_multiplier` / `gear_adjusted` /
    /// `gained` / `slot_scaled` / `flat_after` / `round` / `hit_chance` /
    /// `lucky` / `bifurcate` / `inevitable` / `inevitable_penalty` /
    /// `bifurcate_bonus` / `enemy_increased` / `accuracy_rating` /
    /// `vs_evasion` / `always_hits` / `enemy_block` / `non_crit_hit` /
    /// `crit_hits` / `average_hit` / `enemy_mitigation` / `uses_per_second` /
    /// `chance_to_hit` / `dps_multiplier` / `skill_dot` / `poison_dps` /
    /// `ignite_dps` / `bleed_dps` / `hit_dps` / `dot_dps`.
    op: &'static str,
    /// The amount the step applies (percentage points for `increased`,
    /// `converted`, `hit_chance`, `enemy_block`, `inevitable_penalty`,
    /// `bifurcate_bonus` and `enemy_increased`, the enemy's evasion for
    /// `vs_evasion`, a multiplier for `more` and `action_speed`, seconds
    /// for `added_time`, the limit for caps and the floor, the number of
    /// rolls for `lucky` and `bifurcate`, the percentage of hits that crit
    /// for `crit_hits`, the chance to hit for `chance_to_hit`, a multiplier for `enemy_mitigation`,
    /// `uses_per_second` and `dps_multiplier`, the starting damage per second for
    /// `skill_dot` and `hit_dps`, the amount added otherwise).
    operand: f64,
    /// The running value after the step.
    result: f64,
}

#[derive(Debug, Serialize)]
struct FormulaJson {
    steps: Vec<FormulaStepJson>,
    /// The displayed stat value the steps end at.
    value: f64,
}

fn breakdown_for(session: &CalculationSession, stat: &str) -> Option<BreakdownJson> {
    let rows = session.evaluated_stat_mods(stat);
    let formula = session.stat_formula(stat).map(|formula| FormulaJson {
        steps: formula
            .steps
            .iter()
            .map(|step| FormulaStepJson {
                op: step.op.as_str(),
                operand: step.operand + 0.0,
                result: step.result + 0.0,
            })
            .collect(),
        value: formula.value + 0.0,
    });
    // A stat without modifier lines is still explained by its starting value
    // (a weapon's base crit chance, a skill's base rate).
    if rows.is_empty() && formula.as_ref().is_none_or(|formula| formula.value == 0.0) {
        return None;
    }
    let damage = (stat == "TotalHitAvg")
        .then(|| session.hit_damage_terms())
        .flatten()
        .map(|terms| {
            // The physical row is always computed; leave it out when empty.
            let parts: Vec<_> = terms
                .parts
                .iter()
                .filter(|part| part.base_max != 0.0 || part.max != 0.0)
                .collect();
            let types: Vec<_> = parts.iter().map(|part| part.damage_type).collect();
            let lines = session.hit_damage_mods(&types);
            HitDamageTableJson {
                rows: parts
                    .iter()
                    .zip(&lines.by_type)
                    .map(|(part, (_, mods))| HitDamageRowJson {
                        damage_type: format!("{:?}", part.damage_type),
                        base_min: part.base_min,
                        base_max: part.base_max,
                        increased: part.inc + 0.0,
                        more: part.more,
                        more_min: part.more_min,
                        more_max: part.more_max,
                        min: part.min,
                        max: part.max,
                        avg: part.avg,
                        crit_avg: part.crit_avg,
                        mods: mod_entries(mods),
                    })
                    .collect(),
                shared_mods: mod_entries(&lines.shared),
                damage_effect: terms.damage_effect,
            }
        });
    let mitigation = (stat == "TotalDPS")
        .then(|| session.hit_mitigation_terms())
        .flatten()
        .map(|parts| {
            parts
                .iter()
                .filter(|part| part.average != 0.0)
                .map(|part| HitMitigationRowJson {
                    damage_type: format!("{:?}", part.damage_type),
                    average: part.average,
                    resist: part.resist.map(|resist| resist + 0.0),
                    reduction: part.reduction + 0.0,
                    taken: part.taken,
                    multiplier: part.mitigated / part.average,
                    mitigated: part.mitigated,
                })
                .collect()
        });
    let skill_dot = (stat == "TotalDotDPS")
        .then(|| session.skill_dot_parts())
        .flatten()
        .filter(|parts| !parts.is_empty())
        .map(|parts| {
            let types: Vec<_> = parts.iter().map(|part| part.damage_type).collect();
            let lines = session.skill_dot_mods(&types);
            parts
                .iter()
                .zip(&lines.by_type)
                .map(|(part, (_, mods))| SkillDotRowJson {
                    damage_type: format!("{:?}", part.damage_type),
                    base: part.base,
                    increased: part.inc + 0.0,
                    more: part.more,
                    dot_multiplier: part.dot_multiplier + 0.0,
                    aura: part.aura,
                    enemy: part.enemy,
                    total: part.total,
                    mods: mod_entries(mods),
                })
                .collect()
        });
    let entries = mod_entries(&rows);
    let (base_total, inc_total, more_total) = session.stat_totals(stat);
    Some(BreakdownJson {
        // An empty float sum is `-0.0`; adding zero keeps it out of the JSON.
        base_total: base_total + 0.0,
        inc_total: inc_total + 0.0,
        more_total,
        mods: entries,
        formula,
        damage,
        mitigation,
        skill_dot,
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
    /// The mod-line breakdown for aggregated stats (keyed by display stat id, see [`BREAKDOWN_MOD_NAMES`]).
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
