# Development and CI workflow

开发命令是根 `mise.toml` 中的 mise task：`mise tasks` 列出全部，`mise run <task> [参数]` 执行，
任务名之后的参数原样传递。它们转发到 `devs/scripts/driver.sh`（`.claude/skills/run-pobr/driver.sh` 保留为转发入口）；没有安装 mise 时可直接运行
`bash devs/scripts/driver.sh <命令>`，本机可选的 `.agents` 入口也转发到同一驱动。
游戏 CLI（`cargo run -p pobr-cli`，二进制 `pobr`）只提供游戏计算命令，不包含任何开发命令。
本文维护构建、测试、缓存和 CI 操作；验证范围与计算约定见 [maintainer-guide.md](maintainer-guide.md)，
Web 初次准备见 [Web README](../web/README.md)，数据更新见 [pipeline/README.md](../pipeline/README.md)。

## 工具链来源

| 工具 | 当前配置 |
|------|----------|
| Rust | edition 2024；版本由根 `rust-toolchain.toml` 钉定（含 rustfmt/Clippy），本地与 CI 相同；本地 `mise run setup` 通过 mise 安装它，直接使用 rustup 时首次使用自动安装。升级版本属于工具链变更，需要完整门禁 |
| Node | 本地 `mise.toml` 与 CI 均指定 Node 26 |
| pnpm | 本地 mise 与 CI 仅固定稳定主版本 `12`；运行 `mise install` 配置。依赖 lockfile 固定解析结果，更新直接依赖时保持原主版本，不采用预发布版本 |
| WASM | `wasm32-unknown-unknown` target + wasm-pack（本地 mise 钉定 0.15.0，CI 用 taiki-e/install-action 取最新版），仅构建 Web 引擎时需要 |
| LuaJIT | vendor 抽取、oracle 及相关工具测试需要；普通计算使用已入库数据 |

`mise run setup` 在新机器上一次装好 Rust（mise 读取 `rust-toolchain.toml`，不在 `mise.toml`
重复钉版本）、`wasm32-unknown-unknown` target、wasm-pack 0.15.0、Node/pnpm 和 `web/` 依赖；
重复运行是安全的。LuaJIT 与 vendor 不在其中，仍由 `mise run deps` / `mise run vendor` 按需安装。
首次 `build-wasm` 还会编译一次 `wasm-bindgen-cli`，wasm-pack 会缓存。
`mise run status` 查看环境就绪态。已有可用环境时跳过 bootstrap 和安装，
不为日常测试更新工具链或重复构建整个 workspace。

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
mise run gate                                 # Once per batch of merged changes: every step, then the failures
mise run gate --only core,parity              # Rerun only the failed steps
mise run warm <warm checkout>                 # New worktree: copy-on-write clone of its target/debug
```

基准和手动诊断按需编译，普通测试不编译 Criterion 或五个 WASM 诊断/fixture 生成目标：

```bash
cargo bench -p pobr-core --features benchmarks --bench mod_db_bench
cargo bench -p pobr-build --features benchmarks --bench perform_bench
cargo test -p pobr-wasm --features diagnostics --test perf_timing -- --ignored --nocapture
```

完整 CI 和 `mise run full` 的 Clippy 显式启用这两个 feature，继续检查手动工具；
日常定向测试和 PR 检查不启用。`gen_fixtures` 同样需要 `--features diagnostics`。

小改动的文件与检查步骤见[改动配方](recipes.md)。单个改动只跑定向测试；`gate` 由合并多个改动的人运行一次，
不在每个改动上重复（规则见 [AGENTS.md](../AGENTS.md#validation-budget)）。

短参数 `build` 对应 `pobr-build`，`core` 对应 `pobr-core`；完整包名也可用。
`lib` 选择单元测试，其他名字选择 `--test <suite>`。不传目标会显示用法并退出，
不会默认启动全量测试。多个目标、features 或其他选项直接传 Cargo 参数。

```bash
mise run test -p pobr-build --test skills --test codec
mise run lint -p pobr-build --lib --test skills
mise run test build parity no_regression
mise run build --workspace                    # Only when all binaries are needed
```

测试本身会编译，不必先执行 `check` 或 `build`。只有测试名称过滤仍可能编译
该 crate 的全部测试目标，应同时选择 `--test` 或 `--lib`。跨 crate API 修改还需要
调用方的契约测试。相关检查通过后结束验证，不固定串联 check/build/clippy/test/full。
相同输入下已通过的检查可以复用；仅改文档不使代码测试失效。

Web 纯 TS/CSS 修改复用 WASM，运行相关 Vitest 与 typecheck；`pnpm --dir web build`
已包含 typecheck。交互修改补对应 Playwright spec，运行前准备当前 dist、WASM 和数据。
脚本修改使用 `bash -n`、`python3 devs/scripts/test_workflows.py`；WASM/数据同步脚本
使用 `node --test web/scripts/package-wasm.test.mjs web/scripts/sync-data.test.mjs`。

## 缓存与磁盘占用

每个 worktree 复用一个默认 `target/`。同一 worktree 的会话共用它、串行运行 Cargo，
不按会话、测试或门禁创建 `target-gate` 等副本；不同 worktree 不共享可写 target。
看到 `Blocking waiting for file lock` 时检查已有进程，不再叠加 Cargo。

新 worktree 的 `target/` 为空，第一次测试会重新编译全部依赖。`mise run warm <已编译的 checkout>`
用写时复制（APFS clonefile 或 Linux reflink）克隆那份 `target/debug`，不额外占用空间，也不共享可写目录；
之后只有路径不同的 workspace crate 重新编译。已有 `target/debug` 时不覆盖。
分析工具（`gap`、`gaps`、`triage`、`compare`、`config:coverage`、`skills:sweep`、`output-coverage.py`）
默认各自编译 CLI，并把台账与 PoB2 结果缓存在本 checkout 的 `.cache/`。设 `POBR_BIN=<checkout>/target/debug/pobr`
使用已编译的 CLI，设 `POBR_CACHE_DIR=<checkout>/.cache` 共用已有缓存（台账按程序内容、PoB2 结果按构筑码与 vendor 失效）。
`POBR_BIN` 运行的是那份 checkout 的引擎，用于读取游戏数据、台账和 PoB2 结果，不用于测量本 worktree 的改动。

根 `Cargo.toml` 的 dev/test 对第三方依赖使用 `opt-level = 3`，workspace crate 保持未优化。
游戏数据快照加载包含 JSON 解析和 SHA-256 校验；依赖未优化时历史测量一次加载约 3.4 s，
优化后约 0.65 s。依赖只编译一次并长期缓存，不影响日常修改 workspace 代码后的重编译速度。
`pobr-build --test skills` 的当前快照由套件内一个只读 `LazyLock` 共享，修改数据的测试显式 clone，
合成数据 fixture 保持独立。新增此套件测试复用 `crate::test_data()`，不要重复加载同一版本。
日常测试和完整门禁统一使用 `cargo test`，套件内复用快照，同时覆盖 doctest；无需额外安装测试 runner。
规则生成测试的确定性、覆盖率计数和已提交 golden 共用两次生成；物品往返的 corpus 大小、
语义断言和 byte-diff 报告共用一次遍历，均保留原有断言。

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
pnpm --dir web build-wasm                   # Verify and reuse unchanged output
pnpm --dir web build-wasm --force           # Rebuild through Cargo, without cleaning
```

`cache` 通过离线 Cargo metadata 解析当前 target/build 目录，统计占用与文件数，
细分 host/WASM profile 的 deps、incremental 等目录；同时统计 `.cache`、Web 依赖、
dist、WASM 包和同步数据。不编译、不删除、不跟随符号链接。
支持文件系统时显示分配块大小，否则显示文件字节数；硬链接或压缩可能影响与真实占用的差异。
本地 Cargo 配置和 `CARGO_INCREMENTAL` / `CARGO_PROFILE_*` 可以覆盖仓库默认设置。
全局 Cargo registry/git、pnpm store 和编译器缓存另行存储，未计入本仓库的统计。
已配置 sccache 时用 `sccache --show-stats` 查看位置、命中率、占用及上限；
本仓库沿用已有 wrapper，不安装缓存服务或修改全局设置。

`build-wasm` 使用 `wasm-release`：继承 release 的速度优化，启用 ThinLTO 和单个 codegen unit。
这只影响 WASM 发布构建，不改变日常 dev/test 缓存。固定 locale TOML 在主机构建时解析成字符串表，
TOML parser 不进入 WASM；不要在运行时重新解析这些静态文件。

WASM 构建凭据校验 Rust 源码、嵌入语言文件、共享配置 JSON、manifest/lockfile、JSON 契约、
工具版本、相关环境/Cargo 配置，以及绑定哈希与实际 schema。
输入和产物未变时跳过 wasm-pack，保留原编译 commit 与 dirty 标记；
缺失、损坏或旧凭据需要一次正常重建。普通 UI、文档和运行时游戏数据修改不会触发 WASM 重编译；
`data/overlay-common/*_config_options.json` 是可能内嵌的配置文件族，修改或新增会重新构建，
无需另行登记缓存输入；
游戏数据变化仍需 `pnpm --dir web sync-data`。

这些设置减少后续产物，并非磁盘配额或自动清理器。`.cache/` 中历史诊断、vendor 差异副本、
发布包以及旧 worktree 仍可能累积；先确认具体目录及保留需求，再显式清理。
脚本不会自动删除发布资产、用户资料或其他 worktree。

## 云端验证与发布

```bash
# Push the intended commit first; requires authenticated gh.
mise run ci feature/my-change
mise run ci feature/my-change --e2e          # Also run Playwright E2E (opt-in)
mise run ci-status --branch feature/my-change

# Explicit local Rust gate; Web checks are separate.
mise run full
```

合并、发版、工具链/features/依赖变更或影响范围不明的核心修改需要完整门禁，
优先使用 CI。`ci` 测试已推送的远端分支/tag，不推送本地代码；必须核对结果的提交 SHA，
请求成功不等于验证通过。普通 push 不触发 CI；PR 只自动运行下述轻量检查，不是完整门禁。

[pr.yml](../.github/workflows/pr.yml) 在每个指向 master 的 PR 上运行，包括 fork 提交的 PR：

| 改动范围 | 自动检查 |
|----------|----------|
| Rust 源码、`data/`、`examples/`、脚本或工作流 | fmt、workspace Clippy、脚本测试、`mise run smoke`、`parity no_regression` |
| `web/` | `pnpm typecheck` 与 Vitest 单测（不构建 WASM，不跑 E2E） |
| 仅文档 | 不运行 |
| 发版 PR（根 `Cargo.toml` 的 workspace 版本变化） | 版本元数据校验和 [ci.yml](../.github/workflows/ci.yml) 的完整 Rust / Web 门禁；它已覆盖上面的轻量检查，后者不再重复运行 |

fork PR 使用只读令牌且读不到仓库密钥；首次贡献者的运行需要维护者在 Actions 页批准。
轻量检查通过不代表完整门禁通过：定向套件、Worker、真实 WASM 和 E2E 仍按改动在本地或手动 CI 验证。

同一套轻量检查在 master 的每次推送后再跑一次：合并后的问题当场暴露，构建缓存也存到 master 名下。
PR 只能读取自己和 master 的缓存，所以这次运行决定了新 PR 是否从零编译。
完整门禁另外每晚在 master 上定时运行一次（19:00 UTC；过去 24 小时没有新提交则跳过），
把轻量检查漏掉的回归提前到第二天发现，而不是攒到发版。

当前 [ci.yml](../.github/workflows/ci.yml) 包含独立的 Rust lint、Rust 测试和 Web 门禁。
Rust 检查 fmt/Clippy/Cargo tests（含 doctest）/i18n 与数据兼容；Web 检查脚本、类型、单测、Worker、
真实 WASM 与打包。Playwright E2E 默认不运行：tag、发版 PR 和普通手动触发都跳过，
只有 `mise run ci <ref> --e2e`（或在 Actions 页勾选 `e2e`）才运行，约 15 分钟。
界面交互改动需要时在本地跑对应 spec，或显式请求一次云端 E2E。`mise run full` 仅运行 Rust 门禁，包含 fmt/Clippy、`cargo test --workspace`（含 doctest）和 i18n。离线或排查 CI 时可显式使用，无需重复已通过的云端全量验证。

Linux CI 的 WASM 构建设置 `BINARYEN_CORES=1`，让 wasm-pack 下载的 Binaryen 在主线程执行。
该模块在 Linux 预编译优化器的工作线程上曾触发 SIGSEGV，切为单线程后完整构建与解包计算通过；
参见 [Binaryen #7570](https://github.com/WebAssembly/binaryen/issues/7570)。所有优化步骤仍保留，
不以禁用 wasm-opt 或忽略失败来发布。此设置只影响构建时并行度，不改变浏览器计算的线程模型。

应用发版更新 `[workspace.package].version`，用 `cargo metadata --format-version 1 > /dev/null`
刷新应用/工具的锁文件版本，保留七个库的独立版本。数据活动版本在运行时读取 `data/CURRENT`，
与应用版本分开。版本更新仍需对应检查，但不因应用版本或活动数据标记变化使所有基础库失去缓存。

发版 PR 在合并前就运行完整门禁：`devs/scripts/release-check.sh <base>` 比较 workspace 版本，
版本变化时要求新版本高于基线版本、`v<版本>` tag 尚不存在、`Cargo.lock` 已带新版本，然后启动与 tag 相同的 Rust / Web 门禁
（不发布资产、不部署，不含 E2E）。等这些检查通过再合并、打 tag，避免门禁在 tag 上失败而浪费版本号。
修改根 `Cargo.toml` 但版本不变的 PR 只运行这一步校验，随后跳过完整门禁。

推送 `v*` tag 后由 CI 完成门禁，通过后发布 WASM 资产；Cloudflare 部署仅接受 `v0.*` tag。
不要为同一个 tag 再请求相同的 CI，
也不要求本地先跑一次 full。CI 依赖缓存可能过期，不能保证每次都是热构建。
发布包与凭据规则见 [WASM 打包说明](wasm-package.md)。

## 处理外部 PR

贡献者须知见 [CONTRIBUTING.md](../CONTRIBUTING.md)。维护者按下面的顺序处理 fork 提交的 PR：

1. **先读 diff，再运行。** 外部代码在本机执行前完整读一遍，确认只包含声明的改动。
2. **在当前 master 上验证。** PR 的基点可能已落后；取回后叠在最新 master 上，按
   [maintainer-guide.md](maintainer-guide.md) 的验证分层运行对应套件，计算类改动加 `parity no_regression`。

   ```bash
   git fetch origin master pull/<N>/head:pr-<N>
   git merge-tree --write-tree origin/master pr-<N>   # Conflict check without touching the worktree
   git switch -c review/pr-<N> origin/master && git merge --no-edit pr-<N>
   ```

3. **按项目约定审查。** 重点看三件事：公式或数据引用是否带 PoB2 Lua 行号；是否能用一条通用规则
   代替逐技能 / 逐词条条目；同一机制下是否还有其他受影响的技能。修复必须带能复现错误的断言。
4. **需要调整时在贡献者提交之上追加。** 保留原提交和署名，在 `review/pr-<N>` 上追加提交并另开 PR，
   在原 PR 评论里说明改了什么、为什么；或在贡献者允许时直接推送到其分支。不要重写对方的提交。
5. **合并后收尾。** squash 合并会生成新提交，GitHub 不会把原 PR 标为已合并：手动关闭原 PR 并注明
   合入的 PR 和提交；确认 squash 提交带有 `Co-authored-by`。想让原 PR 自动显示 Merged 时改用
   merge commit 或 rebase 合并。最后删除本地和远端的临时分支。

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
cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown TotalDPS,Life   # How the stats asked for arrive
```

`triage` 是数值差异的第一步，一次对照基准组与 tuned 组的全部构筑，`diagnose` / `compare` 只用于它解释不了的单个构筑；
流程见 [AGENTS.md](../AGENTS.md#accuracy-workflow) 和[维护者指南的“排查数值差异”](maintainer-guide.md#排查数值差异诊断闭环)。
`triage` 与 `compare` 需要 vendor 与 LuaJIT，PoB2 的结果按构筑码缓存在 `.cache/pob2-oracle/`，两者共用；`lookup` 只读
当前数据版本（`POBR_DATA_ROOT` / `POBR_DATA_VERSION` / `CURRENT`，或 `--version`），二者都只用 Python 标准库。

`--timings` 显示编译单元耗时与依赖等待。使用重定向后的 target 时按 Cargo 输出定位报告。
不要为了测量清缓存。测试目标由 `mise run targets` 读取当前 manifest；深层 tests 文件通常是
汇总套件的模块，并非每个文件都会单独链接。

测试耗时应区分编译和执行；不要为每个用例重新加载相同的只读游戏快照。
需要修改 fixture 的测试显式 clone；依赖未初始化状态的测试须独立隔离该状态，不能因执行顺序提前返回。
上述三个 parity 报告仅作诊断，默认忽略；两种模式的 no_regression、golden、契约和
独立语料健康断言继续默认执行，不为加速删减回归覆盖。

## 其他按需检查

- `bash devs/scripts/driver.sh smoke`：聚合、解析、Build Code 的代表测试，不先构建整个工作区；不能替代完整门禁。
- 本地 `.agents/skills/run-pobr/driver.sh` 转发到同一驱动（`.agents` 按仓库规则不入库）。`smoke` 是环境检查，已有相关测试通过后无需再补一次。
- `python3 devs/scripts/test_workflows.py`：检查脚本失败传递、临时目录清理与工作区保护，以及 `compare` 的来源对齐和 `lookup` 的查找（用合成数据），不调用真实 Cargo。
- `devs/scripts/regen-check.sh` 在临时副本中重生成，保留 `overlay-common` 与 examples 语料，不写入或恢复工作区文件。
- `node web/scripts/bench-calc.mjs` measures uncached real-WASM calculation and 16-item batches on three committed builds. It requires built WASM and synced data; `--reference <old-pkg>` additionally checks complete output equality for behavior-preserving optimizations. Reports stay under ignored `.cache/`; this benchmark is opt-in, outside the CI gate.
- `pnpm --dir web package-wasm`：将已准备 WASM 与同步数据打包到 `.cache/wasm-release/`；`pnpm --dir web smoke-wasm-package` 解压后执行真实计算。`build-wasm` 校验源码、工具链、构建设置和产物凭据，未变时直接复用；`pnpm --dir web build-wasm --force` 显式重建但不清 Cargo 缓存。打包继续校验源码/绑定指纹、实际 schema 和数据快照，来源 commit 保留编译时值。脚本改动运行 `node --test web/scripts/package-wasm.test.mjs web/scripts/sync-data.test.mjs`；发布流程见 [WASM 打包说明](wasm-package.md)。tag CI 在 Rust/Web 门禁通过后自动附加 Release 资产，手动 CI 仅保存 Actions artifact。
- `cd web && pnpm test:worker`：在实际 workerd 运行时测试 Worker，使用合成上游响应，无外网依赖。E2E 失败的截图与 trace 由 CI 上传为 `playwright-failure`。
- 计划发版时，在功能 PR 中一并更新 workspace 版本。本地完成相关验证后合并，再推送一次 tag；tag CI 完成全量门禁，通过后自动部署，无需在 master 额外手动运行同一套 CI。
