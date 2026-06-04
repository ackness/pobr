# 暴击机制 (Critical Hits)

暴击是击中时使伤害获得基于**暴击伤害加成 (Critical Damage Bonus)** 的伤害倍增器的效果[^mobalytics-crit]。暴击的额外伤害不是单独作为一个伤害包处理的，而是原始击中伤害的乘数。

**注意**：持续伤害不能暴击。但由暴击施加的 damaging ailment 的伤害会相应缩放。

## 暴击几率 (Critical Hit Chance)

暴击几率在技能首次使用时滚动。在 0 到 99.99 之间滚动一个随机数，该数字作为必须超过的阈值来确认击中是否为暴击。

例如：使用 `Firestorm` 并随机滚动暴击阈值，结果为 17.4。你必须拥有至少 17.5% 的暴击几率才能使该次技能使用的任何击中成为暴击。如果暴击几率低于 17.5%，则该次技能使用的所有击中都是普通非暴击。

**注意**：某些技能和效果可以在使用时改变暴击行为。

### 特殊滚动规则

- **多投射物技能**：某些技能在每个击中基础上重新滚动暴击阈值（如 `Rain of Arrows`）
- **引导技能**：如 `Incinerate`，在每个间隔或阶段重新滚动暴击阈值
- **怪物**：怪物的多投射物技能仅在技能使用时检查一次，因此要么所有击中都是暴击，要么都不是

### 法术暴击几率

法术技能的基础暴击率来自技能宝石本身。根据 POB2 数据，法术默认基础暴击率为 **15%**，可被特定技能定义覆盖为不同值（如 7% 或 12%）[^pob2-deepwiki-crit]。

### 攻击暴击几率

与法术不同，攻击技能的基础暴击率来自用于执行攻击的装备武器[^mobalytics-crit]。空手攻击默认基础暴击率为 **5%**，其他武器的基础暴击率由武器底材数据定义[^pob2-deepwiki-crit]。例外：
- 副手攻击（如 `Shield Charge`）
- 空手攻击（如 `Shattering Palm`）

这些使用技能宝石本身的基础暴击率，类似于法术。

## 缩放暴击几率

有两种类型的修饰词来缩放暴击几率：

### 倍增器修饰词 (Multiplier Modifiers)

更常见，例如 `Heartstopping`。这些修饰词直接乘以基础暴击几率。

例如：拥有 75% 增加暴击几率，使用基础暴击率 7% 的技能（特定技能可能有覆盖值）：
```
最终暴击率 = 7% * (1 + 0.75) = 12.25%
```

增加和减少是加法叠加的。**更多 (More)** 乘数也存在（如 `Charge Regulation`），它们彼此之间是乘法关系。

### 固定暴击几率修饰词 (Flat Critical Hit Chance Modifiers)

更稀有，因为它们在计算任何倍增器之前将百分比加到基础暴击几率上。

例如：`Struck Through` 被动技能树 notable 为攻击增加 1% 基础暴击率。使用 `Pillar of the Caged God` 传奇长棍时，基础暴击率从 10% 提升到 11%，然后被任何倍增器修饰词相乘。

**因此，固定暴击几率修饰词极其强大**，因为它们使倍增器修饰词在提供暴击几率方面更加高效。

## 暴击伤害加成 (Critical Damage Bonus)

玩家角色默认暴击伤害加成为 **100%**，即有效 200% 的基础伤害（双倍伤害）[^mobalytics-crit]。

防御者可能拥有减少暴击伤害加成的修饰词，如 `Battle-hardened`。

## 幸运与不幸 (Lucky and Unlucky)

- **幸运暴击几率**：暴击阈值随机数滚动两次，使用较低的数字
- **不幸暴击几率**：暴击阈值随机数滚动两次，使用较高的数字

## 对暴击的防御

### 闪避机制

闪避提供额外的防御层来对抗暴击[^mobalytics-evasion]：

当一个暴击成功通过精准和闪避的命中检查计算后，必须通过**二次检查**来确认暴击。此时，在 1 到 100 之间滚动一个随机数（完全独立于熵值机制）。如果攻击者的命中几率高于该随机数，暴击被确认；如果等于或低于，暴击被降级为普通非暴击击中。

**注意**：即使二次检查失败，击中仍然会连接，因为它已经通过了初始检查。

### 阻止暴击

- **Resolute Technique**：完全阻止你造成暴击
- **Sunder**：保证暴击（但被 Resolute Technique 覆盖）

## 相关机制

- **精准 (Accuracy)**：影响命中和暴击检查
- **闪避 (Evasion)**：提供额外的暴击防御层
- **基础暴击率**：由武器或技能宝石决定
- **暴击伤害加成**：默认 100%

---

## 参考来源

[^mobalytics-crit]: Mobalytics — PoE 2 Guide: Critical Hits Explained. https://mobalytics.gg/poe-2/guides/critical-hits
[^mobalytics-evasion]: Mobalytics — PoE 2 Guide: Evasion Explained. https://mobalytics.gg/poe-2/guides/evasion
[^pob2-deepwiki-crit]: Path of Building for PoE2 DeepWiki — CalcOffence / Critical Hit Calculation. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^poe2wiki-crit]: PoE2 Wiki — Critical hit. https://www.poe2wiki.net/wiki/Critical_hit
