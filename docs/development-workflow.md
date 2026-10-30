<a id="development-and-ci-workflow"></a>

# 开发与 CI 工作流

开发命令是根 `mise.toml` 中的 mise task：`mise tasks` 列出全部，`mise run <task> [参数]` 执行，
任务名之后的参数原样传递。它们转发到 `devs/scripts/driver.sh`；
`.claude/skills/run-pobr/driver.sh` 仅保留为兼容转发入口。
游戏 CLI（`mise exec -- cargo run -p pobr-cli`，二进制 `pobr`）只提供游戏计算命令，不包含任何开发命令。
本文维护构建、测试、缓存和 CI 操作；验证范围与计算约定见 [maintainer-guide.md](maintainer-guide.md)，
Web 初次准备见 [Web README](../web/README.md)，数据更新见 [pipeline/README.md](../pipeline/README.md)。

## 环境：本地统一 mise，CI 使用原生安装 Action

先按 [mise 官方说明](https://mise.jdx.dev/getting-started.html) 安装/更新工具管理器。
本地和 agent 使用当前稳定版 mise；仓库声明最低兼容版本。
`mise.toml` 精确固定 Node、pnpm、Python、wasm-pack、actionlint 与 gh，
`rust-toolchain.toml` 唯一声明 Rust、fmt/Clippy、wasm target；`mise.lock` 固定下载来源与校验和。
本地命令用 `mise run <任务>`，没有命名任务时用 `mise exec -- <命令>`；
不要由每个 agent 临时安装另一套 runtime。CI 可使用官方或受维护的工具 Action，
由 [setup-tools](../.github/actions/setup-tools/action.yml) 读取同一份版本，CI 不安装 mise。

新环境只需一次：

```bash
mise bootstrap --only packages,tools,task    # 主机依赖 → 精确工具 → 日常依赖，不编译产品
# 兼容的简短入口；已有环境重复执行同样安全。
mise run setup
```

支持当前 Unix 工具链：macOS 与 Debian/Ubuntu。主机包由 mise 原生 bootstrap 声明，
setup 检查 Git、make、C 编译器；macOS 的 Apple SDK/Command Line Tools 仍由系统提供。
工具核验会拒绝 PATH 中遮蔽 mise 版本的旧程序。setup 安装 Web 与 pipeline 的 frozen lockfile，
不下载浏览器、不编译 CLI、不默认获取大型参考数据。

按工作需要自动准备额外依赖：

| 工作 | 入口与自动准备 |
| --- | --- |
| Web 日常开发/测试 | `mise run web <命令>`，例如 `mise run web test <文件>` |
| 数据导出器 | `mise run pipeline test` |
| PoB2 对照、triage、完整数据生成 | 对应任务依赖 `setup:oracle`，自动准备固定 LuaJIT commit 与固定 vendor；重复运行复用缓存 |
| 浏览器回归 | `mise run web:e2e <spec>`，自动安装 lockfile 对应的 Playwright 浏览器/系统依赖；仍须当前 dist/WASM/data |
| 树图资源再生成 | `mise run web:tree-art`，按需由 mise conda backend 安装固定 ImageMagick 7/zstd，自动准备 vendor |

`setup:oracle` 将兼容的 LuaJIT 暴露到 `.cache/runtime/bin`；mise 为任务配置 PATH、
`POBR_LUAJIT` 与 vendor 的 `LUA_PATH`，无需各自修改 shell。
默认安装使用固定上游 commit，保留显式 runtime override 的调试用途。
首次源码构建会自动处理 macOS deployment target；缺依赖的任务失败会给出具体原因。
`mise run status` 只报告工具与数据状态，不下载、不构建。

同一输入下复用已通过检查；只有显式升级工具/依赖时才更新 pins 和 lock。
工具链改动由完整 CI 验证，不把整套产品编译变成日常 setup 前提。

## 日常命令

```bash
mise run targets                              # List current suites without compiling
mise run verify build skills support_gating:: # Selected tests, fmt and Clippy
mise run test core parser mod_parser::        # One integration suite and filter
mise run test item lib                        # Library unit tests
mise run lint build skills                    # fmt and selected Clippy target
mise run build                                # Application CLI only
mise run cache                                # Read-only storage and file counts
mise run regen                                # After a rule edit: regenerate parser artifacts, test the generator
mise run gate                                 # Optional local batch validation on a warm checkout
mise run gate --only core,parity              # Rerun only the failed steps
mise run warm <warm checkout>                 # New worktree: copy-on-write clone of its target/debug
```

基准和手动诊断按需编译，普通测试不编译 Criterion 或五个 WASM 诊断/fixture 生成目标：

```bash
mise exec -- cargo bench -p pobr-core --features benchmarks --bench mod_db_bench
mise exec -- cargo bench -p pobr-build --features benchmarks --bench perform_bench
mise exec -- cargo test -p pobr-wasm --features diagnostics --test perf_timing -- --ignored --nocapture
```

完整 CI 和 `mise run full` 的 Clippy 显式启用这两个 feature，继续检查手动工具；
日常定向测试和 PR 检查不启用。`gen_fixtures` 同样需要 `--features diagnostics`。

小改动的文件与验证选项见[改动配方](recipes.md)。子 Agent 按风险和成本选择代码审阅、
静态检查、定向编译或测试；文档只检查 diff、链接和示例。算法、大幅修改和跨边界行为可以提前测试，
不要求每个 worker 都先编译才能提交。必要的回归断言仍须写入，未运行项明确交接。

协调者先汇总多个 worker 的修改，检查语义冲突，规则变化时集中生成一次产物，再统一验证。
默认依赖远端：PR 轻量、master 合并后全量；高风险批次可先 `mise run ci <pushed-ref>`。
本地 `mise run verify`、`mise run gate` 和 `mise run full` 都保留为按需工具，
不与相同输入的云端结果固定串联。

短参数 `build` 对应 `pobr-build`，`core` 对应 `pobr-core`；完整包名也可用。
`lib` 选择单元测试，其他名字选择 `--test <suite>`。不传目标会显示用法并退出。
需要检查一个 Rust 包或明确执行测试时，例如：

```bash
mise exec -- cargo check --locked -p pobr-core --lib  # Optional; dependencies/macros may still compile
mise run test core parser mod_parser::               # Runtime assertions when useful
mise run verify build skills <filter>                # Tests, fmt and Clippy for one target
```

`cargo check` 不运行测试，`cargo test --no-run` 仍生成测试二进制；名称过滤也会编译整个测试目标。
不要把它们都当作免费检查，或固定串联 check/build/Clippy/test。复用输入未变的证据；
读码、review、编译检查和运行时测试分别说明，不能相互冒充。

Web 纯 TS/CSS 修改复用已有 WASM，按风险选择相关 Vitest、typecheck 或 Playwright。
`mise run web build` 已含 typecheck，普通文档不需要 Web 构建。
自动化修改使用 `mise run check:workflows`，不编译 Rust；安装顺序变化还要验证冷依赖环境。

## 缓存与磁盘占用

本地由 mise 提供工具，CI 由官方或受维护的 Actions 安装；两者读取相同的版本声明，
使用同一份 lockfile 与构建配置，各检查按其既定 profile/features 执行。缓存用于减少下载和编译，
不改变应执行的安装、校验和测试。

<a id="storage-decisions"></a>

### Agent 的存储决策

需要本地构建且要创建新的缓存、运行大批检查，或发现空间不足时，先运行 `mise run cache` 与 `mise exec -- df -h .`。
前者不编译、不删除，按目录定位占用；后者查看所在卷的实际剩余空间。
普通小改动继续使用既有定向检查，不增加存储检查门禁。

- 优先复用当前 worktree 的产物；确需新 worktree 时用下文的写时复制预热，
  不为单项修复另建 target，也不让并行 writer 共用可写 target。
- 工具链、profile、features、target 与编译环境改变会产生不同产物；保持日常设置稳定，
  需要诊断或发布配置时才使用对应任务，不为“加速”临时轮换配置。
- 本 checkout 的报告不含其他 worktree、全局 mise/Rustup 安装、Cargo registry/git、
  pnpm store 或编译器缓存。需要时分别查看；`mise exec -- pnpm store path` 可定位包存储。
  写时复制或硬链接可共享数据块，多个目录的统计不能直接相加为独占空间或预计可释放量。
- 清理前确认目录的用途、仍在运行的构建和保留需求。优先处理已结束任务的临时输出、
  确认废弃的 worktree/构建配置；保留有后续用途的编译缓存、发布产物与用户资料。
  全局 Cargo/pnpm 清理和 `cargo clean` 不作为 setup、测试或空间不足时的自动补救。

### 本地复用

每个 worktree 复用一个默认 `target/`。同一 worktree 的会话共用它、串行运行 Cargo，
不按会话、测试或门禁创建 `target-gate` 等副本；不同 worktree 不共享可写 target。
看到 `Blocking waiting for file lock` 时检查已有进程，不再叠加 Cargo。

新 worktree 的 `target/` 为空，第一次测试会重新编译全部依赖。选择空闲、版本兼容的
已编译 checkout 作为来源，在新 worktree 运行 `mise run warm <已编译的 checkout>`，
用写时复制（APFS clonefile 或 Linux reflink）克隆那份 `target/debug`。开始时复用文件数据块，修改时才占用新块，
两份目录仍可独立写入；Cargo 复用输入未变的依赖，路径或输入改变的包仍可能重编译。已有 `target/debug` 时不覆盖。
它只预热 `target/debug`，不复制 WASM 或 release 目录；文件系统不支持写时复制时停止，
不会回退为整份普通复制。Cargo 仍按实际输入判断哪些产物可复用。
分析工具（`gap`、`gaps`、`triage`、`compare`、`config:coverage`、`skills:sweep`、`output-coverage.py`）
默认会准备 CLI，并把台账与 PoB2 结果缓存在本 checkout 的 `.cache/`。设 `POBR_BIN=<checkout>/target/debug/pobr`
使用已编译的 CLI，设 `POBR_CACHE_DIR=<checkout>/.cache` 共用已有缓存（台账按程序内容、PoB2 结果按构筑码与 vendor 失效）。
`POBR_BIN` 运行的是那份 checkout 的引擎，用于读取游戏数据、台账和 PoB2 结果，不用于测量本 worktree 的改动。

根 `Cargo.toml` 的 dev/test 对第三方依赖使用 `opt-level = 3`，workspace crate 保持未优化。
游戏数据快照加载包含 JSON 解析和 SHA-256 校验，依赖优化有助于降低重复加载成本。
依赖首次编译后可长期复用；日常只改 workspace 源码时，无需重新优化依赖。
`pobr-build --test skills` 的当前快照由套件内一个只读 `LazyLock` 共享，修改数据的测试显式 clone，
合成数据 fixture 保持独立。新增此套件测试复用 `crate::test_data()`，不要重复加载同一版本。
日常测试和完整门禁统一使用 `mise exec -- cargo test`，套件内复用快照，同时覆盖 doctest；无需额外安装测试 runner。
规则生成测试用同两次生成结果检查确定性、覆盖率和已提交 golden。
物品往返测试在同一次语料遍历中检查数量、语义并报告字节差异，不删减原有断言。

根 `Cargo.toml` 的 dev/test 还使用 `debug = "line-tables-only"` 和 `incremental = false`。
前者保留回溯行号，后者停止积累每个 crate 的增量对象图和中间文件。
Cargo 仍按输入指纹复用未变的依赖、库和测试程序；重复相同测试无需重新编译。
修改源码时会重新编译受影响的 crate，关闭增量编译可能增加这一步的耗时。
长期连续编辑同一 crate 时可显式选择 `CARGO_INCREMENTAL=1`，但应保持设置稳定，
避免反复切换产生两组产物。test 继承 dev 配置；WASM 使用独立的 `wasm-release` profile（见下），普通 release 不变。
参见 [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#incremental)
和 [build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)。

首次采用新 profile 时，已有 workspace 产物会重新编译一次。
新设置不会删除此前的 incremental、旧工具链或旧 features 产物。
不要把 `cargo clean` 当构建前置步骤；改变工具链、profile、features、RUSTFLAGS 或 target
都会影响缓存复用。调试变量时可临时使用 `CARGO_PROFILE_DEV_DEBUG=2`，
测试使用 `CARGO_PROFILE_TEST_DEBUG=2`，完成后恢复日常设置。

```bash
mise run cache
mise run web build-wasm                   # Verify and reuse unchanged output
mise run web build-wasm --force           # Rebuild through Cargo, without cleaning
```

`cache` 通过离线 Cargo metadata 解析当前 target/build 目录，统计占用与文件数，
细分 host/WASM profile 的 deps、incremental 等目录；同时统计 `.cache`、Web 依赖、
dist、WASM 包和同步数据。不编译、不删除、不跟随符号链接。
支持文件系统时显示分配块大小，否则显示文件字节数；硬链接、写时复制或压缩可能影响与真实占用的差异。
本地 Cargo 配置和 `CARGO_INCREMENTAL` / `CARGO_PROFILE_*` 可以覆盖仓库默认设置。
全局 Cargo registry/git、pnpm store 和编译器缓存另行存储，未计入本仓库的统计。
已配置 sccache 时用 `sccache --show-stats` 查看位置、命中率、占用及上限；
本仓库沿用已有 wrapper，不安装缓存服务或修改全局设置。

`build-wasm` 使用 `wasm-release`：继承 release 的速度优化，启用 ThinLTO 和单个 codegen unit。
这只影响 WASM 发布构建，不改变日常 dev/test 缓存。固定 locale TOML 在主机构建时解析成字符串表，
TOML parser 不进入 WASM；不要在运行时重新解析这些静态文件。

WASM 构建记录校验 Rust 源码、嵌入语言文件、共享配置 JSON、manifest/lockfile、JSON 契约、
工具版本、相关环境/Cargo 配置，以及绑定哈希与实际 schema。
输入和产物未变时跳过 wasm-pack，保留原编译 commit 与 dirty 标记；
缺失、损坏或旧格式记录需要一次正常重建。普通 UI、文档和运行时游戏数据修改不会触发 WASM 重编译；
`data/overlay-common/*_config_options.json` 是可能内嵌的配置文件族，修改或新增会重新构建，
无需另行登记缓存输入；
游戏数据变化仍需 `mise run web sync-data`。

这些设置减少后续产物，并非磁盘配额或自动清理器。`.cache/` 中历史诊断、vendor 差异副本、
发布包以及旧 worktree 仍可能累积；先确认具体目录及保留需求，再显式清理。
脚本不会自动删除发布资产、用户资料或其他 worktree。

### CI 恢复与跳过

Rust 缓存按 job、平台与工具链等输入隔离；兼容前缀命中后仍由 Cargo 判断需要重编译的内容。
完整 key 命中时不重复保存同一份缓存是正常行为。核查时看恢复 key、命中结果、保存结果和实际构建日志，
不因工作流使用了 cache Action 就认定已经复用。

`setup-node` 恢复 pnpm store，不恢复 `node_modules`。相关 job 仍须运行
`pnpm install --frozen-lockfile`，再执行项目脚本；恢复缓存不能替代依赖安装。
PR merge ref 的缓存供该 PR 后续运行复用，默认分支缓存也可供 PR 恢复；
PR 缓存不会反向成为默认分支或其他 PR 的缓存。

master 选中完整验证时，轻量 Rust/Web/脚本 jobs 按计划跳过；PR 中完整 job 默认跳过。
E2E 默认未选中，显式开启才运行。
缓存命中不会跳过这些正确性检查。更新 PR 后旧 run 的取消可由新 run 接替，
确认结果时始终核对当前 head，以及 gate 是否成功验证了所有应运行的 jobs。

项目专属 [.claude/skills/workflow](../.claude/skills/workflow/SKILL.md) 将本页适配给 agent。
本页与仓库内的 skill 足够完成日常开发，不依赖贡献者的个人 skills 或委派工具。

## CI 的责任与触发

workflow 负责事件、环境、权限；`mise.toml` / `devs/scripts` 负责命令。
本地按实际需要验证，默认不重复远端集成构建。

| 入口 | 触发与职责 | 成功出口 |
| --- | --- | --- |
| [pr.yml](../.github/workflows/pr.yml) | PR 按整个变更范围运行相关轻量检查；master 代码 push 调用完整集成 | `PR gate` |
| [ci.yml](../.github/workflows/ci.yml) | 可复用完整门禁、手动 dispatch、每日 19:00 UTC 的 master 回归 | `Full gate` |
| [release.yml](../.github/workflows/release.yml) | `v*` tag；校验发布身份、完整验证后发布同次产物并按条件部署 | Release / deploy jobs |

PR 比较被测试的 merge commit 与 base parent，包含整个 PR；master 比较 push 的 before/after，
覆盖一次推送的全部提交，重命名按删除和新增两侧处理。PR 无法读取比较基线时快速失败，
不生成绿色空计划；master 首次 push 或缺基线时保守全量。
PR 新提交替换旧运行；master 每个增量 push 独立，后续文档提交不会取消前一次代码验证。

[ci-plan.py](../devs/scripts/ci-plan.py) 是事件和路径选路的唯一实现：

| 变更 | PR 自动检查 | master push |
| --- | --- | --- |
| Rust / 数据 / 构筑输入 | fmt、`cargo check --locked --workspace --all-targets`；数据另含脚本契约 | 完整集成 |
| Web | typecheck、Vitest、Worker；脚本和静态资源另含自动化契约 | 完整集成 |
| 开发脚本 | shell/Actions/Python/Node 契约，不编译 Rust | 完整集成 |
| manifest、lockfile、工具链、工作流、数据管线、共享或未知输入 | 上述全部轻量区域 | 完整集成 |
| 普通文档、模板、说明图片 | 无产品构建，保留稳定 gate；fixture 文本仍视为输入 | 无产品构建，保留稳定 gate |

`PR gate` 要求检测器与每个选中的 job 成功，拒绝失败、取消、缺失与意外跳过。
它在 PR 中证明轻量检查通过；master 中还要求完整 job 成功。
`Full gate` 要求 Rust、Web、自动化全部成功。fork 和 Dependabot 使用同样的只读、无发布密钥路径；
首次外部贡献是否批准执行由 GitHub 设置决定，执行许可不代表正确性批准。

完整 CI 包括 fmt/Clippy（含诊断 features）、workspace tests/doctest、parity/reference 退步断言、
活动数据生成产物的 byte-diff、i18n、数据兼容、Web 类型/单测/Worker、真实 WASM 契约与打包计算。
缺少本地原始导出输入或 vendor 的项目会明确跳过，不能将其描述为已验收。
它不等同于本地 `mise run gate` 的额外 accuracy 报告和全部本地输入。
E2E 只在显式手动输入时运行；交互风险需要时选择相关本地 spec 或云端 E2E。

```bash
mise run check:workflows                   # 自动化契约，不编译 Rust
mise run ci feature/my-change              # 先推送目标提交；只请求检查，不推送代码
mise run ci feature/my-change --e2e        # 显式启用 E2E
mise run ci-status --branch feature/my-change
mise run full                             # 显式本地 Rust 全量检查，不含 Web
```

必须核对 run 对应的 SHA 和最终结果；dispatch 成功不等于检查通过。
计算修复的统计调查由协调者按批次选择；默认完整 CI 已检查 parity/reference 基线，
需要额外 accuracy/triage 报告时再用本地 warm checkout 或相应分析任务。
工作流迁移用 `mise run check:workflows`，无需 Cargo / WASM / E2E。
其 actionlint 1.7.12 尚不认识 GitHub 已支持的 `queue: max`，只豁免该特定未知键错误，
其余语法与表达式检查仍启用；队列语义见 [GitHub 文档](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)。

## Agent 提交、review 与合并

流程服务于可审查的改动与及时反馈，不以表单、agent 数量或 review 轮次衡量质量。

- 普通局部修复：问题/结果说明、必要的回归用例与实际审阅/检查证据；本地执行按需要选择。
  文档或明显机械改动不额外要求回归测试、截图或独立 agent。
- 跨边界、核心计算、持久化/导入兼容、工具链或发布改动：根据实际风险请独立 reviewer
  检查调用方和证据，说明检查过的 head/base 与未验证范围；建议与阻断问题分开。
- 修正在同一 PR 中完成；新提交后重看受影响证据。没有固定审计 JSON、形式上的批准次数、
  每个 PR 必须创建 issue 或必须换 reviewer 等要求。机器人和人工审查都不能替代真实运行结果。

[PR 模板](../.github/pull_request_template.md) 提供 Problem、Change、Validation、Remaining limits
的写作提示，不由 CI 强制正文格式。只写实际执行的命令与结果；界面变化按需要给出前后证据，
机制依据以游戏数据/stat ID 为准。draft 可用于提前获得反馈，不把它当作额外流程阶段。

授权合并前可以运行 `mise run pr-ready <N>`：它只读 PR 当前状态、`PR gate`、其他失败/待定检查
与冲突，并再次核对 head/base，输出带 `--match-head-commit` 的准确命令。它不评论、不合并、
不验证“是否有独立推理”。这是减少误操作的辅助入口；维护者也可以直接核对同样的事实。
`pr-ready` 不要求每个普通 PR 都有合并前 full。若风险需要手动 full，单独核对该 run 的
`head_sha`、结果与当前 base；分支 head 的结果不证明之后变化的 merge commit。
base 更新后的保障取决于已配置的服务端规则和必要复验，不能从本地文件推断 strict 已启用。

## Issue 与自动化维护

Bug 表单收集可复现步骤、版本/数据版本、环境，以及相关构筑码和对照数值；
功能表单问用户目标和能改善的输出，不要求报告者懂内部实现。
构筑输入先删除不愿公开的信息。维护者先查重、确认范围；证据不足时要求具体资料，
确认的缺陷按原因归并，修复只覆盖一部分时用 Refs，不提前关闭整项问题。
没有证据证明有用的事件模拟不自动进入实现，遵守静态模拟范围。

Dependabot 每日分组检查 Actions 更新；Cargo、Rust toolchain、Web 与数据管线每月检查。
当前 Action 使用经核对的最新稳定标签；升级 PR 运行全部轻量检查，master 合并后完整验证，
高风险升级可先手动 full。没有自动合并或按年龄关闭已确认缺陷的机器人。
不为流程形式引入收费 review 服务、贡献准入名单或带真实密钥的 fork 预览。

## 发布与部署

应用版本由 `[workspace.package].version` 管理；七个基础库与 `data/CURRENT` 独立版本。
更新应用版本后用 `mise exec -- cargo metadata --format-version 1 > /dev/null` 刷新锁文件。
[release-policy.py](../devs/scripts/release-policy.py) 按 SemVer 判断升级，检查所有继承 workspace
版本的应用/工具锁版本；远端 tag 查询失败会报错，不能当作“tag 不存在”。
PR 升版要求 tag 未使用；master 的合并后验证可接受指向同一提交的现有 tag。
升版、依赖、工具链 PR 默认运行全部轻量检查；风险需要时在合并前手动 full，
master 合并后和发布 tag 各自执行完整验证。

推送 `v*` tag 后先验证 tag/version/commit/lock 身份，再完整验证该提交；通过后
从同一 run 下载 WASM 资产并校验 SHA256SUMS；上传前再次核对远端 tag 的 peeled SHA，随后附加到 GitHub Release。
已有 Release 重跑会覆盖同名资产，适用于同一不可变 tag 的修复性重试，禁止移动已发布 tag。
带 prerelease 后缀的版本发布为 prerelease。手动 CI 只保存 artifact，不会发布或部署。

生产只接受稳定的 `v0.x.y`。部署等待完整门禁及资产发布成功，直接使用同次 `web-dist`，不重建。
所有生产 jobs 用共享队列串行执行且不取消执行中的部署；获得锁后重新读取远端 tags，
只有最高的稳定 v0 tag 可以部署。旧版晚完成会跳过；同 SHA 重试允许，tag 被移动会拒绝。
这是保守策略：较新稳定 tag 即使 CI 失败，也会阻止较旧版本随后部署。
应修复并发布更新版本，不能依靠迟到的旧 run 隐式回滚；有意回滚另行决定。

验证默认只有 `contents: read`；GitHub Release job 单独获取 `contents: write`，
Cloudflare 凭据只交给最终 deploy action。构建、安装、测试不接触发布凭据。
Linux WASM 继续使用 `BINARYEN_CORES=1`，保留优化步骤；原因见 [Binaryen #7570](https://github.com/WebAssembly/binaryen/issues/7570)。
WASM 包身份与使用方式见 [WASM 打包说明](wasm-package.md)。

## GitHub 服务端启用

本地 YAML 提供检查，不会自动配置合并保护。2026-10-30 只读观察中，master 没有 required checks/reviews，
rulesets 为空。发布这套 workflow 并确认首次真实 PR 运行后，再在 master protection/ruleset 中：

- 要求 PR 合并、禁止删除/强推，要求最新 base 的检查（strict）。
- 将稳定状态 `PR gate` 设为 required；不要同时要求会按区域跳过的 jobs。
- 清理被替换的旧状态名；核对管理员/自动化的 bypass 权限和未解决 review thread 的处理。
- 审计 actor 的独立性按团队身份配置；单一账号不能靠“一个形式上的 APPROVED”解决独立审计。

通过 `gh api repos/ackness/pobr/branches/master/protection` 和
`gh api repos/ackness/pobr/rulesets` 核对实际配置，不以文档或 workflow 成功推断已强制执行。
服务端设置、push、PR、合并和发版按任务授权范围执行；本地流程改动不自动发布。

## 处理外部 PR

先阅读完整 diff，再在隔离 checkout 中执行外部代码；不使用真实账户配置或发布凭据。
常规路径是原 PR 的 review → request changes → 作者更新 → 最终 head/base 复验 → 授权合并，
按上面的风险与检查原则处理。需要集成当前 master 时先检查冲突，再按影响运行相关检查，
不为每个外部小改动重复全量构建。

审查机制依据、通用规则、同类受影响技能、诊断与兼容；出处以游戏数据为准。
只有作者不能继续或需要维护者接管时才在原提交上追加提交/另开替代 PR。
保留署名，说明原 PR 与替代 PR 的关系；squash 时确认 coauthor，手动关闭原 PR 并链接合入结果。
不要重写对方提交，也不要把替代 PR 当成每个外部贡献的固定步骤。

## 慢构建与按需报告

```bash
mise run timings build parity                # Compile selected suite with Cargo HTML timings
# Open target/cargo-timings/cargo-timing.html

time mise run test build parity              # Compare unchanged cold/warm inputs

mise run test build parity parity_baseline_report -- --ignored --nocapture
mise run test build parity effective_switch_dual_run_report -- --ignored --nocapture
mise run test build parity ehp_dual_run_report -- --ignored --nocapture

mise run triage [set dir | build dir ...]                         # Every worked build at once: whose differences, and their shared causes
mise run diagnose <build dir | code file | committed build name>   # One build's dropped inputs
mise run compare <build> [Stat ...]                               # One build against the pinned PoB2, stat by stat
mise run lookup <id or name> [--level N]                          # Game data records of a skill, stat, base, node or unique
mise run accuracy [dir of */code.txt]                             # A corpus against the reference its codes carry
mise exec -- cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown TotalDPS,Life   # How the stats asked for arrive
```

`triage` 是数值差异的第一步，一次对照基准组与 tuned 组的全部构筑，`diagnose` / `compare` 只用于它解释不了的单个构筑；
流程见 [AGENTS.md](../AGENTS.md#accuracy-workflow) 和[维护者指南的“排查数值差异”](maintainer-guide.md#排查数值差异诊断闭环)，
各工具输出的含义见[诊断工具与逐行台账](diagnostics.md)。
`triage` 与 `compare` 需要 vendor 与 LuaJIT，PoB2 的结果按构筑码缓存在 `.cache/pob2-oracle/`，两者共用。`lookup` 只读
当前数据版本（`POBR_DATA_ROOT` / `POBR_DATA_VERSION` / `CURRENT`，或 `--version`）。
这些 Python 脚本本身只用标准库；完整对照仍需要 CLI、vendor 和 LuaJIT。

`--timings` 显示编译单元耗时与依赖等待。改变 target 目录后按 Cargo 输出定位报告。
不要为了测量清缓存。测试目标由 `mise run targets` 读取当前 manifest；深层 tests 文件通常是
汇总套件的模块，并非每个文件都会单独链接。

测试耗时应区分编译和执行；不要为每个用例重新加载相同的只读游戏快照。
需要修改 fixture 的测试显式 clone；依赖未初始化状态的测试须独立隔离该状态，不能因执行顺序提前返回。
上述三个 parity 报告仅作诊断，默认忽略；两种模式的 no_regression、golden、契约和
独立语料健康断言继续默认执行，不为加速删减回归覆盖。

## 其他按需检查

- `mise run smoke`：聚合、解析、Build Code 的代表测试，不先构建整个工作区；不能替代完整门禁。
- 本地 `.agents/skills/run-pobr/driver.sh` 转发到同一驱动（`.agents` 按仓库规则不入库）。`smoke` 是环境检查，已有相关测试通过后无需再补一次。
- `mise exec -- python3 devs/scripts/test_workflows.py`：检查脚本失败传递、临时目录清理与工作区保护，以及 `compare` 的来源对齐和 `lookup` 的查找（用合成数据），不调用真实 Cargo。
- `devs/scripts/regen-check.sh` 在临时副本中重生成，保留 `overlay-common` 与 examples 语料，不写入或恢复工作区文件。
- `mise exec -- node web/scripts/bench-calc.mjs`：用三个已提交构筑测真实 WASM 的非缓存计算和 16 项批量计算。
  需要先构建 WASM、同步数据；`--reference <old-pkg>` 另检查完整输出是否一致，适用于保持行为的优化。
  报告保存在 gitignore 的 `.cache/`，只按需运行，不属于 CI 门禁。
- 本机性能与响应一致性检查需要 `diagnostics` feature，按需运行，不属于 CI 门禁。
  下列命令均加 `--release -- --ignored --nocapture`：
  - `mise exec -- cargo test -p pobr-wasm --features diagnostics --test perf_uncached`：三个构筑的 Web 重算路径，
    包括完整重算、打开属性解释的后续请求和逐技能摘要。每个样本增加不同词条，避免响应或计算缓存代答。
    `perf_timing` 则重复同一请求，测缓存命中。
  - `mise exec -- cargo test -p pobr-wasm --features diagnostics --test perf_corpus`：默认遍历四个集合：
    `demo-bd-test/builds`、`wasm-benchmark/builds`、`reference-builds/tuned/builds`、
    `reference-builds/ninja-top10-2026-10-26/builds`。不包括 holdout，样本数由这些目录的当前内容决定。它测无明细重算、默认明细、单属性明细、逐技能摘要和天赋变体，输出响应哈希与耗时。
    `POBR_CORPUS_OUT=<file>` 保存表；在两个引擎版本上比较哈希，可检查响应是否逐字节保持一致。
    `POBR_CORPUS_ONLY` / `POBR_CORPUS_LIMIT` 缩小范围，`POBR_CORPUS_DIRS` 可替换默认集合，
    `POBR_CORPUS_DUMP=<dir>` 保存用于定位差异的响应。
    `POBR_CORPUS_STAGED=1` 使用浏览器分阶段注入字节、解析后释放原文件的方式加载数据，哈希应不变。
    `POBR_CORPUS_INLINE_DESCRIPTORS=1` 把明细类型表放回响应，便于与它改为可选前的记录比较。
    同一个二进制重复运行却得到不同哈希时，先修复该构筑的非确定性，再比较版本。
  - `perf record --call-graph dwarf` 可查看上述运行的时间花在哪里。
    优化前先测当前版本，保留输入、工具链、主机与原始采样，避免沿用旧版热点结论。
- `mise run web package-wasm`：将已准备 WASM 与同步数据打包到 `.cache/wasm-release/`；`mise run web smoke-wasm-package` 解压后执行真实计算。`build-wasm` 校验源码、工具链、构建设置和产物记录，未变时直接复用；`mise run web build-wasm --force` 显式重建但不清 Cargo 缓存。打包继续校验源码 / 绑定指纹、实际 schema 和数据快照，来源 commit 保留编译时值。脚本改动运行 `mise exec -- node --test web/scripts/package-wasm.test.mjs web/scripts/sync-data.test.mjs`；发布流程见 [WASM 打包说明](wasm-package.md)。release workflow 在完整门禁通过后自动附加 Release 资产，手动 CI 仅保存 Actions artifact。
- `mise run web test:worker`：在实际 workerd 运行时测试 Worker，使用合成上游响应，无外网依赖。E2E 失败的截图与 trace 由 CI 上传为 `playwright-failure`。
- 计划发版时，在功能 PR 中一并更新 workspace 版本。按风险完成审阅和 PR 检查，必要时先手动 full；合并后 master 自动全量，发布 tag 仍须通过对应提交的完整验证。稳定 v0 版本按部署规则上线，不把旧分支的通过结果冒充为 tag 验证。
