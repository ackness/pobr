# 技能功能面机制 (Skill Mechanics)

本文档覆盖 PoE2（0.5.0）技能的**功能面**机制——技能的**行为**而非纯伤害数值：范围、投射物、持续时间、冷却、消耗/保留、重复、引导，以及技能标签如何决定修饰词适用。伤害本体见 [damage-scaling.md](damage-scaling.md) / [damage-types.md](damage-types.md)；速度（攻击/施法/技能速度、服务器帧上限）见 [skill-speed.md](skill-speed.md)，本文不重复；宝石等级/品质/转化见 [gems.md](gems.md)；触发型元宝石的 Energy/触发见 [meta-gems.md](meta-gems.md)；Spirit 保留的回复/上限公式见 [recovery-charges-buffs.md](recovery-charges-buffs.md)，本文只讲技能侧的保留消耗。

> 本文每条结论对照 PoB2 `src/Modules/CalcActiveSkill.lua`、`CalcOffence.lua`（线上）与本地 vendor `src/Data/SkillStatMap.lua`、`Costs.lua`、`Misc.lua`、`Global.lua`。末尾「## 对 pobr 实现的启示」给出对应 pobr 的 `CalcConfig` flags/tags/skill_types 与 `ModName`。

## 技能标签 / 技能类型 (Skill Tags / SkillType)

技能宝石带一组**类型标签**（PoB2 内部是 `SkillType` 枚举，见 `Data/Global.lua` 的 `SkillType = {...}`），它们决定：
1. 技能能被哪些辅助宝石支持；
2. 哪些修饰词适用（修饰词上挂的 `SkillType` / `ModFlag` / `KeywordFlag` 条件需匹配技能的类型集合）。

`SkillStatMap.lua` 里大量修饰词带 `{ type = "SkillType", skillType = SkillType.XXX }` 条件，例如：
- `RepeatCount` 加成只对 `SkillType.Multicastable`（法术回响）/ `Cascadable`（法术级联）/ `WeaponMelee|Unarmed`（近战）等对应类型生效。
- `AdditionalProjectiles` 之类只对 `Projectile` 技能有意义。

关键 `SkillType`（节选，按 0.5.0 枚举值）：`Attack=1`、`Spell=2`、`Projectile=3`、`Area=8`、`Duration=9`、`HasReservation=12`、`ReservationBecomesCost=13`、`Chains=19`、`Melee=20`、`Multicastable=22`（可被 Spell Echo 重复）、`Triggerable=31`/`Triggers=32`/`Triggered=37`、`Channel=48`、`Cascadable=56`、`Warcry=63`、`Instant=64`、`Cooldown=91`、`Slam=93`、`Nova=85`、`Mark=99`、`Cascadable`、`NonRepeatable=95`（如 Blood and Sand）。

**ModFlag / KeywordFlag**（与 `SkillType` 不同的另一套位标志）：技能在初始化时（`CalcActiveSkill.lua::initSkill`）把 `skillFlags.projectile / melee / area / ...` 折成 `skillModFlags`（如 `ModFlag.Projectile`、`ModFlag.Spell`、`ModFlag.Attack`、`ModFlag.WeaponMelee`），修饰词据此匹配。`KeywordFlag.Arrow`/`Aura` 等区分"弓箭专属"投射物计数 vs 通用投射物计数（见 `SkillStatMap` 里 `ProjectileCount` 同时有 Arrow 版与通用版）。

## 范围 (Area of Effect)

### 半径与面积的换算（几何收益递减的真正来源）

PoE2 的 AoE 修饰词（`increased/more Area of Effect`）作用于**面积**，但圆形 AoE 的半径与面积是平方关系，故半径只按**面积乘数的平方根**缩放[^poe2-forum-aoe1][^poe2-forum-aoe2]：

PoB2 `CalcOffence.lua::calcRadius`：

```lua
local function calcRadius(baseRadius, areaMod)
    return m_floor(baseRadius * m_floor(100 * m_sqrt(areaMod)) / 100)
end
```

即 `半径 = floor(baseRadius × floor(100 × √areaMod)/100)`。

- `areaMod = (1 + Σincreased/100) × Π(1 + more/100)`（标准 inc/more 聚合，无额外 DR 曲线）。PoB2：`incArea, moreArea = calcLib.mods(...,"AreaOfEffect",...)`；`AreaOfEffectMod = round(incArea × moreArea, 2)`。
- 例：单一 `+50% Area`（Expanse / Magnified Effect 类）→ 半径只增 `√1.5 − 1 ≈ 22.5%`；`+40% Area` → 半径增 `√1.4 − 1 ≈ 18%`[^poe2-forum-aoe1]。这就是玩家常报"40% AoE 只看到 ~10% 半径"的原因——**没有显式的 AoE 收益递减惩罚**，递减纯粹来自"面积→半径的开方"加上工具提示把半径四舍五入到 0.1m。

> **要点**：`increased Area` 与 `more Area` 仍按通常方式 inc 加法、more 乘法聚合；几何上的"收益递减"是面积/半径关系的必然结果，不是单独的 DR 公式。

### 主/次/三段半径

技能可有 primary/secondary/tertiary 三段独立半径（`AreaOfEffectPrimary/Secondary/Tertiary`、`AreaOfEffectRadius/Secondary/Tertiary`）。Molten Strike 的第三段（弹跳落点）有专门的死区半径处理 `calcMoltenStrikeTertiaryRadius`，受 AoE 与投射物速度共同影响。

`calcRadiusBreakpoints` 计算"再加多少 inc/more 才能跨过下一个 0.1m 半径台阶"——因半径被 floor 到 0.1m，**AoE 有离散台阶**，PoB2 会在 breakdown 里提示最近台阶。

## 投射物 (Projectiles)

### 数量 (Projectile Count)

`CalcOffence.lua`（约 L1287-1296）：

```lua
projBase = Sum("BASE","ProjectileCount")
         + 2 * Sum("BASE","TwoAdditionalProjectilesChance")/100
         + Sum("BASE","SurpassingProjectileChance")/100
projMore = More("ProjectileCount")
output.ProjectileCount = projBase * projMore
```

- `SkillStatMap` 把 `base_number_of_projectiles` / `base_number_of_arrows` 映射成 `ProjectileCount BASE`（并配 `base = -1`，因技能基础值已含 1 发；额外发数走 `number_of_additional_projectiles`）。
- 弓箭专属用 `KeywordFlag.Arrow` 区分通用投射物。
- 标志：`NoAdditionalProjectiles`（`modifiers_to_projectile_count_do_not_apply` 等）锁定数量；`AdditionalProjectilesAddSplitsInstead` / `AdditionalProjectilesAddChainsInstead` 把"额外投射物"转换成额外 Split / Chain。

**散射 / 同目标规则**：同一次"齐射"中**同组投射物每个目标只能被命中一次**，除非技能特别说明（如 Freezing Shards / Fragmentation Rounds 可 Merge）[^poe2wiki-projectile]。

### 投射物行为优先级（PoE2 关键差异）

一次碰撞**只能触发一种行为**，按固定优先级依次尝试剩余行为[^poe2wiki-projectile][^poe2wiki-chain]：

```
1. Split（分裂）  2. Pierce（穿透）  3. Fork（分叉）  4. Chain（连锁）
```

- **Split（分裂）**：首次命中敌人时分裂成 N 个，飞向 6m 内不同目标（不足则随机方向）；分裂体是原投射物的"延续"，共享最大飞行距离、不能再命中已命中目标。`SkillStatMap`：`projectile_number_to_split → SplitCount BASE`；`CannotSplit`（`projectiles_cannot_split` / `projectile_behaviour_only_explode`）。
- **Pierce（穿透）**：穿过目标继续飞，每次穿透独立判定。`PierceCount BASE`（`projectile_base_number_of_targets_to_pierce`）/ `PierceChance BASE`（`pierce_%`）；`PierceAllTargets`（`always_pierce`）；`CannotPierce`。**若可无限穿透，则后续行为永不触发**；地形不可被穿透。
- **Fork（分叉）**：首次命中且未穿透/分裂时一分为二，固定夹角射出。`ForkOnce`（`projectiles_fork`，`ForkCountMax BASE`）；`ForkTwice`/`number_of_additional_forks_base`；`CannotFork`。PoB2 把 `ForkCountMax` 夹在 1（仅 ForkOnce）或 2（ForkTwice）。
- **Chain（连锁）**：碰撞后重定向到最近未被同组命中过的目标。投射物连锁距离 **6m**，其它效果（光束等）连锁距离 **4m**[^poe2wiki-chain]。`ChainCountMax BASE`（`number_of_chains`）；`chains_hit_X_more_times → ChainCountMax MORE`；`BeamChainCountMax`（光束专用）。
  - **因优先级**：能穿透或分叉的投射物**不会从敌人连锁**；但**可以在穿透/分叉后从地形连锁**（terrain chain 独立计数）。`TerrainChainChance`（`projectile_chance_to_chain_1_extra_time_from_terrain_%`），地形连锁默认每投射物只一次，几率加法叠加。

### 返回 (Return)

PoE2 的"返回"是条件态而非计数：`ReturningProjectile` 条件触发 `active_skill_returning_projectile_damage_+%_final → Damage MORE`；`returning_projectiles_always_pierce` 在返回阶段强制穿透。典型来源词条形如"Attack Projectiles Return if they Pierced at least N times"、"Reversing Arrows Pierce all targets and Return to you"[^poe2db-pierce]。

### 速度

`ProjectileSpeed INC/MORE`；`ProjectileSpeedAppliesToProjectileDamage`（速度也加成投射物伤害）；`CastSpeedAppliesToProjectileSpeed`；`arrowSpeedAppliesToAreaOfEffect`（箭速影响 AoE，如某些落地范围技）。

## 持续时间 (Duration)

`CalcOffence.lua`（L362-365 / L1838-1853）：

```lua
durationMod  = calcLib.mod(skillModList, skillCfg, "Duration", "PrimaryDuration",
                           "DamagingAilmentDuration", mineDurationAppliesToSkill and "MineDuration")
durationBase = (skillData.duration or 0) + Sum("BASE","Duration","PrimaryDuration")
output.Duration = durationBase * durationMod          -- 再向上取整到服务器帧
output.Duration = m_ceil(Duration * ServerTickRate) / ServerTickRate
```

- `skill_effect_duration_+% → Duration INC`；`secondary/tertiary` 各有独立池（`SecondaryDuration` / `TertiaryDuration`）。`less duration` 走 MORE 负值。
- **最终时长向上取整到服务器帧**（与冷却同款 `m_ceil(x*ServerTickRate)/ServerTickRate`，ServerTickRate 即 1/0.033≈30.3，见 [skill-speed.md](skill-speed.md)）。
- **与 DoT 时长的关系**：`DamagingAilmentDuration` 共享在 duration 池里参与聚合（damaging ailment 的时长 = 技能时长链的一部分）。特殊标志：`bleed_duration_is_skill_duration` / `poison_duration_is_skill_duration`（`skill("bleedDurationIsSkillDuration")`）让流血/中毒时长直接取技能时长。`mineDurationAppliesToSkill` 让地雷时长加成施加于技能时长。
- 还有 `AuraDuration` / `ReserveDuration` / `SoulGainPreventionDuration` 等派生时长，复用同一 duration 修饰词链。

## 冷却 (Cooldown)

`CalcOffence.lua::calcSkillCooldown`（L325-346）：

```lua
cooldownOverride = Override("CooldownRecovery")
addedCooldown    = Sum("BASE","CooldownRecovery")          -- 直接加/减的毫秒（base_cooldown_modifier_ms）
cooldownBase     = (skillData.cooldown or 0) + addedCooldown
cooldown = cooldownOverride or cooldownBase / max(0, calcLib.mod(...,"CooldownRecovery"))
-- 通常向上取整到服务器帧：
cooldown = m_ceil(cooldown * ServerTickRate) / ServerTickRate
```

- **冷却恢复速率**（`base_cooldown_speed_+% → CooldownRecovery INC`，`..._final → MORE`）是除数：`实际冷却 = 基础冷却 / (1 + Σinc/100) / Π(1+more/100)`。即"increased Cooldown Recovery Rate"缩短冷却。
- **储存次数 (Stored Uses / Charges)**：`skillData.storedUses` 或 `AdditionalCooldownUses BASE`（`+1 use` 类）。**当技能可储存多次使用且有冷却时，冷却值不向服务器帧取整**（PoB2 注释明确：`it doesn't round the cooldown value to server ticks`），按真实值累计可用次数。
- 特殊标志：`NoCooldownRecoveryInDuration`（持续期间不恢复冷却）；`CooldownDoesNotLimitSkillSpeed`（`channelled_skill_do_not_go_on_cooldown_on_finishing_channel`，引导技结束不进冷却，使冷却不限制速度）；`CooldownChanceNotConsume`（几率不消耗冷却）；`CooldownRecoveryFromTemporalis`（独特来源，且有 0.1s 下限保护）。
- **与攻击/施法速度的关系**：纯冷却技能的输出频率由 `min(冷却倒数, 使用速度)` 决定——速度提高到冷却倒数以上后受冷却"门控"；储存次数可在冷却外提供突发。Trap/Mine 有各自的 `trapCooldown` 与投放速度门控。

## 消耗与保留 (Cost / Reservation)

### 资源类型

`Data/Costs.lua` 定义了全部消耗资源（`Resource` / 对应 `Stat` / `Divisor`）：Mana、Life、ES、Rage、Ward、Soul（`Souls Per Use`），以及百分比版（ManaPercent/LifePercent/...）与每分钟版（ManaPerMinute，`Divisor=60` → 显示为每秒）。

### 消耗计算 (CalcOffence.lua, L2050+)

```lua
if not Flag("HasNoCost") then
  -- 1) 先算成本倍乘 mult（辅助宝石的 cost multiplier，四位小数后向下取整）
  -- 2) 逐资源：baseCost = 宝石基础 cost / Divisor
  --    + CostBase (BASE)；ManaCostNoMult (BASE，乘数前的固定加值，如 Divine Blessing)
  -- 3) ManaCost INC（base_mana_cost_-% → 注意是 INC，负则降耗）
  --    ManaCostEfficiency INC（base_mana_cost_efficiency_+%）
  --    ManaCost MORE（no_mana_cost / Cost MORE → no_cost 全免）
  finalBaseCost = floor(baseCost * mult + baseCostNoMult + ...)
```

- 转换：`BaseManaCostAsLifeCost`（Petrified Blood 类，按 % 把法力消耗加成生命消耗）、`ManaCostAsEnergyShieldCost`、`HybridManaAndLifeCost_Life`（混合，上限 100%）、`CostLifeInsteadOfMana`（PoB2 注 PoE2 暂未用）。
- **Soul 消耗**：`unaffectedByGenericCostMults = true`——灵魂消耗**不受通用成本倍乘影响**（Vaal 技能逻辑）。
- `AttackSpeedScalesCost`（`attack_speed_modifiers_apply_to_over_time_cost`）：攻速加成持续型（每秒）消耗。

### Spirit 保留 (Reservation)

- 持续增益/光环/元宝石按宝石保留 **Spirit**（`HasReservation` 类型，非 `ReservationBecomesCost`）。
- `ReservationMultiplier MORE`（宝石等级带 `level.reservationMultiplier`）、`ExtraSpirit BASE`（`level.spiritReservationFlat`）影响保留量。
- **成本倍乘 ≠ 保留倍乘**：带 cost multiplier 的辅助宝石**不**增加 Spirit 保留，除非显式有保留倍乘（见 [meta-gems.md](meta-gems.md)）。
- `ReservationBecomesCost`（Divine Blessing / 图腾光环）把保留转成一次性消耗：`reservedFlat + floor(资源 × reservedPercent/100)`。
- Spirit 的可用上限与回复不在技能侧——见 [recovery-charges-buffs.md](recovery-charges-buffs.md)（本文不重复保留池公式）。

## 重复 (Repeat / Echo)

`CalcOffence.lua`（L981+）：

```lua
output.Repeats = 1 + (repeatSkillTypesCheck(skillTypes) and Sum("BASE","RepeatCount") or 0)
```

- `RepeatCount` 仅对可重复类型生效：`Multicastable`（法术回响 Spell Echo）、`Cascadable`（法术级联）、近战（`WeaponMelee|Unarmed`）、`RequiresShield`。`SkillStatMap`：`skill_repeat_count`、`base_melee_attack_repeat_count`。
- **重复的伤害处理**：每次重复可有独立 more 伤害——PoB2 用 `RepeatOneDamage` / `RepeatTwoDamage` / `RepeatThreeDamage`（MORE）按"第几次重复"施加（L1048-1057），即首击与回响击可不同倍率。
- 标志：`CannotRepeat`（`disable_skill_repeats`）；`NoRepeatBonuses`（`skill_cannot_gain_repeat_bonuses`，不吃重复加成）。
- 另有 `RepeatAreaOfEffect INC`（重复时 AoE 递增，如某些 cascade）。
- **Seal / 蓄力重复**（Plexus 类）走 `SealCooldown` / `SealMax` / `SealRepeatPenalty` 一套独立逻辑，把重复折成 DPS more。

## 引导 (Channelling) 与持续 (Sustained)

- **引导技** = `SkillType.Channel`。`channelTimeMultiplier` / `minChannelTime` / `channelTimeOverride` 决定每次"释放"的引导时间：`ChannelTime = max(channelTimeMultiplier / channelSpeed, minChannelTime)`。`skillFlags.channelRelease` 表示引导到点后释放的击中。
- 引导技结束**可不进冷却**（`CooldownDoesNotLimitSkillSpeed`），使引导速度本身决定节奏。
- **暴击独立滚动**：引导/持续技按**每次击中/每引导间隔**独立滚暴击阈值（不是整次技能滚一次，且不算 Rerolling）——见 [critical-hits.md](critical-hits.md) §暴击检定。
- **持续 (Sustained) 技能**同理按击中独立结算（暴击、命中各自判定）。

## 等级与品质对功能的影响

- **等级**：除基础伤害外，宝石等级会改 `level.duration` / `level.cooldown` / `level.cost` / `reservationMultiplier` / `spiritReservationFlat` 等**功能字段**（`CalcActiveSkill.lua` 读 `level.*` 写入 `skillData` / 注入 mod）。
- **品质**：0.5.0 起所有宝石都有"额外品质属性"（按住 Alt 查看），品质可加成功能面（如额外投射物、AoE、持续）——具体效果按宝石。详见 [gems.md](gems.md)（不重复）。

## 0.5.0 / PoE2 关键变化小结

- 投射物行为**严格优先级 Split→Pierce→Fork→Chain**，一次碰撞只触发一个；能穿透/分叉则不从敌人连锁（但可从地形连锁）。这与 PoE1 的处理不同，是 PoE2 投射物建模的核心差异[^poe2wiki-chain]。
- 投射物连锁距离 6m、其它效果 4m；Split 半径 6m（早期 popup 误写 4m，已修）[^poe2wiki-split]。
- AoE 修饰词作用于**面积**，半径按 √areaMod 缩放，无显式 DR 曲线[^poe2-forum-aoe2]。
- 同组投射物每目标只命中一次（除非 Merge/特例）。
- 冷却与持续都向上取整到服务器帧（≈30.3/s）；可储存多次使用的冷却**不取整**。

## 对 pobr 实现的启示

对照当前 `pobr-core`（`calc/offence.rs`、`config.rs::CalcConfig`、`mod_db.rs`），要支持功能面需补：

**CalcConfig（flags / skill_types / tags）**
- `skill_types`：补齐 `SkillType` 枚举映射（Projectile/Area/Duration/Channel/Multicastable/Cascadable/HasReservation/ReservationBecomesCost/Cooldown/Warcry/Mark...），让带 `SkillType` 条件的修饰词正确 `matches`。
- flags（投射物）：`CannotPierce` / `PierceAllTargets` / `CannotFork` / `ForkOnce` / `ForkTwice` / `CannotChain` / `CannotSplit` / `NoAdditionalProjectiles` / `AdditionalProjectilesAddSplitsInstead` / `AdditionalProjectilesAddChainsInstead` / `ReturningProjectile`。
- flags（消耗/冷却）：`HasNoCost` / `CostLifeInsteadOfMana` / `AttackSpeedScalesCost` / `NoCooldownRecoveryInDuration` / `CooldownDoesNotLimitSkillSpeed`。
- flags（重复）：`CannotRepeat` / `NoRepeatBonuses`。
- KeywordFlag：`Arrow` / `Aura`（区分弓箭/光环专属计数）。

**ModName（新增稳定 ID，沿用 PoB2 命名）**
- 投射物：`ProjectileCount`(BASE/MORE)、`PierceCount`/`PierceChance`、`ChainCountMax`/`ChainCount`/`BeamChainCountMax`/`TerrainChainChance`、`ForkCountMax`、`SplitCount`、`ProjectileSpeed`、`TwoAdditionalProjectilesChance`/`SurpassingProjectileChance`、`BounceCount`。
- 范围：`AreaOfEffect`(INC/MORE，配 Primary/Secondary/Tertiary)、`RepeatAreaOfEffect`。
- 持续：`Duration`/`PrimaryDuration`/`SecondaryDuration`/`TertiaryDuration`/`DamagingAilmentDuration`/`MineDuration`。
- 冷却：`CooldownRecovery`(BASE/INC/MORE/Override)、`AdditionalCooldownUses`、`CooldownChanceNotConsume`。
- 消耗/保留：`ManaCost`(INC/MORE)、`ManaCostNoMult`、`ManaCostEfficiency`、`LifeCost`、`Cost`(MORE=全免)、`BaseManaCostAsLifeCost`、`ManaCostAsEnergyShieldCost`、`HybridManaAndLifeCost_Life`、`ReservationMultiplier`、`ExtraSpirit`。
- 重复：`RepeatCount`、`RepeatOne/Two/ThreeDamage`(MORE)。

**计算公式（确定性，落在 perform 阶段）**
- AoE 半径：`radius = floor(baseRadius × floor(100×√areaMod)/100)`，`areaMod = (1+Σinc/100)×Π(1+more/100)`；breakdown 给出最近 0.1m 台阶（对应 pobr 的 TraceGraph 归因价值）。
- 投射物数：`(ΣBASE + 2×TwoAddl%/100 + Surpassing%/100) × More`。
- 持续/冷却：`base × durationMod` / `cooldownBase / mod`，再 `ceil(x × ServerTickRate)/ServerTickRate`（可储存多次使用的冷却跳过取整）。
- 消耗：`floor(baseCost × costMult + NoMult)`，再走 INC（注意 `base_mana_cost_-%` 是 INC）；Soul 不受通用成本倍乘。

**归因（TraceGraph）**：投射物数量/连锁数/AoE 半径/冷却/消耗的每个修饰词都应能回溯到 `SourceId`（宝石/辅助/天赋/装备），这正是 pobr 相对 PoB 的增量价值。

---

## 参考来源

[^poe2wiki-projectile]: PoE2 Wiki — Projectile（行为优先级 Split/Pierce/Fork/Chain、同组每目标一次、Split/Chain 距离、AOE 投射物）。https://www.poe2wiki.net/wiki/Projectile
[^poe2wiki-chain]: PoE2 Wiki — Chain（连锁最后结算、6m vs 4m 距离、穿透/分叉的投射物不从敌人连锁、地形连锁独立）。https://www.poe2wiki.net/wiki/Chain
[^poe2wiki-split]: PoE2 Wiki — Split（首次命中分裂、6m、共享飞行距离与已命中目标限制）。https://www.poe2wiki.net/wiki/Split
[^poe2db-pierce]: PoE2DB — Pierce（穿透/返回相关词条，如 "Return if they Pierced at least N times"）。https://poe2db.tw/us/Pierce
[^poe2-forum-aoe1]: Path of Exile 官方论坛 — Magnified Effect / AoE radius 数学（面积 +X% → 半径 ×√(1+X)）。https://www.pathofexile.com/forum/view-thread/3637166
[^poe2-forum-aoe2]: Path of Exile 官方论坛 — AoE radius passives 计算（A=πr²，半径按 √areaMod 缩放，无显式 DR）。https://www.pathofexile.com/forum/view-thread/3672838
[^pob2-calcactiveskill]: PathOfBuilding-PoE2 — `src/Modules/CalcActiveSkill.lua`（技能旗标初始化、等级字段、保留倍乘）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcActiveSkill.lua
[^pob2-calcoffence]: PathOfBuilding-PoE2 — `src/Modules/CalcOffence.lua`（calcRadius / 投射物计数 / Chain/Fork/Split/Pierce / Duration / calcSkillCooldown / 消耗）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcOffence.lua
[^pob2-skillstatmap]: PathOfBuilding-PoE2 — `src/Data/SkillStatMap.lua`（词条→ModName 映射：ProjectileCount/PierceCount/ChainCountMax/ForkCountMax/SplitCount/AreaOfEffect/Duration/CooldownRecovery/ManaCost/RepeatCount 等）。
[^pob2-costs]: PathOfBuilding-PoE2 — `src/Data/Costs.lua`（消耗资源类型与 Divisor）。
[^pob2-global]: PathOfBuilding-PoE2 — `src/Data/Global.lua`（`SkillType` 枚举）。
</content>
</invoke>
