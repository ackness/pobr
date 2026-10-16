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

# 数值不对时的第一步：基准组与 tuned 组的全部构筑一次对照参考值和钉定 PoB2，差异分类并跨构筑汇总原因
mise run triage [组目录 | 构筑目录 ...] [--jobs N] [--refresh] [--json <文件>]
# 游戏词条缺口队列：按 benchmark + tuned 中丢掉该输入的构筑数、玩家来源与来源数排序
mise run gaps [--json <文件>] [--limit N] [--kind unique|passive|mods|compound|jewel_node|bases]
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
词条审计（`tools/precompile-mods/src/audit.rs`）先用运行时的显示文本清理（`mod_parser::engine::strip_display_markup`）展开游戏链接与样式，再去掉物品注记：`[Thorns|Thorns]`、`[Strength]` 这类术语是词条的一部分，删掉会改变机制。
审计的 `text_channel`（`gem_property` / `granted_skill`）标出由其他文本入口读取的行（宝石等级与品质、授予技能），只说明这一行有去向，不说明授予的技能都已实现。

### 数据与计算上下文约束

活动数据版本由 `pobr_gamedata::data_version()` 在运行时解析：`POBR_DATA_VERSION` → `POBR_DATA_ROOT/CURRENT`（默认仓库 `data/`）。两者都缺失时直接报错，代码里不内置任何版本号。测试和 PoB2 对照值一律针对活动版本：数据升级时用 `examples/demo-bd-test/tools/recapture_golden.py` 与 `recapture_panel_golden.py` 重采对照值，parity 掉格代表引擎与新版 PoB2 的差距，修引擎而不是降基线。仓库只保留活动版本加最近两个版本（`mise run data:prune`）；国际服与国服版本号规则不同，“最近”按快照入库顺序判断，不比较版本号。

schema 3 的 `manifest.json` 列出运行时文件及 SHA-256；`BuildData::load` 在可选域降级前校验完整快照。旧 schema 1/2 保留兼容行为。`patch/` 与 `overlay-common/` 是独立覆盖层，不属于快照哈希清单。正式数据和 Web 同步先完成候选生成/校验，再替换目录；失败恢复旧目录，不自动 bless golden。

`CalculationSession` / `Env` 只执行一次；再次 `perform` 返回 `AlreadyPerformed`。重算应创建新会话，避免再次应用资源转换。属性缩放与资源派生共享包含职业基础值及 INC/MORE 的最终属性；天赋关联的装备、品质和诊断查询都使用所选树。

Web 的计算、市场目录和天赋树使用初始化时固定的数据版本；重新初始化清除翻译与词缀层级缓存。节点规划逐节点计算，不能按原始词条文本共享包含位置/半径效应的结果。缺少历史树展示元数据时保留导入和计算，禁用不可靠的历史树编辑/规划。存档导入校验嵌套结构，非法存档不覆盖当前内容；启动失败保留原始字节并提供下载恢复。

历史树显式转换由 `pobr_build::tree_convert::preview_conversion` 生成独立 XML；WASM `preview_tree_conversion_json({ code, all_specs })` 返回新分享码与逐 Spec 的保留、删除及共用／两组武器的断路清单。节点须同时匹配编号、种类与身份；两边有稳定 slug 时比较 slug，否则比较去除展示标记的旧名及是否属于升华，数值平衡不作为删除依据。缺少历史目录时报错。当前版本和未选中的 Spec 原文不变；保留存续插槽、武器组专属点、属性选择和不透明 XML，失效插槽的珠宝留在全局物品池。连通性使用当前树及珠宝授权规则；断路节点保留，等待显式修复或退款，修改后的完整分配仍须通过连通性与点数检查（`pobr_tree::allocation::AllocationTree::edit`）。

`node_power_json` 的可选 `nodes` 每次最多 16 个，返回单点试算的 `delta` 与 `distance: 0`，不替调用方寻找合法路线。省略时沿用热力图的邻接搜索，`max_depth` 默认 5、最多 12；整条路线与每点收益由 Web 的完整变体计算，见 [Web 天赋树契约](web-contracts.md#web-passive-tree-editing)。

CLI 完整构筑与 marginal 结果包含数据版本、树版本和未支持词条，调用方必须保留诊断，不能把未支持内容视为零效果。`calculate-build` 另有 `missing_items`（构筑用到但导入器读不出的物品，结果是缺了这件装备的角色）、`reference`（与构筑码内 `<PlayerStat>` 的差异，以及 `<MinionStat>` 与主技能召唤物的差异，行名 `Minion*`、`pob_key` 为 `Minion:<stat>`；仅在请求沿用导出条件时给出）和 `output.minions`（计算创建的每个召唤物：怪物 id、召唤它的技能、它所用的技能 `skill_id`、等级、DPS 与防御）；`--breakdown <stats>` 时还有 `breakdowns`（每个 stat 的计算步骤和读到的词条，与 Web 的分解同源，见 `pobr_build::breakdown`；`Minion:<stat>`（`Life`、`TotalDPS` 等）是主技能召唤物的同一张表，含角色给它的词条；`pobr_build::breakdown::BREAKDOWN_STATS` 所列的 stat 都另有逐步计算；`TotalEHP` 与 `<类型>MaximumHitTaken` 不再把敌人一侧的命中输入列成词条行：它们不能相加，EHP 用到的项见下面的 `output.ehp`，敌人输入及其来源由 `CalculationSession::enemy_hit_mods` 给出，CLI 不输出 WASM 的 `derived` 明细）和 `main_skill`；`--full-dps`（或请求 `FullDPS` / `FullDotDPS` 的分解）时还有 `output.full_dps`、`output.full_dot_dps` 与逐技能的 `output.full_dps_skills`，WASM 请求带 `full_dps: true`，未请求时没有这些字段，也不拿未计算的 0 与参考值比较；`output.ehp` 是 EHP 用到的项：逐类型的敌人伤害、承受的部分和要耗尽的资源池，以及受伤命中次数、伤害几率、减免后的命中次数和不被命中的几率；`--reference-basis` 时 `output` 与分解都按参考值口径（`reference_basis`）；`--usage` 时还有 `usage`（逐行台账，见“排查数值差异”），它总是来自游戏口径、全部模型开启的计算，不随 `--reference-basis` 改变。WASM 的 `{ pob_code }` 请求把同类物品放进 `item_errors`；WASM 的 `line_usage_json`（JS 名 `lineUsageJson`）接受与 `calculate_build_json` 相同的请求，返回 `{ usage, item_errors }`，不缓存。

WASM 的 `calculate_build_json`（JS 名 `calculateBuildJson`）响应另有 `minions` 与 `main_minion_index`。`minions` 是计算创建的每个召唤物：`index`、怪物 `minion_id` 与名字、召唤它的技能 `summoned_by`、它所用的技能 `skill_id`、等级、DPS、生命、护甲、闪避、能量护盾，以及已支持的 `stats`、`breakdowns` 与 `breakdown_descriptors`；每项是一个召唤物的快照，不乘召唤数量，`stats` 包含火、冰、电、混沌抗性及其最大值、超上限值（与角色相同的 `*Resist` / `*ResistMax` / `*ResistOverCap` id）；四种最终抗性有逐步分解。未建模的值不出现，也不补 0。`main_minion_index` 是主技能召唤的那一个，按提供该效果的宝石与召唤来源定位（`reference_check::main_skill_minion`），同名召唤宝石各有自己的结果，主技能组禁用时为空。请求的 `breakdowns` 选择要生成的明细：省略时是 `BREAKDOWN_STATS` 加每个召唤物的全部明细，`[]` 不生成任何明细，列表只生成所列 id（先去重，不认识的 id 是参数错误）；`breakdown_actor`（`{kind: "player"}` / `{kind: "minion", index}`，索引属于本次响应，越界是参数错误）选择计算对象，每份明细带 `actor`。数值与 `actor` 不随所请求的明细变化，逐技能总览与 Full DPS 的缓存忽略这两个字段。`breakdown_descriptors` 给每个可展示字段一种明细（`pobr_build::breakdown::detail::detail_kind`）：`steps`（逐步计算）、`sources`（每组是引擎的一次 ModDb 查询，`pobr_core::calc::session::detail::source_names`）、`derived`（公式与实际输入，`CalculationSession::derived_detail`）或 `none`。新增明细先声明种类，不为凑齐明细把上游输入包成可相加的词条行：来源组直接用计算阶段同一份 ModDb 查询、配置与 MORE 精度；派生项重用实际的计算项并给出公式输入，测试验证这些输入能重建结果；WASM 测试还要验证空选择与未请求的 id 不运行明细生成。`config_context`（Config 页按它筛选配置项）只在请求带 `config_context: true` 时计算并返回：它走读取跟踪的入口 `calculate_with_data_session_tracked`，在分解查询之前取快照；读取跟踪在 benchmark 构筑上最多慢 17.5%（Blood Mage），所以普通重算不跟踪、不返回该字段。这些字段在界面上的行为见 [Web 功能契约](web-contracts.md)。

装备的两瓶药剂和三个护符（含未勾选启用的物品）按槽位输出 `Flask<N>*` / `Charm<N>*`：恢复总量、立即恢复量、每秒持续恢复、持续时间、充能上限、每次消耗、满充能使用次数及每秒充能；`*ChargesGained` 是充能获取倍率。基底数字来自官方游戏表 `Flasks`（药剂与护符的 `LifePerUse`、`ManaPerUse`、`RecoveryTime`，后者为十分之一秒）及 `ComponentCharges`（`MaxCharges`、`PerCharge`），适配到 `base/base_items.json` 的 `utility`；护符持续时间也来自 `Flasks.RecoveryTime`，品质提升药剂恢复量或护符持续时间；`local_*` 对应的裸恢复、时间、充能词条只查询该物品，不能进入全域效果池。全域 `FlaskEffect`、`*FlaskChargeGen` 沿用 PoB key 并提供来源查询，逐槽位数字提供保留实际输入的派生公式；CLI 的 `utility_stats` 与 WASM 的展示字段使用同一结果。单次使用与充能不推算触发或覆盖率；启用时的额外效果沿用下文的生效约定。

明细覆盖分组侧栏的全部字段；仅目录中的 `NumberOfDamagingHits` 与 `NumberOfMitigatedHits` 为 `none`，两者来自资源耗尽模拟，尚未保留可展示的迭代轨迹。最大承伤明细复用已求解的值、敌人倍率和资源池上下文，不另跑求解器；来源组的 BASE / INC / MORE 只描述实际查询贡献，不承诺等于含角色基础值等其他输入的最终结果。`ChannelTimeToTrigger` 是所选 Cast While Channelling 组从空 Energy 到首次触发的秒数：被支持法术的总容量除以持续 Energy 获取速率，含宝石及全局增益；不改变 DPS、持续触发频率或返还模型（`calc::energy::channel_trigger_terms`）。目录不提供 `CausticGroundDPS`、`MirageCausticGroundDPS`、`SustainableTrauma` 或 `TrapCooldown` 标量；`SkillDPS` 的逐技能信息由技能总览和 Full DPS 贡献行承载。

导入的损耗由 `pobr_build::ImportReport` 承载，与 `Build` 并列返回（`import_build*`）；`Build` 只描述计算输入，不携带诊断。`Build.jewels` 的每颗珠宝带 `JewelSocket`：树上自有插槽、导入时记下的已分配天赋（`JewelSocket::Node`，构筑码 `<Socket nodeId>` 的节点）、被授予的插槽（Sinister / 含插槽的核心；珠宝的 “Allocates <插槽>” 同样打开它，例如 Megalomaniac 的 “Allocates Zarokh's Gift”）或装备插槽，“珠宝插槽天赋效果”类词条只作用于树上的珠宝插槽天赋。节点不是珠宝插槽时，按节点自己的词条：Lich 的 Crystalline Phylactery（“Can Socket a non-Unique Basic Jewel into the Phylactery”、“100% increased Effect of bonuses gained from Socketed Jewel”，[poe2db Lich](https://poe2db.tw/us/Lich)）让其中的珠宝按两倍计，The Adorned（“Jewel Socket Passive Skills containing Corrupted Rare Jewels”）不作用于它（`calc_orchestrator::collect::jewel_socket_increase`）。超越肢体（PoB 的 `Arm 1/2`、`Leg 1/2`，游戏物品类 `IncursionArm` / `IncursionLeg`）是与装备并列的一组栏位 `Build.limbs`（按 `LimbSlot`）：肢体的 stat 都是全局的，词条在装备之后注入并按栏位归因（`item.arm1.implicit`），宝石等级加成和 “Allocates” 等按文本读取的通道也包含它们。`Build.weapon_set` 是 `items` 里那组武器所属的武器组，另一组的武器在 `Build.other_weapons`。导入时取主技能使用的那一组：武器授予的技能要拿着那把武器才能用（构筑码把它再列成普通组时，按“排在组首、等级与授予等级相同、无品质、未腐化”认出），设为只在一组使用的组用那一组，其他主技能取当前物品集的 `useSecondWeaponSet`；树上的武器组专属天赋随之取那一组（`xml_build::calculation_weapon_set`）。主技能用的不是物品集那一组时，两组都能用的组绑定到物品集那一组，即角色平时拿着的武器。WASM 请求带 `weapon_swap` 时取它的 `active`，`alternate_items` 是另一组的武器。绑定到另一组的技能组（`SocketGroup::weapon_set`，另一组武器授予的技能也绑定到那一组，`granted_skills::mark_item_granted_skills`）按另一组的武器计算宝石等级与品质（`skill::resolve::apply_other_weapon_set_gem_bonuses`），其余计算仍按当前组。编排据此设置 `WeaponSet1` / `WeaponSet2` 条件，“使用武器组 1/2 时”的词条（例如 Rigwald's Ferocity 的 `support_rigwald_*_in_weapon_set_*`）只在对应的组生效。

构筑码的 `mainActiveSkill`（从 1 起）是技能组里可选效果的序号：每颗宝石的授予效果按游戏表 `GemEffects.GrantedEffectDisplayOrder`（`GemEffectDef::granted_effect_display_order`）排列，未列出的接在后面，再去掉辅助效果；辅助宝石授予的主动效果仍计入（`skill_env::mods::group_active_effects`）。宝石的 `skillId` 是它的主效果，不一定是序号 1：弩弹宝石的 1 是射击、2 是装填，旗帜宝石的 1 是旗帜、2 是保留效果，幻影宝石先列生成幻影的效果、再列 meta 主效果。导入、计算、宝石目录和导出共用这一顺序，导出保留原序号；没有显示顺序的旧快照按主效果、附加效果的顺序。构筑码里 `<Gem enabled="false">` 的宝石保留在 `Build` 里（`GemSkillRef::enabled`），准备计算视图时才移除（`calc_orchestrator::stage_build_view`）：辅助判定、技能、buff、精魄保留、触发与召唤物都看不到它，序号只数剩下的效果。

精魄保留按当前计算的武器组计：绑定到另一组的技能（包括它的辅助宝石追加的保留量）不保留，游戏 stat `display_skill_reserves_in_all_weapon_sets` 明确要求在所有武器组保留的技能除外，例如 Blink、Convalescence 和 Eternal Rage。Svalinn 授予的 Cast on Block 本身没有基础保留量，Direstrike II 与 Herbalism II 的 40 和 30 精魄只在拿着该盾的武器组计入，再应用保留效率。tuned Titan His Scattering Calamity 的主技能在第一组，其他常驻技能绑定到平时持有的第二组，因此计算第一组时保留为 0；构筑码参考值按另一组保留，与当前钉定 PoB2 不同。这里只改变保留量：绑定到另一组的常驻技能的 buff、光环仍按生效计算，与 PoB2 和构筑码参考值一致；游戏里切换武器组后这类 buff 是否保持，没有找到说明，也没有游戏内实测。

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
`LifeOnKill`, `ManaOnKill` and `EnergyShieldOnKill` report flat recovery plus
percentages of their final maximum pool per kill, respecting recovery blocks;
they do not assume a kill frequency. `DazeChance` reports the selected hit's
chance, capped at 100%. Freeze buildup reads `FreezeBuildup` alongside the
existing enemy/immobilisation modifiers. Fully Broken Armour removes Armour
and makes non-player targets take 20% increased Physical damage from Hits;
`FullyBrokenArmourEffect` displays that percentage after effect modifiers,
and it affects hits only when the enemy is configured as fully broken.
These fields expose source details through the display catalog.

伤害型异常（流血、点燃、毒）的同时层数 = 命中率 × 施加几率 × 持续时间 × 每秒命中数，持续时间是基础时间乘持续时间加成（含 “increased duration of Ailments you inflict”，`EnemyAilmentDuration`）与敌人身上效果的过期速度，再除以 “Damaging Ailments deal damage #% faster”：伤害总量不变、造成得更快，每份结束得更早（`calc/perform.rs`，分解里写作 “faster /N”）。

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

几条暗金词条接到已有的计算入口（`overlay-common/special_mods.json`）：Blackheart 的 `base_armour_%_applies_to_chaos_damage` 让这一比例的护甲也用于混沌命中的减伤；Vigilant View 的 `maximum_chance_to_evade_is_50%` 与 Goregirdle 的 `maximum_physical_damage_reduction_is_50%` 限制闪避率与物理命中减伤；Matsya 的 “Skills reserve 50% less Spirit”（`unique_spirit_reservations_are_halved`）乘在保留量上，不是保留效率；Plaguefinger 的 `all_damage_can_poison` 让元素命中也计入中毒的伤害，不提供中毒几率；`ignore_hexproof`（“Curses you inflict can affect Hexproof Enemies”）让诅咒越过敌人的 Hexproof；Venopuncture / Icefang Orbit（`all_damage_to_bleeding_enemies_contributes_to_chill` / `all_damage_to_poisoned_enemies_contributes_to_chill`）只在敌人流血 / 中毒时让全部命中伤害计入冰缓强度。Parry 的 “increased Parry Damage”（`parry_damage_+%`，Silverthorne 与天赋）和 Kaltenhalt 的 Parry 物理转冰（`parry_physical_damage_%_to_convert_to_cold`）只作用于 Parry。Chober Chaber、Trenchtimbre、Crown of Eyes 的 “increases and reductions … also …”（`minion_damage_increases_and_reductions_also_affects_you`、`additive_modifiers_to_minion_attack_speed_also_affect_you`、`additive_spell_damage_modifiers_apply_to_attack_damage`）只复制相应的 INC 词条：召唤物伤害、召唤物攻击速度、法术伤害分别给角色伤害、角色攻击速度、攻击伤害，保留来源和条件，不复制 MORE / LESS、点伤和只限定某类召唤物的词条（`calc::shared_modifiers::apply_shared_increases`）。

元素曝露在角色的防御算出之后归并（`calc::setup_env::reduce_enemy_exposure`，由 `perform` 调用）：Refraction III 的 “Banner applies {0}% Elemental Exposure per 1000 Armour”（`support_tempered_valour_banner_applies_%_elemental_exposure_per_1000_armour`，旗帜插下时）按角色护甲的每满 1000 点计，与其他曝露一起取最强的一份，再按敌人身上的曝露效果缩放。配置的 “Is the enemy Exposed to Fire?” 是角色自己施加的减益，只在角色能施加该元素的曝露时成立（条件 `CanApply<元素>Exposure`，`setup_env::with_exposure_sources`）：某个技能或辅助它的宝石施加曝露（`skill_env::groups_inflict_exposure`），角色有 “Inflict Fire Exposure on Hit” 这类词条，旗帜按护甲施加曝露，或敌人身上已有其他来源的曝露。没有来源时配置不生效，与 PoB2 相同（它在没有来源时隐藏这个选项）。技能自己施加的减益由 `skill_stats::debuff_modifiers` 按游戏 stat 定义，先于抽取的 stat 映射，品质加在同一个数值上（两条同名曝露只取最强的一条，分开会丢掉品质）：Oil Barrage 的油地面施加曝露（`skill_base_oil_exposure_-_to_total_elemental_resistance`，20，品质每 1% 加 0.25），假设敌人站在油里。Crushed 使目标的物理伤害减免降低 15，可以降到负数，目标随之承受更多物理伤害（减益的 stat `base_additional_physical_damage_reduction_%`，[poe2db Crushed](https://poe2db.tw/us/Crushed)；`setup_env` 的敌人基础词条，条件 `EnemyCrushed`，由敌人的 `Condition:Crushed` 或配置设置）。Styrn's Ferocity 的 “Supported Skills Crush targets they Hit for 8 seconds”（`support_knights_ferocity_crush_on_hit_ms`）不论辅助哪个会命中的技能，都让敌人处于 Crushed（`skill_env::skill_crushing_on_hit`）：Raise Shield 打出的命中让 Shield Wall 打的敌人被 Crush，假设 8 秒内用过它。

Culling Strike（Reap 的 `kill_enemy_on_hit_if_under_10%_life`，游戏文本即 “Culling Strike”）在敌人生命降到按稀有度取的阈值时击杀（游戏常量 `culling_strike_<稀有度>_threshold`：普通 35%、魔法 20%、稀有 10%、独特 5%），“increased Culling Strike Threshold” 提高阈值。需要打掉的只是阈值以上的生命，所以 CombinedDPS 乘 100 ÷ (100 − 阈值)（`calc::skill_dot::cull_multiplier`），`TotalDPS` 不变，侧栏的 `CullingDPS` 是这一倍率额外贡献的 DPS。Detonate Living 的 `spells_have_culling_strike`（“Your Spells have Culling Strike”）只给法术。

内置 buff（Arcane Surge、Onslaught 等，`overlay-common/buff_definitions.json`）都是战斗中的 buff：配置项 “Do you have Arcane Surge?” 写入的条件带 “Combat” 限定，计算在战斗口径下时成立（`rules::buff_expander::expand_misc_buffs`）。词条写明拥有该 buff 时同样触发：“You have Unholy Might” 写入同名条件 `Condition:UnholyMight`；只对某类伤害成立的条件不算，“30% chance to gain Onslaught for 4 seconds on Hit” 带命中标志，是否拥有由配置决定。buff 自己写入的 Multiplier（Unholy Might 的量级 100，“Gain 30% of Damage as extra Chaos Damage” 按它计）在展开后并入 multiplier 表。武器上的 “+N to Accuracy Rating” 是游戏的本地 stat `local_accuracy_rating`，只计入用这把武器的攻击（`ingest::item::apply_weapon_hand_conditions`）：Shield Wall 用盾攻击，主手武器的命中值不计入。写明 “this Weapon” 的词条（“Attacks with this Weapon Penetrate 15% Elemental Resistances”）解析时带待定的手别条件 `{Hand}Attack`，由武器所在的栏位定为主手或副手（同一函数）。“Damage with Maces” 这类武器类型伤害和主手武器的类型标志只作用于用武器攻击的技能：技能类型有 `Attack` 而没有 `NonWeaponAttack`（`conditions::attacks_with_weapon`）。Shield Wall 与 Raise Shield 用盾自己的伤害攻击，法术不用武器，都不计入；“while wielding a Mace” 这类持有条件不受影响。Fire Spell on Hit 这类 meta 宝石触发的技能带 `no_cost`，技能本身没有消耗，meta 宝石仍保留精魄。

技能的无值 stat `global_always_hit`（“Your Hits can't be Evaded”，掌套的隐式 stat 也是它）让命中不检查闪避（`CannotBeEvaded`），暴击也不再按命中率折算。Mace Strike 的 `skill_double_hits_when_dual_wielding` 在双持时两把武器各命中一次：DPS 按两次命中计，每次命中的伤害与暴击率不变。所选 stat set 带 `base_deal_no_damage` 时该效果不造成伤害（`DealNoDamage`），武器与附加伤害也不能把 buff、放置动作或技能容器变成命中：Conductive Runes 的放置、Rolling Slam、Spearfield、Tempest Bell 的主 stat set 与 Hollow Focus 的 Bells 为 0，伤害分别在同一技能另选的 Explosion、Slam、Hazard、Shockwave、Nova stat set。Explosive Demise 引爆生命低于阈值的友方召唤物（`command_minion_target_ally_below_x_life`），没有召唤物时不造成伤害（以上见 `rules::skill_stats::skill_modifiers`）。Raise Shield 与 Parry 用盾牌的伤害攻击（`replace_off_hand_unarmed_attack_stats_with_shield_type`，盾牌每满 N 点护甲或闪避为一组，两者的 N 为 5），主手武器不提供伤害或攻击时间（`skill_env::weapon::weapon_contribution`）。Raise Shield 的说明 “Release immediately after Blocking when an enemy is close to perform a Shield Bash” 要求先格挡，它的 250 毫秒最短引导与 35 毫秒释放是举盾的过程、不是盾击频率，所以不走引导周期，按自己 550 毫秒的攻击时间计，假设每次能盾击时都有一次可格挡的命中到达；Parry 没有引导阶段的 stat，按等级行的 750 毫秒（`base_parry_duration_ms` 是敌人身上减益的持续时间）。这两条**没有游戏内实测**，不模拟敌人的攻击或格挡频率。

弩的射击效果读取同组弹药的 `base_number_of_crossbow_bolts` 和 `active_skill_reload_speed_+%_final`，装填速度的品质与等级值先相加（`skill_env::mods::crossbow_reload_modifiers`）。Requiem 的 `crossbow_skill_has_no_ammo` 使激活期间不消耗弹药，也不计装填时间。Magma Spray 的 `off_hand_weapon_*_fire_damage` 与 `off_hand_*_added_fire_damage_per_15_shield_armour/evasion` 使用盾牌自身防御的每满 15 点，技能伤害倍率只乘一次（`skill_env::weapon::shield_elemental_damage_modifiers`）。Chayula's Will 的 `archon_of_chayula_tornado_attack_interval_ms = 200` 表示单个旋风每秒攻击 5 次；The Stars Answer / Walking Calamity 的陨石读取主效果 `walking_calamity_spawn_meteor_every_ms = 300 / 150`，不受攻击／施法速度影响（`rules::skill_stats::skill_modifiers`、`calc_orchestrator::skill::mods::skill_timing_modifiers`）。假设全部落点命中，不混合每第 15 发强化形态，不计多目标频率加成或旋风叠加数量；**没有游戏内实测**。

Snap 的 `snap_damage_+%_final_if_created_from_unique`（“{0}% more damage with Hits for blasts originating from a Unique enemy”）只加命中伤害，把所计算的独特敌人当作爆炸源头。Flameblast（`pobr-core/src/calc/flameblast.rs`）每层按 `charged_blast_spell_damage_+%_final_per_stack` = 75 加更多伤害，层数取 multiplier `FlameblastStage`（默认 1，最多 `flameblast_maximum_stages` = 10）；一次使用是每层施法时间 × 层数与 `base_minimum_channel_time_ms`（490 毫秒）中较大的一个，再加 `channel_skill_end_animation_duration_override_ms`（600 毫秒），仍受冷却限制。层数由配置输入 `skillFlameblastStages` 设置（Web 技能页的层数输入，`config_resolve::resolve_config` 写入 `FlameblastStage`，留空沿用默认）；PoB2 构筑码里自己的层数选择还没有接入。

配置项进入计算的方式各不相同，有 handler id 不等于选项已生效。破甲（`conditionEnemyArmourBroken` 与破甲量 `multiplierArmourBreak`）由 `calc::armour_break::apply_armour_break` 在有效 DPS 口径下从敌人护甲中扣掉所填的量，没填时全部扣掉，“You can Break enemy Armour to below 0” 时下限为原护甲的相反数；没勾选时数值框里的残留不生效。致盲与 Elusive 的效果覆盖（`overrideBuffBlinded` / `overrideBuffElusive`）按百分数读取，不超过角色能施加的致盲、能得到的 Elusive 效果（`calc::blind`、`rules::buff_expander`）。Inexorable Critical 按配置的距上次暴击秒数（`SecondsSinceInevitableCrit`，不到 4 秒时另置 `CritRecently`）乘每秒的加成，受 `support_inevitable_criticals_*_cap` 限制（`calc::crit::apply_inexorable_critical`）。Dark Pact 的目标骷髅生命（`darkPactSkeletonLife`）乘 `skeletal_chains_aoe_%_health_dealt_as_chaos_damage` 为基础混沌伤害，Herald of Ash 的过量伤害（`hoaOverkill`）乘每分钟的燃烧比例 `herald_of_ash_burning_%_overkill_damage_per_minute` 为火焰持续伤害（`calc::configured_damage`）。

敌人受击配置（`calc_orchestrator::enemy_config::modifiers`）以 OVERRIDE 覆盖等级与敌人类别的预设：五种伤害（`enemyPhysicalDamage` 等）、三种元素穿透、暴击率、暴击伤害和物理压制（`enemyPhysicalOverwhelm`）；显式的 0 同样覆盖，负数按 0，非有限值忽略；来源标为 `EnemyConfig`、id `config.<输入名>`，被覆盖的预设行在 TotalEHP 与最大承伤的敌人输入（`CalculationSession::enemy_hit_mods`）里保留但不计入。`enemyDamageType` 选择命中类别（平均、无类型、近战、投射物、法术、法术投射物；持续伤害不是命中），决定闪避、格挡与承伤的上下文（`calc::taken::HitCategory`）；`EHPUnluckyWorstOf` 按 1 / 2 / 4 次中最差的结果估计防御概率。`enemySpeed` 只在 Energy 格挡事件缺少来袭频率时提供回退，不是 EHP 的通用伤害倍率；`enemyDamageRollRange` 和持续伤害的防御视图还没有读取方。敌人等级只解析一次（`calc_orchestrator::skill::mods::resolved_enemy_level`），尸体生命与敌人的减伤共用：调用方显式给的等级优先，其次构筑码 `enemyLevel` 的 Input、Placeholder；都没有时普通怪物和普通首领跟随角色等级，Pinnacle / Uber 取该档的默认等级（82），最后限制在该档的最低等级与敌人最高等级（85）之间。尸体爆炸按这一等级取怪物生命表（82 级 32956，85 级 36012），例如 Detonate Dead 的 `corpse_explosion_monster_life_permillage_physical` = 107 是 10.7%。

侧栏的伤害视图以稳定 ID 区分：`TotalHitAvg` 是角色一侧的平均命中；`AverageDamage` 计入敌人减伤与命中概率，`AverageBurstDamage` 再计一次使用中的重复攻击；`CombinedAvg` 与 `WithIgniteDPS` / `WithBleedDPS` / `WithPoisonDPS` 按技能的平均伤害或 DPS 口径组合；`IgniteDamage` 等异常总伤害是该异常的 DPS 乘有效持续时间，不是叠层 DPS 再乘时间；`MainHandAccuracy` / `OffHandAccuracy` 各读自己那只手的作用域。技能数据的四个持续时间通道进入 `Duration`、`DurationSecondary`、`AuraDuration`、`ReserveDuration`，通用的技能持续时间加成只应用一次。范围半径按内部距离单位计算，`AreaOfEffectRadiusMetres` 除以 10 显示米；`PierceCount`、`ForkCount`、`ChainMax`、`SplitCount` 与投射物行为解析共用禁止与转换规则，几率产生的次数可以是小数，无限穿透沿用 100。弩的 `FiringRate` 是装填前的射击速率，`ReloadTime` 是立即装填几率折算前的装填时间。可展示字段的 id 与 PoB key 相同，只有 `TotalTotemPlacementTime`（PoB key `TotemPlacementTime`）例外。

侧栏其余的机制视图由 `calc/sidebar.rs` 在已算出的结果上取值：

- `EffectiveMovementSpeedMod` 含装备基底的移动惩罚（游戏表 `ArmourTypes.IncreasedMovementSpeed`，`ArmourMovementPenalty`）与行动速度。`MovementSpeedWhileUsingSkill` 只对能在移动中使用的技能（技能类型 `UsableWhileMoving`、stat `can_perform_skill_while_moving`）计算：移动速度乘该技能使用时的速度（`movement_speed_+%_final_while_performing_action`），`movement_speed_penalty_+%_while_performing_action` 与 Mobility / Deliberation 的 `support_mobility_movement_speed_penalty_+%_final_while_performing_action` / `support_deliberation_movement_speed_penalty_+%_final_while_performing_action` 改变的是这一惩罚，不是角色的移动速度。
- `PresenceRadiusMetres` 的基础半径是 4 米（0.2.0 起，[官方公告](https://www.pathofexile.com/forum/view-thread/3740562)），只读 Presence 自己的范围与半径（例如 “Presence Radius is doubled”，`unique_double_presence_radius`），不读技能范围。
- `AuraEffectMod` / `CurseEffectMod` 在主技能是光环 / 诅咒时给出它的效果：全局加成加上该技能与其辅助宝石自带的 `aura_effect_+%` / `curse_effect_+%`。后者记为 `Selected*` 名字（`SelectedMagnitude`、`SelectedCurseEffect` 等）：buff 阶段已把宝石自身的效果算进光环和诅咒，不能再放大一次。
- `ActiveMinionLimit` 是主技能实际创建的召唤物上限（`SelectedMinionLimit`，按召唤技能名限定，其他技能的召唤物不进入），没有上限 stat 的召唤物（例如同伴）为 1；Wolf Pack 读 `skill_wolf_pack_size`（宝石 1 / 18 / 20 级为 3 / 5 / 6 只狼）。
- Blood Price 的 “Enemies in your Presence have at least #% of Life Reserved”（`unique_blood_price_enemies_in_presence_have_at_least_%_life_reserved`，敌人的 `LifeReservationPercent`）在有效口径下取最强的一份，几份不相加。只需打掉未被保留的生命，所以 CombinedDPS 乘 100 ÷ (100 − 保留比例)，单次命中与异常的伤害不变；`ReservationDPS` 是这一倍率额外贡献的 DPS（`perform::fill_skill_dot_stage`，与 `CullingDPS` 同理）。
- `BurningGroundDPS`（Ball Lightning、Incinerate、Oil Grenade、Spear of Solaris 的点燃地面 stat set 取已算的点燃，直接造成燃烧地面的 stat set 取技能自己的持续伤害）、`DecayDPS`（Profane Decay，`TriggeredDecayingHexPlayer` 自己的持续伤害）、`MirageDPS` 与 `MirageBurningGroundDPS`（所选 stat set 带 `skill_disabled_unless_cloned`，即技能只由复制体使用，例如 Mirage Archer 的幻影）都是已算伤害的分类视图，不能再加进 CombinedDPS。
- `ExplodeChance` 是 “Enemies you kill … have a #% chance to explode” 类词条的几率（`EnemyExplodeChance`，上限 100%），爆炸伤害还没有计算；`CombinedAvgToMonsterLife` 是 `CombinedAvg` 占敌人等级在怪物生命表里的生命的百分比。
- `CritBifurcates` 与构筑码里同名参考值同单位，是倍率 1 + c / (2 − c)：c 是一次掷骰的暴击几率（乘命中几率，Lucky 先计），c / (2 − c) 是暴击的命中里两次暴击掷骰都成功的比例（`attacks_roll_crits_twice`），两把武器各算一次后取平均；不在有效口径或没有这一机制时为 0。比较直接用倍率，Web 显示为 (值 − 1) × 100%，例如 1.33333 显示 33.3%。

`ReqStr` / `ReqDex` / `ReqInt` 取全构筑装备与宝石需求的最大值，再与各颜色辅助宝石累计的需求比较（`calc_orchestrator::requirements::requirement_snapshots`）：装备先转换基底需求，再加本地固定需求，乘本地百分比修正并向下取整，最后应用全局修正（Runeforged Leaden Greathammer 的 60 力量、50 智力，转换 20% 到敏捷后再加 81 智力，得到 48 / 22 / 121；三倍武器需求后为 144 / 66 / 363）；宝石按原生等级计，腐化与装备给的技能等级不抬高需求；物品授予的技能和被认作它的普通组不新增一颗宝石的需求（见“已知的有意差异”）。消耗按资源分固定、百分比与每秒三种通道；每秒消耗按使用、触发、装填或放置的频率计算，多发投射物不另付消耗（`perform::fill_resource_cost_rates`）。技能自带每秒消耗时，该资源的持续消耗直接取这一通道，不再把启动消耗按攻击频率叠加：Oil Barrage 是 Wyvern 引导技能，不是弩弹药；27 级的 69 魔力启动消耗与每分钟 4049 魔力在 110% 辅助倍率下分别为 75 与每秒 74。图腾放置用召唤技能自己的游戏 `cast_time`（`calc_orchestrator::skill::mods::skill_timing_modifiers`，陷阱、地雷与战吼同理），施法速度与图腾放置速度分开。不是攻击、也没有专属速度的技能（例如 Wildwood's Gifts，`AncientGiftsPlayer`，只有 1000 毫秒的施法时间、没有 `Spell` 类型）按施法速度加技能速度计算使用时间（`skill_use_time::speed_names_for`）；战吼、图腾、陷阱与地雷仍用各自的速度。游戏把这类技能的基础时间标为 “Cast Time”，PoB2 与构筑码参考值也这样计，**没有找到游戏对施法速度作用范围的正式说明，也没有游戏内实测**。`ChannelTime` 只含引导阶段，不含释放与冷却。

Spell Echo's `echoed_spell_area_of_effect_+%` increases the area of echoes only.
The `repeatMode` Average view weights that increase by echoes / total casts;
Final and Final DPS use the full bonus, and None uses the original cast's area.
For level-20 Frost Bomb (24 + 7 radius units), Spell Echo (one echo, 40% increased
area) and Spell Cascade (20% less area) give Average area ×0.96 and radius 3.0 m;
the original cast is 2.7 m and the echo is 3.2 m. This display convention does not
apply Light Radius modifiers to skill area (`calc::skill_mechanics::calc_aoe`).

恢复（`pobr-core/src/calc/recovery.rs`）分三层：原始再生；经过再生限制与直接恢复后的恢复（`LifeRegenRecovery` 等，含 Runic Ward 与怒气，来自 `base_ward_regeneration_per_minute` “Regenerate 1 Runic Ward per second”、`base_rage_regeneration_per_minute` 等游戏 stat）；扣掉构筑自身损耗后的净恢复（`NetLifeRegen`、`NetManaRegen`、`NetEnergyShieldRegen`、`NetWardRegen`、`TotalNetRegen`）。角色固有的魔力再生是 `character_inherent_mana_regeneration_rate_per_minute_%` = 240（每秒 4% 最大魔力），“No inherent Mana Regeneration” 只去掉这一份（`calc_orchestrator::inject::inject_character_base`）。自身损耗是资源流失（“Lose … per second”，不经减免）、自己受到的类型化持续伤害（经抗性与承伤修正）和主技能的每秒消耗（百分比消耗按资源池换算）：生命、魔力、能量护盾各扣自己的消耗，`NetWardRegen` 是 Runic Ward 的恢复减去它的每秒消耗，`TotalNetRegen` 是这四种净恢复之和，怒气消耗只进入 `TotalBuildDegen`；敌人的持续伤害不计入。Berserk 的 `skill_base_life_loss_%_per_minute_per_rage_while_not_losing_rage_to_apply` = 6 是每点怒气每秒流失 0.1% 最大生命。偷取与命中获得（`LifeLeechGainRate`、`LifeLeechGainPerHit` 等）按[游戏关键词](https://poe2db.tw/us/Keywords)使用一秒基础持续时间，不再把每次偷取量限制为资源池的 10%。[0.5 补丁说明](https://www.pathofexile.com/forum/view-thread/3932540)中的单次命中 40,000 伤害上限已实现：命中总伤害超过上限时，各伤害类型按相同比例缩小，再按各自的偷取比例计算，不能用额外元素伤害补足物理偷取。`base_life_leech_amount_+%` / `base_mana_leech_amount_+%` 的“increased amount of Life Leeched” / “increased amount of Mana Leeched”增加偷取量；`base_life_leech_rate_+%` 等“Leech Life 1% faster”加快速率，不增加总量。最大偷取速率先应用增加/减少与更多/更少修正，再限制实例速率；生命和魔力每秒 20%、能量护盾每秒 10% 的默认上限仍是项目保留的兼容约定，并非 0.5 补丁说明确认的游戏常量。按主技能平均命中频率估计实例覆盖时间，优先计算恢复速率最高的实例，较慢实例只填补其过期后留下的时间，不模拟随机暴击的时间序列；命中获得与瞬间偷取仍按每次命中相加。PoB2 在没有自身类型化持续伤害时不给出净恢复字段，PoBR 照常计算。

Full DPS（`calc_orchestrator::full_dps::calculate`，`DataOrchestratorOptions::full_dps`，默认关闭）是构筑码中勾选 `includeInFullDPS` 的组里每个启用效果的持续输出之和，不模拟技能轮转。数量取组的 `groupCount`，没有时取宝石的 `count`（可为小数，不是正的有限数时不计入）；每个效果按自己的武器组、天赋、辅助与触发环境计算，召唤物行用该召唤宝石创建的召唤物；物品内建触发技能的自动行在可编辑的组已纳入同一技能时跳过。命中 DPS 乘数量相加；流血、点燃、毒共用各自的层数上限，单层伤害高的先占层（`full_dps::allocate_stacks`）；同一技能不可叠加的持续伤害只取较强的一份，可叠加的（`DotCanStack`）乘数量，持续伤害总上限与最强的 Culling Strike 倍率各只计一次。逐技能的行（`FullDpsSkill`：数量、单实例命中 DPS、分到的持续伤害、最终贡献、未支持词条与错误）之和等于 `FullDPS`。`includeInFullDPS`、`groupCount`、宝石的 `count` 与 `enableGlobal1/2` 导入导出时保留，WASM 请求里是组的 `include_in_full_dps`、`group_count` 与宝石的 `count`；Web 的技能页编辑它们，只在有组纳入时请求 `full_dps: true`。

Uhtred 系辅助（“+2 to Level of Supported Skills if exactly one other Support is modifying them”）只把辅助宝石算作辅助：meta 宝石（Cast on Elemental Ailment）的触发效果是技能宝石的第二个效果，不算；只辅助 meta 宝石的辅助（Boundless Energy II 要求 `GeneratesEnergy`）也不修改被触发的技能（`skill_env::resolve::support_granted_gem_levels`）。“Skills from Corrupted Gems have …” 在主技能取等级的那颗宝石已腐化时生效（条件 `GemCorrupted`，`calc_orchestrator::skill::resolve::from_corrupted_gem`）。

Into the Breach 的紫色火焰（“7% of damage as extra Chaos … stacks up to 10 times”，[Into the Breach](https://poe2wiki.net/wiki/Into_the_Breach)）按 `breach_flame_chaos_addition_%` 乘配置的火焰数（`PurpleFlamesCount`，最多 10），每份的数值再乘残骸效果：技能自身与辅助宝石的 `remnant_effect_+%` 加上角色的 “increased effect of Remnants”（生成残骸的技能，`GeneratesRemnants`）。“Effect and Duration of Flames of Chayula on You is Doubled” 与只取红 / 蓝火焰的词条还没有建模。Decompose 的毒云 “Poisons enemies as though dealing Chaos damage equal to {0}% of Corpse's maximum Life” 必定中毒；它不命中，所以不能暴击，也没有暴击伤害加成（`skill_stats::corpse_damage_without_hit`，所选 stat set 没有自己的基础伤害时带 `NeverCrit`）。毒云的爆炸带基础火焰伤害，是命中，按技能的暴击率。PoB2 的毒云暴击率为 0 但保留暴击伤害加成，所以 `CritMultiplier` 与参考值不同。

光环由 `calc/buff_pass.rs` 统一处理；旧的 `buff-pass-aura` Cargo feature 和编排层直接注入光环的通道均已删除。运行时 buff 阶段受 `mode_buffs` 控制，curse/debuff 另受 `mode_effective` 控制；不要按历史迁移文档重新引入旧开关。

队伍保存在 `Build.party`；`party::parse_party` / `party::party_xml` 保留 `<Party>` 的 `ImportedBuffs` / `ExportedBuffs`，导出启用成员的合并标准块，并用可忽略的 `PoBRMember` 保留成员边界与禁用状态。`party::import_member` / WASM `build_api::import_party_member_json(code)` 先清空成员自身的队伍输入，再算生命、能量护盾、魔力、属性和已支持的外放光环／诅咒；能重算的同名效果替换导出值，其余沿用原始块。队友效果走 `calc::buff_pass` 的接收者缩放、召唤物投递、最强副本与施法者各自的诅咒上限，敌人效果走既有敌人通道；来源 id 为 `party.<成员>.<目标>.<来源>`，参与缓存身份与逐行归因。战吼、链接等不能从原生技能重建的效果依赖导出块或手填文本；仅支持简单的 `parent` / `partyMembers` 资源属性缩放。嵌套 LIST、复杂标签及不支持的 `GlobalEffect` 字段保留并诊断，不能剥掉限制后当作无条件词条；成员导出的跳过项有警告，PoB2 再保存可能丢弃成员边界。

## 与 PoB2 的关系（逐步独立）

方向（项目所有者，2026-10）：PoBR 是独立实现的引擎，不是 PoB2 的移植。PoB2 的构筑码导入导出是要保留的产品功能；它的代码和数据是要逐步减少的依赖。“逐步”是约束的一部分：不做一次性重写，每一步都保持门禁通过。

新代码的做法：

- 机制以游戏自己的定义为依据实现：`data/<版本>/base/` 的 stat id、技能数据和游戏常量，词条文本，官方 patch notes。代码和测试的出处写这些（例如 `attack_damage_final_permyriad_per_rage = 100`），PoB2 只用于交叉核对。范例见 `pobr-core/src/calc/rage.rs`。
- 技能 stat 的含义写进 `pobr-core/src/rules/skill_stats.rs`：以游戏 stat id 为键，用引擎自己的 modifier 表达，出处是该 stat 的游戏描述文本。这张表先于抽取来的 `skill_stat_map.json` 查询，表里有的 stat 不再读抽取数据；抽取映射拒绝或缺失的 stat（`mise run diagnose` 第 4 节）优先在这里补，而不是去扩充按 PoB2 名称写的白名单。
- 不再新增“因为 vendor `X.lua:行号` 这样写”式的注释；行号随 vendor 升级失效，也说明不了游戏规则。改到旧代码时顺手把这类注释换成规则本身的描述。
- PoBR 与 PoB2 结果不同时，先判断谁符合游戏的词条文本和数据，不默认 PoB2 正确。已知的有意差异：
  - 偷取每种资源只取恢复速率最高的有效实例（[0.5 补丁说明](https://www.pathofexile.com/forum/view-thread/3932540)：“Only a single Leech instance per resource (Life, Mana, or Energy Shield) can apply at a time.”）；[游戏关键词](https://poe2db.tw/us/Keywords)给出一秒基础恢复时间，偷取量和偷取速度分别修正。钉定的 PoB2 仍叠加实例，保留按资源池计算的单实例偷取量上限，并把“increased amount … Leeched”当作最大速率加成。tuned 组的 Deadeye（Twister，`bed1edc6d8b2`）`ManaLeechGainRate` 为 PoBR 80.72 对参考值 166.17；Pathfinder（Ice Shot，`728cbde29279`）同一字段为 162.96 对 190.12；Blood Mage（Arc，`5bc8be0fa967`）`LifeLeechGainRate` 为 513.68 对 575.23。去掉单实例偷取量上限后，原来吻合的三行 `LifeLeechGainRate` 也出现有意差异：Warbringer（Shield Wall，`1e4e329e5e5c`）为 582.978 对 552.40416，Blood Mage（Comet，`bc750d615159`）为 641.43144 对 207.01274，Gemling Legionnaire（Frostbolt，`42f790c0f9ba`）为 0.2 对 0.0635；这些行已登记在 `devs/scripts/known-differences.json`。前述默认速率上限仍是兼容约定，平均命中频率也只是覆盖时间估计；这些数值不是游戏内实测结果。
  - PoB2 用自己的符文表覆盖物品原文；PoBR 以物品原文为准。同一物品上同一 stat 的符文在原文里合成一行：tuned 组的 Infernalist（Comet）武器上是一行 “45% less maximum Life”，PoBR 按 0.55 计，PoB2 按两块符文的 15% 与 30% 分别连乘（0.595），所以它的生命比参考值低。
  - 同一个诅咒由两个技能组施加时，敌人身上只有最强的那一份（“Debuffs of the same type do not stack, and only the copy with the strongest effect applies”，[Curse](https://www.poe2wiki.net/wiki/Curse)），比较的是各份自身与其辅助宝石的诅咒效果（`skill_env::buff_specs::curse_strength`）。PoB2 按自己的优先级表挑，光环来源优先：tuned 组的 Acolyte of Chayula（Decompose）有 Doedre's Undoing 组和 Blasphemy 组两份 Despair，PoB2 用后者（敌人混沌抗性 −31），PoBR 用更强的前者（−58）。
  - 尸体爆炸的伤害类型取自游戏 stat：`corpse_explosion_monster_life_permillage_<类型>` 与 `corpse_explosion_monster_life_%_<类型>` 写明类型；不带类型的 `corpse_explosion_monster_life_%`（Fiery Death）按它的文本 “Explosion deals base Fire Damage equal to {0}% of the Corpse's Maximum Life” 是火焰伤害（`skill_stats::corpse_explosion_damage`），PoB2 按物理计。
  - Doedre's Undoing 的 Witchtoad 每 0.25 秒生成一只，PoB2 把这个间隔乘以施法时间，施法越快爆炸越多且没有上限；PoBR 按“区域数 × 1000 / 间隔”计算，与施法速度无关（见 `skill_stats::granted_skill_modifiers`）。Dark Consequences 自带魔力消耗为 0，但 Archmage 的“Non-Channelling Spells cost an additional … of your maximum Mana”仍给它本身增加消耗；这不是继承诅咒的消耗。tuned 组 Blood Mage 的 5.7% 最大魔力经 Atalui 转成生命、Efficiency II 减少 40% 后为每次爆炸 199 生命，两区域每秒 8 次为 1592。PoB2 的每秒生命消耗却用它的施法速率 2.14/s，得到 425.86；PoBR 保留按爆炸频率支付，伤害计算不变。
  - Instruments of Power 的 “50% reduced bonuses gained from Equipped Focus” 让 focus 上的 stat 按一半给予，“+# to Level of all X Skills” 不低于 1（[Instruments of Power](https://www.poe2wiki.net/wiki/Instruments_of_Power)）：focus 的 +2 法术等级计 +1，+1 仍为 +1（`skill::resolve::scale_slot_gem_property_bonuses`）。PoB2 减半 focus 的其他词条，宝石等级不减半：demo 语料的 Disciple of Varashta（Comet）的法术比参考值低一级，TotalDPS 0.86×，`ninja_parity` 的进攻与持续伤害基线各降 1 / 2 格。
  - Bonestorm（“Channel to conjure a swarm of bone spikes in the air, then release to fire them at enemies and explode”，“Can fire up to 20 Projectiles”，[Bonestorm](https://poe2db.tw/us/Bonestorm)）引导时每次施法召出一根骨刺，释放时全部射出：PoBR 按每根骨刺命中一次计，每秒命中数即施法速率；一次引导最多 20 根（`base_number_of_allowed_bone_storm_projectiles`），不改变每秒的骨刺数。PoB2 的引导层数（`Multiplier:BonestormStage`）没有配置时为 0，按最短引导时间 0.3 秒（`base_minimum_channel_time_ms`）命中一次：tuned 组的 Abyssal Lich（Bonestorm）PoB2 每秒 3.33 次，PoBR 11.96 次。**没有游戏内实测**：假设每根骨刺都命中目标。
  - “Minions have 10% chance to inflict Withered on Hit” 在没有召唤物的构筑上，PoB2 把这条词条当作玩家自己能施加 Withered，并按配置的层数给敌人加混沌承伤；PoBR 把它交给召唤物，角色自己的技能、Presence 或已有的召唤物能施加 Withered 时才计入配置的层数（`calc/withered.rs`），所以这类构筑会比参考值低。每层 5%、最多 10 层两边一致。
  - Lament / Portent / Absent Amulet 授予的技能不保留精魄（游戏表 `ItemInherentSkills` 的 `NoReservation`，三个基底列在 `overlay-common/item_granted_skills.json`）。PoB2 导入角色时把这个技能再列成一个普通宝石组并按宝石保留 30 / 60 / 100 精魄，所以这类构筑码自带的 `SpiritUnreserved` 常为负数，而游戏里保留量不可能超过精魄总量。PoBR 把“排在组首、等级与授予等级相同、无品质、未腐化”的那个普通组认作被授予的技能本身，不计保留，也按授予技能的需求降级（见“宝石等级与品质加成”后一段）；其余同名宝石照常保留（`calc_orchestrator/granted_skills.rs`）。其他物品授予、需要保留精魄的技能也是这样认出的同一个技能，只保留一次；PoB2 的物品组和宝石组各保留一次（tuned 组 Gemling Legionnaire Frostbolt 与 Oracle Grim Pillars 胸甲授予的 Blink：PoB2 120，PoBR 60）。PoB2 只对仅由物品授予的技能（`fromItem`，如 Purity of Ice）这样合并，所以 Absent Amulet 的 Eternal Rage 在 PoB2 里是两个组：降级的物品组和未降级的宝石组。该技能上的辅助宝石追加的精魄也一并免除，PoB2 仍计入：依据是权杖授予的技能在游戏数据里以 `reservation_multiplier = -100` 表达“不保留”，辅助的追加量随之归零；这一点没有游戏内实测。
  - Rain of Arrows 的命中频率按游戏的两条规则计算（`pobr-core/src/calc/rain.rs`）：“One arrow lands every 0.08 seconds”（`rain_of_arrows_delay_per_arrow`，“Modifiers to Projectile speed apply to arrow landing frequency”）和 “Can Hit each enemy only once every 0.12 seconds”（`rain_hit_delay_ms`）。命中只能来自落下的箭：间隔过后落下的第一支箭命中，所以每 ⌈0.12 ÷ 落箭间隔⌉ 支箭命中一次；一次使用射出 N 支箭（技能的投射物数量），一场箭雨最多命中 ⌈N ÷ 该数⌉ 次，使用得慢、箭雨在下一次使用前已经落完时按每次使用的命中数封顶（“Maximum 1 arrow rain”）。PoB2 把命中间隔记为 max(0.08, 0.12) 秒，等于假设敌人每次能被命中时恰好有箭落下，也不计投射物速度：没有投射物速度时游戏规则给出每秒 6.25 次，PoB2 为 8.33 次；tuned 组的 Rain of Arrows 构筑（169% 增加的投射物速度和 Projectile Acceleration III 的 40% 更多投射物速度，每 0.0212 秒落一支）是每 6 支箭命中一次，每秒 7.85 次。以下是假设，**没有游戏内实测**：每支落下的箭都打到敌人（箭雨半径 3.6 米、单支箭的冲击半径 0.7 米，对单个敌人实际更少，取决于体型）；落箭节奏均匀，按游戏的数值、不按服务器帧取整；角色持续使用该技能；品质的 “Arrows fall #% faster” 是箭下落的速度，不改落箭频率。同样带 `rain_hit_delay_ms` 的 Firestorm、Icestorm 见下一条；Hailstorm Rounds（`rain_hit_delay_ms` = 200）的每次发射提供一次落点，连续发射时每 ⌈0.2 × 发射速率⌉ 次落点命中一次，再按弹匣与装填周期折算（`perform::fill_crossbow_reload`），一次攻击的多个投射物共用这个命中间隔，命中限制不减慢弹药消耗；`siege_cascade_fall_time_ms`（300）是飞行时间，不是落点间隔。Hailstorm Rounds 没有模拟散布和弹匣交界处的命中锁定相位，**没有游戏内实测**。参考口径（`on_reference_basis()`）按 PoB2 的计法比较：箭雨每当敌人能再被命中时就算一次，Hailstorm Rounds 取发射速率与每秒 5 次中的较小值。
  - 球与风暴的命中不随施法速率（`pobr-core/src/calc/periodic.rs`，stat 由 `rules::spell_hits::modifiers` 映射）：Ball Lightning “Hits enemies in range every {0} seconds”（`ball_lightning_base_hit_frequency_ms` = 200），Solar Orb “Pulses every {0} seconds”（`solar_orb_base_pulse_frequency_ms` = 1400）；Firestorm / Icestorm 每 `fire_storm_fireball_delay_ms`（200 / 150）落一次，按上一条箭雨的落点规则受 `rain_hit_delay_ms`（150）限制；Thunderstorm “Hits one target every {0} seconds”（`lightning_storm_hit_frequency_ms` = 350），同一敌人 `lightning_storm_hit_prevention_duration_ms`（500）内只被命中一次，取两者约束下的潜在频率，不模拟 `lightning_storm_hit_frequency_variance_ms` 的随机间隔。角色维持一场风暴或一个球，多个不叠加；有持续时间的按 min(持续时间 × 每秒使用次数, 实例上限) 计覆盖率（最多 1），Ball Lightning 没有这类持续时间 stat，假设目标一直在范围内、不估算球的行进。Ball Lightning、Solar Orb 的其他 stat set 不继承这个频率。PoB2 假设风暴与球始终覆盖目标，参考口径关掉 `DataOrchestratorOptions::periodic_uptime` 按它比较。**没有游戏内实测**：假设敌人始终在范围内，每个符合间隔的落点都命中。
  - 引导攻击（Detonating Arrow、Supercharged Slam、Perfect Strike、Gathering Storm、Snipe、Shield Charge，`calc::channel::skill_modifiers`）的一次使用近似为“释放前引导 + 释放”（`pobr-core/src/calc/channel.rs`）。引导取 `base_minimum_channel_time_ms`、`channel_start_lock_cancelling_of_attack_time_%` × 攻击时间、层数 × 攻击时间中最大的一个；完美时机只给出窗口宽度（如 `perfect_strike_timing_window_base_ms`），没有开启时间的 stat，所以完美与普通 stat set 各算自己的伤害，时间都用最短引导 / 起手锁定。释放取 `channel_end_duration_as_%_of_attack_time` 的比例，或 `channel_skill_end_animation_duration_override_ms`（有时优先，两者不相加）；比例随攻击速度缩放，固定毫秒不缩放，只有动画倍率、没有基础时长的阶段不另计。层数同时用于伤害和时间，默认取上限（含品质），构筑配置的 multiplier `ChannelStages`（配置输入 `skillChannelStages`，Web 技能页的层数输入）可覆盖（取整并限制在 1 到上限）：Detonating Arrow 每层 `detonating_arrow_all_damage_%_to_gain_as_fire_per_stage` = 120；Supercharged Slam 每层 “40% more Damage with Hits for each stage”（只加命中），“Releases 1 Aftershock per Stage”，冲击 stat set 计冲击加全部余震、余震 stat set 只计余震，用等级行 850 毫秒不受攻击速度影响的攻击时间，余震延迟不阻止下一次引导。Shield Charge 按 1300 毫秒最短引导，不模拟行进与碰撞。PoB2 多只把释放阶段当作命中间隔、默认一层并用武器的攻击时间。**没有游戏内实测**。
  - 带 `base_skill_show_average_damage_instead_of_dps` 的技能（Grim Pillars、Lightning Warp、Vile Disruption、Skyfall、Living Bomb、Volcano、Incinerate、Rake、Parry、Raise Shield、Hand of Chayula、Stampede、Gathering Storm、Perfect Strike 等）在游戏面板上显示平均伤害；这个 stat 没有词条文本，不改变伤害或使用速率。PoBR 的 `CombinedDPS` 仍是每秒伤害：命中 DPS 加持续伤害 DPS，再计 Culling Strike；PoB2 在这里改填平均命中伤害，也不加它已算出的持续伤害（Incinerate 的 `base_fire_damage_to_deal_per_minute`、Living Bomb / Volcano 的点燃、Rake 的流血）。这是输出口径不同，不能拿这一行当作伤害倍率去补。Vivid Stampede（`skill_triggered_on_hit_while_have_vivid_stag_wisps`，攻击命中触发、每个 Vivid Wisp 一只鹿）同样只显示平均伤害：PoB2 的 TotalDPS 为 0、CombinedDPS 是每次触发的伤害；PoBR 暂按武器攻击速率每次攻击一只估算，精灵数量与触发来源还没有建模，两边不能比较。
  - 伤害区间与命中率保留小数：Earthquake / Stampede 的 Wooden Club 6–10 乘等级行的 `damage_multiplier` 1.25 为 7.5–12.5，Toxic Growth 的 6–9 乘 0.62 为 3.72–5.58；PoB2 先把伤害上下界取整（8–13、4–6）、命中率取整到百分数，低伤害的合成构筑上差别可超过 5%。这是计算精度的约定，游戏内部是否取整**没有游戏内实测**。
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
  - 图腾使用的技能（图腾宝石自己的技能，或插在法术图腾这类 meta 图腾宝石里的技能，游戏类型 `UsedByTotem` / `SummonsTotem`）按“角色有图腾、最近召唤过图腾”计算，施法和攻击不算角色自己的（不置 `CastSpellRecently` / `AttackedRecently`）。“每个已召唤图腾”类词条按 1 个图腾计，配置填了“已召唤图腾数”时按配置。PoB2 默认按图腾上限计，而它给法术图腾里的技能算出的上限不含宝石随等级增加的 `base_number_of_totems_allowed`（两个 tuned 构筑上是 2，按游戏数据 23 级以上的法术图腾是 3，再翻倍）。PoBR 还没有图腾上限模型，所以不假设达到上限（`calc_orchestrator/prepare.rs`）。异常的层数同理：插在法术图腾里的技能，PoB2 按该技能自己的图腾上限乘层数，而上限的 stat 在图腾宝石上，它取到 0，点燃为 0；PoBR 按 1 个图腾的命中计算点燃（tuned 组两个 Grim Pillars 构筑的点燃只有几十，参考值为 0）。
  - Charged Staff 的 buff 按 “Buff grants {0} to {1} Added Quarterstaff Attack Lightning damage per Power Charge Consumed, up to your maximum Power Charges”（`charged_staff_attack_minimum_added_lightning_damage_per_stack` / `charged_staff_attack_maximum_added_lightning_damage_per_stack`）计算：每消耗一个暴击球一层，最多为最大暴击球数，只加给 Quarterstaff 攻击。PoB2 把 One with the Storm 的 “count as consuming an additional Power Charge”（`quarterstaff_skills_that_consume_power_charges_count_as_consuming_x_additional_power_charges`）加在这个上限之外，最大 6 个暴击球时按 7 层计，且不限于 Quarterstaff 攻击；PoBR 按可消耗的暴击球数（当前减最小值）计，满球时为 6 层，不读取这个额外的球，暴击球未满时它也不计入（`skill_stats::player_buff_modifiers`）。
  - Purity of Fire 这类光环的技能类型（`Buff`、`Aura` 等）里没有 `Spell`，“increased Critical Spell Damage Bonus” 这类法术词条不作用于它；PoB2 给它的 stat set 写了 `spell = true` 的基础标志，主技能是光环时暴击伤害加成按法术计（tuned 组的 Ritualist Purity of Fire，2.24 对参考值 2.49）。这一行对光环没有实际意义。
  - Lich 的 Crystalline Phylactery 让其中的珠宝按两倍计（见上文 `Build.jewels`），The Adorned 不作用于它。PoB2 不解析 “Effect of bonuses gained from Socketed Jewel”，并把 The Adorned 用在护符匣里的珠宝上，构筑码自带的参考值也是这样算的：tuned 组四个护符匣构筑（Abyssal Lich Twister 4028 / 9d1c、Lich Spark ce24 / 0a17）的伤害、暴击与个别抗性随之与参考值不同。
  - Bear form 按游戏的形态说明 “+10 to Armour per level” 给 10 × 角色等级的护甲，并让 30% 的护甲也作用于元素伤害（`pobr-core/src/calc/bear_form.rs`，主技能带 `Shapeshift` 与 `Bear` 类型时生效）；PoB2 多给 10 点（10 + 10 × 等级），参考值因此略高（约 0.2%）。
  - Pounce 的 Predator's Mark（“Marked target takes {0}% increased damage for each of its enemies within 6 metres of it, up to {1}%”，`skill_wolf_mark_damage_taken_+%_per_nearby_enemy` / `skill_wolf_mark_damage_taken_+%_cap`，品质加在等级值上，20 品质为 4% / 60%）按角色在 Presence 内已有的召唤物计数：计算中创建的召唤物，或配置的 “# of Minions in your Presence”（`MinionPresenceCount`，`skill_stats::curse_modifiers`）。不计 Pounce 在被标记的敌人死亡时召唤的狼（`WolfPounceSummonWolfPlayer` 的 `skill_cross_slash_triggered_summon_+_wolves_if_rare` / `_if_unique`），被计算的敌人活着时它们还不存在。tuned 组的 Witchhunter 构筑上这个数是 0：它的 Living Lightning II 所在组没有能被辅助的技能，不会出现 Living Lightning；构筑码自带的参考值按 8 只计（+32%），比 PoBR 高。辅助宝石授予的召唤物在辅助能生效时也还没有创建。
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
(`local_%_chance_to_gain_flask_charge_on_kill`) adds no passive generation
because kill rate is not modelled. Lavianga's “This Flask cannot be Used but
applies its Effect constantly” (`local_flask_always_drinking`) reports zero
available uses while keeping its recovery rate.

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
maim/on-hit armour break, slow potency and heavy-stun buildup remain outside
reported calculations; their modifiers remain visible in diagnostics. Ox's
threshold bonus feeds `AilmentThreshold`. “Energy Shield Recharge starts on use”,
“Creates Ignited Ground … when used”, “Creates Consecrated Ground on Use”,
and “Gain Guard equal to Current Runic Ward … when Effect ends” remain
unsupported pending an event model.

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
| Modifier → 计算 | 存入的名字从未被查询；依赖的 `Multiplier` 没有任何地方赋值（含从角色快照读取的 `actor: player` 类，例如 “per non-Idol Augment in your Equipment”）；等待的条件不在条件表里，也没有计算阶段推导它 | `Diagnosis::unread_modifiers` / `unprovided_multipliers` / `unset_conditions` |

`pobr_build::diagnose` 运行同一条计算管线并返回以上全部，外加实际计算的主技能及其等级（技能或等级选错不会丢输入，所以单独列出）。它打开 `ModDb::track_reads` 与 `CalcConfig::track_reads` 并以 Compare 模式记录 stat 映射，结果与普通计算逐位相同（`diagnosis_does_not_change_the_result` 对全部已入库构筑断言），但更慢，只用于排查。

逐行台账 `pobr_build::usage::line_usage`（CLI `calculate-build --usage [--json]`、WASM `line_usage_json`）在同一次诊断上回答“构筑的每一行词条用上了没有、用在哪里”：计算收到的每一行（物品各区段、肢体、药剂与护符、珠宝、天赋节点、任务奖励与自定义词条、配置项，以及宝石 stat）列一次，带来源 id、持有者名字（节点名、物品基底）、解析出的每个 modifier 和它的去向：`applied`（被计数；`used_in` 列出它进入的 stat）、`partial`、`inactive`（名字被读取但没被计数，`reason` 给出缺的条件或标志，例如 “needs Attack”）、`unread`（没有任何查询问过这个名字）、`outside`（在 ModDb 之外消费：物品自身的防御 / 武器伤害、宝石等级与品质、珠宝半径，`reason` 写明读取方；宝石等级与品质行还写明提升了哪些技能，一个都没提升时为 `unread`）、`no_effect`（登记为不影响任何输出的 S0 条目，或本构筑无人询问的配置项）、`unmodelled`（有规则识别但不产生 modifier，机制未建模）、`unparsed`、`no_modifier`（解析出 modifier 却没有存入，台账应当学会说明原因）。给召唤物的词条（`MinionModifier`）按召唤物自己的计算是否读取了它给出的名字判断，构筑没有召唤物时为 `unread`。`Condition:<X>` / `Multiplier:<X>` 类 modifier 经条件表消费：有 modifier 等待 `<X>`、或计算阶段问过 `<X>`（`CalcConfig::read_variables`）时算作用上。行的去向由编排在写入来源时记录（`SourceWriter::note_line`，`LineFate`）；在 ModDb 之外拿走或丢弃词条行的新代码（剥离本地词条、按文本读取）要同时调用 `note_line` 并写明读取方，否则台账会把这些行报成未解析或没有 modifier。`mise run triage` 带 `--usage` 运行每个构筑，把未建模的行、没有任何构筑读取的名字、无人设置的条件和被拒绝的宝石 stat 跨构筑汇总，不论该构筑有没有不一致的行。

条件不在条件表里时，有两类不是线索：计算阶段自己推导、成立时才写入的（暴击的一半、主副手、怒气与战斗、敌人的冰缓与感电、诅咒与标记、Elusive、Surrounded、所用武器组、内置 buff 附带的条件、`AffectedBy*`、`Using*Flask` / `Using*Charm`，见 `calc/session.rs::CONDITIONS_SET_BY_A_PASS`），以及编排按构筑本身设置、换一个构筑才会成立的（武器类型、盾、主技能的战斗条件、首领、同伴、普通胸甲、辅助宝石颜色，见 `diagnosis.rs::SET_BY_THE_ORCHESTRATION`）。其余条件中有配置项能设置的标为 “config: off”，由用户决定是否打开；没有任何来源的才是丢掉的输入。新增由计算阶段或编排设置的条件时登记进对应清单，否则其他构筑会误报它。

排查顺序（先批量、后单个；完整流程见 [AGENTS.md](../AGENTS.md#accuracy-workflow)）：

1. `mise run triage` 一次计算基准组与 tuned 组的全部构筑和钉定的 PoB2（也可给组目录或构筑目录），把与构筑码参考值不一致的每一行按今天的 PoB2 分类：`engine`（PoB2 与参考值一致而 PoBR 不一致）、`both`（三方都不同）、`reference`（PoBR 与 PoB2 一致，参考值出自另一版导出工具，不用改）、`known`（`devs/scripts/known-differences.json` 已解释的有意差异；PoB2 额外保留护身符授予技能的精魄也自动归入）。随后把 engine / both 行的线索跨构筑汇总：按来源与类型对齐后只有一边有或数值不同的词条、PoBR 有却没生效的词条、伤害因子与敌人的抗性 / 穿透 / 承伤、最大承伤与 EHP 背后 PoB2 的资源池、精魄总量与逐技能保留，以及没有规则能解析的词条（按模板合并，按涉及的构筑数排序）；再从每个构筑的逐行台账汇总计算丢掉的输入：有规则识别却未建模的行、存入后没有任何构筑读取的名字（先列没有读取方的）、无人设置的条件、被拒绝的宝石 stat。EHP 与最大承伤的行另列两边逐类型的敌人伤害、承受的部分、资源池和命中次数中不同的项，以及 PoB2 一侧敌人获得的额外伤害与它的来源（oracle 的 `enemyMitigation.gainMods`）。每个构筑另列本构筑存入却没有读取、名字与不一致的行同词的词条（例如 `EffectiveBlockChance` 旁的 `ReplaceShieldBlock`）：只有少数构筑有的名字排不进跨构筑的清单。每个构筑另列两边按不同等级或品质计算的技能（PoBR 取自 `--usage` 账本的 `skills`，PoB2 取自 oracle 的 `skillLevels`）：技能等级改变 buff、光环、诅咒的数值，却不丢任何输入，其他线索都指不出它。按涉及构筑最多的原因成批修复，整批修完再跑一次（PoB2 一侧按构筑码缓存，与 `compare` 共用）。holdout 组只用 `mise run accuracy` 测量，`triage` 拒绝它。
2. 分诊解释不了的行，再单独看那个构筑：`mise run diagnose <构筑>`。第 1 节是导入损耗，第 2 节是与构筑码自带参考值的差异，第 3–9 节是丢掉的输入（第 6 节是等待无人设置的条件的词条，配置项能设置的条件另列为 “config: off”）。多数差异在这里就能对上原因。
3. 看一个 stat 怎么算出来：`cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown <Stat,...>` 打印计算步骤和读到的每条词条（`--reference-basis` 按参考值口径，`--json` 输出 JSON；加 `--usage` 列出没有完全用上的每一行及原因）；技能、stat、基底、天赋或暗金在游戏数据里的记录用 `mise run lookup <id 或名字>`，不必逐个打开数据文件。
4. 清单解释不了的差异，用 `mise run compare <构筑> [Stat ...]` 对照钉定的 PoB2：每个 stat 两边的总值与比值、BASE / INC / MORE 合计，以及按来源（天赋节点、装备槽、技能、角色基础与属性、配置、任务奖励）对齐后只有一边有或数值不同的词条。不给 stat 时比较构筑码参考值中不一致的行。`ActionRate` 在 PoB2 有 `HitSpeed` 时与它比较（命中频率，不是施放速率），没有时才用 `Speed`；召唤物的总值以 `Minion:<stat>` 一并列出。PoB2 的结果按构筑码缓存在 `.cache/pob2-oracle/`，重跑只重算 PoBR（`--refresh` 两边都重算）。更细的中间值仍可直接用 `tools/pob2-oracle/run.sh`。
5. 参考值不是真值。构筑码内的 `<PlayerStat>` 是导出工具当时的结果（版本未知，可能早于当前数据）；PoB2 自身也有假设和缺陷。两边不一致时，以游戏自己的词条文本和数据判断谁对，再决定改哪一边。导出工具缺失的词条用 `exporter: "absent"` 与游戏来源登记，让参考口径自动排除，不降低基线、不按构筑登记 known differences；导出工具口径不同的规则用 `"differs"` 保留并报告。其余确认为有意差异的行才写进上文与 `devs/scripts/known-differences.json`。

同一类行反复需要逐个构筑人工排查时，说明分诊或诊断的输出缺了东西：扩充工具，让下一轮批量运行直接给出这类线索。

找下一批机制缺口时用 `mise run gaps`：把当前数据的 `generated/modifier-audit.json` 与 benchmark、tuned 的 `--usage` 台账按数字替换为 `#` 的模板合并，先按丢掉输入的构筑数，再按 unique、passive、mods、compound、jewel_node、bases 的来源优先级及来源数排序。纯 translation 来源不入队；`pobr_gap` 是 PoB2 能解析而 PoBR 未完整处理的线，`upstream_gap` 是两边都不完整，`recognized_empty` 是识别却未产出 modifier，`no_effect` 是已登记不影响输出的条目（供审查，不代表待实现）；队列只收录有审计来源的游戏文本；研究卡也可查询只见于构筑台账的文本，此时标为 `ledger_gap`。读取不到的 modifier 也可能只是该构筑用不上，涉及构筑数是研究线索，不是错误断言。`--kind` 按来源过滤，`--limit` 只限制文本行数，`--json` 保存全部模板、变体、审计字段与构筑台账。命令默认构建一次 CLI，不运行 PoB2；可用 `--binary <pobr>` 显式复用已构建程序。台账与 `triage` 共用 `.cache/build-usage/`，按构筑码、程序内容、收集脚本及数据文件路径/大小/修改时间失效；`--refresh` 强制重算。两条命令只读 worked 组，不读 holdout。

从队列选中一行后运行 `mise run gap "<模板或文本>"`，一次列出数字变体、从 `overlay/stat_descriptions.json` 反查的单 stat 或 compound 全部成员（逐项确认 `base/stats.json` 是否存在）、每个来源的完整词条文本（含暗金其他行与咒符触发隐式）、节点名、每个携带它的 worked 构筑及台账去向、引擎名字和 special/mod_parser 近似规则。引擎与规则候选仅按关键词匹配，不证明支持；暗金与节点的 poe2db 链接标为猜测，未经联网验证。确认游戏含义时可再把卡片上的稳定 id 交给 `mise run lookup <id>`。

已入库构筑覆盖不到的技能用 `mise run skills:sweep`（`devs/scripts/skill-sweep.py`）：对活动数据里每颗非辅助宝石（含 meta）的所有非辅助主动效果，按 `GemEffects` 显示顺序枚举，保留来源宝石与主效果，以 `mainActiveSkill` / `mainActiveSkillCalcs` 选择效果；行标识为 `主效果/所选效果@序号`，主／附加效果分别计数，弹药装填不能代替射击覆盖。最小合成构筑使用 90 级 Witch、20 级零品质宝石、普通武器基底，同时计算 PoBR 与钉定 PoB2，按技能族汇总 `agree` / `differs` / `pobr_zero` / `pob2_zero` / `both_zero` / `error` / `pob2_other_skill`；纯指令与占位效果另列。扫描不读 holdout；两边实际效果均须核对，vendor 缺少效果或附加效果的来源宝石不算吻合。`both_zero` 不证明 buff、无载荷幻影、无事件触发或召唤物已实现；召唤物只看一个主召唤物，不代表整支召唤物队伍。平均命中因减伤口径不同只展示、不分类；速率优先对照 PoB2 的 `HitSpeed`，并对照 CLI JSON 的 `firing_rate` / `reload_time`（`FiringRate` / `ReloadTime`）。每效果只测第一个 stat set，不穷举辅助、触发载荷、尸体、充能、目标与战斗条件，不混合轮转或过热。完整扫描写 `.cache/skill-sweep/report.json` / `report.txt`，`--skill` 定向扫描默认写 `report-selected.json` / `report-selected.txt`，显式 `--json` / `--text` 优先；保留完整轮次再做定向扫描。结果缓存在 `.cache/skill-sweep/`，oracle 缓存键含 XML、vendor commit 与 wrapper 内容。

侧栏字段的覆盖与数值吻合分开衡量：`python3 devs/scripts/output-coverage.py` 用已构建的 `target/debug/pobr`，按 `display_catalog` 的 PoB key 逐字段对照 oracle 的完整输出（默认 benchmark、tuned 与 demo 三组，拒绝 holdout），给出每个字段两边都有有限值的构筑数、非零样本数、5% / 10% 内的吻合数和最差的构筑；`--oracle-cache` 只读已有缓存（键与 `compare-build.py` 相同），`--refresh-missing` 补算缺少的。没有样本不算验证通过，大量零值吻合也不证明非零的机制正确。

“未被读取”是线索不是结论：攻击技能不读法术伤害是正常的。报告用全部已入库构筑的读取并集区分“任何构筑都不读（没有读取方）”和“本构筑的技能用不上”。

新增按名字消费 modifier 的代码用 `ModDb::named(name)` 或聚合查询，不要用 `iter_mods().filter(名字)`：后者不算读取，诊断会把有读取方的名字报成没有。确实在数据库之外消费的名字（按文本扫描、构筑结构）登记在 `diagnosis.rs::READ_OUTSIDE_THE_DATABASE` 并写明消费者。

宝石等级与品质加成（“+1 to Level of all Spell Skills”“+2% to Quality of all Skills”）按文本读取，不经过 ModDb：技能等级要在任何 modifier 存入之前确定。读取面是角色的全部词条来源：装备与珠宝、已分配和被物品授予的天赋、配置行（任务奖励、自定义词条），以及调用方传入的 `extra_modifier_texts`（由 `prepared_build_view` 并入构筑视图，主面板、逐技能总览和触发子计算共用）。新增一条文本通道时要同时接入 `skill::resolve::gem_property_bonuses`，否则这类词条在该通道里既不生效也不报未支持。被这样读取的行不计入未支持清单，也不再交给解析规则（规则会把“with an Intelligence requirement”误读成属性需求）。种类有两种写法，“all Cold Skills”和“all Nova Skill Gems”，含义相同。种类按宝石标签判定（游戏表 `GemEffects.GemTags`，adapter 写进 `base/skill_gems.json`；每个标签对应一条 `<标签>_skill_gem_level_+` stat），种类里的每个词都要是宝石的标签：Rain of Arrows 带 `projectile` 标签，尽管它的技能不是 `Projectile` 类型；Shockwave Totem 带它的图腾所用 Slam 的 `slam` / `melee` / `nova` 标签。“Elemental”不是标签，指火、冰、电三种之一（`elemental_skill_gem_level_+`）。辅助宝石的标签描述它辅助的技能和它授予的技能，它本身没有等级，所以不按标签判定；没有标签的旧快照也退回按技能类型判定，种类名与技能类型不同的（Totem → `SummonsTotem`、Curse → `AppliesCurse`、Strike → `MeleeSingleTarget`、Crossbow、Elemental）列在 `skill_env::mods::kind_skill_types`（`skill_env::mods::gem_tags_match_category` / `gem_level_category_matches`）。“increased bonuses gained from Equipped Rings and Amulets”（Ritualist 的 Mystic Attunement）放大戒指和护身符上的其他词条，也放大其中的宝石等级与品质加成，向下取整：+4 在 25% 下为 5（`skill::resolve::scale_slot_gem_property_bonuses`）。按技能名写的等级词条（Cursecarver 的 “+4 to Level of Despair Skills”，不带 all）走同一通道，按游戏的 `ActiveSkills.DisplayedName`（`GrantedEffectDef.displayed_name`，不分大小写）匹配，Sniper's Mark 这类带标点、不能从 id 还原的名字也能匹配（`skill_env::mods::parse_gem_property_bonus`、`skill_env::resolve::gem_property_applies`）；只接受完整的 “Skills” / “Skill Gems” 写法，“Socketed”、“Supported” 和带其他条件的写法有各自的作用域，不当作全局等级加成。

物品授予的技能（“Grants Skill: Level 18 Purity of Ice”）有自己的等级和属性需求，与物品的穿戴需求无关；角色达不到时降到能满足的最高等级（[poe2wiki: Granted skill](https://www.poe2wiki.net/wiki/Granted_skill)），一级都达不到时用它最低的等级。每个宝石等级需要的角色等级取自游戏表 `ItemExperiencePerLevel`（`SkillGemDef::level_requirement`；表里没列的等级沿用前一级）；属性需求按宝石的属性占比计算，L 为该等级需要的角色等级：round((5 + (L − 3) × 1.7) × (占比 / 100)^0.9) + 4（`SkillGemDef::attribute_requirements`，与游戏显示的逐级数值一致，例如 Purity of Ice 18 级需要 42 力量、106 智力）。被授予的组在构筑视图里记下授予等级（`SocketGroup::granted_level`，物品自己的组和上面那个被认作授予技能的普通组），`prepared_build_view` 再按角色等级和属性降级（`granted_skills::scale_granted_skills`）。属性来自不含技能的一次来源写入（`attributes_without_skills`：职业、装备、肢体、珠宝、配置、天赋和额外词条），技能自己的效果不计入；只有构筑有需要属性的授予技能时才运行，计算量约为一次计算的来源写入阶段。没有等级的授予行（“Grants Skill: Raise Shield”）不降级。已分配天赋授予的技能在构筑码里是 `<Skill source="Tree:<节点>">`，等级只是占位；它按角色等级取宝石能达到的最高等级（宝石经验表列到的最后一级，例如 98 级角色的 Sorcery Ward 为 20），同样记下授予等级、再检查属性。游戏导出的角色还把它列成一个普通组（排在组首、等级相同、带辅助宝石），那个组就是被授予的技能，天赋的组让位给它，主技能组随之改指；节点未分配时天赋的组不授予任何技能（`granted_skills::resolve_tree_granted_skills`）。

辅助宝石辅助一颗宝石的任一效果时，它给的等级和品质作用于整颗宝石：Physical Mastery（要求 Physical）辅助 Iron Ward 的物理新星，Iron Ward 的 buff 也高一级。等级计算接收宝石每个效果的辅助判定（`skill_env::buff_specs::gem_judgements`，主效果在前），同一个辅助只算一次；Uhtred 系列“没有其他辅助时”的“其他辅助”也按整颗宝石计。

配置行指任务奖励与自定义词条。其中解析不了的行以 `Config: <行>` 记入未支持清单，其余行照常生效；任务奖励选项的 “None”（未领取）不是词条行。对任何计算输出都没有影响的词条在 `overlay-common/special_mods.json` 里登记为只识别、不产生 modifier 的条目（`"mods": []`、`"batch": "S0"`，例如游戏 stat `experience_gain_+%` 的 “5% increased Experience Gain”；其他批次的条目必须产生 modifier 或挂 handler，`pobr-gamedata` 的 `non_s0_has_traceable_provenance` 检查），它不是丢失的输入；尚未实现的机制不能用这种条目消掉，必须留在未支持清单里。

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
