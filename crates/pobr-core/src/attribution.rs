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

use crate::{TraceGraph, TraceNodeId};

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
}

impl AttributionRequest {
    pub fn new(output: impl Into<DisplayStatId>) -> Self {
        Self {
            output: output.into(),
            sources: Vec::new(),
            group_by: AttributionGroup::Source,
            mode: AttributionMode::Marginal,
        }
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
                let value = direct_value_for_source(trace, output_node, source);
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
/// 遍历 `output_node` 的祖先，匹配 `source` 的输入（Input）节点直接相加。
fn direct_value_for_source(trace: &TraceGraph, output_node: TraceNodeId, source: &SourceId) -> f64 {
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

/// 计算 marginal（+ 可选 interaction）口径报告（文档 §6.2 / §6.3）。
///
/// `recompute(excluded)` 必须返回"剔除 `excluded` 中全部 source 后"的最终输出。
/// 对每个来源：`marginal_delta = final - recompute(&[source])`，
/// `marginal_percent = marginal_delta / final`。
///
/// `direct_trace` 可选；若提供则同时填充 direct 口径（与文档
/// [`AttributionMode::DirectAndMarginal`] 对应）。
pub fn attribute<F>(
    request: &AttributionRequest,
    final_value: f64,
    direct_trace: Option<(&TraceGraph, TraceNodeId)>,
    mut recompute: F,
) -> AttributionReport
where
    F: FnMut(&[SourceId]) -> f64,
{
    let want_marginal = request.mode.wants_marginal();
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
            let value = direct_value_for_source(trace, output_node, source);
            entry.value = value;
            entry.percent_of_final = percent(value, final_value);
            entry.path = vec![output_node];
        }

        entries.push(entry);
    }

    let interaction = if request.mode.wants_interaction() {
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
