# 加权交易搜索中的特殊词缀

市场搜索的分数是官方交易查询的加权总和（Sum），不是 DPS。PoBR 通过替换词缀重算构筑，估计各交易属性的权重，再用同一组权重给当前装备评分。购买前仍应把完整商品放入换装比较。

## 数据来源

PoB2 参考版本由活动数据的 `overlay/mod_parser_rules.json::_meta.vendor_commit` 指定。首次实现对照 `ce566eac45ea8a86477f513c7ee65a1ebe60014e` 的以下文件；这个旧 hash 不表示现在的上游 HEAD：

- `TradeQueryGenerator.lua`：`InitMods`、`ProcessMod`、`canModSpawnForItemCategory`、`GenerateModWeights`。
- `ModItem.lua`、`ModVeiled.lua`（亵渎）、`ModCorrupted.lua`、`ModJewel.lua`、`Essence.lua`、`TradeSiteStats.lua`。

PoB2 除普通生成权重外，还补完美精华、Genesis Tree 制作和符文影响类别，但其初始化没有提供 Alloy 类别覆盖。PoBR 从同版本游戏 `EssenceMods`、`Essences`、`EssenceTargetItemCategories` 读取配方，包括 `OutcomeMods` 中真实的替代结果及腐化精华。表经现有 GGG 管线获取，`trade_crafting_sources.json` 保留 modifier ID、来源种类、物品类别、需求等级、stat ID 和取值范围；游戏配方类别优先于 PoB2 精华回退。

例如 `AlloyCastSpeedDamageAsExtraColdHybridOneHand1` 属于 Wand/Focus，更强的 `AlloyCastSpeedDamageAsExtraColdHybrid1` 属于 Staff。它们普通生成权重为零，仍可出现在成品搜索中。不能凭名字猜类别，也不能给所有武器。

## 搜索与评分规则

- `mods` 是普通词缀池。可选 `search_mods` 追加 alloy、essence、desecrated、breach、influence、corrupted 来源，并保留类别限制；没有该字段的旧目录仍可加载。
- 一个官方交易 hash 只重算一次、只得一个查询系数。复合词缀的不同 hash 是独立属性；同 hash 的多行导出文本先合并、只计一次，`source_lines` 也用于匹配当前装备的换行文本。
- 一个可变占位符用该值，两个用均值，没有占位符则按存在标志取 1。条件说明中的固定数字不参与均值，位置由 `value_indices` 记录。超过两个可变占位符或数值模板不一致时排除，不猜值。
- `trade_line` 保留官方措辞，包括与 PoB 导出相反的描述／符号；两种写法匹配同一 ID 与数值。ID 的 `|选项` 后缀是离散选项，不是别名；查询和评分保留完整 ID，Medium/Large/Very Large 珠宝半径分别匹配。
- 制作、破裂、亵渎的显式词缀统一用 `explicit.stat_*`，不按来源另造重复项；腐化用 `enchant.stat_*`，符文增强和普通隐式分开。分配天赋附魔有独立交易 ID，不混入这一类。
- 删除复合属性时删除全部关联行；删除计入隐式数量的附魔时同步 `Implicits:`，避免下一条显式词缀变成隐式。

当前装备最低分使用**最终发出的查询系数**，包括舍入和最多 32 个过滤器。只识别一部分相关文本时，仍保留已识别部分的 Sum 作为最低分，并显示 `partial-current-score` 提醒。所有入选权重为正，因此已识别分数是完整可匹配分数的下界；不能因为剩余行未知就撤掉最低分，也不能把部分分数说成完整装备价值。当前实现为 `max(0, ceil((score − 1e-9) × 1000) / 1000)`。这是 [#170](https://github.com/ackness/pobr/pull/170) 合并后的行为，见 [tradeOptimizer.ts](../web/src/lib/tradeOptimizer.ts) 与其测试。

PoB2 的 PoE2 pseudo-stat 映射有意留空，因为伪属性可能混入镶嵌物贡献。PoBR 同样保留实际带命名空间的 hash，不用伪总量替换复合属性。

## 计算支持边界

特殊属性和普通属性一样，分别在空装备、当前装备环境中添加／移除，比较目标变化。如果未支持效果诊断发生变化，就拒绝该次评分；移除未知部分后使其余词条突然能注入的情况也拒绝，防止产生虚高权重。

文本能解析仍不代表效果生效，还要有计算读取者并满足当前条件。零分不证明该词条在游戏中无用；不能为凑正分推算触发频率或层数。

固定 vendor 的 `ModItem.lua` 缺 `AlloyPuppetMasterChance1` / `EssenceGrantedPassive`，但其 stat 描述和官方交易条目仍有定义。回退用 GGG stat ID 选描述，并要求官方条目无歧义。未知复合描述、变换和限制继续保留未映射；当前配方包没有未映射 modifier ID。

| 来源 | 搜索方式与限制 |
|---|---|
| `AlloyPuppetMasterChance1` | Sceptre、等级 25，使用 Command 时 30–50% Surpassing Chance 获得 Puppet Master 层，交易 ID `explicit.stat_2840930496`。两边引擎都未模拟触发／层数覆盖，只提供明确标为未建模的可选必需过滤器，不生成 DPS 系数 |
| `EssenceGrantedPassive` | 胸甲、等级 1；`[0, 0]` 是动态天赋引用，不是零收益。按 `TreeData/0_5/tree.lua` 校验官方 `Allocates <name>` 的 ID 与名字，当前生成 875 个候选，每项值为 1，ID 为 `explicit.stat_2954116742|<node>`。这是交易候选清单，不是制作概率或所有节点均已支持的证明 |

分配天赋的重算走 `GrantedPassive`，已分配／已授予的显著天赋不重复。候选替换当前装备的授予，其他装备涂油保留。先完成全部必需单属性重算，再进行有预算的组合搜索；必要时把组合预算至少提高到这些必需项数量，同时保留分批、进度和取消。当前装备文本只匹配一次，供全部移除比较共用。

同类普通数值配方可复用描述回退，无需每条新增代码；新效果或新描述变换仍需各自支持与回归。升级树版本时，也要更新回退使用的树源。

参考组合搜索和手动词缀编辑器仍只使用与基底兼容的普通前／后缀池。特殊来源目录不是制作模拟器：不证明多种制作可共存、不估成本或成功率，也不保证搜索结果就是最优购买。

## 数据再生与检查

修改此数据功能时，在仓库根目录先导出同版本 GGG 表，再按需要运行以下命令。**仅改本文时不运行这些构建或测试**；本次文档审查只检查 diff 与链接。

```sh
trade_overlay="data/$(cat data/CURRENT)/overlay"
mise exec -- python3 pipeline/extract-trade-crafting.py pipeline/tables/English "$trade_overlay/trade_crafting_sources.json"
mise exec -- luajit pipeline/extract-trade-catalog.lua vendor/PathOfBuilding-PoE2/src "$trade_overlay/trade_catalog.json" "$trade_overlay/trade_crafting_sources.json"
mise exec -- luajit pipeline/extract-trade-map.lua vendor/PathOfBuilding-PoE2/src "$trade_overlay/trade_stat_map.json"
mise exec -- python3 pipeline/test-trade-crafting.py
mise exec -- luajit pipeline/test-trade-stats.lua
mise exec -- luajit pipeline/test-trade-crafting-fallback.lua
mise run web test src/lib/tradeCatalog.test.ts src/lib/equipmentScore.test.ts src/lib/tradeOptimizer.test.ts
mise run web sync-data
mise run web build
mise run web:e2e e2e/trade-upgrades.spec.ts --workers=1
```

`pipeline/regen-all.sh` 按依赖顺序执行两个提取阶段。没有原始 GGG 表的 checkout 可把已提交 crafting sources 作为 catalog 提取器第三个参数；省略时只有 PoB2 来源覆盖。
