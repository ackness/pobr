# PoB2 后续实现计划（缺口路线图）

> 来源：2026-06-10 多 agent 缺口分析，对照 `vendor/PathOfBuilding-PoE2/src/Modules/Calc*.lua` 与 `agent-docs/`。
> 影响/工作量评级来自 verify agent；`partial`=部分已实现、`stub`=有骨架待接线、`missing`=完全缺失。
> 已实现项（`implemented`）不在此列（如 multi-spell trigger rotation、mastery 选择、quality 缩放、effective DPS 模式等）。

## 优先级波次（建议落地顺序）

### Wave 1 — 进攻/防御核心阻塞项（critical + high-impact）

这些直接决定主流构建 DPS/EHP 是否「能算对」，应最先补。

| 特性 | 模块 | 状态 | 工作量 | vendor 参考 |
|------|------|------|--------|-------------|
| Buff/Aura 应用与效果缩放（BuffEffect/BuffEffectOnSelf/mergeBuff） | perform | missing | 大 | `CalcPerform.lua:1944-2145` |
| Curse/Hex/Mark 应用 + 数量上限/优先级 | perform | missing | 大 | `CalcPerform.lua:454, 2286-2333` |
| Leech / Gain-on-Hit 与 DPS 交互 | offence | missing | 大 | `CalcOffence.lua:~3650+` |
| Exposure / 元素 debuff（Flammability/Brittle/Sap…） | offence/perform | missing/partial | 大 | `CalcOffence.lua:~2500-2600` |
| 非伤害异常效果与堆叠（Chill%/Shock%/Freeze/Electrocute buildup via poise） | offence | stub/missing | 大 | `CalcOffence.lua:~4400-4800` |
| 多段/多投射/分裂·连锁·穿透·分叉 方差 | offence | missing | 大 | `CalcOffence.lua:~1200-1400` |
| 穿透 vs Overwhelm（元素 vs 物理） | offence | partial | 中 | `CalcOffence.lua:~3000-3100` |
| 宝石等级/品质/觉醒等级缩放（贯穿伤害聚合） | offence/skill | partial | 中 | `CalcOffence.lua:~2000-2100`、`CalcActiveSkill.lua:82-140` |

### Wave 2 — 防御层与 buff 体系补全（high-impact 防御）

| 特性 | 模块 | 状态 | 工作量 | vendor 参考 |
|------|------|------|--------|-------------|
| Keystone 影响防御顺序（IronReflexes/MindOverMatter/EldritchBattery…） | defence | partial | 大 | `active-defences.md §五` |
| Mind Over Matter（法力承伤，池序插入） | defence | partial | 中 | `CalcDefence.lua` MoM 插入 |
| ES 充能机制（延迟/中断/加速/RecoveryRateMod） | defence | partial | 中 | `energy-shield.md`、`defence.rs::calc_es_recharge` |
| EHP 按伤害类型聚合 + 最低承伤逻辑 | defence/ehp | partial | 中 | `ehp.rs::physical_max_hit_overwhelm` |
| Fortification 层叠减伤 | defence/perform | missing/stub | 中 | `CalcPerform.lua:519-538` |
| Guard 吸收池（Molten Shell 等） | defence | missing | 大 | `CalcDefence.lua:500-570` |
| Evasion 熵值机制（100-tick 记忆，去连续未命中） | defence | missing | 大 | `evasion.md §熵值机制` |
| 流血/中毒/腐化之血绕过 ES 直击生命 | defence | missing | 中 | `energy-shield.md §基本机制` |
| Flask/Charm 摄取·效果·恢复 mod | perform | partial | 大 | `CalcPerform.lua:1430-1600` |
| 状态 buff 群（Onslaught/Adrenaline/Elusive/Convergence/Fanaticism/UnholyMight…） | perform/defence | missing | 小×N | `CalcPerform.lua:539-631` |

### Wave 3 — 技能/触发/召唤/敌方配置（功能完整性）

| 特性 | 模块 | 状态 | 工作量 | vendor 参考 |
|------|------|------|--------|-------------|
| 技能分段选择（multi-mode skills, skillPart） | skill | missing | 中 | `CalcActiveSkill.lua:420-438` |
| Transfigured Gems（替代宝石版本） | skill/data | missing | 中 | `CalcTools.lua:113` |
| 支持宝石：cost multiplier 注入 + 技能类型门控接线 | skill | partial | 小 | `CalcActiveSkill.lua:109-129`、`SkillStatMap.lua` |
| 召唤物 DPS 总量聚合（per-instance × count） | minion/output | partial | 小 | `output.rs::MinionOutput` |
| 召唤物属性灌注接线（StrengthAddedToMinions） | minion/perform | partial | 小 | `minion.rs::write_attribute_infusion_mods` |
| Spectre/Companion hidden damage fixup | minion | missing | 小 | `minions.md §1.3` |
| Mirage Archer / Warrior / Tawhoa（技能克隆机制） | skill/新 mirage.rs | partial | 大 | `CalcMirages.lua` |
| Meta-gem Spirit 预留 + Persistent flag | skill | partial | 小 | `CalcActiveSkill.lua` |
| 敌方配置（等级/诅咒/曝光/Intimidate 等 debuff toggle） | setup/config | partial | 中 | `CalcSetup.lua:681-692, 1205-1250` |
| ConfigOptions 标准开关目录（60+ combat/enemy toggle） | config | partial | 大 | `ConfigOptions.lua:110-300+` |
| 敌方 debuff 群（Wither/Blind/Maim/Intimidate/Unnerve/Crush/Sap…） | perform | missing/stub | 小×N | `debuffs.md §135-207` |

### Wave 4 — 物品系统 / 天赋树扩展（结构性大件）

| 特性 | 模块 | 状态 | 工作量 | vendor 参考 |
|------|------|------|--------|-------------|
| 符文/魂核（Augments）+ 按槽位差异化修饰 | item/data | missing | 大 | `Data/ModRunes.lua`（5005 行） |
| 双武器组分支与对应天赋分配（含副升华职业） | tree/build | missing | 大 | `PassiveSpec.lua:42-44,137-150`、`CalcSetup.lua:792` |
| 腐化隐式（Vaal Orb 结果） | item | missing | 中 | `Data/ModCorrupted.lua` |
| 属性需求 + 转换（Giant's Blood / 魂核转换） | item/setup | partial | 中 | `Item.lua ~70`、`attributes.md` |
| 属性→属性派生转换注入 ModDb（Str→Life 等基础 + 稀有词条） | setup | partial | 小 | `CalcSetup.lua:610-622` |
| 时限/条件类 buff 输入 schema（Banner Valour/Crab Barriers 等） | config | partial | 中 | `ConfigOptions.lua:120-250+` |
| Jewel radius（Timeless/Time-Lost 已实现；Cluster 属 PoE1 legacy，确认不做） | tree | partial | — | `CalcSetup.lua:106-183` |

### Wave 5 — 长尾（low-impact，按需）

Double/Triple damage 概率、Lucky/Unlucky、Bifurcated/Inevitable crit、Critical Weakness、AoE 半径断点、Culling、资源消耗与效率、Repeats/Multicast、Skill duration/cooldown 细节、Recoup、Doom 堆叠、Banner、Herald、Party/ally buff 导出、Runic Ward（0.5.0+ 池）、Deflection（0.5.0）、Block recovery、Heavy Stun、Chaos 双倍打 ES、Zealot's Oath、护甲应用于元素伤害、ArmourBreak/NegativeArmour、ilvl/affix tier 门控、Ancient Augment 全局上限、Vaal 品质等。

> **明确不做**（PoE1 legacy，PoE2 已无）：Cluster Jewel 子图分配（`PassiveSpec.lua:84-85,336+`）、Impale（`CalcOffence.lua:~5000`，PoE2 仅余 Soul Core 来源时再议）、Spell Suppression（本次已移除）。

## 接线优先建议（"partial" 速赢）

以下已有骨架/字段、只差接线，工作量「小」即可点亮，建议穿插在各 Wave 优先做：

- 支持宝石 cost multiplier / skill-type 门控（`skill_source.rs` 已有 `can_support()`/字段，缺 perform 侧调用）；
- 召唤物属性灌注 / DPS×count 聚合；
- 属性→派生属性 ModDb 注入；
- 各类一行式敌方 debuff（Wither/Blind/Maim…，多为固定 INC/MORE）；
- 状态 buff 群（Onslaught/Adrenaline/… 多为固定数值 buff）。
