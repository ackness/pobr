# 时迭珠宝搜索与天赋珠宝支持

2026-09-17 对照 PoB2 `ce566eac45ea8a86477f513c7ee65a1ebe60014e`。
查询官方仓库时，该提交也是最新 HEAD；不把 PoE1 的 Timeless 种子算法套用到 PoE2。

## 使用方式

在「升级」页选择一个已分配的珠宝插槽，将「珠宝类型」设为「时迭珠宝搜索」，
选择 DPS、生命或自定义目标后计算词缀分数。天赋树的珠宝编辑器也提供此入口。
已装备时迭珠宝会自动识别类型。搜索按当前武器组、该插槽周围已分配的小型及显著
天赋实际重算，再生成官方交易网站的加权查询；价格、服务器和赛季沿用现有设置。

传奇珠宝可以直接粘贴到天赋树的珠宝编辑器，或随 PoB 构筑导入。选择插槽后会显示
受影响的圆形／环带；环带内可独立分配的节点有高亮。转换后的节点名称和词条用于
搜索、悬浮提示与实际计算。替代职业起点不占天赋点；未连回职业起点的环带节点
不能作为向外加点的路径起点。取下或替换珠宝会回收失去支持的已知依赖点，保留
正常连接、其他珠宝仍支持的点和无法解释的导入节点。

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

`overlay/passive_jewels.json` 由 `pipeline/extract-passive-jewels.lua` 从固定的 PoB2
源代码快照导出，已接入 `regen-all.sh`。职业起点以稳定 ID 匹配所选树，环带选择、
征服者、基石名称和词条都来自数据。普通数值／文本变化随数据再生生效；上游程序性
变换结构变化会使提取失败，保留旧产物并要求复核，不能静默沿用旧常数。
旧包缺少此文件仍可加载；新增的独立分配和征服功能保持禁用，原有半径词条授予不受影响。
没有环带选择数据时不会猜测有歧义的 Variable 半径。
替换节点词条和属性点附加词条同时进入词条审计，分别保留节点／家族来源及范围端点样本。
半径授予词条与计算共用目标解析器，审计单独记录 `radius_grant` 载荷并验证其解析结果，
不会把半径效果记作普通全局词条；未知目标或载荷仍报缺口。

计算视图按 PoB 的 Variant、Alt Variant、Version 和分组选择筛选词条，再处理范围
取值；原始物品文本仍保留用于编辑和导出。范围取值沿用现有线性解析器，复杂的
多值联动／催化数值转换仍属于其既有限制。

## 传奇珠宝调研结论

| 类别 | PoB2 的机制 | PoBR 当前范围 |
| --- | --- | --- |
| Against the Darkness / 时迭 grant | 向半径内对应类型天赋添加词缀 | 导入后按分配节点展开；本轮补小型效果、半径升级及重叠效果处理。时迭搜索只取普通生成词缀池，尚无该传奇的独立专属词缀搜索 |
| Voices | 额外 Sinister 插槽 | 本轮移除固定版本数字节点数组，按所选树稳定 ID 和分配数量启用；导入与手动编辑共用路径 |
| The Adorned | 腐化魔法珠宝效果提高 | 已有计算；特殊插槽豁免仍需单独验收，不能由本轮搜索测试推导完全支持 |
| Grand Spectrum | 按同名已镶嵌珠宝数缩放 | 不属于天赋点替换；已有计数/词条路径，本轮未扩展其机制 |
| Controlled Metamorphosis | 可变环带内允许断开分配 | 已接入环带边界、单点分配、路径限制、回收及自动规划 |
| Split Personality / From Nothing | 从其他职业起点或指定核心周围分配 | 已接入额外起点／核心半径分配，随提供珠宝启用或撤销 |
| Heroic Tragedy | 征服、核心替换、种子相关节点变换 | 三种固定基石已参与计算，包含移除属性固有收益；种子相关普通／显著节点保留未支持诊断 |
| Undying Hate | 深渊核心替换、Tribute、种子相关变换 | 已替换固定核心、普通点和属性点附加 Tribute；核心的复杂效果仍按现有计算支持范围处理，未解析词条明确诊断；显著节点种子结果不可用 |

[PassiveSpec.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/PassiveSpec.lua)
包含征服、部分核心/深渊节点替换及手动覆盖逻辑，但显著节点种子表相关分支仍注释为待获得数据。
[TreeTab.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/TreeTab.lua)
的 `Find Timeless Jewel` 按钮也被注释禁用。这不是本轮实现的 Time-Lost / Radius 搜索。
PoB2 的固定数据写明 Circular Teachings 移除敏捷的固有收益，但本次固定版本的解析器
尚未识别其单独词条。本实现补上 `NoDexBonusToAccuracy` 规则及实际消费者，按该词条
语义移除敏捷命中收益；这项修复不宣称与该版本 PoB2 的遗漏行为数值一致。

缺失种子结果的节点不能被自动规划新增或退还；已导入构筑会保留诊断，不把原始天赋
数值当作已验证的完整种子结果。多个受数量限制的传奇珠宝同时装备的合法性检查尚未
覆盖，本轮测试使用合法单件配置。

旧数据包的历史树若不含坐标，半径效果无法可靠计算；不会借用当前树坐标伪造结果。
目前搜索覆盖已分配的普通树插槽，额外授予的特殊插槽可以计算已导入珠宝，搜索界面尚不枚举它们。
