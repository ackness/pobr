# 抗性系统 (Resistances)

抗性是 Path of Exile 2 中减轻元素和混沌伤害的主要防御机制之一[^mobalytics-resistances]。每种抗性类型独立计算，可以堆叠到上限以获得最大减伤效果。

## 抗性类型

### 元素抗性 (Elemental Resistances)

| 抗性 | 减轻的伤害类型 | 相关异常状态 |
|------|--------------|------------|
| **火焰抗性 (Fire Resistance)** | 火焰伤害 | 点燃 (Ignite) |
| **冰霜抗性 (Cold Resistance)** | 冰霜伤害 | 冰冷 (Chill)、冻结 (Freeze) |
| **闪电抗性 (Lightning Resistance)** | 闪电伤害 | 感电 (Shock)、电击 (Electrocute) |

### 混沌抗性 (Chaos Resistance)

- 减轻混沌伤害
- 与毒 (Poison) 相关
- 在伤害防御体系中具有特殊交互（见下方）

> **0.5.0 更新**："Defences" 关键词不再使用，明确改为 "Armour, Evasion and Energy Shield"，以澄清抗性、符咒护佑和格挡不属于此范畴[^maxroll-050-patchnotes]。

## 默认上限

- **默认最大抗性**：**75%**
- 可以通过特定修饰词提高最大抗性上限
- 超过 75% 的抗性提供额外的减伤

## 抗性与减伤

抗性按百分比减轻对应类型的伤害：

```
实际承受伤害 = 原始伤害 * (1 - 抗性%)
```

例如，拥有 75% 火焰抗性：
- 1,000 火焰伤害 → 实际承受 250 火焰伤害

## 抗性来源

### 装备
- 戒指、腰带、项链通常提供抗性
- 胸甲、头盔、手套、靴子也可能有抗性修饰词
- 最高元素抗性修饰词：+41-45%（需要物品等级 82+）

### 被动技能树
- 各种抗性节点
- 特定元素抗性集群
- 所有元素抗性节点

### 技能与增益
- 某些光环/增益提供抗性
- 药瓶临时提供抗性

### 符文
- 装备插槽中的抗性符文

## 特殊交互

### 负抗性

诅咒 (Curses)、暴露 (Exposure) 和其他效果可以将抗性降至 **0% 以下**（负数）。

- 负抗性会使对应伤害类型的承受伤害增加
- **穿透 (Penetration)** 不能低于 0% 抗性——它只能将抗性降至 0%，不能使其变为负数

### 混沌抗性与能量护盾

- 混沌伤害默认对能量护盾造成**两倍**伤害
- 高混沌抗性可以减轻此效果
- `Chaos Inoculation` 完全免疫混沌伤害（但将最大生命设为 1）

### 最大抗性上限

某些修饰词可以提高最大抗性上限：
- `Unnatural Resilience`：通过堆叠火焰抗性提升最大火焰抗性
- 某些传奇物品提供 +1% 到 +5% 最大抗性

## 抗性与异常状态

抗性不仅影响伤害减免，还影响异常状态：

- **冰霜抗性**：影响被冻结的几率（伤害减免后计算）
- **闪电抗性**：影响被感电的几率和效果
- **元素异常阈值**：独立机制，也影响异常状态抗性

## 硬上限

伤害减免和抗性是两个独立的层，各自硬上限为 **90%**。

## 建议

### 战役阶段
- 尽量保持元素抗性在 50-75%
- 混沌抗性在早期不那么重要

### 后期游戏
- 尽量将所有元素抗性堆至 75% 上限
- 根据构建需求堆叠混沌抗性
- 考虑使用提高最大抗性的修饰词

## 相关机制

- **护甲 (Armour)**：针对物理伤害的防御层
- **闪避 (Evasion)**：针对命中的防御层
- **能量护盾 (Energy Shield)**：资源型防御
- **元素异常阈值 (Elemental Ailment Threshold)**：针对异常状态的防御
- **诅咒 (Curses)**：可以降低目标抗性

---

## 参考来源

[^mobalytics-resistances]: Mobalytics — PoE 2 Resistances Explained. https://mobalytics.gg/poe-2/guides/resistances
[^sportskeeda-defense]: Sportskeeda — PoE 2 Defense & Resistance Guide. https://www.sportskeeda.com/mmo/exile-2-poe2-defense-resistance-guide-energy-shield-armor-evasion
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
