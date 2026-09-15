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

### 本地运行

Web 应用请按 [Web 安装指南](web/README.zh-CN.md)准备 WASM、游戏数据并启动 Vite。
体验 CLI 时，使用仓库配置的 Rust 工具链，在仓库根目录运行：

```bash
# CLI (binary name: pobr)
cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
cargo run -p pobr-cli -- decode-code <pob_code>        # PoB build code -> XML
cargo run -p pobr-cli -- parse-mod "20% increased Fire Damage"
```

修改代码时选择相关的测试和 lint，以下以 Build Code 为例：

```bash
cargo test -p pobr-build --test codec
bash .claude/skills/run-pobr/driver.sh lint -p pobr-build --lib --test codec
```

普通本地提交无需全量检查；合并、发版或影响范围较大的修改按 [CLAUDE.md](CLAUDE.md)
运行一次 `driver.sh full`（nextest + doctest，无 nextest 时回退 Cargo）。在线版在 `v0.x`
版本 tag 通过 CI 后自动部署。

Rust **edition 2024**，全部 crate 共享一个 workspace 版本，与 `v0.x` 发布 tag 保持同步。

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
cargo test -p pobr-build --test parity -- --nocapture   # parity 仪表盘
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
