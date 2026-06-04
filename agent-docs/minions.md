# 召唤物 (Minions)

召唤物是玩家通过技能 / 效果 / 增益召出或复活的**盟友 (Ally)**，拥有**自己独立的一套属性**（生命、伤害、抗性、暴击、命中、防御…），按怪物式 (monster-style) 公式从**等级**派生基础值，再被一部分**专门指向召唤物的玩家修饰词**放大[^poe2wiki-minion]。

> **核心心智模型（对 pobr 最关键的一条）**：召唤物在 PoB2 里是一个**独立的 Actor**（`env.minion`，持有自己的 `modDB` / `output`），它的伤害/防御走的是**与玩家完全相同的 CalcOffence/CalcDefence 管线**——只是把 Actor 换成召唤物、`modDB` 换成召唤物自己的库。玩家身上的普通词条**默认不传递**给召唤物；只有形如「Minions deal/have …」「光环/盟友 buff」「`MinionModifier`」的东西才会被显式注入召唤物的 `modDB`。

> 本文与既有文档**互补、不重复**：宝石类型/精神宝石/品质见 [gems.md](./gems.md)；伤害 added/inc/more 叠加、转换、Gain as Extra、双三倍见 [damage-scaling.md](./damage-scaling.md)；暴击几率/爆伤链见 [critical-hits.md](./critical-hits.md)；充能默认值、Spirit 保留、Presence、增益效果 (BuffEffect) 见 [recovery-charges-buffs.md](./recovery-charges-buffs.md)。本文只补「召唤物作为独立 Actor 的基础属性来源 + player→minion 传递规则 + 召唤物专属机制」。末尾「## 对 pobr 实现的启示」给落地建议，「## 参考来源」给脚注。

---

## 一、召唤物的基础属性来源（怪物式 scaling）

召唤物**不**从玩家继承基础属性。它的基础值来自**两层乘法**：`等级对应的怪物基准表` × `该召唤物类型的归一化乘数`（`Minions.lua` 里每个条目的 `life` / `damage` / `armour` / `evasion` / `critChance` 等字段）[^pob2-minions][^pob2-calcperform]。

### 1.1 等级 (level)

召唤物等级由召唤技能决定（`CalcActiveSkill.lua`，按优先级）：
- `minionLevelIsEnemyLevel` → 用区域/敌人等级（**Spectre 幽魂**走这条：召唤时锁定为当时区域等级，之后不随玩家变化）；
- `minionLevelIsTriggeredSkillLevel` → 用触发技能等级查 `data.minionLevelTable`；
- `minionLevelIsPlayerLevel` → 取 `min(角色等级, 上限)`；
- 否则 `skillData.minionLevel` 或 `data.minionLevelTable[宝石等级]`。
- 最终 `clamp(level, 1, 100)`。

`data.minionLevelTable`（`Misc.lua`，来自 `MinionGemLevelScaling.dat`）= `{2,4,6,…,80}`——即宝石等级 1→怪物等级 2，宝石等级 40→怪物等级 80。

### 1.2 生命 (Life)

```
baseLife = lifeTable[level] × minionData.life          -- 再 m_floor
（敌对召唤物额外 × mapLevelLifeMult[enemyLevel]）
```

- 非敌对（玩家盟友）用 `data.monsterAllyLifeTable`；敌对召唤物（如某些转化 / spectre 标记 hostile）用 `data.monsterLifeTable`[^pob2-calcactiveskill]。
- `minionData.life` 是归一化系数：Raised Zombie `0.7`、Raging Spirit `0.25`、骷髅战士约 `0.7`…（`Minions.lua`）。
- 部分召唤物有 `energyShield = 0.15`（如骷髅法师），对应 `LifeConvertToEnergyShield BASE = energyShield×100`，即把一部分生命转成 ES[^pob2-minions]。

### 1.3 伤害 (Damage) — 经由武器数据 (weaponData)

召唤物的攻击伤害**不是直接的伤害词条**，而是合成一把**虚拟武器** `weaponData1`，再让召唤物的攻击技能像玩家一样从武器读基础伤害（`CalcActiveSkill.lua`）：

```
damage = damageTable[level]
if not baseDamageIgnoresAttackSpeed then damage = damage × attackTime end
weaponData1 = {
  AttackRate  = 1 / attackTime,
  CritChance  = minionData.critChance,                       -- 默认 5
  PhysicalMin = round(damage × (1 − minionData.damageSpread)),
  PhysicalMax = round(damage × (1 + minionData.damageSpread)),
  range       = minionData.attackRange,
}
```

- 伤害基准表：玩家盟友用 `monsterAllyDamageTable`，敌对/幽魂用 `monsterDamageTable`（`Misc.lua`）。
- `damageSpread`（如 Zombie `0.3`、Raging Spirit `0.2`）决定 min/max 区间宽度。
- `baseDamageIgnoresAttackSpeed = true` 的召唤物（Zombie/RagingSpirit 等）基础伤害**不乘攻速**——即攻速只影响 DPS、不影响每击伤害。
- 法术型召唤物 / 用宝石定义伤害的召唤物走 `minionData.skillList` 里的技能基础伤害，机制同玩家法术。
- **隐藏等级缩放修正** `hiddenDamageFixup`：Spectre/Companion 这类用「ally 表」与「monster 表」基准不同的召唤物，会补一条 `Damage MORE = hiddenDamageFixup×100`，以及 `damageFixup`（`Damage MORE` / `Speed MORE`）抵消基准差异[^pob2-calcactiveskill]。

### 1.4 防御 (Armour / Evasion / Resists)

```
Armour   BASE = round(monsterArmourTable[level]  × (minionData.armour  or 1))
Evasion  BASE = round(monsterEvasionTable[level] × (minionData.evasion or 1))
FireResist/ColdResist/LightningResist/ChaosResist BASE = minionData.<x>Resist   -- 默认多为 0
```

护甲/闪避减伤公式与玩家相同（见 [armour.md] / `CalcDefence`），只是 Actor 换成召唤物。

### 1.5 暴击 / 命中 / 爆伤 — 召唤物的「内禀属性」

PoB2 `Misc.lua::playerMinionIntrinsicStats`（来自 `PlayerMinionIntrinsicStats.dat`）给出召唤物的硬编码内禀值：

| 字段 | 值 | 含义 |
|------|----|------|
| `global_always_hit` | 1 | **召唤物默认必中**（0.3.0 起召唤物不再需要命中检定） |
| `base_critical_hit_damage_bonus` | 70 | 召唤物**额外**爆伤 +70%（叠加在 monster 基础上） |
| `stun_base_duration_override_ms` | 500 | 眩晕基础时长覆盖 |
| `attack_damage_final_permyriad_per_rage` | 100 | 每点怒气 1% 攻击伤害（0.5.0 起 Rage 可辅助召唤物技能） |

CalcPerform 实际结算（`CalcPerform.lua`，召唤物初始化段）：

```lua
-- 命中：0.3.0 起召唤物默认必中（除非 MinionAccuracyEqualsAccuracy 旗标令其用玩家命中）
env.minion.modDB:NewMod("CannotBeEvaded", "FLAG", 1, "Minion Attacks always hit")
-- 暴击几率：来自虚拟武器 critChance（默认 5%，见 1.3）
-- 爆伤：monster 基础 + 召唤物内禀 +70%
env.minion.modDB:NewMod("CritMultiplier", "BASE",
    monsterConstants["base_critical_hit_damage_bonus"] + playerMinionIntrinsicStats["base_critical_hit_damage_bonus"], "Base")
```

> 注意：玩家默认爆伤 +100%（见 [critical-hits.md](./critical-hits.md)）；召唤物走的是「怪物基础 + 内禀 +70」这条不同的链，**不要照搬玩家的 +100**。

---

## 二、玩家修饰词如何作用于召唤物（传递规则）

### 2.1 默认不传递

> **铁律**：召唤物**不**享受「影响玩家自身攻防的修饰词」。只有**明确指向召唤物 / 盟友**的修饰词才生效[^poe2wiki-minion]。即玩家身上的 "increased Physical Damage"、抗性、攻速……对召唤物**无效**，除非词条写成 "Minions deal/have …" 或通过光环/盟友 buff。

### 2.2 `MinionModifier`：传递的核心机制

PoB2 把所有「Minions deal/have X」类词条，在 `SkillStatMap.lua` 里映射成一条**包裹型** `mod("MinionModifier","LIST",{ mod = <真正要给召唤物的 mod> })`。它挂在**玩家的 skillModList** 上，但 value 是「一条等着被注入召唤物 modDB 的内层 mod」。

CalcPerform 在召唤物属性结算后，遍历玩家技能的 `MinionModifier` 列表，把内层 mod 逐条 `AddMod` 到召唤物的 `modDB`（`CalcPerform.lua`）：

```lua
for _, value in ipairs(player.mainSkill.skillModList:List(skillCfg, "MinionModifier")) do
    if not value.type or env.minion.type == value.type then   -- value.type 可限定只对某类召唤物
        env.minion.modDB:AddMod(value.mod)                     -- 注入召唤物自己的库
    end
end
```

`SkillStatMap.lua` 的真实映射举例（**核对基准**）：

| 游戏内 stat | 注入召唤物 modDB 的 mod |
|---|---|
| `minion_damage_+%` | `Damage INC` |
| `minion_melee_damage_+%` | `Damage INC`（`ModFlag.Melee`） |
| `minion_maximum_life_+%` | `Life INC` |
| `minion_attack_speed_+%` | `Speed INC`（`ModFlag.Attack`） |
| `minion_critical_strike_chance_+%` | `CritChance INC` |
| `minion_critical_strike_multiplier_+` | `CritMultiplier BASE` |
| `minion_accuracy_rating_+%` | `Accuracy INC` |
| `minion_elemental_resistance_%` / `summon_fire_resistance_+` | `ElementalResist` / `FireResist BASE` |
| `minion_additional_physical_damage_reduction_%` | `PhysicalDamageReduction BASE` |
| `minion_chance_to_deal_double_damage_%` | `DoubleDamageChance BASE` |
| `minions_deal_%_of_physical_damage_as_additional_chaos_damage` | `PhysicalDamageGainAsChaos BASE` |
| `active_skill_minion_damage_+%_final` | `Damage MORE`（技能自带 final，见 §2.4） |
| `minion_always_crit` | `CritChance OVERRIDE = 100` |
| `minions_are_gigantic` | `flag("Gigantic")` |

关键点：注入的 mod **携带原本的 type/flag/condition/tag**（如 Melee/Attack flag、FullLife condition、`ActorCondition enemy=Frozen` 等），因此它们在召唤物自己的 `matches(cfg)` 判定下生效，语义与玩家侧完全一致。

### 2.3 敌对召唤物 (hostile) 走 `EnemyModifier`

若召唤物是敌对的（`minionData.hostile`），传递的不是 `MinionModifier` 而是玩家 `modDB` 上的 `EnemyModifier`（即「敌人受到的…」debuff），逻辑对称（`CalcPerform.lua`）：

```lua
if env.minion.hostile then
    for _, value in ipairs(env.modDB:Tabulate(nil,nil,"EnemyModifier")) do
        env.minion.modDB:AddMod(setSource(copy, ...))
    end
end
```

### 2.4 「more damage vs 非唯一怪」的技能内禀缩放

召唤物对**非唯一怪**有一条基于召唤技能等级的 more 伤害（hits 与 ailment）：**技能等级 3 起 +3%，到等级 8 达 +50% more**[^poe2wiki-minion]。这正是 `active_skill_minion_damage_+%_final` → `Damage MORE` 的体现（技能数据里的 `_final` permyriad 随等级给出），对 boss 不享受。

### 2.5 光环 / 盟友 buff 传递

光环 / 持续 buff 通过 `buff.applyMinions` / `buff.applyAllies` / 旗标 `BuffAppliesToAllies` 传给召唤物（`CalcPerform.lua`），强度按**召唤物自己的** `BuffEffectOnSelf` 缩放（不是玩家的）：

```lua
if env.minion and not env.minion.hostile and (buff.applyMinions or buff.applyAllies or BuffAppliesToAllies) then
    local inc  = modStore:Sum("INC", skillCfg, "BuffEffect") + env.minion.modDB:Sum("INC", nil, "BuffEffectOnSelf")
    local more = modStore:More(skillCfg, "BuffEffect") × env.minion.modDB:More(nil, "BuffEffectOnSelf") × Magnitude
    srcList:ScaleAddList(buff.modList, (1 + inc/100) × more)
    mergeBuff(srcList, minionBuffs, buff.name)
end
```

### 2.6 属性 (Str/Dex/Int) 与召唤物

召唤物**默认 0 基础属性**，也**不**自动继承玩家属性。只有专门旗标才把玩家属性灌给召唤物（`CalcPerform.lua`）：
- `StrengthAddedToMinions` / `HalfStrengthAddedToMinions` → 给召唤物 `Str BASE`；
- `DexterityAddedToMinions` → `Dex BASE`；
- Companion 版：`StrengthAddedToCompanions` / `DexterityAddedToCompanions`（限 `SkillType.Companion`）。

被灌入后，召唤物按**与玩家相同的属性派生规则**把 Str/Dex/Int 转成生命/护甲/闪避/暴击等（属性派生在召唤物 Actor 内重新算）。许多「Minions from this skill have 1% increased Damage per 1 of your Strength」是把玩家属性以 multiplier 形式引用进 MinionModifier，而非把属性本体灌入。

### 2.7 不传递的典型项

玩家自身的：基础生命/法力、武器伤害（除非召唤物 `uses` 该武器槽，如 Bow+Quiver / The Iron Mass）、护甲/闪避/ES 评级、玩家抗性、玩家充能（召唤物有自己的充能体系，见 §4）、普通（非 Minion 限定）的 increased/more 词条。`Necronomicon` 笔记等社区资料对 PoE1 的「Necro Aegis / 共享盾属性」描述**不直接适用** PoE2，以一手数据为准。

---

## 三、召唤物自身的攻防计算（沿用同一套 calc）

召唤物的 DPS/暴击/异常/命中/防御**完全复用** CalcOffence/CalcDefence，只是把 Actor 与 modDB 换成召唤物：

- **暴击**：召唤物武器 `critChance`（默认 5%）作基础暴击率，走 [critical-hits.md](./critical-hits.md) 的「基础 × (1+inc) × more → ×命中率(若需) → 幸运/分岔/必然」链；爆伤基础 = monster + 内禀 +70（§1.5）。`minion_always_crit` 直接 `CritChance OVERRIDE = 100`。
- **命中**：0.3.0 起召唤物默认 `CannotBeEvaded`（必中），不必算命中率；除非 `MinionAccuracyEqualsAccuracy` 令其改用玩家命中。
- **异常 (ailments)**：召唤物施加的点燃/流血/中毒/冰冻等，magnitude 与阈值用**召唤物自己**的伤害/属性，走 [damage-scaling.md](./damage-scaling.md) 的 DoT/异常路径；`minion_ailment_damage_+%` → 召唤物 `Damage INC`（`KeywordFlag.Ailment`）。
- **防御 / EHP**：召唤物的护甲/闪避/抗性/ES/Max Hit 按 [recovery-charges-buffs.md](./recovery-charges-buffs.md) §四同构求解，Actor 换成召唤物。
- **眩晕 buildup**：召唤物按怪物常量给 `PhysicalHeavyStunBuildup` / `EnemyHeavyStunBuildup` MORE（怪物式系数，与玩家不同）。

---

## 四、数量上限 / 存活 / 复活 / 持续

### 4.1 数量上限 (Limit)

每类召唤物有一个**上限稳定 ID**（`minionData.limit`，如 `ActiveZombieLimit` / `ActiveSkeletonLimit` / `ActiveRagingSpiritLimit` / `ActiveSpectreLimit` / `ActiveGolemLimit` / `ActiveWolfLimit` / `ActiveSpiderLimit` …）。上限值由 `base_number_of_<x>_allowed` 类 stat 给出，玩家 `+N to Maximum …` 累加（`SkillStatMap.lua` / `CalcPerform.lua`）：

```lua
limit = floor( Override(limitName) or ( val(skillModList, limitName) × More(skillCfg,"ActiveMinionLimit") ) )
-- 暴露为乘数供 per-minion 词条引用：
modDB:NewMod("Multiplier:SummonedMinion",       "BASE", limit, ...)
modDB:NewMod("Multiplier:MinionPresenceCount",  "BASE", limit, ...)
```

`Multiplier:SummonedMinion` 供「N% increased Damage per Minion」「5% increased Attack Damage for each Minion in your Presence (上限 80%)」这类 per-minion 词条引用（与 [recovery-charges-buffs.md](./recovery-charges-buffs.md) 的 charge multiplier 同构）。`MinionPerCastCount` / `number_of_*_skeletons_to_summon` 决定每次施放召出几个。

### 4.2 持续 (Duration)

- **持续型 (temporary)** 召唤物（SRS、Living Lightning、Wardbound 等）有 `Minion duration`，由 `base_minion_duration_+%` → `Duration INC`（`SkillType.CreatesMinion`）缩放；有的按「释放 N 次后消散」(`Minions disperse after N`)。
- **持久型 (persistent / Reviving)** 召唤物（多数 Spirit 保留召唤）无固定时长，靠保留 Spirit 维持（见 [recovery-charges-buffs.md](./recovery-charges-buffs.md) §五 Spirit 保留；`spectreReservation` / `companionReservation` 字段给单个召唤的保留量）。

### 4.3 复活 / 存活 (Reviving Minions)

PoE2 的持久召唤多为 **Reviving Minion**：避免受伤一段时间后**回血**，死亡后**短延迟自动复活**；该延迟在**另一个 Reviving Minion 死亡时被重置**[^poe2wiki-minion]。相关辅助：
- `Last Gasp`（Persistent 召唤死后续战 4 秒再死）；`Tecrod's Revenge`（续战 20 秒 + Soul Eater）。
- `Infernal Legion`（召唤物每秒按自身最大生命 % 受火焰伤、灼烧周围；0.5.0 从 20% → **10%**）[^maxroll-050]。
- `Bone Offering` / `Skeletal Frost Mage` 的护盾减伤。

---

## 五、召唤物专用关键词与标签（对应 pobr 的 Actor/flags）

- **技能标签**：`Minion`（召唤物技能）、`Companion`（同伴，0.5.0 强化的 Tame Beast / Spirit Walker 体系）、`Persistent`、`Trigger`、`Duration`、`Spectre`。辅助宝石标签 `Minion` 决定能否辅助召唤技能。
- **召唤物身上的 flag**（注入其 modDB）：`Gigantic`、`CannotBeEvaded`（必中）、`HiddenMonster`（隐藏怪不享盟友 buff）、`Condition:CommandableSkill`（指令技能）、`DealNoDamage`（如 Umbral Well 禁伤）。
- **per-minion / presence multiplier**：`Multiplier:SummonedMinion`、`Multiplier:MinionPresenceCount`。
- **限定 type 的 MinionModifier**：`value.type == minion.type` 时才注入（「Minions from this skill …」只作用于该技能召出的召唤物）。

---

## 六、0.5.0 相关变化

- **移除优先级**：因资源变化（如换武器）移除召唤物时，**优先移除较新（younger）的召唤物**，以免浪费老召唤物的冷却技能[^poe2-050-forum][^poe2wiki-050]。
- **Living Lightning**：满数量时召新的**不再自动顶替**已有召唤物[^poe2wiki-050]。
- **Rage I/II/III**：现在**可以辅助召唤物技能**（配合 `attack_damage_final_permyriad_per_rage = 100` 内禀，每点怒气 +1% 攻击伤害），开辟新召唤流派[^poebuilds-050]。
- **Infernal Legion I/II**：生命消耗与召唤物点燃伤害 20% → **10%**[^poe2wiki-050]。
- **新增 `Minion Splash` / `Minion Splash II`** 力量辅助（让 Strike 召唤物的打击具溅射）；新 Companion 升华 **Spirit Walker**（Huntress）与 **Martial Artist**（Monk，召幻象钟）[^poe2-050-forum]。
- **Companion 系统强化**：Tame Beast 大幅提伤、足够 Spirit 时新驯服的野兽可立即召唤[^poebuilds-050]。
- **Wardbound Minions / Eternal March**（Kalguuran/Runic Ward 体系）等新召唤技能加入。
- 召唤物**基础属性派生公式**（怪物表 × 归一化乘数、内禀必中/+70 爆伤、MinionModifier 传递机制）在 0.5.0 未见结构性改动（以 PoB2 `dev` 分支为准）。

---

## PoB2 计算实现（核对基准）

变量/旗标取自 [PathOfBuilding-PoE2 `dev`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的 `src/Modules/CalcActiveSkill.lua`、`CalcPerform.lua`、`CalcOffence.lua` 与 `src/Data/{Minions,Misc,SkillStatMap}.lua`：

**Actor 与 modDB（`CalcActiveSkill.lua` / `CalcPerform.lua`）**
```lua
activeSkill.minion = { type, minionData = data.minions[type], level, parent, enemy, weaponData1, weaponData2 }
env.minion = env.player.mainSkill.minion
env.minion.modDB = new("ModDB"); env.minion.modDB.actor = env.minion
env.minion.modDB.multipliers["Level"] = env.minion.level
-- 基础值
baseLife = floor(minion.lifeTable[level] × minionData.life × (hostile and mapLevelLifeMult or 1))
Armour/Evasion BASE = round(monster{Armour,Evasion}Table[level] × (minionData.{armour,evasion} or 1))
CritMultiplier BASE = monsterConstants.base_critical_hit_damage_bonus + playerMinionIntrinsicStats.base_critical_hit_damage_bonus(=70)
CannotBeEvaded FLAG = 1   -- 默认必中（0.3.0+）
{Fire,Cold,Lightning,Chaos}Resist BASE = minionData.<x>Resist
```

**虚拟武器（`CalcActiveSkill.lua`）**
```lua
damage = damageTable[level]; if not baseDamageIgnoresAttackSpeed then damage = damage × attackTime end
weaponData1 = { AttackRate=1/attackTime, CritChance=minionData.critChance,
                PhysicalMin=round(damage×(1-damageSpread)), PhysicalMax=round(damage×(1+damageSpread)), range=attackRange }
hiddenDamageFixup = monsterDamage and (allyTable[level]/damageTable[level] × SpectreBeastDamageFixup - 1) or 0
```

**player→minion 传递（`CalcPerform.lua`）**
```lua
-- 友方：注入 MinionModifier 内层 mod
for value in player.mainSkill.skillModList:List(skillCfg,"MinionModifier") do
    if not value.type or minion.type == value.type then env.minion.modDB:AddMod(value.mod) end
-- 敌对：注入 EnemyModifier
for value in env.modDB:Tabulate(nil,nil,"EnemyModifier") do env.minion.modDB:AddMod(copy) end
-- 光环/buff：按 env.minion.modDB 的 BuffEffectOnSelf 缩放
```

**常量（`Misc.lua`）**
```lua
data.minionLevelTable = {2,4,6,…,80}                       -- 宝石等级→怪物等级
data.playerMinionIntrinsicStats = { global_always_hit=1, base_critical_hit_damage_bonus=70,
                                     stun_base_duration_override_ms=500, attack_damage_final_permyriad_per_rage=100 }
data.monsterLifeTable / monsterAllyLifeTable / monsterDamageTable / monsterAllyDamageTable / monsterArmourTable / monsterEvasionTable
-- 默认上限 3 charges（召唤物也是 monster，沿用 monster 充能默认；见 recovery-charges-buffs.md）
```

**SkillStatMap（`SkillStatMap.lua`，`MinionModifier LIST` 包裹）**：见 §2.2 表。Limit：`base_number_of_<x>_allowed` → `Active<X>Limit BASE`。

---

## 对 pobr 实现的启示

对照 `pobr-core`（`config.rs::CalcConfig` 的 Actor 概念、`mod_db.rs` 聚合、`calc/env.rs::Env` 的 player/enemy `Actor`、`trace.rs`），实现召唤物域时：

1. **召唤物 = 第三个独立 Actor，持有独立 `ModList`/`ModDb`。**
   `Env` 当前持有 player/enemy 两个 `Actor`；召唤物需要 `env.minion: Option<Actor>`，有自己的 `modDB` 与 `output`。召唤物的 offence/defence **直接复用** `calc/offence.rs` / `calc/defence.rs`，把 Actor/ModDb 换成召唤物——**不要**为召唤物另写一套公式。这与 PoB2 的 `env.minion` 结构一一对应。

2. **基础属性派生：怪物表 × 归一化乘数，落在 `pobr-data`。**
   把 `monsterLifeTable` / `monsterAllyLifeTable` / `monsterDamageTable` / `monsterAllyDamageTable` / `monsterArmour/EvasionTable` / `minionLevelTable` 与每个召唤物条目（`life`/`damage`/`damageSpread`/`attackTime`/`critChance`/`armour`/`evasion`/`energyShield`/`<x>Resist`/`limit`/`spectreReservation` 等）入库为 `pobr-data` 的新 schema（如 `MinionDef`）。`pobr-gamedata` 负责反序列化。

3. **虚拟武器 (weaponData) 是召唤物伤害入口。**
   召唤物攻击不是直接伤害词条，而是合成 `WeaponData { phys_min/max, crit_chance, attack_rate, range }`，再喂给和玩家相同的攻击伤害管线。`baseDamageIgnoresAttackSpeed` 决定基础伤害是否乘 `attackTime`。法术型召唤物走 skillList 技能基础伤害。

4. **player→minion 传递 = `MinionModifier` 包裹 mod 的注入，绝不全量继承。**
   - 新增 `ModName::MinionModifier`（LIST 语义），其值是**一条内层 `Modifier`**（含完整 flag/condition/tag）。聚合阶段把玩家技能上的 `MinionModifier` 内层 mod **`AddMod` 进召唤物 `ModDb`**，可选 `type` 限定（只对该类召唤物）。
   - 敌对召唤物改注入 `EnemyModifier`（对称）。
   - 这条是召唤物域**最核心**的设计：玩家普通词条**默认不进**召唤物库，只有 `MinionModifier` / 盟友 buff / 属性灌注旗标三条通道。

5. **暴击/命中/爆伤用召唤物专属默认值，别套玩家默认。**
   爆伤基础 = `monster_base_crit_damage_bonus + 70`（内禀），非玩家 +100；默认 `CannotBeEvaded`（必中，0.3.0+），`MinionAccuracyEqualsAccuracy` 才改用命中检定。基础暴击率来自虚拟武器 `critChance`（默认 5）。

6. **数量/per-minion multiplier。**
   `Active<X>Limit` 走稳定 ID；最终上限暴露为 `Multiplier:SummonedMinion` / `MinionPresenceCount`，供「per Minion / per Minion in Presence」词条经 `Modifier::effective_number` 的 Multiplier tag 引用——与充能 multiplier 完全同构，直接复用现有机制。

7. **属性灌注是旗标驱动的 BASE 注入。**
   `StrengthAddedToMinions` 等旗标 → 给召唤物 `Str/Dex BASE`，随后召唤物 Actor 内按相同属性派生规则转生命/护甲/暴击。不要默认继承玩家属性。

8. **归因 (TraceGraph) 的增量价值。**
   每条 `MinionModifier` 注入、每个盟友 buff、属性灌注，都应能把召唤物输出回溯到玩家侧 `SourceId`（哪件装备/天赋/光环的「Minions deal …」贡献了多少召唤物 DPS）。这正是 pobr 相对 PoB 的核心增量——PoB2 把召唤物 mod 注入后**丢失了来源链**，pobr 可保留「玩家来源 → 召唤物输出」的跨 Actor 归因。

---

## 参考来源

[^poe2wiki-minion]: PoE2 Wiki — Minion（召唤物不享玩家攻防词条、仅 Minion/Ally 词条生效；Reviving Minion 回血/复活；对非唯一怪 skill lv3 +3% → lv8 +50% more 伤害）。https://www.poe2wiki.net/wiki/Minion
[^pob2-minions]: PathOfBuilding-PoE2 — `src/Data/Minions.lua`（各召唤物 `life`/`damage`/`damageSpread`/`attackTime`/`critChance`/`armour`/`evasion`/`energyShield`/`<x>Resist`/`limit`/`spectreReservation`/`companionReservation`/`baseDamageIgnoresAttackSpeed` 字段）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Minions.lua
[^pob2-calcactiveskill]: PathOfBuilding-PoE2 — `src/Modules/CalcActiveSkill.lua`（minion 等级判定、lifeTable/damageTable 选择、虚拟 weaponData1 合成、`hiddenDamageFixup`/`damageFixup`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcActiveSkill.lua
[^pob2-calcperform]: PathOfBuilding-PoE2 — `src/Modules/CalcPerform.lua`（`env.minion` 初始化、baseLife/Armour/Evasion/Resist/CritMultiplier/CannotBeEvaded、`MinionModifier`/`EnemyModifier` 注入、buff→ally 传递、属性灌注旗标、Limit→`Multiplier:SummonedMinion`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcPerform.lua
[^pob2-skillstatmap]: PathOfBuilding-PoE2 — `src/Data/SkillStatMap.lua`（`minion_*` stats → `mod("MinionModifier","LIST",{mod=…})` 包裹映射；`base_number_of_*_allowed` → `Active<X>Limit`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/SkillStatMap.lua
[^pob2-misc]: PathOfBuilding-PoE2 — `src/Data/Misc.lua`（`minionLevelTable`、`playerMinionIntrinsicStats`={global_always_hit=1, base_critical_hit_damage_bonus=70, attack_damage_final_permyriad_per_rage=100}、monster life/damage/armour/evasion 表）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Misc.lua
[^poe2-050-forum]: Path of Exile 官方论坛 — Content Update 0.5.0 "Return of the Ancients" Patch Notes（younger minion 优先移除、Minion Splash 辅助、Spirit Walker/Martial Artist 升华、Rage 可辅助召唤物）。https://www.pathofexile.com/forum/view-thread/3932540
[^poe2wiki-050]: PoE2 Wiki — Version 0.5.0（Living Lightning 不再顶替、Infernal Legion 20%→10%、Last Gasp/Tecrod's Revenge、Wardbound/Eternal March）。https://www.poe2wiki.net/wiki/Version_0.5.0
[^poebuilds-050]: PoeBuilds.net — PoE2 0.5.0 Patch Notes（Tame Beast/Companion 强化、Rage 辅助召唤物、Living Lightning）。https://www.poebuilds.net/post/path-of-exile-2-0-5-0-patch-notes
[^maxroll-050]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients。https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
