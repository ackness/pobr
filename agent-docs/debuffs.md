# 减益 / 诅咒 / 控制机制 (Debuffs / Curses / Control)

减益 (Debuff) 是施加在目标身上、持续一段时间的负面状态效果。本文聚焦**非异常状态**的减益：诅咒 (Curses)、曝光 (Exposure)、凋萎 (Wither)、破甲 (Armour Break)，以及位移/控制类 (Maim / Blind / Hinder / Pin / Daze 等) 和增伤型 debuff (Intimidate / Unnerve 等)。

> **分工说明**：
> - **异常状态** (流血/毒/点燃/冰冷/冻结/感电/电击) 归 [`ailments.md`](ailments.md)，本文不重复。
> - **眩晕 / 重眩晕** 归 [`stun.md`](stun.md)。
> - **护甲与伤害减免公式** 归 [`armour.md`](armour.md)；本文「破甲」一节只讲减益侧机制，减伤公式见 armour.md。
> - **暴击弱点 (Critical Weakness)** 已在 [`critical-hits.md`](critical-hits.md) 详述，本文只简述并指回。
>
> 减益在 PoB2 计算中几乎都建模为**敌方 modDB (`enemyDB`) 上的修饰词**——这是 pobr 需要对齐的核心：玩家输出 = 玩家 modList 聚合 × 敌方 modList 上的「受到伤害」类修饰。本文末尾「对 pobr 实现的启示」给出落地建议。

## 减益的通用作用方式：敌方 modDB

PoB2 把"敌人受到的增益/减益"放进 `enemyModList`/`enemyDB`，在计算玩家击中伤害时叠加进去。常见的两类标识：

- **`DamageTaken`（及 `<Type>DamageTaken`）**：敌人受到的伤害增减，按伤害管线 `INC` / `MORE` 聚合。例：感电 `DamageTaken INC 15`（见 ailments.md）、曝光把抗性压低间接增伤。
- **`<Element>Resist BASE -N`**：直接降低敌方抗性（曝光、Despair/Conductivity 等诅咒走这条）。
- **`Condition:<Name>` FLAG**：开启某种条件，让玩家侧的"对 X 状态敌人 +伤害"词条生效（如 `Condition:Maimed` → 玩家 `against Maimed Enemies` 词条）。

敌方减伤管线与玩家攻击聚合在 `CalcOffence.lua` 汇合。pobr 当前 `Env` 持有 player/enemy 两个 `Actor`（见 `CLAUDE.md` 计算引擎架构），方向一致。

---

## 诅咒 (Curses)

诅咒是施加在敌人身上的减益，分两大类[^poe2wiki-curse][^poe2db-curses]：

### Hex（咒术）vs Mark（标记）

| | **Hex（咒术）** | **Mark（标记）** |
|---|---|---|
| 作用范围 | 区域 (AoE)，可同时诅咒多个敌人 | 单体，只标记一个目标 |
| 典型例子 | Despair、Enfeeble、Conductivity、Frostbite、Flammability | Sniper's Mark、Punishment 等 |
| 上限 | 受**咒术上限 (Curse Limit)** 约束 | 受**标记上限 (Mark Limit)** 约束（独立计数） |
| Hexproof | 可被 Hexproof（咒术免疫）阻挡 | 通常不受 Hexproof 影响 |
| Doom（恶名） | 仅 Hex 可积累 Doom 提升效果 | 不积累 Doom |

**PoB2 旗标**：`skillFlags.hex` / `skillFlags.curse`；`curse.isMark` 区分标记。

### 诅咒上限 (Curse / Mark Limit)

- **默认咒术上限 = 1，标记上限 = 1**（PoB2 `CalcSetup.lua`：`EnemyCurseLimit BASE 1`、`EnemyMarkLimit BASE 1`）。两者**独立**，互不挤占。
- 提升上限的来源：被动天赋、装备 (`+1 to maximum number of Curses`)、辅助宝石。PoB2 求和 `EnemyCurseLimit` / `EnemyMarkLimit`，`curses.limit = EnemyCurseLimit + EnemyMarkLimit`。
- 当诅咒数量超限时，PoB2 用 `determineCursePriority`（基于 `data.cursePriority`：插槽顺序 / 来源 aura vs equipment / 环位等）决定哪些诅咒占用有限槽位，低优先级被挤掉。
- 特殊旗标：`CursesIgnoreCurseLimit`（无视上限，如 Vixen's/特定唯一）、`SocketedCursesAdditionalLimit`、`SocketedCursesHexLimitValue`、`CurseLimitIsMaximumPowerCharges`（上限 = 最大能量球数）。

### 诅咒效果 (Curse Effect)

最终诅咒强度 = 基础值 × 诅咒效果倍率。PoB2 在 `CalcPerform.lua` 聚合：

```lua
inc  = skillModList:Sum("INC", skillCfg, "CurseEffect") + enemyDB:Sum("INC", nil, "CurseEffectOnSelf")
more = skillModList:More(skillCfg, "CurseEffect") * enemyDB:More(nil, "CurseEffectOnSelf")
mult = (1 + inc/100) * more
```

- `CurseEffect`：施法者侧"诅咒效果提高/降低"。
- `CurseEffectOnSelf`：目标侧"受到的诅咒效果"修饰（含 boss 降低、亵渎之地 +10% 等）。
- `CurseEffectAgainstPlayers`：诅咒打到玩家时的额外修饰（你被诅咒时）。

### Doom（恶名，Hex 专属）

Hex 在持续期间会积累 Doom，提升该 Hex 的效果，至 `MaxDoom` 封顶：

```lua
hexDoom  = modDB:Sum("BASE", nil, "Multiplier:HexDoomStack")
maxDoom  = skillModList:Sum("BASE", nil, "MaxDoom")
skillModList:NewMod("CurseEffect", "INC", min(hexDoom, maxDoom) * DoomEffect, "Doom")
```

**PoB2 旗标**：`MaxDoom`、`Multiplier:HexDoomStack`、`DoomEffect`、`HexDoomLimit`。

### 对高稀有度怪物的诅咒效果降低（重要 0.3.0 变更）

> **0.3.0 起**：高稀有度敌人受诅咒影响降低——**魔法怪 15% less、稀有怪 30% less、唯一/Boss 怪 50% less 诅咒效果**[^poe2-030-patch]。PoB2 Boss 配置把 `CurseEffectOnSelf MORE -50` 写入 enemyDB（同一处也写 `ExposureEffectOnSelf MORE -50`）。

这意味着对 Boss 实战的诅咒收益只有面板的一半，pobr 做 Boss 场景计算时必须带上该 `MORE -50`。

### Hexproof（咒术免疫）

- 目标带 Hexproof 时 Hex 不生效；玩家侧 `CursesIgnoreHexproof` 可无视。
- PoB2：`modDB:Flag(nil, "Hexproof")` / `CursesIgnoreHexproof`。

### PoE2（0.3.0+）现存典型诅咒[^poe2db-curses][^poe2db-conductivity][^poe2db-despair]

| 诅咒 | 类型 | 默认效果（无加成时） | PoB2 落点 |
|---|---|---|---|
| **Despair（绝望）** | Hex (Chaos) | 降低混沌抗性约 -25%（宝石 -35~-49%） | `ChaosResist BASE -N` |
| **Enfeeble（衰弱）** | Hex | 目标造成伤害降低（唯一 -10%、其它 -20%；宝石 21~29% less） | 敌方 `Damage INC -N` |
| **Conductivity（导电）** | Hex (Lightning) | 降低闪电抗性 -23~-29% | `LightningResist BASE -N` |
| **Frostbite / Flammability** | Hex | 降低冰/火抗（同 Conductivity 套路） | `ColdResist`/`FireResist BASE -N` |
| **Sniper's Mark** | Mark | 受到的下一次暴击 +暴击伤害加成，并可消耗（见 critical-hits.md） | 敌方 `SelfCritMultiplier` 等 |

> 抗性削减型诅咒最终也是把敌方 `<Element>Resist` 压低，从而提升玩家命中该元素的有效伤害。与"穿透/曝光"叠加方式见下节。

---

## 曝光 (Exposure)

曝光是一种**降低指定元素抗性**的减益（火/冰/电三系各自独立），与诅咒、穿透是**不同来源**，可叠加。

### 数值与叠加规则（PoB2 实测）

- **基础曝光 = 该元素抗性 -20%**（PoB2 config：`FireExposure / ColdExposure / LightningExposure BASE 20`）。
- **不同来源的曝光不相加，取最强的一份**——PoB2 `CalcPerform.lua` 遍历 `enemyDB:Tabulate("BASE", element.."Exposure")`，对每个来源独立结算后**取 magnitude 最大者**：

```lua
value = floor((value + extraExposure)
              * ((globalExposureEffect + skillExposureEffect)/100 + 1)
              * exposureEffectOnSelf)   -- exposureEffectOnSelf = enemyDB:More("ExposureEffectOnSelf")
magnitude = max(magnitude, value)       -- 取最强单一来源
...
enemyDB:NewMod(element.."Resist", "BASE", -magnitude, source)
```

- `ExposureMin`（override）可设最低曝光量（某些效果保底）。
- 结算后置 `Condition:Has<Element>Exposure` / `Condition:HasExposure` / `Condition:AppliedExposureRecently` 标志。
- **Boss**：与诅咒一样，`ExposureEffectOnSelf MORE -50`（唯一怪曝光效果 50% less）。

### 与穿透 / 负抗的区别

| 机制 | 作用对象 | 叠加 |
|---|---|---|
| **曝光 (Exposure)** | 降低敌方**抗性数值**（同元素多来源取最强一份） | 与诅咒/穿透分开，可共存 |
| **抗性穿透 (Penetration)** | 在伤害计算时**临时忽略**一部分抗性，**不改抗性数值** | 玩家侧词条，按伤害类型计 |
| **降低抗性诅咒** (Conductivity/Despair) | 降低敌方抗性数值（同曝光阶段，但不同来源标签） | 与曝光叠加 |

三者最终都让"有效抗性"更低；曝光与降抗诅咒**叠加进同一抗性 BASE**，穿透在伤害公式末端单独减。曝光与诅咒不互相挤占（曝光不占咒术上限）。

**PoB2 旗标**：`<Element>Exposure`、`<Element>ExposureChance`、`InflictExposure`、`<Element>ExposureEffect`、`ExposureEffectOnSelf`、`ExtraExposure`/`Extra<Element>Exposure`、`ExposureMin`、`Players cannot inflict Exposure`。

---

## 凋萎 (Wither)

凋萎是一种**可叠加层数、增加目标受到的混沌伤害**的减益。

- 每层 **+6% 受到的混沌伤害 (increased Chaos Damage taken)**，**最多 15 层**（游戏当前值，PoE2DB / PoE Wiki[^poe2db-wither][^poewiki-withered]）。
- 每层**独立计时**（基础 2 秒），施加新层**不刷新**旧层——因此实战层数受刷新频率限制。
- 由于每层是独立的 `INC` 修饰，`increased Effect of Withered` 只在约 16.67% 的倍数处跨越档位（向上取整）：0%→6%、17%→7%、34%→8%……每档 +1%。
- **Wither 技能**本身同时施加 **Hinder（移速降低）** 和 Withered 层；二者是不同减益。
- 凋萎也可施加在**玩家自己**身上（PoB2 `Condition:CanBeWithered`，`Multiplier:WitheredStackCountSelf`）。

> **PoB2 建模与游戏值的偏差（核对注意）**：PoB2 `ConfigOptions.lua` 当前把凋萎建模为 **5% increased Chaos Damage taken、上限 10 层**（`CalcPerform.lua`：`enemyDB:NewMod("ChaosDamageTaken", "INC", effect, "Withered", { type="Multiplier", var="WitheredStack", limit=10 })`，config tooltip 写 "5% ... up to 10 stacks"），与游戏的 6%/15 层存在差异。pobr 实现时以**游戏一手数据 (6%/15 层)** 为准，并记录 PoB2 偏差以便对账。

**PoB2 旗标**：`Multiplier:WitheredStack`（limit 10）、`Condition:CanWither` / `Condition:CanBeWithered`、`Multiplier:WitheredStackCount(Self)`、`ChaosDamageTaken`。

---

## 破甲 / 完全破甲 (Armour Break / Fully Broken Armour)

破甲是攻击机制，**临时移除目标固定数量的护甲**，默认基础持续 **12 秒**。机制与减伤公式细节见 [`armour.md`](armour.md)；这里只讲减益侧的 PoB2 建模。

- 护甲被破到 0 → **完全破甲 (Fully Broken Armour)**，对其物理击中失去护甲减免，并解锁"对完全破甲敌人"词条。
- 对怪物施加破甲有倍数：普通怪 ×3、魔法怪 ×2 护甲（见 armour.md）。
- **破甲到负数**：Warbringer 的 `Imploding Impacts`/`CanArmourBreakBelowZero` 允许把护甲破到负值（下限 = 原始护甲的相反数），负护甲给物理击中**伤害倍增**。
- **Sunder** 等"利用破甲"机制；某些武器"完全破甲时施加曝光"（数据中存在 `Inflicts Fire Exposure when this Weapon Fully Breaks Armour`）。

**PoB2 落点（`ConfigOptions.lua` / `CalcOffence.lua`）**：

```lua
-- 完全破甲：把敌方护甲 OVERRIDE 为 0
enemyModList:NewMod("Armour", "OVERRIDE", 0, "ArmourBreak",
  { type="Condition", var="ArmourFullyBroken" },
  { type="GlobalEffect", effectType="Debuff", effectName="ArmourBreak" },
  { type="ActorCondition", actor="enemy", var="CanArmourBreakBelowZero", neg=true })
-- 破到负数（Max）：OVERRIDE 为 -原始护甲
enemyDB:ReplaceMod("Armour", "OVERRIDE", -enemyDB:Sum("BASE", {source="Config"}, "Armour"), "ArmourBreak", ...)
```

**PoB2 旗标/条件**：`ArmourBreak`（GlobalEffect Debuff）、`Condition:CanArmourBreak`、`Condition:ArmourBroken`、`Condition:ArmourFullyBroken`、`Condition:ArmourBrokenBelowZeroMax`、`CanArmourBreakBelowZero`、`ArmourBreakPerHit`、`ArmourBreakEffect`（MORE）、`ArmourBreakDuration`、`multiplierArmourBreak`。

---

## 位移 / 控制类减益

这些减益多以 `Condition:<Name>` FLAG 写入敌方 modDB，并附带固定数值修饰。下表数值取自 PoB2 `CalcSetup.lua` / `ConfigOptions.lua`：

| 减益 | 精确效果 (PoB2) | PoB2 落点 |
|---|---|---|
| **Maim（残废）** | 敌方 **-30% increased 移动速度** + **-15% increased 闪避**；解锁"对 Maimed 敌人"词条（如对 Maimed +近战伤害） | `MovementSpeed INC -30` / `Evasion INC -15`（cond `Maimed`）；`Condition:Maimed` |
| **Blind（致盲）** | 敌方 **20% less 命中 (Accuracy)** + **20% less 闪避 (Evasion)**，并把视野压到最小；默认 4 秒 | `Blind` FLAG（cond `Blinded`）；`BlindEffect` OVERRIDE 可设效果 |
| **Hinder（妨碍）** | 降低目标移动速度（Wither 技能附带）；不与多来源相加，刷新持续 | `Condition:Hindered` |
| **Pin（钉刺）** | 目标无法移动且无法闪避 4 秒，钉刺瞬间附带 Light Stun；属 Poise 系（见 stun.md / ailments.md） | `Condition:Pinned` + `Immobilised` + `CannotEvade` + `StunnedRecently` |
| **Daze（眩目）** | 目标承受 **50% more 眩晕积累**，8 秒 | `Condition:Dazed`（详见 stun.md） |
| **Heavy Stun（重眩晕）/ Electrocute / Freeze** | Poise/控制类，**置 `Immobilised`** | 见 [`stun.md`](stun.md) / [`ailments.md`](ailments.md) |
| **Taunt（嘲讽）** | 强制目标攻击你 | `Condition:Taunted` |
| **Immobilised（被定身）** | 冻结/钉刺/重眩晕/电击等导致无法移动的统称条件 | `Condition:Immobilised` |

> Blind 在 PoE2 是 **less Accuracy + less Evasion**（不是直接改暴击）；它通过拉低敌方命中、拉低敌方闪避两条线影响攻防。配合 `Blindside` 辅助（对致盲敌人更易暴击、更高伤害，见 critical-hits.md）才间接联动暴击。

---

## 增伤 / 减输出型减益

| 减益 | 精确效果 (PoB2) | PoB2 落点 |
|---|---|---|
| **Intimidate（恫吓）** | 敌人 **+10% increased 受到伤害** 且 **-10% increased 自身伤害** | `DamageTaken INC 10` + `Damage INC -10`（cond `Intimidated`） |
| **Unnerve（胆寒）** | 敌人 **+10% increased 受到法术伤害**（仅 Spell） | `DamageTaken INC 10` (ModFlag.Spell, cond `Unnerved`) |
| **Crush（碾压）** | 敌人 **-15 物理伤害减免 (PhysicalDamageReduction BASE -15)** | cond `Crushed` |
| **Sap（削弱）** | 敌人造成伤害降低，至 20% less；可设效果值 | `SapVal BASE`（cond `Sapped`/`SappedConfig`） |
| **Debilitate（衰退）** | 敌人 **10% less 造成伤害** | `Damage MORE -10`（cond `Debilitated`） |
| **Scorch / Brittle**（异常状态，见 ailments.md） | Scorch 降元素抗（至 -30%）、Brittle 提升对其暴击率（至 +6%） | `ScorchVal` / `BrittleVal`（cond） |
| **Covered in Ash** | +20% increased 受到火焰伤害 + 20% less 移速 | `CoveredInAshEffect BASE 20` |
| **Covered in Frost** | +20% increased 受到冰霜伤害 + 50% less 暴击率 | `CoveredInFrostEffect BASE 20` |
| **暴击弱点 (Critical Weakness)** | 每层 +0.5% 基础暴击率，至 20 层 = +10%（详见 critical-hits.md） | `SelfCritChance BASE`（cond `ApplyCriticalWeakness`） |

> **"敌人受到的增伤"类 debuff 如何作用于计算**：它们都写在 **enemyDB** 上，作为 `DamageTaken`（可带元素/Spell 等 ModFlag）的 `INC`/`MORE` 或抗性 `BASE`。玩家击中伤害在 `CalcOffence` 末端乘上 `(1 + Σenemy DamageTaken INC/100) × Π(1 + MORE/100)`，抗性削减则进入元素有效抗性。Intimidate/Unnerve/Crush 等是"对所有玩家伤害"的乘区，曝光/降抗诅咒是"抗性"区。

---

## 0.5.0 / 近期版本变化备注

- **0.3.0**：诅咒对高稀有度怪物效果降低（魔法 15% / 稀有 30% / 唯一·Boss 50% less）。Despair 大幅加强为 -35~-49% 混沌抗（原 -20~-24%）；Enfeeble 改为统一 21~29% less damage（不再区分唯一/非唯一）[^poe2-030-patch]。
- **0.5.0**：未发现针对 curses/exposure/wither/armour-break 的专门数值改动（patch notes 主要改护甲数值缩放与重眩晕衰减，见 armour.md / stun.md）[^maxroll-050-patchnotes]。如后续 PoB2 数据更新，以一手数据校正本节。
- **PoB2 vs 游戏的 Wither 偏差**见上文凋萎节（5%/10 vs 6%/15）。

---

## 对 pobr 实现的启示

pobr 当前 `Env` 已是 player/enemy 双 `Actor` 结构，减益天然落在 **enemy `ModList` / `ModDb`**。对齐 PoB2 需要：

1. **统一"敌方受到伤害"乘区**：在 `calc/offence.rs` 计算玩家击中时，末端引入敌方 `DamageTaken`（`Inc`/`More`，可带 `ModFlag::Spell` / 元素 tag）。Intimidate(`+10 inc`)、Unnerve(`+10 inc spell`)、Debilitate(`-10 more`)、Covered in Ash/Frost、感电 都走这条。归一为一个 `ModName::DamageTaken` 家族 + tag。
2. **敌方抗性 BASE 削减**：曝光 / 降抗诅咒 (Despair/Conductivity) → 敌方 `<Element>Resist` 的 `Base` 负值。**曝光多来源取最强一份**（非求和），需要在聚合层为 Exposure 做"按来源 max"特例（不同于普通 `sum`），可单列一个 reducer 或在入口预归并。穿透保持玩家侧、伤害公式末端单独减，不混入抗性 BASE。
3. **诅咒上限与优先级**：建模 `EnemyCurseLimit`(默认1) + `EnemyMarkLimit`(默认1) 两个独立计数；超限时按优先级（来源/插槽）裁剪。初版可简化为"全部生效 + 配置开关"，但要预留 limit/priority 字段以对账。Hex/Mark 用枚举区分。
4. **`Condition:` 标志驱动玩家词条**：`Maimed`/`Blinded`/`Cursed`/`ArmourFullyBroken` 等作为敌方 `CalcConfig` 条件，让玩家侧 "against X enemies" 修饰通过 `matches(cfg)` 生效（pobr 已有 condition/flags 机制，扩充 condition 名集即可）。
5. **Boss 场景的 `MORE -50`**：实现 Boss 预设时，对 `CurseEffectOnSelf` / `ExposureEffectOnSelf` 注入 `More -50`，否则诅咒/曝光收益偏高 1 倍。
6. **破甲**：建模为对敌方 `Armour` 的 `Override`（0 = 完全破甲；负值 = 破到负数上限），与 `armour.md` 的减伤公式衔接；条件 `ArmourFullyBroken` / `CanArmourBreakBelowZero`。
7. **凋萎**：敌方 `ChaosDamageTaken Inc` × `Multiplier:WitheredStack`，**采用游戏值 6%/上限15**，并在测试 fixture 中标注与 PoB2(5%/10) 的差异以便回归对账。
8. **归因 (TraceGraph)**：每个减益来源（诅咒宝石/曝光技能/破甲攻击/Maim 词条）都应回溯到 `SourceId`——这正是 pobr 相对 PoB 的增量价值（"这 12% 增伤来自 Despair + 曝光各贡献多少"）。

**核对过的关键 PoB2 变量/旗标**：`EnemyCurseLimit`(BASE 1)、`EnemyMarkLimit`(BASE 1)、`CurseEffect`、`CurseEffectOnSelf`、`CurseEffectAgainstPlayers`、`MaxDoom`/`HexDoomStack`/`DoomEffect`、`Hexproof`/`CursesIgnoreHexproof`、`CursesIgnoreCurseLimit`、`<Element>Exposure`/`<Element>ExposureEffect`/`ExposureEffectOnSelf`/`ExposureMin`/`InflictExposure`、`Multiplier:WitheredStack`(limit 10)/`ChaosDamageTaken`、`ArmourBreak`/`Condition:ArmourFullyBroken`/`CanArmourBreakBelowZero`、`Condition:Maimed`+`MovementSpeed INC -30`+`Evasion INC -15`、`Blind`/`BlindEffect`、`Condition:Pinned`+`CannotEvade`+`Immobilised`、`Condition:Dazed`、`Condition:Intimidated`+`DamageTaken INC 10`+`Damage INC -10`、`Condition:Unnerved`+`DamageTaken INC 10`(Spell)、`PhysicalDamageReduction BASE -15`(Crushed)、`Damage MORE -10`(Debilitated)、`SelfCritChance`(Critical Weakness)。

---

## 参考来源

[^poe2wiki-curse]: PoE2 Wiki — Curse (gem tag). https://www.poe2wiki.net/wiki/Curse_(gem_tag)
[^poe2db-curses]: PoE2DB — Curses（Enfeeble/Despair 默认效果）. https://poe2db.tw/Curses
[^poe2db-conductivity]: PoE2DB — Conductivity. https://poe2db.tw/Conductivity
[^poe2db-despair]: PoE2DB — Despair. https://poe2db.tw/us/Despair
[^poe2-030-patch]: Path of Exile — Early Access Patch Notes 0.3.0 (The Third Edict)（诅咒对高稀有度怪物 less effect；Despair/Enfeeble 重做）. https://www.pathofexile.com/forum/view-thread/3826682
[^poe2db-wither]: PoE2DB — Wither / Withered（6% increased Chaos Damage taken, up to 15）. https://poe2db.tw/us/Wither
[^poewiki-withered]: PoE Wiki — Withered（独立计时、Effect 档位 16.67% 倍数）. https://www.poewiki.net/wiki/Withered
[^poe2wiki-blind]: PoE2 Wiki — Blind（20% less Accuracy & Evasion, 4s）. https://www.poe2wiki.net/wiki/Blind
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-configoptions]: PathOfBuilding-PoE2 — `src/Modules/ConfigOptions.lua`（敌方减益 config：Maim/Blind/Pin/Daze/Intimidate/Unnerve/Crush/Exposure/ArmourBreak/Wither/Curse 各项数值与旗标）. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/ConfigOptions.lua
[^pob2-calcperform]: PathOfBuilding-PoE2 — `src/Modules/CalcPerform.lua`（曝光取最强、诅咒上限/优先级、Doom、Withered 注入 enemyDB）. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcPerform.lua
[^pob2-calcsetup]: PathOfBuilding-PoE2 — `src/Modules/CalcSetup.lua`（Maim/Intimidate/Unnerve/Crush/Debilitate 基础敌方 mod、EnemyCurseLimit/EnemyMarkLimit 默认 1）. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcSetup.lua
