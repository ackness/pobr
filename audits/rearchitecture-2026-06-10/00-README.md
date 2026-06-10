# PoBR 重构审计 2026-06-10 · 导读

> 审计日期：2026-06-10 · 对照基准：vendor PoB2 Lua 源码（`vendor/PathOfBuilding-PoE2/src`）↔ PoBR Rust workspace
> 性质：**结构性 / 架构性审计**（与上一轮数值 parity 审计互补，见下文 §4）

---

## 1. 背景

PoBR 的目标是把 Path of Building PoE2（PoB2，Lua）的核心计算与业务逻辑重写为 Rust，三个核心诉求：

1. **加速计算** —— 解决大规模 Modifier 聚合 / 多技能并行计算的性能瓶颈；
2. **source-level 数据归因** —— 每个输出都能追踪到装备 / 词条 / 天赋 / 宝石 / 配置的贡献（PoBR 相对 PoB 的核心增量）；
3. **数据与框架彻底分离** —— PoB2 的 Lua 代码大量是由游戏数据自动生成的"数据类代码"（`src/Data/` 约 38MB，如 `ModCache.lua`、Gems/Skills/Uniques/Minions 数据文件），且生成物中混入了人工策展层（Export 模板 `#baseMod`/`#flags`/`#set`、per-skill statMap override、specialModList 等）。PoBR 希望框架代码稳定，每个游戏版本只需更新 `data/<版本>/*.json`。

现有数据管线：`pipeline/`（下载 GGG dat 表）→ `tools/pobr-data-adapter`（离线转 JSON）→ `data/4.5.0.3.4/*.json`（schema = `crates/pobr-data/src/catalog.rs`）→ `crates/pobr-gamedata`（运行时 loader）→ 上层计算（`pobr-core` / `pobr-build`）。

本轮审计回答的核心问题：**对照 PoB2 的完整实现，PoBR 在结构上还缺什么？哪些缺口是"逻辑没写"，哪些是"数据没入库"，哪些是"数据被错误地固化进了框架"（违背诉求 3）？**

## 2. 方法

**10 领域对照分析 + 对抗核查**：

- 把 PoB2 计算与业务逻辑切成 10 个领域（见 §3 索引），每个领域由独立分析对 PoB2 Lua 一手源码与 PoBR 对应 crate 做结构对照：
  - 先画 PoB2 该领域的**代码结构地图**（模块/类/数据文件职责与数据流）；
  - 再列 PoBR 实现现状，逐项产出**缺口清单**（🔴 high / 🟡 medium / 🟢 low，附 missing/partial/design 分类）；
  - 每个领域文档附**"数据 vs 逻辑切分建议"**——哪些应 JSON 化随版本更新、哪些留在框架，以及当前 `catalog.rs` schema 还缺的表/字段（直接服务于诉求 3）；
  - 文档末尾附核查说明（verification_notes），每条缺口标注 PoB2 Lua 行号区间 ↔ PoBR 源文件位置，可独立复核。
- **对抗核查**：领域结论由第二方对照源码行号抽查复核，剔除误报（如 PoBR 已实现但放在非对应文件的情况），并与上一轮 parity 审计的已修结论去重（本轮各文档明确声明"不重复 01-01～01-06 等已修项"）。
- 大文件（`CalcOffence.lua` 343KB / `ModParser.lua` 642KB / `CalcDefence.lua` 229KB）按函数名/章节注释 grep 定位后分段精读，不整读。

严重级口径：

| 级别 | 含义 |
|------|------|
| 🔴 high | 缺失导致大类 build 计算结果错误/不可算，或结构性违背"数据框架分离"目标 |
| 🟡 medium | 特定机制/词条族不正确，或有 workaround 但口径偏差 |
| 🟢 low | 边缘机制、查询原语、便利性缺口 |

## 3. 文档索引

| 编号 | 文档 | 领域 | 🔴/🟡/🟢 |
|------|------|------|----------|
| 10 | [10-mod-system.md](10-mod-system.md) | Modifier 系统（解析/存储/聚合：ModParser/ModStore/ModDB/ModCache） | 3/4/1 |
| 11 | [11-calc-orchestration.md](11-calc-orchestration.md) | 计算编排（CalcSetup/CalcPerform/环境、buff/aura/curse 阶段） | 2/7/2 |
| 12 | [12-offence.md](12-offence.md) | 进攻计算（伤害/暴击/速度/DPS、双武器 pass、ModFlags） | 6/6/2 |
| 13 | [13-defence.md](13-defence.md) | 防御计算（护甲/闪避/ES/承伤转换/扣池/EHP） | 5/7/4 |
| 14 | [14-triggers-minions-ailments.md](14-triggers-minions-ailments.md) | 触发/召唤物/异常状态 | 6/5/1 |
| 15 | [15-data-pipeline.md](15-data-pipeline.md) | 数据层与导出管线（**核心：数据/框架分离**，statdesc/SkillStatMap/Export 策展层） | 5/5/3 |
| 16 | [16-items.md](16-items.md) | 物品系统（variant/range/局部 mod/BaseItemDef schema） | 4/8/3 |
| 17 | [17-passive-tree.md](17-passive-tree.md) | 天赋树（武器组节点/节点效果缩放/ReplaceNode） | 3/5/4 |
| 18 | [18-skills-gems.md](18-skills-gems.md) | 技能与宝石（quality 链路/support 裁决/statSet） | 4/4/2 |
| 19 | [19-config-build-display.md](19-config-build-display.md) | 配置/Build/展示层（ConfigOptions/customMods/Enemy Stats） | 3/7/2 |
| 20 | 20-architecture.md | 跨领域架构综合：目标架构与数据/逻辑切分总图（基于 10–19 汇总，撰写中） | — |
| 21 | 21-roadmap.md | 重构路线图：缺口修复分波次排期（基于 01 排序，撰写中） | — |
| — | [01-gap-summary.md](01-gap-summary.md) | 跨领域缺口汇总（总览表 + 全部 high 清单 + 正确性影响 Top 10） | 41/58/24 |

## 4. 与上一轮审计（`audits/pob2-parity-2026-06-09/`）的关系

| | 上一轮（2026-06-09） | 本轮（2026-06-10） |
|---|---|---|
| 性质 | **数值 parity 审计** | **结构性 / 架构性审计** |
| 问法 | "已实现的计算路径，数值口径是否与 PoB2 一致？"（聚合舍入、flags 匹配语义、触发冷却 cap、ES 充能拆分等） | "对照 PoB2 完整结构，整块还缺什么？数据与框架的边界切对了没有？" |
| 范围 | 6 个子系统（mod 解析聚合 / setup / perform / offence / DPS / defence），限于已有代码路径 | 10 个领域，覆盖物品/天赋树/宝石/触发/召唤物/数据管线/配置展示等上一轮未涉足的整块 |
| 结论形态 | finding → 修复（1 CRITICAL + 10 HIGH + 多波 MED/LOW 已落地，997 tests + ninja_parity 门禁全绿） | 缺口清单 + 数据/逻辑切分建议 + 架构与路线图输入，**不直接产出代码修复** |
| 衔接 | 本轮各领域文档**前置已读其 FINDINGS.md**，不重复其已修结论；其 defer 项（触发 support 链路、催化剂、add_skill_types 等）在本轮对应领域中以结构缺口形式重新归位 | 本轮 high 缺口（共 41 条）将经 21-roadmap 排期，进入与上一轮相同的"实现 → ninja_parity 门禁 → 合并"循环 |

一句话：**上一轮校准了"已建成部分"的刻度；本轮丈量"尚未建成部分"的版图，并审查地基（数据/框架分离）是否打歪。**
