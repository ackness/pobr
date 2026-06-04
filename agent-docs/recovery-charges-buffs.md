# 恢复 / 充能 / 增益 / 防御层细节 (Recovery / Charges / Buffs / Defensive Layers)

本文档汇总 PoE2（0.5.0）中**恢复 (Recovery)**、**充能 (Charges)**、**增益 (Buffs)**、**EHP / 防御层乘数**以及 **Spirit 保留 (Reservation)**、**存在范围 (Presence)** 的细分机制。

> 本文与既有文档**互补、不重复**：
> - 伤害承受顺序（ES → 生命 → 符咒护佑 → 死亡）见 [damage-defence-order.md](./damage-defence-order.md)；
> - 能量护盾充能 (Recharge) 与混沌双倍伤害、0.5.0 偷取重制概览见 [energy-shield.md](./energy-shield.md)；
> - 护甲/PDR、闪避、格挡、抗性各自成文，本文只补它们没覆盖的「恢复 / 充能 / 增益 / EHP 层乘数」主题，并交叉引用。
>
> 末尾「PoB2 计算实现」给出核对过的真实变量/旗标名，是 pobr 的回归基准。

---

## 一、三大充能 (Charges)

### 1.1 关键转变：PoE2 充能**不再自带属性**

> **与 PoE1 的核心差异**：在 PoE2 中，充能本身**不授予任何固有数值加成**（不像 PoE1 的「每暴击充能 +X% 暴击几率」）。充能只是一种**可被技能 / 词条消耗或引用**的资源[^poe2wiki-charge][^poe2db-charges]。

加成只来自**显式引用充能的修饰词**，两种形式：
- **`per X Charge`（每层）**：如「12% increased Critical Damage Bonus per Power Charge」「8% increased Evasion Rating per Frenzy Charge」「5% of maximum Life for each Endurance Charge consumed」。
- **`while you have a Charge`（持有时）**：如 `Charge Regulation`（持续增益宝石）「15% more Critical Hit Chance while you have a Power Charge / 12% more Defences while you have an Endurance Charge / 8% more Skill Speed while you have a Frenzy Charge」[^poe2wiki-charge]。

含义：充能的价值完全由 build 引用它们的词条决定。PoB2 把当前层数暴露为 `modDB.multipliers["PowerCharge"]` / `["FrenzyCharge"]` / `["EnduranceCharge"]`，再由带 `Multiplier` tag 的修饰词放大（与本仓 `Modifier::effective_number` 的 Multiplier tag 同构）。

### 1.2 三种充能与所属属性

| 充能 | 关联属性 | 典型获取 | 典型用途 |
|------|---------|---------|---------|
| **Power（能量）** | 智力 (Int) | 暴击时获得（如「25% chance to gain a Power Charge on Critical Hit」） | 暴击 / 法术缩放、被技能消耗 |
| **Frenzy（狂怒）** | 敏捷 (Dex) | 击中 / 冻结 / 击杀时获得 | 速度 / 伤害缩放、被技能消耗 |
| **Endurance（耐力）** | 力量 (Str) | 眩晕敌人 / 战吼时获得 | 防御 / 抗性缩放、被技能消耗回血 |

### 1.3 数值（PoB2 核对）

- **最大层数**：默认 **3 层 / 种**（PoB2 `Data/Misc.lua`: `max_power_charges = max_frenzy_charges = max_endurance_charges = 3`）。装备 / 天赋可加 `+N to Maximum X Charges`[^poe2db-charges]。
- **充能持续时间**：基础 **15 秒**（PoB2 `CalcSetup.lua`: `NewMod("ChargeDuration","BASE",15,"Base")`；0.5.0 之前为 20 秒）[^poe2wiki-charge]。
  - 同种充能新获得一层会**刷新该种全部层的计时**；满层时再获得不增加层数但仍刷新计时。
  - **0.5.0**：消耗充能的技能在使用期间会**暂停**对应充能的计时（"pause their corresponding charge's duration while in use"）[^poe2wiki-charge]。
- **持续时间修饰词**：`ChargeDuration`（全部）/ `PowerChargesDuration` / `FrenzyChargesDuration` / `EnduranceChargesDuration`（单种），通过 `calcLib.mod` 以 inc/more 缩放。

### 1.4 其它充能家族

PoB2 还建模了一批衍生 / 升华充能：`Inspiration`（基础上限 5）、`Blood`（5）、`Siphoning` / `Challenger` / `Blitz` / `Brutal` / `Absorption` / `Affliction` / `Spirit` / `CrabBarrier` / `GhostShroud` 等，多由特定升华或传奇赋予（默认上限 0）。其中一些以「最大值等于另一种充能」联动（如 `MaximumFrenzyChargesIsMaximumPowerCharges`、`EnduranceChargesConvertToBrutalCharges`）。

### 1.5 最小充能 (Minimum Charges)

`PowerChargesMin` / `FrenzyChargesMin` / `EnduranceChargesMin`：保证常驻一定层数（如「+1 to Minimum Frenzy Charges」）。`MinimumXChargesIsMaximumXCharges` 旗标可把最小拉满到上限——常见的「常驻满层」实现。PoB2 默认按「条件 `Condition:UseXCharges` 为真即取上限」估算 DPS。

---

## 二、恢复机制 (Recovery)

PoE2 恢复分四类：**再生 (Regeneration)**、**偷取 (Leech)**、**返还 (Recoup)**、**直接恢复 / 恢复速率 (Recovery / Recovery Rate)**（含药剂 flask / 护符 charm）。能量护盾的**充能 (Recharge)** 是单独一类，见 [energy-shield.md](./energy-shield.md)，不在此重复。

### 2.1 再生 (Regeneration)

按资源（Life / Mana / Energy Shield）独立结算，遵循标准管线：

```
基础再生 = Σ(资源Regen 固定值) + 资源池 × Σ(资源RegenPercent)/100
再生/秒  = 基础再生 × (1 + Σinc/100) × Π(1 + more/100) × RecoveryRateMod
```

- inc/more 来自 `XRegen` **与** `XRecoveryRate` 两类词条之和（PoB2 把它们合并 sum/more）。
- `RecoveryRateMod` = `LifeRecoveryRate` / `ManaRecoveryRate` / `EnergyShieldRecoveryRate` 的 calcLib.mod 结果，**同时**作用于再生、直接恢复与 Recoup。
- **能量护盾默认不再生**（其恢复靠 Recharge）。`ZealotsOath` 把生命再生转给 ES（`OverflowEnergyShieldRecovery`）；`ManaRegenAppliesToEnergyShieldRecharge`（Waveshaper）把法力再生折进 ES 充能。
- 关键旗标：`NoLifeRegen` / `CannotGainLife`、`UnaffectedByLifeRegen`、`CannotRecoverLifeOutsideLeech`。

### 2.2 偷取 (Leech) — 0.5.0 重制

偷取是把一次击中伤害的一部分，**随时间**恢复为资源。0.5.0 偷取被完全重制（概览见 [energy-shield.md](./energy-shield.md)），本节补**实例化与上限**细节：

- **每实例速率**：每个偷取实例以「被偷伤害 × 偷取% 」为总量，按固定**每秒比例**回复（PoE 系列默认 2%/s/实例，至总量回完）。
- **单资源单实例（0.5.0 关键）**：每种资源**同时只有一个偷取实例生效**；多实例时**只有恢复率最高者生效**直到过期，再轮到次高者[^maxroll-050-patchnotes]。这与 PoE1「实例无限叠加」截然不同。
- **偷取速率上限 (Max Leech Rate)**：恢复速率上限按资源池百分比给出（PoB2 `CalcSetup.lua`）：
  - 生命 / 法力：默认 **20%**（`MaxLifeLeechRate = MaxManaLeechRate = 20`）；
  - 能量护盾：默认 **10%**（`MaxEnergyShieldLeechRate = 10`）。
  - 即 `MaxLifeLeechRate = Life × 20% /秒`，所有实例叠加后被此值 clamp。
- **单实例上限 (Max Leech Instance)**：单个实例可恢复的资源量上限默认 **10%** 池（`MaxLifeLeechInstance = MaxManaLeechInstance = MaxEnergyShieldLeechInstance = 10`）。
- **单击伤害上限 40,000**：用于偷取的有效击中伤害上限为 **40,000**（PoB2 `Data/Misc.lua`: `EffectiveMaxDamageForLeech = 40000`）；超过部分按比例缩放[^maxroll-050-patchnotes]。
- **伤害类型限制**：多数 PoE2 偷取**默认仅物理**（"most PoE2 leech is physical only by default"）；元素偷取需依赖物理偷取词条转化[^pob2-deepwiki-es]。
- **流血 / 毒绕过 ES**：偷取本身是恢复，与「流血/毒绕过 ES」是承受侧机制，见 [energy-shield.md](./energy-shield.md)。
- 关键旗标：`CannotLeechLife` / `CannotLeechEnergyShield` / `CannotLeechMana`、敌方 `CannotLeechLifeFromSelf`、`MaximumLifeLeechIsEqualToParent`（召唤物继承）。

### 2.3 返还 (Recoup)

Recoup 把一次击中**承受的伤害**的一部分，在固定时长内**返还为恢复**：

- 默认**在 8 秒内**返还（PoB2：`4SecondRecoup` / `4SecondLifeRecoup` 等旗标可改为 4 秒）。
- 按返还到的资源分 `LifeRecoup` / `ManaRecoup` / `EnergyShieldRecoup`，并乘对应 `RecoveryRateMod`。
- 可按伤害类型限定（PoB2 用 `damageType..recoupType.."Recoup"`），且 `AddLifeRecoupToEnergyShieldRecoup` 等旗标可把生命返还转给 ES（如 `Sacrosanctum`）。
- Recoup 基于**承受后**伤害（在防御减免之后那部分进入资源池的伤害），与偷取（基于造成的击中伤害）不同。

### 2.4 恢复速率与药剂 / 护符 (Recovery Rate / Flask & Charm)

- `LifeRecoveryRate` / `ManaRecoveryRate` / `EnergyShieldRecoveryRate` 是统一的「恢复速率」乘区，作用于上述所有恢复来源。
- 药剂 (Flask) / 护符 (Charm) 提供瞬时或持续的直接恢复（`XRecovery` BASE），同样受 `RecoveryRateMod` 影响；其充能数 (`MaxCharges`) 与效果 (`FlaskEffect`) 是独立体系（如 Silver Flask 给 Onslaught，见 §3.2）。

---

## 三、增益 (Buffs)

增益给玩家自身加 buff（区别于充能：buff **直接授予数值**）。统一缩放因子是 **`BuffEffectOnSelf`**（自身增益效果）——几乎所有内置 buff 的强度都乘 `(1 + Σ BuffEffectOnSelf/100)`。以下数值取自 PoB2 `CalcPerform.lua`（已核对）。

### 3.1 Fortification / Fortify（坚定）

PoE2 用**层数制**的 Fortification：

```
每层减伤 = floor( (1 + Σ BuffEffectOnSelf/100) × 1 )    -- 每层 1% MORE 减伤
→ modDB:NewMod("DamageTakenWhenHit", "MORE", -stacks×effectScale, "Fortification")
```

- 即「Fortification 层数」直接转成 `DamageTakenWhenHit` 的 **MORE 负值**（受击时少受这么多伤害）。
- `MaximumFortification` 为层数上限；`MinimumFortification`、`FortificationStacks` 可覆盖；满层置 `Condition:HaveMaximumFortification`。
- `Condition:NoFortificationMitigation` 可关闭减伤（只保留触发条件）。

### 3.2 Onslaught（猛攻）

```
effect = floor( 10 × (1 + (OnslaughtEffect + BuffEffectOnSelf)/100) )
→ Speed +2×effect (Attack & Cast), WarcrySpeed +2×effect, MovementSpeed +effect
```

基础 `effect=10` 时 = **+20% 攻击/施法速度、+10% 移动速度**。可由 Silver Flask 授予（PoB2 会把 flask 的 `effectInc` 叠进 `flaskEffectInc`）。

### 3.3 其它通用增益（PoB2 内置，effect 基数 × (1+BuffEffectOnSelf/100)）

| Buff | 效果（基数） |
|------|------------|
| **Adrenaline** | +100% increased 伤害、+25% 攻击/施法/移动速度、**+10 BASE PhysicalDamageReduction** |
| **Convergence** | +30% MORE 元素伤害 |
| **Fanaticism**（自施法术时）| +75% MORE 施法速度、−75% 法力消耗、+75% 范围 |
| **Unholy Might** | 30% 物理 Gain As Chaos（按 Magnitude=100 multiplier）|
| **Chaotic Might** | +30 BASE PhysicalDamageGainAsChaos |
| **Elusive** | 按 `ElusiveEffect` 缩放：+15% AvoidAllDamageFromHits、+30% 移动速度（效果会随时间衰减，PoB2 取阈值/2 的估值）|
| **Her Embrace** | 免疫眩晕/冰冻/冰缓/点燃、剑攻击 +123% 物转火、+20% 攻击/施法/移动速度 |

> **致盲 (Blind)** 虽列在 buff 段，但属于**对敌 debuff**：`Accuracy` 与 `Evasion` 各 `MORE −20% × (1+BlindEffect/100)`（与暴击交互见 [critical-hits.md](./critical-hits.md) 的 `Blindside`）。

### 3.4 Tailwind（顺风）

PoE2 的 Tailwind 是「行动速度 (Action Speed)」类增益（每层提升 action speed）；在 PoB2 中其影响汇入 `ActionSpeed`（见 `calcs.actionSpeedMod`，受 `MinimumActionSpeed` / `MaximumActionSpeedReduction` clamp，`UnaffectedBySlows` 只取正值）。具体每层数值随版本/来源而定，实现时以游戏内一手数据为准。

### 3.5 增益效果修饰 (Buff Effect)

- **`BuffEffectOnSelf`**：放大施加在自己身上的 buff（光环、Fortify、Onslaught…）。
- 各 buff 专属：`OnslaughtEffect`、`ElusiveEffect`、`BlindEffect`、`ShrineBuffEffect`…
- buff **时长**通常由 `Duration` / 技能 `secondaryDuration` 体系处理（与本文档恢复/充能分离）。

---

## 四、防御层与 EHP 细节 (Defensive Layers & EHP)

伤害承受**顺序**与各防御层（护甲/闪避/ES/格挡/抗性/符咒护佑）已在 [damage-defence-order.md](./damage-defence-order.md) 等文档覆盖。本节只补**「承受伤害乘数」的叠加规则**与 **Max Hit / EHP**。

### 4.1 「承受的伤害」乘数 (Damage Taken Multiplier)

每种伤害类型，受击时的「承受伤害乘数」遵循与攻击侧同构的管线（PoB2 `CalcDefence.lua`）：

```
TakenHitMult(type) = max( 0, (1 + Σinc/100) × Π(1 + more/100) )
其中
  inc  = DamageTaken + <type>DamageTaken + DamageTakenWhenHit + <type>DamageTakenWhenHit (+ Elemental… 若元素)
  more = 同名 MORE 连乘
```

- **inc 加法成桶、more 各自连乘**——与本仓 `ModDb::sum` / `ModDb::more` 语义一致。
- 三种细分上下文：`...WhenHit`（击中）、`...OverTime`（持续）、`...Reflect`（反射），各算各的乘数。
- **顺序**（承受侧，见 damage-defence-order §5）：固定 (`BASE DamageTaken`) → inc/reduced（求和）→ more/less（连乘）。Fortify/Bulwark/Wither/Shock 等都落在这条管线的对应桶里。
- DoT 还要额外乘 `(1 − resist/100) × (1 − reduction/100)`（持续伤害走 resist + DR，再乘 taken 乘数）。

### 4.2 单次最大承受伤害 (Max Hit)

PoB2 在 `calcs.buildDefenceEstimations` / EHP 段，对每种伤害类型计算**能在不死亡前提下承受的最大单次击中**：把命中后的减免链（resist + 护甲/DR + taken 乘数 + 转移/Guard/ES/生命池）反解出能消化的 incoming 伤害上限。

- 护甲对 Max Hit 的贡献是**非线性**的（护甲减伤随击中变大而下降，见 [armour.md](./armour.md)），所以 Max Hit 不是简单 `池 / 乘数`，PoB2 用迭代/解析求解。
- 物理 / 火 / 冰 / 电 / 混沌**各有独立 Max Hit**——这是「各伤害类型有效血量不同」的根因。
- **EHP**：把 Max Hit 与回避率（闪避/躲避/格挡/AvoidAllDamageFromHits）组合，得到统计意义的有效血量。`EHPUnluckyWorstOf` 配置项可把回避按「取较差」缩放（`luck = luck / worstOf`），用于悲观估计。

### 4.3 命中前的「不被击中几率」

PoB2 把各避免层乘起来（`CalcDefence.lua` ~2018）：

```
MeleeNotHitChance      = 1 − (1−Evade) × (1−Dodge) × (1−AvoidAllDamageFromHits)
ProjectileNotHitChance = 上式 × (1−AvoidProjectiles)
SpellNotHitChance      = 1 − (1−SpellEvade) × (1−SpellDodge) × (1−AvoidAllDamageFromHits)
```

这是 EHP 的「命中前层」；命中后才走 §4.1 / §4.2 的减免与池。各层独立、乘法叠加。

---

## 五、Spirit 保留 (Reservation) 与存在范围 (Presence)

### 5.1 Spirit 与保留

PoE2 用**Spirit（精魂）**而非 PoE1 的法力百分比来驱动持久效果：

- **Spirit 是一个固定池**（`modDB.Spirit`，BASE 起始 0）。战役任务最多给 +100 永久 Spirit；其余主要来自装备（如 Sceptre 权杖）与升华[^poe2wiki-spirit]。
- **保留 (Reservation)**：持久技能（光环 / 持续 buff / 永久召唤 / 触发 meta gem）**保留**一定 Spirit；被保留的 Spirit 不能再用于其它目的。`剩余 Spirit = 总 Spirit − 已保留`[^poe2wiki-reservation]。
- 保留**不改变资源最大值**，只改「未保留上限」。**持久保留效果在死亡时不解除**。
- **由装备/升华直接授予的持久 buff / 召唤通常不保留 Spirit**（如权杖光环、部分升华）[^poe2wiki-spirit]。

### 5.2 保留效率 (Reservation Efficiency)

```
所有 #% increased/reduced Reservation Efficiency 先求和，再按效率公式换算最终保留
最终保留 ≈ 基础保留 / (1 + 总效率/100)        （效率越高、保留越少；100%+ 效率→无限保留→不可用）
```

- 「Skills reserve N% less Spirit」「Persistent Buffs have N% less Reservation」类**乘区 (less)** 与效率**乘法叠加**[^poe2wiki-reservation]。
- 效率词条多带条件域：`Reservation Efficiency of Skills` / `of Buff Skills` / `of Companion Skills` / 创建亡灵的技能等。
- **非技能来源的保留**（如 `Widow's Reign` 把生命损失转为保留、`Blood Price` 敌人保留生命）**不受效率影响**。
- 升华举例：Infernalist「Reserves 25% of Life」换 +Spirit/Mana/ES；Tactician「Persistent Buffs have 50% less Reservation」。

### 5.3 存在范围 (Presence)

- **Presence** 是角色周围一片区域，许多**光环 (Aura)** 与「Enemies in your Presence」类效果在此生效。基础半径 **4 米**（PoB2 `CalcSetup.lua`: `PresenceRadius = base_presence_radius = 40`，单位 0.1m）[^poe2db-presence]。
- 由 **Presence Area 修饰词**缩放（`PresenceRadius` / `PresenceAreaOfEffect`），**不受**普通「技能范围 (Skill Area)」修饰词影响。
- 与暴击的联系：暴击弱点光环 `Malice`、对敌 debuff 光环都以 Presence 为作用域（见 [critical-hits.md](./critical-hits.md) §暴击弱点）。

---

## PoB2 计算实现（核对基准）

变量/旗标取自 [PathOfBuilding-PoE2 `dev`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的 `src/Modules/CalcSetup.lua`、`CalcPerform.lua`、`CalcDefence.lua`、`CalcOffence.lua` 与 `src/Data/Misc.lua`，是 pobr 的回归基准：

**充能（`CalcSetup.lua` / `CalcPerform.lua` / `Data/Misc.lua`）**
```lua
-- 基础常量
ChargeDuration BASE 15;  max_power/frenzy/endurance_charges = 3
-- 每种：XChargesMax / XChargesMin / XChargesDuration / Condition:UseXCharges / HaveMaximumXCharges
-- 暴露为乘数供 per-charge 词条引用：
modDB.multipliers["PowerCharge" | "FrenzyCharge" | "EnduranceCharge" | "TotalCharges" | "RemovableXCharge"]
-- 联动旗标：MaximumFrenzyChargesIsMaximumPowerCharges、EnduranceChargesConvertToBrutalCharges、
--           PowerChargesConvertToAbsorptionCharges、MinimumXChargesIsMaximumXCharges
```

**恢复（`CalcDefence.lua`）**
```lua
-- 再生
baseRegen = Sum("BASE", XRegen) + pool × Sum("BASE", XRegenPercent)/100
regen     = baseRegen × (1 + Sum("INC", XRegen, XRecoveryRate)/100) × More(XRegen, XRecoveryRate)
output[X.."RegenRecovery"] = regen − degen + recovery + overflow      -- 再乘 XRecoveryRateMod
-- 偷取上限（CalcSetup BASE）：MaxLifeLeechRate=20, MaxManaLeechRate=20, MaxEnergyShieldLeechRate=10
--                            MaxLifeLeechInstance=MaxManaLeechInstance=MaxEnergyShieldLeechInstance=10
MaxLifeLeechRate = Life × MaxLifeLeechRatePercent/100
EffectiveMaxDamageForLeech = 40000        -- Data/Misc.lua，单击偷取上限
-- Recoup
output[damageType..recoupType.."Recoup"] = recoup × output[recoupType.."RecoveryRateMod"]
-- 时长默认 8s；旗标 4SecondRecoup / 4Second<Type>Recoup → 4s；AddLifeRecoupToEnergyShieldRecoup
```

**增益（`CalcPerform.lua`，统一乘 `BuffEffectOnSelf`）**
```lua
Fortification: NewMod("DamageTakenWhenHit","MORE", -floor((1+BuffEffectOnSelf/100)*stacks))
Onslaught:     effect=floor(10*(1+(OnslaughtEffect+BuffEffectOnSelf)/100)); Speed +2*effect(Attack/Cast)
Adrenaline:    Damage INC 100, Speed INC 25, PhysicalDamageReduction BASE 10  (× effectMod)
Convergence:   ElementalDamage MORE 30;  Fanaticism: Cast Speed MORE 75
-- 旗标：Fortified / MaximumFortification / Onslaught / Adrenaline / Convergence / Fanaticism /
--       UnholyMight / ChaoticMight / Elusive / Blind / UnaffectedBySlows
```

**防御层 / EHP（`CalcDefence.lua`）**
```lua
TakenHitMult = max(0, (1 + takenInc/100) * takenMore)               -- inc 求和、more 连乘
-- takenInc 来自 DamageTaken/<type>DamageTaken/DamageTakenWhenHit/<type>DamageTakenWhenHit(+Elemental)
-- DoT 额外乘 (1-resist/100)*(1-reduction/100)
MeleeNotHitChance = 1 - (1-Evade)*(1-Dodge)*(1-AvoidAllDamageFromHits)
-- Max Hit / EHP 按伤害类型分别求解；EHPUnluckyWorstOf 用于悲观估计 (luck = luck / worstOf)
```

**Spirit / 保留 / Presence（`CalcSetup.lua` / `CalcPerform.lua`）**
```lua
Spirit BASE 0;  output.Spirit = floor(calcLib.val(modDB, "Spirit"))
PresenceRadius BASE base_presence_radius(=40);  PresenceAreaOfEffect
-- 保留：Reservation Efficiency 词条求和后换算；less Reservation 乘区与效率乘法叠加
-- CannotGainSpiritFromEquipment 旗标
```

---

## 对 pobr 实现的启示

对照 `pobr-core`（`mod_db.rs` / `config.rs::CalcConfig` / `calc/*.rs` / `trace.rs`）落地建议：

1. **充能建模为「multiplier + per-charge 引用」，而非固有属性。**
   - 在 `CalcConfig` / `ModList` 增加 `multipliers`：`PowerCharge` / `FrenzyCharge` / `EnduranceCharge` / `TotalCharges`，由 `Modifier` 的 **Multiplier tag**（已存在于 `effective_number`）放大对应词条。**不要**给充能挂任何默认数值。
   - 充能上限/最小/持续走稳定 `ModName`（`PowerChargesMax/Min/Duration` …），默认上限 3、时长 15。

2. **恢复三类分层，复用现有 sum/more 管线。**
   - 再生：`base + pool×percent → ×(1+Σinc/100)×Π(1+more/100) × RecoveryRateMod`，与 `scaled_numeric_stat` 同构，直接复用。
   - 偷取：实现「单资源单实例（取最高速率）」+ 三个上限（`MaxLeechRate` 20/20/10%、`MaxLeechInstance` 10%、单击 40000）+ 默认仅物理。这是 0.5.0 与 PoE1 的最大行为差异，需测试 fixture 锁定。
   - Recoup：默认 8 秒、可 4 秒旗标；基于**承受后**伤害，与偷取（基于造成伤害）区分清楚。

3. **增益统一走 `BuffEffectOnSelf` 乘区。**
   - Fortify → `DamageTakenWhenHit` MORE 负值；Onslaught → `Speed` inc 等。把 buff 实现为「打开一个 flag/condition → 注入若干 Modifier」，便于 TraceGraph 把每个输出回溯到 buff 来源 `SourceId`。

4. **承受伤害乘数复用 `ModDb::sum`/`more`。**
   - `TakenHitMult = (1+Σinc/100)×Π(1+more/100)`，inc 求和、more 连乘——与攻击侧完全同构，`calc/defence.rs::scaled_defence_stat` 可直接套用，只是 mod 名换成 `DamageTaken*` 家族。
   - 按 `WhenHit` / `OverTime` / `Reflect` 分上下文。

5. **Max Hit / EHP 按伤害类型分别求解。**
   - 因护甲减伤非线性，需迭代/解析；先实现各类型独立 Max Hit，再组合「命中前避免层」得 EHP。`EHPUnluckyWorstOf` 作为可选配置。这是 pobr 相对 PoB 可提供**逐来源 EHP 归因**的增量点。

6. **Spirit 保留是新维度，不要照搬 PoE1 法力百分比保留。**
   - `Spirit` 作为固定池；保留 = `基础保留 /(1+效率/100)` 再乘 less 乘区；区分「技能保留（受效率）」与「非技能保留（不受）」。Presence 半径 = `PresenceRadius`（默认 40 内部单位 = 4m），供光环/对敌 debuff 作用域判定。

---

## 参考来源

[^poe2wiki-charge]: PoE2 Wiki — Charge. https://www.poe2wiki.net/wiki/Charge
[^poe2db-charges]: PoE2DB — Charges. https://poe2db.tw/us/Charges
[^poe2wiki-spirit]: PoE2 Wiki — Spirit. https://www.poe2wiki.net/wiki/Spirit
[^poe2wiki-reservation]: PoE2 Wiki — Reservation. https://www.poe2wiki.net/wiki/Reservation
[^poe2db-presence]: PoE2DB — Presence. https://poe2db.tw/us/Presence
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-es]: Path of Building for PoE2 DeepWiki — CalcDefence / Leech & Recovery. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^pob2-calcsetup]: PathOfBuilding-PoE2 — `src/Modules/CalcSetup.lua`（充能/偷取上限/Spirit/Presence 基础常量）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcSetup.lua
[^pob2-calcperform]: PathOfBuilding-PoE2 — `src/Modules/CalcPerform.lua`（充能结算、Fortify/Onslaught/Adrenaline 等 buff、再生/Recoup）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcPerform.lua
[^pob2-calcdefence]: PathOfBuilding-PoE2 — `src/Modules/CalcDefence.lua`（偷取/再生/Recoup、承受伤害乘数、Max Hit/EHP）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcDefence.lua
[^pob2-misc]: PathOfBuilding-PoE2 — `src/Data/Misc.lua`（`max_*_charges=3`、`EffectiveMaxDamageForLeech=40000`、`base_presence_radius=40`、`energy_shield_recharge_rate_per_minute_%=750`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Misc.lua
