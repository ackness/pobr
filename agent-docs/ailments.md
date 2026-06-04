# 异常状态 (Ailments)

异常状态是 Path of Exile 2 中的一类常见 **debuff**，与特定伤害类型绑定，可对角色或怪物造成持续伤害或施加负面状态效果[^mobalytics-ailments][^poe2wiki-ailment]。玩家和怪物都能根据所造成的伤害类型与施加几率施加异常状态。

> **关键约定**：除非特别说明，**同类型的 debuff 不叠加，只有"效果最强"的那一份生效**（PoE2 通用 debuff 规则）。少数异常状态（如可叠层的中毒/流血、或带 `*CanStack` 的特殊配置）是例外。

> 本文档把 PoE2 异常状态拆成几条常被忽略的脉络：**异常阈值 (Ailment Threshold) / 姿态阈值 (Poise Threshold)**、**强度 (Magnitude) 与有效效果 (Effect)**、**几率派生 vs. 积累派生 (Chance vs. Buildup)**、持续伤害型 (Bleed/Poison/Ignite) 与非伤害型 (Chill/Freeze/Shock/Electrocute) 的各自公式、暴击与异常状态的关系、叠层 (Stacks)、增殖 (Proliferation)，以及它们在 PoB2 `CalcOffence.lua` / `CalcPerform.lua` 中的建模方式（见末尾「PoB2 计算实现」）。

## 异常状态类型一览

PoE2 当前正式异常状态共 7 个（PoB2 `data.ailmentTypeList` + `Electrocute`）：

- **持续伤害型 (Damaging)**：流血 (Bleeding)、中毒 (Poison)、点燃 (Ignite)
- **非伤害型 (Non-damaging)**：冰缓 (Chill)、冰冻 (Freeze)、感电 (Shock)、电击 (Electrocute)

> PoB2 中 `data.elementalAilmentTypeList = { Ignite, Chill, Freeze, Shock }`（Electrocute 当前未列入元素异常表，按姿态 debuff 处理），`data.nonElementalAilmentTypeList = { Bleed, Poison }`。

> **遗留 (Legacy) 异常**：Scorched（灼烧）/ Brittle（脆弱）/ Sapped（衰竭）/ Impale（穿刺）是 PoE1 异常，PoB2 源码注释为 *"Legacy PoE1 ailments (to be removed later)"*，几率默认硬编码为 0（`output.ScorchChance = 0` 等）。它们在 PoE2 中目前只能由特定来源（如某些 **Soul Core**、辅助宝石、暴击专属配置 `CritAlwaysAltAilments`）施加，不是常规伤害派生异常。实现时按"低优先级特殊来源"处理即可。

### 造成持续伤害的异常状态 (Damaging Ailments)

| 异常状态 | 伤害类型 | 默认持续时间 | 基础强度 (每秒) | 缩放来源 (ScalesFrom) |
|---------|---------|------------|----------------|----------------------|
| **流血 (Bleeding)** | 物理 | 5 秒 | 造成它的击中的**未减免物理伤害**的 **15%/秒** | 物理 |
| **中毒 (Poison)** | 混沌 | 2 秒 | 造成它的击中的**未减免物理+混沌伤害**的 **20%/秒** | 物理 + 混沌 |
| **点燃 (Ignite)** | 火焰 | 4 秒 | 造成它的击中的**火焰伤害**的 **20%/秒** | 火焰 |

**基础强度的来源（PoB2 `Misc.lua` gameConstants）**——这些都是"每分钟百分比"，除以 60/100 转成"每秒倍率"：
- `BleedingHitDamagePercentPerMinute = 900` → `900/60/100 = 0.15` → **15%/秒**
- `IgniteHitDamagePercentPerMinute = 1200` → `1200/60/100 = 0.20` → **20%/秒**
- `PoisonHitDamagePercentPerMinute = 1200` → `1200/60/100 = 0.20` → **20%/秒**
- 默认时长：`BaseBleedingDuration = 5`、`BaseIgniteDuration = 4`、`BasePoisonDuration = 2`

**注**：
- 流血和中毒默认**绕过能量护盾 (Energy Shield)**[^poe2wiki-ailment]
- **强度 (Magnitude) 取自击中的未减免伤害**：即使目标防御减免了击中，damaging ailment 仍按原始伤害量结算，并且**之后不再受任何"你造成的伤害"修饰词影响**。但**影响敌人受到伤害的修饰词**（如感电、抗性、`DamageTakenOverTime`）仍会作用于异常状态伤害（PoB2 中即 `effMult`）。
- **流血加重 (Aggravated)**：流血在目标移动或流血被加重时造成额外 **100%** 伤害（PoB2 `BloodstainedMultiplierWhenMovingOrBleedingAggravated = 2`）。
- 默认只能对敌人施加**一个中毒**，但可通过修饰词/技能开启**叠层 (`PoisonCanStack`)**，对每个 damaging ailment 都有对应的 `<Ailment>CanStack` / `<Ailment>Stacks` 机制（见「叠层与权重平均」）。

> **0.5.0 更新**：玩家身上的流血不再在玩家移动时增加伤害（`Misc.lua` 玩家段 `no_extra_bleeding_damage_while_moving = 1` / `bleeding_moving_damage_%_of_base_override = 1`）。此更改**不影响**玩家施加给怪物的流血[^maxroll-050-patchnotes]。
> **0.5.0 更新**：`Poisonburst Arrow` 等技能的中毒时长被固定为 3 秒（不再随宝石等级变化），属于技能层面调整[^sidia-050]。

### 非伤害性异常状态 (Non-Damaging Ailments)

| 异常状态 | 效果 | 默认时长（非玩家/玩家） | 施加方式 | 缩放来源 |
|---------|------|------------------------|---------|---------|
| **冰缓 (Chill)** | 降低目标行动速度 (ActionSpeed) | 8 秒 / 2 秒 | 冷伤击中**默认必定**（达到最小强度即可） | 冷 |
| **冰冻 (Freeze)** | 行动速度归零，无法移动/行动 | 4 秒（通用） | **积累 (Buildup)**，达 100% 触发 | 冷 |
| **感电 (Shock)** | 目标受到的伤害**增加**（默认 +20%/层基准） | 8 秒 / 4 秒 | 几率派生（伤害 vs 阈值） | 闪电 |
| **电击 (Electrocute)** | 打断并阻止目标一切行动 | 5 秒（通用） | **积累 (Buildup)**，需特定技能/效果 | 闪电（仅特定来源） |

**默认值（PoB2 `Misc.lua` + `Data.lua` nonDamagingAilment）**：
- 冰缓：`default/min = 30`，`max = ChillMaxEffect = 50`，时长 `BaseChillDuration = 8`（玩家 2）。
- 冰冻：`min = 0.3` 秒，`max = 3` 秒，`FreezeDuration = 4`（玩家 2），`FreezeThresholdModifier = 500`。
- 感电：`default/min = BaseShockMagnitude = 20`，`max = 100`，时长 `BaseShockDuration = 8`（玩家 4）。
- 电击：`ElectrocuteDuration = 5000ms`（玩家 2000ms），`ElectrocuteThresholdModifier = 500`。

> **冰缓最小阈值**：冷伤击中"默认必定冰缓"，但强度 **< 30% 的冰缓会被丢弃**（0.5.0 之前阈值是 5%）。PoB2 据此推出 `chillMinimumThreshold = enemyThreshold / ChillEffectMultiplier`（`ChillEffectMultiplier = 100`），只有未减免冷伤 > 该阈值才标记可冰缓。
> **冰缓上限**：`Stormweaver's Heavy Snows` 等可把冰缓最大值从 50% 提到 70%[^poe2wiki-chill]。冰缓是一种 **Slow**，因此受 `Slow Magnitude` 与"敌人稀有度 less slow effect"影响——稀有度减免在最大冰缓**之后**结算（如 unique 50% less slow → 有效最大冰缓 25%）。

## 异常阈值 (Ailment Threshold) 与姿态阈值 (Poise Threshold)

异常状态能否施加、强度多大，取决于攻击者造成的**特定类型未减免伤害**与防御者**阈值**的比例。可以把阈值理解为防御异常的"元素护甲/抗性"。

### 两套阈值

1. **异常阈值 (Ailment Threshold)** —— 用于 几率派生型：点燃、感电、冰缓。
   - **玩家**：默认 = **最大生命的 50%**（`PlayerAilmentThresholdLifeFactor = 0.5`）。
   - **怪物**：与**怪物等级**挂钩、**与怪物生命无关**，查 `data.monsterAilmentThresholdTable[enemyLevel]`（例如 lv1 = 15、lv65 ≈ 8001、lv80+ 巨幅跳升用于 pinnacle boss）。
   - PoB2：`enemyThreshold = monsterAilmentThresholdTable[enemyLevel] * mod(EnemyAilmentThreshold)`。

2. **姿态阈值 (Poise Threshold)** —— 用于 积累/固化型 (Immobilisation)：冰冻、电击、重眩晕 (Heavy Stun)、钉刺 (Pin)。
   - 查 `data.monsterPoiseThresholdTable[enemyLevel]`，并乘 `PoiseThreshold` / `<Ailment>Threshold` / `EnemyAilmentThreshold`（冰冻、电击两个 immobilisation 同时也吃 `EnemyAilmentThreshold`）。
   - PoB2 注释明确：在 PoE2 中"冰冻和电击属于**姿态 (Poise)** 相关 debuff，与重眩晕、钉刺同类"[^pob2-deepwiki-ailments]。

> **更高异常/姿态阈值**对防御者越有利：被元素伤害施加异常越难，受到的冰冻/电击积累也越少。`Gain additional Ailment Threshold equal to X% of maximum Energy Shield`（如 0.5.0 `Soul Core of Retreat`：15% ES）、`+X% Elemental Ailment Threshold` 都是防御向词条[^fextra-050][^poe2wiki-ailment]。
> **Boss 抗性递增**：每当 boss 被冰冻（直接或积累触发），其对冰冻的有效阈值会**临时升高**，随时间衰减回基准——用于防止无限连冻。

## 几率 / 强度的两条派生路径

PoE2 的异常状态按"如何决定是否生效"分两类——这是建模时最重要的分叉。

### A. 几率派生型 (Chance scaling)：点燃、感电

每次击中按"伤害占阈值比例"算出一个施加几率，再叠加固定几率：

```
hitChance = (avgUnmitigatedDmg / enemyThreshold) * <Ailment>ChanceMultiplier
finalChance = (hitChance + ΣbaseChance) * (1 + Σinc/100) * Π(more)        -- clamp 到 100
```

- `ShockChanceMultiplier = 25`，`IgniteChanceMultiplier = 20`（`Misc.lua`）。
- **感电 1% 几率 / 4% 阈值伤害** 即由此而来：`25 = 1/0.04`，wiki 表述"By default a Hit has 1% chance to Shock for every 4% of the target's Ailment Threshold dealt"[^poe2wiki-ailment]。
- 固定几率来源：技能/辅助/天赋的 `EnemyShockChance` / `EnemyIgniteChance` / `AilmentChance`，以及敌方 `SelfShockChance` 等。
- PoB2 分别算 `<Ailment>ChanceOnHit` 与 `<Ailment>ChanceOnCrit`（暴击伤害更高 → 暴击几率更高）。

> **点燃几率特例**：wiki 描述点燃几率"由目标身上 **Flammability 总强度**决定（含本次击中新增量）"。PoB2 当前用上式的 `IgniteChanceMultiplier` 近似建模；非击中点燃（如 `Scorching Ray`）走 `NonHitFlammabilityPermyriadThresholdToIgnite = 5000` / `IgniteNonHitFlammabilityModifier = 0.8` 等单独常量。

### B. 强度直出 / 积累型：冰缓、感电效果、冰冻、电击

非伤害异常的**效果强度 (Effect / Magnitude)** 直接由伤害/阈值比例算出（与几率分开）：

**冰缓 (Chill) 效果**（`nonDamagingAilmentsConfig.Chill`）：
```
chillEffect = ChillEffectMultiplier * (damage / enemyThreshold) * effectMod   -- ChillEffectMultiplier = 100
```
线性缩放，clamp 到 `[min=30, max=50]`（默认）；`effectMod = mod(EnemyChillMagnitude, AilmentMagnitude) * mod(SelfChillMagnitude, AilmentMagnitude)`。

**感电 (Shock) 效果**（`nonDamagingAilmentsConfig.Shock`，幂律、ramping）：
```
shockEffect = 50 * (damage / enemyThreshold)^0.4 * effectMod                  -- clamp 到 [min=20, max=100]
```
与 PoE wiki 公式 `E = 1/2 * (D/T)^0.4 * (1+M)` 一致（这里 50 = 1/2 × 100，因为效果以百分点表示）[^poewiki-shock]。即"打满阈值伤害 → 50% 感电"，wiki 示例表：10.12% 阈值伤害 → 20% 感电，100% → 50%。

> **0.5.0 注意**：上面 `BaseShockMagnitude = 20` 是 PoB2 把感电默认/最小值设为 20。`max = 100` 是感电效果上限（远高于通常达到的 50%，配合 `increased Magnitude of Shock` 与 `ManaAppliesToShockEffect` 等可逼近）。

**冰冻 / 电击 / 重眩晕 / 钉刺 积累 (Buildup)**（`Misc.lua` `<Ailment>DamageScale` + 姿态阈值）：
```
poiseBuildup% = <Ailment>DamageScale / enemyPoiseThreshold * (1 + Σinc/100) * Π(more) * 100
本次击中积累 = hitDamage * poiseBuildup%
积累 ≥ 100% → 施加固定时长的该状态，并把计数器清零
```
- `FreezeDamageScale = 2.1`（玩家 2.0）、`ElectrocuteDamageScale = 1.7`、`HeavyStunDamageScale = 0.58`（玩家 0.65）、`PinDamageScale = 4.2`。
- 相关 inc/more 旗标：`Enemy<Ailment>Buildup`、`EnemyImmobilisationBuildup`、`ImmobilisationBuildup`，敌方侧 `<Ailment>Buildup`。
- **冰冻时长**取决于击中后伤害 / 阈值；PoB2 推导"最小 0.3 秒冰冻所需阈值上限 = baseVal × 20 × FreezeDurationMod"。
- **格挡不阻止冰冻积累**（wiki）[^poe2wiki-freeze]。

> **0.5.0 / 近期平衡**：wiki 版本历史记录"冰冻在敌人身上积累速度约慢 48%"，并"在已冰冻后降低后续积累"以抑制连冻[^poe2wiki-freeze]。`Ice Shot` 在 0.5.0 不再额外提供冰冻积累[^sidia-050]。

## 施加几率、保证施加、几率上限

- **流血 / 中毒必须有显式几率**：它们的伤害**不**贡献几率（伤害只贡献强度）。即使打出巨额物理击中，若 `BleedChance = 0` 也**不会**施加流血。几率来自技能宝石、辅助宝石、天赋、装备（`BleedChance` / `PoisonChance` / `AilmentChance`）。
  - PoB2：`chance = min(100, Override or base*(1+inc/100)*more)`，分别算 `OnHit` / `OnCrit`。
- **点燃 / 感电**走上文 A 的几率派生（伤害贡献几率），同样可叠 `AilmentChance`。
- **冰缓**默认必定（只要强度 ≥ 30%），**不需要**几率。
- **几率上限**：所有施加几率 clamp 到 **100%**。
- **保证施加**：可由 `Override` 直接置 100%（如某些技能"必定点燃/感电"）。

## 异常状态规避 / 免疫 / 抵抗 (Avoidance / Immunity)

防御侧：
- **`AvoidChill` / `AvoidFreeze` / `AvoidShock` / `AvoidIgnite` / `AvoidElementalAilments`**（百分比，clamp 100% = 免疫）。例：`Her Embrace` 给 100% AvoidFreeze + AvoidChill。
- **`<Ailment>Immune` / `Condition` 类免疫**（如某些 Archon buff："Immune to Freeze and Chill while affected by an Archon Buff"）。
- **缩短异常时长**：`SelfAilmentDuration` / `Self<Ailment>Duration` / `SelfElementalAilmentDuration`（敌方/受害者侧 INC）。
- **降低异常效果**：`reduced Effect of Chill/Shock on you`（在基础强度算出**之后**加法叠加，但不能超过最大值）。
- **提高阈值**：见上文 `EnemyAilmentThreshold` / `PoiseThreshold` / `+% Elemental Ailment Threshold` / `Energy Shield as Ailment Threshold`。

攻击侧增强：
- **`increased chance to inflict Ailments`** = `AilmentChance` (INC)。
- **`increased Magnitude of Ailments / Non-Damaging Ailments`** = `AilmentMagnitude` / `Enemy<Ailment>Magnitude`。
- **`increased Effect of Ailments`** = `AilmentEffect`（PoB2 中 damaging ailment DPS 乘 `effectMod = mod(AilmentEffect)`）。
- **`increased Duration of (Damaging/Elemental) Ailments`** = `Enemy<Ailment>Duration` / `DamagingAilmentDuration` / `EnemyElementalAilmentDuration` / `EnemyAilmentDuration`。
- **`faster/slower Ailment`**（持续伤害型的 burn rate）= `<Ailment>Faster` / `<Ailment>Slower`：加快每秒伤害但等比缩短时长（总伤害不变，DPS 上升）。

> **区分**：`Magnitude`（强度，影响每跳数值/效果大小）与 `Effect`（效果，PoB2 damaging ailment 的 DPS 乘子）与 `Duration`（时长）与 `Faster/Slower`（节奏）是**四个不同维度**，对应不同词条与不同计算位置——这是异常状态 scaling 最常被混淆的地方。

## 暴击与异常状态的关系

持续伤害本身**不能暴击**，但**由暴击击中施加的 damaging ailment，其基础伤害取自那次暴击击中的伤害**，因此随爆伤一起放大。PoB2 对此精细建模：

- 对每个 ailment 同时计算"非暴击来源伤害 (`sourceHitDmg`)"与"暴击来源伤害 (`sourceCritDmg`)"，按暴击几率加权：
  ```
  chanceFromHit  = chanceOnHit  * (1 - critChance)
  chanceFromCrit = chanceOnCrit * critChance
  baseVal = sourceHitDmg*chanceFromHit/(总) + sourceCritDmg*chanceFromCrit/(总)
  ```
- **暴击施加的异常默认更强**：暴击击中伤害更高 → 既提升几率派生型的几率，也提升强度/积累。
- **`AilmentsAreNeverFromCrit`** 旗标：让异常永远按非暴击伤害结算（部分技能/配置）。
- 叠层 over-stacking 会提升"在场存在暴击异常的概率"：`ailmentCritChance = 100 * (1 - (1 - critChance)^max(stackPotential, 1))`。
- 配置项 `ailmentMode`（PoB2 Configuration）：`AVERAGE`（默认，按几率加权）或 `CRIT`（只算暴击施加的异常，把 `*ChanceOnHit` 清零）。

## 叠层与权重平均 (Stacks & Weighted Average)

默认大多数异常**不叠层**（只保留最强一份）；开启 `<Ailment>CanStack` 后可叠多层，PoB2 用一套**权重平均**估算实战 DPS：

- `maxStacks = Override or (1 + ΣbaseStacks) * more(Stacks)`。
- `ailmentStacks`（实战平均在场层数）≈ `命中率 × 施加几率 × duration × 攻击/施法频率 × dpsMultiplier`（图腾乘活动图腾数；可被 config `Multiplier:<Ailment>Stacks` 覆盖）。
- `StackPotential = ailmentStacks / maxStacks`：>100% 时把单层 roll 向高位偏移（`RollAverage = (ailmentStacks - (maxStacks-1)/2)/(ailmentStacks+1) * 100`），<100% 时取 50%（区间中点）。
- 最终 `ailmentDPS = baseVal * effectMod * rateMod * activeAilments * effMult`，再 clamp 到 `DotDpsCap`。

> 感电/冰缓也有"叠层"分支（`ShockCanStack` / `ChillCanStack`），通过 `Multiplier:ShockStacks` 等把多层效果累加为 `CurrentShock` / `CurrentChill`，再写入敌人 `DamageTaken` / `ActionSpeed`。

## 增殖 / 传播 (Proliferation)

- **增殖 (Proliferation)**：把已施加在某敌人身上的异常状态扩散到其周围一定范围内的其他敌人（常见于点燃/冰缓/感电相关辅助宝石或天赋）。PoB2 主要影响 DPS 估算的覆盖范围/有效目标数，建模上落在"几率/在场层数"与 AoE 配置，而非改变单体公式。
- **传染 (类 Bonechill / 易伤联动)**：如 `Bonechill`（冰缓时叠加冷伤易伤）、`Shock` 让目标 `DamageTaken` 增加并因此放大后续异常伤害（`effMult` 链）。
- **元素易伤 (Exposure)**：`Fire/Cold/LightningExposureChance` / `InflictExposure` 降低敌人对应抗性，间接提升异常伤害与几率。

## 元素 / 非元素归类与例外

| 伤害类型 | 可施加的异常（默认 ScalesFrom）|
|---------|------------------------------|
| 火焰 | 点燃 (Ignite) |
| 冷 | 冰缓 (Chill)、冰冻 (Freeze) |
| 闪电 | 感电 (Shock)、电击 (Electrocute，仅特定来源) |
| 物理 | 流血 (Bleeding) |
| 物理 / 混沌 | 中毒 (Poison) |

**改写施加规则的例外**（PoB2 用 `<DamageType>Can<Ailment>` / `Cannot<Ailment>` 旗标实现）：
- `Voltaxic Rift`：允许混沌伤害贡献感电几率（`ChaosCanShock`）。
- `Blood Barbs` / `Blistering Bond` 等：允许用元素伤害施加流血（`FireCanBleed` 等）。
- `LightningCanFreeze` / `ChaosCanFreeze`（如 `ColdUnravel` / `LightningUnravel` 条件触发）。
- 头盔类隐喻（wiki 示例）：`Fire Damage from Hits Contributes to Shock Chance instead of ...`、`Lightning Damage Contributes to Freeze Buildup instead of Shock Chance`。
- "Allowing another damage type to contribute"语义：让该类型伤害**汇总进**对应异常的几率/积累/强度计算[^poe2wiki-ailment]。

## 异常状态与伤害计算的衔接

- damaging ailment 的 magnitude 基于击中的**未减免伤害 (Pre-mitigation)**，但**敌方"受到伤害"修饰词仍生效**：PoB2 `effMult = (1 - resist/100) * (1 + takenInc/100) * takenMore`（抗性、`DamageTaken`、`DamageTakenOverTime`、对应类型 taken 等）。
- 转化：`<Ailment>ToChaos` / `<Ailment>ToFire` 让该异常按混沌/火焰结算抗性与 taken。
- 派生地面：点燃可 `IgniteDpsAsBurningGround`（燃烧地面），中毒可 `PoisonDpsAsCausticGround`（腐蚀地面）。
- 所有 DoT 受全局 `DotDpsCap` 限制（PoB2 会显示 overcap 提示）。

---

## PoB2 计算实现（`CalcOffence.lua` / `CalcPerform.lua` / `Data.lua` / `Misc.lua`，核对基准）

以下取自 [PathOfBuilding-PoE2 `dev` 分支](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcOffence.lua)，是 pobr 的回归基准。

**阈值**
```lua
-- 异常阈值（点燃/感电/冰缓）
enemyThreshold = data.monsterAilmentThresholdTable[enemyLevel] * mod(EnemyAilmentThreshold)
-- 姿态阈值（冰冻/电击/重眩晕/钉刺）
enemyPoiseThreshold = floor(monsterPoiseThresholdTable[enemyLevel]
    * mod(PoiseThreshold, <Ailment>Threshold, [EnemyStunThreshold], [EnemyAilmentThreshold]))
```

**几率派生（点燃 / 感电）**
```lua
hitChance  = hitAvg  / enemyThreshold * data.gameConstants[ailment.."ChanceMultiplier"]  -- Shock=25, Ignite=20
finalChance = min(100, (hitChance + base) * (1 + inc/100) * more)
-- base/inc/more 来自 Enemy<Ailment>Chance / AilmentChance（+ 敌方 Self<Ailment>Chance）
```

**冰缓 / 感电 效果 (nonDamagingAilmentsConfig)**
```lua
Chill.effect = ChillEffectMultiplier * (damage/enemyThreshold) * effectMod   -- 线性, clamp [30,50]
Shock.effect = 50 * (damage/enemyThreshold)^0.4 * effectMod                  -- 幂律 ramping, clamp [20,100]
chillMinimumThreshold = enemyThreshold / ChillEffectMultiplier               -- 冰缓最小可施加阈值
```

**冰冻 / 电击 积累 (poise buildup)**
```lua
poiseBuildup = data.gameConstants[ailment.."DamageScale"] / enemyPoiseThreshold
    * (1 + inc/100) * more * 100
-- inc/more 来自 Enemy<Ailment>Buildup / EnemyImmobilisationBuildup / (敌方) <Ailment>Buildup
```

**持续伤害 DPS（damaging ailment）**
```lua
ailmentPercentBase = data.misc[ailment.."PercentBase"] * MagnitudeEffect   -- Bleed .15/s, Ignite/Poison .20/s
baseVal = calcAilmentDamage(...) * ailmentPercentBase
rateMod = (mod(<Ailment>Faster) + 敌方 Self<Ailment>Faster/100) / mod(<Ailment>Slower)
ailmentDPS = min(DotDpsCap, baseVal * mod(AilmentEffect) * rateMod * activeAilments * effMult)
duration   = durationBase * durationMod / rateMod * debuffDurationMult
```

**关键稳定标识 / 旗标**：
- 数据表：`monsterAilmentThresholdTable`、`monsterPoiseThresholdTable`、`nonDamagingAilment`（Chill/Freeze/Shock 的 default/min/max/duration/precision）、`defaultAilmentDamageTypes`、`buildupTypes`、`ailmentTypeList`、`elementalAilmentTypeList`。
- 常量（Misc.lua）：`BleedingHitDamagePercentPerMinute=900`、`IgniteHitDamagePercentPerMinute=1200`、`PoisonHitDamagePercentPerMinute=1200`、`ChillEffectMultiplier=100`、`ChillMaxEffect=50`、`BaseShockMagnitude=20`、`ShockChanceMultiplier=25`、`IgniteChanceMultiplier=20`、`FreezeDamageScale=2.1`(玩家2.0)、`ElectrocuteDamageScale=1.7`、`FreezeThresholdModifier/ElectrocuteThresholdModifier=500`、`PlayerAilmentThresholdLifeFactor=0.5`、各 `Base*Duration`。
- 旗标/词条名：`<Ailment>Chance`、`AilmentChance`、`Enemy<Ailment>Chance`、`Self<Ailment>Chance`、`<Ailment>CanStack`、`<Ailment>Stacks`、`AilmentMagnitude`、`Enemy<Ailment>Magnitude`、`AilmentEffect`、`<Ailment>Faster/Slower`、`Enemy<Ailment>Duration`、`DamagingAilmentDuration`、`EnemyElementalAilmentDuration`、`Self*AilmentDuration`、`EnemyAilmentThreshold`、`PoiseThreshold`、`Enemy<Ailment>Buildup`、`EnemyImmobilisationBuildup`、`<DamageType>Can<Ailment>` / `Cannot<Ailment>`、`<Ailment>ToChaos/ToFire`、`AilmentsAreNeverFromCrit`、`Avoid<Ailment>`、`IgniteDpsAsBurningGround`、`PoisonDpsAsCausticGround`、`CurrentShock/CurrentChill/MaximumShock/MaximumChill`、`ManaAppliesToShockEffect`。

---

## 对 pobr 实现的启示

当前 `pobr-core` 尚无异常状态建模（`calc/offence.rs` 仅有 minimal offence）。要对齐 PoB2，建议在 `CalcConfig` / `ModName` / flags / 数据层分层落地：

- **双阈值数据表**：把 `monsterAilmentThresholdTable` / `monsterPoiseThresholdTable`（按 enemyLevel 索引）纳入 `pobr-data` 的 catalog（或 gameConstants 表），由 `pobr-gamedata` loader 读入；`EnemyAilmentThreshold` / `PoiseThreshold` 作为可聚合 `ModName`。
- **两条派生路径分开实现**：
  - 几率派生（点燃/感电）：复用现有 `(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)` 管线，base 项里注入 `hitDmg/threshold * ChanceMultiplier`，最后 clamp 100。
  - 强度直出/积累（冰缓线性、感电幂律 `^0.4`、冰冻/电击 `DamageScale/poiseThreshold`）：需要在 calc 里加非线性算子；TraceGraph 的 `TraceOperation` 要支持幂律节点以便归因。
- **强度/效果/时长/节奏四维分离**：分别用独立 `ModName`（`AilmentMagnitude` / `AilmentEffect` / `*AilmentDuration` / `*Faster|*Slower`），避免把它们混进同一聚合桶——这是与 PoB2 行为一致的关键。
- **暴击加权**：异常基础伤害需同时持有 hit/crit 两份来源并按暴击几率加权（`calcAilmentDamage` 语义），与 `calc/offence.rs` 的暴击链对接；提供 `AilmentsAreNeverFromCrit` flag 与 `ailmentMode = AVERAGE|CRIT` 配置。
- **未减免 vs 受到伤害的边界**：magnitude 取 pre-mitigation；`effMult`（抗性 + DamageTaken/DamageTakenOverTime）单独成段——这正好契合 pobr "calc 纯函数 + 确定性"约定，把 player 侧与 enemy 侧聚合在只读快照阶段分别求值。
- **叠层权重平均**：`<Ailment>CanStack` / `StackPotential` / `RollAverage` 是 DPS 估算的核心，应作为一等公民建模（含 `Multiplier:<Ailment>Stacks` config 覆盖），并施加 `DotDpsCap`。
- **归因 (TraceGraph) 增量**：异常的每一步（几率来源、magnitude 词条、effect 词条、faster、敌方 threshold/抗性、暴击贡献、叠层）都应回溯到 `SourceId`——这是 pobr 相对 PoB 的核心增量价值。
- **0.5.0 差异显式标注**：流血移动增伤仅对怪物生效、冰缓最小阈值 30%（非 5%）、冰缓非玩家 8 秒、冰冻积累约慢 48%、`BaseShockMagnitude=20`——这些常量应集中在 `pobr-data` 的版本化数据目录 `data/<poe_version>/`，避免硬编码到计算逻辑。
- **遗留异常**（Scorch/Brittle/Sapped/Impale）：按"几率默认 0、仅特定来源触发"的低优先级旁路处理，不要进主伤害派生路径。

---

## 参考来源

[^mobalytics-ailments]: Mobalytics — PoE 2 Ailments Explained. https://mobalytics.gg/poe-2/guides/ailments
[^poe2wiki-ailment]: PoE2 Wiki — Ailment / Ailment Threshold / Elemental Ailments. https://www.poe2wiki.net/wiki/Ailment
[^poe2wiki-chill]: PoE2 Wiki — Chill. https://www.poe2wiki.net/wiki/Chill
[^poe2wiki-freeze]: PoE2 Wiki — Freeze（含版本历史：积累约慢 48%、连冻抑制）。https://www.poe2wiki.net/wiki/Freeze
[^poewiki-shock]: PoE Wiki — Shock（效果公式 `E = 1/2 * (D/T)^0.4 * (1+M)` 与示例表）。https://www.poewiki.net/wiki/Shock
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^fextra-050]: Fextralife PoE2 Wiki — Patch Notes 0.5.0（Soul Core of Retreat 改为 +15% ES 异常/眩晕阈值）。https://pathofexile2.wiki.fextralife.com/Patch+Notes+050
[^sidia-050]: SidiaDevelopment/poe2-patch — 0.5.0 raw patch notes（Ice Shot 不再给冰冻积累、Poisonburst Arrow 固定 3 秒中毒等）。https://github.com/SidiaDevelopment/poe2-patch/blob/main/patchnotes-0.5.0-raw.md
[^pob2-deepwiki-ailments]: Path of Building for PoE2 DeepWiki — CalcOffence / Ailment Calculations。https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^pob2-calcoffence]: PathOfBuilding-PoE2 — `src/Modules/CalcOffence.lua`（异常段：阈值/几率派生/冰缓-感电效果/冰冻-电击积累/damaging DPS/叠层权重平均）、`src/Modules/CalcPerform.lua`（非伤害异常施加 Current/Maximum）、`src/Modules/Data.lua`（nonDamagingAilment / defaultAilmentDamageTypes / buildupTypes）、`src/Data/Misc.lua`（monsterAilmentThresholdTable / monsterPoiseThresholdTable / gameConstants）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcOffence.lua
