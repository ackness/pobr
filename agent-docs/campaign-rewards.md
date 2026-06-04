# 战役永久奖励与进度惩罚 (Campaign Rewards)

本页记录 Path of Exile 2 0.5.0 中会影响角色计算的战役进度、永久奖励和可重选奖励。实现时这些内容应作为 Build/Config 输入进入 `ModDb` 或特殊机制公式，保留 `SourceId`，方便 PoB 兼容导入和 source-level attribution。

## 实现原则

- 战役奖励是角色配置的一部分，不属于装备、天赋或技能。
- 普通数值奖励进入 modifier 聚合层，来源使用 `SourceKind::CampaignReward`。
- 进度惩罚进入配置层，来源使用 `SourceKind::Config` 或 `SourceKind::CampaignReward`，并在 trace 中可见。
- 二选一或多选一奖励必须保留选择项 ID；可重选奖励允许替换当前选择。
- `Rakiata's Lesson` 这类机制型奖励进入专门公式：护甲作用于元素击中、按闪避派生偏转、能量护盾 recharge delay。

## 元素抗性进度惩罚

PoE2 的元素抗性惩罚由战役进度和区域等级决定。该惩罚作用于火焰、冰霜、闪电抗性；当前资料没有确认混沌抗性随同降低。

| 进度 | 元素抗性惩罚 |
|------|--------------|
| Act 1 | 0% |
| Act 2 | -10% |
| Act 3 | -20% |
| Act 4 | -30% |
| Interlude area level 54-59 | -40% |
| Interlude area level 60-64 | -50% |
| Endgame area level 65+ | -60% |

PoB-PoE2 的 `CalcSetup.lua` 把 `env.configInput.resistancePenalty` 作为 `BASE` modifier 注入 `FireResist` / `ColdResist` / `LightningResist`，`ChaosResist` 基础为 0。PoBR 应复用这个结构，并按上表生成元素抗性惩罚。

## 固定抗性奖励

| 来源 | 位置 | 奖励 | ModName |
|------|------|------|---------|
| `Head of the Winter Wolf` | Act 1 `Beira of the Rotten Pack` | +10% to Cold Resistance | `ColdResistance` |
| `Sisters of Garukhan` | Act 2 `The Spires of Deshar` | +10% to Lightning Resistance | `LightningResistance` |
| `The Flame Core` | Act 3 `Blackjaw, the Remnant` | +10% to Fire Resistance | `FireResistance` |

## Halls of the Dead 纹身选择

每个 Blank Tattoo 都是二选一永久奖励。

| Blank Tattoo | 抗性选择 | 属性选择 |
|--------------|----------|----------|
| `Blank Tattoo of Ngamahu` | `Fire Tattoo of Ngamahu`: +5% Fire Resistance | `Strength Tattoo of Ngamahu`: +5 Strength |
| `Blank Tattoo of Tawhoa` | `Lightning Tattoo of Tawhoa`: +5% Lightning Resistance | `Dexterity Tattoo of Tawhoa`: +5 Dexterity |
| `Blank Tattoo of Tasalio` | `Cold Tattoo of Tasalio`: +5% Cold Resistance | `Intelligence Tattoo of Tasalio`: +5 Intelligence |

属性选择会继续派生到生命、魔力或精准：

- Strength: +2 maximum Life per point。
- Dexterity: +6 Accuracy Rating per point。
- Intelligence: +2 maximum Mana per point。

## Venom Draught 三选一

`Corpse-snake Venom` 可兑换一个 Venom Draught，该选择是永久选择。

| 选择 | 奖励 | ModName / 机制 |
|------|------|----------------|
| `Venom Draught of Stone` | 25% increased Stun Threshold | `StunThreshold` `Inc` |
| `Venom Draught of the Veil` | 30% increased Elemental Ailment Threshold | `ElementalAilmentThreshold` `Inc` |
| `Venom Draught of Clarity` | 25% increased Mana Regeneration Rate | `ManaRegen` `Inc` |

## Tribal Medicine: Shark Fin 二选一

`Shark Fin` 可兑换 `Kaom's Lesson` 或 `Rakiata's Lesson`，该选择是永久选择。

| 选择 | 奖励 | 实现方式 |
|------|------|----------|
| `Kaom's Lesson` | 30% increased Armour, Evasion and Energy Shield | 分别生成 `Armour` / `Evasion` / `EnergyShield` 的 `Inc` modifier |
| `Rakiata's Lesson` | 15% of Armour also applies to Elemental Damage from Hits | 元素 hit mitigation 中加入 armour contribution |
| `Rakiata's Lesson` | Gain Deflection Rating equal to 12% of Evasion Rating | 从最终 Evasion Rating 派生 Deflection Rating |
| `Rakiata's Lesson` | 12% faster start of Energy Shield recharge | `EnergyShieldRechargeDelay` 或等价 recharge delay 机制 |

`Rakiata's Lesson` 不是普通加法属性。计算时需要在 trace 中保留从奖励到护甲元素减伤、偏转、ES recharge 的路径。

## Seven Pillars / Qimah 相关选择

`Seven Pillars` 位于 Interlude 2 的 `Qimah` 区域，该区域连接 `Qimah Reservoir`。中文资料可能把该流程描述为奇玛 / 奇玛水源地 / 奇玛水库附近的柱子选择；实现中应使用稳定 ID `qimah.seven_pillars`，显示文本交给 i18n。

Wiki 标注该选择可以重选。角色同时只能激活一个 pillar boon；实现时保存当前选择，不需要像永久奖励一样锁定历史选择。

| 选择 | 奖励 | 计算影响 |
|------|------|----------|
| `Ahkeli's Boon` | 15% increased Global Armour, Evasion and Energy Shield | armour / evasion / energy shield |
| `Galai's Boon` | 20% increased Presence Area of Effect | presence 范围 |
| `Halani's Boon` | 12% increased Cooldown Recovery Rate | cooldown |
| `Kochai's Boon` | +5 to all Attributes | Strength/Dexterity/Intelligence |
| `Orbala's Boon` | 3% increased Movement Speed | movement speed |
| `Tabana's Boon` | +5% to Elemental Resistances | Fire/Cold/Lightning Resistance |
| `Alima's Disgrace` | 5% increased Experience Gain plus several penalties | experience / armour/evasion/energy shield / presence / cooldown / attributes / movement / elemental resistance |

PoB-PoE2 的 `src/Data/QuestRewards.lua` 使用的文本是 `15% increased Global Armour, Evasion and Energy Shield`，负面奖励使用 `15% reduced Global Armour, Evasion and Energy Shield`。实现中应按该文本生成 `Armour` / `Evasion` / `EnergyShield` 三个 `Inc` modifier。

## 其他固定资源奖励

| 来源 | 奖励 | ModName |
|------|------|---------|
| `Candlemass' Essence` | +20 to maximum Life | `MaximumLife` `Base` |
| `Molten One's Gift` | 5% increased Maximum Life | `MaximumLife` `Inc` |
| `Navali's Rest` | 5% increased maximum Mana | `MaximumMana` `Inc` |
| `Gembloom Skull` | +30 to Maximum Spirit | `MaximumSpirit` `Base` |
| `Gemrot Skull` | +30 to Maximum Spirit | `MaximumSpirit` `Base` |
| `Gemcrust Skull` | +40 to Maximum Spirit | `MaximumSpirit` `Base` |

## PoBR 数据模型建议

```rust
pub struct CampaignState {
    pub resistance_penalty: ElementalResistancePenalty,
    pub permanent_rewards: Vec<CampaignRewardChoice>,
    pub reselectable_rewards: Vec<CampaignRewardChoice>,
}
```

每个 `CampaignRewardChoice` 输出一组普通 modifiers 和一组特殊 mechanic flags：

- 普通 modifiers: 抗性、属性、生命、魔力、Spirit、defence inc。
- 特殊 flags: armour applies to elemental hit damage、deflection from evasion、ES recharge delay。
- Trace source id: `campaign.<quest_or_reward>.<choice>`。

## 参考来源

[^poe2wiki-quest-item]: PoE2 Wiki — Quest item / Permanent character bonuses. https://www.poe2wiki.net/wiki/Quest_item
[^poe2wiki-resistance]: PoE2 Wiki — Resistance / Elemental resistance penalties. https://www.poe2wiki.net/wiki/Resistance
[^poe2wiki-venom-stone]: PoE2 Wiki — Venom Draught of Stone. https://www.poe2wiki.net/wiki/Venom_Draught_of_Stone
[^poe2wiki-venom-veil]: PoE2 Wiki — Venom Draught of the Veil. https://www.poe2wiki.net/wiki/Venom_Draught_of_the_Veil
[^poe2wiki-venom-clarity]: PoE2 Wiki — Venom Draught of Clarity. https://www.poe2wiki.net/wiki/Venom_Draught_of_Clarity
[^poe2wiki-kaom]: PoE2 Wiki — Kaom's Lesson. https://www.poe2wiki.net/wiki/Kaom%27s_Lesson
[^poe2wiki-rakiata]: PoE2 Wiki — Rakiata's Lesson. https://www.poe2wiki.net/wiki/Rakiata%27s_Lesson
[^pob-poe2-quest-rewards]: PathOfBuildingCommunity/PathOfBuilding-PoE2 — `src/Data/QuestRewards.lua`. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/QuestRewards.lua
[^pob-poe2-calc-setup]: PathOfBuildingCommunity/PathOfBuilding-PoE2 — `src/Modules/CalcSetup.lua`. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcSetup.lua
