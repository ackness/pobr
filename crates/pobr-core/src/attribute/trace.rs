use pobr_data::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceNodeId(usize);

impl TraceNodeId {
    pub fn as_usize(self) -> usize {
        self.0
    }
}

//  Pass partitioning and combine nodes (RFC m4-rfc-attribution-passes §2-§3)

/// The hand partition. `Single` = spells/non-attack skills (PoB2 passList's "Skill" pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandTag {
    Single,
    MainHand,
    OffHand,
}

/// The crit partition. `Blended` = nodes outside the crit dual-pass, or nodes that already went through CritBlend merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CritTag {
    Blended,
    Crit,
    NonCrit,
}

/// A pass identifier: the two-dimensional hand × crit partition (RFC §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId {
    pub hand: HandTag,
    pub crit: CritTag,
}

impl PassId {
    pub const fn new(hand: HandTag, crit: CritTag) -> Self {
        Self { hand, crit }
    }

    /// The Blended pass for a given hand (after CritBlend merging / outside the crit dual-pass).
    pub const fn hand_blended(hand: HandTag) -> Self {
        Self {
            hand,
            crit: CritTag::Blended,
        }
    }
}

/// combineStat merge modes (a branch-by-branch port of vendor
/// `CalcOffence.lua:2451-2538`, plus `:4395` for CritBlend).
///
/// Note: `Chance` / `ChanceAilment` / `CritBlend` are **coefficient modes** —
/// their merged value and weights depend on exogenous coefficients (portion /
/// stack share / this hand's crit rate c), which the graph-building side
/// freezes into constants written to [`TraceOperation::Combine`]'s `weights`;
/// [`CombineMode::combine`] / [`CombineMode::linearized_weights`] return
/// `None` for them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CombineMode {
    /// `MH or OH` (vendor `:2453-2454`; `not bothWeaponAttack` also takes this branch — single-hand passthrough).
    Or,
    /// `MH + OH` (`:2455-2456`).
    Add,
    /// `(MH + OH) / 2` (`:2457-2458`).
    Average,
    /// `MH + OH`, further `/2` unless `doubleHitsWhenDualWielding` (`:2541-2545`).
    Dps { double_hits: bool },
    /// doubleHits: `MH + OH − MH×OH/100`; otherwise `(MH+OH)/2` (`:2459-2464`, CritChance only, percentage values).
    Crit { double_hits: bool },
    /// Either leg 0 → 0; otherwise `2/(1/MH + 1/OH)` (`:2465-2470`, used for Speed).
    HarmonicMean,
    /// `MH×mainPortion + OH×offPortion`, portion = chance×HitChance share (`:2471-2496`).
    Chance,
    /// `maxInstance×stacks + minInstance×(1−stacks)` (`:2497-2538`).
    ChanceAilment,
    /// The inner crit merge `hitAvg×(1−c) + critAvg×c` (`:4395`). Incoming
    /// edge order is by convention NonCrit first, Crit second, i.e.
    /// weights = `[1−c, c]`.
    CritBlend,
}

impl CombineMode {
    /// Self-sufficient modes merge the leg values via the vendor formula;
    /// coefficient modes (Chance/ChanceAilment/CritBlend) return `None` (the
    /// merged value is computed by the graph-building side using exogenous
    /// coefficients).
    ///
    /// `legs` is ordered MH first, OH second by convention (a missing leg is
    /// folded in as 0 by the caller, or passes through as a single leg via
    /// Or); a single leg always passes through (vendor's `not
    /// bothWeaponAttack` branch).
    pub fn combine(self, legs: &[f64]) -> Option<f64> {
        if legs.len() == 1 {
            return match self {
                Self::Chance | Self::ChanceAilment | Self::CritBlend => None,
                _ => Some(legs[0]),
            };
        }
        debug_assert_eq!(legs.len(), 2, "combineStat 是双腿算子（MH/OH）");
        let (mh, oh) = (legs[0], legs[1]);
        match self {
            Self::Or => Some(mh),
            Self::Add => Some(mh + oh),
            Self::Average | Self::Crit { double_hits: false } => Some((mh + oh) / 2.0),
            Self::Crit { double_hits: true } => Some(mh + oh - mh * oh / 100.0),
            Self::HarmonicMean => {
                if mh == 0.0 || oh == 0.0 {
                    Some(0.0)
                } else {
                    Some(2.0 / (1.0 / mh + 1.0 / oh))
                }
            }
            Self::Dps { double_hits } => Some(if double_hits {
                mh + oh
            } else {
                (mh + oh) / 2.0
            }),
            Self::Chance | Self::ChanceAilment | Self::CritBlend => None,
        }
    }

    /// First-order linearized weights for direct-attribution purposes (RFC
    /// §3.2 table; = `∂combined/∂leg_i` at the current leg values).
    ///
    /// Coefficient modes return `None` — their weights are exogenous
    /// coefficients frozen at graph-building time (Chance's portion,
    /// ChanceAilment's stack share, CritBlend's `[1−c, c]`), supplied
    /// directly by the graph-building side.
    pub fn linearized_weights(self, legs: &[f64]) -> Option<Vec<f64>> {
        if legs.len() == 1 {
            return match self {
                Self::Chance | Self::ChanceAilment | Self::CritBlend => None,
                _ => Some(vec![1.0]),
            };
        }
        debug_assert_eq!(legs.len(), 2, "combineStat 是双腿算子（MH/OH）");
        let (mh, oh) = (legs[0], legs[1]);
        match self {
            Self::Or => Some(vec![1.0, 0.0]),
            Self::Add | Self::Dps { double_hits: true } => Some(vec![1.0, 1.0]),
            Self::Average
            | Self::Crit { double_hits: false }
            | Self::Dps { double_hits: false } => Some(vec![0.5, 0.5]),
            // Partial derivative: ∂(MH+OH−MH·OH/100)/∂MH = 1−OH/100 (the cross
            // term means the weighted sum ≠ the output; conservation is
            // backstopped by marginal — see review C2).
            Self::Crit { double_hits: true } => Some(vec![1.0 - oh / 100.0, 1.0 - mh / 100.0]),
            Self::HarmonicMean => {
                if mh == 0.0 || oh == 0.0 {
                    Some(vec![0.0, 0.0])
                } else {
                    let denom = (mh + oh) * (mh + oh);
                    Some(vec![2.0 * oh * oh / denom, 2.0 * mh * mh / denom])
                }
            }
            Self::Chance | Self::ChanceAilment | Self::CritBlend => None,
        }
    }
}

/// Note: the `Combine` variant carries `Vec<f64>` weights, so this enum can
/// no longer derive `Eq` (f64 has no total-order equality). Whole-repo audit
/// (review C6a): no exhaustive `match TraceOperation` sites and no consumer
/// depends on `Eq` (only `==` comparisons, for which `PartialEq` suffices).
#[derive(Debug, Clone, PartialEq)]
pub enum TraceOperation {
    Input,
    QuerySum,
    QueryMore,
    QueryFlag,
    QueryOverride,
    Add,
    Multiply,
    MoreProduct,
    Clamp,
    Cap,
    Floor,
    Convert,
    Mitigate,
    Average,
    Chance,
    SelectMax,
    Stack,
    Aggregate,
    /// A combineStat / CritBlend merge node.
    /// `weights[i]` corresponds to the i-th incoming edge (matching
    /// `add_edge`'s insertion order; MH first then OH by convention,
    /// CritBlend is NonCrit first then Crit).
    Combine {
        mode: CombineMode,
        weights: Vec<f64>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceNode {
    pub id: TraceNodeId,
    pub label: String,
    pub value: f64,
    pub operation: TraceOperation,
    pub source: Option<SourceId>,
    /// The pass partition this node belongs to. `None` means a pass-agnostic
    /// node (global inputs, defence, outer hand-combine output). Stamped
    /// automatically by the [`TraceGraph::begin_pass`] scope stack.
    pub pass: Option<PassId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEdge {
    pub from: TraceNodeId,
    pub to: TraceNodeId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracedValue {
    pub value: f64,
    pub node_id: TraceNodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceOutput {
    pub stat: DisplayStatId,
    pub node_id: TraceNodeId,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TraceGraph {
    nodes: Vec<TraceNode>,
    edges: Vec<TraceEdge>,
    /// The pass scope stack. Transient graph-building state; should be empty once the graph is built.
    pass_stack: Vec<PassId>,
}

impl TraceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enters a pass scope: subsequent `add_node` / `add_source_node` calls
    /// automatically stamp the current pass. When nested, the top of the
    /// stack wins (e.g. `begin_pass` again inside a hand's outer scope
    /// overrides the crit dimension).
    pub fn begin_pass(&mut self, pass: PassId) {
        self.pass_stack.push(pass);
    }

    /// Exits the current pass scope. Panics on an empty stack in debug builds
    /// (surfaces orchestration bugs early); silently ignored in release.
    pub fn end_pass(&mut self) {
        debug_assert!(
            !self.pass_stack.is_empty(),
            "end_pass 在空 pass 栈上调用（begin/end 不配对）"
        );
        self.pass_stack.pop();
    }

    /// The currently active pass (top of the stack); an empty stack means `None` (pass-agnostic).
    pub fn current_pass(&self) -> Option<PassId> {
        self.pass_stack.last().copied()
    }

    pub fn add_node(
        &mut self,
        label: impl Into<String>,
        value: f64,
        operation: TraceOperation,
    ) -> TraceNodeId {
        let id = TraceNodeId(self.nodes.len());
        let pass = self.current_pass();
        self.nodes.push(TraceNode {
            id,
            label: label.into(),
            value,
            operation,
            source: None,
            pass,
        });
        id
    }

    pub fn add_source_node(
        &mut self,
        label: impl Into<String>,
        value: f64,
        source: SourceId,
    ) -> TraceNodeId {
        let id = TraceNodeId(self.nodes.len());
        let pass = self.current_pass();
        self.nodes.push(TraceNode {
            id,
            label: label.into(),
            value,
            operation: TraceOperation::Input,
            source: Some(source),
            pass,
        });
        id
    }

    /// Adds a combine node and wires up edges in `legs` order: guarantees
    /// `weights[i]` corresponds one-to-one with the i-th incoming edge (the
    /// weight/edge ordering is locked at this single construction point, per
    /// RFC §3.1's MH-first-then-OH convention).
    pub fn add_combine_node(
        &mut self,
        label: impl Into<String>,
        value: f64,
        mode: CombineMode,
        legs: &[(TraceNodeId, f64)],
    ) -> TraceNodeId {
        let weights = legs.iter().map(|(_, w)| *w).collect();
        let node = self.add_node(label, value, TraceOperation::Combine { mode, weights });
        for (leg, _) in legs {
            self.add_edge(*leg, node);
        }
        node
    }

    pub fn add_edge(&mut self, from: TraceNodeId, to: TraceNodeId) {
        self.edges.push(TraceEdge { from, to });
    }

    pub fn node(&self, id: TraceNodeId) -> Option<&TraceNode> {
        self.nodes.get(id.as_usize())
    }

    pub fn nodes(&self) -> &[TraceNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[TraceEdge] {
        &self.edges
    }

    pub fn incoming(&self, to: TraceNodeId) -> Vec<TraceNodeId> {
        self.edges
            .iter()
            .filter(|edge| edge.to == to)
            .map(|edge| edge.from)
            .collect()
    }

    pub fn outgoing(&self, from: TraceNodeId) -> Vec<TraceNodeId> {
        self.edges
            .iter()
            .filter(|edge| edge.from == from)
            .map(|edge| edge.to)
            .collect()
    }

    pub fn source_ancestors(&self, node_id: TraceNodeId) -> Vec<&SourceId> {
        let mut sources = Vec::new();
        let mut stack = self.incoming(node_id);

        while let Some(current) = stack.pop() {
            if let Some(node) = self.node(current) {
                if let Some(source) = &node.source {
                    sources.push(source);
                }
                stack.extend(self.incoming(current));
            }
        }

        sources
    }

    /// Invariant I1 / review C3 diagnostic: the sets of **pass-stamped**
    /// ancestors of a combine node's incoming legs must be pairwise disjoint
    /// (cross-leg sharing is only allowed among structural ancestors with
    /// `pass == None` — this is the precondition for the direct algorithm's
    /// correctness when traversing each leg independently, RFC §2.4 / §5.1).
    ///
    /// Returns the stamped nodes that appear in ≥2 leg subgraphs; empty means
    /// the invariant holds. Direct attribution asserts this set is empty for
    /// every Combine node in debug builds.
    pub fn combine_partition_violations(&self, combine: TraceNodeId) -> Vec<TraceNodeId> {
        let legs = self.incoming(combine);
        let mut seen_in_leg: Vec<Option<usize>> = vec![None; self.nodes.len()];
        let mut violations = Vec::new();
        for (leg_idx, leg) in legs.iter().enumerate() {
            // Collect this leg's full ancestor set (including the leg root
            // itself); only check pass-stamped nodes for cross-leg overlap.
            let mut visited = vec![false; self.nodes.len()];
            let mut stack = vec![*leg];
            while let Some(current) = stack.pop() {
                let idx = current.as_usize();
                if visited.get(idx).copied().unwrap_or(true) {
                    continue;
                }
                visited[idx] = true;
                let Some(node) = self.node(current) else {
                    continue;
                };
                if node.pass.is_some() {
                    match seen_in_leg[idx] {
                        Some(prev) if prev != leg_idx => violations.push(current),
                        _ => seen_in_leg[idx] = Some(leg_idx),
                    }
                }
                stack.extend(self.incoming(current));
            }
        }
        violations.sort();
        violations.dedup();
        violations
    }
}
