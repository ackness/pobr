# M3-W5 `mode_effective` 口径切换报告（先报告后切换）

> 实施序（D3 纪律）：**commit A**（本报告 + 双跑对照测试，零行为）→ **commit B**（双跑暴露的
> 可快修 bug，附 vendor 行号与测试）→ **commit C**（ninja_parity 默认口径切 effective +
> baseline 重算 + 面板口径守卫）。各 commit 实测数据按落地顺序补入本文。

## 0. 口径依据（vendor 实读）

- `CalcSetup.lua:583-588`：非 CALCS 模式 `buffMode` **恒为 `"EFFECTIVE"`** ——
  ```lua
  if mode == "CALCS" then
      buffMode = env.calcsInput.misc_buffMode
  else
      buffMode = "EFFECTIVE"
  end
  ```
  随后 `:589-592`：`EFFECTIVE → env.mode_buffs / mode_combat / mode_effective 全 true`。
  PoB2 主面板（也即 18-build `meta.json::player_stats` 黄金值的导出口径）就是 effective 口径。
- 因此 ninja_parity 当前以 `mode_effective=false`（面板口径）对照 effective 口径的 golden，
  **口径本身即已知系统性偏差源**：DPS 族把「我方未计敌人减伤」与「golden 已计敌人减伤」直接相比。
- golden 侧实证（18 build `meta.json` 逐一验证）：`TotalDPS = AverageDamage × Speed`
  （攻击）/ `TotalDPS = AverageHit × Speed`（法术）**逐 build 精确成立** ——
  即 golden 的 DPS/平均伤害都已含命中率、暴击与敌人减伤链。

蓄在该开关后的已落地链路（全部带 vendor 依据合并于 master，面板口径下惰性）：
穿透词条（R3）、cooldown 族（R1 部分）、EnemyModifier curse-take 转发（C4）、curse 增伤映射
（Despair/EW/Enfeeble/Sniper's Mark，5 stat）、Shock→DamageTaken 异常闭环、enemy 数值覆盖
（config ③）、boss `CurseEffectOnSelf -50` 交互、hex 面板 gate（vendor :2289）。

## 1. 双跑方法

`crates/pobr-build/tests/ninja_parity.rs::effective_switch_dual_run_report`：同一 build 以
`mode_effective=false/true` 各算一遍，逐 stat 三列（panel / effective / PoB2 golden）+
双比值 + 命中带迁移标记（`↑5%` 收敛 / `↓LOST` 恶化 / `Δ` 数值变动不迁移带）。

```
cargo test -p pobr-build --test ninja_parity -- effective_switch_dual_run_report --nocapture
```

## 2. 切换前（master 现状）双跑结果

聚合（命中数 / 分母，@5% 与 @10%）：

| 指标 | panel（现基线口径） | effective（直接切换） | 变化 |
|------|--------------------|----------------------|------|
| def core-8 | 130/144 = 90.3%（@10% 92.4%） | 130/144 = 90.3%（@10% 92.4%） | 不动 |
| def 25 列 | 374/450 = 83.1%（@10% 86.7%） | 374/450 = 83.1%（@10% 86.7%） | 不动 |
| offensive | **27/80 = 33.8%**（@10% 42.5%） | **23/80 = 28.8%**（@10% 42.5%） | **−4 @5%** |

防御侧 425 行逐值不变（防御管线不消费 `mode_effective`，符合预期）。进攻侧逐 build
TotalDPS / CritChance 比值（panel→effective，golden=1.00）：

| build | TotalDPS | CritChance | @5% 迁移 |
|---|---|---|---|
| druid-oracle-comet | 0.46→0.33 | 0.41→0.37 | Δ |
| druid-oracle-ember-fusillade | 0.18→0.10 | 0.15→0.13 | Δ |
| huntress-ritualist-bow-shot | 0.63→0.60 | 0.90→0.90 | Δ |
| huntress-spirit-walker-twister | 0.36→0.27 | 0.52→0.52 | Δ |
| mercenary-gemling-explosive-grenade | 0.19→0.13 | 0.83→0.83 | Δ |
| mercenary-tactician-wolf-pack | 1.00→1.00（golden 0） | — | 不动 |
| monk-invoker-frost-bomb | 0.25→0.18 | 0.68→0.61 | Δ |
| monk-martial-artist-flicker-strike | 0.63→0.35 | 0.94→0.94 | Δ |
| monk-martial-artist-twister | 0.32→0.22 | 0.94→0.94；CritMult 0.77→**0.95** | Δ（CritMult 改善） |
| ranger-deadeye-explosive-grenade | 0.69→0.68 | 1.00→1.00 | Δ |
| ranger-pathfinder-ice-shot | 0.16→0.08 | 0.55→0.55 | Δ |
| sorceress-chronomancer-essence-drain | inf→inf（golden 0，DoT build） | 0.41→0.39 | 不动 |
| sorceress-disciple-of-varashta-comet | 0.31→0.15 | **1.00→0.91 ↓LOST** | −1 |
| sorceress-stormweaver-comet | 0.86→0.54 | **0.97→0.91 ↓LOST** | −1 |
| warrior-smith-of-kitava-shield-wall | 0.57→0.53 | **1.00→0.93 ↓LOST** | −1 |
| warrior-titan-shield-wall | 0.61→0.47 | 0.61→0.57 | Δ |
| witch-abyssal-lich-detonate-dead | 0.08→0.04 | **1.00→0.89 ↓LOST** | −1 |
| witch-blood-mage-coiling-bolts | 0.09→0.07 | 0.45→0.38 | Δ |

分布：**收敛 0 项 / 恶化 4 项（全部 CritChance）/ 其余为带内不迁移的数值变动**。
预期中的「挂 curse/穿透/感电 build DPS 收敛」未发生——原因见 §3-R3：上游伤害量级缺口
比敌方减伤链更大，panel 口径此前靠「不算敌人减伤」获得的虚高补偿被切换揭掉。

## 3. 恶化/不收敛项 root cause（逐条）

### R1（bug，本线快修）：编排从未填 `cfg.skill_types` → 法术被卷入精准命中检定

- 现象：4 个 `↓LOST` 中 3 个（disciple-comet / stormweaver-comet / abyssal-lich-DD）是**法术
  build** 的 CritChance 从精确 1.00x 掉到 0.89-0.91x；frost-bomb / blood-mage / oracle 等法术
  build 的 CritChance 同步劣化（带外 Δ）。
- root cause：`calc_orchestrator.rs` 构造 `CalcConfig` 时只设 `ModFlags`，从未调
  `with_skill_types` → `cfg.is_spell()/is_attack()` 对一切 build 恒 false →
  `offence.rs` 的命中检定分支把法术也按攻击做精准/闪避公式（panel 口径下已错误地把
  `hit_chance≈0.89-0.94` 乘进 DPS；effective 口径下 vendor `CalcOffence.lua:3700`
  暴击二次命中检定又把同一错误命中率乘进 CritChance，**同一 bug 二次显影**）。
- vendor 依据：`CalcOffence.lua:2611-2612` `if not isAttack then output.AccuracyHitChance
  = 100`（非攻击必中，无精准检定）；`:3700` `output.CritChance = output.CritChance *
  output.AccuracyHitChance / 100`（暴击降级只乘 AccuracyHitChance）。
- 修复（commit B）：① 编排把主技能 `skill_types` token（Attack/Spell）映射进
  `cfg.skill_types`；② `offence.rs` 命中检定分支 `cfg.is_spell()` → `!cfg.is_attack()`
  （逐字对齐 vendor `not isAttack`）。

### R2（上游既存缺口，登记 M4 不修）：攻击 build 玩家精准聚合低估

- 现象：smith-of-kitava CritChance 1.00→0.93 ↓LOST；titan/twister 等攻击 build 命中率
  0.93 上下。
- root cause：18 build golden `HitChance` **全部 =100**（titan 99）——PoE2 命中公式
  `acc×1.25/(acc+eva×0.3)`（vendor `CalcDefence.lua:32-38`，cap 100）下 PoB2 的玩家精准
  足够大（如 smith OffHandAccuracy=1438 vs 敌方闪避 1175 → raw 100.4% → cap 100）。
  PoBR 同公式但玩家精准聚合 ≈1015（角色基础 6/级 + 6/敏捷已对齐 vendor
  `Data.lua:174 AccuracyPerDexBase=6` / `Misc.lua:154 accuracy_rating_per_level=6`；
  差额来自**装备/天赋精准词条与武器局部精准**未入聚合）→ hit 92.8% → effective 下
  CritChance 被多降一档。
- 处置：属 M4 进攻深化（精准聚合/武器局部词条），登记不修。本切换后该项是 OFF_HIT5
  唯一净损失（−1，见 §5）。

### R3（上游既存缺口集合，登记 M4 不修）：伤害量级低估被敌方减伤揭露

- 现象：全部 DPS 行 panel→effective 比值下降 0.03-0.28，但**无一原本在带内**，故不构成
  @5% 净损失；也无一收敛进带。
- root cause：panel 口径的「PoBR 未减伤 vs golden 已减伤」是**反向补偿**——上游伤害量级
  低估（宝石伤害表级差/支援乘区/charge·buff 覆盖等，M4 缺口面）被「少乘一个 ~0.5-0.7 的
  敌方减伤因子」部分抵消，panel 比值（如 stormweaver 0.86x）属假性接近。切换后假补偿
  消失，真实量级缺口显影（0.54x）。证据：golden 恒等式 `TotalDPS = AvgHit × Speed`
  （§0）说明 golden 已含敌方减伤；而 PoBR 的 effMult（boss 元素抗 50 − 穿透 − 曝光 −
  curse）方向与 vendor 一致。curse/穿透/Shock 链路在 effective 下确在生效
  （twister 的 CritMultiplier 0.77→0.95 即 Sniper's Mark 链路激活的直接证据），
  只是其增益（×1.1-1.4）小于减伤因子（×0.5-0.7），无法抵消量级缺口。
- 处置：登记 M4（伤害量级族缺口：宝石 per-level 伤害、support 乘区完备性、charge/buff
  默认覆盖、flicker/grenade 速率族）。逐 build 缺口归因见 §6。

### R4（harness 口径错配，随切换修正）：AverageDamage 行比较口径

- 现象：AverageDamage 行用 PoBR `total_hit_avg`（玩家侧未减伤、不含命中率）对比 golden
  `AverageDamage`（含命中率/暴击/敌方减伤，§0 恒等式）。effective 切换后该行成为
  **结构性错配**。
- 修复（commit C 随切换）：该行 PoBR 侧改取 `dps / action_rate`（与 golden 同一恒等式
  口径）。AverageHit（法术导出键）回填扩列登记 M4 harness 项。

## 4. 快修（commit B）后双跑

修复 R1（skill_types + `!is_attack()` 命中门）后复跑（R4 同步生效后聚合不变，
AverageDamage 行两口径均带外）：

| 指标 | panel | effective | 切换前 effective |
|------|-------|-----------|------------------|
| def core-8 / 25 列 | 不变（90.3% / 83.1%） | 不变 | 不变 |
| offensive @5% | 27/80 = 33.8% | **26/80 = 32.5%** | 23/80 = 28.8% |
| offensive @10% | **35/80 = 43.8%**（+2，DD/blood-mage 法术 DPS 回带） | 35/80 = 43.8% | 34/80 = 42.5% |

- 法术 build 的 CritChance 恶化全部消除（disciple/stormweaver 1.00x、DD 1.00x、
  frost-bomb 0.68x 持平 panel）；法术 panel DPS 同步上修（不再乘错误命中率：
  stormweaver 0.86→0.92、DD 0.08→0.09、blood-mage 0.09→0.11）。
- 残余 `↓LOST` 仅 1 项：smith CritChance 1.00→0.93（R2，M4 登记项）。
- effective_switch 后 OFF_HIT5 26 vs 切换前 panel 27：净 −1，root cause 即 R2，
  按任务授权显式登记并主线审查。

## 5. 切换与 baseline 重算（commit C 实测）

ninja_parity 默认口径切 `mode_effective=true`（`run_build` → effective；与 golden 同口径），
同 commit 落地 R4（AverageDamage 行 → `dps/action_rate`）。切换前后聚合对照（含 §4 快修）：

| 指标 | 切换前（panel 口径门禁） | 切换后（effective 口径门禁） | baseline 动作 |
|------|------------------------|----------------------------|---------------|
| def core-8 @5% | 130/144 = 90.3% | 130/144 = 90.3% | 130 不变 |
| def 25 列 @5% | 374/450 = 83.1% | 374/450 = 83.1% | 374 不变 |
| def 25 列 @10% | 390/450 = 86.7% | 390/450 = 86.7% | 390 不变 |
| off @5% | 27/80 = 33.8% | **26/80 = 32.5%** | **27→26（已审查例外，见下）** |
| off @10% | 33/80 = 41.2%（M1+M2 记录；快修后 35） | **35/80 = 43.8%** | 33→35 上调 |

- **OFF_HIT5 27→26 已审查例外（唯一净损失）**：smith-of-kitava CritChance 1.00x→0.93x。
  root cause = §3-R2（玩家精准聚合低估 ≈1015 vs golden 1438；golden HitChance=100 →
  PoB2 不降级暴击；PoBR hit 92.8% × vendor :3700 二次检定 → 0.93x）。M4 精准聚合修复后
  该项应自然回带并上调 baseline。
- **面板口径保留守卫**：新增 `panel_mode_no_regression`（`PANEL_OFF_HIT5=27 /
  PANEL_OFF_HIT10=35`），面板口径水平不因切换失去保护；防御侧两口径逐值相同，由主门禁覆盖。
- 双口径对照仪表盘 `effective_switch_dual_run_report` 常驻（打印型，不设门禁）。

切换后逐 build TotalDPS / CritChance（effective vs golden）：

| build | TotalDPS | CritChance | 主导缺口（§6） |
|---|---|---|---|
| druid-oracle-comet | 0.38 | 0.41 | 法术量级 + Inevitable/lucky 暴击链 |
| druid-oracle-ember-fusillade | 0.12 | 0.15 | 多弹/repeat + golden 暴击 14.5→100（Inevitable 链）|
| huntress-ritualist-bow-shot | 0.60 | 0.90 | 弓攻量级（局部词条/精准） |
| huntress-spirit-walker-twister | 0.27 | 0.52 | twister 多段命中 |
| mercenary-gemling-explosive-grenade | 0.13 | 0.83 | grenade 速率族（已知，任务点名） |
| mercenary-tactician-wolf-pack | 1.00（golden 0） | — | minion DPS 不在 hit 列（harness M4 扩列） |
| monk-invoker-frost-bomb | 0.21 | 0.68 | cooldown 技能量级 |
| monk-martial-artist-flicker-strike | 0.35 | 0.94 | flicker 速率族（已知，任务点名） |
| monk-martial-artist-twister | 0.22 | 0.94（CritMult 0.95） | twister 多段；mark 链路已激活 |
| ranger-deadeye-explosive-grenade | 0.68 | 1.00 | grenade per-hit（M1 已登记） |
| ranger-pathfinder-ice-shot | 0.08 | 0.55 | 多投射/bow 量级 |
| sorceress-chronomancer-ED | inf（golden 0，DoT） | 0.41 | DoT DPS 列缺（harness M4 扩列） |
| sorceress-disciple-of-varashta-comet | 0.18 | 1.00 | 法术量级（varashta 机制） |
| sorceress-stormweaver-comet | 0.59 | 0.97 | 法术量级（archmage/buff 覆盖） |
| warrior-smith-of-kitava-shield-wall | 0.53 | 0.93 | 武器局部词条 + 精准聚合（R2） |
| warrior-titan-shield-wall | 0.47 | 0.57 | 同上 + 暴击来源 |
| witch-abyssal-lich-detonate-dead | 0.05 | 1.00 | DD 尸体基伤未实现 |
| witch-blood-mage-coiling-bolts | 0.09 | 0.45 | coiling bolts 多段 |

## 6. 验收对照与缺口归因（M3 §9.2）

实测（effective 口径，2026-06-12）：

| 验收项 | M3 目标 | 实测 | 判定 |
|--------|---------|------|------|
| 进攻 @5% | ≥55% | **32.5%**（26/80；@10% 43.8%） | **未达** |
| 防御 @5% | ≥85%（或 M2 终点+5pp 顺延） | **83.1%**（374/450；core-8 90.3%） | **未达**（M2 终点 83.1%，M3 防御侧零回归达成、增量未发生——M3 各 track 均为进攻/编排侧） |
| aura/curse build 不再系统性偏低 | 中位差收敛 ±5% 或逐 build 解释 | 未收敛（§7），逐 build 解释见 §5 表 | 以逐 build 归因满足条件 B |

**进攻未达标的结构性结论**：切换暴露的主导缺口是**上游伤害量级**（§3-R3），不是
M3 编排链路——curse/穿透/Shock/敌覆盖链路已激活且方向正确（twister CritMult
0.77→0.95、frost-bomb/blood-mage @10% 回带），但量级缺口（0.05-0.6x）在任何
口径下都不可能靠编排收敛。@5% 进攻命中目前由 CritChance（10/18 带内）与
golden-0 项贡献，TotalDPS 带内仅 wolf-pack（双 0）。

缺口归因清单（M4 登记，按影响 build 数排序）：

1. **伤害量级族**（全部 16 个有伤害 build）：宝石 per-level 伤害表精度、support
   乘区完备性、buff/charge 默认覆盖（详见 m4-offence-deep.md 既有清单）。
2. **精准聚合**（攻击 6 build；§3-R2）：装备/天赋精准词条 + 武器局部精准未入聚合
   → hit<100% → DPS 与暴击双重低估。唯一造成切换净损失（−1）的项。
3. **技能特化机制**：DD 尸体基伤、coiling bolts/twister/ember 多段、grenade/flicker
   速率族（后两者任务已点名属 M4 不修）。
4. **harness 扩列**（M4）：DoT DPS（chronomancer-ED golden TotalDPS=0 而 DoT 是真输出）、
   minion DPS（wolf-pack）、AverageHit（法术导出键）回填。
5. **暴击特化链**：Inevitable Criticals / lucky crit 的 build 级触发条件
   （oracle/ember 两 druid build golden 暴击被抬到 100）。

## 7. aura/curse 子集分布（切换后）

- curse/mark 子集（10 build，排除 golden-0 的 wolf-pack/chronomancer 后 8 个）
  TotalDPS 中位 ≈ **0.20x**；非 curse 子集（8 build）中位 ≈ **0.41x**。
- curse 子集仍系统性更低，但驱动项是该子集恰好集中了量级缺口最大的技能特化 build
  （DD 0.05 / ice-shot 0.08 / blood-mage 0.09），而非 curse 链路本身：
  curse 链路活性证据——twister(monk) 的 CritMultiplier 0.77→0.95（Sniper's Mark
  SelfCritMultiplier 转发）、boss `CurseEffectOnSelf -50` 与 hex gate（vendor :2289）
  均按 vendor 语义参与。满足 §9.2「或给出逐 build 解释」分支（§5 表）。
- aura/herald 子集（17 build）与全集基本重合，无独立信号。
