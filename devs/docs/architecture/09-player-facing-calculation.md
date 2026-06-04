# 玩家可见计算输出

---

## 1. 目标

PoBR 的计算结果必须回答玩家的核心问题：

1. 我更改天赋点、装备、技能宝石或配置后，选中技能的伤害如何变化？
2. 我更改天赋点、装备、技能宝石或配置后，生存面如何变化？
3. 变化来自哪里：基础值、装备词条、天赋、技能系数、敌人抗性、暴击、异常状态、速度、命中、穿透、减伤、恢复还是上限？
4. 每种伤害类型和每层防御在最终结果里占多少？

因此 `pobr-core` 输出不能只是 `dps: f64` 和 `life: f64`。输出必须同时包含：

- 可排序/可对比的 `StatValue`。
- 可展开的 `BreakdownTree`。
- 按伤害类型和机制分桶的占比。
- base build 与 variant build 的 delta。
- 对 UI 友好的 display stat catalog。

本文件列出的字段目录是分组和首批实现方向。最终属性覆盖以 `10-pob-parity-and-attribution.md` 中的 PoB 全量 catalog/parity matrix 为准，PoBR 不能把本文件的字段清单当作上限。

---

## 2. PoB 参考边界

PoB 大致分三层：

| 层 | PoB 模块 | PoBR 目标 |
|----|----------|-----------|
| 计算 | `CalcOffence.lua`, `CalcDefence.lua`, `Calcs.lua` | `pobr-core::calc` |
| 详细解释 | `CalcSections.lua` | `pobr-core::breakdown` + `pobr-build::sections` |
| 侧栏与对比字段 | `BuildDisplayStats.lua` | `pobr-build::display_stats` |

PoBR 需要保留 PoB 的能力边界，同时用强类型输出替代 Lua 的动态 output table。

---

## 3. 输出字段目录

### 3.1 Offence

第一优先级：

- `TotalDps`
- `HitDps`
- `DotDps`
- `AverageHit`
- `AverageDamage`
- `ActionRate`
- `SkillUseTime`
- `HitChance`
- `CritChance`
- `CritMultiplier`
- `CritDpsContribution`
- `NonCritDpsContribution`
- `CullingDpsGain`

伤害类型：

- `PhysicalHit`
- `FireHit`
- `ColdHit`
- `LightningHit`
- `ChaosHit`
- `PhysicalDot`
- `FireDot`
- `ColdDot`
- `LightningDot`
- `ChaosDot`

异常与 debuff：

- `BleedDps`
- `IgniteDps`
- `PoisonDps`
- `CorruptedBloodDps`
- `ShockEffect`
- `ChillEffect`
- `FreezeChance`
- `ScorchEffect`
- `BrittleEffect`
- `SapEffect`

技能机制：

- `ProjectileCount`
- `ChainCount`
- `ForkCount`
- `PierceCount`
- `SplitCount`
- `AreaRadius`
- `AreaDamageOverlap`
- `Cooldown`
- `StoredUses`
- `TriggerRate`
- `EffectiveTriggerRate`
- `Duration`
- `Uptime`
- `DpsMultiplier`

### 3.2 Survivability

第一优先级：

- `TotalEhp`
- `PhysicalMaxHit`
- `FireMaxHit`
- `ColdMaxHit`
- `LightningMaxHit`
- `ChaosMaxHit`
- `LowestMaxHit`
- `EffectiveHitPoolByType`
- `TotalMitigationByType`

防御层：

- `Life`
- `LifeUnreserved`
- `Mana`
- `ManaUnreserved`
- `EnergyShield`
- `Armour`
- `Evasion`
- `Ward`
- `ChanceToBeHit`
- `PhysicalDamageReduction`
- `BlockChance`
- `SpellBlockChance`
- `DodgeChance`
- `SpellDodgeChance`
- `SpellSuppressionChance`
- `SpellSuppressionEffect`

抗性与上限：

- `FireResistanceTotal`
- `FireResistanceFinal`
- `FireResistanceMax`
- `FireResistanceOvercap`
- `ColdResistanceTotal`
- `ColdResistanceFinal`
- `ColdResistanceMax`
- `ColdResistanceOvercap`
- `LightningResistanceTotal`
- `LightningResistanceFinal`
- `LightningResistanceMax`
- `LightningResistanceOvercap`
- `ChaosResistanceTotal`
- `ChaosResistanceFinal`
- `ChaosResistanceMax`
- `ChaosResistanceOvercap`

Avoidance / Immunity：

- `StunAvoidChance`
- `FreezeAvoidChance`
- `ChillAvoidChance`
- `ShockAvoidChance`
- `IgniteAvoidChance`
- `BleedAvoidChance`
- `PoisonAvoidChance`
- `CorruptedBloodImmune`
- `CurseAvoidChance`
- `CritDamageReduction`

### 3.3 Resources, Recovery, Costs

资源：

- `LifeRecoverable`
- `EnergyShieldRecharge`
- `EnergyShieldRecoveryCap`
- `Rage`
- `Spirit`

恢复：

- `LifeRegen`
- `LifeLeechRate`
- `LifeLeechPerHit`
- `ManaRegen`
- `ManaLeechRate`
- `ManaLeechPerHit`
- `EnergyShieldRegen`
- `EnergyShieldLeechRate`
- `TotalBuildDegen`
- `TotalNetRecovery`

技能消耗：

- `ManaCost`
- `ManaCostPerSecond`
- `LifeCost`
- `LifeCostPerSecond`
- `EnergyShieldCost`
- `EnergyShieldCostPerSecond`
- `RageCost`
- `SoulCost`

### 3.4 Requirements and Utility

- `Strength`
- `Dexterity`
- `Intelligence`
- `RequiredStrength`
- `RequiredDexterity`
- `RequiredIntelligence`
- `MovementSpeed`
- `Charges`
- `ChargeDuration`
- `MinionLimit`
- `AuraEffect`
- `CurseEffect`
- `ExposureEffect`
- `WitherStacks`

---

## 4. Breakdown 模型

### 4.1 BreakdownTree

每个关键字段必须能解释来源。推荐结构：

```rust
pub struct BreakdownTree {
    pub stat: DisplayStatId,
    pub root: BreakdownNode,
}

pub struct BreakdownNode {
    pub label: BreakdownLabel,
    pub value: f64,
    pub percent_of_parent: Option<f64>,
    pub operation: BreakdownOperation,
    pub source: BreakdownSource,
    pub children: Vec<BreakdownNode>,
}

pub enum BreakdownOperation {
    Base,
    Add,
    Increased,
    More,
    Override,
    Clamp,
    Convert,
    Mitigate,
    Average,
    Chance,
    Cap,
}

pub enum BreakdownSource {
    CharacterBase,
    SkillGem(SkillId),
    SupportGem(SkillId),
    Item(ItemSlot),
    Passive(NodeId),
    Ascendancy(NodeId),
    Config(String),
    Enemy,
    GameConstant,
}
```

### 4.2 伤害占比

对选中技能，输出必须能展示：

```text
TotalDps
├─ HitDps
│  ├─ PhysicalHitDps
│  ├─ FireHitDps
│  ├─ ColdHitDps
│  ├─ LightningHitDps
│  └─ ChaosHitDps
└─ DotDps
   ├─ BleedDps
   ├─ IgniteDps
   ├─ PoisonDps
   ├─ CorruptedBloodDps
   └─ OtherDotDps
```

每个叶子需要：

- raw value
- percent of `TotalDps`
- affected by enemy mitigation
- key contributors

### 4.3 生存占比

对生存面，输出必须能展示：

```text
FireMaxHit
├─ Life / ES / Mana taken pool
├─ FireResistanceFinal
├─ DamageTaken modifiers
├─ Block / Suppression / Avoidance when applicable
└─ Recovery or guard contribution when applicable
```

EHP/MaxHit 必须按 damage type 输出，因为玩家需要知道是物理、元素还是混沌短板。

---

## 5. Build Comparison

### 5.1 比较入口

```rust
pub struct ComparisonRequest {
    pub selected_skill: SkillInstanceId,
    pub base: BuildSnapshot,
    pub variant: BuildSnapshot,
    pub focus: ComparisonFocus,
}

pub enum ComparisonFocus {
    SelectedSkillDamage,
    Survivability,
    Recovery,
    Costs,
    Requirements,
    AllDisplayStats,
}

pub struct ComparisonResult {
    pub selected_skill: SkillInstanceId,
    pub changed_inputs: Vec<InputChange>,
    pub stat_deltas: Vec<StatDelta>,
    pub top_gains: Vec<ContributionDelta>,
    pub top_losses: Vec<ContributionDelta>,
}
```

### 5.2 StatDelta

```rust
pub struct StatDelta {
    pub stat: DisplayStatId,
    pub before: StatValue,
    pub after: StatValue,
    pub absolute_delta: f64,
    pub percent_delta: Option<f64>,
    pub category: DisplayStatCategory,
    pub severity: DeltaSeverity,
}
```

UI 可以基于这个结构回答：

- 换武器后 `TotalDps +18.3%`。
- 伤害提升主要来自 `base physical damage` 和 `attack speed`。
- 但 `FireMaxHit -9.4%`，原因是失去 `+max fire resistance`。
- `ManaCostPerSecond +22%`，可能导致 sustain 失败。

### 5.3 ContributionDelta

```rust
pub struct ContributionDelta {
    pub source: BreakdownSource,
    pub stat: DisplayStatId,
    pub before_contribution: f64,
    pub after_contribution: f64,
    pub delta: f64,
    pub explanation_key: String,
}
```

这个结构用于“为什么变了”，比单纯输出最终数值更重要。

---

## 6. DisplayStatCatalog

`DisplayStatCatalog` 是 PoBR 对应 PoB `BuildDisplayStats.lua` 的强类型版本。

```rust
pub struct DisplayStatDefinition {
    pub id: DisplayStatId,
    pub category: DisplayStatCategory,
    pub default_visible: bool,
    pub compare_visible: bool,
    pub format: StatFormat,
    pub higher_is_better: Option<bool>,
    pub breakdown_kind: BreakdownKind,
}

pub enum DisplayStatCategory {
    Offence,
    DamageType,
    Ailment,
    Defence,
    Resistance,
    Recovery,
    Cost,
    Requirement,
    Utility,
}
```

规则：

- `pobr-core` 负责生成 raw stats 和 breakdown。
- `pobr-build` 负责选择展示字段、排序和比较。
- `pobr-i18n` 负责把 `DisplayStatId` 转成 `en-US` / `zh-TW` 文本。
- UI 不直接读取计算内部字段名。

---

## 7. 开发验收

每新增一个计算机制，必须同时交付：

1. core unit test：公式正确。
2. output stat：进入 `DisplayStatCatalog` 或明确标记内部字段。
3. breakdown：能解释来源和运算。
4. comparison：base/variant delta 可见。
5. fixture：至少一个装备、天赋或技能变化用例。
6. parity matrix：对应 PoB output/display key 的 `ParityStatus` 已更新。
7. attribution：至少支持 source/item/passive/gem/config 中相关来源的直接或边际贡献。

例子：

```text
新增 maximum resistance
├─ unit: hard cap 90, default 75, overcap
├─ output: FireResistanceFinal, FireResistanceMax, FireResistanceOvercap
├─ breakdown: base max + item/passive max + hard cap clamp
├─ comparison: 换盾牌后 FireMaxHit 增减
└─ fixture: shield_with_plus_max_fire_resistance
```

---

## 8. 下一步优先级

1. `DisplayStatId` / `DisplayStatCatalog` 类型。
2. `BreakdownTree` 替换当前 flat `BreakdownTable`。
3. `StatBoundary` 重构抗性，并输出 total/final/max/overcap/missing。
4. `SkillUseTime` 替换当前简化 action rate。
5. `DamageComponent` 输出 physical/fire/cold/lightning/chaos hit shares。
6. `ComparisonResult` 最小实现：比较两个 `OutputTable`。
7. 再接入 bleed/corrupted blood、skill gem coefficient、max hit/EHP。

---

## 9. 资料来源

- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcSetup.lua` / `CalcPerform.lua`：base stats、attributes、config mods、计算编排。
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcOffence.lua`：active skill damage、DPS、ailment、skill-specific mechanics。
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcDefence.lua`：resistance、mitigation、max hit、EHP、生存计算。
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `QuestRewards.lua`：战役永久奖励和 Seven Pillars/Qimah 选择。
- PathOfBuildingCommunity `CalcSections.lua`：UI detailed breakdown sections。
- PathOfBuildingCommunity `BuildDisplayStats.lua`：sidebar display stats 和 item/passive comparison 字段。
- DeepWiki 对 PathOfBuildingCommunity/PathOfBuilding-PoE2 的 calculation/display 模块分析。
