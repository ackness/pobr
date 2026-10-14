# Maintainer guide

Detailed repository guidance for contributors and coding agents: build environment, validation scope, architecture and conventions. [AGENTS.md](../AGENTS.md) contains the concise contributor guide.

> **PoBR** = Path of Building in Rust。目标是把 PathOfBuilding（Lua）的核心计算引擎迁移到 Rust：解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈，并在 PoB 兼容的基础上额外提供 source-level 归因（每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献）。

## 构建环境

开发命令是根 `mise.toml` 里的 mise task（`mise tasks` 查看全部，如 `mise run verify build skills`），也可直接使用 Cargo。游戏 CLI（`apps/pobr-cli`，二进制 `pobr`）和 WASM 包只包含游戏计算能力，不承载测试、lint、CI 等开发命令；新增开发命令加到 mise task，不要加进 CLI 或 WASM 接口。日常命令、工具链来源和缓存管理统一见 [开发与 CI 工作流](development-workflow.md)。`cargo-nextest` 用于完整门禁，定向测试默认用 Cargo。

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

表中 `driver.sh` 指 `bash devs/scripts/driver.sh`。测试目标先查对应 `Cargo.toml` 或 `tests/*.rs`；用 `--test <suite>` / `--lib` 限定编译目标，只有名称 filter 仍可能编译整个 crate 的测试。计算修复必须有能复现错误的断言，不降低 parity 基线来换取通过（对照值本身有误时的做法见“Parity 体系”）。

- 编辑中只有需要快速定位编译错误时才单独 `cargo check -p <crate> --lib`。Clippy 按改动选择 `--lib` / `--bin <name>` / `--test <suite>`；测试专属改动无需扩大到 `--all-targets`。
- 同一代码、依赖、工具链、features、数据版本下已通过的检查复用结果；后续只改文档不使代码测试失效。失败后先重跑失败目标，只有新改动、失败或明确影响范围才扩大验证。
- Web 纯 TS/CSS 改动复用已构建 WASM；WASM 产物缺失，或 WASM 及其 Rust 依赖的源码、features、工具链改变时重建。`pnpm build` 已包含 typecheck，最终需要 build 时不额外重复 typecheck。E2E 前确认 dist 对应当前代码。
- **合并 / 发版、工具链 / Cargo features / workspace 依赖变更、影响范围不明的核心改动**需要完整门禁，优先交给 CI。发版由 tag CI 完成；其他场景先推送分支，再运行 `mise run ci <remote-ref>`，并检查该提交的结果。本地不再强制先跑 `full`；离线或排查 CI 时可显式运行 `mise run full`（仅 Rust 门禁，Web 另验）。完整 CI 在 tag、手动触发、发版 PR（workspace 版本变化）和每晚定时（master 当天有新提交时）运行；普通 PR 和 master 的每次推送自动运行的是轻量检查（[pr.yml](../.github/workflows/pr.yml)），不能替代完整门禁；发起 CI 不等于通过。相同代码不重复运行本地和云端全量门禁。
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

# 数值不对时的第一步：一个构筑的导入损耗、与构筑码自带参考值的差异、计算中丢掉的全部输入
mise run diagnose <构筑目录 | 构筑码文件 | 已入库构筑名片段>
# 构筑码对照它们自带的参考值（无需 vendor / luajit）：不带参数给出每组已入库构筑的总数，带目录给出该组每个构筑的差异
mise run accuracy [含 */code.txt 的目录]
# 从 poe.ninja 抓一组当前赛季构筑（每个升华一个，选择不依赖计算结果）
mise run builds:fetch <组目录> [--pick first|last]

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
- 根 `Cargo.toml` 的 dev/test 对第三方依赖使用 `opt-level = 3`（测试逐用例加载 JSON 数据快照，未优化的 serde_json / sha2 使一次加载从约 0.65 s 变为约 3.4 s）；workspace crate 保持未优化的快速编译，不要为“统一”去掉这项设置。dev/test 保留 `debug = "line-tables-only"`，默认 `incremental = false`，减少增量中间文件并继续复用未变的 Cargo 产物。修改源码后的重编译可能更慢；长期连续编辑时可显式启用 `CARGO_INCREMENTAL=1`。调试变量时可临时使用 `CARGO_PROFILE_DEV_DEBUG=2`（测试用 `CARGO_PROFILE_TEST_DEBUG=2`）。改变这些设置会生成另一组缓存，不作为日常准备步骤。
- 完整 CI 以已入库的 [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) 为准（PR 轻量检查见 [`pr.yml`](../.github/workflows/pr.yml)）：Rust 检查 fmt / Clippy / nextest / doctest，Web 独立检查类型、单测、Worker、WASM 构建与打包。E2E 默认关闭，仅 `mise run ci <ref> --e2e` 显式请求时运行；不要在未被要求时发起。涉及计算/Modifier/parser 的改动需补对应的集成测试或 golden fixture。

### 快速检查与发版

mise task 的日常命令、缓存、按需诊断、WASM 打包、Worker 测试和发版步骤统一见
[开发与 CI 工作流](development-workflow.md)；外部 PR 的处理见其中“处理外部 PR”一节。

- `mise run verify build skills support_gating::` 一次执行定向测试和 lint；`mise run targets` 列出测试目标，不编译。
- `bash devs/scripts/driver.sh full` 是完整 Rust 门禁，只用于上面列出的完整门禁场景，不是每次本地提交的收尾动作。
- PR 自动运行轻量检查（fmt / Clippy / 冒烟 / parity 回归；改动 `web/` 时加类型检查和单测）；完整门禁在 tag、手动触发、发版 PR 和每晚定时运行。发版 PR 的完整门禁通过后再合并、打 tag。


## Workspace 结构

当前结构以根 [Cargo.toml](../Cargo.toml)、各 crate 的 `src/lib.rs` 和契约/回归测试为准。workspace 有 14 个成员：7 个库、3 个应用入口、4 个 Rust 工具；`web/` 与 Lua oracle 独立于 Cargo workspace。桌面入口仍是占位程序。

`devs/docs/architecture/` 中的 18–21 文档记录具体功能契约。其余以本文件、[Web README](../web/README.md)、[数据管线说明](../pipeline/README.md) 和代码为准。

| Crate | 职责 | 主要依赖 |
|-------|------|------|
| `crates/pobr-data` | 纯数据定义，零逻辑、零 I/O，所有 crate 的底层依赖。核心是 `catalog.rs`（入库 JSON schema：`BaseItemDef`/`StatDef`/`ModDef`/`SkillGemDef`/`GrantedEffectDef`/`PassiveNodeDef`/`DataManifest` 等），另有 damage/build_config/display_stat/stat/monster 等域类型 | 无（外部依赖 `serde`） |
| `crates/pobr-core` | Modifier 解析/存储/聚合 + 计算引擎 + source-level 归因 + 来源接入（item/passive/gem/flask ingest）。零 I/O | `pobr-data` |
| `crates/pobr-gamedata` | 运行时数据 loader——数据系统里唯一持有文件 I/O 的层。把 `data/<poe_version>/` 入库 JSON 反序列化为 `pobr-data::catalog` 类型（`GameData::new(version_dir)`，按域懒加载 + i18n 边车；`repo_data_root()` 定位仓库数据目录） | `pobr-data` |
| `crates/pobr-i18n` | 语言包加载 / fallback / 显示文本映射；`en-US`（canonical）+ `zh-TW`，locale toml 经 `include_str!` 内嵌 | `pobr-data` |
| `crates/pobr-tree` | 天赋树拓扑、allocated node mod 收集、范围珠宝（first pass） | `pobr-data` |
| `crates/pobr-build` | Build 状态、PoB Build Code 编解码（XML ↔ zlib ↔ URL-safe Base64，padding 容错）、导入识别、`calc_orchestrator/`（`calculate_with_data` / `calculate_skill_metrics`）与 `CalcCache`、Build 对比。**parity 测试的主战场**（见下） | `pobr-data`/`pobr-core`/`pobr-tree`/`pobr-item`/`pobr-gamedata` |
| `crates/pobr-item` | raw item 文本的**全保真编辑态**解析 + 逆向序列化（`draft.rs`/`annotations.rs`/`build_raw.rs`）。职责边界清晰：**calc 视图**（剥标注 / variant 门控 / range 取值后喂引擎）由 `pobr-core::item_text` + `item::ingest_item` 承担；**编辑态视图**（保留 calc 刻意丢弃的 variant 名列表 / 行级标注 / 未建模标注，支持 BuildRaw 往返）由本 crate 的 `ItemDraft` 承担。复用 `pobr-core::mod_parser` 解析 modifier，避免规则重复 | `pobr-data` + `pobr-core` |
| `apps/pobr-cli` | CLI：`calculate` / `calculate-build` / `parse-mod` / `explain-mod` / `decode-code` / `encode-code`（`parse-item` 尚未实现，返回错误；命令逻辑在 lib，便于测试） | `pobr-build`/`pobr-core`/`pobr-data`/`pobr-gamedata` |
| `apps/pobr-wasm` | `build_api/` 提供 JSON 契约；默认 features 可在宿主测试，`wasm` feature 启用 wasm-bindgen 绑定 | `pobr-build`/`pobr-core`/`pobr-data`/`pobr-gamedata`/`pobr-item`/`pobr-i18n` |
| `apps/pobr-desktop` | 示例 build 计算与文本摘要；尚未引入 GUI 框架 | `pobr-build`/`pobr-core`/`pobr-data`/`pobr-i18n` |
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

### 数据与计算上下文约束

活动数据版本由 `pobr_gamedata::data_version()` 在运行时解析：`POBR_DATA_VERSION` → `POBR_DATA_ROOT/CURRENT`（默认仓库 `data/`）。两者都缺失时直接报错，代码里不内置任何版本号。测试和 PoB2 对照值一律针对活动版本：数据升级时用 `examples/demo-bd-test/tools/recapture_golden.py` 与 `recapture_panel_golden.py` 重采对照值，parity 掉格代表引擎与新版 PoB2 的差距，修引擎而不是降基线。仓库只保留活动版本加最近两个版本（`mise run data:prune`）；国际服与国服版本号规则不同，“最近”按快照入库顺序判断，不比较版本号。

schema 3 的 `manifest.json` 列出运行时文件及 SHA-256；`BuildData::load` 在可选域降级前校验完整快照。旧 schema 1/2 保留兼容行为。`patch/` 与 `overlay-common/` 是独立覆盖层，不属于快照哈希清单。正式数据和 Web 同步先完成候选生成/校验，再替换目录；失败恢复旧目录，不自动 bless golden。

`CalculationSession` / `Env` 只执行一次；再次 `perform` 返回 `AlreadyPerformed`。重算应创建新会话，避免再次应用资源转换。属性缩放与资源派生共享包含职业基础值及 INC/MORE 的最终属性；天赋关联的装备、品质和诊断查询都使用所选树。

Web 的计算、市场目录和天赋树使用初始化时固定的数据版本；重新初始化清除翻译与词缀层级缓存。节点规划逐节点计算，不能按原始词条文本共享包含位置/半径效应的结果。缺少历史树展示元数据时保留导入和计算，禁用不可靠的历史树编辑/规划。存档导入校验嵌套结构，非法存档不覆盖当前内容；启动失败保留原始字节并提供下载恢复。

CLI 完整构筑与 marginal 结果包含数据版本、树版本和未支持词条，调用方必须保留诊断，不能把未支持内容视为零效果。`calculate-build` 另有 `missing_items`（构筑用到但导入器读不出的物品，结果是缺了这件装备的角色）和 `reference`（与构筑码内 `<PlayerStat>` 的差异；仅在请求沿用导出条件时给出）。WASM 的 `{ pob_code }` 请求把同类物品放进 `item_errors`。

导入的损耗由 `pobr_build::ImportReport` 承载，与 `Build` 并列返回（`import_build*`）；`Build` 只描述计算输入，不携带诊断。`Build.jewels` 的每颗珠宝带 `JewelSocket`：树上自有插槽、被授予的插槽（Sinister / 含插槽的核心）或装备插槽，“珠宝插槽天赋效果”类词条只作用于第一种。

## 计算引擎架构（pobr-core）

目录按 Modifier 的生命周期拆分（见 [pobr-core/src/lib.rs](../crates/pobr-core/src/lib.rs)）：

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

`calc/session.rs::CalculationSession` 是底层会话入口；完整 Build 由 `crates/pobr-build/src/calc_orchestrator/mod.rs` 编排。该目录按 `skill/resolve.rs`、`item/weapon.rs`、`inject.rs`、`skill/buffs.rs`、`skill/triggers.rs` 等拆分技能解析、武器来源、注入和触发逻辑。`calc/perform.rs` 执行计算阶段，`display_catalog.rs` 定义可展示字段。

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

## 与 PoB2 的关系（逐步独立）

方向（项目所有者，2026-10）：PoBR 是独立实现的引擎，不是 PoB2 的移植。PoB2 的构筑码导入导出是要保留的产品功能；它的代码和数据是要逐步减少的依赖。“逐步”是约束的一部分：不做一次性重写，每一步都保持门禁通过。

新代码的做法：

- 机制以游戏自己的定义为依据实现：`data/<版本>/base/` 的 stat id、技能数据和游戏常量，词条文本，官方 patch notes。代码和测试的出处写这些（例如 `attack_damage_final_permyriad_per_rage = 100`），PoB2 只用于交叉核对。范例见 `pobr-core/src/calc/rage.rs`。
- 技能 stat 的含义写进 `pobr-core/src/rules/skill_stats.rs`：以游戏 stat id 为键，用引擎自己的 modifier 表达，出处是该 stat 的游戏描述文本。这张表先于抽取来的 `skill_stat_map.json` 查询，表里有的 stat 不再读抽取数据；抽取映射拒绝或缺失的 stat（`mise run diagnose` 第 4 节）优先在这里补，而不是去扩充按 PoB2 名称写的白名单。
- 不再新增“因为 vendor `X.lua:行号` 这样写”式的注释；行号随 vendor 升级失效，也说明不了游戏规则。改到旧代码时顺手把这类注释换成规则本身的描述。
- PoBR 与 PoB2 结果不同时，先判断谁符合游戏的词条文本和数据，不默认 PoB2 正确。已知的有意差异：
  - PoB2 用自己的符文表覆盖物品原文；PoBR 以物品原文为准。
  - Doedre's Undoing 的 Witchtoad 每 0.25 秒生成一只，PoB2 把这个间隔乘以施法时间，施法越快爆炸越多且没有上限；PoBR 按“区域数 × 1000 / 间隔”计算，与施法速度无关（见 `skill_stats::granted_skill_modifiers`）。
  - “Minions have 10% chance to inflict Withered on Hit” 在没有召唤物的构筑上，PoB2 把这条词条当作玩家自己能施加 Withered，并按配置的层数给敌人加混沌承伤；PoBR 把它交给召唤物，角色自己的技能、Presence 或已有的召唤物能施加 Withered 时才计入配置的层数（`calc/withered.rs`），所以这类构筑会比参考值低。每层 5%、最多 10 层两边一致。
  - Lament / Portent / Absent Amulet 授予的技能不保留精魄（游戏表 `ItemInherentSkills` 的 `NoReservation`，三个基底列在 `overlay-common/item_granted_skills.json`）。PoB2 导入角色时把这个技能再列成一个普通宝石组并按宝石保留 30 / 60 / 100 精魄，所以这类构筑码自带的 `SpiritUnreserved` 常为负数，而游戏里保留量不可能超过精魄总量。PoBR 把“排在组首、等级与授予等级相同、无品质、未腐化”的那个普通组认作被授予的技能本身，不计保留；其余同名宝石照常保留（`calc_orchestrator/granted_skills.rs`）。该技能上的辅助宝石追加的精魄也一并免除，PoB2 仍计入：依据是权杖授予的技能在游戏数据里以 `reservation_multiplier = -100` 表达“不保留”，辅助的追加量随之归零；这一点没有游戏内实测。
  - Cast on Critical 里的法术，PoBR 按宝石触发它的频率计算 DPS 和使用速率（`pobr-core/src/calc/energy.rs`）；PoB2 没有能量模型，它的触发配置要求同组有攻击技能（PoE1 的用法），找不到就退回法术自身施放速率，构筑码自带的参考值因此是自身速率口径。规则取自游戏数据和文本：能量上限是支援半的 `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`（每 0.1 秒基础施法时间 10 点），技能自带的 `total_cast_time_+_ms` 按术语表 Energy 条目计两倍（Comet 为 300）；每次暴击获得 `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit = 100`（每点敌人 Power 1 点）× 暴击伤害占敌人异常阈值的比例 × 能量获取修正（`energy_generated_+%`，即“Meta Skills gain #% increased Energy”）；独特怪 Power 恒为 20；被触发的技能不产生能量，所以充能来源是角色自己使用的其他伤害技能，取填满最快的一个，没有来源时法术保持自身速率。触发频率 = 来源技能每秒暴击次数 ÷ ⌈上限 ÷ 每次暴击能量⌉。**没有游戏内实测**，以下是假设：比例按“减免前暴击伤害 ÷ 阈值”线性计算且不封顶（玩家测得的公式，游戏未公布）；每次暴击取平均暴击伤害；每次使用按命中一次算；首领的异常阈值只按等级取表（游戏 stat 表里有 `ailment_threshold_pluspercent_final_for_map_boss`，取值不在已入库数据里）；获得能量时视为“近期造成过暴击”。其他靠能量触发的 meta 宝石（Cast on Elemental Ailment / Minion Death 等）尚未建模，仍按自身速率。`DataOrchestratorOptions::energy_triggers = false` 回到自身速率口径。
  - 图腾使用的技能（图腾宝石自己的技能，或插在法术图腾这类 meta 图腾宝石里的技能，游戏类型 `UsedByTotem` / `SummonsTotem`）按“角色有图腾、最近召唤过图腾”计算，施法和攻击不算角色自己的（不置 `CastSpellRecently` / `AttackedRecently`）。“每个已召唤图腾”类词条按 1 个图腾计，配置填了“已召唤图腾数”时按配置。PoB2 默认按图腾上限计，而它给法术图腾里的技能算出的上限不含宝石随等级增加的 `base_number_of_totems_allowed`（两个 tuned 构筑上是 2，按游戏数据 23 级以上的法术图腾是 3，再翻倍）。PoBR 还没有图腾上限模型，所以不假设达到上限（`calc_orchestrator/prepare.rs`）。

现状清单（`data/<版本>/` 各文件的 `_meta.generator` 可核对）：

| 来源 | 内容 |
|------|------|
| 游戏数据（GGG `.dat` → `pobr-data-adapter`） | `base/` 下的物品基底、词缀、stat、技能与等级、stat set（含无值 stat，如 `base_is_projectile`）、天赋树、宝石（含授予的效果与宝石标签，游戏表 `GemEffects` / `GemTags`）；`overlay/gem_quality_stats.json` |
| 从 PoB2 Lua 抽取（`sync-pob-catalog`） | 词条解析规则 `mod_parser_rules.json`、特殊词条 `generated/special_vendor.json`、`mod_scalability.json`、技能 stat 映射 `skill_stat_map.json` / `skill_overrides.json` / `stat_set_labels.json`、配置项 `config_options.json`、暗金 `uniques.json`、基底修正 `base_item_overrides.json`、符文 `runes.json`、`gem_effects.json`（宝石与效果的链接已由游戏表写进 `base/skill_gems.json`；此文件留给旧快照和几颗宝石共用的效果）、召唤物与幽魂、触发与幻影配置、`curse_priority.json`、`catalysts.json`，以及 stat 描述 `stat_descriptions.json`（来自 PoB2 的导出，不是直接来自游戏客户端）和由它派生的 `stat_id_map.json` |
| 按 PoB2 语义手工整理 | `overlay-common/` 的 `buff_definitions.json`、`special_mods.json`、`high_precision_mods.json`、`local_mods.json` |
| 按游戏数据手工整理 | `overlay-common/item_granted_skills.json`（游戏表 `ItemInherentSkills` 中 `NoReservation` 的基底；adapter 读到这张表后改为生成） |
| 回归基准 | `examples/demo-bd-test/builds/*/meta.json` 的对照值由 `tools/pob2-oracle` 重录 |

所以目前机制覆盖不会超过 PoB2，版本更新也要等 PoB2 的导出。建议的退出顺序（每一项单独立项）：stat 描述改为直接取自游戏数据；词条从“正则规则”改为“游戏 stat id → 引擎 modifier”的自有映射（可用现有 `stat_id_map.json` 一次性引导，之后新 stat 以“未映射”出现在审计里而不是等 PoB2）；技能 stat 映射和配置项冻结为自有数据后按游戏数据差异维护；回归基准增加游戏内实测值。

## Parity 体系（回归基准）

PoB2 对照是现有的回归基准（对照值不是真值，见上一节和“排查数值差异”），三层校验互补：

1. **`crates/pobr-build/tests/parity/ninja_parity.rs`** — 遍历 `examples/demo-bd-test/builds/*/`（真实 PoB2 build + `meta.json::player_stats` 黄金数值），零硬编码对比全部职业/技能；`parity_no_regression` 断言聚合命中率不低于基线。防御/属性与 DPS 分列报告，未完成的 offence 管线不会掩盖防御侧信号。基线只在一种情况下下降：PoB2 的对照值被游戏自己的数据证明有误、PoBR 有意与它不同（上一节“已知的有意差异”）；这时在基线常量旁写明是哪个构筑的哪一格、依据哪条规则，其余格子仍须全部命中。
2. **golden / dual-run 套件** — `crates/pobr-build/tests/parity/` 下的 golden 与 PoB2 对照用例，以及 `tests/dualrun/` 下的 statmap / config 对照，钉住中间值与配置语义。
3. **`tools/pob2-oracle`** — 需要逐分量定位偏差时，从 vendored PoB2 直接 dump Lua 侧计算分解对照。

与构筑码参考值比较的入口（`ninja_parity`、`reference_report`、`golden_canary`）统一用 `DataOrchestratorOptions::reference_conditions()`，它由两部分组成：`against_reference_enemy()`（EFFECTIVE 口径、Pinnacle 首领，引擎的模型全部开启）和 `on_reference_basis()`（关掉导出工具没有的模型：meta 宝石里的法术按自身速率）。PoBR 新增一个导出工具没有的模型时，把它的开关加进 `on_reference_basis()`，比较的两边才是同一个问题；模型本身由自己的测试验证，不靠参考值。`mise run diagnose` 的第 2 节按参考口径比较，其余各节（丢失的输入、读取并集）来自 `against_reference_enemy()` 下的计算，否则只有该模型读取的词条会被误报成“没有读取方”。CLI `calculate-build` 的 `output` 是游戏口径（`output.energy_trigger` 给出能量上限、充能技能和触发频率），`reference` 块在这种构筑上改按自身速率比较并以 `at_own_rate_of_use` 标明。

`crates/pobr-build/tests/` 下的 `codec.rs`、`skills.rs`、`parity.rs`、`dualrun.rs` 是 Cargo 集成测试目标，其子目录文件是用例模块；`--test` 使用汇总目标名。`pobr-core` 同样按 aggregation / parser / sources / engine / offence / defence / ailments / golden 汇总。

PoB2 对照值与逻辑回归都使用活动数据版本，没有单独钉住的对照版本；代码和测试不写具体版本号。依赖本地 vendor 的对照可能跳过，需如实记录，不能将跳过视为公式已获验证。

`vendor/PathOfBuilding-PoE2/` 是 [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的完整检出（版本见 `vendor/.pob2-version.txt`；gitignore 不入库，`bash devs/scripts/driver.sh vendor` 可按钉定 commit 重新克隆），公式核对直接读本地 Lua（`CalcOffence.lua`/`CalcDefence.lua`/`CalcPerform.lua` 等），不要去网上找。

## 排查数值差异（诊断闭环）

一个输入可能在四处丢失，引擎必须把每一处都记下来，而不是让数值悄悄偏小：

| 环节 | 丢失方式 | 记录位置 |
|------|----------|----------|
| 导入（XML → `Build`） | 物品块被拒、物品所在装备槽没有模型 | `ImportReport::skipped_items` / `unmodelled_slots` |
| 文本 → Modifier | 没有规则能解析的词条行（物品、珠宝、已分配天赋 `Tree:<节点>: …`、配置行 `Config: …`） | `unsupported_modifier_texts` |
| 宝石 stat → Modifier | stat map 有条目但引擎拒绝（未知 mod 名、不支持的 tag）；完全没有条目的 stat 多数是展示或动画数据，按所属宝石列出供查找缺失的机制 | `Diagnosis::unmapped_stats` / `unknown_stats` |
| Modifier → 计算 | 存入的名字从未被查询；依赖的 `Multiplier` 没有任何地方赋值 | `Diagnosis::unread_modifiers` / `unprovided_multipliers` |

`pobr_build::diagnose` 运行同一条计算管线并返回以上全部，外加实际计算的主技能及其等级（技能或等级选错不会丢输入，所以单独列出）。它打开 `ModDb::track_reads` 并以 Compare 模式记录 stat 映射，结果与普通计算逐位相同（`diagnosis_does_not_change_the_result` 对全部已入库构筑断言），但更慢，只用于排查。

排查顺序：

1. `mise run diagnose <构筑>`。第 1 节是导入损耗，第 2 节是与构筑码自带参考值的差异，第 3–8 节是丢掉的输入。多数差异在这里就能对上原因。
2. 清单解释不了的差异，再用 `tools/pob2-oracle` 逐分量对照钉定的 PoB2。
3. 参考值不是真值。构筑码内的 `<PlayerStat>` 是导出工具当时的结果（版本未知，可能早于当前数据）；PoB2 自身也有假设和缺陷。两边不一致时，以游戏自己的词条文本和数据判断谁对，再决定改哪一边。

“未被读取”是线索不是结论：攻击技能不读法术伤害是正常的。报告用全部已入库构筑的读取并集区分“任何构筑都不读（没有读取方）”和“本构筑的技能用不上”。

新增按名字消费 modifier 的代码用 `ModDb::named(name)` 或聚合查询，不要用 `iter_mods().filter(名字)`：后者不算读取，诊断会把有读取方的名字报成没有。确实在数据库之外消费的名字（按文本扫描、构筑结构）登记在 `diagnosis.rs::READ_OUTSIDE_THE_DATABASE` 并写明消费者。

宝石等级与品质加成（“+1 to Level of all Spell Skills”“+2% to Quality of all Skills”）按文本读取，不经过 ModDb：技能等级要在任何 modifier 存入之前确定。读取面是角色的全部词条来源：装备与珠宝、已分配和被物品授予的天赋、配置行（任务奖励、自定义词条），以及调用方传入的 `extra_modifier_texts`（由 `prepared_build_view` 并入构筑视图，主面板、逐技能总览和触发子计算共用）。新增一条文本通道时要同时接入 `skill::resolve::gem_property_bonuses`，否则这类词条在该通道里既不生效也不报未支持。被这样读取的行不计入未支持清单，也不再交给解析规则（规则会把“with an Intelligence requirement”误读成属性需求）。种类有两种写法，“all Cold Skills”和“all Nova Skill Gems”，含义相同。游戏按宝石标签判定种类，入库数据还没有宝石标签（`GemEffects.GemTags` 未进 adapter），目前用技能类型代替：种类的名字与技能类型不同的（Totem → `SummonsTotem`、Curse → `AppliesCurse`、Strike → `MeleeSingleTarget`、Crossbow、Elemental）列在 `skill_env::mods::kind_skill_types`。标签只来自宝石附带效果、技能类型里没有同名项的情形（如 Rain of Arrows 的 Projectile）目前匹配不到。

配置行指任务奖励与自定义词条。其中解析不了的行以 `Config: <行>` 记入未支持清单，其余行照常生效；任务奖励选项的 “None”（未领取）不是词条行。对任何计算输出都没有影响的词条在 `overlay-common/special_mods.json` 里登记为只识别、不产生 modifier 的条目（`"mods": []`、`"batch": "S0"`，例如游戏 stat `experience_gain_+%` 的 “5% increased Experience Gain”；其他批次的条目必须产生 modifier 或挂 handler，`pobr-gamedata` 的 `non_s0_has_traceable_provenance` 检查），它不是丢失的输入；尚未实现的机制不能用这种条目消掉，必须留在未支持清单里。

`examples/demo-bd-test/builds/` 是回归门禁语料，构筑抓取于 2026-06（上一赛季开服第一周），不包含之后的物品和机制；门禁全绿只说明这批构筑没有回退。当前赛季的构筑分三组，都用构筑码自带的参考值衡量：`examples/wasm-benchmark/builds/`（8 个）和 `examples/reference-builds/tuned/`（每个升华一个）用来找差异、修引擎；`examples/reference-builds/holdout/`（每个升华另一个）只测量，不对它运行 `diagnose`，也不按它的差异挑选要修的内容，它的总数回答“没人调过的构筑上有多准”。`mise run accuracy` 不带参数打印三组的总数。`reference_agreement_does_not_regress` 断言每组的吻合数（`TotalDPS` 吻合的构筑数、吻合的行数）不低于 `reference_report.rs` 里记录的基线。改动提升了吻合数就上调基线，不为通过而下调；数据版本变化使数字移动时按 `ninja_parity` 的做法重录。参考值被游戏自己的词条或数据证明有误时（例如导出工具当时没有实现某条规则），引擎按游戏实现，基线随“吻合了错误值”的行数下降，并在基线旁写明是哪些构筑、哪条规则、凭什么判断参考值有误。这个门禁防的是退步，不证明正确：参考值可能本身有误，构筑也可能因为错误的原因吻合。需要新一轮目标或新赛季开始时，用 `mise run builds:fetch` 重新抓取（见 [reference-builds](../examples/reference-builds/README.md)）：原来的 holdout 并入 tuned，新抓的一批作 holdout。

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

升级指引（市集 / 宝石 / 天赋规划）、构筑工作区、武器组与技能选择的行为契约见
[Web 功能契约](web-contracts.md)。改动 `web/src/lib/trade*`、各 planner、
`useBuildSession` / `buildWorkspace`、武器组或 PoB 导入导出前先读对应小节，行为变化时同步更新。
暂时隐藏的流派参考面板也记录在该文档末尾。
