# POE2 游戏机制文档

本目录包含 Path of Exile 2（流放之路2）的游戏机制文档，内容基于官方 Wiki、社区指南、补丁说明和 Path of Building 项目数据整理而成。文档包含原始引用链接，并标注了 0.5.0 版本更新中的机制变化。

## 文档索引

### 核心伤害机制

| 文档 | 内容 |
|------|------|
| [damage-types.md](damage-types.md) | 五种伤害类型：物理、火焰、冰霜、闪电、混沌。包括与属性的关联、减伤方式、相关异常状态 |
| [ailments.md](ailments.md) | 异常状态系统：流血、毒、点燃等，含 0.5.0 流血移动增伤移除说明 |
| [critical-hits.md](critical-hits.md) | 暴击机制：暴击几率计算、基础暴击率来源、暴击伤害加成、幸运/不幸机制 |
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

### 伤害与防御计算流程

| 文档 | 内容 |
|------|------|
| [damage-defence-order.md](damage-defence-order.md) | 完整的8步伤害防御计算顺序，含 POB 计算引擎核对说明 |

### 物品与制作系统

| 文档 | 内容 |
|------|------|
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
