# PoBR — 流放之路 2 在线 BD 规划工具

[English](README.md) | **简体中文**

**换装备、搭辅助宝石、点天赋之前，先看看你的 BD 会提升多少。**

PoBR（Path of Building in Rust）是一款开源的 **流放之路 2（Path of Exile 2 / PoE2）
在线 BD 规划工具**。在浏览器里导入 PoB2 代码或 WeGame 分享链接，查看伤害与防御，
比较下一步升级。使用 Rust + WebAssembly 计算，支持简体中文、繁体中文和英文界面。

**[打开在线版](https://pobr-web.pages.dev)** ·
[更新记录](https://github.com/ackness/pobr/releases) ·
[反馈问题](https://github.com/ackness/pobr/issues)

> **测试版：** 已可体验，游戏机制覆盖仍不完整。比较升级收益前，请核对未支持效果和战斗配置。

## 看看下一步能怎么提升

| 装备总览与实时角色属性 | 换装比较与词缀阶级模拟 |
| --- | --- |
| [![装备布局与实时伤害、防御、抗性数据](docs/screenshots/equipment.png)](docs/screenshots/equipment.png) | [![市集戒指与两个已装备戒指的换装比较，支持前后缀编辑](docs/screenshots/item-comparison.png)](docs/screenshots/item-comparison.png) |
| **排除不想使用的辅助宝石，比较完整搭配** | **在天赋树上预览连通的升级路线** |
| [![辅助宝石排除列表与搭配的 DPS、EHP 提升](docs/screenshots/support-upgrades.png)](docs/screenshots/support-upgrades.png) | [![高亮已分配节点和建议升级路线的天赋树](docs/screenshots/passive-tree.png)](docs/screenshots/passive-tree.png) |

点击图片可查看大图。截图使用虚构的演示角色，不是 BD 攻略。

## 你可以用它做什么

- **买装备前比较换装。** 粘贴从市集复制的完整物品，在适用的装备位置比较每秒伤害（DPS）、
  有效承伤量（EHP）与抗性变化。
- **为主技能挑辅助宝石。** 比较已计算的整套搭配，排除不想用的辅助，并预览等级、品质升级。
- **规划接下来的天赋点。** 寻找包含过路点成本的连通路线，预览节点变化，再决定是否应用。
- **按自己的 BD 搜装备。** 生成带词缀权重、角色等级要求和预算筛选的 PoE2 官方市集链接。
- **看懂数值来源。** 编辑装备、技能与战斗配置，同时查看伤害分解和词条来源。

## 用你的 BD 试一试

1. [打开在线版](https://pobr-web.pages.dev)，无需安装桌面软件。
2. 在「构筑」页粘贴 **PoB2 代码**或 **WeGame PoE2 分享链接**并导入；也可以导入
   `.build` 文件，或新建角色。
3. 核对主技能、战斗配置与未支持效果，再到「提升」「技能」或「天赋树」比较改动。

修改会自动保存在当前浏览器中。可以下载 JSON 备份以保留副本或换设备，也可以导出
PoB2 代码分享修改后的 BD。导入其他构筑会替换当前内容，如需保留，请先下载备份。

PoBR 是以 [Path of Building for PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
为计算参考的独立实现，尚未覆盖全部 PoB2 机制。推荐范围限于已计算的候选与已建模效果；
市集权重用于初筛，不保证找到最优购买方案，购买前请比较完整物品的换装收益。

## 参与改进与关注更新

发现计算差异或未支持词条？欢迎[提交 Issue](https://github.com/ackness/pobr/issues/new)，
附上可公开分享的最小复现构筑、相关物品或技能，以及预期值与实际值。界面反馈和翻译修正也很有帮助。

想参与开发，可以从[贡献指南](AGENTS.md)或[添加词条规则](docs/contributing-mods.md)开始。
如果 PoBR 对你有用，可以 **Star 收藏仓库**；希望收到发版通知，可选择 **Watch → Custom → Releases**。

## 开发者入口

Rust 计算引擎由 WebAssembly 应用和 CLI 共用，支持词条离线预编译，并通过 `TraceGraph`
和 `AttributionReport` 提供计算追踪与来源归因。计算使用稳定 ID，与显示翻译分离；游戏数据
使用版本化 JSON 快照。PoB2 parity 测试保护已记录的回归基线，不代表所有机制均已覆盖。

计算在浏览器内完成；WeGame 和市集 HTTP 适配由 Pages Worker 提供。桌面应用目前仍是
占位入口，WASM/JSON API 与 CLI 仍在迭代，可能随版本变更。

### WASM 性能：PoBR 与 pob-web

按同一批已入库 PoB2 构筑码，实跑 **PoBR Rust WASM** 与
[pob-web 的 Lua WASM 包](https://github.com/krauthaufen/pob-web)。
计时范围为**构筑码导入＋计算＋JSON 响应**，不是缓存属性读取，也不是单独的计算内核。

实测环境：**2026-10-11**，Apple M1 Pro / macOS arm64，Node **26.10.0**；
PoBR `d60488f` 的 release WASM，schema 7，数据 `4.5.5.3`。

#### 最新赛季：Forbidden Rites

于 **2026-10-11** 从 [poe.ninja 最新赛季](https://poe.ninja/poe2/builds/forbiddenrites)
保存 8 个公开 BD，每个基础职业一个，快照为 `0632-20261011-47556`。
[选样与来源证据](examples/wasm-benchmark/selection.json)保留索引 URL、角色及哈希。
选样在计时前完成：每个职业使用一个预先声明的升华筛选，在索引返回的前 100 个角色中
按账号／角色名字典序取首个；不替换运行失败的候选。
[新增样本](examples/wasm-benchmark/builds/)与旧 parity 语料分开，不作为新的数值 golden。

耗时单位为 ms；**加速倍数 = Lua 耗时 / Rust 耗时**。每个完成案例采样九次；
p95 使用 nearest rank，在九次采样下即最大值。

| 职业／升华／主技能 | PoBR 中位数 | Lua 中位数 | 加速倍数 | PoBR p95 | Lua p95 | p95 耗时比 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Druid / Oracle / Lightning Bolt — Lua 报错 | — | — | — | — | — | — |
| Huntress / Amazon / Ice Shot | 11.05 | 773.07 | 69.95× | 11.86 | 1144.06 | 96.49× |
| Mercenary / Gemling Legionnaire / Comet — Lua 报错 | — | — | — | — | — | — |
| Monk / Martial Artist / Ice Shot | 11.18 | 793.01 | 70.92× | 14.02 | 815.01 | 58.14× |
| Ranger / Deadeye / Ice Shot — Lua 报错 | — | — | — | — | — | — |
| Sorceress / Stormweaver / Comet — Lua 报错 | — | — | — | — | — | — |
| Warrior / Titan / His Scattering Calamity — Lua 报错 | — | — | — | — | — | — |
| Witch / Blood Mage / Impending Doom — Lua 报错 | — | — | — | — | — | — |

新赛季六个失败案例均在 Lua `CalcDefence.lua` 触发 `s_format` 参数越界，不发布加速倍数。
**完成案例的数值也有明显差异，PoBR 仍有未支持机制**；这些是 API 工作负载耗时，
不能推广为等价机制或整个计算引擎的通用性能结论：

| 完成的新赛季 BD | PoBR TotalDPS | Lua TotalDPS | PoBR 未支持词条数 |
| --- | ---: | ---: | ---: |
| Amazon / Ice Shot | 88,117.54 | 237,926.57 | 26 |
| Martial Artist / Ice Shot | 2,877,759.58 | 5,173,634.32 | 13 |

两个完成的新赛季案例均未报告物品解析错误。

<details>
<summary>历史对比样本：7 个 BD</summary>

这些是较早的 Runes of Aldur 构筑，不作为最新赛季 BD。按用户要求，从性能基准中
移除了此前报错或结果不稳定的 11 个旧 BD，原有 parity 回归语料保持不变。
这组历史样本是在观察上次结果后筛选的，不代表无偏的历史 BD 整体。

| 职业／升华／主技能 | PoBR 中位数 | Lua 中位数 | 加速倍数 | PoBR p95 | Lua p95 | p95 耗时比 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Huntress / Spirit Walker / Twister | 11.69 | 726.38 | 62.16× | 12.49 | 905.85 | 72.55× |
| Mercenary / Gemling Legionnaire / Explosive Grenade | 9.23 | 779.69 | 84.50× | 10.08 | 1228.82 | 121.89× |
| Ranger / Deadeye / Explosive Grenade | 8.18 | 653.80 | 79.97× | 8.55 | 680.15 | 79.58× |
| Ranger / Pathfinder / Ice Shot | 10.30 | 561.06 | 54.46× | 11.07 | 586.64 | 53.01× |
| Warrior / Smith of Kitava / Shield Wall | 8.96 | 922.45 | 102.92× | 10.61 | 1502.46 | 141.60× |
| Warrior / Titan / Shield Wall | 10.49 | 664.66 | 63.35× | 10.80 | 686.03 | 63.54× |
| Witch / Abyssal Lich / Detonate Dead | 14.35 | 634.40 | 44.21× | 14.97 | 1948.49 | 130.16× |

</details>

| 初始化：单次观测 | PoBR（ms） | Lua（ms） | Lua / Rust |
| --- | ---: | ---: | ---: |
| 引擎与数据启动 | 715.65 | 2579.03 | 3.60× |

#### 测量边界

每个构筑先预热两次，采样时交替两侧执行顺序；PoBR 每次使用不同的合法 JSON
空白，绕过按原始请求字符串缓存的响应。双方计时都包含构筑码解压。
Lua 侧还包含 Node Worker 通信、适配器检查及无界面 UI 状态构建；PoBR 返回的
JSON 契约不同且更详细。初始化不含产物读取、哈希检查和 Lua 宿主 Worker 创建，
但包含模块导入、数据注入及启动。未测浏览器渲染、下载、增量编辑或运行时内存。

报告保留全部 **15 个入选 BD**：**9 个完成对比，6 个最新赛季案例被拒绝**。脚本拒绝被上游吞掉的
Lua 错误、不完整结果及重复计算结果不稳定的案例，不把失败当成“更快”；
任一案例失败时以非零状态退出。速度不能证明机制覆盖完整或数值 parity，
未支持词条与物品诊断均保留在报告中。

[已记录的采样、诊断与产物哈希](devs/ci/pob-web-performance-baseline.json)
提供本次观测的证据。Lua/JS/数据下载自 `pob.awx.at`，部署产物的源码 revision
未知；包装层、shims 与加载补丁单独固定在上游 `ee33ce6`。
保留通过哈希校验的本地快照可重跑同一组产物；重新下载可能得到更新的部署。

#### 打包大小

按同一组引擎与数据产物测量实际字节，**1 MiB = 1,048,576 字节**。
数据包使用 gzip level 9；传输估算同时压缩 WASM 与 JS，不重复压缩已有 gzip 数据包。

| 组件／口径（MiB） | PoBR | pob-web | PoBR / Lua 体积比 |
| --- | ---: | ---: | ---: |
| WASM 原始文件 | 4.891 | 0.250 | 19.56× |
| JavaScript 原始文件 | 0.023 | 0.066 | 0.34× |
| 压缩数据包 | 4.286 | 4.633 | 0.93× |
| WASM＋JS＋压缩数据合计 | 9.200 | 4.949 | 1.86× |
| gzip 传输估算 | 6.072 | 4.763 | 1.27× |

**当前 PoBR 的运行包更大，不是更小。** Rust 计算引擎编译进 WASM；Lua 侧较小的
WASM 是解释器，计算代码放在数据包里。两侧数据内容、语言包及机制覆盖不同；
这些体积不是运行时内存。PoBR 完整发布归档约 **6.058 MiB**，还包含类型声明、
文档、示例与 agent skill；参考部署没有可按同口径比较的完整发布归档。

[Lua 实测体积基线](devs/ci/pob-web-wasm-size-baseline.json)记录哈希、日期及压缩设置。
生成体积报告并验证实际发布包：

```bash
node web/scripts/measure-pob-web-size.mjs --output devs/ci/pob-web-wasm-size-baseline.json
pnpm --dir web package-wasm
pnpm --dir web smoke-wasm-package
node --test web/scripts/measure-pob-web-size.test.mjs web/scripts/package-wasm.test.mjs
# .cache/wasm-release/wasm-size.json 与 wasm-size.md
```

#### 完整脚本与测试

在仓库根目录运行，需要 Node 22+、stable Rust、wasm-pack 与 pnpm：

```bash
pnpm --dir web build-wasm
pnpm --dir web sync-data
node web/scripts/benchmark-pob-web.mjs --prepare
node web/scripts/bench-pob-web.mjs --iterations 9 --warmup 2
# 本次运行存在被拒绝的案例，因此退出码为 1；完整报告仍写入下列文件。
# 原始采样、完整响应、诊断、宿主环境与来源：
# .cache/pob-web-performance.json
node web/scripts/snapshot-pob-web-benchmark.mjs
# 保存精简实测基线，并在 stdout 输出 Markdown 表格行。

# 无需网络、Rust 构建或下载 Lua 的回归测试：
node --test web/scripts/benchmark-comparison.test.mjs web/scripts/benchmark-builds.test.mjs web/scripts/benchmark-pob-web.test.mjs
```

WASM、数据与 Lua 参考快照准备好后，已入库 BD 可离线重复测量。
在线采集或重放另需 Python 3 与 curl：

```bash
# 从站点发现最新赛季，每个基础职业预选一个公开 BD。
python3 web/scripts/fetch-ninja-benchmarks.py --selection base-class-first-v1
# 按已记录的赛季／快照／候选重新抓取到独立目录。
python3 web/scripts/fetch-ninja-benchmarks.py \
  --selection-file examples/wasm-benchmark/selection.json \
  --output .cache/ninja-replay/builds --manifest .cache/ninja-replay/selection.json
python3 -m unittest discover -s web/scripts -p 'fetch_ninja_benchmarks_test.py'
```

采集脚本保留来源 URL、快照、抓取时间、导出 XML 的角色信息与构筑码哈希；
若上游撤销旧快照，重放会明确失败。不会把新增样本加入 parity golden 语料，
也不会替换失败候选以美化性能结果。

完整实现见[下载与 Lua 适配器](web/scripts/benchmark-pob-web.mjs)、
[测量逻辑](web/scripts/benchmark-comparison.mjs)和[可执行入口](web/scripts/bench-pob-web.mjs)。
准备脚本下载实际引擎、数据、固定版本的上游源码与许可证；后续运行校验本地哈希，
可离线复用。快照不完整或被修改时直接报错，不静默修补。
准备新快照用 `--directory <new-dir>`，测量时用 `--reference-dir <new-dir>`；
其余选项见 `--help`。
回归测试覆盖统计计算、参数边界、防缓存调用标识、重复结果稳定性、错误与部分
结果拒绝、安全模板提取、来源及文件篡改。性能基准按需运行，不把机器相关耗时作为 CI 门禁。

### 本地运行

Web 应用请按 [Web 安装指南](web/README.zh-CN.md)准备 WASM、游戏数据并启动 Vite。
体验 CLI 时，复用本机 stable Rust 工具链，在仓库根目录运行：

```bash
# CLI (binary name: pobr)
cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
cargo run -p pobr-cli -- decode-code <pob_code>        # PoB build code -> XML
cargo run -p pobr-cli -- parse-mod "20% increased Fire Damage"
```

修改代码时选择相关的测试和 lint，以下以 Build Code 为例：

```bash
./pobr verify build codec
```

普通本地提交只运行相关检查；合并、发版或影响范围较大的修改由云端 CI 完成完整门禁。
推送发版 tag，或对已推送分支运行 `./pobr ci <pushed-ref>`；本地无需再跑同一套全量验证。
`./pobr full` 保留为显式本地 Rust 门禁，在线版在 `v0.x` tag 通过 CI 后自动部署。
命令、缓存复用和磁盘统计见 [开发工作流](docs/development-workflow.md)。

Rust **edition 2024**；应用与工具使用 workspace 发布版本，与 `v0.x` tag 同步。
七个内部库保持独立版本，避免应用版本更新使整个库依赖链重新编译。

## 架构一览

数据流：

```
GGG .dat 导出
  └─(pobr-data-adapter 离线适配)→ data/<version>/*.json
       └─(pobr-gamedata 运行时 loader)→ 上层计算
```

计算流水线（`pobr-core`）：

```
modifier 文本 → 解析 → ModDb → 聚合查询 → calc
  → OutputTable + Breakdown + TraceGraph + AttributionReport
```

标准属性聚合公式：`(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`。

游戏数据文件读取收口在 `pobr-gamedata`；生产 `pobr-data` / `pobr-core` 不读取数据文件，测试规则加载是显式的测试 feature。应用与工具负责网络及文件输入输出。`pobr-data` 是项目内依赖的底层。

核心源码按 `model/`、`parse/`、`rules/`、`ingest/`、`aggregate/`、`calc/`、`attribute/` 分层。Web 的 `api/wasmBackend.ts` 通过 `apps/pobr-wasm/src/build_api/` JSON 契约调用计算；启动时 fetch 数据并注入内存 `GameData` / `BuildData`。

## Workspace 结构

14 个 member：`crates/` 为 7 个库、`apps/` 为 3 个应用入口、`tools/` 含 4 个 Rust 工具。React / TypeScript 的 `web/` 与 Lua oracle 不在 Cargo workspace 中。

| Crate | 职责 |
|-------|------|
| `crates/pobr-data` | 纯数据定义（catalog schema：BaseItem/Stat/Mod/SkillGem/PassiveNode…），零逻辑零 I/O，所有 crate 的底层依赖 |
| `crates/pobr-core` | Modifier 解析 / 存储 / 聚合 + 计算引擎 + source-level 归因 + 来源接入（item/passive/gem/flask）。零 I/O |
| `crates/pobr-gamedata` | 运行时数据 loader——数据系统里唯一持有文件 I/O 的层，按域懒加载 + i18n 边车 |
| `crates/pobr-i18n` | 语言包加载 / fallback / 显示文本映射（`en-US` canonical + `zh-TW`） |
| `crates/pobr-tree` | 天赋树拓扑、allocated node mod 收集、范围珠宝 |
| `crates/pobr-build` | Build 状态、PoB Build Code 编解码、导入识别、`calc_orchestrator/` + `CalcCache`、Build 对比。**parity 测试主战场** |
| `crates/pobr-item` | raw item 文本的全保真编辑态解析 + 逆向序列化（BuildRaw 往返） |
| `apps/pobr-cli` | CLI：`calculate` / `parse-mod` / `decode-code` / `encode-code` |
| `apps/pobr-wasm` | Web/WASM API：纯 Rust JSON 入出，`wasm` feature 下 wasm-bindgen 绑定 |
| `apps/pobr-desktop` | 示例计算与文本摘要占位入口，尚未接入 GUI 框架 |
| `tools/pobr-data-adapter` | 数据管线适配器：GGG `.dat` 导出 → 反范式化为入库 JSON |
| `tools/sync-pob-catalog` | 从 PoB 核心 Lua 抽取属性 catalog、parity 检查 / diff |
| `tools/lint-i18n` | 语言包完整性检查 |
| `tools/precompile-mods` | mod-parser 规则离线预编译 / 覆盖率报表 |

（`tools/pob2-oracle` 是非 workspace 成员的纯 Lua wrapper，用于 dump PoB2 侧计算分解做逐分量对照。）

## Parity 体系（回归基准）

PoB2 兼容是硬回归门禁，三层校验互补：

1. **`crates/pobr-build/tests/parity/ninja_parity.rs`** — 遍历真实 PoB2 build + 黄金数值，零硬编码对比全部职业 / 技能；`parity_no_regression` 断言聚合命中率不低于基线。
2. **golden / dual-run 套件** — `tests/parity/` 与 `tests/dualrun/` 下的用例钉住中间值与配置语义，由 `parity.rs` / `dualrun.rs` 汇总运行。
3. **`tools/pob2-oracle`** — 需要逐分量定位偏差时，从 vendored PoB2 直接 dump Lua 侧计算分解对照。

```bash
./pobr test build parity parity_baseline_report -- --ignored --nocapture
```

`vendor/PathOfBuilding-PoE2/` 是完整检出，公式核对直接读本地 Lua，不必上网找。

## 文档

- [`AGENTS.md`](AGENTS.md) — 贡献指南与开发入口。
- [`CLAUDE.md`](CLAUDE.md) — 验证分层、命令速查、关键约定（贡献前必读）。
- [`agent-docs/`](agent-docs/) — PoE2（0.5.0）机制中文参考（伤害类型 / 抗性 / 护甲闪避 ES / 暴击 / 异常 / 计算顺序等）。
- [`web/README.md`](web/README.md) — Web 前端。

早期 `devs/docs/architecture/00–16` 设计与审计仅在本机保留；已入库的 18–21 是功能契约。
历史快照不是当前待办或新检出环境的前置条件，当前结构以代码、CLAUDE.md 和相关测试为准。

## 约定

- **计算内部只用稳定 ID**（`StatId` / `ModName` / `SourceId`），显示文本走 `pobr-i18n`。
- **不可变 / 确定性**：calc 函数对 `Env` 的可变写入集中在 `perform`，并行化只在只读快照阶段展开。
- 涉及计算 / Modifier / parser 的改动需补对应集成测试或 golden fixture；改变 crate 边界 / 聚合语义 / catalog / parity 规则时同步更新架构文档。

## 参考与致谢

- [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)（MIT）——
  本项目的参考实现与 parity 回归基准：计算公式、Modifier 语义、specialModList
  解析规则均以其 Lua 实现为准源逐一核对（本地检出到 `vendor/`，不入库；
  钉定 commit 记录在 `data/<version>/overlay/mod_parser_rules.json::_meta`）。
- [poe2db.tw](https://poe2db.tw/) 与 PoE2 Wiki——游戏机制与文本翻译的查证来源。

## License

代码以 [MIT](LICENSE) 协议发布。

本项目与 Grinding Gear Games 无任何关联，亦未获其背书。`data/` 下的游戏数据
派生自 Path of Exile 2 客户端资源，版权归 Grinding Gear Games 所有，仅用于
构建计算的互操作目的（与 Path of Building 社区惯例一致）。
