# 宝石系统 (Gem System)

Path of Exile 2 的宝石系统相比前作有显著变化[^poe2wiki-spirit-gem][^skycoach-gems]。宝石不再镶嵌在装备上，而是直接镶嵌在角色的技能面板中。每种宝石类型有不同的机制和用途。

## 宝石类型

### 技能宝石 (Skill Gems)

技能宝石是执行主动技能的核心。它们定义了技能的基础效果、伤害类型、使用速度和其他属性。

技能宝石可以有以下标签：
- **攻击 (Attack)** / **法术 (Spell)**
- **投射物 (Projectile)** / **范围 (Area)** / **近战 (Melee)**
- **火焰 (Fire)** / **冰霜 (Cold)** / **闪电 (Lightning)** / **物理 (Physical)** / **混沌 (Chaos)**
- **引导 (Channeling)** / **持续 (Duration)**

宝石标签决定了它与哪些辅助宝石兼容。

### 辅助宝石 (Support Gems)

辅助宝石通过链接到技能宝石来修改技能的行为。在 POE2 中，辅助宝石直接链接到技能宝石上，而不是像 POE1 那样通过装备上的链接槽[^poe2wiki-spirit-gem]。

辅助宝石的效果包括：
- 增加伤害
- 改变伤害类型
- 添加效果（如额外投射物、范围扩大）
- 改变消耗（如法力倍乘）

### 精神宝石 / 持续增益技能宝石 (Spirit Gems / Persistent Buff Skill Gems)

精神宝石是一种特殊的技能宝石，只能通过**未切割精神宝石 (Uncut Spirit Gem)** 创建或升级[^poe2wiki-spirit-gem]。

这些宝石提供持续效果，如：
- 增加防御或伤害
- 光环 (Auras)
- 永久召唤物 (Permanent Minions)
- 只要其**精神 (Spirit)** 消耗被保留，效果就保持激活

### 元宝石 (Meta Gems)

元宝石（Meta Gems / Trigger Skills）是高级宝石，在特定条件下触发独特效果，并且也可以被辅助宝石增强[^mobalytics-meta-gems]。详见 [meta-gems.md](meta-gems.md)（注意：POB2 对元宝石伤害计算的支持目前尚不完整）。

详见 [meta-gems.md](meta-gems.md)。

## 宝石等级与品质

### 等级 (Level)

- 宝石通过击杀怪物获得经验升级
- 最高等级因宝石而异
- 等级影响基础伤害、效果范围和持续时间等
- 瓦尔宝珠可以增加/减少宝石等级 ±1

### 品质 (Quality)

- 默认最高 20%
- 瓦尔宝珠可以将品质提升至最高 23%
- 品质提供额外的属性加成，具体效果因宝石类型而异

## 宝石插槽

技能宝石有辅助宝石插槽，可以通过珠宝匠宝珠增加：
- **小型珠宝匠宝珠 (Lesser)**
- **高级珠宝匠宝珠 (Greater)**
- **完美珠宝匠宝珠 (Perfect)**

## 宝石属性

### 基础暴击率 (Base Critical Hit Chance)

- **法术**：基础暴击率来自技能宝石本身
  - 纯物理法术最高（如 `Bonestorm` 15%）
  - 纯火焰法术最低（如 `Fireball` 7%）
  
- **攻击**：基础暴击率来自装备武器
  - 例外：副手攻击和空手攻击使用技能宝石的基础暴击率

### 伤害效果 (Damage Effectiveness)

攻击技能宝石上列出的攻击伤害百分比用于缩放来自武器和装备的伤害加成。

### 基础使用速度

每个技能宝石有自己的基础使用速度（以秒为单位），可以通过技能速度、攻击速度或施法速度修饰词来修改。

## 转化宝石 (Transfigured Gems)

转化宝石系统替换了旧的技能变体系统。每个技能宝石可能有多个转化版本，提供不同的效果或机制变化[^skycoach-gems]。

## 宝石与精神 (Spirit)

精神是一种资源，用于保留持续增益技能的效果。
- 精神宝石和元宝石有精神保留成本
- 总可用精神限制了可以同时激活的永久效果数量

> **0.5.0 更新**：`Gemling Legionnaire` 升华的 `Advanced Thaumaturgy` 现在不再提供 Thaumaturgical Dynamism 技能，而是改为"宝石品质赋予插槽技能额外效果"。所有宝石现在都有可通过按住 Alt 查看的额外品质属性[^maxroll-050-patchnotes]。
>
> **POB2 状态说明**：POB2 对 Meta Skills / Trigger Skills 的伤害计算尚在进行全面重构中，因此通过 POB2 核对元宝石相关数值时可能存在不准确性[^pob2-deepwiki-gems]。

---

## 参考来源

[^skycoach-gems]: SkyCoach — PoE 2 Gems Guide. https://skycoach.gg/blog/path-of-exile-2/articles/gems-guide
[^poe2wiki-spirit-gem]: PoE2 Wiki — Spirit gem. https://www.poe2wiki.net/wiki/Spirit_gem
[^mobalytics-meta-gems]: Mobalytics — PoE 2 Meta Gems Guide. https://mobalytics.gg/poe-2/guides/meta-gems
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-gems]: Path of Building for PoE2 DeepWiki — Game Mechanics / Meta Skills & Spirit. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
