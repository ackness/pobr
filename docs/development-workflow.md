# Development and CI workflow

根目录 `./pobr` 是 Rust 开发入口，与应用二进制 `pobr` 分开。
它转发到 `.claude/skills/run-pobr/driver.sh`；本机可选的 `.agents` 入口也转发到同一驱动。
本文维护构建、测试、缓存和 CI 操作；验证范围与计算约定见 [CLAUDE.md](../CLAUDE.md)，
Web 初次准备见 [Web README](../web/README.md)，数据更新见 [pipeline/README.md](../pipeline/README.md)。

## 工具链来源

| 工具 | 当前配置 |
|------|----------|
| Rust | edition 2024，CI 使用 stable；仓库未钉定 Rust 版本。复用本机可用工具链，lint 需要 rustfmt/Clippy |
| Node | 本地 `mise.toml` 指定 Node 26；CI 指定 Node 22 |
| pnpm | 使用 `web/package.json` 的 `packageManager` 版本 |
| WASM | `wasm32-unknown-unknown` target + wasm-pack，仅构建 Web 引擎时需要 |
| cargo-nextest | 本地可选，CI 安装；完整门禁另跑 doctest |
| LuaJIT | vendor 抽取、oracle 及相关工具测试需要；普通计算使用已入库数据 |

`./pobr status` 查看环境就绪态。已有可用环境时跳过 bootstrap 和安装，
不为日常测试更新工具链或重复构建整个 workspace。

## 日常命令

```bash
./pobr targets                              # List current suites without compiling
./pobr verify build skills support_gating:: # Selected tests, fmt and Clippy
./pobr test core parser mod_parser::        # One integration suite and filter
./pobr test item lib                        # Library unit tests
./pobr lint build skills                    # fmt and selected Clippy target
./pobr build                                # Application CLI only
./pobr cache                                # Read-only storage and file counts
```

短参数 `build` 对应 `pobr-build`，`core` 对应 `pobr-core`；完整包名也可用。
`lib` 选择单元测试，其他名字选择 `--test <suite>`。不传目标会显示用法并退出，
不会默认启动全量测试。多个目标、features 或其他选项直接传 Cargo 参数。

```bash
./pobr test -p pobr-build --test skills --test codec
./pobr lint -p pobr-build --lib --test skills
./pobr test build parity no_regression
./pobr build --workspace                    # Only when all binaries are needed
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

根 `Cargo.toml` 的 dev/test 使用 `debug = "line-tables-only"` 和 `incremental = false`。
前者保留回溯行号，后者停止积累每个 crate 的增量对象图和中间文件。
Cargo 仍按输入指纹复用未变的依赖、库和测试程序；重复相同测试无需重新编译。
修改源码时会重新编译受影响的 crate，关闭增量编译可能增加这一步的耗时。
长期连续编辑同一 crate 时可显式选择 `CARGO_INCREMENTAL=1`，但应保持设置稳定，
避免反复切换产生两组产物。test 继承 dev 配置，release/WASM 继续使用原有发布 profile。
参见 [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#incremental)
和 [build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)。

首次采用新 profile 时，已有 workspace 产物会重新编译一次。
新设置不会删除此前的 incremental、旧工具链或旧 features 产物。
不要把 `cargo clean` 当构建前置步骤；改变工具链、profile、features、RUSTFLAGS 或 target
都会影响缓存复用。调试变量时可临时使用 `CARGO_PROFILE_DEV_DEBUG=2`，
测试使用 `CARGO_PROFILE_TEST_DEBUG=2`，完成后恢复日常设置。

```bash
./pobr cache
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

WASM 构建凭据校验 Rust 源码、嵌入语言文件、manifest/lockfile、JSON 契约、
工具版本、相关环境/Cargo 配置，以及绑定哈希与实际 schema。
输入和产物未变时跳过 wasm-pack，保留原编译 commit 与 dirty 标记；
缺失、损坏或旧凭据需要一次正常重建。普通 UI、文档和运行时游戏数据修改不会触发 WASM 重编译；
游戏数据变化仍需 `pnpm --dir web sync-data`。

这些设置减少后续产物，并非磁盘配额或自动清理器。`.cache/` 中历史诊断、vendor 差异副本、
发布包以及旧 worktree 仍可能累积；先确认具体目录及保留需求，再显式清理。
脚本不会自动删除发布资产、用户资料或其他 worktree。

## 云端验证与发布

```bash
# Push the intended commit first; requires authenticated gh.
./pobr ci feature/my-change
./pobr ci-status --branch feature/my-change

# Explicit local Rust gate; Web checks are separate.
./pobr full
```

合并、发版、工具链/features/依赖变更或影响范围不明的核心修改需要完整门禁，
优先使用 CI。`ci` 测试已推送的远端分支/tag，不推送本地代码；必须核对结果的提交 SHA，
请求成功不等于验证通过。普通 push/PR 不自动触发 CI。

当前 [ci.yml](../.github/workflows/ci.yml) 包含独立的 Rust lint、Rust 测试和 Web 门禁。
Rust 检查 fmt/Clippy/nextest/doctest/i18n 与数据兼容；Web 检查脚本、类型、单测、Worker、
真实 WASM、打包与 E2E。`./pobr full` 仅运行 Rust 门禁，包含 fmt/Clippy、nextest + doctest
（无 nextest 时使用 Cargo）和 i18n。离线或排查 CI 时可显式使用，无需重复已通过的云端全量验证。

应用发版更新 `[workspace.package].version`，用 `cargo metadata --format-version 1 > /dev/null`
刷新应用/工具的锁文件版本，保留七个库的独立版本。数据活动版本在运行时读取 `data/CURRENT`，
与应用版本分开。版本更新仍需对应检查，但不因应用版本或活动数据标记变化使所有基础库失去缓存。

推送 `v*` tag 后由 CI 完成门禁，通过后发布 WASM 资产；Cloudflare 部署仅接受 `v0.*` tag。
不要为同一个 tag 再请求相同的 CI，
也不要求本地先跑一次 full。CI 依赖缓存可能过期，不能保证每次都是热构建。
发布包与凭据规则见 [WASM 打包说明](wasm-package.md)。

## 慢构建与按需报告

```bash
./pobr timings build parity                # Compile selected suite with Cargo HTML timings
# Open target/cargo-timings/cargo-timing.html

time ./pobr test build parity              # Compare unchanged cold/warm inputs
cargo nextest run -p pobr-build --test parity

./pobr test build parity parity_baseline_report -- --ignored --nocapture
./pobr test build parity effective_switch_dual_run_report -- --ignored --nocapture
./pobr test build parity ehp_dual_run_report -- --ignored --nocapture
```

`--timings` 显示编译单元耗时与依赖等待。使用重定向后的 target 时按 Cargo 输出定位报告。
不要为了测量清缓存。测试目标由 `./pobr targets` 读取当前 manifest；深层 tests 文件通常是
汇总套件的模块，并非每个文件都会单独链接。

nextest 按用例启动独立进程，进程间不共享 `OnceLock`；大量游戏数据初始化时应实测运行成本。
日常定向测试默认 Cargo，完整门禁使用 nextest 并补 doctest。
上述三个 parity 报告仅作诊断，默认忽略；两种模式的 no_regression、golden、契约和
独立语料健康断言继续默认执行，不为加速删减回归覆盖。
