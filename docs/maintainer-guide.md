# Maintainer guide

Detailed repository guidance for contributors and coding agents: build environment, validation scope, architecture and conventions. [AGENTS.md](../AGENTS.md) contains the concise contributor guide.

> **PoBR** = Path of Building in Rust。目标是把 PathOfBuilding（Lua）的核心计算引擎迁移到 Rust：解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈，并在 PoB 兼容的基础上额外提供 source-level 归因（每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献）。

## 构建环境

开发命令是根 `mise.toml` 里的 mise task（`mise tasks` 查看全部，如 `mise run verify build skills`），也可直接使用 Cargo。游戏 CLI（`apps/pobr-cli`，二进制 `pobr`）和 WASM 包只包含游戏计算能力，不承载测试、lint、CI 等开发命令；新增开发命令加到 mise task，不要加进 CLI 或 WASM 接口。日常命令、工具链来源和缓存管理统一见 [开发与 CI 工作流](development-workflow.md)。定向测试和完整门禁统一使用 Cargo，套件内复用只读 fixture。

- 每个 worktree 复用**自己的 `./target`**。同一 worktree 的多个会话共用该目录、串行运行 Cargo，不为会话或门禁创建额外 target。**禁止**不同 worktree 共享可写 `CARGO_TARGET_DIR`——并发 cargo 会在构建目录锁上串行排队（症状：长时间无输出；stderr 的 `Blocking waiting for file lock` 提示不要用 `| tail` 等管道吞掉）。
- 同一 target 目录下 cargo 命令**一次一条、前台执行**，禁止后台叠加。

## 验证分层（本地提交默认定向验证）

**先按改动选检查，通过后结束验证并提交。普通本地提交不要求全量门禁。** 不把 `check → build → clippy → test → full` 当作固定流水线；已有相关测试能检查编译时，不先重复运行 check/build。

| 改动 | 最小相关验证 |
|------|------|
| Rust 局部逻辑 | `driver.sh test -p <crate> --test <suite> [filter]`；收尾跑 `driver.sh lint -p <crate> --lib --test <suite>` |
| 一条词条规则（`special_mods.json` / `skill_stats.rs`） | 规则写 `examples`，`mise run regen` 校验规则与示例并重生成产物；名字没有读取方时补一条引擎测试。步骤见[改动配方](recipes.md) |
| 计算 / Modifier / parser | 对应集成套件的定向用例；完整 build 数值（parity、accuracy）由合并后的批次门禁 `mise run gate` 统一测量 |
| 跨 crate API / 数据结构 | 相关 crate 测试 + 直接使用者的契约或集成测试；边界无法明确时扩大到 workspace |
| Web TS / React | `pnpm --dir web test <test-file>` + `pnpm --dir web typecheck`；交互改动补相关 Playwright spec |
| Rust → WASM / Web 契约 | Rust 契约测试 + 重建 WASM + 相关真实 WASM / E2E 测试 |
| Worker | `pnpm --dir web test:worker` |
| 仅文档 | 审查 diff；不运行 Cargo / Web 构建 |
| 验证脚本 | `bash -n <script>` + `python3 devs/scripts/test_workflows.py`；不为改脚本重跑整个 workspace |

**批次门禁。** 多个改动（并行代理的分支或同一分支的多条词条）合并后，由合并者在已编译的 checkout 上运行一次 `mise run gate`：fmt、规则产物漂移、Clippy、各 crate 套件、parity、i18n、Web、accuracy，每一步都运行，最后列出失败项；修复后 `mise run gate --only <步骤>` 只重跑失败项。单个改动不重复运行这些步骤：它们分别编译 parity 测试程序、CLI 和全部测试目标，并重算全部构筑。新 worktree 用 `mise run warm <已编译的 checkout>` 写时复制克隆其构建缓存；分析工具设 `POBR_BIN` / `POBR_CACHE_DIR` 复用已有 CLI、台账和 PoB2 结果，不再各自编译、重算（见[改动配方](recipes.md#working-in-a-new-worktree)）。

表中 `driver.sh` 指 `bash devs/scripts/driver.sh`。测试目标先查对应 `Cargo.toml` 或 `tests/*.rs`；用 `--test <suite>` / `--lib` 限定编译目标，只有名称 filter 仍可能编译整个 crate 的测试。计算修复必须有能复现错误的断言，不降低 parity 基线来换取通过（对照值本身有误时的做法见“Parity 体系”）。

- 编辑中只有需要快速定位编译错误时才单独 `cargo check -p <crate> --lib`。Clippy 按改动选择 `--lib` / `--bin <name>` / `--test <suite>`；测试专属改动无需扩大到 `--all-targets`。
- 同一代码、依赖、工具链、features、数据版本下已通过的检查复用结果；后续只改文档不使代码测试失效。失败后先重跑失败目标，只有新改动、失败或明确影响范围才扩大验证。
- Web 纯 TS/CSS 改动复用已构建 WASM；WASM 产物缺失，或 WASM 及其 Rust 依赖的源码、features、工具链改变时重建。`pnpm build` 已包含 typecheck，最终需要 build 时不额外重复 typecheck。E2E 前确认 dist 对应当前代码。
- **合并 / 发版、工具链 / Cargo features / workspace 依赖变更、影响范围不明的核心改动**需要完整门禁，优先交给 CI。发版由 tag CI 完成；其他场景先推送分支，再运行 `mise run ci <remote-ref>`，并检查该提交的结果。本地不再强制先跑 `full`；离线或排查 CI 时可显式运行 `mise run full`（仅 Rust 门禁，Web 另验）。完整 CI 在 tag、手动触发、发版 PR（workspace 版本变化）和每晚定时（master 当天有新提交时）运行；普通 PR 和 master 的每次推送自动运行的是轻量检查（[pr.yml](../.github/workflows/pr.yml)），不能替代完整门禁；发起 CI 不等于通过。相同代码不重复运行本地和云端全量门禁。
- 不自动 `cargo clean`、改 profile / features / `RUSTFLAGS` 或共享 target 来“加速”；这些可能使缓存失效。看到构建锁先检查已有进程，不叠加 Cargo 命令。

## 常用命令

```bash
cargo test --workspace                                 # 全部测试（含 doctest）
cargo clippy --workspace --all-targets --features pobr-core/benchmarks,pobr-build/benchmarks,pobr-wasm/diagnostics -- -D warnings # Full lint, including manual tools
cargo fmt --check                                      # 格式检查（CI gate）

cargo test -p pobr-core --test aggregation mod_db::   # 定向编译套件 + 过滤用例
cargo test -p pobr-build --test skills support_gating::
cargo bench -p pobr-core --features benchmarks --bench mod_db_bench          # ModDB 热查询基准（criterion）

# PoB2 parity 仪表盘：逐 build 打印 PoBR vs PoB2 对照 + 聚合命中率
mise run test build parity parity_baseline_report -- --ignored --nocapture
# 其中 parity_no_regression 用例是回归门禁（命中率不得低于已记录基线）

# 数值不对时的第一步：基准组与 tuned 组的全部构筑一次对照参考值和钉定 PoB2，差异分类并跨构筑汇总原因
mise run triage [组目录 | 构筑目录 ...] [--jobs N] [--refresh] [--json <文件>]
# 游戏词条缺口队列：按 benchmark + tuned 中丢掉该输入的构筑数、玩家来源与来源数排序
mise run gaps [--json <文件>] [--limit N] [--kind unique|passive|mods|compound|jewel_node|bases] [--all]
# 一行词条的研究卡：变体、游戏 stat、完整来源文本、逐构筑去向、引擎名字与近似规则
mise run gap "<模板或词条文本>" [--json <文件>]
# 一个构筑的导入损耗、与构筑码自带参考值的差异、计算中丢掉的全部输入
mise run diagnose <构筑目录 | 构筑码文件 | 已入库构筑名片段>
# 清单解释不了的差异：同一构筑与钉定 PoB2 逐个 stat 对照（需 vendor 与 luajit；PoB2 结果按构筑码缓存）
mise run compare <构筑目录 | 构筑码文件 | 已入库构筑名片段> [Stat ...]
# 一个 id 或名字在游戏数据里的记录：技能、stat、物品基底、天赋节点、暗金
mise run lookup <IceShotPlayer | stat id | "Fox Idol" | 节点编号或名字 | 暗金名> [--level N]
# 构筑码对照它们自带的参考值（无需 vendor / luajit）：不带参数给出每组已入库构筑的总数，带目录给出该组每个构筑的差异
mise run accuracy [含 */code.txt 的目录]
# 从 poe.ninja 抓一组当前赛季构筑（每个升华一个，选择不依赖计算结果）
mise run builds:fetch <组目录> [--pick first|last]
# 每颗玩家主动宝石的主授予效果：用最小合成构筑同时计算 PoBR 与钉定 PoB2，按技能族汇总差异
mise run skills:sweep [--jobs N] [--refresh] [--no-build] [--skill <效果 id> ...] [--json <文件>] [--text <文件>]
# 侧栏字段的覆盖：按 display_catalog 的 PoB key 逐字段对照 oracle 的完整输出（拒绝 holdout）
python3 devs/scripts/output-coverage.py [组目录 | 构筑 ...] [--oracle-cache <目录>] [--refresh-missing] [--json <文件>]

# CLI（apps/pobr-cli，二进制名 pobr）
cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown TotalDPS,Life  # 所列 stat 的计算步骤与读到的词条（Minion:Life 为主技能召唤物）
cargo run -p pobr-cli -- calculate-build --file <code.txt> --usage [--json]          # 逐行台账：每一行词条的去向、影响的 stat、没用上的原因
cargo run -p pobr-cli -- calculate-build --file <code.txt> --full-dps                # 另算勾选进 Full DPS 的全部技能：合计与逐技能贡献
cargo run -p pobr-cli -- decode-code <pob_code>        # PoB Build Code → XML
cargo run -p pobr-cli -- parse-mod "20% increased Fire Damage"

# 工具
cargo run -p sync-pob-catalog -- scan  --pob-root <PoB路径> [--out catalog.json]
cargo run -p sync-pob-catalog -- check --pob-root <PoB路径> --catalog catalog.json
pobr_data_version="$(cat data/CURRENT)"
cargo run -p sync-pob-catalog -- extract-lua --vendor-root "$PWD/vendor/PathOfBuilding-PoE2/src" \
    --out "data/$pobr_data_version/overlay/skill_overrides.json"  # Requires pinned vendor + LuaJIT
cargo run -p lint-i18n                                  # 语言包完整性检查
tools/pob2-oracle/run.sh <build.xml>                    # PoB2 headless oracle：dump Lua 侧完整计算分解为 JSON（需 luajit；非 workspace 成员）
```

- Rust **edition 2024**；根 `[workspace.package].version` 是应用/工具发布版本，与 v0.x tag 同步。七个 `crates/` 库使用独立版本，日常应用发版不改库版本，避免仅改发布号就使底层库和所有使用者重新编译。库版本不代表当前应用发布号；将来单独发布库时再按 API 兼容性管理。
- 根 `Cargo.toml` 的 dev/test 对第三方依赖使用 `opt-level = 3`（测试加载 JSON 数据快照，未优化的 serde_json / sha2 使一次加载从约 0.65 s 变为约 3.4 s）；workspace crate 保持未优化的快速编译，不要为“统一”去掉这项设置。dev/test 保留 `debug = "line-tables-only"`，默认 `incremental = false`，减少增量中间文件并继续复用未变的 Cargo 产物。修改源码后的重编译可能更慢；长期连续编辑时可显式启用 `CARGO_INCREMENTAL=1`。调试变量时可临时使用 `CARGO_PROFILE_DEV_DEBUG=2`（测试用 `CARGO_PROFILE_TEST_DEBUG=2`）。改变这些设置会生成另一组缓存，不作为日常准备步骤。
- 完整 CI 以已入库的 [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) 为准（PR 轻量检查见 [`pr.yml`](../.github/workflows/pr.yml)）：Rust 检查 fmt / Clippy / Cargo tests（含 doctest），Web 独立检查类型、单测、Worker、WASM 构建与打包。E2E 默认关闭，仅 `mise run ci <ref> --e2e` 显式请求时运行；不要在未被要求时发起。涉及计算/Modifier/parser 的改动需补对应的集成测试或 golden fixture。

### 快速检查与发版

mise task 的日常命令、缓存、按需诊断、WASM 打包、Worker 测试和发版步骤统一见
[开发与 CI 工作流](development-workflow.md)；外部 PR 的处理见其中“处理外部 PR”一节。

- `mise run verify build skills support_gating::` 一次执行定向测试和 lint；`mise run targets` 列出测试目标，不编译。
- `bash devs/scripts/driver.sh full` 是完整 Rust 门禁，只用于上面列出的完整门禁场景，不是每次本地提交的收尾动作。
- PR 自动运行轻量检查（fmt / Clippy / 冒烟 / parity 回归；改动 `web/` 时加类型检查和单测）；完整门禁在 tag、手动触发、发版 PR 和每晚定时运行。发版 PR 的完整门禁通过后再合并、打 tag。


## Workspace 结构

当前结构以根 [Cargo.toml](../Cargo.toml)、各 crate 的 `src/lib.rs` 和契约/回归测试为准。workspace 有 13 个成员：7 个库、2 个应用入口、4 个 Rust 工具；`web/` 与 Lua oracle 独立于 Cargo workspace。

`devs/docs/architecture/` 中的 18–21 文档记录具体功能契约。其余以本文件、[Web README](../web/README.md)、[数据管线说明](../pipeline/README.md) 和代码为准。

| Crate | 职责 | 主要依赖 |
|-------|------|------|
| `crates/pobr-data` | 纯数据定义，零逻辑、零 I/O，所有 crate 的底层依赖。核心是 `catalog.rs`（入库 JSON schema：`BaseItemDef`/`StatDef`/`ModDef`/`SkillGemDef`/`GrantedEffectDef`/`PassiveNodeDef`/`DataManifest` 等），另有 damage/build_config/display_stat/stat/monster 等域类型 | 无（外部依赖 `serde`） |
| `crates/pobr-core` | Modifier 解析/存储/聚合 + 计算引擎 + source-level 归因 + 来源接入（item/passive/gem/flask ingest）。零 I/O | `pobr-data` |
| `crates/pobr-gamedata` | 运行时数据 loader——数据系统里唯一持有文件 I/O 的层。把 `data/<poe_version>/` 入库 JSON 反序列化为 `pobr-data::catalog` 类型（`GameData::new(version_dir)`，按域懒加载 + i18n 边车；`repo_data_root()` 定位仓库数据目录） | `pobr-data` |
| `crates/pobr-i18n` | 语言包加载 / fallback / 显示文本映射；`en-US`（canonical）+ `zh-TW`，locale TOML 在构建期校验并生成内嵌字符串表，运行时不包含 TOML parser | `pobr-data` |
| `crates/pobr-tree` | 天赋树拓扑、allocated node mod 收集、范围珠宝（first pass） | `pobr-data` |
| `crates/pobr-build` | Build 状态、PoB Build Code 编解码（XML ↔ zlib ↔ URL-safe Base64，padding 容错）、导入识别、`calc_orchestrator/`（`calculate_with_data` / `calculate_skill_metrics`）与 `CalcCache`、Build 对比。**parity 测试的主战场**（见下） | `pobr-data`/`pobr-core`/`pobr-tree`/`pobr-gamedata` |
| `crates/pobr-item` | raw item 文本的**全保真编辑态**解析 + 逆向序列化（`draft.rs`/`annotations.rs`/`build_raw.rs`）。职责边界清晰：**calc 视图**（剥标注 / variant 门控 / range 取值后喂引擎）由 `pobr-core::item_text` + `item::ingest_item` 承担；**编辑态视图**（保留 calc 刻意丢弃的 variant 名列表 / 行级标注 / 未建模标注，支持 BuildRaw 往返）由本 crate 的 `ItemDraft` 承担。modifier 行保留原始文本，计算解析由 build/core 链路承担 | `pobr-data` |
| `apps/pobr-cli` | CLI：`calculate` / `calculate-build` / `parse-mod` / `explain-mod` / `decode-code` / `encode-code` / `parse-item`（命令逻辑在 lib，便于测试） | `pobr-build`/`pobr-core`/`pobr-data`/`pobr-gamedata` |
| `apps/pobr-wasm` | `build_api/` 提供 JSON 契约；默认 features 可在宿主测试，`wasm` feature 启用 wasm-bindgen 绑定 | `pobr-build`/`pobr-core`/`pobr-data`/`pobr-gamedata`/`pobr-item`/`pobr-i18n` |
| `tools/pobr-data-adapter` | 数据管线适配器——GGG `.dat` 导出 → 解析外键、反范式化为入库最小 JSON 落到 `data/<poe_version>/`。直接调用时缺列默认告警降级，正式再生成始终使用 `--strict-columns` 并校验原始表版本收据；产物 `_meta.regen_command` 记录再生成命令 | `pobr-data` |
| `tools/sync-pob-catalog` | 从 PoB 核心 Lua 抽取属性 catalog、parity 检查/diff、vendor Lua → overlay JSON | `pobr-data`/`pobr-core`/`pobr-gamedata` |
| `tools/lint-i18n` | 语言包完整性检查（非 canonical 语言不得有 en-US 之外的多余 key） | `pobr-i18n` |
| `tools/precompile-mods` | M6 mod-parser 规则离线预编译 / codegen 工具：把四层语料（build XML / passive_tree / special_derived / `--corpus-extra`）去重后逐行过 `pobr-core::parse_mod` 预解析，产出 `data/<version>/generated/parsed_mods.json` + 覆盖率报表（当前为离线回归产物；生产 `ParseCtx` 使用编译后的规则，`ModCache` 仅为单一规则快照内的内存 memo） | `pobr-data` + `pobr-core` + `pobr-gamedata` |
| `tools/pob2-oracle` | **非 workspace 成员**（纯 Lua wrapper）：把 vendored PoB2 引导成 headless，加载 build 并 dump Lua 侧完整计算分解（中间值+最终值）为 JSON，用于钉死逐分量偏差。不修改 vendor 源 | luajit |

`pobr-data` 是最底层，不依赖其他项目内 crate。`pobr-build` 组合计算、物品、树与数据加载；应用层使用这些能力。生产计算核心不读取数据文件；`test-rules` 是仅供测试的规则加载 feature。

| 非 workspace 目录 | 当前职责 |
|-------------------|----------|
| `web/src/` | React / TypeScript / Vite；`api/` 定义 JSON 边界，`hooks/useBuildSession.ts` 编排编辑与重算，`components/` 与 `lib/` 承担界面和规划 |
| `web/public/` | 静态资源、同步后的游戏数据、市场复制 userscript，以及 `_worker.js` 的 WeGame / 市集 HTTP 适配 |
| `web/e2e/`、`web/worker-tests/` | Playwright 用户流程与实际 workerd 运行时检查 |
| `data/` | 按版本保存的 base / overlay / generated / i18n 数据；默认版本见 `data/CURRENT` |
| `pipeline/` | 数据下载、抽取、再生与版本差异工具 |

Web 的 `api/wasmBackend.ts` 在浏览器中加载 WASM，调用 `apps/pobr-wasm/src/build_api/`；`web/src/api/types.ts` 镜像 JSON DTO，`apps/pobr-wasm/tests/contract_golden.rs` 约束契约。启动时由 JS fetch 数据，经 `stageDataFile` / `initStagedData` 建立内存 `GameData` / `BuildData`，随后计算读取内存数据。Pages Worker 处理受限的 HTTP 适配，计算仍在浏览器 WASM 中执行。原生调用通过 `GameData::new` 读取文件，WASM 使用 `GameData::from_memory`。

schema 3 的运行时域读取与历史树枚举受 manifest inventory 约束；未列文件在原生与内存端均视为不存在，patch 和 overlay-common 保留独立层语义。GameData 首次访问验证并缓存不可变快照，替换磁盘快照后必须创建新实例。触发源子计算复用已准备的 Build 视图，不能再次叠加品质；传入核心的 enabled-group 序号必须映射回原始组索引后才能修改选择。

**数据管线**：`GGG .dat 导出` →（`pobr-data-adapter` 离线适配）→ `data/<poe_version>/*.json`（schema = `pobr-data::catalog`，默认版本见 `data/CURRENT`，含 `overlay/` 人工修正层）→（`pobr-gamedata` 运行时 loader）→ 上层计算。游戏数据文件访问收口在 `pobr-gamedata`；下载、剪贴板及 HTTP 适配属于应用或工具边界。

宝石品质自 `4.5.5.2` 起由 adapter 的 `--gem-quality` 独立生成，沿用历史 `overlay/` 路径。
该入口始终严格校验三张官方输入表及版本收据；不受通用 `--strict-columns` 开关控制，
也不从 vendor 或旧产物动态推导范围。来源、兼容边界与升版步骤见[品质生成说明](../pipeline/gem-quality/README.md)。

特殊 Modifier 规则在编译时递归校验 flags、标签字段/作用对象、捕获、handler 和运算。
`precompile-mods --check` 校验运行时合并后的完整集合，并输出文件哈希、顺序和规则来源；
`refresh-modifiers.sh` 在审计和发布产物前执行此检查。`StatId` 仍是开放名称，解析通过不证明计算支持。
覆盖语义与边界见[维护者规则说明](contributing-mods.md#7-validate-test-and-open-a-pr)。
词条审计（`tools/precompile-mods/src/audit.rs`）先用运行时的显示文本清理（`mod_parser::engine::strip_display_markup`）展开游戏链接与样式，再去掉物品注记：`[Thorns|Thorns]`、`[Strength]` 这类术语是词条的一部分，删掉会改变机制。
审计的 `text_channel`（`gem_property` / `granted_skill`）标出由其他文本入口读取的行（宝石等级与品质、授予技能），只说明这一行有去向，不说明授予的技能都已实现。

### 数据与计算上下文约束

活动数据版本由 `pobr_gamedata::data_version()` 在运行时解析：`POBR_DATA_VERSION` → `POBR_DATA_ROOT/CURRENT`（默认仓库 `data/`）。两者都缺失时直接报错，代码里不内置任何版本号。测试和 PoB2 对照值一律针对活动版本：数据升级时用 `examples/demo-bd-test/tools/recapture_golden.py` 与 `recapture_panel_golden.py` 重采对照值，parity 掉格代表引擎与新版 PoB2 的差距，修引擎而不是降基线。仓库只保留活动版本加最近两个版本（`mise run data:prune`）；国际服与国服版本号规则不同，“最近”按快照入库顺序判断，不比较版本号。

schema 3 的 `manifest.json` 列出运行时文件及 SHA-256；`BuildData::load` 在可选域降级前校验完整快照。旧 schema 1/2 保留兼容行为。`patch/` 与 `overlay-common/` 是独立覆盖层，不属于快照哈希清单。正式数据和 Web 同步先完成候选生成/校验，再替换目录；失败恢复旧目录，不自动 bless golden。

`CalculationSession` / `Env` 只执行一次；再次 `perform` 返回 `AlreadyPerformed`。构筑计算入口返回 `CompletedCalculationSession`，只允许读取输出、来源和分解，不能再修改输入；手动组装会话使用 `finish(self)` 完成计算。Full DPS 使用消费式 `with_full_dps` 附加汇总，不开放输入修改。重算应创建新会话，避免再次应用资源转换。属性缩放与资源派生共享包含职业基础值及 INC/MORE 的最终属性；天赋关联的装备、品质和诊断查询都使用所选树。

`GameData::identity()` 对快照、共享覆盖层及有效 patch 的文件字节计算稳定摘要；`BuildData::load` 在加载时固定它，克隆共享缓存。原生目录与 WASM 内存数据使用同一算法。CLI 完整构筑、marginal 和 WASM 完整计算响应中的 `provenance` 包含 `engine_version`、`data_version`、`data_sha256`。摘要用于定位输入数据包，JSON 空白变化也会改变摘要，不代表计算语义变化；同版本开发构建还须保留源码 revision，完整复现还需要构筑输入与所选条件。修改磁盘数据后应创建新的 `GameData`，不能复用已加载实例。

Web 的计算、市场目录和天赋树使用初始化时固定的数据版本；重新初始化清除翻译与词缀层级缓存。节点规划逐节点计算，不能按原始词条文本共享包含位置/半径效应的结果。缺少历史树展示元数据时保留导入和计算，禁用不可靠的历史树编辑/规划。存档导入校验嵌套结构，非法存档不覆盖当前内容；启动失败保留原始字节并提供下载恢复。

历史树显式转换由 `pobr_build::tree_convert::preview_conversion` 生成独立 XML；WASM `preview_tree_conversion_json({ code, all_specs })` 返回新分享码与逐 Spec 的保留、删除及共用／两组武器的断路清单。节点须同时匹配编号、种类与身份；两边有稳定 slug 时比较 slug，否则比较去除展示标记的旧名及是否属于升华，数值平衡不作为删除依据。缺少历史目录时报错。当前版本和未选中的 Spec 原文不变；保留存续插槽、武器组专属点、属性选择和不透明 XML，失效插槽的珠宝留在全局物品池。连通性使用当前树及珠宝授权规则；断路节点保留，等待显式修复或退款，修改后的完整分配仍须通过连通性与点数检查（`pobr_tree::allocation::AllocationTree::edit`）。

`node_power_json` 的可选 `nodes` 每次最多 16 个，返回单点试算的 `delta` 与 `distance: 0`，不替调用方寻找合法路线。省略时沿用热力图的邻接搜索，`max_depth` 默认 5、最多 12；整条路线与每点收益由 Web 的完整变体计算，见 [Web 天赋树契约](web-contracts.md#web-passive-tree-editing)。

CLI 完整构筑与 marginal 结果包含数据版本、树版本和未支持词条，调用方必须保留诊断，不能把未支持内容视为零效果。`calculate-build` 另有 `missing_items`（构筑用到但导入器读不出的物品，结果是缺了这件装备的角色）、`reference`（与构筑码内 `<PlayerStat>` 的差异，以及 `<MinionStat>` 与主技能召唤物的差异，行名 `Minion*`、`pob_key` 为 `Minion:<stat>`；仅在请求沿用导出条件时给出）和 `output.minions`（计算创建的每个召唤物：怪物 id、召唤它的技能、它所用的技能 `skill_id`、等级、DPS 与防御）；`--breakdown <stats>` 时还有 `breakdowns`（每个 stat 的计算步骤和读到的词条，与 Web 的分解同源，见 `pobr_build::breakdown`；`Minion:<stat>`（`Life`、`TotalDPS` 等）是主技能召唤物的同一张表，含角色给它的词条；`pobr_build::breakdown::BREAKDOWN_STATS` 所列的 stat 都另有逐步计算；`TotalEHP` 与 `<类型>MaximumHitTaken` 不再把敌人一侧的命中输入列成词条行：它们不能相加，EHP 用到的项见下面的 `output.ehp`，敌人输入及其来源由 `CalculationSession::enemy_hit_mods` 给出，CLI 不输出 WASM 的 `derived` 明细）和 `main_skill`；`--full-dps`（或请求 `FullDPS` / `FullDotDPS` 的分解）时还有 `output.full_dps`、`output.full_dot_dps` 与逐技能的 `output.full_dps_skills`，WASM 请求带 `full_dps: true`，未请求时没有这些字段，也不拿未计算的 0 与参考值比较；`output.ehp` 是 EHP 用到的项：逐类型的敌人伤害、承受的部分和要耗尽的资源池，以及受伤命中次数、伤害几率、减免后的命中次数和不被命中的几率；`--reference-basis` 时 `output` 与分解都按参考值口径（`reference_basis`）；`--usage` 时还有 `usage`（逐行台账，见“排查数值差异”），它总是来自游戏口径、全部模型开启的计算，不随 `--reference-basis` 改变。WASM 的 `{ pob_code }` 请求把同类物品放进 `item_errors`；WASM 的 `line_usage_json`（JS 名 `lineUsageJson`）接受与 `calculate_build_json` 相同的请求，返回 `{ usage, item_errors }`，不缓存。

WASM 的 `calculate_build_json`（JS 名 `calculateBuildJson`）响应另有 `minions` 与 `main_minion_index`。`minions` 是计算创建的每个召唤物：`index`、怪物 `minion_id` 与名字、召唤它的技能 `summoned_by`、它所用的技能 `skill_id`、等级、DPS、生命、护甲、闪避、能量护盾，以及已支持的 `stats`、`breakdowns` 与 `breakdown_descriptors`；每项是一个召唤物的快照，不乘召唤数量，`stats` 包含火、冰、电、混沌抗性及其最大值、超上限值（与角色相同的 `*Resist` / `*ResistMax` / `*ResistOverCap` id）；四种最终抗性有逐步分解。未建模的值不出现，也不补 0。`main_minion_index` 是主技能召唤的那一个，按提供该效果的宝石与召唤来源定位（`reference_check::main_skill_minion`），同名召唤宝石各有自己的结果，主技能组禁用时为空。请求的 `breakdowns` 选择要生成的明细：省略时是 `BREAKDOWN_STATS` 加每个召唤物的全部明细，`[]` 不生成任何明细，列表只生成所列 id（先去重，不认识的 id 是参数错误）；`breakdown_actor`（`{kind: "player"}` / `{kind: "minion", index}`，索引属于本次响应，越界是参数错误）选择计算对象，每份明细带 `actor`。数值与 `actor` 不随所请求的明细变化，逐技能总览与 Full DPS 的缓存忽略这两个字段。`breakdown_descriptors` 给每个可展示字段一种明细（`pobr_build::breakdown::detail::detail_kind`）：`steps`（逐步计算）、`sources`（每组是引擎的一次 ModDb 查询，`pobr_core::calc::session::detail::source_names`）、`derived`（公式与实际输入，`CalculationSession::derived_detail`）或 `none`。新增明细先声明种类，不为凑齐明细把上游输入包成可相加的词条行：来源组直接用计算阶段同一份 ModDb 查询、配置与 MORE 精度；派生项重用实际的计算项并给出公式输入，测试验证这些输入能重建结果；WASM 测试还要验证空选择与未请求的 id 不运行明细生成。`config_context`（Config 页按它筛选配置项）只在请求带 `config_context: true` 时计算并返回：它走读取跟踪的入口 `calculate_with_data_session_tracked`，在分解查询之前取快照；读取跟踪在 benchmark 构筑上最多慢 17.5%（Blood Mage），所以普通重算不跟踪、不返回该字段。这些字段在界面上的行为见 [Web 功能契约](web-contracts.md)。

EHP 的 Calcs 字段直接读取计算保留的同一份 `EhpTerms`：五种类型（Physical / Fire / Cold / Lightning / Chaos）的 `EnemyDamage`（敌人加成与暴击后）、`TakenDamage`（承伤转换后、减免前）、`TakenHit`（减免后）、`TakenHitMult`（后两者之比；转换后输入为 0 时为 0）和 `EnergyShieldBypass`（百分比），三种元素的 `DamageReduction`，以及 `totalEnemyDamage` / `totalTakenDamage` / `totalTakenHit`、`ConfiguredDamageChance` / `ConfiguredNotHitChance` 和 `DamageTakenOnBlock`。总量与比值只是已有命中的展示视图；减免和概率的 `derived` 明细保留实际阶段输入，不重跑求解器；五种 `EnemyDamage` 与五种 `EnergyShieldBypass` 为 `none`，因为当前只保留聚合／覆盖后的值，没有可展示的来源轨迹。持续伤害一侧的字段见下文“防御持续伤害”一段。展示不改变既有计算；Evergrasping 的全部伤害额外混沌、Runic Ward 和恢复规则差异仍按本指南的约定解释。

同一份 `EhpTerms` 还保留敌人组装、减免快照（`MitigationCtx`）与资源池，作为五种类型的 `EnemyDamageMult`、`EnemyPen`（百分点；物理与混沌没有穿透输入，为 0）、`ResistTakenHitMulti`（穿透后的抗性倍率，负抗性不受穿透）、`AttackTakenHitMult` / `SpellTakenHitMult`（攻击与法术命中的承伤层，Average 类别取两者之平均）、`AfterReductionTakenHitMulti`（所选命中类别的承伤倍率 × 偏斜倍率）、`BaseTakenHitMult`（抗性 × 减伤 × 减伤后倍率，不含固定承伤）、`DamageReductionMax`、`ManaEffectiveLife` / `MoMHitPool`（Mind over Matter 分担后的生命池与命中资源池）和 `TotalHitPool`，以及全局 `DamageReductionMax`、`LifeHitPool`（可恢复生命计入生命流失防止）和四种抗性的 `ResistTotal`（截断后、上限与下限之前的总值，与 `resolve_resistance` 同一份结果）。Chaos Inoculation 的混沌承伤层同为 0。`EnemyDamageMult` 与 `EnemyPen` 只保留敌人一侧聚合后的值，明细为 `none`；攻击与法术承伤层为 `sources`（实际查询的承伤名字）；其余为 `derived`，资源池拆成已保留的阶段（生命 + Mind over Matter 分担的魔力、Energy Shield、Runic Ward / Aegis / Guard / 友军层）。未提供 `TakenReflect`（没有反射伤害模型）。

防御持续伤害：四种 `ResistOverTime`（火、冰、电、混沌；带 `Dot` 标志重新解析的同一份 `resolve_resistance`，所以 “+1% Chaos Resistance against Damage Over Time” 这类游戏 stat `additional_*_resistance_against_damage_over_time_%` 只进入这里）、五种 `TakenDotMult` = max((1 − ResistOverTime/100) × (1 − 基础减伤/100) × 持续伤害承伤倍率, 0)（物理没有抗性；护甲不作用于持续伤害，只计不分命中的额外减伤，按各类型的减伤上限；Chaos Inoculation 的 “Immune to Chaos Damage” 使混沌为 0）、五种 `TotalPool`（非命中资源池：Mind over Matter 分担后的生命池 + 按旁路比例计的 Energy Shield + 未被旁路时的 Runic Ward；不含伤害损失防止、护盾技能、Guard 等只对命中的层）、五种 `DotEHP` = TotalPool / TakenDotMult（免疫时为无穷）和 `EHPSurvivalTime`。自身持续伤害的净恢复（`calc::recovery::net_recovery_terms`）与这些视图共用 `recovery::dot_taken`：持续伤害只对持续伤害的抗性、元素基础减伤和 CI 的混沌免疫因此同样计入净恢复，此前净恢复只用命中口径的最终抗性。`EHPSurvivalTime` = TotalNumberOfHits × 敌人攻击时间 ÷ 敌人行动速度（`calc::skill_use_time::action_speed_factor` 读敌人 ModDb，冰缓等减速随之计入）；攻击时间只取显式的配置 `enemySpeed`（毫秒，`EnemyActionTime`），游戏没有定义默认的敌人攻击时间，构筑码里的 700 毫秒 Placeholder 是导出工具的假设，不采用，没有配置时为 0。PoB2 用 Placeholder 并另有敌人行动速度修正，在 benchmark / tuned / demo 上它的 `enemySkillTime` 多为 1.75 或 3.5 秒，与 PoBR 不可比，`output-coverage` 中这一行不吻合是口径不同，不是缺陷。四种抗性有逐步分解，其余字段是重建自实际输入的 `derived` 明细。tuned Smith of Kitava Shield Wall `fb03d7dce08e` 的混沌持续伤害倍率 0.36（PoB2 0.34）沿用上文 Soul Core of Tacati 的原文约定；其余 `ResistOverTime` 差异与 `ResistTotal` 已有的抗性差异相同（例如 Blood Mage Comet `bc750d615159` 混沌 55 对 65），没有游戏内实测。tuned 组 Infernalist Comet `d7afa87a05ea` 的 `LifeHitPool` 与各类型 MoM 资源池随它的生命按上文的符文原文约定低于 PoB2；其余抗性差异与 `TakenHit` 已有的抗性差异相同。

最后一批 Calcs 字段按各自的游戏规则输出，侧栏字段的目录都已覆盖，详见 `display_catalog` 的单位。`GemLevel` 是所选效果实际运行的技能等级：宝石等级加角色额外等级（`GemBaseLevel` + `GemBonusLevels`，沿用上文宝石等级通道的结果）。`PreEffectiveCritMultiplier` 是暴击倍率在 Bifurcate 与敌人暴击减免之前的部分，1 + max(0, 暴击伤害加成)（基础、附加、INC、MORE、覆盖值；不能暴击或暴击伤害被禁用时为 1），输入按 Primary / Secondary 两个计算分支列出（`crit::CritMultiplierTerms`）。`AccuracyHitChance` 是命中判定通过率的百分数，在敌人格挡之前；非攻击与 `CannotBeEvaded`（`global_always_hit`）为 100，派生项为 `{Primary,Secondary}AccuracyChance`。`AverageBurstHits` 是一次使用中的重复命中数（`BurstTerms::hits()`，与 `AverageBurstDamage` 同源）。`CullMultiplier` 与 `ReservationDpsMultiplier` 分别是 100 ÷ (100 − Culling Strike 阈值) 与 100 ÷ (100 − 敌人被保留的生命比例)，与上文 `CullingDPS` / `ReservationDPS` 使用同一倍率，没有对应机制时为 1。`EnemyAilmentThreshold` 是敌人等级在游戏怪物表（`monster_scaling.json`）中的异常阈值，乘敌人侧的 INC / MORE。`EnemyCritChance` 是敌人暴击几率（基础 × (1 + 增加) × (1 − 配置的闪避)，限制在 0–100，“Always / Never”配置直接取 100 / 0），与 `EnemyCritEffect` 分开。`MaxLifeLeechRate`、`MaxManaLeechRate`、`MaxEnergyShieldLeechRate` 与 `MaxLifeLeechRatePercent` 是每秒偷取速率上限：默认生命 20%、魔力 20%、能量护盾 10% 的最大值每秒，再乘 “increased / more maximum Leech rate”。这三个默认值沿用上文的兼容约定，游戏数据里没有对应的 stat；“increased amount of Mana Leeched” 增加的是偷取量、不是最大速率，所以 benchmark 的 Oracle Lightning Bolt 的 `MaxManaLeechRate` 为 212.73 对 PoB2 的 266.0（上文有意差异的同一规则）。单个偷取实例的 10% 资源池上限也已取消，所以不输出 `LifeLeechInstanceRate` 与 `MaxLifeLeechInstance`（它们来自 PoB2 的单实例上限，游戏当前只允许每种资源一个偷取实例）。`DeflectEffect` 是偏转挡下的伤害百分比：现有常数 40 加 BASE `DeflectEffect`，限制在 0–100（游戏 stat `base_damage_%_deflected`，**没有游戏内实测**）。`SecondMinimalMaximumHitTaken` 是五种类型 `<类型>MaxHit` 中第二小的一个。`SelfChillEffect` 是玩家一侧的修正视图 (100 + INC) × MORE（游戏 stat `chill_effectiveness_on_self_+%`），与 `SelfFreezeEffect`、`SelfPoisonEffect`、`SelfBlindDuration` 同属下文的玩家一侧视图。`EffectiveLootRarityMod` 是 (1 + `LootRarity` INC / 100) × MORE，词条 “increased Rarity of Items found” 因此有读取方；`LootQuantity` 仍登记为无输出；护符上的该词条按护符启用的约定计入，PoB2 在护符未使用时不计，所以带这类护符的构筑高于导出值（benchmark 的 Amazon 为 1.15 对 1.0）。这一批没有输出的 PoB2 字段与依据：战吼的 `OffensiveWarcryEffect`、`MaxOffensiveWarcryEffect`、`AilmentWarcryEffect`、`RallyingHitEffect` 来自 PoE1 的 Intimidating Cry / Rallying Cry 按友军数递增的 exert 伤害，PoE2 游戏数据里没有这两个技能，也没有每个友军的 exert 伤害 stat（“Rallying Cry” 只是物品授予的 buff `LargeScarabBuff` 的显示名），旧快照里它们恒为 1，不输出常数；`LifeLeechInstanceRate`、`MaxLifeLeechInstance` 见上；`MissingTotem*Resist` 等图腾防御字段见图腾防御一段（PoB2 写死 40 元素 / 20 混沌，PoBR 按游戏的图腾宝石等级 stat）。

装备的两瓶药剂和三个护符（含未勾选启用的物品）按槽位输出 `Flask<N>*` / `Charm<N>*`：恢复总量、立即恢复量、每秒持续恢复、持续时间、充能上限、每次消耗、满充能使用次数及每秒充能；`*ChargesGained` 是充能获取倍率。基底数字来自官方游戏表 `Flasks`（药剂与护符的 `LifePerUse`、`ManaPerUse`、`RecoveryTime`，后者为十分之一秒）及 `ComponentCharges`（`MaxCharges`、`PerCharge`），适配到 `base/base_items.json` 的 `utility`；护符持续时间也来自 `Flasks.RecoveryTime`，品质提升药剂恢复量或护符持续时间；`local_*` 对应的裸恢复、时间、充能词条只查询该物品，不能进入全域效果池。全域 `FlaskEffect`、`*FlaskChargeGen` 沿用 PoB key 并提供来源查询，逐槽位数字提供保留实际输入的派生公式；CLI 的 `utility_stats` 与 WASM 的展示字段使用同一结果。单次使用与充能不推算触发或覆盖率；启用时的额外效果沿用下文的生效约定。

明细覆盖分组侧栏的全部字段；除上述 EHP 原始项外，目录中的 `NumberOfDamagingHits` 与 `NumberOfMitigatedHits` 为 `none`，两者来自资源耗尽模拟，尚未保留可展示的迭代轨迹。最大承伤明细复用已求解的值、敌人倍率和资源池上下文，不另跑求解器；来源组的 BASE / INC / MORE 只描述实际查询贡献，不承诺等于含角色基础值等其他输入的最终结果。`ChannelTimeToTrigger` 是所选 Cast While Channelling 组从空 Energy 到首次触发的秒数：被支持法术的总容量除以持续 Energy 获取速率，含宝石及全局增益；不改变 DPS、持续触发频率或返还模型（`calc::energy::channel_trigger_terms`）。目录不提供 `CausticGroundDPS`、`MirageCausticGroundDPS`、`SustainableTrauma` 或 `TrapCooldown` 标量；`SkillDPS` 的逐技能信息由技能总览和 Full DPS 贡献行承载。

导入的损耗由 `pobr_build::ImportReport` 承载，与 `Build` 并列返回（`import_build*`）；`Build` 只描述计算输入，不携带诊断。`Build.jewels` 的每颗珠宝带 `JewelSocket`：树上自有插槽、导入时记下的已分配天赋（`JewelSocket::Node`，构筑码 `<Socket nodeId>` 的节点）、被授予的插槽（Sinister / 含插槽的核心；珠宝的 “Allocates <插槽>” 同样打开它，例如 Megalomaniac 的 “Allocates Zarokh's Gift”）或装备插槽，“珠宝插槽天赋效果”类词条只作用于树上的珠宝插槽天赋。节点不是珠宝插槽时，按节点自己的词条：Lich 的 Crystalline Phylactery（“Can Socket a non-Unique Basic Jewel into the Phylactery”、“100% increased Effect of bonuses gained from Socketed Jewel”，[poe2db Lich](https://poe2db.tw/us/Lich)）让其中的珠宝按两倍计，The Adorned（“Jewel Socket Passive Skills containing Corrupted Rare Jewels”）不作用于它（`calc_orchestrator::collect::jewel_socket_increase`）。超越肢体（PoB 的 `Arm 1/2`、`Leg 1/2`，游戏物品类 `IncursionArm` / `IncursionLeg`）是与装备并列的一组栏位 `Build.limbs`（按 `LimbSlot`）：肢体的 stat 都是全局的，词条在装备之后注入并按栏位归因（`item.arm1.implicit`），宝石等级加成和 “Allocates” 等按文本读取的通道也包含它们。`Build.weapon_set` 是 `items` 里那组武器所属的武器组，另一组的武器在 `Build.other_weapons`。导入时取主技能使用的那一组：武器授予的技能要拿着那把武器才能用（构筑码把它再列成普通组时，按“排在组首、等级与授予等级相同、无品质、未腐化”认出），设为只在一组使用的组用那一组，其他主技能取当前物品集的 `useSecondWeaponSet`；树上的武器组专属天赋随之取那一组（`xml_build::calculation_weapon_set`）。主技能用的不是物品集那一组时，两组都能用的组绑定到物品集那一组，即角色平时拿着的武器。WASM 请求带 `weapon_swap` 时取它的 `active`，`alternate_items` 是另一组的武器。绑定到另一组的技能组（`SocketGroup::weapon_set`，另一组武器授予的技能也绑定到那一组，`granted_skills::mark_item_granted_skills`）按另一组的武器计算宝石等级与品质（`skill::resolve::apply_other_weapon_set_gem_bonuses`），其余计算仍按当前组。编排据此设置 `WeaponSet1` / `WeaponSet2` 条件，“使用武器组 1/2 时”的词条（例如 Rigwald's Ferocity 的 `support_rigwald_*_in_weapon_set_*`）只在对应的组生效。

构筑码的 `mainActiveSkill`（从 1 起）是技能组里可选效果的序号：每颗宝石的授予效果按游戏表 `GemEffects.GrantedEffectDisplayOrder`（`GemEffectDef::granted_effect_display_order`）排列，未列出的接在后面，再去掉辅助效果；辅助宝石授予的主动效果仍计入（`skill_env::mods::group_active_effects`）。宝石的 `skillId` 是它的主效果，不一定是序号 1：弩弹宝石的 1 是射击、2 是装填，旗帜宝石的 1 是旗帜、2 是保留效果，幻影宝石先列生成幻影的效果、再列 meta 主效果。导入、计算、宝石目录和导出共用这一顺序，导出保留原序号；没有显示顺序的旧快照按主效果、附加效果的顺序。构筑码里 `<Gem enabled="false">` 的宝石保留在 `Build` 里（`GemSkillRef::enabled`），准备计算视图时才移除（`calc_orchestrator::stage_build_view`）：辅助判定、技能、buff、精魄保留、触发与召唤物都看不到它，序号只数剩下的效果。

精魄保留按当前计算的武器组计：绑定到另一组的技能（包括它的辅助宝石追加的保留量）不保留，游戏 stat `display_skill_reserves_in_all_weapon_sets` 明确要求在所有武器组保留的技能除外，例如 Blink、Convalescence 和 Eternal Rage。Svalinn 授予的 Cast on Block 本身没有基础保留量，Direstrike II 与 Herbalism II 的 40 和 30 精魄只在拿着该盾的武器组计入，再应用保留效率。tuned Titan His Scattering Calamity 的主技能在第一组，其他常驻技能绑定到平时持有的第二组，因此计算第一组时保留为 0；构筑码参考值按另一组保留，与当前钉定 PoB2 不同。这里只改变保留量：绑定到另一组的常驻技能的 buff、光环仍按生效计算，与 PoB2 和构筑码参考值一致；游戏里切换武器组后这类 buff 是否保持，没有找到说明，也没有游戏内实测。

Runic Ward absorbs damage after Life reaches 1. Life-loss prevention (Grasping Wounds, `life_loss_prevented_%`) applies only to the Life share, not damage absorbed by Ward. In `pool_damage::reduce_pools`, spend the Ward share before the Life-only overkill cap: with 1000 Life, 500 Ward, 25% prevention and a 1500-damage hit, 1332 damage brings Life to 1 (333 deferred), and 168 consumes Ward, leaving 332. Previously, PoBR and pinned PoB2 discarded damage as overkill before spending Ward; at 1 Life a large hit could consume only 1 Ward. Frozen Blood Mage ranks 01/02/04/05/07/08 all combine Ward and 25% prevention, so agreement with PoB2 did not establish correctness. Source: Runic Ward game text (damage taken from Ward before Life while at 1 Life), Grasping Wounds node 56162. The interaction is tested arithmetically, **not in game**; deferred-loss timing remains governed by the existing simulator assumptions.

Equipment conditions use the selected weapon set. Dance with Death requires an empty off hand and a one-handed martial weapon in the main hand; caster weapons, unarmed, and originally two-handed weapons held through Giant's Blood do not qualify. Helmet Armour/Evasion comparisons use the item's final local ratings with strict inequality, before character-wide scaling; ties grant neither conditional immunity.

Mark Effect (`mark_effect_+%`, “increased Effect of your Mark Skills”) scales the enemy-side Mark effect and the damage-as-extra buff granted by Freezing/Voltaic Mark. The latter retains the existing assumption that the Mark's buff is active; no activation rate or uptime is inferred. Mark self-buffs now use `CurseBuff` specs, so duplicate copies do not stack and the existing buff truncation applies (30% becomes 33% at 12% increased effect, 36% at 21%). Party exports retain the caster's Mark Effect for shared enemy debuffs. This text rule is `exporter: "absent"`: the pinned exporter parses it as `LocalEffect` without a reader. This is based on the game stat and skill payloads, **not an in-game measurement**.

Doryani's Prototype (`enemies_in_presence_lightning_resist_equal_to_yours`, `UniqueNearbyEnemyLightningResistanceEqual1`) sets nearby enemies' Lightning Resistance from the player's final resistance after buffs, charges and resource-derived modifiers. The shared enemy override replaces its own resistance and curse/exposure reductions, so minions, damage over time and Sacred Flame's lowest-resistance rule consume the same value. An explicit `EnemyInPresence = false` disables it. Existing explicit enemy overrides and the enemy's maximum resistance retain their priority; the positive-resistance interaction (player 80%, enemy capped at 75%) is a conservative assumption without an in-game measurement. Frozen `chronomancer-10` has −184% player Lightning Resistance, where that upper-cap question does not affect the result.

Summon counts include the active skill's native game stat: Raise Zombie's `base_number_of_zombies_allowed` is 10 before temporary-minion bonuses (15 with the frozen Chronomancer's +5). Wolf Pack's `skill_wolf_pack_size` is 6 at the frozen Acolyte's level 19, plus one from quality; its limit stays 7 even though the pinned PoB2 reports 0. Supports compatible with a summoned minion's skills also contribute their cost multiplier once to the summoning skill. Minion Splash requires both a minion source and a melee strike; this combined eligibility does not bypass explicit minion exclusions or give the support's offensive payload to the player.

Presence item auras reach minions through `ExtraAura`; “You and Allies” additionally gives the player one copy. Ally modifiers on a weapon are not its numeric local attack-speed lines. Offering buff-effect queries use the Offering's own skill types, so generic Offering-effect passives apply even when another skill is selected. In frozen `chronomancer-07`, two 15% passives scale Pain Offering's damage/speed bonuses from 83/35 to 107/45 with the existing integer rounding. Danse Macabre's extra-skeleton-consumption condition and the player's `HitRecently` remain inactive without an explicit applicable snapshot; they are not enabled to match PoB2.

The Coming Calamity's enemy resistance overrides retain the normalized `EnemyInPresence` condition, including inside nested modifier payloads; an already prefixed name must not become `EnemyEnemyInPresence`. Legacy of Northpaw's 7% weapon base critical chance replaces the locally modified weapon base before added critical chance and global increases. It applies independently to each actual weapon source; unarmed attacks, shields, spells and attacks with their own skill-defined base critical chance retain their own bases.

Frozen `amazon-02` illustrates the existing Blind-effect difference: 8993 Evasion against level-82 enemy Accuracy 2114, with the strongest Blind II's single 20% increased effect, gives 24% less enemy accuracy and 55% rounded evade chance (16% Deflect). The pinned PoB2 counts three copies from Pounce's three active effects, giving 32% less accuracy and 58% evade (18% Deflect). Only those evade/deflect rows are classified as explained by Blind. The separate Energy Shield/Recovery Cap error was fixed by resolving Discipline's requirements against its own weapon-set tree; explaining Blind alone does not establish TotalEHP correctness.

`SpiritReserved` reports the total reservation requested by the enabled skills,
even when it exceeds the Spirit pool; `SpiritUnreserved = Spirit - SpiritReserved`
can be negative, and the calculator does not disable skills to fit the pool.
This is an output convention for an overcommitted build, not a claim that the game
allows over-reservation. PoB2 caps its displayed `SpiritReserved` at `Spirit` but
keeps the negative `SpiritUnreserved` and the uncapped per-skill breakdown. Thus
tuned Bonestorm `be0d16c86268` reserves 100 + 54 + 60 + 30 = 244 in both skill
breakdowns, yet PoB2 displays 137 reserved; both report −107 unreserved. Blood
Mage Comet `bc750d615159` similarly requests 355 with a pool of 200 and −155
unreserved. `output-coverage.py` retains the direct field comparison and shows
`oracle_before_pool_cap` on capped Spirit reservation pairs; compare that total
and the per-skill rows before investigating efficiency or support costs. Free
amulet grants and deduplicated item skills remain separate differences described
below; undoing the display cap does not undo those differences.

JSON/WASM 握手版本为 15（`pobr_wasm::SCHEMA_VERSION`，与 Web 的 `EXPECTED_SCHEMA_VERSION` 配对）。解码返回 `config_sets` / `skill_sets` 及各自的活动 id；配置和技能集按 XML 稳定 id 选择，不把稀疏 id 当作文档序号。`xml_merge::manage_named_set` 支持新建、复制、重命名、删除，拒绝不存在的 id 和删除最后一组；旧平铺 Config / Skills 管理时包装为 Default，导出平铺 Skills 同样包装。合并只替换活动片段，保留其他组、未知配置标量、原值拼写、扩展属性和子节点；技能／宝石扩展属性只在来源身份可识别时保留，删除重复组造成歧义时不猜归属。`SkillSet.mainSocketGroup` 记忆各集主技能组（从 1 起，0 为未指定），当前值同时写入 `Build.mainSocketGroup`；未记忆的非活动集首次选择第一组，PoB2 再保存可能丢弃扩展记忆。`loadout::derive_loadouts` 按同名或花括号标识同时关联配置与技能集，只有一组的类别豁免匹配。

游戏 Build Planner v1 `.build` 由 `build_api::decode_build_file_json` 识别，旧国服 `items` 与 WeGame 导入仍可用；`build_api::encode_build_file_json({ code, name, all_loadouts, source })` 返回 `files`（文件名与 JSON 内容）和去重的 `warnings`，可按来源 loadout 分别导出，不切换会话。格式使用游戏的天赋／宝石 id 和武器组，装备栏内容只是提示，不生成装备、药剂或珠宝；角色等级默认为 1，宝石等级／品质默认为 1/0，只识别精确的 `Level N, Q% Quality` 等级提示。`planner_source` 保存作者、链接、说明、等级区间和文本，供存档与再导出使用；区间不参与计算。未知 id 会诊断，导出跳过 meta 宝石及无游戏映射的条目，珠宝与肢体无装备栏映射；历史树须先转换。兼容性由格式契约和合成 fixture 验证，尚无游戏内导入验证。

## 计算引擎架构（pobr-core）

目录按 Modifier 的生命周期拆分（见 [pobr-core/src/lib.rs](../crates/pobr-core/src/lib.rs)）：

| 目录 | 职责与主要入口 |
|------|----------------|
| `model/` | `modifier.rs` 定义 Modifier / ModTag，`config.rs` 定义 CalcConfig |
| `parse/` | `mod_parser/`、`mod_cache.rs`、`apply_range.rs` 处理文本、解析规则与缓存 |
| `rules/` | stat map、特殊词条、配置、buff 解释与 handler 契约 |
| `ingest/` | item / passive / skill / character / campaign 接入，保留 SourceId 归因 |
| `aggregate/` | `mod_db.rs` 提供 sum / more / flag / override / list 及带 trace 的查询 |
| `calc/` | CalculationSession / Env 与 offence、defence、ailment、buff、EHP 等计算阶段；`channel.rs` 引导攻击的使用周期，`periodic.rs` 球与风暴的周期命中，`recovery.rs` 恢复、偷取与净恢复 |
| `attribute/` | TraceGraph 与 direct / marginal / interaction 归因 |

数据流：**数据/文本 → 解析与规则映射 → 来源接入 → ModDb → calc → OutputTable / Breakdown / Trace / Attribution**。旧的 `pobr_core::mod_parser`、`mod_db`、`modifier` 等公共路径通过 `lib.rs` 重导出保留；它们不代表源码仍在根目录。

`calc/session.rs::CalculationSession` 是底层会话入口；完整 Build 由 `crates/pobr-build/src/calc_orchestrator/mod.rs` 编排。该目录按 `skill/resolve.rs`、`item/weapon.rs`、`inject.rs`、`skill/buffs.rs`、`skill/triggers.rs` 等拆分技能解析、武器来源、注入和触发逻辑。`calc/perform.rs` 执行计算阶段，`display_catalog.rs` 定义可展示字段。

编排前置解析由 `calc_orchestrator/prepare.rs` 返回完整的技能、配置和武器结果，不使用默认值占位后分阶段修改。主技能的伤害标志里，攻击、法术、近战来自技能类型（含辅助追加的类型）；命中算不算投射物、是不是范围伤害由所选 stat set 的游戏无值 stat 决定（`base_is_projectile` / `is_area_damage`，`skill_env::stat_set_hit_flags`），没有这类 stat 的旧快照退回技能类型。词条的关键字（“Damage with Warcries” 的 Warcry、“with Fire Skills” 的 Fire、“of Movement Skills”、“Curse Skills”、陷阱与地雷、元素与持续伤害类型）限定它作用于哪些技能：技能按自己的类型带关键字（Warcry、Movement、Aura、AppliesCurse、Physical / Fire / Cold / Lightning / Chaos 等，`skill_env::skill_keyword_flags`），主技能、战吼、光环与 buff、召唤物的技能都按各自的类型取。规则、stat 映射和特殊词条里的关键字名统一由 `KeywordFlags::from_name` 解释，不认识的名字必须报错，不能丢掉：丢掉关键字的词条会作用于所有技能（例如 “Damage with Warcries” 曾加到 Detonate Dead 上）。`CalculationContext` 显式携带 stat-map catalog、观察模式和触发子计算状态；计算热路径不使用线程局部上下文。需要映射诊断时使用 `calculate_with_data_report`，记录由返回的 `CalculationReport.stat_map_records` 独占（含触发子计算），原 `take_stat_map_compare_records` 接口已移除。

`DataCalcCache::new(&BuildData, &DataOrchestratorOptions, capacity)` is an opt-in,
caller-owned bounded output/trigger memo. Its immutable borrows bind data and
options; complete structural Build identity includes config placeholders.
Compare mode and report/session entry points remain uncached. Manual key
projections destructure every field without `..`; keep that safeguard when
adding inputs. The legacy text `CalcCache` defaults to 64 entries and verifies
structural equality for computation; `peek(u64)` remains hash-only. Neither
cache provides dependency-driven stage recomputation or automatically changes
application caching. `BuildData` retains its flat public fields and constructors,
with lookup implementations organized under `build_data/{skills,equipment,passives,rules}.rs`.

来源写入阶段只持有 `calc_orchestrator/sources.rs::SourceWriter`，不暴露 `mod_db()`、聚合查询或 `Deref`。`finish_sources` 消费写入对象后才开放读取：精神保留读取完整来源（含额外词条），召唤物数量在属性准备与条件桥接之后读取。保留线性编排，不为每个注入函数增加状态类型。

角色属性派生和初始资源快照由 `CalculationSession::prepare_player_stats` 负责；build 只补装备事实、计数和来源，再调用 `bridge_player_conditions`。初始快照位于 buff 展开之前，`perform` 在防御资源转换后刷新 Life/Mana 的 stats 与 multipliers；两处共用 core 的资源池计算。保留这两个时点的既有求值顺序，不能把初始快照当成转换后的最终值。

召唤物（`calc/minion.rs`、`calc/perform.rs::perform_minions`、`skill_env/minions.rs`）用它怪物的一个技能攻击：宝石的 `skillMinionSkill`（从 1 起，数据里存在的技能中第几个；构筑码导入、WASM 请求的 `minion_skill` 和导出都保留）选择，默认第一个。技能类型决定攻击、法术等标志；该技能自己的 stat，以及同组里按召唤物技能类型判断能辅助它的辅助宝石，作用于召唤物。召唤宝石与其辅助宝石上关于召唤物的 stat（“Minions deal {0}% more damage”、Muster 按不同种类的复生召唤物计等）由 `skill_stats::minion_modifiers` 交给召唤物，技能本身不再报告这些 stat。伤害基础取游戏 `DefaultMonsterStats.MinionDamage`（`pobr_data::monster::monster_ally_damage`），攻击技能还带武器的暴击率。召唤物的攻击按所选技能在召唤物等级下的 `damage_multiplier` 与 `attack_speed_multiplier` 计算，伤害倍率同时作用于武器基础与附加伤害；不是选定怪物、也不是复制武器的同伴，附加攻击伤害还乘怪物的 `MonsterVarieties.DamageMultiplier`（`calc::env`）。召唤技能上按角色属性分组的加成（“Minions from this skill have 1% increased Damage per {0} of your Strength”，`minion_1%_damage_+%_per_X_player_strength`；`active_skill_minion_1%_attack_speed_+%_per_X_player_dexterity`、`minion_1%_area_of_effect_+%_per_X_player_dexterity`，属性 ÷ 数值向下取整）与 “Minions convert {0}% of Physical Damage to Fire Damage”（`minion_skill_physical_damage_%_to_convert_to_fire`）给该技能的召唤物，不给角色。Supporting Fire 每次命令的齐射数（`death_from_above_number_of_volleys`）在 DPS 末端计数，不放大单次命中。Manifest Weapon（“Manifest a copy of your main hand Melee Martial Weapon”）复制主手近战武器的伤害、攻击速度与暴击率，主手不是近战武器时不创建召唤物（`skill_env::minions::spawn_minions`）。幽魂和驯服的野兽保留怪物本身的伤害（`monster_damage`），再按两条更多伤害缩放：两张表之比，和 `spectre_tamed_beast_inherent_damage_+%_final`（快照里有这个 stat 没有数值，25 取自 PoB2 的游戏内测量，`SPECTRE_TAMED_BEAST_INHERENT_DAMAGE`）。角色的光环（技能类型 `Aura`）也作用于召唤物：按游戏的技能描述，光环作用于 “you and Allies in your Presence”（Discipline：“Emit an Aura that grants you and Allies in your Presence additional Total Energy Shield”），召唤物得到的效果按角色的光环与 buff 效果加上召唤物自身受到的 buff / 光环效果，同名光环取较强的一份（`calc::buff_pass::auras_on_minion`）。角色另外给召唤物的东西有三种来源：`MinionModifier` 词条；只作用于友军的光环 “Allies in your Presence …”（`ExtraAura`，按召唤物自身的 buff / 光环效果缩放）；“Companions gain added Attack damage equal to #% of your main hand Weapon's damage”（Catha's Balance 等：角色主手武器本地每种伤害的这个份额，只给带 `GainMainHandDmgFromParent` 的召唤物的攻击；戒指等给角色的全局附加伤害不属于武器，`perform::apply_main_hand_share`）。只作用于召唤物的技能 buff 也走 `ExtraAura`：Pain Offering 的 “Aura grants Minions {0}% increased damage / Attack and Cast Speed” 不给角色本人，按角色的 buff 效果加上该技能与其辅助宝石的 buff 效果（Sacrificial Offering 的 “Supported Offering Skills have {0}% increased Buff effect”，`skill_stats::OFFERING_BUFF_EFFECT`）缩放；按游戏的描述，骨刺的光环作用于其范围内的召唤物，这里假设召唤物都在范围内。“Minions / Companions gain your Strength”（Sturdy Ally）把角色的最终力量给召唤物，每点力量 +2 最大生命（`calc::minion::LIFE_PER_STRENGTH`）。Gigantic 召唤物（“Your Minions are Gigantic”、Tangmazu's Thurible 的无值 stat `companions_are_gigantic`）按游戏的说明 “Gigantic Minions have 20% more Maximum Life, 20% more Damage” 计算（`perform::apply_gigantic`）。召唤物施放法术时（例如 Water Djinn 的 Navira's Fracturing，`ChilledGroundBurstWaterDjinn`），伤害是该法术自己的基础伤害，每个施法时间使用一次，受技能冷却限制（等级行的冷却、可储存次数，法术还有等级行的暴击率）。召唤物技能的 stat set 只有不到 5 个等级行，游戏按使用者的等级取值：适配器写出每个随效能成长的 stat 每点效能的值（`SkillStatSetLevel::per_effectiveness`，游戏表 `FloatStatsValues`）和 stat set 的效能成长（`StatSetDef::user_level_scaling`，`IncrementalEffectiveness` / `DamageIncrementalEffectiveness`），等级 L 的效能 = `SkillDamageBaseEffectiveness` 3.885209 × `BaseEffectiveness` × (1 + 成长 × (L − 1)) × (1 + 伤害成长)^(L − 1)（`calc::minion::stats_at_user_level`；`GameConstants` 表没有导出，常数取自 PoB2 的数据，按它算出的 1 级值与游戏表里已解析的值一致）。游戏表里每行另有使用者等级和每个 stat 的插值方式，导出里没有这两列，“不到 5 行”的界线与 PoB2 相同。带无值 stat `is_commandable_skill` 的是 Command 技能，召唤物使用它时 `CommandableSkill` 条件成立（`skill_stats::COMMAND_SKILL`），“with Command Skills” 类词条和 Bidding 的 “Supported Minions deal {0}% more Damage with Command Skills” / “increased Cooldown Recovery Rate for Command Skills” 才生效。召唤物抗性从游戏的 `MonsterVarieties` → `MonsterTypes` → `MonsterResistances` 普通稀有度列读取，叠加召唤物词条、召唤技能与辅助宝石的抗性 stat、作用于召唤物的光环；复用 `calc::offence::resolve_resistance` 的向零截断、最大值（默认 75%、通常硬上限 90%）、−200% 下限与超上限计算。基础抗性只在召唤物 ModDb 中计一次。WASM 与 CLI 的每个召唤物 `stats` 都输出四种抗性、最大值和超上限值，参考比较支持构筑码携带的 `Minion:<stat>` 抗性行。游戏[抗性说明](https://poe2db.tw/us/Keywords)将剧情惩罚写为玩家的抗性降低，因此不转移给召唤物。[Companion](https://poe2db.tw/us/Companion) 只定义同伴是一种复生召唤物；[Tame Beast](https://poe2db.tw/us/Tame_Beast) 虽要求捕捉 Rare Beast 并保留最多四条怪物词缀，没有明确召唤后的基础抗性按 Rare 列计算，当前同伴也用普通列，不采用 PoB2 的 Rare 覆盖值；保留的怪物词缀尚未建模。

召唤物自己的诅咒由它怪物的 `AppliesCurse` 技能组装（`skill_env::buff_specs::minions::minion_curse_specs`），按召唤物等级取值，效果读施法召唤物自己的诅咒效果与能辅助该技能的辅助宝石，不读角色的；配置 `raiseSpectreEnableCurses` / `summonCompanionEnableCurses`（条件 `SpectreCursesEnabled` / `CompanionCursesEnabled`）决定幽魂 / 同伴是否施加。每个施法者先按自己的诅咒上限取诅咒（怪物的 `EnemyCurseLimit` 是总数，Ribrattle 为 3），敌人身上同名诅咒只保留缩放后更强的一份，诅咒槽数取各施法者上限中的最大值（`calc::buff_pass`）。Temporal Chains 的行动速度减慢没有输出，SerpentClanCurse 的流血几率还没有建模。幽魂与同伴的 buff 开关（`raiseSpectreEnableBuffs`、`summonCompanionEnableBuffs`）没有注册：Vaal Embalmed Bearer 的旗帜回复（`ABTTProcessionBannerRegen` / `ABTTProcessionBannerRegenSpectre`）在游戏表里的 stat set 与等级行都是空的，名字和说明也为空，拿到它的游戏定义之前不建模。

同伴的数法按 PoB2（项目所有者 2026-10-14 的决定）：每个启用组里的召唤技能各自创建召唤物，物品或天赋授予的技能与放辅助宝石的同名技能组各算一个；同一种怪物在第二个组里出现时照常创建、计入同伴生命并保留精魄（同伴和幽魂按组与怪物计保留，其他技能放在几个组里都只保留一次），但不增加角色的召唤物数量（`Multiplier:SummonedMinion`）。精魄保留效率按角色的计数读取，例如 “2% increased Reservation Efficiency of Skills per Idol in your Equipment” 按装备里的偶像数。辅助宝石给出的 buff 是它所辅助的每个技能的 buff，以该技能命名，选了怪物的同伴 / 幽魂再按怪物区分（`buff_specs::supported_skill_buff_name`）；同名 buff 的同类词条只保留较强的一条。stat 映射自带 buff 名（`effectName`，例如 Refraction 的 “Refractive Plating”）的 payload 不论辅助哪个技能都是同一个 buff（`stat_map_engine::player_buff_effect_name`）。所以 Loyalty 辅助 7 只不同的同伴是 7 个 buff，“10% of Damage from Hits is taken from your Damageable Companion's Life before you” 合计 70%；Refraction 辅助 3 面旗帜仍是一个 buff。游戏里一个技能只有一份，授予的技能和放辅助的组是同一个技能，这样数会多算，没有游戏内实测。

“每个召唤物”类词条（Hysseg's Claw 的 `damage_+%_per_active_minion` 等，含 “per Minion, up to …”）在角色与召唤物上读同一份数量快照（`calc::minion::bridge_count_multipliers`，在 buff 与召唤物计算之前桥接，重复桥接结果不变）：默认是计算实际创建的数量（`Multiplier:SummonedMinion`，按上段的数法），配置 `multiplierSummonedMinion` 覆盖它，显式的 0 也覆盖；Presence 内的数量（`MinionPresenceCount`）默认等于它，配置 `multiplierMinionsInPresence` 可单独覆盖。召唤物的计算沿用这份快照，不再用来源数量改写 Presence 数，桥接之后也不把 ModDb 里的来源数量再加一次。“每个已召唤的某类召唤物”（例如 `attack_and_cast_speed_+%_per_summoned_raging_spirit`）读该类自己创建的数量（`ActiveRagingSpiritLimit`、`ActiveSkeletonLimit`、`ActiveGolemLimit`、`ActiveSentinelOfPurityLimit`），没有创建该类时为 0，其他种类的召唤物不计入。图腾的数量另按下文“已知的有意差异”里的图腾口径。

“Gain #% of Physical Damage as Chaos Damage”（`PhysicalDamageAsChaos`，不带 extra）与 “as extra Chaos Damage” 一样是额外获得的伤害，进入同一张 gain 表（`calc/damage.rs::gain_names`），来源伤害不减少。Embitter（`all_damage_gained_as_cold_instead`，“causing all sources of Damage Gained to instead be treated as Damage Gained as extra Cold”）把这张表里每一行获得的额外伤害都记为冰（`skill_stats::GAINS_ONLY_COLD`）。

Player `AilmentThreshold` starts at half maximum Life, adds flat threshold and
percentages of final maximum Energy Shield, then applies threshold increases
and multipliers ([game keyword text](https://poe2db.tw/us/Keywords)).
The display retains fractional threshold: Chaos Inoculation's 1 Life gives
0.5, rather than PoB2's 1 from rounding percentage contributions upward.
The keyword specifies half Life and no integer ceiling; this is an intentional
precision convention, without an in-game measurement of the rounding boundary.
`LifeOnKill`, `ManaOnKill` and `EnergyShieldOnKill` report flat recovery plus
percentages of their final maximum pool per kill, respecting recovery blocks;
the selected skill's hit kills must belong to the player. Totems, traps, mines
and other proxies report zero player recovery, consistent with the keyword's
separate entity using the skill. They do not assume a kill frequency or model
damage-over-time kill attribution. Cannibalism support's
`support_cannibalism_recover_%_maximum_life_on_kill` remains an unmapped buff
stat (Cannibalism II gives 3 in `base/granted_effect_stat_sets.json`); the
Amazon Twister `5355205a1743` missing 81 is a mechanic gap, not a different
unit or default. `DazeChance` reports the selected hit's
chance, capped at 100%. Freeze buildup reads `FreezeBuildup` alongside the
existing enemy/immobilisation modifiers. Fully Broken Armour removes Armour
and makes non-player targets take 20% increased Physical damage from Hits;
`FullyBrokenArmourEffect` displays that percentage after effect modifiers,
and it affects hits only when the enemy is configured as fully broken.
These fields expose source details through the display catalog.

The Molten One's Gift (passive 28542) extends the same Fully Broken Armour
percentage to Fire Hits; Storm's Rebuke (22726) extends it to Cold and Lightning
Hits (`fully_broken_armour_and_sundered_armour_you_inflict_also_applies_to_*`
in the active game's `base/stats.json`, with the wording in `base/passive_tree.json`).
The existing effect modifiers scale each granted type once, including the
15% increased effect on The Molten One's Gift. This is the player's inflicted
debuff on the shared enemy, so minion Hits benefit too; minion-only grants and
effect modifiers do not alter it. The existing Fully Broken Armour state is
required even when the enemy has no Armour. Damage over time is unaffected,
and neither breaking Armour nor its uptime is inferred from possessing a grant.

Conditional offence uses existing state: `conditionEnemyFrozen` and
`conditionEnemyPinned` pass through `config_resolve::bridge_enemy_conditions`;
enabled Herald skills set `AffectedByHeraldofAsh` / `AffectedByHeraldofThunder`
in `calc_orchestrator::inject`, and the selected triggered payload receives
`Triggered` in `prepare::stage_resolve_main_skill`. Possessing a freeze or Pin
source does not establish the target state. Coming Calamity's 40% elemental
bonuses require their matching Herald; added attack damage (including
Asphyxia's Wrath) applies only to attacks. Daze duration and attack Maim chance
feed the debuff uptime views described under “On-hit effect views” below;
primed-Pin thresholds and duration feed `PinBuildupRequired` / `PinDuration`
as described below. Sources: game `base/stats.json` (`damage_+%_vs_frozen_enemies`,
`bleeding_magnitude_+%_against_pinned_enemies`, `triggered_spell_spell_damage_+%`,
`hit_damage_stun_multiplier_+%`, `daze_duration_+%`, `maim_on_hit_%`,
`pin_almost_pinned_enemies`) and `base/passive_tree.json` (Coming
Calamity, Seeing Stars, Maiming Strike, Constricting Command). The Daze-duration
and attack-Maim rules are exporter-absent.
`FireDamage`, `LightningDamage` and `AilmentMagnitude` have readers gated by
the selected skill's source damage; the usage ledger reports that gate when a
non-damaging skill such as Mana Remnants does not reach those readers.

`HeavyStunBuildupAvg` reports the selected successful hit's average enemy Heavy
Stun buildup as a percentage of poise. The game's `KeywordPopups` entries
Stun / HeavyStun (55 / 57) base buildup on damage dealt; Premitigation (422)
distinguishes damage before enemy mitigation. The calculation reuses the
hit-DPS pass's per-type mitigated damage, with armour applied separately to
non-critical and critical hits before crit weighting. Damage types present
only on critical hits remain in the average. It does not multiply
by hit chance; alternating hands are averaged per successful hit. Player
physical damage and melee hits each give 50% more buildup, multiplicatively;
cold contributes zero (`KeywordPopups[3] Cold`). The monster 100% physical /
33% melee bonuses describe damage dealt by monsters. Dazed enemies receive
50% more Stun Buildup (`KeywordPopups[252]`); `conditionEnemyDazed` is
Effective-gated, as are the other enemy configuration debuffs.

The shared poise formula uses `GameConstants.HeavyStunDamageScale = 58 / 100`
(0.58), the level's `DefaultMonsterStats.PoiseThreshold`, player
`EnemyHeavyStunBuildup` / `EnemyImmobilisationBuildup`, and enemy buildup and
threshold modifiers. Boss poise combines the unique-rarity 500% MORE
(`Mods.MonsterUnique2`) with `MonsterVarieties.PoiseThreshold / 100`, both
behind `Condition:Effective`. The 0.5.5.4.2 game rows are manually curated in
`data/overlay-common/boss_poise_thresholds.json`, including row IDs and export
SHA256: 313 for Boss (the mode among MAP rows with BossHealthBar), 1700 for
Pinnacle/Uber (Xesht, row 1946; no distinct Uber variety). These are representative
tier defaults, not a claim that all bosses share a threshold. The adapter should
generate this domain when that column joins the regular export. The vendor
Map Boss/Xesht Poise entries in `base/enemy_presets.json` remain unused.

`CannotStun` / `CannotHeavyStun` suppress buildup. The derived detail reuses the
retained calculation terms. Mauling Stuns uses `enemyDistance <= metres * 10`,
sharing Focused Thrust's handler and ledger gate; unset distance retains the
zero-distance convention.

`PinBuildupAvg` reports the selected successful hit's average enemy Pin buildup
as a percentage of the Pin threshold. The game's `KeywordPopups[60]` says that
"Certain skills and effects allow damage to build up Pinned", so buildup is
opt-in per damage type: a hit builds Pin only from the types that an applying
source enables. The sources are the selected skill's
`base_physical_damage_can_pin` (Physical) and `base_all_damage_can_pin`, a
weapon's `local_all_damage_can_pin` (that hand's hits only),
`all_projectile_damage_can_pin` (projectile hits only),
`physical_spell_damage_can_pin_on_critical_hit` (Bone Chains: the Physical part
of a critical spell hit only; the non-critical leg and other types add nothing)
and the Pin support's `support_pin_physical_damage_can_pin`. `cannot_pin` and
"Cannot Immobilise" force 0. They are the `CanPin` / `<Type>CanPin` flags;
critical-only sources reuse Heavy Stun's separate critical and non-critical
mitigated legs (`calc::ailment::pin_terms`). The scale is
`GameConstants[45].PinDamageScale = 420 / 100` (4.2) over the same poise
threshold as Heavy Stun (`PinThresholdModifier` [48] and
`HeavyStunThresholdModifier` [29] are both 500, so the boss poise rows above
apply unchanged). Player `EnemyPinBuildup` / `EnemyImmobilisationBuildup` (Pin
support, "increased Pin Buildup", `active_skill_pins_as_though_dealt_damage_+%_final`)
and the enemy's `PinBuildup` / `ImmobilisationBuildup` scale it. Heavy Stun's
50% physical and melee multipliers are not applied: the game text gives them
only for Light Stun chance and Heavy Stun buildup (`KeywordPopups[55]` / [57]).
That is a convention, **no in-game test**. A skill with
`support_pin_buildup_pin_instead_of_stun` (the Pin support) builds no Heavy Stun
(`HeavyStunBuildupAvg` is 0) and its damage counts toward Pin; the support's
buildup modifiers are the Pin ones, and PoBR does not copy the skill's Heavy
Stun modifiers to Pin as PoB2 does. `PinBuildupRequired` reports the buildup
threshold for the selected hit: normally 100%, or the target's Primed-Pin
threshold when `pin_almost_pinned_enemies` (Constricting Command) grants
`PinOnPrimed`. Game `KeywordPopups[400].PrimedPin` gives 40% / 50% / 60% / 70%
for Normal / Magic / Rare / Unique enemies. This rule can Pin an already
primed enemy without granting the skill a source of buildup. `PinDuration`
uses `GameConstants[46].PinDuration = 3000 / 1000` (3 seconds), the inflicted
`pin_duration_+%`, the Pin support's duration MORE, target `SelfPinDuration`
and enemy debuff expiry rate. Both views are zero without a Pin source,
when the skill cannot hit, or when Pin is prohibited. These are application
requirements and duration views, with derived inputs that rebuild each value;
no hit-sequence, buildup decay or uptime is inferred, and `conditionEnemyPinned`
stays a configured state. The new rules are exporter-absent. PoB2 pins from unmitigated hit
averages and evaluates `Physical Spell Critical Hits build Pin` once against the
skill, so a physical spell pins with every damage type; that rule declares
`exporter: "differs"`. Worked comparisons against the pinned oracle: Chronomancer
Reap `3875ffe5d15e` 12.976 vs 55.896 (0.232 = 0.552 from the Xesht 17 / 9.38
poise factor, as for Heavy Stun, times 0.42 because PoBR pins only the Physical
critical portion while PoB2 adds Cold, Fire and Lightning); Tactician Oil Barrage
`02b94461578f` 6.615 vs 17.082 (0.387) and Crossbow Shot `406c76f8e89a` 6.747 vs
12.506 (0.539) follow their `HeavyStunBuildupAvg` ratios (0.396 and 0.541), so the
remaining gap there is the shared poise and mitigated-versus-unmitigated source
damage, not Pin; Blood Mage Impending Doom `add19135df93` is 0.364 vs 0, because
the 3260 Physical part of the Dark Consequences frog explosion (a Spell by game
skill type) pins in PoBR while the oracle reports empty `skillFlags` for it.

伤害型异常（流血、点燃、毒）的同时层数 = 命中率 × 施加几率 × 持续时间 × 每秒命中数，持续时间是基础时间乘持续时间加成（含 “increased duration of Ailments you inflict”，`EnemyAilmentDuration`）与敌人身上效果的过期速度，再除以 “Damaging Ailments deal damage #% faster”：伤害总量不变、造成得更快，每份结束得更早（`calc/perform.rs`，分解里写作 “faster /N”）。

Calcs additionally exposes `IgniteChancePerHit`, `BleedChance`, `PoisonChance`, `ShockChance`; each damaging ailment's `Duration`, `EffMult`, `MagnitudeEffect`, `RollAverage`, `StacksMax` and `StackPotentialPercent`; `Ignite{Fire,Cold,Lightning}{Min,Max}`, `BleedPhysical{Min,Max}` and `Poison{Physical,Chaos}{Min,Max}`. These read the retained ailment-stage inputs, without another damage calculation. Chances, roll percentiles and stack potential are percentages; durations are seconds; effect and mitigation values are multipliers. Source ranges are non-critical stored hit damage after the damage type's ailment buildup multiplier, before the high-roll selection. Ignite chance is measured before over-stacking; bleed/poison chance is the final weighted application chance. Global damaging-ailment views use the last applying hand, while bleed/poison chances average all hand passes (a hand that cannot apply contributes zero); DPS keeps its existing stronger/weaker-hand merge. An absent ailment reports zero, including its multipliers. Stack potential is uncapped and uses the existing full-stack fallback when no hit-rate estimate exists; duration includes enemy debuff expiration and division by faster damage. Derived details reuse those terms (and the shared enemy-mitigation terms) and must reconstruct each displayed value. `ShockChance` retains the existing non-critical-component source approximation. `FreezeBuildupPct` remains a percentage per damage point, not an alias for `FreezeBuildupAvg`.

`ShockDuration` and `ChillDuration` (seconds) use the [game keyword](https://poe2db.tw/us/Keywords) base of 8 seconds on non-players (`GameMechanicsConstantsDef::shock_base_duration` / `chill_base_duration`), times `1 + increases / 100` and the more multipliers, then the enemy's debuff expiration (`calc::ailment::ailment_duration_terms`). The increases add the player's `Enemy<X>Duration`, `EnemyAilmentDuration` and `EnemyElementalAilmentDuration` (skill stats `shock_duration_+%`, `chill_duration_+%`, `lightning_ailment_duration_+%`, `cold_ailment_duration_+%`, `support_pure_shock_shock_duration_+%_final` now pass the stat map) to the target's own `Self<X>Duration`, `SelfAilmentDuration` and `SelfElementalAilmentDuration`. They are reported only when the selected hit can shock (non-zero shock chance) or chill (cold hit damage, no `CannotChill`). Shock and chill magnitude now multiply the attacker's `Enemy<X>Magnitude` / `AilmentMagnitude` factor by the target's `Self<X>Magnitude` / `AilmentMagnitude` factor (`calc::ailment::effect_mod_factors`, each floored at zero); `ChillEffectMod` and `ShockEffectMod` display that product (neutral 1). `FreezeBuildupAvg` is the crit-weighted, mitigated hit damage that builds Freeze (Cold, plus any type flagged `<Type>CanFreeze`, e.g. Pinnacle of Power) times `FreezeBuildupPct` times the target's `FreezeBuildup` / `ImmobilisationBuildup` multiplier, as a percentage of the Freeze threshold (`calc::ailment::freeze_buildup_terms`); it reuses Heavy Stun's mitigated per-hit damage and is zero when the skill does not hit or the player `CannotFreeze`. `active_skill_hit_damage_freeze_multiplier_+%_final` (more Freeze buildup) and Freezing Mark's curse payload `freezing_mark_hit_damage_freeze_multiplier_+%_final` (the target's `FreezeBuildup` MORE) now reach these terms. The boss poise threshold difference recorded for `HeavyStunBuildupAvg` (Xesht 17× against PoB2's 9.38×) applies unchanged, so PoB2's value is about 1.81× PoBR's on Pinnacle bosses. Worked comparisons beyond that factor: benchmark Deadeye Ice Shot `b6ce7b3a4329` is 9.035 vs 12.128 (0.745 = 0.552 × 1.35, Minion Pact II's 35% more damage with the Rhoa; the per-damage buildup, 35% increased × 25% more × Freezing Mark's target MORE, agrees). Tuned Acolyte of Chayula Decompose `7505238a9c8d` is 0 vs 1.079: the poison cloud deals no hit, and Chaotic Freeze's text gives Chaos damage Freeze buildup only "for Hits against targets taking damage over time from Supported Skills", while PoB2 builds Freeze from the cloud's Chaos damage. Tuned Tactician Oil Barrage `02b94461578f` is 0.440 vs 1.548 (0.284): 0.552 for poise, 0.671 for the Cold mitigation already registered for its DPS (Soul Core of Topotante's 15% penetration against PoB2's 25%, Pounce's Mark 0% against 32% taken), and a 0.769 residual traced to Tainted Strike's critical-hit ailment-magnitude line (investigation below). The Freeze support that gives 50% more sits in the Spear Stab group, not Oil Barrage's. Derived details give `BaseAilmentDuration`, `AilmentDurationIncreased`, `AilmentDurationMore`, `EnemyDebuffDuration`; `AttackerChillMagnitude`, `TargetChillMagnitude`; and `FreezeSourceHitAverage`, `FreezeBuildupPerDamage`, `EnemyFreezeBuildupMultiplier`, which reconstruct each value. `FreezeDuration` (seconds) uses the same terms with the Freeze keyword's base ("lasts 4 seconds by default", `freeze_base_duration`); `freeze_duration_+%`, `chill_and_freeze_duration_+%` and `cold_ailment_duration_+%` reach it as `EnemyFreezeDuration` INC, `active_skill_freeze_duration_+%_final` as its MORE, with the target's `SelfFreezeDuration`. It is reported only when the selected hit builds Freeze (`FreezeBuildupAvg` > 0); how long the buildup takes and refreeze timing are not modelled, and PoB2 has no corresponding output. The attacker magnitude factor is weighted by the hit's critical share c (its crit chance, 0 under `AilmentsAreNeverFromCrit`): `(1 - c) * factor + c * factor_with_CriticalStrike` (`calc::ailment::effect_mod_factors`), so "increased Magnitude of Shock / Chill you inflict with Critical Hits" (`shock_effect_+%_with_critical_strikes`, `chill_effect_+%_with_critical_strikes`) applies to the critical share of Shock and Chill magnitude and of `ShockEffectMod` / `ChillEffectMod`. Chill and Shock still use the existing non-critical / crit-averaged source damage; weighting the factor rather than computing a separate critical magnitude is an approximation with no in-game test.

Oil Barrage investigation (2026-10-17, tuned `02b94461578f`): the selected
`OilBarragePlayer` group is group 9; its supports are Nova Projectiles I, Salvo,
Ricochet II, Fork and Armour Break III. [Tainted Strike](https://poe2db.tw/us/Tainted_Strike)
(game passive `criticals79`, node `22864`; stat
`non_damaging_ailment_effect_+%_with_critical_strikes`) grants 30% increased
non-damaging ailment magnitude with critical hits. PoB2 tabulates it as
`EnemyFreezeBuildup` INC 30 under `CriticalStrike` and `WeaponSet2`; together
with Suppressing Fire (`44746`)'s 40% more Immobilisation buildup this gives
1.30 × 1.40 = 1.82. PoBR's existing parser maps the line to `FreezeBuildup`,
but its buildup query leaves the critical condition inactive, giving 1.40.
In a temporary copy of this worked build, adding only “30% reduced Magnitude
of Non-Damaging Ailments you inflict with Critical Hits” cancels that modifier:
PoB2's `FreezeBuildupAvg` falls from 1.547927978 to 1.190713830 (exactly 1.30×
within output precision), while its 100% crit chance, Cold hit damage and DPS
stay unchanged. PoBR stays at 0.440092450 in both copies.
**OPEN gameplay question:** does this magnitude stat modify Freeze buildup?
The [game keywords](https://poe2db.tw/us/Keywords) define magnitude as the stats
a buff/debuff applies and Freeze buildup as a contribution from Cold hits;
they do not establish that equivalence. The existing parser mapping is
exporter-derived, not independent game proof. This isolates the residual's
source but does not justify changing the engine or declaring the oracle wrong.
Game-client measurement or an explicit game rule remains required; no new
known-differences exemption is added, and the poise/Cold-mitigation conventions
above remain unchanged.

Player-side views `SelfFreezeEffect`, `SelfPoisonEffect` and `SelfBlindDuration` follow the other `Self*` views: nonnegative `(100 + INC) * MORE` percentages. Freeze effect reads `SelfFreezeEffect` (game `non_damaging_ailment_effect_+%_on_self`, also feeding Chill and Shock); Poison effect reads `SelfPoisonEffect` (`unaffected_by_damaging_ailments` gives 0). Blind has no self-duration game stat (`blind_duration_+%` is outgoing), so `SelfBlindDuration` is the 100% base times `DebuffExpirationModifier / 100`: Blind is a debuff. Incoming freeze, poison and blind remain unmodelled, so these views change no other output.

“你和 Presence 内的友军 …” 类词条（例如 Infernalist 的 Bringer of Flame：“All Damage from you and Allies in your Presence contributes to Flammability and Ignite Magnitudes”）角色自己也有。PoBR 的 `ExtraAura` 只给友军，所以抽取器把 PoB2 规则里不带 `onlyAllies` 的 `ExtraAura` 展开为角色自己的词条（带同样的条件）加给友军的 `ExtraAura`（`sync-pob-catalog` 的 `character_share_of_aura`）；带 `onlyAllies` 的规则抽取器不收。

物品的符文护盾（Runic Ward）是基底的值加上物品自身词条（隐式与显式）里的 “+N to maximum Runic Ward”（游戏 stat `local_ward`，只出现在暗金上，例如 The Brass Dome），再乘物品自己的 “N% increased Runic Ward”（`local_ward_+%`）和品质；镶嵌的合金和武器上的同一文本是 `base_maximum_ward`，加给角色（`item::defence::item_ward_modifiers`）。“Gain X% of maximum Life as Extra maximum Runic Ward”（`base_maximum_life_%_to_gain_as_maximum_ward`）按加成之前的生命乘比例，再按符文护盾的加成缩放，与其他 “Gain … as Extra …” 的资源转换相同（`calc::defence`）；这一口径与 PoB2 相同，**没有游戏内实测**。从生命、魔力转换或获得的量取最近的整数；各件装备的防御按栏位名的顺序累加，同一构筑的总数不随物品的读入顺序在 .5 处变化。

The tuned Smith of Kitava Shield Wall (`fb03d7dce08e`) boots retain the imported
Soul Core of Tacati's explicit +11% Chaos Resistance, rather than substituting
the current socketable table's +13%. With the item's +53%, Chaos resistance is
64% and a 386 Chaos hit deals 138.96 (PoB2 substitutes 66% and takes 131.24).
With 2162 Runic Ward, including The Brass Dome's local +991 Ward, PoBR's
ChaosMaxHit is 8001 and TotalEHP is 35908.717951478; the older code references
are 2468 and 11096.445514375. This follows the imported-text and local-Ward
rules above, rather than the exporter version's result.

Warbringer 的 Renly's Training “Gain 35% Base Chance to Block from Equipped Shield instead of the Shield's value”（`ReplaceShieldBlock`）在装备盾牌时替换盾牌自身的格挡值，再乘格挡的加成（`calc::defence_panels::calc_block`）。Gemling 的 Virtuous Barrier 的属性 Mote 上限按角色从宝石获得的技能计：每个启用组里的技能宝石按属性需求计（单属性 +2，多属性各 +1），授予技能的辅助宝石（标签 `grants_active_skill`，例如 Eonyr's Thunder）也计；物品或天赋授予的组不计，构筑里同一技能已另列一个组（`inject::virtuous_mote_counts`）。 [Virtuous Barrier 的游戏文本](https://poe2db.tw/us/Virtuous_Barrier) 按需要该属性的技能计，物品授予的技能也有自己的属性需求。tuned Gemling Twister（682f26ed1957）的权杖 Purity of Fire 因此计一次：Intelligence Mote 为 3 + Purity of Fire + Herald of Ice + Ghost Dance + Trinity = 7；每点再生加成为 5% + floor(19 品质 × 0.125) = 7%，合计 49%。PoB2 跳过物品授予的技能，只有 6 点 / 42%；PoBR 的 LifeRegenRecovery 为 110.3391，构筑码参考值为 103.34，钉定 PoB2 为 101.76。

偶像数量按 socketable 类别计，表里没有条目时才回退到基底标签。[Carved Majesty](https://poe2db.tw/us/Carved_Majesty) 虽在 `base_items.json` 标为 `rune`，仍是 Idol，不承受 Idolatry 的 −4% 全元素抗性惩罚。因此 tuned 组 Spirit Walker `e071351f645d` 有五个非 Idol 镶嵌物，合计 −20%。它的 `ColdResistOverCap` 为 14，与钉定 PoB2 一致；构筑码参考值 2 漏掉了 Fox Idol 的 Bonded +12% Cold Resistance，胸甲的 “Idols socketed in this item gain the benefits of their Bonded modifiers” 使这条词条生效。

Smith of Kitava 的 “Modifiers to Fire Resistance also grant Cold and Lightning Resistance at 50% of their value” 与 “Modifiers to Maximum Fire Resistance also grant Maximum Cold and Lightning Resistance”（`modifiers_to_fire_resistance_also_apply_to_cold_lightning_resistance_at_%_value` / `modifiers_to_maximum_fire_resistance_apply_to_maximum_cold_and_lightning_resistance`）把火抗的每条词条按比例、最大火抗的每条词条按全值加给冰抗和电抗；元素抗性惩罚不是火抗的词条，不计入；“all Elemental Resistances” 已经给冰电抗，不再转换其展开后的火抗分量。先合计可转换的词条并乘比例，保留半点等小数，到最终总抗性与最大抗性时才向零截断，再计算超上限抗性（`calc::offence::resolve_resistance`）。Smith of Kitava 的 “Body Armour grants” 词条（Kitavan Engraving 的 `body_armour_grants_x_maximum_life_+%`、Leather Bindings 的 `body_armour_grants_x_life_regeneration_rate_per_minute_%`）只在装备普通稀有度胸甲时生效。同一类天赋（Tribute to Utula、Support Straps、Tantalum Alloy、Flowing Metal、Molten Symbol）给出的精魄、力量、晕眩门槛、抗性、最大火抗、护甲作用于元素伤害、物理命中改为火焰承受也受这一条件控制。咒符效果（`charm_effect_+%`）只放大启用的咒符。`temporary_minion_limit_+`（“Temporary Minion Skills have +# to Limit”）只提高带 `Duration` 类型的召唤技能的上限（`skill_env::minions`，`TemporaryMinionLimit`），不影响常驻召唤物。Sacrosanctum 的 `life_recoup_also_applies_to_energy_shield` 把生命吸纳的比例也加给能量护盾吸纳，两者各按自己的回复速度与时长；Igniferis 的 `life_recovery_from_regeneration_is_not_applied` 让生命再生不再回复生命，原始再生值仍供转移类机制读取（`calc::survivability`）。

Three equipment-adjacent readings share one convention: the value is the item's own displayed number, and suppression is tracked per modifier name. Strong Chin's `stun_threshold_+_from_lowest_of_base_helmet_evasion_rating_and_armour` grants Stun Threshold equal to the lower of the helmet's displayed Armour and Evasion (pre-global scaling; the stat name's "base" is the pre-global item base). The Invoker's `cannot_gain_spirit_from_equipment` drops only BASE Spirit with an equipment origin (per-item base Spirit, explicit +N lines, socketed runes); passive and quest Spirit still count, and the suppressed equipment lines still show as applied in the usage ledger because modifier reads are tracked per name. Elemental recoup (`recoup_%_elemental_damage_as_energy_shield`, parsed as `Fire/Cold/LightningEnergyShieldRecoup`) adds its percentage to its own damage type's recoupable only, sharing the Energy Shield recoup duration and recovery rate (`calc::survivability::es_recoup_terms`, which also feeds the EsRecoupRate derived detail).

“Physical Damage prevented Recouped as Life” (`physical_damage_prevented_recouped_as_life_%`, [Made to Last](https://poe2db.tw/us/Made_to_Last) 5%, Wandering Reliquary 50%) uses an explicit convention: prevented damage is the Physical damage entering the physical reduction stage minus that leaving it, counting Armour and additional physical damage reduction after the cap and overwhelm. This follows [Iron Ward's game text](https://poe2db.tw/us/Iron_Ward), which describes damage prevented by Armour or other sources of damage reduction; it is an assumption, **没有游戏内实测 / no in-game test**. Damage shifted to another type before reduction and later damage-taken multipliers do not count; blocked and deflected hits contribute nothing, including partially damaging blocks. Evaded hits never enter the landed-hit loop. The [December 2024 bug report](https://www.pathofexile.com/forum/view-thread/3661840) and [November 2025 discussion](https://www.pathofexile.com/forum/view-thread/3882155) are verification leads, not a developer ruling on this convention. The mitigated EHP loop accumulates prevented damage alongside ordinary damage-taken recoup; `calc::survivability::life_recoup_terms` combines both using Life's 8-second duration (4 seconds with either Life/global flag) and LifeRecoveryRate, and also supplies the `LifeRecoupRate` detail. The parser rule is `exporter: "absent"`, so reference-basis calculations omit it. The Chaos counterpart (Forsaken Bangle, below) reuses this definition with Chaos resistance in place of Armour, and the Mana side uses `mana_recoup_terms`.

几条暗金词条接到已有的计算入口（`overlay-common/special_mods.json`）：Blackheart 的 `base_armour_%_applies_to_chaos_damage` 让这一比例的护甲也用于混沌命中的减伤；Vigilant View 的 `maximum_chance_to_evade_is_50%` 与 Goregirdle 的 `maximum_physical_damage_reduction_is_50%` 限制闪避率与物理命中减伤；Matsya 的 “Skills reserve 50% less Spirit”（`unique_spirit_reservations_are_halved`）乘在保留量上，不是保留效率；Plaguefinger 的 `all_damage_can_poison` 让元素命中也计入中毒的伤害，不提供中毒几率；`ignore_hexproof`（“Curses you inflict can affect Hexproof Enemies”）让诅咒越过敌人的 Hexproof；Venopuncture / Icefang Orbit（`all_damage_to_bleeding_enemies_contributes_to_chill` / `all_damage_to_poisoned_enemies_contributes_to_chill`）只在敌人流血 / 中毒时让全部命中伤害计入冰缓强度。Parry 的 “increased Parry Damage”（`parry_damage_+%`，Silverthorne 与天赋）和 Kaltenhalt 的 Parry 物理转冰（`parry_physical_damage_%_to_convert_to_cold`）只作用于 Parry。Chober Chaber、Trenchtimbre、Crown of Eyes 的 “increases and reductions … also …”（`minion_damage_increases_and_reductions_also_affects_you`、`additive_modifiers_to_minion_attack_speed_also_affect_you`、`additive_spell_damage_modifiers_apply_to_attack_damage`）只复制相应的 INC 词条：召唤物伤害、召唤物攻击速度、法术伤害分别给角色伤害、角色攻击速度、攻击伤害，保留来源和条件，不复制 MORE / LESS、点伤和只限定某类召唤物的词条（`calc::shared_modifiers::apply_shared_increases`）。

Berek's Grip/Pass/Respite and The Taming carry the game stats `wind_skills_count_as_empowered_by_{ignited,shocked,chilled}_ground`: their joined item lines set `Condition:TwisterFire/Lightning/Cold` exactly like the Twister config checkboxes, bridged into the conditions table by `CalculationSession::bridge_player_conditions`, so Twister's `SkillDamageGainAs` tags apply without any config (`special_mods.json`: `wind_ground_ignited/shocked/chilled/all_elements`). The game text wraps across two item lines, so equipment reuses the tree wrap-merge (`filter_item_parseable` via `collect::combine_wrapped`) and the pair is ledgered once as its joined line. `wind_skills_can_be_empowered_by_multiple_elements` is S0 (`wind_ground_multiple_surfaces`): PoBR applies each Twister element condition independently and never limits a Twister to one element, so two Berek's rings without The Taming both apply — a known simplification with no in-game test. All five rules are `exporter: absent`, so the reference basis is unchanged and the game-mode DPS rise shows only in the beyond-PoB2 report.

元素曝露在角色的防御算出之后归并（`calc::setup_env::reduce_enemy_exposure`，由 `perform` 调用）：Refraction III 的 “Banner applies {0}% Elemental Exposure per 1000 Armour”（`support_tempered_valour_banner_applies_%_elemental_exposure_per_1000_armour`，旗帜插下时）按角色护甲的每满 1000 点计，与其他曝露一起取最强的一份，再按敌人身上的曝露效果缩放。配置的 “Is the enemy Exposed to Fire?” 是角色自己施加的减益，只在角色能施加该元素的曝露时成立（条件 `CanApply<元素>Exposure`，`setup_env::with_exposure_sources`）：某个技能或辅助它的宝石施加曝露（`skill_env::groups_inflict_exposure`），角色有 “Inflict Fire Exposure on Hit” 这类词条，旗帜按护甲施加曝露，或敌人身上已有其他来源的曝露。没有来源时配置不生效，与 PoB2 相同（它在没有来源时隐藏这个选项）。技能自己施加的减益由 `skill_stats::debuff_modifiers` 按游戏 stat 定义，先于抽取的 stat 映射，品质加在同一个数值上（两条同名曝露只取最强的一条，分开会丢掉品质）：Oil Barrage 的油地面施加曝露（`skill_base_oil_exposure_-_to_total_elemental_resistance`，20，品质每 1% 加 0.25），假设敌人站在油里。Crushed 使目标的物理伤害减免降低 15，可以降到负数，目标随之承受更多物理伤害（减益的 stat `base_additional_physical_damage_reduction_%`，[poe2db Crushed](https://poe2db.tw/us/Crushed)；`setup_env` 的敌人基础词条，条件 `EnemyCrushed`，由敌人的 `Condition:Crushed` 或配置设置）。Styrn's Ferocity 的 “Supported Skills Crush targets they Hit for 8 seconds”（`support_knights_ferocity_crush_on_hit_ms`）不论辅助哪个会命中的技能，都让敌人处于 Crushed（`skill_env::skill_crushing_on_hit`）：Raise Shield 打出的命中让 Shield Wall 打的敌人被 Crush，假设 8 秒内用过它。

Culling Strike（Reap 的 `kill_enemy_on_hit_if_under_10%_life`，游戏文本即 “Culling Strike”）在敌人生命降到按稀有度取的阈值时击杀（游戏常量 `culling_strike_<稀有度>_threshold`：普通 35%、魔法 20%、稀有 10%、独特 5%），“increased Culling Strike Threshold” 提高阈值。需要打掉的只是阈值以上的生命，所以 CombinedDPS 乘 100 ÷ (100 − 阈值)（`calc::skill_dot::cull_multiplier`），`TotalDPS` 不变，侧栏的 `CullingDPS` 是这一倍率额外贡献的 DPS。Detonate Living 的 `spells_have_culling_strike`（“Your Spells have Culling Strike”）只给法术。

内置 buff（Arcane Surge、Onslaught 等，`overlay-common/buff_definitions.json`）都是战斗中的 buff：配置项 “Do you have Arcane Surge?” 写入的条件带 “Combat” 限定，计算在战斗口径下时成立（`rules::buff_expander::expand_misc_buffs`）。词条写明拥有该 buff 时同样触发：“You have Unholy Might” 写入同名条件 `Condition:UnholyMight`；只对某类伤害成立的条件不算，“30% chance to gain Onslaught for 4 seconds on Hit” 带命中标志，是否拥有由配置决定。buff 自己写入的 Multiplier（Unholy Might 的量级 100，“Gain 30% of Damage as extra Chaos Damage” 按它计）在展开后并入 multiplier 表。武器上的 “+N to Accuracy Rating” 是游戏的本地 stat `local_accuracy_rating`，只计入用这把武器的攻击（`ingest::item::apply_weapon_hand_conditions`）：Shield Wall 用盾攻击，主手武器的命中值不计入。写明 “this Weapon” 的词条（“Attacks with this Weapon Penetrate 15% Elemental Resistances”）解析时带待定的手别条件 `{Hand}Attack`，由武器所在的栏位定为主手或副手（同一函数）。“Damage with Maces” 这类武器类型伤害和主手武器的类型标志只作用于用武器攻击的技能：技能类型有 `Attack` 而没有 `NonWeaponAttack`（`conditions::attacks_with_weapon`）。Shield Wall 与 Raise Shield 用盾自己的伤害攻击，法术不用武器，都不计入；“while wielding a Mace” 这类持有条件不受影响。Fire Spell on Hit 这类 meta 宝石触发的技能带 `no_cost`，技能本身没有消耗，meta 宝石仍保留精魄。

技能的无值 stat `global_always_hit`（“Your Hits can't be Evaded”，掌套的隐式 stat 也是它）让命中不检查闪避（`CannotBeEvaded`），暴击也不再按命中率折算。Mace Strike 的 `skill_double_hits_when_dual_wielding` 在双持时两把武器各命中一次：DPS 按两次命中计，每次命中的伤害与暴击率不变。所选 stat set 带 `base_deal_no_damage` 时该效果不造成伤害（`DealNoDamage`），武器与附加伤害也不能把 buff、放置动作或技能容器变成命中：Conductive Runes 的放置、Rolling Slam、Spearfield、Tempest Bell 的主 stat set 与 Hollow Focus 的 Bells 为 0，伤害分别在同一技能另选的 Explosion、Slam、Hazard、Shockwave、Nova stat set。Explosive Demise 引爆生命低于阈值的友方召唤物（`command_minion_target_ally_below_x_life`），没有召唤物时不造成伤害（以上见 `rules::skill_stats::skill_modifiers`）。Raise Shield 与 Parry 用盾牌的伤害攻击（`replace_off_hand_unarmed_attack_stats_with_shield_type`，盾牌每满 N 点护甲或闪避为一组，两者的 N 为 5），主手武器不提供伤害或攻击时间（`skill_env::weapon::weapon_contribution`）。Raise Shield 的说明 “Release immediately after Blocking when an enemy is close to perform a Shield Bash” 要求先格挡，它的 250 毫秒最短引导与 35 毫秒释放是举盾的过程、不是盾击频率，所以不走引导周期，按自己 550 毫秒的攻击时间计，假设每次能盾击时都有一次可格挡的命中到达；Parry 没有引导阶段的 stat，按等级行的 750 毫秒（`base_parry_duration_ms` 是敌人身上减益的持续时间）。这两条**没有游戏内实测**，不模拟敌人的攻击或格挡频率。

弩的射击效果读取同组弹药的 `base_number_of_crossbow_bolts` 和 `active_skill_reload_speed_+%_final`，装填速度的品质与等级值先相加（`skill_env::mods::crossbow_reload_modifiers`）。Requiem 的 `crossbow_skill_has_no_ammo` 使激活期间不消耗弹药，也不计装填时间。Magma Spray 的 `off_hand_weapon_*_fire_damage` 与 `off_hand_*_added_fire_damage_per_15_shield_armour/evasion` 使用盾牌自身防御的每满 15 点，技能伤害倍率只乘一次（`skill_env::weapon::shield_elemental_damage_modifiers`）。Chayula's Will 的 `archon_of_chayula_tornado_attack_interval_ms = 200` 表示单个旋风每秒攻击 5 次；The Stars Answer / Walking Calamity 的陨石读取主效果 `walking_calamity_spawn_meteor_every_ms = 300 / 150`，不受攻击／施法速度影响（`rules::skill_stats::skill_modifiers`、`calc_orchestrator::skill::mods::skill_timing_modifiers`）。假设全部落点命中，不混合每第 15 发强化形态，不计多目标频率加成或旋风叠加数量；**没有游戏内实测**。

Snap 的 `snap_damage_+%_final_if_created_from_unique`（“{0}% more damage with Hits for blasts originating from a Unique enemy”）只加命中伤害，把所计算的独特敌人当作爆炸源头。Flameblast（`pobr-core/src/calc/flameblast.rs`）每层按 `charged_blast_spell_damage_+%_final_per_stack` = 75 加更多伤害，层数取 multiplier `FlameblastStage`（默认 1，最多 `flameblast_maximum_stages` = 10）；一次使用是每层施法时间 × 层数与 `base_minimum_channel_time_ms`（490 毫秒）中较大的一个，再加 `channel_skill_end_animation_duration_override_ms`（600 毫秒），仍受冷却限制。层数由配置输入 `skillFlameblastStages` 设置（Web 技能页的层数输入，`config_resolve::resolve_config` 写入 `FlameblastStage`，留空沿用默认）；PoB2 构筑码里自己的层数选择还没有接入。

Crystallised Immunities uses the game stats `immune_to_chill_if_majority_blue_supports_socketed`, `immune_to_ignite_if_majority_red_supports_socketed` and `immune_to_shock_if_majority_green_supports_socketed` (Crystallised Immunities, node 5332). Count socketed support gems across enabled groups using the official `SkillGems.GemColour` values: strength/red, dexterity/green and intelligence/blue. A majority means **strictly more than half of all counted supports**; empty sets, ties and a mere plurality grant no immunity. The existing `ChillImmune` / `IgniteImmune` / `ShockImmune` path gives 100% avoidance. PoB2 accepts a strict plurality instead; these rules carry `exporter: "differs"`. Sources: `base/stats.json`, `base/passive_tree.json` and `base/skill_gems.json`.

On-hit effect views (`calc::effect_views`, all reported only: configured enemy states such as Fully Broken Armour, Dazed, Maimed or Blinded still decide every damage and defence effect; **没有游戏内实测 / no in-game test**). Shared terms feed both each value and its derived detail:

- `ArmourBreakPerHit` = (flat `ArmourBreakPerHit` BASE + Σ dealt damage × `<Type>DamageAsArmourBreak` %) × (1 + INC / 100) × MORE × rarity. Dealt damage is the selected hit's average after enemy mitigation (crit-weighted, before hit chance); “Break Armour on Critical Hit with Spells” uses only the critical share c·m / (1 − c + c·m). KeywordPopups ArmourBreak: “Players Break 3 times Armour against Normal Monsters and 2 times Armour against Magic Monsters”; there is no Magic enemy configuration, so non-Rare/Unique enemies use 3×. `HitsToFullyBreakArmour` = ⌈enemy Armour before break ÷ per hit⌉, `TimeToFullyBreakArmour` = hits ÷ landed hits per second (action rate × repeats × hit chance). Sources: `armour_break_amount_+%` / `_vs_enemies_affected_by_ailments` (Exposed Wounds), `ArmourBreakerPlayer`'s `apply_X_armour_break_on_hit`, `armour_break_{physical,fire}_damage_%_dealt_as_armour_break`, `support_increased_armour_break_armour_break_amount_+%_final`.
- `DazeUptime` / `MaimUptime` / `BlindUptime` use KeywordPopups durations (Daze 8 s, Maim 4 s, Blind 4 s) times duration modifiers and enemy debuff expiration. With evenly spaced landed hits, n = rate × duration, k = ⌊n⌋, f = n − k and per-hit chance p, uptime = f·(1 − (1 − p)^(k+1)) + (1 − f)·(1 − (1 − p)^k); at 100% chance this is min(1, rate × duration). Attack-only chances (`maim_on_hit_%`, `attacks_chance_to_blind_on_hit_%`) apply to attacks; `Cannot*` and enemy immunity give zero. Turning a ≥99.9% uptime at 100% chance into the enemy state automatically is a possible follow-up, not implemented.
- `Maximum{Fire,Cold,Lightning}Infusions` = 3 (GameConstants `BaseMaximumInfusionCount`, KeywordPopups “up to 3 of each Infusion”) + per-type and all-type additions. `SurroundedRadiusMetres` is the 3 m Surrounded radius (KeywordPopups; `BaseRequiredEnemiesToBeConsideredSurrounded` = 5) scaled by `SurroundedArea` like other areas.
- `CurseActivationDelay` is the selected curse's `curse_delay_duration_ms` × (1 + delay increase − activation-speed increase) × MORE; Whispers of Doom (`keystone_hex_master`, KeywordPopups HexMaster “Double Activation Delay of Curses”) is MORE 100. The view and its readers need a curse as the selected skill (a registered read gate).
- `RemnantsPerUse` = 1 + `chance_%_to_create_additional_remnant` / 100 (Remnant Attraction 10%; Lingering Illusion's 156%) for skills with `GeneratesRemnants`. Into the Breach's configured collected Purple Flame stacks are not a creation rate, so extra creation chance does not multiply them; remnant collection, Arcane / Empowering Remnants, Elemental Equilibrium and Storm's Recollection still need a collection model.

The new rules are `exporter: "absent"`. Standalone “Immune to Chaos Damage” (game stat `chaos_immunity`) sets `ChaosImmune`, which `defence::chaos_immune` shares with Chaos Inoculation for chaos hits and chaos damage over time (`exporter: "differs"`); the CI pool rerouting stays tied to the keystone.

`mise run config:coverage [--json <文件>]` 按配置分区报告 `read` / `handler` / `implied` / `unread` / `no_effect`：复用 benchmark、tuned 的 `.cache/build-usage/` 读取并集，条件与倍率另用台账的 `condition_reads`、`multiplier_reads`、`referenced_variables` 和计算阶段条件清单；列出未覆盖技能、引擎及活动数据中没有精确名字引用的项、未注册 handler、stub / context gate，以及空 effects 中已有 scalar 读取方与丢失的机制。`--binary <pobr>` 复用已构建 CLI，`--refresh` 重算台账；不读 holdout。这是读取覆盖，静态名字引用与 handler 注册都不证明选项改变了数值；近似名字不是别名，已穿透／连锁次数不能映射为穿透／连锁上限。

Config coverage also reports `not_in_game` and excludes it from the relevant
`(read + handler) / options` denominator. `devs/scripts/config_relevance.py`
checks required skills against game effect IDs/displayed names and skill-gem
item names; other controls are excluded only when their mechanic words lack
base-game evidence or inherited overlay item/stat-description text. The overlay
text is exporter-derived, not an independent game-data source. Ambiguous matches
and legacy stats stay in the denominator; this is not proof of playability.
`python3 devs/scripts/config_relevance.py` regenerates the versioned Web filter,
and workflow tests check it against the snapshot. Static computed-name leads
(such as `ShockVal`) are not observed reads. `detonateDeadCorpseLife` supplies
an explicit nonnegative finite corpse Life, including zero, to existing corpse
damage shares (`corpse_explosion_monster_life_permillage_physical` / `_chaos`);
absent, negative or nonnumeric values retain the enemy-level life table. It
does not change enemy Life or add corpse damage to unrelated skills.

配置项进入计算的方式各不相同，有 handler id 不等于选项已生效。破甲（`conditionEnemyArmourBroken` 与破甲量 `multiplierArmourBreak`）由 `calc::armour_break::apply_armour_break` 在有效 DPS 口径下从敌人护甲中扣掉所填的量，没填时全部扣掉，“You can Break enemy Armour to below 0” 时下限为原护甲的相反数；没勾选时数值框里的残留不生效。致盲与 Elusive 的效果覆盖（`overrideBuffBlinded` / `overrideBuffElusive`）按百分数读取，不超过角色能施加的致盲、能得到的 Elusive 效果（`calc::blind`、`rules::buff_expander`）。Inexorable Critical 按配置的距上次暴击秒数（`SecondsSinceInevitableCrit`，不到 4 秒时另置 `CritRecently`）乘每秒的加成，受 `support_inevitable_criticals_*_cap` 限制（`calc::crit::apply_inexorable_critical`）。Dark Pact 的目标骷髅生命（`darkPactSkeletonLife`）乘 `skeletal_chains_aoe_%_health_dealt_as_chaos_damage` 为基础混沌伤害，Herald of Ash 的过量伤害（`hoaOverkill`）乘每分钟的燃烧比例 `herald_of_ash_burning_%_overkill_damage_per_minute` 为火焰持续伤害（`calc::configured_damage`）。

敌人受击配置（`calc_orchestrator::enemy_config::modifiers`）以 OVERRIDE 覆盖等级与敌人类别的预设：五种伤害（`enemyPhysicalDamage` 等）、三种元素穿透、暴击率、暴击伤害和物理压制（`enemyPhysicalOverwhelm`）；显式的 0 同样覆盖，负数按 0，非有限值忽略；来源标为 `EnemyConfig`、id `config.<输入名>`，被覆盖的预设行在 TotalEHP 与最大承伤的敌人输入（`CalculationSession::enemy_hit_mods`）里保留但不计入。`enemyDamageType` 选择命中类别（平均、无类型、近战、投射物、法术、法术投射物；持续伤害不是命中），决定闪避、格挡与承伤的上下文（`calc::taken::HitCategory`）；`EHPUnluckyWorstOf` 按 1 / 2 / 4 次中最差的结果估计防御概率。`enemySpeed` 只在 Energy 格挡事件缺少来袭频率时提供回退，不是 EHP 的通用伤害倍率；`enemyDamageRollRange` 和持续伤害的防御视图还没有读取方。敌人等级只解析一次（`calc_orchestrator::skill::mods::resolved_enemy_level`），尸体生命与敌人的减伤共用：调用方显式给的等级优先，其次构筑码 `enemyLevel` 的 Input、Placeholder；都没有时普通怪物和普通首领跟随角色等级，Pinnacle / Uber 取该档的默认等级（82），最后限制在该档的最低等级与敌人最高等级（85）之间。尸体爆炸按这一等级取怪物生命表（82 级 32956，85 级 36012），例如 Detonate Dead 的 `corpse_explosion_monster_life_permillage_physical` = 107 是 10.7%。

侧栏的伤害视图以稳定 ID 区分：`TotalHitAvg` 是角色一侧的平均命中；`AverageDamage` 计入敌人减伤与命中概率，`AverageBurstDamage` 再计一次使用中的重复攻击；`CombinedAvg` 与 `WithIgniteDPS` / `WithBleedDPS` / `WithPoisonDPS` 按技能的平均伤害或 DPS 口径组合；`IgniteDamage` 等异常总伤害是该异常的 DPS 乘有效持续时间，不是叠层 DPS 再乘时间；`MainHandAccuracy` / `OffHandAccuracy` 各读自己那只手的作用域。技能数据的四个持续时间通道进入 `Duration`、`DurationSecondary`、`AuraDuration`、`ReserveDuration`，通用的技能持续时间加成只应用一次。范围半径按内部距离单位计算，`AreaOfEffectRadiusMetres` 除以 10 显示米；`PierceCount`、`ForkCount`、`ChainMax`、`SplitCount` 与投射物行为解析共用禁止与转换规则，几率产生的次数可以是小数，无限穿透沿用 100。弩的 `FiringRate` 是装填前的射击速率，`ReloadTime` 是立即装填几率折算前的装填时间。可展示字段的 id 保留 JSON 兼容性，与 PoB 对照用 `display_catalog.pob_key`；既有别名包括 `TotalHitAvg` → `AverageHit`、`ShockEffect` → `ShockEffectMod`、`AoeAreaMod` → `AreaOfEffectMod` 和 `TotalTotemPlacementTime` → `TotemPlacementTime`。除明确记录的有意差异外，单位与默认值须与对应 key 一致。

`TotalHitAvg` deliberately retains the game's [Pre-mitigation Damage](https://poe2db.tw/us/Keywords)
view, including critical-hit averaging. PoB2's `AverageHit` includes enemy
mitigation but not hit chance; `AverageDamage` additionally includes hit chance.
These are different views, so `TotalHitAvg` remains excluded from reference
agreement. Comet `dd7f512456b6` and Living Bomb `9a0fb35e890a` face a 0.5
elemental damage multiplier, explaining their approximately twofold difference;
Seismic Cry `3e588bde3126` has physical and cold multipliers of 0.25 and 0.5.
Do not change the underlying hit or DPS to reconcile these display definitions.

`BlockEffect` is the percentage of hit damage prevented by a block: 100 by
default, as the [Blocking keyword](https://poe2db.tw/us/Keywords) prevents the
whole hit. The internal `block_effect` remains the damage-taken share used by
EHP; only its catalog view is complemented. `ShockEffect` (`ShockEffectMod`)
is the magnitude modifier, `(1 + increased / 100) * more` on the player times
the corresponding modifier on the enemy, with neutral value 1 (zero if the
selected skill cannot inflict shock); it is not shock's base 20% increased
damage taken or the hit/threshold-derived magnitude.
The existing `shock_effect` calculation is retained separately.
`AoeAreaMod` (`AreaOfEffectMod`) is likewise a multiplier (neutral 1), even
without a known base radius; unknown radius remains zero. Both multiplier
fields use numeric catalog units, not percentage points.

侧栏其余的机制视图由 `calc/sidebar.rs` 在已算出的结果上取值：

- `EffectiveMovementSpeedMod` 含装备基底的移动惩罚（游戏表 `ArmourTypes.IncreasedMovementSpeed`，`ArmourMovementPenalty`）与行动速度。`MovementSpeedWhileUsingSkill` 只对能在移动中使用的技能（技能类型 `UsableWhileMoving`、stat `can_perform_skill_while_moving`）计算：移动速度乘该技能使用时的速度（`movement_speed_+%_final_while_performing_action`），`movement_speed_penalty_+%_while_performing_action` 与 Mobility / Deliberation 的 `support_mobility_movement_speed_penalty_+%_final_while_performing_action` / `support_deliberation_movement_speed_penalty_+%_final_while_performing_action` 改变的是这一惩罚，不是角色的移动速度。
- `PresenceRadiusMetres` 的基础半径是 4 米（0.2.0 起，[官方公告](https://www.pathofexile.com/forum/view-thread/3740562)），只读 Presence 自己的范围与半径（例如 “Presence Radius is doubled”，`unique_double_presence_radius`），不读技能范围。
- `AuraEffectMod` / `CurseEffectMod` 在主技能是光环 / 诅咒时给出它的效果：全局加成加上该技能与其辅助宝石自带的 `aura_effect_+%` / `curse_effect_+%`。后者记为 `Selected*` 名字（`SelectedMagnitude`、`SelectedCurseEffect` 等）：buff 阶段已把宝石自身的效果算进光环和诅咒，不能再放大一次。
- `ActiveMinionLimit` 是主技能实际创建的召唤物上限（`SelectedMinionLimit`，按召唤技能名限定，其他技能的召唤物不进入），没有上限 stat 的召唤物（例如同伴）为 1；Wolf Pack 读 `skill_wolf_pack_size`（宝石 1 / 18 / 20 级为 3 / 5 / 6 只狼）。
- Blood Price 的 “Enemies in your Presence have at least #% of Life Reserved”（`unique_blood_price_enemies_in_presence_have_at_least_%_life_reserved`，敌人的 `LifeReservationPercent`）在有效口径下取最强的一份，几份不相加。只需打掉未被保留的生命，所以 CombinedDPS 乘 100 ÷ (100 − 保留比例)，单次命中与异常的伤害不变；`ReservationDPS` 是这一倍率额外贡献的 DPS（`perform::fill_skill_dot_stage`，与 `CullingDPS` 同理）。
- `BurningGroundDPS`（Ball Lightning、Incinerate、Oil Grenade、Spear of Solaris 的点燃地面 stat set 取已算的点燃，直接造成燃烧地面的 stat set 取技能自己的持续伤害）、`DecayDPS`（Profane Decay，`TriggeredDecayingHexPlayer` 自己的持续伤害）、`MirageDPS` 与 `MirageBurningGroundDPS`（所选 stat set 带 `skill_disabled_unless_cloned`，即技能只由复制体使用，例如 Mirage Archer 的幻影）都是已算伤害的分类视图，不能再加进 CombinedDPS。
- `ExplodeChance` 是 “Enemies you kill … have a #% chance to explode” 类词条的几率（`EnemyExplodeChance`，上限 100%），爆炸伤害还没有计算；`CombinedAvgToMonsterLife` 是 `CombinedAvg` 占敌人等级在怪物生命表里的生命的百分比。
- `CritBifurcates` 与构筑码里同名参考值同单位，是倍率 1 + c / (2 − c)：c 是一次掷骰的暴击几率（乘命中几率，Lucky 先计），c / (2 − c) 是暴击的命中里两次暴击掷骰都成功的比例（`attacks_roll_crits_twice`），两把武器各算一次后取平均；不在有效口径或没有这一机制时为 0。比较直接用倍率，Web 显示为 (值 − 1) × 100%，例如 1.33333 显示 33.3%。

Limit, speed, recovery and requirement multiplier views (`calc/limit_display.rs`, filled at the end of `calc/sidebar.rs`) read values the calculation already resolves; none re-evaluates a mechanic. `CharmLimit` is the usable Charm slots (`env_finalize::charm_slots`). `EnemyCurseLimit` and `EnemyMarkLimit` are the player's own limits (`buff_pass::player_curse_limit` / `player_mark_limit`, shared with the curse pass, so `CurseLimitIsMaximumPowerCharges` is honoured); the internal `OutputTable::enemy_curse_limit` that party code reads is the largest limit among all casters and can be higher. `ActionSpeedMod` is `skill_use_time::action_speed_factor` (negative `ActionSpeed` slows scaled by `SlowEffectOnSelf`, dropped under `UnaffectedBySlows`, plus positive bonuses); `MovementSpeedMod` is the movement speed before action speed, so `EffectiveMovementSpeedMod` = `MovementSpeedMod` × `ActionSpeedMod` (rounded to 0.001). `LightRadiusMod`, `LifeRecoveryRateMod`, `ManaRecoveryRateMod` and `EnergyShieldRecoveryRateMod` are `(1 + INC / 100) * MORE` of `LightRadius` and of the recovery-rate query that `survivability::recoup_recovery_rate` feeds to Recoup. Because `LightRadiusMod` reads every `LightRadius` modifier, a Light Radius line counts as read in the usage ledger even without a transfer. `GlobalGemAttributeRequirements` / `GlobalItemAttributeRequirements` / `GlobalWeaponAttributeRequirements` are the per-class multipliers (neutral 1, including `GlobalAttributeRequirements`) that `skill_mechanics::global_requirement_modifiers` supplies to the requirement calculation; The Vertex's “Equipment has no Attribute Requirements” is a 100% less `GlobalItemAttributeRequirements`, so the item and weapon multipliers read 0 where PoB2 (which does not apply that line) shows 1 and 3; this is the listed Vertex difference (Spirit Walker Zekoa e071, Blood Mage Impending Doom add1), the only disagreement for these fields among the worked builds. `PresenceRadius` is `PresenceRadiusMetres` × 10 (tenths of a metre, the unit PoB2 uses); the metre field keeps its own floor. `LowLifePercentage` is the game's `LowPoolThreshold` × 100 (35), the same constant that decides the automatic Low Life condition; `LowestAttribute` is the minimum of the three final attributes that `prepare_player_stats` stores. `MaximumChill` / `MaximumShock` and `CurrentChill` / `CurrentShock` are the values of the non-damaging ailment stage (`ailment_apply::maximum_terms` / `AppliedAilment`, retained in `Env::applied_ailments`): the game maximum (Chill 50, Shock 100; the Shock bound comes from the PoB2-derived `SHOCK_MAX_EFFECT` and has not been checked against the game) plus `ChillMax` / `ShockMax`, or the override when one applies, and `floor(min(max(source magnitude, magnitude already on the enemy), maximum))` when a source applies the ailment, otherwise 0. PoB2 adds an unconditional 30 Chill base for the enemy, so its `CurrentChill` is never 0; PoBR keeps its convention that no chill source means no applied chill, so this field disagrees with the oracle in every worked build while the maximums agree. `CurrentShock` is 0 in both. The derived details are `LowLifePercentage`, `LowestAttribute`, the four ailment fields and `LifeCancellableReservation` / `ManaCancellableReservation` / `FullLifePercentage` / `DurationMod` / `StrikeTargets` (below); the rest are modifier queries.

`LifeCancellableReservation` / `ManaCancellableReservation` are the percentage of the pool a skill reservation can still release: 100 minus the permanent share, `max(100 - Σ BASE Extra<Pool>Reserved, 0)` (“Reserves 25% of Life”, the Beidat’s Hand / Gaze / Will and Midnight Bargain lines, which parse to `ExtraLifeReserved` / `ExtraManaReserved`). The permanent share is part of `LifeReserved` / `ManaReserved` and of the low-pool percentage (`perform::pool_reservations`, `survivability::permanent_reserved_percent`), but Reservation Efficiency and `ReservationMultiplier` scale only the skill share, never the permanent one; neutral is 100 and it never goes below 0. There is no game source for a permanent Mana or Spirit reservation line beyond these, so `ExtraSpiritReserved` is not read. `DurationMod` is `max((1 + INC / 100) * MORE, 0)` of the `Duration` and `PrimaryDuration` terms (neutral 1), computed by `skill_mechanics::duration_modifier_terms`, the function `duration_terms` uses for the skill duration, so the view and the duration apply one modifier; PoB2 also folds in damaging-ailment duration (read by `calc/ailment.rs` here) and mine duration (PoE2 has no mines), so it can differ. `FullLifePercentage` is the `FullLifePercentage` OVERRIDE (game stat `full_life_threshold_%_override`, “You count as on Full Life while at N% of maximum Life or above”, exporter-absent) or 100; the game exports no constant for it, no current item or node carries the stat, and there is no in-game test. It does not set the `FullLife` condition, which stays explicit configuration (or Chaos Inoculation); `FullManaPercentage` is the same view for Full Mana: the `FullManaPercentage` OVERRIDE from `full_mana_threshold_%_override` (Idle Hands, `Mods.UniqueFullManaThreshold1` = 90; exporter-absent) or 100, and it does not set the `FullMana` condition either. `StrikeTargets` is 0 unless the selected skill is a Strike (skill type `MeleeSingleTarget`; Slam attacks are not Strikes per the [game Keywords](https://poe2db.tw/us/Keywords)), otherwise 1 + Σ BASE `AdditionalStrikeTarget` (“Non-Vaal Strike Skills target N additional nearby Enemies”, `melee_attack_number_of_spirit_strikes`); it multiplies no damage, so DPS is unchanged by extra targets. Ancestral Boost uses the explicit attack snapshot described below; extra targets still multiply no damage.

Ancestral Boost snapshots (`calc/session/preparation::bridge_ancestral_boost`) resolve one shared state after enabled buffs are assembled. The [game keyword](https://poe2db.tw/us/Keywords) gives a boosted Strike two additional targets and a boosted Slam 30% more damage and 25% increased area. The active native `GrantedEffectStatSets` supply Ancestral Call I/II/III's `ancestral_call_spirit_strike_interval_ms` (5000/3000/1000 ms) and Fist of War I/II/III's `ancestral_slam_interval_duration` (6000/4000/14000 ms, also milliseconds); no rank interval comes from vendor constants. `double_ancestral_boost_effect` doubles the bonus once (60% more Slam damage, 50% increased area), not the whole attack or another copy of its support modifiers. [Call III](https://poe2db.tw/us/Ancestral_Call_III)'s `support_ancestral_call_damage_+%_final` is an unconditional 30% less damage on the supported skill, including unboosted uses.

The default game calculation is an unboosted attack. `conditionAncestrallyBoosted=true` (or the existing `warcryMode=MAX`) selects an available boosted use **only with a source**. Alternatively, `multiplierAncestralBoostElapsedSeconds` supplies elapsed seconds since that support last boosted this skill: before its native interval the attack is unboosted, at/after it the next use is boosted. A supplied timer takes precedence over the requested maximum; negative/non-finite values are not ready. Explicit `conditionAncestrallyBoosted=false` selects an unboosted attack even with an active Cry. These inputs are intentionally raw API controls: XML `<Input name="conditionAncestrallyBoosted" boolean="true"/>`, or JSON `config_inputs: {"conditionAncestrallyBoosted": true, "multiplierAncestralBoostElapsedSeconds": 5}` (and `"conditionAncestralCryActive": true` for an active Cry). `config_resolve` makes raw JSON edits authoritative over imported XML state. Native callers may instead set `Build.config.conditions["AncestrallyBoosted"]` / `["AncestralCryActive"]` and `Build.config.multipliers["AncestralBoostElapsedSeconds"]` when no raw override is present. Reads are tracked for config diagnostics, but these controls have no dedicated Web/catalog entries or new display labels yet; the existing Max Hit control remains available. They describe one attack, never a repeated-use frequency or an average DPS rotation. No timer, first-use readiness, automatic uptime, cooldown-recovery scaling, or rate of boost consumption is inferred. The existing `AVERAGE` selection therefore leaves this snapshot unboosted; it is not a claim that an averaged rotation has been solved.

[Ancestral Cry](https://poe2db.tw/us/Ancestral_Cry)'s enabled buff additionally needs `conditionAncestralCryActive=true`, meaning the player declares that the buff is currently active. Its native `ancestral_cry_empowered_attacks_strike_X_additional_enemies` gives the same two-target boost; the skill text applies the boost to both melee Strikes and Slams. Call/Fist and Cry share one boost, not independent damage/target multipliers. `ancestral_cry_non_skill_physical_to_fire_convert_%` converts 100% of physical damage on these boosted attacks through the non-skill conversion channel. Cry's additional quality effect (enabled by Advanced Thaumaturgy) grants its global fire-damage bonus with the same explicit buff state; ordinary quality only extends duration per consumed charge. Triggered attacks (including Volcanic Steps/Eruption), totem/proxy attacks and unrelated skills never acquire this player-use boost. Cry's native eight-second duration, 50% more duration per removable Endurance Charge, Glory generation, charge consumption and volcanic trigger frequency do not imply uptime; expiry and rotations are not simulated. There is no in-game timing test. `DataOrchestratorOptions::ancestral_boost_snapshots=false`, selected by `on_reference_basis`, preserves the previous supported-Strike snapshot; new Cry and Call III rules are exporter-absent, and the new Slam bonus only applies to game snapshots. Reference compatibility is tested separately from the game behavior.

Not exposed, because the current game data has no corresponding mechanic and a number would be invented: `ActiveTrapLimit` / `ActiveMineLimit` (no `Trap` or `Mine` skill type or supporting gem exists in the 0.5 data; only the legacy `Trappable` / `Mineable` types and the Hazard type remain, and the stats `base_number_of_traps_allowed`, `base_number_of_remote_mines_allowed` and their `number_of_additional_*` siblings are referenced by no stat set, mod or passive node; PoB2’s base of 15 is absent from `character_constants.json` and `game_constants.json`; the Hazard rune limit is reported separately as `ActiveHazardLimit`, Σ BASE `HazardRuneLimit` from the selected skill's `rune_hazard_base_number_of_runes_allowed`: Conductive Runes' constant 20, 0 for any other skill; no mod or node modifies it in the 0.5 data), `BloodCharges` / `BloodChargesMax` and `InspirationCharges` / `InspirationChargesMax` (only legacy stats such as `show_blood_charge_info`, `vaal_reap_additional_maximum_blood_charges` and `inspiration_charge_duration_+%` remain, referenced by no current mod, stat set, passive node or unique; the game’s Charges keyword lists only Endurance, Frenzy and Power; PoB2’s 5 is its setup default) and `TTakenReflect` (PoB2’s `<Type>TakenReflectMult` / `AnyTakenReflect` keys are hard-coded false there and never output; the game data has only `ReflectDamageTakenEssence1` with spawn weight 0, a unique ring mod no current unique carries, and Atlas / map / Heist mods, and Bramblejack’s reflect is reflect to the attacker, so there is no player-side reflect-taken reduction to show).


Widow's Reign (`reserve_life_instead_of_loss_from_damage_for_x_ms`, in the active
and retained `base/stats.json` / `overlay/stat_descriptions.json`) replaces
Life lost to damage with reservation. The current item has a 3000 ms release
window; `overlay/uniques.json` retains the old 5000 ms variant (the
[game item text](https://poe2db.tw/us/Widows_Reign) confirms the current value).
`pool_setup` reads `DamageInsteadReservesLife`; `PoolState.life_reserved_by_damage`
retains the amount reserved across hits, after ES, MoM and other absorbing
layers. It is separate from maximum Life, ordinary missing Life (for example
Life spent on costs), and configured skill/permanent reservation. Damage consumes
reservation capacity even with ordinary Life missing; it clamps current Life to
the remaining capacity instead of subtracting from it again. Fully consuming
that capacity is lethal, including the last point under CI; chaos immunity and
ES bypass retain their normal gates. `life_recovery_cap` / `recover_life` allow
ordinary missing Life to recover but never remove reservation or revive a dead
pool. The EHP loop applies that ceiling to recovery on block, including its
accelerated batches; damage remains eligible for recoup, whose recovery cannot
release reserved Life. Reservation efficiency still affects only skill
reservation, not reservation caused by damage.

`LifeRecoverable` is the fresh snapshot's configured unreserved Life; each
subsequent pool state supplies its reduced recovery ceiling. Skill reservations
remain enabled, and permanent reservation (for example Beidat's 25%) is never
available. PoB2 instead uses its entire cancellable share for Widow's initial
pool and permits ordinary recovery in its hit loop. We do not infer that the
player switches off skills: change the configured reservations explicitly to
model that choice. Neither the static snapshot nor the consecutive-hit loop
infers a damage-free 3/5-second window, cancels reservation automatically, or
simulates a release/recovery event. Callers can provide an explicit current
`PoolState.life_reserved_by_damage` and current `life`; these are engine state,
not new UI controls. The DoT pool remains Life/MoM/ES/Ward without hit-only
Guard or loss prevention; there is no elapsed-time DoT/recharge simulation.
Recovery panels remain potential recovery rates for ordinary missing resources,
not a claim that regeneration/leech offsets damage reservation. No game-client
expiry, rounding or deferred-Life-loss interaction has been measured; the
reservation state uses the existing continuous pool precision and loss-prevention
ordering. No new comparison key or display label is introduced.

`ReqStr` / `ReqDex` / `ReqInt` 取全构筑装备与宝石需求的最大值，再与各颜色辅助宝石累计的需求比较（`calc_orchestrator::requirements::requirement_snapshots`）：装备先转换基底需求，再加本地固定需求，乘本地百分比修正并向下取整，最后应用全局修正（Runeforged Leaden Greathammer 的 60 力量、50 智力，转换 20% 到敏捷后再加 81 智力，得到 48 / 22 / 121；三倍武器需求后为 144 / 66 / 363）；宝石按原生等级计，腐化与装备给的技能等级不抬高需求；物品授予的技能和被认作它的普通组不新增一颗宝石的需求（见“已知的有意差异”）。消耗按资源分固定、百分比与每秒三种通道；每秒消耗按使用、触发、装填或放置的频率计算，多发投射物不另付消耗（`perform::fill_resource_cost_rates`）。技能自带每秒消耗时，该资源的持续消耗直接取这一通道，不再把启动消耗按攻击频率叠加：Oil Barrage 是 Wyvern 引导技能，不是弩弹药；27 级的 69 魔力启动消耗与每分钟 4049 魔力在 110% 辅助倍率下分别为 75 与每秒 74。图腾放置用召唤技能自己的游戏 `cast_time`（`calc_orchestrator::skill::mods::skill_timing_modifiers`，陷阱、地雷与战吼同理），施法速度与图腾放置速度分开。不是攻击、也没有专属速度的技能（例如 Wildwood's Gifts，`AncientGiftsPlayer`，只有 1000 毫秒的施法时间、没有 `Spell` 类型）按施法速度加技能速度计算使用时间（`skill_use_time::speed_names_for`）；战吼、图腾、陷阱与地雷仍用各自的速度。游戏把这类技能的基础时间标为 “Cast Time”，PoB2 与构筑码参考值也这样计，**没有找到游戏对施法速度作用范围的正式说明，也没有游戏内实测**。`ChannelTime` 只含引导阶段，不含释放与冷却。

Spell Echo's `echoed_spell_area_of_effect_+%` increases the area of echoes only.
The `repeatMode` Average view weights that increase by echoes / total casts;
Final and Final DPS use the full bonus, and None uses the original cast's area.
For level-20 Frost Bomb (24 + 7 radius units), Spell Echo (one echo, 40% increased
area) and Spell Cascade (20% less area) give Average area ×0.96 and radius 3.0 m;
the original cast is 2.7 m and the echo is 3.2 m. This display convention does not
apply Light Radius modifiers to skill area by itself (`calc::skill_mechanics::calc_aoe`).
Light Radius (`light_radius_+%`) transfers only its increases/reductions when an
explicit transfer is equipped: `light_radius_additive_modifiers_apply_to_area_%_value`
uses the rolled percentage (game Mods: 25–50); the accuracy and damage transfers
use `light_radius_increases_apply_to_accuracy` and
`light_radius_additive_modifiers_apply_to_damage`. Copies retain source and
conditions. `LightRadiusMod` displays the modifier itself, so without a transfer the ledger still counts the line as read there but the shared copies stay absent.

`buff_time_passed_+%_only_buff_category` changes elapsed buff time: “Buffs on you
expire 10% slower” divides the selected Buff skill's duration and the nominal
`ArchonDuration` by 0.9, after duration modifiers. Non-buff skill durations,
enemy debuffs, and the Archon recovery period do not use it. The existing effect
expiry slow ceiling is retained; event-driven uptime is not modelled. The rule
is exporter `differs` because the exporter treats this as increased duration.

`life_recovery_from_flasks_also_recovers_energy_shield` (Shavronne's Satchel)
adds `Flask{1,2}EnergyShieldRecovery`, `EnergyShieldInstantRecovery`, and
`EnergyShieldRecoveryPerSecond`. Life flask amounts, quality, flask recovery,
and the instant fraction are reused; gradual recovery uses the destination's
Energy Shield recovery rate, not Life recovery rate. Mana flasks do not copy,
and `CannotGainEnergyShield` blocks the copied recovery.

PassiveSkills nodes 11335 / 47190 / 12183 / 36676 grant ordinary passive points
through their “Grants 1 Passive Skill Point” text. `AllocationTree::edit` includes
shared allocated grants in the candidate budget and rejects refunds that remove
spent grants; `ExtraPoints` is consumed outside ModDb. Heroic Tragedy's
`local_unique_jewel_alternate_tree_seed` / `_version` Remembrancing line is
`JewelData`, read by `jewel_tree`; missing seeded node data remains unsupported.
`spell_damage_+%_while_wielding_melee_weapon` uses `UsingMeleeWeapon` from the
currently equipped weapon classes. The existing
`critical_hit_damage_bonus_+%_if_consumed_power_charge_recently` convention uses
at least one removable Power Charge, controlled by `usePowerCharges` and
`overridePowerCharges`; minimum charges cannot activate it. This is a charge
snapshot convention, not a simulated recent-consumption event.

Additional skill slots (`local_item_additional_skill_slots` /
`num_additional_skill_slots`) and knockback distance (`knockback_distance_+%`)
change no reported output. Whispers of Doom's curse delay (`keystone_hex_master`)
feeds `CurseActivationDelay`.

When the selected skill places or is used by a totem (types `SummonsTotem` /
`UsedByTotem`), `calc::totem` reports the totem's defences: `TotemLife`,
`TotemDurationMod`, `Totem<T>Resist`, `Totem<T>ResistTotal` and
`MissingTotem<T>Resist` for Fire, Cold, Lightning and Chaos, each with a derived
detail. The totem monster comes from game `ActiveSkills.SkillTotemId` →
`SkillTotemVariations` → `MonsterVarieties` (`overlay-common/totem_varieties.json`,
with its `LifeMultiplier`: 120 for Ancestral Warrior's SlamTotem, 100 for the
others). Its level is game `MinionGemLevelScaling` (2 × the totem gem's effective
level, 40 for a level-20 gem); `TotemLifeBase` is
`floor(DefaultMonsterStats.AllyLife[level] × LifeMultiplier)`, scaled by
`totem_life_+%` (INC) and `totem_life_+%_final` / `support_totem_life_+%_final`
(MORE) and rounded (`calc_orchestrator::skill::mods::totem_base_modifiers`).
Resistances start from the totem monsters' 0% (`base/monster_resistances.json`)
and add the totem gem's own level stats `totem_elemental_resistance_%` /
`totem_chaos_resistance_%` (0 below gem level 13, 75 / 35 from level 20) and
supports such as Reinforced Totems; the maximum is the monster base 75% plus
`totem_maximum_all_elemental_resistances_%`, truncated like the player's.
`TotemDurationMod` is `(1 + totem_duration_+% / 100) × more`. PoB2 instead gives
every totem 40% elemental and 20% chaos resistance, absent from the game tables,
and ignores the gem's level stats: tuned Shockwave Totem `8468096e8b99` reports
65 / 30 in PoBR against PoB2's 40 / 41, and Grim Pillars `19a4efbb971d` 75 / 35
against 40 (Fire 75) / 20. `TotemLife` agrees with PoB2 on the three tuned totem
builds checked (2285, 2490, 5019). A totem is an Ally in the character's Presence, so the character's auras
(`calc::buff_pass::auras_on_minion`, as on a minion with no effect-on-self
modifiers of its own) and ally-only `ExtraAura` lists add their resistances
(level-20 Purity of Fire: +41% Fire) to the totem's; this assumes the totem
stands in the Presence, with no in-game test. `TotemArmour` / `TotemEvasion` are
`floor(DefaultMonsterStats.Armour/Evasion[level] × MonsterTypes.Armour/Evasion / 100)`
of the totem's monster type (game `MonsterTypes`: Armour 100 for the Spell,
Ballista, Ancestral Warrior and Mortar Cannon totem types, 0 for Shockwave and
Dark Effigy; Evasion and `EnergyShieldFromLife` 0 for all), so a level-20 Spell
Totem has 418 Armour and no Evasion; `TotemEnergyShield` is
`EnergyShieldFromLife`% of TotemLife plus `totem_maximum_energy_shield`, scaled by
`totem_energy_shield_+%` (no current source carries either stat). PoB2 reports
0 Armour. No game table or stat defines a totem Block chance, so
`TotemBlockChance` is not output. `ActiveTotemLimit` is the totem gem's
`base_number_of_totems_allowed` (Spell Totem: 2 at level 20) plus BASE
`ActiveTotemLimit` (“+1 to maximum number of Summoned Totems”, the attack /
spell / melee / ballista variants through their skill tags), times its MORE
(Ancestral Bond), or its OVERRIDE (`non_modifiable_totem_limit`); 0 without a
totem. It is only reported: damage still counts one totem. The level mapping has
no in-game measurement. These
fields change no DPS or totem count.

Charge durations (`PowerChargesDuration`, `FrenzyChargesDuration`,
`EnduranceChargesDuration`) display seconds: the [game's Charges keyword
text](https://poe2db.tw/us/Charges) gives a 15-second base, multiplied by
`(1 + sum of generic and kind-specific increased duration / 100)` and the
product of generic and kind-specific more/less duration modifiers, clamped at
zero. Fractional seconds are retained because the game text specifies no
whole-second rounding; unlike the oracle's whole-second floor, 3% reduced
duration displays 14.55 seconds rather than 14. The in-game tick rounding
boundary has not been measured. `charge_duration_+%` uses the existing three-kind
parser rule; `base_charge_duration_+%` applies generically, and
`frenzy_charge_duration_+%_per_frenzy_charge` uses the resolved Frenzy Charge
count. Details share the calculation's terms. No charge uptime or expiry is
simulated, and duration changes no other output.

恢复（`pobr-core/src/calc/recovery.rs`）分三层：原始再生；经过再生限制与直接恢复后的恢复（`LifeRegenRecovery` 等，含 Runic Ward 与怒气，来自 `base_ward_regeneration_per_minute` “Regenerate 1 Runic Ward per second”、`base_rage_regeneration_per_minute` 等游戏 stat）；扣掉构筑自身损耗后的净恢复（`NetLifeRegen`、`NetManaRegen`、`NetEnergyShieldRegen`、`NetWardRegen`、`TotalNetRegen`）。角色固有的魔力再生是 `character_inherent_mana_regeneration_rate_per_minute_%` = 240（每秒 4% 最大魔力），“No inherent Mana Regeneration” 只去掉这一份（`calc_orchestrator::inject::inject_character_base`）。自身损耗是资源流失（“Lose … per second”，不经减免）、自己受到的类型化持续伤害（经抗性与承伤修正）和主技能的每秒消耗（百分比消耗按资源池换算）：生命、魔力、能量护盾各扣自己的消耗，`NetWardRegen` 是 Runic Ward 的恢复减去它的每秒消耗，`TotalNetRegen` 是这四种净恢复之和，怒气消耗只进入 `TotalBuildDegen`；敌人的持续伤害不计入。Berserk 的 `skill_base_life_loss_%_per_minute_per_rage_while_not_losing_rage_to_apply` = 6 是每点怒气每秒流失 0.1% 最大生命。偷取与命中获得（`LifeLeechGainRate`、`LifeLeechGainPerHit` 等）按[游戏关键词](https://poe2db.tw/us/Keywords)使用一秒基础持续时间，不再把每次偷取量限制为资源池的 10%。[0.5 补丁说明](https://www.pathofexile.com/forum/view-thread/3932540)中的单次命中 40,000 伤害上限已实现：命中总伤害超过上限时，各伤害类型按相同比例缩小，再按各自的偷取比例计算，不能用额外元素伤害补足物理偷取。`base_life_leech_amount_+%` / `base_mana_leech_amount_+%` 的“increased amount of Life Leeched” / “increased amount of Mana Leeched”增加偷取量；`base_life_leech_rate_+%` 等“Leech Life 1% faster”加快速率，不增加总量。最大偷取速率先应用增加/减少与更多/更少修正，再限制实例速率；生命和魔力每秒 20%、能量护盾每秒 10% 的默认上限仍是项目保留的兼容约定，并非 0.5 补丁说明确认的游戏常量。按主技能平均命中频率估计实例覆盖时间，优先计算恢复速率最高的实例，较慢实例只填补其过期后留下的时间，不模拟随机暴击的时间序列；命中获得与瞬间偷取仍按每次命中相加。PoB2 在没有自身类型化持续伤害时不给出净恢复字段，PoBR 照常计算。

Equipped Focus/Shield defence increases (`energy_shield_from_focus_+%`,
`shield_armour_evasion_energy_shield_+%`) add to global increases only for the
matching off-hand item's locally computed defences. The curated Focus rule is
marked `exporter: differs`: the exporter applies it to any off-hand, whereas
the game text requires a Focus (`UsingFocus`). Recharge rate stats
(`energy_shield_recharge_rate_+%`, `virtual_energy_shield_recharge_rate_+%`)
read `EnergyShieldRecharge`, faster start (`energy_shield_delay_-%_while_affected_by_archon`)
reads `EnergyShieldRechargeFaster`. Thawing Charm's
`charm_energy_shield_recharge_starts_when_used` follows the enabled-charm
convention: an enabled, in-budget charm is in effect (`UsingCharm`), so
`EnergyShieldRechargeBase` is 0 and recharge starts immediately; charm
uptime is not simulated (exporter-absent).

Scarred Faith's “Energy Shield does not Recharge” (`keystone_alternate_es_recovery`,
[game text](https://poe2db.tw/us/Scarred_Faith)) sets recharge rate, amount and
start delay to zero; its separate regeneration prohibition makes net ES
regeneration zero when there are no other gains or drains. Recharge is not
included in the current EHP hit-pool simulation. Resonance (`keystone_charge_cycle`
and `gain_*_charges_instead_of_*`, [game text](https://poe2db.tw/us/Resonance))
redirects Power → Endurance → Frenzy → Power once. With no charge-gain event
model, configured counts above their minima stand in for incoming gains;
minima and maxima stay with their original type, incoming gains merge and
are capped by the destination maximum. On-gain proc effects, duration and
consumption timing are not simulated.

Conduit (`keystone_conduit`, passive 33979: “If you would gain a Charge,
Allies in your Presence gain that Charge instead”) uses the same snapshot
convention: configured counts above their minima represent incoming gains,
so the player's Power, Frenzy and Endurance counts stay at their minima,
including with Resonance. Minimum charges are treated as always held, not
gained, and remain with the player; this convention has no in-game test.
Allies receiving the charges and their effects remain unmodelled: there is
no ally/minion charge-gain model. The rule declares `exporter: "absent"`,
so reference-basis calculations omit it.

Event Horizon
(`lose_%_of_life_and_energy_shield_when_you_use_a_chaos_skill`, passive
`chaos33_`: 3%, [game text](https://poe2db.tw/us/Event_Horizon)) subtracts
3% of each final maximum pool per selected player-used Chaos skill use from
net Life/ES recovery and includes it in `TotalBuildDegen`. It uses the existing
cost-use frequency (one payment per repeat sequence), excludes triggered and
proxy actions, and is neither mitigated damage nor a cost affected by cost
modifiers. Other skills in a rotation and channel start/stop events are not
simulated. These charge and loss rules, and the previously unrecognized Scarred Faith
regeneration wording, declare `exporter: "absent"`.
Thaumaturgical Dynamism (`ascendancy_gemling_charge_generator_buff_display`)
reports a five-second generation interval and 0.2 generation events per second, from game
`BuffTemplates.AscendancyGemlingChargeGenerator`'s
`gain_random_charge_every_X_ms_from_total_socketed_gem_attribute_requirements = 5000`
([buff data](https://poe2db.tw/us/Thaumaturgical_Dynamism)). Random charge
selection is biased by socketed gem attribute requirements, but the [game keyword](https://poe2db.tw/us/Keywords) supplies no numeric probability formula.
`calc::charge_generation` reports the total event rate, directed to allies by Conduit;
multiple grants do not stack. It leaves configured charge counts unchanged and does
not simulate consumption, per-type probabilities, extra on-gain chains or uptime.
These event views have no invented PoB reference key.

Full DPS（`calc_orchestrator::full_dps::calculate`，`DataOrchestratorOptions::full_dps`，默认关闭）是构筑码中勾选 `includeInFullDPS` 的组里每个启用效果的持续输出之和，不模拟技能轮转。数量取组的 `groupCount`，没有时取宝石的 `count`（可为小数，不是正的有限数时不计入）；每个效果按自己的武器组、天赋、辅助与触发环境计算，召唤物行用该召唤宝石创建的召唤物；物品内建触发技能的自动行在可编辑的组已纳入同一技能时跳过。命中 DPS 乘数量相加；流血、点燃、毒共用各自的层数上限，单层伤害高的先占层（`full_dps::allocate_stacks`）；同一技能不可叠加的持续伤害只取较强的一份，可叠加的（`DotCanStack`）乘数量，持续伤害总上限与最强的 Culling Strike 倍率各只计一次。逐技能的行（`FullDpsSkill`：数量、单实例命中 DPS、分到的持续伤害、最终贡献、未支持词条与错误）之和等于 `FullDPS`。`includeInFullDPS`、`groupCount`、宝石的 `count` 与 `enableGlobal1/2` 导入导出时保留，WASM 请求里是组的 `include_in_full_dps`、`group_count` 与宝石的 `count`；Web 的技能页编辑它们，只在有组纳入时请求 `full_dps: true`。

Uhtred 系辅助（“+2 to Level of Supported Skills if exactly one other Support is modifying them”）只把辅助宝石算作辅助：meta 宝石（Cast on Elemental Ailment）的触发效果是技能宝石的第二个效果，不算；只辅助 meta 宝石的辅助（Boundless Energy II 要求 `GeneratesEnergy`）也不修改被触发的技能（`skill_env::resolve::support_granted_gem_levels`）。“Skills from Corrupted Gems have …” 在主技能取等级的那颗宝石已腐化时生效（条件 `GemCorrupted`，`calc_orchestrator::skill::resolve::from_corrupted_gem`）。

Into the Breach 的紫色火焰（“7% of damage as extra Chaos … stacks up to 10 times”，[Into the Breach](https://poe2wiki.net/wiki/Into_the_Breach)）按 `breach_flame_chaos_addition_%` 乘配置的火焰数（`PurpleFlamesCount`，最多 10），每份的数值再乘残骸效果：技能自身与辅助宝石的 `remnant_effect_+%` 加上角色的 “increased effect of Remnants”（生成残骸的技能，`GeneratesRemnants`）。“Effect and Duration of Flames of Chayula on You is Doubled” 与只取红 / 蓝火焰的词条还没有建模。Decompose 的毒云 “Poisons enemies as though dealing Chaos damage equal to {0}% of Corpse's maximum Life” 必定中毒；它不命中，所以不能暴击，也没有暴击伤害加成（`skill_stats::corpse_damage_without_hit`，所选 stat set 没有自己的基础伤害时带 `NeverCrit`）。毒云的爆炸带基础火焰伤害，是命中，按技能的暴击率。PoB2 的毒云暴击率为 0 但保留暴击伤害加成，所以 `CritMultiplier` 与参考值不同。

光环由 `calc/buff_pass.rs` 统一处理；旧的 `buff-pass-aura` Cargo feature 和编排层直接注入光环的通道均已删除。运行时 buff 阶段受 `mode_buffs` 控制，curse/debuff 另受 `mode_effective` 控制；不要按历史迁移文档重新引入旧开关。

Archon shared bonuses use the explicit `conditionAffectedByArchon` configuration
(default off; possessing a source does not establish uptime). The game's
[Archon keyword](https://poe2db.tw/us/Archon_Buff) defines a 10-second duration
and a 20-second recovery period; `ArchonDuration` and `ArchonRecoveryPeriod`
report these nominal timings after `archon_duration_+%`,
`archon_delay_expires_x%_faster` and `archon_no_recovery_period` (Dominion).
Invigorating Archon's movement and elemental-resistance grants are scaled by
Archon effect plus buff effect on self; Energising Archon's conditional recharge
start bonus is not part of the buff and is not scaled. Rules are exporter-absent.
The Command-use gain chance is recognized but does not imply an active buff;
proc timing, uptime, and intrinsic payloads of individual Archon types are not
modelled by this shared-bonus state. Nature's Archon needs direct BuffTemplates
verification before its intrinsic damage is implemented: the keyword says 25%
more damage, while the displayed `ArchonNature` template uses
`plant_skill_damage_+% = 25` (increased).

队伍保存在 `Build.party`；`party::parse_party` / `party::party_xml` 保留 `<Party>` 的 `ImportedBuffs` / `ExportedBuffs`，导出启用成员的合并标准块，并用可忽略的 `PoBRMember` 保留成员边界与禁用状态。`party::import_member` / WASM `build_api::import_party_member_json(code)` 先清空成员自身的队伍输入，再算生命、能量护盾、魔力、属性和已支持的外放光环／诅咒；能重算的同名效果替换导出值，其余沿用原始块。队友效果走 `calc::buff_pass` 的接收者缩放、召唤物投递、最强副本与施法者各自的诅咒上限，敌人效果走既有敌人通道；来源 id 为 `party.<成员>.<目标>.<来源>`，参与缓存身份与逐行归因。战吼、链接等不能从原生技能重建的效果依赖导出块或手填文本；仅支持简单的 `parent` / `partyMembers` 资源属性缩放。嵌套 LIST、复杂标签及不支持的 `GlobalEffect` 字段保留并诊断，不能剥掉限制后当作无条件词条；成员导出的跳过项有警告，PoB2 再保存可能丢弃成员边界。

## 与 PoB2 的关系（逐步独立）

方向（项目所有者，2026-10）：PoBR 是独立实现的引擎，不是 PoB2 的移植。PoB2 的构筑码导入导出是要保留的产品功能；它的代码和数据是要逐步减少的依赖。“逐步”是约束的一部分：不做一次性重写，每一步都保持门禁通过。

新代码的做法：

- 机制以游戏自己的定义为依据实现：`data/<版本>/base/` 的 stat id、技能数据和游戏常量，词条文本，官方 patch notes。代码和测试的出处写这些（例如 `attack_damage_final_permyriad_per_rage = 100`），PoB2 只用于交叉核对。范例见 `pobr-core/src/calc/rage.rs`。
- 技能 stat 的含义写进 `pobr-core/src/rules/skill_stats.rs`：以游戏 stat id 为键，用引擎自己的 modifier 表达，出处是该 stat 的游戏描述文本。这张表先于抽取来的 `skill_stat_map.json` 查询，表里有的 stat 不再读抽取数据；抽取映射拒绝或缺失的 stat（`mise run diagnose` 第 4 节）优先在这里补，而不是去扩充按 PoB2 名称写的白名单。
- 不再新增“因为 vendor `X.lua:行号` 这样写”式的注释；行号随 vendor 升级失效，也说明不了游戏规则。改到旧代码时顺手把这类注释换成规则本身的描述。
- PoBR 与 PoB2 结果不同时，先判断谁符合游戏的词条文本和数据，不默认 PoB2 正确。已知的有意差异：
  - Boss poise uses the current game table rather than vendor presets. In patch 0.5.5.4.2, `MonsterVarieties` row 1946 (Xesht) has `PoiseThreshold = 1700` (17×), while the pinned PoB2 Xesht preset uses 838% MORE (9.38×). The curated game-source rows and export hash are in `data/overlay-common/boss_poise_thresholds.json`; map-boss mode 313 agrees with the older 213% MORE preset. On worked Shaman Spark `5dcf9ecdd375`, `HeavyStunBuildupAvg` is 3.627930% versus the oracle's 6.578395%; the dominant oracle/engine factor is 17/9.38 = 1.812367 (remaining hit/rounding difference about 0.05%). Pinnacle and Uber use Xesht as the representative monster; no distinct Uber Xesht variety exists in this export. This threshold also affects Freeze/Electrocute poise buildup; it is separate from unique rarity's 6×.
  - 偷取每种资源只取恢复速率最高的有效实例（[0.5 补丁说明](https://www.pathofexile.com/forum/view-thread/3932540)：“Only a single Leech instance per resource (Life, Mana, or Energy Shield) can apply at a time.”）；[游戏关键词](https://poe2db.tw/us/Keywords)给出一秒基础恢复时间，偷取量和偷取速度分别修正。钉定的 PoB2 仍叠加实例，保留按资源池计算的单实例偷取量上限，并把“increased amount … Leeched”当作最大速率加成。tuned 组的 Deadeye（Twister，`bed1edc6d8b2`）`ManaLeechGainRate` 为 PoBR 80.72 对参考值 166.17；Pathfinder（Ice Shot，`728cbde29279`）同一字段为 162.96 对 190.12；Blood Mage（Arc，`5bc8be0fa967`）`LifeLeechGainRate` 为 513.68 对 575.23。去掉单实例偷取量上限后，原来吻合的三行 `LifeLeechGainRate` 也出现有意差异：Warbringer（Shield Wall，`1e4e329e5e5c`）为 582.978 对 552.40416，Blood Mage（Comet，`bc750d615159`）为 641.43144 对 207.01274，Gemling Legionnaire（Frostbolt，`42f790c0f9ba`）为 0.2 对 0.0635；这些行已登记在 `devs/scripts/known-differences.json`。前述默认速率上限仍是兼容约定，平均命中频率也只是覆盖时间估计；这些数值不是游戏内实测结果。
  - PoB2 用自己的符文表覆盖物品原文；PoBR 以物品原文为准。同一物品上同一 stat 的符文在原文里合成一行：tuned 组的 Infernalist（Comet）武器上是一行 “45% less maximum Life”，PoBR 按 0.55 计，PoB2 按两块符文的 15% 与 30% 分别连乘（0.595），所以它的生命比参考值低。
  - 同一个诅咒由两个技能组施加时，敌人身上只有最强的那一份（“Debuffs of the same type do not stack, and only the copy with the strongest effect applies”，[Curse](https://www.poe2wiki.net/wiki/Curse)），比较的是各份自身与其辅助宝石的诅咒效果（`skill_env::buff_specs::curse_strength`）。PoB2 按自己的优先级表挑，光环来源优先：tuned 组的 Acolyte of Chayula（Decompose）有 Doedre's Undoing 组和 Blasphemy 组两份 Despair，PoB2 用后者（敌人混沌抗性 −31），PoBR 用更强的前者（−58）。
  - 尸体爆炸的伤害类型取自游戏 stat：`corpse_explosion_monster_life_permillage_<类型>` 与 `corpse_explosion_monster_life_%_<类型>` 写明类型；不带类型的 `corpse_explosion_monster_life_%`（Fiery Death）按它的文本 “Explosion deals base Fire Damage equal to {0}% of the Corpse's Maximum Life” 是火焰伤害（`skill_stats::corpse_explosion_damage`），PoB2 按物理计。
  - Doedre's Undoing 的 Witchtoad 每 0.25 秒生成一只，PoB2 把这个间隔乘以施法时间，施法越快爆炸越多且没有上限；PoBR 按“区域数 × 1000 / 间隔”计算，与施法速度无关（见 `skill_stats::granted_skill_modifiers`）。Dark Consequences 自带魔力消耗为 0，但 Archmage 的“Non-Channelling Spells cost an additional … of your maximum Mana”仍给它本身增加消耗；这不是继承诅咒的消耗。tuned 组 Blood Mage 的 5.7% 最大魔力经 Atalui 转成生命、Efficiency II 减少 40% 后为每次爆炸 199 生命，两区域每秒 8 次为 1592。PoB2 的每秒生命消耗却用它的施法速率 2.14/s，得到 425.86；PoBR 保留按爆炸频率支付，伤害计算不变。
  - The frozen Forbidden Rites 2026-10-26 rank files `blood-mage-03/04/06/08/10` and `chronomancer-04` select Dark Consequences, despite their Impending Doom titles. The same Witchtoad interval gives 8/s; after dividing out the oracle's cast-speed-dependent rate, the five Blood Mage TotalDPS differences are below 1.5%. Chronomancer also differs in Hindered state, so only its rate rows are classified as explained. New known-difference entries bind the decoded XML SHA-256 as well as the short rank name, preventing reuse against another snapshot.
  - Instruments of Power 的 “50% reduced bonuses gained from Equipped Focus” 让 focus 上的 stat 按一半给予，“+# to Level of all X Skills” 不低于 1（[Instruments of Power](https://www.poe2wiki.net/wiki/Instruments_of_Power)）：focus 的 +2 法术等级计 +1，+1 仍为 +1（`skill::resolve::scale_slot_gem_property_bonuses`）。PoB2 减半 focus 的其他词条，宝石等级不减半：demo 语料的 Disciple of Varashta（Comet）的法术比参考值低一级，TotalDPS 0.86×，`ninja_parity` 的进攻与持续伤害基线各降 1 / 2 格。
  - Bonestorm（“Channel to conjure a swarm of bone spikes in the air, then release to fire them at enemies and explode”，“Can fire up to 20 Projectiles”，[Bonestorm](https://poe2db.tw/us/Bonestorm)）引导时每次施法召出一根骨刺，释放时全部射出：PoBR 按每根骨刺命中一次计，每秒命中数即施法速率；一次引导最多 20 根（`base_number_of_allowed_bone_storm_projectiles`），不改变每秒的骨刺数。PoB2 的引导层数（`Multiplier:BonestormStage`）没有配置时为 0，按最短引导时间 0.3 秒（`base_minimum_channel_time_ms`）命中一次：tuned 组的 Abyssal Lich（Bonestorm）PoB2 每秒 3.33 次，PoBR 11.96 次。**没有游戏内实测**：假设每根骨刺都命中目标。
  - “Minions have 10% chance to inflict Withered on Hit” 在没有召唤物的构筑上，PoB2 把这条词条当作玩家自己能施加 Withered，并按配置的层数给敌人加混沌承伤；PoBR 把它交给召唤物，角色自己的技能、Presence 或已有的召唤物能施加 Withered 时才计入配置的层数（`calc/withered.rs`），所以这类构筑会比参考值低。每层 5%、最多 10 层两边一致。
  - Lament / Portent / Absent Amulet 授予的技能不保留精魄（游戏表 `ItemInherentSkills` 的 `NoReservation`，三个基底列在 `overlay-common/item_granted_skills.json`）。PoB2 导入角色时把这个技能再列成一个普通宝石组并按宝石保留 30 / 60 / 100 精魄，所以这类构筑码自带的 `SpiritUnreserved` 常为负数，而游戏里保留量不可能超过精魄总量。PoBR 把“排在组首、等级与授予等级相同、无品质、未腐化”的那个普通组认作被授予的技能本身，不计保留，也按授予技能的需求降级（见“宝石等级与品质加成”后一段）；其余同名宝石照常保留（`calc_orchestrator/granted_skills.rs`）。其他物品授予、需要保留精魄的技能也是这样认出的同一个技能，只保留一次；PoB2 的物品组和宝石组各保留一次（tuned 组 Gemling Legionnaire Frostbolt 与 Oracle Grim Pillars 胸甲授予的 Blink：PoB2 120，PoBR 60）。PoB2 只对仅由物品授予的技能（`fromItem`，如 Purity of Ice）这样合并，所以 Absent Amulet 的 Eternal Rage 在 PoB2 里是两个组：降级的物品组和未降级的宝石组。该技能上的辅助宝石追加的精魄也一并免除，PoB2 仍计入：依据是权杖授予的技能在游戏数据里以 `reservation_multiplier = -100` 表达“不保留”，辅助的追加量随之归零；这一点没有游戏内实测。
  - Rain of Arrows 的命中频率按游戏的两条规则计算（`pobr-core/src/calc/rain.rs`）：“One arrow lands every 0.08 seconds”（`rain_of_arrows_delay_per_arrow`，“Modifiers to Projectile speed apply to arrow landing frequency”）和 “Can Hit each enemy only once every 0.12 seconds”（`rain_hit_delay_ms`）。命中只能来自落下的箭：间隔过后落下的第一支箭命中，所以每 ⌈0.12 ÷ 落箭间隔⌉ 支箭命中一次；一次使用射出 N 支箭（技能的投射物数量），一场箭雨最多命中 ⌈N ÷ 该数⌉ 次，使用得慢、箭雨在下一次使用前已经落完时按每次使用的命中数封顶（“Maximum 1 arrow rain”）。PoB2 把命中间隔记为 max(0.08, 0.12) 秒，等于假设敌人每次能被命中时恰好有箭落下，也不计投射物速度：没有投射物速度时游戏规则给出每秒 6.25 次，PoB2 为 8.33 次；tuned 组的 Rain of Arrows 构筑（169% 增加的投射物速度和 Projectile Acceleration III 的 40% 更多投射物速度，每 0.0212 秒落一支）是每 6 支箭命中一次，每秒 7.85 次。以下是假设，**没有游戏内实测**：每支落下的箭都打到敌人（箭雨半径 3.6 米、单支箭的冲击半径 0.7 米，对单个敌人实际更少，取决于体型）；落箭节奏均匀，按游戏的数值、不按服务器帧取整；角色持续使用该技能；品质的 “Arrows fall #% faster” 是箭下落的速度，不改落箭频率。同样带 `rain_hit_delay_ms` 的 Firestorm、Icestorm 见下一条；Hailstorm Rounds（`rain_hit_delay_ms` = 200）的每次发射提供一次落点，连续发射时每 ⌈0.2 × 发射速率⌉ 次落点命中一次，再按弹匣与装填周期折算（`perform::fill_crossbow_reload`），一次攻击的多个投射物共用这个命中间隔，命中限制不减慢弹药消耗；`siege_cascade_fall_time_ms`（300）是飞行时间，不是落点间隔。Hailstorm Rounds 没有模拟散布和弹匣交界处的命中锁定相位，**没有游戏内实测**。参考口径（`on_reference_basis()`）按 PoB2 的计法比较：箭雨每当敌人能再被命中时就算一次，Hailstorm Rounds 取发射速率与每秒 5 次中的较小值。
  - 球与风暴的命中不随施法速率（`pobr-core/src/calc/periodic.rs`，stat 由 `rules::spell_hits::modifiers` 映射）：Ball Lightning “Hits enemies in range every {0} seconds”（`ball_lightning_base_hit_frequency_ms` = 200），Solar Orb “Pulses every {0} seconds”（`solar_orb_base_pulse_frequency_ms` = 1400）；Firestorm / Icestorm 每 `fire_storm_fireball_delay_ms`（200 / 150）落一次，按上一条箭雨的落点规则受 `rain_hit_delay_ms`（150）限制；Thunderstorm “Hits one target every {0} seconds”（`lightning_storm_hit_frequency_ms` = 350），同一敌人 `lightning_storm_hit_prevention_duration_ms`（500）内只被命中一次，取两者约束下的潜在频率，不模拟 `lightning_storm_hit_frequency_variance_ms` 的随机间隔。角色维持一场风暴或一个球，多个不叠加；有持续时间的按 min(持续时间 × 每秒使用次数, 实例上限) 计覆盖率（最多 1），Ball Lightning 没有这类持续时间 stat，假设目标一直在范围内、不估算球的行进。Ball Lightning、Solar Orb 的其他 stat set 不继承这个频率。PoB2 假设风暴与球始终覆盖目标，参考口径关掉 `DataOrchestratorOptions::periodic_uptime` 按它比较。**没有游戏内实测**：假设敌人始终在范围内，每个符合间隔的落点都命中。
  - 引导攻击（Detonating Arrow、Supercharged Slam、Perfect Strike、Gathering Storm、Snipe、Shield Charge，`calc::channel::skill_modifiers`）的一次使用近似为“释放前引导 + 释放”（`pobr-core/src/calc/channel.rs`）。引导取 `base_minimum_channel_time_ms`、`channel_start_lock_cancelling_of_attack_time_%` × 攻击时间、层数 × 攻击时间中最大的一个；完美时机只给出窗口宽度（如 `perfect_strike_timing_window_base_ms`），没有开启时间的 stat，所以完美与普通 stat set 各算自己的伤害，时间都用最短引导 / 起手锁定。释放取 `channel_end_duration_as_%_of_attack_time` 的比例，或 `channel_skill_end_animation_duration_override_ms`（有时优先，两者不相加）；比例随攻击速度缩放，固定毫秒不缩放，只有动画倍率、没有基础时长的阶段不另计。层数同时用于伤害和时间，默认取上限（含品质），构筑配置的 multiplier `ChannelStages`（配置输入 `skillChannelStages`，Web 技能页的层数输入）可覆盖（取整并限制在 1 到上限）：Detonating Arrow 每层 `detonating_arrow_all_damage_%_to_gain_as_fire_per_stage` = 120；Supercharged Slam 每层 “40% more Damage with Hits for each stage”（只加命中），“Releases 1 Aftershock per Stage”，冲击 stat set 计冲击加全部余震、余震 stat set 只计余震，用等级行 850 毫秒不受攻击速度影响的攻击时间，余震延迟不阻止下一次引导。Shield Charge 按 1300 毫秒最短引导，不模拟行进与碰撞。PoB2 多只把释放阶段当作命中间隔、默认一层并用武器的攻击时间。**没有游戏内实测**。
  - 带 `base_skill_show_average_damage_instead_of_dps` 的技能（Grim Pillars、Lightning Warp、Vile Disruption、Skyfall、Living Bomb、Volcano、Incinerate、Rake、Parry、Raise Shield、Hand of Chayula、Stampede、Gathering Storm、Perfect Strike 等）在游戏面板上显示平均伤害；这个 stat 没有词条文本，不改变伤害或使用速率。PoBR 的 `CombinedDPS` 仍是每秒伤害：命中 DPS 加持续伤害 DPS，再计 Culling Strike；PoB2 在这里改填平均命中伤害，也不加它已算出的持续伤害（Incinerate 的 `base_fire_damage_to_deal_per_minute`、Living Bomb / Volcano 的点燃、Rake 的流血）。这是输出口径不同，不能拿这一行当作伤害倍率去补。Vivid Stampede（`skill_triggered_on_hit_while_have_vivid_stag_wisps`，攻击命中触发、每个 Vivid Wisp 一只鹿）同样只显示平均伤害：PoB2 的 TotalDPS 为 0、CombinedDPS 是每次触发的伤害；PoBR 暂按武器攻击速率每次攻击一只估算，精灵数量与触发来源还没有建模，两边不能比较。
  - Repulsion Wave、Shield Wave、Combust 与 Caltrops 保留平均命中伤害，事件频率未建模时 `ActionRate` / `TotalDPS` 为 0。官方 `ActiveSkills.Description` 中，Repulsion 需要命中被诅咒的敌人，Fortifying Cry 需要后续盾牌攻击造成伤害，Temper Weapon 需要被强化的攻击命中；Trail of Caltrops 在闪避时布置，敌人触发时才造成伤害。`caltrops_on_dodge_X_triggers = 3`、首次在闪避的 5%、间隔为闪避的 20% 只定义布置时机，不能从武器攻速推算闪避或敌人触发频率；Combust 的 100% 触发几率也不能替代强化次数与攻击来源。`CombinedDPS` 仍是每秒伤害，不改填平均命中；因此 sweep 的 `pobr_zero` 分类可能已有非零 `AverageHit`，不能据此判定没有伤害。20 级 Repulsion Wave 的基础物理伤害为 128–192 × 2.04，Shield Wave 为 (85–128 + 每完整 15 点盾牌护甲的 5–7) × 2.04。Blood Explosion 为 196–294 × 4.08、基础暴击率 5%；其 sweep 差异来自前述攻击命中检查，不补伤害倍率。Blood Explosion 的现有每秒一次口径尚未按失血、重度眩晕或死亡事件建模。这些事件模型均未做游戏内实测。
  - 伤害区间与命中率保留小数：Earthquake / Stampede 的 Wooden Club 6–10 乘等级行的 `damage_multiplier` 1.25 为 7.5–12.5，Toxic Growth 的 6–9 乘 0.62 为 3.72–5.58；PoB2 先把伤害上下界取整（8–13、4–6）、命中率取整到百分数，低伤害的合成构筑上差别可超过 5%。这是计算精度的约定，游戏内部是否取整**没有游戏内实测**。

    The 2026-10-16 ranged sweep has the same precision difference. All six fixtures use
    the game's Makeshift Crossbow (`base/base_items.json`, `FourCrossbow1`):
    7–12 Physical damage, 625 ms attack time and 5% base crit. Level-20 damage
    multipliers and conversion percentages below come from the first stat set
    in `base/granted_effect_stat_sets.json`; attack-speed multipliers come from
    `base/granted_effect_levels.json`. No added damage or damage increases apply.
    Each bounds cell shows PoBR's result after conversion → PoB2's integer bounds
    before mitigation and crit; the arrows do not indicate an engine change.

    | Selected effect | Damage multiplier / conversion | Physical bounds | Elemental bounds | TotalDPS PoBR / PoB2 |
    |---|---|---|---|---|
    | `IceShardsPlayer` | 0.29 / 60% Cold | 0.812–1.392 → 1–1 | 1.218–2.088 → 1–2 | 5.423944799 / 4.952566667 |
    | `ShockburstRoundsPlayer` | 0.36 / 40% Lightning | 1.512–2.592 → 2–3 | 1.008–1.728 → 1–2 | 4.615380330 / 5.334733228 |
    | `PermafrostBoltsPlayer` | 0.49 / 80% Cold | 0.686–1.176 → 1–1 | 2.744–4.704 → 3–5 | 1.183943073 / 1.279610526 |
    | `RapidShotPlayer` | 0.94 / 60% Fire | 2.632–4.512 → 3–5 | 3.948–6.768 → 4–7 | 23.651653516 / 24.984921520 |
    | `ArtilleryBallistaProjectilePlayer` | 0.40 / none | 2.8–4.8 → 3–5 | — | 0.306053311 / 0.324168000 |
    | `SiegeBallistaProjectilePlayer` | 0.66 / 60% Fire | 1.848–3.168 → 2–3 | 2.772–4.752 → 3–5 | 0.807980742 / 0.850941000 |

    Both sides use one projectile hit per action in these selected stat sets,
    75% Physical reduction and 50% elemental resistance. With physical and
    elemental midpoints P and E, DPS is `(0.25P + 0.5E) × h × (1 + 0.05h) × rate`:
    PoBR retains hit chance h = 0.775345425; PoB2 uses 0.78 (also in crit
    confirmation). In table order, both sides' sustained rates are
    6.111111111, 4.787401575, 0.701754386, 8.221225710, 0.4 and 0.4/s.
    The four ammo skills' firing rates are 8.8, 6.4, 1.6 and 8.8/s; magazine
    sizes are 16, 19, 1 and 100, with reload times 0.8, 1, 0.8 and 0.8 seconds.
    Substituting the respective bounds and h reproduces every TotalDPS above;
    no projectile-count, damage-multiplier or reload correction is justified.
    This confirms the existing precision convention, not the game's internal
    rounding. No benchmark or tuned build carries these six skills at this
    audit, so no worked-build row is added to `known-differences.json`.

  - Bursting Fen Toad、Conductive Runes 的 Explosion、Blood Explosion 与 Repulsion Wave 是非武器攻击（游戏类型 `Attack` / `NonWeaponAttack`，Bursting Fen Toad 的说明是 “dealing Attack damage”），stat set 没有 `global_always_hit`，照常检查命中；PoB2 给它们的基础标志缺少攻击，不检查命中。没有游戏内实测证明这类爆炸不能被闪避。Primal Strikes 的 Exposure（`all_exposure_on_hit_magnitude` = 50，“The third attack is a large swipe that inflicts Elemental Exposure”）只在第三击的 stat set `PrimalStrikesFinalPlayer` 上，计算普通击时不假设已用过第三击；PoB2 默认计入（敌人元素抗性由 50 降到 25）。连段的覆盖率还没有建模。
  - 主 stat set 带 `base_deal_no_damage` 的效果（Conductive Runes 的放置、Rolling Slam、Spearfield、Tempest Bell 的主 stat set，Hollow Focus 的 Bells）PoBR 为 0，钉定的 PoB2 可能给它们算出默认武器或附加伤害（见“计算引擎架构”的无伤害效果）。
  - Black Powder Blitz 按游戏等级表的冷却限制使用频率；Requiem 按 `crossbow_skill_has_no_ammo` 不计弹匣与装填时间，不能用重复装填降低它的持续射击频率。两者与钉定 PoB2 的默认周期可不同，不改伤害倍率追平。
  - 图腾放置时间按召唤技能自己的 `cast_time`：Spell Totem（`SummonMetaTotemSpellTotemPlayer`）500 毫秒，Shockwave Totem 600 毫秒；PoB2 的 Spell Totem 按 600 毫秒，tuned 组 Grim Pillars 构筑的 `TotalTotemPlacementTime` 与按放置频率计的每秒消耗随之不同。
  - The Vertex 的 “Equipment has no Attribute Requirements”（游戏 stat `global_equipment_no_attribute_requirements`）让所有装备的属性需求归零，包括武器；PoB2 未应用这行。Spirit Walker Zekoa（e071）的武器需求因此不是 180 力量 / 450 智力，基准组 Blood Mage Impending Doom（add1）的 Array Buckler 也不要求 78 敏捷；宝石和辅助宝石的需求照常计算。
  - 物品授予的技能（例如 “Grants Skill: Level N Eternal Rage”）的属性需求只决定它降到哪一级（见“物品授予的技能”一段），不计入 `ReqStr` / `ReqDex` / `ReqInt`；构筑码另列的同等级普通组被认作同一技能，也不计入（`calc_orchestrator::requirements::requirement_snapshots`）。PoB2 按宝石计入，因此 Wolf Pack / Eternal Rage 抬高参考值的力量需求，Trinity 抬高敏捷需求，Blink / Cast on Critical 抬高智力需求；权杖的 Purity of Ice 和护身符的 Time of Need 也适用。
  - Cast on Critical 里的法术，PoBR 按宝石触发它的频率计算 DPS 和使用速率（`pobr-core/src/calc/energy.rs`）；PoB2 没有能量模型，它的触发配置要求同组有攻击技能（PoE1 的用法），找不到就退回法术自身施放速率，构筑码自带的参考值因此是自身速率口径。规则取自游戏数据和文本：能量上限是支援半的 `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`（每 0.1 秒基础施法时间 10 点），技能自带的 `total_cast_time_+_ms` 按术语表 Energy 条目计两倍（Comet 为 300）；每次暴击获得 `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit = 100`（每点敌人 Power 1 点）× 暴击伤害占敌人异常阈值的比例 × 能量获取修正（`energy_generated_+%`，即“Meta Skills gain #% increased Energy”）；独特怪 Power 恒为 20；被触发的技能不产生能量，所以充能来源是角色自己使用的其他伤害技能，取填满最快的一个，没有来源时法术保持自身速率。触发频率 = 来源技能每秒暴击次数 ÷ ⌈上限 ÷ 每次暴击能量⌉。**没有游戏内实测**，以下是假设：比例按“减免前暴击伤害 ÷ 阈值”线性计算且不封顶（玩家测得的公式，游戏未公布）；每次暴击取平均暴击伤害；每次使用按命中一次算；首领的异常阈值只按等级取表（游戏 stat 表里有 `ailment_threshold_pluspercent_final_for_map_boss`，取值不在已入库数据里）；获得能量时视为“近期造成过暴击”。自动触发按整次事件充满，多出的 Energy 丢失；半额返还按概率加权下一轮的事件数（`calc::energy::triggers_per_second`）。`DataOrchestratorOptions::energy_triggers = false` 保留参考口径的自身速率或既有触发配置路径。
  - Cast on Elemental Ailment、Cast on Block、Cast on Minion Death、Spellslinger、Elemental Invocation 同样按 Energy 计算（`skill_env::triggers::energy_trigger_modifiers`、`calc::energy::apply_energy_trigger`）。格挡每次给 25，召唤物被击杀时每点 Power 给 50；Spellslinger 按自施法的基础使用时间每 10 ms 给 0.2，Total use time 的加法同样计双倍。元素事件只有点燃、感电、冻结，没有冰缓；每点敌人 Power 的感电给 1、冻结给 10，点燃再乘新实例完整减伤前伤害与异常阈值之比，不封顶。20 级前四族自带 57% increased Energy，Elemental Invocation 为 68%；兼容 meta 的增益与半额返还辅助作用于 meta 本身，不要求能辅助 payload。Spellslinger 与 Elemental Invocation 的 500 储能和每轮法术消耗分开，后者每品质加 5 储能；自动触发的容量按同组被支持法术的总消耗计算。
  - Energy 的外部事件输入由 `config_resolve::resolve_config` 读取：`energyIncomingHitRate` 是可格挡来袭命中／秒，乘有效格挡率；未填才回退到显式 `enemySpeed` 的 1000／毫秒，显式 0 优先且表示零事件，`enemySpeed = 0` 同理。该回退把一次敌人动作当作一次可格挡命中，不另算敌人命中或玩家闪避。`energyMinionDeathsPerSecond` 只计被击杀，不模拟到期、生命或死亡；`energyMinionPower` 默认 1。两种速率缺失或没有格挡等必要机制时保留法术自身频率；显式零事件停止触发。`energyInvocationUsesPerSecond` 未填时假设有能量即使用，受冷却和服务器 tick 限制；显式 0 即使无来源也停止使用。Invocation 保留余能，按平均返还消耗及 ⌊容量／整轮消耗⌋ × 使用频率限制吞吐；不分别排程 payload，不模拟整轮超容量时的部分触发。上述外部速率和 Invocation 使用模型**没有游戏内实测**。
  - Energy 的来源近似是按基础频率预选最多六个玩家自用、非触发且非持续保留的技能，再取吞吐最高的一项；不叠加多来源轮转、多目标或额外投射物重叠。异常事件用单目标命中频率与施加概率；感电沿用非暴击来源近似，冻结按冷伤 buildup 向上取整所需命中数，不计冻结持续时间和再冻结间隔。混合异常按概率平均每事件 Energy，不模拟同一击的事件顺序；点燃比例、这些事件率和来源策略均**没有游戏内实测**。Barrier Invocation 与 Cast While Channelling 的持续 Energy 吞吐尚未建模，后者只展示首次充满时间。
  - CLI/WASM 的 `energy_trigger` 给出储能、每轮消耗、来源、事件率、每事件 Energy、返还和频率上限。六族缺来源时带 `at_own_rate: true`、`source: null` 与稳定的 `missing_source`，命中与持续伤害保留自身速率；`SkillTriggerRate` / `TriggerRateCap` 的明细给出自身速率与补充输入提示，原始触发率字段仍为未解出的 0。有来源时 CoC 的 `TriggerRateCap` 是来源暴击事件率。`ActionRate` / `TotalDPS` 的派生公式包含 Energy 调整，Full DPS 使用调整后的输出；CLI 保留 `energy_per_critical_hit` 兼容字段。四个配置项由 `data/overlay-common/energy_config_options.json` 经 `extract_config_options::append_energy_options` 合入目录，按稳定 meta id 的 `if_skill` 筛选，重新生成不重复。
  - 图腾使用的技能（图腾宝石自己的技能，或插在法术图腾这类 meta 图腾宝石里的技能，游戏类型 `UsedByTotem` / `SummonsTotem`）按“角色有图腾、最近召唤过图腾”计算，施法和攻击不算角色自己的（不置 `CastSpellRecently` / `AttackedRecently`）。“每个已召唤图腾”类词条按 1 个图腾计，配置填了“已召唤图腾数”时按配置。PoB2 默认按图腾上限计，而它给法术图腾里的技能算出的上限不含宝石随等级增加的 `base_number_of_totems_allowed`（两个 tuned 构筑上是 2，按游戏数据 23 级以上的法术图腾是 3，再翻倍）。PoBR 的图腾上限 `ActiveTotemLimit` 只是展示（见下文 `calc::totem` 一段），伤害仍不假设达到上限（`calc_orchestrator/prepare.rs`）。异常的层数同理：插在法术图腾里的技能，PoB2 按该技能自己的图腾上限乘层数，而上限的 stat 在图腾宝石上，它取到 0，点燃为 0；PoBR 按 1 个图腾的命中计算点燃（tuned 组两个 Grim Pillars 构筑的点燃只有几十，参考值为 0）。
  - Charged Staff 的 buff 按 “Buff grants {0} to {1} Added Quarterstaff Attack Lightning damage per Power Charge Consumed, up to your maximum Power Charges”（`charged_staff_attack_minimum_added_lightning_damage_per_stack` / `charged_staff_attack_maximum_added_lightning_damage_per_stack`）计算：每消耗一个暴击球一层，最多为最大暴击球数，只加给 Quarterstaff 攻击。PoB2 把 One with the Storm 的 “count as consuming an additional Power Charge”（`quarterstaff_skills_that_consume_power_charges_count_as_consuming_x_additional_power_charges`）加在这个上限之外，最大 6 个暴击球时按 7 层计，且不限于 Quarterstaff 攻击；PoBR 按可消耗的暴击球数（当前减最小值）计，满球时为 6 层，不读取这个额外的球，暴击球未满时它也不计入（`skill_stats::player_buff_modifiers`）。
  - Purity of Fire 这类光环的技能类型（`Buff`、`Aura` 等）里没有 `Spell`，“increased Critical Spell Damage Bonus” 这类法术词条不作用于它；PoB2 给它的 stat set 写了 `spell = true` 的基础标志，主技能是光环时暴击伤害加成按法术计（tuned 组的 Ritualist Purity of Fire，2.24 对参考值 2.49）。这一行对光环没有实际意义。
  - Lich 的 Crystalline Phylactery 让其中的珠宝按两倍计（见上文 `Build.jewels`），The Adorned 不作用于它。PoB2 不解析 “Effect of bonuses gained from Socketed Jewel”，并把 The Adorned 用在护符匣里的珠宝上，构筑码自带的参考值也是这样算的：tuned 组四个护符匣构筑（Abyssal Lich Twister 4028 / 9d1c、Lich Spark ce24 / 0a17）的伤害、暴击与个别抗性随之与参考值不同。
  - Bear form 按游戏的形态说明 “+10 to Armour per level” 给 10 × 角色等级的护甲，并让 30% 的护甲也作用于元素伤害（`pobr-core/src/calc/bear_form.rs`，主技能带 `Shapeshift` 与 `Bear` 类型时生效）；PoB2 多给 10 点（10 + 10 × 等级），参考值因此略高（约 0.2%）。
  - Pounce 的 Predator's Mark（“Marked target takes {0}% increased damage for each of its enemies within 6 metres of it, up to {1}%”，`skill_wolf_mark_damage_taken_+%_per_nearby_enemy` / `skill_wolf_mark_damage_taken_+%_cap`，品质加在等级值上，20 品质为 4% / 60%）按角色在 Presence 内已有的召唤物计数：计算中创建的召唤物，或配置的 “# of Minions in your Presence”（`MinionPresenceCount`，`skill_stats::curse_modifiers`）。计算 Pounce、Predator's Mark 或其他主技能时，不计 Pounce 在被标记的敌人死亡时召唤的狼：被计算的敌人活着时它们还不存在。显式选择 Pounce 的第三个效果 `WolfPounceSummonWolfPlayer` 为主技能时，创建该效果的主召唤物（`WolfMinion`，默认使用 `MeleeAtAnimationSpeedWolfCompanion`），继承来源宝石的有效等级与辅助。数量按一次激活的一只狼加游戏 stat `skill_cross_slash_triggered_summon_+_wolves_if_rare` / `_if_unique` 的稀有度加成（Rare +0、Unique +1），即普通／稀有敌人一只、独特敌人两只；不是假设达到 20 级 `number_of_wolves_allowed` 的六只上限。只有这个效果是所选主技能时，这些狼才计入 `MinionPresenceCount` / `SummonedMinion`；不推算击杀频率或持续累积的狼数。所选召唤效果的最小 sweep 仍有差异：PoBR 的两只狼使同颗宝石的 Mark 增加 6% 承伤，单只狼 DPS 为 49.8971；只保留召唤效果、去掉 Mark 时为 47.0728，钉定 PoB2 为 46.9565（剩余约 0.25% 来自伤害区间与敌人护甲取整）。这里保留现有 Mark 应用规则，不按参考值补伤害倍率。tuned 组的 Witchhunter 构筑上这个数是 0：它的 Living Lightning II 所在组没有能被辅助的技能，不会出现 Living Lightning；构筑码自带的参考值按 8 只计（+32%），比 PoBR 高。辅助宝石授予的召唤物在辅助能生效时也还没有创建。
  - Presence 内的召唤物数（`MinionPresenceCount`）计入计算中创建的每个召唤物，包括没有数量上限 stat 的同伴，例如 Rhoa Mount 的 Rhoa（技能类型 `Minion` / `Companion`）；PoB2 只计入技能带数量上限 stat 的召唤物。所以 Minion Pact（“Supported Skills remove {0}% of your maximum Life from a nearby Minion on use to deal {1}% more damage”，`support_minion_pact_user_life_%_removed_from_nearby_minion` / `support_minion_pact_damage_+%_final`）在只有这类召唤物的构筑上，PoBR 计入、参考值不计：benchmark 组的 Chaos Inoculation Deadeye 最大生命为 1，每次只从 4182 生命的 Rhoa 抽 1.5 点，35% 更多伤害生效。宝石说明里的 “Only takes effect if you have a Minion with enough life to drain the full amount” 还没有建模（PoB2 也不检查），召唤物生命不够抽取时两边都会多算。
  - Wolf Pack 召唤的狼数按游戏的 `skill_wolf_pack_size` 随有效宝石等级取值（1 / 18 / 20 级为 3 / 5 / 6 只），整群狼都计入召唤物数，并假设都在 Presence 内（**没有游戏内实测**）。Licking Wounds（天赋 55375，“5% increased Life and Mana Regeneration Rate for each Minion in your Presence, up to a maximum of 40%”）因此按整群计：tuned 组 Ritualist Purity of Fire（d557864ceebc）的 `LifeRegenRecovery` / `ManaRegenRecovery` 为 104.4 / 40.8，参考值 91.2 / 33.2。侧栏的 `ActiveMinionLimit` 是主技能实际创建的召唤物上限，没有上限 stat 的召唤物为 1；PoB2 对这类主技能输出 0：tuned 组三个 Spirit Walker（Zekoa the Headcrusher）构筑 PoBR 为 1，参考值为 0。
  - Evergrasping Ring 的 “Enemies in your Presence Gain (6-12)% of Damage as Extra Chaos Damage”（游戏 stat `enemies_in_presence_non_skill_base_all_damage_%_to_gain_as_chaos`）按 stat 名是敌人全部伤害的份额：敌人的每种伤害都按这个比例额外获得混沌伤害，再按敌人对两种类型的加成缩放（`calc::ehp::assemble_enemy_damage`）。PoB2 只从物理伤害获得，所以有这枚戒指的构筑 EHP 比参考值低：tuned 组的 Ritualist Purity of Fire 三枚戒指共 39%，TotalEHP 140,103 对参考值 174,152；Stormweaver Frost Bomb 14%，12,061 对 14,185。
  - 技能自己或它的辅助宝石带的 “increased Blind Effect”（`blind_effect_+%`，例如 Blind II 的 20%）只作用于这个技能施加的致盲。致盲不叠加，敌人身上只有一个，PoBR 取角色的技能里最强的那个（`skill_env::strongest_skill_blind`），转给敌人，与角色自己的 “increased Blind Effect” 相加。PoB2 把这条加成按每个被辅助的技能各算一次、全部加到角色身上：tuned 组的 Amazon（Rain of Arrows）有五个技能带 Blind II，PoB2 的致盲是 −40% 命中，PoBR 是 −24%，闪避率 72% 对参考值 76%，TotalEHP 随之较低。
  - “Enemies in your Presence Resist Elemental Damage based on their Lowest Resistance”（`enemies_in_presence_elemental_damage_resisted_by_lowest_elemental_resistance`）是敌人的属性：Presence 内的敌人承受火、冰、电伤害时都用三种元素抗性中最低的那个，穿透按所用的那种抗性取（穿透降低的是它写明的抗性），命中、持续伤害和异常都经过 `calc::offence::enemy_resist_final`。游戏另有一条 “Elemental Damage from your Hits is Resisted by the enemy's lowest Elemental Resistance” 才限于命中。PoB2 把两条都只用于命中，所以点燃这类持续伤害在敌人某一抗性被降低时比参考值高：tuned 组的 Gemling Legionnaire Twister 点燃为参考值的 1.22 倍。**没有游戏内实测**。

现状清单（`data/<版本>/` 各文件的 `_meta.generator` 可核对）：

| 来源 | 内容 |
|------|------|
| 游戏数据（GGG `.dat` → `pobr-data-adapter`） | `base/` 下的物品基底、词缀、stat、技能与等级（含技能的显示名 `ActiveSkills.DisplayedName`：buff 名、`AffectedBy<名字>` 条件和诅咒优先级都按它，见 `skill_env::buff_skill_name`）、stat set（含无值 stat，如 `base_is_projectile`；召唤物技能另有按使用者等级取值的效能成长）、天赋树、宝石（含授予的效果与宝石标签，游戏表 `GemEffects` / `GemTags`；每个宝石等级需要的角色等级，游戏表 `ItemExperiencePerLevel`）；`overlay/gem_quality_stats.json`；`base/monster_resistances.json`（`MonsterVarieties` → `MonsterTypes` → `MonsterResistances`，普通稀有度的 `Fire1/Cold1/Lightning1/Chaos1`，合并全部链接并保留负数弱点，用于召唤物基础抗性） |
| 从 PoB2 Lua 抽取（`sync-pob-catalog`） | 词条解析规则 `mod_parser_rules.json`、特殊词条 `generated/special_vendor.json`、`mod_scalability.json`、技能 stat 映射 `skill_stat_map.json` / `skill_overrides.json` / `stat_set_labels.json`、配置项 `config_options.json`（含显示条件 `if*` 与静态说明 `tooltip`，只决定配置页显示哪些选项，不决定计算效果）、暗金 `uniques.json`、基底修正 `base_item_overrides.json`、符文 `runes.json`、`gem_effects.json`（宝石与效果的链接已由游戏表写进 `base/skill_gems.json`；此文件留给旧快照和几颗宝石共用的效果，以及效果的显示顺序 `GemEffects.GrantedEffectDisplayOrder`，`pathofexile-dat@15.2.0` 使用的社区 schema 尚未命名这一列，不能按列名导出，显示顺序继续由此文件提供）、召唤物与幽魂（`overlay/minions.json` 的 `monster_variety_id` 从 `Export/Minions/Minions.txt` 的 `#monster` 映射抽取；已入库的召唤 stat 没有可反查的 variety 链接（只有 monster hash 等数值）；幽魂与驯服野兽的 id 已是游戏 variety id。只从 PoB2 获取这一映射，抗性数值读游戏数据）、触发与幻影配置、`curse_priority.json`、`catalysts.json`，以及 stat 描述 `stat_descriptions.json`（来自 PoB2 的导出，不是直接来自游戏客户端）和由它派生的 `stat_id_map.json` |
| 按 PoB2 语义手工整理 | `overlay-common/` 的 `buff_definitions.json`、`special_mods.json`、`high_precision_mods.json`、`local_mods.json` |
| 按游戏数据手工整理 | `overlay-common/item_granted_skills.json`（游戏表 `ItemInherentSkills` 中 `NoReservation` 的基底；adapter 读到这张表后改为生成）、`energy_config_options.json`（Energy 外部事件输入，由配置目录抽取器合入） |
| 回归基准 | `examples/demo-bd-test/builds/*/meta.json` 的对照值由 `tools/pob2-oracle` 重录 |

所以目前机制覆盖不会超过 PoB2，版本更新也要等 PoB2 的导出。建议的退出顺序（每一项单独立项）：stat 描述改为直接取自游戏数据；词条从“正则规则”改为“游戏 stat id → 引擎 modifier”的自有映射（可用现有 `stat_id_map.json` 一次性引导，之后新 stat 以“未映射”出现在审计里而不是等 PoB2）；技能 stat 映射和配置项冻结为自有数据后按游戏数据差异维护；回归基准增加游戏内实测值。

## Parity 体系（回归基准）

PoB2 对照是现有的回归基准（对照值不是真值，见上一节和“排查数值差异”），三层校验互补：

1. **`crates/pobr-build/tests/parity/ninja_parity.rs`** — 遍历 `examples/demo-bd-test/builds/*/`（真实 PoB2 build + `meta.json::player_stats` 黄金数值），零硬编码对比全部职业/技能；`parity_no_regression` 断言聚合命中率不低于基线。防御/属性与 DPS 分列报告，未完成的 offence 管线不会掩盖防御侧信号。基线只在一种情况下下降：PoB2 的对照值被游戏自己的数据证明有误、PoBR 有意与它不同（上一节“已知的有意差异”）；这时在基线常量旁写明是哪个构筑的哪一格、依据哪条规则，其余格子仍须全部命中。
2. **golden / dual-run 套件** — `crates/pobr-build/tests/parity/` 下的 golden 与 PoB2 对照用例，以及 `tests/dualrun/` 下的 statmap / config 对照，钉住中间值与配置语义。
3. **`tools/pob2-oracle`** — 需要逐分量定位偏差时，从 vendored PoB2 直接 dump Lua 侧计算分解对照。

与构筑码参考值比较的入口（`ninja_parity`、`reference_report`、`golden_canary`）统一用 `DataOrchestratorOptions::reference_conditions()`，它由两部分组成：`against_reference_enemy()`（EFFECTIVE 口径、Pinnacle 首领，引擎的模型全部开启）和 `on_reference_basis()`（关掉导出工具没有的模型：meta 宝石里的法术按自身速率；箭雨每当敌人能再被命中时就算一次命中；球与风暴始终覆盖目标，`periodic_uptime = false`）。PoBR 新增一个导出工具没有的模型时，把它的开关加进 `on_reference_basis()`；独立词条规则用下述 coverage 字段，不逐构筑降基线。模型本身由游戏数值的测试验证，不靠参考值。`mise run diagnose` 的第 2 节按参考口径比较，其余各节（丢失的输入、读取并集）来自 `against_reference_enemy()` 下的计算，否则只有该模型读取的词条会被误报成“没有读取方”。CLI `calculate-build` 的 `output` 是游戏口径（`output.energy_trigger` 给出能量上限、充能技能和触发频率），`reference` 块在这种构筑上改按自身速率比较并以 `at_own_rate_of_use` 标明；主技能是箭雨时 `output.rain` 给出落箭间隔、命中间隔和每几支箭命中一次，`reference` 块按导出工具的计法比较并以 `rain_hits_whenever_allowed` 标明；主技能是球或风暴时以 `periodic_hits_assume_sustained` 标明。

PoBR 自有词条规则声明 `exporter: "absent"`（导出工具未实现）或 `"differs"`（已实现但口径不同），并填 `game_source`（游戏 stat id、原文或链接）；缺省与抽取的 `vnd_*` 规则都是 `"same"`。`overlay-common/special_mods.json` 由 gamedata 验证；stat-id 规则在 `rules/skill_stats.rs::exporter_rule` 登记，stat id 本身就是来源。modifier 持有共享的规则元数据，替换物品/宝石 origin 时仍保留，嵌套 payload 也一样。`on_reference_basis()` 设置 `exporter_absent_rules = false`，在写入数据库、展开 buff 之前去掉 absent；differs 保留。游戏口径、Web、CLI `output` 与 `--usage` 保留全部规则，CLI `reference.exporter_absent_rules_omitted` 标明比较时的排除。

`mise run accuracy` 与 triage 的 “beyond PoB2” 按规则列出出现的构筑数、改变的构筑数与行数：固定其余参考模型，每次只恢复一条 absent 规则，按数值变化计数（不是 5% 吻合阈值）；多规则影响同一行时不能相加。differs 在两种口径都开启，列出但模式变化为零。holdout 只输出计数，不输出构筑与行的身份。这些数值用于观察实现影响，正确性仍由游戏数据的回归测试保证。

Generic player slows already represented by negative `ActionSpeed` modifiers now use one calculation for skill rates, warcry timing, proxy placement, channel timing and movement (`calc::skill_use_time::action_speed_factor`). The [game Slow keyword](https://poe2db.tw/us/Keywords) defines separate slows as multiplicative; each penalty is scaled by `SlowEffectOnSelf` (`base_slow_potency_+%`, for example Mail Coat’s 30% reduction), and `UnaffectedBySlows` (`slows_have_no_potency_on_you`, Wanderlust / Relentless Pursuit) removes those penalties while preserving positive speed bonuses. A 50% slow with 30% reduced potency is a 35% slow. The potency text rules are `exporter: "absent"`; reference mode omits them. Existing custom modifiers can supply an explicit action-speed slow; self Temporal Chains and movement-only debuffs remain unmodelled, and ordinary equipment / skill movement penalties are unchanged. `SkillSpeed`, `WarcrySpeed` and `TotemPlacementSpeed` already have separate consumers: a partial ledger row on a build without a warcry or totem is expected, not a missing speed bonus. Every negative `ActionSpeed` modifier counts as a slow. That holds because the only player sources today are debuffs: self Chill, Temporal Chains and Maddening Presence. The game defines Slows as "modifiers from Debuffs" and keeps non-debuff reductions under separate stats (`action_speed_-%`, `overencumbered_action_speed_+%_final`). A new non-debuff source of reduced Action Speed must therefore use a separate name, or slowing potency would scale it.

Outgoing “Debuffs you inflict have N% increased Slow Magnitude” is
`base_debuff_slow_magnitude_+%` (active game `Mods` entry
`PassageUniqueAmanamuIncreasedDebuffSlowMagnitude`, range 12–20), distinct
from incoming `base_slow_potency_+%`. The game's
[Slow Magnitude Modifier](https://poe2db.tw/us/Slow_Magnitude_Modifier)
applies only to the slowing component of an inflicted debuff. The rule
currently feeds `EnemyChillMagnitude`, reusing the existing Chill magnitude
sum, trace and derived details: 30% Chill with 20% increased Chill magnitude
and 20% increased Slow magnitude becomes 42%, capped at the game's default
50% maximum ([Chill keyword](https://poe2db.tw/us/Chill)).
The additive Chill/Slow aggregation is an inference from both increases
modifying the same outgoing slowing magnitude, supported by the official
[0.2.0 patch notes](https://www.pathofexile.com/forum/view-thread/3740562)
explicitly making Curse Magnitude and Slow Magnitude additive for Temporal
Chains. Neither that example nor the retained `Stats` rows explicitly proves
the Chill/Slow pair; this pairing has not been validated by an in-game
measurement. Enemy slowing potency then scales that capped reduction
separately (50% less gives a 21% slow). This applies to the hit magnitude
estimate and explicit player
`ChillBase` / `ChillMinimum` sources. Enemy `ChillVal` and player
`ChillOverride` already specify final magnitudes and remain unchanged.
The line supplies no source, condition or uptime; cold damage alone still
does not write an enemy action-speed penalty. It changes neither incoming
slows, Shock/Freeze magnitude, nor debuff duration/expiration. Inactive
sources leave enemy modifiers and conditions untouched. The rule is
exporter-absent and omitted on the reference basis. **Remaining scope:**
[Temporal Chains](https://poe2db.tw/us/Temporal_Chains) has separate slow
and effect-expiration components; its enemy slow payload is not yet
consumed, and Hinder/Maim movement reductions have no enemy movement model.
These need their own consumers before this rule can cover their slowing
components. No in-game timing or uptime measurement is assumed.

Player Shock and explicitly configured Chill magnitudes are scaled by
`SelfShockEffect` / `SelfChillEffect` (`calc::player_debuffs`), using game stats
`shocked_effect_on_self_+%` and `chill_effectiveness_on_self_+%` from
`base/stats.json`; the passive tree includes Stormwalker's 40% Shock reduction
and 40% Chill reduction nodes. Shock scales only the configured damage-taken
contribution (the existing imported config supplies 15%, so 40% reduction gives
9%); the new Shock reduction rule is exporter-absent. Chill requires both the
player's Chilled state and an explicit `conditionChilledEffect` / `ChillVal`:
50% Chill with 40% reduced effect is a 30% action-speed slow, then subject to
slowing potency. No incoming hit or default Chill magnitude is inferred.
Effect reductions cannot make a debuff beneficial. `CurseEffectOnSelf`, the six
`Self<Ailment>Duration` fields, and `CorruptedBloodImmunity`, `MaimImmunity` and
`HinderImmunity` display player-side modifier views. Curse effect and durations
are percentages of base: `(100 + INC) * MORE` (curse effect is floored at zero).
Durations combine the ailment-specific and all-ailment modifiers, plus
elemental-ailment modifiers for Ignite, Chill, Freeze and Shock. Cool Headed,
Warm Blooded and Strong Hearted supply their game-data 50% less duration through
the same views.
Immunity views reuse the existing `*Immune` modifiers, including “cannot be
inflicted” text. `DebuffExpirationModifier` is `10000 / (100 + expiration rate)`
percent (game `debuff_time_passed_+%`); each duration view is multiplied by
`DebuffExpirationModifier / 100`. `ExposureEffectOnSelf`, `WitherEffectOnSelf`,
`SelfBleedEffect` and `SelfIgniteEffect` likewise expose nonnegative
`(100 + INC) * MORE` views from
`exposure_effect_on_you_+%`, `withered_effect_on_self_+%`,
`base_bleeding_magnitude_+%_on_self` and `ignite_effect_on_self_+%`. The other
views use `curse_effect_on_self_+%`, `self_bleed_duration_+%`,
`base_all_ailment_duration_on_self_+%` and the three
`immune_to_*` stats. Incoming curses, ailments, Corrupted Blood, Maim and Hinder
remain unmodelled, so these views change no other output. Enemy curses read the
enemy database separately. Generic `PhysicalDegen` has neither ailment identity
nor duration, so duration and immunity lines cannot alter it.

Spell Mana Cost Efficiency（`base_spell_mana_cost_efficiency_+%`）只降低法术魔力消耗，效率 INC 与通用效率相加后，以 `1 + 效率 / 100` 除消耗；它标为 absent。The Fall of the Axe 的 “Grants Onslaught during effect”（`charm_gain_onslaught_during_effect`）同样标为 absent；按 [游戏 Onslaught 定义](https://poe2db.tw/us/Onslaught) 增加 20% Skill Speed 与 10% Movement Speed。沿用 flask/charm 约定：构筑中启用且可生效的护符视为效果激活；“Used when you are affected by a Slow” 不建模为 uptime，另一件生效的物品不会激活被关闭或超出槽位的护符。

Charm/flask “Used when …” triggers (game `local_flask_use_on_*` and
`local_charm_trigger_when_cursed`) are recognized with no numeric effect under
the enabled-item convention. “Recover # Mana when Used”
(`charm_recover_X_mana_when_used`, game `Mods.CharmGainManaOnUse1..8`) is the
local per-use `Charm<N>ManaRecoveryOnUse`, independent of regeneration and
quality. “Mana Recovery from Flasks can Overflow maximum Mana during Effect”
(`local_unique_flask_mana_flask_recovery_can_overcap_mana_during_flask_effect`)
changes no output: recovery amounts are uncapped and current mana is not
simulated. “#% Chance to gain a Charge when you kill an enemy”
(`local_%_chance_to_gain_flask_charge_on_kill`) is the local
`FlaskChargeOnKillChance`: one charge per kill at that chance, so the slot's
`*ChargeGen` adds kills per second × chance / 100, times its charges-gained
factor. The kill rate is the PoBR config input `enemyKillsPerSecond`
(`Multiplier:EnemyKillsPerSecond`, merged with the Energy inputs by
`append_energy_options`); left empty, the line is inactive with that config
gate and no kill rate is assumed. The rule is exporter-absent. Lavianga's “This Flask cannot be Used but
applies its Effect constantly” (`local_flask_always_drinking`) reports zero
available uses while keeping its recovery rate. The flask line “N% of Damage taken
during effect Recouped as Life” (`local_flask_damage_taken_goes_to_life_over_4_seconds_%_during_effect`,
for example Blood of the Warrior) is `LifeRecoup` under `UsingFlask`, so it follows
the enabled-flask convention and shares the Life recoup duration (8 seconds by
default; the stat id's `over_4_seconds` is legacy naming, and the line states no
duration). It is not flat Damage Taken: the generic `N% of` rule used to read it
that way, adding 29 (Abyssal Lich Twister `4028d0b52984`) or 21 (Martial Artist Ice
Shot `71a879f43f2f`) to every damage type, and the pinned PoB2 drops the line
because its mid-line form leaves an unparsed remainder. The rule is
`exporter: "absent"`.

Three flask/charm lines that the generic phrase table read in part now have rules
(`special_mods.json`, all `exporter: "absent"`). “N% more Recovery if used while on Low
Mana / Low Life” (game stats `local_flask_amount_to_recover_+%_when_on_low_mana` /
`_on_low_life`, mods `FlaskIncreasedRecoveryOnLowMana` / `OnLowLife`) is a local
`FlaskRecovery` MORE tagged with the existing `LowMana` / `LowLife` condition: the
ledger reports it `inactive` until that condition holds (config `conditionLowMana` /
`conditionLowLife`; `LowLife` is also derived from Life reservation), then the flask's
recovery is multiplied by `1 + N / 100`. The generic rule had made it a
`FlaskLifeRecoveryLowLife` bonus that nothing read, even for the Low Mana wording. No
use timing or uptime of the Low pool is inferred, and there is no in-game test of the
stacking with other flask recovery modifiers. “Removes N% of Life Recovered from Mana
when used” and its Mana-from-Life twin (game stats
`local_flask_removes_%_of_life_recovery_from_mana_on_use` /
`..._mana_recovery_from_life_on_use`; rules `flask_removes_life_recovery_from_mana_on_use` /
`flask_removes_mana_recovery_from_life_on_use`, local utility properties
`UtilityLifeRecoveryRemovedFromMana` / `UtilityManaRecoveryRemovedFromLife`) make the
other resource pay a share of the flask's recovery each use. Each Flask slot reports
`Flask<N>ManaRemovedOnUse` (on a Life flask) or `Flask<N>LifeRemovedOnUse` (on a Mana
flask) as N% of that flask's total `Flask<N>Recovery` (instant plus gradual, after the
flask's own recovery modifiers such as the Low Life MORE); the opposite direction and
every flask without the line report 0, and the flask's own recovery is not reduced (the
generic rule used to make it a −N `FlaskRecovery`). Charms do not have these fields.
The derived detail is `UtilityRecovery * UtilityRemovedPercent / 100`
(`calc::utility`); it is a per-use amount, so no use frequency, current-pool state or net
resource model is assumed, and nothing is subtracted from `Net*Regen`. The Forsaken Bangle
charm line “N% of Chaos damage you prevent when Hit Recouped as Life and Mana during
effect” (game stat
`charm_x%_of_chaos_damage_from_hits_prevented_recouped_as_life_and_mana_during_effect`,
rule `charm_chaos_damage_prevented_recouped_as_life_and_mana`) stores
`ChaosDamagePreventedLifeRecoup` and `ChaosDamagePreventedManaRecoup`, each N, under the
enabled-charm convention above (the charm must be enabled and within the charm limit; no
uptime is inferred). The Chaos damage a hit “prevents” follows the physical definition: Chaos
damage entering the Chaos mitigation stage after taken-as shifts, minus what leaves it
after Chaos resistance and Chaos damage reduction, never below zero (negative resistance
prevents nothing). Flat damage taken and later damage-taken multipliers do not count, and
blocked and deflected hits contribute nothing; evaded hits never enter the landed-hit
loop. The game text does not call resistance “prevention”, so this is an assumption with
**no in-game test**. The mitigated EHP loop accumulates the per-type prevented damage
(`EhpTerms::prevented`); `survivability::life_recoup_terms` adds Chaos to the Life rate and
`survivability::mana_recoup_terms` returns the same amount as Mana (`ManaRecoupRate`,
with its own derived detail), each over its own recoup duration (8 seconds, 4 seconds
with `4SecondManaRecoup` / `4SecondRecoup` for Mana) and recovery rate
(`ManaRecoveryRate`). Ordinary `ManaRecoup` shares the Mana timing. Physical prevented
recoup still returns Life only. All three rules are batch `S2` and `exporter: "absent"`.

A line the parser reads only in part is not applied. A parse that returns `Parsed`
with a non-empty `unparsed` remainder (for example “2% more Mana Flask Recovery”,
which is read as 2% more maximum Mana, “1% more Life Flask Recovery” as 1% more
maximum Life, and “1% increased speed of Recoup Effects” as a `LocalEffect` increase)
used to be applied by ingest as a wrong modifier. Every ingest path now
goes through `ParseOutcome::into_applicable`: item sections, flasks and charms,
passive nodes, gem and support-gem stat texts, free `extra_modifier_texts`, jewel
texts and local requirement lines apply a line only when `ParseOutcome::is_complete`
holds; otherwise the original line is kept in `unsupported_modifier_texts` and
the usage ledger reports it as `unparsed`, exactly like a line no rule matched. A
remainder that is only display residue is fixed in the parser rule, not tolerated at
ingest; the benchmark and tuned groups had none. Silently dropped pre-flag tags
(`dropped_pre_flag_tags`, audit class `Partial`) are a separate, unchanged diagnostic.

Focused Thrust (`projectile_damage_+%_with_spears_while_there_no_enemies_surrounding_you`, passive 9227) grants 40% increased Damage to Projectile hits with Spears when `enemyDistance` is at least 30 (3 m; ten units per metre), remains inactive with an `enemyDistance` config gate when unset, and assumes the configured target is the only enemy; this single-target convention has no in-game test, and the rule is `exporter: "absent"`.

“#% increased Damage per Curse on you” (`damage_+%_per_active_curse_on_self`)
scales by the PoBR config input `multiplierCursesOnSelf`
(`Multiplier:CurseOnSelf`); with no count the line is inactive with that config
gate. A reader blocked by a numeric `MultiplierThreshold` also reports a
config gate when the catalog declares its multiplier setter and setting that
one boundary satisfies every other requirement. Cadiro's unspecified arrow
therefore remains inactive/config-gated, rather than an implementation gap.
Unknown setters, unrelated skill/condition gates, conflicting thresholds,
non-finite inputs and stored zero/false keep their existing classification.
The calculation and its actual unmet reason are unchanged.
The ledger finds count gates from the modifier's zero-scaling
`Multiplier` tags and the config catalog. Static gate labels are borrowed;
dynamic labels and config ids are owned, with the same JSON string fields.
Plain and removable Power/Frenzy/Endurance charge counts share the calculation's
input mapping, so their zero-scaling ledger gates point to the catalog's matching
`use<Kind>Charges` option (with `override<Kind>Charges` setting the enabled count).

Rite of Passage's “Possessed by Spirit Of The Bear / Boar / Cat / Owl / Ox /
Primate / Serpent / Stag / Wolf for # seconds on use” uses game
`base/mods.json`: `SpiritOfThe*PossessedPlayer` (Monkey for Primate, Snake for
Serpent) and `SpiritPossessedStr/Dex/IntPlayer`. Separate fixed buffs grant
20% reduced damage taken (Strength), 30% skill speed / 15% movement speed
(Dexterity), or 80% damage (Intelligence), plus the spirit's own stats
(`calc::charm_spirits`). These buffs do not scale with charm effect/quality;
shared family bonuses apply once. Game Mods values take priority over the
[Wolf tooltip](https://poe2db.tw/us/Spirit_Of_The_Wolf#SpiritOfTheWolfPossessedPlayerRef),
which still says 10% skill speed / 5% movement speed. All new rules declare
`exporter: "absent"` and a game source. Periodic spirit attacks, Wolf's
maim/on-hit armour break remain outside reported calculations. Ox's slow
potency is
read when a slow applies, and its threshold bonus feeds `AilmentThreshold`. “Energy Shield Recharge starts on use” remains unsupported pending an event model;
Ignited and Consecrated Ground use the explicit snapshots below.

Olroth's Resolve “Gain Guard equal to Current Runic Ward for 10 seconds when
Effect ends” (`local_unique_flask_gain_%_of_current_ward_as_guard_when_flask_effect_ends`,
[game text](https://poe2db.tw/us/Olroths_Resolve)) retains local event metadata
for equipped flasks, including inactive slots. Config `olrothGuard` supplies
Guard still present from an earlier use, after any intervening damage, within
the ten-second lifetime; unset/zero grants none. This is an explicit snapshot,
not an estimate from current/maximum Ward or flask uptime. Reusing the flask
does not by itself remove existing Guard. The strongest Guard grant wins
(including charm Guard), capped at 200% maximum Life, and feeds EHP/max-hit
pools without changing Ward. Flask effect, quality and flask duration do not
scale the end-event metadata. `Flask<N>GuardAfterEffect` / `GuardDuration` show
the input capped for that slot and the nominal lifetime, with reconstructible
details and no invented PoB reference keys. The rule is exporter-absent; there
is no in-game timing test.

Beira's Anguish (official TW 貝伊拉的悲痛) creates Ignited Ground (點燃地面)
for 4 seconds, igniting as though dealing Fire damage equal to 500% maximum
Life (`UniqueCharmIgniteEnemiesInPresence1`, current game Mods). Current
[game Keywords](https://poe2db.tw/us/Keywords) give a default ground radius of
2 metres and require non-hit ground to build Flammability (易燃) to 50% before
Igniting; the [current item text](https://poe2db.tw/us/Beiras_Anguish) replaced
its pre-0.3 immediate nearby Ignite. Per-charm `IgnitedGroundDuration`,
`IgnitedGroundRadiusMetres`, `IgnitedGroundSourceDamage`,
`IgnitedGroundIgniteDPS` and `IgnitedGroundIgniteDuration` report potential
from currently equipped sources, even inactive charms. Current maximum Life
is the creation snapshot, including reserved Life. This is independent of the
selected skill's hit, crit, attack/spell modifiers and local supports. Shared
global Ignite magnitude, rate, duration and enemy resistance/taken apply;
hit-only penetration does not. Charm quality/effect/buff duration do not scale
these on-use event properties; no in-game timing test has been performed.

`conditionBeiraIgniteActive` explicitly declares an already established,
currently active ground Ignite in combat; merely equipping/enabling the charm
never grants DPS. `BeiraIgniteDPS` is the strongest such charm source.
`CombinedIgniteDPS` and total DoT use the strongest of that and the selected
skill's ordinary Ignite, never their sum. Optional Full DPS includes that charm
source once in the shared Ignite limit, independent of included skill counts.
When an included/selected skill supports
multiple simultaneous Ignites, cross-source stacking is unverified: the Beira
value remains separate and is not added to total damage. No use frequency,
buildup time, location, expiry or uptime is inferred. The item rule is
exporter-absent; reference basis excludes the new source. Calcs labels and
configuration help are application copy using the verified official TW terms;
CN copy is contextual, using the pinned `i18n/zh-CN/words.json` item name
`贝拉的苦楚` and the matching `stat_lines.json` term
燃烧地面; these dictionaries combine trade and PoE2DB sources and are not
claimed to be a retained official CN KeywordPopups table. Headhunter remains
unchanged at the owner's request.

Cat possession's game `critical_strike_chance_+% = 100` is an INC modifier to
`CriticalStrikeChance` (5% base becomes 10%); all possession names have reader
coverage, including Bear's `hit_damage_stun_multiplier_+% = 60` in enemy Heavy
Stun buildup. Charm “Recover # Life when Used”
(`charm_recover_X_life_when_used`, `CharmGainLifeOnUse1..8`) reports local
`Charm<N>LifeRecoveryOnUse`, independent of quality and regeneration, matching
the Mana sibling. “Also grants # Guard” (`charm_gain_X_guard_for_duration`,
`CharmGuardWhileActive1..6`) uses the existing hit-absorption pool for enabled,
in-budget charms: charm effect scales the grant; the strongest grant wins,
absorbs 100% of hit damage before Life/ES, and is capped at 200% maximum Life
([game Guard definition](https://poe2db.tw/us/Guard)); no reuse or duration
uptime is inferred. Both charm rules are exporter-absent. `BleedFaster`,
`IgniteFaster` and `PoisonFaster` retain their existing DPS/duration calculation
and the ledger identifies the selected skill's ability to inflict that ailment
as their read gate. Attack Blind chance (`attacks_chance_to_blind_on_hit_%`)
feeds `BlindUptime`; Blind's effects still use the configured enemy state.
“Your Totem Limit is doubled”
(`keystone_ancestral_bond`) is an `ActiveTotemLimit` MORE 100 (exporter-absent);

“Corrupting will always result in change”
(`soul_core_cannot_roll_no_outcome_with_corruption`) is crafting-only `S0`.

`crates/pobr-build/tests/` 下的 `codec.rs`、`skills.rs`、`parity.rs`、`dualrun.rs` 是 Cargo 集成测试目标，其子目录文件是用例模块；`--test` 使用汇总目标名。`pobr-core` 同样按 aggregation / parser / sources / engine / offence / defence / ailments / golden 汇总。

PoB2 对照值与逻辑回归都使用活动数据版本，没有单独钉住的对照版本；代码和测试不写具体版本号。依赖本地 vendor 的对照可能跳过，需如实记录，不能将跳过视为公式已获验证。

`vendor/PathOfBuilding-PoE2/` 是 [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的完整检出（版本见 `vendor/.pob2-version.txt`；gitignore 不入库，`bash devs/scripts/driver.sh vendor` 可按钉定 commit 重新克隆），公式核对直接读本地 Lua（`CalcOffence.lua`/`CalcDefence.lua`/`CalcPerform.lua` 等），不要去网上找。

## 排查数值差异（诊断闭环）

### 静态模拟的范围与取舍

PoBR 主要计算静态构筑及选定条件下的结果，不追求完整还原游戏中的战斗时间线。词条在游戏中有作用，不代表模拟器需要实现它。**实现或派发任何词条功能前，先评估它是否符合需求**：在目标角色、技能、条件和模拟假设下，它会影响哪个目标输出，解决什么实际错误或用户需求，以及这些收益是否值得增加实现与维护复杂度。无法说明实际用途时，不应仅因为词条尚未支持就立项。优先修复影响目标结果的错误，沿用现有计算路径做最小修改。

- 判断依据是本次计算目标及其假设，不是固定的词条名称或触发类型名单。同一词条在不同模拟目标或条件下可以有不同取舍。能够在选定条件下影响目标结果的效果按该条件计算；不适用的保持不生效，不为证明实现有用而虚构事件频率、触发历史或覆盖率。
- 缺口队列是诊断盘点，不是必须逐项清零的开发待办。出现次数多、尚未解析或没有读取方，本身不足以立项；不为降低缺口数量新增模型、配置项、展示字段或通用抽象，也不做没有实际需求或测量依据的性能优化。
- 保留原始词条和准确的诊断，范围内但尚未实现的机制可以暂缓。本次不模拟某种事件，不等于该词条在所有计算中无效果；不能据此标成 `no_effect`。只有确认不影响任何受支持输出的内容才沿用已有的 `no_effect` / S0 约定。

猎手偷取怪物词缀、移动回蓝、拾取魔力残片回魔只是说明静态计算中通常无需追踪的事件收益，例子并不穷尽，也不是固定排除名单。其他词条同样先评估需求；具体取舍由目标计算决定。

### 输入去向与诊断

`AdditionalRingSlot` 和 `GemlingQuality` 由现有构筑预处理读取天赋文本，分别决定第三戒指栏与宝石附加品质效果，不依赖 ModDb 查询。诊断只在来源为对应的无条件天赋行、且实际预处理判定启用时将其列为 `outside`；自定义词条或其他来源的同名 flag 不能据此消除未读取提示。

一个输入可能在四处丢失，引擎必须把每一处都记下来，而不是让数值悄悄偏小：

| 环节 | 丢失方式 | 记录位置 |
|------|----------|----------|
| 导入（XML → `Build`） | 物品块被拒、物品所在装备槽没有模型 | `ImportReport::skipped_items` / `unmodelled_slots` |
| 文本 → Modifier | 没有规则能解析的词条行（物品、珠宝、已分配天赋 `Tree:<节点>: …`、配置行 `Config: …`）；只读懂一部分、留有未解析余文的行（`Parsed` 但 `unparsed` 非空）同样不应用，按原文记入 | `unsupported_modifier_texts`（台账 `unparsed`） |
| 宝石 stat → Modifier | stat map 有条目但引擎拒绝（未知 mod 名、不支持的 tag）；完全没有条目的 stat 多数是展示或动画数据，按所属宝石列出供查找缺失的机制 | `Diagnosis::unmapped_stats` / `unknown_stats` |
| Modifier → 计算 | 存入的名字从未被查询；依赖的 `Multiplier` 没有任何地方赋值（含从角色快照读取的 `actor: player` 类，例如 “per non-Idol Augment in your Equipment”）；等待的条件不在条件表里，也没有计算阶段推导它 | `Diagnosis::unread_modifiers` / `unprovided_multipliers` / `unset_conditions` |

`pobr_build::diagnose` 运行同一条计算管线并返回以上全部，外加实际计算的主技能及其等级（技能或等级选错不会丢输入，所以单独列出）。它打开 `ModDb::track_reads` 与 `CalcConfig::track_reads` 并以 Compare 模式记录 stat 映射，结果与普通计算逐位相同（`diagnosis_does_not_change_the_result` 对全部已入库构筑断言），但更慢，只用于排查。

逐行台账 `pobr_build::usage::line_usage`（CLI `calculate-build --usage [--json]`、WASM `line_usage_json`）在同一次诊断上回答“构筑的每一行词条用上了没有、用在哪里”：计算收到的每一行（物品各区段、肢体、药剂与护符、珠宝、天赋节点、任务奖励与自定义词条、配置项，以及宝石 stat）列一次，带来源 id、持有者名字（节点名、物品基底）、解析出的每个 modifier 和它的去向：`applied`（被计数；`used_in` 列出它进入的 stat）、`partial`、`inactive`（名字被读取但没被计数，或已验证的读取方只在本次未到达的状态下运行；`reason` 给出缺的条件或标志，例如 “needs Attack”；后者另带 `read_gate: { state, config_option }`，没有配置入口时 `config_option` 为 null；台账的 `condition_configs` 另列被引用条件的配置入口，包含已设置为 false 或由计算阶段推导而不在 unset 列表里的条件）、`unread`（没有任何查询问过这个名字）、`outside`（在 ModDb 之外消费：物品自身的防御 / 武器伤害、宝石等级与品质、珠宝半径，`reason` 写明读取方；宝石等级与品质行还写明提升了哪些技能，一个都没提升时为 `unread`）、`no_effect`（登记为不影响任何输出的 S0 条目，或本构筑无人询问的配置项）、`unmodelled`（有规则识别但不产生 modifier，机制未建模）、`unparsed`、`no_modifier`（解析出 modifier 却没有存入，台账应当学会说明原因）。给召唤物的词条（`MinionModifier`）按召唤物自己的计算是否读取了它给出的名字判断，构筑没有召唤物时为 `unread`。`Condition:<X>` / `Multiplier:<X>` 类 modifier 经条件表消费：有 modifier 等待 `<X>`、或计算阶段问过 `<X>`（`CalcConfig::read_variables`）时算作用上。行的去向由编排在写入来源时记录（`SourceWriter::note_line`，`LineFate`）；在 ModDb 之外拿走或丢弃词条行的新代码（剥离本地词条、按文本读取）要同时调用 `note_line` 并写明读取方，否则台账会把这些行报成未解析或没有 modifier。`mise run triage` 带 `--usage` 运行每个构筑，把未建模的行、没有任何构筑读取的名字、无人设置的条件和被拒绝的宝石 stat 跨构筑汇总，不论该构筑有没有不一致的行。

条件不在条件表里时，有两类不是线索：计算阶段自己推导、成立时才写入的（暴击的一半、主副手、怒气与战斗、敌人的冰缓与感电、诅咒与标记、Elusive、Surrounded、所用武器组、内置 buff 附带的条件、`AffectedBy*`、`Using*Flask` / `Using*Charm`，见 `calc/session.rs::CONDITIONS_SET_BY_A_PASS`），以及编排按构筑本身设置、换一个构筑才会成立的（武器类型、盾、主技能的战斗条件、首领、同伴、普通胸甲、辅助宝石颜色，见 `diagnosis.rs::SET_BY_THE_ORCHESTRATION`）。其余条件中有配置项能设置的标为 “config: off”，由用户决定是否打开；没有任何来源的才是丢掉的输入。新增由计算阶段或编排设置的条件时登记进对应清单，否则其他构筑会误报它。

排查顺序（先批量、后单个；完整流程见 [AGENTS.md](../AGENTS.md#accuracy-workflow)）：

1. `mise run triage` 一次计算基准组与 tuned 组的全部构筑和钉定的 PoB2（也可给组目录或构筑目录），把与构筑码参考值不一致的每一行按今天的 PoB2 分类：`engine`（PoB2 与参考值一致而 PoBR 不一致）、`both`（三方都不同）、`reference`（PoBR 与 PoB2 一致，参考值出自另一版导出工具，不用改）、`known`（`devs/scripts/known-differences.json` 已解释的有意差异；PoB2 额外保留护身符授予技能的精魄也自动归入）。随后把 engine / both 行的线索跨构筑汇总：按来源与类型对齐后只有一边有或数值不同的词条、PoBR 有却没生效的词条、伤害因子与敌人的抗性 / 穿透 / 承伤、最大承伤与 EHP 背后 PoB2 的资源池、精魄总量与逐技能保留，以及没有规则能解析的词条（按模板合并，按涉及的构筑数排序）；再从每个构筑的逐行台账汇总计算丢掉的输入：有规则识别却未建模的行、存入后没有任何构筑读取的名字（先列没有读取方的）、无人设置的条件、被拒绝的宝石 stat。EHP 与最大承伤的行另列两边逐类型的敌人伤害、承受的部分、资源池和命中次数中不同的项，以及 PoB2 一侧敌人获得的额外伤害与它的来源（oracle 的 `enemyMitigation.gainMods`）。每个构筑另列本构筑存入却没有读取、名字与不一致的行同词的词条（例如 `EffectiveBlockChance` 旁的 `ReplaceShieldBlock`）：只有少数构筑有的名字排不进跨构筑的清单。每个构筑另列两边按不同等级或品质计算的技能（PoBR 取自 `--usage` 账本的 `skills`，PoB2 取自 oracle 的 `skillLevels`）：技能等级改变 buff、光环、诅咒的数值，却不丢任何输入，其他线索都指不出它。按涉及构筑最多的原因成批修复，整批修完再跑一次（PoB2 一侧按构筑码缓存，与 `compare` 共用）。holdout 组只用 `mise run accuracy` 测量，`triage` 拒绝它。
2. 分诊解释不了的行，再单独看那个构筑：`mise run diagnose <构筑>`。第 1 节是导入损耗，第 2 节是与构筑码自带参考值的差异，第 3–9 节是丢掉的输入（第 6 节是等待无人设置的条件的词条，配置项能设置的条件另列为 “config: off”）。多数差异在这里就能对上原因。
3. 看一个 stat 怎么算出来：`cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown <Stat,...>` 打印计算步骤和读到的每条词条（`--reference-basis` 按参考值口径，`--json` 输出 JSON；加 `--usage` 列出没有完全用上的每一行及原因）；技能、stat、基底、天赋或暗金在游戏数据里的记录用 `mise run lookup <id 或名字>`，不必逐个打开数据文件。
4. 清单解释不了的差异，用 `mise run compare <构筑> [Stat ...]` 对照钉定的 PoB2：每个 stat 两边的总值与比值、BASE / INC / MORE 合计，以及按来源（天赋节点、装备槽、技能、角色基础与属性、配置、任务奖励）对齐后只有一边有或数值不同的词条。不给 stat 时比较构筑码参考值中不一致的行。`ActionRate` 在 PoB2 有 `HitSpeed` 时与它比较（命中频率，不是施放速率），没有时才用 `Speed`；召唤物的总值以 `Minion:<stat>` 一并列出。PoB2 的结果按构筑码缓存在 `.cache/pob2-oracle/`，重跑只重算 PoBR（`--refresh` 两边都重算）。更细的中间值仍可直接用 `tools/pob2-oracle/run.sh`。
5. 参考值不是真值。构筑码内的 `<PlayerStat>` 是导出工具当时的结果（版本未知，可能早于当前数据）；PoB2 自身也有假设和缺陷。两边不一致时，以游戏自己的词条文本和数据判断谁对，再决定改哪一边。导出工具缺失的词条用 `exporter: "absent"` 与游戏来源登记，让参考口径自动排除，不降低基线、不按构筑登记 known differences；导出工具口径不同的规则用 `"differs"` 保留并报告。其余确认为有意差异的行才写进上文与 `devs/scripts/known-differences.json`。

同一类行反复需要逐个构筑人工排查时，说明分诊或诊断的输出缺了东西：扩充工具，让下一轮批量运行直接给出这类线索。

找下一批机制缺口时用 `mise run gaps`：把当前数据的 `generated/modifier-audit.json` 与 benchmark、tuned 的 `--usage` 台账按数字替换为 `#` 的模板合并，先按丢掉输入的构筑数，再按 unique、passive、mods、compound、jewel_node、bases 的来源优先级及来源数排序。纯 translation 来源不入队；`pobr_gap` 是 PoB2 能解析而 PoBR 未完整处理的线，`upstream_gap` 是两边都不完整，`recognized_empty` 是识别却未产出 modifier，`no_effect` 是已登记不影响输出的条目（供审查，不代表待实现）；队列只收录有审计来源的游戏文本；研究卡也可查询只见于构筑台账的文本，此时标为 `ledger_gap`。队列与 triage 共用全部 worked 构筑的读取并集：`dropped` 只计未解析、未建模、解析后未存入、没有任何 worked 构筑读取的名字，以及没有已登记状态、配置入口或技能范围要求解释的 inactive；名字在别的 worked 构筑读过则归 `unused`（本构筑用不上），有配置入口的 inactive 归 `config-gated`，已登记状态的 inactive 归 `state-gated`（包括 `SET_BY_THE_ORCHESTRATION` 与 `CONDITIONS_SET_BY_A_PASS` 中由构筑或计算阶段决定的条件，如武器类型、辅助宝石颜色与严格多数，以及整个 `AffectedBy*` buff／光环条件族；台账用 `condition_states` 列出被引用的这些条件，研究卡解释为 `condition set from the build: <name>`；这些条件即使有配置入口也归 `state-gated`），所选技能不满足的 flag、keyword 或 SkillType 要求归 `skill-mismatch`，条件与阈值不归入此桶；命中词条在技能确实产生命中时复用带 `Hit` 的命中上下文解释，所以 Thin Ice 报未满足的 `EnemyFrozen` 配置条件，而不是缺少 `Hit`。其余已处理的输入归 `handled`。标题打印各桶的模板／构筑对数，同一行同时有缺口与门控时会进入多个桶；排序只使用 `dropped` 的构筑数。默认隐藏只有门控、技能不匹配、未使用或已处理输入的模板，未在构筑出现的审计缺口仍以零构筑数保留；`--all` 同时显示这些行及每桶计数。`--kind` 按来源过滤，`--limit` 只限制文本行数，`--json` 保存所选队列的全部模板、变体、审计字段与构筑台账。命令默认构建一次 CLI，不运行 PoB2；可用 `--binary <pobr>` 显式复用已构建程序。台账与 `triage` 共用 `.cache/build-usage/`，按构筑码、程序内容、收集脚本及数据文件路径/大小/修改时间失效；`--refresh` 强制重算。两条命令只读 worked 组，不读 holdout。

从队列选中一行后运行 `mise run gap "<模板或文本>"`，一次列出数字变体、从 `overlay/stat_descriptions.json` 反查的单 stat 或 compound 全部成员（逐项确认 `base/stats.json` 是否存在）、每个来源的完整词条文本（含暗金其他行与咒符触发隐式）、节点名、每个携带它的 worked 构筑及台账去向和桶（包含门控原因）、引擎名字和 special/mod_parser 近似规则。引擎与规则候选仅按关键词匹配，不证明支持；暗金与节点的 poe2db 链接标为猜测，未经联网验证。确认游戏含义时可再把卡片上的稳定 id 交给 `mise run lookup <id>`。

已入库构筑覆盖不到的技能用 `mise run skills:sweep`（`devs/scripts/skill-sweep.py`）：对活动数据里每颗非辅助宝石（含 meta）的所有非辅助主动效果，按 `GemEffects` 显示顺序枚举，保留来源宝石与主效果，以 `mainActiveSkill` / `mainActiveSkillCalcs` 选择效果；行标识为 `主效果/所选效果@序号`，主／附加效果分别计数，弹药装填不能代替射击覆盖。最小合成构筑使用 90 级 Witch、20 级零品质宝石、普通武器基底，同时计算 PoBR 与钉定 PoB2，按技能族汇总 `agree` / `known` / `differs` / `pobr_zero` / `pob2_zero` / `both_zero` / `error` / `pob2_other_skill`；纯指令与占位效果另列。扫描不读 holdout；两边实际效果均须核对，vendor 缺少效果或附加效果的来源宝石不算吻合。`both_zero` 不证明 buff、无载荷幻影、无事件触发或召唤物已实现；召唤物只看一个主召唤物，不代表整支召唤物队伍。`known` 只按所选 stat set 的平均伤害显示／无伤害 stat、本指南列明的技能模型或可重算解释 DPS 比值的伤害区间取整分类，保留两边数值与原因、从技能族的未解释差异排名中排除；平均伤害显示只豁免 `CombinedDPS`，其余字段仍须吻合，取整须在 0.1% 内解释 DPS。平均命中因减伤口径不同只展示、不分类；速率优先对照 PoB2 的 `HitSpeed`，并对照 CLI JSON 的 `firing_rate` / `reload_time`（`FiringRate` / `ReloadTime`）。每效果只测第一个 stat set，不穷举辅助、触发载荷、尸体、充能、目标与战斗条件，不混合轮转或过热。完整扫描写 `.cache/skill-sweep/report.json` / `report.txt`，`--skill` 定向扫描默认写 `report-selected.json` / `report-selected.txt`，显式 `--json` / `--text` 优先；保留完整轮次再做定向扫描。结果缓存在 `.cache/skill-sweep/`，oracle 缓存键含 XML、vendor commit 与 wrapper 内容。

侧栏字段的覆盖与数值吻合分开衡量：`python3 devs/scripts/output-coverage.py` 用已构建的 `target/debug/pobr`，按 `display_catalog` 的 PoB key 逐字段对照 oracle 的完整输出（默认 benchmark、tuned 与 demo 三组，拒绝 holdout），给出每个字段两边都有有限值的构筑数、非零样本数、5% / 10% 内的吻合数和最差的构筑；`--oracle-cache` 只读已有缓存（键与 `compare-build.py` 相同），`--refresh-missing` 补算缺少的。没有样本不算验证通过，大量零值吻合也不证明非零的机制正确。

“未被读取”是线索不是结论：攻击技能不读法术伤害是正常的。报告用全部已入库构筑的读取并集区分“任何构筑都不读（没有读取方）”和“本构筑的技能用不上”。

新增按名字消费 modifier 的代码用 `ModDb::named(name)` 或聚合查询，不要用 `iter_mods().filter(名字)`：后者不算读取，诊断会把有读取方的名字报成没有。确实在数据库之外消费的名字（按文本扫描、构筑结构）登记在 `diagnosis.rs::READ_OUTSIDE_THE_DATABASE` 并写明消费者。只在某状态下读取的名字登记在旁边的 `READ_UNDER_A_STATE`，写明实际读取方、状态及可设置它的配置 id；例如 slow potency 只在角色受减速且没有减速免疫时读取，Archon 共享加成等待 `conditionAffectedByArchon`。每个登记名字必须在 `tests/skills/line_usage.rs` 中用状态关闭／开启的构筑验证 `inactive` → `applied`，新增登记没有 fixture 时测试失败；不能用登记掩盖没有读取方的机制。

宝石等级与品质加成（“+1 to Level of all Spell Skills”“+2% to Quality of all Skills”）按文本读取，不经过 ModDb：技能等级要在任何 modifier 存入之前确定。读取面是角色的全部词条来源：装备与珠宝、已分配和被物品授予的天赋、配置行（任务奖励、自定义词条），以及调用方传入的 `extra_modifier_texts`（由 `prepared_build_view` 并入构筑视图，主面板、逐技能总览和触发子计算共用）。新增一条文本通道时要同时接入 `skill::resolve::gem_property_bonuses`，否则这类词条在该通道里既不生效也不报未支持。被这样读取的行不计入未支持清单，也不再交给解析规则（规则会把“with an Intelligence requirement”误读成属性需求）。种类有两种写法，“all Cold Skills”和“all Nova Skill Gems”，含义相同。种类按宝石标签判定（游戏表 `GemEffects.GemTags`，adapter 写进 `base/skill_gems.json`；每个标签对应一条 `<标签>_skill_gem_level_+` stat），种类里的每个词都要是宝石的标签：Rain of Arrows 带 `projectile` 标签，尽管它的技能不是 `Projectile` 类型；Shockwave Totem 带它的图腾所用 Slam 的 `slam` / `melee` / `nova` 标签。“Elemental”不是标签，指火、冰、电三种之一（`elemental_skill_gem_level_+`）。辅助宝石的标签描述它辅助的技能和它授予的技能，它本身没有等级，所以不按标签判定；没有标签的旧快照也退回按技能类型判定，种类名与技能类型不同的（Totem → `SummonsTotem`、Curse → `AppliesCurse`、Strike → `MeleeSingleTarget`、Crossbow、Elemental）列在 `skill_env::mods::kind_skill_types`（`skill_env::mods::gem_tags_match_category` / `gem_level_category_matches`）。“increased bonuses gained from Equipped Rings and Amulets”（Ritualist 的 Mystic Attunement）放大戒指和护身符上的其他词条，也放大其中的宝石等级与品质加成，向下取整：+4 在 25% 下为 5（`skill::resolve::scale_slot_gem_property_bonuses`）。按技能名写的等级词条（Cursecarver 的 “+4 to Level of Despair Skills”，不带 all）走同一通道，按游戏的 `ActiveSkills.DisplayedName`（`GrantedEffectDef.displayed_name`，不分大小写）匹配，Sniper's Mark 这类带标点、不能从 id 还原的名字也能匹配（`skill_env::mods::parse_gem_property_bonus`、`skill_env::resolve::gem_property_applies`）；只接受完整的 “Skills” / “Skill Gems” 写法，“Socketed”、“Supported” 和带其他条件的写法有各自的作用域，不当作全局等级加成。

物品授予的技能（“Grants Skill: Level 18 Purity of Ice”）有自己的等级和属性需求，与物品的穿戴需求无关；角色达不到时降到能满足的最高等级（[poe2wiki: Granted skill](https://www.poe2wiki.net/wiki/Granted_skill)），一级都达不到时用它最低的等级。每个宝石等级需要的角色等级取自游戏表 `ItemExperiencePerLevel`（`SkillGemDef::level_requirement`；表里没列的等级沿用前一级）；属性需求按宝石的属性占比计算，L 为该等级需要的角色等级：round((5 + (L − 3) × 1.7) × (占比 / 100)^0.9) + 4（`SkillGemDef::attribute_requirements`，与游戏显示的逐级数值一致，例如 Purity of Ice 18 级需要 42 力量、106 智力）。被授予的组在构筑视图里记下授予等级（`SocketGroup::granted_level`，物品自己的组和上面那个被认作授予技能的普通组），`prepared_build_view` 再按角色等级和属性降级（`granted_skills::scale_granted_skills`）。属性来自不含技能的一次来源写入（`attributes_without_skills`：职业、装备、肢体、珠宝、配置、天赋和额外词条），技能自己的效果不计入；只有构筑有需要属性的授予技能时才运行，计算量约为一次计算的来源写入阶段。没有等级的授予行（“Grants Skill: Raise Shield”）不降级。已分配天赋授予的技能在构筑码里是 `<Skill source="Tree:<节点>">`，等级只是占位；它按角色等级取宝石能达到的最高等级（宝石经验表列到的最后一级，例如 98 级角色的 Sorcery Ward 为 20），同样记下授予等级、再检查属性。游戏导出的角色还把它列成一个普通组（排在组首、等级相同、带辅助宝石），那个组就是被授予的技能，天赋的组让位给它，主技能组随之改指；节点未分配时天赋的组不授予任何技能（`granted_skills::resolve_tree_granted_skills`）。

辅助宝石辅助一颗宝石的任一效果时，它给的等级和品质作用于整颗宝石：Physical Mastery（要求 Physical）辅助 Iron Ward 的物理新星，Iron Ward 的 buff 也高一级。等级计算接收宝石每个效果的辅助判定（`skill_env::buff_specs::gem_judgements`，主效果在前），同一个辅助只算一次；Uhtred 系列“没有其他辅助时”的“其他辅助”也按整颗宝石计。

The usage ledger reports the selected `GrantedEffectsPerLevel` row, rather than
the requested gem level before row selection: Armour Explosion and Raise Shield
have only level 1, Pinnacle of Power only level 20, and ordinary player skills
end at level 40 in the current game tables. Their former higher ledger levels
did not mean the calculation extrapolated beyond those rows. Support-granted
effects inherit the first compatible host gem's level, checking all that gem's
effects ([Living Lightning's game text](https://poe2db.tw/us/Living_Lightning)
explicitly says it copies its socketing gem's level): Living Lightning can
support Repulsion Wave, and Shock Siphon can support Pounce's mark. Buff assembly
uses the same host. Named skill bonuses
also apply to the whole gem: “+3 to Level of all Galvanic Shards Skills” raises
both the attack and Load Galvanic Shards (`GemEffects` links the two effects).

Spear Throw's imported level-1 entries are default-attack placeholders. Its
base level follows the character (`ItemExperiencePerLevel`: level 19 at character
84, level 20 at 90), then receives global skill bonuses, per the game's
[Default Attack keyword](https://poe2db.tw/us/Keywords). Unbound crossbow, spear
and shapeshift groups use the other weapon set when only that set has the
required weapon class ([Rend requires Talismans](https://poe2db.tw/us/Rend)).
Granted-skill attribute checks likewise use the granting weapon set's equipment;
its Bonded modifiers are settled before attributes and level bonuses are read.
The prepared global level bonus stays separate from the base gem level, so a
level-3 Pounce plus five non-weapon levels stays level 8 after swapping away
from a +5 melee weapon; an intermediate base level must not clamp to 1.
The alternate set's passive allocation is not retained in the `Build` view, so
these checks still share the imported passive tree. Global quality applies to
[granted skills](https://www.poe2wiki.net/wiki/Granted_skill), including Raise
Shield even though it has no quality effect; the exporter's synthetic Thorns and
Enemy Explode entries have no game skill definition and receive no bonus quality.
The ledger lists all enabled imported groups. PoB2 consolidates some item grants
with their supported ordinary group and adds some default attacks automatically;
therefore extra lower Mirror of Refraction / The Stars Answer entries, another
weapon set's Spear Throw, or an extra PoB2 Spear Stab can remain inventory
differences even when the supported skill's effective level agrees. They do not
justify changing the imported gem or the game's level bonuses.

配置行指任务奖励与自定义词条。其中解析不了的行以 `Config: <行>` 记入未支持清单，其余行照常生效；任务奖励选项的 “None”（未领取）不是词条行。对任何计算输出都没有影响的词条在 `overlay-common/special_mods.json` 里登记为只识别、不产生 modifier 的条目（`"mods": []`、`"batch": "S0"`，例如游戏 stat `experience_gain_+%` 的 “5% increased Experience Gain”；除 `unmodelled` 外的其他批次必须产生 modifier 或挂 handler，`pobr-gamedata` 的 `non_s0_has_traceable_provenance` 检查），它不是丢失的输入；尚未实现的机制不能用这种条目消掉，必须留在未支持清单里。规则能识别、但机制尚未建模的行登记为 `"batch": "unmodelled"`（同样 `"mods": []`，`source_note` 写明缺的模型，例如 “per-hit Armour Break is not modelled”、“remnant count not modelled”、“kill rate not modelled”）：台账报 `unmodelled`，审计报 `recognized_empty`，缺口队列照常列出它，不会当作无效果隐藏。

`examples/demo-bd-test/builds/` 是回归门禁语料，构筑抓取于 2026-06（上一赛季开服第一周），不包含之后的物品和机制；门禁全绿只说明这批构筑没有回退。当前赛季的构筑分三组，都用构筑码自带的参考值衡量：`examples/wasm-benchmark/builds/`（8 个）和 `examples/reference-builds/tuned/`（每个升华三个：2026-10-13 抓的一批和之后的两批 holdout，都在 2026-10-15 并入）用来找差异、修引擎；`examples/reference-builds/holdout/`（每个升华另一个，2026-10-15 第二次抓取）只测量，不对它运行 `diagnose` 或 `compare`，也不按它的差异挑选要修的内容，它的总数回答“没人调过的构筑上有多准”。`mise run accuracy` 不带参数打印三组的总数。`reference_agreement_does_not_regress` 断言每组的吻合数（伤害吻合的构筑数——主技能召唤召唤物的构筑看召唤物的 `TotalDPS`——和吻合的行数）不低于 `reference_report.rs` 里记录的基线。改动提升了吻合数就上调基线，不为通过而下调；数据版本变化使数字移动时按 `ninja_parity` 的做法重录。导出工具未实现的词条规则标为 `exporter: "absent"`，由参考口径自动排除，游戏实现与参考基线分别验证；其他参考值被游戏数据证明有误时，在基线旁记录具体依据与影响。这个门禁防的是退步，不证明正确：参考值可能本身有误，构筑也可能因为错误的原因吻合。新增的展示字段让构筑码里更多 `<PlayerStat>` 行进入比较，行数随之变化不代表原有计算更准。需要新一轮目标或新赛季开始时，用 `mise run builds:fetch` 重新抓取（见 [reference-builds](../examples/reference-builds/README.md)）：原来的 holdout 并入 tuned，新抓的一批作 holdout。

## 游戏机制资料库（agent-docs/）

`agent-docs/` 是 **PoE2（0.5.0）机制中文参考资料**（伤害类型、抗性、护甲/闪避/ES、暴击、异常状态、伤害-防御计算顺序、宝石、通货等）。

**实现任何机制前的查证顺序**：

1. 游戏数据：`data/<版本>/base/` 的 stat、技能数据与常量，词条文本，官方 patch notes；
2. `agent-docs/` 对应主题，以及 PoE2 Wiki / PoE2DB；
3. vendored PoB-PoE2 Lua 用于交叉核对，结论不同时按第 1 项判断。

`agent-docs/` 是**开发输入资料，不是最终权威**；与一手数据冲突时以可验证来源为准，并直接修正文档（保留来源说明）。注意 PoB 公式多基于 PoE1，与 PoE2 存在差异（如护甲系数 `*5` vs `*10`），文档中已标注。

## 关键约定

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本走 `pobr-i18n`（`en-US`/`zh-TW`）。
- **不可变 / 确定性**：calc 函数对 `Env` 的可变写入集中在 `perform`，并行化只在只读快照阶段展开。
- **Build Code** 走 XML → deflate → URL-safe Base64（`pobr-build::{decode,encode}_pob_code`，已用真实 PoB2 ninja code 验证）；自定义/复制物品需保留原始文本块以便和 PoB2 对比。
- **发布产物只含游戏计算能力**：`POBR_DBG_*` / `POBR_GATE_DENY` 诊断开关（`dbg_env!`）仅存在于调试构建，发布版 CLI 与 WASM 中恒为关闭；测试辅助（`pobr-core` 的 `test-rules`、`pobr-gamedata` 的 `test-pins`）在 feature 后面，只由 dev-dependencies 启用。新增诊断或测试辅助沿用这两种方式，不进入默认构建。
- 文档以可执行契约为主；改变 crate 边界、聚合语义、catalog/parity 规则时更新本文件和受影响的已入库契约文档。本地历史审计无需逐项更新。

## Web 功能契约

升级指引（市集 / 宝石 / 天赋规划）、构筑工作区与构筑比较、物品打造与暗金目录、武器组与技能选择（形态与层数、
宝石启用、Full DPS、召唤物选择）、天赋树的武器组加点、配置页和计算明细的行为契约见
[Web 功能契约](web-contracts.md)。改动 `web/src/lib/trade*`、各 planner、
`useBuildSession` / `buildWorkspace`、武器组或 PoB 导入导出前先读对应小节，行为变化时同步更新。
暂时隐藏的流派参考面板也记录在该文档末尾。
功能覆盖按三层记录：引擎能计算、JSON / CLI 能输出、玩家能在 Web 中完成操作。已有输出字段不代表 Calcs 有分解，
已有配置控件或 handler 不代表对应机制已生效，handler 的数量不是界面控件的覆盖率。

Calcs 的命中区间 `Physical/Fire/Cold/Lightning/Chaos` + `Min/Max` 和 `TotalMin/TotalMax` 是非暴击命中经过敌人减伤后的上下界：直接把现有命中分量乘 DPS 阶段保留的该类型 `EffMult`，明细给出减伤前界限与倍率，不再运行减伤计算。保留小数，沿用上文的精度约定；PoB2 的整数上下界可能造成小额命中的明显相对误差。攻击的区间、`EffMult`、`CritEffect` 只在 `MainHand…` / `OffHand…` 字段展示，与 PoB2 的手别表对应，顶层同名字段留 0；法术等无手别计算使用顶层。`DpsMultiplier` 保留技能、INC、MORE、手榴弹重复引爆与弹幕因子，双持按交替攻击取平均、同时命中取和；不包含 `QuantityMultiplier`。`Repeats` 沿用现有重复次数，`TotalNumberOfHits` 是 EHP 阶段算出的死亡前承受攻击总次数（已计避免被击中），不是技能命中次数。`StrikeTargets` 按选定 Strike 技能和祖灵增幅快照计算，额外目标不乘入单目标伤害，详见上文。这些字段只展示已有计算，不改变伤害或 EHP。

Consecrated Ground (`calc::consecrated_ground`) uses the game
[ConsecratedGround keyword](https://poe2db.tw/us/Keywords): an actor explicitly
standing on it regenerates 5% of its current maximum Life per second and has
50% reduced Curse effect (`life_regeneration_rate_per_minute_%`,
`curse_effect_on_self_+%`, buff `ground_consecration`). Config
`conditionOnConsecratedGround` and native `OnConsecratedGround` /
`Condition:OnConsecratedGround` share the combat snapshot; duplicate grants
apply once. Minions require `conditionMinionsOnConsecratedGround` or their
own condition flag, never the player's position. Sanguis Heroum's
`UniqueCharmCreateConsecratedGround1` in the active game's `base/mods.json`
and [item text](https://poe2db.tw/us/Sanguis_Heroum) supply
`charm_create_consecrated_ground_when_used_duration_ms = 4000` and
`charm_create_consecrated_ground_when_used_radius = 14` (1.4 metres). The local
`Charm<N>ConsecratedGroundDuration` / `ConsecratedGroundRadiusMetres` views
retain these per-use properties on inactive equipped charms too; they neither
establish that ground exists nor place any actor on it. Charm effect, quality
and charm duration do not scale this separate ground event. This source rule
is exporter-absent; the base ground benefit is implemented by PoB2 too.
Ground ownership and an ally's ground effect are not inputs, so
`ConsecratedGroundEffect` is deliberately not consumed: the user's own-ground
modifiers must not scale an ally's ground. No use frequency, uptime, lingering
effect or in-game timing measurement is assumed. TW labels use the game's
[奉獻地面 keyword](https://poe2db.tw/tw/Keywords) and
[嗜血英雄／止血護符 text](https://poe2db.tw/tw/Sanguis_Heroum); surrounding UI
phrasing is application copy, not an official translation. Retained official
tables provide English and TW only; the new CN labels/help are contextual
application wording, not an official CN translation.

Cadiro's Gambit (`calc::cadiro_arrows`) uses the retained active unique,
`UniquePerandusArrows1` and stat `each_arrow_fired_gains_random_perdandus_prefix`.
The game's linked keywords define [Crescendo](https://poe2db.tw/us/Crescendo_Arrows)
as 6 additional chains and 60% increased damage per prior chain,
[Splinter](https://poe2db.tw/us/Splinter_Arrows) as splitting to 6 targets,
[Reversing](https://poe2db.tw/us/Reversing_Arrows) as piercing all targets and
returning, [Diamond](https://poe2db.tw/us/Diamond_Arrows) as guaranteed critical
hits with 60% increased critical damage bonus,
[Covetous](https://poe2db.tw/us/Covetous_Arrows) as 600% increased rarity on its
kills, and [Blunt](https://poe2db.tw/us/Blunt_Arrows) as 600% increased stun buildup.
The single `cadiroArrow` selection maps to native multiplier `CadiroArrow`:
0/unset is neutral, and 1–6 follow that order. Explicit zero/false/invalid input
retires an imported selection. Only the selected attack's `skill_can_fire_arrows`
stat and Bow weapon flags qualify; spells, crossbows, minions, proxy skills and
non-arrow secondary stat sets do not inherit the effect. Combat mode is required.
Crescendo reuses `skillChainCount` (native `Chain`) for the hit's prior chains,
capped by available chains, defaulting to the initial hit. This is increased
damage, not MORE damage or additional single-target hits. Diamond preserves
the first accuracy check, bypasses the critical evasion reroll, and yields to
`NeverCrit`. The source's fixed effects do not stack from duplicate grants.
`CadiroArrowReturn` and `CadiroArrowKillRarity` retain source details and have no
invented PoB keys; return geometry/timing and loot outcomes remain unmodelled.
No equal probabilities, average DPS, all-six simultaneous effects, or extra
return/chain hits are assumed. The new source rule is exporter-absent and
excluded on the reference basis; the ordinary quiver attack speed is unchanged.
TW and CN arrow names use their respective game KeywordPopups
([TW](https://poe2db.tw/tw/Crescendo_Arrows),
[CN](https://poe2db.tw/cn/Crescendo_Arrows), and corresponding five linked names);
these differ from the abbreviations in the item line. Surrounding control/help
wording is application copy. No in-game timing or geometry test is claimed.

Remnant pickup views (`calc::remnant_pickup`) keep the collector's Arcane
Remnants recovery separate from the creator's Empowering Remnants effect grants.
The active game's `base/passive_tree.json` nodes 46124 / 40985 and `base/stats.json`
provide `remnant_recover_%_mana_on_pickup` (3% maximum Mana per collection) and
`created_remnants_have_%_chance_to_duplicate_pick_up_results` (15% chance for
**Remnants you create** to grant their effects twice; also visible in the
[current passive text](https://poe2db.tw/us/Empowering_Remnants)). The older generic
`stat_descriptions` overlay says a pickup counts as another pickup, but that is
not the current passive wording and does not establish that the collector's
separate recovery triggers twice. Accordingly Arcane recovery is not multiplied
by duplication chance or Remnant effect. Instant on-pickup recovery is not Mana
regeneration and is not scaled by recovery-rate modifiers.

`remnantPickupsPerSecond` / native `RemnantPickupsPerSecond` counts the player's
collections from any creator. `createdRemnantPickupsPerSecond` /
`CreatedRemnantPickupsPerSecond` counts collections of the player's own created
Remnants, including collections by another recipient; another creator's chance
is unknown and is never substituted. `remnantPickupMissingMana` /
`RemnantPickupMissingMana` is an optional deficit immediately before each pickup:
recovery retains the maximum-Mana percentage basis, but cannot exceed that
deficit or unreserved Mana capacity. Full Mana or an explicit zero deficit gives
zero recovery. `CannotGainMana` also suppresses recovery; `NoManaRegen` does not
block this instant event. Without a deficit, displays explicitly say **potential** recovery;
none of these views is added to regeneration, sustain, or EHP. A supplied rate
assumes that same deficit before each event, not simulated spending between them.
Missing or zero rates do not infer collection from skill use. XML, JSON raw inputs
and native multipliers agree; explicit false, zero or invalid raw input retires
an imported value. Config reads are tracked even when absent. Combat is required
for event outputs; source percentage/chance properties remain inspectable.

Expected intrinsic effect-grant counts are `1 + clamp(chance, 0, 100) / 100` for
one collected own-created Remnant; this is event metadata, not buff magnitude,
duration, stacks, DPS, or a recursive pickup chain. Potential grants per use reuse
`effect_terms.remnants_per_use()` and explicitly require **all created Remnants
to be collected**; the view does not establish that any are collected. Products
saturate only at the largest finite representable number, recorded in details.
The two rules are exporter-absent and excluded on the reference basis. No in-game
timing or duplication interaction test is claimed. Retained official TW skill
names `mana_remnants` / `life_remnants` are 魔力痕跡 / 生命痕跡; the new labels and
help use 痕跡, with the remaining wording being contextual application copy.
The CN labels/help are contextual UI copy, not an official CN translation.

Volatility restoration (`calc::volatility_events`) uses active game stat
`volatility_refresh_%_chance`, Instability passive 43944 (25%), and
`maximum_volatility_allowed` (Controlled Chaos: 30). The game's
[Volatility keyword](https://poe2db.tw/us/Volatility#VolatilityRef) defines a
200-stack default maximum, a four-second explosion timer reset by each new
stack, and at most one gain per 0.1 seconds. The explosion consumes its stacks;
on a successful refresh the equivalent exploded amount is regained. The keyword
also describes 100 Physical self-damage per stack and ten seconds of Volatile
Power, but these are not simulated by this restoration view. The explicit
`volatilityExplosionQuantity` / native `VolatilityExplosionQuantity` snapshot
must be a finite positive whole number; it is capped by the final maximum,
which Controlled Chaos replaces rather than increases (also clarified in the
[0.3.0 patch notes](https://poe2db.tw/us/Version_0.3.0)). Expected restoration per
explosion is capped quantity times the summed refresh chance, clamped to 0–100%.
The optional `volatilityExplosionsPerSecond` / native
`VolatilityExplosionsPerSecond` is a supplied rate of eligible explosions of
that same quantity. Without it, the per-second view is zero; invalid rates and
overflow are rejected. No rate is inferred from attack speed or the resetting
timer, and restored stacks never feed back into an infinite-chain calculation,
even at 100% chance. No current stack count, damage, defence, Volatile Power,
expiry, or average uptime is changed. Without a source, all views are zero. With a player source, chance and cap
remain potential metadata even with absent/zero/invalid quantity or in noncombat
mode; all event outcomes remain zero. Minion actors are excluded. Explicit raw false,
zero or invalid values retire imported native settings; absent inputs preserve
them, and both controls are tracked even when absent. Both new source rules are
exporter-absent and excluded on the reference basis. Fields have retained
formula inputs, source groups, and no invented PoB keys. Retained game names
[易爆](https://poe2db.tw/tw/Volatility) / [无常](https://poe2db.tw/cn/Volatility)
supply the TW/CN term; complete labels and help are contextual application copy.
No in-game timing or gain-cooldown interaction test is claimed; callers supply
eligible events rather than assuming refresh bypasses any gain restrictions.

### Combo generation snapshots

`calc::combo` reports `ComboMaximum` and `ComboGainPerEligibleEvent` for the selected
receiving skill. The current [Combo keyword](https://poe2db.tw/us/Combo) describes a
counter on some skills, built by striking enemies with the same bound weapon;
weapon swaps lose it. [Tempest Bell](https://poe2db.tw/us/Tempest_Bell) and
[Mantra of Destruction](https://poe2db.tw/us/Mantra_of_Destruction) explicitly build
it with other skills. Current `base/granted_effect_stat_sets.json` gives required
amounts 4 and 8; Combo Finisher I/II require 3/5, and Culmination I/II and Vruun's
Inevitability give an explicit maximum of 10. Required amounts serve as capacity
only without an explicit maximum; they are never added to that separate cap.
Distinct requirements use the greatest threshold, not their sum. Signed quality
changes first adjust the active skill's own requirement (for example Mantra);
Finisher requirements and explicit maxima each use their greatest contribution.
Supported receivers come through the existing support eligibility checks.

This is one successful eligible event **starting from an empty counter**, with
only global gain modifiers, not the receiving skill's own hit/cast rate.
`additional_combo_gain_on_hit` ([Martial Adept, passive 41751](https://poe2db.tw/us/Martial_Adept))
adds its integer count once to that event. `additional_combo_gain_chance_%` says
“chance to build an additional Combo”: its contributions add to one probability
bounded to 0–100%, adding exactly one Combo on success. Therefore with fixed bonus
`a`, chance `p` as a fraction and capacity `c`, expected gain is
`(1-p) × min(c, 1+a) + p × min(c, 2+a)`. Count inputs are nonnegative integers.
This additive, nonrecursive interpretation follows the wording and the shared
stat IDs; the combined interaction has **not been measured in game**. There is
no overflow mechanic in those descriptions. No held counter, decay, uptime,
event frequency, automatic spending, recovery or DPS bonus is inferred.
Practiced Combo modifies the generating strike, not the receiver: its support
bonus is not borrowed from a different skill or included in this global-only view.

The Finality keyword forbids Combo gain. `conditionFinality` / native `Finality`
is an explicit current snapshot; false/off retires an imported true state, and
an explicit native condition overrides a `Condition:Finality` modifier flag.
Absent a receiver, outside combat, during Finality, or for unsupported proxy /
minion receivers, generation is zero; inactive paths do not read global gain
modifiers. The checkbox only models this gain restriction, not Finality's other
bonuses. New generation rules are exporter-absent; reference mode preserves the
existing Culmination selected-stack multiplier but excludes these generation
views. Calcs formulas retain their inputs and have no invented PoB output keys.
`連擊` and `終極` are the corresponding current game TW keyword terms
([TW keyword](https://poe2db.tw/tw/Combo)); full labels/help are contextual UI copy.
CN uses `连击` and `绝杀` from the corresponding retained
`i18n/zh-CN/stat_lines.json` gain-Combo and gain-Finality entries.

Giant's Blood uses the current game's passive 32349 and stat
`can_wield_2h_axe_sword_mace_in_one_hand`: two-handed Axes, Maces and Swords may
supply either hand of a dual-wield weapon attack. The prepared equipment view
provides possible weapon sources; after all modifiers and item-granted keystones
arrive, the `GiantsBlood` flag enables the second hand and `DualWielding`.
Without it, the second-hand candidate contributes no attack. Local modifiers of
an unused off-hand weapon never become global modifiers. This changes permitted
grip only: base classes, per-hand `Weapon2H` flags, `UsingTwoHandedMelee`,
two-handed damage modifiers, the existing triple martial-weapon attribute
requirements and halved inherent Strength Life are retained. Shields remain
shield sources; staves, quarterstaves, bows, crossbows and other excluded weapon
types gain no second-hand eligibility. This is static weapon-source selection,
not a general equipment legality or skill weapon-restriction validator.

### Static helmet threshold and projectile Armour

Strong Chin (current passive 10998,
`stun_threshold_+_from_lowest_of_base_helmet_evasion_rating_and_armour`) already
uses the generated parser's `StunThreshold` BASE modifier and the existing
`LowestOfArmourAndEvasionOnhelmet` snapshot. The retained game text says
“Evasion and Armour on your Helmet”: the minimum is taken after the helmet's
local flat/increased modifiers and quality, before global defence increases.
Known bases use current game base values; imported defence displays are only a
fallback for unknown bases. Missing either defence contributes zero. Ordinary
Stun Threshold increases/multipliers then apply to the added amount.

Projectile Bulwark (current passive 19236,
`defend_with_%_armour_against_ranged_attacks`) says “Defend with 120% of Armour
against Projectile Attacks”. It uses the configured incoming enemy
`enemyDamageType = Projectile` category, independently of the player's skill.
Melee, spell, spell-projectile, untyped and unspecified/Average hits receive no
conditional benefit. It changes effective applied Armour in the shared
mitigation snapshot used by damage taken, EHP, max-hit and existing details;
character Armour and borrowed Evasion/ES terms remain separate. The percentage
uses the existing strongest-`ArmourDefense` convention rather than multiplying
several “defend with” percentages; this stacking interaction has no in-game
measurement here. The current pinned exporter has no projectile-specific rule,
so that source is exporter-absent. No new condition, event or display is added.

### Selected equipment, resources and hit snapshots

Imported magic item names remain unchanged. Base lookup first accepts an exact
catalog name, then the longest complete base between a prefix and optional
` of ...` suffix. The base itself may contain `of` (Fists of Stone); a base
mentioned only inside a suffix is not a match. Local defence and weapon readers
share this lookup, so an imported defence display is not counted again as a
base when the game base is known. Grand Spectrum counts selected socketed jewels
across implicit, explicit and enchant text; corruption can move its identifying
line into the implicit section.

Skills bound to another weapon set resolve attributes, granted levels, quality
and socket effects against that set's passive allocation as well as its weapons.
Only the group explicitly identified as an item grant is exempt from ordinary
gem requirements. A listed grant keeps its supports and enabled state instead
of acquiring a second enabled source copy. Minion-compatible supports can count
for the summoning gem's support-based levels and reservation, using the same
eligibility and no-type-borrowing restrictions as summon costs.

Selected minion hits can establish the `Damage` capability required by a
support even when their raw game skill types omit it, as with Fire Djinn's
Brand. This uses that selected effect's intrinsic damage payload; it does not
add raw types or borrow damage from a sibling skill. Non-damaging effects and
otherwise incompatible supports remain ineligible.

Selected warcries use `WarcrySpeed` for both action rate and its breakdown.
Generic Skill Speed already feeds that bucket and is counted once; Cast Speed
does not apply. Game cast time, action speed, cooldown recovery and explicit
Ignore Warcry Cooldowns retain their existing roles. Frozen Warbringer05 has
an 800 ms base and 868% applied increased speed, giving 12.1 uses per second
with its cooldown explicitly removed.

Blackened Heart scales Unholy Might buffs the character grants from the
character's completed maximum Mana, once per full 100 Mana. Necromantic
Conduit's ally grant expands that named buff after it reaches the minion;
the minion's Mana does not determine the caster's granted magnitude. Recipient
buff effect remains separate, duplicate flags establish one buff, and the
player's own buff condition is not inherited by every minion. Low Mana,
inactive grants, zero Presence count and blocked ally auras preserve their
gates. The derived ally magnitude retains the caster's passive origin.

Enemy `<Element>Resistance` and `<Element>Resist` modifiers are aliases in
mitigation and its source breakdown. Xoph's Presence reduction therefore
combines with Exposure; an explicit winning resistance override still
bypasses those base modifiers. Hits, damage over time, minions and Sacred
Flame use the same resolved resistance.

Blackflame Covenant (game passive 42680,
`keystone_fire_spells_become_chaos_spells`) applies only when the selected skill
has both the Spell and Fire types. Its three retained game lines convert Fire
to Chaos, include Chaos hit damage in the existing Ignite source calculation,
and make that Ignite deal Chaos damage. The last operation changes enemy
resistance and damage-taken queries, not the ailment's source, magnitude,
duration or chance; damage increases are not applied a second time and
penetration still does not affect damage over time. The type-changing passive
remains an ancestor of the Ignite trace. This is a static rule, with no new
combat state or inferred trigger history. The [game text](https://poe2db.tw/us/Blackflame_Covenant)
and retained passive agree; PoB2 is a cross-check.

Skin of the Loyal's four uncapped-resistance flags add the corresponding final
uncapped resistance as an increased Armour, Evasion, Energy Shield or Ailment
Threshold modifier. These are ordinary increases, not multipliers. They read
the buff/charge resource snapshot before defence conversion, avoiding a
self-referential conversion loop. Companion-in-Presence defaults on only when
an enabled companion skill exists; explicit true/false remains a selected
condition. Hinder applied by an active curse also supplies EnemyHindered.
Enemy modifiers gated by an applied mark or curse are forwarded again after
curse-slot selection. This enables Marked for Sickness only when a mark takes
effect, without duplicating modifiers already active before the buff pass.

Direct total Energy Shield, such as Discipline, converts into a direct Life or
Mana total after the destination's increases/multipliers. It is not a new base
pool and is not fed back into the defence conversion matrix. Converted totals
retain their source attribution. Damage sharing is bounded to the incoming
damage and the companion's finite pool. Life prevention consumes an exactly
exhausted boundary as dead, rather than retaining a floating-point residue;
legitimate positive sublethal Life is preserved. Incoming critical damage uses
the incoming Hit context, independently of the player's selected skill.

Remaining-chain modifiers apply to the selected hit: available chains minus
the explicitly selected number already used, bounded to zero and capacity.
They do not multiply DPS by a hypothetical number of targets. Twisted Empyrean's
Mana-based added Cold damage uses the item's hand and the final maximum Mana;
the existing PercentStat convention rounds each endpoint up before damage
effectiveness. Its current `with Mace Skills` Fire-to-Cold conversion wording
uses the ordinary conversion matrix and normalizes totals above 100%. The
pinned exporter only recognizes the older `of ... skills` wording, so this
conversion rule is exporter-absent.

The frozen 2026-10-26 corpus also exposes reference differences that are not
engine targets. Spell Totem's retained game-level cap reaches three (and its
cast time is 0.5 seconds); the pinned exporter's hidden support overrides the
cap to two and uses a 0.6-second placement fallback. Mortar Cannon's retained
cast time is one second. A selected non-totem has no selected-totem capacity,
and capacity alone does not establish a summoned-count snapshot. Runic
Tempering's conditional reduction requires held Runic Plates; an enabled skill
without that snapshot does not activate it. Frozen socket modifier text remains
an input even when the current rune definition changes (for example Zalatl's
3% versus 5% Mana). Blind comparisons must distinguish the strongest applicable
effect from duplicate copies; explaining Evade/Deflect does not by itself
explain TotalEHP. The Blood Mage03 pinned-oracle EHP has a separately traced
positive-Life floating residue after lethal damage; its recursive batched-hit
result is not a valid numerical target.

Frozen Lich02's Storm Mage reaches gem level 38 / monster level 76 after the
named +3 level bonus is recognized. Its base Life 4729 and 213% increased Life
give 14801.77. The pinned exporter additionally deducts the monster's 15%
Energy Shield share, producing approximately 12582 Life. The retained
`energyShield` field and the native schema's conversion comment come from the
vendor extraction path; they do not independently establish whether the game
deducts Life or grants additional Energy Shield. Keep this row unresolved,
without changing the formula or classifying it as a known reference error.

Storm Mage's separate quality stat
`minion_maximum_life_%_to_gain_as_maximum_energy_shield` grants extra ES from
that minion's completed Life. Its retained handler uses `EnergyShield BASE`
with `PercentStat(Life)`, not the base-resource gain-as matrix channel. The
existing PercentStat convention rounds this contribution upward before normal
ES scaling; this rounding has no in-game measurement. Resolve the recipient's
Life after its buffs, before offence and defence, and use the same snapshot
for its source queries and ES formula. Player Life and unrelated minions do
not supply this value. This quality grant does not deduct Life or resolve the
intrinsic 15% question above.

The frozen top-ten corpus has 23 remaining attribute-requirement differences
whose reference is exactly the nominal requirement of an exported amulet grant
(Eternal Rage, Wolf Pack, Trinity, Cast on Critical or Cast on Elemental
Ailment). An ordinary exported listing matched to that grant is the same skill,
not a second equipped gem. Keep its nominal requirement out of `ReqStr`,
`ReqDex` and `ReqInt`; the granted skill's usable level is checked separately.
An independent genuine gem still contributes its requirement. The bounded
regression covers ten grant/level combinations and the Gemling's 8% reduction;
the evidence is in `fix-round-3/requirements-evidence.json`.

Undying Hate's explicit "Conquered Small Passive Skills also grant ..." lines
are independent of its seed-specific notable transformations. Attach a fully
parsed payload to each reconstructed non-attribute small passive in the
conquered radius; allocation and small-passive effect scaling then follow the
ordinary tree path. Attribute passives have their own category and do not
receive this addition. Unknown payloads and unknown notable replacements
remain diagnostic. Frozen Titan06 has twelve allocated small passives with
6% Elemental Damage each; Hulking Form raises each to 9%, adding 108% in total.
The retained game stat is
`local_historic_abyss_jewel_conquered_small_passives_grant_elemental_damage_+%`;
its imported roll is preserved instead of rebuilding it from a current range.

Frozen Witchhunter10's Megalomaniac grants Leather Bound Gauntlets (51446):
1323 equipped-glove Armour adds 1323 base Evasion. This affects Armour/Evasion,
Sorcery Ward and the downstream defence panels; the pinned exporter omits the
granted passive. Frozen Infernalist03 instead differs at its Runeseeker rune:
the imported line is 45% less Life, whereas PoB2 rebuilds two MORE modifiers
and rounds their product. Preserve the frozen line. Six defence rows across
four Warbringer builds use the existing selected-totem count instead of assuming
an additional totem. Exact affected rows and arithmetic are in
`fix-round-3/static-defence-attribution.json`.

The five frozen Pathfinder examples already report their finite flask recovery
in `Flask1RecoveryPerSecond` and `Flask1EnergyShieldRecoveryPerSecond`. The
reference adds those rates to regeneration. PoBR keeps the per-use output
separate; selecting an active flask does not establish continuous charge supply
or repeated-use uptime. No extra state or sustained-regeneration contribution
is introduced for these comparisons.

Jewel and passive-effect scaling reaches numeric payloads inside
`MinionModifier` lists as well as top-level modifiers. The Adorned still uses
the jewel's corruption, rarity and socket eligibility; Hulking Form still uses
the allocated small passive. Preserve the wrapper's scope and source, and add
only the effect delta once. Flags and MORE modifiers do not acquire a numeric
passive-effect delta.

The companion Presence rune pair uses its retained game stats
`companions_in_presence_non_skill_base_all_damage_%_to_gain_as_random_element`
and `companions_in_presence_base_resist_all_elements_%`. Both effects follow
the existing ally aura path, restricted by the summoning skill's Companion
type. Explicit zero Presence, hidden recipients and blocked ally auras suppress
both the benefit and the resistance penalty. Random elemental gain uses the
existing equal three-element average; positive Presence counts do not assign
individual actor positions. Recipient aura/buff scaling follows the current
ExtraAura convention, without an in-game effect/range measurement.

PoB item text's `Implicits: N` is a serialized line boundary, including enchant
and rune lines inside that span. Their destination remains `enchant_texts`,
but they consume the count before the following explicit lines are classified.
Otherwise a wrapped Adorned modifier can be split between implicit and explicit
sections and lose its effect even though neither raw line was dropped. The
earlier whole-corpus structural import audit checks retention and identifiers;
it does not prove correct section assignment or downstream modifier application.

Flame Wall's existing `FlameWallAddedDamage` selection supplies only the named
projectile stat set's flat Fire damage. Resolve the enabled source's effective
level, then apply the recipient projectile's ordinary effectiveness and damage
pipeline. Duplicate local and party copies use the existing strongest-effect
merge. The selection does not infer passage, lingering Ignite, infusion or
uptime, and no additional configuration state is introduced.

Some monster generic attacks retain both Melee and Projectile skill types as
capabilities. Their actual projectile hit flag uses retained weapon context
when the selected attack can fire arrows or wand projectiles; an explicit
`base_is_projectile` takes precedence. Known unarmed recipients do not receive
weapon-projectile bonuses. Missing context remains unknown, and raw types stay
available for support compatibility rather than being rewritten globally.

Enabled ordinary buff copies must reach the existing strongest-modifier merge
before deduplication. A first-copy filter can discard a stronger later quality
bonus: the frozen Gemling08 has Cast on Critical copies with 22% and 50%
increased critical chance. Keep the larger contribution and its origin once;
disabled groups or gems do not supply a candidate. Ordinary persistent buffs
retain their existing alternate-weapon behavior; the frozen corpus has no
explicit false `enableGlobal1/2` flags and does not validate that legacy state.
This does not invent additional buff uptime or change frozen item rolls to
match an exporter.
For those same named buffs, matching `ExtraAura` lists merge their numeric
children after effect scaling instead of stacking duplicate Offering payloads.
Each child first inherits its own wrapper's source when needed. Keep surviving
children inside their original source-bearing wrappers, removing displaced
children and empty wrappers; both minion details and the outer-modifier line
ledger must retain the winning sources. Other LIST kinds retain their existing
append behavior.

Frozen Shaman08's configured Flame Wall supplies 18–26 Fire damage at level 15;
its recipient Spark can therefore ignite under the existing ailment model.
The pinned exporter's zero stack potential reads only the BASE totem limit
while the skill supplies an OVERRIDE. This zero is not evidence that added
Fire damage cannot ignite. The exact steady-state value with a chosen number
of simultaneous totems remains unverified, so the two affected rows stay in
the diagnostic queue rather than being declared fully explained.
