# PoBR — Path of Building in Rust

[English](README.md) | **简体中文**

**在线版：<https://pobr-web.pages.dev>** —— 每次 `v0.x` 版本 tag 通过 CI 后自动部署。

> **⚠️ 测试版。** PoBR 仍在活跃开发中：计算结果、游戏数据、wasm/JSON API 与
> CLI 都在迭代，可能随时变动或不稳定——暂时不要把重要工作依赖在这些 API 上。

PoBR 是把 [Path of Building (PoE2)](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
核心计算引擎从 Lua **重写**为 Rust 的项目。PoB2 兼容始终是硬性回归基准；重写要解决的是移植解决不了的问题：

- **性能** — 消除大规模 Modifier 聚合、多技能计算的瓶颈；计算核心纯函数 +
  确定性，重负载路径在只读快照上并行展开，词条解析热路径离线预编译、运行时零解析。
- **source-level 归因** — 在 PoB2 对齐之外，每个输出都能回溯到是哪件装备 /
  词条 / 天赋 / 宝石 / 配置贡献的（`TraceGraph` + `AttributionReport`）。
- **原生 i18n** — 计算内部只用稳定 ID，显示文本全部走语言包（`en-US` 基准 +
  `zh-TW`，Web 侧另有 zh-CN 边车），Web 前端甚至支持直接粘贴简中物品文本。
  加一门语言是加数据，不是改代码。
- **WASM 到处跑** — 引擎编译为 WebAssembly、以 JSON 契约暴露，Web 版完全在
  浏览器内计算、无需服务器；同一个核心同时驱动 CLI 与桌面入口。
- **为扩展而设计** — 分层 workspace（data → core → build → apps）+ 数据驱动
  管线：游戏数据是从 GGG `.dat` 导出生成的版本化 JSON，大部分词条/属性行为
  是数据而非硬编码规则。

## 快速上手

标准 cargo 工作流（推荐安装 [`cargo-nextest`](https://nexte.st/) 跑测试）：

```bash
cargo nextest run --workspace          # 全部测试
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# CLI（二进制名 pobr）
cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
cargo run -p pobr-cli -- decode-code <pob_code>        # PoB Build Code → XML
cargo run -p pobr-cli -- parse-mod "20% increased Fire Damage"
```

Web 前端见 [`web/README.md`](web/README.md)（Vite + React + TS，通过 wasm JSON 契约与引擎解耦，不进 cargo workspace）。

Rust **edition 2024**，workspace 版本统一 `0.1.0`。

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

I/O 收口在 `pobr-gamedata` 一处；`pobr-data` / `pobr-core` 维持零 I/O。依赖方向只能向下，`pobr-data` 是最底层。

## Workspace 结构

15 个 member，`crates/` 为库、`apps/` 为可执行、`tools/` 为数据/维护工具：

| Crate | 职责 |
|-------|------|
| `crates/pobr-data` | 纯数据定义（catalog schema：BaseItem/Stat/Mod/SkillGem/PassiveNode…），零逻辑零 I/O，所有 crate 的底层依赖 |
| `crates/pobr-core` | Modifier 解析 / 存储 / 聚合 + 计算引擎 + source-level 归因 + 来源接入（item/passive/gem/flask）。零 I/O |
| `crates/pobr-gamedata` | 运行时数据 loader——数据系统里唯一持有文件 I/O 的层，按域懒加载 + i18n 边车 |
| `crates/pobr-i18n` | 语言包加载 / fallback / 显示文本映射（`en-US` canonical + `zh-TW`） |
| `crates/pobr-tree` | 天赋树拓扑、allocated node mod 收集、范围珠宝 |
| `crates/pobr-build` | Build 状态、PoB Build Code 编解码、导入识别、`CalcOrchestrator`（带缓存）、Build 对比。**parity 测试主战场** |
| `crates/pobr-item` | raw item 文本的全保真编辑态解析 + 逆向序列化（BuildRaw 往返） |
| `crates/pobr-trade` | Trade 查询 / 价格抽象（`TradeBackend` trait + 离线 `MockBackend`） |
| `apps/pobr-cli` | CLI：`calculate` / `parse-mod` / `decode-code` / `encode-code` |
| `apps/pobr-wasm` | Web/WASM API：纯 Rust JSON 入出，`wasm` feature 下 wasm-bindgen 绑定 |
| `apps/pobr-desktop` | 桌面入口最小骨架 |
| `tools/pobr-data-adapter` | 数据管线适配器：GGG `.dat` 导出 → 反范式化为入库 JSON |
| `tools/sync-pob-catalog` | 从 PoB 核心 Lua 抽取属性 catalog、parity 检查 / diff |
| `tools/lint-i18n` | 语言包完整性检查 |
| `tools/precompile-mods` | mod-parser 规则离线预编译 / 覆盖率报表 |

（`tools/pob2-oracle` 是非 workspace 成员的纯 Lua wrapper，用于 dump PoB2 侧计算分解做逐分量对照。）

## Parity 体系（回归基准）

PoB2 兼容是硬回归门禁，三层校验互补：

1. **`crates/pobr-build/tests/ninja_parity.rs`** — 遍历真实 PoB2 build + 黄金数值，零硬编码对比全部职业 / 技能；`parity_no_regression` 断言聚合命中率不低于基线。
2. **golden / dual-run 套件** — 钉住中间值与配置语义。
3. **`tools/pob2-oracle`** — 需要逐分量定位偏差时，从 vendored PoB2 直接 dump Lua 侧计算分解对照。

```bash
cargo test -p pobr-build --test parity -- --nocapture   # parity 仪表盘
```

`vendor/PathOfBuilding-PoE2/` 是完整检出，公式核对直接读本地 Lua，不必上网找。

## 文档

- [`CLAUDE.md`](CLAUDE.md) — 验证分层、命令速查、关键约定（贡献前必读）。
- [`agent-docs/`](agent-docs/) — PoE2（0.5.0）机制中文参考（伤害类型 / 抗性 / 护甲闪避 ES / 暴击 / 异常 / 计算顺序等）。
- [`web/README.md`](web/README.md) — Web 前端。

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
