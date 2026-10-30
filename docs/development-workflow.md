<a id="development-and-ci-workflow"></a>

# 开发与 CI 工作流

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
| WASM | `wasm32-unknown-unknown` target + wasm-pack（本地 mise 钉定 0.15.0，CI 从 mise.toml 读取同一个版本，再由 taiki-e/install-action 安装），仅构建 Web 引擎时需要 |
| LuaJIT | oracle 需要支持 `+=` 的 runtime；`mise run deps` 按 PoB2 Dockerfile 的 upstream commit 编译并缓存，见 [oracle setup](../tools/pob2-oracle/README.md#依赖--阻塞解决)。普通计算使用已入库数据 |

`mise run setup` 在新机器上一次装好 Rust（mise 读取 `rust-toolchain.toml`，不在 `mise.toml`
重复钉版本）、`wasm32-unknown-unknown` target、wasm-pack 0.15.0、Node/pnpm 和 `web/` 依赖；
重复运行是安全的。LuaJIT 与 vendor 不在其中，仍由 `mise run deps` / `mise run vendor` 按需安装。
`deps` 需要 Git、make、C 编译器，缓存 runtime 到 `${XDG_CACHE_HOME:-$HOME/.cache}/pobr/runtimes/`，
不替换系统 LuaJIT。运行 Rust vendor 抽取时可设
`POBR_LUAJIT="$(bash devs/scripts/pob2-luajit.sh resolve)"`；oracle runner 自动选择兼容的缓存。
首次 `build-wasm` 还会编译一次 `wasm-bindgen-cli`，wasm-pack 会缓存。
`mise run status` 查看工具和数据是否就绪。已有可用环境时跳过 bootstrap 和安装，
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
用写时复制（APFS clonefile 或 Linux reflink）克隆那份 `target/debug`。开始时复用文件数据块，修改时才占用新块，
两份目录仍可独立写入。之后只有路径不同的 workspace crate 重新编译。已有 `target/debug` 时不覆盖。
分析工具（`gap`、`gaps`、`triage`、`compare`、`config:coverage`、`skills:sweep`、`output-coverage.py`）
默认会准备 CLI，并把台账与 PoB2 结果缓存在本 checkout 的 `.cache/`。设 `POBR_BIN=<checkout>/target/debug/pobr`
使用已编译的 CLI，设 `POBR_CACHE_DIR=<checkout>/.cache` 共用已有缓存（台账按程序内容、PoB2 结果按构筑码与 vendor 失效）。
`POBR_BIN` 运行的是那份 checkout 的引擎，用于读取游戏数据、台账和 PoB2 结果，不用于测量本 worktree 的改动。

根 `Cargo.toml` 的 dev/test 对第三方依赖使用 `opt-level = 3`，workspace crate 保持未优化。
游戏数据快照加载包含 JSON 解析和 SHA-256 校验；依赖未优化时历史测量一次加载约 3.4 s，
优化后约 0.65 s。依赖首次编译后可长期复用；日常只改 workspace 源码时，无需重新优化依赖。
`pobr-build --test skills` 的当前快照由套件内一个只读 `LazyLock` 共享，修改数据的测试显式 clone，
合成数据 fixture 保持独立。新增此套件测试复用 `crate::test_data()`，不要重复加载同一版本。
日常测试和完整门禁统一使用 `cargo test`，套件内复用快照，同时覆盖 doctest；无需额外安装测试 runner。
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

WASM 构建记录校验 Rust 源码、嵌入语言文件、共享配置 JSON、manifest/lockfile、JSON 契约、
工具版本、相关环境/Cargo 配置，以及绑定哈希与实际 schema。
输入和产物未变时跳过 wasm-pack，保留原编译 commit 与 dirty 标记；
缺失、损坏或旧格式记录需要一次正常重建。普通 UI、文档和运行时游戏数据修改不会触发 WASM 重编译；
`data/overlay-common/*_config_options.json` 是可能内嵌的配置文件族，修改或新增会重新构建，
无需另行登记缓存输入；
游戏数据变化仍需 `pnpm --dir web sync-data`。

这些设置减少后续产物，并非磁盘配额或自动清理器。`.cache/` 中历史诊断、vendor 差异副本、
发布包以及旧 worktree 仍可能累积；先确认具体目录及保留需求，再显式清理。
脚本不会自动删除发布资产、用户资料或其他 worktree。

项目专属 [.claude/skills/workflow](../.claude/skills/workflow/SKILL.md) 将本页适配给 agent；
跨项目的通用设计使用独立安装在个人 skills 目录的 `repo-workflow`，不把 PoBR 规则写入全局 skill。

## CI 的责任与触发

workflow 负责事件、环境、权限；`mise.toml` / `devs/scripts` 负责命令。
按改动选本地定向检查；CI 的集成检查不要求开发者在本地重复编译。

| 入口 | 触发与职责 | 成功出口 |
| --- | --- | --- |
| [pr.yml](../.github/workflows/pr.yml) | 所有 master PR、每次 master push；按整个变更范围选轻量检查或调用完整门禁 | `PR gate` |
| [handoff.yml](../.github/workflows/handoff.yml) | PR 创建、更新提交、编辑正文、ready/draft；只校验交接记录 | `PR handoff` |
| [ci.yml](../.github/workflows/ci.yml) | 可复用完整门禁、手动 dispatch、每日 19:00 UTC 的 master 回归 | `Full gate` |
| [release.yml](../.github/workflows/release.yml) | `v*` tag；先校验发布身份，调用完整门禁，再发布同次产物和符合条件的生产部署 | Release / deploy jobs |

PR 变更比较测试中的 merge commit 与它的 base parent；master 比较 push 事件的 before/after，
包括一次推送的所有提交。缺少比较基线时选择完整门禁；重命名按删除旧路径、增加新路径处理。
未知输入保守进入完整门禁。PR 新提交取消旧检查；master 的每个增量 push 独立运行，
避免后续文档提交取消前一次代码检查。每日完整门禁也会重试旧提交的失败，不按提交年龄跳过。

[ci-plan.py](../devs/scripts/ci-plan.py) 是路径选择的唯一实现：

| 变更 | 自动验证 |
| --- | --- |
| Rust / 数据 / 构筑语料 | fmt、workspace Clippy、smoke、请求契约、parity；数据还检查脚本契约 |
| Web 代码 | typecheck、Vitest、Worker runtime；脚本/静态资源还检查脚本契约 |
| 普通开发脚本 | 不编译 Rust 的自动化契约：shell、Python、Node、工作流语法 |
| workspace/包 manifest、lockfile、工具链、mise、workflow、CI/发布策略脚本、数据管线、未知输入 | 完整 Rust、Web 与自动化契约；不重复轻量 jobs |
| 普通文档（含 Web README）、模板、说明图片 | 无产品构建；仍有稳定 gate 和 PR 交接校验 |

`PR gate` 校验检测器成功、输出合法、所有选中 jobs 成功；失败、取消、缺失或意外跳过都拒绝。
`Full gate` 同样要求全部 Rust/Web/脚本 jobs 成功。fork 和 Dependabot 使用相同的只读、无密钥验证路径；
首次外部贡献的 Actions 批准取决于 GitHub 设置。这里的批准只允许执行，不表示代码正确。

完整门禁检查 fmt/Clippy（含手动诊断 features）、workspace tests/doctest、i18n、
数据兼容（含安装了 Lua/dkjson 的提取测试）、Web 类型/单测/Worker、真实 WASM 契约、打包与解包计算；
本地 vendor 才能执行的对照可能跳过，不能据此声称这些公式已获验证。
E2E 只由显式手动输入启用，PR、tag、nightly 默认不运行。
界面交互改动仍按维护者指南在本地验证相关 spec，或显式请求云端 E2E。

```bash
mise run check:workflows                   # 自动化契约，不编译 Rust
mise run ci feature/my-change              # 先推送目标提交；只请求检查，不推送代码
mise run ci feature/my-change --e2e        # 显式启用 E2E
mise run ci-status --branch feature/my-change
mise run full                             # 显式本地 Rust 全量检查，不含 Web
```

必须核对 run 对应的 SHA 和最终结果；dispatch 成功不等于检查通过。
计算修复的完整数值测量仍由批次 `mise run gate` 负责，不因 PR 流程改变而逐次运行。
工作流迁移用 `mise run check:workflows`，无需 Cargo / WASM / E2E。
其 actionlint 1.7.12 尚不认识 GitHub 已支持的 `queue: max`，只豁免该特定未知键错误，
其余语法与表达式检查仍启用；队列语义见 [GitHub 文档](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)。

## Agent 提交、审计与合并

1. **作者交接。** 一项可审查的问题一个 PR；填写 Problem、Change、Validation、Remaining limits。
   Validation 写实际执行的定向命令、结果和未验证部分；文档只需 diff/link review。
   界面变化给出有用的前后截图；计算变化引用游戏数据/stat ID，PoB2 仅交叉核对。
   先开 draft 可以尽早获得 CI，不要求草稿已有审计。
2. **独立审计。** 另一个 agent/reviewer 读取 diff、相关调用方和真实证据，核对当前 head 与 base。
   将正确性、回归、契约或权限问题作为 blocking；风格建议明确标为 suggestion。
   修正在同一 PR 追加，保留作者提交与署名。不要因使用 agent 就省略真实执行结果。
3. **记录结论。** ready 前把实际审计结论写入 PR 正文的唯一 `pobr-audit` JSON comment。
   `head` / `base` 用完整 SHA，`reviewer` 记录模型与 session/review 链接；
   `verdict` 必须是 `pass`；blocking findings 必须 resolved。没有 findings 时写空数组。
4. **机器校验。** `PR handoff` 检查正文有实际内容、审计格式、提交新鲜度和未解决阻断项。
   新提交或 ready 事件自动重新检查；编辑正文只触发交接检查，不重新编译产品；检查读取实时 PR 元数据，旧事件重跑不能恢复过期正文或 draft 状态。
   同一 GitHub 身份下，记录不能证明审计来自独立推理，也不能证明正文所写命令执行过；
   CI 负责其实际检查，维护者负责审计来源和不在 CI 中的证据，禁止编造审计或通过记录。
5. **最终预检。** 获准合并前运行 `mise run pr-ready <N>`。它只读当前 PR、fresh audit、
   两个必需 gate、其他失败/待定检查与冲突状态，并在返回前再次核对 head/base。
   它不推送、不评论、不合并；成功时输出准确 head 及带 `--match-head-commit` 的合并命令。
   作者随后推送时，该参数使合并失败。base 更新仍须最新集成检查与服务端 strict 规则约束。

```json
{"head":"<40-character head SHA>","base":"<40-character base SHA>","reviewer":"<model/session or review link>","verdict":"pass","findings":[]}
```

有发现时，例如 `{"severity":"blocking","status":"resolved","summary":"Verified the fixed schema migration"}`。
不要为凑格式生成虚假审计；如审计受阻，保持 draft 或失败状态并说明原因。
Dependabot PR 也由 agent 补充真实交接与审计后再 ready/合并，不因机器人身份免除检查。

## Issue 与自动化维护

Bug 表单收集可复现步骤、版本/数据版本、环境，以及相关构筑码和对照数值；
功能表单问用户目标和能改善的输出，不要求报告者懂内部实现。
构筑输入先删除不愿公开的信息。维护者先查重、确认范围；证据不足时要求具体资料，
确认的缺陷按原因归并，修复只覆盖一部分时用 Refs，不提前关闭整项问题。
没有证据证明有用的事件模拟不自动进入实现，遵守静态模拟范围。

Dependabot 每月按 Actions、Cargo、Rust toolchain、Web 与数据管线分别维护依赖；
manifest/lock/toolchain 改动自动进入完整门禁。没有自动合并或按年龄关闭已确认缺陷的机器人。
不为流程形式引入收费 review 服务、贡献准入名单或带真实密钥的 fork 预览。

## 发布与部署

应用版本由 `[workspace.package].version` 管理；七个基础库与 `data/CURRENT` 独立版本。
更新应用版本后用 `cargo metadata --format-version 1 > /dev/null` 刷新锁文件。
[release-policy.py](../devs/scripts/release-policy.py) 按 SemVer 判断升级，检查所有继承 workspace
版本的应用/工具锁版本；远端 tag 查询失败会报错，不能当作“tag 不存在”。
PR 升版要求 tag 未使用；master 的合并后验证可接受指向同一提交的现有 tag。
升版、依赖、工具链 PR 会在合并前走完整门禁。

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
- 将稳定状态 `PR gate` 和 `PR handoff` 设为 required；不要同时要求会按区域跳过的 jobs。
- 清理被替换的旧状态名；核对管理员/自动化的 bypass 权限和未解决 review thread 的处理。
- 审计 actor 的独立性按团队身份配置；单一账号不能靠“一个形式上的 APPROVED”解决独立审计。

通过 `gh api repos/ackness/pobr/branches/master/protection` 和
`gh api repos/ackness/pobr/rulesets` 核对实际配置，不以文档或 workflow 成功推断已强制执行。
服务端设置、push、PR、合并和发版按任务授权范围执行；本地流程改动不自动发布。

## 处理外部 PR

先阅读完整 diff，再在隔离 checkout 中执行外部代码；不使用真实账户配置或发布凭据。
常规路径是原 PR 的 review → request changes → 作者更新 → 最终 head/base 复验 → 授权合并，
采用上面的交接与检查要求。需要集成当前 master 时先检查冲突，再按影响运行相关检查，
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
cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown TotalDPS,Life   # How the stats asked for arrive
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

- `bash devs/scripts/driver.sh smoke`：聚合、解析、Build Code 的代表测试，不先构建整个工作区；不能替代完整门禁。
- 本地 `.agents/skills/run-pobr/driver.sh` 转发到同一驱动（`.agents` 按仓库规则不入库）。`smoke` 是环境检查，已有相关测试通过后无需再补一次。
- `python3 devs/scripts/test_workflows.py`：检查脚本失败传递、临时目录清理与工作区保护，以及 `compare` 的来源对齐和 `lookup` 的查找（用合成数据），不调用真实 Cargo。
- `devs/scripts/regen-check.sh` 在临时副本中重生成，保留 `overlay-common` 与 examples 语料，不写入或恢复工作区文件。
- `node web/scripts/bench-calc.mjs`：用三个已提交构筑测真实 WASM 的非缓存计算和 16 项批量计算。
  需要先构建 WASM、同步数据；`--reference <old-pkg>` 另检查完整输出是否一致，适用于保持行为的优化。
  报告保存在 gitignore 的 `.cache/`，只按需运行，不属于 CI 门禁。
- 本机性能与响应一致性检查需要 `diagnostics` feature，按需运行，不属于 CI 门禁。
  下列命令均加 `--release -- --ignored --nocapture`：
  - `cargo test -p pobr-wasm --features diagnostics --test perf_uncached`：三个构筑的 Web 重算路径，
    包括完整重算、打开属性解释的后续请求和逐技能摘要。每个样本增加不同词条，避免响应或计算缓存代答。
    `perf_timing` 则重复同一请求，测缓存命中。
  - `cargo test -p pobr-wasm --features diagnostics --test perf_corpus`：默认遍历四个集合：
    `demo-bd-test/builds`、`wasm-benchmark/builds`、`reference-builds/tuned/builds`、
    `reference-builds/ninja-top10-2026-10-26/builds`。在本次文档核对的 `81f8733e` 上共 325 个，不包括 holdout。它测无明细重算、默认明细、单属性明细、逐技能摘要和天赋变体，输出响应哈希与耗时。
    `POBR_CORPUS_OUT=<file>` 保存表；在两个引擎版本上比较哈希，可检查响应是否逐字节保持一致。
    `POBR_CORPUS_ONLY` / `POBR_CORPUS_LIMIT` 缩小范围，`POBR_CORPUS_DIRS` 可替换默认集合，
    `POBR_CORPUS_DUMP=<dir>` 保存用于定位差异的响应。
    `POBR_CORPUS_STAGED=1` 使用浏览器分阶段注入字节、解析后释放原文件的方式加载数据，哈希应不变。
    `POBR_CORPUS_INLINE_DESCRIPTORS=1` 把明细类型表放回响应，便于与它改为可选前的记录比较。
    同一个二进制重复运行却得到不同哈希时，先修复该构筑的非确定性，再比较版本。
  - `perf record --call-graph dwarf` 可查看上述运行的时间花在哪里。
    git 历史中记录的早期分析主要耗时在来源整理和特殊规则匹配，不是公式；不能当作当前版本的新测量。
- `pnpm --dir web package-wasm`：将已准备 WASM 与同步数据打包到 `.cache/wasm-release/`；`pnpm --dir web smoke-wasm-package` 解压后执行真实计算。`build-wasm` 校验源码、工具链、构建设置和产物记录，未变时直接复用；`pnpm --dir web build-wasm --force` 显式重建但不清 Cargo 缓存。打包继续校验源码 / 绑定指纹、实际 schema 和数据快照，来源 commit 保留编译时值。脚本改动运行 `node --test web/scripts/package-wasm.test.mjs web/scripts/sync-data.test.mjs`；发布流程见 [WASM 打包说明](wasm-package.md)。release workflow 在完整门禁通过后自动附加 Release 资产，手动 CI 仅保存 Actions artifact。
- `cd web && pnpm test:worker`：在实际 workerd 运行时测试 Worker，使用合成上游响应，无外网依赖。E2E 失败的截图与 trace 由 CI 上传为 `playwright-failure`。
- 计划发版时，在功能 PR 中一并更新 workspace 版本。本地完成相关验证、审计与 PR 完整门禁后合并，再推送一次 tag；release workflow 验证后发布，稳定 v0 版本按部署规则上线，无需在 master 额外手动运行同一套 CI。
