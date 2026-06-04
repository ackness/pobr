# 异常状态 (Ailments)

异常状态是 Path of Exile 2 中的一种 debuff 类型，可以对角色或怪物造成持续伤害或施加负面状态效果[^mobalytics-ailments]。玩家和怪物都可以根据伤害类型和施加异常状态的几率来施加异常状态[^poe2wiki-ailment]。

## 异常状态类型

### 造成持续伤害的异常状态 (Damaging Ailments)

| 异常状态 | 伤害类型 | 默认持续时间 | 基础强度 |
|---------|---------|------------|---------|
| **流血 (Bleeding)** | 物理 | 5 秒 | 造成该流血的击中的未减免物理伤害的 15%/秒 |
| **毒 (Poison)** | 混沌 | 2 秒 | 造成该毒的击中的未减免物理伤害的 20%/秒 |
| **点燃 (Ignite)** | 火焰 | 4 秒 | 造成该点燃的击中的火焰伤害的 20%/秒 |

**注**：
- 流血和毒默认**绕过能量护盾 (Energy Shield)**
- 流血在目标移动或流血为**加重 (Aggravated)** 时造成额外 100% 伤害
- 默认情况下只能对敌人施加**一个毒**，但可通过修饰词或技能实现**堆叠 (PoisonCanStack)**[^pob2-deepwiki-ailments]

> **0.5.0 更新**：玩家身上的流血不再在玩家移动时增加伤害。此更改**不影响**玩家施加给怪物的流血[^maxroll-050-patchnotes]。

### 非伤害性异常状态 (Non-Damaging Ailments)

| 异常状态 | 效果 | 默认持续时间（玩家/非玩家） |
|---------|------|--------------------------|
| **冰冷 (Chill)** | 减缓目标行动速度 | 2 秒 / 8 秒 |
| **冻结 (Freeze)** | 使目标无法移动或行动 | 4 秒（通用） |
| **感电 (Shock)** | 使目标受到的伤害增加 20% | 4 秒 / 8 秒 |
| **电击 (Electrocute)** | 打断目标动作并阻止其执行任何动作 | 5 秒（通用） |

## 施加异常状态

异常状态施加的几率取决于攻击者造成的特定类型伤害量与防御者的**异常阈值 (Ailment Threshold)** 的比较。

**异常阈值**可以被视为防御异常状态的"元素护甲"。一般来说，击中越重，施加异常状态的几率越大，施加的异常状态 magnitude 也越大。

### 不同异常状态的特殊规则

#### 流血 (Bleeding) 和 毒 (Poison)
- 需要**几率施加该异常状态**
- 流血：默认伤害来源必须是物理伤害
- 毒：默认伤害来源必须是物理或混沌伤害
- 即使造成了巨大的物理击中，如果流血几率为 0%，也不会施加流血
- 可以通过技能宝石、辅助宝石、被动技能树或物品提升几率

#### 冻结 (Freeze) 和 电击 (Electrocute)
- 使用**积累 (Buildup)** 机制
- 在 POE2 中，冻结和电击属于**姿态 (Poise)** 相关 debuff，与重眩晕 (Heavy Stun) 和钉刺 (Pin) 同属一类[^pob2-deepwiki-ailments]
- 持续对怪物造成正确类型的伤害会积累这些异常状态
- 积累达到 100% 时，异常状态被施加
- 可以通过被动树、装备和宝石提升积累速度

#### 点燃 (Ignite) 和 感电 (Shock)
- 由对应元素伤害的击中直接施加
- 几率基于伤害大小与异常阈值的比较

### 提升异常状态的方法

1. **增加施加几率**：被动树、宝石、装备
2. **增加强度 (Magnitude)**：被动树、物品、辅助宝石。这是不增加伤害就能施加更强异常状态的另一种方式
3. **增加伤害**：更高的伤害 = 更高的施加几率和 magnitude

## 元素异常状态

| 元素 | 异常状态 |
|------|---------|
| 火焰 | 点燃 (Ignite) |
| 冰霜 | 冰冷 (Chill)、冻结 (Freeze) |
| 闪电 | 感电 (Shock)、电击 (Electrocute) |

## 非元素异常状态

| 伤害类型 | 异常状态 |
|---------|---------|
| 物理 | 流血 (Bleeding) |
| 物理 / 混沌 | 毒 (Poison) |

**例外情况**：
某些独特物品可以改变异常状态的施加规则：
- `Voltaxic Rift`：允许混沌伤害对感电几率有贡献
- `Blood Barbs`：允许血法师用元素伤害施加流血

## 异常状态与伤害计算

Damaging ailment 的 magnitude 是基于击中的**未减免伤害 (Pre-mitigation Damage)** 计算的。这意味着：
- 即使目标的防御减免了击中伤害
- 造成的 damaging ailment 仍然基于原始伤害量

## 相关机制

- **元素异常阈值 (Elemental Ailment Threshold)**：防御元素异常状态的能力
- **抗性 (Resistances)**：减轻元素伤害和异常状态效果
- **能量护盾 (Energy Shield)**：流血和毒绕过此层
- **伤害类型 (Damage Types)**：决定可以施加哪些异常状态

---

## 参考来源

[^mobalytics-ailments]: Mobalytics — PoE 2 Ailments Explained. https://mobalytics.gg/poe-2/guides/ailments
[^poe2wiki-ailment]: PoE2 Wiki — Ailment. https://www.poe2wiki.net/wiki/Ailment
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-ailments]: Path of Building for PoE2 DeepWiki — CalcOffence / Ailment Calculations. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
