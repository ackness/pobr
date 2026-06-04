# 命中机制与敌人/怪物参数 (Accuracy / Hit Chance / Enemy & Monster Stats)

本文档收录 PoE2（0.5.0）**进攻命中侧**与**默认敌人/怪物参数**——它们共同构成 DPS 计算的关键输入：玩家攻击要先过命中检定，命中率又取决于敌人闪避；而"有效 DPS (effective DPS)"还要乘上敌人抗性/护甲/受到伤害修饰词。PoB2 用 **player / enemy 双 Actor + 双 ModDB** 建模这一切，敌人参数大量来自按**怪物等级**索引的缩放表。

> 本文与既有文档**互补、不重复**，交叉引用：
> - **命中-闪避公式的防御侧**（怪物击中玩家 `monsterHitChance`、熵值机制、闪避降级暴击）见 [evasion.md](./evasion.md)；本文只补**进攻侧**（玩家命中怪物 `calcs.hitChance`）。
> - **暴击的闪避降级**（`有效暴击几率 = 暴击几率 × 命中率`）见 [critical-hits.md](./critical-hits.md)；敌人爆伤默认 +30%（怪物 base_critical_hit_damage_bonus）见下 §五。
> - **异常/姿态阈值的怪物查表**（`monsterAilmentThresholdTable` / `monsterPoiseThresholdTable`）已在 [ailments.md](./ailments.md) 详述，本文只列 DPS 上下文需要的默认敌人阈值并指针引用。
> - 抗性/最大抗性见 [resistances.md](./resistances.md)；护甲减伤见 [armour.md](./armour.md)；敌人 debuff（致盲/致残/暴露/感电等）见 [debuffs.md](./debuffs.md)。
> - 末尾「PoB2 计算实现」给出核对过的真实变量/表名/常量名，是 pobr 的回归基准。

---

## 一、精准值 (Accuracy Rating)

精准值是**攻击 (Attack)** 命中所需的属性，对照目标闪避值决定命中率[^poe2wiki-acc]。**法术 (Spell) 不使用精准值**（见 §三）。

### 1.1 玩家精准来源

| 来源 | 数值（0.5.0） | 备注 |
|------|--------------|------|
| 每角色等级 | **+6 Accuracy Rating**（0.5.0 由 +3 提升）| `accuracy_rating_per_level = 6`[^poe2wiki-acc] |
| 每 1 点敏捷 (Dexterity) | **+6 Accuracy Rating**（0.5.0 由 +5 提升）| `AccuracyPerDexBase = 6`，见 [attributes.md](./attributes.md) |
| 装备/天赋词条 | `+N to Accuracy Rating`、`N% increased Accuracy Rating` | 局部（武器）与全局两类 |

> **0.5.0 命中改动**[^poe2wiki-acc]：① 每级 +6（旧 +3）、每点敏捷 +6（旧 +5）；② **距离衰减改为 2m→9m 线性插值**，9m 及更远固定 90% less（旧版无上限、14m 时 100% less）；③ **玩家召唤物攻击改为必定命中**（不再需要精准）；④ 闪避公式被下调以降低被闪避比例（防御侧见 [evasion.md](./evasion.md)）。

### 1.2 精准的聚合（base / inc / more）

PoB2 用标准属性管线，且区分**通用 Accuracy** 与**仅对敌 AccuracyVsEnemy**：

```
output.Accuracy  = floor( base      * (1 + inc/100)      * more )            -- 面板精准
accuracyVsEnemy  = floor( baseVsEnemy* (1 + incVsEnemy/100)* moreVsEnemy )   -- 命中检定用
accuracyVsEnemy *= accuracyPenalty                                           -- 再乘距离衰减（除非 NoAccuracyDistancePenalty）
```

### 1.3 距离衰减 (Accuracy Falloff)

攻击对目标的命中精准随距离衰减[^poe2wiki-acc]：

```
2m 内：无惩罚
2m → 9m：线性插值
≥9m：最大 90% less Accuracy
```

PoB2 常量：`AccuracyFalloffStart = 20`（=2m，10 单位/米）、`AccuracyFalloffEnd = 90`（=9m）、`MaxAccuracyRangePenalty = 90`（来自 `accuracy_rating_+%_final_at_max_distance_scaled = -90`）。配置项 `enemyDistance`（默认 2m，即 placeholder 20 单位）控制此衰减；`NoAccuracyDistancePenalty` 旗标可免除。

---

## 二、命中率公式（进攻侧 `calcs.hitChance`）

玩家**攻击**命中怪物的几率（核对 `CalcDefence.lua::calcs.hitChance`，与 wiki/Mobalytics 一致）[^poe2wiki-acc][^mobalytics-acc]：

```
未截断命中率 = AA * 1.25 * 100 / (AA + DE * 0.3)
```

- **AA** = 攻击者对敌精准值 (accuracyVsEnemy，已含距离衰减)
- **DE** = 防御者（怪物）闪避值

### 2.1 上下限

- **下限 5%**：命中率不能低于 5%（对应闪避上限 95%）；`accuracy < 0` 时直接返回 5。
- **上限 100%**（默认 clamp）。**但公式本身上限是 125%**（DE=0 时 `1.25*100`）——只有带 `Condition:HitChanceCanExceed100` 旗标（如 Amazon `Critical Strike` notable）时，PoB2 才保留 >100% 的"超额命中"（`AccuracyHitChanceUncapped`），把多出的部分写入 `Multiplier:ExcessHitChance` 供其它机制消费。

> **与防御侧公式不同（重要）**：这是**玩家打怪**用的式子；**怪物打玩家**用的是另一支 `calcs.monsterHitChance = (1 - 0.95*evasion/(evasion + 4*accuracy)) * 100`（见 [evasion.md](./evasion.md)）。两者**不是简单互为补数**，pobr 实现时务必分清攻防两侧各自的公式。

### 2.2 绕过命中检定

- **`CannotBeEvaded`（"Your Hits can't be Evaded"）/ 技能 `cannotBeEvaded`**：命中率直接置 100（如 `Killing Palm` 的 Culling 段）。
- **敌方 `CannotEvade`**（仅 `mode_effective` 下）：同样置 100。

### 2.3 命中率在 DPS 链上的位置

```
output.HitChance = output.AccuracyHitChance * (1 - enemyBlockChance/100)
```

随后 `HitChance/100` 作为乘子进入平均伤害、并参与暴击的闪避降级（`有效暴击几率 = 暴击几率 × AccuracyHitChance/100`，见 [critical-hits.md](./critical-hits.md)）。敌人格挡 (`enemyBlockChance`) 也在此扣除（格挡机制见 [block.md](./block.md)）。

---

## 三、法术必中、近战/远程差异

- **法术 (Spell) 必中**：`if not isAttack then output.AccuracyHitChance = 100`。法术不做精准检定、不被闪避（怪物的闪避对法术无效）[^poe2wiki-acc][^mobalytics-acc]。因此**法术构建无需堆精准**。
- **攻击 (Attack)**：近战与远程**同一套命中公式**；差异只在**距离衰减**——远程更容易处在 >2m 触发衰减，近战通常贴脸（2m 内无惩罚）。投射物被闪避表现为"未碰撞"。
- **召唤物 (Minion)**：0.5.0 起攻击必定命中（`global_always_hit = 1`，`playerMinionIntrinsicStats`）。

---

## 四、敌人/怪物默认参数（DPS 计算的默认上下文）

PoE2 怪物的**生命/精准/闪避/护甲/伤害**等随**怪物等级**缩放，PoB2 用按等级索引的查表（`Data/Misc.lua`，源自 `DefaultMonsterStats.dat`）。`enemyLevel` 默认 = `min(MaxEnemyLevel=85, 角色等级)`，可在配置里覆盖。

### 4.1 怪物随等级缩放表（节选，索引 = 怪物等级）

| 等级 | 生命 monsterLife | 精准 monsterAccuracy | 闪避 monsterEvasion | 护甲 monsterArmour | 基础伤害 monsterDamage |
|------|------------------|----------------------|---------------------|--------------------|------------------------|
| 1 | 15 | 32 | 24 | 3 | 9.16 |
| 20 | 249 | 140 | 160 | 89 | 37.29 |
| 65 | 8001 | 1158 | 677 | 6718 | 333.7 |
| 82 | 31065 | 2192 | 996 | 5081 | 484 |
| 83 | 31997 | 2273 | 1015 | 5375 | 495.9 |
| 85 (上限) | 33945 | 2444 | 1053 | 6011 | 523.9 |

完整 99 级表见 vendor `src/Data/Misc.lua`（`data.monsterLifeTable` / `monsterAccuracyTable` / `monsterEvasionTable` / `monsterArmourTable` / `monsterDamageTable`，另有盟友版 `monsterAllyLifeTable` / `monsterAllyDamageTable`）。

> **注意**：`monsterLifeTable` 在 lv65（=`EndgameStartLevel`）附近及之后有几处**巨幅跳升**（lv69 起 18272…），用于终局/pinnacle 区间；`monsterArmourTable` 在 lv82 处相对 lv65 反而回落（5081 vs 6718），因为终局区间另起缩放段。实现时直接照搬整张表，勿用线性外推。

### 4.2 怪物固定常量（`monsterConstants` / `gameConstants`）

| 常量 | 值 | 含义 |
|------|----|------|
| `base_critical_hit_damage_bonus` | **30** | 怪物基础爆伤 +30%（玩家/召唤物分别 +100% / +70%），见 [critical-hits.md](./critical-hits.md) |
| `maximum_physical_damage_reduction_%` | 75 | 怪物物理减伤上限（`EnemyPhysicalDamageReductionCap`）|
| `base_maximum_all_resistances_%` | 75 | 怪物最大抗性（`EnemyMaxResist`）|
| `max_endurance/frenzy/power_charges` | 3 | 怪物充能上限 |
| `energy_shield_recharge_rate_per_minute_%` | 750 | 怪物 ES 充能率 |
| `MonsterAccuracyBase/Incremental` | 28 / 280 | 怪物精准公式常量（与查表互参）|
| `MonsterEvasionBase/Incremental` | 18 / 6 | 怪物闪避公式常量 |
| `MonsterArmourBase/Incremental` | 2 / 1.75 | 怪物护甲公式常量 |

### 4.3 怪物种类/图腾生命倍率

`data.monsterVarietyLifeMult`（按怪物名，如 `Rotting Hulk = 2.5`、`Vile Imp = 0.65`、`Dread Servant = 1.5`）与 `totemLifeMult`、`monsterVarietyLifeMult` 用于把基准生命按种类微调。地图等级倍率 `mapLevelLifeMult` / `mapLevelBossLifeMult` / `mapLevelBossAilmentMult` 在 0.5.0 当前**全为 1**（占位）。

---

## 五、Boss / 稀有怪：额外加成与惩罚

配置项 **`enemyIsBoss`** 有四档（`ConfigOptions.lua`，**默认 `defaultIndex = 3` 即 "Pinnacle"**——PoB2 默认对 Guardian/Pinnacle Boss 算 DPS）：

| 档位 | 元素抗性默认 | 混沌抗性 | 护甲/闪避倍率 | 穿透 (Pen) | 受到伤害 | 伤害缩放 (DPS mult) |
|------|------------|---------|--------------|-----------|---------|---------------------|
| **None**（普通）| 空（0）| 空 | ×100% | 无 | — | `normalEnemyDPSMult = 1/4.40` |
| **Boss**（标准）| **+30%** | 0 | ×100% | 无 | — | `stdBossDPSMult = 4/4.40` |
| **Pinnacle**（守护者/巅峰）| **+50%** | 0 | `PinnacleArmourMean` / `PinnacleEvasionMean`（按 Bosses.lua 均值）| `pinnacleBossPen = 15/5 = 3` | — | `pinnacleBossDPSMult = 8/4.40` |
| **Uber**（究极巅峰）| **+50%** | 0 | `UberArmourMean` / `UberEvasionMean` | `uberBossPen = 40/5 = 8` | **`DamageTaken MORE -70`** | `uberBossDPSMult = 10/4.25` |

补充规则：

- **等级**：Boss/普通默认 = 角色等级（cap 85）；**Pinnacle/Uber 默认且最低 82**（`m_max(配置, 82)`）。
- **Boss 通用 debuff 抗性**（Boss/Pinnacle/Uber 共有）：`CurseEffectOnSelf MORE -50`、`ExposureEffectOnSelf MORE -50`、`SlowEffectOnSelf MORE -50`、`KnockbackDistanceOnSelf MORE -75`、`MinimumMovementSpeed 20`、`PoiseThreshold MORE +500`（外加 Map Boss +213 / Xesht +838）；并标记 `Condition:Unique` / `Condition:RareOrUnique`（Pinnacle/Uber 另加 `Condition:PinnacleBoss`）。这些直接削弱诅咒/暴露/减速类 debuff 对 Boss 的有效度。
- **Boss 默认护甲/闪避**：取 `monsterArmourTable[lv] × ArmourMean%` / `monsterEvasionTable[lv] × EvasionMean%`。均值来自 `Data/Bosses.lua` 各 boss 的 `armourMult`/`evasionMult`（注：当前 Bosses.lua 仍是 **PoE1 遗留 boss 列表**：Shaper/Sirus/Maven 等，PoB2 尚未替换为 PoE2 boss，实现时需留意此为占位数据）。
- **稀有怪 (Rare)**：配置项 `conditionEnemyRareOrUnique` → `Condition:RareOrUnique`（用于斩杀阈值 `CanCull` 等），不自带抗性加成。
- **Boss Power**：`WarcryPower 20` + `Multiplier:EnemyPower 20`（战吼按敌人强度授予，Boss 视为 20 Power）。

### 5.1 默认怪物输出伤害（用于 EHP，非玩家 DPS）

`enemyXDamage` 占位 = `monsterDamageTable[lv] × 1.5 × DPSMult`（每种伤害类型），混沌伤害 = `/2.5`（Uber `/4`）。`enemySpeed` 默认 700ms，`enemyCritChance` 5%，`enemyCritDamage` = 30（怪物 base bonus）。这些只用于**承受/EHP**计算，不影响玩家进攻 DPS。

---

## 六、敌人配置项 (Configuration → Enemy) 一览

PoB2 把敌人状态/debuff 写入 **`env.enemyDB`**（独立 ModDB），常用项（`ConfigOptions.lua`，影响有效 DPS）：

**抗性/护甲/格挡（直接进伤害减免）**
- `enemyFireResist` / `enemyColdResist` / `enemyLightningResist` / `enemyChaosResist` → `*Resist BASE`
- `enemyPhysicalReduction` → `PhysicalDamageReduction BASE`；`enemyArmour` → `Armour BASE`；`enemyEvasion` → `Evasion BASE`（影响你的命中率）
- `enemyBlockChance` → `BlockChance BASE`（你的 `HitChance` 会乘 `1-block/100`）
- `enemyMaxResist`（勾选 = `DoNotChangeMaxResFromConfig`，锁 75）

**降抗/暴露/易伤（增有效 DPS）**
- `conditionEnemyFire/Cold/LightningExposure` → -20% 对应抗性
- `conditionEnemyScorched`（最多 -30% 元素抗，遗留）、`conditionEnemyOnFungalGround`（-10% 全抗）
- `conditionEnemyCoveredInAsh`（+20% 火伤承受）/ `CoveredInFrost`（+20% 冷伤承受，-50% 暴击率，防御向）

**受到伤害增益型 debuff**
- `conditionEnemyShocked`（感电，默认 +20% 受伤，可配 `conditionShockEffect` / `ShockStacks`）
- `conditionEnemyIntimidated`（+10% 受伤 & -10% 输出）、`conditionEnemyUnnerved`（+10% 法术承受）、`conditionEnemyCrushed`（-15% 物理减伤）、`conditionEnemyDazed`（+50% 眩晕积累）
- `deliriousPercentage`（Delirium：100% 时敌人 `DamageTaken MORE -80` + `Damage INC 30`，是**降 DPS** 项）

**影响命中/暴击的 debuff（与本文主题直接相关）**
- `conditionEnemyBlinded`（致盲：敌人 -20% less Accuracy 且 **-20% less Evasion** → 提升你的命中率；`overrideBuffBlinded` 可调效果）
- `conditionEnemyMaimed`（致残：敌人 **-15% Evasion** + -30% 移速）
- `conditionEnemyPinned`（钉刺：4 秒内 `CannotEvade` → 你必中）
- `conditionEnemyCriticalWeakness`（暴击弱点：每层 +0.5% 你的暴击率，默认 20 层 = +10%，见 [critical-hits.md](./critical-hits.md)）/ `conditionEnemyBrittle`（脆弱，遗留，最多 +6% 暴击率）

**其它**：`enemyLevel`（覆盖默认等级，影响命中/闪避/护甲减伤/异常阈值）、`enemyDistance`（命中距离衰减）、各类 `conditionEnemy*`（Bleeding/Poisoned/Frozen…，用于触发"against X enemies"词条）。

---

## 七、有效 DPS (effective DPS) vs 面板 DPS（口径差异，`mode_effective`）

PoB2 的 `buffMode` 决定 `env.mode_effective`：

| buffMode | mode_buffs | mode_combat | mode_effective | 含义 |
|----------|-----------|-------------|----------------|------|
| `EFFECTIVE`（默认）| ✔ | ✔ | **✔** | **有效 DPS**：计入命中率、敌人抗性/护甲/受伤 debuff、暴击闪避降级、对敌条件词条 |
| `COMBAT` | ✔ | ✔ | ✘ | 战斗中但不计有效命中/敌人交互 |
| `BUFFED` | ✔ | ✘ | ✘ | 仅自身增益 |
| `UNBUFFED` | ✘ | ✘ | ✘ | 裸面板 |

`mode_effective` 为真时，命中率 (`AccuracyHitChance`)、`有效暴击几率 = 暴击几率 × 命中率`（见 [critical-hits.md](./critical-hits.md)）、敌人 `CannotEvade`、敌人抗性/护甲减伤、`reduceEnemyBlock` 等才生效。因此：

- **面板 DPS（非 effective）**：`命中率视为 100%`、不扣敌人抗性/护甲——是"理论满命中、对零防御假想敌"的上界。
- **有效 DPS（effective）**：乘 `命中率`、乘 `(1 - 敌人抗性/护甲减伤/block)`、乘敌人 `DamageTaken` 链——是面对**实际配置敌人**（默认 Pinnacle Boss）的更真实期望。
- `mode_effective` 还驱动 `Condition:Effective` 旗标（Boss 的 `Condition:Unique` 等多以 `var = "Effective"` 注入），故许多"against Unique/Boss enemies"词条只在 effective 模式生效。

---

## PoB2 计算实现（核对基准）

取自 [PathOfBuilding-PoE2 `dev`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的 `src/Modules/CalcSetup.lua`、`CalcOffence.lua`、`CalcDefence.lua`、`ConfigOptions.lua`、`src/Modules/Data.lua`、`src/Data/Misc.lua`、`src/Data/Bosses.lua`，是 pobr 的回归基准：

**双 Actor / 双 ModDB 与敌人初始化（`CalcSetup.lua`）**
```lua
env.enemyLevel = build.configTab.enemyLevel or m_min(data.misc.MaxEnemyLevel, build.characterLevel)  -- MaxEnemyLevel = 85
env.player = { modDB = env.modDB, level = build.characterLevel }
env.enemy  = { modDB = env.enemyDB, level = env.enemyLevel }
env.player.enemy = env.enemy; env.enemy.enemy = env.player   -- 互指
-- 敌人只在 setup 注入 Accuracy（其余 Evasion/Armour/Resist 走 config 占位/mod）：
enemyDB:NewMod("Accuracy", "BASE", data.monsterAccuracyTable[env.enemyLevel], "Base")
-- buffMode → mode_effective（见 §七）
```

**精准 / 命中率（`CalcOffence.lua` + `CalcDefence.lua`）**
```lua
output.Accuracy = floor(base * (1+inc/100) * more)
accuracyVsEnemy = floor(baseVsEnemy * (1+incVsEnemy/100) * moreVsEnemy) * accuracyPenalty  -- 距离衰减
if not isAttack then output.AccuracyHitChance = 100                                          -- 法术必中
else
  enemyEvasion = max(round(val(enemyDB,"Evasion")), 0)
  cannotBeEvaded = Flag(cfg,"CannotBeEvaded") or skillData.cannotBeEvaded or (mode_effective and enemyDB:Flag("CannotEvade"))
  output.AccuracyHitChance = cannotBeEvaded and 100 or calcs.hitChance(enemyEvasion, accuracyVsEnemy) * mod(HitChance)
end
output.HitChance = output.AccuracyHitChance * (1 - output.enemyBlockChance/100)
-- calcs.hitChance(evasion, accuracy):  rawChance = accuracy*1.25 / (accuracy + evasion*0.3) * 100; clamp [5,100]（uncapped 时仅 max 5）
-- 距离衰减常量: AccuracyFalloffStart=20(2m), AccuracyFalloffEnd=90(9m), MaxAccuracyRangePenalty=90
```

**敌人 Boss 档位（`ConfigOptions.lua` enemyIsBoss，defaultIndex=3=Pinnacle）**
```lua
-- 通用(Boss/Pinnacle/Uber): CurseEffectOnSelf MORE -50, ExposureEffectOnSelf MORE -50, SlowEffectOnSelf MORE -50,
--   KnockbackDistanceOnSelf MORE -75, MinimumMovementSpeed 20, PoiseThreshold MORE +500, WarcryPower/Multiplier:EnemyPower 20
-- Boss:     +30% ele resist; armour/evasion ×100%;            damage = monsterDamageTable[lv]*1.5*stdBossDPSMult
-- Pinnacle: +50% ele resist; armour ×PinnacleArmourMean%, evasion ×PinnacleEvasionMean%; pen=pinnacleBossPen(3); lv≥82
-- Uber:     +50% ele resist; armour ×UberArmourMean%, evasion ×UberEvasionMean%; pen=uberBossPen(8);
--           enemyModList:NewMod("DamageTaken","MORE",-70,"Boss"); lv≥82
```

**EHP / 伤害缩放常量（`Data.lua` data.misc）**
```lua
MaxEnemyLevel = 85; EnemyMaxResist = 75; EnemyPhysicalDamageReductionCap = 75; ResistFloor = -200; MaxResistCap = 90
normalEnemyDPSMult = 1/4.40; stdBossDPSMult = 4/4.40; pinnacleBossDPSMult = 8/4.40; uberBossDPSMult = 10/4.25
pinnacleBossPen = 15/5; uberBossPen = 40/5
AccuracyPerDexBase = 6
-- bossStats = { PinnacleArmourMean, PinnacleEvasionMean, UberArmourMean, UberEvasionMean } 由 Data/Bosses.lua 均值算出
```

**怪物缩放表（`Data/Misc.lua`，源自 DefaultMonsterStats.dat / GameConstants.dat / Monster.ot）**
```
data.monsterLifeTable / monsterAccuracyTable / monsterEvasionTable / monsterArmourTable / monsterDamageTable  (各 99 项, 索引=等级)
data.monsterAllyLifeTable / monsterAllyDamageTable / monsterVarietyLifeMult / totemLifeMult
data.monsterAilmentThresholdTable / monsterPoiseThresholdTable  (见 ailments.md)
gameConstants: MonsterAccuracyBase=28, MonsterAccuracyIncremental=280, MonsterEvasionBase=18, MonsterArmourBase=2, EndgameStartLevel=65
monsterConstants: base_critical_hit_damage_bonus=30, maximum_physical_damage_reduction_%=75, base_maximum_all_resistances_%=75, max_*_charges=3
```

**关键稳定标识 / 旗标**：`Accuracy`、`AccuracyVsEnemy`、`AccuracyHitChance`、`AccuracyHitChanceUncapped`、`HitChance`、`CannotBeEvaded`、`CannotEvade`、`NoAccuracyDistancePenalty`、`Condition:HitChanceCanExceed100`、`Multiplier:ExcessHitChance`、`AccuracyPenalty`、`Multiplier:enemyDistance`；敌方：`Evasion`、`Armour`、`*Resist`、`PhysicalDamageReduction`、`BlockChance`、`reduceEnemyBlock`、`DamageTaken`、`Condition:Unique`/`RareOrUnique`/`PinnacleBoss`、`CurseEffectOnSelf`/`ExposureEffectOnSelf`/`SlowEffectOnSelf`、`PoiseThreshold`、`Multiplier:EnemyPower`、`Condition:Blinded`/`Maimed`/`Shocked`/`Intimidated`、`BlindEffect`、`DoNotChangeMaxResFromConfig`。

---

## 对 pobr 实现的启示

对照 `pobr-core`（`calc/env.rs::Env`、`calc/offence.rs`、`config.rs::CalcConfig`、`mod_db.rs`、`trace.rs`）与数据层（`pobr-data` / `pobr-gamedata`）落地建议：

1. **`Env` 必须是 player / enemy 双 Actor + 双 ModDB（互指）。**
   - 当前 `env.rs::Env` 持有 player/enemy `Actor`，应让 enemy 侧拥有独立 ModDB（精准/闪避/护甲/抗性/受到伤害 debuff），且 `player.enemy ↔ enemy.player` 互引，供"对敌"聚合与敌方 `Self*` 词条求值。命中率求值在**只读快照阶段**展开，符合 pobr "calc 纯函数 + 确定性"约定。

2. **命中率攻防两侧分开实现，勿混用 evasion.md 的防御公式。**
   - 进攻侧 `hit_chance(accuracy, enemy_evasion) = clamp(accuracy*1.25*100/(accuracy + enemy_evasion*0.3), 5, 100)`；防御侧用 `monsterHitChance`（见 [evasion.md](./evasion.md)）。提供 `CannotBeEvaded`/`CannotEvade` 短路置 100，以及 `HitChanceCanExceed100`（保留 uncapped 与 `ExcessHitChance`）。
   - **法术分支**：`is_attack == false → hit_chance = 100`（不进精准管线）。把"是否攻击"作为技能/`CalcConfig` 的一等标志。

3. **精准 base/inc/more 复用现有标准管线，并区分 `AccuracyVsEnemy` 与距离衰减。**
   - 基础精准从角色等级（+6/级）、敏捷（+6/点）派生（见 [attributes.md](./attributes.md)），不得用全 0 默认；距离衰减作为可配置 `enemy_distance` 的乘区（2m→9m，max 90% less），可被 `NoAccuracyDistancePenalty` 关闭。

4. **敌人默认参数表纳入版本化数据，按 enemyLevel 索引。**
   - 把 `monsterLifeTable`/`monsterAccuracyTable`/`monsterEvasionTable`/`monsterArmourTable`/`monsterDamageTable`（及异常/姿态阈值表，见 [ailments.md](./ailments.md)）作为 `pobr-data` catalog（或 gameConstants 表），由 `pobr-gamedata` loader 读入 `data/<poe_version>/`。**整表照搬**（含 lv65/82 的非线性跳变），勿外推。`enemyLevel` 默认 `min(85, charLevel)`。

5. **Boss 档位用"旗标 → 注入若干 enemy Modifier"建模，对齐 PoB2 四档。**
   - `EnemyTier::{None, Boss, Pinnacle, Uber}`：分别注入元素抗性 +30/+50、护甲/闪避均值倍率、穿透、Uber 的 `DamageTaken MORE -70`、以及通用 `CurseEffectOnSelf/-50` 等 debuff 抗性与 `PoiseThreshold +500`。**默认档位选 Pinnacle**（与 PoB2 `defaultIndex=3` 一致），lv 默认/最低 82。注意 `Data/Bosses.lua` 当前是 PoE1 遗留均值，属占位，待 PoE2 数据替换。

6. **`mode_effective` 是 DPS 口径开关，必须建模为 `CalcConfig` 标志。**
   - effective=true 时：DPS 乘命中率、乘敌人抗性/护甲减伤/block、启用暴击闪避降级、启用 `Condition:Effective`（Boss 的 Unique/PinnacleBoss 等条件词条只在此生效）；effective=false 给"面板/裸 DPS"上界。`session.rs::CalculationSession` 应暴露该模式，`perform_minimal()` 默认走 effective 以贴近实战。

7. **归因 (TraceGraph) 增量**：命中率（精准来源、距离衰减、敌人闪避/致盲/致残）、敌人每一档 Boss 加成与每个 debuff（暴露/感电/Intimidate/Delirium）都应能回溯到 `SourceId`——把"有效 DPS 相对面板 DPS 的每一步折扣/增益"显式归因，正是 pobr 相对 PoB 的核心价值。

---

## 参考来源

[^poe2wiki-acc]: PoE2 Wiki — Accuracy / Accuracy Rating（命中公式 `AA*1.25*100/(AA+DE*0.3)`、上限 125%/下限 5%、法术不需精准、距离衰减 2m→9m、0.5.0 版本历史：+6/级、+6/敏捷、召唤物必中、闪避公式下调）。https://www.poe2wiki.net/wiki/Accuracy_Rating
[^mobalytics-acc]: Mobalytics — PoE 2 Guide: Accuracy Explained（玩家攻击命中公式、法术 Hit 不做检定、怪物伤害不分攻击/法术、按目标计算）。https://mobalytics.gg/poe-2/guides/accuracy
[^pob2-calcsetup]: PathOfBuilding-PoE2 — `src/Modules/CalcSetup.lua`（player/enemy 双 Actor、`enemyLevel`、`monsterAccuracyTable` 注入、`mode_effective`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcSetup.lua
[^pob2-calcoffence]: PathOfBuilding-PoE2 — `src/Modules/CalcOffence.lua`（Accuracy/AccuracyVsEnemy、距离衰减、AccuracyHitChance、法术必中、enemyBlockChance、HitChance）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcOffence.lua
[^pob2-calcdefence]: PathOfBuilding-PoE2 — `src/Modules/CalcDefence.lua`（`calcs.hitChance` 进攻公式 / `calcs.monsterHitChance` 防御公式 / `calcs.deflectChance`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcDefence.lua
[^pob2-config]: PathOfBuilding-PoE2 — `src/Modules/ConfigOptions.lua`（`enemyLevel`/`enemyIsBoss`(defaultIndex=3) 四档与默认抗性/护甲/闪避/穿透/伤害、`enemy*Resist`/`enemyArmour`/`enemyEvasion`/`enemyBlockChance`、致盲/致残/感电/Intimidate/Delirium 等敌人 debuff、`enemyDistance`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/ConfigOptions.lua
[^pob2-data]: PathOfBuilding-PoE2 — `src/Modules/Data.lua`（`data.misc`：MaxEnemyLevel=85、EnemyMaxResist=75、normal/std/pinnacle/uber DPSMult 与 Pen、AccuracyFalloff 常量、`bossStats`/`enemyIsBossTooltip`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/Data.lua
[^pob2-misc]: PathOfBuilding-PoE2 — `src/Data/Misc.lua`（`monsterLife/Accuracy/Evasion/Armour/DamageTable`、`monsterAilment/PoiseThresholdTable`、`gameConstants`/`monsterConstants`：base_critical_hit_damage_bonus=30、MonsterAccuracyBase=28、EndgameStartLevel=65）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Misc.lua
[^pob2-bosses]: PathOfBuilding-PoE2 — `src/Data/Bosses.lua`（boss `armourMult`/`evasionMult`/`isUber`，当前为 PoE1 遗留 boss 列表，用于算 PinnacleArmourMean/UberEvasionMean 等均值）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Bosses.lua
