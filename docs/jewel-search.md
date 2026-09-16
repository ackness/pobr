# 时迭珠宝搜索与天赋珠宝支持

2026-09-17 对照 PoB2 `ce566eac45ea8a86477f513c7ee65a1ebe60014e`。
查询官方仓库时，该提交也是最新 HEAD；不把 PoE1 的 Timeless 种子算法套用到 PoE2。

## 使用方式

在「升级」页选择一个已分配的珠宝插槽，将「珠宝类型」设为「时迭珠宝搜索」，
选择 DPS、生命或自定义目标后计算词缀分数。天赋树的珠宝编辑器也提供此入口。
已装备时迭珠宝会自动识别类型。搜索按当前武器组、该插槽周围已分配的小型及显著
天赋实际重算，再生成官方交易网站的加权查询；价格、服务器和赛季沿用现有设置。

新珠宝的词缀探测使用 Small 半径，与 PoB2 的 Radius 搜索参考物品一致。
同基础的已装备珠宝保留实际半径和升级词缀。加权分数不能完整表达所有词缀间的
交互，购买前可把完整商品文本粘贴到换装比较中重算。

## 实现依据

- [TradeQueryGenerator.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/TradeQueryGenerator.lua)：
  Base/Radius 类型过滤、Small 半径参考珠宝、按计算目标探测交易词缀。
- [Data.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Modules/Data.lua)：
  原始 `ModJewel` 的 `nodeType` 决定补上小型或显著天赋 grant 前缀。
- [ModParser.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Modules/ModParser.lua) 与
  [CalcSetup.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Modules/CalcSetup.lua)：
  grant 的目标类型、半径升级、局部小型/显著天赋效果；全局与局部小型效果相加后统一取整。

基础物品、生成权重、词缀数值、交易 ID 来自当前数据包的 `trade_catalog.json`；
半径及乘数来自 `jewel_radii.json`，节点及坐标来自所选树。
半径表按数字版本选择不晚于目标版本的一组，不能按字符串排序，也不能套用未来版本。
新数值和同类 grant 词缀无需新增 Rust 分支。grant 解析为结构化属性后逐项缩放，
恢复等小数词缀复用 `high_precision_mods.json`，伤害区间的两端都参与缩放。
新机制仍需要明确的计算消费者与验证。

## 传奇珠宝调研结论

| 类别 | PoB2 的机制 | PoBR 当前范围 |
| --- | --- | --- |
| Against the Darkness / 时迭 grant | 向半径内对应类型天赋添加词缀 | 导入后按分配节点展开；本轮补小型效果、半径升级及重叠效果处理。时迭搜索只取普通生成词缀池，尚无该传奇的独立专属词缀搜索 |
| Voices | 额外 Sinister 插槽 | 本轮移除固定版本数字节点数组，按所选树稳定 ID 和分配数量启用；导入与手动编辑共用路径 |
| The Adorned | 腐化魔法珠宝效果提高 | 已有计算；特殊插槽豁免仍需单独验收，不能由本轮搜索测试推导完全支持 |
| Grand Spectrum | 按同名已镶嵌珠宝数缩放 | 不属于天赋点替换；已有计数/词条路径，本轮未扩展其机制 |
| Controlled Metamorphosis | 可变环带内允许断开分配 | 环带数据已入库，天赋规划器尚未接入环带分配约束 |
| Split Personality / From Nothing | 从其他职业起点或指定核心周围分配 | 需要天赋规划器的额外起点/分配约束；本轮没有宣称支持 |
| Heroic Tragedy / Undying Hate | 征服、核心替换、种子相关节点变换 | 尚无完整种子变换消费链；保留原始物品文本，不生成虚假的种子结果 |

[PassiveSpec.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/PassiveSpec.lua)
包含征服、部分核心/深渊节点替换及手动覆盖逻辑，但显著节点种子表相关分支仍注释为待获得数据。
[TreeTab.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/TreeTab.lua)
的 `Find Timeless Jewel` 按钮也被注释禁用。这不是本轮实现的 Time-Lost / Radius 搜索。

旧数据包的历史树若不含坐标，半径效果无法可靠计算；不会借用当前树坐标伪造结果。
目前搜索覆盖已分配的普通树插槽，额外授予的特殊插槽可以计算已导入珠宝，搜索界面尚不枚举它们。
