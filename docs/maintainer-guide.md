<a id="maintainer-guide"></a>

# 维护者指南

**PoBR** 是 Path of Building in Rust：用 Rust 独立实现构筑计算，保留 PoB 构筑码兼容。
引擎需要高效汇总大量 Modifier（词条的结构化表示）、计算多个技能，
并保留装备、词条、天赋、宝石和配置等来源，供结果明细与来源贡献分析使用。

本文是所有贡献者和代码代理共用的规则：构建与验证、架构、约定和排查流程。[CLAUDE.md](../CLAUDE.md) 整篇导入本文，所以这里只放跨主题、长期有效的规则；具体机制、输出字段和与 PoB2 的差异写在下面的专题文档里，按需打开。[AGENTS.md](../AGENTS.md) 是给其他代理工具的英文摘要：只列硬性规则并链接本文，两者不一致时以本文为准。

## 文档地图

| 要做的事 | 文档 |
|----------|------|
| 日常命令、缓存、CI、合并与发版机制、处理外部 PR | [开发与 CI 工作流](development-workflow.md) |
| 换数据、发版上线与回退的步骤 | [RELEASING.md](../RELEASING.md) |
| 常见小改动（一行词条、一个 Calcs 字段、一个机制）改哪些文件、跑哪一个检查 | [改动配方](recipes.md) |
| 词条规则的语法与 overlay 数据 | [contributing-mods.md](contributing-mods.md) |
| 某个机制的游戏出处、实现与假设 | [机制约定](mechanics/README.md) |
| CLI / WASM 输出字段与 Calcs 明细 | [calc-outputs.md](calc-outputs.md) |
| 构筑模型、导入、存档格式、计算步骤与缓存、Web 运行时 | [build-model.md](build-model.md) |
| 数据来源、三仓库职责、存储边界与更新流程 | [数据架构](data-architecture.md) |
| 当前数据锁、各领域缺口与近期顺序 | [数据现状](data-status.md) |
| 仍依赖 PoB2 的数据与已知的有意差异 | [pob2-differences.md](pob2-differences.md) |
| 诊断工具的输出、逐行台账、参考口径与语料 | [diagnostics.md](diagnostics.md) |
| 各导入格式 / Web 界面行为 | [import-formats.md](import-formats.md) / [web-contracts.md](web-contracts.md) |

按使用、集成与开发任务组织的入口见 [docs/README.md](README.md)。

## 构建环境

- 本地与 agent 统一经 mise 安装和执行环境；首次 `mise bootstrap --only packages,tools,task`，日常用 `mise run` / `mise exec --`。CI 可用最新稳定的原生安装 Action，读取同一份工具版本。开发命令是根 `mise.toml` 里的 mise task（`mise tasks` 查看），由 `devs/scripts/driver.sh` 实现。游戏 CLI（`apps/pobr-cli`，二进制 `pobr`）和 WASM 包只含游戏计算能力；新的开发命令加成 mise task，不加进 CLI 或 WASM 接口。
- 每个 worktree 用**自己的 `./target`**。同一 worktree 的会话共用它，Cargo 命令一次一条、前台执行；不同 worktree **禁止**共享可写的 `CARGO_TARGET_DIR`（症状是长时间无输出；stderr 的 `Blocking waiting for file lock` 不要用 `| tail` 吞掉）。看到构建锁先检查已有进程，不叠加 Cargo 命令。
- 新 worktree 需要本地编译时，用 `mise run warm <已编译的 checkout>` 克隆缓存，文件修改时才复制数据块。分析工具设 `POBR_BIN` / `POBR_CACHE_DIR` 复用那份 CLI、台账与 PoB2 结果（见[改动配方](recipes.md#working-in-a-new-worktree)）。
- 新增构建缓存、大批构建或排查磁盘不足时，用 `mise run cache` 与 `mise exec -- df -h .` 确认占用和剩余空间。其他 worktree、全局工具与包存储需另计；按[存储决策](development-workflow.md#storage-decisions)复用和清理，不把清缓存当作环境准备或验证步骤。
- 根 `Cargo.toml` 的 dev/test profile（第三方依赖 `opt-level = 3`、`debug = "line-tables-only"`、`incremental = false`）是有意的设置。不要为“统一”或“加速”改它，也不要自动 `cargo clean`、改 features / `RUSTFLAGS`：这些都会让缓存失效。原因与临时调试变量见[缓存与磁盘占用](development-workflow.md#缓存与磁盘占用)。
- Rust edition 2024。根 `[workspace.package].version` 是应用发布版本（与 `v0.x` tag 同步）；七个 `crates/` 库各自保留版本，应用发版不改库版本。

<a id="验证分层本地提交默认定向验证"></a>

## 验证分层

**按风险、收益和成本选验证，不把本地编译作为每个提交的前提。**
文档只审查 diff、链接与示例；算法、大幅改动或跨边界行为可提前运行定向测试、编译或独立 review。
子 Agent 交付改动、必要的回归用例、已审阅/已执行证据和未运行项；协调者汇总后统一验收。
`cargo check` 仍会编译依赖、宏和 build scripts，`test --no-run` 仍生成测试程序，不能把它们视为零成本。

下表是相关行为最终需要的验收范围，不要求每个 worker 在本地执行整行：

| 改动 | 相关验收证据 |
|------|------|
| Rust 局部逻辑 / 算法 | 代码与调用方审阅、能复现变化的回归断言；需要时 `mise run verify <crate> <suite> [filter]`；合并前须有绿色的 `mise run ci <branch>` |
| 词条规则 | 填写游戏来源与 `examples`；协调者整批运行一次 `mise run regen`，解析覆盖率上升时同步抬高 `devs/ci/parse-coverage-baseline.json` |
| 计算 / Modifier / parser | 定向机制用例；合并前须有绿色的 `mise run ci <branch>`（含 parity / reference 基线）；调查性 accuracy/triage 按批次另选 |
| 跨 crate API / 数据结构 | 直接使用者的契约与回归用例；合并前须有绿色的 `mise run ci <branch>` |
| Web TS / React / Worker | typecheck、相关 Vitest / Worker 测试；交互改动运行本地 [`mise run e2e`](development-workflow.md#浏览器测试)（`@smoke` 加受影响的 spec） |
| Rust → WASM / Web 契约 | 完整 CI 重建 WASM、真实 WASM 契约与打包计算；交互影响用本地 `mise run e2e` |
| 数据锁 bump | 审阅 `data:bump` 打印的身份差异；合并前须有绿色的 `mise run ci <branch>`（锁变化时含冒烟 E2E） |
| 验证脚本 | `mise run check:workflows`（shell/Actions/Python/Node，不编译 Rust）；冷依赖场景验证安装顺序 |
| 仅文档 | diff、链接与示例审阅；不运行产品构建，不额外创建固定 reviewer |

- 需要本地测试时，用 `mise run targets` 查目标（不编译），再用 `--test <suite>` / `--lib` 限定编译。名称 filter 不会减少该测试目标的编译量；不固定串联 check → build → Clippy → test。
- 行为修复保留回归断言和游戏依据，不降低 parity/吻合基线换取通过。用例可交接为未运行，但不能省略验证责任或虚报结果。
- 同一代码、依赖、工具链、features 和数据下的通过证据可复用。先修复并重跑失败的相关项，仅在影响范围改变时扩大验证。
- 读码、review、`cargo check` 与运行时测试分别报告，不能互相冒充；通用 skill 的检查清单不覆盖本节。
- **集中集成**：协调者先合并 worker 的变更与生成产物，再选一次批次验证，优先 `mise run ci <pushed-ref>`。`mise run gate` 保留为 warm checkout 上的显式本地方案，含额外 accuracy 报告；不要求与远端完整 CI 重复执行。修复后可用 `--only` 重跑本地失败步骤。
- **CI 分层**：PR 按整个变更范围运行轻量检查；master 代码改动、手动、nightly 与 tag 运行完整 CI。Rust、data 或 pipeline 改动合并前须有绿色的 `mise run ci <分支>`；其他改动按风险选择。浏览器测试只在本地分层运行（`mise run e2e <层级>`，每次发版前运行完整的 `release` 层，见[浏览器测试](development-workflow.md#浏览器测试)）；云端只在数据锁改变时由完整 CI 加跑一个冒烟 spec。`PR gate` 只证明应运行的轻量检查通过，不能代替完整集成结论。推送前的本地检查、选路、分片与缓存复用见[开发工作流](development-workflow.md#ci-的责任与触发)。
- 上述合并前完整 CI 要求是 review 规则，`pr-ready` 不强制执行。
- **推送前检查**：执行 push 的协调者在推送前运行 `mise run check:pr`（opt-in hook 与跳过方法见[开发工作流](development-workflow.md#日常命令)）；只在本地提交的 worker 不必运行。它是 push 前按变更选区的检查，不改变上面"不固定串联"的 worker 规则，也不代替合并前的完整 CI。
- **Review 按风险选择**：复杂、跨边界或高风险改动（核心计算、持久化／导入兼容、工具链、发布）使用独立 reviewer，说明检查过的 head/base 与未验证范围，建议与阻断问题分开；文档和明显小改动不增加审计表或固定 agent。修正留在同一 PR，新提交后重看受影响的证据。合并前的核对（`mise run pr-ready`）见[开发工作流](development-workflow.md#agent-提交review-与合并)。
- Web 纯 TS/CSS 修改复用已构建的 WASM。WASM/Rust/features/工具链变化时再重建；`mise run web build` 已含 typecheck，E2E 使用对应代码的 dist/WASM/data。

## 常用命令

```bash
mise run check:pr                                 # 推送前按变更选区检查；hook 设置见开发工作流
mise run targets                                  # 测试目标，不编译
mise run verify build skills support_gating::     # 定向测试 + fmt + Clippy
mise run test core parser mod_parser::            # 一个套件 + filter（也接受原始 cargo test 参数）
mise run regen                                    # 改了词条规则后：校验规则与示例、重生成产物
mise run data:prepare && mise run data:check      # 导入固定 owner 数据；离线检查数据归属与工作树干净
mise run data:bump <owner> <tag> [<owner> <tag>]  # 换数据：锁改到 owner 的 Release 并导入，打印身份差异（步骤见 RELEASING.md）
mise run gate [--only 步骤,...]                   # 批次门禁

mise exec -- cargo run -p pobr-cli -- calculate-build --file <code.txt> [--breakdown TotalDPS,Life] [--usage] [--full-dps] [--reference-basis] [--json]
mise exec -- cargo run -p pobr-cli -- decode-code <pob_code>   # 另有 encode-code / parse-mod / explain-mod / parse-item / calculate

mise run triage [组或构筑目录 ...]                 # 数值不对的第一步：全部参与修复的构筑对照参考值与固定 PoB2，跨构筑汇总原因
mise run diagnose <构筑>                           # 一个构筑的导入损耗、参考差异与丢掉的输入
mise run compare <构筑> [Stat ...]                 # 一个构筑与固定 PoB2 逐 stat 对照（需 vendor 与 LuaJIT）
mise run gaps [--kind unique|passive|...]          # 词条缺口队列；选中一行后 mise run gap "<模板或文本>"
mise run lookup <id 或名字> [--level N]            # 游戏数据里的技能、stat、基底、天赋节点、暗金
mise run accuracy [含 */code.txt 的目录]           # 构筑码对照自带参考值（无需 vendor）
mise run skills:sweep / mise run config:coverage   # 技能与配置项的覆盖
mise run audit:display [--release <tag>] <构筑集>  # 不开浏览器看网页会显示什么：报错、名字里的原始 ID、未翻译与未支持的行
```

各工具输出的含义见 [diagnostics.md](diagnostics.md)；参数、缓存和其余命令（基准、parity 报告、WASM 打包、数据与 vendor 任务）见[开发与 CI 工作流](development-workflow.md)。PoB2 oracle（无界面运行并输出计算结果的参考工具）是 `mise exec -- bash tools/pob2-oracle/run.sh <build.xml>`；词条规则与 PoB2 parser 的对照审计是 `mise exec -- bash pipeline/diagnostics/modifier-audit.sh [--offline]`，只报告、不改写规则。

## 并行 Agent、提交与 PR

- 并行写入者各用独立的 checkout／branch（worktree）；只读助手可以共用一个稳定版本。**不用 `git stash`**：所有 worktree 共用同一个 stash。
- 被委派的 worker 只在本地提交；集成与经授权的 push／PR 由协调者负责。追加型冲突（catalog 条目、标签、规则列表）保留两侧并检查接线；生成文件重新生成，不手工合并；规则批次集成后统一 `mise run regen` 一次。
- 并发 agent 尽量修改不相交的文件。两个 agent 添加同一个显示 id 或规则是语义冲突，退回给后到的一方。完成时交代审阅／运行了什么、留给集成的是什么。
- 提交标题沿用现有风格（如 `fix: preserve skill selection`、`docs(pipeline): …`），一次提交一件事。PR 说明问题、结果行为、实际验证与剩余限制，链接相关 issue，可见 UI 改动附截图。push、PR 更新、合并与发版只在授权范围内进行；外部贡献的要求见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

## Workspace 结构

以根 [Cargo.toml](../Cargo.toml)、各 crate 的 `src/lib.rs` 和契约 / 回归测试为准。11 个成员：7 个库、2 个应用、2 个 Rust 工具；`web/` 与 Lua oracle 不在 Cargo workspace 里。把 RePoE 事实转换成运行 JSON 的 `pobr-data-adapter` 及其领域政策在 [ackness/repoe](https://github.com/ackness/repoe/tree/master/tools/pobr-data-adapter)，经不可移动的 tag `contract/<n>.<m>` 依赖本仓库的数据契约 `pobr-data`。改动数据包类型、加载解释或数据仓库调用的 `pobr-data` 代码前，先读[数据契约版本](data-architecture.md#31-数据契约版本)：何时把 `CONTRACT_VERSION` 加 1、何时打 tag 只在那里规定；只改引擎不需要 tag。

| 成员 | 职责 |
|------|------|
| `crates/pobr-data` | 纯数据定义（入库 JSON 格式在 `catalog/`，另有 damage / build_config / display_stat / stat / monster 等类型，以及数据仓库 adapter 也用的纯文本工具 `text`），不含计算或 I/O，是所有 crate 的底层 |
| `crates/pobr-core` | Modifier 解析、存储、汇总、计算与来源归因；不做 I/O |
| `crates/pobr-gamedata` | 运行时数据加载器；计算所需游戏数据的文件 I/O 集中在此：`GameData::new(version_dir)` 按域懒加载，WASM 用 `GameData::from_memory` |
| `crates/pobr-i18n` | 与语言无关的游戏文本引擎：`LocalePack` 读入一种语言的翻译库语言包（调用方传入内容，不做 I/O），回退链由语言包 `_meta.json` 的 `fallback` 声明；`lines::LineParser`（目标语言词条行 → 英文规范行）与 `LineRenderer`（英文 → 目标语言显示）按语言包模板匹配，是否为目标语言也由模板与名词判断；只依赖 `pobr-data` |
| `crates/pobr-tree` | 天赋树拓扑、已分配节点的词条收集、范围珠宝、加点编辑（`allocation::AllocationTree::edit`） |
| `crates/pobr-build` | Build 状态、PoB 构筑码编解码、导入识别、`calc_orchestrator/` 编排（`calculate_with_data` / `calculate_skill_metrics`）、诊断与逐行台账、构筑对比；parity 测试的主战场 |
| `crates/pobr-item` | 保留物品原文的编辑模型与序列化（`ItemDraft`，支持 BuildRaw 往返）；计算视图由 `pobr-core::item_text` + `ingest_item` 承担 |
| `apps/pobr-cli` | CLI `pobr`：`calculate` / `calculate-build` / `parse-mod` / `explain-mod` / `parse-item` / `decode-code` / `encode-code`，全局 `--lang <code>` 按数据包语言包读入与显示词条行；命令逻辑在 lib |
| `apps/pobr-wasm` | `build_api/` 的 JSON 契约；默认 features 可在宿主测试，`wasm` 启用绑定，`diagnostics` 启用计时与 fixture 生成目标 |
| `tools/sync-pob-catalog` | PoB2 显示／输出 catalog 的扫描与对照（`scan` / `check` / `diff` / `fixture-check`），只作测试参考，不生成数据 |
| `tools/precompile-mods` | 词条规则离线预编译、语料覆盖与审计（`generated/parsed_mods.json`、`modifier-audit.json`；`--check` 校验运行时合并后的规则集） |
| `tools/pob2-oracle` | 非 workspace 成员（Lua wrapper）：无界面启动固定版本 PoB2，把完整计算分解输出为 JSON |

依赖方向：`pobr-data` 在最底层；`pobr-build` 组合计算、物品、树与数据加载；应用层使用它们。生产计算核心不读文件；测试辅助（`pobr-core` 的 `test-rules`、`pobr-gamedata` 的 `test-pins`、`pobr-data` 的 `test-fixtures`）在 feature 后面，只由 dev-dependencies 启用。

其他目录：`web/`（React / TypeScript / Vite；`src/api/` 定义 JSON 边界并默认在 module Worker 中运行 WASM，`public/` 是静态资源、同步数据与 Pages Worker，另有 `e2e/` 与 `worker-tests/`）、`data/`（Git 只跟踪 `CURRENT` 与两份自有规则；版本目录和七份外部 shared 输入按 `pipeline/data.lock.json` 临时准备）、`pipeline/`（固定输入声明、导入、组包、候选与诊断）、`examples/`（回归与参考语料）、`devs/`（开发脚本、CI 基线与 fixtures；差异登记在 `scripts/known-differences.json`）、`agent-docs/`（被代码引用的早期机制研究）。当前行为契约集中在 `docs/`。

**数据管线**：游戏与语言数据只来自 RePoE 与翻译库的不可变 Release，按唯一数据锁 `pipeline/data.lock.json` 临时导入（`mise run data:prepare`，来源缺失或 pin 不符即失败），组成 schema 3 运行包与精简发行包，经 `pobr-gamedata` 进入计算；计算所需的游戏文件访问集中在 `pobr-gamedata`。完整数据不进 PoBR Git，`mise run data:check` 离线确认 `data/` 只跟踪自有文件。缺数据或缺翻译在 owner 仓库修复并发布，不在 PoBR 内补；PoB2 vendor 只作测试参考，PoBR 不从 vendor Lua 或本地原表再生数据。设计见[数据架构](data-architecture.md)，现状见[数据现状](data-status.md)，命令见[数据管线](../pipeline/README.md)，换数据与发版步骤见 [RELEASING.md](../RELEASING.md)。

## 运行时数据与会话

- 活动数据版本在运行时解析：`POBR_DATA_VERSION` → `POBR_DATA_ROOT/CURRENT`（默认仓库的 `data/`），都缺失时报错。普通测试按准备好的 CURRENT 取数据；固定历史对照必须显式绑定对应外部版本锁／manifest 与参考身份，不能给旧 golden 重贴当前版本。运行数据目录是忽略的临时物化物，不在 Git 保留多份完整快照；需要回滚或多版本实测时显式准备独立旧版锁。数据升级导致实际参考变化时，按证据选择 `examples/demo-bd-test/tools/recapture_golden.py`／`recapture_panel_golden.py`；parity 回退先排查引擎、来源和参考口径，不能降低基线换取通过。
- schema 3 的 `manifest.json` 列出运行时文件与 SHA-256，未列的文件视为不存在；`patch/` 与 `overlay-common/` 是独立覆盖层。`GameData` 首次访问时验证并缓存不可变快照，替换磁盘数据后必须创建新实例。
- `GameData::identity()` 用摘要标识快照、共享覆盖层与有效 patch，进入完整计算响应的 `provenance`（`engine_version`、`data_version`、`data_sha256`）。
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

完整 Build 的计算步骤由 `crates/pobr-build/src/calc_orchestrator/` 组织：`prepare.rs` 前置解析，`sources.rs` 来源写入，`inject.rs` 注入，`skill/` 技能解析、buff 与触发，`item/` 武器来源，`full_dps.rs`，`diagnosis.rs`。以下规则必须保持：

- 计算前先解析完整的技能、配置和武器结果，不用默认值占位后再逐步补齐。
- 来源写入阶段只持有 `SourceWriter`（不暴露 ModDb 读取）；`finish_sources` 接收并结束它之后才开放读取。按顺序组织步骤，不为每个注入函数增加状态类型。
- 词条关键字统一由 `KeywordFlags::from_name` 解释，不认识的名字必须报错：丢掉关键字的词条会作用于所有技能。
- `CalculationContext` 显式传递 stat-map catalog、观察模式和触发子计算状态，不靠线程局部变量传递这些上下文；需要映射诊断时用 `calculate_with_data_report`。
- 光环与 buff 只经 `calc/buff_pass.rs`；运行时受 `mode_buffs` 控制，curse / debuff 另受 `mode_effective` 控制。不要重新引入已删除的 `buff-pass-aura` feature 或编排层直接注入光环的通道。
- 对 `Env` 的可变写入集中在 `perform`；并行只在只读快照上展开。
- 逐技能总览与 Full DPS 只能在 `selection_memo.rs` 的 `SelectionScope` 里复用重复计算结果：函数在注释里列出它读取的字段，debug 构建会比对每次命中与重算的结果；一次普通计算与它的触发来源子计算共用同一个作用域（子计算的克隆经 `adopt` 并入，算完即撤销）。
- `DataCalcCache` 由调用方持有并显式启用。构造缓存键时逐字段解构输入，不用 `..`，
  避免新增输入被遗漏；增加字段时仍须保持这一点。

属性快照何时取得、缓存如何复用等细节见 [build-model.md](build-model.md#编排来源写入与缓存)；单个机制的实现见[机制约定](mechanics/README.md)。

## 与 PoB2 的关系（逐步独立）

方向（项目所有者，2026-10）：PoBR 是独立实现的引擎，不是 PoB2 的移植。PoB2 构筑码的导入导出是要保留的产品功能；它的代码和数据是要逐步减少的依赖。“逐步”是约束的一部分：不做一次性重写，每一步都保持门禁通过。

- 机制以游戏自己的定义为依据：`data/<版本>/base/` 的 stat id、技能数据和常量，词条文本，官方 patch notes。代码和测试的出处写这些（例如 `attack_damage_final_permyriad_per_rage = 100`，范例见 `crates/pobr-core/src/calc/rage.rs`）；PoB2 只用于交叉核对。
- 技能 stat 的含义写进 `crates/pobr-core/src/rules/skill_stats.rs`：以游戏 stat id 为键，用引擎自己的 modifier 表达。这张表先于随 owner 包导入、由 PoB2 转换的 `skill_stat_map.json` 查询；抽取映射拒绝或缺失的 stat（`mise run diagnose` 第 4 节）优先在这里补，不扩充按 PoB2 名称写的白名单。
- 不新增“因为 vendor `X.lua:行号` 这样写”式的注释：行号随 vendor 升级失效，也说明不了游戏规则。改到旧代码时顺手换成规则本身的描述。
- 结果不同时，先按游戏词条和数据判断谁对，不默认 PoB2 正确。PoBR 自有的词条规则声明 `exporter: "absent"`（导出工具没有实现）或 `"differs"`（口径不同），并填 `game_source`；absent 规则在参考口径下自动排除。确认的有意差异写进 [pob2-differences.md](pob2-differences.md)，逐行登记到 `devs/scripts/known-differences.json`。
- 普通技能、树、基底、符文、actor 与描述已逐批由目标事实接回原消费者；parser、config、special、部分 stat-map／关系、暗金与兼容字段仍保留固定 PoB2／社区解释。按字段区分自主事实、策展与历史 codec，不把整个已迁域重新归为 PoB2 导出依赖。现存依赖和退出顺序见 [pob2-differences.md](pob2-differences.md#数据来源现状与退出顺序)；自有规则仍以游戏依据和实际输出需求实现。

`vendor/PathOfBuilding-PoE2/` 是固定 commit 的完整检出（gitignore；`mise run vendor` 按 `devs/reference-sources.json` 的独立参考 pin 克隆，`mise run vendor:lua "<pattern>"` 搜索）。核对公式时读本地 Lua，不去网上找。生产数据的来源与参考测试版本分别维护，见[数据管线](../pipeline/README.md)。

## Parity 体系（回归基准）

PoB2 对照是回归基准，不是真值。三层互补：

1. `crates/pobr-build/tests/parity/ninja_parity.rs` 遍历 `examples/demo-bd-test/builds/*/`（真实 PoB2 构筑 + `meta.json` 的对照值），`parity_no_regression` 断言命中率不低于基线；防御 / 属性与 DPS 分列报告。
2. golden / dual-run 套件（`tests/parity/` 的 golden 与 PoB2 对照，`tests/dualrun/` 的 statmap / config）固定中间值与配置含义，防止回归。
3. `tools/pob2-oracle` 逐分量输出 PoB2 的计算分解，供 `compare` 与 `triage` 使用。

- 与构筑码参考值比较的入口统一用 `DataOrchestratorOptions::reference_conditions()` = `against_reference_enemy()`（有效口径、Pinnacle 首领、引擎模型全开）+ `on_reference_basis()`（关掉导出工具没有的模型）。新增导出工具没有的**模型**时把开关加进 `on_reference_basis()`；单条**词条规则**用 `exporter` 字段，不逐构筑降基线。模型本身用游戏数值的测试验证，不靠参考值。
- 语料：`examples/demo-bd-test/builds/`（2026-06 抓取，parity 门禁）；`examples/wasm-benchmark/builds/`（8 个）与 `examples/reference-builds/tuned/`（找差异、修引擎）；`examples/reference-builds/holdout/`（**只测量**：不对它运行 `triage` / `diagnose` / `compare`，不按它的差异挑选要修的内容）。`ninja-top10-2026-10-26/`、`batch-2026-10-27/` 等额外批次不在默认门禁里，见 [reference-builds](../examples/reference-builds/README.md)。
- `reference_agreement_does_not_regress` 断言每组的吻合构筑数与行数不低于 `reference_report.rs` 记录的基线。改进了就上调基线；只有参考值被游戏数据证明有误时才下降，并在常量旁写明构筑、格子与依据。门禁防退步，不证明正确。
- 依赖本地 vendor 的对照可能跳过，要如实记录，不能把跳过当作公式已验证。

集成测试目标：`pobr-build` 是 `codec`、`dualrun`、`parity`、`skills`、`tree_convert`；`pobr-core` 是 `aggregation`、`ailments`、`defence`、`engine`、`golden`、`offence`、`parser`、`sources`。子目录里的文件是这些目标的模块，`--test` 用汇总目标名。参考口径的细节见 [diagnostics.md](diagnostics.md#参考口径导出工具规则与语料)。

## 排查数值差异（诊断闭环）

### 静态模拟的范围与取舍

PoBR 主要计算静态构筑及选定条件下的结果，不追求还原游戏中的战斗时间线。词条在游戏中有作用，不代表模拟器需要实现它。**实现或派发任何词条功能前，先评估它是否符合需求**：在目标角色、技能、条件和模拟假设下，它影响哪个输出，解决什么实际错误或用户需求，收益是否值得增加的实现与维护复杂度。说不出实际用途时，不因为词条尚未支持就立项；优先修复影响目标结果的错误，沿用现有计算路径做最小修改。

- 判断依据是本次计算的目标及其假设，不是固定的词条名称或触发类型名单。在选定条件下能影响结果的效果按该条件计算；不适用的保持不生效，不为证明实现有用而虚构事件频率、触发历史或覆盖率。猎手偷取怪物词缀、移动回蓝、拾取魔力残片回魔是静态计算中通常无需追踪的事件收益，例子并不穷尽。
- 缺口队列是问题清单，不是必须清零的待办。出现次数多、尚未解析或没有读取方，本身不足以立项；不为降低缺口数量新增模型、配置项、展示字段或通用抽象，也不做没有测量依据的性能优化。
- 保留原始词条和准确的诊断，范围内但尚未实现的机制可以暂缓。本次不模拟某种事件，不等于该词条在所有计算中无效果，不能据此标成 `no_effect`；只有确认不影响任何受支持输出的内容才用 `no_effect` / S0。

### 流程

先批量、后单个；各工具的输出见 [diagnostics.md](diagnostics.md)。

1. `mise run triage` 一次计算基准组与 tuned 组的全部构筑和固定版本 PoB2，把不一致的行分为 `engine` / `both`（引擎要修）、`reference`（参考值来自另一版导出工具，不用修）和 `known`（`devs/scripts/known-differences.json` 已登记的有意差异），并跨构筑汇总线索与计算丢掉的输入。holdout 组只用 `mise run accuracy` 测量，不对它 triage 或诊断。
2. 按涉及构筑最多的原因成批修复：triage 的共同线索、未解析行的模板，以及各构筑逐行台账（`calculate-build --usage`）显示计算丢掉的输入——规则认得但没有模型的行、没有读取方的 modifier 名、没人设置的条件、被拒的宝石 stat——不论该行是否有差异。`mise run gaps [--kind unique|passive|mods|...]` 按涉及构筑与来源排序游戏文本缺口，选中一行后用 `mise run gap "<模板或文本>"` 看 stat、完整来源文本、台账去向与相近的引擎规则。按游戏数据和物品原文实现，PoB2 只作交叉核对；并行 agent 写独立的修复并报告审阅／检查证据，协调者集成后选批次验证。
3. 每批验证一次，优先远端完整 CI；需要显式的本地 accuracy／parity 批次时用 `mise run gate`。之后重跑 `mise run triage`（PoB2 一侧有缓存）。改进后上调基线，把原因与证据写进提交或 PR；生成的报告留在本地。
4. triage 解释不了的行才单独看：`mise run diagnose <构筑>`、`mise run compare <构筑> [Stat ...]`、`calculate-build --breakdown <stats> --usage`。同一类行反复需要逐个人工排查，说明工具缺了输出：扩充 triage 或诊断，让下一轮批量运行直接给出这类线索。
5. 参考值和 PoB2 都不是真值：按游戏词条和数据判断改哪一边（见上一节）。游戏文本或数据证明参考值有误时，把有意差异写进 [pob2-differences.md](pob2-differences.md)，并在 `known-differences.json` 登记对应行；PoB2 没有实现的规则只需 `exporter: "absent"`，不需要 known-difference 条目。

### 输入不能悄悄丢失

输入可能在四处丢失：导入、文本解析、宝石 stat 映射、ModDb 读取。每一处都必须留下记录（`ImportReport`、`unsupported_modifier_texts`、`Diagnosis` 的各项、逐行台账 `pobr_build::usage::line_usage`），而不是让数值悄悄偏小。写代码时：

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

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本在显示边界解析：界面文字在 Web，游戏名词与词条译文来自翻译库的语言包。代码与注释用英文。
- **代码风格与测试位置**：Rust edition 2024，rustfmt 四空格；TypeScript 两空格；Rust 函数／模块 `snake_case`，类型与 React 组件 `PascalCase`。Rust 集成测试放在各 crate 的 `tests/`，前端单测 `*.test.ts` 与源码同目录，浏览器测试 `*.spec.ts` 在 `web/e2e/`。
- **确定性**：同一输入的结果逐字节相同；不依赖 HashMap 遍历顺序之类的偶然顺序。
- **保留原文并保持兼容**：Build Code 走 XML → deflate → URL-safe Base64；导入的物品原文、未知配置、扩展属性原样保留，JSON 契约保持兼容。WASM 握手版本 `pobr_wasm::SCHEMA_VERSION` 与 Web 的 `EXPECTED_SCHEMA_VERSION` 必须一起改。
- **发布产物只含游戏计算能力**：`POBR_DBG_*` / `POBR_GATE_DENY` 诊断开关（`dbg_env!`）只在调试构建存在；测试辅助在 feature 后面。新增诊断或测试辅助沿用这两种方式。
- **功能覆盖分三层**：引擎能计算、JSON / CLI 能输出、玩家能在 Web 中完成操作。已有输出字段不代表 Calcs 有分解，已有配置控件或 handler 不代表机制已生效。
- **新机制的写法**：游戏出处写进代码与测试；没有游戏内实测的假设明确标出；不推算没有输入的事件频率或覆盖率；约定写进[机制约定](mechanics/README.md)。

## 文档维护

- 本文只放跨主题、长期有效的规则。新的机制约定写进 [mechanics/](mechanics/README.md) 的对应小节，输出字段写进 [calc-outputs.md](calc-outputs.md)，构筑模型与接口写进 [build-model.md](build-model.md)，与 PoB2 的差异写进 [pob2-differences.md](pob2-differences.md)；不要把单个机制的段落追加到本文。
- 改变 crate 边界、聚合语义、catalog / parity 规则或验证流程时，同步更新本文和受影响的契约文档，硬性规则变化时同步 [AGENTS.md](../AGENTS.md) 的摘要；Web 行为变化同步 [web-contracts.md](web-contracts.md)。
- 每条规则只在一处维护，其他文件链接而不复制：验证分层、约定、诊断闭环与并行 agent 规则在本文；命令、CI、合并与发版机制在[开发工作流](development-workflow.md)；换数据与发版步骤在 [RELEASING.md](../RELEASING.md)；外部贡献要求与修复归属在 [CONTRIBUTING.md](../CONTRIBUTING.md)；三仓库分流在 [workflow skill](../.agents/skills/workflow/SKILL.md)；数据设计在[数据架构](data-architecture.md)，现状在[数据现状](data-status.md)。
- 被代码取代的叙述直接改掉或删除。个人调查、审计交接与机器测量放在忽略的 `study/` 或 `.cache/`，结论写入相应契约、测试或 PR；不把过程日志累加到当前指南里。
- 公开仓库保留能帮助复现和理解项目的文档、样本来源、有效基线与源代码引用。收起历史材料前，先迁移仍有效的约定，并检查跟踪文件中的链接；仅有本地文件存在不代表公开链接有效。
