# M2 F-3：EHP 口径切换 + defensive_rows 扩列 + baseline 重记（显式审查记录）

> 生成：2026-06-11 · worktree `m2/defence`（前置 = F-2 报告 19d2b84 + 敌方元素穿透修复）
> 蓝图依据：blueprints/m2-defence.md §2 Track F commit 3、§4.2、§5（R2/扩列稀释行）、
> §6 开放问题 3（owner 裁决：扩列后 ≥80% 且旧 8 列子集 ≥111）
> 复现：`cargo test -p pobr-build --test ninja_parity parity_baseline_report -- --nocapture`

## 1. 切换内容（单 commit revert 即回旧口径）

| 项 | 切换前（F-1 双跑） | 切换后（F-3） |
|---|---|---|
| `total_ehp` | 旧 lowest-max-hit 口径 | **PoB2 口径** `TotalNumberOfHits × totalEnemyDamageIn`（CalcDefence.lua:3271/:3322） |
| `*_max_hit` | 旧自洽迭代（纯抗性/护甲） | **PoB2 口径**（TotalHitPool 池扩展层 + taken-as，:3540-3697） |
| 旧口径保留 | `total_ehp` 本体 | `total_ehp_lowest_max_hit`（perform 旧管线不删码） |
| `avoid_stun` / Stun 三字段 | reference_hit（life+ES）近似 | 真值 totalTakenHit（per-hit taken，:2444/:2554-2557/:2617） |
| display_catalog | M2 扩展 24 条 Planned | 全部翻 Computed（W0 冻结的唯一例外通道） |

前置修复（独立 commit，parity 中性）：EHP 管线消费 Pinnacle/Uber 敌方元素穿透
placeholder（`pinnacleBossPen = 15/5 = 3`，Modules/Data.lua:231；ConfigOptions.lua:
2072-2074；CalcDefence.lua:2363 `resMult = 1 − max(resist − enemyPen, 0)/100`）——
消除 F-2 遗留「元素 max hit 系统性 1.12-1.17x」（druid/invoker/varashta/pathfinder
等 → 1.00-1.04x）。

## 2. 扩列与基线重记

defensive_rows 8 → 25 列（核心 8 + 扩展 17：TotalEHP、五系 MaximumHitTaken、
EffectiveBlock/SpellBlock、Spirit/SpiritUnreserved、Evade/MeleeEvade、
Life/ManaUnreserved、ESRecoveryCap、PhysicalDamageReduction、DeflectChance）。
golden ∞（sanitize 占位 ≥1e307）按双 ∞ 命中口径计。

| 基线 | 旧值 | 新值 | 说明 |
|---|---:|---:|---|
| `BASELINE_DEF_CORE_HIT5`（旧 8 列子集） | 111 | **114**/144 = 79.2% | owner 裁决下限 111 ✓（防扩列稀释） |
| `BASELINE_DEF_HIT5`（25 列全量） | — | **308**/450 = **68.4%** | 扩列后新分母 |
| `BASELINE_DEF_HIT10` | — | **331**/450 = 73.6% | |
| `BASELINE_OFF_HIT5` / `HIT10` | 23 / 31 | 23 / 31（不变） | M2 不动进攻 ✓ |

## 3. 验收对照（蓝图 §4.2）

1. **扩列后 ≥80%@5%：未达——68.4%**（308/450）。旧 8 列子集 114 ≥ 111 ✓。
2. OFF_HIT5 = 23 ≥ 23 ✓ 不倒退。
3. 专项 fixture（`m2_f3_specialty_fixtures`，@5% 对 golden）：
   - MoM（sorceress-stormweaver-comet）：TotalEHP 1.03x ✓、PhysMaxHit 1.01x ✓；
   - CI（monk-invoker-frost-bomb）：TotalEHP 1.04x ✓、ChaosMaxHit 双 ∞ ✓；
   - 盾 block（warrior-titan / smith-of-kitava）：EffectiveBlock/SpellBlock ✓
     （TotalEHP 0.48x/0.24x 残差**不在**断言内，归因见 §4）；
   - taken-as：18-build golden 无词条载体，由 pobr-core 合成 fixture 覆盖
     （tests/taken_as.rs Lightning Coil 型 + ehp_pob2.rs，手算期望 :356-455）。
4. 本记录即 18-build 对照报告的切换批注（F-2 报告 + 本文件，commit message 附摘要）。
5. mod_db_bench：sum_inc/sum_base/more 7.3/7.3/14.9 µs，无回退；harness 全套 0.66s。

## 4. 80% 缺口的逐列归因（剩余 142 miss @5%）

| 列簇 | 命中 | 主因（残差归属） |
|---|---|---|
| EHP 族（TotalEHP + 5 max hit） | 38/102 | 上游池/护甲聚合（wolf-pack Armour 0.55x、smith/titan 护甲 + 格挡回复 GainWhenHit（vendor :3168-3177）未实现、sorceress/witch 系 ES/Life 池 ±8-20%）——口径本身已对齐（中性 build 1.00-1.04x） |
| SpiritUnreserved | 0/18 | `spirit_reserved` 由 **M1**（gem 侧 spirit 预留聚合，蓝图 D track 分工注）交付；本 worktree 恒 0 → unreserved == spirit。M1 合并后自动改善 |
| Life/ES/LifeUnres/ESRecoveryCap/PhysDR | 11-12/18 | Life/ES 池上游聚合既有缺口（与核心 8 列 miss 同源） |
| 核心 8 列 | 114/144 | M2 前既有缺口（armour/ES 聚合） |

**结论**：口径切换本身无量级异常（中性 build 新旧 max hit 数学等价、MoM/CI 系
1.00-1.04x）；68.4% < 80% 的缺口集中在 (a) 上游聚合（非 Track F 文件归属）、
(b) M1 边界（SpiritUnreserved）、(c) 格挡回复 GainWhenHit（F-1 允许置 0 的简化）。
是否按蓝图 §6-3 备选口径（「max-hit/EHP 新列单独 ≥70% + 旧列 ≥90%」——当前
EHP 族 37.3%，同样未达）或滚动到 M2 收尾批，**留 owner/主会话裁决**；
本 commit 的门禁以重记基线（308/331/114/23/31）零回归口径执行。
