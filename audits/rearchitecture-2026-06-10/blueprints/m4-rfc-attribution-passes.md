# RFC(m4-attribution) — 双 pass × 归因模型 + ModFlags 30 位切换

> 状态：**草案待评审**（评审通过 = M4-T2 合并前置条件，见 m4-offence-deep.md §1.5 / §3.1）。
> 撰写：2026-06-11（pre-m4 前置波，自 m4-offence-deep.md §1 草案扩写为正式 RFC）。
> 评审人：主工作区 owner。修订直接改本文并在 commit message 标 `rfc(m4-attribution)`。
> 上游：`m4-offence-deep.md`（W-A1 / W-B1 / W-B2 / W-B3）、`00-index.md` §2.2（"M4 RFC: PassId/Combine 归因模型 → M4-T2；P17 红线约束 M2/M5a 不得提前改 TraceGraph"）、`audits/rearchitecture-2026-06-10/12-offence.md`（G1/G2/G3）、`devs/docs/architecture/10-pob-parity-and-attribution.md` §6-7。
> vendor 参照根：`vendor/PathOfBuilding-PoE2/src/`；下文所有行号已在 2026-06-11 逐段亲验。

---

## 0. 摘要与裁决清单

PoB2 进攻是 **2×2 嵌套 pass**（外层 MH/OH，内层暴击/非暴击），末端用 `combineStat`（8 种模式）与 `AverageHit` 混合公式合并。PoBR 的核心卖点 source-level 归因（TraceGraph DAG + AttributionReport）当前假设"一个输出 stat = 一条聚合链"，双 pass 打破该假设。本 RFC 给出**最小侵入、可整体回退**的模型扩展，并附带 ModFlags 30 位切换的完整位分配与双跑方案（W-A1 是 W-B2 的前置，两者共享 feature/双跑纪律，故并入同一 RFC 评审）。

| # | 裁决 | 一句话 |
|---|---|---|
| D1 | **pass = 单一 TraceGraph 内的节点级 `PassId` 分区**（§2） | 否决"每 pass 独立 graph"；`TraceNode` 增 `pass: Option<PassId>` 纯增量字段 |
| D2 | **combineStat = 带权重的 `TraceOperation::Combine` 合并节点**（§3） | direct 口径按一阶线性化权重摊销；非线性精确语义由 marginal 兜底（§5.2） |
| D3 | **`SourceId` / `SourceKind` 不扩展**（§2.4） | pass 归属是图节点属性而非来源属性；同一 SourceId 的 Input 节点允许逐 pass 重复出现 |
| D4 | **OutputTable per-hand 形态 = 强类型子表 `HandOutput`**（§4，蓝图开放问题 2） | 扁平 `MainHand.X` 键否决；PoB 扁平键映射收口在 display_catalog 层的 `pob_key` |
| D5 | **ModFlags 30 位按 PoB2 `Global.lua` 逐位等值落库**，feature `modflags-pob2` 双跑后切换（§6） | 武器类型位由 `weapon_types.json` 派生；现 5 位中 3 位位值搬家是必须双跑的原因 |

---

## 1. 问题陈述

### 1.1 vendor 的 2×2 嵌套结构（行号亲验）

- **外层 MH/OH pass**：`CalcOffence.lua:2369-2449`。`isAttack` 时按 `skillFlags.weapon1Attack/weapon2Attack` 构造 passList（label = "Main Hand"/"Off Hand"，各带独立 `source`（weaponData）与 `cfg`（weapon1Cfg/weapon2Cfg））；非攻击技能单 pass（label = "Skill"）。
- **内层暴击/非暴击 pass**：`CalcOffence.lua:3978-3980`，`for pass = 1, 2 do cfg.skillCond["CriticalStrike"] = (pass == 1)`——pass 1 = 暴击腿（`:4028-4032` 该腿 `allMult ×= CritMultiplier`），pass 2 = 非暴击腿。
- **内层合并**：`:4047-4057` 把两腿的 pre-mitigation 均值分存 `Stored<Type>CritAvg/HitAvg`，并按暴击率加权累计 `Stored<Type>CombinedAvg`；`:4395` `AverageHit = totalHitAvg × (1 − CritChance/100) + totalCritAvg × CritChance/100`（下称 **CritBlend**）。
- **外层合并**：`combineStat(stat, mode, ...)`（定义 `:2451-2538`，8 模式）；调用大表分布在 `:3023-3028`（命中/速度）、`:4554-4601`（暴击/伤害/leech/弩）、`:5737-5755`（异常）。

### 1.2 当前 PoBR 模型的破坏点

现 `trace.rs::TraceGraph`（153 行）是平铺 DAG：节点 = {Input(带 SourceId) | 算子}，`source_ancestors` / `attribution.rs::direct_value_for_source` 把输出节点的全部祖先 Input 按 SourceId 直加。该模型在双 pass 下失效于三点：

1. **同一 SourceId 在不同 pass 内贡献不同**——per-hand 词条（`Adds # Physical Damage to Attacks with Maces`）只进对应手的 pass；`increased Damage on Critical Hit` 类条件词条只进暴击腿。"全部祖先直加"会把两腿的 Input 混在一个桶里，丢失 per-pass 解释力。
2. **合并节点是非平凡多入度算子**——AVERAGE/DPS/CRIT/HARMONICMEAN/CHANCE 各有不同权重形状，部分非线性（CRIT-doubleHits、HARMONICMEAN）。直加语义在合并节点处既不正确（无权重）也不可解释（无法回答"这条腿贡献多少"）。
3. **无 pass 维度查询**——无法回答"这件副手武器贡献了多少 OffHand DPS"（PoBR 增量卖点的自然延伸需求）。

---

## 2. D1：pass = TraceGraph 子图（节点级 PassId 分区）

### 2.1 类型定义

```rust
// pobr-core/src/trace.rs 扩展（纯增量；现有字段/方法签名全部不变）

/// 手分区。Single = 法术/非攻击技能（PoB2 passList 的 "Skill" pass）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandTag { Single, MainHand, OffHand }

/// 暴击分区。Blended = 不在暴击双 pass 内、或已完成 CritBlend 合并的节点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CritTag { Blended, Crit, NonCrit }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId { pub hand: HandTag, pub crit: CritTag }

pub struct TraceNode {
    // ……现有字段（id/label/value/operation/source）不变……
    /// None = pass 无关节点（全局输入、防御、合并后的顶层输出）。
    pub pass: Option<PassId>,
}
```

### 2.2 备选方案否决记录

**否决"每 pass 独立 TraceGraph + 顶层 merge graph"**，理由（蓝图 §1.2 既有 + 补充）：

- `source_ancestors` / `direct_value_for_source` / `node_for` 等全部消费方都要改成跨图遍历，破坏面大；
- marginal 重算闭包（`attribution::attribute` 的 `recompute`）需持多图状态，违反"纯函数 + 调用方闭包"的现行设计（attribution.rs 模块头文档明确的契约）；
- 单图方案下 `TraceNode.pass` 是纯增量字段，**回退 = 不读该字段**，单 pass 路径无感（§8）。

### 2.3 节点/边类型与子图定义

- **节点类型**：现有 `TraceOperation` 全部变体保留不动；新增 `Combine`（§3）。Input 节点（`add_source_node`）与算子节点都可携带 `pass`。
- **边类型**：不新增边类型。`TraceEdge { from, to }` 维持无属性；Combine 的权重放在节点上（`weights[i]` 对应第 i 条入边，与 `add_edge` 插入顺序一致——`TraceGraph::incoming` 按 edges 向量顺序过滤，天然保序，已亲验 trace.rs:122-128）。
- **子图定义**：pass P 的子图 = `{n | n.pass == Some(P)}` ∪ 其引用的 pass-无关（`pass == None`）祖先。
- **合并节点的 pass 归属约定**：
  - 内层 CritBlend 节点（每手一个）：`pass = Some(PassId { hand, crit: Blended })`——它是该手的合并结果，仍属该手子图；
  - 外层 hand-combine 节点（每 stat 一个）：`pass = None`——它是全局输出，不属任何手。

### 2.4 D3：SourceId 不扩展

`SourceId { kind, id, label_key }` 与 `SourceKind`（18 变体，`pobr-data/src/source.rs`）**均不新增变体/字段**。论证：

1. 来源（装备槽/词条/天赋/宝石/配置）是 build 级事实，与"该来源在哪个 pass 内被消费"正交——后者是计算结构信息，归图节点（`TraceNode.pass`）承载。把 pass 编进 SourceId 会让同一件物品在归因报告里裂成多个"来源"，破坏 `AttributionRequest.sources` 的请求语义与既有消费方。
2. 武器本身已有天然来源身份：Weapon1/Weapon2 槽位的 `SourceKind::Item` SourceId 不同，per-hand 归因不需要 SourceId 再区分手。
3. **同一 SourceId 的 Input 节点允许在多个 pass 内各出现一次**（每 pass 的 `sum_traced`/`more_traced` 各自落 Input 节点，盖各自 pass 戳）。这是有意设计：同一来源在不同 pass 的贡献值本就不同（条件翻转、per-hand flags 过滤），归因按 pass 分桶后再经合并节点加权汇总。
4. 由 3 推出关键**结构不变式 I1**：**任意两个不同 pass 的子图不共享带 pass 戳的节点**；跨腿共享只发生在 `pass == None` 的祖先（如全局常量输入）。这是 §5.1 direct 算法逐腿独立遍历正确性的前提，落为 debug 断言 + 单测。

### 2.5 四象限如何叠加（2×2 → 图结构）

每个攻击 stat 的图形状（以 `AverageDamage`/`TotalDPS` 链为例）：

```
[MH·Crit 子图]──┐                          [OH·Crit 子图]──┐
                ├─ CritBlend(MH)·…·=AvgDmg(MH) ┐           ├─ CritBlend(OH)·…·=AvgDmg(OH) ┐
[MH·NonCrit]────┘        (pass=MH/Blended)     ├─ Combine{Dps}(pass=None) → TotalDPS 顶层 │
                                               │                                          │
                          [OH 同构] ───────────┘ ←────────────────────────────────────────┘
```

1. **内层先合**：每手内，暴击腿与非暴击腿经 `Combine { mode: CritBlend, weights: [1−c, c] }` 合并（c = 该手 CritChance/100）。c 自身是该手内的 Traced 值，其来源归因走 c 节点自己的入边（即"提升暴击率的词条"在 direct 口径经 c 的子图贡献，不经 weights——weights 是冻结在当前点的常数，见 §3.2）。
2. **外层后合**：两手的同名 stat 经 `Combine { mode: <COMBINE_TABLE[stat]>, weights: … }` 合并为顶层输出。模式查 `:3023-3028 / :4554-4601 / :5737-5755` 照抄的 Rust 静态表 `COMBINE_TABLE: &[(&str, CombineMode)]`（W-B2）。
3. **非攻击技能**：单 `HandTag::Single` pass，hand-combine 退化为 OR 直通（weights = [1.0]，单入边）；内层 CritBlend 照常。这保证"消灭特例"（W-B2 设计）同时单腿图形状与现状同构。
4. **跨象限例外——`Stored<Type>*` 族**：`:4047-4057` 的 `StoredCombinedAvg` 在内层两轮 pass 间**累计**（`+= avg × c` / `+= avg × (1−c)`），图上表达为一个 `Combine { mode: CritBlend }` 节点（语义相同：crit 腿与 non-crit 腿按 c 加权），`StoredCritAvg/StoredHitAvg` 则是各腿原值直出（pass 戳分别为 Crit/NonCrit）。外层两手的 `StoredCombinedAvg` 按 DPS 模式合并（`:4588`）。该族是 ailment magnitude 的输入（W-B3 必落字段），归因链由此保持连续。

### 2.6 写入 API：pass scope 栈

```rust
impl TraceGraph {
    /// 进入 pass 作用域：之后 add_node / add_source_node 自动盖当前 pass 戳。
    pub fn begin_pass(&mut self, pass: PassId);
    /// 退出当前 pass 作用域（内部 Vec<PassId> 栈，支持 2×2 嵌套：hand 外层、crit 内层）。
    pub fn end_pass(&mut self);
}
```

- pass 管线代码（hand_pass.rs / crit_pass.rs）不需逐节点传参，包一层 scope 即可；嵌套时栈顶生效（MH 作用域内再 `begin_pass(MH·Crit)` 覆盖 crit 维度）。
- 栈空时 `add_node` 落 `pass: None`——既有全部调用点（防御、minimal 链路）零改动即保持现行为。
- debug_assertions 下 `end_pass` 栈空时 panic（编排 bug 早暴露）；release 静默忽略。

---

## 3. D2：combineStat = 合并节点的精确模型

### 3.1 类型定义

```rust
// trace.rs
#[derive(Debug, Clone, PartialEq)]
pub enum CombineMode {
    Or,
    Add,
    Average,
    Dps { double_hits: bool },
    Crit { double_hits: bool },
    HarmonicMean,
    Chance,          // 按 chance×HitChance 占比加权
    ChanceAilment,   // maxInstance×stacks 占比
    CritBlend,       // 内层：hitAvg×(1−c) + critAvg×c
}

pub enum TraceOperation {
    // ……现有变体不变……
    Combine { mode: CombineMode, weights: Vec<f64> },
}
```

`weights[i]` 对应第 i 条入边（`add_edge` 顺序）。约定入边顺序：**MH 先、OH 后**（CritBlend 为 NonCrit 先、Crit 后，即 weights = [1−c, c]）；构图侧（W-B2）写单测锁死顺序。

### 3.2 权重的精确定义

**weights = 该合并算子在当前计算点对每条入腿的一阶线性化系数**（即 `∂combined/∂leg_i` 在当前各腿取值处的值，必要时归一到"贡献摊销"口径）。direct 归因按"来源经腿 i 的 direct 贡献 × weights[i]"摊销（§5.1）。逐模式（vendor 公式行号均亲验）：

| mode | PoB2 公式 | vendor | weights(MH, OH) | 线性？ |
|---|---|---|---|---|
| OR | `MH or OH`（`mode=="OR"` **或 `not skillFlags.bothWeaponAttack`** 走此分支） | `:2453-2454` | 存在腿 1.0，另一腿 0 | 线性 |
| ADD | `MH + OH` | `:2455-2456` | (1, 1) | 线性 |
| AVERAGE | `(MH + OH) / 2` | `:2457-2458` | (0.5, 0.5) | 线性 |
| CRIT | doubleHits：`MH + OH − MH×OH/100`；否则 `(MH+OH)/2` | `:2459-2464` | doubleHits → 偏导 `(1−OH/100, 1−MH/100)`；否则 (0.5, 0.5) | doubleHits 非线性 |
| HARMONICMEAN | 任一腿为 0 → 0；否则 `2/(1/MH + 1/OH)`（Speed 用，`:3026`） | `:2465-2470` | 偏导 `(2·OH²/(MH+OH)², 2·MH²/(MH+OH)²)`；任一腿 0 → (0,0) | 非线性 |
| CHANCE | `MH×mainPortion + OH×offPortion`，portion = `chance×HitChance` 占比 | `:2471-2496` | (mainPortion, offPortion)——**portion 冻结为常数**（§3.4） | portion 联动非线性 |
| CHANCE_AILMENT | `maxInstance×maxInstanceStacks + minInstance×(1−maxInstanceStacks)` | `:2497-2532` | 大腿得 maxInstanceStacks、小腿得 1−maxInstanceStacks（stacks 占比冻结为常数） | max/min 选择 + stacks 联动非线性 |
| DPS | `MH + OH`，非 doubleHitsWhenDualWielding 再 `/2` | `:2533-2538` | doubleHits → (1,1)；否则 (0.5,0.5) | 线性 |
| CritBlend | `hitAvg×(1−c) + critAvg×c` | `:4395` | (1−c, c)；c = 该手 CritChance/100 **冻结为常数** | c 联动非线性 |

补充裁决（公式边角，照抄 vendor，不做"更合理"的发明）：

- **缺腿语义**：vendor 对 `MainHand[stat] or 0` 的 nil 腿按 0 参加 ADD/AVERAGE/DPS——Rust 侧 `HandOutput` 字段用 `Option<f64>`，缺腿按 0 折入且其权重位仍占位（weights 长度恒等于入边数）；OR/CHANCE/CHANCE_AILMENT 的缺腿走 `or` 直通（单腿权重 1.0）。
- **HARMONICMEAN 的零腿**：任一腿为 0 时输出 0（`:2466-2467`），此时 weights = (0, 0)（direct 归因得 0，与输出一致；marginal 兜底捕获真实敏感度）。
- **CRIT 模式只用于 `CritChance`**（`:4555`），单位是百分数（0-100），故交叉项除以 100。

### 3.3 权重的"冻结常数"裁决

CHANCE 的 portion、CHANCE_AILMENT 的 stacks 占比、CritBlend 的 c 都是**由其他 Traced 值派生的系数**。裁决：**direct 口径把它们冻结为合并时点的常数写进 weights**，不在 direct 内追它们的来源链；它们自身的来源解释由两条正交通道承担：

1. 该系数对应的 stat 自己就是顶层输出（如 `CritChance` 经 CRIT 模式合并后有自己的归因报告）；
2. marginal 口径剔除来源整体重算，系数联动被如实捕获（§5.2）。

理由：direct 本来就是"按公式形状摊"的解释性口径（现实现的"祖先 Input 直加"同样不追乘法因子的二阶效应）；把系数链折进 weights 等价于做全微分，复杂度上升但解释力反而下降（用户读不懂二阶摊销）。

---

## 4. D4：OutputTable per-hand 形态裁决（蓝图开放问题 2）

### 4.1 两案对比

| | 方案 A：强类型子表 | 方案 B：扁平键 |
|---|---|---|
| 形态 | `OutputTable { main_hand: Option<HandOutput>, off_hand: Option<HandOutput>, … }` | `OutputTable` 增 `HashMap<String, f64>` 或逐字段 `main_hand_dps: f64` 平铺 |
| 类型安全 | 编译期字段存在性；`Option` 表达"该 build 无副手 pass" | 字符串键运行期才炸 / 平铺则字段爆炸（每 stat ×2） |
| 与现状一致性 | OutputTable 是纯 struct（294 行无任何 map），`From<&MinimalOutput>` / `Default` 模式直接延续 | 引入 map 是对现 OutputTable 设计的第一次破例，且与"计算内部只用稳定 ID"约定相抵（字符串键无编译期校验） |
| 消费侧 | `extract_display_values(&OutputTable)`（display_catalog.rs）是强类型字段读取，子表 = 一次 `as_ref().map(...)` | map 键需要常量表防 typo；平铺则 Default/From 两处样板各 +N 行 |
| PoB 对拍 | PoB 的 `MainHand.X` 是 Lua table 嵌套，子表正是同构搬迁 | 扁平 `"MainHand.X"` 字符串才是发明（PoB 内部也不是这个形态） |

### 4.2 裁决：**方案 A（强类型子表），推荐采纳**

```rust
// pobr-core/src/calc/output.rs（W-B2 落地；字段集 = combineStat 入参面，§3.1 大表的 per-hand 侧）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HandOutput {
    pub accuracy: f64,
    pub hit_chance: f64,            // AccuracyHitChance（fraction）
    pub crit_chance: f64,
    pub pre_effective_crit_chance: f64,
    pub crit_multiplier: f64,
    pub speed: f64,                 // 该手攻速（HARMONICMEAN 入参）
    pub damage_components: Vec<DamageComponent>,   // 该手 per-type 击中分量（CritBlend 后）
    pub average_hit: f64,           // CritBlend 后均值
    pub average_damage: f64,        // × hit_chance 后
    pub total_dps: f64,             // 该手单独 DPS（合并前）
    pub stored_combined_avg: Vec<(DamageTypeId, f64)>,   // ailment 输入（§2.5 例外族）
    // 弩字段（W-D2）：firing_rate / reload_time / effective_bolt_count …按需 append
}

pub struct OutputTable {
    // ……现有字段不变，语义改为"combineStat 之后"（单手 build 走 OR 直通数值不变）……
    pub main_hand: Option<HandOutput>,
    pub off_hand: Option<HandOutput>,
}
```

**M5 display 对接论证**（开放问题 2 的核心顾虑）：display/CalcSections 侧需要的"扁平 PoB 键"（如 `MainHand.AverageDamage`）**收口在 display_catalog 层**——`DisplayStatDefinition` 已有 `with_pob_key(...)` 通道（display_catalog.rs 现状），per-hand 字段按 `computed("MainHandAverageDamage", …, "MainHand.AverageDamage")` 声明，`extract_display_values` 从子表取值。即：**扁平化是展示层职责，不是计算层数据形态**；两层各取所需，互不妥协。这与"计算内部只用稳定 ID，显示文本走 i18n"的既有分层约定同构。

衍生约定：

- 顶层既有字段（dps/crit_chance/…）语义改为"合并后"，单手 build 经 OR 直通逐值不变（W-B2 等价性测试的对象）；
- `main_hand`/`off_hand` 均 `Option`：非攻击技能两者皆 None（Single pass 直写顶层）；单手攻击 main_hand=Some、off_hand=None；
- `TracedMinimalOutput::node_for` 签名不变；新增 `node_for_pass(stat, PassId) -> Option<TraceNodeId>`；
- golden/attribution fixture 将变化：**重建 baseline 走独立 commit**（roadmap §1.1 纪律）。

---

## 5. 与现有 attribution.rs 三口径的兼容性论证

### 5.1 direct：腿内沿用现算法，合并节点处加权摊销

现实现（attribution.rs:205-226 `direct_value_for_source`）：从输出节点反向 visited-DFS，命中 `source` 的 Input 节点直加。扩展算法：

```
direct(source, output_node):
    若 output_node 不是 Combine：维持现 visited-DFS（遍历中遇 Combine 节点按下述递归）
    遇 Combine { weights } 节点 C：
        total = Σ_i weights[i] × direct(source, leg_i)    # leg_i = C 的第 i 条入边源点
    腿内（两个 Combine 之间 / Combine 以下）：现 visited-DFS 语义原样，visited 集合按腿独立
```

正确性依赖**不变式 I1**（§2.4：不同 pass 子图不共享带 pass 戳节点）——每条腿是独立子图，腿内 dedup 不跨腿串扰；`pass == None` 的共享祖先在多条腿内**各计一次再按腿权重加权**，这是正确语义（全局来源同时增益两手，ADD/doubleHits 权重 (1,1) 下计两次 = 它对"两手之和"的真实直接贡献；AVERAGE 权重和为 1，不虚增）。

**零回归论证**：无 Combine 节点的图（防御、现 minimal 链路、回退态）退化为单腿、权重 1 → 算法逐字节等价于现实现。带 Combine 但单手（OR 直通，weights = [1.0] 单入边）→ 同样等价。这就是 W-B2 "单手 build 逐值不变"等价性测试在归因侧的镜像（§9 checklist 第 1/4 条）。

非线性模式（CRIT-doubleHits / HARMONICMEAN / CHANCE / CritBlend）的 weights 是当前点一阶线性化（§3.2）——direct 口径本就是近似解释（现实现对乘法链同样不做精确分解），**精确语义统一由 marginal 兜底**：

### 5.2 marginal：零修改自动正确（非线性兜底）

`attribution::attribute()` 的契约是 `recompute(excluded) -> f64` 闭包重算"剔除来源后的最终输出"（attribution.rs:236-296）。双 pass 落地后该闭包重跑**整条 2×2 管线**（移除来源 → 四象限全部重算 → CritBlend/combineStat 重算）——任何非线性（doubleHits 交叉项、调和平均、portion/c 联动、CHANCE_AILMENT 的 max/min 翻转）都被如实捕获，`marginal_delta = final − final_without_source` 的定义与实现**一行不改**。

代价：每来源一次重算的计算量 ≈ ×4（2 hand × 2 crit）。处置：

- 进 W-F1 bench 门禁，traced/归因路径单独 case，预算 4×，超出记录不阻塞（归因非热路径，蓝图 §2 T0 既定）；
- 闭包内可复用 W-F1 若做的惰性短路（非双持跳 OffHand、无暴击条件词条短路 crit pass）——短路自身带等价性测试，归因端免费受益。

### 5.3 interaction：定义不变，量级预期变大并接受

`interaction = final − baseline − Σ(individual marginal deltas)`（attribution.rs:276-288）定义、实现、报告口径均不变。双 pass 下 interaction 桶会系统性变大——条件翻转（一个来源同时改变暴击率与暴击腿伤害）与合并非线性都进该桶，这正是该桶的设计语义（"无法归到单一来源的协同效应"）。报告消费方无需改动；文档（10-pob-parity-and-attribution.md §6.3）在 W-B1 合并时补一段"双 pass 下 interaction 的解读"说明即可。

### 5.4 per-pass 查询（新增能力，PoBR 增量卖点）

```rust
pub struct AttributionRequest {
    // ……现有字段不变……
    /// Some(p) 时 direct 口径只累计 node.pass == Some(p) 的 Input 节点。
    pub pass_filter: Option<PassId>,
}
```

- 回答"这件副手武器贡献了多少 OffHand DPS"：`pass_filter = Some(PassId { hand: OffHand, crit: Blended })` + §5.1 算法在腿内遍历时按 filter 过滤 Input。
- `pass_filter = None`（默认，`AttributionRequest::new` 不变）= 现行为，全部既有调用点零改动。
- marginal 口径与 pass_filter 正交（剔除来源永远是全局动作）；请求带 filter 时 marginal 字段照常计算，文档注明其口径仍是全局输出。

### 5.5 兼容性不变量汇总（落为测试）

| # | 不变量 | 验证方式 |
|---|---|---|
| I1 | 不同 pass 子图不共享带 pass 戳节点 | 构图后 debug 断言 + 双持 fixture 图结构单测 |
| I2 | 无 Combine 图上新 direct 算法 == 旧算法 | 既有 `tests/attribution.rs`（8.3K）/`tests/trace.rs` 零改动通过 |
| I3 | 单手 build：双 pass 路径输出 + 归因逐值 == 现状 | W-B2 等价性测试 + direct 报告对比 |
| I4 | `Σ weights × leg`（线性模式）== 合并节点 value | 每 CombineMode 一个权重单测（手算值） |
| I5 | CritBlend 恒等：blend(c, x·m, x) == x·(1+(m−1)c) == x·crit.effect | W-B3 数学恒等测试（替换 `offence.rs:293` 单因子的正确性证明） |
| I6 | marginal 在 doubleHits/CRIT 非线性样例上 ≠ direct 且符合手算 | W-B1 单测 |

---

## 6. D5：ModFlags 30 位位分配表（W-A1 规范附录）

> W-A1 是 W-B2 的硬前置（per-hand cfg 消费武器位），且与本 RFC 共享"双跑 diff 干净才切换"纪律，故位分配的权威版本收口在本 RFC，蓝图 §2 W-A1 表为其摘要。

### 6.1 位分配表（vendor `Data/Global.lua:222-259` 逐位亲验，u64）

| 位 | 常量 | 值 | 组 | | 位 | 常量 | 值 | 组 |
|---|---|---|---|---|---|---|---|---|
| 0 | ATTACK | `0x1` | damage mode | | 20 | MACE | `0x100000` | 武器类型 |
| 1 | SPELL | `0x2` | damage mode | | 21 | STAFF | `0x200000` | 武器类型（PoE2 = Quarterstaff） |
| 2 | HIT | `0x4` | damage mode | | 22 | SWORD | `0x400000` | 武器类型 |
| 3 | DOT | `0x8` | damage mode | | 23 | WAND | `0x800000` | 武器类型 |
| 4 | CAST | `0x10` | damage mode | | 24 | UNARMED | `0x1000000` | 武器类型 |
| 5 | THORNS | `0x20` | damage mode | | 25 | FISHING | `0x2000000` | 武器类型 |
| 8 | MELEE | `0x100` | damage source | | 26 | CROSSBOW | `0x4000000` | 武器类型 |
| 9 | AREA | `0x200` | damage source | | 27 | FLAIL | `0x8000000` | 武器类型 |
| 10 | PROJECTILE | `0x400` | damage source | | 28 | SPEAR | `0x10000000` | 武器类型 |
| 11 | AILMENT | `0x800` | damage source | | 29 | WARSTAFF | `0x20000000` | 武器类型 |
| 12 | MELEE_HIT | `0x1000` | damage source | | 30 | TALISMAN | `0x40000000` | 武器类型 |
| 13 | WEAPON | `0x2000` | damage source | | 32 | WEAPON_MELEE | `0x100000000` | 武器类 |
| 16 | AXE | `0x10000` | 武器类型 | | 33 | WEAPON_RANGED | `0x200000000` | 武器类 |
| 17 | BOW | `0x20000` | 武器类型 | | 34 | WEAPON_1H | `0x400000000` | 武器类 |
| 18 | CLAW | `0x40000` | 武器类型 | | 35 | WEAPON_2H | `0x800000000` | 武器类 |
| 19 | DAGGER | `0x80000` | 武器类型 | | | | | |

掩码（同文件）：

- `SOURCE_MASK = 0x600`（MELEE 不含——vendor 原值如此，= AREA|PROJECTILE）；
- `WEAPON_MASK = 0xF5FFF0000`。**vendor 事实标注**：该掩码 = 武器类 4 位 + 武器类型位中**除 WARSTAFF(0x20000000) 与 STAFF 之外**……展开核验：`0x5FFF0000` 含位 16-27 全部 + 位 28(SPEAR) + 位 30(TALISMAN)，**不含位 29(WARSTAFF)**。这是 vendor 原样（疑似 PoE2 沿 PoE1 掩码未补 Warstaff），**逐位照抄不修**；若上游修复随 vendor bump 同步。
- 位 6-7、14-15、31 在 vendor 中空置——Rust 侧**不得占用**（保持与 vendor 数值可直接对拍的能力是本表第一目的）。

**位值搬家清单**（必须双跑的原因）：现 `pobr-data/src/modifier.rs:36-42` 五位中，`ATTACK(1<<0)`/`SPELL(1<<1)` 位值不变；`MELEE 1<<2 → 1<<8`、`PROJECTILE 1<<3 → 1<<10`、`AREA 1<<4 → 1<<9` 三位**搬家**。任何按旧位值持久化/硬编码的地方都是雷——见 §6.4 步骤 5 的 fixture 检查项。

### 6.2 与 `weapon_types.json` 的派生关系

数据源：`data/4.5.0.3.4/base/weapon_types.json`（已落库，`{id, one_hand, melee, flag}`，亲验含 Bow/Claw/Crossbow/Dagger/Fishing Rod/Flail/None(Unarmed)/One Hand Axe/One Hand Mace/One Hand Sword/Spear/Staff(label=Quarterstaff)/Talisman/…）。派生规则**逐字对应** `CalcActiveSkill.lua:274-309 getWeaponFlags`（亲验）：

```
flags = 武器类型位[weapon_types.flag]            # "Mace" → MACE 等，名称→位映射表留 Rust（P1 L4 刹车：位枚举是框架语义）
if weapon.type != "None":                        # Unarmed（id="None"）只得类型位，不得 WEAPON/类位
    flags |= WEAPON
    flags |= one_hand ? WEAPON_1H : WEAPON_2H
    flags |= melee    ? WEAPON_MELEE : WEAPON_RANGED
```

名称→位映射闭集（flag 字符串共 17 个值，与 §6.1 武器类型位一一对应）：`Axe/Bow/Claw/Dagger/Mace/Staff/Sword/Wand/Unarmed/Fishing/Crossbow/Flail/Spear/Warstaff/Talisman`（+ 数据中暂未出现者保留位）。映射表带"未知 flag 名 → 警告 + NONE"的防御分支，drift 由 regen-check 兜底。

**本阶段不做**（vendor 同函数内的两个旁路，无消费 build，登记 M5+）：`countsAsAll1H`（`:292-294`，bor 六个 1H 类型位）、`asThoughUsing`（`:285`）。`MELEE_HIT` 不在 getWeaponFlags 内派生，由攻击 cfg 侧（melee 攻击技能）置位——W-B2 的 per-hand cfg 构造负责。

### 6.3 feature flag

- **名称：`modflags-pob2`**。定义在 `crates/pobr-data/Cargo.toml`（位常量所在 crate）；`pobr-core`/`pobr-build` 各声明同名 feature 透传（`modflags-pob2 = ["pobr-data/modflags-pob2"]`），workspace 顶层一处即可全开。
- 语义：开 = 30 位新表 + 武器位派生/解析双写生效；关 = 现 5 位表。**默认关**，直至 §6.4 步骤 4 翻转。
- 生命周期目标：M4 末删除（feature 翻默认 → 删旧常量 → 删 feature 声明），不留长期双轨。

### 6.4 双跑切换步骤（W-A1 commit 切分的规范版）

1. **commit-1（常量双套）**：新位表以 `#[cfg(feature = "modflags-pob2")]` 落 `pobr-data/src/modifier.rs`；同 crate 位值断言单测（逐常量 `==` Global.lua 数值，§6.1 表即测试期望值）。`KeywordFlags` 已对齐 vendor（modifier.rs:89-117 现状）不在本次范围。
2. **commit-2（派生与双写）**：`pobr-build` 构造 `WeaponContribution` 时按 §6.2 从 `weapon_types.json` 算 `flags: ModFlags` 新字段；`mod_parser.rs` 武器后缀段（`:1039-1047` 一带，`with maces` → 现 `UsingMace` condition）在 feature 下**同时**产出武器位（condition 字符串保留双写，两消费通道并存——旧 condition 路径是回退保险）。
3. **commit-3（双跑脚本）**：`devs/scripts/modflags-dualrun.sh` = `cargo test --workspace` 与 `cargo test --workspace --features modflags-pob2` 各跑 ninja_parity + golden，diff 报告。**预期 diff = 0**：此时尚无人按新位消费（武器位产出但 cfg 侧未置位，`is_subset_of` 空集恒匹配语义不变）。
4. **翻转（W-B2 落地后）**：per-hand cfg 开始消费武器位后再跑双跑，确认 parity 只升不降 → `modflags-pob2` 翻 default feature → 删旧 5 位常量与 `UsingMace` 类 condition 近似消费路径（**退役放 M4 末，单独 commit**）。
5. **检查项（贯穿）**：grep golden fixture / build code XML / serde 落盘面是否有序列化的 flags bits（`ModFlags` 是透明 u64，蓝图标注）——若有，fixture 重生与位值切换**同 commit**，避免双跑期 fixture 撕裂。

### 6.5 schema 常量草案（纯文档，供 W-A1 commit-1 直接取用；本 RFC 不落 .rs 文件）

```rust
// 草案：pobr-data/src/modifier.rs（feature = "modflags-pob2" 下的新表）
// 位值逐项 == vendor Data/Global.lua:222-259（§6.1 表）；注释保留 vendor 名便于对拍。
impl ModFlags {
    pub const NONE: Self = Self(0);
    // -- damage modes --
    pub const ATTACK: Self = Self(0x0000_0000_0000_0001);      // ModFlag.Attack
    pub const SPELL: Self = Self(0x0000_0000_0000_0002);       // ModFlag.Spell
    pub const HIT: Self = Self(0x0000_0000_0000_0004);         // ModFlag.Hit
    pub const DOT: Self = Self(0x0000_0000_0000_0008);         // ModFlag.Dot
    pub const CAST: Self = Self(0x0000_0000_0000_0010);        // ModFlag.Cast
    pub const THORNS: Self = Self(0x0000_0000_0000_0020);      // ModFlag.Thorns
    // -- damage sources --
    pub const MELEE: Self = Self(0x0000_0000_0000_0100);       // ModFlag.Melee（旧 1<<2，搬家）
    pub const AREA: Self = Self(0x0000_0000_0000_0200);        // ModFlag.Area（旧 1<<4，搬家）
    pub const PROJECTILE: Self = Self(0x0000_0000_0000_0400);  // ModFlag.Projectile（旧 1<<3，搬家）
    pub const SOURCE_MASK: Self = Self(0x0000_0000_0000_0600); // ModFlag.SourceMask
    pub const AILMENT: Self = Self(0x0000_0000_0000_0800);     // ModFlag.Ailment
    pub const MELEE_HIT: Self = Self(0x0000_0000_0000_1000);   // ModFlag.MeleeHit
    pub const WEAPON: Self = Self(0x0000_0000_0000_2000);      // ModFlag.Weapon
    // -- weapon types --
    pub const AXE: Self = Self(0x0000_0000_0001_0000);
    pub const BOW: Self = Self(0x0000_0000_0002_0000);
    pub const CLAW: Self = Self(0x0000_0000_0004_0000);
    pub const DAGGER: Self = Self(0x0000_0000_0008_0000);
    pub const MACE: Self = Self(0x0000_0000_0010_0000);
    pub const STAFF: Self = Self(0x0000_0000_0020_0000);       // PoE2 = Quarterstaff
    pub const SWORD: Self = Self(0x0000_0000_0040_0000);
    pub const WAND: Self = Self(0x0000_0000_0080_0000);
    pub const UNARMED: Self = Self(0x0000_0000_0100_0000);
    pub const FISHING: Self = Self(0x0000_0000_0200_0000);
    pub const CROSSBOW: Self = Self(0x0000_0000_0400_0000);
    pub const FLAIL: Self = Self(0x0000_0000_0800_0000);
    pub const SPEAR: Self = Self(0x0000_0000_1000_0000);
    pub const WARSTAFF: Self = Self(0x0000_0000_2000_0000);
    pub const TALISMAN: Self = Self(0x0000_0000_4000_0000);
    // -- weapon classes --
    pub const WEAPON_MELEE: Self = Self(0x0000_0001_0000_0000);
    pub const WEAPON_RANGED: Self = Self(0x0000_0002_0000_0000);
    pub const WEAPON_1H: Self = Self(0x0000_0004_0000_0000);
    pub const WEAPON_2H: Self = Self(0x0000_0008_0000_0000);
    /// vendor 原值（不含 WARSTAFF，§6.1 标注）；逐位照抄不修。
    pub const WEAPON_MASK: Self = Self(0x0000_000F_5FFF_0000);
}

// 草案：weapon_types.json flag 名 → 位（pobr-build 派生侧；闭集映射 + 防御分支）
const WEAPON_FLAG_BITS: &[(&str, ModFlags)] = &[
    ("Axe", ModFlags::AXE), ("Bow", ModFlags::BOW), ("Claw", ModFlags::CLAW),
    ("Dagger", ModFlags::DAGGER), ("Mace", ModFlags::MACE), ("Staff", ModFlags::STAFF),
    ("Sword", ModFlags::SWORD), ("Wand", ModFlags::WAND), ("Unarmed", ModFlags::UNARMED),
    ("Fishing", ModFlags::FISHING), ("Crossbow", ModFlags::CROSSBOW),
    ("Flail", ModFlags::FLAIL), ("Spear", ModFlags::SPEAR),
    ("Warstaff", ModFlags::WARSTAFF), ("Talisman", ModFlags::TALISMAN),
];
```

---

## 7. 实施清单

| 步骤 | 工作项 | 文件（归属见蓝图 §3.2） | 前置 | 测试 |
|---|---|---|---|---|
| 1 | 本 RFC 评审通过 | 本文 | — | §9 checklist |
| 2 | W-A1 commit-1/2/3（位表/派生/双跑） | `pobr-data/src/modifier.rs`、orchestrator 武器段、`mod_parser.rs` 武器短语段、`devs/scripts/modflags-dualrun.sh` | RFC §6 | 位值断言；双跑 diff=0；三条武器词条端到端 |
| 3 | W-F1 bench 基线 commit | `pobr-build/benches/perform_bench.rs` | — | criterion 基线落档 |
| 4 | W-B1：`PassId`/`begin_pass`/`Combine`/`pass_filter` 四件套 + §5.1 direct 算法 | `pobr-core/src/trace.rs`、`attribution.rs`（T2 独占） | 步骤 1 | I1-I6 全表；每 CombineMode 权重单测；既有 attribution/trace 测试零回归 |
| 5 | W-B2：hand_pass + `HandOutput` + COMBINE_TABLE | `calc/hand_pass.rs`（新）、`offence.rs`、`output.rs`（区块 append）、orchestrator 武器段 | 步骤 2(commit-2)、4 | 单手等价性（逐值）；双持 fixture 手算对拍；doubleHits DPS 不除 2 |
| 6 | W-B3：crit_pass + Stored* 族 + CritBlend | `calc/crit_pass.rs`（新）、`offence.rs` | 步骤 4；与 5 同 track 串行 | I5 恒等；on-crit 词条只放大 crit 腿；Stored* oracle 对拍 ≥3 build |
| 7 | ModFlags 翻默认 + 旧路径退役（独立 commit） | 同步骤 2 文件 | 步骤 5 后双跑 parity 只升不降 | 双跑脚本最终报告 |
| 8 | baseline bump（独立 `chore(parity)` commit） | `ninja_parity.rs` 常量 | 各行为步骤合并后 | parity 报告归功列明 |
| 9 | 文档回写：10-pob-parity-and-attribution.md §6.3 interaction 解读、CLAUDE.md trace 描述行 | docs | 步骤 4-6 合并 | — |

门禁（每步通用）：fmt + clippy `-D warnings` + workspace test + `parity_no_regression`；行为修复独立 commit 附 PoB2 一手依据。

## 8. 回退方案

分层回退，每层独立可执行：

1. **ModFlags（步骤 7 之前任意时点）**：feature 不翻默认即回退态——旧 5 位表 + condition 字符串路径完整保留；翻默认后发现问题 = revert 翻转 commit（旧常量删除与翻转分两个 commit 正是为此留窗口）。
2. **Combine / PassId（W-B1）**：两者是纯增量（新枚举变体 + `Option` 字段 + 默认 None 的请求字段）。回退 = 上层不构造 Combine 节点、不调 `begin_pass`——图退化为现平铺形态，§5.1 算法对无 Combine 图逐字节等价（I2），`pass_filter=None` 默认即现行为。trace.rs 本身无需 revert。
3. **双 pass 管线（W-B2/B3）**：单 pass 路径保留到 M4 阶段末（蓝图 §5 R8 既定）——`calculate_minimal_vs_enemy` 内部以"输入是否含多 HandSource / 暴击条件词条"分流，等价性测试（I3/I5）就是回退开关的正确性证明。回退 = 分流恒走旧路径的一行改动。
4. **OutputTable 子表**：`Option` 字段回退态恒 None，消费方（display_catalog 新增条目）按 None 跳过，不产生脏值。
5. **golden/baseline**：每次 bump 独立 commit，revert 范围清晰；fixture 重生脚本可重放。

## 9. 评审 checklist（合并 W-B1/B2/B3 的前置，扩写自蓝图 §1.5）

- [ ] 单手 + 无暴击条件词条 build：双 pass 路径输出与现单 pass **逐值相等**（I3，W-B2/B3 测试计划）。
- [ ] direct 权重表（§3.2）与 vendor 公式逐模式对得上（每模式一个单测，I4）；缺腿/零腿边角与 `:2453-2538` 行为一致。
- [ ] §5.1 算法在无 Combine 图上与现实现逐字节等价（I2：既有 attribution/trace 测试零改动通过）。
- [ ] marginal 在 doubleHits/CRIT 非线性样例上 ≠ direct 且符合手算（I6）。
- [ ] 不变式 I1 有 debug 断言 + 双持 fixture 图结构单测。
- [ ] `Stored<Type>*` 族落 HandOutput 且与 oracle 中间值对拍 ≥3 build（ailment 链不断，W-B3）。
- [ ] bench：perform_bench ≤ 2.5× 基线；traced/marginal 路径 ≤ 4×（超出记录不阻塞）。
- [ ] ModFlags：位值断言单测全绿；双跑 diff=0（切换前）；fixture 序列化位检查（§6.4 步骤 5）完成。
- [ ] per-hand display 字段命名与 `pob_key`（`MainHand.X` 形）一经合入 display_catalog 不再改（蓝图 §3.3-6）。
- [ ] 评审人签字：主工作区 owner。

## 10. 开放问题（不阻塞评审，留给实施期）

1. **CHANCE/CHANCE_AILMENT 的 portion 来源展示**：§3.3 冻结为常数后，breakdown 层（M5 display）是否要把 portion 数值作为说明行展示（PoB2 breakdown 有，`:2478-2492`）——属展示层范畴，W-B2 落 `HandOutput` 时把 portion 留在合并节点 label 里即可支持。
2. **惰性短路是否默认开启**（非双持跳 OffHand pass / 无暴击条件词条短路 crit pass）：等 W-F1 基线数据；若开启，短路与全跑的等价性测试纳入 I3 同组。
3. **`node_for_pass` 是否需要按 stat × pass 全量登记**：初版只对 combineStat 大表内的 stat 登记 per-hand 节点；防御等 pass-None 输出不登记。若 M5 display 需要更细粒度再扩。
