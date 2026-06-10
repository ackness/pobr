# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **PoBR** = Path of Building in Rust。目标是把 PathOfBuilding（Lua）的核心计算引擎迁移到 Rust：解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈，并在 PoB 兼容的基础上额外提供 source-level 归因（每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献）。

## 构建环境

本仓库位于本地 APFS 盘（`origin` = `git@github.com:ackness/pobr.git`），**直接使用普通 cargo 命令**，无需 target-dir 重定向或 `CARGO_INCREMENTAL=0`。

## 常用命令

```bash
cargo test --workspace                                 # 全部测试（CI gate）
cargo build --workspace                                # 只编译 lib/bin
cargo clippy --workspace --all-targets -- -D warnings  # lint（CI gate，warning 即失败）
cargo fmt --check                                      # 格式检查（CI gate）

cargo test -p pobr-core --test mod_db                  # 单个测试套件
cargo test -p pobr-core --test mod_db -- sum_traced    # 单个用例（名称子串过滤）
cargo bench -p pobr-core --bench mod_db_bench          # ModDB 热查询基准（criterion）

# CLI（apps/pobr-cli，二进制名 pobr）
cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
cargo run -p pobr-cli -- decode-code <pob_code>        # PoB Build Code → XML
cargo run -p pobr-cli -- parse-mod "20% increased Fire Damage"

# 工具
cargo run -p sync-pob-catalog -- scan  --pob-root <PoB路径> [--out catalog.json]
cargo run -p sync-pob-catalog -- check --pob-root <PoB路径> --catalog catalog.json
cargo run -p sync-pob-catalog -- extract-lua --vendor-root vendor/PathOfBuilding-PoE2/src \
    --out data/4.5.0.3.4/overlay/skill_overrides.json   # vendor Lua → overlay JSON（需 luajit）
cargo run -p lint-i18n                                  # 语言包完整性检查
```

- Rust **edition 2024**；workspace 版本统一 `0.1.0`。
- CI gate（见 `devs/docs/architecture/06-development-workflow.md`）= fmt + clippy + test。涉及计算/Modifier/parser 的改动需补对应的集成测试或 golden fixture。

## Workspace 结构

`devs/docs/architecture/` 描述目标架构；当前实现进度见各 crate。14 个 member 中，除 `pobr-item` 外均已落地实现：

| Crate | 职责 | 依赖 |
|-------|------|------|
| `crates/pobr-data` | 纯数据定义，零逻辑、零 I/O。所有 crate 的底层依赖。含 `catalog.rs`（入库 JSON schema：`BaseItemDef`/`StatDef`/`ModDef`/`SkillGemDef`/`GrantedEffectDef`/`PassiveNodeDef`/`DataManifest` 等）、`damage.rs`（`AilmentInstance`/`DebuffInstance`）、`build_config.rs`、`display_stat.rs`（`DisplayStatDefinition`/`DisplayStatValue`/`ParityStatus`）、`stat.rs`（含 `BoundarySpec`） | 仅 `serde` |
| `crates/pobr-core` | Modifier 解析/存储/聚合 + 计算引擎 + source-level 归因 + 来源接入（item/passive/gem ingest）。零 I/O | `pobr-data` |
| `crates/pobr-gamedata` | 运行时数据 loader——数据系统里唯一持有文件 I/O 的层。把 `data/<poe_version>/` 入库 JSON 反序列化为 `pobr-data::catalog` 类型（`GameData::new(version_dir)`，按域懒加载 + i18n 边车） | `pobr-data` + `serde_json` |
| `crates/pobr-i18n` | 语言包加载 / fallback / 显示文本映射；`en-US`（canonical）+ `zh-TW`，locale toml 经 `include_str!` 内嵌。`Translator`/`LanguageId`/`stat_text` | `pobr-data` + `toml` |
| `crates/pobr-tree` | 天赋树拓扑、allocated node mod 收集、范围珠宝（first pass）。节点数据用 `pobr-data::catalog::PassiveNodeDef` | `pobr-data` |
| `crates/pobr-build` | Build 状态、PoB Build Code 编解码（XML ↔ zlib ↔ URL-safe Base64，padding 容错）、导入识别、`CalcOrchestrator`（带缓存）、Build 对比 | `pobr-data`/`pobr-core`/`pobr-tree`/`pobr-item` + `quick-xml`/`base64`/`flate2` |
| `crates/pobr-trade` | Trade 查询/价格抽象：`TradeBackend` trait + 离线 `MockBackend`（测试不联网；真实 HTTP 后续接入） | `pobr-data` |
| `crates/pobr-item` | **占位骨架（仍未实现）**——raw item 解析/自定义物品。raw item 文本解析当前已由 `pobr-core::item_text` + `item::ingest_item` 承担；本 crate 的职责边界（custom item 编辑态）待厘清后实现 | `pobr-data` + `pobr-core` |
| `apps/pobr-cli` | CLI：`calculate` / `parse-mod` / `decode-code` / `encode-code`（命令逻辑在 lib，便于测试） | `pobr-build`/`pobr-core`/`pobr-i18n` |
| `apps/pobr-wasm` | Web/WASM API：默认 features 为纯 Rust JSON 入出（`calculate_json`/`translate`），`wasm` feature 下 wasm-bindgen 绑定 | `pobr-build`/`pobr-core`/`pobr-i18n` |
| `apps/pobr-desktop` | 桌面入口最小骨架（headless 不验证 GUI；egui 等重 GUI 框架后续接入） | `pobr-build`/`pobr-core`/`pobr-i18n` |
| `tools/pobr-data-adapter` | 数据管线适配器——把 GGG `.dat` 导出解析外键、反范式化为入库最小 JSON 落到 `data/<poe_version>/`。仅离线工具 | `pobr-data` + `serde_json` |
| `tools/sync-pob-catalog` | 从 PoB 核心 Lua 抽取属性 catalog、parity 检查/diff、self-contained fixture。仅工具 | `pobr-data` + `regex`/`serde_json` |
| `tools/lint-i18n` | 语言包完整性检查（非 canonical 语言不得有 en-US 之外的多余 key） | `pobr-i18n` |

依赖方向只能向下，`pobr-data` 是最底层、不依赖任何项目内 crate。计算核心保持纯函数 + 确定性，不引入共享可变状态。

**数据管线**：`GGG .dat 导出` →（`pobr-data-adapter` 离线适配）→ `data/<poe_version>/*.json`（schema = `pobr-data::catalog`）→（`pobr-gamedata` 运行时 loader）→ 上层计算。I/O 收口在 `pobr-gamedata` 一处；`pobr-data`/`pobr-core` 维持零 I/O。

> 历史整合：`devs/docs/integration-inventory.md` 记录了一次从外部沙箱实现迁移到本仓库的清单（冲突一律保留本仓库权威实现）；上层 crate 与部分 calc 机制由此落地。

## 计算引擎架构（pobr-core）

数据流：**modifier 文本 → 解析 → ModDb → 聚合查询 → calc → OutputTable + Breakdown + TraceGraph + AttributionReport**。

- **`mod_parser.rs`** — 英文 PoB 兼容的 modifier 文本解析。识别 `N% increased/reduced`（→ Inc）、`N% more/less`（→ More）、`+N`/`N`（→ Base）以及前缀（`attacks/spells deal`）和后缀 tag（`while on full life`、`per X charge`）。无法解析的文本归为 `ParseStatus::Unsupported`（不报错，收集起来）。
- **`modifier.rs`** — `Modifier { name, mod_type, value, source(原文), origin(归因), flags, keyword_flags, tags }`。`matches(cfg)` 判定是否适用，`effective_number(cfg)` 应用 Multiplier tag。
- **`mod_db.rs`** — 按 `ModName` 索引的 Modifier 库：`sum`(Base/Inc 相加) / `more`(连乘 `Π(1+v/100)`) / `flag` / `override_`(后写覆盖) / `list`；`sum_traced`/`more_traced`/`flag_traced`/`override_traced`/`contributions` 返回带来源贡献构建 TraceGraph；`filtered`/`iter_mods` 供归因重算。标准属性管线 `(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`。`ModList` 支持 parent 链式聚合。
- **`config.rs::CalcConfig`** — 计算上下文：flags / keyword_flags / skill_types / damage_type / conditions / multipliers（如 `Channelling` 条件）。`CalcConfig::attack()` 是常用预设。
- **`trace.rs::TraceGraph` + `attribution.rs`** — source-level 归因（PoBR 相对 PoB 的核心增量）。DAG 把每个输出回溯到 `SourceId`；`attribute()` + `AttributionReport::direct()` 给出 direct / marginal / interaction 三种口径（移除某来源重算得边际贡献）。
- **来源接入**：`item.rs::ingest_item` / `item_text.rs::parse_item_text` / `passive.rs::ingest_passive_nodes` / `skill_source.rs::ingest_gem` 把装备/天赋/宝石转为带 `SourceId` 归因的 modifier 注入 ModDb。
- **`calc/`** — 计算编排：
  - `session.rs::CalculationSession` 是高层入口：`new(MinimalInput)` → `add_modifier_texts(...)` / `add_item`/`add_passive_nodes`/`add_gem` → `perform_minimal()` → `MinimalOutput`；`snapshot()` 供归因，`output()` 取完整 `OutputTable`。
  - `perform.rs::perform(env)` 编排 minimal offence + defence，并在末尾 fill 机制阶段（抗性边界/技能时间/伤害向量/异常/EHP/预留·恢复·格挡·抑制）写入 `OutputTable`。
  - `offence.rs` 算 life/mana/抗性/暴击/命中/DPS（`calculate_minimal` + 归因版 `calculate_minimal_traced`）。`defence.rs` 算 armour/evasion/ES、命中率 `accuracy/(accuracy+(evasion/4)^0.8)`、护甲减伤 `armour/(armour+10*raw_hit)`。
  - 机制模块：`stat_boundary.rs`（抗性 uncapped/max/final/overcap/missing + floor）、`skill_use_time.rs`（speed bucket + action speed 独立乘区 + 服务器帧 cap + channelling）、`damage.rs`（按伤害类型分桶 + 转换归一化 + gain-as-extra + increased double-dip）、`ailment.rs`（流血/点燃/中毒 magnitude + 感电 + 腐化之血）、`ehp.rs`（各类型 max hit + EHP）、`survivability.rs`（预留/恢复/格挡/法术抑制）。
  - `display_catalog.rs`（在 pobr-core 根）—— 强类型展示字段目录（`display_catalog()` 列出 Computed/Planned + `ParityStatus`）+ `extract_display_values(&OutputTable)`，对应 PoB `BuildDisplayStats`。

## 游戏机制资料库（agent-docs/）

`agent-docs/` 是 **PoE2（0.5.0）机制中文参考资料**（伤害类型、抗性、护甲/闪避/ES、暴击、异常状态、伤害-防御计算顺序、宝石、通货等）。

**实现任何机制前的查证顺序**（见 `06-development-workflow.md` §2.1.1）：
1. 先查 `agent-docs/` 对应主题；
2. 对照 PoB-PoE2 Lua 计算实现（`CalcSetup.lua` / `CalcPerform.lua` / `CalcOffence.lua` / `CalcDefence.lua` / `ConfigOptions.lua` / `QuestRewards.lua` / `Data.lua` 等）；
3. 对照官方 patch notes / PoE2 Wiki / PoE2DB / 游戏数据。

`agent-docs/` 是**开发输入资料，不是最终权威**；与一手数据冲突时以可验证来源为准，并直接修正文档（保留来源说明）。注意 PoB 公式多基于 PoE1，与 PoE2 存在差异（如护甲系数 `*5` vs `*10`），文档中已标注。

## 关键约定

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本走 `pobr-i18n`（`en-US`/`zh-TW`）。
- **不可变 / 确定性**：calc 函数对 `Env` 的可变写入集中在 `perform`，并行化只在只读快照阶段展开。
- **PoB2 兼容是回归基准**：Build Code 走 XML → deflate → URL-safe Base64（`pobr-build::{decode,encode}_pob_code`，已用真实 PoB2 ninja code 验证；样本见 `examples/demo-bd-test/`）；自定义/复制物品需保留原始文本块以便和 PoB2 对比。
- 文档以可执行契约为主；改变 crate 边界、聚合语义、catalog/parity 规则时同步更新 `devs/docs/architecture/*`。
