# 能量护盾 (Energy Shield)

能量护盾是一种防御资源，默认情况下优先于生命 (Life) 承受伤害。任何本应施加于生命的伤害会先由能量护盾承受，但**流血 (Bleeding)** 和**毒 (Poison)** 的伤害除外[^mobalytics-energy-shield]。

## 基本机制

当混沌伤害对能量护盾造成伤害时，默认会造成**两倍**的伤害[^poe2wiki-energy-shield]。来自毒的伤害会直接绕过能量护盾对生命造成伤害。

流血可以被施加于拥有能量护盾的实体。但流血的伤害会直接绕过能量护盾对生命造成伤害。

**注意**：`Chaos Inoculation`（混沌免疫）通过使你免疫流血和毒的混沌伤害来防止这些交互。

> **0.5.0 更新**：能量护盾数值在 65 级时约增加 8%，80 级以上不变。同时生命偷取机制已重制（见下文）[^maxroll-050-patchnotes]。

## 能量护盾充能 (Recharge)

能量护盾具有固有的恢复机制称为**充能 (Recharge)**。

### 默认充能机制

- 充能速率：每秒恢复 **12.5%** 的能量护盾（POB2 确认：`character_inherent_energy_shield_recharge_rate_per_minute_% = 750` → 750/60/100 = 12.5%/秒）[^pob2-deepwiki-es]
- 延迟：在 4 秒内没有因承受伤害而损失能量护盾后开始充能（POB2 确认基础延迟为 4 秒）[^pob2-deepwiki-es]
- 在充能期间承受能量护盾伤害会**中断充能**

**注意**：充能是一种恢复形式，不被视为再生 (Regeneration)。

### 改善充能

可以使用改善充能的修饰词，例如：
- 增加恢复量
- 缩短充能开始前的延迟

**更快的充能开始**：例如，拥有"100% 更快的能量护盾充能开始"，充能会在 2 秒（而不是 4 秒）后开始。

还有一些机制可以：
- **强制能量护盾立即开始充能**
- **防止充能被承受伤害中断**（如 `Energy Barrier Support`）

## 生命偷取重制 (Leech Rework) — 0.5.0

> **0.5.0 重大更新**：偷取机制已完全重制[^maxroll-050-patchnotes]：
> - 每个资源（生命、法力、能量护盾）只能同时有一个偷取实例
> - 当多个偷取实例激活时，只有恢复率最高的那个会生效直到过期，之后次高的实例才会应用
> - 单次击中的偷取伤害上限为 **40,000**。超过此值的击中按比例缩放后计算偷取量
> - **POB2 核对**：大多数 POE2 偷取默认仅限物理伤害（"most PoE2 leech is physical only by default"），元素伤害偷取需依赖物理伤害偷取修饰词转换[^pob2-deepwiki-es]

## 混沌免疫 (Chaos Inoculation)

`Chaos Inoculation` 是一个关键被动技能，使你：
- 完全免疫混沌伤害
- 防止流血和毒的交互效果
- 但可能有其他代价（如最大生命降至 1）

## 获取能量护盾

能量护盾主要来源于：
- 基于智力的装备（Intelligence-aligned gear）
- 护盾、胸甲、头盔、手套、靴子等
- 某些传奇物品提供大量能量护盾
- 被动技能树上的能量护盾节点（通常在智力区域）

## 能量护盾与其他资源

在伤害承受顺序中：
1. 能量护盾优先承受伤害
2. 能量护盾耗尽后，伤害转由生命承受
3. 生命降至 1 后，**符咒护佑 (Runic Ward)** 开始承受伤害

**例外**：
- 流血和毒直接绕过能量护盾
- 混沌伤害对能量护盾造成双倍伤害
- 某些修饰词可以改变伤害承受顺序

## 相关机制

- **生命 (Life)**：能量护盾保护的主要资源
- **混沌抗性 (Chaos Resistance)**：减轻混沌伤害
- **符咒护佑 (Runic Ward)**：生命之后的下一层防御
- **Mind Over Matter**：使伤害先从法力承受
- **Eldritch Battery**：将能量护盾用于法力消耗

---

## 参考来源

[^mobalytics-energy-shield]: Mobalytics — PoE 2 Guide: Energy Shield Explained. https://mobalytics.gg/poe-2/guides/energy-shield
[^poe2wiki-energy-shield]: PoE2 Wiki — Energy Shield. https://www.poe2wiki.net/wiki/Energy_Shield
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-es]: Path of Building for PoE2 DeepWiki — CalcDefence / Energy Shield Recharge & Leech. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
