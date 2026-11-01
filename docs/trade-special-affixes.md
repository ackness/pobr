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

游戏事实由 RePoE 独立保存和处理；PoBR 在构建或实物测试中临时导入固定产物。当前 [v2 构建锁](../pipeline/build-data.lock.json) 强制读取独立 `repoe-native-crafting-facts/v1` 包，游戏 part 不再携带旧 crafting 成品。PoBR 的[投影器](../pipeline/crafting_inputs.py)只负责 UI 前缀筛选、类别聚合及已识别 regular-stat 目录；缺包或 pin 不符即失败。

RePoE 从同一 F77 快照中的 `BaseItemTypes`、`ItemClasses`、`Mods`、`Stats`、`Essences`、`EssenceTargetItemCategories`、`EssenceMods` 导出中立关系，保留七表原字节、schema／decoder／producer 与原输入 manifest。原生 Mods 使用 `Stat1`–`Stat8`、`StatNMin/Max` 及 hardmode 通道，不使用旧四槽 `StatNValue` 视图。目标 0.5.5.5 international 是继承的声明；该导出不新增观测客户端身份或扩大来源接受。F77 缺失的语言声明保持未知，不默认 English；物理 ID／数值关联不推断来源语言。

`Mod`、`DisplayMod`、有序 `OutcomeMods` 与原权重分别保留。仅在 PoBR 目录政策中，非空 `OutcomeMods` 优先于非空 `Mod`，不使用 `DisplayMod` 替代。目录保留八槽已识别 stat 的零／负数区间；null stat 的非零原值、hardmode 未投影和源诊断写入运行包外的构建输入收据。它不是完整效果包、制作概率或全部适用性认证，见[详细契约](../pipeline/crafting-inputs.md)。

当前有界对比从 89 个源行得到 92 条目录记录，业务字段与历史基线一致。7 处权重不齐位于未选取的普通精华，仍保留源诊断；`AlloyBallistaLimit1` 的 null stat 槽带 `-20/-20` 原值，保持未表达诊断，不补 ID／0 或宣称 effectless。新文件使用准确来源元数据，快照身份相应更新，不强留旧生成命令。

历史输出的 31,192 bytes／SHA-256 `6153ccd3eee148312fac673c9669896c66c8ab1cf712167a5e836544821a4202` 及旧提取器 SHA-256 `9cd3a04aa6a703312b55fb8c93b606cbbc0370ebd7fbf5ade64a5349fc51c7f2` 只固定旧身份；原始执行输入、补丁与服别仍未知。新目标业务相等不能证明恢复旧执行或全部游戏语义。旧 raw 提取器已删除，不再把它当成构建回退。

```sh
mise run data:prepare
# For a separate regeneration candidate with a matching build lock:
mise exec -- python3 pipeline/crafting_inputs.py --patch 0.5.5.5 \
  --out "$POBR_CANDIDATE_OVERLAY/trade_crafting_sources.json"
mise exec -- luajit pipeline/extract-trade-catalog.lua vendor/PathOfBuilding-PoE2/src "$POBR_CANDIDATE_OVERLAY/trade_catalog.json" "$POBR_CANDIDATE_OVERLAY/trade_crafting_sources.json"
mise exec -- python3 pipeline/test-trade-crafting.py
mise exec -- luajit pipeline/test-trade-stats.lua
mise exec -- luajit pipeline/test-trade-crafting-fallback.lua
mise run web test src/lib/tradeCatalog.test.ts src/lib/equipmentScore.test.ts src/lib/tradeOptimizer.test.ts
```

`pipeline/regen-all.sh` 在昂贵生成器前核对 crafting 目标，生成失败中止整个候选；后续 catalog 的文本／交易 hash 关联仍有独立固定 vendor 与回归边界。单独省略 catalog 提取器第三个参数只产生 vendor 覆盖，不证明已消费制作事实。完整数据只留在独立来源仓库或临时构建／测试目录。
