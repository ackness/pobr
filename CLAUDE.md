# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **PoBR** = Path of Building in Rust。目标是把 PathOfBuilding（Lua）的核心计算引擎迁移到 Rust：解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈，并在 PoB 兼容的基础上额外提供 source-level 归因（每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献）。

## 构建环境

直接使用普通 cargo 命令，无特殊环境要求（推荐安装 `cargo-nextest` 跑测试）。

- 每个 worktree / 并行会话使用**自己的 `./target`**。**禁止**设置共享 `CARGO_TARGET_DIR`——并发 cargo 会在构建目录锁上串行排队（症状：长时间无输出；stderr 的 `Blocking waiting for file lock` 提示不要用 `| tail` 等管道吞掉）。
- 同一 target 目录下 cargo 命令**一次一条、前台执行**，禁止后台叠加。

## 验证分层（重要：避免无谓的全量测试）

| 阶段 | 命令 | 说明 |
|------|------|------|
| 编辑循环中 | `cargo check -p <crate>` | 只查类型/借用错误，最快反馈 |
| 完成一个完整任务后 | `cargo nextest run -p <crate> --test <suite>`（或 `-E 'test(...)'`） | 只跑改动相关的定向测试，**不要跑全量** |
| 提交/合并门禁前 | `cargo nextest run --workspace` + clippy + fmt | 全量只在这一刻跑 |

## 常用命令

```bash
cargo nextest run --workspace                          # 全部测试（推荐；不含 doctest）
cargo test --workspace                                 # 全部测试（CI gate；仅在无 nextest 时用）
cargo clippy --workspace --all-targets -- -D warnings  # lint（CI gate，warning 即失败）
cargo fmt --check                                      # 格式检查（CI gate）

cargo nextest run -p pobr-core --test mod_db           # 单个测试套件
cargo nextest run -p pobr-core -E 'test(sum_traced)'   # 单个用例（filterset 过滤）
cargo bench -p pobr-core --bench mod_db_bench          # ModDB 热查询基准（criterion）

# PoB2 parity 仪表盘：逐 build 打印 PoBR vs PoB2 对照 + 聚合命中率
cargo test -p pobr-build --test parity -- --nocapture
# 其中 parity_no_regression 用例是回归门禁（命中率不得低于已记录基线）

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
tools/pob2-oracle/run.sh <build.xml>                    # PoB2 headless oracle：dump Lua 侧完整计算分解为 JSON（需 luajit；非 workspace 成员）
```

- Rust **edition 2024**；workspace 版本统一 `0.1.0`。
- 根 `Cargo.toml` 设置 `[profile.dev] debug = "line-tables-only"` 以加速 ~100 个测试二进制的链接（保留 panic 回溯行号）；需要 lldb 单步调试时临时改回 `debug = true`（会触发全量重编译）。
- CI gate（见 `devs/docs/architecture/06-development-workflow.md`）= fmt + clippy + test。涉及计算/Modifier/parser 的改动需补对应的集成测试或 golden fixture。

## Workspace 结构

`devs/docs/architecture/`（00–15 共 16 篇）描述目标架构与路线图；当前实现进度以代码 + `14-remaining-work-recheck.md` + parity 门禁为准（`11-implementation-progress.md` 已过时，见其顶部声明）。15 个 member 均已落地实现：

| Crate | 职责 | 依赖 |
|-------|------|------|
| `crates/pobr-data` | 纯数据定义，零逻辑、零 I/O，所有 crate 的底层依赖。核心是 `catalog.rs`（入库 JSON schema：`BaseItemDef`/`StatDef`/`ModDef`/`SkillGemDef`/`GrantedEffectDef`/`PassiveNodeDef`/`DataManifest` 等），另有 damage/build_config/display_stat/stat/monster 等域类型 | 仅 `serde` |
| `crates/pobr-core` | Modifier 解析/存储/聚合 + 计算引擎 + source-level 归因 + 来源接入（item/passive/gem/flask ingest）。零 I/O | `pobr-data` |
| `crates/pobr-gamedata` | 运行时数据 loader——数据系统里唯一持有文件 I/O 的层。把 `data/<poe_version>/` 入库 JSON 反序列化为 `pobr-data::catalog` 类型（`GameData::new(version_dir)`，按域懒加载 + i18n 边车；`repo_data_root()` 定位仓库数据目录） | `pobr-data` + `serde_json` |
| `crates/pobr-i18n` | 语言包加载 / fallback / 显示文本映射；`en-US`（canonical）+ `zh-TW`，locale toml 经 `include_str!` 内嵌 | `pobr-data` + `toml` |
| `crates/pobr-tree` | 天赋树拓扑、allocated node mod 收集、范围珠宝（first pass） | `pobr-data` |
| `crates/pobr-build` | Build 状态、PoB Build Code 编解码（XML ↔ zlib ↔ URL-safe Base64，padding 容错）、导入识别、`CalcOrchestrator`（带缓存）、Build 对比。**parity 测试的主战场**（见下） | `pobr-data`/`pobr-core`/`pobr-tree`/`pobr-item` + `quick-xml`/`base64`/`flate2` |
| `crates/pobr-trade` | Trade 查询/价格抽象：`TradeBackend` trait + 离线 `MockBackend`（测试不联网） | 独立叶子，自带类型（`query`/`types`），无项目内 crate 依赖；接真实后端时应桥接到 `pobr-data` 的 item/stat ID |
| `crates/pobr-item` | raw item 文本的**全保真编辑态**解析 + 逆向序列化（P16，`draft.rs`/`annotations.rs`/`build_raw.rs`，~1039 行）。职责边界清晰：**calc 视图**（剥标注 / variant 门控 / range 取值后喂引擎）由 `pobr-core::item_text` + `item::ingest_item` 承担；**编辑态视图**（保留 calc 刻意丢弃的 variant 名列表 / 行级标注 / 未建模标注，支持 BuildRaw 往返）由本 crate 的 `ItemDraft` 承担。复用 `pobr-core::mod_parser` 解析 modifier，避免规则重复 | `pobr-data` + `pobr-core` |
| `apps/pobr-cli` | CLI：`calculate` / `parse-mod` / `decode-code` / `encode-code`（命令逻辑在 lib，便于测试） | `pobr-build`/`pobr-core`/`pobr-i18n` |
| `apps/pobr-wasm` | Web/WASM API：默认 features 纯 Rust JSON 入出，`wasm` feature 下 wasm-bindgen 绑定 | 同上 |
| `apps/pobr-desktop` | 桌面入口最小骨架（headless 不验证 GUI） | 同上 |
| `tools/pobr-data-adapter` | 数据管线适配器——GGG `.dat` 导出 → 解析外键、反范式化为入库最小 JSON 落到 `data/<poe_version>/`。缺列默认告警降级（不中止，serde 按 `Option`/`default` 兜底），`--strict-columns` 才致命；产物 `_meta.regen_command` 记录再生成命令 | `pobr-data` + `serde_json` |
| `tools/sync-pob-catalog` | 从 PoB 核心 Lua 抽取属性 catalog、parity 检查/diff、vendor Lua → overlay JSON | `pobr-data` + `regex`/`serde_json` |
| `tools/lint-i18n` | 语言包完整性检查（非 canonical 语言不得有 en-US 之外的多余 key） | `pobr-i18n` |
| `tools/precompile-mods` | M6 mod-parser 规则离线预编译 / codegen 工具：把四层语料（build XML / passive_tree / special_derived / `--corpus-extra`）去重后逐行过 `pobr-core::parse_mod` 预解析，产出 `data/<version>/generated/parsed_mods.json` + 覆盖率报表（运行时懒加载为 `text→Vec<Modifier>` 缓存，热路径零解析） | `pobr-data` + `pobr-core` + `pobr-gamedata` |
| `tools/pob2-oracle` | **非 workspace 成员**（纯 Lua wrapper）：把 vendored PoB2 引导成 headless，加载 build 并 dump Lua 侧完整计算分解（中间值+最终值）为 JSON，用于钉死逐分量偏差。不修改 vendor 源 | luajit |

依赖方向只能向下，`pobr-data` 是最底层、不依赖任何项目内 crate。计算核心保持纯函数 + 确定性，不引入共享可变状态。

**数据管线**：`GGG .dat 导出` →（`pobr-data-adapter` 离线适配）→ `data/<poe_version>/*.json`（schema = `pobr-data::catalog`，当前版本 `4.5.0.3.4/`，含 `overlay/` 人工修正层）→（`pobr-gamedata` 运行时 loader）→ 上层计算。I/O 收口在 `pobr-gamedata` 一处；`pobr-data`/`pobr-core` 维持零 I/O。

## 计算引擎架构（pobr-core）

数据流：**modifier 文本 → 解析 → ModDb → 聚合查询 → calc → OutputTable + Breakdown + TraceGraph + AttributionReport**。

- **`mod_parser.rs`** — 英文 PoB 兼容的 modifier 文本解析。识别 `N% increased/reduced`（→ Inc）、`N% more/less`（→ More）、`+N`/`N`（→ Base）以及前缀（`attacks/spells deal`）和后缀 tag（`while on full life`、`per X charge`）。无法解析的文本归为 `ParseStatus::Unsupported`（不报错，收集起来）。
- **`modifier.rs`** — `Modifier { name, mod_type, value, source(原文), origin(归因), flags, keyword_flags, tags }`。`matches(cfg)` 判定是否适用，`effective_number(cfg)` 应用 Multiplier tag。
- **`mod_db.rs`** — 按 `ModName` 索引的 Modifier 库：`sum`(Base/Inc 相加) / `more`(连乘 `Π(1+v/100)`) / `flag` / `override_`(后写覆盖) / `list`；`*_traced` 变体返回带来源贡献构建 TraceGraph；`filtered`/`iter_mods` 供归因重算。标准属性管线 `(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`。`ModList` 支持 parent 链式聚合；敌方侧有独立 enemy ModDb。
- **`config.rs::CalcConfig`** — 计算上下文：flags / keyword_flags / skill_types / damage_type / conditions / multipliers。`CalcConfig::attack()` 是常用预设。
- **`trace.rs::TraceGraph` + `attribution.rs`** — source-level 归因（PoBR 相对 PoB 的核心增量）。DAG 把每个输出回溯到 `SourceId`；`attribute()` + `AttributionReport::direct()` 给出 direct / marginal / interaction 三种口径。
- **来源接入**：`item.rs::ingest_item`（含 flask/charm 词条分支）/ `item_text.rs::parse_item_text` / `passive.rs::ingest_passive_nodes` / `skill_source.rs::ingest_gem` 把装备/天赋/宝石转为带 `SourceId` 归因的 modifier 注入 ModDb。
- **`calc/`** — 计算编排：
  - `session.rs::CalculationSession` 是高层入口：`new(MinimalInput)` → `add_modifier_texts(...)` / `add_item`/`add_passive_nodes`/`add_gem` → `perform_minimal()` → `MinimalOutput`；`snapshot()` 供归因，`output()` 取完整 `OutputTable`。
  - `perform.rs::perform(env)` 编排 minimal offence + defence，并在末尾 fill 机制阶段（抗性边界/技能时间/伤害向量/异常/EHP/预留·恢复·格挡·抑制）写入 `OutputTable`。env finalize 分阶段接入 buff/flask/charm 等动态来源（部分按 `mode_combat`/`mode_buffs` 门控）。
  - `offence.rs` 算 life/mana/抗性/暴击/命中/DPS（`calculate_minimal` + 归因版 `calculate_minimal_traced`）。`defence.rs` 算 armour/evasion/ES、命中率 `accuracy/(accuracy+(evasion/4)^0.8)`、护甲减伤 `armour/(armour+10*raw_hit)`。
  - 机制模块：`stat_boundary.rs`（抗性边界）、`skill_use_time.rs`（speed bucket + action speed + 服务器帧 cap + channelling）、`damage.rs`（伤害分桶/转换归一化/gain-as-extra/double-dip）、`ailment.rs`（异常 magnitude）、`ehp.rs`（max hit + EHP）、`survivability.rs`（预留/恢复/格挡/法术抑制）、buff_pass（aura 乘区 + curse priority/limit）。
  - `display_catalog.rs`（在 pobr-core 根）—— 强类型展示字段目录 + `extract_display_values(&OutputTable)`，对应 PoB `BuildDisplayStats`。
- **过渡用 feature gate**：`buff-pass-aura`（pobr-core，pobr-build 转发）——M3 双计防护开关，默认关；行为切换 commit 落地后会删除。同类"先接线零行为、feature/mode 门控、最后单 commit 翻开关"是本仓库行为变更的标准模式。

## Parity 体系（回归基准）

PoB2 兼容是硬回归基准，三层校验互补：

1. **`crates/pobr-build/tests/ninja_parity.rs`** — 遍历 `examples/demo-bd-test/builds/*/`（真实 PoB2 build + `meta.json::player_stats` 黄金数值），零硬编码对比全部职业/技能；`parity_no_regression` 断言聚合命中率不低于基线。防御/属性与 DPS 分列报告，未完成的 offence 管线不会掩盖防御侧信号。
2. **golden / dual-run 套件** — `golden_regression.rs`、`statmap_dual_run.rs`、`config_dualrun.rs`、`defence_panels_golden.rs`、`pob2_parity.rs` 等钉住中间值与配置语义。
3. **`tools/pob2-oracle`** — 需要逐分量定位偏差时，从 vendored PoB2 直接 dump Lua 侧计算分解对照。

`vendor/PathOfBuilding-PoE2/` 是 [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的完整检出（版本见 `vendor/.pob2-version.txt`；gitignore 不入库，`bash .claude/skills/run-pobr/driver.sh vendor` 可按钉定 commit 重新克隆），公式核对直接读本地 Lua（`CalcOffence.lua`/`CalcDefence.lua`/`CalcPerform.lua` 等），不要去网上找。

## 游戏机制资料库（agent-docs/）

`agent-docs/` 是 **PoE2（0.5.0）机制中文参考资料**（伤害类型、抗性、护甲/闪避/ES、暴击、异常状态、伤害-防御计算顺序、宝石、通货等）。

**实现任何机制前的查证顺序**（见 `06-development-workflow.md` §2.1.1）：
1. 先查 `agent-docs/` 对应主题；
2. 对照 vendored PoB-PoE2 Lua 计算实现；
3. 对照官方 patch notes / PoE2 Wiki / PoE2DB / 游戏数据。

`agent-docs/` 是**开发输入资料，不是最终权威**；与一手数据冲突时以可验证来源为准，并直接修正文档（保留来源说明）。注意 PoB 公式多基于 PoE1，与 PoE2 存在差异（如护甲系数 `*5` vs `*10`），文档中已标注。

## 关键约定

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本走 `pobr-i18n`（`en-US`/`zh-TW`）。
- **不可变 / 确定性**：calc 函数对 `Env` 的可变写入集中在 `perform`，并行化只在只读快照阶段展开。
- **Build Code** 走 XML → deflate → URL-safe Base64（`pobr-build::{decode,encode}_pob_code`，已用真实 PoB2 ninja code 验证）；自定义/复制物品需保留原始文本块以便和 PoB2 对比。
- 文档以可执行契约为主；改变 crate 边界、聚合语义、catalog/parity 规则时同步更新 `devs/docs/architecture/*`。
