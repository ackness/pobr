# 属性系统 (Attributes)

Path of Exile 2 中有三种主要属性：力量 (Strength)、敏捷 (Dexterity) 和智力 (Intelligence)。这些基本属性决定了角色的许多基本统计数据[^fextralife-attributes][^sportskeeda-attributes]。

## 主要属性

## 基础属性与等级

截至 0.5.0，PoB-PoE2 在 `CalcSetup.lua` 中注入职业起始属性、生命/魔力/精准等级成长，在 `CalcPerform.lua` 中计算属性并派生 inherent attribute bonus[^pob-poe2-calc-setup][^pob-poe2-calc-perform]：

- 生命：1 级角色基础生命 28；每个玩家等级 +12 最大生命；每 1 点 Strength +2 最大生命。
- 魔力：1 级角色基础魔力 34；每个玩家等级 +4 最大魔力；每 1 点 Intelligence +2 最大魔力。
- 魔力回复：默认每秒回复 4% 最大魔力。
- 精准：每个玩家等级 +6 Accuracy Rating；每 1 点 Dexterity +6 Accuracy Rating。

这些基础值应从角色等级、职业和属性推导，不能在构建计算入口中长期使用全 0 默认值。

## 职业起始属性

PoE2 职业按纯属性 / 混合属性 / 非相关属性决定起始属性：

| 属性 | 纯属性职业 | 混合属性职业 | 非相关职业 |
|------|------------|--------------|------------|
| Strength | Warrior / Marauder: 15 | Templar / Druid / Duelist / Mercenary: 11 | 7 |
| Dexterity | Ranger / Huntress: 15 | Shadow / Monk / Duelist / Mercenary: 11 | 7 |
| Intelligence | Witch / Sorceress: 15 | Templar / Druid / Shadow / Monk: 11 | 7 |

实现时应把职业转换为基础 Strength / Dexterity / Intelligence，再由属性公式派生生命、魔力和精准。

## 主要属性

### 力量 (Strength)

- 需求：装备大多数提供护甲的装备，以及各种近战对齐的武器和技能。
- **每 1 点力量提供 +2 生命 (Life)**。
- 与火焰伤害主题关联。
- 被动技能树左侧是力量区域。
- 战士 (Warrior) 从力量区域开始。

### 敏捷 (Dexterity)

- **每 1 点敏捷提供 +6 精准 (Accuracy Rating)**。
- 与闪电伤害主题关联。
- 与弓、长矛、十字弓等武器关联。
- 游侠 (Ranger) 从敏捷区域开始。

### 智力 (Intelligence)

- **每 1 点智力提供 +2 法力 (Mana)**。
- 与冰霜伤害主题关联。
- 与法杖等武器关联。
- 女巫 (Witch) 从智力区域开始。

## 属性与伤害类型的关联

| 属性 | 主要伤害类型 | 武器类型 |
|------|-----------|---------|
| 力量 | 火焰 | 钉头锤 (Mace)、部分长矛/十字弓 |
| 敏捷 | 闪电 | 弓 (Bow)、长矛 (Spear)、十字弓 (Crossbow) |
| 智力 | 冰霜 | 法杖 (Staff)、部分长矛 |

**注意**：属性关联并非绝对严格。某些技能可能出现在非典型武器上（如长矛上的 `Glacial Lance`）。

## 属性需求

大多数装备有属性需求才能装备：
- 力量需求：主要用于护甲和近战武器
- 敏捷需求：主要用于远程武器和闪避装备
- 智力需求：主要用于能量护盾装备和法杖

## 属性获取方式

1. **被动技能树**：属性节点提供属性点数
2. **装备**：各种装备提供属性加成
3. **珠宝**：珠宝可以提供属性
4. **技能宝石**：某些宝石提供属性
5. **升华职业**：某些升华 notable 提供属性
6. **战役永久奖励**：例如 Halls of the Dead 纹身、Seven Pillars 的 `Kochai's Boon`（见 [campaign-rewards.md](campaign-rewards.md)）

## 属性与构建规划

理解属性需求对于构建规划至关重要：
- 确保有足够属性装备关键物品
- 属性节点在被动树上的位置影响路径规划
- 某些构建可能需要通过装备弥补属性不足

---

## 参考来源

[^fextralife-attributes]: Fextralife — PoE 2 Stats & Attributes. https://pathofexile2.wiki.fextralife.com/Stats+%26+Attributes
[^sportskeeda-attributes]: Sportskeeda — PoE 2 Stat & Attribute Guide. https://www.sportskeeda.com/mmo/path-exile-2-poe2-stat-attribute-guide-strength-dex-intelligence
[^poe2wiki-life]: PoE2 Wiki — Life. https://www.poe2wiki.net/wiki/Life
[^poe2wiki-mana]: PoE2 Wiki — Mana. https://www.poe2wiki.net/wiki/Mana
[^poe2wiki-strength]: PoE2 Wiki — Strength. https://www.poe2wiki.net/wiki/Strength
[^poe2wiki-dexterity]: PoE2 Wiki — Dexterity. https://www.poe2wiki.net/wiki/Dexterity
[^poe2wiki-intelligence]: PoE2 Wiki — Intelligence. https://www.poe2wiki.net/wiki/Intelligence
[^pob-poe2-calc-setup]: PathOfBuildingCommunity/PathOfBuilding-PoE2 — `src/Modules/CalcSetup.lua`. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcSetup.lua
[^pob-poe2-calc-perform]: PathOfBuildingCommunity/PathOfBuilding-PoE2 — `src/Modules/CalcPerform.lua`. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcPerform.lua
