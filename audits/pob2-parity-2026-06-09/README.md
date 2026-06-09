# PoBR ↔ PoB2 计算引擎 Parity 审查报告

**审查日期**：2026-06-09
**审查对象**：`crates/pobr-core` 计算引擎（mod 解析/聚合 · 环境装配 · perform 编排 · 伤害核心 · 命中/暴击/异常/DPS · 防御/EHP）及 `crates/pobr-build` 编排层接线
**对照基准**：vendor PoB2 Lua 源码（`CalcSetup.lua` / `CalcPerform.lua` / `CalcOffence.lua` / `CalcDefence.lua` / `CalcTriggers.lua` / `ModList.lua` / `ModStore.lua` / `Global.lua` / `ConfigOptions.lua` / `Item.lua` / `Data.lua`）
**交叉参考文档**：`agent-docs/pob2-calc-engine.md`（PoB2 计算引擎实现解构）

## 1. 审查目的

PoBR 的目标是将 PathOfBuilding（Lua）核心计算引擎迁移到 Rust，并以 **PoB2 兼容为回归基准**。本次审查逐子系统对照 vendor PoB2 权威实现，定位 PoBR 与 PoB2 之间的**方向性数值偏差**、**潜在正确性地雷**与**功能缺口**，输出可执行的修复方案（具体到文件/函数/公式），供后续 parity harness 对账与修复排期使用。

## 2. 审查范围

| 编号 | 子系统 | Rust 模块 | 报告文件 |
|------|--------|-----------|----------|
| 01 | Modifier 解析与 ModDb 聚合 | `mod_parser.rs` / `mod_db.rs` / `modifier.rs` / `config.rs` | [01-mod-parse-aggregate.md](01-mod-parse-aggregate.md) |
| 02 | 环境/来源装配 (Setup) | `setup_env.rs` / `env` / `actor` / `skill_source.rs` / `item.rs` / `item_text.rs` / `passive.rs` | [02-setup.md](02-setup.md) |
| 03 | 编排 (Perform / Buff / 技能时序 / 触发) | `perform.rs` / `skill_use_time.rs` / `skill_mechanics.rs` / `trigger.rs` | [03-perform.md](03-perform.md) |
| 04 | 伤害核心 (转换 / gain-as-extra / inc-more) | `calc/damage.rs` + `offence.rs` | [04-offence-core.md](04-offence-core.md) |
| 05 | 命中 / 暴击 / 异常 / DPS | `crit.rs` / `ailment.rs` / `offence.rs` | [05-offence-dps.md](05-offence-dps.md) |
| 06 | 防御与 EHP | `defence.rs` / `ehp.rs` / `survivability.rs` / `stat_boundary.rs` | [06-defence.md](06-defence.md) |

## 3. 审查方法

1. 对每个子系统的 Rust 实现逐函数比对 vendor PoB2 对应 Lua 源码行（公式系数、取整规则、聚合语义、cap/clamp 边界、tag/condition 门控）。
2. 区分三类问题：**方向性数值偏差**（结果系统性偏高/偏低）、**潜在正确性地雷**（当前未触发但一旦相关词条/路径启用即错）、**功能缺口**（机制尚未建模）。
3. 每条 finding 标注 severity，给出 PoB2 行为（引用 Lua 行号）、PoBR 现状（`file:line`）、可执行修复方案。
4. 数据来源以一手 vendor 源码为准；`agent-docs/pob2-calc-engine.md` 作为开发输入交叉参考（非最终权威）。

> 注：部分 vendor 文件（如 `ModParser.lua`）在本地为部分检出，全量核验需走 `gh` 取源（见 05-02 修复方案）。

## 4. Severity 汇总

| Severity | 数量 | 含义 |
|----------|------|------|
| CRITICAL | 1 | 系统性数值方向错误 / 关键机制完全丢失，必须优先修复 |
| HIGH | 10 | 方向性错误或功能不可用，应尽快修复 |
| MEDIUM | 14 | 边界/特定场景数值错误，可维护性/口径问题 |
| LOW | 10 | 影响面小、对称化/精度差异、架构分工差异 |
| INFO | 2 | 已知简化/边角机制，记录在案 |
| **合计** | **37** | |

按子系统分布：

| 子系统 | CRITICAL | HIGH | MEDIUM | LOW | INFO | 小计 |
|--------|:-:|:-:|:-:|:-:|:-:|:-:|
| 01 Mod 解析/聚合 | 0 | 2 | 2 | 2 | 0 | 6 |
| 02 Setup 装配 | 1 | 1 | 3 | 1 | 0 | 6 |
| 03 Perform/Trigger | 0 | 2 | 2 | 2 | 0 | 6 |
| 04 伤害核心 | 0 | 1 | 1 | 2 | 1 | 5 |
| 05 命中/暴击/异常/DPS | 0 | 2 | 3 | 2 | 0 | 7 |
| 06 防御/EHP | 0 | 2 | 3 | 1 | 1 | 7 |
| **合计** | **1** | **10** | **14** | **10** | **2** | **37** |

## 5. Top 优先修复清单（跨子系统 CRITICAL / HIGH）

按「方向性影响 + 真实 build 触发面」排序：

| 优先级 | ID | severity | 标题 | 影响 |
|:-:|----|----------|------|------|
| P0 | 02-01 | CRITICAL | Boss 元素穿透（Pinnacle+3%/Uber+8%）完全未注入玩家 modDB | Boss 档进攻数值系统性偏高，所有元素 build 受影响 |
| P1 | 03-02 | HIGH | 触发 source_rate 误用主技能速率 + build 层未注入触发数据 | 触发类 build 输出在真实场景恒为 0（功能不可用） |
| P1 | 03-01 | HIGH | 冷却驱动触发未走 rotation 模拟、未乘 triggerChance | 触发速率口径错误（缺命中×暴击×触发几率折算） |
| P1 | 06-01 | HIGH | EHP/max-hit 漏算 DamageTakenWhenHit 承受乘数 | 受击 EHP/max-hit 系统性偏乐观 |
| P1 | 06-02 | HIGH | 元素走护甲时 armour DR 用 post-resist 而非 raw | 元素 max-hit 偏乐观 |
| P1 | 05-01 | HIGH | 异常暴击加权用裸暴击率，未做 over-stacking 修正 | 异常 DPS 偏差（叠层 build 明显） |
| P1 | 05-02 | HIGH | 异常 magnitude DoT 词条与 AilmentEffect 双重/错位计数 | 异常 DPS 存在 ×2 风险 |
| P2 | 04-01 | HIGH | 缺失 Min/Max<Type>Damage 的分 min/max MORE 乘区 | 带该类词条的 build 平均伤害偏差 |
| P2 | 01-01 | HIGH | MORE 聚合缺逐-mod round(·,2) 精度归一 | 多 more 乘区末位漂移，影响 golden 逐值对账 |
| P2 | 01-02 | HIGH | ModFlags 匹配用 intersects 而非子集语义 | 潜在地雷：多 flag mod 一旦启用即方向性错误 |
| P2 | 02-02 | HIGH | setup_enemy 覆写 env.enemy，破坏 enemyDB:AddList 增量装配语义 | 多来源敌方 mod 装配易丢失，架构性隐患 |

**修复建议节奏**：P0 立即修（一处注入即可，附回归测试）；P1 触发链路（03-01/03-02）需 build 层接线后才完整，可先补占位标注 + core 侧改造；P1 防御/异常项各自独立可并行；P2 中 01-02 是潜在地雷应在引入多 flag mod 前预先修正并补单测固化语义。

## 6. 与 agent-docs/pob2-calc-engine.md 的交叉引用

| 报告子系统 | 对应 agent-docs 章节 |
|------------|---------------------|
| 01 Mod 解析/聚合 | §一 数据来源解析与 ModStore 聚合（1.1 ModParser / 1.3 ModStore 统一聚合接口 / 1.4 EvalMod·条件·倍率·归因） |
| 02 Setup 装配 | §二 CalcSetup：环境与来源装配（2.1 initEnv·actor/modDB 分层 / 2.2 来源装配 / 2.3 conversionTable） |
| 03 Perform/Trigger | §三 CalcPerform：全局编排（3.2 充能 / 3.3 Buff·Aura / 3.4 Reservation）+ §五 5.4 速度与 DPS 总装（触发） |
| 04 伤害核心 | §四 CalcOffence 伤害核心（4.1 伤害类型桶与转换矩阵 / 4.2 calcDamage：inc·more 应用） |
| 05 命中/暴击/异常/DPS | §五 命中/暴击/异常/DPS 组装（5.1 命中率 / 5.2 暴击 / 5.3 异常 / 5.4 速度与 DPS 总装） |
| 06 防御/EHP | §六 CalcDefence（生命/护盾/护甲/闪避/抗性/EHP，详见各防御小节） |

> 审查中若发现 `agent-docs/pob2-calc-engine.md` 与一手 vendor 源码冲突，应以 vendor 为准并回写修正文档（见项目 `CLAUDE.md` 约定）。本次审查未发现文档与一手实现的方向性冲突。
