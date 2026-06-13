# M4 阶段验收报告（进攻深水区）

> 日期：2026-06-13 · master `10e4743` · 验收口径 = 蓝图 m4-offence-deep.md §4.2（roadmap M4 节原文）

## 0. 验收结论：**通过**

| 验收条目（§4.2） | 目标 | 实测 | 判定 |
|---|---|---|---|
| 1. 进攻 parity | ≥70%@5% | **61/80 = 76.2%@5%**（86.2%@10%） | ✅ 超额 |
| 1b. 防御不倒退 | ≥ M3 验收线（83.1% 顺延口径） | 379/450 = 84.2%（core 132/144 = 91.7%） | ✅ +1.1pp |
| 2. 三类 fixture 入 golden | 弩/CoC/双持各 ≥1 | crossbow_reload_golden / coc_trigger_golden / dual_wield（golden 基线 1e-6） | ✅ |
| 3. bench 无回归 | mod_db 无回归 + perform ≤2.5× | mod_db sum 8.9µs（基线 9.2）；perform 1.02-1.04×（基线 2.0951ms） | ✅ 远低于预算 |
| 4. ModFlags 切换完成 | 双跑 diff 干净后翻默认 + 删旧码 | 两次双跑 diff=0；i1 去 feature 化、legacy 5 位表删除 | ✅ |
| 5. baseline bump 纪律 | 独立 chore(parity) commit + 归功列明 | 全程 8 次独立 bump commit，每次附实测与归功 | ✅ |
| 6. 双跑纪律 | diff 干净才切换 | ModFlags 两次；pass 短路走等价性测试（单手逐值） | ✅ |

**进攻轨迹**（@5%，自 M3 末起）：32.5% → 35.0%（缺口波 g）→ 50.0%（全局线波 h）→ 52.5%（续波 j+k1/k2）→ 57.5%（l 波）→ 65.0%（m 波）→ **76.2%（n 波）**。
**DoT 三列**（M4 新增门禁面）：扩列时 8.1% → **45.9%@5% / 56.8%@10%**。

## 1. 交付结构（六 track + 七轮缺口波）

- **T0-T5 蓝图主体**：bench 闸门、ModFlags 30 位、双 pass 归因（RFC APPROVED-WITH-CONDITIONS C1-C6 全满足）、乘区/DPS 末端、技能 DoT/弩、触发 61 项+CoC。
- **集成波 i**：武器位 replace_weapon_flags、ModFlags 切换、DoT/弩接线（essence-drain oracle 1.0000）、CoC golden、原语去重。
- **缺口波 g/j/k/l/m/n**：精准三路、异常 magnitude 接 Stored 族、DD 尸体基伤、冷却整链、isSwitchable 72 变体、IncDamage 聚合（PerStat 池值 bug）、毒/流血八族、curse 残项、Debuff 注入面、击中量级线（Rakiata 反转/条件族/弓系转伤）、曝光/Buff 载荷、带边尾差五修、varashta 残差包、Buff flag 载荷（stormweaver ignite 0→0.83）。

## 2. 方法论沉淀（七轮波次的共性）

1. **oracle 钉值优先**：每个修复前先用 pob2-oracle dump vendor 中间值逐段定位——七轮中四次证伪了登记假设（Vorana 抵消项、bolt 数、hitFrequency、Malice 聚合差），避免了按错误假设实现。
2. **伪命中的诚实回调**：六处「命中」被 vendor 钉值修复揭示为双计/抵消巧合（deadeye CombinedDPS、druid dot、twister dot 系、coiling 1.01x 等），全部按「已审查例外」格式注释登记并下修基线——基线的每个数字都可追溯。
3. **vendor 行号纪律**：全部行为 commit 附 CalcOffence/CalcPerform/ModParser/SkillStatMap 实读行号；蓝图与登记文件（m4-skill-gaps.md §1-§9）形成接力链。

## 3. 已登记的 M5+ 残项（m4-skill-gaps.md 各节）

- 进攻余 19 列脱靶的归属：coiling 量级线（curse 计数修正后真实低估）、monk-twister enemyDistance 阈值机制、wolf-pack/minion 系（M5a 召唤物）、Uhtred support 等级授予、unscalable 缩放豁免、速度线残差。
- DoT 余项：ground-dot 旗标、decay、DotCanStack duration 数据。
- 结构性：M6 parser-rules runtime、非主组 support 注入面全集、范围珠宝两个无样本近似、F8 charm 基底 buff。
- 工程：git 历史 target-master 污染改写（任务 #8，push 前）；F3 pipeline/tables 快照重下。
