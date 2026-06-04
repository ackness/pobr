# PoB 属性全量兼容与贡献追踪

---

## 1. 硬性目标

PoBR 的设计底线是：**包含 PoB 的全部可计算属性**。在此基础上，PoBR 需要提供比 PoB 更细的追踪能力：

- 最终输出字段来自哪些装备、词条、天赋点、技能宝石、support gem、配置项、敌人配置和游戏常量。
- 每个来源对选中技能伤害、生存面、恢复、消耗、需求的贡献是多少。
- 对非线性公式、上限/下限、more 连乘、暴击、命中、抗性、穿透、转伤、异常状态等机制，能区分直接贡献、边际贡献和交互贡献。

这不是 UI 增强项，而是计算核心的数据契约。

实现具体机制时，优先参考 `agent-docs/` 中的本地游戏概念文档；这些文档是开发资料库，不是最终权威。涉及公式、版本改动、术语或边界条件时，必须与 PoB-PoE2 源码、官方 patch notes、PoE2 Wiki 或游戏数据交叉验证。发现 `agent-docs/` 错误时直接修正文档并保留来源说明。

---

## 2. PoB 属性覆盖基线

PoB 的可计算属性不是单一文件手写列表，PoBR 需要从多个来源建立覆盖基线：

| 来源 | 用途 | PoBR 处理 |
|------|------|-----------|
| `CalcSetup.lua` | base stats、职业属性、抗性惩罚、常量注入 | 抽取基础 modifier 与初始化阶段 |
| `CalcOffence.lua` | 技能伤害、DPS、异常、速度、命中、暴击、技能机制 | 抽取/记录所有写入 `output` 的 offence key |
| `CalcDefence.lua` | EHP、max hit、抗性、减伤、block、avoidance、恢复、防御层 | 抽取/记录所有写入 `output` 的 defence key |
| `Calcs.lua` / `CalcPerform.lua` | 计算编排、FullDPS、多技能汇总 | 抽取汇总输出 key 和阶段顺序 |
| `CalcSections.lua` | UI 详细计算面板和 breakdown 引用字段 | 抽取所有 `output` 引用和 breakdown section |
| `BuildDisplayStats.lua` | 侧栏显示、装备/天赋对比字段 | 抽取 display stat catalog |
| `ConfigOptions.lua` | 配置项、章节奖励选项、enemy/player config mods | 抽取 config input 与 generated mods |
| `QuestRewards.lua` | 战役奖励和 Seven Pillars/Qimah 选择 | 抽取 reward catalog 和 modifier text |
| `Data.lua` | 常量、power stat list、cap/floor、机制表 | 同步 constants 和 stat list |
| `CalcBreakdown.lua` | breakdown 生成模式 | 映射到 `BreakdownTree` 操作类型 |

PoBR 不能只维护手工挑选的字段。必须生成一个覆盖矩阵：

```text
pob_output_key
├─ source_file
├─ category
├─ value_type
├─ display_definition
├─ breakdown_definition
├─ pobr_status: computed | parsed | planned | unsupported
├─ owner_module
├─ tests
└─ fixture_refs
```

`pobr_status = unsupported` 必须有明确原因，例如 PoE1-only、已被 PoE2 移除、数据未公开、机制依赖未实现。默认状态是 `planned`，不是忽略。

---

## 3. 生成式 Stat Catalog

新增工具：

```text
tools/sync-pob-catalog
├─ 输入：PathOfBuildingCommunity/PathOfBuilding-PoE2 checkout 或指定 commit
├─ 解析：
│  ├─ CalcSetup.lua
│  ├─ BuildDisplayStats.lua
│  ├─ CalcSections.lua
│  ├─ CalcOffence.lua
│  ├─ CalcDefence.lua
│  ├─ Calcs.lua
│  ├─ ConfigOptions.lua
│  ├─ QuestRewards.lua
│  └─ Data.lua
├─ 输出：
│  ├─ fixtures/pob/parity/pob-output-catalog.json
│  ├─ fixtures/pob/parity/pob-display-stats.json
│  ├─ fixtures/pob/parity/pob-breakdown-sections.json
│  └─ crates/pobr-data/generated/display_stat_ids.rs
└─ 检查：
   └─ cargo run -p sync-pob-catalog -- check --pob-root ../PathOfBuilding-PoE2 --catalog fixtures/pob/parity/pob-catalog.json
```

CI 规则：

- PoB catalog 发生变化时必须更新 fixture。
- 新增 PoB output key 必须在 PoBR catalog 中分类。
- `computed` 状态必须有单元测试或 golden fixture。
- `display` 或 `comparison` 字段必须有 `DisplayStatDefinition`。
- 未分类 key 直接失败。

---

## 4. DisplayStatDefinition

所有玩家可见字段使用强类型 ID：

```rust
pub struct DisplayStatDefinition {
    pub id: DisplayStatId,
    pub pob_key: Option<PobOutputKey>,
    pub category: DisplayStatCategory,
    pub value_type: StatValueType,
    pub format: StatFormat,
    pub default_visible: bool,
    pub comparison_visible: bool,
    pub higher_is_better: Option<bool>,
    pub breakdown_policy: BreakdownPolicy,
    pub parity_status: ParityStatus,
}

pub enum ParityStatus {
    Computed,
    ParsedOnly,
    Planned,
    Unsupported { reason: String },
}
```

分类至少包括：

- offence
- hit damage
- dot damage
- ailment
- skill mechanics
- defence
- resistance
- avoidance
- mitigation
- resource
- recovery
- degen
- cost
- requirement
- minion
- enemy
- utility

---

## 5. 贡献追踪核心模型

### 5.1 SourceId

每个 modifier、base stat、game constant 和配置输入都必须带来源：

```rust
pub struct SourceId {
    pub kind: SourceKind,
    pub id: String,
    pub label_key: Option<String>,
}

pub enum SourceKind {
    CharacterBase,
    Item,
    ItemAffix,
    ItemImplicit,
    ItemEnchant,
    ItemQuality,
    PassiveNode,
    AscendancyNode,
    Jewel,
    SkillGem,
    SupportGem,
    SkillLevel,
    GemQuality,
    Config,
    EnemyConfig,
    CampaignReward,
    GameConstant,
    Derived,
}

pub struct ModifierSource {
    pub source_id: SourceId,
    pub parent_source_id: Option<SourceId>,
    pub slot: Option<ItemSlot>,
    pub raw_text: Option<String>,
    pub stat_id: Option<StatId>,
    pub mod_type: Option<ModType>,
    pub value: Option<ModValue>,
}
```

例子：

```text
Item: weapon
└─ ItemAffix: explicit#2 "+25% increased Physical Damage"
   └─ Modifier PhysicalDamage Inc 25
```

### 5.2 TraceGraph

计算时生成可选 trace graph：

```rust
pub struct TraceGraph {
    pub nodes: Vec<TraceNode>,
    pub edges: Vec<TraceEdge>,
}

pub struct TraceNode {
    pub id: TraceNodeId,
    pub label: TraceLabel,
    pub value: StatValue,
    pub operation: TraceOperation,
    pub source: Option<SourceId>,
}

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
}
```

性能策略：

- 默认计算可以只生成 `OutputTable`。
- 用户打开 breakdown 或 comparison 时启用 trace。
- trace 使用 `TraceMode` 控制粒度：

```rust
pub enum TraceMode {
    Off,
    Stat(DisplayStatId),
    SelectedSkill,
    Survivability,
    Full,
}
```

---

## 6. 贡献百分比

“某件装备贡献了最终 DPS 的多少百分比”不能只用线性拆分，因为 DPS 由 base damage、inc、more、crit、hit chance、speed、enemy mitigation 等非线性部分连乘。PoBR 需要明确三种贡献口径：

### 6.1 Direct Contribution

直接贡献用于局部线性项，例如：

```text
FireResistanceTotal = base + item + passive + config
```

装备贡献：

```text
item_contribution_percent = item_flat_resistance / final_resistance_total
```

适合：

- flat resistance
- flat attributes
- flat life/mana/ES
- local base damage
- additive inc bucket 内部占比

### 6.2 Marginal Contribution

边际贡献用于玩家最关心的问题：移除这件装备/这个天赋后最终输出变化多少。

```text
marginal_delta = final_output(build) - final_output(build_without_source)
marginal_percent = marginal_delta / final_output(build)
```

适合：

- 某装备对 `TotalDps` 的贡献
- 某天赋点对 `FireMaxHit` 的贡献
- 某 support gem 对 `IgniteDps` 的贡献
- 某配置项对 `HitChance` 的贡献

### 6.3 Interaction Contribution

当多个来源存在明显交互时，边际贡献加总可能超过或低于最终变化。PoBR 需要把交互项显式输出：

```text
interaction = final_output
  - baseline_output
  - sum(individual_marginal_deltas)
```

后续可选实现 Shapley approximation，用于更公平地分配 more multiplier、conversion、crit、penetration 等交互收益。第一阶段先输出 `Interaction` bucket。

---

## 7. AttributionReport

```rust
pub struct AttributionRequest {
    pub build: BuildSnapshot,
    pub selected_skill: Option<SkillInstanceId>,
    pub outputs: Vec<DisplayStatId>,
    pub group_by: AttributionGroup,
    pub mode: AttributionMode,
}

pub enum AttributionGroup {
    Source,
    ItemSlot,
    Item,
    ItemAffix,
    PassiveNode,
    SkillGem,
    SupportGem,
    Config,
}

pub enum AttributionMode {
    Direct,
    Marginal,
    DirectAndMarginal,
    MarginalWithInteraction,
}

pub struct AttributionReport {
    pub output: DisplayStatId,
    pub final_value: StatValue,
    pub entries: Vec<AttributionEntry>,
    pub interaction: Option<AttributionEntry>,
}

pub struct AttributionEntry {
    pub source: SourceId,
    pub value: f64,
    pub percent_of_final: Option<f64>,
    pub marginal_delta: Option<f64>,
    pub marginal_percent: Option<f64>,
    pub path: Vec<TraceNodeId>,
    pub explanation_key: String,
}
```

示例输出：

```text
Selected skill TotalDps = 1,250,000
├─ Weapon: +410,000 marginal DPS (+32.8%)
│  ├─ base physical damage
│  ├─ local increased physical damage
│  └─ attack speed
├─ Amulet: +96,000 marginal DPS (+7.7%)
│  ├─ critical multiplier
│  └─ added fire damage
├─ Passive node #41822: +42,000 marginal DPS (+3.4%)
│  └─ 12% increased attack damage
└─ Interaction: +58,000 DPS (+4.6%)
   └─ crit * more damage * penetration synergy
```

---

## 8. 计算实现要求

### 8.1 ModDB 查询必须可追踪

`ModDb::sum` / `more` / `flag` / `override_` 需要保留快速无追踪路径，同时提供 trace 版本：

```rust
pub fn sum(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64;

pub fn sum_traced(
    &self,
    mod_type: ModType,
    cfg: &CalcConfig,
    names: &[ModName],
    trace: &mut TraceGraph,
) -> TracedValue;
```

### 8.2 计算阶段必须输出中间节点

关键阶段：

- source collection
- modifier parse
- stat query
- base damage
- skill coefficient
- conversion
- added damage
- inc/reduced
- more/less
- crit
- hit chance
- enemy mitigation
- ailment/debuff
- resistance/floor/cap
- max hit/EHP
- recovery/cost

### 8.3 Comparison 使用相同 trace

`ComparisonResult` 不能只比较最终数值。它必须关联：

- changed inputs
- changed modifiers
- changed trace nodes
- affected display stats
- top gains/losses
- interaction bucket

---

## 9. 验收标准

全量属性覆盖完成的标准：

1. `sync-pob-catalog -- check --pob-root <PathOfBuilding> --catalog <catalog>` 通过。
2. 所有 PoB display/comparison stats 在 `DisplayStatCatalog` 中有定义。
3. 所有 PoB output key 有 `ParityStatus`。
4. `ParityStatus::Computed` 字段有测试或 golden fixture。
5. 选中技能输出能展示 hit/dot/damage type/ailment 占比。
6. 生存输出能展示 max hit/EHP/mitigation/recovery 占比。
7. 任意装备、天赋点、技能 gem、support gem、配置项可生成 marginal attribution。
8. 文档中不得出现“暂时只覆盖常见属性”作为最终设计目标；允许作为实现阶段状态。

---

## 10. 开发顺序

1. `DisplayStatId` 与 `ParityStatus`。
2. `sync-pob-catalog` 生成 PoB 属性覆盖矩阵。
3. `SourceId` / `ModifierSource` 贯穿 item/passive/gem/config。
4. `TraceGraph` 与 `TraceMode`。
5. `ModDb::*_traced` 查询。
6. `AttributionReport` 对当前已实现字段先跑通。
7. 扩展到完整 damage/survivability/recovery/cost。
8. PoB golden fixtures 对照所有 computed 字段。

---

## 11. 资料来源

- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcSetup.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcPerform.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcOffence.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcDefence.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `Calcs.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcSections.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `BuildDisplayStats.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `ConfigOptions.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `QuestRewards.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcBreakdown.lua`
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `Data.lua`
- DeepWiki 对 PathOfBuildingCommunity/PathOfBuilding-PoE2 calculation、display stats、breakdown 和 stat weight 的分析
