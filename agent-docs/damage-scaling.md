# 伤害缩放 (Damage Scaling)

伤害缩放是把"基础伤害"放大到"最终击中/持续伤害"的全过程。本文档只补充那些**细致、容易被忽略**的概念，并对每个关键结论给出 PoB2 真实旗标名作为回归基准。

> 总体计算顺序（避免击中 → 伤害计算 → 承受为 → 减伤 → 承受倍率 → 眩晕/格挡 → 资源损失）已在 [`damage-defence-order.md`](./damage-defence-order.md) 覆盖；5 种伤害类型的属性关联与抗性见 [`damage-types.md`](./damage-types.md)；暴击倍率（爆伤）、幸运暴击、双倍/三倍 on Crit 见 [`critical-hits.md`](./critical-hits.md)。本文聚焦**伤害包的构建与放大**这一段（即 `damage-defence-order.md` 步骤 2.1–2.6 的内部细节），尽量不重复。

## 核心叠加语义：added (Base) / increased / more

PoE2 的标准伤害管线对**每个伤害类型**独立结算（PoB2 `CalcOffence.lua::calcDamage`）：

```
最终该类型伤害 = (基础 + Σadded) × (1 + Σincreased/100) × Π(1 + more/100)
```

- **added / flat（PoB2 `BASE`）**：所有"+N to X Damage""Adds N to M X Damage"求和，构成基础伤害包的加项。
- **increased / reduced（PoB2 `INC`）**：**同区加法叠加**——`80% increased` 与 `20% reduced` = 净 `+60%`，整体作为一个 `(1 + 0.6)` 乘区。
- **more / less（PoB2 `MORE`）**：**彼此独立乘区连乘**——两个 `30% more` = `1.3 × 1.3 = 1.69`，而不是 `1.6`。这是 more 强于 increased 的根本原因。

PoB2 实现（`calcDamage`，核对基准）：

```lua
local inc  = 1 + skillModList:Sum("INC", cfg, unpack(modNames)) / 100   -- 同区求和
local more = skillModList:More(cfg, unpack(modNames))                   -- 连乘
return round(summedMin * inc * more + addMin), round(summedMax * inc * more + addMax)
```

> 注意 `modNames` 是按 typeFlags 展开的一组名字（如 `{"Damage","PhysicalDamage"}` 或元素时加上 `"ElementalDamage"`）：一条"increased Damage"、一条"increased Physical Damage"、一条"increased Elemental Damage"对火焰击中会**全部命中同一个 `Sum("INC")` 加法桶**。这正是 pobr `ModDb::sum`/`more` 需要复刻的"按多个 ModName 同时聚合"行为。

## Added Damage Effectiveness（附加伤害效率）

技能对**附加伤害（来自装备/天赋/辅助的 flat added damage）的吸收效率**，PoE1 叫 "damage effectiveness"。

- 它**不影响**技能宝石自带的基础伤害，只缩放外部附加的 flat 伤害。例如某技能 "Damage Effectiveness 150%"，戒指上的 `+10 to Fire Damage` 实际按 `15` 计入基础包。
- 在 PoB2 中它**不是**独立步骤，而是被建模为一条作用于附加伤害的 `MORE` 修饰词，名字叫 **`AddedDamage`**（见 `SkillStatMap.lua`：`added_damage_+%_final` / `active_skill_added_damage_+%_final` → `mod("AddedDamage","MORE")`；`Added<Type>Damage` 为分类型版本）。

PoB2 在构建基础包时（`CalcOffence.lua` 基础击中段）：

```lua
local addedMult = calcLib.mod(skillModList, cfg, "Added"..damageType.."Damage", "AddedDamage")
local baseMin = ((source[damageTypeMin] or 0) + bonus + (addedMin * addedMult)) * baseMultiplier
```

- `source[...]` = 武器/技能自带基础；`addedMin * addedMult` = 外部 flat × 效率；外层还有 `baseMultiplier`（技能等级的 `baseMultiplier`，对**整个**基础包再乘）。
- 关键差异：**效率只乘 `addedMin/addedMax`（外部附加），不乘 `source`（武器/技能自带）**。pobr 若把"效率"实现成对整个 base 的 More 就会高估自带伤害。

## 伤害转换 (Damage Conversion)

转换把一部分某类型基础伤害"变成"另一类型，**沿固定顺序链式进行**。

### 转换顺序与链式

PoB2 的类型顺序：`Physical → Lightning → Cold → Fire → Chaos`（`dmgTypeList`）。转换沿此顺序传递，先前类型转出的伤害可被后续类型再次转换（链式）。

转换分两阶段（`CalcOffence.lua` 转换段）：

1. **技能转换 (skill conversion)**：技能宝石自带的转换（`Skill<From>DamageConvertTo<To>`），先结算。
2. **全局转换 (global conversion)**：天赋/装备/增益（`<From>DamageConvertTo<To>` 等），作用于技能转换后剩余的未转换部分**以及**已被技能转换出的部分。

每个来源类型如果转换总和 **> 100%，按比例归一化到 100%**：

```lua
if total > 100 then local factor = 100 / total; ... total = 100 end
```

例：`100% Phys→Fire` + `50% Phys→Cold` → 归一为 `67% Fire / 33% Cold`。

### 转换分量的 increased/more 口径（**PoE2 = 仅最终类型，无转换源 double-dip**）

> **修正（一手来源）**：早期本节据 PoE1 描述为"转换后伤害同时吃来源 + 目标类型 increased（double-dip）"。
> 经 PoB2 源码 + headless oracle 逐分量验证，**PoE2 已移除此机制**：转换分量只吃**最终伤害类型**自身的
> increased/more（外加 Elemental，若为元素）。依据：`CalcOffence.lua` `calcDamage(activeSkill, output, cfg,
> …, damageType, **0**)`（:3990）——`typeFlags` 传入 **0**，函数内 `typeFlags = bor(0, dmgTypeFlags.flags[damageType])`
> 只含被计算的最终 `damageType`，转换源类型的 flag **不**累加。转换本身（base 在类型间搬运）已预折进
> `output[damageType.."SummedBase"]`，`calcDamage` 仅按最终类型缩放该 base。

物理转火焰后，火焰分量只吃 "increased Fire Damage" + "increased Elemental Damage"，**不**吃 "increased Physical
Damage"。pobr 实现见 `crates/pobr-core/src/calc/damage.rs::aggregate_inc_more`（只按 `type_path` 末位最终类型聚合）。
`type_path` 仍保留转换沿途类型集合，但**仅用于归因/展示**，不参与 inc/more 聚合。

### Gain #% as Extra（额外伤害包，**不是转换**）

"Gain #% of X Damage as Extra Y Damage" 与转换的关键区别：

| | 转换 (Convert) | 额外获得 (Gain as Extra) |
|---|---|---|
| 来源伤害是否减少 | **是**（转出部分从原类型扣除） | **否**（原类型伤害不变） |
| 产生的新伤害 | 替换原伤害 | **额外叠加**一份新伤害包 |
| increased 生效 | **仅最终类型**（PoE2 无源双重） | **仅最终类型**（gained 包按目标类型 increased，PoB2 L3990 同口径） |

PoB2 用独立的 `gainTable`（`buildGainTable`）与 `calcGainedDamage` 处理，旗标名形如 `<From>DamageGainAs<To>` / `DamageGainAs<To>` / `ElementalDamageGainAs<To>` / `SkillDamageGainAs<To>`（BASE，单位 %）。注意 `gainTable` 的来源是"**转换后**的伤害"——gain 计算里会先 `calcConvertedDamage` 再乘 gain 系数，所以 extra 包是在转换链之后追加的。

> 直觉记法：转换"搬走"伤害（从原类型扣除）；gain as extra "复制一份"伤害且不动原始包。**两者的 increased 都只按最终/目标类型生效（PoE2，无 PoE1 的转换源 double-dip）**；gain 不受"转换总和归一化到 100%"约束。

## 幸运 / 不幸伤害 (Lucky / Unlucky Damage)

伤害在 min–max 之间掷骰确定数值；幸运掷两次取**高**，不幸掷两次取**低**，二者相互抵消（与暴击的 Lucky 同理，见 `critical-hits.md`）。

PoB2 用**期望值**建模（`CalcOffence.lua`）：

```lua
damageTypeHitAvgNotLucky = damageTypeHitMin / 2 + damageTypeHitMax / 2          -- 普通：均匀分布均值
damageTypeHitAvgLucky    = damageTypeHitMin / 3 + 2 * damageTypeHitMax / 3      -- 幸运：max(两次均匀)的均值
damageTypeHitAvg = AvgNotLucky * (1 - luckyChance) + AvgLucky * luckyChance
```

- 关键数字：均匀分布两次取大的期望 = `min/3 + 2·max/3`（偏向最大值）。
- 旗标：`LuckyHits`（全幸运）、`<Type>LuckyHitsChance` / `LuckyHitsChance`（按概率部分幸运）、`CritLucky`、`LightningNoCritLucky`（如 `Voltaxic`/特定闪电"非暴击伤害 Lucky"）。

## 双倍 / 三倍伤害 (Double / Triple Damage)

按概率把整次击中伤害 ×2 / ×3。PoB2 折成一个期望乘数 `ScaledDamageEffect`（`CalcOffence.lua`）：

```lua
output.TripleDamageEffect = 2 * TripleDamageChance / 100   -- 三倍 = +2 份额外
output.DoubleDamageEffect = DoubleDamageChance / 100       -- 双倍 = +1 份额外
-- 三倍覆盖双倍：先扣掉两者同时发生的概率，归给三倍
if TripleDamageChance > 0 then
    DoubleDamageChance = max(DoubleDamageChance - TripleDamageChance * DoubleDamageChance / 100, 0)
end
output.ScaledDamageEffect = ScaledDamageEffect * (1 + DoubleDamageEffect + TripleDamageEffect)
```

- "三倍覆盖双倍"不是简单互斥，而是**从双倍概率里减去重叠概率** `P(triple)·P(double)`，避免双重计数。
- on Crit 版本：`DoubleDamageChanceOnCrit`/`TripleDamageChanceOnCrit` 会折算成 `chanceOnCrit × CritChance/100` 加进总概率（细节见 `critical-hits.md`）。
- 都先 `m_min(..., 100)` 截到 100%。

## Overwhelm（PoE2 物理版"无视减免"）

**Overwhelm = 使物理击中无视目标一定百分比的物理伤害减免 (Physical Damage Reduction, PDR)**。它是物理伤害的"穿透等价物"，但作用对象是 PDR（护甲贡献 + 额外 PDR）而非抗性。

- **不能把 PDR 压到 0% 以下**（与穿透同理）。能把 PDR 打成负的只有 **Armour Break**（护甲破坏），Overwhelm 不行[^poe2wiki-physical][^mobalytics-armour]。
- PoB2 把玩家侧 "Overwhelm N% physical damage reduction" 直接映射成 **`EnemyPhysicalDamageReduction` BASE = −N**（`ModParser.lua`），即在结算 PDR 时把敌人 PDR 直接减 N，再 clamp：

```lua
resist = m_min(m_max(-NegArmourDmgBonusCap,
        enemyPDR + EnemyPhysicalDamageReduction + armourReduction*(1-chanceIgnoreArmour)),
        EnemyPhysicalDamageReductionCap)   -- 物理 resist 即 PDR
```

  另有 `ChanceToIgnoreEnemyPhysicalDamageReduction`（几率完全无视）、`PartialIgnoreEnemyPhysicalDamageReduction`（按比例无视）、`IgnoreEnemyPhysicalDamageReduction`（flag）。
- 上限来自游戏数据 `EnemyPhysicalDamageReductionCap = monsterConstants["maximum_physical_damage_reduction_%"]`；下限 `−NegArmourDmgBonusCap`（护甲为负时给攻击者的增伤上限）。

### Overwhelm / Penetration / Exposure / 负抗 / Armour Break 的区别

| 机制 | 作用对象 | 能否压到 0% 以下 | 适用伤害 |
|---|---|---|---|
| **Overwhelm** | 物理 PDR | 否 | 仅物理击中 |
| **Penetration** | 元素/混沌抗性 | 否（除极少数如 Leopold's Applause 到 −50%） | 仅对应类型击中 |
| **Exposure / 诅咒 (Elemental Weakness)** | 抗性（debuff 持续在目标上） | **是**（可到负抗） | 击中与持续都受益 |
| **Armour Break** | 护甲 → PDR | **是**（可负） | 物理 |

- 穿透与"降低抗性/负抗"**互斥**：穿透在 debuff 之后结算，抗性已 ≤0 时穿透全浪费[^mobalytics-pen]。pobr 公式 `m_max(resist - pen, minPen)` 已体现（见 `damage-defence-order.md` 步骤 4）。
- 穿透**只作用于击中**，不作用于持续伤害；异常 magnitude 基于减免前伤害，因此也不吃穿透[^mobalytics-pen]。

## 命中伤害 vs 持续伤害 (Hit vs DoT) 的缩放归属

同一条 modifier 可能只对其中一种生效，**ModName/标签必须区分**：

- 仅命中：`increased Damage with Hits`、`Penetration`、`Overwhelm`、爆伤（默认）、双倍/三倍伤害、added flat（flat 只进基础击中包）。
- 仅持续：`increased Damage over Time`、`<Ailment> Magnitude`、`Faster <Ailment>`（更快结算）等。
- 两者都吃：`increased <Type> Damage`、`increased Elemental Damage`（既算击中也常算 DoT，视具体词条）。
- **min/max 取值**：击中在 min–max 间掷骰（受 Lucky 影响）；DoT 通常取**确定的每秒值**（不掷骰、不暴击），其基础常按"造成它的那次击中伤害的某百分比"（如流血 = 击中前物理的 15%/秒）计算且**不再吃额外的伤害 modifier**[^poe2wiki-physical]。PoB2 DoT 走独立分支（`<Type>Dot`、`baseVal = skillData[damageType.."Dot"]`）。

## Culling Strike（斩杀）

当目标生命降到斩杀阈值以下时立即击杀。PoB2（`CalcOffence.lua`）：

```lua
regularCull  = Override("CullPercent") or (Flag("CanCull")    and gameConstants["CullingStrike"..rarity.."Threshold"] or 0)
criticalCull = Override("CriticalCullPercent") or (Flag("CritCanCull") and gameConstants["CullingStrike"..rarity.."Threshold"] or 0)
maxCullPercent = max(criticalCull, regularCull) * (1 + Sum("INC","CullPercent")/100)
CullMultiplier = 100 / (100 - CullPercent)   -- 折成等效 DPS 倍率
```

- **PoB2 的斩杀阈值按敌人稀有度区分**（`Misc.lua`，0.5.0）：Normal **35%**、Magic **20%**、Rare **10%**、Unique **5%**。`CanCull`/`CritCanCull` 是开关 flag；`CullPercent`/`CriticalCullPercent` 可被 `Override` 直接指定阈值；`increased Cull Threshold` 进 `INC CullPercent`。
- 斩杀按"等效 DPS 倍率"计入工具提示（提前结束战斗）。

## 特殊词条与伤害标签

- **`Deals no X damage`**：PoB2 用 flag `DealNo<Type>` / `DealNoDamage`，在算转换前就把该类型禁用（`canDeal[type] = not Flag("DealNo"..type)`）。如 Brutality 的"deal no chaos/elemental damage"。
- **damage 标签（近战/投射物/范围/attack/spell）**：决定一条 modifier 是否适用，对应 PoB2 的 `ModFlag`（`Melee`/`Projectile`/`Area`/`Attack`/`Spell`/`Hit`…）与 `KeywordFlag`，以及 `SkillType`/`Condition` 标签。例如 "increased Projectile Damage" 只在 `skillFlags.projectile` 时进聚合；敌人侧 `ProjectileDamageTaken` 也按同标签叠加到 `takenInc`。这正对应 pobr `Modifier.flags` / `keyword_flags` / `tags` 与 `matches(cfg)`。
- **"extra damage as"**：即上文 Gain as Extra，注意区别于"伤害承受为 (Damage Taken As)"（防御侧伤害转移，见 `damage-defence-order.md` 步骤 3）。

## 0.5.0 相关变化

- "Defences" 关键词废弃，明确改为 "Armour, Evasion and Energy Shield"（影响哪些词条适用于 Runic Ward；见 `damage-defence-order.md`）[^maxroll-050]。
- 护甲/闪避数值曲线在 65 级后整体上调（间接影响 Overwhelm/Armour Break 的相对价值）[^maxroll-050]。
- 转换链/伤害效率/双三倍/Overwhelm 的核心公式在 0.5.0 未见结构性改动（以 PoB2 `dev` 分支为准）。

## 对 pobr 实现的启示

对照 `pobr-core`（`ModDb` 的 Base/Inc/More 聚合 + `calc/offence.rs::scaled_numeric_stat`）：

1. **聚合按"一组 ModName 同时求和"**：`calcDamage` 对火焰击中同时聚合 `Damage`/`FireDamage`/`ElementalDamage`。pobr 的 `sum(Inc, ...)` / `more(...)` 应支持**传入多个 ModName 一次性聚合**到同一加法桶/乘区，而非逐个查。
2. **Damage Effectiveness = 作用于"外部 added flat"的 More**：新增 `ModName::AddedDamage` / `Added<Type>Damage`（More 语义），**只乘 added 部分，不乘武器/技能自带 base**；技能等级的 `baseMultiplier` 才乘整个基础包。
3. **转换链需保留"沿途类型标签"**：实现 `conversionTable` 时把来源类型 flag 累积进目标包的 tag 集，聚合 increased 时按 flag 集合命中——这样自动得到"increased 双重生效"。先技能转换、再全局转换，单来源 >100% 归一化。
4. **Gain as Extra 独立于转换**：单独的 `gainTable`（`DamageGainAs<To>` 等 Base%），基于转换后伤害追加新包，不扣减来源、不参与 100% 归一。
5. **Lucky/Unlucky 用期望值**：幸运均值 `min/3 + 2·max/3`，普通 `(min+max)/2`，按概率混合；新增 flag `LuckyHits` / ModName `LuckyHitsChance`，与暴击 Lucky 共用同一抵消语义。
6. **Double/Triple 折成 `ScaledDamageEffect`**：`1 + double + 2·triple`，并先做 `double -= triple·double/100` 去重叠；on-crit 版本乘 `CritChance`。
7. **Overwhelm 建模为 `EnemyPhysicalDamageReduction` 的负 Base**（敌方侧），与 `EnemyPhysicalPenetration`/`ChanceToIgnore...`/`PartialIgnore...` 并列；统一 clamp 到 `[−NegArmourCap, PhysReductionCap]`。区分穿透（抗性，`m_max(resist−pen, 0)`）与 Overwhelm（PDR），二者都"不破 0"。
8. **Hit vs DoT 用 flags/tags 隔离**：`ModFlag::Hit` vs DoT 专用 ModName；DoT 不掷骰、不暴击、不吃穿透，走独立路径。
9. **Culling 阈值按稀有度 + `INC CullPercent`**：阈值常量入 `pobr-data`（Normal 35/Magic 20/Rare 10/Unique 5），`CanCull`/`CritCanCull` 作 flag，折成 `CullMultiplier = 100/(100−cull%)`。
10. **归因价值**：每个 increased/more/转换/extra/Overwhelm 贡献都应能经 `TraceGraph` 回溯到 `SourceId`，这正是 pobr 相对 PoB 的增量——尤其转换链让"一条物理 increased 影响了火焰输出"这种跨类型贡献可被显式展示。

---

## 参考来源

[^poe2wiki-physical]: PoE2 Wiki — Physical（含 Overwhelm 直接无视 PDR、不破 0；Armour Break 可破 0；Bleeding magnitude 取击中前物理 15%/秒）。https://www.poe2wiki.net/wiki/Physical
[^mobalytics-armour]: Mobalytics — PoE 2 Guide: Armour Explained（Overwhelm 定义、护甲公式、Overwhelm 不能突破 0% PDR）。https://mobalytics.gg/poe-2/guides/armour
[^mobalytics-pen]: Mobalytics — PoE 2 Guide: Penetration Explained（穿透不破 0%、只作用击中、与负抗互斥、Leopold's Applause 例外）。https://mobalytics.gg/poe-2/guides/penetration
[^mobalytics-order]: Mobalytics — Damage & Defence Calculation Order（转换、Gain as Extra、Damage Taken As、减伤层）。https://mobalytics.gg/poe-2/guides/damage-defence-calc-order
[^maxroll-050]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients（Defences 关键词废弃、护甲/闪避曲线调整）。https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-calcoffence]: PathOfBuildingCommunity/PathOfBuilding-PoE2 — `src/Modules/CalcOffence.lua`（`calcDamage` 叠加、转换链 `conversionTable`、`buildGainTable`/`calcGainedDamage`、Lucky 期望、`Double/TripleDamageEffect`、`ScaledDamageEffect`、Overwhelm/PDR 段、Culling）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcOffence.lua
[^pob2-skillstatmap]: PathOfBuilding-PoE2 — `src/Data/SkillStatMap.lua`（`added_damage_+%_final` → `mod("AddedDamage","MORE")`，damage effectiveness 映射）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/SkillStatMap.lua
[^pob2-modparser]: PathOfBuilding-PoE2 — `src/Modules/ModParser.lua`（"overwhelm N% physical damage reduction" → `EnemyPhysicalDamageReduction BASE −N`；`ChanceToIgnore/PartialIgnoreEnemyPhysicalDamageReduction`；`DealNo<Type>`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/ModParser.lua
[^pob2-misc]: PathOfBuilding-PoE2 — `src/Data/Misc.lua`（`CullingStrike{Normal35,Magic20,Rare10,Unique5}Threshold`）；`src/Modules/Data.lua`（`EnemyPhysicalDamageReductionCap = monsterConstants["maximum_physical_damage_reduction_%"]`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Misc.lua
