use pobr_data::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceNodeId(usize);

impl TraceNodeId {
    pub fn as_usize(self) -> usize {
        self.0
    }
}

//  pass 分区与合并节点（RFC m4-rfc-attribution-passes §2-§3）

/// 手分区。`Single` = 法术/非攻击技能（PoB2 passList 的 "Skill" pass）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandTag {
    Single,
    MainHand,
    OffHand,
}

/// 暴击分区。`Blended` = 不在暴击双 pass 内、或已完成 CritBlend 合并的节点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CritTag {
    Blended,
    Crit,
    NonCrit,
}

/// pass 标识：hand × crit 二维分区（RFC §2.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId {
    pub hand: HandTag,
    pub crit: CritTag,
}

impl PassId {
    pub const fn new(hand: HandTag, crit: CritTag) -> Self {
        Self { hand, crit }
    }

    /// 该手的 Blended（CritBlend 合并后 / 不在暴击双 pass 内）pass。
    pub const fn hand_blended(hand: HandTag) -> Self {
        Self {
            hand,
            crit: CritTag::Blended,
        }
    }
}

/// combineStat 合并模式（vendor `CalcOffence.lua:2451-2538` 逐分支 + `:4395` CritBlend）。
///
/// 注意：`Chance` / `ChanceAilment` / `CritBlend` 是**系数模式**——合并值与权重依赖
/// 外生系数（portion / stacks 占比 / 该手暴击率 c），由构图侧冻结为常数写进
/// [`TraceOperation::Combine`] 的 `weights`；
/// [`CombineMode::combine`] / [`CombineMode::linearized_weights`] 对它们返回 `None`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CombineMode {
    /// `MH or OH`（vendor `:2453-2454`；`not bothWeaponAttack` 也走此分支——单手直通）。
    Or,
    /// `MH + OH`（`:2455-2456`）。
    Add,
    /// `(MH + OH) / 2`（`:2457-2458`）。
    Average,
    /// `MH + OH`，非 `doubleHitsWhenDualWielding` 再 `/2`（`:2541-2545`）。
    Dps { double_hits: bool },
    /// doubleHits：`MH + OH − MH×OH/100`；否则 `(MH+OH)/2`（`:2459-2464`，仅 CritChance 用，百分数）。
    Crit { double_hits: bool },
    /// 任一腿 0 → 0；否则 `2/(1/MH + 1/OH)`（`:2465-2470`，Speed 用）。
    HarmonicMean,
    /// `MH×mainPortion + OH×offPortion`，portion = chance×HitChance 占比（`:2471-2496`）。
    Chance,
    /// `maxInstance×stacks + minInstance×(1−stacks)`（`:2497-2538`）。
    ChanceAilment,
    /// 内层暴击合并 `hitAvg×(1−c) + critAvg×c`（`:4395`）。约定入边顺序 NonCrit 先、Crit 后，
    /// 即 weights = `[1−c, c]`。
    CritBlend,
}

impl CombineMode {
    /// 自给模式按 vendor 公式合并各腿值；系数模式（Chance/ChanceAilment/CritBlend）返回
    /// `None`（合并值由构图侧用外生系数计算）。
    ///
    /// `legs` 约定 MH 先、OH 后（缺腿由调用方按 0 折入或走 Or 单腿直通）；
    /// 单腿一律直通（vendor `not bothWeaponAttack` 分支）。
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

    /// direct 归因口径的一阶线性化权重（RFC §3.2 表；= `∂combined/∂leg_i` 在当前腿值处）。
    ///
    /// 系数模式返回 `None`——其权重是构图时冻结的外生系数（Chance 的 portion、
    /// ChanceAilment 的 stacks 占比、CritBlend 的 `[1−c, c]`），由构图侧直接给出。
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
            // 偏导：∂(MH+OH−MH·OH/100)/∂MH = 1−OH/100（交叉项使加权和 ≠ 输出，
            // 守恒由 marginal 兜底——评审 C2）。
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

/// 注：起 `Combine` 变体携带 `Vec<f64>` 权重，故本枚举不再可derive `Eq`
/// （f64 无全序相等）。全仓核查（评审 C6a）：无穷尽 `match TraceOperation` 点、
/// 无依赖 `Eq` 的消费（仅 `==` 比较，`PartialEq` 足够）。
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
    /// combineStat / CritBlend 合并节点。
    /// `weights[i]` 对应第 i 条入边（与 `add_edge` 插入顺序一致；约定 MH 先、OH 后，
    /// CritBlend 为 NonCrit 先、Crit 后）。
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
    /// 所属 pass 分区。`None` = pass 无关节点（全局输入、防御、
    /// 外层 hand-combine 输出）。由 [`TraceGraph::begin_pass`] 作用域栈自动盖戳。
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
    /// pass 作用域栈。构图期瞬态状态；图建完应为空。
    pass_stack: Vec<PassId>,
}

impl TraceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// 进入 pass 作用域：之后 `add_node` / `add_source_node` 自动盖当前 pass 戳。
    /// 嵌套时栈顶生效（hand 外层作用域内再 `begin_pass` 覆盖 crit 维度）。
    pub fn begin_pass(&mut self, pass: PassId) {
        self.pass_stack.push(pass);
    }

    /// 退出当前 pass 作用域。debug 下栈空即 panic（编排 bug 早暴露）；release 静默忽略。
    pub fn end_pass(&mut self) {
        debug_assert!(
            !self.pass_stack.is_empty(),
            "end_pass 在空 pass 栈上调用（begin/end 不配对）"
        );
        self.pass_stack.pop();
    }

    /// 当前生效的 pass（栈顶）；栈空 = `None`（pass 无关）。
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

    /// 添加合并节点并按 `legs` 顺序连边：保证 `weights[i]` 与第 i 条
    /// 入边一一对应（权重/边顺序锁死在单一构造点，RFC §3.1 约定 MH 先、OH 后）。
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

    /// 不变式 I1 / 评审 C3 诊断：合并节点各入腿的**带 pass 戳**祖先
    /// 集合必须两两不相交（跨腿共享只允许发生在 `pass == None` 的结构性祖先上——
    /// 这是 direct 算法逐腿独立遍历正确性的前提，RFC §2.4 / §5.1）。
    ///
    /// 返回出现在 ≥2 条入腿子图中的带戳节点；空 = 满足不变式。
    /// direct 归因在 debug 构建下对每个 Combine 节点断言此集合为空。
    pub fn combine_partition_violations(&self, combine: TraceNodeId) -> Vec<TraceNodeId> {
        let legs = self.incoming(combine);
        let mut seen_in_leg: Vec<Option<usize>> = vec![None; self.nodes.len()];
        let mut violations = Vec::new();
        for (leg_idx, leg) in legs.iter().enumerate() {
            // 收集该腿（含腿根自身）的全部祖先；只对带 pass 戳的节点做跨腿重叠检查。
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
