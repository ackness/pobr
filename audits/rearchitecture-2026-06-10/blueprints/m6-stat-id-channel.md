# M6 E/F — stat_id 第二通道 设计文档

> 撰写：2026-06-27 · 架构裁决依据 [20-target-architecture.md](../20-target-architecture.md) P10 · 路线图 [21-roadmap.md](../21-roadmap.md) M6 节
> 研究基础：3 路并行只读研究（stat_descriptions 抽取 / tree stat_id 重抽 / 双通道 harness），结论汇入本文。

## 0. 目标与定位（P10）

PoB2 走**英文文本解析**是历史包袱（文本随本地化漂移、歧义、解析规则维护重）。PoBR 长期可由 **stat_id 直通**绕过文本解析——每个词条由稳定 GGG `stat_id` + 数值，经一张 `stat_id → Modifier 模板` 表直接产出 Modifier。这是 PoBR 比 PoB2 更稳的架构增量。

**P10 裁决 = 双通道**：文本通道先达 parity 等价（门禁基准是 PoB2 文本语义），stat_id 通道作**第二通道**与之并跑、出 differential diff 报告，**按域（先 `passive_tree` 再 `mods`）`DIFF<0.1%` 后渐进切换**。不能先于 parity 基准切换。

**一句话**：E/F 不是从零造，是把仓库**已有的** skill 域 stat_id 通道推广到 passive_tree / mods，用**已有的**五态 diff harness 对拍。

---

## 1. 已有可复用资产（不重造轮子）

| 资产 | 位置 | 复用方式 |
|------|------|----------|
| **stat_id→Modifier 引擎**（已生产） | `crates/pobr-core/src/rules/stat_map_engine.rs::map_stat`（:213-248） | `(effect_id, set_key, stat, stat_value) → Vec<Modifier>`；merge 公式 `注入值 = entry.value or stat值×mult×scalar/div + base`（:9-18）。推广为通用 Stats 域版本即得第二通道核心 |
| **模板表 schema** | `crates/pobr-data/src/catalog/stat_map.rs::StatMapEntry`（:94-122） | `mods[]`（name/mod_type/flags/keyword_flags/tags）+ merge 参数。stat_id_map 的条目同构 |
| **五态 diff harness** | `crates/pobr-core/tests/parser/parser_dual_run.rs`（Tally:22-34、name_blind:44-61、run_corpus:236-298、门禁:391-423） | 文本通道 legacy-vs-engine 已跑到 DIFF=0；改两个产出源即得 text-vs-statid 对拍 |
| **比较单位** | `crates/pobr-core/src/mod_parser/canonical.rs::canonical_outcome`（:14-41） | 排序 `Vec<Modifier>` 规范串（剔 origin、f64 最短往返）。**禁两套序列化**（文件头契约），两通道都过此函数 |
| **归一资产** | `overlay/vendor_name_aliases.json` + [m6-alias-table.md](m6-alias-table.md)（real-rename:50-74、五类归一 C1-C5:83-235） | vendor→PoBR 词表归一。第二通道产出 Modifier 时**必须套同一张表**（与文本引擎 `engine.rs:208 normalize_pobr_name` 落同一形态），diff 才可能逼近 0.1% |
| **skill 域 .dat 数值通道（先例）** | `data/<ver>/base/granted_effect_stat_sets.json`（stat_id + 分级数值，直接来自 .dat） | 证明「模板 + .dat 数值 → Modifier」范式已跑通；passive_tree 缺的就是这份「stat_id+value」供料 |

---

## 2. 文本通道现状（要对齐的产物形态）

- 入口：`ingest_passive_nodes_with_ctx(nodes, ctx)`（`crates/pobr-core/src/passive.rs:56-89`）对每行 `modifier_texts` 调 `ctx.parse` → `parse_mod_engine`（`engine.rs:36`）。
- 单行产物 = `ParseOutcome{ mods: Vec<Modifier>, status, unparsed }`（`outcome.rs:20-33`）。
- 可比 `Modifier` 字段（`modifier.rs:197-207`）：`name(ModName) / mod_type(Base/Inc/More/Flag/List/Override) / value / flags / keyword_flags / tags(Condition/Multiplier/PerStat/DamageType/SkillTypes/...)`。diff 时**剔 origin、保留 source**。
- 关键：引擎产出已是**归一后**形态（`engine.rs:208 normalize_pobr_name`：C3 damage flag→专名、C5 DamageType tag、Speed 拆分等）。第二通道要对齐的是归一**后**形态，不是裸 vendor 名。

---

## 3. 🔴 核心前置：passive_tree 的 stat_id + value 供料（最大未决项）

**当前缺口**：`PassiveNodeDef.stats` 是 `Vec<String>` 渲染英文文本（`catalog/tree.rs:72-74`），**无 stat_id、无数值槽**。数据源 `pipeline/tree/data.json`（GGG 社区树导出 `poe2-skilltree-export`，`tools/pobr-data-adapter/src/tree.rs:1-8`）已经把 stat_id+value 经 StatDescriptions **渲染成文本**——值在文本里（`"15% increased chance to Shock"` 的 15）。

**vendor PoB 树同样只有文本**（`TreeData/0_5/tree.json` 的 node 无 `sd`、无 stat 引用）——PoB 也靠 ModParser 解析文本。故 PoB 不能作 stat_id 源。

### 三条恢复路线

| 路线 | 可行度 | 工作量 | 阻塞/风险 |
|------|--------|--------|----------|
| **A. 重抽 GGG `PassiveSkills.dat` 的 Stats 外键** | 中（取决于 schema + CDN） | 中-高 | `pipeline/config.json` 当前 20 表**无 PassiveSkills**（未下载）；CDN 只留当前补丁（旧版 404）；config patch 已是 `4.5.2.1.3` 而入库数据仍 `4.5.0.3.4`（版本错位需先对齐）；`PassiveSkills` 是否带 stat **值**待验；需新 join（data.json `skill` ↔ PassiveSkills 行） |
| **B. 从 PoB tree 取** | **不可行** | — | PoB tree 也只有文本 |
| **C. 反向 文本→stat_id 匹配** | 中（简单条 80%+，复合/条件歧义） | 中 | 需先把 StatDescriptions 抽成 JSON（见 §4）；一行多 stat_id / 条件·否定变体 → 不可 1:1；须 en-US canonical + 同款剥标记（`engine.rs:611 strip_pob_brackets`） |

**裁决建议**：优先验证 **Route A**（PoE2 `PassiveSkills.dat` 是否在 dat-schema 带 `Stats` 列且当前 patch 可下）——成立则最干净、无歧义。否则退 **Route C**（反查），复用 §4 的 stat_descriptions 抽取，对复合/条件行保留文本通道兜底。**此前置不落地，passive_tree 第二通道无数据可跑**——可先在 `mods` 域试点（词缀池 `mods.json` 本身在 .dat 带 stat_id+value，数据前置更轻），但 roadmap 钦定 passive_tree 先。

### schema 改动
`PassiveNodeDef`（`catalog/tree.rs:62`）与现有文本 `stats` 并存，新增：
```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub stat_ids: Vec<(String, f64)>,   // (stat_id, 节点掷值)；节点级一组 set
```
**节点级一组 set，不强行与 `stats[i]` 逐行对齐**（GGG data.json 不给逐行 stat_id；一行文本可能映多 stat_id）。文本 `stats` 仍作主通道，`stat_ids` 作第二通道交叉源。adapter 改动：Route A 在 `tree.rs:138-155` `RawNode→PassiveNodeDef` 加 join（需先 `config.json` 加 `PassiveSkills` 表重下）；Route C 仿 `tree_coords.rs` 模式新增 `--tree-stat-ids` pass（读 `passive_tree.json` → 回填 → 原地写回），`main.rs:90-183` 加 `Mode` 分支。

---

## 4. stat_id → Modifier 映射表抽取（两段式）

源头 = vendor `StatDescriptions/*.lua`（`passive_skill_stat_descriptions.lua` 4229 行 + 继承 `stat_descriptions.lua` 24 万行）。结构：descriptor 数组 + `["stat_id"]=index` lookup 表 + `parent="<scope>"` 作用域链。每 descriptor：`stats={stat_id...}` + 文本变体数组（`text="{0:+d} to Devotion"` + `limit` 区间 + special 条件如 `{k="negate",v=1}`）。

**覆盖率画像**：单 stat 单 text（喂正值→渲染→解析）≈ **94%**；compound（多 stat / mode 选择子 / guard-stat）≈ **6%**。先吃单 stat 即拿 90%+ 覆盖。

### 段 A：luajit 渲染 canonical 文本 → `overlay/stat_descriptions.json`
新增 `sync-pob-catalog extract-lua --what stat-descriptions`，仿 `extract_skill_stat_map.lua` 模式（stub + `loadVendorChunk` + `invoke_luajit_jsonl`，`extract_lua.rs:162`）。**直接遍历 descriptor 数组**（不调 `describeStats`——它按 `stats[1]` 折叠 compound、用 lineMap 映回最后 stat，对单 stat 不利）：
- 单 stat：端口最小 `matchLimit`，喂正代表值 `V=1`（自然命中正向/非 negate 变体）→ 占位符 gsub（照搬 `StatDescriber.lua:281-315` gsub 链）→ 按 `\n` 切行 → emit `{stat, text, value_arg:0, variant}`。
- compound：emit `{stat:<stats[1]>, compound:true, member_stats:[...], template:"<raw text>", rendered:[...]}`，**不在抽取期强解**。
- 重复 stat_id → 报错不静默覆盖（仿 `extract_stat_map.rs:78`）。

### 段 B：canonical 文本 → Modifier 模板 → `overlay/stat_id_map.json`
`sync-pob-catalog` **不依赖 pobr-core**（语义解析是 pobr-core 职责）。故解析步骤放 **pobr-core 侧 generator（集成测试 / xtask）**：读 `stat_descriptions.json` + `mod_parser_rules.json` → 每条 canonical 文本跑 `parse_mod_engine` → `ParseOutcome` → 物化 + golden 断言 byte-stable。`status==Unsupported` 的 stat_id 自然成「需手工 overlay」清单。**好处**：抽取边界（vendor 文本）与解析边界（parser 规则）各自版本可追溯，任一升级 golden diff 兜住。

### 输出 schema（`stat_id_map.json`）
```jsonc
{ "_meta": { "schema":"stat_id_map/v1", "source_overlay":"stat_descriptions.json", "parser_rules_commit":"…" },
  "stats": {
    "base_devotion": { "text":"+1 to Devotion", "status":"parsed",
      "mods":[ { "name":"Devotion","mod_type":"Base","flags":[],"keyword_flags":[],"tags":[],"value_arg":0 } ] },
    "strength_+%":   { "text":"1% increased Strength","status":"parsed",
      "mods":[ { "name":"Strength","mod_type":"Inc","tags":[],"value_arg":0 } ] },
    "some_weird_stat": { "text":"…","status":"unsupported","mods":[] } },
  "compound": {
    "local_minimum_added_physical_damage": {
      "member_stats":["…min…","…max…"], "template":"Adds {0} to {1} Physical Damage", "status":"parsed",
      "mods":[ {"name":"PhysicalMin","mod_type":"Base","value_arg":0},
               {"name":"PhysicalMax","mod_type":"Base","value_arg":1} ] } } }
```
`value_arg` = 本 mod 数值取自哪个 `{n}` 占位符 / member stat（单 stat=0；compound 区分 min/max）——**表达 compound 一对多的关键**。Modifier 模板字段对齐 `modifier.rs:197-207`，去 `value/source/origin`。

---

## 5. 双通道 diff harness（复用 `parser_dual_run` 五态）

对同一节点/词缀，文本通道 vs stat_id 通道各产 `Vec<Modifier>` → `canonical_outcome` 逐条比，沿用 `Tally` 五态：

| 态 | 含义 |
|----|------|
| **EQ** | 两通道 canonical 逐字节一致 |
| **DIFF** | 都产但不一致，拆 name-only（`name_blind` 去名后等价）vs structural（去名后仍异） |
| **TEXT_ONLY** | 文本产、stat_id 空（旧 harness 的 OLD_ONLY） |
| **STATID_ONLY** | stat_id 产、文本空（增益 / 文本 Unsupported 的覆盖，不阻塞） |
| **UNSUP** | 双失配 |

**门禁度量**（roadmap「diff<0.1%」）= 按域实例级 `DIFF/total < 0.1%` **且 `TEXT_ONLY = 0`**（第二通道须是文本通道能力的形态超集，同 `c1_diff_zero_gate` 口径）。

**compound 不需特殊拆值**：.dat 里 min/max 是两个独立 stat_id 各带一值，各产一 mod；diff 按集合（`canonical.rs:15` 已 `lines.sort()`）逐条配平。真正风险是**单 stat 模板产多 mod**（聚合名等，见 §7）。

---

## 6. 切换机制（按域 + 单 commit 翻开关）

- **分发缝在域 ingest 入口，不在 `parse_mod_engine`**（后者是逐行文本派发，与 stat_id 无关）。正确缝：`ingest_passive_nodes_with_ctx`（`passive.rs:56`）/ `item::ingest_item` / 词缀 ingest——已是「来源→Modifier」对称范式。
- 仿 `ParseCtx` 多态分发，给 ingest 入口加通道选择（按域独立）：
  ```rust
  enum ChannelMode { Text, StatId, DualRun }   // DualRun = 两路都产、喂 diff harness
  struct ChannelSelect { passive_tree: ChannelMode, mods: ChannelMode, items: ChannelMode }
  ```
  `AllocatedNode` 扩 `node_stats: Vec<(StatId, f64)>`（§3 回填）；`Text` 走现 `ctx.parse`，`StatId` 走新 `map_node_stat(stat, value)`（复用 `map_stat` 范式），`DualRun` 两路对拍。归因 `SourceId` 两路一致（diff 时 origin 本就剔）。
- **落地节奏 = 仓库标准范式**（CLAUDE.md：buff-pass-aura / parser-engine / statmap 双跑皆此）：① 接线零行为（DualRun 只跑 diff、不改 calc 主路径，parity 零回归）；② diff 报告该域干净（DIFF<0.1% 且 TEXT_ONLY=0）；③ 单 commit 把该域默认翻 `StatId`、baseline 显式审查；④ **先 passive_tree 再 mods**。开关在 pobr-build orchestrator（注入），分发在 pobr-core ingest 入口（保 P9 零 I/O）。

---

## 7. 风险与务实分波

### diff<0.1% 可能到不了的两类（参考删 legacy 的 off-by-8 教训，见 [[pobr-m3-progress]]）
1. **归一层未覆盖的残留**：通用 Stats 模板表的 tag/聚合/flag 归一不完整 → structural DIFF 不收敛。须先用 `vendor_name_aliases.json` + 五类归一补到 DIFF=0 才切。
2. **per-stat / 引擎 gap**：PerStat vs Multiplier tag（C2）、AilmentMagnitude 漏 POISON kw（C3 真 bug）等两通道落点不一致 → 永久 structural DIFF。这些是「引擎需补」，须切换前逐条收敛、独立审 baseline。

### 务实分波（可达性递增）
- **波 0（前置，先于一切）**：§3 的 `PassiveSkills.dat` + adapter 回填 `node_stats`；core 建**通用 Stats 域 stat_id→Modifier 模板表**（复用 `map_entry`）。无此则 passive_tree 通道无数据。
- **波 1（首切目标）**：普通小天赋（`Normal`）+ 单 stat 单值 + 无 tag，且名落别名表 56 identity / 20 real-rename（`+N to Strength` / `N% increased Fire Damage` / 抗性）。compound/conditional/aggregate 都不沾，最易 DIFF<0.1%。
- **波 2**：compound（min/max 伤害）+ 带 DamageType/Condition tag 的小天赋（验模板 tag 通道）。
- **波 3+（可留文本通道）**：聚合名展开、notable/keystone、`Allocates <passive>` special、跨 actor/PerStat——保留文本通道，stat_id 通道长期收敛，不阻塞整体节奏。

---

## 8. 实施步骤序列（落地清单）

1. **可行性验证（先做）**：确认 PoE2 `PassiveSkills.dat` 在 dat-schema 带 `Stats` 列 + 当前 patch CDN 可下；同时对齐入库数据版本（`4.5.0.3.4` vs config `4.5.2.1.3`）。→ 决定 Route A vs C。
2. **段 A 抽取**：`sync-pob-catalog extract-lua --what stat-descriptions` → `overlay/stat_descriptions.json`（+ regen 测试）。
3. **段 B 映射**：pobr-core generator → `overlay/stat_id_map.json` + golden byte-stable 测试；产 `status:unsupported` 清单。
4. **波 0 供料**：tree adapter 回填 `node_stats`（Route A/C）；通用 Stats 模板表（复用 `map_entry`）。
5. **diff harness**：`tests/.../statid_dual_run.rs`（复用 `Tally`/`name_blind`/`canonical_outcome`），DualRun 跑 passive_tree 域，出五态报告。
6. **归一收敛**：套 `vendor_name_aliases.json` + 五类归一，逐条消化 structural DIFF（独立审 baseline），到 passive_tree 域 DIFF<0.1% 且 TEXT_ONLY=0。
7. **翻开关**：单 commit 把 `passive_tree` ChannelMode 默认 `StatId`、baseline 显式审查。
8. **mods 域**：重复 4-7（mods 数据前置更轻）。
9. M6 E/F 完成 → M7。

---

## 附：本设计未决/待 owner 拍板

- **Route A 可行性**（PassiveSkills.dat 可下且带 Stats 值）——决定整个 passive_tree 通道走法；步骤 1 验证后定。
- **数据版本对齐**：config patch=`4.5.2.1.3` vs 入库 `4.5.0.3.4`——重抽前需对齐（否则 stat_id↔节点 join 错位）。
- **mods 域是否提前**：若 Route A 受阻，可先切 `mods`（数据前置轻），但偏离 roadmap 钦定顺序。
