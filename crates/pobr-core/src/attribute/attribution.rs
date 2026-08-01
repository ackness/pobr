//! 来源贡献总账（AttributionReport）。
//!
//! 在 [`TraceGraph`] 之上提供把某个最终输出（如 `TotalDPS` / `Life`）按来源
//! （装备槽 / 天赋节点 / 宝石 / 配置）分解的 direct / marginal / interaction
//! 贡献报告。这是 PoBR 相对 PoB 的核心增量"source-level 归因"的总账层。
//!
//! 规格依据：`devs/docs/architecture/10-pob-parity-and-attribution.md` §6-7。
//!
//! # 与文档 §7 的差异（实现现状所致）
//!
//! 文档 §7 的 [`AttributionRequest`] 含 `build: BuildSnapshot` 与
//! `selected_skill: Option<SkillInstanceId>`，但 `BuildSnapshot` / `SkillInstanceId`
//! 这两个高层类型尚未实现。在它们落地前，本模块用**纯函数 + 调用方提供的重算闭包**
//! 表达同等语义：
//!
//! - Direct contribution 由 [`AttributionReport::direct`] 消费一个 [`TraceGraph`]
//!   + 输出节点产生（基于 `source_ancestors`，对应文档 §6.1）。
//! - Marginal / Interaction 由 [`attribute`] 消费一个 `recompute: Fn(&[SourceId]) -> f64`
//!   闭包产生：闭包对"剔除给定 source 集合后的 build"重算最终输出（文档 §6.2/§6.3）。
//!   闭包让 attribution 保持纯函数 / 确定性，重算走只读快照（如 [`ModDb::filtered`]），
//!   不引入共享可变状态。
//!
//! [`ModDb::filtered`]: crate::ModDb::filtered

use pobr_data::prelude::*;

use crate::{PassId, TraceGraph, TraceNodeId, TraceOperation};

/// 归因分组维度（文档 §7 `AttributionGroup`）。当前总账按 [`SourceId`] 逐条聚合，
/// 该枚举用于声明请求意图，更细的分组（按槽位 / 按词条）后续实现。
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

/// 归因口径（文档 §7 `AttributionMode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributionMode {
    /// 仅直接贡献（§6.1）。
    Direct,
    /// 仅边际贡献（§6.2）。
    Marginal,
    /// 直接 + 边际。
    DirectAndMarginal,
    /// 边际 + 交互桶（§6.3）。
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

/// 归因请求（文档 §7 `AttributionRequest`）。
///
/// 字段名与文档对齐：`output`（文档作 `outputs: Vec<DisplayStatId>`，当前一次一个输出）、
/// `group_by`、`mode`。`build` / `selected_skill` 见模块级差异说明，由重算闭包替代。
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionRequest {
    /// 被归因的最终输出。
    pub output: DisplayStatId,
    /// 待归因的来源集合。
    pub sources: Vec<SourceId>,
    /// 分组维度。
    pub group_by: AttributionGroup,
    /// 归因口径。
    pub mode: AttributionMode,
    /// per-pass 过滤：`Some(p)` 时 direct 口径只累计
    /// `node.pass == Some(p)` 的 Input 节点（回答"这件副手武器贡献了多少 OffHand DPS"）。
    ///
    /// **口径裁决（评审 C4）**：`pass_filter` 非 `None` 时 `marginal_delta` /
    /// `marginal_percent` / `interaction` 一律置 `None`（拒绝混口径——剔除来源是
    /// 全局动作，其 delta 是全局输出口径，与腿内 direct 并排展示会误读）。
    /// 需要全局 marginal 时另发一个 `pass_filter = None` 的请求。
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

    /// 设定 per-pass 过滤（见 [`AttributionRequest::pass_filter`] 的口径）。
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

/// 单个来源的贡献条目（文档 §7 `AttributionEntry`）。
///
/// 字段名与文档对齐。`value` / `percent_of_final` 承载 direct 口径；
/// `marginal_delta` / `marginal_percent` 承载 marginal 口径；未计算的口径为 `None`。
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionEntry {
    /// 贡献来源。
    pub source: SourceId,
    /// 直接贡献绝对值（direct 口径，§6.1）。无 direct 口径时为 0.0。
    pub value: f64,
    /// 直接贡献占最终值的比例（`value / final`）。
    pub percent_of_final: Option<f64>,
    /// 边际贡献：`final - final_without_source`（§6.2）。
    pub marginal_delta: Option<f64>,
    /// 边际占比：`marginal_delta / final`。
    pub marginal_percent: Option<f64>,
    /// 该来源在 TraceGraph 中可达的输出路径节点（direct 口径时填充）。
    pub path: Vec<TraceNodeId>,
    /// i18n 解释 key（占位，i18n 落地前为稳定英文 key）。
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

/// 来源贡献报告（文档 §7 `AttributionReport`）。
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionReport {
    /// 被归因的输出。
    pub output: DisplayStatId,
    /// 最终输出值。
    pub final_value: f64,
    /// 逐来源贡献条目，顺序与请求 `sources` 一致。
    pub entries: Vec<AttributionEntry>,
    /// 交互桶（§6.3）。仅 [`AttributionMode::MarginalWithInteraction`] 时填充。
    ///
    /// 复用 [`AttributionEntry`] 承载：`marginal_delta` =
    /// `final - baseline - Σ(individual marginal deltas)`，
    /// `source` 用 [`SourceKind::Derived`] 占位标记。
    pub interaction: Option<AttributionEntry>,
}

impl AttributionReport {
    /// 计算 direct 口径报告（文档 §6.1）。
    ///
    /// 从 `trace` 中输出节点 `output_node` 的祖先里，累加每个 `request.source`
    /// 直接贡献的输入节点值，`percent_of_final = value / final`。
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

/// 累加某来源在输出节点祖先链中的直接贡献输入值。
///
/// 算法（RFC §5.1；**评审 C1 注记：这是对旧"单一全局 visited 扁平 DFS"的算法重写**，
/// 不是"加字段不读即回退"——I2 零回归靠"无 Combine 图上递归分支不触发、行为逐字节
/// 等价"保证，等价性由 `legacy_equivalence` 单测锁定；回退 = git revert 本文件改动，
/// 旧实现副本保留于 `#[cfg(test)] direct_value_for_source_legacy`）：
///
/// - 输出节点为 [`TraceOperation::Combine`]：`total = Σᵢ weights[i] × direct(腿ᵢ)`（含腿自身）。
/// - 腿内：维持旧 visited-DFS 语义，但 **visited 集合按腿独立**；遍历中再遇 Combine
///   节点不展开其入边，改按权重递归（嵌套合并）。
/// - `pass_filter = Some(p)` 时只累计 `node.pass == Some(p)` 的 Input 节点（§5.4）。
///
/// 正确性依赖不变式 I1（不同 pass 子图不共享带 pass 戳节点，
/// [`TraceGraph::combine_partition_violations`]，debug 构建下逐 Combine 断言）；
/// `pass == None` 的共享祖先在多腿内**各计一次再按腿权重加权**——这是有意语义
/// （全局来源同时增益两手）。
fn direct_value_for_source(
    trace: &TraceGraph,
    output_node: TraceNodeId,
    source: &SourceId,
    pass_filter: Option<PassId>,
) -> f64 {
    // 输出节点自身是 Combine：直接按权重递归各腿（与旧算法一致地不计输出节点自身——
    // Combine 节点恒为算子节点，无 source）。
    if let Some(node) = trace.node(output_node)
        && let TraceOperation::Combine { weights, .. } = &node.operation
    {
        return combine_weighted_direct(trace, output_node, weights, source, pass_filter);
    }
    leg_direct(trace, trace.incoming(output_node), source, pass_filter)
}

/// Combine 节点处的加权摊销：`Σᵢ weights[i] × direct(腿ᵢ，含腿自身)`。
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
        "Combine 权重数必须等于入边数（构图侧用 add_combine_node 锁定顺序）"
    );
    debug_assert!(
        trace.combine_partition_violations(combine).is_empty(),
        "不变式 I1 违例：Combine 节点 {combine:?} 的入腿共享带 pass 戳节点（RFC §2.4 / 评审 C3）"
    );
    legs.iter()
        .zip(weights)
        .map(|(leg, weight)| weight * leg_direct(trace, vec![*leg], source, pass_filter))
        .sum()
}

/// 腿内 direct DFS：**visited 集合按腿独立**；`seeds` 含腿根自身（腿根可能本身就是
/// 匹配的 Input 节点）。遇嵌套 Combine 节点按权重递归、不展开其入边。
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
            // 嵌套合并节点：visited 已标记（同腿内经多路径到达只计一次），递归内各腿
            // 再各自独立 visited。
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

/// 计算 marginal（+ 可选 interaction）口径报告（文档 §6.2 / §6.3）。
///
/// `recompute(excluded)` 必须返回"剔除 `excluded` 中全部 source 后"的最终输出。
/// 对每个来源：`marginal_delta = final - recompute(&[source])`，
/// `marginal_percent = marginal_delta / final`。
///
/// `direct_trace` 可选；若提供则同时填充 direct 口径（与文档
/// [`AttributionMode::DirectAndMarginal`] 对应）。
///
/// **C4 口径裁决**：`request.pass_filter` 非 `None` 时 marginal / interaction 一律
/// 置 `None`（见 [`AttributionRequest::pass_filter`]），`recompute` 不被调用。
pub fn attribute<F>(
    request: &AttributionRequest,
    final_value: f64,
    direct_trace: Option<(&TraceGraph, TraceNodeId)>,
    mut recompute: F,
) -> AttributionReport
where
    F: FnMut(&[SourceId]) -> f64,
{
    // C4：pass 过滤请求拒绝混口径——marginal/interaction 是全局输出口径，与腿内
    // direct 并排会误读；置 None，消费方需要全局 marginal 时另发无 filter 请求。
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

    // C4：interaction 基于全局 marginal 求和，pass_filter 下同样置 None。
    let interaction = if request.mode.wants_interaction() && request.pass_filter.is_none() {
        // baseline = 剔除全部来源后的输出（§6.3）。
        let baseline = recompute(&request.sources);
        // interaction = final - baseline - Σ(individual marginal deltas)。
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

/// `value / final`，`final == 0` 时返回 `None`（避免除零产生 NaN/Inf）。
fn percent(value: f64, final_value: f64) -> Option<f64> {
    if final_value == 0.0 {
        None
    } else {
        Some(value / final_value)
    }
}

#[cfg(test)]
mod direct_rewrite_tests {
    //! 评审 C1：direct 是**算法重写**，旧实现按字节保留于此作等价性证物——
    //! 无 Combine 图上新旧算法必须逐字节等价（I2 的内部镜像；外部镜像 =
    //! `tests/attribution.rs` / `tests/trace.rs` 零改动通过）。

    use super::*;
    use crate::{CombineMode, TraceOperation};

    /// 旧实现原样副本（重写前的 `direct_value_for_source`，
    /// 单一全局 visited 扁平 DFS）。仅供等价性测试，勿在产品路径调用。
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

    /// 菱形共享 + 多层算子的无 Combine 图：新旧算法逐字节等价（I2）。
    #[test]
    fn no_combine_graph_matches_legacy_bit_for_bit() {
        let mut trace = TraceGraph::new();
        let a = trace.add_source_node("weapon flat", 60.0, src("weapon"));
        let b = trace.add_source_node("helmet flat", 40.0, src("helmet"));
        let sum = trace.add_node("base sum", 100.0, TraceOperation::Add);
        trace.add_edge(a, sum);
        trace.add_edge(b, sum);
        // 同一来源的第二个 Input（如同件装备的第二条词条）+ 菱形汇聚。
        let a2 = trace.add_source_node("weapon inc", 30.0, src("weapon"));
        let scaled = trace.add_node("scaled", 130.0, TraceOperation::Multiply);
        trace.add_edge(sum, scaled);
        trace.add_edge(a2, scaled);
        let out = trace.add_node("final", 130.0, TraceOperation::Multiply);
        trace.add_edge(scaled, out);
        trace.add_edge(sum, out); // 菱形：sum 经两条路径可达 out

        for id in ["weapon", "helmet", "absent"] {
            let source = src(id);
            assert_eq!(
                direct_value_for_source(&trace, out, &source, None),
                direct_value_for_source_legacy(&trace, out, &source),
                "no-Combine 图上新旧 direct 必须逐字节等价（source={id}）"
            );
        }
    }

    /// OR 直通单腿 Combine（单手 build 形态）：等价于旧算法直读该腿（I3 归因侧镜像）。
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
