# 配置/Build/展示层

> 领域：ConfigOptions / Build / BuildDisplayStats / CalcSections / CalcBreakdown
> 审计日期：2026-06-10 ｜ 角色：只读分析 ｜ 全部 high/medium 缺口已逐条核查（见附录）

## PoB2 代码结构（结构地图）

该领域由 5 个文件组成，数据流为：

```
Build XML <Config><Input>
  → ConfigTab 读 ConfigOptions 条目
  → 每条目 apply(val, modList, enemyModList, build) 把配置翻译成 Modifier，注入玩家/敌人 modDB
  → calc 输出 output 表
  → BuildDisplayStats（侧边栏）与 CalcSections（Calcs 页）按声明式表渲染
  → CalcBreakdown 生成公式分解文本
```

| 文件 | 行数 | 职责 | 关键结构 |
|------|------|------|----------|
| `Modules/ConfigOptions.lua` | 2323（已核实） | 配置条目大表 configSettings | 542 个条目（`grep -cE '\{ var = "'`=542，已核实）；7 个 section：General(:112) / Skill Options(:208) / When In Combat(:796) / For Effective DPS(:1620) / Enemy Stats(:1958) / Custom Modifiers(:2277) / Quest Rewards(:57 动态生成) |
| `Modules/Build.lua` | 2499（已核实） | buildMode 主控 | 计算相关非 UI 部分：Init 装载 displayStats(:310)；`EstimatePlayerProgress`(:1001-1058) 由已点天赋点数反推角色等级（`characterLevelAutoMode`，:1117 从 XML 读）；`Save`(:1153-1202) 把 displayStats 当前值 + extraSaveStats 写回 XML 供第三方工具；`FormatStat`(:2067) / `AddDisplayStatList`(:2090-2105) 实现展示门控（statData.flag 对 mainSkill skillFlags、condFunc(v,output)、childStat 二级取值、overCapStat 附注）；`CompareStatList`(:2265) 节点/物品对比 |
| `Modules/BuildDisplayStats.lua` | 263（已核实） | 纯声明数据 | 三张表：displayStats ~130 条玩家侧字段（stat/label/fmt/color/flag/condFunc/warnFunc/lowerIsBetter/compPercent/overCapStat/pool）、minionDisplayStats 29 条、extraSaveStats 8 个（充能/图腾/召唤物上限，存 XML 供第三方工具，:252-261 已核实） |
| `Modules/CalcSections.lua` | 2674（已核实） | Calcs 页 29 个 section 的声明表 | :51 HitDamage … :1992 DamageTaken；每个展示格声明 `{ modName, modType, cfg }` 即"该格子应聚合哪些 ModName"（如 :9-11 physicalHitTaken = DamageTaken/PhysicalDamageTaken/CurseEffectOnSelf/Armour/IgnoreArmour），点击即弹出对应 mod 列表 + breakdown |
| `Modules/CalcBreakdown.lua` | 252（已核实） | breakdown 生成器 | 10 个纯函数：multiChain(:19 连乘链)、simple(:46 base×inc×more)、mod(:71)、slot(:83 按装备槽分解)、area(:97 含 AoE 0.1m 断点提示)、effMult(:113 抗性/穿透/承伤链)、dot(:160)、critDot(:173)、leech(:196)；由 calc 各处调用填充 breakdown 表 |

### ConfigOptions 条目 schema

- `var`（XML key）、`type`（check×328 / count×146 / list×30 / integer×5 / float×1 / text×1）
- `label` / `tooltip`、`defaultState` / `defaultIndex` / `defaultPlaceholderState`
- 可见性门控：`ifCond` / `ifFlag` / `ifMult` / `ifEnemyCond` / `ifSkillData`
- 条件蕴含：`implyCond` / `implyCondList`（共约 60 处，含 6 处 implyCondList）
- 核心 `apply` 闭包：**517 个（已核实）**。约九成是模板化 `modList:NewMod(name, type, value, source, tags)`（如 :133-135 conditionMoving→FLAG；:117-119 带 clamp 的 Multiplier；:2150-2156 写 enemyModList 的抗性 BASE），少数是真逻辑：
  - `enemyIsBoss`(:1963-2120)：按四档注入整组 boss mod 并联动十几个 placeholder
  - `presetBossSkills`(:2170-2249)：按 data.bossSkills 数据表回填敌方伤害/穿透/速度
  - `customMods`(:2278-2296，已核实)：逐行走 modLib.parseMod
  - `questRewards`(:56-108)：由 data.questRewards 数据动态生成条目

## pobr 实现现状

pobr 对应实现分三块：

### 1) 配置

- `crates/pobr-data/src/build_config.rs`：仅 `ViewMode` / `BanditChoice` 两个枚举。
- `crates/pobr-build/src/build_config.rs`：`BuildConfig`（is_attack / is_spell / damage_type / conditions:HashMap / multipliers:HashMap / global_modifier_texts），经 `to_calc_config` 进 `CalcConfig`。
- XML 导入在 `xml_build.rs` 的 `parse_config`（~:133-205）：按前缀通用规则处理 `condition*`（要求 boolean 属性）/ `multiplier*`（number）/ `quest*`（string，走 global_modifier_texts）/ `use*Charges`，外加 7 条 defaultState=true 特例表 `DEFAULT_TRUE_CONDITIONS`（已核实，对应上轮审计 01-06 修复）。
- 消费侧在 `calc_orchestrator.rs`：条件蕴含仅 2 条（:992-1004，已核实）、抗性惩罚常量化（:58-59 恒 -60，已核实）、enemy_level / enemy_tier 由 `DataOrchestratorOptions` 调用方传入（:71-95，已核实；xml_build 中 grep 'enemy' 零命中）。
- 另有 `pobr-core/src/campaign.rs::CampaignProgress` 已把 0/-10/…/-60 抗性惩罚表做成数据并带测试，但**未接到 XML 配置**。

覆盖度结论：约能吃下 PoB2 check 型 + multiplier 前缀 count 型，但**所有带定制 apply 的条目语义、count 型 condition*、Enemy Stats 数值覆盖、customMods、list 型条目全部丢失或硬编码**。

### 2) 展示目录

- `crates/pobr-data/src/display_stat.rs`：`DisplayStatDefinition` / `DisplayStatValue` / `ParityStatus` / `PobCatalog` 类型契约，质量不错。
- `crates/pobr-core/src/display_catalog.rs`：硬编码 ~85 个字段全标 Computed，`extract_display_values` 从 OutputTable 取值；grep 'minion' 零命中（已核实）。
- 对照 PoB2 130+ 玩家字段约覆盖 65%，minion 展示目录为零。
- `tools/sync-pob-catalog` 已会扫 BuildDisplayStats.lua / CalcSections.lua 等生成 PobCatalog 做 parity diff——基础设施在，但 diff 出的缺口未消化完。

### 3) breakdown / 归因

- `pobr-core/src/calc/breakdown.rs`：扁平 (name, value) 步骤表（已核实 `BreakdownStep{name,value}`；防御侧只 push 了 armour/evasion/energy_shield/chance_to_be_hit 四项，defence.rs:104-107 已核实）。
- `trace.rs` TraceGraph + `attribution.rs`：提供 PoB2 没有的 source-level 归因（direct / marginal / interaction）。
- Build 状态/对比在 `build.rs` / `comparison.rs` / `snapshot.rs` / `calc_cache.rs`，对应 Build.lua 的非 UI 骨架（CompareStatList → compare_outputs），但无自动等级估算、无 extraSaveStats 回写（grep extraSaveStats / extra_save 零命中，已核实）。

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|------|--------|------|-----------|-----------|------|
| 1 | ConfigOptions 带定制 apply 的条目语义未建模，仅靠命名前缀导入 | 🔴 high | partial | ConfigOptions.lua:110-2323（542 条目 / 517 apply，已核实） | xml_build.rs parse_config（~:133-205） | 凡 apply 不是"set 同名 Condition/Multiplier"的条目语义全丢 |
| 2 | customMods 自定义词条完全不导入 | 🔴 high | missing | ConfigOptions.lua:2278-2296 | 无（全仓 grep 仅命中测试/注释） | 用户自定义词条静默丢失，无任何提示 |
| 3 | enemyIsBoss 及 Enemy Stats 数值覆盖项不从 build XML 读取 | 🔴 high | missing | ConfigOptions.lua:1959/:1963/:2143-2274 | calc_orchestrator.rs:71-95；xml_build grep 'enemy' 零命中 | 敌人档位由调用方写死；显式选 None/Boss/Uber 的 build 全按 Pinnacle 算 |
| 4 | count 型 condition*（如 conditionStationary）被静默丢弃 | 🟡 medium | incorrect | ConfigOptions.lua:120-131 | xml_build.rs parse_config 的 condition* 分支 | number 属性两个分支都不命中；配置修复是词条端到端生效的前置条件 |
| 5 | resistancePenalty 配置被硬编码为 -60，CampaignProgress 数据表未接线 | 🟡 medium | incorrect | ConfigOptions.lua:113（list 0/-10/…/-60，defaultIndex=7） | calc_orchestrator.rs:58-59；campaign.rs 数据已就位 | 非终局配置的 build 三抗全错 0~60 点 |
| 6 | implyCond/implyCondList 约 60 处仅实现 2 条 | 🟡 medium | partial | ConfigOptions.lua（grep 60 行，含 6 处 implyCondList） | calc_orchestrator.rs:992-1005 | 勾选母条件时依赖子条件的词条不生效 |
| 7 | 展示目录覆盖 ~85/130+，ChaosResist/属性/Spirit/DoT 汇总等缺位 | 🟡 medium | partial | BuildDisplayStats.lua:186/:95-100/:139-141/:43-70/:216 等 | display_catalog.rs；output.rs grep chaos 零命中 | 混沌抗已内联计算但不出现在 OutputTable 与展示目录 |
| 8 | minionDisplayStats 与 extraSaveStats 无对应实现 | 🟡 medium | missing | BuildDisplayStats.lua:221-261 + Build.lua:1170-1202 | 无（grep 零命中） | 召唤物侧边栏字段无契约；第三方生态依赖的 XML 统计快照不产出 |
| 9 | 展示条目门控/告警语义（flag/condFunc/warnFunc/overCapStat/pool）未建模 | 🟡 medium | design | Build.lua:2090-2105；BuildDisplayStats.lua:186 等 | display_stat.rs（仅静态 default_visible/comparison_visible） | 上层 UI/CLI 无法还原 PoB2 面板的条件显隐/告警/溢出附注 |
| 10 | CalcSections「展示格 ↔ ModName 集合」映射无对应物 | 🟡 medium | missing | CalcSections.lua:9-22、:51-1992 | 无 schema 对应；trace.rs TraceGraph | TraceGraph 与 PoB2 breakdown 是错位互补而非超集 |
| 11 | breakdown 生成器表达力差距：扁平步骤 vs 结构化公式链 | 🟢 low | partial | CalcBreakdown.lua:19-250 | breakdown.rs；defence.rs:104-107 仅 4 项 | 无 base/inc/more 角色标注、slot 分解、断点提示 |
| 12 | characterLevelAutoMode / 玩家进度自动估算缺失 | 🟢 low | missing | Build.lua:1001-1058、:1117 | build.rs:105 CharacterIdentity（静态 level） | Auto 模式 build 的等级口径差，波及基础属性与敌人默认等级 |

## 缺口详述

### Gap 1：ConfigOptions 带定制 apply 的条目语义未建模（🔴 high / partial）

**PoB2 证据**：ConfigOptions.lua:110-2323（542 个 `{ var =` 条目、517 个 apply 闭包，计数已核实）。非平凡例：
- :114-116 `detonateDeadCorpseLife` → SkillData LIST
- :117-119 `multiplierCurrentManaPercentage` 带 m_max/m_min clamp
- :1961-1962 `conditionEnemyRareOrUnique` 写 enemyModList 并带 `Condition:Effective` tag

**pobr 位置**：`crates/pobr-build/src/xml_build.rs` parse_config（~:133-205）。

**影响**：【已核实成立】通读 parse_config 全函数体确认仅 `condition*`(boolean) / `use*Charges` / `DEFAULT_TRUE_CONDITIONS` / `multiplier*`(number) / `quest*` 五个分支。凡 apply 不是「set 同名 Condition/Multiplier」的条目（注入 SkillData、写敌方 modDB、值 clamp、一项配置产出多条 mod）语义全部丢失。这是该领域最大的系统性缺口：每条丢失配置都直接让对应条件型词条/敌方状态在计算里失效。

**修复方向**：把 542 条目（含 effects 数组、imply、可见性门控）声明化为 `config_options.json`，pobr 侧用一个小解释器替换前缀启发式（详见「数据 vs 逻辑切分建议」）。这能一次性消掉 Gap 1/4/5/6 的硬编码根因。

### Gap 2：customMods 自定义词条完全不导入（🔴 high / missing）

**PoB2 证据**：ConfigOptions.lua:2278-2296（customMods text 型，逐行 StripEscapes 后走 modLib.parseMod，source=Custom，已核实）。

**pobr 位置**：无——xml_build.rs parse_config 无 customMods 分支；全仓 grep customMods 仅命中测试/注释。

**影响**：【已核实成立】PoB2 用户常在 Custom Modifiers 框补未建模 buff/词条；pobr 导入这类 build 时整块静默丢失——customMods 根本未从 XML 读出，连 mod_parser 的 `ParseStatus::Unsupported` 收集通道都到不了，DPS/防御直接偏低且无任何提示。

**修复方向**：pobr 已有 mod_parser 与 `global_modifier_texts` 通道（`quest*` 即走此路），在 parse_config 增加 customMods 分支逐行喂入即可，接入成本低；无法解析的行落 Unsupported 以保证可见性。

### Gap 3：enemyIsBoss 及 Enemy Stats 数值覆盖项不从 build XML 读取（🔴 high / missing）

**PoB2 证据**：ConfigOptions.lua:1959 enemyLevel(count)、:1963 enemyIsBoss（list 四档 None/Boss/Pinnacle/Uber，defaultIndex=3=Pinnacle，已核实）、:2143-2169 enemy*Resist/Armour/Evasion/Block、:2251 enemyDamageType、:2260 enemySpeed、:2264-2274 enemyCrit*/enemy*Damage/*Pen。

**pobr 位置**：`crates/pobr-build/src/calc_orchestrator.rs:71-95`（enemy_level/enemy_tier 由 DataOrchestratorOptions 调用方传入，已核实）；xml_build.rs 中 grep 'enemy' 零命中；`tests/ninja_parity.rs:67` 恒 `EnemyTier::Pinnacle`。

**影响**：【已核实成立，含一处口径细节】build XML 里保存的 `<Input name="enemyIsBoss" string=...>` 与全部敌方数值覆盖被忽略，敌人档位由 Rust 调用方写死。注意 PoB2 defaultIndex=3 即默认就是 Pinnacle，所以「从未改过该项」的 build 与 pobr 硬编码恰好一致；但显式保存为 None/Boss/Uber 的 build 一律按 Pinnacle 口径算（有效 DPS、EHP 的敌方伤害基线全错位），用户手填的敌方抗性/护甲/伤害覆盖也无效。

**修复方向**：`setup_env.rs:112` 的 tier 默认值数据已就位（已核实），缺的是 XML → DataOrchestratorOptions 的接线：parse_config 读出 enemyIsBoss/enemyLevel/各 enemy* 覆盖值，按「显式值 else tier 默认值」的取值规则下传；boss 注入 mod 组与 bossSkills 预设应一并 JSON 化（见切分建议）。

### Gap 4：count 型 condition*（如 conditionStationary）被静默丢弃（🟡 medium / incorrect）

**PoB2 证据**：ConfigOptions.lua:120-131（conditionStationary type=count，apply 同时写 `Multiplier:StationarySeconds` BASE 与 `Condition:Stationary` FLAG，并有旧版 boolean 兼容分支，已核实）。

**pobr 位置**：xml_build.rs parse_config 的 condition* 分支（要求 boolean 属性）。

**影响**：【已核实成立，影响表述细化】count 型条目在新版 XML 中存为 number 属性（PoB2 apply 中的 boolean 兼容分支佐证旧版才是 boolean），且名字不以 multiplier 开头，于是两个分支都不命中——既没置 Condition 也没置 Multiplier。需注意：pobr mod_parser 中 grep 'stationary' 零命中，即「while stationary / per second while stationary」词条本身大概率也尚未解析为条件标签；该 gap 的直接事实是配置层静默丢弃，端到端影响要等词条解析支持后才完全显形。

**修复方向**：配置层按 number 解析 condition* 并同时产出 Multiplier + FLAG（这是词条侧支持后能端到端生效的前置条件）；根因修复同 Gap 1（条目 JSON 化后 effects 即可表达"一条配置产出两条 mod"）。

### Gap 5：resistancePenalty 硬编码 -60，已有的 CampaignProgress 数据表未接线（🟡 medium / incorrect）

**PoB2 证据**：ConfigOptions.lua:113（list：0 / -10 / … / -60，defaultIndex=7，已核实；消费处 CalcSetup `configInput.resistancePenalty or -60`）。

**pobr 位置**：`calc_orchestrator.rs:58-59`（ENDGAME_RESISTANCE_PENALTY 常量）+ :311-318；另 `crates/pobr-core/src/campaign.rs::CampaignProgress` 已实现 0/-10/…/-60 全表（带测试 tests/campaign.rs）。

**影响**：【已核实成立，pobr 侧描述修正】PoB2 允许按章节选抗性惩罚（练级 build 用 0/-10…）；pobr 计算编排恒 -60。该 list 型条目在 XML 中存 number，parse_config 不识别。值得注意 campaign.rs 的 CampaignProgress 枚举已把这张惩罚表做成数据且与 PoB2 逐档一致——缺的不是数据而是「XML resistancePenalty → CampaignProgress/惩罚值」的接线。对终局 build parity 无影响，但任何非终局配置的 build 三抗全错 0~60 点。

**修复方向**：parse_config 识别 resistancePenalty（number），映射到 CampaignProgress 或直接下传惩罚值给 stat_boundary。

### Gap 6：implyCond/implyCondList 约 60 处仅实现 2 条（🟡 medium / partial）

**PoB2 证据**：ConfigOptions.lua grep implyCond 共 60 行，其中 6 处为 implyCondList；高频蕴含：UsedSkillRecently×9、BeenHitRecently×7、Leeching/KilledRecently/HitRecently/ConsumedCorpseRecently/Burning 各 3（已核实）。

**pobr 位置**：`calc_orchestrator.rs:992-1005` apply_condition_implications（已核实仅 EnemyIgnited→EnemyBurning、EnemyFrozen→EnemyChilled 两条）。

**影响**：【已核实成立，数量修正：原报告 44 处实为约 60 处（含 implyCondList）】其余约 58 条蕴含链（UsedSkillRecently/BeenHitRecently 等玩家侧 buff 链、其他异常链等）缺失，意味着勾选母条件时依赖子条件的词条不生效。

**修复方向**：蕴含关系本身是纯数据，应随 config 条目一起 JSON 化（`imply_conditions[]` 字段）而非逐条手写。

### Gap 7：展示目录覆盖 ~85/130+，ChaosResist/属性/Spirit/DoT 汇总等缺位（🟡 medium / partial）

**PoB2 证据**：BuildDisplayStats.lua:186(ChaosResist，已核实)、:95-100(Str/Dex/Int+Req*)、:139-141(Spirit)、:43-70(TotalDot/With*DPS/CombinedDPS)、:216(FullDPS)、:172(PhysicalDamageReduction)、:190(EffectiveMovementSpeedMod，已核实)。

**pobr 位置**：`display_catalog.rs`；`calc/output.rs`（grep chaos_res/ChaosResist 零命中，已核实）；`perform.rs:166-183`（混沌抗内联计算）。

**影响**：【已核实成立】display_catalog 约 85 字段全标 Computed，无一条 Planned 占位，缺口不可见。最扎眼的是玩家混沌抗性：perform.rs:166-183 已内联计算（含 DEFAULT_MAX_RESISTANCE+max_bonus、HARD_MAX_RESISTANCE 截断）但只喂给 EHP 的 ResistanceSuite，OutputTable 与展示目录均无此字段——PoE2 角色面板基础项缺失。Str/Dex/Int 与装备需求告警（ReqStr warnFunc）、Spirit 总量（只有 spirit_reserved）、DoT 汇总族、FullDPS 多技能汇总亦缺。

**修复方向**：先把已计算未导出的（混沌抗）补进 OutputTable 与目录；其余缺位字段以 `ParityStatus::Planned` 占位入目录使缺口可见；用 sync-pob-catalog 的 PobCatalog diff 作为完整性门禁。

### Gap 8：minionDisplayStats 与 extraSaveStats 无对应实现（🟡 medium / missing）

**PoB2 证据**：BuildDisplayStats.lua:221-250（29 条 minion 字段）、:252-261（extraSaveStats 8 项，已核实）+ Build.lua:1170-1202（Save 时把 displayStats/extraSaveStats 值写回 XML 供第三方工具）。

**pobr 位置**：无（display_catalog.rs grep 'minion' 零命中；全仓 grep extraSaveStats/extra_save 零命中，已核实）。

**影响**：【已核实成立】召唤物 build 的侧边栏字段（minion DPS/Life/Leech 等）没有展示契约；PoB2 导出 XML 自带的统计快照（poe.ninja 等第三方依赖 extraSaveStats）pobr encode 侧不产出，影响生态兼容。

**修复方向**：展示目录加 minion 维度（或独立 minion 目录）；`encode_pob_code` 侧在 Save 路径补 displayStats/extraSaveStats 回写以维持第三方工具兼容。

### Gap 9：展示条目门控/告警语义未建模（🟡 medium / design）

**PoB2 证据**：Build.lua:2090-2105 AddDisplayStatList（statData.flag 对 skillFlags 门控、condFunc(v,output)、childStat）；BuildDisplayStats.lua 各条目（如 :186 ChaosResist 的 condFunc=not ChaosInoculation + overCapStat，已抽查核实）。

**pobr 位置**：`display_stat.rs`（仅静态 default_visible/comparison_visible）。

**影响**：PoB2 的同名 stat 会按主技能 flag 显示不同 label（Speed→Attack Rate/Cast Rate/Trigger Rate）、按 condFunc 条件显隐（如 CI 时混沌抗显示 Immune）、warnFunc 产警告、overCapStat 附注溢出。pobr 的 DisplayStatDefinition 没有这些维度，上层 UI/CLI 无法还原 PoB2 面板行为。

**修复方向**：condFunc 多为简单谓词（v>0、v≠另一字段），可声明化为枚举谓词（eq/ne/gt 字段引用 + and/or）扩展 DisplayStatDefinition；少数复杂闭包标 native 回退到框架代码。

### Gap 10：CalcSections 的「展示格 ↔ ModName 集合」映射无对应物（🟡 medium / missing）

**PoB2 证据**：CalcSections.lua:9-22（physicalHitTaken 等 per-type mod 列表）、:51-1992（29 个 section，每格声明 modName/modType/cfg）。

**pobr 位置**：无 schema 对应；`pobr-core/src/trace.rs`（TraceGraph）。

**影响**：PoB2 breakdown 回答两个问题：(a) 这个数字的公式分解；(b) 点开某格子看「哪些 mod 参与了这组 ModName 的聚合」。pobr 的 TraceGraph 能更强地回答 (a) 的来源归因（直接/边际/交互，PoB2 做不到），但 (b) 需要的声明式 ModName 分组映射（哪个面板格子聚合哪些 ModName + 哪个 cfg 上下文）完全没有——二者是错位互补而非超集。

**修复方向**：要做 Calcs 页等价物，必须把 CalcSections 的格子→ModName 映射作为数据表移植（建议直接落 JSON），再用 `ModDb::contributions` 填充每格的 mod 列表。

### Gap 11：breakdown 生成器表达力差距（🟢 low / partial）

【已核实成立】pobr BreakdownTable（`BreakdownStep{name,value}` 扁平表）无 base/inc/more 角色标注、无装备槽 slot 分解、无断点提示（area 的「再 +x% inc 到下一档」），且只有少数 stat 真正填充（defence.rs:104-107 仅 4 项）。归因（TraceGraph）解决「谁贡献的」，breakdown 解决「公式怎么走的」——后者结构需补强（对齐 CalcBreakdown 的 multiChain/simple/slot/area/effMult/dot/leech 等形态）才能渲染 PoB2 级别的悬浮分解。

### Gap 12：characterLevelAutoMode / 玩家进度自动估算缺失（🟢 low / missing）

PoB2 Build.lua:1001-1058 EstimatePlayerProgress 由已点天赋点数 + acts questPoints 表反推角色等级（characterLevelAutoMode 从 XML :1117 读取）；pobr `build.rs:105` CharacterIdentity 直接采信 XML 静态 level。Auto 模式的 build XML 中 level 字段可能与 PoB2 运行时实际采用的估算等级不一致，影响角色基础属性、敌人默认等级（enemyLevel placeholder = characterLevel）两条链路的口径。低频但属于已知口径差；其输入 acts questPoints 表是数据，算法留框架。

## 数据 vs 逻辑切分建议

该领域是整个 PoB2 里「数据被写成代码」最典型的地方，切分结论如下。

### 本质是数据、应 JSON 化

1. **ConfigOptions 的 542 个条目**：schema 字段（var/type/label/tooltip/list 选项/defaultState/defaultIndex/defaultPlaceholderState/section/ifCond/ifFlag/ifMult/ifEnemyCond/ifSkillData/implyCond）全是数据；517 个 apply 闭包中约九成是模板化的 `NewMod(name, type, value, source, tags)`，完全可声明化为 effects 数组：

   ```
   { target: player|enemy,
     mod_name, mod_type,
     value: literal | input | clamp(input,min,max) | negate(input),
     tags: [{Condition,var,...}] }
   ```

   一条配置可带多条 effect（conditionStationary 即「Multiplier + FLAG」两条）。implyCond/implyCondList 约 60 处也是纯数据。剩下一成真逻辑见下。

2. **BuildDisplayStats 的 130+29+8 条目**：纯数据表。condFunc/warnFunc 闭包绝大多数是「v>0」「v≠output.X」「flag 存在」级别的简单谓词，可设计一个小型声明式谓词（eq/ne/gt 字段引用 + and/or），少数复杂的（TotalDotDPS 的长链去重判断）允许标 native 回退到框架代码。

3. **CalcSections 的 29 个 section**：每个展示格的 `{modName[], modType, cfg}` 映射是纯数据，且是做 Calcs 页钻取的必要输入；顶部的 physicalHitTaken 等公共 mod 列表同理。

4. **敌方/boss 数据**：enemyIsBoss 四档注入的 mod 组（MonsterUnique 系列 MORE/BASE）、data.bossSkills 预设（伤害倍率/穿透/速度/uber 变体）、monsterDamageTable/ArmourTable/EvasionTable——pobr 的 `pobr-data/src/monster.rs` 已把 tier 默认值做成了 Rust 常量数据（好的方向），`pobr-core/src/campaign.rs` 也已把 resistancePenalty 档位表数据化，但二者都尚未与 XML 配置接线；boss skill 预设表尚未入库。

### 本质是逻辑、留在框架

- CalcBreakdown 的 10 个生成器（公式分解渲染）；
- customMods/questRewards 里对 modLib.parseMod 的调用（解析器即 pobr mod_parser）；
- enemyIsBoss/presetBossSkills 里的 placeholder 联动（这是 UI 回填，计算侧只需「显式值 else tier 默认值」的取值规则）；
- FormatStat/CompareStatList 的格式化与对比；
- EstimatePlayerProgress 的等级反推算法（但其输入 acts questPoints 表是数据）。

### PoB2 如何混在一起（反面教材）

517 个 Lua 闭包把「这条配置产生哪些 mod」这一数据性事实埋进了不可序列化的函数体里，与 UI placeholder 操作、clamp、tooltip 拼接交织在同一个 apply 里；BuildDisplayStats 的显隐谓词同样以闭包形式与声明字段混排；CalcSections 虽是纯表但体量 2674 行直接内嵌源码。

### pobr 当前 JSON schema 还缺的表/字段

catalog.rs 现有 BaseItemDef/StatDef/ModDef/SkillGemDef/GrantedEffectDef/PassiveNodeDef 等，配置/展示域为零。建议补：

| 建议新增 | 内容 | 收益 |
|----------|------|------|
| `config_options.json` | 新 `ConfigOptionDef`：var / input_type / default / list_options / section / visibility{if_cond, if_flag, if_mult, if_skill_data} / effects[] / imply_conditions[] | 配合 pobr-core 一个小解释器替换 xml_build.rs 的前缀启发式与 DEFAULT_TRUE_CONDITIONS 硬编码——一次性消掉 Gap 1/4/5/6 的硬编码根因 |
| `boss_skills.json` / `quest_rewards.json` | presetBossSkills 数据源 data.bossSkills；ConfigOptions 动态生成 Quest Rewards 段的数据源 data.questRewards | Gap 3 的敌方预设数据化 |
| `display_stats.json` + `calc_sections.json` | 展示目录实例 + CalcSections 的 ModName 分组映射 | display_stat.rs 的类型契约已就绪，但目录实例硬编码在 pobr-core/display_catalog.rs（编译期 Rust）。展示目录跨游戏版本相对稳定，留框架内可接受，但应从 sync-pob-catalog 扫出的 PobCatalog 自动生成/校验（目前是手工维护、缺 45+ 字段且无 Planned 占位）；CalcSections 的 ModName 分组映射则强烈建议直接落 JSON——它和 ModName 词表一样会随版本/词条演化 |

## 附录：核查说明

逐条打开 vendor PoB2 源码与 pobr 实际代码核查了全部 3 条 high + 5 条 medium（Gap 4/5/6/7/8）+ 抽查 1 条 low（Gap 11），并对全仓（crates/apps/tools）做了 customMods、enemyIsBoss、resistancePenalty、Stationary、implyCond、ChaosResist、minion、extraSaveStats 等关键词全局 grep 以排除"在别处实现"的可能。结论与修正：

**3 条 high 全部查实，保留**

1. Gap 1（定制 apply 未建模）：通读 xml_build.rs parse_config 全函数体，确认仅五个分支；ConfigOptions.lua 条目数 542、apply 闭包数 517 用 grep -c 复核一致；引用的非平凡例（:114-116 detonateDeadCorpseLife SkillData LIST、:117-119 clamp、:1961-1962 enemyModList+Condition:Effective）逐一在源码中确认存在且语义如描述。
2. Gap 2（customMods）：ConfigOptions.lua:2278-2296 逐行 modLib.parseMod 已读到原文；pobr 全仓 grep customMods 仅命中测试注释，parse_config 无该分支。
3. Gap 3（enemyIsBoss/Enemy Stats）：ConfigOptions.lua:1959 enemyLevel、:1963 enemyIsBoss 四档原文确认；xml_build.rs grep 'enemy' 零命中；DataOrchestratorOptions 由调用方传入确认；ninja_parity.rs:67 恒 Pinnacle 确认。补充原报告遗漏的口径细节：PoB2 defaultIndex=3 默认即 Pinnacle，故"从未改过该项"的 build 与 pobr 硬编码恰好一致，错位只发生在显式选 None/Boss/Uber 的 build——影响面比原描述略窄，但敌方数值覆盖全部失效仍成立，维持 high。

**medium 修正 3 处**

4. Gap 4（conditionStationary）：ConfigOptions.lua:120-131 双 mod 注入 + boolean 旧版兼容分支确认；parse_config 要求 boolean 属性确认。但发现 pobr mod_parser 中 'stationary' 零命中——相关词条本身也尚未解析，原 detail"所有 while stationary 类词条对这类 build 失效"把锅全归配置层有夸大；已改写为"配置层静默丢弃查实，端到端影响需词条解析支持后才完全显形，配置修复是前置条件"。维持 medium/incorrect。
5. Gap 5（resistancePenalty）：硬编码 -60（calc_orchestrator.rs:58-59）查实；但全局 grep 发现 pobr-core/src/campaign.rs::CampaignProgress 已把 0/-10/…/-60 全表数据化且有测试——原报告 pobr_ref 漏掉了这个"数据已就位、仅缺接线"的事实，已补入。严重度不变。
6. Gap 6（implyCond）：grep 实测 60 行（含 6 处 implyCondList），原报告写 44 处，已修正为约 60；并补充高频蕴含分布（UsedSkillRecently×9 等）。pobr 侧 apply_condition_implications 确认仅 2 条。维持 medium。

**其余抽查均成立，保留**

7. Gap 7（ChaosResist 等展示缺位）：perform.rs:166-183 混沌抗内联计算只喂 EHP ResistanceSuite 确认；output.rs/display_catalog.rs grep chaos 零命中确认；BuildDisplayStats.lua:186 ChaosResist 条目（含 condFunc=not ChaosInoculation + overCapStat）原文确认。
8. Gap 8（minion/extraSaveStats）：BuildDisplayStats.lua:252-261 extraSaveStats 8 项原文确认；pobr 全仓 minion（display_catalog）/extraSaveStats grep 零命中确认。
9. Gap 11（breakdown 表达力）：breakdown.rs BreakdownStep{name,value} 扁平结构、defence.rs:104-107 仅 push 4 项均已读源码确认。

无条目被删除或降级；修正集中在数量（44→60）、影响面表述（Gap 3 默认档恰合、Gap 4 双层缺口）、pobr 侧已有但未接线的数据（Gap 5 CampaignProgress）。pob2_structure/pobr_status/data_logic_split 中相应数字与引用已同步修正。
