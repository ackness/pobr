# 护甲机制 (Armour)

护甲是一种防御属性，默认用于减轻来自**击中 (Hit)** 的物理伤害[^mobalytics-armour]。护甲提供的物理伤害减免不是固定的，而是基于击中伤害大小与总护甲值的比例动态变化的[^poewiki-armour]。

## 基本机制

护甲默认减轻**物理击中伤害**。护甲不 interacts with 持续伤害，不会减轻来自**流血 (Bleeding)** 或**腐化之血 (Corrupted Blood)** 的物理持续伤害。

护甲的设计使其在对抗多个小伤害时更强，而不是少数大伤害，即使总伤害相同。

> **0.5.0 更新**：65 级时护甲数值约增加 33%，80 级以上增加 15%。现有物品的基础护甲会自动调整，护甲修饰词可用神圣宝珠更新[^maxroll-050-patchnotes]。

## 伤害减免公式

```
伤害减免 = AR / (AR + 10 * DMG)
```

> **与 POB2 的核对**：Path of Building for POE2 的 `calcs.armourReductionF`（`CalcDefence.lua`）使用公式 `armour / (armour + raw * data.misc.ArmourRatio) * 100`，其中 `data.misc.ArmourRatio = 10`（`Data.lua` 中定义）[^pob2-deepwiki-armour]。
>
> 对比：旧版 POE1 POB 使用的护甲公式为 `armour / (armour + raw * 5)`。在相同护甲与伤害比例下，POE2 的减免效果约为 POE1 的一半。

其中：
- **AR** = 防御者的总护甲值
- **DMG** = 防御者受到的击中的总物理伤害

护甲最多可以减免其总值五分之一的伤害。例如：
- 一个拥有 20,000 护甲的角色最多可以从单次击中减免 4,000 物理伤害

当击中物理伤害远超总护甲值时，护甲至少能减免其值十分之一的伤害。

### 减免比例参考

| 目标减免率 | 所需护甲 vs 伤害比例 |
|-----------|---------------------|
| 33.3% | 5 倍伤害 |
| 50% | 10 倍伤害 |
| 66.6% | 20 倍伤害 |
| 75% | 30 倍伤害 |
| 90% | 90 倍伤害 |

**物理伤害减免从所有来源硬上限为 90%。**

## 额外物理伤害减免 (Additional Physical Damage Reduction / PDR)

额外物理伤害减免是一种提供固定百分比物理伤害减免的修饰词。多个来源的额外物理伤害减免**加法叠加**。

这些修饰词提供的物理伤害减免直接加到上述护甲计算的百分比上。例如：
- 角色从盾牌获得 8% 额外物理伤害减免
- 拥有 10,000 总护甲
- 受到 1,000 物理伤害的击中
- 护甲单独减免 50%
- 加上 PDR 后总减免为 58%

**与护甲不同，额外物理伤害减免修饰词也会应用于任何物理持续伤害**，如流血或腐化之血的伤害。

## 护甲应用于其他伤害类型

有些修饰词会将护甲应用于非物理伤害类型，主要是元素伤害。这些修饰词最常见于基于力量的护甲装备，如胸甲或手套。

例如，`Heatproofing` 被动技能树 notable 将 30% 的护甲应用于来自击中的火焰伤害。

当护甲被应用于非物理伤害时，其伤害减免计算在**抗性 (Resistances)** 之前应用。

`Blackbraid` 传奇胸甲使护甲应用于所有火焰、冰霜和闪电伤害击中，除标准物理伤害外。

当护甲在同一击中中被应用于多种伤害类型时，伤害减免计算分别应用于每种伤害类型。

## 护甲击破 (Armour Break)

护甲击破是一种攻击机制，暂时从目标移除固定数量的护甲，默认基础持续时间为 **12 秒**。

当移除的护甲足以将目标的护甲值降至零时，护甲被视为**完全击破 (Fully Broken)**。护甲将在 12 秒内保持完全击破状态，或直到被某个技能消耗。

对怪物施加护甲击破时：
- 对普通怪物：击破 3 倍护甲
- 对魔法怪物：击破 2 倍护甲

**Warbringer** 升华职业分支的 `Imploding Impacts` 允许将怪物的护甲击破到零以下（负数）。负数护甲上限为原始总护甲值的相反数。例如，一个拥有 2,000 护甲的怪物可以被降至 -2,000 护甲。负数护甲会使对该目标的物理伤害击中获得伤害倍增器。

## 相关机制

- **闪避 (Evasion)**：另一种主要防御层
- **能量护盾 (Energy Shield)**：资源型防御
- **额外物理伤害减免 (PDR)**：固定百分比减免
- **护甲击破 (Armour Break)**：POE2 新增机制

---

## 参考来源

[^mobalytics-armour]: Mobalytics — PoE 2 Guide: Armour Explained. https://mobalytics.gg/poe-2/guides/armour
[^poewiki-armour]: PoE Wiki — Armour. https://www.poewiki.net/wiki/Armour
[^mobalytics-evasion]: Mobalytics — PoE 2 Guide: Evasion Explained. https://mobalytics.gg/poe-2/guides/evasion
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-armour]: Path of Building for PoE2 DeepWiki — CalcDefence / armourReductionF. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
