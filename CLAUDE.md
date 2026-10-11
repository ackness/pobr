# CLAUDE.md

Detailed repository guidance for coding agents and contributors. [AGENTS.md](AGENTS.md) contains the concise contributor guide.

> **PoBR** = Path of Building in Rust。目标是把 PathOfBuilding（Lua）的核心计算引擎迁移到 Rust：解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈，并在 PoB 兼容的基础上额外提供 source-level 归因（每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献）。

## 构建环境

开发命令是根 `mise.toml` 里的 mise task（`mise tasks` 查看全部，如 `mise run verify build skills`），也可直接使用 Cargo。游戏 CLI（`apps/pobr-cli`，二进制 `pobr`）和 WASM 包只包含游戏计算能力，不承载测试、lint、CI 等开发命令；新增开发命令加到 mise task，不要加进 CLI 或 WASM 接口。日常命令、工具链来源和缓存管理统一见 [开发与 CI 工作流](docs/development-workflow.md)。`cargo-nextest` 用于完整门禁，定向测试默认用 Cargo。

- 每个 worktree 复用**自己的 `./target`**。同一 worktree 的多个会话共用该目录、串行运行 Cargo，不为会话或门禁创建额外 target。**禁止**不同 worktree 共享可写 `CARGO_TARGET_DIR`——并发 cargo 会在构建目录锁上串行排队（症状：长时间无输出；stderr 的 `Blocking waiting for file lock` 提示不要用 `| tail` 等管道吞掉）。
- 同一 target 目录下 cargo 命令**一次一条、前台执行**，禁止后台叠加。

## 验证分层（本地提交默认定向验证）

**先按改动选检查，通过后结束验证并提交。普通本地提交不要求全量门禁。** 不把 `check → build → clippy → test → full` 当作固定流水线；已有相关测试能检查编译时，不先重复运行 check/build。

| 改动 | 最小相关验证 |
|------|------|
| Rust 局部逻辑 | `driver.sh test -p <crate> --test <suite> [filter]`；收尾跑 `driver.sh lint -p <crate> --lib --test <suite>` |
| 计算 / Modifier / parser | 对应集成套件；改变完整 build 数值时再跑 `cargo test -p pobr-build --test parity no_regression` |
| 跨 crate API / 数据结构 | 相关 crate 测试 + 直接使用者的契约或集成测试；边界无法明确时扩大到 workspace |
| Web TS / React | `pnpm --dir web test <test-file>` + `pnpm --dir web typecheck`；交互改动补相关 Playwright spec |
| Rust → WASM / Web 契约 | Rust 契约测试 + 重建 WASM + 相关真实 WASM / E2E 测试 |
| Worker | `pnpm --dir web test:worker` |
| 仅文档 | 审查 diff；不运行 Cargo / Web 构建 |
| 验证脚本 | `bash -n <script>` + `python3 devs/scripts/test_workflows.py`；不为改脚本重跑整个 workspace |

表中 `driver.sh` 指 `bash .claude/skills/run-pobr/driver.sh`。测试目标先查对应 `Cargo.toml` 或 `tests/*.rs`；用 `--test <suite>` / `--lib` 限定编译目标，只有名称 filter 仍可能编译整个 crate 的测试。计算修复必须有能复现错误的断言，不降低 parity 基线来换取通过。

- 编辑中只有需要快速定位编译错误时才单独 `cargo check -p <crate> --lib`。Clippy 按改动选择 `--lib` / `--bin <name>` / `--test <suite>`；测试专属改动无需扩大到 `--all-targets`。
- 同一代码、依赖、工具链、features、数据版本下已通过的检查复用结果；后续只改文档不使代码测试失效。失败后先重跑失败目标，只有新改动、失败或明确影响范围才扩大验证。
- Web 纯 TS/CSS 改动复用已构建 WASM；WASM 产物缺失，或 WASM 及其 Rust 依赖的源码、features、工具链改变时重建。`pnpm build` 已包含 typecheck，最终需要 build 时不额外重复 typecheck。E2E 前确认 dist 对应当前代码。
- **合并 / 发版、工具链 / Cargo features / workspace 依赖变更、影响范围不明的核心改动**需要完整门禁，优先交给 CI。发版由 tag CI 完成；其他场景先推送分支，再运行 `mise run ci <remote-ref>`，并检查该提交的结果。本地不再强制先跑 `full`；离线或排查 CI 时可显式运行 `mise run full`（仅 Rust 门禁，Web 另验）。完整 CI 在 tag、手动触发和发版 PR（workspace 版本变化）时运行；普通 PR 自动运行的是轻量检查（[pr.yml](.github/workflows/pr.yml)），不能替代完整门禁；发起 CI 不等于通过。相同代码不重复运行本地和云端全量门禁。
- 不自动 `cargo clean`、改 profile / features / `RUSTFLAGS` 或共享 target 来“加速”；这些可能使缓存失效。看到构建锁先检查已有进程，不叠加 Cargo 命令。

## 常用命令

```bash
cargo nextest run --workspace                          # 全部测试（推荐；不含 doctest）
cargo test --workspace --doc                           # nextest 不执行 doctest，完整门禁另跑
cargo test --workspace                                 # 无 nextest 时的完整测试（含 doctest）
cargo clippy --workspace --all-targets -- -D warnings  # lint（CI gate，warning 即失败）
cargo fmt --check                                      # 格式检查（CI gate）

cargo test -p pobr-core --test aggregation mod_db::   # 定向编译套件 + 过滤用例
cargo test -p pobr-build --test skills support_gating::
cargo bench -p pobr-core --bench mod_db_bench          # ModDB 热查询基准（criterion）

# PoB2 parity 仪表盘：逐 build 打印 PoBR vs PoB2 对照 + 聚合命中率
mise run test build parity parity_baseline_report -- --ignored --nocapture
# 其中 parity_no_regression 用例是回归门禁（命中率不得低于已记录基线）

# CLI（apps/pobr-cli，二进制名 pobr）
cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
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
- 根 `Cargo.toml` 的 dev/test 保留 `debug = "line-tables-only"`，默认 `incremental = false`，减少增量中间文件并继续复用未变的 Cargo 产物。修改源码后的重编译可能更慢；长期连续编辑时可显式启用 `CARGO_INCREMENTAL=1`。调试变量时可临时使用 `CARGO_PROFILE_DEV_DEBUG=2`（测试用 `CARGO_PROFILE_TEST_DEBUG=2`）。改变这些设置会生成另一组缓存，不作为日常准备步骤。
- 完整 CI 以已入库的 [`.github/workflows/ci.yml`](.github/workflows/ci.yml) 为准（PR 轻量检查见 [`pr.yml`](.github/workflows/pr.yml)）：Rust 检查 fmt / Clippy / nextest / doctest，Web 独立检查类型、单测、Worker、WASM 构建与打包。E2E 默认关闭，仅 `mise run ci <ref> --e2e` 显式请求时运行；不要在未被要求时发起。涉及计算/Modifier/parser 的改动需补对应的集成测试或 golden fixture。

### 快速检查与发版

mise task 的日常命令、缓存、按需诊断、WASM 打包、Worker 测试和发版步骤统一见
[开发与 CI 工作流](docs/development-workflow.md)；外部 PR 的处理见其中“处理外部 PR”一节。

- `mise run verify build skills support_gating::` 一次执行定向测试和 lint；`mise run targets` 列出测试目标，不编译。
- `bash .claude/skills/run-pobr/driver.sh full` 是完整 Rust 门禁，只用于上面列出的完整门禁场景，不是每次本地提交的收尾动作。
- PR 自动运行轻量检查（fmt / Clippy / 冒烟 / parity 回归；改动 `web/` 时加类型检查和单测）；完整门禁在 tag、手动触发和发版 PR 时运行。发版 PR 的完整门禁通过后再合并、打 tag。


## Workspace 结构

当前结构以根 [Cargo.toml](Cargo.toml)、各 crate 的 `src/lib.rs` 和契约/回归测试为准。workspace 有 14 个成员：7 个库、3 个应用入口、4 个 Rust 工具；`web/` 与 Lua oracle 独立于 Cargo workspace。桌面入口仍是占位程序。

`devs/docs/architecture/` 中早期 00–16 文档是可选的本地设计/审计资料，不随新检出提供；已入库的 18–21 文档记录具体功能契约。`11-implementation-progress.md` 和 `14-remaining-work-recheck.md` 都是历史快照，不是当前待办或验收依据。缺少本地资料时直接使用本文件、[Web README](web/README.md)、[数据管线说明](pipeline/README.md) 和代码。

| Crate | 职责 | 主要依赖 |
|-------|------|------|
| `crates/pobr-data` | 纯数据定义，零逻辑、零 I/O，所有 crate 的底层依赖。核心是 `catalog.rs`（入库 JSON schema：`BaseItemDef`/`StatDef`/`ModDef`/`SkillGemDef`/`GrantedEffectDef`/`PassiveNodeDef`/`DataManifest` 等），另有 damage/build_config/display_stat/stat/monster 等域类型 | 无（外部依赖 `serde`） |
| `crates/pobr-core` | Modifier 解析/存储/聚合 + 计算引擎 + source-level 归因 + 来源接入（item/passive/gem/flask ingest）。零 I/O | `pobr-data` |
| `crates/pobr-gamedata` | 运行时数据 loader——数据系统里唯一持有文件 I/O 的层。把 `data/<poe_version>/` 入库 JSON 反序列化为 `pobr-data::catalog` 类型（`GameData::new(version_dir)`，按域懒加载 + i18n 边车；`repo_data_root()` 定位仓库数据目录） | `pobr-data` |
| `crates/pobr-i18n` | 语言包加载 / fallback / 显示文本映射；`en-US`（canonical）+ `zh-TW`，locale toml 经 `include_str!` 内嵌 | `pobr-data` |
| `crates/pobr-tree` | 天赋树拓扑、allocated node mod 收集、范围珠宝（first pass） | `pobr-data` |
| `crates/pobr-build` | Build 状态、PoB Build Code 编解码（XML ↔ zlib ↔ URL-safe Base64，padding 容错）、导入识别、`calc_orchestrator/`（`calculate_with_data` / `calculate_full_dps`）与 `CalcCache`、Build 对比。**parity 测试的主战场**（见下） | `pobr-data`/`pobr-core`/`pobr-tree`/`pobr-item`/`pobr-gamedata` |
| `crates/pobr-item` | raw item 文本的**全保真编辑态**解析 + 逆向序列化（`draft.rs`/`annotations.rs`/`build_raw.rs`）。职责边界清晰：**calc 视图**（剥标注 / variant 门控 / range 取值后喂引擎）由 `pobr-core::item_text` + `item::ingest_item` 承担；**编辑态视图**（保留 calc 刻意丢弃的 variant 名列表 / 行级标注 / 未建模标注，支持 BuildRaw 往返）由本 crate 的 `ItemDraft` 承担。复用 `pobr-core::mod_parser` 解析 modifier，避免规则重复 | `pobr-data` + `pobr-core` |
| `apps/pobr-cli` | CLI：`calculate` / `parse-mod` / `decode-code` / `encode-code`（命令逻辑在 lib，便于测试） | `pobr-build`/`pobr-core`/`pobr-i18n` |
| `apps/pobr-wasm` | `build_api/` 提供 JSON 契约；默认 features 可在宿主测试，`wasm` feature 启用 wasm-bindgen 绑定 | `pobr-build`/`pobr-core`/`pobr-data`/`pobr-gamedata`/`pobr-item`/`pobr-i18n` |
| `apps/pobr-desktop` | 示例 build 计算与文本摘要；尚未引入 GUI 框架 | `pobr-build`/`pobr-core`/`pobr-data`/`pobr-i18n` |
| `tools/pobr-data-adapter` | 数据管线适配器——GGG `.dat` 导出 → 解析外键、反范式化为入库最小 JSON 落到 `data/<poe_version>/`。直接调用时缺列默认告警降级，正式再生成始终使用 `--strict-columns` 并校验原始表版本收据；产物 `_meta.regen_command` 记录再生成命令 | `pobr-data` |
| `tools/sync-pob-catalog` | 从 PoB 核心 Lua 抽取属性 catalog、parity 检查/diff、vendor Lua → overlay JSON | `pobr-data` |
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
也不从 vendor 或旧产物动态推导范围。来源、兼容边界与升版步骤见[品质生成说明](pipeline/gem-quality/README.md)。

特殊 Modifier 规则在编译时递归校验 flags、标签字段/作用对象、捕获、handler 和运算。
`precompile-mods --check` 校验运行时合并后的完整集合，并输出文件哈希、顺序和规则来源；
`refresh-modifiers.sh` 在审计和发布产物前执行此检查。`StatId` 仍是开放名称，解析通过不证明计算支持。
覆盖语义与边界见[维护者规则说明](docs/contributing-mods.md#7-validate-test-and-open-a-pr)。

### 数据与计算上下文约束

活动数据版本由 `pobr_gamedata::data_version()` 在运行时解析：`POBR_DATA_VERSION` → `POBR_DATA_ROOT/CURRENT`（默认仓库 `data/`）→ 固定兼容回退。`pobr-data` 不再编译嵌入 `data/CURRENT`；其旧 `DATA_VERSION` / `data_version()` 调用需迁移到 loader。数值 golden 仍单独固定。

schema 3 的 `manifest.json` 列出运行时文件及 SHA-256；`BuildData::load` 在可选域降级前校验完整快照。旧 schema 1/2 保留兼容行为。`patch/` 与 `overlay-common/` 是独立覆盖层，不属于快照哈希清单。正式数据和 Web 同步先完成候选生成/校验，再替换目录；失败恢复旧目录，不自动 bless golden。

`CalculationSession` / `Env` 只执行一次；再次 `perform` 返回 `AlreadyPerformed`。重算应创建新会话，避免再次应用资源转换。属性缩放与资源派生共享包含职业基础值及 INC/MORE 的最终属性；天赋关联的装备、品质和诊断查询都使用所选树。

Web 的计算、市场目录和天赋树使用初始化时固定的数据版本；重新初始化清除翻译与词缀层级缓存。节点规划逐节点计算，不能按原始词条文本共享包含位置/半径效应的结果。缺少历史树展示元数据时保留导入和计算，禁用不可靠的历史树编辑/规划。存档导入校验嵌套结构，非法存档不覆盖当前内容；启动失败保留原始字节并提供下载恢复。

CLI 完整构筑与 marginal 结果包含数据版本、树版本和未支持词条，调用方必须保留诊断，不能把未支持内容视为零效果。

## 计算引擎架构（pobr-core）

目录按 Modifier 的生命周期拆分（见 [pobr-core/src/lib.rs](crates/pobr-core/src/lib.rs)）：

| 目录 | 职责与主要入口 |
|------|----------------|
| `model/` | `modifier.rs` 定义 Modifier / ModTag，`config.rs` 定义 CalcConfig |
| `parse/` | `mod_parser/`、`mod_cache.rs`、`apply_range.rs` 处理文本、解析规则与缓存 |
| `rules/` | stat map、特殊词条、配置、buff 解释与 handler 契约 |
| `ingest/` | item / passive / skill / character / campaign 接入，保留 SourceId 归因 |
| `aggregate/` | `mod_db.rs` 提供 sum / more / flag / override / list 及带 trace 的查询 |
| `calc/` | CalculationSession / Env 与 offence、defence、ailment、buff、EHP 等计算阶段 |
| `attribute/` | TraceGraph 与 direct / marginal / interaction 归因 |

数据流：**数据/文本 → 解析与规则映射 → 来源接入 → ModDb → calc → OutputTable / Breakdown / Trace / Attribution**。旧的 `pobr_core::mod_parser`、`mod_db`、`modifier` 等公共路径通过 `lib.rs` 重导出保留；它们不代表源码仍在根目录。

`calc/session.rs::CalculationSession` 是底层会话入口；完整 Build 由 `crates/pobr-build/src/calc_orchestrator/mod.rs` 编排。该目录按 `skill_resolve.rs`、`weapon.rs`、`inject.rs`、`buffs.rs`、`triggers.rs` 等拆分技能解析、武器来源、注入和触发逻辑。`calc/perform.rs` 执行计算阶段，`display_catalog.rs` 定义可展示字段。

编排前置解析由 `calc_orchestrator/prepare.rs` 返回完整的技能、配置和武器结果，不使用默认值占位后分阶段修改。`CalculationContext` 显式携带 stat-map catalog、观察模式和触发子计算状态；计算热路径不使用线程局部上下文。需要映射诊断时使用 `calculate_with_data_report`，记录由返回的 `CalculationReport.stat_map_records` 独占（含触发子计算），原 `take_stat_map_compare_records` 接口已移除。

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

光环由 `calc/buff_pass.rs` 统一处理；旧的 `buff-pass-aura` Cargo feature 和编排层直接注入光环的通道均已删除。运行时 buff 阶段受 `mode_buffs` 控制，curse/debuff 另受 `mode_effective` 控制；不要按历史迁移文档重新引入旧开关。

## Parity 体系（回归基准）

PoB2 兼容是硬回归基准，三层校验互补：

1. **`crates/pobr-build/tests/parity/ninja_parity.rs`** — 遍历 `examples/demo-bd-test/builds/*/`（真实 PoB2 build + `meta.json::player_stats` 黄金数值），零硬编码对比全部职业/技能；`parity_no_regression` 断言聚合命中率不低于基线。防御/属性与 DPS 分列报告，未完成的 offence 管线不会掩盖防御侧信号。
2. **golden / dual-run 套件** — `crates/pobr-build/tests/parity/` 下的 golden 与 PoB2 对照用例，以及 `tests/dualrun/` 下的 statmap / config 对照，钉住中间值与配置语义。
3. **`tools/pob2-oracle`** — 需要逐分量定位偏差时，从 vendored PoB2 直接 dump Lua 侧计算分解对照。

`crates/pobr-build/tests/` 下的 `codec.rs`、`skills.rs`、`parity.rs`、`dualrun.rs` 是 Cargo 集成测试目标，其子目录文件是用例模块；`--test` 使用汇总目标名。`pobr-core` 同样按 aggregation / parser / sources / engine / offence / defence / ailments / golden 汇总。

数值 golden 钉住 `GOLDEN_PARITY_DATA_VERSION`，逻辑回归通常使用活动数据版本；两者不能互相替代。依赖本地 vendor 的对照可能跳过，需如实记录，不能将跳过视为公式已获验证。

`vendor/PathOfBuilding-PoE2/` 是 [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的完整检出（版本见 `vendor/.pob2-version.txt`；gitignore 不入库，`bash .claude/skills/run-pobr/driver.sh vendor` 可按钉定 commit 重新克隆），公式核对直接读本地 Lua（`CalcOffence.lua`/`CalcDefence.lua`/`CalcPerform.lua` 等），不要去网上找。

## 游戏机制资料库（agent-docs/）

`agent-docs/` 是 **PoE2（0.5.0）机制中文参考资料**（伤害类型、抗性、护甲/闪避/ES、暴击、异常状态、伤害-防御计算顺序、宝石、通货等）。

**实现任何机制前的查证顺序**：

1. 先查 `agent-docs/` 对应主题；
2. 对照 vendored PoB-PoE2 Lua 计算实现；
3. 对照官方 patch notes / PoE2 Wiki / PoE2DB / 游戏数据。

`agent-docs/` 是**开发输入资料，不是最终权威**；与一手数据冲突时以可验证来源为准，并直接修正文档（保留来源说明）。注意 PoB 公式多基于 PoE1，与 PoE2 存在差异（如护甲系数 `*5` vs `*10`），文档中已标注。

## 关键约定

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本走 `pobr-i18n`（`en-US`/`zh-TW`）。
- **不可变 / 确定性**：calc 函数对 `Env` 的可变写入集中在 `perform`，并行化只在只读快照阶段展开。
- **Build Code** 走 XML → deflate → URL-safe Base64（`pobr-build::{decode,encode}_pob_code`，已用真实 PoB2 ninja code 验证）；自定义/复制物品需保留原始文本块以便和 PoB2 对比。
- **发布产物只含游戏计算能力**：`POBR_DBG_*` / `POBR_GATE_DENY` 诊断开关（`dbg_env!`）仅存在于调试构建，发布版 CLI 与 WASM 中恒为关闭；测试辅助（`pobr-core` 的 `test-rules`、`pobr-gamedata` 的 `test-pins`）在 feature 后面，只由 dev-dependencies 启用。新增诊断或测试辅助沿用这两种方式，不进入默认构建。
- 文档以可执行契约为主；改变 crate 边界、聚合语义、catalog/parity 规则时更新本文件和受影响的已入库契约文档。本地历史审计无需逐项更新。

## Web 功能契约

升级指引（市集 / 宝石 / 天赋规划）、构筑工作区、武器组与技能选择的行为契约见
[Web 功能契约](docs/web-contracts.md)。改动 `web/src/lib/trade*`、各 planner、
`useBuildSession` / `buildWorkspace`、武器组或 PoB 导入导出前先读对应小节，行为变化时同步更新。
暂时隐藏的流派参考面板也记录在该文档末尾。
