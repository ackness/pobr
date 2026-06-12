# RFC(m4-attribution) 评审报告 — 双 pass × 归因模型 + ModFlags 30 位

> 评审人：M4 阶段独立评审（P17/R8）
> 评审材料：`m4-offence-deep.md` §1、`m4-rfc-attribution-passes.md`（全文）、`trace.rs`/`attribution.rs`/`mod_db.rs`/`modifier.rs`/`offence.rs` 现状、`20-target-architecture.md` P17/R8
> 日期：2026-06-12

## 0. 评审结论

**APPROVED-WITH-CONDITIONS**

模型骨架（D1 单图 + PassId 分区、D2 Combine 节点、D3 SourceId 不扩展、D4 强类型子表、D5 ModFlags 30 位双跑）方向正确，回退路径设计扎实，与 vendor 2×2 结构同构。但 RFC 在三处存在**模型正确性缺陷或表述不实**，需在 T2 实施时满足下列 C1–C6 条件方可动工/合并。这些条件均为局部修正，不动摇整体裁决。

核心问题集中在：(1) §5.1 把 direct 描述为「纯增量字段」与回退「不读字段」自相矛盾——它实际是**算法替换**；(2) CRIT-doubleHits 权重的线性化在「direct 各来源之和 = 输出」一致性上**不成立**，RFC 自己也未声称成立，但 checklist I4 的措辞会让实施者误以为所有模式都满足该不变式；(3) marginal「零修改自动正确」对 per-hand 过滤来源成立，但**与 §5.4 pass_filter 的交互**未定义清楚。

---

## 1. §1.5 / §9 checklist 逐项裁定

§1.5（蓝图摘要）与 §9（RFC 扩写版，含 I1–I6）实为同一份，按 §9 的细化项裁定：

| # | checklist 项 | 裁定 |
|---|---|---|
| 1 | 单手+无暴击条件词条 build 双 pass 输出逐值相等（I3） | **PASS**。数学上 OR 直通 + CritBlend 恒等（I5）保证；现 `calculate_minimal_vs_enemy` L296-300 的单因子 `non_crit×crit.effect` 与 blend 恒等成立（见 §2-b）。 |
| 2 | direct 权重表逐模式对得上（I4）+ 缺腿/零腿边角 | **PASS-WITH-CONDITION C2**。线性模式（OR/ADD/AVERAGE/DPS）权重正确且满足「Σweights×leg == value」。但 I4 不变式被表述为对**所有** CombineMode 成立，对非线性模式（CRIT-doubleHits/HARMONICMEAN）**不成立**——见 §2-a。条件：I4 单测须按线性/非线性分两组，非线性组只断言「权重 == 当前点偏导」而非「加权和 == 输出」。 |
| 3 | §5.1 算法在无 Combine 图上逐字节等价（I2，既有测试零回归） | **PASS-WITH-CONDITION C1**。结论正确（无 Combine 节点时递归不触发，退化为现 `direct_value_for_source`），但前提是新算法**正确实现了「遇 Combine 才递归、否则维持全局 visited DFS」的分流**。现状 `direct_value_for_source`（attribution.rs:205-226）是**单一全局 visited 的扁平 DFS**，改成「腿内独立 visited + Combine 处递归」是**重写**，不是 RFC §8 回退条所说的「不读字段即回退」。条件见 C1。 |
| 4 | marginal 在 doubleHits/CRIT 非线性样例 ≠ direct 且符合手算（I6） | **PASS**。marginal 走整管线重算闭包，非线性如实捕获，与 direct 一阶近似必然有差，手算可验。 |
| 5 | 不变式 I1（不同 pass 子图不共享带 pass 戳节点）debug 断言 + fixture 图结构单测 | **PASS-WITH-CONDITION C3**。I1 是 §5.1 direct 正确性的基石，设计自洽。但 I1 与 §2.4 条款 3「同一 SourceId 的 Input 节点允许逐 pass 各出现一次」必须配合一条更强不变式：`pass==None` 的共享祖先在多腿 DFS 中**按腿各计一次**——这正是 §5.1 想要的语义，但当前全局 visited 实现会把共享祖先**只计一次**（去重）。条件见 C3。 |
| 6 | `Stored<Type>*` 族落 HandOutput 且 oracle 对拍 ≥3 build | **PASS**。属 W-B3 实现验收，模型层 §2.5 表达自洽（StoredCombinedAvg = CritBlend 节点）。 |
| 7 | bench perform ≤2.5×、traced/marginal ≤4× | **PASS**。预算合理；marginal ×4 是 2hand×2crit 的直接后果。注意 §2-c 指出 marginal 在 per-source 上是 ×4×|sources|，但归因非热路径，4× 单次重算预算口径成立。 |
| 8 | ModFlags 位值断言全绿 + 双跑 diff=0 + fixture 序列化位检查 | **PASS-WITH-CONDITION C5**。位表（§6.1）与 `Global.lua` 对齐方案完整，搬家清单（MELEE/AREA/PROJECTILE）准确。条件：WEAPON_MASK 的「不含 WARSTAFF」需落一条解释性断言注释 + fixture 序列化位检查须在 commit-1 前先 grep 确认现状无落盘（见 C5）。 |
| 9 | per-hand display 字段命名 + pob_key 一经合入不再改 | **PASS**。D4 收口于 display_catalog `pob_key` 的分层正确（见 §5）。 |
| 10 | 评审人签字 | 本报告即评审；签字＝APPROVED-WITH-CONDITIONS。 |

---

## 2. 模型正确性深查

### (a) direct 线性化权重在 Combine 节点的数学一致性

**核心问题：direct 各来源之和是否仍等于输出？**

- **线性模式（OR/ADD/AVERAGE/DPS-非doubleHits/DPS-doubleHits）**：成立。这些模式 `combined = Σ wᵢ·legᵢ`，wᵢ 为常数，故 `Σ_source direct(source) = Σ_source Σ_leg wᵢ·directᵢ(source) = Σ_leg wᵢ·legᵢ = combined`（前提：每条腿内 direct 各来源之和 == 该腿值，这是现 direct 算法在单链上的已有性质）。

- **CRIT-doubleHits**：`combined = MH + OH − MH·OH/100`。RFC §3.2 给权重 `(1−OH/100, 1−MH/100)`。则 `wₘₕ·MH + wₒₕ·OH = MH(1−OH/100) + OH(1−MH/100) = MH + OH − 2·MH·OH/100 ≠ combined`（交叉项被减了**两次**）。**加权和 ≠ 输出**。这是一阶偏导线性化的固有性质（双线性函数的偏导和会重复计交叉项）。RFC §3.2 文字承认「一阶线性化」，但 **§9 checklist I4「`Σ weights × leg`（线性模式）== 合并节点 value」用括号限定了「线性模式」——这是对的**。问题在 §1.5 蓝图摘要项「direct 权重表与 vendor 公式逐模式对得上」措辞含糊，实施者可能写出一个对 doubleHits 也断言加权和==value 的错误单测。

- **HARMONICMEAN**：`combined = 2/(1/MH+1/OH)`。权重 `(2·OH²/(MH+OH)², 2·MH²/(MH+OH)²)`。加权和 = `2(MH·OH²+OH·MH²)/(MH+OH)² = 2·MH·OH/(MH+OH) = combined`。**恰好成立**（一阶齐次函数的欧拉定理，调和平均是 1 次齐次）。所以 HARMONICMEAN 的权重和反而 == 输出，与 doubleHits 行为不同。RFC 把两者并列为「非线性、加权和不保证」，**对 HARMONICMEAN 是过度保守的正确**（不是错，但描述不精确）。

**裁定**：模型对 direct「整体一致性」的处理是**有意放弃全局守恒、用 marginal 兜底**，这一裁决（§3.3）合理且与现实现哲学一致（现 direct 对乘法链也不守恒）。**条件 C2**：I4 单测必须显式分三类断言——线性模式断言「加权和==value」；HARMONICMEAN 单独断言「加权和==value（齐次巧合）」；CRIT-doubleHits/CHANCE/CHANCE_AILMENT/CritBlend 仅断言「权重==解析偏导」且**显式注释 direct 在这些模式下不守恒、守恒由 marginal 兜底**。否则实施者会困惑于「为什么 doubleHits 的 direct 加起来不等于 DPS」。

### (b) marginal 口径在双 pass 下的语义 + combineStat 权重重算

**问题：移除影响 MH 但不影响 OH 的来源时，combineStat 权重是否应重算？**

marginal 的定义是 `final − recompute(without source)`，`recompute` 闭包重跑**整条 2×2 管线**（§5.2，且现状 attribution.rs:236-296 + tests/attribution.rs:89 已证实闭包由调用方提供、重跑 `calculate_minimal`）。因此：移除「只影响 MH」的来源时，重算闭包会得到新的 MH 腿值、**新的 combineStat 权重**（如 CritBlend 的 c、CHANCE 的 portion 都随之变），合并节点用新权重重算。**这正是 marginal 的优点——权重自动重算，无需 RFC 特别处理。RFC 选择的是「marginal 不冻结权重，整管线重跑」，与 direct「冻结权重」对立互补。**

**边界 case 验证**：
- 权重退化为 0（OR 模式缺腿 / HARMONICMEAN 零腿）：marginal 重算时该腿恒 0，delta 自洽；direct 在零腿处权重 (0,0)，direct 值 0，与输出 0 一致。**成立**。
- 权重退化为 1（OR 单腿直通）：marginal 重算等于单 pass 重算，direct 权重 [1.0] 等于现算法。**成立**，且这正是 I3 单手等价性的来源。

**裁定 PASS**，但**条件 C4**：RFC §5.4 说「marginal 与 pass_filter 正交（剔除来源永远是全局动作）」。这埋了一个语义陷阱——当用户请求 `pass_filter = Some(OffHand)` 且 `mode = DirectAndMarginal` 时，direct 字段是 OH 腿过滤值，marginal 字段是全局 delta，**两者口径不同却同列一个 entry**。RFC §5.4 末句已注明「marginal 字段口径仍是全局」，但这需要在 `AttributionEntry` 层面有显式标记或文档强约束，否则消费方（CLI/M5 display）会把两个不同口径的数并排展示导致误读。条件：pass_filter 非 None 时，要么 marginal 字段置 None（拒绝混口径），要么 entry 带 `pass_scope` 标记，二选一并在 RFC §5.4 定稿。

### (c) PassId 分区与既有 TraceGraph 消费者的兼容性

既有消费者：`source_ancestors`（trace.rs:138）、`direct_value_for_source`（attribution.rs:205）、`tests/attribution.rs`、`tests/trace.rs`、CLI 输出、M3 buff_pass 的 trace 写入。

- `source_ancestors`：RFC §1.2 称「行为不变，合并节点入边自然跨 pass」。**核实成立**——它只收集 source、不关心 pass、不去重权重，单纯遍历祖先。新增 Combine 节点对它透明。**PASS**。
- `direct_value_for_source`：**这是唯一被实质改写的消费者**（见 C1）。现状全局 visited DFS，新算法需腿内独立 visited + Combine 递归。RFC §5.1 伪代码描述了新算法，但**没有指出现状是全局 visited、改造是重写**。§8 回退条 2「trace.rs 本身无需 revert，上层不构造 Combine 即退化」**对 trace.rs 成立，但对 attribution.rs 不成立**——新 direct 算法即使在无 Combine 图上也是新代码路径，I2 零回归靠的是「无 Combine 时递归分支不触发」，不是「字段没被读」。**这是 §8 回退表述的不实之处**，见 C1。
- CLI 输出：`TraceNode` 增 `pass: Option<PassId>` 是结构体加字段。需确认 CLI 是否对 `TraceNode` 做了**穷尽式匹配或序列化**。从 trace.rs 看 `TraceNode` 无 `#[non_exhaustive]`，`TraceOperation` 加 `Combine` 变体会让任何对 `TraceOperation` 做穷尽 match 的代码**编译失败**。需 grep 确认（见 C6）。

---

## 3. 回退路径核查（§5 风险表 / §8 回退条）

§5 R8 行与 §8 声称「Combine 节点与 PassId 是纯增量字段，回退=不读字段，单 pass 路径保留到阶段末」。逐项核查：

1. **`TraceNode.pass: Option<PassId>` 加字段**：真增量。默认 None，写侧靠 begin_pass 栈，栈空落 None（§2.6）。`add_node`/`add_source_node` 现签名不变，内部读栈。**回退成立**（不 begin_pass 即全 None）。✔

2. **`TraceOperation::Combine` 加枚举变体**：**半增量**。对构造侧是增量（不构造即不出现）。但对**任何穷尽匹配 `TraceOperation` 的现有代码**是破坏性变更（需加 match 臂）。RFC 未核查现状有无穷尽匹配点。**这是 §5/§8「纯增量」声称的第一个漏洞**——见 C6。

3. **`AttributionRequest.pass_filter: Option<PassId>` 加字段**：真增量，`new()` 默认 None（§5.4）。但 `AttributionRequest` 现 `#[derive(PartialEq)]` 且字段全 pub，加字段会让**所有结构体字面量构造点编译失败**（除非都用 builder）。现状 attribution.rs:86-110 提供 builder（`new` + `with_*`），若全仓构造点都走 builder 则增量；若有字面量构造则破坏。需确认（并入 C6）。

4. **`direct_value_for_source` 算法重写**：**非增量**（见 §2-c、C1）。这是回退表述最大的不实点。正确表述应为：「新 direct 算法在无 Combine 节点的图上与旧算法**行为等价**（I2 保证），但代码是替换；回退=保留旧函数副本或 git revert attribution.rs 改动」。

5. **双 pass 管线（W-B2/B3）单 pass 保留到阶段末**：成立。`calculate_minimal_vs_enemy` 内部分流（输入是否多 HandSource）是真回退开关，I3 等价测试是其正确性证明。✔

**裁定**：回退**大方向成立**（最坏情况可整体 revert T2 commits，单 pass 路径独立保留），但 §5/§8「纯增量字段」「trace.rs 无需 revert」的措辞对 `TraceOperation::Combine` 穷尽匹配和 `direct_value_for_source` 重写**不准确**。条件 C1、C6。

---

## 4. 与 M3 落地形态的冲突检查

### (a) ModTag actor 维度 / EvalContext（开放问题 1）

现状 `modifier.rs` 已落地 `ActorRef`（Player/Parent/Minion）、`ModTag::Multiplier{actor, limit_var, limit_actor}`、`ModTag::Condition{actor}`，且 `effective_number(&self, cfg: &CalcConfig)` 签名**仍是 `&CalcConfig`**——M3 通过 `cfg.actor_multipliers` 快照承载 actor 取数，**未升级 effective_number 入参为 EvalContext**。

RFC 本身不动 `effective_number`（归因 RFC 与 EvalContext 是 W-A3 的事，开放问题 1 已正确登记由 T1 对齐）。**对 RFC 无冲突**。RFC §2.4 条款 3「同一 SourceId 的 Input 节点逐 pass 各出现一次」依赖 per-hand cfg 翻转——M3 的 actor 快照机制与 hand pass 的 cfg 克隆翻转**正交不冲突**：hand pass 改的是 `cfg.flags`/`cfg.conditions`，actor 快照是另一组键。**PASS**。

### (b) buff_pass 的 TraceGraph 写入（M3-T3-C2/C3）

buff_pass 写入 ModDb（注入缩放后的 modifier），**不直接写 TraceGraph 节点**——TraceGraph 由 sum_traced/more_traced 从 ModDb 重建。因此 buff_pass 注入的 modifier 在双 pass 下会被每个 pass 的 sum_traced 各落一次 Input 节点（符合 §2.4 条款 3）。

**冲突点**：buff_pass 注入的全局增益 modifier 本身没有 pass 标记，它们落的 Input 节点会带当前 pass 栈顶的 pass 戳——与 §5.1 假设的「全局来源是 pass==None 共享祖先」不一致。实际影响：因为 §2.4 明确「每 pass 的 sum_traced 各落 Input 节点」，结果数值正确；但 §5.1「pass==None 的共享祖先按腿各计一次再加权」这一句在 sum_traced 逐 pass 落节点的实现下基本不触发（没有真正跨腿共享的带值 Input 节点；共享的只会是无 source 的常量节点）。

**裁定 C-INFO（非阻塞）**：RFC 应补一条「M3 buff_pass 适配说明」段，明确 buff 注入物在双 pass trace 中的落点（带 pass 戳的 Input，不是 None 共享祖先）；唯一的 `pass==None` 节点是结构性常量（base 输入）与外层 hand-combine 输出节点。建议并入 C3 的不变式定稿。

---

## 5. 裁决 §7-Q2：per-hand 子表 vs 扁平键

RFC D4 已选**方案 A（强类型子表 `HandOutput`）**，扁平 PoB 键收口于 display_catalog 的 `pob_key`。**评审同意方案 A**：

1. **与现状架构一致**：`OutputTable` 现为纯 struct，引入 `HashMap<String,f64>` 会破坏「计算内部只用稳定 ID」的项目铁律（CLAUDE.md 关键约定）。
2. **M5 display_catalog 对接无损**：计算层强类型子表 + 展示层 `pob_key="MainHand.AverageDamage"` 与现有 `DisplayStatDefinition::with_pob_key` 通道同构。
3. **Option 语义清晰**：`main_hand/off_hand: Option<HandOutput>` 天然表达「无副手 pass」「非攻击技能两者皆 None」，回退态恒 None 不产脏值。
4. **PoB 对拍同构**：PoB 的 `MainHand.X` 本身是 Lua table 嵌套，子表是同构搬迁。

**唯一补充条件（并入 C6）**：建议 T2 落地时先冻结 HandOutput 的最小字段集（combineStat 入参面），弩/ailment 扩展字段用独立后续 commit append，避免 display_catalog 反复改 pob_key。

---

## 6. 合并前置条件汇总（T2 实施时满足即可动工）

- **C1（HIGH）**：修正 §8 回退条 2 与 §5.1 的表述——明确 `direct_value_for_source` 是**算法重写**（现状 attribution.rs:205 全局 visited DFS → 腿内独立 visited + Combine 递归），I2 零回归靠「无 Combine 时递归分支不触发」而非「字段未读」。回退方案应为保留旧函数副本或整体 revert，而非「不读字段」。

- **C2（HIGH）**：I4 单测分三组断言——线性模式（含 HARMONICMEAN 齐次巧合）断言「Σweights×leg == value」；CRIT-doubleHits/CHANCE/CHANCE_AILMENT/CritBlend 仅断言「权重 == 解析偏导」并显式注释 direct 在这些模式下不守恒、守恒由 marginal 兜底。§1.5 蓝图摘要「逐模式对得上」措辞同步收紧。

- **C3（MEDIUM）**：I1 定稿须配套一条更强不变式并落 debug 断言——带值 Input 节点（带 source）恒带 pass 戳且不跨腿共享；`pass==None` 节点仅限结构性常量与外层 combine 输出。澄清 §5.1「pass==None 共享祖先按腿各计一次」在 sum_traced 逐 pass 落节点实现下的实际触发条件（基本不触发）。

- **C4（MEDIUM）**：定义 `pass_filter` 非 None 时 marginal 字段的口径处置——置 None（拒绝混口径）或给 entry 加 `pass_scope` 标记，二选一写入 §5.4。防止 CLI/M5 把 OH 腿 direct 与全局 marginal 并排误读。

- **C5（MEDIUM）**：ModFlags 切换——(a) WEAPON_MASK「不含 WARSTAFF」落解释性断言注释；(b) commit-1 前先 grep 确认 golden fixture / build code XML / serde 落盘面**当前无序列化 flags bits**，若有则 fixture 重生与位切换同 commit。

- **C6（MEDIUM，编译破坏核查）**：W-B1 落地前核查并记录三处「加字段/加变体」的实际增量性——(a) 全仓有无对 `TraceOperation` 的穷尽 match；(b) 有无对 `TraceNode`/`AttributionRequest` 的结构体字面量构造点；(c) `HandOutput` 最小字段集冻结、弩/ailment 字段独立 commit append。若存在穷尽匹配点，需在 RFC §8 注明这些点需同步加臂。

**非阻塞（INFO）**：补一段「M3 buff_pass 适配说明」（§4-b），明确 buff 注入的全局 modifier 在双 pass trace 中经各 pass sum_traced 落为带 pass 戳的 Input 节点。
