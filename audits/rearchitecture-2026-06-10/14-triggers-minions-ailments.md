# 触发/召唤物/异常状态

> 领域：Triggers / Minions+Mirages / Ailments
> 审计日期：2026-06-10 ｜ 系列：rearchitecture-2026-06-10 ｜ 编号 14
> 上一轮审计参照：`audits/pob2-parity-2026-06-09/FINDINGS.md`（本文聚焦其未覆盖的内容；与 03-01/03-02/05-01 相关处已注明）

---

## PoB2 代码结构（结构地图）

### 触发系统 — `Modules/CalcTriggers.lua`（1465 行 / 78K）

```
calcs.triggers(env, actor)                        (L1436)  入口
 └─ configTable                                   (L882-1418, 共 61 项，已逐项计数核实)
     键：四级 key（技能名 → triggeredBy 名 → awakened 名 → unique 物品名）
     值：{triggerSkillCond, triggeredSkillCond, triggerChance, comparer, customHandler...} 闭包配置
     覆盖："cast on critical strike"(L1089)、"cast when damage taken"(L1113)、
           "cast while channelling"(CWCHandler)、"focus"(helmetFocusHandler L135)、
           mjolner(L1075) / poet's pen / svalinn 等约 45 个 unique 触发
 └─ defaultTriggerHandler                         (L384 起，约千行，主干流程)
     ├─ findTriggerSkill(L74)：从 activeSkillList 找源技能，
     │   读 GlobalCache 缓存的完整子计算结果 HitSpeed/Speed (L67-70 defaultComparer / L83) 作 trigRate
     ├─ 修正链：双持 /2(L431)、Unleash dpsMult(L443)、多重投射 multiHitDpsMult、
     │   Kitava repeats、Manaforged 法力阈值、战吼 uptime
     ├─ 冷却模型：cd = max(triggeredCD+added, triggerCD+addsCastTime)/icdr
     │   → 帧上取整 → TriggerRateCap
     ├─ 源命中率/暴击率折入 triggerChance (L729-770，含双武器独立 roll 的 effective hit/crit)
     ├─ calcMultiSpellRotationImpact(L90-133)：1000 次触发机会的确定性轮转模拟
     │   （next_trig = ceil_b(floor_b(now, tick)+cd, tick)，ServerTickTime）+ 几何分布折算触发几率
     └─ 末段：trigger bots ×2、overlaps、addTriggerIncMoreMods(L23)
         把 TriggeredDamage INC/MORE 注入技能
```

> ⚠️ 重要事实：**vendor PoB2-PoE2 没有任何能量（Energy）模型**（`grep -i energy` 于 CalcTriggers.lua 0 命中，已复核）。PoE2 元宝石在 PoB2 中仍沿用 PoE1 CoC 暴击率口径。

### 幻影系统 — `Modules/CalcMirages.lua`（420 行）

```
calcs.mirages(env)                                (L56)  入口
 └─ 5 类配置分支：Mirage Archer / Saviour Mirage Warriors /
    Tawhoa's Chosen(L178) / Sacred Wisps / General's Cry
 └─ calculateMirage(L22)：
     找到玩家技能 → copyActiveSkill 复制到新 env
     → preCalc 注入 MORE less-damage / QuantityMultiplier 修正
     → 重跑 calcs.perform(newEnv)
     → postCalc 把幻影 DPS 写回主面板
```

### 召唤物系统 — 数据与逻辑分布

| 位置 | 性质 | 内容 |
|------|------|------|
| `Data/Minions.lua`（43K，32 条） | **纯数据**（文件头注明 automatically generated, do not edit，已核实） | 每条 `{name, monsterTags, life/damage/armour/evasion 归一化乘数, damageSpread, attackTime, critChance, 抗性, limit, spectreReservation, skillList, modList}` |
| `Data/Spectres.lua`（624K，593 条） | **纯数据**（同 schema，条数已核实） | 同上 |
| `CalcActiveSkill.lua` L846-907 | 逻辑 | 定召唤物列表与等级（minionLevelTable）/lifeTable/hiddenDamageFixup；`grantedEffect.minionList` 外键来自技能数据（`Data/Skills/act_int.lua` 等，如 L16184） |
| `CalcActiveSkill.lua` L1049-1119 | 逻辑 | `createMinionSkills`：把 `minionData.skillList` 经 `env.data.skills` 建为召唤物自己的主动技能（按 levelRequirement≤minion.level 选级、minionDamageEffectiveness L1104、ExtraMinionSkill L1061） |
| `CalcPerform.lua` L964-971 | 逻辑 | activeSkill.minion → createMinionSkills |
| `CalcPerform.lua` L983-1075 | 逻辑 | 装配 env.minion.modDB：baseLife×表、敌对×mapLevelLifeMult、armour/evasion 表、0.3.0 起 `CannotBeEvaded` 必中(L1005)、CritMultiplier=怪物30+内禀70、HeavyStunBuildup 常量(L1013-1014)、hiddenDamageFixup Damage MORE(L1016)、minionData.modList、Talisman/IronMass/弓+箭袋/minion itemSet、Str/Dex 灌注 flag |
| `CalcPerform.lua` L1183-1191 | 逻辑 | limit → `Multiplier:SummonedMinion` |
| `CalcPerform.lua` L1676 | 逻辑 | `MinionModifier` LIST 按 type 过滤注入 |

召唤物之后复用同一套 offence/defence 管线作为独立 actor。

### 异常状态 — 数据与逻辑分布

**数据侧**（`Modules/Data.lua` + `Data/Misc.lua`）：

| 位置 | 内容 |
|------|------|
| Data.lua L347-351 | `data.nonDamagingAilment`（已逐行核实）：Chill default=30, min=30, max=gameConstants["ChillMaxEffect"]=50；Freeze min=0.3s, max=3s；Shock default=min=gameConstants["BaseShockMagnitude"]=20, max=100；duration 均引 Data/Misc.lua gameConstants |
| Data.lua L353+ | `buildupTypes`（Electrocute/Freeze/HeavyStun ScalesFrom）、`defaultAilmentDamageTypes` |
| Data.lua L203-208 | Bleed/Poison/Ignite 基数 = `gameConstants["XHitDamagePercentPerMinute"]/60/100` |

**逻辑侧**：

| 位置 | 内容 |
|------|------|
| CalcOffence L5463-5475 | `nonDamagingAilmentsConfig`：Chill effect=`ChillEffectMultiplier×(damage/threshold)×effectMod`（线性）；Shock=`50×(damage/threshold)^0.4×effectMod` |
| CalcOffence L4769+ | `calcDamagingAilmentOutputs`（命中/暴击加权 + stack potential） |
| CalcPerform L673-691 | 玩家自身冰缓/冰冻 → ActionSpeed（含 SelfChillEffect、L682 SelfChillEffectIsReversed、Freeze→ActionSpeed -70×、Unravelling→ChaosCanFreeze/Ignite/Shock，已核实） |
| CalcPerform L1152-1177 | Shock Ground ShockOverride(L1155) / Skitterbots / supportBonechill 等保证型来源 |
| CalcPerform L3077-3180 | 敌方异常施加循环（已核实）：ailments 表按 Chill/Shock 等聚合 XOverride/XMinimum → Current/Maximum 输出 → Condition:Chilled/Shocked → Bonechill ColdDamageTaken(L3094)、ChillCanStack/ShockCanStack 叠层、Shock→enemy DamageTaken INC、Multiplier:ChillEffect/ShockEffect 回写 |

---

## pobr 实现现状

### 触发 — `crates/pobr-core/src/calc/trigger.rs`（49K）

四段式结构：

1. **§一 冷却驱动**：`resolve_trigger_rate`(L120)——`max(triggeredCD, triggerCD)/icdr` 帧上取整 → cap → `min(cap, sourceRate)`，对齐 PoB2。
2. **§二 能量驱动元宝石模型**（centienergy）：`calc_energy_per_event`(L264) / `calc_energy_trigger_rate`(L338)——**超出 PoB2 基线**（vendor 无此模型，出处是 agent-docs + act_int.lua 游戏数据），无法用 PoB2 parity 验证。
3. **§三** `calc_multi_spell_rotation`：移植 calcMultiSpellRotationImpact（含几何分布折算）。
4. **§四 CWC**：`calc_cwc_trigger_rate`(L599)。

均带 traced 版本。

build 层接线：`calc_orchestrator.rs:1504 trigger_modifiers` 已接线**内建触发**（skill_types 含 Triggered/InbuiltTrigger，L1515-1523 gate），注入 TriggeredSkillCooldown/TriggerCooldownBase/TriggerSourceRate；`perform.rs:381 fill_trigger` 消费并乘 trigger_chance_multiplier。但：

- 源速率取 `1/use_time_s`（宝石基础时间，calc_orchestrator.rs:1595-1599 已核实）；
- CoC/CWDT 等 support 元触发、unique 触发、defaultTriggerHandler 的各修正均未建模（orchestrator L1486 注释自承「无 gem-link / triggeredBy 关系」）；
- CalcMirages 全无（grep mirage 仅 pob2_parity.rs / skill_stat_map.rs 注释提及缺口）；
- 能量模型当前无任何消费方（perform.rs / pobr-build 均不调用 `calc_energy_trigger_rate`），属悬空代码。

### 召唤物 — `crates/pobr-core/src/calc/minion.rs`（24K）+ `crates/pobr-data/src/minion.rs`

core 侧自包含纯函数已就绪：

- `MinionDef` schema（字段一一对应 Minions.lua，含 skill_list / spectre_reservation）；
- `derive_minion_base_stats`（等级表×归一化乘数、虚拟武器、爆伤 30+70、CannotBeEvaded）；
- 三通道注入（MinionModifier / 盟友 buff / 属性灌注）、`write_summoned_minion_multipliers`；
- `perform.rs:75 perform_minions` 把 env.minions 跑 `calculate_minimal_vs_enemy` + `calc_defence` 出 MinionOutput 快照（含跨 actor trace 边、SummonedMinion multiplier 注入）。

但存在系统性断点：

- 数据只有 **4 条手抄常量构造函数**（minion.rs:261/296/331/366）+ Spectre 占位(:405)；`data/4.5.0.3.4/` 无 minions.json/spectres.json（已核实目录仅 11 个宝石/词缀/天赋域文件）；pobr-gamedata 与 pobr-data-adapter 对 minion 零引用（grep 已核实）；
- calc_orchestrator 不识别召唤物宝石、从不调用 `add_minion_from_def`（grep minion 于 pobr-build/src 零命中，已核实）——真实 build 召唤物面板恒空；
- skill_list 字段存而不用（仅在 def 构造里赋值，calc 侧无读取，无 createMinionSkills 等价物），法术系召唤物 DPS 无来源；
- mod_parser 不产 MinionModifier 包裹（"Minions deal X% increased Damage" 类词条进不了召唤物 ModDb）。

### 异常 — `crates/pobr-core/src/calc/ailment.rs`（56K）

覆盖最好的一块：

- 伤害异常：bleed 15% / ignite 20% / poison 20% 基数、AilmentMagnitude 缩放、duration、effMult（敌方抗性 + DamageTaken 链）、暴击加权 weighted_source_damage、stack potential（经 FINDINGS 05-01 启用）；
- 非伤害异常公式：chill 线性 clamp[30,50]、shock 50×r^0.4 clamp[20,100]、freeze/electrocute poise buildup——均对齐 PoB2；
- 常量集中在 pobr-data（monster.rs CHILL_MAX_EFFECT=50 / CHILL_MIN_EFFECT=30、constants.rs SHOCK_MIN_EFFECT=20，与 Misc.lua gameConstants 数值一致，已核实）。

缺的是**消费侧闭环**：敌方异常施加循环（Override/stacking/Bonechill/Condition:Chilled/Shocked/Shock→DamageTaken 影响自身 DPS）、玩家自身异常（SelfChillEffect→ActionSpeed）、保证型来源（Skitterbots/感电地面）全部缺失（grep `Condition:Shocked|ChillOverride|Bonechill|SelfChillEffect` 于 crates/apps/tools 全部零命中，已核实）——shock/chill 目前只是面板展示值（perform.rs:660-701 仅写 chill_effect/shock_effect），不回灌任何 DPS/防御计算。

---

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|------|--------|------|-----------|-----------|------|
| 1 | 触发 configTable（61 项 per-skill/per-unique 触发配置）未建模，CoC/CWDT/unique 触发链路缺失 | 🔴 high | missing | CalcTriggers.lua:882-1418 configTable + :1436 四级 key 查表 | calc_orchestrator.rs:1504（仅 gate 内建触发） | support/unique 触发关系无识别入口，相关 build 触发面板退化为自施法 |
| 2 | 触发源速率用基础 use_time，未用计算后的攻速/施速 | 🔴 high | incorrect | CalcTriggers.lua:67-70/:74-87 GlobalCache HitSpeed/Speed | calc_orchestrator.rs:1595-1599 `rate = 1.0/use_time_s` | 堆攻速的触发 build 源速率系统性低估 |
| 3 | Minions/Spectres 数据未 JSON 化：仅 4 条手抄 Rust 常量，数据管线零支持 | 🔴 high | design | Data/Minions.lua（32 条，auto-generated）+ Data/Spectres.lua（593 条） | pobr-data/src/minion.rs:261-405；data/4.5.0.3.4/ 无 minions.json | 违反"框架稳定、只换 data JSON"核心目标 |
| 4 | 召唤物未接入 build 链路：orchestrator 不识别召唤物宝石，召唤物面板恒空 | 🔴 high | missing | CalcPerform.lua:964-971/:983+；CalcActiveSkill.lua:846-907 minionList | env.rs:32/:48 API 仅测试可达；pobr-build grep minion 零命中 | 导入任何召唤物 build，OutputTable.minions 恒为空 |
| 5 | 召唤物技能（createMinionSkills）未建模：法术系召唤物 DPS 无来源 | 🔴 high | missing | CalcActiveSkill.lua:1049-1119 createMinionSkills | perform.rs:75-117 仅虚拟武器；skill_list 存而不用 | 法术召唤物伤害完全错误 |
| 6 | 非伤害异常消费侧闭环缺失：敌方施加循环 + Shock→enemy DamageTaken 不影响玩家 DPS | 🔴 high | missing | CalcPerform.lua:3077-3180 施加循环 + :1152-1160 保证型来源 | perform.rs:660-701 仅写面板值；全仓相关 grep 零命中 | 感电系 build 与 PoB2 的一阶 DPS 偏差 |
| 7 | CalcMirages 幻影域整体缺失（Mirage Archer / Saviour / Tawhoa / Sacred Wisps / General's Cry） | 🟡 medium | missing | CalcMirages.lua:22/:56 + 五类配置分支 | 无（仅 pob2_parity.rs:137、skill_stat_map.rs:394 注释） | 相关 build 幻影 DPS 全部为 0，parity 测试已自证偏差 |
| 8 | defaultTriggerHandler 修正族缺失：双持 /2、命中/暴击折入、addsCastTime、Unleash、多投射、trigger bots、TriggeredDamage 注入 | 🟡 medium | partial | CalcTriggers.lua:431/:729-770/:44-62/:443/:439/:23-30 | perform.rs:381+ fill_trigger（仅 cap/rotation/CWC） | 逐项都是触发面板一阶误差项，CoC 缺暴击率折算尤甚 |
| 9 | minion modDB 装配缺注入项：hiddenDamageFixup、HeavyStunBuildup、mapLevelLifeMult、装备/箭袋通道 | 🟡 medium | partial | CalcPerform.lua:1016/:1013-1014/:989-991/:1031-1060 | minion.rs write_intrinsics（仅基础属性） | hiddenDamageFixup 缺失致召唤物伤害基线系统性偏差 |
| 10 | 玩家自身异常（self-ailment）缺失：自身冰缓/冰冻→ActionSpeed、unravel 条件 | 🟡 medium | missing | CalcPerform.lua:673-691 | 无（grep SelfChillEffect 零命中） | 防御/生存口径下自身减速无模拟 |
| 11 | nonDamagingAilment / buildupTypes / gameConstants 参数硬编码 Rust，未进 data/<版本>/ JSON | 🟡 medium | design | Data.lua:347-351/:353+/:203-208 + Misc.lua gameConstants | monster.rs:111/:117、constants.rs:146、GameConstants::poe2() | 版本数值变动需改框架代码重编译；ChillMax override 通道缺失 |
| 12 | 能量驱动元宝石模型超出 PoB2 基线、无 parity 参照，且当前为悬空代码 | 🟢 low | design | CalcTriggers.lua grep energy 零命中；:1089 CoC 仍 PoE1 口径 | trigger.rs:264/:338（无任何调用方） | 需明确 parity 模式口径 + 补游戏实测 fixture |

---

## 缺口详述

### 1. 🔴 触发 configTable（61 项）未建模，CoC/CWDT/unique 触发链路缺失

**PoB2 证据**：`CalcTriggers.lua:882-1418` configTable（"mjolner":1075、"cast on critical strike":1089、"cast when damage taken":1113 等 61 项，已逐项计数）+ `:1436 calcs.triggers` 四级 key 查表。
**pobr 位置**：`crates/pobr-build/src/calc_orchestrator.rs:1504 trigger_modifiers`（:1515-1523 仅 gate 在 skill_types 含 Triggered/InbuiltTrigger 的内建触发）。

**影响**：pobr 当前只覆盖"被触发技能自带 Triggered/InbuiltTrigger 类型"的内建触发与 CWC 单技能路径；由 support 宝石（CoC、CWDT、Spellslinger……）或 unique 物品（Mjolner、Vixen's、Svalinn……）建立的触发关系完全没有识别入口（orchestrator L1486 注释自承"数据模型无 gem-link / triggeredBy 关系"）——这些 build 的触发面板退化为自施法。FINDINGS 03-01/03-02 标"support-gem 触发链路 defer"，但未量化范围：实为 61 项配置表。

**修复方向**：触发条件谓词（triggerSkillCond/triggeredSkillCond）是闭包逻辑，而"技能名→触发器类型/几率/插槽匹配规则"本质是数据，可抽成 `trigger_configs.json` 表 + 少量条件原语（见数据/逻辑切分节）。前置依赖：build 数据模型需要 gem-link / triggeredBy 关系。

### 2. 🔴 触发源速率用基础 use_time，未用计算后的攻速/施速

**PoB2 证据**：`CalcTriggers.lua:67-70 defaultComparer / :74-87 findTriggerSkill`——`GlobalCache.cachedData[env.mode][uuid].HitSpeed or Speed`（源技能完整子计算结果，缺缓存时 `calcs.buildActiveSkill` 现算）。
**pobr 位置**：`calc_orchestrator.rs:1562 in_group_trigger_source_rate`（:1595-1599 `rate = 1.0/resolved.use_time_s`，宝石基础用时）。

**影响**：PoB2 的 EffectiveSourceRate 来自源技能**含全部攻速/施速词条**的缓存计算；pobr 注入的 TriggerSourceRate 只取 resolve_skill_level_with_gem_bonus 给出的宝石基础 use_time 倒数。任何堆攻速的触发 build（CoC 类核心玩法）源速率被系统性低估，触发 DPS 偏低且**随攻速投资无增长**——这是定性级错误，不只是数值偏差。

**修复方向**：需要"对组内源技能跑一次速度子计算"的机制，即 PoB2 GlobalCache 的等价物（一次性子计算 + 缓存源技能 HitSpeed/Speed，再喂给 fill_trigger）。

### 3. 🔴 Minions/Spectres 数据未 JSON 化（4 条手抄常量 vs 32+593 条导出数据）

**PoB2 证据**：`Data/Minions.lua`（43K，32 条，文件头"automatically generated, do not edit"已核实）+ `Data/Spectres.lua`（624K，593 条同 schema）。
**pobr 位置**：`crates/pobr-data/src/minion.rs:261/296/331/366`（minion_def_zombie 等 4 个硬编码构造函数）+ :405 Spectre 占位；`data/4.5.0.3.4/` 无 minions.json（已核实）；pobr-gamedata、tools/pobr-data-adapter grep minion 零命中（已核实）。

**影响**：这是本领域最典型的"数据混在代码里"：PoB2 侧两份文件 100% 是导出数据（与 pobr 三分离目标完全同构），pobr 的 MinionDef schema 已设计好且字段一一对应，但 adapter 没有导出步骤、gamedata 没有 loader，结果 32+593 条数据退化成 4 条手抄常量内嵌 Rust——每个版本更新都要改框架代码，直接违反"框架稳定、只换 data/<版本>/*.json"的核心目标。

**修复方向**：adapter 增加 minions.json / spectres.json 导出（从 .dat 的 MonsterVarieties 反范式化，或临时从 Minions.lua 转换），gamedata 增加懒加载域，catalog.rs 把 MinionDef 挂进 DataManifest（schema 已写好但 manifest/loader/adapter 三处都没挂）。

### 4. 🔴 召唤物未接入 build 链路

**PoB2 证据**：`CalcPerform.lua:964-971`（activeSkill.minion→createMinionSkills）/ `:983+` env.minion modDB 装配；`CalcActiveSkill.lua:846-907`（grantedEffect.minionList→minion 列表 + minionLevelTable 等级判定）。
**pobr 位置**：`crates/pobr-core/src/calc/env.rs:32 add_minion / :48 add_minion_from_def`（仅 Env API + 测试可达）；calc_orchestrator grep minion 零命中（已核实）。

**影响**：core 侧 minion.rs 是 greenfield 就绪状态（基础属性派生/三通道注入/perform_minions 都有），但 orchestrator 解析 build 时不识别 Raise Zombie 等召唤物宝石、不解析 grants-minion 的 granted effect、从不调用 add_minion_from_def。导入任何召唤物 build，OutputTable.minions 恒为空。

**修复方向**：需要 granted effect → MinionDef 的 minionList 外键数据 + orchestrator 接线。**数据出处注意**（核查修正项）：宝石→召唤物关联的 minionList 字段在 PoB2 不在 Data/Gems.lua（grep 0 命中），而在 granted effect 技能数据（Data/Skills/act_int.lua:16184 等）——pobr 的 granted_effects.json schema 需补该字段。

### 5. 🔴 召唤物技能（createMinionSkills）未建模

**PoB2 证据**：`CalcActiveSkill.lua:1049-1119 createMinionSkills`（skillList→env.data.skills、按 levelRequirement≤minion.level 选级、:1061 ExtraMinionSkill、:1104 minionDamageEffectiveness）。
**pobr 位置**：`perform.rs:75-117 perform_minions`（仅 MinimalInput::from(minion.base) 虚拟武器）；MinionDef.skill_list 字段存而不用（calc 侧零读取，已核实）。

**影响**：PoB2 召唤物的主 DPS 来自它自己的技能（如 Storm Mage 的 ArcSkeletonMageMinion，走 granted effect 等级数据），虚拟武器只是 melee 类的基础。pobr 只算虚拟武器物理攻击：法术召唤物伤害完全错误，melee 召唤物也缺技能自身的 damage multiplier / added damage。

**修复方向**：所需数据（skill_list→granted_effects/granted_effect_levels）pobr 数据管线已具备（granted_effects.json 已入库），缺的是"用 skillList 解析召唤物主技能并喂进 offence 管线"的编排逻辑——这是纯框架工作，无新数据依赖。

### 6. 🔴 非伤害异常消费侧闭环缺失

**PoB2 证据**：`CalcPerform.lua:3077-3180`（敌方异常施加循环：ailments 表聚合 XOverride/XMinimum、Current/Maximum 输出、Condition:Chilled/Shocked、:3094 Bonechill ColdDamageTaken、ChillCanStack/ShockCanStack 叠层、Shock→enemy DamageTaken INC、Multiplier:ChillEffect/ShockEffect 回写）、`:1152-1160`（Shock Ground ShockOverride :1155 / Skitterbots / supportBonechill 保证型）。
**pobr 位置**：`perform.rs:660-701`（chill_traced/shock_traced 仅写面板 chill_effect/shock_effect）；grep `Condition:Shocked|Condition:Chilled|ShockOverride|ChillOverride|Bonechill` 于 crates/apps/tools 全部零命中（已核实）。

**影响**：pobr 把 chill/shock 算成展示值后即止：不向 enemyDB 写 DamageTaken INC（**感电的全部 DPS 价值**）、不写 Condition:Chilled/Shocked（大量"对抗冰缓敌人"词条失效）、不支持配置"敌人已感电/冰缓"输入（setup_env.rs/config.rs/build_config.rs 无 shock/chill 命中）、无 Bonechill/叠层/保证型来源。对感电系 build 这是与 PoB2 的一阶 DPS 偏差。注意：ailment.rs 中的 DamageTaken 链只用于异常 DoT 的 effMult 消费方向，与"感电回灌敌方受伤"是不同通道，不可混淆为已实现。

**修复方向**：实现敌方异常施加循环（Override/Minimum 聚合 → Current/Maximum → 写 enemy ModDb 的 DamageTaken INC 与 Condition）+ build_config 增加"敌人已感电/冰缓"配置输入 + 保证型来源通道。这一段是纯逻辑（聚合语义），输入参数（default/max/precision）应随 nonDamagingAilment 数据走（见 gap 11）。

### 7. 🟡 CalcMirages 幻影域整体缺失

**PoB2 证据**：`CalcMirages.lua:22 calculateMirage、:56 calcs.mirages`、五类配置分支（Mirage Archer ~L63-116、Saviour、Tawhoa's Chosen L178+、Sacred Wisps、General's Cry）。
**pobr 位置**：无实现（grep 仅 pob2_parity.rs:137 与 skill_stat_map.rs:394 的缺口注释）。

**影响**：PoB2 用 copyActiveSkill 把玩家技能复制进隔离 env、注入 MirageArcherLessDamage/SaviourMirageWarriorLessDamage 等 MORE + QuantityMultiplier 后重跑 perform，幻影 DPS 计入主面板。pobr 完全没有"复制技能到隔离环境重算"的机制，使用这 5 类机制的 build 幻影 DPS 全部为 0——pobr 自己的 parity 测试注释（"Mirage Deadeye 全局 −25% more 缺失"）已承认该缺口造成回归偏差。

**修复方向**：机制本身是少量逻辑（calculateMirage 框架：复制技能 → 注入修正 → 重跑 → 写回）+ 5 份配置数据（哪个 stat 提供数量/less-damage），配置适合 JSON 化（`mirage_configs.json`）。前置依赖与 gap 2 同类：需要"子环境重算"的框架能力。

### 8. 🟡 defaultTriggerHandler 修正族缺失

**PoB2 证据**：`CalcTriggers.lua:431`（dual wield /2）、`:729-770`（sourceHitChance×CritChance→triggerChance，含双武器独立 roll；核查修正：原报告 719-770 微偏）、`:44-62 processAddedCastTime`、`:443 unleashDpsMult`、`:439 storedUses→ignoresTickRate`、HaveTriggerBots ×2、`:23-30 addTriggerIncMoreMods`。
**pobr 位置**：`perform.rs:381+ fill_trigger`（仅 cap/rotation/CWC + trigger_chance_multiplier 通道）。

**影响**：pobr 03-01 留了 trigger_chance_multiplier 接口，但 build 层从不注入源技能命中率/暴击率——CoC 触发率应乘源 crit chance，这是 CoC 的核心折减。双持源 /2、法术施放时间并入冷却、Unleash/多投射对源速率的放大、充能型 ignoresTickRate 旁路、TriggeredDamage INC/MORE→Damage 注入均无对应实现。逐项都是触发面板的一阶误差项。

**修复方向**：依赖 gap 2 的源技能子计算机制（命中/暴击折算需要源技能完整 crit/hit 输出）；其余修正项可在 fill_trigger 中逐项补齐，均为不随版本变化的规则逻辑。

### 9. 🟡 minion modDB 装配缺 PoB2 注入项

**PoB2 证据**：`CalcPerform.lua:1016`（Damage MORE hiddenDamageFixup "Hidden Level Scaling"）、`:1013-1014`（Physical/EnemyHeavyStunBuildup）、`:989-991`（hostile×mapLevelLifeMult）、`:1031-1060`（弓+箭袋/itemSet AddList）。
**pobr 位置**：`minion.rs write_intrinsics`（仅 crit/必中/life/armour/evasion/ES/抗性）。

**影响**：hiddenDamageFixup 是 PoB2 对 minion 等级伤害表的隐藏修正 MORE 乘区（CalcActiveSkill.lua 由 monsterAllyDamageTable 派生），缺失会让召唤物伤害基线系统性偏差；其余为次级通道（重击积累常量影响 stun 派生、敌对 minion 生命、Bow-minion 共享箭袋词条）。

**修复方向**：随 gap 4/5 的接线工作一并补齐；hiddenDamageFixup 的派生输入（monsterAllyDamageTable）需进数据侧（见切分建议第 3 条）。

### 10. 🟡 玩家自身异常（self-ailment）缺失

**PoB2 证据**：`CalcPerform.lua:673-691`（ChillVal override + SelfChillEffect→ActionSpeed INC -effect、:682 SelfChillEffectIsReversed、Freeze→ActionSpeed -70×SelfChillEffect、Unravelling→ChaosCanFreeze/Ignite/Shock，逐行核实）。
**pobr 位置**：无（grep SelfChillEffect 于 crates/apps/tools 零命中，已核实；仅 ailment.rs player_ailment_threshold）。

**影响**：配置"你被冰缓/冰冻"时 PoB2 把 ActionSpeed 全局降低（影响玩家攻速/施速/移动派生），pobr 无此通道；SelfChillEffectIsReversed（特定 unique）与 chaos-unravel 旗标也无。影响防御/生存口径下的自身减速模拟。

**修复方向**：在 perform 的 ActionSpeed 装配段（pobr 已有 action speed 独立乘区，见 skill_use_time.rs）增加 self-chill/freeze 输入通道 + build_config 配置项。

### 11. 🟡 异常参数硬编码 Rust，未进 data/<版本>/ JSON

**PoB2 证据**：`Data.lua:347-351 data.nonDamagingAilment`（default/min/max/duration 引 Misc.lua gameConstants：ChillMaxEffect=50 :77、BaseShockMagnitude=20 :75，已核实）、`:353+ buildupTypes`、`:203-208 Bleed/Poison/IgnitePercentBase`。
**pobr 位置**：`monster.rs:111/:117`（CHILL_MAX_EFFECT=50/CHILL_MIN_EFFECT=30 const）、`constants.rs:146`（SHOCK_MIN_EFFECT=20）、GameConstants::poe2()。

**影响**：这些参数在 PoB2 由 .dat 导出的 gameConstants 驱动（0.5.0 改过 BaseShockMagnitude=20、Chill min=30），pobr 写成 Rust const——版本更新这些数值变动时需要改框架代码并重编译，违反三分离目标。且 pobr chill clamp 上限固定 50，PoB2 用 gameConstants["ChillMaxEffect"] 且可被 ChillMax override（CalcPerform.lua:680 `modDB:Override(nil,"ChillMax")`），override 通道缺失。

**修复方向**：迁入 `data/<版本>/game_constants.json`（pobr catalog 现无此表）；公式中的 clamp 上下限改为从数据读 + 支持 ModDb Override。

### 12. 🟢 能量驱动元宝石模型：超出 PoB2 基线、无 parity 参照、悬空代码

**说明**：PoE2 真实元宝石是能量机制，vendor PoB2 自己尚未实现（CalcTriggers.lua grep -i energy 零命中；:1089 CoC 仍按源暴击率折算，PoE1 口径）。pobr 的能量模型（trigger.rs:264/:338，出处 agent-docs/triggers.md + act_int.lua）可能比 PoB2 更接近游戏，但无法被 ninja_parity 回归验证（基准是 PoB2 输出）；且已核实 `calc_energy_trigger_rate` 在 perform.rs 与 pobr-build 中均无调用方（仅 trigger.rs 内部 traced 包装），属悬空代码。需明确：parity 模式下走哪个口径、能量模型只在"超越 PoB2"模式启用，并补游戏实测 fixture。

---

## 数据 vs 逻辑切分建议

本领域是三分离目标的最佳试金石——PoB2 把"纯数据"与"逻辑"混得最严重的地方恰好集中在这里。

### 纯数据（应 JSON 化）

1. **Data/Minions.lua（43K/32 条）与 Data/Spectres.lua（624K/593 条）**：文件头自注"automatically generated, do not edit"（已核实），每条是 `{归一化乘数, 抗性, attackTime, limit, reservation, monsterTags, skillList, modList}` 的平铺记录——与 pobr 已有 `MinionDef`（crates/pobr-data/src/minion.rs）schema 一一对应。**这是 pobr 缺失的最大一块入库数据**：应由 pobr-data-adapter 从 .dat（MonsterVarieties + 相关表）反范式化产出 `data/<版本>/minions.json` + `spectres.json`，pobr-gamedata 加懒加载域。当前 4 条手抄 Rust 常量是反模式。另注意 granted effect 的 `minionList` 外键（召唤技能→召唤物种类）在 PoB2 位于技能数据（Data/Skills/act_int.lua 等），**pobr 的 granted_effects.json schema 需补该字段**。
2. **Modules/Data.lua 的异常参数块**（nonDamagingAilment L347-351、buildupTypes、defaultAilmentDamageTypes、Bleed/Poison/IgnitePercentBase L203-208）与 **Data/Misc.lua gameConstants**：全部是版本敏感数值（0.5.0 的 BaseShockMagnitude=20 / ChillMaxEffect=50 已核实），pobr 现为 Rust const（monster.rs / constants.rs / GameConstants::poe2()），应迁入 `data/<版本>/game_constants.json`（pobr catalog 现无此表）。
3. **怪物等级表**（monsterAilmentThresholdTable / PoiseThresholdTable / AllyLife / AllyDamage 等，Misc.lua 各 100 项）：pobr 已抄进 monster.rs 静态表——同理应入 JSON（可并入 game_constants 或单独 monster_tables.json）。minion 伤害基线必需的 monsterAllyDamageTable / hiddenDamageFixup 派生输入目前完全没进 pobr。

### 半数据（配置表 + 少量条件原语，可大部分 JSON 化）

4. **CalcTriggers.lua configTable（L882-1418，61 项）**：表面是 Lua 闭包，但拆开后 90% 是声明性事实——"触发器名 X：源条件=Attack 且同插槽组 / 被触发条件=triggeredByCoc / triggerChance=来自 stat Y / 用 cast rate 还是 attack rate / 是否 globalTrigger"。建议设计 `trigger_configs.json`：

   ```json
   {
     "trigger_id": "cast_on_critical_strike",
     "source_skill_filter": { "skill_types": ["Attack"], "slot_rule": "same_group" },
     "triggered_skill_filter": { "flag": "triggeredByCoc" },
     "chance_stat": "ChanceToTriggerOnCrit",
     "use_cast_rate": false,
     "special_handler_id": null
   }
   ```

   少数真特殊（Doom Blast/Vixen、战吼 uptime）留 handler 枚举给框架。
5. **CalcMirages 的 5 类配置**同理：`{mirage_id, count_stat, less_damage_stat, skill_match 规则}` 可全数据化（`mirage_configs.json`），calculateMirage 框架本身是稳定逻辑。
6. **Minions.lua 每条的 modList 与 skillList→granted effect 关联**：数据侧已有 granted_effects.json 承接，缺的是 minion 记录里的外键。

### 纯逻辑（留框架）

7. 触发冷却帧模型、calcMultiSpellRotationImpact 轮转模拟、几何分布折算（pobr trigger.rs 已对齐）；defaultTriggerHandler 的修正族（双持/命中暴击折算/Unleash）——这些是规则，不随版本数据变。但其中"源技能完整子计算速率"依赖 GlobalCache 等价机制，是**框架级缺口**（gap 2）。
8. 召唤物独立 actor 装配 + createMinionSkills 选级逻辑 + 三通道注入语义（pobr minion.rs 已就绪一半，缺 minion 技能编排与 build 接线）。
9. 异常公式（chill 线性 / shock 幂 0.4 / poise buildup）与敌方施加循环聚合语义（Override/Minimum/max/叠层）——逻辑留框架，但其中每个常数（50、0.4、ChillEffectMultiplier）都应从 game_constants.json 读。

### pobr 当前 JSON schema 缺口汇总

`data/<版本>/` 现有 11 个文件全部是宝石/词缀/天赋域（已核实目录清单）。本领域需要新增：

| 新增文件 | 性质 | 内容 |
|----------|------|------|
| `minions.json` | 纯数据 | 32 条召唤物基础定义（MinionDef schema 已就绪） |
| `spectres.json` | 纯数据 | 593 条魂灵定义（同 schema） |
| `game_constants.json` | 纯数据 | nonDamagingAilment / buildupTypes / gameConstants / 怪物等级表 |
| `trigger_configs.json` | 半数据 | 61 项触发配置（声明性部分）+ handler 枚举 |
| `mirage_configs.json` | 半数据 | 5 类幻影机制配置 |

另：`granted_effects.json` 需补 minionList 外键字段；`catalog.rs` 需补 MinionDef 入 DataManifest（schema 已写好但 manifest / loader / adapter 三处都没挂）。

---

## 附录：核查说明

核查范围与方法：逐条打开全部 6 条 severity=high 的 gap 的双侧引用，另抽查 4 条疑点（medium gap 7/8/10/11、low gap 12、以及结构地图中"vendor 无 Energy 模型"的断言）。PoB2 侧用 grep+sed 分段读 CalcTriggers.lua / CalcMirages.lua / CalcPerform.lua / CalcActiveSkill.lua / Data.lua / Misc.lua / Data/Minions.lua / Spectres.lua / Data/Gems.lua / Data/Skills/act_int.lua；pobr 侧全局 grep crates/apps/tools 防止"在别处实现"误判。

**全部 6 条 high 查实成立，保留**：

1. configTable 61 项：awk 计数 L882-1418 恰为 61 项，calcs.triggers 四级 key 查表属实，pobr orchestrator L1515-1523 确实只 gate 内建触发且注释自承无 gem-link 数据；
2. 源速率用基础 use_time：calc_orchestrator.rs:1595-1599 `rate = 1.0/use_time` 逐字核实，PoB2 defaultComparer/findTriggerSkill 用 GlobalCache HitSpeed/Speed 核实；
3. Minions/Spectres 未 JSON 化：32/593 条目数、auto-generated 文件头、data 目录无 minions.json、gamedata/adapter grep 零命中全部核实；
4. 召唤物未接线：pobr-build/src grep minion 零命中核实，env.rs:32/:48 API 存在核实；
5. createMinionSkills 缺失：CalcActiveSkill.lua:1049/:1104/:1061 核实，pobr skill_list 字段仅在 def 构造赋值、calc 侧零读取核实；
6. 异常消费闭环缺失：CalcPerform 3077+ 施加循环（Bonechill :3094、ShockOverride :1155）核实，pobr 全仓 grep Condition:Shocked/ChillOverride/Bonechill/SelfChillEffect 零命中核实，并补充澄清 ailment.rs 的 DamageTaken 链只是 DoT effMult 消费方向、与感电回灌是不同通道。

**修正项**：

- ① gap「召唤物接线」detail 的数据出处有误：宝石→召唤物关联的 minionList 字段不在 Data/Gems.lua（grep 0 命中），而在 granted effect 技能数据（Data/Skills/act_int.lua:16184 等）——已改写 detail 与切分建议对应段，并据此把落地建议改为"granted_effects.json 补 minionList 外键"。
- ② 行号修正："cast on critical strike" 在 L1089 而非 L1110（L1110 附近是 nova/CWDT 区域）、mjolner L1075、命中/暴击折算段 L729-770（原 719-770）、hiddenDamageFixup MORE 在 CalcPerform L1016、mapLevelLifeMult L989-991、defaultComparer L67-70——均已同步更正。
- ③ gap「能量模型悬空」：核实 calc_energy_trigger_rate 在 perform.rs 与 pobr-build 均无调用方，"悬空代码"断言成立，标题补"悬空"。

**抽查的 medium/low 均成立，未降级未删除**：mirage 全缺（pobr 仅两处注释提及缺口，无实现，且 parity 测试注释自证偏差影响）；dual wield /2 在 L431 逐字核实；self-chill L673-691 逐行对上（SelfChillEffectIsReversed 在 L682）；nonDamagingAilment 在 Data.lua L347-351、Misc.lua ChillMaxEffect=50/:77、BaseShockMagnitude=20/:75，pobr 侧 CHILL_MAX_EFFECT/SHOCK_MIN_EFFECT 硬编码 const 核实，ChillMax override 通道在 PoB2 CalcPerform:680 存在而 pobr 无。结构地图中"grep energy 0 命中"断言复核为真。无条目需要删除或降级；所有 severity 维持原级。
