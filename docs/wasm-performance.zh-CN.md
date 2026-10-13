# WASM 性能与 BD 分析

[English](wasm-performance.md) | **简体中文**

按同一批已入库 PoB2 构筑码，实跑 **PoBR Rust WASM** 与
[pob-web 的 Lua WASM 包](https://github.com/krauthaufen/pob-web)。
计时范围为**构筑码导入＋计算＋JSON 响应**，不是缓存属性读取，也不是单独的计算内核。

后续排查与版本核对见[导入与计算问题排查](#导入与计算问题排查2026-10-12)。

测量时间 **2026-10-12T16:13Z**，Apple M4 / macOS 27.0 arm64，Node **26.11.1**。
PoBR 为来自 `18b6a03` 的 release WASM（rustc 1.99.0、wasm-pack 0.15.0），schema 8，
数据 `0.5.5.4.2`。构建时唯一未提交的改动是 `mise.toml`，因此构建凭据中 `dirty: true`；
编译源码指纹 `02320289…` 覆盖了被编译的源码。Lua 侧为线上部署的 pob-web 包
（PoB2 运行时 `v0.23.0`），产物已通过哈希校验。每个构筑预热两次、采样九次。

## 最新赛季：Forbidden Rites

于 **2026-10-11** 从 [poe.ninja 最新赛季](https://poe.ninja/poe2/builds/forbiddenrites)
保存 8 个公开 BD，每个基础职业一个，快照为 `0632-20261011-47556`。
[选样与来源证据](../examples/wasm-benchmark/selection.json)保留索引 URL、角色及哈希。
选样在计时前完成：每个职业使用一个预先声明的升华筛选，在索引返回的前 100 个角色中
按账号／角色名字典序取首个；不替换运行失败的候选。
[新增样本](../examples/wasm-benchmark/builds/)与旧 parity 语料分开，不作为新的数值 golden。

耗时单位为 ms；**加速倍数 = Lua 耗时 / Rust 耗时**。每个完成案例采样九次；
p95 使用 nearest rank，在九次采样下即最大值。

| 职业／升华／主技能 | PoBR 中位数 | Lua 中位数 | 加速倍数 | PoBR p95 | Lua p95 | p95 耗时比 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Druid / Oracle / Lightning Bolt † | 10.07 | Lua 报错 | — | 10.43 | — | — |
| Huntress / Amazon / Ice Shot | 8.90 | 515.99 | 57.98× | 9.29 | 890.59 | 95.91× |
| Mercenary / Gemling Legionnaire / Comet † | 10.86 | Lua 报错 | — | 11.18 | — | — |
| Monk / Martial Artist / Ice Shot | 9.57 | 529.74 | 55.33× | 10.23 | 547.08 | 53.47× |
| Ranger / Deadeye / Ice Shot † | 7.85 | Lua 报错 | — | 8.00 | — | — |
| Sorceress / Stormweaver / Comet † | 9.27 | Lua 报错 | — | 10.19 | — | — |
| Warrior / Titan / His Scattering Calamity † | 8.07 | Lua 报错 | — | 8.30 | — | — |
| Witch / Blood Mage / Impending Doom † | 11.86 | Lua 报错 | — | 12.00 | — | — |

† 为同一次会话中的 PoBR 单独测量（`bench-pob-web.mjs --pobr-only`，2026-10-12T16:15Z），
预热、采样次数和稳定性校验与配对测试相同；这些调用没有与 Lua 交替执行，不是配对结果。
[原始采样、实际所选技能、诊断与哈希](../devs/ci/pobr-only-performance-2026-10-12-m4.json)。

**六个 Lua 报错属于 pob-web Lua WASM 的兼容问题，不是 PoBR 导入失败**：
`CalcDefence.lua` 把无穷大传入整数 `s_format` 格式化（五个在第 2789 行，Deadeye 在第 3844 行）。
保留 PoBR 耗时，但不为中断的 Lua 计算发布耗时或加速倍数。
**完成配对案例的数值也有明显差异，PoBR 仍有未支持机制**；这些是 API 工作负载耗时，
不能推广为等价机制或整个计算引擎的通用性能结论：

| 完成的新赛季 BD | PoBR TotalDPS | Lua TotalDPS | PoBR 未支持词条数 |
| --- | ---: | ---: | ---: |
| Amazon / Ice Shot | 88,117.54 | 237,926.57 | 26 |
| Martial Artist / Ice Shot | 2,900,388.52 | 5,173,634.32 | 13 |

15 个 BD 均无物品解析错误。

> 后续更正：这次测量所用版本在 `{ pob_code }` 请求下不报告构筑码内读不出的物品。
> Stormweaver 的胸甲（`Rarity: RELIC`）当时被丢弃，它的防御和伤害是缺了这件装备的数值；
> 现在该稀有度可以读取，其余读不出的物品会出现在 `item_errors`。本节数值保持原测量。

<details>
<summary>历史对比样本：7 个 BD</summary>

这些是较早的 Runes of Aldur 构筑，不作为最新赛季 BD。按用户要求，从性能基准中
移除了此前报错或结果不稳定的 11 个旧 BD，原有 parity 回归语料保持不变。
这组历史样本是在观察较早一次结果后筛选的，不代表无偏的历史 BD 整体。
它们的 `TotalDPS` 与 Lua 相差不超过 1.8%（最大为 Pathfinder，1.73%），这是观测结果，不是 parity 结论。

| 职业／升华／主技能 | PoBR 中位数 | Lua 中位数 | 加速倍数 | PoBR p95 | Lua p95 | p95 耗时比 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Huntress / Spirit Walker / Twister | 10.69 | 501.93 | 46.96× | 11.27 | 642.42 | 57.02× |
| Mercenary / Gemling Legionnaire / Explosive Grenade | 8.49 | 547.25 | 64.46× | 8.95 | 834.05 | 93.19× |
| Ranger / Deadeye / Explosive Grenade | 7.38 | 435.78 | 59.06× | 7.79 | 874.92 | 112.37× |
| Ranger / Pathfinder / Ice Shot | 9.01 | 373.88 | 41.47× | 9.19 | 375.55 | 40.88× |
| Warrior / Smith of Kitava / Shield Wall | 8.08 | 658.55 | 81.46× | 8.60 | 1471.68 | 171.03× |
| Warrior / Titan / Shield Wall | 9.38 | 444.76 | 47.44× | 10.17 | 450.95 | 44.35× |
| Witch / Abyssal Lich / Detonate Dead | 13.48 | 426.56 | 31.65× | 14.44 | 445.93 | 30.88× |

</details>

| 初始化：单次观测 | PoBR（ms） | Lua（ms） | Lua / Rust |
| --- | ---: | ---: | ---: |
| 引擎与数据启动 | 463.10 | 1898.10 | 4.10× |

九个配对 BD 的加速倍数为 **31.65×–81.46×**；全部 15 个 BD 的 PoBR 中位数为 6.83–13.13 ms。
四分钟后又完整跑了一次配对测试：每个加速倍数相差不超过 6.8%，每个 PoBR 中位数相差不超过
11.0%（Lua 为 10.7%），所以记录的这一次对这台机器有代表性，但仍然只是一台机器。
[配对采样、诊断与产物哈希](../devs/ci/pob-web-performance-2026-10-12-m4.json)。

## 测量边界

配对测试中，每个构筑先预热两次，采样时交替两侧执行顺序；PoBR 每次使用不同的合法 JSON
空白，绕过按原始请求字符串缓存的响应。双方计时都包含构筑码解压。
Lua 侧还包含 Node Worker 通信、适配器检查及无界面 UI 状态构建；PoBR 返回的
JSON 契约不同且更详细。初始化不含产物读取、哈希检查和 Lua 宿主 Worker 创建，
但包含模块导入、数据注入及启动。未测浏览器渲染、下载、增量编辑或运行时内存。

报告保留全部 **15 个入选 BD**：**9 个完成对比，6 个最新赛季案例被拒绝**。脚本拒绝被上游吞掉的
Lua 错误、不完整结果及重复计算结果不稳定的案例，不把失败当成“更快”；
任一案例失败时以非零状态退出。速度不能证明机制覆盖完整或数值 parity，
未支持词条与物品诊断均保留在报告中。

Lua/JS/数据下载自 `pob.awx.at`，部署产物的源码 revision 未知，其 SHA-256 记录在配对记录中；
包装层、shims 与加载补丁单独固定在上游 `ee33ce6`。
保留通过哈希校验的本地快照可重跑同一组产物；重新下载可能得到更新的部署。

## 打包大小

按同一组引擎与数据产物测量实际字节，**1 MiB = 1,048,576 字节**。
数据包使用 gzip level 9；传输估算同时压缩 WASM 与 JS，不重复压缩已有 gzip 数据包。
PoBR 数字来自本检出的 `package-wasm` 报告（v0.0.36，schema 8，数据 `0.5.5.4.2`）。

| 组件／口径（MiB） | PoBR | pob-web | PoBR / Lua 体积比 |
| --- | ---: | ---: | ---: |
| WASM 原始文件 | 5.057 | 0.250 | 20.23× |
| JavaScript 原始文件 | 0.023 | 0.066 | 0.34× |
| 压缩数据包 | 4.340 | 4.633 | 0.94× |
| WASM＋JS＋压缩数据合计 | 9.420 | 4.949 | 1.90× |
| gzip 传输估算 | 6.191 | 4.763 | 1.30× |

**当前 PoBR 的运行包更大，不是更小。** Rust 计算引擎编译进 WASM；Lua 侧较小的
WASM 是解释器，计算代码放在数据包里。两侧数据内容、语言包及机制覆盖不同；
这些体积不是运行时内存。PoBR 完整发布归档约 **6.182 MiB**，
还包含类型声明、文档、示例与 agent skill；参考部署没有可按同口径比较的完整发布归档。

[Lua 实测体积基线](../devs/ci/pob-web-wasm-size-baseline.json)记录哈希、日期及压缩设置。
生成体积报告并验证实际发布包：

```bash
node web/scripts/measure-pob-web-size.mjs --output devs/ci/pob-web-wasm-size-baseline.json
pnpm --dir web package-wasm
pnpm --dir web smoke-wasm-package
node --test web/scripts/measure-pob-web-size.test.mjs web/scripts/package-wasm.test.mjs
# .cache/wasm-release/wasm-size.json 与 wasm-size.md
```

## 完整脚本与测试

在仓库根目录运行。`mise run setup` 会装好所需的全部环境：`rust-toolchain.toml` 钉定的 Rust
及 `wasm32-unknown-unknown` target、wasm-pack、Node 26、pnpm 12 与 Web 依赖。
下面的命令需要在已激活 mise 的 shell 中运行，或在每条前加 `mise exec --`。
首次 `build-wasm` 还会编译一次 `wasm-bindgen-cli`（wasm-pack 会缓存），多花几分钟。

```bash
mise run setup
pnpm --dir web build-wasm
pnpm --dir web sync-data
node web/scripts/benchmark-pob-web.mjs --prepare
node web/scripts/bench-pob-web.mjs --iterations 9 --warmup 2
# 本次运行存在被拒绝的案例，因此退出码为 1；完整报告仍写入下列文件。
# 原始采样、完整响应、诊断、宿主环境与来源：
# .cache/pob-web-performance.json
node web/scripts/bench-pob-web.mjs --pobr-only --iterations 9 --warmup 2
# 只测 PoBR，覆盖全部 15 个 BD，无需 Lua 参考快照（即标 † 的行）。
# 报告写入 .cache/pobr-performance.json，不会覆盖上面的配对报告。
node web/scripts/snapshot-pob-web-benchmark.mjs --output devs/ci/pob-web-performance-<date>.json
node web/scripts/snapshot-pob-web-benchmark.mjs --input .cache/pobr-performance.json \
  --output devs/ci/pobr-only-performance-<date>.json
# 保存精简实测记录（保留原始采样与诊断，不含完整分解响应），并在 stdout 输出 Markdown 表格行。
# 必须指定 --output。

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

完整实现见[下载与 Lua 适配器](../web/scripts/benchmark-pob-web.mjs)、
[测量逻辑](../web/scripts/benchmark-comparison.mjs)和[可执行入口](../web/scripts/bench-pob-web.mjs)。
准备脚本下载实际引擎、数据、固定版本的上游源码与许可证；后续运行校验本地哈希，
可离线复用。快照不完整或被修改时直接报错，不静默修补。
准备新快照用 `--directory <new-dir>`，测量时用 `--reference-dir <new-dir>`；
其余选项见 `--help`。
回归测试覆盖统计计算、参数边界、防缓存调用标识、重复结果稳定性、错误与部分
结果拒绝、安全模板提取、来源及文件篡改。性能基准按需运行，不把机器相关耗时作为 CI 门禁。

## 导入与计算问题排查（2026-10-12）

本次排查使用已入库的基准 BD，未另收到用户失败构筑码。

### PoBR：导入成功，但机制覆盖不完整

实跑 **15 个 BD**，均通过 `calculateBuildJson` 和输出校验，`item_errors` 全部为零，
使用当前 release WASM（schema 8，数据 `0.5.5.4.2`）。运行成功不代表数值 parity。

| 新赛季 BD | 未支持诊断条目数 | 物品解析错误 |
| --- | ---: | ---: |
| Oracle / Lightning Bolt | 23 | 0 |
| Amazon / Ice Shot | 26 | 0 |
| Gemling Legionnaire / Comet | 23 | 0 |
| Martial Artist / Ice Shot | 13 | 0 |
| Deadeye / Ice Shot | 8 | 0 |
| Stormweaver / Comet | 15 | 0 |
| Titan / His Scattering Calamity | 17 | 0 |
| Blood Mage / Impending Doom | 17 | 0 |

Blood Mage 选中 `ChaosFrogExplosionPlayer`（辅助宝石授予的效果 2，`TotalDPS` 1,787.28）。
Doom Blast 是效果 3，由 `skillMetricsJson` 单独报告
（见 `crates/pobr-build/tests/skills/skill_metrics.rs` 与 [Web 功能契约](web-contracts.md)）。
这条耗时不断言哪个效果才是该 BD 玩家眼中的主要伤害技能。

当前诊断中的缺口包括 Bonded 效果、条件重复攻击、常驻药剂效果与额外投射物机制。
Pathfinder 样本明确报告 Fork 的
`support_fork_forked_projectile_damage_+%_final`：
`conditional numerical effect not modeled (UnsupportedTag("StatThreshold"))`。
诊断条目中也有元数据与拆行文本，不能把条数等同于缺失机制数；
上面 Ice Shot 的 DPS 差距尚未逐项归因，不能全部算在某条词缀上。

基准里的六行配对失败不等于六次 PoBR 导入失败：
[`bench-pob-web.mjs`](../web/scripts/bench-pob-web.mjs) 在 Lua 失败时拒绝整个配对，
只保留错误，不保留该轮已经完成的 PoBR 结果；`--pobr-only` 会单独测量这些 BD。

### Lua：将无穷大传给整数格式化

对新下载且通过哈希校验的部署快照重放八个新赛季 BD，重现六个失败、两个成功。
临时探针调用原始格式化函数并重新抛出错误，未吞错、修补输出或修改正式适配器。

- 五个案例在 `CalcDefence.lua:2789` 执行
  `s_format("= %d", poolProtected)`，实测参数为 `inf`。
  部署代码在 `sharedMindOverMatter >= 100` 时显式令 `poolProtected = math.huge`。
- Deadeye 在 `CalcDefence.lua:3844` 执行
  `s_format("Total Effective Dot Pool: %d", output[damageType.."DotEHP"])`，
  实测参数同样为 `inf`；前一步是 `TotalPool / TakenDotMult`。
- 部署解释器报告 `Lua 5.2`。最小探针中，`%d` 格式化 `1.5` 成功输出 `1`；
  `math.huge`、NaN、`2147483648` 则触发 `not a number in proper range`。
  本机 LuaJIT 接受这些调用。六个实际失败均是无穷大，并非普通小数。

这是导入后的防御明细格式化兼容问题，不是构筑码损坏。
对应[上游防御代码](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/99bc6e107fd36ae541b349fc36a44281cb371313/src/Modules/CalcDefence.lua#L2773-L2789)
及 [Lua 5.2 格式化实现](https://www.lua.org/source/5.2/lstrlib.c.html#str_format)。
重新下载相同产物不会解决；
[重新下载 PoB2 源码的对照](#重新下载-pob2-源码对照)也在 v0.23.1 重现了相同错误。
基准必须继续拒绝这些中断的计算。

### Lua WASM 是否最新

核对日期：**2026-10-12**。“最新”需要分层回答：

| 层次 | 实测／查询结果 | 结论 |
| --- | --- | --- |
| pob-web 包装层与 shims | 固定提交 `ee33ce63920f23e634cc30e37d8ad339244dbf34` 等于上游 HEAD | 最新 |
| 公开部署产物 | 新下载的 `lua.wasm`、`lua.mjs`、`pob-lua.json` 的 SHA-256 全部与原基准一致 | 与当前部署相同 |
| 内含 PoB2 | 运行时报 `v0.23.0`，上游最新发布为 `v0.23.1` | 不是最新 PoB2 |

来源：[pob-web 提交](https://github.com/krauthaufen/pob-web/commit/ee33ce63920f23e634cc30e37d8ad339244dbf34)、
[本次核对的 PoB2 最新发布](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/releases/tag/v0.23.1)、
[已记录的产物哈希](../devs/ci/pob-web-wasm-size-baseline.json)。
包装层源码固定的 PoB2 子模块是 `99bc6e1`；
本次查询的 PoB2 HEAD
[`bb52d6b`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/bb52d6b368307457eb9c54bb13f1829993d390b1)
比该源码固定点多 115 个提交。这不能证明部署产物的精确源码提交，
`deployedRevision` 仍未知。
包装层的[解释器构建脚本](https://github.com/krauthaufen/pob-web/blob/ee33ce63920f23e634cc30e37d8ad339244dbf34/packages/lua-wasm/scripts/build.mjs)
使用 PUC Lua 5.2.4，而不是 LuaJIT；升级语言版本本身不等于修复兼容性。

复核部署时使用新目录，保留原基准快照：

```bash
node web/scripts/benchmark-pob-web.mjs --prepare \
  --directory .cache/pob-web-freshness-2026-10-12
# 对比 provenance.json 的 files[].sha256 与已记录的产物哈希。
# 部署发生变化时记录新观测，不覆盖旧计时表。
```

### 重新下载 PoB2 源码对照

于 **2026-10-12** 将 PoB2 最新发布版
[`v0.23.1`／`7d6f530`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/7d6f530cbdab20389ff8bc6ba97a37ac27f74e41)
和当前上游
[`HEAD`／`bb52d6b`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/bb52d6b368307457eb9c54bb13f1829993d390b1)
重新下载到独立临时源码快照，使用 pob-web 固定版本的
[打包脚本](https://github.com/krauthaufen/pob-web/blob/ee33ce63920f23e634cc30e37d8ad339244dbf34/packages/pob-data/bundle-lua.mjs)
生成 Lua／数据包，仅调整输入与输出目录，没有修补 PoB2 源码。
部署的 WASM／JS、包装层、shims 和加载补丁均保持不变；
临时适配器参数仅在 MEMFS 加载前指定实验数据包。
这是本地重建包，**不是 pob-web 发布了新部署**。

| PoB2 数据包 | 相对线上包的内容变化 | 同一批 15 个 BD 的结果 |
| --- | --- | --- |
| 线上 v0.23.0 | 原始参考 | 9 个完成配对，6 个 Lua 报错 |
| 新下载 v0.23.1 | 4 个文件变化，防御模块未变 | 相同 9 个完成，相同 6 个 `s_format` 报错 |
| 新下载 HEAD `bb52d6b` | 191 个文件变化，新增 8 个 | 引擎启动失败，尚未进入 BD 计算 |

最新发布版仍在 `CalcDefence.lua:2789`、`:3844` 失败，运行时版本信息已变为
`v0.23.1`。九个成功案例的 Lua 已记录选定属性（`Life`、`TotalDPS`、
`CombinedDPS`、`TotalEHP`）与旧部署完全相同；不据此声称所有明细或机制都没变化。

HEAD 更早失败：`Modules/Main.lua:340: syntax error near '+'`。
该[源码行](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/bb52d6b368307457eb9c54bb13f1829993d390b1/src/Modules/Main.lua#L340)
是 `count += 1`，当前 Lua WASM 加载路径无法解析。
它不是特定 BD 的错误；本实验也不能证明 PoB2 桌面版无法运行。

v0.23.1 包完成了同样的九个 BD，记录的统计值相同，同样的六个失败；本节不报告该对照的耗时。
[完整对照证据](../devs/ci/pob2-refresh-comparison-2026-10-12.json)
保留源码提交、数据包哈希、文件变化清单、原始采样、诊断，以及精确错误。
Lua 报错和 HEAD 启动失败均不发布加速倍数。

**结论：当前 pob-web Lua WASM／适配组合存在已复现的兼容问题；
只替换成最新 PoB2 发布版不能解决，当前 HEAD 还需要额外兼容工作才能启动。**
这不抵消 PoBR 自身的未支持机制。
