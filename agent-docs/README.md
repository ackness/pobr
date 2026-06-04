# POE2 游戏机制文档

本目录包含 Path of Exile 2（流放之路2）的游戏机制文档，内容基于官方 Wiki、社区指南、补丁说明和 Path of Building 项目数据整理而成。文档包含原始引用链接，并标注了 0.5.0 版本更新中的机制变化。

## 文档索引

### 核心伤害机制

| 文档 | 内容 |
|------|------|
| [damage-types.md](damage-types.md) | 五种伤害类型：物理、火焰、冰霜、闪电、混沌。包括与属性的关联、减伤方式、相关异常状态 |
| [damage-scaling.md](damage-scaling.md) | 伤害缩放细节：added/increased/more 叠加顺序、伤害效率、转换链与双重 dip、Gain as Extra、幸运/不幸伤害、双倍/三倍伤害、Overwhelm vs 穿透 vs 曝光、Hit vs DoT、斩杀，附 PoB2 公式 |
| [ailments.md](ailments.md) | 异常状态系统（详）：异常/姿态双阈值、强度与几率派生、点燃/流血/中毒 DoT 公式、冰缓/冰冻/感电/电击积累、规避免疫、叠层与增殖，含 0.5.0 变化 |
| [critical-hits.md](critical-hits.md) | 暴击机制：暴击几率计算/上限、基础暴击率来源、暴击伤害加成(爆伤)、幸运/不幸、重投、分岔暴击、必然暴击、暴击弱点(含 Malice 光环)、闪避降级，附 PoB2 `CalcOffence` 公式与 pobr 实现启示 |
| [skill-speed.md](skill-speed.md) | 技能速度：通用技能速度、特定技能速度、计算公式、服务器限制（30.3 APS 上限） |

### 防御机制

| 文档 | 内容 |
|------|------|
| [armour.md](armour.md) | 护甲机制：伤害减免公式（AR/(AR+10*DMG)）、PDR、护甲击破、0.5.0 数值调整、POB 公式差异 |
| [evasion.md](evasion.md) | 闪避机制：闪避几率公式、熵值机制、偏转公式（0.5.0 更新）、POB 公式差异 |
| [energy-shield.md](energy-shield.md) | 能量护盾：充能机制、混沌伤害双倍效果、0.5.0 偷取重制、数值调整 |
| [runic-ward.md](runic-ward.md) | 符咒护佑：Runes of Aldur (0.5.0)、Runeforging、Kalguuran Skills、合金与符文、在防御体系中的位置 |
| [resistances.md](resistances.md) | 抗性系统：元素抗性/混沌抗性、默认上限75%、硬上限90%、负抗性与穿透 |
| [block.md](block.md) | 格挡机制：被动格挡/主动格挡、法术压制、格挡恢复、无法格挡的Boss技能 |
| [stun.md](stun.md) | 眩晕机制：眩晕阈值（基于最大生命）、重眩晕、50% More 物理近战重眩晕积累 |
| [active-defences.md](active-defences.md) | 主动/进阶防御：翻滚闪避(无敌帧)、守护吸收(Molten Shell/Guard)、规避(异常/眩晕/伤害)与抵抗的区别、减少受到暴击额外伤害、防御向 Keystone(CI/EB/MoM…)，并标注 PoE2 已移除的法术压制/Acrobatics |
| [accuracy-and-enemy.md](accuracy-and-enemy.md) | 命中与敌人参数：精准值来源/进攻命中率公式、法术必中、怪物等级缩放表(life/acc/eva/armour)、Boss 四档(默认 Pinnacle)加成与惩罚、敌人配置项、有效 vs 面板 DPS 口径(mode_effective) |

### 减益与控制

| 文档 | 内容 |
|------|------|
| [debuffs.md](debuffs.md) | 减益/控制：诅咒(Hex vs Mark、Doom、上限、Boss减效)、曝光(取最强)、凋萎、破甲/完全破甲、残废/致盲/钉刺/眩目、威吓/挫志/碾压等增伤debuff，附 enemy modDB 建模 |

### 恢复 · 充能 · 增益

| 文档 | 内容 |
|------|------|
| [recovery-charges-buffs.md](recovery-charges-buffs.md) | 充能(Power/Frenzy/Endurance，PoE2 无固有属性)、偷取(0.5.0 重制)/再生/Recoup、增益与 BuffEffect、承受伤害乘数与 Max Hit/EHP、Spirit 保留与 Presence |

### 伤害与防御计算流程

| 文档 | 内容 |
|------|------|
| [damage-defence-order.md](damage-defence-order.md) | 完整的8步伤害防御计算顺序，含 POB 计算引擎核对说明 |

### 技能功能机制

| 文档 | 内容 |
|------|------|
| [skill-mechanics.md](skill-mechanics.md) | 技能功能面：技能标签/类型、范围(√area 缩放)、投射物(Split→Pierce→Fork→Chain 优先级)、持续时间、冷却与储存次数、消耗/保留、重复(Echo)、引导/持续，附 SkillType 枚举与 ModName |
| [triggers.md](triggers.md) | 触发机制：on Hit/Crit/Kill/Ailment/Block 等来源、元宝石能量(Energy)量化、触发冷却与速率上限(ServerTick≈30.3)、CoC/CWC 对应物、多技能轮转模拟、与 Spirit/冷却交互 |
| [minions.md](minions.md) | 召唤物：作为独立 Actor 复用 calc、怪物式等级缩放、player→minion 修饰词传递三通道(MinionModifier/盟友buff/属性灌注)、内禀必中/爆伤+70、虚拟武器伤害、数量上限/存活/Spirit 保留 |

### 物品与制作系统

| 文档 | 内容 |
|------|------|
| [item-character-systems.md](item-character-systems.md) | 物品与角色系统：稀有度/数量(MF)、物品等级与词条池、需求、插槽/符文/灵魂核心(PoE2 无连线)、品质、宝石等级/品质、被动点/觉醒/respec、珠宝、瓶子/护符 |
| [currency.md](currency.md) | 通货系统：所有主要通货的效果、稀有度分级、碎片系统、0.5.0 神圣/瓦尔变化 |
| [crafting.md](crafting.md) | 物品制作：修饰词类型、稀有度、前缀/后缀、物品等级与修饰词等级、0.5.0 通货变化 |

### 宝石系统

| 文档 | 内容 |
|------|------|
| [gems.md](gems.md) | 宝石系统：技能宝石、辅助宝石、精神宝石、宝石等级与品质、0.5.0 Gemling Legionnaire 变化 |
| [meta-gems.md](meta-gems.md) | 元宝石：触发型元宝石、能量机制、敌人力量、精神保留 |

### 角色基础

| 文档 | 内容 |
|------|------|
| [attributes.md](attributes.md) | 属性系统：基础生命/魔力/精准、职业起始属性、力量(+2生命)、敏捷(+6精准)、智力(+2法力) |
| [campaign-rewards.md](campaign-rewards.md) | 战役永久奖励、Seven Pillars/Qimah 选择、抗性进度惩罚、Venom/Shark Fin/纹身奖励 |

### 数据与解包

| 文档 | 内容 |
|------|------|
| [content-ggpk.md](content-ggpk.md) | GGPK文件格式：Content.ggpk 结构、解包工具（ggpk-tool、VisualGGPK2等）、.dat文件解析 |

## 数据来源与核对

主要参考来源：
- https://www.poe2wiki.net/wiki/Path_of_Exile_2_Wiki
- https://poe2db.tw/
- https://mobalytics.gg/poe-2/guides
- https://maxroll.gg/poe2/resources
- https://www.sportskeeda.com/mmo

**Path of Building 核对**：PoE2 计算、章节奖励、配置项和展示字段优先与 [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 交叉核对。旧 [PathOfBuildingCommunity/PathOfBuilding](https://github.com/PathOfBuildingCommunity/PathOfBuilding) 仅作为 PoE1 历史架构和差异参考。

**本地 PoB2 源码**：仓库内 `vendor/PathOfBuilding-PoE2/` 是 PoB2 的本地检出，但为**部分检出**——只含 `src/Data/`（常量与数据表，如 `Misc.lua`、`Global.lua`、`SkillStatMap.lua`、`Costs.lua`、`Gems.lua`、`ModFlask.lua`、`ModCharm.lua`、`ModJewel.lua`、`ClusterJewels.lua`、`Bases/`、`Uniques/` 等）与 `src/Export/`，**不含 `src/Modules/` 计算引擎**（`CalcOffence.lua` / `CalcDefence.lua` / `CalcSetup.lua` / `CalcPerform.lua` 等）。因此：数据/常量/词条映射优先读本地 vendor；计算公式需用 `gh api repos/PathOfBuildingCommunity/PathOfBuilding-PoE2/contents/src/Modules/<File>.lua --jq '.content' | base64 -d` 取远程。

## 版本说明

- 文档已更新至 **0.5.0 Return of the Ancients** 版本
- 标注了 0.5.0 中的关键机制变化：
  - "Defences" 关键词废弃，明确为 "Armour, Evasion and Energy Shield"
  - 护甲/闪避数值调整（65级约+33%）
  - 偷取机制重制（单实例、上限40,000）
  - 偏转公式调整（上限95%）
  - 流血移动增伤移除（仅对玩家承受流血）
  - 神圣宝珠/腐化效果改为基于当前值倍增
  - 蜕变/增幅宝珠掉落率降低
  - Runes of Aldur 联盟机制（Kalguuran Skills、合金、Augment Runes）
  - 战役永久奖励与 Seven Pillars/Qimah 可重选奖励
- 游戏可能会在后续更新中继续调整数值和机制
