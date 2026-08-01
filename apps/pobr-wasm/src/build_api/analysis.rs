//! What-if analysis endpoints: `node_power_json` (the tree node power
//! heatmap), `optimize_variants_json` (generic variant evaluation), and
//! `attribution_json` (source-contribution attribution). All three share
//! the "a what-if = a full orchestration pass" basis: the baseline build is
//! assembled once, and each variant is a clone with incremental changes stacked on top, fully recalculated.

use std::collections::BTreeMap;

use pobr_build::build::GemSkillRef;
use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data_session};
use pobr_core::calc::CalculationSession;
use pobr_core::item_text::parse_pob_xml_item;
use pobr_data::passive_tree::NodeId;
use serde::{Deserialize, Serialize};

use super::request::{
    CalculateBuildRequest, GemInput, SlotItemInput, apply_request_overrides, orchestrator_options,
    parse_build_from_request, run_session_for_build,
};
use super::{localize_input_text, slot_from_id};
use crate::state;

// node_power_json (the tree node power heatmap: a port of PoB2 CalcsTab:PowerBuilder)

#[derive(Debug, Deserialize)]
struct NodePowerRequest {
    /// The full calculation request (baseline).
    request: CalculateBuildRequest,
    /// The target display stat id (e.g. `TotalDPS` / `Life` / `TotalEHP`).
    power_stat: String,
    /// The max BFS depth from the allocated frontier (PoB2's nodePowerMaxDepth; defaults to 5).
    max_depth: Option<u32>,
}

#[derive(Debug, Serialize)]
struct NodePowerEntry {
    /// The node's skill id.
    skill: u32,
    /// The target stat's delta after a single-point trial allocation (can be negative).
    delta: f64,
    /// The distance (in steps) from the frontier (1 = adjacent to an allocated node).
    distance: u32,
}

#[derive(Debug, Serialize)]
struct NodePowerResponse {
    /// The baseline stat value.
    base: f64,
    entries: Vec<NodePowerEntry>,
}

/// Extracts a display stat value from the full output (defaults to 0 if missing).
fn display_stat_value(session: &CalculationSession, stat_id: &str) -> f64 {
    pobr_core::extract_display_values(session.output())
        .into_iter()
        .find(|s| s.id.as_str() == stat_id)
        .map(|s| s.value)
        .unwrap_or(0.0)
}

/// Passive node power (PoB2 heatmap semantics): runs a BFS from the
/// allocated node set as the frontier, and for every unallocated,
/// stat-carrying node within depth, does a full recalculation with that
/// single node trial-allocated, producing the target stat's delta. Nodes
/// sharing the same stat combination share one calculation (matching PoB2's
/// modKey caching basis); attribute-choice small nodes (which need a
/// three-way choice) are skipped.
pub fn node_power_json(request_json: &str) -> Result<String, String> {
    state::cached_response("node_power", request_json, || {
        node_power_impl(request_json).map_err(super::ApiError::into_json)
    })
}

fn node_power_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: NodePowerRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let max_depth = req.max_depth.unwrap_or(5);
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;

    let mut base_build = parse_build_from_request(&req.request)?;
    // Degraded records don't go into this response — the main panel's
    // calculate already reports the same item_errors.
    let _ = apply_request_overrides(&mut base_build, &req.request, &data)?;
    let base_session = run_session_for_build(&base_build, &req.request)?;
    let base = display_stat_value(&base_session, &req.power_stat);

    // Topology: skill id -> node definition plus undirected adjacency.
    let mut by_skill: BTreeMap<u32, &pobr_data::catalog::PassiveNodeDef> = BTreeMap::new();
    let mut adjacency: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for node in data.passive_nodes.values() {
        by_skill.insert(node.skill, node);
        for &target in &node.connections {
            adjacency.entry(node.skill).or_default().push(target);
            adjacency.entry(target).or_default().push(node.skill);
        }
    }

    let allocated: std::collections::HashSet<u32> = base_build
        .tree
        .allocated_nodes
        .iter()
        .map(|n| n.0)
        .collect();

    // BFS: the frontier = the allocated node set (depth 0), expanding layer by layer into unallocated nodes.
    let mut distance: BTreeMap<u32, u32> = BTreeMap::new();
    let mut frontier: Vec<u32> = allocated.iter().copied().collect();
    for depth in 1..=max_depth {
        let mut next = Vec::new();
        for &skill in &frontier {
            for &neighbor in adjacency.get(&skill).map(Vec::as_slice).unwrap_or(&[]) {
                if allocated.contains(&neighbor) || distance.contains_key(&neighbor) {
                    continue;
                }
                distance.insert(neighbor, depth);
                next.push(neighbor);
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    // Single-node trial allocation: nodes sharing the same stat combination share one full recalculation.
    let mut cache: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut entries: Vec<NodePowerEntry> = Vec::new();
    for (&skill, &dist) in &distance {
        let Some(node) = by_skill.get(&skill) else {
            continue;
        };
        if node.stats.is_empty() || node.name.as_deref() == Some("Attribute") {
            continue;
        }
        let key = node.stats.join("\n");
        let delta = match cache.get(&key) {
            Some(&d) => d,
            None => {
                let mut variant = base_build.clone();
                variant.tree.allocated_nodes.push(NodeId(skill));
                let session = run_session_for_build(&variant, &req.request)?;
                let d = display_stat_value(&session, &req.power_stat) - base;
                cache.insert(key, d);
                d
            }
        };
        entries.push(NodePowerEntry {
            skill,
            delta,
            distance: dist,
        });
    }

    Ok(serde_json::to_string(&NodePowerResponse { base, entries })
        .map_err(|e| format!("serialize: {e}"))?)
}

// optimize_variants_json (generic variant evaluation: the compute side of the optimization framework)
//
// Division-of-labor contract: Rust only does the expensive part — each
// variant stacks a set of incremental changes on top of the baseline build
// and does a full recalculation, returning display stat values; scoring /
// constraints / sorting happen on the frontend in
// `web/src/lib/optimize.ts`, so switching the target objective re-sorts
// instantly with zero recalculation. Gems/equipment/passives/arbitrary mod
// text all share this one channel.

/// Appends gems to a skill group (the gem-combination optimization channel).
#[derive(Debug, Deserialize)]
struct AddGemsInput {
    /// The target skill group (0-based, aligned with the request's socket_groups).
    group_index: usize,
    gems: Vec<GemInput>,
}

/// A single variant: each channel can be stacked arbitrarily; all-empty = recompute the baseline.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct VariantInput {
    /// An echo label (for display in the frontend's result rows; not consumed by the calculation).
    label: Option<String>,
    add_gems: Option<AddGemsInput>,
    /// Overrides an equipment slot (an empty `text` means unequip that slot).
    set_items: Vec<SlotItemInput>,
    /// Additional allocated nodes (connectivity isn't validated — this is a
    /// hypothetical what-if, so pathing is the caller's responsibility).
    allocate_nodes: Vec<u32>,
    deallocate_nodes: Vec<u32>,
    /// Arbitrary mod text (the catch-all channel: "as long as there's a mod
    /// line, it can be calculated"; Chinese lines are automatically reverse-translated to English).
    extra_modifiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OptimizeVariantsRequest {
    /// The full calculation request (baseline; overrides apply as usual).
    request: CalculateBuildRequest,
    /// The display stat ids to collect (display_catalog; an unknown id records 0).
    stats: Vec<String>,
    variants: Vec<VariantInput>,
    /// Defaults to true; the frontend turns this off in later batches when
    /// calling in batches, to skip one baseline calculation.
    include_baseline: Option<bool>,
}

#[derive(Debug, Serialize)]
struct VariantStatsJson {
    /// The index within the request (lets the frontend map back to a variant's definition).
    index: usize,
    label: Option<String>,
    /// An empty table plus an error when the calculation fails (a single
    /// variant failing doesn't take down the whole batch).
    stats: BTreeMap<String, f64>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct OptimizeVariantsResponse {
    baseline: Option<BTreeMap<String, f64>>,
    variants: Vec<VariantStatsJson>,
}

/// The cap on variants per call. wasm runs single-threaded and
/// synchronously, so a batch that's too large makes the UI unresponsive —
/// the frontend calls in small batches instead, yielding the main thread between them (with progress and cancellation).
const VARIANT_CAP: usize = 512;

/// Collects a batch of display stat values from a session.
fn collect_stats(session: &CalculationSession, stat_ids: &[String]) -> BTreeMap<String, f64> {
    stat_ids
        .iter()
        .map(|id| (id.clone(), display_stat_value(session, id)))
        .collect()
}

/// Applies a variant's incremental changes onto copies of the build/orchestration options.
fn apply_variant(
    build: &mut Build,
    opts: &mut DataOrchestratorOptions,
    variant: &VariantInput,
    data: &BuildData,
) -> Result<(), String> {
    if let Some(add) = &variant.add_gems {
        let group_count = build.socket_groups.len();
        let group = build
            .socket_groups
            .get_mut(add.group_index)
            .ok_or_else(|| {
                format!(
                    "group_index {} out of range (build has {group_count} socket groups)",
                    add.group_index
                )
            })?;
        for gem in &add.gems {
            if gem.skill_id.is_empty() {
                continue;
            }
            group.gem_skills.push(GemSkillRef {
                skill_id: gem.skill_id.clone(),
                gem_level: gem.level,
                quality: gem.quality,
                stat_set_index: None,
                name_spec: None,
            });
            if let Some(effect) = data.gem_effects.get(&gem.skill_id) {
                group.gem_ids.push(effect.gem_id.clone());
            }
        }
    }
    for item in &variant.set_items {
        let slot = slot_from_id(&item.slot)?;
        if item.text.trim().is_empty() {
            build.items.remove(&slot);
        } else {
            let text = localize_input_text(&item.text);
            let parsed = parse_pob_xml_item(&text)
                .map_err(|e| format!("parse item in slot {}: {e:?}", item.slot))?;
            build.items.insert(slot, parsed);
        }
    }
    if !variant.allocate_nodes.is_empty() {
        let existing: std::collections::HashSet<u32> =
            build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        build.tree.allocated_nodes.extend(
            variant
                .allocate_nodes
                .iter()
                .filter(|n| !existing.contains(n))
                .map(|&n| NodeId(n)),
        );
    }
    if !variant.deallocate_nodes.is_empty() {
        build
            .tree
            .allocated_nodes
            .retain(|n| !variant.deallocate_nodes.contains(&n.0));
    }
    opts.extra_modifier_texts.extend(
        variant
            .extra_modifiers
            .iter()
            .map(|line| localize_input_text(line)),
    );
    Ok(())
}

/// Generic variant evaluation: the baseline build is decoded/assembled only
/// once, and each variant is a clone with incremental changes stacked on
/// top for a full recalculation (the same "a what-if = a full
/// orchestration pass" basis as node_power), returning a stat-value matrix.
pub fn optimize_variants_json(request_json: &str) -> Result<String, String> {
    state::cached_response("optimize_variants", request_json, || {
        optimize_variants_impl(request_json).map_err(super::ApiError::into_json)
    })
}

fn optimize_variants_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: OptimizeVariantsRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    if req.stats.is_empty() {
        return Err(super::ApiError::bad_request("stats must not be empty"));
    }
    if req.variants.len() > VARIANT_CAP {
        return Err(super::ApiError::bad_request(format!(
            "{} variants exceed cap {VARIANT_CAP}; split into batches",
            req.variants.len()
        )));
    }
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;

    let mut base_build = parse_build_from_request(&req.request)?;
    // Degraded records don't go into this response — the main panel's
    // calculate already reports the same item_errors.
    let _ = apply_request_overrides(&mut base_build, &req.request, &data)?;
    let base_opts = orchestrator_options(&req.request)?;

    let baseline = if req.include_baseline.unwrap_or(true) {
        let session = calculate_with_data_session(&base_build, &data, &base_opts)
            .map_err(|e| format!("calculate baseline: {e}"))?;
        Some(collect_stats(&session, &req.stats))
    } else {
        None
    };

    let mut variants = Vec::with_capacity(req.variants.len());
    for (index, variant) in req.variants.iter().enumerate() {
        let mut build = base_build.clone();
        let mut opts = base_opts.clone();
        let session = apply_variant(&mut build, &mut opts, variant, &data).and_then(|()| {
            calculate_with_data_session(&build, &data, &opts).map_err(|e| format!("calculate: {e}"))
        });
        variants.push(match session {
            Ok(session) => VariantStatsJson {
                index,
                label: variant.label.clone(),
                stats: collect_stats(&session, &req.stats),
                error: None,
            },
            Err(error) => VariantStatsJson {
                index,
                label: variant.label.clone(),
                stats: BTreeMap::new(),
                error: Some(error),
            },
        });
    }

    Ok(
        serde_json::to_string(&OptimizeVariantsResponse { baseline, variants })
            .map_err(|e| format!("serialize: {e}"))?,
    )
}

// 0.4 attribution_json (source contribution, on a recompute-delta basis)

/// The attribution request: for each source (equipment slot / skill group /
/// flask), does a "recompute after removal", reporting its marginal
/// contribution to the specified display fields (marginal via recompute —
/// reuses the full pipeline, zero new calculation logic). Shaped
/// isomorphically to node_power / optimize_variants: embeds a full
/// calculation request as the baseline.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AttributionRequest {
    /// The full calculation request (baseline; overrides apply as usual).
    request: CalculateBuildRequest,
    /// The target display fields for attribution (display_catalog ids); defaults to `TotalDPS`/`Life`/`TotalEHP`.
    fields: Vec<String>,
}

const DEFAULT_ATTRIBUTION_FIELDS: &[&str] = &["TotalDPS", "Life", "TotalEHP"];

#[derive(Debug, Serialize)]
struct AttributionEntryJson {
    /// The source category: `item` / `socket_group` / `flask`.
    kind: &'static str,
    /// The source's stable id (equipment slot id / group index / flask slot name).
    id: String,
    /// This source's marginal contribution to each field (`baseline - the
    /// value after removal`; positive = a gain).
    deltas: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct AttributionResponse {
    /// The baseline (full build) value for each field.
    baseline: BTreeMap<String, f64>,
    entries: Vec<AttributionEntryJson>,
}

fn display_values_map(session: &CalculationSession, fields: &[String]) -> BTreeMap<String, f64> {
    let all = pobr_core::extract_display_values(session.output());
    fields
        .iter()
        .filter_map(|f| {
            all.iter()
                .find(|v| v.id.as_str() == f.as_str())
                .map(|v| (f.clone(), v.value))
        })
        .collect()
}

/// 0.4: source-contribution attribution (a recompute-delta basis).
///
/// Computation cost = `1 + the number of sources` full orchestration
/// passes; feeds the click-triggered attribution panel, and isn't called on every recalculation.
pub fn attribution_json(request_json: &str) -> Result<String, String> {
    state::cached_response("attribution", request_json, || {
        attribution_impl(request_json).map_err(super::ApiError::into_json)
    })
}

fn attribution_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: AttributionRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let fields: Vec<String> = if req.fields.is_empty() {
        DEFAULT_ATTRIBUTION_FIELDS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        req.fields.clone()
    };
    let calc_req = &req.request;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let mut build = parse_build_from_request(calc_req)?;
    // Degraded records don't go into this response — the main panel's
    // calculate already reports the same item_errors.
    let _ = apply_request_overrides(&mut build, calc_req, &data)?;

    let baseline_session = run_session_for_build(&build, calc_req)?;
    let baseline = display_values_map(&baseline_session, &fields);

    // The variant list: equipment slots (remove the item) / enabled skill
    // groups (disable them) / flask slots (remove them). Jewels aren't
    // individually attributed yet (radius jewels are geometrically coupled
    // to tree sockets, skipped for v1).
    let mut variants: Vec<(&'static str, String, Build)> = Vec::new();
    let mut slots: Vec<_> = build.items.keys().copied().collect();
    slots.sort_by_key(|s| s.id());
    for slot in slots {
        let mut v = build.clone();
        v.items.remove(&slot);
        variants.push(("item", slot.id().to_string(), v));
    }
    for (idx, group) in build.socket_groups.iter().enumerate() {
        if !group.enabled {
            continue;
        }
        let mut v = build.clone();
        v.socket_groups[idx].enabled = false;
        variants.push(("socket_group", idx.to_string(), v));
    }
    for (idx, (slot_name, _)) in build.utility_slots.iter().enumerate() {
        let mut v = build.clone();
        v.utility_slots.remove(idx);
        variants.push(("flask", slot_name.clone(), v));
    }

    let entries = variants
        .into_iter()
        .map(|(kind, id, variant)| {
            let session = run_session_for_build(&variant, calc_req)?;
            let without = display_values_map(&session, &fields);
            let deltas = fields
                .iter()
                .map(|f| {
                    let base = baseline.get(f).copied().unwrap_or(0.0);
                    let removed = without.get(f).copied().unwrap_or(0.0);
                    (f.clone(), base - removed)
                })
                .collect();
            Ok(AttributionEntryJson { kind, id, deltas })
        })
        .collect::<Result<Vec<_>, super::ApiError>>()?;

    let response = AttributionResponse { baseline, entries };
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}
