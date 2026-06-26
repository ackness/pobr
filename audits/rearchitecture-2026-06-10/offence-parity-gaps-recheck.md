# 进攻 parity 逐 build 补差清单（2026-06-18 实测）

> 来源：`cargo test -p pobr-build --test parity ninja_parity -- --nocapture`（18-build，
> @5% 容差，effective 口径 = 有效 DPS vs PoB2 golden `meta.json::player_stats`）。
> 本文是「逐 build 补差」的 worklist；**修复须在有 `pob2-oracle`（luajit）的本地环境做**
> ——逐分量 dump PoBR vs PoB2 中间值定位缺口，云端无 luajit 无法跑 oracle，盲改进攻
> 计算会回归当前命中的 ~13 个 build 且有静默错算风险（与 `m6-delete-legacy-3b-handoff`
> 同一 vendor/luajit 缺失约束）。

## 当前命中态（TotalDPS e-ratio）

多数 build 已 @5% 命中（0.96–1.04x）。剩余系统性 miss：

| # | PoBR (effective) | PoB2 golden | e-ratio | 信号 / 优先级 |
|---|---|---|---|---|
| A | 328,810 | 422,109 | **0.78x** | under ~22%；中等体量 |
| B | 3,258 | 6,265 | **0.52x** | ≈ 恰好一半 → 疑缺 ×2 机制（双击 / 双投射命中 / 双持 / DoubleDamage） |
| C | 115,245 | 143,711 | **0.80x** | under ~20% |
| D | 78,050 | 83,227 | 0.94x | 仅差 6%，临界 |
| E | 171 | 0 | **infx** | PoBR 在 PoB2=0 处算出 DPS → 主技能选取/非伤害技能口径分歧（PoB2 该 build TotalDPS=0） |

> panel 口径（p-ratio）普遍更低（0.39–0.65x），因 panel 不计敌人交互；effective 才是
> 与 PoB2 可比口径。命中判定用 effective。

## 逐 build 定位手册（本地 oracle 流程）

对每个 miss build：

1. `tools/pob2-oracle/run.sh <build.xml>` dump PoB2 侧完整计算分解（中间值 + 最终值）。
2. PoBR 侧用 `POBR_DBG_*` 仪表（见 `m6-switch-decision` 仪表清单）dump 对应分量。
3. 逐分量对照定位首个偏差点（base hit / 转换 / inc / more / 暴击 / 命中 / 速率 / 异常 DoT）。
4. 修复附 PoB2 一手依据（vendor `CalcOffence.lua` 行号 / oracle 中间值），baseline 变动
   独立 commit 显式审查（路线图 §0 纪律：**禁自行 bump baseline**）。

### 重点假设（待 oracle 证实/证伪）
- **B（0.52x）**：≈½ 强烈指向单一 ×2 机制缺失。先查该 build 主技能是否双持/双击/多重
  投射全命中 / DoubleDamage 词条未生效。最高性价比（单点可能闭合整 2x）。
- **A/C（0.78–0.80x）**：~20% 量级，疑某一 more 乘区或伤害转换分量漏算（非整数倍 →
  非"漏一个开关"，更像某来源未注入或某乘区少乘）。
- **E（infx）**：PoB2 golden=0 但 PoBR>0。先判该 build PoB2 主技能是否非伤害（守护/光环/
  位移），PoBR `resolve_main_skill` 是否误选了伤害技能；或 golden 未捕获（meta.json 缺列）。
  注意：FullDPS（`calculate_full_dps`，M7 脚手架）对"主技能非伤害但组内另有伤害技能"的
  口径与单技能 TotalDPS 不同，E 的 infx 可能随 FullDPS 口径澄清。

## 不在本清单（已确认非缺口）
- speed / cast-speed / AoE / duration 族 SkillStatMap：`skill_stat_map.json` **均已覆盖**
  （实测 `attack_speed_+%` / `cast_speed_+%` / `base_skill_area_of_effect_+%` /
  `skill_effect_duration_+%` / `attack_and_cast_speed_+%` / `active_skill_attack_speed_+%_final`
  全 PRESENT）。doc15 chain-A「map_speed_percent 待补」是 M1 前的陈旧记述（彼时
  `map_skill_stat` Rust 启发式，M1 已删 751 行改数据驱动 `skill_stat_map.json` + stat_map_engine）。
- support 宝石伤害 inc/more/附加倍率：已注入（`calc_orchestrator::support_modifiers` →
  stat_map_engine，doc14 P0-2 已闭合）。
