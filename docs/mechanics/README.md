# 机制约定

这里记录 PoBR 当前怎样计算各机制、依据是什么，以及哪些地方只是配置或近似。[维护者指南](../maintainer-guide.md)只保留跨主题的规则，单个机制的约定写在这里，需要时打开对应文件或直接搜索 stat id、函数名。

| 文件 | 内容 |
|------|------|
| [offence.md](offence.md) | 命中与技能类型、无伤害效果、伤害转换与获得、敌人抗性与曝露、Culling Strike、条件与 buff、使用速率、弹药与范围 |
| [defence.md](defence.md) | 资源池与转换（Runic Ward、Widow's Reign）、抗性与减伤、恢复与偷取、吸纳、能量护盾充能、充能事件、自身减益与减速 |
| [ailments.md](ailments.md) | 伤害型异常、冰缓／感电／冻结、晕眩与钉住、poise、破甲与命中效果视图 |
| [minions.md](minions.md) | 召唤物的创建、技能、数量与诅咒，光环与友军共享，同伴，图腾与 Ballista |
| [items.md](items.md) | 装备条件、暗金与珠宝、宝石等级与品质通道、授予技能、属性需求与消耗、药剂与护符 |
| [snapshots.md](snapshots.md) | 配置项与敌人配置，以及按显式快照计算的事件（祖灵增幅、Archon、地面与护符事件、箭袋、残骸、Volatility、Combo） |

与 PoB2 结果不同的地方另在 [pob2-differences.md](../pob2-differences.md) 登记；输出字段的含义见 [calc-outputs.md](../calc-outputs.md)。

本文各页已按提交 `81f8733e`、活动数据 `0.5.5.4.2` 核对。后续游戏网页可能更新；版本相关结论优先看入库的游戏 stat、技能数据和官方补丁，PoB2 只作交叉检查。已有对照数字保留其原调查日期／样本，不表示本次重跑。

常用术语：`BASE` 是固定追加量，`INC` 是相加的增加／减少，`MORE` 是相乘的更多／更少；“读取者”指实际用到该词条的计算。`exporter: absent` 表示钉定导出器缺此规则，参考比较会排除；`differs` 表示两边读法不同。`unmodelled` 表示已识别但尚未计算，不能当作无效果。

## 写一条约定

- 先按[静态模拟的范围](../maintainer-guide.md#静态模拟的范围与取舍)确认这个机制值得实现。
- 写进最贴近的文件和小节，一个机制一段，不写进维护者指南。
- 分清三件事：游戏文字／数据说了什么，当前代码做了什么，用户配置或模型假设了什么。每段写明：游戏出处（stat id、词条原文、游戏表或官方说明，可带链接）；实现位置（模块或函数）；所做的假设，没有游戏内实测时写明 **没有游戏内实测 / no in-game test**；不推算的事件频率、覆盖率或触发历史；词条规则的 `exporter` 状态（`absent` / `differs`）。
- 结果与 PoB2 不同、且有逐构筑的数字时，差异写进 [pob2-differences.md](../pob2-differences.md)，逐行登记到 `devs/scripts/known-differences.json`。
- 机制改变后同步修改或删除旧段落；被代码取代的叙述不保留。
