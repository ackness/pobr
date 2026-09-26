# Development and CI workflow

日常修改只运行相关验证；完整发版门禁交给云端 CI，不要求本地先跑一遍。
根目录 `./pobr` 是开发命令入口，与应用二进制 `pobr` 分开。
旧的 `.claude/skills/run-pobr/driver.sh` 命令仍然可用。

## 常用命令

```bash
./pobr help
./pobr targets                              # List test targets without compiling
./pobr verify build skills support_gating:: # Targeted tests, fmt and Clippy
./pobr test core parser mod_parser::        # Select one suite and filter
./pobr test item lib                        # Library unit tests
./pobr lint build skills                    # Targeted lint only
./pobr build                                # Build the application CLI only
./pobr build --workspace                    # Explicitly build all binaries
```

短参数中 `build` 对应 `pobr-build`，`core` 对应 `pobr-core`；完整包名也可用。
`lib` 选择单元测试，其他名字选择 `--test <suite>`。不传目标会显示用法并退出，
不会默认启动全量测试。需要多个目标、features 或其他选项时直接传 Cargo 参数。

```bash
./pobr test -p pobr-build --test skills --test codec
./pobr lint -p pobr-build --lib --test skills
./pobr test build parity no_regression
```

测试本身会编译，不必先执行 `check` 或 `build`。仅用测试名称过滤仍可能编译
该 crate 的所有集成目标，应同时选择 `--test` 或 `--lib`。跨 crate API 变更
还需要调用方的契约测试，不能只验证被修改的库。完整选择规则见
[CLAUDE.md](../CLAUDE.md)。Web 命令继续使用 `pnpm --dir web`。

## 云端验证与发布

```bash
# Push the intended commit first; requires authenticated gh.
./pobr ci feature/my-change
./pobr ci-status --branch feature/my-change

# Explicit local Rust gate; does not include Web checks.
./pobr full
```

`ci` 会请求 GitHub Actions，测试远端指定分支/tag。它不会推送本地代码，
也不会把“请求成功”当成“验证通过”；检查结果时必须核对提交 SHA。
远端默认分支需要已存在 `workflow_dispatch` 工作流，且指定 ref 必须存在。
普通 push/PR 仍不自动触发 CI，避免为每次提交付全量成本。

发版时更新根 `Cargo.toml` 中的 `[workspace.package].version`，并刷新锁文件
中的应用/工具版本，例如 `cargo metadata --format-version 1 > /dev/null`
（解析依赖，不编译）。检查 diff，保留七个 `crates/` 库的独立版本。
Vite 的发布标签和 WASM 包版本继续使用 workspace 版本。
版本号更新本身不执行测试，也不要求本地完整门禁。
游戏数据的活动版本也在运行时读取 `data/CURRENT`，不再通过 `include_str!`
嵌入基础库。更新数据仍须通过来源、快照完整性和相关数值检查，但不再因为
活动版本标记变化而使所有基础库失去编译缓存。

提交并推送版本 tag 后，由 tag CI 完成 Rust lint、Rust 测试和 Web 三路门禁；
全部成功后才部署和发布 WASM。不要为同一发版再手动请求一遍相同的完整 CI。
发布仍必须通过完整门禁，未执行或失败的验证不能算作通过。
CI 并行减少串行等待，但可能增加总 runner 用量；缓存可能过期，不能假定冷构建
会消失。当前改动不增加定时或每次 push 的自动 CI。

## 找到慢在哪里

```bash
./pobr timings build parity                # Compile only and write HTML timings
# Open target/cargo-timings/cargo-timing.html

# Compare execution after compilation, keeping targets and features identical.
time ./pobr test build parity
cargo nextest run -p pobr-build --test parity
```

Cargo 的 `--timings` 报告能看到每个编译单元、依赖等待和关键路径。
冷缓存和热缓存应分别记录；不要为了测量或“修缓存”随手 `cargo clean`。
`nextest` 仍先编译测试程序，然后按测试用例启动独立进程并行执行；它不能免除
编译成本。大量规则/游戏数据初始化时，进程间不能共享 `OnceLock`，应实测
Cargo 与 nextest 的执行成本再选择，不能保证换运行器一定更快。
参见 [Cargo timings](https://doc.rust-lang.org/cargo/reference/timings.html) 与
[nextest 执行模型](https://nexte.st/docs/design/how-it-works/)。

当前工作区有 44 个集成测试目标，已经按领域合并过；深层 `tests/` 文件是模块，
并非每个文件都会单独链接。减少编译成本应优先限制目标、保留稳定缓存输入，
再根据计时报告处理关键依赖。不要仅按 crate 数量或测试数量删代码。
同一 worktree 的 Cargo 命令串行执行，不共享不同 worktree 的可写 target。
不在此次修改中引入新 linker、全局 RUSTFLAGS 或共享缓存服务。

## 保留断言，按需运行报告

以下三项仅用于展示诊断，默认忽略，避免重复遍历整批构筑。
它们保留为显式命令，方便定位数值差异。

```bash
./pobr test build parity parity_baseline_report -- --ignored --nocapture
./pobr test build parity effective_switch_dual_run_report -- --ignored --nocapture
./pobr test build parity ehp_dual_run_report -- --ignored --nocapture
```

默认仍执行两种模式的 `no_regression` 门禁、golden/契约检查和含独立语料健康
断言的 `corpus_unsupported_report`。已有性能诊断和 fixture 生成器也保持按需运行。
慢测试如果保护独立行为，就先减少重复初始化或优化测试结构，不能直接删掉断言。

本次版本缓存实验在临时两成员工作区进行：只提高 workspace 应用版本后，固定版本
的库仍显示 `Fresh`，应用重新编译。真实 PoBR checkout 当时没有 `target` 缓存，
没有测出全量前后耗时，因此不能承诺“30 分钟降到几分钟”。
