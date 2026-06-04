# 伤害类型 (Damage Types)

Path of Exile 2 中有 5 种不同的伤害类型[^mobalytics-damage-types]。每种伤害类型与不同的防御层交互方式不同，并且与特定的属性有关联[^poe2db-damage-types]。

## 概述

- **物理 (Physical)**
- **火焰 (Fire)**
- **冰霜 (Cold)**
- **闪电 (Lightning)**
- **混沌 (Chaos)**

火焰、冰霜和闪电伤害统称为**元素伤害 (Elemental Damage)**。

## 物理伤害 (Physical Damage)

### 攻击
物理伤害通常是攻击类技能的主要伤害类型，例如 `Boneshatter`。如果攻击技能没有任何固有的伤害转换，它将使用武器上的基础伤害值以及来自其他装备（如戒指）或"额外获得伤害"类修饰词的附加伤害。大多数武器都有基础物理伤害和本地修饰词来增强该物理伤害[^mobalytics-damage-types]。

### 法术
物理伤害不仅限于攻击——也有造成物理伤害的法术。纯物理伤害法术通常拥有较高的基础暴击率，例如 `Bonestorm` 的基础暴击率可为 15%（法术默认基础暴击率为 15%，特定技能可能覆盖此值）[^mobalytics-damage-types][^pob2-deepwiki-crit]。

### 减伤方式
物理伤害没有对应的抗性用于减伤，而是使用**护甲 (Armour)** 和**额外物理伤害减免 (Additional Physical Damage Reduction)** 作为主要减伤手段[^mobalytics-armour]。

### 相关异常状态
- **流血 (Bleeding)**：造成物理持续伤害的 damaging ailment
- **腐化之血 (Corrupted Blood)**：类似的 debuff，也造成物理持续伤害
- 对实体造成的物理持续伤害会产生**失血 (Blood Loss)**
- **毒 (Poison)** 虽然可以由物理伤害的击中触发，但本身不造成物理伤害

## 火焰伤害 (Fire Damage)

### 属性关联
火焰伤害与**力量 (Strength)** 属性相关联[^mobalytics-damage-types]。许多钉头锤攻击技能有火焰主题，如 `Molten Blast` 或 `Perfect Strike`。这些技能通常有将部分物理伤害转换为火焰伤害的机制。

### 法术
火焰主题法术的基础暴击率取决于具体技能定义。某些火焰法术（如 `Fireball`）可能被定义为较低的基础暴击率（如 7%），但这并非火焰法术的普遍规律——法术默认基础暴击率为 15%[^mobalytics-damage-types][^pob2-deepwiki-crit]。

### 减伤方式
**火焰抗性 (Fire Resistance)** 是减免火焰伤害的主要方法。可以在被动技能树的左侧，尤其是力量区域找到火焰防御主题的被动技能。例如 `Unnatural Resilience` 可以通过堆叠火焰抗性来提升最大火焰抗性上限。

### 相关异常状态
- **点燃 (Ignite)**：与火焰伤害绑定的 damaging ailment，造成火焰持续伤害
- 被点燃的实体被视为**燃烧 (Burning)**
- 许多造成火焰持续伤害的效果都归因于点燃

## 冰霜伤害 (Cold Damage)

### 属性关联
冰霜伤害与**智力 (Intelligence)** 属性相关联[^mobalytics-damage-types]。虽然没有纯智力武器类型，但混合智力武器如 `Quarterstaff` 有许多冰霜主题技能，如 `Glacial Cascade`。不过属性关联并非绝对严格——例如长矛（混合力量和敏捷武器）也有冰霜主题技能 `Glacial Lance`。

### 法术
纯物理法术之后，冰霜伤害法术是游戏中基础暴击率第二高的。许多冰霜法术注重大范围控制，使用冰冷区域减缓怪物并创造冰墙阻碍移动。

### 减伤方式
**冰霜抗性 (Cold Resistance)** 是减免冰霜伤害的主要方法。由于非 damaging ailment（如冻结）是在击中伤害被减免后计算的，冰霜抗性和其他减伤层对防止被冻结很重要。此外，**元素异常阈值 (Elemental Ailment Threshold)** 用于计算一次击中是否会在你身上造成冻结，因此增加最大生命或直接提升元素异常阈值（如 `Unbreaking`）是好策略。

### 相关异常状态
- **冰冷 (Chill)**：减缓目标行动速度
- **冻结 (Freeze)**：使目标无法移动或行动

## 闪电伤害 (Lightning Damage)

### 属性关联
闪电伤害通常与**敏捷 (Dexterity)** 属性相关联[^mobalytics-damage-types]。许多闪电主题攻击技能在敏捷对齐的武器上，如弓、长矛和十字弓。例如 `Galvanic Shards` 或 `Lightning Rod`。

### 法术
闪电法术的基础暴击率同样取决于具体技能定义。默认法术基础暴击率为 15%，特定技能可能有覆盖值。

### 减伤方式
**闪电抗性 (Lightning Resistance)** 是减免闪电伤害的主要方法。闪电伤害击中可以施加**感电 (Shock)**，因此堆叠闪电抗性有助于防止被感电。

### 相关异常状态
- **感电 (Shock)**：使目标受到的伤害增加 20%
- **电击 (Electrocute)**：打断目标动作并阻止其执行任何动作。需要特殊修饰词或技能来启用，例如 `Voltaic Grenade` 可以施加电击

## 混沌伤害 (Chaos Damage)

### 属性关联
混沌伤害的属性关联不如元素伤害类型那么明确，但通常会与特定属性相关[^mobalytics-damage-types]。例如，**毒 (Poison)** 是与混沌伤害相关的 damaging ailment。

### 法术
许多混沌主题法术与毒无关，而是造成非毒的混沌持续伤害。这些法术提供与元素法术截然不同的选择——不注重爆发伤害，而是缓慢的大范围持续伤害，可以覆盖大范围并在怪物之间传播。`Essence Drain` 和 `Contagion` 是很好的例子。

### 减伤方式
**混沌抗性 (Chaos Resistance)** 是减免混沌伤害的主要方法。但混沌伤害在与不同资源的交互中有特殊属性：
- 默认情况下，混沌伤害对能量护盾造成**两倍**的伤害
- 可以通过使用 `Chaos Inoculation`（混沌免疫）来完全免疫混沌伤害

### 相关异常状态
- **毒 (Poison)**：可以由物理或混沌伤害的击中触发，造成混沌持续伤害。默认情况下绕过能量护盾直接对生命造成伤害
- **凋零 (Wither)**：使实体从所有来源受到的混沌伤害增加

## 伤害类型的颜色编码与动画

| 伤害类型 | 颜色 | 动画特征 |
|---------|------|---------|
| 物理 | 红色、米色、灰色 | 血旋、岩石、金属 |
| 火焰 | 橙色、黄色 | 火焰、熔岩 |
| 冰霜 | 蓝色、白色 | 冰、水晶、雪花 |
| 闪电 | 黄色、白色 | 分叉闪电、电光 |
| 混沌 | 紫色、绿色（毒） | 发光植物、毒气云 |

注意：虽然颜色编码通常准确，但也存在一些例外情况。

---

## 参考来源

[^mobalytics-damage-types]: Mobalytics — PoE 2 Guide: Damage Types Explained. https://mobalytics.gg/poe-2/guides/damage-types
[^mobalytics-armour]: Mobalytics — PoE 2 Guide: Armour Explained. https://mobalytics.gg/poe-2/guides/armour
[^pob2-deepwiki-crit]: Path of Building for PoE2 DeepWiki — CalcOffence / Base Critical Hit Chance. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^poe2db-damage-types]: PoE2DB — Damage Types. https://poe2db.tw/us/
