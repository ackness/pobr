# 伤害与防御计算顺序 (Damage & Defence Calculation Order)

Path of Exile 2 中的伤害和防御机制按特定顺序计算[^mobalytics-damage-order]。理解这个顺序对于评估某些修饰词的价值和优化构建至关重要。

> **与 PoB-PoE2 的核对**：Path of Building PoE2 的 `CalcOffence.lua` 和 `CalcDefence.lua` 模块也遵循类似的计算管线：基础伤害、增加/减少、更多/更少、防御减免。PoBR 实现时以 PoB-PoE2 和 PoE2 Wiki 的公式为主；旧 PoB PoE1 仅用于历史差异参考[^pob2-deepwiki-calc]。

## 完整计算顺序

### 步骤 1：避免击中 (Avoiding the Hit)

在伤害计算之前，首先检查是否可以完全避免击中：
- **闪避 (Evasion)**：基于闪避值与精准值的计算
- **躲避 (Dodge)**：通过翻滚躲避
- 某些 Boss 技能是不可避免的（有预警提示）

如果被完全避免，计算在此停止。

### 步骤 2：伤害计算 (Damage Calculation)

#### 2.1 固定伤害 (Flat Damage)

- **攻击**：基础伤害来自武器伤害（空手攻击使用空手基础伤害，副手攻击来自技能宝石和装备盾牌）
- **法术**：基础伤害来自技能宝石
- 来自各种来源的附加伤害（技能/辅助宝石、装备）直接加到基础伤害
- 总固定伤害被相关的伤害效果修饰词缩放
- 任何影响最小/最大伤害的修饰词在此阶段应用

#### 2.2 伤害转换与额外获得 (Damage Conversion & Gain As Extra)

伤害转换分两阶段进行：

1. **技能类型转换**：包括技能宝石上列出的转换（如 `Perfect Strike`）
2. **次要来源转换**：来自被动技能树、装备修饰词或全局增益的转换

如果任一阶段转换超过 100%，将被缩放并标准化为 100%。

例如：同时有"100% 物理伤害转换为火焰"和"50% 物理伤害转换为冰霜"，最终转换比例为 67%/33%。

#### 2.3 伤害倍增器 (Damage Multipliers)

- **增加/减少 (Increased/Reduced)**：加法叠加
- **更多/更少 (More/Less)**：乘法关系，通常出现在技能宝石、辅助宝石和升华 notable 上

> **与 PoB-PoE2 的核对**：PoB-PoE2 的 `CalcOffence.lua` 仍保留 PoB 系列的核心聚合语义：增加/减少修饰词先求和成 additive bucket，多个 `more`/`less` 修饰词作为独立乘区连乘。PoBR 的 `ModDb::sum` 与 `ModDb::more` 应保持该语义[^pob2-deepwiki-damage]。

#### 2.4 暴击 (Critical Hit)

如果暴击检查成功：
- 暴击伤害加成默认 **100%**（有效 200% 基础伤害）
- 防御者可能拥有减少暴击伤害加成的修饰词

#### 2.5 伤害滚动 (Damage Rolled)

在最小和最大伤害数值确定后，滚动确定精确伤害数值。
- **幸运 (Lucky)**：滚动两次，取更有利的结果
- **不幸 (Unlucky)**：滚动两次，取更不利的结果

#### 2.6 双倍/三倍伤害 (Double/Triple Damage)

- **双倍伤害**：一定几率使伤害翻倍
- **三倍伤害**：一定几率使伤害翻三倍
- 三倍伤害总是覆盖双倍伤害（不能同时触发）

### 步骤 3：伤害转换承受 (Damage Taken As)

伤害转移（Damage Shifting）由"伤害承受为"类修饰词提供（如 `Cloak of Flame` 的"50% 物理伤害承受为火焰伤害"）。

- 伤害只能转移一次
- 所有"伤害承受为"修饰词同时应用
- 转移后的伤害失去其固有的伤害类型属性（如穿透）

**注意**：在此步骤之前，由击中施加的 damaging ailment 及其 magnitude 是基于未减免伤害计算的。

### 步骤 4：减伤 (Mitigation)

#### 免疫
- 伤害免疫（如 `Chaos Inoculation` 的修饰词）在此完全防止指定伤害类型

#### 护甲和 PDR
- 护甲和额外物理伤害减免应用于物理击中伤害
- 护甲提供的减免基于 incoming 物理击中计算
- 额外 PDR 修饰词求和并加到护甲提供的减免上

> **0.5.0 更新**：65 级时护甲和闪避数值约增加 33%，80 级以上增加 15%[^maxroll-050-patchnotes]。

#### 护甲应用于非物理伤害
- 如 `Prism Guard` 等修饰词使护甲在抗性之前提供对非物理伤害的减免

#### 抗性
- 火焰、冰霜、闪电和混沌伤害由各自的抗性减轻
- 诅咒、暴露等效果可以将抗性降至 0% 以下，但**穿透不能超过 0% 抗性**

**注意**：伤害减免和抗性是独立的层，各自硬上限为 **90%**。

### 步骤 5：伤害承受修饰词 (Damage Taken Modifiers)

按特定顺序应用：

1. **固定伤害承受**：首先应用（如 `Ashrend`）
2. **增加/减少承受的伤害**：求和后应用（如 `Wither` 和 `Shock`）
3. **更多/更少承受的伤害**：乘法关系（如 `The Dancing Mirage` 和 `Bulwark`）

### 步骤 6：眩晕 (Stun)

如果伤害击中超过目标的**眩晕阈值 (Stun Threshold)**，将施加眩晕。

- 眩晕阈值默认基于最大生命
- 较强的怪物有调整的眩晕阈值
- 击中会积累眩晕；眩晕伤害会积累**重眩晕 (Heavy Stun)**
- **玩家不能被重眩晕**
- 重眩晕属于**姿态 (Poise)** 相关 debuff，所有伤害类型均可贡献积累，物理近战通常有额外加成[^pob2-deepwiki-stun]

### 步骤 7：格挡 (Block)

- 被动格挡几率滚动
- 或玩家可以使用主动格挡（如 `Raise Shield`）

**注意**：某些 Boss 技能不能被格挡（有红色闪光和音效提示）。

格挡击中时，默认防止所有伤害，但击中仍然发生。
- 格挡不会阻止任何击中效果（如眩晕或冻结）

### 步骤 8：最终伤害承受与资源损失 (Final Damage Taken & Loss of Resources)

#### 伤害承受顺序

1. **转移伤害**：其他实体先承受伤害的修饰词（如 `Wooden Wall`）。如果该实体被伤害杀死，剩余伤害重定向回你。
2. **增益/物体**：如 `Sorcery Ward` 或 `Encase in Jade`
3. **守护 (Guard)**：来自护符修饰词或 `Olroth's Resolve`
4. **能量护盾 (Energy Shield)**：
   - 混沌伤害对能量护盾造成**两倍**伤害
   - 毒和流血直接绕过能量护盾
5. **生命 (Life)**：
   - `Mind Over Matter` 等修饰词使伤害先从法力承受
   - 法力耗尽后，剩余伤害重定向回生命
6. **剩余伤害从生命损失**：
   - `Grasping Wounds` 等修饰词阻止生命损失
7. **符咒护佑 (Runic Ward)**：
   - 生命完全耗尽后，伤害由符咒护佑承受

> **0.5.0 更新**："Defences" 关键词已废弃，明确改为 "Armour, Evasion and Energy Shield"，以澄清这些修饰词不适用于符咒护佑[^maxroll-050-patchnotes]。

**注意**：非 damaging ailment（如 `Shock`）的 magnitude 基于击中承受的伤害计算，因此在此阶段计算。

## 实际 DPS vs 工具提示 DPS

工具提示 DPS 不考虑服务器限制（每秒最多 30.3 个动作）和实际游戏中的各种交互。实际 DPS 可能低于工具提示显示的数值。

---

## 参考来源

[^mobalytics-damage-order]: Mobalytics — Damage & Defence Calculation Order. https://mobalytics.gg/poe-2/guides/damage-defence-calc-order
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-calc]: PathOfBuildingCommunity/PathOfBuilding-PoE2 DeepWiki — Calculation Engine / Defence Calculations. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^pob2-deepwiki-damage]: PathOfBuildingCommunity/PathOfBuilding-PoE2 DeepWiki — Calculation Engine / Damage Calculations. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
