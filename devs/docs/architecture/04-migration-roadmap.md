# 迁移路线图

---

## 0. 路线原则

计算核心是 PoBR 的主线。Build Code、复制导入、自定义物品、多语言和 GUI 都围绕核心计算提供输入、验证或展示。

优先级：

1. 稳定数据 ID 与 modifier 语义。
2. 正确实现 ModDB 聚合。
3. 建立 PoB 全量 output/display stat catalog 与 parity matrix。
4. 建立可追踪计算模型：SourceId、TraceGraph、BreakdownTree、AttributionReport。
5. 建立最小可验证计算闭环。
6. 接入天赋、物品、技能等 modifier 来源。
7. 用 PoB 兼容格式和 fixtures 做回归验证。
8. 做快捷导入、custom item、多语言和应用层。

---

## 阶段一：Workspace 与基础类型

**目标**：建立 Rust workspace 和所有计算依赖的基础类型。

**周期**：1-2 周

### 任务清单

- [ ] 配置 workspace：`crates/`, `apps/`, `tools/`, `fixtures/`。
- [ ] `pobr-data`：定义稳定 ID。
  - `StatId`, `ModName`, `SkillId`, `ItemBaseId`, `NodeId`
  - `DamageType`, `ClassId`, `ItemRarity`, `GemLevel`
  - `ModType`, `ModFlags`, `KeywordFlags`, `SkillTypes`
- [ ] `pobr-data`：定义最小计算数据结构。
  - `Item`
  - `Gem`
  - `SkillEffect`
  - `PassiveTreeSpec`
  - `GameData`
- [ ] 建立 fixture 目录和测试工具骨架。
- [ ] 建立 CI 基础命令。

### 里程碑

```bash
cargo fmt --check
cargo test -p pobr-data
```

---

## 阶段二：Modifier 语义与 ModDB

**目标**：实现计算引擎的核心数据结构：Modifier、ModDB、ModList、CalcConfig。

**周期**：3-4 周

### 任务清单

- [ ] `pobr-core::modifier`
  - `Modifier`
  - `ModValue`
  - `ModTag`
  - `Modifier::matches`
- [ ] `pobr-core::mod_db`
  - `add_mod`
  - `sum`
  - `more`
  - `flag`
  - `override_`
  - `list`
- [ ] `pobr-core::mod_list`
  - parent 链或只读 parent 引用模型
  - scale/add/list transformation
- [ ] `pobr-core::config`
  - `CalcConfig`
  - condition/multiplier/scope 查询
- [ ] 单元测试覆盖 modifier 条件、flags、skill type、damage type、multiplier limit。
- [ ] `criterion` benchmark 覆盖 5000 modifiers 下的 hot query。

### 里程碑

```bash
cargo test -p pobr-core mod_db
cargo bench -p pobr-core mod_db
```

验收标准：

- `sum` / `more` / `flag` 行为可独立验证。
- 查询接口无堆分配热路径。
- ModDB 查询模型无需共享可变全局状态。

---

## 阶段三：英文 Modifier Parser 与 Cache

**目标**：将 PoB 英文 modifier 文本转换为结构化 `Modifier`，并生成可回归的解析缓存。

**周期**：3-4 周

### 任务清单

- [ ] `pobr-core::mod_parser`
  - form 匹配
  - mod name 映射
  - tag 匹配
  - pre flag / keyword flag
  - unsupported reason
- [ ] `pobr-core::mod_cache`
  - 生成 cache
  - 读取 cache
  - cache diff 测试
- [ ] `tools/gen-mod-cache`
  - 从 fixture / PoB data 生成 Rust cache 文件
  - CI 中检查 cache 稳定性
- [ ] parser fixtures
  - item mods
  - skill mods
  - passive node mods
  - conditional mods

### 里程碑

```bash
cargo test -p pobr-core mod_parser
cargo run -p gen-mod-cache -- check
```

验收标准：

- 英文 PoB modifier 是第一兼容输入。
- unsupported modifier 返回结构化错误，保留原始文本。
- cache 变化可审查。

---

## 阶段四：PoB 属性目录与贡献追踪基础

**目标**：建立 PoB 全量可计算属性覆盖矩阵，并让所有计算输入都带来源。

**周期**：3-5 周

### 任务清单

- [ ] `tools/sync-pob-catalog`
  - 抽取 `BuildDisplayStats.lua`
  - 抽取 `CalcSections.lua`
  - 抽取 `CalcOffence.lua` / `CalcDefence.lua` / `Calcs.lua` output key
  - 抽取 `Data.lua` constants/power stat list
- [ ] `DisplayStatCatalog`
  - `DisplayStatId`
  - `DisplayStatCategory`
  - `ParityStatus`
  - `StatFormat`
- [ ] `SourceId`
  - equipment slot
  - item affix
  - passive node
  - skill gem
  - support gem
  - config/enemy config
  - game constant
- [ ] `TraceGraph`
  - trace node
  - trace edge
  - operation kind
  - trace mode
- [ ] `AttributionReport`
  - direct contribution
  - marginal contribution
  - interaction bucket

### 里程碑

```bash
cargo run -p sync-pob-catalog -- scan --pob-root ../PathOfBuilding-PoE2 --out fixtures/pob/parity/pob-catalog.json
cargo run -p sync-pob-catalog -- check --pob-root ../PathOfBuilding-PoE2 --catalog fixtures/pob/parity/pob-catalog.json
cargo test -p pobr-core trace
```

验收标准：

- 所有 PoB output/display/comparison key 有 `ParityStatus`。
- 新增/缺失 PoB key 会让 catalog check 失败。
- 已实现字段能从 `SourceId` 追踪到装备、词条、天赋、技能、配置或常量。
- 至少当前最小 DPS、抗性、生命字段支持 source attribution。

---

## 阶段五：最小计算闭环

**目标**：实现从输入 modifier 到输出核心属性的最小可验证计算。

**周期**：4-6 周

### 任务清单

- [ ] `pobr-core::calc::env`
  - `CalculationSession`
  - `Env`
  - `Actor`
  - `OutputTable`
  - `BreakdownTable`
- [ ] 初始化阶段
  - base attributes
  - conditions
  - life/mana
  - resistances
- [ ] 计算边界原语
  - `GameConstants`
  - `StatBoundary`
  - resistance floor / max / hard cap
  - total / final / overcap / missing 输出
- [ ] 技能时间原语
  - skill speed 与 attack/cast speed additive bucket
  - action speed 独立乘区
  - total use time penalty
  - 非 channelling server frame cap
- [ ] 防御最小集
  - armour
  - evasion
  - energy shield
  - resistances
  - hit chance
- [ ] 攻击最小集
  - base damage
  - damage type
  - damage component vector
  - skill gem damage coefficient
  - `INC` / `MORE`
  - attack/cast speed
  - crit chance / multiplier
  - total hit avg
  - DPS
- [ ] `perform` 分阶段编排。
- [ ] snapshot 输出格式。

### 里程碑

```bash
cargo test -p pobr-core calc_minimal
```

验收标准：

- 单技能、无装备、无天赋 fixture 可完整计算。
- 每个输出字段有 breakdown。
- 计算阶段顺序显式，避免隐式重入初始化。

---

## 阶段六：技能、物品、天赋来源接入

**目标**：把主要 modifier 来源接入最小计算闭环。

**周期**：5-7 周

### 任务清单

- [ ] `pobr-core::calc::active_skill`
  - active gem
  - support gem
  - skill type compatibility
  - mana multiplier
  - skill instance mods
- [ ] `pobr-item`
  - raw item text 英文解析
  - `CustomItemDraft -> Item`
  - mod section 保留
  - unsupported lines 保留
- [ ] `pobr-tree`
  - allocated nodes
  - node modifier collection
  - jewel socket gating
  - radius jewel first pass
- [ ] `pobr-core::setup`
  - equipment mods
  - gem mods
  - passive mods
  - config mods
- [ ] fixtures
  - simple weapon attack
  - spell skill
  - rare item with custom explicit mods
  - passive tree allocated nodes

### 里程碑

```bash
cargo test -p pobr-item raw_item
cargo test -p pobr-tree
cargo test -p pobr-core setup_sources
```

验收标准：

- 物品、技能、天赋都能贡献 modifier。
- custom item 参与计算路径与 raw item 一致。
- 数据来源层不持有计算结果。

---

## 阶段七：PoB Build 兼容与 Golden Regression

**目标**：用 PoB 兼容导入/导出作为计算回归输入。

**周期**：3-4 周

### 任务清单

- [ ] `pobr-build::build_code`
  - decode PoB code
  - encode PoB code
  - XML load/save
- [ ] `pobr-build::Build`
  - items
  - skills
  - tree spec
  - config
- [ ] `pobr-build::CalcOrchestrator`
  - 调用 `pobr-core`
  - cache key
  - snapshot 输出
- [ ] golden fixtures
  - 10 个 PoB 导出 build
  - 关键输出字段
  - 容差规则

### 里程碑

```bash
cargo test -p pobr-build pob_code
cargo test -p pobr-build golden
```

验收标准：

- PoB Build Code roundtrip 稳定。
- 完整 Build 可转换为计算输入。
- golden 输出差异可定位到字段和阶段。

---

## 阶段八：更完整的计算域

**目标**：扩展 PoB 关键玩法机制。

**周期**：持续迭代

### 任务清单

- [ ] damage conversion
- [ ] fire/cold/lightning/chaos hit damage
- [ ] maximum resistance / overcap / floor
- [ ] ailment
  - bleed
  - ignite
  - shock
  - chill/freeze
  - poison
- [ ] debuff DoT
  - corrupted blood
  - wither-like stacks
- [ ] reservation
- [ ] charges
- [ ] flask/tincture
- [ ] minion
- [ ] trigger
- [ ] mirage
- [ ] EHP/max hit
- [ ] timeless jewel
- [ ] PoE2 专属机制预留

每个机制都需要：

- 最小单元测试。
- fixture 或 golden case。
- breakdown 输出。
- 性能基准或复杂度说明。

---

## 阶段九：多语言、快捷导入与应用层

**目标**：在计算可信后提供完整用户工作流。

**周期**：4-6 周

### 任务清单

- [ ] `pobr-i18n`
  - `en-US`
  - `zh-TW`
  - fallback
  - key lint
- [ ] `apps/pobr-cli`
  - calculate
  - parse-mod
  - parse-item
  - decode-code
  - encode-code
- [ ] `apps/pobr-wasm`
  - calculation API
  - item parser API
  - i18n text API
- [ ] `apps/pobr-desktop`
  - build workspace
  - paste/import
  - custom item editor
  - language switch
- [ ] `pobr-trade`
  - trade query
  - price fetch

### 里程碑

```bash
cargo run -p pobr-cli -- calculate --build-code fixtures/pob-codes/simple.txt
cargo run -p lint-i18n
```

---

## 阶段十：性能优化

**目标**：针对真实 hot path 优化。

**周期**：持续迭代

### 优化方向

| 优化项 | 策略 | 触发条件 |
|--------|------|----------|
| ModDB 查询 | flags/name/type 分桶 | benchmark 证明查询占主耗时 |
| ModDB 内存布局 | SoA 或 compact arena | 大 build 内存和 cache miss 明显 |
| Parser cache | 预编译映射表 | 解析成为导入瓶颈 |
| 并行计算 | 多技能/多召唤物只读快照 | 单线程完整计算稳定后 |
| Damage conversion | memoization | conversion 重复递归明显 |

### 里程碑

```bash
cargo bench --workspace
```

---

## 风险与缓解方案

| 风险 | 影响 | 缓解方案 |
|------|------|----------|
| Modifier 语义偏差 | 高 | 阶段二独立测试，阶段三 cache diff，阶段七 golden regression |
| PoB 属性覆盖缺口 | 高 | 阶段四 catalog check，所有 PoB output/display key 必须有 ParityStatus |
| 贡献追踪不可信 | 高 | SourceId 贯穿输入来源，TraceGraph 与 AttributionReport 必须有 fixture |
| Env/Actor 共享可变状态设计失控 | 高 | 使用 `CalculationSession` 和显式阶段，禁止全局 mutable table 模型 |
| 计算逻辑覆盖不足 | 高 | 每个机制要求 unit + fixture + breakdown |
| PoB XML 字段不完整 | 中 | Build 兼容阶段保留 unknown section，逐步覆盖 |
| 多语言反向解析复杂 | 中 | 第一阶段多语言用于显示，raw item 多语言导入独立排期 |
| GUI 过早牵引架构 | 中 | CLI/WASM API 稳定后再做桌面 UI |

---

## 时间线总览

```
Week  1-2:  [====      ] 阶段一：Workspace 与基础类型
Week  3-6:  [========  ] 阶段二：Modifier 语义与 ModDB
Week  7-10: [========  ] 阶段三：英文 Modifier Parser 与 Cache
Week 11-15: [========  ] 阶段四：PoB 属性目录与贡献追踪基础
Week 16-21: [==========] 阶段五：最小计算闭环
Week 22-28: [==========] 阶段六：技能、物品、天赋来源接入
Week 29-32: [======    ] 阶段七：PoB Build 兼容与 Golden Regression
Week 33+:   [持续      ] 阶段八至十：完整机制、应用层、性能优化
```

**建议主线**：阶段一 → 阶段二 → 阶段三 → 阶段四 → 阶段五 → 阶段六。  
**并行支线**：fixtures 收集、PoB 对照数据整理、语言包 key 规划。  
**延后实现**：GUI、交易 API、多语言 raw item 导入、大规模性能优化。
