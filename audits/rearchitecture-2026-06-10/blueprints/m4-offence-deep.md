# M4 实施蓝图 — 进攻深水区（MH/OH 双 pass · 暴击双 pass · 全乘区 · 技能 DoT · 触发数据接线）

> 2026-06-11 · 规划者产出，供多 agent 并行实施。自包含：实施 agent 只需读本文 + 代码 + vendor Lua，不需回读 roadmap/审计。
> 上游依据：`audits/rearchitecture-2026-06-10/21-roadmap.md` M4 节、`12-offence.md`、`14-triggers-minions-ailments.md`（gap 1/2/8）、`10-mod-system.md`（gap 3/5/6）、`20-target-architecture.md`（P7/P12/P17/P18、§5 DSL 硬边界）。
> vendor 参照根：`vendor/PathOfBuilding-PoE2/src/`（下文 `CalcOffence.lua` 等均指 `Modules/` 下文件；行号已逐段亲验，与审计一致）。

---

## 0. 阶段定位、目标与前置假设

**目标**（roadmap M4 原文）：MH/OH 与暴击双 pass、全乘区补齐、技能 DoT、触发数据接线——**进攻 parity 冲 70%@5%**（当前 24%@5%）；防御 ≥85% 不倒退。

**对应缺口**：12-G1（MH/OH 双 pass）、12-G2（ModFlags 位宽）、12-G3（暴击双 pass）、12-G4（Double/Triple 乘区）、12-G5（技能 DoT）、12-G6（dpsMultiplier/quantityMultiplier）、14-G1（触发 configTable 61 项）、14-G2（触发源速率）、10-G3 余下 tag（PerStat/globalLimit）、10-G6（写侧原语）、16-G4 部分（弩 reload 消费侧）。

**前置假设**（M0 交付 + 主工作区 W3 进行中）：

1. 三层数据目录 `data/<ver>/{base,overlay,generated}` + manifest v2 + overlay merge 引擎已就绪（`crates/pobr-gamedata/src/{overlay.rs,manifest.rs,domains/}`）。
2. **常量注入管道已存在**：`CalcConfig.constants: RuntimeConstants`（`pobr-core/src/config.rs:38`，注入入口 `CalculationSession::set_constants`，挂 cfg 线程化到全部 calc 函数）；`GameData::load_ruleset()`（`pobr-gamedata/src/ruleset.rs`）聚合骨架在，M4 新表沿用同一注入模式。本蓝图所有"新常量/新表"一律走该管道，**禁止新增硬编码魔数**（CI 有 no_embedded_data lint）。
3. extract-lua 子命令（`tools/sync-pob-catalog`，luajit 执行 vendor 后序列化）可用——`trigger_configs.json` 等 overlay 表用它抽取。
4. 九张 L1 常量表已落库（`data/4.5.0.3.4/base/`：`game_constants.json`、`weapon_types.json`、`unarmed_data.json`、`monster_scaling.json` 等）。`weapon_types.json` 已含 `{id, one_hand, melee, flag}`（亲验）。

**现状代码地图**（进攻域）：

| 文件 | 行数 | 角色 |
|---|---|---|
| `crates/pobr-core/src/calc/offence.rs` | 1244 | 单 pass 主管线：`calculate_minimal_vs_enemy`（L183），DPS 末端 L297 `dps = total_hit_avg_for_dps × action_rate × hit_chance` |
| `crates/pobr-core/src/calc/damage.rs` | 716 | 伤害分量/转换链；`DamageComponent::avg()` 写死中点（L99-102） |
| `crates/pobr-core/src/calc/crit.rs` | 431 | 暴击管线（resolve_crit + traced），逐字对齐 PoB2，**保留不动**，双 pass 在其外层包 |
| `crates/pobr-core/src/calc/skill_use_time.rs` | 188 | 速度 bucket（`speed_names_for_db`），弩 reload 折算落点 |
| `crates/pobr-core/src/calc/ailment.rs` | 1458 | 异常 DoT（与技能 DoT 是两路，互不覆盖） |
| `crates/pobr-core/src/calc/perform.rs` | 853 | 编排 + fill 机制阶段（`fill_trigger` L381+） |
| `crates/pobr-core/src/calc/trigger.rs` | 1247 | 冷却/CWC/轮转触发模型 |
| `crates/pobr-core/src/{mod_db,modifier,trace,attribution}.rs` | 502/189/153/305 | 聚合内核 + 归因（本阶段最大模型扩展点） |
| `crates/pobr-data/src/modifier.rs` | 208 | `ModFlags` 仅 5 位（L36-42）、`KeywordFlags` 13 常量 |
| `crates/pobr-build/src/calc_orchestrator.rs` | 2662 | `weapon_contribution`（L1096，只读 Weapon1）、`trigger_modifiers`（L1498）、`in_group_trigger_source_rate`（L1556，用基础 use_time——14-G2 根因） |
| `crates/pobr-build/tests/ninja_parity.rs` | — | 18-build 门禁（`parity_no_regression` 断言 ≥ baseline 常量） |
| `crates/pobr-core/benches/mod_db_bench.rs` | — | 5000-mod sum/more 吞吐基准 |

---

## 1. 归因 RFC 草案（P17 / R8）——**蓝图内直接给出，实施前评审通过是 T2 合并前置条件**

### 1.1 问题陈述

PoB2 进攻是 **2×2 嵌套 pass**：外层 MH/OH（`CalcOffence.lua:2369-2449` passList），内层暴击/非暴击（`:3978-3980` `for pass=1,2; cfg.skillCond["CriticalStrike"]=(pass==1)`），末端 `combineStat`（`:2453-2538`，8 种模式）合并双手、`AverageHit = hitAvg×(1-c)+critAvg×c`（`:4395`）合并暴击。PoBR 的核心卖点是 source-level 归因（TraceGraph DAG + AttributionReport direct/marginal），当前模型假设"一个输出 stat = 一条聚合链"。双 pass 打破该假设：**同一个 SourceId 在不同 pass 内贡献不同**（per-hand 词条只进对应手、`on Critical Hit` 条件词条只进暴击 pass），合并节点是非平凡多入度算子（部分非线性）。若无结构化设计，归因会退化为"只能解释合并前某一条腿"。

### 1.2 模型设计：pass = TraceGraph 子图（带 PassId 标记的节点分区）

**裁决建议：单一 TraceGraph + 节点级 PassId 标记**（备选"每 pass 独立 graph + 顶层 merge graph"否决：`source_ancestors` 跨图遍历要改所有消费方，且 marginal 重算闭包需持多图状态）。

```rust
// pobr-core/src/trace.rs 扩展
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandTag { Single, MainHand, OffHand }     // Single = 法术/非攻击（PoB2 passList 的 "Skill" pass）

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CritTag { Blended, Crit, NonCrit }        // Blended = 不在暴击双 pass 内的节点（防御等）

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId { pub hand: HandTag, pub crit: CritTag }

pub struct TraceNode {
    // ……现有字段不变……
    pub pass: Option<PassId>,   // None = pass 无关节点（输入、全局聚合、防御）
}
```

- 写入方式：`TraceGraph` 增 `begin_pass(PassId)/end_pass()`（内部栈，当前 pass 自动盖戳到 `add_node`/`add_source_node`）。pass 管线代码不需逐节点传参，包一层 scope 即可。
- **子图定义**：pass P 的子图 = `{n | n.pass == Some(P)}` ∪ 其引用的 pass-无关祖先。同一 SourceId 的 Input 节点**允许在多个 pass 内各出现一次**（每 pass 的 `sum_traced` 各自落 Input 节点）——这是有意的：同一来源在不同 pass 的贡献值本就不同，归因按 pass 分桶后再加权合并。
- `source_ancestors` 行为不变（合并节点的入边自然跨 pass，遍历会收齐两腿的来源）。

### 1.3 combineStat = 合并节点（带权重的新 TraceOperation）

```rust
// trace.rs
pub enum CombineMode { Or, Add, Average, Dps { double_hits: bool }, Crit { double_hits: bool },
                       HarmonicMean, Chance, ChanceAilment, CritBlend /* hitAvg×(1-c)+critAvg×c */ }

pub enum TraceOperation {
    // ……现有变体不变……
    Combine { mode: CombineMode, weights: Vec<f64> },  // weights[i] 对应第 i 条入边（与 add_edge 顺序一致）
}
```

**weights = 该合并算子对每条入腿的线性化系数**（direct 归因口径下"来源经腿 i 的贡献 × weights[i]"）。各模式的权重定义（vendor 公式 → 权重）：

| mode | PoB2 公式（CalcOffence.lua:2453-2538 / :4395） | weights(MH, OH) |
|---|---|---|
| OR | `MH or OH`（非 bothWeaponAttack 必走此分支） | 存在腿 1.0，另一腿 0 |
| ADD | `MH + OH` | (1, 1) |
| AVERAGE | `(MH + OH) / 2` | (0.5, 0.5) |
| DPS | `MH + OH`，非 `doubleHitsWhenDualWielding` 再 `/2`（:2534-2538） | doubleHits→(1,1)；否则 (0.5,0.5) |
| CRIT | doubleHits：`MH + OH − MH×OH/100`；否则 `(MH+OH)/2` | doubleHits→偏导线性化 `(1−OH/100, 1−MH/100)`；否则 (0.5,0.5) |
| HARMONICMEAN | `2/(1/MH + 1/OH)`（Speed 用） | 偏导线性化 `(2·OH²/(MH+OH)², 2·MH²/(MH+OH)²)` |
| CHANCE | 按 `chance×HitChance` 占比加权（:2475-2480） | (mainPortion, offPortion)——portion 当常数 |
| CHANCE_AILMENT | maxInstance×stacks 占比（:2500-2505） | (maxInstanceStacks 归到大腿, 1−stacks 归到小腿) |
| CritBlend | `hitAvg×(1−c) + critAvg×c`（:4395） | (1−c, c)，c=CritChance；c 自身的来源归因走 c 节点的入边 |

**归因语义裁决**：

1. **direct 口径**：合并节点处按 weights 把每腿内来源贡献线性缩放后相加。非线性模式（CRIT-doubleHits / HARMONICMEAN / CHANCE）的 weights 是当前点的一阶线性化——direct 本来就是"按公式形状摊"的近似口径，可接受；精确语义由 marginal 兜底。
2. **marginal 口径**：**零修改自动正确**——`attribution::attribute()` 的 filtered-recompute 闭包重跑整条 2×2 管线（移除来源 → 四个 pass 全部重算 → combine 重算），任何非线性都被如实捕获。代价是重算量 ×4，进 bench 门禁（§6）。
3. **per-pass 查询**（新增能力，PoBR 增量卖点）：`AttributionRequest` 增 `pub pass_filter: Option<PassId>`，direct 归因时只累计 `node.pass == filter` 的 Input 节点——回答"这件副手武器贡献了多少 OffHand DPS"。
4. **interaction**：维持现定义（marginal − direct 残差），双 pass 下 interaction 会变大（条件翻转 × 合并非线性），报告口径不变。

### 1.4 OutputTable / API 变化

- `OutputTable` 增 per-hand 子结果：`pub main_hand: Option<HandOutput>`、`pub off_hand: Option<HandOutput>`（`HandOutput` 含 accuracy/hit_chance/crit_chance/speed/per-type avg/DPS 等 combineStat 入参字段；置于 `calc/output.rs`）。顶层既有字段语义改为"combineStat 之后"（单手 build 数值不变——OR 模式直通）。
- `TracedMinimalOutput::node_for` 不变；新增 `node_for_pass(stat, PassId)`。
- golden/attribution fixture 将变化：**重建 baseline 走独立 commit**（roadmap §1.1 纪律，见 §6）。

### 1.5 评审 checklist（T2 合并前置）

- [ ] 单手 + 无暴击条件词条的 build：双 pass 路径输出与现单 pass **逐值相等**（等价性测试，见 W-B2/B3 测试计划）。
- [ ] direct 权重表与 vendor 公式逐模式对得上（每模式一个单测）。
- [ ] marginal 在 doubleHits/CRIT 非线性样例上 ≠ direct 且符合手算。
- [ ] `source_ancestors`/既有 attribution 测试（`tests/attribution.rs` 8.3K）零回归。
- [ ] bench：traced 路径 ×4 重算在预算内（§6）。
- [ ] 评审人：主工作区 owner；RFC 修订直接改本文 §1 并在 commit message 标 `rfc(m4-attribution)`。

---

## 2. 工作项分解

> 每项 = {目标 / 涉及文件 / vendor 参照 / 数据-schema / 测试计划 / 预估规模}。规模单位：人日（含测试）。
> 编号前缀 = 所属 track（§3）。**全部 calc 改动遵守搬迁不变式与"修复附 PoB2 一手依据"纪律。**

### T1 — mod 内核扩展

#### W-A1 ModFlags 5→30 位（12-G2 / 10-G5）

**目标**：`ModFlags` 对齐 PoB2 全位表，位值**逐位等于** `Data/Global.lua:222-259`（便于对拍调试）；武器类型位由 `weapon_types.json` 派生；feature-gated 双跑后切换。

**位分配表**（u64，直接采用 PoB2 数值；现 5 位中 MELEE/PROJECTILE/AREA 位值要**搬家**——这是必须双跑的原因）：

| 位 | 常量 | 值 | | 位 | 常量 | 值 |
|---|---|---|---|---|---|---|
| 0 | ATTACK | 0x1 | | 16 | AXE | 0x10000 |
| 1 | SPELL | 0x2 | | 17 | BOW | 0x20000 |
| 2 | HIT | 0x4 | | 18 | CLAW | 0x40000 |
| 3 | DOT | 0x8 | | 19 | DAGGER | 0x80000 |
| 4 | CAST | 0x10 | | 20 | MACE | 0x100000 |
| 5 | THORNS | 0x20 | | 21 | STAFF | 0x200000 |
| 8 | MELEE | 0x100 | | 22 | SWORD | 0x400000 |
| 9 | AREA | 0x200 | | 23 | WAND | 0x800000 |
| 10 | PROJECTILE | 0x400 | | 24 | UNARMED | 0x1000000 |
| 11 | AILMENT | 0x800 | | 25 | FISHING | 0x2000000 |
| 12 | MELEE_HIT | 0x1000 | | 26 | CROSSBOW | 0x4000000 |
| 13 | WEAPON | 0x2000 | | 27 | FLAIL | 0x8000000 |
| 32 | WEAPON_MELEE | 0x100000000 | | 28 | SPEAR | 0x10000000 |
| 33 | WEAPON_RANGED | 0x200000000 | | 29 | WARSTAFF | 0x20000000 |
| 34 | WEAPON_1H | 0x400000000 | | 30 | TALISMAN | 0x40000000 |
| 35 | WEAPON_2H | 0x800000000 | | mask | SOURCE_MASK | 0x600 |
| | | | | mask | WEAPON_MASK | 0xF5FFF0000 |

**派生规则**（对应 `CalcActiveSkill.lua:274-309 getWeaponFlags`）：`weapon_types.json` 的 `flag` 字符串 → 武器类型位（名称→位的映射表留 Rust，P1 的 L4 刹车：位枚举是框架语义）；`one_hand`→WEAPON_1H/2H、`melee`→WEAPON_MELEE/RANGED + MELEE_HIT；任何武器 → WEAPON。`countsAsAll1H`/`asThoughUsing` 本阶段不做（无消费 build，登记 M5+）。

**切换步骤**（feature-gated 双跑）：
1. commit-1：新位表以 `#[cfg(feature = "modflags-pob2")]` 双套常量落 `pobr-data/src/modifier.rs`；同 crate 加位值断言单测（逐常量 == Global.lua 值）。
2. commit-2：派生侧——`pobr-build` 在构造 `WeaponContribution` 时由 `weapon_types.json` 算出 `flags: ModFlags`（新字段）；mod_parser 武器后缀（`mod_parser.rs:1016-1027` 一带）在 feature 下**同时**产出武器位（保留 condition 字符串双写，消费两通道并存）。
3. commit-3：双跑脚本 `devs/scripts/modflags-dualrun.sh`——`cargo test --workspace` 与 `cargo test --workspace --features modflags-pob2` 各跑一遍 ninja_parity + golden，diff 报告（应为零：此时尚无人按新位消费）。
4. T2 的 hand_pass 落地后（per-hand cfg 开始消费武器位），再跑双跑确认 parity 只升不降 → 翻默认 feature → 删旧 5 位常量与 `UsingMace` 类 condition 近似路径（**退役放 M4 末，单独 commit**）。
5. 检查项：grep golden fixture / build code XML 是否有序列化的 flags bits 落盘（`ModFlags` 是 serde 透明 u64）——若有，fixture 重生与位值切换同 commit。

文件：`pobr-data/src/modifier.rs`（独占）、`pobr-build/src/calc_orchestrator.rs` 武器段（L1096-1250 区域）、`pobr-core/src/mod_parser.rs`（武器短语段）。
测试：位值断言；`is_subset_of` 在新位宽下的既有语义测试搬迁；`with Maces`/`with One Handed Melee Weapons`/`Unarmed` 三条词条的解析→匹配端到端。
规模：3 人日。

#### W-A2 mod_db 写侧原语 ReplaceMod / ConvertMod / ScaleAddMod（10-G6）

**目标**：补三个写侧原语 + 统一 ScaleAddMod 取整规则。弩 reload 的 `Multiplier:BoltsReloadedPastSixSeconds` 回写（`CalcOffence.lua:2890-2894` 用 `ReplaceMod`）和宝石等级缩放都依赖它。

**vendor 参照**：`ModStore.lua:45-81`（ScaleAddMod 取整：`highPrecisionMods[name][type]` 走 `floor(v×scale×10^p)/10^p`、`+level` 类 floor、默认 `m_modf(round(·,2))`）；`:114-127`（ReplaceMod = 按 name+type+source 匹配替换否则 append；ConvertMod = 改 name 后 append）。

**API**（`pobr-core/src/mod_db.rs`）：
```rust
pub fn replace_mod(&mut self, m: Modifier);                       // 同 name+type+source 替换，否则 add
pub fn convert_mod(&mut self, from: &ModName, to: &ModName);      // 改名搬桶
pub fn scale_add_mod(&mut self, m: Modifier, scale: f64, precision: &HighPrecisionRules);
```
`HighPrecisionRules` 来自 `overlay/high_precision_mods.json`（新表，extract-lua 抽 `Data.lua` `highPrecisionMods`/`defaultHighPrecision`；**总架构评审裁决：M0 规划的该表实查未交付，W-A2 是其唯一正式落表点，M5c-E2 / M6 只消费不自建**），经 RuleSet 注入；fallback = 默认 round(·,2) 截整。**注意**：10-G6 还指出 more 聚合的 highPrecision 例外（`ModList.lua:131-144`）至今未补——本项一并修（`round_more` 增例外分支），**行为修复独立 commit + oracle 中间值依据**。

文件：`pobr-core/src/mod_db.rs`（独占）、`pobr-data/src/catalog/`（HighPrecisionDef schema）、`pobr-gamedata`（loader 域 + RuleSet 字段）、`tools/sync-pob-catalog`（extract 段）。
测试：三原语单测（replace 命中/未命中、convert 跨桶）；ScaleAddMod 取整 oracle 抽样 ≥10 条（pob2-oracle 同输入对拍）；round_more 例外回归。
规模：3 人日。

#### W-A3 EvalMod 第二批：PerStat 读 output + globalLimit（10-G3 的 M4 份额）

**目标**：`ModTag` 增 `PerStat`（与 Multiplier 拆开：从 actor **output** 读已算出 stat，而非编排层预灌 cfg.multipliers）与 `globalLimit/globalLimitKey` 跨 mod 累计限幅——后者是 chance-to-deal-Double-Damage（DOUBLED form）机制的依赖，T3 的 W-C1 消费它。

**vendor 参照**：`ModStore.lua:325-885` EvalMod（PerStat 分支 + 末尾 globalLimit 累计）；M3 已做 actor/limitActor 系，本项只做这两种。

**设计**：求值上下文升级——`Modifier::effective_number` 的入参从 `&CalcConfig` 升为轻量 `EvalContext<'a> { cfg: &'a CalcConfig, stat_lookup: Option<&'a dyn Fn(&str) -> Option<f64>> }`（M3 若已落 actor 上下文则在其上加字段，**以 M3 合并后的实际签名为准**）。globalLimit 在聚合循环（`mod_db.rs` sum/more 内）按 `globalLimitKey` 分桶累计、超限截断；traced 路径同步（限幅作为 Clamp 节点入图）。

文件：`pobr-core/src/modifier.rs`、`mod_db.rs`（与 W-A2 同 track 串行避免冲突）、`config.rs`。
测试：`per 100 maximum Life` 类 PerStat 词条端到端；globalLimit 两 mod 累计超限截断单测 + traced 版。
规模：2.5 人日。

### T2 — 双 pass 与归因（RFC 评审通过后动工实现，骨架可先行）

#### W-B1 TraceGraph/attribution 扩展（落地 §1 RFC）

**目标**：`PassId`/`begin_pass`/`Combine{mode,weights}`/`AttributionRequest.pass_filter` 四件套 + direct 权重摊销实现。
文件：`pobr-core/src/trace.rs`、`attribution.rs`（独占）。
测试：每 CombineMode 一个权重单测（手算值）；pass_filter 过滤正确性；既有 `tests/attribution.rs`、`tests/trace.rs` 零回归。
规模：3 人日。**前置：§1.5 评审通过。**

#### W-B2 `calc/hand_pass.rs`：MH/OH 双 pass + combineStat（12-G1）

**目标**：攻击技能按主/副手各跑一遍进攻管线，按 combineStat 表合并。

**vendor 参照**：`CalcOffence.lua:2369-2449`（passList：weapon1Attack/weapon2Attack 各一 pass；unarmed 用 `data.unarmedWeaponData[classId]`；`setOffHandPhysicalMin/Max`、`skillData.attackTime` 覆盖 source——pobr 的 `non_weapon_attack_contribution` 已等价实现单手版，搬进 OffHand pass）；`:2453-2538`（combineStat 8 模式，本蓝图 §1.3 表）；`:4554-4705`（末端合并大表——哪个 stat 用哪个 mode，**照抄成 Rust 静态表** `COMBINE_TABLE: &[(&str, CombineMode)]`，机制逻辑跨版本稳定，按 P2 判据留框架）。

**结构**：
```rust
// pobr-core/src/calc/hand_pass.rs（新文件，~350 行）
pub struct HandSource {           // 由 pobr-build 构造（接口契约，见 §4）
    pub label: HandTag,
    pub weapon: WeaponBase,       // phys min/max、attack_rate、crit_chance、flags: ModFlags、局部乘区已折入
    pub cfg_overrides: HandCfg,   // per-hand flags（武器位）+ 条件（MainHandAttack/OffHandAttack）
}
pub fn run_hand_passes(db, enemy_db, cfg, passes: &[HandSource], input) -> CombinedOutput;
```
- per-hand cfg：`cfg.flags |= hand.weapon.flags`（W-A1 位）+ `conditions["MainHandAttack"/"OffHandAttack"]=true`（PoB2 weapon1Cfg/weapon2Cfg 等价）。
- 非攻击技能 = 单 `HandTag::Single` pass，**走同一入口**（消灭特例）；`skillFlags.bothWeaponAttack` 为假时全部 stat 走 OR 模式直通——这是单手 build 逐值不变的保证。
- `doubleHitsWhenDualWielding`：读技能数据布尔（schema 见 W-D1 顺带扩列）。
- 编排侧：`calc_orchestrator.rs` `weapon_contribution`（L1096）改产 `Vec<HandSource>`（Weapon2 为武器时产第二份；盾/无副手不产）。

测试：① 等价性——现有全部进攻测试在"单手输入"下输出逐值不变（接到 `calculate_minimal_vs_enemy` 内部改走 run_hand_passes 后跑全量）；② 双持 fixture：构造 MH 锤 + OH 剑、各带 per-hand 词条（`with Maces` 只进 MH）的合成 build，手算对拍；③ ninja 集内双持 build 的 parity 提升记录；④ doubleHits 技能（如双持类技能）DPS = MH+OH 不除 2。
规模：5 人日。**前置：W-B1、W-A1 commit-2。**

#### W-B3 `calc/crit_pass.rs`：暴击/非暴击双 pass（12-G3）

**目标**：伤害主体按 `CriticalStrike` 条件分别聚合、分别过敌方减伤，末端 CritBlend 合并；替换现 `total_hit_avg = non_crit_hit_avg × crit.effect` 单因子（`offence.rs:293`）。

**vendor 参照**：`CalcOffence.lua:3978-3980`（`cfg.skillCond["CriticalStrike"]=(pass==1)`）、`:4028-4032`（pass1 allMult ×CritMultiplier）、`:4047-4057`（`Stored<Type>CritAvg/HitAvg/CombinedAvg` 分存——ailment magnitude 的输入，**本项必须落这组字段**否则 ailment 链断）、`:4395`（CritBlend）。

**结构**：`crit_pass.rs` 提供 `run_crit_passes(db, enemy_db, cfg, base_components, all_mult) -> CritPassOutput { crit: PerTypeHit, non_crit: PerTypeHit, stored: StoredHitAvgs }`，在每个 hand pass 内调用（2×2 嵌套：hand 外层、crit 内层，与 PoB2 同构）。`crit.rs` 的 `resolve_crit` 不动（它算几率与倍率，本项只消费）。leech 分 pass 累积（`:3970+`）本阶段**不做**（pobr 无 leech 管线，登记缺口防失忆）。

测试：① 等价性——无 `CriticalStrike` 条件词条时输出与旧单因子逐值相等（数学恒等：blend(c, x×m, x) = x×(1+(m−1)c) = x×crit.effect）；② 带 `increased Damage on Critical Hit` 合成用例：词条只放大 crit 腿；③ Stored* 字段与 oracle 中间值对拍 ≥3 build。
规模：3 人日。**前置：W-B1；与 W-B2 在同一 track 串行（共改 offence.rs）。**

### T3 — 乘区与 DPS 末端（独立模块先行，最后接线）

#### W-C1 ScaledDamageEffect：Double/Triple Damage 乘区（12-G4）

**目标**：新增独立计算单元，产出 allMult 因子。

**vendor 参照**：`CalcOffence.lua:3840`（`ScaledDamageEffect = 1` 初始化——**只有 DD/TD 乘它**，已亲验）；`:3842-3861`：
- `TripleDamageChanceOnCrit` cap100；`TripleDamageChance = min(Sum + enemy SelfTriple(仅 effective) + OnCrit×CritChance/100, 100)`；`TripleDamageEffect = 2×chance/100`。
- Double 同构；Intimidate：`Condition:WarcryMaxHit` 时 DD=100，否则 `+IntimidatingUpTimeRatio`（warcry 未实现时该输入为 None，跳过）。
- **Triple 抵扣 Double**：`DD = max(DD − TD×DD/100, 0)`。
- `ScaledDamageEffect ×= (1 + DDEffect + TDEffect)`。
- allMult 全清单（`:4023-4025`）：`ScaledDamageEffect × FistOfWarDamageEffect × AncestralCallDamageEffect × AncestralEmpowermentDamageEffect × AncestralEmpowermentCombinedDamageEffect × OffensiveWarcryEffect(或 Max 变体)`——**M4 范围 = ScaledDamageEffect；其余五因子建模为 `AllMultExtras` 结构、默认 1.0 占位**（warcry/ancestral 是 M5+ 机制，留接口防返工）。
- `chance to deal Double Damage` 的 DOUBLED form 词条带 globalLimit（依赖 W-A3）。

文件：`pobr-core/src/calc/scaled_damage.rs`（新，~150 行）；接线点 = W-B3 的 `all_mult` 入参。
测试：DD/TD 去重手算单测；OnCrit 折算（与 CritChance 联动）；`SelfDoubleDamageChance` 仅 effective 口径；oracle 对拍 1 条带 DD 词条 build。
规模：2 人日。**接线前置 W-B3；模块本体无前置。**

#### W-C2 LuckyHits 掷骰平均（12-G12）

**vendor 参照**：`CalcOffence.lua:4036-4046`——lucky 几率来源：`LuckyHits` flag / `CritLucky`(仅 crit pass) / `LightningNoCritLucky`(仅 non-crit 且 Lightning) / `ElementalLuckHits`(三元素) / `Sum(BASE, <Type>LuckyHitsChance, LuckyHitsChance)` cap100；`avg = notLucky×(1−p) + (min/3 + 2max/3)×p`。

**实现**：`DamageComponent::avg()` 增带参版本 `avg_with_lucky(p: f64)`（damage.rs，旧 `avg()` 保留 = p=0）；lucky 几率解析按 (pass, damage_type) 在 crit_pass 内求。注意与 `crit.rs` 的 CritChanceLucky 是**两个机制**，不要合并。
文件：`pobr-core/src/calc/damage.rs`（avg 函数族）+ crit_pass 内消费。
测试：(min,max)=(10,100) 时 lucky avg=70 vs 55；CritLucky 只影响 crit 腿。
规模：1 人日。

#### W-C3 canDeal / DealNo\<Type\> 门控（12-G8，roadmap 点名）

**vendor 参照**：`CalcOffence.lua:2226-2230`（`canDeal[type] = not Flag("DealNo"..type, "DealNoDamage")`）；消费三处 `:3989/4793/5451`（hit / ailment / DoT）。**顺序关键**：转换先发生，清零的是转换后残留。

**实现**：`damage.rs` 转换链末尾增 `apply_can_deal(components, db, cfg)`（flag 查询 `DealNoPhysical` 等 5+1 名）；技能 DoT（W-D1）与 ailment 侧同函数复用。
测试：Avatar of Fire 形态合成用例（phys→fire 转换 + DealNoPhysical：残留 phys 清零、已转 fire 保留）。
规模：1 人日。

#### W-C4 dpsMultiplier / quantityMultiplier 接入 TotalDPS（12-G6）

**vendor 参照**：`CalcOffence.lua:3128-3130`（`quantityMultiplier = max(Sum(BASE, skillCfg, "QuantityMultiplier"), 1)`）、`:3863`（`skillData.dpsMultiplier ×= calcLib.mod(skillModList, skillCfg, "DPS")`——"DPS" ModName 的 inc/more 在此消费）、`:4407`（`TotalDPS = AverageDamage × (HitSpeed or Speed) × dpsMultiplier × quantityMultiplier`）。

**实现**：
- 数据：`SkillStatSetDef`/`SkillLevelDef` 增 `dps_multiplier: Option<f64>`（catalog schema + adapter 透传；vendor skillData 字段，多次打击/分裂箭类技能携带）。
- calc：`offence.rs` DPS 末端（现 L297）乘两因子；`quantityMultiplier` 经 ModDb（`QuantityMultiplier` BASE）聚合，floor 1.0。
- 技能 DoT 末端同乘（W-D1 消费同一对值，`:5931`）。

文件：`pobr-data/src/catalog/skills.rs`、`tools/pobr-data-adapter`、`pobr-core/src/calc/offence.rs` DPS 末端一行带、`pobr-build/src/calc_orchestrator.rs` 透传。
测试：带 dpsMultiplier 的真实技能（从 granted_effect 数据选一个非 1 值）golden；`QuantityMultiplier` 词条端到端。
规模：1.5 人日。**审计标注"改动小、回报直接，可优先做"——T3 内最先做。**

### T4 — 技能 DoT 与弩（数据 schema + 消费）

#### W-D1 技能 DoT：dot 基值族消费 + 合并 DPS 族（12-G5）

**目标**：DoT 主体技能（毒雨/Decay/点燃地面类）DPS 从 0 → 对齐 PoB2。

**vendor 参照**：`CalcOffence.lua:5831-5945`（逐行亲验）：
- dotCfg：`flags = ModFlag.Dot | skillCfg.flags`，再按 `dotIsArea/dotIsProjectile/dotIsSpell/dotIsAttack/dotIsHit` 五布尔**剥**对应位（无该布尔则去掉 Area/Projectile/Spell/Attack/Hit 位）；`keywordFlags &= !KeywordFlag.Hit`。→ **依赖 W-A1 的 Hit/Dot 位**。
- 逐类型：`baseVal = skillData[type.."Dot"]`（canDeal 门控，W-C3）；`total = baseVal × (1+inc/100) × more × (1 + (Override(DotMultiplier) or Sum(DotMultiplier)+Sum(<type>DotMultiplier))/100) × aura × effMult`；effMult = 敌方 `DamageTaken/DamageTakenOverTime/<type>DamageTaken(OverTime)/ElementalDamageTaken` + 抗性（物理走 `EnemyPhysicalDamageReductionCap`）。
- `TotalDotInstance` 累计 clamp `DotDpsCap`（读 `cfg.constants`，**已在 game_constants.json**）。
- `DotCanStack` flag：`TotalDot = min(instance × speed × Duration × dpsMultiplier × quantityMultiplier, DotDpsCap)`（`:5931`）；速率按 keywordFlags Mine/Trap 换 `MineLayingSpeed/TrapThrowingSpeed`——pobr 无图腾/陷阱吞吐（12-G11，M4 不做），**该分支留 match 臂 + None 时退 Speed，登记注释**。
- 末端合并族（`:6093-6234`）：`WithDotDPS`、`TotalDotDPS = Σ(dot+poison+caustic+ignite+burning+bleed+corrupting+decay)` clamp DotDpsCap、`CombinedDPS`。pobr 已有异常 DoT 各值（ailment.rs/perform.rs），本项把技能 DoT 并入求和 + 落 OutputTable 新字段。

**数据侧**：`granted_effect_stat_sets.json` 已含 `base_<type>_damage_to_deal_per_minute` 类 stat（亲验首条即是 `base_fire_damage_to_deal_per_minute`）——**dot 基值数据已在库**（roadmap："数据列 M1 已入库"），消费 = stat→skillData 映射：`base_X_damage_to_deal_per_minute / 60 → XDot`（对应 PoB2 SkillStatMap 同名映射）。**总架构评审勘误**：M1 已完成 P5 切换并删除 `skill_stat_map.rs`（751 行启发式，M1-T2.4），M4 时点该文件不存在——映射走 `overlay/skill_stat_map.json` + `rules/stat_map_engine`；954 条全量抽取应已含上述同名条目（SkillStatMap.lua 原生条目），实查缺失时按 overlay 通道补条目/per-statset 覆盖（数据 commit + regen），**禁止恢复任何 Rust 启发式映射**。`dotIs*` 五布尔：vendor skillData 字段，.dat 不直给——**经 extract-lua 抽技能 skillData 布尔进 overlay `skill_overrides.json`（M0 已有该通道）**；抽不到的技能默认全 false（保 flag 不剥，偏保守）。`doubleHitsWhenDualWielding`（W-B2 要用）同通道顺带抽。

文件：`pobr-core/src/calc/skill_dot.rs`（新，~250 行）、`perform.rs` fill 段新增 `fill_skill_dot`（函数级新增，见 §3 共享文件规则）、`calc/output.rs`（字段区块）、`data/<ver>/overlay/skill_stat_map.json`（如需补条目，走 extract/override 通道再生）、`tools/sync-pob-catalog`（dotIs* 抽取段）。
测试：选 1 个纯 DoT 技能（如 Essence Drain 类）端到端 golden + oracle 对拍 `TotalDot` 中间值；DotMultiplier Override 优先级单测；TotalDotDPS clamp 单测；dotIs* 剥 flag 后 `increased Area Damage` 不再作用于非 area DoT 的回归。
规模：4 人日。**前置：W-A1（Dot/Hit 位）；W-C3/C4 提供 canDeal 与 dpsMultiplier（接口先 stub）。**

#### W-D2 弩 reload 模型（16-G4 消费侧 + 12-G10）

**目标**：`base_items.reload_time_ms`/`bolt_count` 入库 + speed 链 reload 循环平均。

**vendor 参照**：`CalcOffence.lua:2867-2897`（亲验）：`FiringRate = Speed`；`EffectiveBoltCount = boltCount / (1 − ChanceToNotConsumeAmmo/100)`（≥100 → ∞ 即不进 reload）；`TotalFiringTime = EffectiveBoltCount / FiringRate`；`EffectiveReloadTime = reloadTime × (1 − InstantReloadChance/100)`；`Speed = EffectiveBoltCount / (TotalFiringTime + EffectiveReloadTime)`。`Multiplier:BoltsReloadedPastSix/EightSeconds` 回写用 `ReplaceMod`（依赖 W-A2）。

**数据侧**：现 `base_items.json` 弩条目 weapon 段**无 reload 字段**（亲验 Makeshift Crossbow 仅 phys/speed/crit/range）——adapter 需从 .dat（WeaponTypes 表 ReloadTime 列）补列 `weapon.reload_time_ms`；`bolt_count` 在 PoB2 是 skillData（技能携带，非武器），经 W-D1 的 skill_overrides 通道抽。**schema 变更走 base 再生管线 + regen-check**。

文件：`pobr-data/src/catalog/items.rs`（weapon 段字段）、`tools/pobr-data-adapter`、`pobr-core/src/calc/skill_use_time.rs`（reload 折算函数，在 server-tick cap 之后应用——vendor 顺序 `:2864-2867` 先 tick cap 后 reload）、orchestrator 透传 reload/bolt。
测试：手算用例（boltCount=5, reload=0.8s, FiringRate=3）；ChanceToNotConsumeAmmo≥100 退化为 FiringRate；弩 build fixture（验收门禁点名）——ninja 集选/构造 1 个弩 build 入 golden。
规模：3 人日。**前置：W-A2（ReplaceMod，仅 Multiplier 回写部分；主模型无前置）。**

### T5 — 触发数据接线

#### W-E1 `trigger_configs.json` 61 项：schema + extract + 装载（14-G1）

**目标**：CoC/CWDT/unique 触发关系从"完全无识别入口"变为数据驱动识别。

**vendor 参照**：`CalcTriggers.lua:882-1418` configTable（61 项，四级 key：技能名→triggeredBy 名→awakened 名→unique 物品名；`:1436` 查表）。亲验条目形态：~90% 是声明性事实（`triggerName`、`triggerOnUse`、`triggerSkillCond = 技能类型/ModFlag 谓词`、`cooldown` 覆盖、`triggerRateCapOverride`、`globalTrigger`、disable 条件），少数带真逻辑（Mjolner 双源、The Hidden Blade 的 Phasing 门控等）。

**schema**（`pobr-data/src/catalog/triggers.rs`，overlay 表；**受限谓词遵守 20 号 §5 硬边界**——字段引用 + eq/and/or，禁自由表达式）：
```json
{ "key": {"kind": "unique_item|skill|triggered_by", "name": "law of the wilds"},
  "trigger_name": null,
  "trigger_on_use": false,
  "source_skill_cond": { "any_skill_types": ["Melee","Attack"], "all_mod_flags": ["Claw"], "not_skill_types": ["SummonsTotem"] },
  "triggered_skill_cond": null,
  "trigger_chance_stat": null,
  "cooldown_override_s": null,
  "trigger_rate_cap_override": null,
  "global_trigger": false,
  "requires_condition": null,            // 如 "Phasing"（The Hidden Blade）
  "handler_id": null,                    // 真逻辑条目（Mjolner 类）→ 注册表；监控计数 <100
  "verified": false }
```
抽取：extract-lua 新增 `trigger-configs` 段（luajit 执行 configTable，闭包条目无法序列化的字段标 `handler_id` + 人工映射清单；handler 未映射即 check 告警——M0 的 handler 覆盖清单机制复用）。装载：gamedata 新域 + RuleSet 字段；消费在 orchestrator `trigger_modifiers`（L1498）扩为四级 key 匹配（build 数据模型的 gem-link/triggeredBy 关系：从 XML 导入的 socket group 内 support 关系推导——`同组 support 名→triggeredBy key`）。

文件：`pobr-data/src/catalog/triggers.rs`（新）、`tools/sync-pob-catalog`（extract 段）、`pobr-gamedata`（域+RuleSet）、`pobr-build/src/calc_orchestrator.rs` 触发段（L1486-1620 区域，T5 独占）。
测试：schema 往返单测；61 项抽取计数断言（=61，drift 防线）；CoC build fixture 识别出触发关系（trigger 面板不再退化为自施法）。
规模：4 人日。

#### W-E2 源速率改计算后攻速 + 命中/暴击折入（14-G2 / 14-G8）

**目标**：修"堆攻速的 CoC build 源速率不随攻速增长"的定性级错误。

**vendor 参照**：`CalcTriggers.lua:67-87`（`GlobalCache.cachedData[mode][uuid].HitSpeed or Speed`——源技能**完整子计算**的速度）、`:729-770`（`triggerChance ×= sourceHitChance × sourceCritChance`，CoC 核心折减，含双武器独立 roll）、`:431`（dual wield 源 /2）。

**实现（GlobalCache 等价物，最小版）**：orchestrator 对触发组内的**源技能**先跑一次完整 `CalculationSession` 子计算（同一 BuildData，主技能换成源技能），取其 `OutputTable.{action_rate, hit_chance, crit_chance}` 作 `TriggerSourceStats` 注入 `fill_trigger`（替换 `in_group_trigger_source_rate` L1595-1599 的 `1/use_time_s`）。子计算结果按 (build hash, skill id) 缓存于既有 `CalcCache`。CoC 链路 triggerChance 折入 `hit×crit`；**口径走 PoB2（P12）**：能量模型继续 feature-gated 悬空，parity 门禁不碰。
- **源速率注入位置**：`fill_trigger`（`perform.rs:381+`）消费 `TriggerSourceRate` 的现通道保留，值的**生产侧**换为子计算结果——改 orchestrator 不改 perform 接口。

文件：`pobr-build/src/calc_orchestrator.rs` 触发段（与 W-E1 同 track 串行）、`pobr-core/src/calc/trigger.rs`（折减入参扩展）。
测试：CoC fixture（验收门禁点名）：源技能 +100% 攻速词条 → 触发 DPS 同步上升的方向性断言；triggerChance = trigger 几率 × 源 crit 的手算对拍；oracle 对 1 个真实 CoC build 的 EffectiveSourceRate 中间值对拍。
规模：4 人日。**前置：W-E1（识别出触发关系才知道谁是源）。**

### T0 — 横切：bench 门禁（W-F1）

**目标**：双 pass 让进攻热路径计算量近似 ×4（2 hand × 2 crit），R6 风险需量化闸门。

**方案**：
1. 既有 `mod_db_bench` 保持（聚合内核吞吐不得回归——双 pass 不该改聚合本身）。
2. 新增 `crates/pobr-build/benches/perform_bench.rs`：对 ninja 集中 1 个双持攻击 build 跑完整 `calculate_with_data`，criterion 基线。**预算**：M4 结束时 ≤ M4 开始基线的 **2.5×**（双 pass 理论 ×4，但 hand pass 仅攻击技能、crit pass 仅伤害主体段；超预算 = 必须做惰性化：非双持跳过 OffHand pass、无暴击词条时短路——短路本身要有等价性测试）。
3. traced/归因路径单独 bench case（marginal ×4 重算），预算 4×，超出记录不阻塞（归因非热路径）。
4. 落地：基线 commit 在 T2 动工前打（`devs/scripts/` 记录 criterion baseline 流程）；CI 不跑 criterion（时长），门禁为合并前手动跑 + 结果贴 PR——与现 `cargo bench -p pobr-core --bench mod_db_bench` 惯例一致。

规模：1.5 人日（T0 由 T2 实施者兼任，基线先行）。

---

## 3. 并行 track 切分与文件归属

### 3.1 Track 总表（6 条）

| Track | 工作项 | 可动工时点 | 预估 |
|---|---|---|---|
| **T0 门禁** | W-F1 bench 基线 | 立即 | 1.5 日 |
| **T1 mod 内核** | W-A1 → W-A2 → W-A3（track 内串行，共改 mod_db/modifier） | 立即 | 8.5 日 |
| **T2 双 pass/归因** | RFC 评审 → W-B1 → W-B2 → W-B3 | RFC 评审过 + W-A1 commit-2 后 W-B2 | 11 日 |
| **T3 乘区/末端** | W-C4 → W-C1 → W-C2 → W-C3（模块先行，接线最后） | 立即（接线等 T2） | 5.5 日 |
| **T4 DoT/弩** | W-D1 ∥ W-D2（数据侧先行） | 数据侧立即；calc 侧等 W-A1 | 7 日 |
| **T5 触发** | W-E1 → W-E2 | 立即 | 8 日 |

关键串行链：**W-A1(commit-1/2) → W-B2**（per-hand 武器位）与 **W-A1 → W-D1**（Dot/Hit 位）——T1 的 W-A1 是全阶段第一优先级，目标第 1 周内出 commit-2。**RFC 评审与 W-A1 并行进行**。W-A3(globalLimit) → W-C1(DOUBLED 词条)；W-A2(ReplaceMod) → W-D2(Multiplier 回写部分)；W-E1 → W-E2。

### 3.2 文件归属表（独占写；未列文件 = 默认禁改，需要时先在 PR 说明）

| 文件/目录 | 归属 | 说明 |
|---|---|---|
| `pobr-data/src/modifier.rs` | **T1** | ModFlags/KeywordFlags |
| `pobr-core/src/mod_db.rs`、`modifier.rs`、`config.rs` | **T1** | 写侧原语 / EvalMod / EvalContext |
| `pobr-core/src/trace.rs`、`attribution.rs` | **T2** | RFC 落地 |
| `pobr-core/src/calc/hand_pass.rs`、`crit_pass.rs`（新） | **T2** | |
| `pobr-core/src/calc/offence.rs` | **T2** | 双 pass 重构主战场；T3 的 DPS 末端两因子（W-C4 一行带）**由 T2 代为接线**（T3 提供 diff 说明） |
| `pobr-core/src/calc/scaled_damage.rs`（新） | **T3** | |
| `pobr-core/src/calc/damage.rs` | **T3** | avg 函数族 / canDeal |
| `pobr-core/src/calc/skill_dot.rs`（新）、`skill_use_time.rs` | **T4** | DoT 模块 / reload 折算 |
| `pobr-core/src/calc/ailment.rs` | **T4** | 仅 TotalDotDPS 求和处只读消费，若需改先协调 |
| `pobr-core/src/calc/trigger.rs` | **T5** | |
| `pobr-data/src/catalog/skills.rs`、`items.rs` | **T4** | dps_multiplier / reload 字段 |
| `pobr-data/src/catalog/triggers.rs`（新） | **T5** | |
| `tools/pobr-data-adapter` | **T4** | reload 列 + dps_multiplier |
| `tools/sync-pob-catalog` | **T5 主责**；T4 的 dotIs* 抽取段以函数级新增并入，T5 审 | extract-lua 多段 |
| `pobr-gamedata`（loader/RuleSet） | **T5 主责**；T1 的 high_precision 域按"每域独立文件 + RuleSet 一行字段"模式各自加 | 域文件互不相交 |
| `pobr-build/src/calc_orchestrator.rs` | **分区**：武器段（L1096-1250 一带）= T2；触发段（L1486-1620 一带）= T5；技能解析/stat_map 透传 = T4。跨区改动先在 PR 标注 | 2662 行大文件，按函数区块分 |
| `pobr-core/src/calc/perform.rs` | **T2 协调**：fill 段新增函数（`fill_skill_dot` 等）各 track 函数级新增，perform 主序由 T2 统一合 | |
| `pobr-core/src/calc/output.rs` | **按字段区块**：每 track 在文件尾自己的 `// === M4-Tx ===` 区块 append，不动他人区块 | |
| `pobr-core/src/calc/mod.rs`、各 `lib.rs` | 一行级 re-export，谁先合谁加，冲突 trivial | |
| `crates/pobr-build/tests/ninja_parity.rs` baseline 常量 | **只许独立 baseline commit 改**（任何 track 不得顺手改） | §6 |
| `data/4.5.0.3.4/**` | 禁手改；只许 adapter/extract-lua 再生（CI regen-check） | |

### 3.3 track 间接口契约（先冻结签名再各自实现）

1. **`HandSource`**（T2 定义于 hand_pass.rs，pobr-build 构造）：见 W-B2。T2 先落 struct + 单 pass 直通实现（第 1 周内 commit），orchestrator 侧可立刻迁移。
2. **`fn scaled_damage_effect(db, enemy_db, cfg, crit_chance: f64) -> ScaledDamage { effect: f64, double_chance: f64, triple_chance: f64 }`**（T3 提供，T2 在 crit_pass 调用）。T2 在 W-B3 用常量 1.0 stub，T3 合并后替换。
3. **`fn apply_can_deal(&mut Vec<DamageComponent>, db, cfg)`**（T3 提供；T4 的 DoT 与 T2 的 hit 共用）。
4. **`TriggerSourceStats { action_rate, hit_chance, crit_chance }`**（T5 定义；perform `fill_trigger` 入参扩展，T5 自己消费，不跨 track）。
5. **`EvalContext`**（T1 定义）：T1 负责把全仓 `effective_number`/`matches` 调用点机械迁移（一次性 commit，提前广播避免与 T2/T4 的新调用点撞——T2/T4 新代码直接用新签名）。
6. **OutputTable 新字段命名**：per-hand 子表 `main_hand/off_hand`；DoT 族 `skill_dot_instance/skill_total_dot/total_dot_dps/with_dot_dps/combined_dps`；命名一经合入 display_catalog 不再改（ParityStatus 标 computed）。

---

## 4. 门禁与验收

### 4.1 每 track 局部门禁（合并条件）

通用：`cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + **ninja_parity `parity_no_regression` 通过**（防御/进攻命中数 ≥ baseline 常量）。涉及 calc/Modifier/parser 改动必须带集成测试或 golden fixture（CLAUDE.md CI gate）。

| Track | 附加局部门禁 |
|---|---|
| T1 | ModFlags 双跑脚本 diff=0（切换前）；ScaleAddMod oracle 抽样 ≥10 条全中；位值断言单测 |
| T2 | §1.5 RFC checklist 全勾；单手等价性测试（逐值）；attribution 既有测试零回归；perform_bench ≤2.5× 基线 |
| T3 | 每乘区一个手算单测 + ≥1 条 oracle 中间值对拍 |
| T4 | regen-check.sh 通过（schema 变更后 base 再生 byte-stable）；DoT golden + oracle TotalDot 对拍；弩手算用例 |
| T5 | 61 项抽取计数断言；handler_id 条目数计入监控（全阶段 <100 总闸，20 号 §5）；CoC 方向性断言 |

### 4.2 阶段整体验收（roadmap M4 原文，逐条引用）

> "**验收门禁**：进攻 **≥70%@5%**；弩/CoC/双持 fixture；`mod_db_bench` 无回归；ModFlags 双跑 diff 干净后切换。"

操作化：
1. **进攻 ≥70%@5%**：`cargo test -p pobr-build --test ninja_parity -- parity_baseline_report` 输出进攻侧 hit5 占比 ≥70%；防御 ≥85% 不倒退（M3 验收线）。
2. **三类 fixture 入 golden**：弩（W-D2）、CoC（W-E2）、双持异种武器（W-B2）各 ≥1 个 build 进 `golden_regression.rs`/ninja 集，先建 baseline 后入门禁。
3. **mod_db_bench 无回归** + perform_bench ≤2.5×（§2 W-F1）。
4. **ModFlags 切换完成**：默认 feature 翻转 + 旧 condition 近似路径退役 commit 已合并。
5. **baseline 纪律**（roadmap §1.1 原文）："行为修复必须附 PoB2 一手依据，baseline 更新独立 commit 显式审查"——M4 每个把输出改"更对"的工作项（双 pass/乘区/DoT 都属此类）合并时若提升了 hit 数，由合并者打独立 `chore(parity): baseline bump` commit 抬高 `BASELINE_OFF_HIT5` 等常量，PR 描述列明归功的工作项。
6. **双跑纪律**：ModFlags 与（若 T2 选择做惰性短路）pass 短路两处，均"diff 干净才切换"。

---

## 5. 风险与回退（roadmap R# 在本阶段的落点）

| 风险 | 落点 | 缓解 / 回退 |
|---|---|---|
| **R8 双 pass × 归因模型冲突**（PoBR 核心卖点最大模型扩展） | §1 RFC；T2 全部 | RFC 评审是 T2 合并前置（不评审不动 trace.rs）；direct 用线性化权重、marginal 兜底精确语义；回退：`Combine` 节点与 PassId 是纯增量字段，单 pass 路径保留到阶段末（等价性测试就是回退开关的正确性证明） |
| **R6 性能：热路径 ×4** | W-F1 | perform_bench 2.5× 预算闸门；超标做惰性短路（非双持跳 OffHand、无暴击条件词条短路 crit pass）且短路带等价性测试；mod_db 聚合内核禁改动 |
| **R11 零回归 vs 提升的张力** | 全部 | 等价性测试先行（W-B2/B3 的"单手/无条件词条逐值不变"）；行为修复独立 commit + oracle 中间值；baseline bump 独立 commit |
| ModFlags 位值搬家破坏隐式依赖 | W-A1 | 双跑 diff=0 才切换；fixture 序列化位检查（W-A1 步骤 5）；回退 = feature 不翻默认 |
| trigger 谓词 DSL 膨胀（R1 余波） | W-E1 | 受限谓词三字段封顶（any/all/not）；扩能力需 ≥20 条目受益（20 号 §5 闸门）；不满足→handler_id，计数监控 <100 |
| dotIs*/bolt_count 抽取不到（vendor skillData 覆盖不全） | W-D1/D2 | 默认保守值（dotIs* 全 false 不剥 flag；bolt_count 缺省按技能基础值 5/PoB2 默认）+ `verified:false` 元数据，parity 报告单列 |
| 触发子计算递归/循环（源技能又是触发技能） | W-E2 | 子计算 env 强制 `trigger 关系剥离`（一层深度），循环检测直接退回基础 use_time 并 warn |
| 多 agent 合并冲突 | §3.2 | 文件归属表 + orchestrator/perform/output 三个共享文件的函数级/区块级规则；T2 是 offence.rs 唯一写者 |

---

## 6. 建议实施顺序（甘特概要）

```
周1   T1:W-A1(commit1-3)        T0:bench基线   T2:RFC评审+W-B1   T3:W-C4   T4:数据侧(schema+adapter)   T5:W-E1 schema+extract
周2   T1:W-A2→W-A3              T2:W-B2        T3:W-C1/C2        T4:W-D2   T5:W-E1 装载+识别
周3   T1:双跑→切换准备           T2:W-B3        T3:W-C3+接线交接   T4:W-D1 calc   T5:W-E2
周4   集成：T2 合 T3 乘区/T4 DoT 末端 → ModFlags 翻默认 → 三类 fixture 入 golden → baseline bump → 验收报告
```

## 7. 实施前仍需裁决的问题（open questions）

1. **EvalContext 签名与 M3 的衔接**：M3（actor/limitActor tag）若已改过求值上下文，W-A3 以其落地形态为准——动工前 T1 与 M3 实施者对齐一次签名，避免二次迁移。
2. **OutputTable per-hand 子表 vs 扁平 `MainHand.X` 键**：本蓝图选子表（强类型），但 display_catalog/CalcSections 对接（M5）可能更适合扁平键——RFC 评审时一并裁决。
3. **dotIs* 与 bolt_count 的权威数据源**：extract-lua 抽 vendor skillData 是 L2 通道；若 .dat 侧（GrantedEffectsPerLevel 列）能直给则应走 base 通道——T4 动工时先做 30 分钟数据勘察再定通道，两通道 schema 相同不影响消费侧。
