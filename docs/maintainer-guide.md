# Maintainer guide

> **PoBR** = Path of Building in Rust。目标是把 PathOfBuilding（Lua）的核心计算引擎迁移到 Rust：解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈，并在 PoB 兼容的基础上额外提供 source-level 归因（每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献）。

本文是所有贡献者和代码代理共用的规则：构建与验证、架构、约定和排查流程。[CLAUDE.md](../CLAUDE.md) 整篇导入本文，所以这里只放跨主题、长期有效的规则；具体机制、输出字段和与 PoB2 的差异写在下面的专题文档里，按需打开。[AGENTS.md](../AGENTS.md) 是英文的简短版。

## 文档地图

| 要做的事 | 文档 |
|----------|------|
| 日常命令、缓存、CI、发版、处理外部 PR | [开发与 CI 工作流](development-workflow.md) |
| 常见小改动（一行词条、一个 Calcs 字段、一个机制）改哪些文件、跑哪一个检查 | [改动配方](recipes.md) |
| 词条规则的语法与 overlay 数据 | [contributing-mods.md](contributing-mods.md) |
| 某个机制的游戏出处、实现与假设 | [机制约定](mechanics/README.md) |
| CLI / WASM 输出字段与 Calcs 明细 | [calc-outputs.md](calc-outputs.md) |
| 构筑模型、导入、存档格式、编排不变量与缓存、Web 运行时 | [build-model.md](build-model.md) |
| 仍依赖 PoB2 的数据与已知的有意差异 | [pob2-differences.md](pob2-differences.md) |
| 诊断工具的输出、逐行台账、参考口径与语料 | [diagnostics.md](diagnostics.md) |
| 各导入格式 / Web 界面行为 | [import-formats.md](import-formats.md) / [web-contracts.md](web-contracts.md) |

全部文档的索引和按日期保存的记录见 [docs/README.md](README.md)。

## 构建环境

- 开发命令是根 `mise.toml` 里的 mise task（`mise tasks` 查看），由 `devs/scripts/driver.sh` 实现；没有 mise 时运行 `bash devs/scripts/driver.sh <命令>`。游戏 CLI（`apps/pobr-cli`，二进制 `pobr`）和 WASM 包只含游戏计算能力；新的开发命令加成 mise task，不加进 CLI 或 WASM 接口。
- 每个 worktree 用**自己的 `./target`**。同一 worktree 的会话共用它，Cargo 命令一次一条、前台执行；不同 worktree **禁止**共享可写的 `CARGO_TARGET_DIR`（症状是长时间无输出；stderr 的 `Blocking waiting for file lock` 不要用 `| tail` 吞掉）。看到构建锁先检查已有进程，不叠加 Cargo 命令。
- 新 worktree 先 `mise run warm <已编译的 checkout>` 写时复制它的构建缓存；分析工具设 `POBR_BIN` / `POBR_CACHE_DIR` 复用那份 CLI、台账与 PoB2 结果（见[改动配方](recipes.md#working-in-a-new-worktree)）。
- 根 `Cargo.toml` 的 dev/test profile（第三方依赖 `opt-level = 3`、`debug = "line-tables-only"`、`incremental = false`）是有意的设置。不要为“统一”或“加速”改它，也不要自动 `cargo clean`、改 features / `RUSTFLAGS`：这些都会让缓存失效。原因与临时调试变量见[缓存与磁盘占用](development-workflow.md#缓存与磁盘占用)。
- Rust edition 2024。根 `[workspace.package].version` 是应用发布版本（与 `v0.x` tag 同步）；七个 `crates/` 库各自保留版本，应用发版不改库版本。

## 验证分层

**按改动选检查，通过后结束验证并提交。** 普通本地提交不要求全量门禁，也不固定串联 check → build → clippy → test：测试本身会编译。

| 改动 | 最小相关验证 |
|------|------|
| Rust 局部逻辑 | `mise run verify <crate> <suite> [filter]`（定向测试，再跑 fmt 与该目标的 Clippy） |
| 一条词条规则（`special_mods.json` / `skill_stats.rs`） | 规则写 `examples`，`mise run regen` 校验规则与示例并重生成产物；名字没有读取方时补一条引擎测试。步骤见[改动配方](recipes.md) |
| 计算 / Modifier / parser | 对应集成套件的定向用例；完整构筑的数值（parity、accuracy）由批次门禁统一测量 |
| 跨 crate API / 数据结构 | 相关 crate 测试 + 直接使用者的契约或集成测试；边界无法明确时扩大到 workspace |
| Web TS / React | `pnpm --dir web test <test-file>` + `pnpm --dir web typecheck`；交互改动补相关 Playwright spec |
| Rust → WASM / Web 契约 | Rust 契约测试 + 重建 WASM + 相关真实 WASM / E2E 测试 |
| Worker | `pnpm --dir web test:worker` |
| 验证脚本 | `bash -n <script>` + `python3 devs/scripts/test_workflows.py`；WASM 打包与数据同步脚本用 `node --test web/scripts/package-wasm.test.mjs web/scripts/sync-data.test.mjs` |
| 仅文档 | 审查 diff 与链接；不运行 Cargo / Web 构建 |

- 测试目标用 `mise run targets` 查（不编译）。用 `--test <suite>` / `--lib` 限定编译目标：只给名称 filter 仍可能编译整个 crate 的测试。
- 计算修复必须有能复现错误的断言；不降低 parity 或吻合基线来换取通过。
- 同一代码、依赖、工具链、features、数据下已通过的检查直接复用；只改文档不使代码测试失效。失败后先重跑失败的目标，只有新改动或明确的影响范围才扩大验证。
- **批次门禁**：多个改动（并行代理的分支，或同一分支的多条词条）合并后，由合并者在已编译的 checkout 上运行一次 `mise run gate`：fmt、规则产物漂移、Clippy、各 crate 套件、parity、i18n、Web、accuracy，每一步都运行，最后列出失败项；修复后 `mise run gate --only <步骤>` 只重跑失败项。单个改动不重复这些步骤。
- **完整门禁**（合并 / 发版、工具链 / Cargo features / workspace 依赖变更、影响范围不明的核心改动）优先交给 CI：推送分支后 `mise run ci <remote-ref>`，再核对该提交的结果；发起 CI 不等于通过。PR 与 master 推送自动运行的是轻量检查（[pr.yml](../.github/workflows/pr.yml)），不能替代完整门禁。E2E 只在 `mise run ci <ref> --e2e` 时运行，未被要求时不发起。离线或排查 CI 时可显式 `mise run full`（仅 Rust）。相同代码不重复本地与云端全量门禁。
- Web 纯 TS/CSS 改动复用已构建的 WASM；WASM 及其 Rust 依赖、features、工具链改变时重建。`pnpm build` 已含 typecheck；E2E 前确认 dist 对应当前代码。

## 常用命令

```bash
mise run targets                                  # 测试目标，不编译
mise run verify build skills support_gating::     # 定向测试 + fmt + Clippy
mise run test core parser mod_parser::            # 一个套件 + filter（也接受原始 cargo test 参数）
mise run regen                                    # 改了词条规则后：校验规则与示例、重生成产物
mise run gate [--only 步骤,...]                   # 批次门禁

cargo run -p pobr-cli -- calculate-build --file <code.txt> [--breakdown TotalDPS,Life] [--usage] [--full-dps] [--reference-basis] [--json]
cargo run -p pobr-cli -- decode-code <pob_code>   # 另有 encode-code / parse-mod / explain-mod / parse-item / calculate

mise run triage [组或构筑目录 ...]                 # 数值不对的第一步：全部 worked 构筑对照参考值与钉定 PoB2，跨构筑汇总原因
mise run diagnose <构筑>                           # 一个构筑的导入损耗、参考差异与丢掉的输入
mise run compare <构筑> [Stat ...]                 # 一个构筑与钉定 PoB2 逐 stat 对照（需 vendor 与 LuaJIT）
mise run gaps [--kind unique|passive|...]          # 词条缺口队列；选中一行后 mise run gap "<模板或文本>"
mise run lookup <id 或名字> [--level N]            # 游戏数据里的技能、stat、基底、天赋节点、暗金
mise run accuracy [含 */code.txt 的目录]           # 构筑码对照自带参考值（无需 vendor）
mise run skills:sweep / mise run config:coverage   # 技能与配置项的覆盖
```

各工具输出的含义见 [diagnostics.md](diagnostics.md)；参数、缓存和其余命令（基准、parity 报告、WASM 打包、数据与 vendor 任务）见[开发与 CI 工作流](development-workflow.md)。PoB2 headless oracle 是 `tools/pob2-oracle/run.sh <build.xml>`；从 vendor Lua 抽取 overlay 用 `cargo run -p sync-pob-catalog -- extract-lua --vendor-root "$PWD/vendor/PathOfBuilding-PoE2/src" --out <文件>`（`--vendor-root` 必须是绝对路径，需钉定的 vendor 与 LuaJIT）。

## Workspace 结构

以根 [Cargo.toml](../Cargo.toml)、各 crate 的 `src/lib.rs` 和契约 / 回归测试为准。13 个成员：7 个库、2 个应用、4 个 Rust 工具；`web/` 与 Lua oracle 不在 Cargo workspace 里。

| 成员 | 职责 |
|------|------|
| `crates/pobr-data` | 纯数据定义（入库 JSON schema `catalog.rs`，以及 damage / build_config / display_stat / stat / monster 等域类型），零逻辑、零 I/O，所有 crate 的底层 |
| `crates/pobr-core` | Modifier 解析、存储与聚合，计算引擎，source-level 归因，来源接入；零 I/O |
| `crates/pobr-gamedata` | 运行时数据 loader，数据系统里唯一做文件 I/O 的层：`GameData::new(version_dir)` 按域懒加载，WASM 用 `GameData::from_memory` |
| `crates/pobr-i18n` | 语言包加载、fallback 与显示文本；`en-US`（canonical）+ `zh-TW`，构建期校验并生成内嵌字符串表 |
| `crates/pobr-tree` | 天赋树拓扑、已分配节点的词条收集、范围珠宝、加点编辑（`allocation::AllocationTree::edit`） |
| `crates/pobr-build` | Build 状态、PoB 构筑码编解码、导入识别、`calc_orchestrator/` 编排（`calculate_with_data` / `calculate_skill_metrics`）、诊断与逐行台账、构筑对比；parity 测试的主战场 |
| `crates/pobr-item` | 物品原文的全保真编辑态解析与逆向序列化（`ItemDraft`，BuildRaw 往返）；计算视图由 `pobr-core::item_text` + `ingest_item` 承担 |
| `apps/pobr-cli` | CLI `pobr`：`calculate` / `calculate-build` / `parse-mod` / `explain-mod` / `parse-item` / `decode-code` / `encode-code`，命令逻辑在 lib |
| `apps/pobr-wasm` | `build_api/` 的 JSON 契约；默认 features 可在宿主测试，`wasm` 启用绑定，`diagnostics` 启用计时与 fixture 生成目标 |
| `tools/pobr-data-adapter` | GGG `.dat` 导出 → `data/<版本>/` 入库 JSON；正式再生成使用 `--strict-columns` 并校验版本收据，宝石品质由 `--gem-quality` 生成（见[品质生成说明](../pipeline/gem-quality/README.md)） |
| `tools/sync-pob-catalog` | 从 PoB2 Lua 抽取 catalog 与 overlay（`extract-lua` 等子命令）、parity 检查 |
| `tools/lint-i18n` | 语言包完整性检查 |
| `tools/precompile-mods` | 词条规则离线预编译、语料覆盖与审计（`generated/parsed_mods.json`、`modifier-audit.json`；`--check` 校验运行时合并后的规则集） |
| `tools/pob2-oracle` | 非 workspace 成员（Lua wrapper）：把钉定的 PoB2 引导成 headless，dump 完整计算分解为 JSON |

依赖方向：`pobr-data` 在最底层；`pobr-build` 组合计算、物品、树与数据加载；应用层使用它们。生产计算核心不读文件；测试辅助（`pobr-core` 的 `test-rules`、`pobr-gamedata` 的 `test-pins`）在 feature 后面，只由 dev-dependencies 启用。

其他目录：`web/`（React / TypeScript / Vite；`src/api/` 定义 JSON 边界并默认在 module Worker 中运行 WASM，`public/` 是静态资源、同步数据与 Pages Worker，另有 `e2e/` 与 `worker-tests/`）、`data/`（按版本的 base / overlay / generated / i18n 与共享的 `overlay-common/`，默认版本见 `data/CURRENT`）、`pipeline/`（下载、抽取、再生与版本差异）、`examples/`（回归与参考语料）、`devs/`（开发脚本、`known-differences.json`，以及 `devs/docs/architecture/` 的 18–21 号功能契约）、`agent-docs/`（机制研究资料）。

**数据管线**：GGG `.dat` 导出 →（`pobr-data-adapter`）→ `data/<版本>/*.json`（schema = `pobr-data::catalog`，含 `overlay/` 修正层）→（`pobr-gamedata`）→ 计算。游戏数据的文件访问收口在 `pobr-gamedata`；下载、剪贴板与 HTTP 适配属于应用或工具边界。细节见[数据管线说明](../pipeline/README.md)与[版本升级结构](version-bump-architecture.md)。

## 运行时数据与会话

- 活动数据版本在运行时解析：`POBR_DATA_VERSION` → `POBR_DATA_ROOT/CURRENT`（默认仓库的 `data/`），都缺失时报错；代码和测试不写具体版本号。测试与 PoB2 对照值都针对活动版本：数据升级时用 `examples/demo-bd-test/tools/recapture_golden.py` 与 `recapture_panel_golden.py` 重采，parity 掉格代表引擎与新版 PoB2 的差距，修引擎而不是降基线。仓库只保留活动版本加最近入库的两个（`mise run data:prune`）。
- schema 3 的 `manifest.json` 列出运行时文件与 SHA-256，未列的文件视为不存在；`patch/` 与 `overlay-common/` 是独立覆盖层。`GameData` 首次访问时验证并缓存不可变快照，替换磁盘数据后必须创建新实例。
- `GameData::identity()` 是快照、共享覆盖层与有效 patch 的字节摘要，进入完整计算响应的 `provenance`（`engine_version`、`data_version`、`data_sha256`）。
- `CalculationSession` / `Env` 只执行一次（再次 `perform` 返回 `AlreadyPerformed`）；构筑入口返回只读的 `CompletedCalculationSession`。重算创建新会话，避免再次应用资源转换。
- CLI 与 WASM 结果里的未支持词条、`missing_items`、`item_errors` 是诊断，调用方必须保留，不能当作零效果。

细节见 [build-model.md](build-model.md#数据快照身份与会话)。

## 计算引擎架构

`pobr-core` 按 Modifier 的生命周期分目录（见 [lib.rs](../crates/pobr-core/src/lib.rs)）：

| 目录 | 职责 |
|------|------|
| `model/` | Modifier / ModTag 与求值上下文 `CalcConfig` |
| `parse/` | 文本 → Modifier：`mod_parser/`、`mod_cache.rs`、`apply_range.rs` |
| `rules/` | 规则数据 → Modifier：stat map、特殊词条、配置、buff 解释、`skill_stats.rs`、handler 契约 |
| `ingest/` | 来源接入（item / passive / skill / character / campaign），保留 `SourceId` 归因 |
| `skill_env/` | 技能环境：技能与 stat set、辅助判定、buff spec、召唤物、触发、武器贡献 |
| `aggregate/` | `ModDb`：sum / more / flag / override / list 与带 trace 的查询 |
| `calc/` | `CalculationSession` / `Env` 与 offence、defence、ailment、buff、EHP、恢复等阶段；`perform.rs` 执行各阶段 |
| `attribute/` | TraceGraph 与 direct / marginal / interaction 归因 |
| `display_catalog.rs` | 可展示字段、单位、PoB key 与明细种类 |

数据流：数据 / 文本 → 解析与规则映射 → 来源接入 → ModDb → calc → OutputTable / Breakdown / Trace / Attribution。旧的 `pobr_core::mod_parser`、`mod_db`、`modifier` 等公共路径由 `lib.rs` 重导出保留，不代表源码仍在根目录。

完整 Build 由 `crates/pobr-build/src/calc_orchestrator/` 编排：`prepare.rs` 前置解析，`sources.rs` 来源写入，`inject.rs` 注入，`skill/` 技能解析、buff 与触发，`item/` 武器来源，`full_dps.rs`，`diagnosis.rs`。必须保持的不变量：

- 前置解析返回完整的技能、配置和武器结果，不用默认值占位后分阶段修改。
- 来源写入阶段只持有 `SourceWriter`（不暴露 ModDb 读取）；`finish_sources` 消费它之后才开放读取。保持线性编排，不为每个注入函数增加状态类型。
- 词条关键字统一由 `KeywordFlags::from_name` 解释，不认识的名字必须报错：丢掉关键字的词条会作用于所有技能。
- `CalculationContext` 显式携带 stat-map catalog、观察模式和触发子计算状态，热路径不用线程局部状态；需要映射诊断时用 `calculate_with_data_report`。
- 光环与 buff 只经 `calc/buff_pass.rs`；运行时受 `mode_buffs` 控制，curse / debuff 另受 `mode_effective` 控制。不要重新引入已删除的 `buff-pass-aura` feature 或编排层直接注入光环的通道。
- 对 `Env` 的可变写入集中在 `perform`；并行只在只读快照上展开。
- 逐技能总览与 Full DPS 中的记忆化只能放在 `selection_memo.rs` 的 `SelectionScope` 里：被记忆的函数在注释里列出它读取的字段，debug 构建会比对每次命中与重算的结果。
- `DataCalcCache` 是调用方拥有的 opt-in 缓存，键投影解构每个字段、不用 `..`；新增输入时保持这一点。

属性快照的两个时点、缓存语义等细节见 [build-model.md](build-model.md#编排来源写入与缓存)；单个机制的实现见[机制约定](mechanics/README.md)。

## 与 PoB2 的关系（逐步独立）

方向（项目所有者，2026-10）：PoBR 是独立实现的引擎，不是 PoB2 的移植。PoB2 构筑码的导入导出是要保留的产品功能；它的代码和数据是要逐步减少的依赖。“逐步”是约束的一部分：不做一次性重写，每一步都保持门禁通过。

- 机制以游戏自己的定义为依据：`data/<版本>/base/` 的 stat id、技能数据和常量，词条文本，官方 patch notes。代码和测试的出处写这些（例如 `attack_damage_final_permyriad_per_rage = 100`，范例见 `pobr-core/src/calc/rage.rs`）；PoB2 只用于交叉核对。
- 技能 stat 的含义写进 `pobr-core/src/rules/skill_stats.rs`：以游戏 stat id 为键，用引擎自己的 modifier 表达。这张表先于抽取来的 `skill_stat_map.json` 查询；抽取映射拒绝或缺失的 stat（`mise run diagnose` 第 4 节）优先在这里补，不扩充按 PoB2 名称写的白名单。
- 不新增“因为 vendor `X.lua:行号` 这样写”式的注释：行号随 vendor 升级失效，也说明不了游戏规则。改到旧代码时顺手换成规则本身的描述。
- 结果不同时，先按游戏词条和数据判断谁对，不默认 PoB2 正确。PoBR 自有的词条规则声明 `exporter: "absent"`（导出工具没有实现）或 `"differs"`（口径不同），并填 `game_source`；absent 规则在参考口径下自动排除。确认的有意差异写进 [pob2-differences.md](pob2-differences.md)，逐行登记到 `devs/scripts/known-differences.json`。
- 仍从 PoB2 抽取的数据（词条解析规则、技能 stat 映射、配置项、暗金、符文、召唤物映射、stat 描述等）与建议的退出顺序见 [pob2-differences.md](pob2-differences.md#数据来源现状与退出顺序)。因此机制覆盖暂时不会超过 PoB2，版本更新也要等 PoB2 的导出。

`vendor/PathOfBuilding-PoE2/` 是钉定 commit 的完整检出（gitignore；`mise run vendor` 按钉定 commit 克隆，`mise run vendor:lua "<pattern>"` 搜索）。核对公式时读本地 Lua，不去网上找。

## Parity 体系（回归基准）

PoB2 对照是回归基准，不是真值。三层互补：

1. `crates/pobr-build/tests/parity/ninja_parity.rs` 遍历 `examples/demo-bd-test/builds/*/`（真实 PoB2 构筑 + `meta.json` 的对照值），`parity_no_regression` 断言命中率不低于基线；防御 / 属性与 DPS 分列报告。
2. golden / dual-run 套件（`tests/parity/` 的 golden 与 PoB2 对照，`tests/dualrun/` 的 statmap / config）钉住中间值与配置语义。
3. `tools/pob2-oracle` 逐分量 dump PoB2 的计算分解，供 `compare` 与 `triage` 使用。

- 与构筑码参考值比较的入口统一用 `DataOrchestratorOptions::reference_conditions()` = `against_reference_enemy()`（有效口径、Pinnacle 首领、引擎模型全开）+ `on_reference_basis()`（关掉导出工具没有的模型）。新增导出工具没有的**模型**时把开关加进 `on_reference_basis()`；单条**词条规则**用 `exporter` 字段，不逐构筑降基线。模型本身用游戏数值的测试验证，不靠参考值。
- 语料：`examples/demo-bd-test/builds/`（2026-06 抓取，parity 门禁）；`examples/wasm-benchmark/builds/`（8 个）与 `examples/reference-builds/tuned/`（找差异、修引擎）；`examples/reference-builds/holdout/`（**只测量**：不对它运行 `triage` / `diagnose` / `compare`，不按它的差异挑选要修的内容）。`ninja-top10-2026-10-26/`、`batch-2026-10-27/` 等额外批次不在默认门禁里，见 [reference-builds](../examples/reference-builds/README.md)。
- `reference_agreement_does_not_regress` 断言每组的吻合构筑数与行数不低于 `reference_report.rs` 记录的基线。改进了就上调基线；只有参考值被游戏数据证明有误时才下降，并在常量旁写明构筑、格子与依据。门禁防退步，不证明正确。
- 依赖本地 vendor 的对照可能跳过，要如实记录，不能把跳过当作公式已验证。

集成测试目标：`pobr-build` 是 `codec`、`dualrun`、`parity`、`skills`、`tree_convert`；`pobr-core` 是 `aggregation`、`ailments`、`defence`、`engine`、`golden`、`offence`、`parser`、`sources`。子目录里的文件是这些目标的模块，`--test` 用汇总目标名。参考口径的细节见 [diagnostics.md](diagnostics.md#参考口径导出工具规则与语料)。

## 排查数值差异（诊断闭环）

### 静态模拟的范围与取舍

PoBR 主要计算静态构筑及选定条件下的结果，不追求还原游戏中的战斗时间线。词条在游戏中有作用，不代表模拟器需要实现它。**实现或派发任何词条功能前，先评估它是否符合需求**：在目标角色、技能、条件和模拟假设下，它影响哪个输出，解决什么实际错误或用户需求，收益是否值得增加的实现与维护复杂度。说不出实际用途时，不因为词条尚未支持就立项；优先修复影响目标结果的错误，沿用现有计算路径做最小修改。

- 判断依据是本次计算的目标及其假设，不是固定的词条名称或触发类型名单。在选定条件下能影响结果的效果按该条件计算；不适用的保持不生效，不为证明实现有用而虚构事件频率、触发历史或覆盖率。猎手偷取怪物词缀、移动回蓝、拾取魔力残片回魔是静态计算中通常无需追踪的事件收益，例子并不穷尽。
- 缺口队列是诊断盘点，不是必须清零的待办。出现次数多、尚未解析或没有读取方，本身不足以立项；不为降低缺口数量新增模型、配置项、展示字段或通用抽象，也不做没有测量依据的性能优化。
- 保留原始词条和准确的诊断，范围内但尚未实现的机制可以暂缓。本次不模拟某种事件，不等于该词条在所有计算中无效果，不能据此标成 `no_effect`；只有确认不影响任何受支持输出的内容才用 `no_effect` / S0。

### 流程

先批量、后单个（完整步骤见 [AGENTS.md](../AGENTS.md#accuracy-workflow)，各工具输出见 [diagnostics.md](diagnostics.md)）：

1. `mise run triage` 一次计算基准组与 tuned 组的全部构筑和钉定 PoB2，把不一致的行分为 `engine` / `both`（引擎要修）、`reference`（参考值来自另一版导出工具）和 `known`（已登记的有意差异），并跨构筑汇总线索与计算丢掉的输入。按涉及构筑最多的原因成批修复，整批修完再跑一次（PoB2 一侧有缓存）。
2. 找机制缺口用 `mise run gaps`，选中一行后用 `mise run gap "<文本>"` 看 stat、完整来源文本与逐构筑去向。
3. triage 解释不了的行才单独看：`mise run diagnose <构筑>`、`calculate-build --breakdown <stats> --usage`、`mise run compare <构筑> [Stat ...]`。
4. 参考值和 PoB2 都不是真值：按游戏词条和数据判断改哪一边（见上一节）。
5. 同一类行反复需要逐个人工排查，说明工具缺了输出：扩充 triage 或诊断，让下一轮批量运行直接给出这类线索。

### 输入不能悄悄丢失

一个输入可能在导入、文本解析、宝石 stat 映射、ModDb 读取四处丢失，每一处都必须留下记录（`ImportReport`、`unsupported_modifier_texts`、`Diagnosis` 的各项、逐行台账 `pobr_build::usage::line_usage`），而不是让数值悄悄偏小。写代码时：

- 按名字消费 modifier 用 `ModDb::named(name)` 或聚合查询，不用 `iter_mods().filter(..)`：后者不算读取，诊断会误报“没有读取方”。在 ModDb 之外消费的名字登记进 `diagnosis.rs::READ_OUTSIDE_THE_DATABASE`；只在某个状态下读取的登记进 `READ_UNDER_A_STATE`，并在 `tests/skills/line_usage.rs` 用状态关闭／开启的构筑验证。
- 在 ModDb 之外拿走或丢弃词条行（剥离本地词条、按文本读取）时调用 `SourceWriter::note_line` 写明读取方。
- 由计算阶段或编排设置的新条件登记进 `calc/session.rs::CONDITIONS_SET_BY_A_PASS` 或 `diagnosis.rs::SET_BY_THE_ORCHESTRATION`，否则其他构筑会误报它。
- 宝石等级与品质加成按文本读取（`skill::resolve::gem_property_bonuses`）；新增一条文本通道时同时接入它。
- 只读懂一部分的行（`Parsed` 但有 `unparsed` 余文）不应用，按原文记为未解析；只是显示残留的余文在规则里修。
- 不影响任何输出的词条登记为 `"batch": "S0"`；识别了但机制未建模的登记为 `"batch": "unmodelled"` 并写明缺的模型。不能用 S0 掩盖未实现的机制。

## 游戏机制资料与查证顺序

实现任何机制前按顺序查证：

1. 游戏数据：`data/<版本>/base/` 的 stat、技能数据与常量，词条文本，官方 patch notes（`mise run lookup` 直接查）；
2. `agent-docs/`（PoE2 0.5.0 机制中文资料）与 PoE2 Wiki / PoE2DB；
3. vendored PoB2 Lua，用于交叉核对；结论不同时按第 1 项判断。

`agent-docs/` 是开发输入资料，不是权威；与一手数据冲突时以可验证来源为准，并直接修正文档（保留来源说明）。PoB 公式多基于 PoE1，与 PoE2 有差异（例如护甲系数 `*5` vs `*10`），文档中已标注。

## 关键约定

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本走 `pobr-i18n`。代码与注释用英文。
- **确定性**：同一输入的结果逐字节相同；不依赖 HashMap 遍历顺序之类的偶然顺序。
- **保留原文与兼容**：Build Code 走 XML → deflate → URL-safe Base64；导入的物品原文、未知配置、扩展属性原样保留，JSON 契约保持兼容。WASM 握手版本 `pobr_wasm::SCHEMA_VERSION` 与 Web 的 `EXPECTED_SCHEMA_VERSION` 必须一起改。
- **发布产物只含游戏计算能力**：`POBR_DBG_*` / `POBR_GATE_DENY` 诊断开关（`dbg_env!`）只在调试构建存在；测试辅助在 feature 后面。新增诊断或测试辅助沿用这两种方式。
- **功能覆盖分三层**：引擎能计算、JSON / CLI 能输出、玩家能在 Web 中完成操作。已有输出字段不代表 Calcs 有分解，已有配置控件或 handler 不代表机制已生效。
- **新机制的写法**：游戏出处写进代码与测试；没有游戏内实测的假设明确标出；不推算没有输入的事件频率或覆盖率；约定写进[机制约定](mechanics/README.md)。

## 文档维护

- 本文只放跨主题、长期有效的规则。新的机制约定写进 [mechanics/](mechanics/README.md) 的对应小节，输出字段写进 [calc-outputs.md](calc-outputs.md)，构筑模型与接口写进 [build-model.md](build-model.md)，与 PoB2 的差异写进 [pob2-differences.md](pob2-differences.md)；不要把单个机制的段落追加到本文。
- 改变 crate 边界、聚合语义、catalog / parity 规则或验证流程时，同步更新本文、[AGENTS.md](../AGENTS.md) 和受影响的契约文档；Web 行为变化同步 [web-contracts.md](web-contracts.md)。
- 被代码取代的叙述直接改掉或删除；当时的调查和测量写成[按日期保存的记录](README.md#dated-records)，之后不再更新。
