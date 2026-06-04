# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **PoBR** = Path of Building in Rust。目标是把 PathOfBuilding（Lua）的核心计算引擎迁移到 Rust：解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈，并在 PoB 兼容的基础上额外提供 source-level 归因（每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献）。

## 构建环境

本仓库已 `git clone` 到本地 APFS 盘（`origin` = `git@github.com:ackness/pobr.git`），`target/` 建在仓库默认位置即可，**直接使用普通 cargo 命令**，无需任何重定向或 `CARGO_INCREMENTAL=0`。

> 历史背景：早期源码曾放在 SMB 网络卷（`smbfs`，不支持文件锁），cargo 会因 `could not create session directory lock file: Operation not supported (os error 45)` 失败，当时通过项目级 `.cargo/config.toml` 把 `build.target-dir` 重定向到本地盘绕过。迁移到本地后该问题不再存在；若将来在 SMB/网络卷上开发，可用环境变量 `CARGO_TARGET_DIR` 指向本地盘临时绕过。

## 常用命令

```bash
cargo test --workspace                                # 全部测试
cargo build --workspace                               # 只编译 lib/bin（不编译 test 目标）
cargo test -p pobr-core --test mod_db                 # 单个测试套件
cargo test -p pobr-core --test mod_db -- sum_traced   # 单个用例（名称子串过滤）
cargo fmt --check                                     # 格式检查（CI gate）
cargo clippy --workspace --all-targets -- -D warnings  # lint（CI gate，warning 即失败）

# sync-pob-catalog 工具：从 PoB Lua 抽取 output/display/breakdown catalog 并做 parity 检查
cargo run -p sync-pob-catalog -- scan  --pob-root <PoB路径> [--out catalog.json]
cargo run -p sync-pob-catalog -- check --pob-root <PoB路径> --catalog catalog.json
```

- Rust **edition 2024**（需要较新的工具链）；workspace 版本统一 `0.1.0`。
- CI gate（见 `devs/docs/architecture/06-development-workflow.md`）= fmt + clippy + test。涉及计算/Modifier/parser 的改动需补对应的集成测试或 golden fixture。

## 实现现状 vs. 设计文档（重要）

`devs/docs/architecture/` 描述的是**目标架构**，其中**大部分尚未实现**。workspace 现有 13 个 members，但**只有 5 个有实质实现**，其余 8 个是占位骨架（最小可编译、仅挂项目内 path 依赖，无业务逻辑）：

**有实质实现（5 个）：**

| Crate | 职责 | 依赖 |
|-------|------|------|
| `crates/pobr-data` | 纯数据定义，零逻辑、零 I/O。所有 crate 的底层依赖。含 `catalog.rs`（入库 JSON 的自有 schema：`BaseItemDef` / `StatDef` / `ModDef` / `SkillGemDef` / `GrantedEffectDef` / `DataManifest`） | 仅 `serde` |
| `crates/pobr-core` | Modifier 解析/存储/聚合 + 计算引擎 + 归因。零 I/O | `pobr-data` |
| `crates/pobr-gamedata` | **运行时数据 loader**——数据系统里唯一持有文件 I/O 的层。用 serde 把 `data/<poe_version>/` 的入库 JSON 反序列化为 `pobr-data::catalog` 类型（`GameData::new(version_dir)`，按域懒加载 + i18n 边车） | `pobr-data` + `serde`/`serde_json` |
| `tools/sync-pob-catalog` | 从 PoB 核心 Lua 抽取属性 catalog、检查 parity（仅工具，不参与运行时） | `pobr-data` + `regex`/`serde_json` |
| `tools/pobr-data-adapter` | **数据管线适配器**——把 GGG `.dat` 原始导出（pathofexile-dat 产物）解析外键、反范式化为入库最小 JSON，落在 `data/<poe_version>/`（base_items / stats / mods / skill_gems / granted_effects + zh-TW i18n 边车）。仅离线工具 | `pobr-data` + `serde`/`serde_json` |

依赖方向只能向下，`pobr-data` 是最底层、不依赖任何项目内 crate。计算核心保持纯函数 + 确定性，不引入共享可变状态。

**占位骨架（8 个，尚未实现）：** `crates/pobr-i18n`（语言包/显示文本，→ pobr-data）、`crates/pobr-tree`（天赋树拓扑/寻路/珠宝，→ pobr-data）、`crates/pobr-item`（raw item 解析/自定义物品，→ pobr-data + pobr-core）、`crates/pobr-build`（Build 状态/PoB Build Code/计算编排，→ pobr-data + pobr-core + pobr-tree + pobr-item）、`crates/pobr-trade`（Trade API，→ pobr-data）、`apps/pobr-cli`、`apps/pobr-desktop`、`apps/pobr-wasm`（均 → pobr-build）。这些 crate 仅有模块级占位说明，无业务逻辑；外部重依赖（egui / wasm-bindgen / reqwest / quick-xml / flate2 等）等真正实现时再加。实现某个域时从对应骨架起步，并参照 `02-crate-design.md` 同名章节。

**数据管线**：`GGG .dat 导出` →（`pobr-data-adapter` 离线适配）→ `data/<poe_version>/*.json`（自有 schema = `pobr-data::catalog`）→（`pobr-gamedata` 运行时 loader）→ 上层计算。I/O 收口在 `pobr-gamedata` 一处；`pobr-data`/`pobr-core` 维持零 I/O。注意：02-crate-design.md 早期把 `GameData` 设计成 `pobr-data` 内 `include_bytes!` + bincode 编译期内联，实际改为「独立 loader crate + 运行时读 JSON」，以该实现为准。早期规划的 `tools/export-poe-data` / `gen-mod-cache` 已分别由 `pobr-data-adapter` / `sync-pob-catalog` 承担，不再单建。

## 计算引擎架构（pobr-core）

数据流：**modifier 文本 → 解析 → ModDb → 聚合查询 → calc → OutputTable + Breakdown + TraceGraph**。

- **`mod_parser.rs`** — 英文 PoB 兼容的 modifier 文本解析。识别 `N% increased/reduced`（→ Inc）、`N% more/less`（→ More）、`+N`/`N`（→ Base）以及前缀（`attacks/spells deal`）和后缀 tag（`while on full life`、`per X charge`）。`parse_name` 把文本映射到稳定的 `ModName`。无法解析的文本归为 `ParseStatus::Unsupported`（不报错，收集到 `unsupported_modifier_texts`）。
- **`modifier.rs`** — `Modifier { name, mod_type, value, source(原文), origin(归因), flags, keyword_flags, tags }`。`matches(cfg)` 判定是否适用（flags / keyword / condition / damage_type / skill_types），`effective_number(cfg)` 应用 Multiplier tag。
- **`mod_db.rs`** — 按 `ModName` 索引的 Modifier 库。核心聚合语义：
  - `sum(Base|Inc, …)`：直接相加
  - `more(…)`：连乘 `Π(1 + v/100)`
  - `flag` / `override_`（后者后写覆盖先写）/ `list`
  - `sum_traced(…)` / `contributions(…)`：返回带来源的贡献，构建 TraceGraph
  - 标准属性管线：`(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`（见 `calc/offence.rs::scaled_numeric_stat`、`calc/defence.rs::scaled_defence_stat`）。`ModList` 支持 parent 链式聚合。
- **`config.rs::CalcConfig`** — 计算上下文：flags / keyword_flags / skill_types / damage_type / conditions / multipliers。`CalcConfig::attack()` 是常用预设。
- **`trace.rs::TraceGraph`** — source-level 归因（PoBR 相对 PoB 的核心增量）。节点（`TraceNode` + `TraceOperation`）+ 边构成 DAG，把每个最终输出回溯到 `SourceId`（装备/词条/天赋/宝石/配置）。
- **`calc/`** — 计算编排：
  - `session.rs::CalculationSession` 是**高层入口**：`new(MinimalInput)` → `add_modifier_texts(...)`（自动解析）→ `perform_minimal()` → `MinimalOutput`。
  - `perform.rs::perform(env)` 编排 minimal offence + defence。`env.rs::Env` 持有 player/enemy `Actor` + `CalcConfig`。
  - `offence.rs` 算 life/mana/抗性/暴击/命中/DPS（含 `calculate_minimal` 与归因版 `calculate_minimal_traced`）。`defence.rs` 算 armour/evasion/ES、命中率公式 `accuracy/(accuracy + (evasion/4)^0.8)`、护甲减伤 `armour/(armour + 10*raw_hit)`。

## 游戏机制资料库（agent-docs/）

`agent-docs/` 是 **PoE2（0.5.0）机制中文参考资料**（伤害类型、抗性、护甲/闪避/ES、暴击、异常状态、伤害-防御计算顺序、宝石、通货等）。

**实现任何机制前的查证顺序**（见 `06-development-workflow.md` §2.1.1）：
1. 先查 `agent-docs/` 对应主题；
2. 对照 PoB-PoE2 Lua 计算实现（`CalcSetup.lua` / `CalcPerform.lua` / `CalcOffence.lua` / `CalcDefence.lua` / `ConfigOptions.lua` / `QuestRewards.lua` / `Data.lua` 等）；
3. 对照官方 patch notes / PoE2 Wiki / PoE2DB / 游戏数据。

`agent-docs/` 是**开发输入资料，不是最终权威**；与一手数据冲突时以可验证来源为准，并直接修正文档（保留来源说明）。注意 PoB 公式多基于 PoE1，与 PoE2 存在差异（如护甲系数 `*5` vs `*10`），文档中已标注。

## 关键约定

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本走 i18n（尚未实现）。
- **不可变 / 确定性**：calc 函数对 `Env` 的可变写入集中在 `perform`，并行化只在只读快照阶段展开。
- **PoB2 兼容是回归基准**：未来 Build Code 走 XML → deflate → URL-safe Base64；自定义/复制物品需保留原始文本块以便和 PoB2 对比。
- 文档以可执行契约为主；改变 crate 边界、聚合语义、catalog/parity 规则时同步更新 `devs/docs/architecture/*`。
