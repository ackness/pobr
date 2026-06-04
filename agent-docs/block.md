# 格挡机制 (Blocking)

格挡是一种防御机制，可以完全防止一次击中的伤害[^poewiki-blocking]。格挡分为被动格挡和主动格挡两种方式。

## 格挡类型

### 被动格挡 (Passive Block)

- 基于装备的**格挡几率 (Block Chance)**
- 在防御计算顺序的后期自动滚动
- 不需要玩家主动操作
- 格挡几率硬上限为 **90%**（POB2 确认：`data.misc.BlockChanceCap = 90`）[^pob2-deepwiki-block]

### 主动格挡 (Active Block)

- 通过使用技能如 `Raise Shield` 或类似技能执行
- 需要玩家主动操作
- 可以在关键时刻使用以防御致命伤害

> **0.5.0 更新**：格挡、招架和共振盾不再延迟重眩晕积累的预期衰减时间[^maxroll-050-patchnotes]。

## 格挡计算

格挡在伤害计算的**步骤 7** 发生（在减伤和伤害承受修饰词之后）。

### 格挡效果

当一次击中被格挡时：
- **默认防止所有伤害**
- 但击中仍然发生（只是伤害为 0）
- **格挡不会阻止任何击中效果**，如：
  - 眩晕 (Stun)
  - 冻结 (Freeze)
  - 感电 (Shock)
  - 其他击中触发的 debuff

## 法术压制 (Spell Suppression)

与格挡相关的机制是**法术压制 (Spell Suppression)**。

### 法术压制效果

- 有一定几率**压制**受到的法术伤害
- 被压制的法术伤害默认防止 **50%**
- 可以通过修饰词增加压制时防止的伤害比例

### 获取法术压制

- 某些装备（特别是基于闪避的装备）提供法术压制几率
- 被动技能树节点
- 某些升华职业特性
- 某些传奇物品

### 法术压制与闪避

法术压制和闪避可以叠加使用：
- 先检查闪避
- 如果未闪避，检查法术压制
- 提供多层防御

## 格挡恢复 (Block Recovery)

格挡后有短暂的恢复时间，在此期间：
- 角色可能无法执行某些动作
- 可以通过修饰词缩短或消除格挡恢复

某些装备提供"增加格挡恢复"或"格挡时立即恢复"的效果。

## 无法格挡的击中

某些 Boss 技能**不能被格挡**，通常有：
- 红色闪光视觉效果
- 特殊音效提示

## 格挡与眩晕

被格挡的击中仍然可以造成眩晕（如果伤害原本会超过眩晕阈值）。这是因为格挡只防止伤害，不阻止击中效果。

## 获取格挡

### 装备

| 装备类型 | 格挡几率 |
|---------|---------|
| 盾牌 (Shield) | 最高（20-30%+） |
| 某些胸甲 | 可能有格挡相关修饰词 |
| 某些传奇物品 | 可能有特殊格挡效果 |

### 被动技能树

- 各种格挡几率节点
- 格挡恢复节点
- 特定武器类型的格挡节点

### 升华职业

某些升华职业提供格挡相关的特性：
- 增加格挡几率
- 格挡时的特殊效果
- 格挡恢复的改进

## 格挡与其他防御层的关系

```
防御计算顺序：
1. 闪避 (Evasion)
2. 减伤 (Mitigation) - 护甲、抗性
3. 伤害承受修饰词
4. 眩晕 (Stun)
5. 格挡 (Block) ← 此处
```

## 相关机制

- **闪避 (Evasion)**：命中前的防御层
- **护甲 (Armour)**：减伤防御层
- **抗性 (Resistances)**：元素减伤
- **法术压制 (Spell Suppression)**：法术伤害的额外防御
- **眩晕 (Stun)**：格挡不能防止的效果之一

---

## 参考来源

[^poewiki-blocking]: PoE Wiki — Blocking. https://www.poewiki.net/wiki/Blocking
[^sportskeeda-defense]: Sportskeeda — PoE 2 Defense & Resistance Guide. https://www.sportskeeda.com/mmo/exile-2-poe2-defense-resistance-guide-energy-shield-armor-evasion
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-block]: Path of Building for PoE2 DeepWiki — CalcDefence / Block Mechanics. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
