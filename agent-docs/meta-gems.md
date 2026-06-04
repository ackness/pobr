# 元宝石 (Meta Gems)

元宝石（又称触发技能 / Meta Skills）是 Path of Exile 2 中的高级宝石，在特定条件下触发独特效果，并且也可以被辅助宝石增强[^mobalytics-meta-gems]。

> **POB2 状态说明**：POB2 的更新日志明确指出"Meta Skills / Trigger Skills damage calculation - this needs an entire overhaul we didn't have time to do thus far"，说明 POB2 对元宝石伤害计算的支持目前尚不完整[^pob2-deepwiki-gems]。

## 触发型元宝石 (Trigger Meta Gems)

触发型元宝石使用一种称为**能量 (Energy)** 的资源计数器来确定何时触发法术，或触发多少次。

### 能量 (Energy)

能量在满足元宝石设定的特定条件时获得。例如：
- `Cast on Ignite`：每当点燃一个敌人时获得并储存能量
- 达到最大能量时，自动触发所有插槽中的法术

**最大能量**由插槽中所有法术的基础施法时间总和决定。

### 敌人力量 (Enemy Power / Monster Power)

敌人力量是一种根据怪物稀有度和难度分配值的机制：

| 怪物类型 | 稀有度乘数 |
|---------|-----------|
| 普通 (Normal) | 1 |
| 魔法 (Magic) | 2 |
| 稀有 (Rare) | 5 |
| 独特 (Unique) | 20（固定） |

基础力量值乘以稀有度得到最终力量值。例如，一个基础力量为 2 的稀有怪物，最终力量为 2 × 5 = 10。

### 触发技能 (Triggering a Skill)

当触发插槽中的法术时，元宝石的能量计数器重置为 0。

**注意**：触发技能时仍然需要支付任何相关的资源成本（通常是法力）。

### 精神保留 (Spirit Reservation)

触发型元宝石有基于宝石的精神保留成本。例如，`Barrier Invocation` 保留 60 精神。

这些宝石也被视为**持续增益 (Persistent Buffs)**，因此可以被持续增益辅助宝石（如 `Clarity I`）支持，只要你有足够的额外精神来保留。

**注意**：具有成本倍乘的辅助宝石不会增加精神保留，除非它们明确指定了保留倍乘。

## 其他元宝石

除了触发型元宝石外，还有其他类型的元宝石提供各种机制：
- 条件触发
- 效果连锁
- 资源管理

## 元宝石与辅助宝石

元宝石可以被辅助宝石增强，就像普通技能宝石一样。辅助宝石的效果取决于：
- 元宝石的类型
- 触发条件的类型
- 被触发的技能类型

## 元宝石与精神系统

由于元宝石被视为持续增益，它们：
- 消耗精神保留
- 受限于总可用精神
- 可以被增加精神上限的修饰词影响

## 元宝石使用策略

1. 选择与你的构建机制相符的触发条件
2. 确保有足够的精神来保留元宝石
3. 优化插槽中的法术以匹配触发频率
4. 使用辅助宝石增强触发效果

---

## 参考来源

[^mobalytics-meta-gems]: Mobalytics — PoE 2 Meta Gems Guide. https://mobalytics.gg/poe-2/guides/meta-gems
[^pob2-deepwiki-gems]: Path of Building for PoE2 DeepWiki — Game Mechanics / Meta Skills & Spirit. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
