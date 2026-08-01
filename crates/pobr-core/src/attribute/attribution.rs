//! The source contribution ledger (AttributionReport).
//!
//! Builds on [`TraceGraph`] to break down a final output (e.g. `TotalDPS` /
//! `Life`) by source (equipment slot / passive node / gem / config) into
//! direct / marginal / interaction contribution reports. This is the ledger
//! layer for "source-level attribution", PoBR's core value-add over PoB.
//!
//! Spec: `devs/docs/architecture/10-pob-parity-and-attribution.md` §6-7.
//!
//! # Differences from the doc's §7 (due to current implementation state)
//!
//! The doc's §7 [`AttributionRequest`] includes `build: BuildSnapshot` and
//! `selected_skill: Option<SkillInstanceId>`, but `BuildSnapshot` /
//! `SkillInstanceId` — these two higher-level types — aren't implemented yet.
//! Until they land, this module expresses the same semantics with **pure
//! functions + a recompute closure supplied by the caller**:
//!
//! - Direct contribution is produced by [`AttributionReport::direct`], which
//!   consumes a [`TraceGraph`] + output node (based on `source_ancestors`,
//!   corresponding to the doc's §6.1).
//! - Marginal / Interaction is produced by [`attribute`], which consumes a
//!   `recompute: Fn(&[SourceId]) -> f64` closure: the closure recomputes the
//!   final output for "the build with the given source set removed" (doc's
//!   §6.2/§6.3). The closure keeps attribution a pure/deterministic function —
//!   the recompute goes through a read-only snapshot (e.g. [`ModDb::filtered`]),
//!   introducing no shared mutable state.
//!
//! [`ModDb::filtered`]: crate::ModDb::filtered

use pobr_data::prelude::*;

use crate::{PassId, TraceGraph, TraceNodeId, TraceOperation};

/// The attribution grouping dimension (doc §7's `AttributionGroup`). The
/// ledger currently aggregates per-[`SourceId`]; this enum declares request
/// intent, and finer grouping (by slot / by affix) is implemented later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AttributionGroup {
    #[default]
    Source,
    ItemSlot,
    Item,
    ItemAffix,
    PassiveNode,
    SkillGem,
    SupportGem,
    Config,
}

/// The attribution mode (doc §7's `AttributionMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributionMode {
    /// Direct contribution only (§6.1).
    Direct,
    /// Marginal contribution only (§6.2).
    Marginal,
    /// Direct + marginal.
    DirectAndMarginal,
    /// Marginal + interaction bucket (§6.3).
    MarginalWithInteraction,
}

impl AttributionMode {
    fn wants_marginal(self) -> bool {
        matches!(
            self,
            Self::Marginal | Self::DirectAndMarginal | Self::MarginalWithInteraction
        )
    }

    fn wants_interaction(self) -> bool {
        matches!(self, Self::MarginalWithInteraction)
    }
}

/// An attribution request (doc §7's `AttributionRequest`).
///
/// Field names align with the doc: `output` (the doc has
/// `outputs: Vec<DisplayStatId>`; currently one output at a time), `group_by`,
/// `mode`. See the module-level differences note for `build` /
/// `selected_skill`, which are replaced by the recompute closure.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionRequest {
    /// The final output being attributed.
    pub output: DisplayStatId,
    /// The set of sources to attribute.
    pub sources: Vec<SourceId>,
    /// The grouping dimension.
    pub group_by: AttributionGroup,
    /// The attribution mode.
    pub mode: AttributionMode,
    /// Per-pass filter: when `Some(p)`, direct mode only accumulates Input
    /// nodes where `node.pass == Some(p)` (answers "how much OffHand DPS did
    /// this off-hand weapon contribute").
    ///
    /// **Mode decision (review C4)**: when `pass_filter` is not `None`,
    /// `marginal_delta` / `marginal_percent` / `interaction` are always set to
    /// `None` (mixed modes are rejected — removing a source is a global
    /// action, so its delta is in global-output terms, and displaying it
    /// alongside per-leg direct values would be misread). Send a separate
    /// request with `pass_filter = None` when global marginal is needed.
    pub pass_filter: Option<PassId>,
}

impl AttributionRequest {
    pub fn new(output: impl Into<DisplayStatId>) -> Self {
        Self {
            output: output.into(),
            sources: Vec::new(),
            group_by: AttributionGroup::Source,
            mode: AttributionMode::Marginal,
            pass_filter: None,
        }
    }

    /// Sets the per-pass filter (see [`AttributionRequest::pass_filter`] for the semantics).
    pub fn with_pass_filter(mut self, pass: PassId) -> Self {
        self.pass_filter = Some(pass);
        self
    }

    pub fn with_mode(mut self, mode: AttributionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_group_by(mut self, group_by: AttributionGroup) -> Self {
        self.group_by = group_by;
        self
    }

    pub fn with_sources(mut self, sources: impl IntoIterator<Item = SourceId>) -> Self {
        self.sources = sources.into_iter().collect();
        self
    }
}

/// A single source's contribution entry (doc §7's `AttributionEntry`).
///
/// Field names align with the doc. `value` / `percent_of_final` carry direct
/// mode; `marginal_delta` / `marginal_percent` carry marginal mode; a mode
/// that wasn't computed is `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionEntry {
    /// The contributing source.
    pub source: SourceId,
    /// The absolute direct contribution (direct mode, §6.1). 0.0 when direct mode wasn't computed.
    pub value: f64,
    /// The direct contribution's share of the final value (`value / final`).
    pub percent_of_final: Option<f64>,
    /// The marginal contribution: `final - final_without_source` (§6.2).
    pub marginal_delta: Option<f64>,
    /// The marginal share: `marginal_delta / final`.
    pub marginal_percent: Option<f64>,
    /// The output-path nodes reachable from this source in the TraceGraph (filled in direct mode).
    pub path: Vec<TraceNodeId>,
    /// The i18n explanation key (placeholder; a stable English key until i18n lands).
    pub explanation_key: String,
}

impl AttributionEntry {
    fn new(source: SourceId) -> Self {
        Self {
            source,
            value: 0.0,
            percent_of_final: None,
            marginal_delta: None,
            marginal_percent: None,
            path: Vec::new(),
            explanation_key: String::new(),
        }
    }
}

/// The source contribution report (doc §7's `AttributionReport`).
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionReport {
    /// The output being attributed.
    pub output: DisplayStatId,
    /// The final output value.
    pub final_value: f64,
    /// Per-source contribution entries, in the same order as the request's `sources`.
    pub entries: Vec<AttributionEntry>,
    /// The interaction bucket (§6.3). Only filled for
    /// [`AttributionMode::MarginalWithInteraction`].
    ///
    /// Reuses [`AttributionEntry`] to carry it: `marginal_delta` =
    /// `final - baseline - Σ(individual marginal deltas)`, with `source`
    /// marked by the [`SourceKind::Derived`] placeholder.
    pub interaction: Option<AttributionEntry>,
}

impl AttributionReport {
    /// Computes the direct-mode report (doc §6.1).
    ///
    /// Sums the input-node values directly contributed by each
    /// `request.source` among `trace`'s ancestors of the output node
    /// `output_node`, with `percent_of_final = value / final`.
    pub fn direct(
        request: &AttributionRequest,
        final_value: f64,
        trace: &TraceGraph,
        output_node: TraceNodeId,
    ) -> Self {
        let entries = request
            .sources
            .iter()
            .map(|source| {
                let value =
                    direct_value_for_source(trace, output_node, source, request.pass_filter);
                let percent_of_final = percent(value, final_value);
                AttributionEntry {
                    source: source.clone(),
                    value,
                    percent_of_final,
                    path: vec![output_node],
                    explanation_key: "attribution.direct".to_string(),
                    ..AttributionEntry::new(source.clone())
                }
            })
            .collect();

        Self {
            output: request.output.clone(),
            final_value,
            entries,
            interaction: None,
        }
    }
}

/// Sums the input-node values a source directly contributes among an output
/// node's ancestor chain.
///
/// Algorithm (RFC §5.1; **review C1 note: this is an algorithm rewrite of the
/// old "single global-visited flat DFS"**, not "add a field, ignore it,
/// fallback" — I2's zero-regression guarantee rests on "the new recursive
/// branch never triggers on Combine-free graphs, and behavior is bit-for-bit
/// identical", locked by the `legacy_equivalence` unit test; rolling back
/// means `git revert` on this file's changes, with the old implementation
/// kept as a copy in `#[cfg(test)] direct_value_for_source_legacy`):
///
/// - If the output node itself is a [`TraceOperation::Combine`]:
///   `total = Σᵢ weights[i] × direct(legᵢ)` (including the leg root).
/// - Within a leg: keeps the old visited-DFS semantics, but **the visited set
///   is independent per leg**; encountering a nested Combine node during
///   traversal doesn't expand its incoming edges, it recurses by weight
///   instead (nested merging).
/// - When `pass_filter = Some(p)`, only accumulates Input nodes where
///   `node.pass == Some(p)` (§5.4).
///
/// Correctness depends on invariant I1 (different pass subgraphs don't share
/// pass-stamped nodes, see [`TraceGraph::combine_partition_violations`],
/// asserted per-Combine in debug builds); a shared ancestor with `pass ==
/// None` is **counted once per leg and then weighted by that leg's weight**
/// across multiple legs — this is intentional (a global source boosts both
/// hands simultaneously).
fn direct_value_for_source(
    trace: &TraceGraph,
    output_node: TraceNodeId,
    source: &SourceId,
    pass_filter: Option<PassId>,
) -> f64 {
    // The output node itself is a Combine: recurse by weight over its legs
    // directly (matching the old algorithm, which never counted the output
    // node itself — a Combine node is always an operator node with no source).
    if let Some(node) = trace.node(output_node)
        && let TraceOperation::Combine { weights, .. } = &node.operation
    {
        return combine_weighted_direct(trace, output_node, weights, source, pass_filter);
    }
    leg_direct(trace, trace.incoming(output_node), source, pass_filter)
}

/// Weighted amortization at a Combine node: `Σᵢ weights[i] × direct(legᵢ, including the leg root)`.
fn combine_weighted_direct(
    trace: &TraceGraph,
    combine: TraceNodeId,
    weights: &[f64],
    source: &SourceId,
    pass_filter: Option<PassId>,
) -> f64 {
    let legs = trace.incoming(combine);
    debug_assert_eq!(
        weights.len(),
        legs.len(),
        "the Combine weight count must equal the incoming-edge count (the graph-building side locks the order via add_combine_node)"
    );
    debug_assert!(
        trace.combine_partition_violations(combine).is_empty(),
        "invariant I1 violation: Combine node {combine:?}'s incoming legs share a pass-stamped node (RFC §2.4 / review C3)"
    );
    legs.iter()
        .zip(weights)
        .map(|(leg, weight)| weight * leg_direct(trace, vec![*leg], source, pass_filter))
        .sum()
}

/// Within-leg direct DFS: **the visited set is independent per leg**; `seeds`
/// includes the leg root itself (the leg root may itself be a matching Input
/// node). Encountering a nested Combine node recurses by weight rather than
/// expanding its incoming edges.
fn leg_direct(
    trace: &TraceGraph,
    seeds: Vec<TraceNodeId>,
    source: &SourceId,
    pass_filter: Option<PassId>,
) -> f64 {
    let mut total = 0.0;
    let mut visited = vec![false; trace.nodes().len()];
    let mut stack = seeds;

    while let Some(current) = stack.pop() {
        let idx = current.as_usize();
        if visited.get(idx).copied().unwrap_or(true) {
            continue;
        }
        visited[idx] = true;

        let Some(node) = trace.node(current) else {
            continue;
        };
        if let TraceOperation::Combine { weights, .. } = &node.operation {
            // A nested merge node: visited is already marked (reached via
            // multiple paths within the same leg counts once), and the
            // recursive call gives each of its legs its own independent
            // visited set.
            total += combine_weighted_direct(trace, current, weights, source, pass_filter);
            continue;
        }
        if node.source.as_ref() == Some(source)
            && pass_filter.is_none_or(|filter| node.pass == Some(filter))
        {
            total += node.value;
        }
        stack.extend(trace.incoming(current));
    }

    total
}

/// Computes a marginal (+ optional interaction) mode report (doc §6.2 / §6.3).
///
/// `recompute(excluded)` must return the final output "with every source in
/// `excluded` removed". For each source:
/// `marginal_delta = final - recompute(&[source])`,
/// `marginal_percent = marginal_delta / final`.
///
/// `direct_trace` is optional; when provided, direct mode is also filled in
/// (corresponding to the doc's [`AttributionMode::DirectAndMarginal`]).
///
/// **C4 mode decision**: when `request.pass_filter` is not `None`, marginal /
/// interaction are always set to `None` (see
/// [`AttributionRequest::pass_filter`]), and `recompute` is never called.
pub fn attribute<F>(
    request: &AttributionRequest,
    final_value: f64,
    direct_trace: Option<(&TraceGraph, TraceNodeId)>,
    mut recompute: F,
) -> AttributionReport
where
    F: FnMut(&[SourceId]) -> f64,
{
    // C4: a pass-filtered request rejects mixed modes — marginal/interaction
    // are in global-output terms and would be misread alongside per-leg
    // direct values; set to None, and the consumer sends a separate
    // filter-less request when global marginal is needed.
    let want_marginal = request.mode.wants_marginal() && request.pass_filter.is_none();
    let wants_direct = matches!(
        request.mode,
        AttributionMode::Direct | AttributionMode::DirectAndMarginal
    );

    let mut entries = Vec::with_capacity(request.sources.len());
    let mut marginal_sum = 0.0;

    for source in &request.sources {
        let mut entry = AttributionEntry::new(source.clone());
        entry.explanation_key = "attribution.marginal".to_string();

        if want_marginal {
            let without = recompute(std::slice::from_ref(source));
            let delta = final_value - without;
            entry.marginal_delta = Some(delta);
            entry.marginal_percent = percent(delta, final_value);
            marginal_sum += delta;
        }

        if let (true, Some((trace, output_node))) = (wants_direct, direct_trace) {
            let value = direct_value_for_source(trace, output_node, source, request.pass_filter);
            entry.value = value;
            entry.percent_of_final = percent(value, final_value);
            entry.path = vec![output_node];
        }

        entries.push(entry);
    }

    // C4: interaction sums over global marginal values, so it's likewise set to None under pass_filter.
    let interaction = if request.mode.wants_interaction() && request.pass_filter.is_none() {
        // baseline = the output with every source removed (§6.3).
        let baseline = recompute(&request.sources);
        // interaction = final - baseline - Σ(individual marginal deltas).
        let interaction_value = final_value - baseline - marginal_sum;
        let mut entry = AttributionEntry::new(SourceId::new(SourceKind::Derived, "interaction"));
        entry.marginal_delta = Some(interaction_value);
        entry.marginal_percent = percent(interaction_value, final_value);
        entry.explanation_key = "attribution.interaction".to_string();
        Some(entry)
    } else {
        None
    };

    AttributionReport {
        output: request.output.clone(),
        final_value,
        entries,
        interaction,
    }
}

/// `value / final`, returning `None` when `final == 0` (avoids a division-by-zero NaN/Inf).
fn percent(value: f64, final_value: f64) -> Option<f64> {
    if final_value == 0.0 {
        None
    } else {
        Some(value / final_value)
    }
}

#[cfg(test)]
mod direct_rewrite_tests {
    //! Review C1: direct is an **algorithm rewrite**; the old implementation
    //! is kept here byte-for-byte as evidence of equivalence — the old and
    //! new algorithms must be bit-for-bit identical on Combine-free graphs
    //! (I2's internal mirror; the external mirror is `tests/attribution.rs` /
    //! `tests/trace.rs` passing with zero changes).

    use super::*;
    use crate::{CombineMode, TraceOperation};

    /// A verbatim copy of the old implementation (`direct_value_for_source`
    /// before the rewrite, a single global-visited flat DFS). For
    /// equivalence testing only — never call it on a production path.
    fn direct_value_for_source_legacy(
        trace: &TraceGraph,
        output_node: TraceNodeId,
        source: &SourceId,
    ) -> f64 {
        let mut total = 0.0;
        let mut visited = vec![false; trace.nodes().len()];
        let mut stack = trace.incoming(output_node);

        while let Some(current) = stack.pop() {
            let idx = current.as_usize();
            if visited.get(idx).copied().unwrap_or(true) {
                continue;
            }
            visited[idx] = true;

            if let Some(node) = trace.node(current) {
                if node.source.as_ref() == Some(source) {
                    total += node.value;
                }
                stack.extend(trace.incoming(current));
            }
        }

        total
    }

    fn src(id: &str) -> SourceId {
        SourceId::new(SourceKind::Item, id)
    }

    /// A Combine-free graph with diamond sharing + multi-layer operators: old and new algorithms are bit-for-bit identical (I2).
    #[test]
    fn no_combine_graph_matches_legacy_bit_for_bit() {
        let mut trace = TraceGraph::new();
        let a = trace.add_source_node("weapon flat", 60.0, src("weapon"));
        let b = trace.add_source_node("helmet flat", 40.0, src("helmet"));
        let sum = trace.add_node("base sum", 100.0, TraceOperation::Add);
        trace.add_edge(a, sum);
        trace.add_edge(b, sum);
        // A second Input from the same source (e.g. a second affix on the same item) + diamond convergence.
        let a2 = trace.add_source_node("weapon inc", 30.0, src("weapon"));
        let scaled = trace.add_node("scaled", 130.0, TraceOperation::Multiply);
        trace.add_edge(sum, scaled);
        trace.add_edge(a2, scaled);
        let out = trace.add_node("final", 130.0, TraceOperation::Multiply);
        trace.add_edge(scaled, out);
        trace.add_edge(sum, out); // Diamond: sum reaches out via two paths

        for id in ["weapon", "helmet", "absent"] {
            let source = src(id);
            assert_eq!(
                direct_value_for_source(&trace, out, &source, None),
                direct_value_for_source_legacy(&trace, out, &source),
                "on a no-Combine graph, new and old direct must be bit-for-bit equal (source={id})"
            );
        }
    }

    /// An OR passthrough single-leg Combine (single-hand build shape): equivalent to the old algorithm reading the leg directly (I3's attribution-side mirror).
    #[test]
    fn single_leg_or_combine_matches_legacy_on_leg() {
        let mut trace = TraceGraph::new();
        let input = trace.add_source_node("weapon dps leg", 50.0, src("weapon"));
        let leg = trace.add_node("MH TotalDPS", 50.0, TraceOperation::Multiply);
        trace.add_edge(input, leg);
        let combined = trace.add_combine_node("TotalDPS", 50.0, CombineMode::Or, &[(leg, 1.0)]);

        let source = src("weapon");
        assert_eq!(
            direct_value_for_source(&trace, combined, &source, None),
            direct_value_for_source_legacy(&trace, leg, &source),
        );
    }
}
