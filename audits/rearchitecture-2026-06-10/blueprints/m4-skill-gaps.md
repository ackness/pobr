# M4-G 技能伤害量级特化——勘察结论与剩余缺口登记

> 来源：W5 报告 R3「若干 build DPS 量级差 0.05-0.6x，根因 = 技能特化机制缺失」。
> 本波（M4-G，2026-06-13）按 ninja 命中收益勘察四类、修复两类；其余按本表登记。
> 勘察方法：`tools/pob2-oracle` dump vendor 中间值（summedBase / intermediates /
> skillInfo）对照 PoBR `OutputTable::damage_components` 逐分量定位。

## 0. 已修复（本波 commit）

| 机制 | build | TotalDPS before→after（effective） | commit |
|---|---|---|---|
| 尸体爆炸基伤（`explodeCorpse` + `corpseExplosionLifeMultiplier` × `monsterLifeTable[enemyLevel]`，CalcOffence.lua:2211-2217） | witch-abyssal-lich-detonate-dead | 0.05x → **0.83x**（panel 0.09x → 1.09x） | 搬迁 `e742300` + enemyLevel `f7dface` + 消费 `a06b833` |
| grenade 二次起爆（`GrenadeActivateTwice` → DPS 末端倍率，CalcOffence.lua:1124-1127 / :4407） | ranger-deadeye-explosive-grenade | 0.68x → **1.02x** | `27e6fba` |
| 同上（部分收益） | mercenary-gemling-legionnaire-explosive-grenade | 0.13x → 0.20x（剩余见 §3） | `27e6fba` |

DD 剩余 0.83x 差 = effective 口径减伤乘区全局问题（PoB2 EFFECTIVE 的 EffMult
已折进 AverageHit，PoBR 另乘一道 mitigation；panel 口径 1.09x），非尸体机制本身，
归全局 effective 对齐线，不在本表。

## 1. witch-blood-mage-coiling-bolts（TotalDPS 0.09x）——非「bolt 数」缺口

**勘察结论（oracle 钉值）**：W5 报告的「多段/多发（bolt 数）」假设**不成立**。
vendor 主技能 = 选中 statSet 1（Physical），AvgHit 206919 = 单次施放口径，
无 bolt 数乘子。真实差距构成：

- **CritChance 32.41 vs 72.45（0.45x）**：vendor `IncCritChance 383`，PoBR 聚合
  不足（来源待逐 mod 定位；属暴击聚合线，非技能特化）。CritMultiplier 5.34 已逐位对齐。
- **per-hit 量级 ~0.39x**：vendor SummedBase（Physical 2840-4260 / Chaos 1840-2760）
  显著高于 gem 基伤（phys 1136-1704 / set2 chaos 994-1846）——多出部分 =
  **added damage 通道**（Blood Mage 升华「Skills gain added Physical Damage equal
  to % of Life Cost」族 + 装备 added chaos to spells × addedMult），并叠加
  `DamageGainAs_Physical 150 / DamageGainAs_Chaos 162`（vendor intermediates）。
  PoBR 当前 gain-as 仅 13-16% 档接入，150/162 档来源（升华/notable/支援）未注入。
- **statSet 2（Chaos bolt）的基伤未被 merge**（vendor 选中 set1 时 set2 仅 global
  merge，CalcActiveSkill.lua:124-140——但 oracle Chaos SummedBase 1840 来自 added，
  非 set2 基伤；PoBR 行为与 vendor 此点一致，不是缺口）。

**登记**：① Blood Mage 升华 life-cost→added-phys 机制（数据源 = 升华树 stat，
vendor ModParser/树侧）；② 大档 gain-as 来源核查（oracle `intermediates.DamageGainAs_*`
对拍）；③ 暴击聚合差（IncCritChance 383 vs PoBR 实聚合）。三者皆非 per-skill
特化，归 M4 暴击/聚合线。

## 2. twister（huntress 0.27x / monk 0.22x）——非「hitFrequency」缺口

**勘察结论**：`twister_hit_interval_ms 660`（act_dex.lua:9921）在 vendor
**无任何消费方**（SkillStatMap.lua / CalcOffence.lua 均无 hit_interval 映射；
hitTimeOverride/maxHitRatePerEnemy 不涉 Twister）。golden TotalDPS = AvgDamage ×
Speed（攻速桶），无命中频率乘子——W5 的「持续多段（twister 命中频率）」假设不成立。
真实差距构成（huntress 例）：

- **CritChance 34.08 vs 65.07**：vendor `PreEffectiveCritChance 40.90` →
  final 65.07，中间有 **crit 幸运/条件抬升**（lucky crit 族）；PoBR 34.08 接近
  pre-effective 档，缺的是 effective 抬升段（vendor CalcOffence crit 幸运分支）。
- **pre-crit per-hit ~0.66x**：vendor `IncDamage 350` + `DamageGainAs Chaos 27 /
  Fire 16 / Lightning 10`；PoBR inc/gain-as 聚合缺口待逐 mod 对拍（twister
  `baseMultiplier` 分等级值已入库且已消费，排除该因）。
- `twister_damage_+%_final_per_whirling_slash_stage 80`（statMap → Damage MORE ×
  Multiplier:WhirlwindStages）：build config 无 WhirlwindStages 输入，vendor 同样
  0 层零贡献——非本两 build 的差距来源（但该 statMap Multiplier 通道本身未接，
  其它 build 可能踩中，登记）。

**登记**：① crit 幸运/condition 抬升段（vendor CalcOffence.lua:3700 附近暴击二次
检定族）；② per-skill statMap 的 `Multiplier:<Var>` tag 条目（如
WhirlwindStages per-stage MORE）在 PoBR statmap 引擎的 tag 翻译白名单核查。

## 3. 投掷/齐射速率族（mercenary grenade 0.20x、druid ember-fusillade 0.12x）

- **mercenary-gemling-legionnaire-explosive-grenade**（修后 0.20x）：
  - per-hit 0.36x：vendor AvgDamage 116832 @ gem lv24 q31 + 支援
    （Vorana's Siege / Payload / Innervate…）；PoBR 42493。差距主体在 added/
    gain-as（vendor `DamageGainAs_Cold 46 / Lightning 28`）与 q31 quality 段。
  - Speed 0.26 vs 0.29（0.90x）：grenade 冷却模型——Payload
    `base_cooldown_speed_+% -70`（sup_dex.lua:3577，statmap → CooldownRecovery INC）
    PoBR 的 `CooldownRecovery` 不在 statmap ModName 直通族 → 该条目当前
    UnknownModName 丢弃；Vorana's Siege 的冷却/弹数改写亦未建模。**注意**：
    单独接通 -70% 会让 rate 偏离 golden（vendor 链含 Vorana 抵消项），须整链
    一起做（vendor CalcOffence.lua:2858-3007 cooldown-governed speed 段对拍）。
  - vendor 行号：CalcOffence.lua:1124-1127（已修）、:2858-3007（cooldown speed）、
    SkillStatMap.lua:2798-2800（support_grenade_damage_+%_final）。
- **druid-oracle-ember-fusillade**（0.12x）：oracle 钉值 vendor CritChance **100**
  （CritMult 1.69——Oracle 升华/Spellslinger 链的必暴机制）、Speed 5.4 vs PoBR
  4.875、pre-crit per-hit ~0.29x（`IncDamage 409` + `DamageGainAs_Cold 46.7` +
  `ElementalDamageGainAs_Cold 31` + `More 1.23`）。`ember_fusillade_damage_+%_
  final_per_ember_fired`（act_int.lua qualityStats/constantStats）在 vendor
  **无 statMap 消费**（与 twister hit_interval 同类「数据存在但 vendor 不算」），
  非缺口。差距主体 = 必暴条件链 + 聚合，无 per-skill 数据可补，归 crit/聚合线。

## 4. 方法论沉淀

- 「技能特化」四类中两类（twister hitFrequency、coiling bolts 多发）经 oracle
  证伪——**vendor 对这些 stat 根本不消费**；伤害量级差的大头反复落在三条全局线：
  ① crit 条件/幸运抬升；② added damage / gain-as 大档来源；③ effective 减伤乘区。
  后续波次建议先做这三条线，再回头看单技能。
- oracle 用法：`/Users/…/pobr/tools/pob2-oracle/run.sh <decoded.xml> out.json`
  （worktree 无 vendor 检出时用主仓的 run.sh，路径自解析）；`summedBase` 直接
  暴露 base 段缺口（如 DD 的 Physical 4548 = 32956×0.138 一眼钉死）。
