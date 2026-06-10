# 数据层与导出管线（核心：数据/框架分离）

> 审计日期：2026-06-10 ｜ 领域：数据层与导出管线（数据/框架分离）
> 角色：只读分析。本文聚焦上一轮 calc parity 审计（`audits/pob2-parity-2026-06-09/FINDINGS.md`）未覆盖的数据管线与数据/框架切分问题。

---

## PoB2 代码结构（结构地图）

PoB2 的数据层分三段：**Export/（离线生成器）→ Data/（生成物 + 手工数据）→ Modules/Data.lua（运行时装配）**，与 pobr 的 `pipeline/ → pobr-data-adapter → data/<版本>/ → pobr-gamedata` 链路逐段对应。

```
GGPK .dat ──(Export 模板 + Scripts，叠加人工策展)──> Data/*.lua
          ──(Modules/Data.lua 装配 + 二次加工)──> data.* ──> 计算模块取用
```

### (1) Export/ —— 离线生成器（= pobr 管线对应物）

`Export/Main.lua`（875 行）是一个 GGPK/.dat 浏览器 + 代码生成宿主。核心机制是 `processTemplateFile(name, inDir, outDir, directiveTable)`（Main.lua:112）：读取**手工维护的模板 txt**（如 `Export/Skills/act_str.txt` 28K、`Export/Bases/*.txt`），逐行执行 `#directive`，由 `Export/Scripts/*.lua` 里的 directive handler 查 .dat 表并写出 `Data/*.lua`。

关键 handler 见 `Export/Scripts/skills.lua`：

| directive | 位置 | 作用 |
|---|---|---|
| `#skill` | skills.lua:126 | 选定技能（GrantedEffect） |
| `#set` | skills.lua:470 | 选择/取舍 stat-set |
| `#flags` | skills.lua:782 | 人工指定技能 flags |
| `#baseMod` | skills.lua:791 | 人工注入 baseMods 常量 |
| `#mods` | skills.lua:798 | 结束 stat 段落 |

**模板里携带大量人工策展知识（技能 flags、baseMods 常量、stat 取舍），不在 GGG dat 中**。实例：`Export/Skills/act_int.txt:639` 的 `#baseMod mod("Speed", "MORE", 285, ModFlag.Attack)` 属于 FlickerStrikePlayer。

`Export/Scripts/statdesc.lua` + `Export/statdesc.lua` 解析 `stat_descriptions.csd`，把 stat id + 数值渲染成英文词条文本，是 `mods.lua`（生成 ModItem/ModFlask/ModJewel/ModCorrupted/ModRunes/ModVeiled 等 10 个词缀池文件）和 `skills.lua` 的**共同依赖**。

Scripts 共 23 个：assets / bases / bossData / costs / enums / essence / flavourText / legionPassives / minions / miscdata / modScalability / mods / pantheons / passivetree(+_ggg) / skillGemList / skills / soulcores / spectreList / statdesc / uModsToText / worldAreas。

### (2) Data/ —— 38MB 生成物 + 手工数据

**自动生成**（头部标 "automatically generated"）：

| 文件/目录 | 规模 | 内容 |
|---|---|---|
| `Gems.lua` | 501K | 宝石 → grantedEffectId / additionalStatSet1-2 / additionalGrantedEffectId / tags / weaponRequirements / Tier |
| `Skills/act_*\|sup_*\|other\|minion\|spectre.lua` | ~4MB | 每技能 levels / qualityStats / statSets{statMap, baseFlags, constantStats, stats, levels} |
| `Bases/*.lua` | 27 文件 | 基底，含 req / socketLimit / implicit 文本 |
| `Mod*.lua` 系列 | ModItem 1MB（~2550 行）、ModItemExclusive 1.8M 等 | 渲染文本 + weightKey/weightVal + tradeHashes；ModRunes/ModJewel/ModCorrupted/ModVeiled/ModFlask/ModCharm/ModMap |
| `Misc.lua` | 23K | "From DefaultMonsterStats.dat" 怪物等级表（monsterLife/Damage/Armour/Evasion/AilmentThreshold）+ "From GameConstants.dat" gameConstants + characterConstants/monsterConstants |
| `ModScalability.lua` | 1.3M | catalyst/品质可缩放性标记 |
| `ModCache.lua` | 1MB | mod 文本 → 预解析 modlist 缓存（Lua 解析性能补偿） |
| `StatDescriptions/*.lua` | stat_descriptions.lua 3.9M + 9 份 | stat → 文本模板 |
| 其它 | — | `Costs.lua`、`Essence.lua`、`Minions/Spectres.lua`、`WorldAreas.lua`、`QuestRewards.lua`、`QueryMods/TradeSiteStats.lua` |

**手工维护**：

| 文件 | 规模 | 内容 |
|---|---|---|
| `SkillStatMap.lua` | 105K | **954 条 stat id → 内部 mod 映射**，含 div、PerStat/Multiplier/GlobalEffect tag、skill() data key |
| `Uniques/` | 31 文件（含 Special/Generated/New） | 暗金库，含 variant |
| `Rares.lua` | 24K | trader 模板 |
| `Global.lua` | — | ModFlag/KeywordFlag/SkillType 位枚举（PoB 内部语义） |

### (3) Modules/Data.lua —— 运行时装配（1066 行）

LoadModule 串起全部 Data/*，并叠加一层手工数据与加工逻辑：

- **手工数据**：`data.misc` magic numbers（:171，ServerTickTime/LeechRateBase/DotDpsCap/boss DPS 乘数等，部分引用 gameConstants）、cursePriority（:274）、keystones（:304）、nonDamagingAilment（:347）、highPrecisionMods（:415）、weaponTypeInfo（:532）、unarmedWeaponData（:553）、jewelRadii（:595）
- **加工逻辑**：skillStatMap metatable 把映射懒绑定到每个 statSet（:834-887）、setupGem 链接 gem→grantedEffectList/additionalGrantedEffects（:901-980）、boss armour/evasion 均值统计与 boss 配置文案（:775-830）、itemBases 按 type 建索引、Uniques 装载（:1054-1060）

---

## pobr 实现现状

管线链路：

```
pipeline/（download-index.mjs，按 config.json 从 GGG CDN 下载 16 张表 .dat→JSON，English+繁中）
  → tools/pobr-data-adapter（1957 行 Rust，5 文件）
      main.rs 395 行（物品基底 + 武器/护甲数值）
      mods.rs 183 行（词缀/stat 注册表）
      skills.rs 699 行（宝石/授予效果/分等级/stat-set/cost types）
      tree.rs 198 行 + tree_coords.rs 482 行（天赋树）
  → data/4.5.0.3.4/ 11 个 JSON
      base_items 1.5M ｜ mods 5.5M ｜ stats 2.7M ｜ skill_gems 227K
      granted_effects 2.4M ｜ granted_effect_levels 1.4M ｜ granted_effect_stat_sets 3.8M
      cost_types ｜ passive_tree(+meta) ｜ i18n/zh-TW 边车
  → schema：crates/pobr-data/src/catalog.rs（457 行）
  → 运行时 loader：crates/pobr-gamedata（162 行，全量 serde 反序列化、缺域向后兼容）
```

**已覆盖**（对应 PoB2 Data/）：

- Gems + Skills 的核心计算面：levels 的 cost/cooldown/critChance/baseMultiplier + stat-set 分等级伤害/速度/暴击/穿透 stat（经 skills.rs `is_mappable_stat`（约 :375）白名单过滤）
- Bases 的数值面（WeaponTypes/ArmourTypes join）
- Mods 的结构面（stat id + 掷值区间 + tags，**无文本**）
- Costs（cost_types.json 对齐 Costs.lua）
- 天赋树（走 GGG 官方树导出而非 dat，含坐标回填）

**以硬编码 Rust 常量存在（数据进框架，反模式）**：

| 文件 | 规模 | 对应 PoB2 |
|---|---|---|
| `pobr-data/src/monster.rs` | 46.6K | Misc.lua 怪物表逐字移植 |
| `pobr-data/src/constants.rs` | 8.3K | 抗性上限/护甲系数/帧时间（gameConstants 面） |
| `pobr-data/src/minion.rs` | 25.1K | Minions.lua 子集 |
| `pobr-core/src/campaign.rs` | 135 行 | QuestRewards 的抗性惩罚面 |
| `pobr-build/src/skill_stat_map.rs` | 751 行 | SkillStatMap 的后缀启发式重写 |

**完全缺失**：StatDescriptions（`pobr-data/src/stat.rs:38` 有一个 `StatDescription{id,text}` 占位 struct，但无任何解析/渲染实现与数据）、Uniques、Essence、Rares、WorldAreas、FlavourText、ModScalability、Spectres、QueryMods/TradeSiteStats、宝石 qualityStats、ModItem 系的文本/权重层。

Export 23 个脚本中，pobr 实质覆盖约 **5 个域**（mods/skills/bases/costs/passivetree）；statdesc 这一最核心的共享依赖为零。

**管线不可重现点**：catalog 的 `skill_attack_speed_more`（catalog.rs:297）由 vendor Lua 手工合并（数据 JSON 中恰 1 条 285.0），适配器恒写 None（skills.rs:544），重跑会冲掉。

---

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|---|---|---|---|---|---|
| 1 | stat_descriptions（statdesc）渲染链路完全缺失 | 🔴 high | missing | Export/Scripts/statdesc.lua + Export/Scripts/mods.lua:1-5；Data/StatDescriptions/（10 份，主文件 3.9MB） | 无（仅 stat.rs:38 占位 struct，零使用者） | 词缀池→modifier 文本通路断裂，craft/暗金展示/i18n 词条全部受阻 |
| 2 | SkillStatMap 被固化为框架内 Rust 启发式 | 🔴 high | design | Data/SkillStatMap.lua（954 条，含 div/tag/skill key）+ Modules/Data.lua:834-887 | pobr-build/src/skill_stat_map.rs（751 行启发式）+ skills.rs is_mappable_stat 白名单 | 条件型/带 tag 映射丢失、双重静默丢弃、加 stat 需改框架重发版 |
| 3 | Export 模板人工策展层无系统性通道，已现一次性手工补丁 | 🔴 high | missing | skills.lua directive set(:470)/flags(:782)/baseMod(:791)；act_int.txt:639 FlickerStrikePlayer 285 baseMod | skills.rs:544 恒写 None；JSON 中恰 1 条手工合并的 285.0 | 管线重跑即丢手工数据，不可重现 |
| 4 | Misc.lua 全局常数与怪物等级表硬编码为 Rust 常量 | 🔴 high | design | Data/Misc.lua（From DefaultMonsterStats/GameConstants/MinionGemLevelScaling.dat）+ Modules/Data.lua:171 | monster.rs 46.6K / constants.rs 8.3K / minion.rs 25.1K；pipeline 未下载这三张表 | 版本数据进框架，换版本须改代码重编译 |
| 5 | 宝石 qualityStats 数据缺失，宝石品质对计算无效 | 🔴 high | missing | Data/Skills/act_int.lua:17 起 qualityStats（act_int 内 154 处） | catalog 无 quality 域；build.rs:22 GemSkillRef 无 quality；with_quality 零调用 | 20 品质宝石按 0 品质算，确定性 DPS 偏差 |
| 6 | Gems 域字段缺口：additionalStatSet/additionalGrantedEffectId/weaponRequirements/GemEffects 表 | 🟡 medium | partial | Data/Gems.lua + Modules/Data.lua:901-980 setupGem | catalog.rs:154-157 TODO；GemEffects FK 列已下载但目标表未下载 | 多 stat-set/多授予效果宝石、武器限制校验无数据 |
| 7 | mods.json 缺 spawn weights、词缀文本与前后缀语义 | 🟡 medium | partial | Data/ModItem.lua（weightKey/weightVal/modTags/tradeHashes） | mods.rs 零处 SpawnWeight/Families（列已下载被丢弃） | 词缀合法性/权重/分组互斥不可知，craft 与 trade 前置缺口 |
| 8 | base_items.json 缺 req（属性/等级需求）与 socketLimit | 🟡 medium | partial | Data/Bases/wand.lua（req/socketLimit/implicit/quality） | catalog.rs:33 BaseItemDef 无 req/socket_limit | 装备需求校验、符文插槽上限无数据 |
| 9 | Minions/Spectres/Bosses 数据域基本缺失（少量硬编码） | 🟡 medium | partial | Data/Minions.lua(43K)/Spectres.lua(623K)/Bosses.lua + Data.lua:775-830 | minion.rs 硬编码；Spectres 全缺；boss 均值写死 | 召唤 build parity 受限；同属数据进框架反模式 |
| 10 | Uniques/ 暗金数据库缺失 | 🟡 medium | missing | Data/Uniques/*（31 文件）+ Data.lua:1054-1060 | 无（仅 ItemRarity::Unique 枚举值） | 暗金导入/variant/装备库不可用；build code 原文可兜底 |
| 11 | ModScalability 缺失（catalyst/品质可缩放性） | 🟢 low | missing | Data/ModScalability.lua（1.3M） | 无 | 上轮审计 catalyst defer 的数据前置 |
| 12 | QuestRewards 清单未数据化 | 🟢 low | partial | Data/QuestRewards.lua + Data.lua:1062 | campaign.rs（135 行，仅硬编码惩罚面） | 任务奖励改动又要改框架代码 |
| 13 | WorldAreas/Essence/Rares/FlavourText/TimelessJewelData/QueryMods/TradeSiteStats 缺失 | 🟢 low | missing | Data/ 对应文件（QueryMods 579K + TradeSiteStats 1M 等） | 无（pobr-trade 为 MockBackend） | 按用途分级，QueryMods 是 trade 真实接入必备映射 |

统计：🔴 high × 5 ｜ 🟡 medium × 5 ｜ 🟢 low × 3

---

## 缺口详述

### Gap 1（🔴 high）stat_descriptions（statdesc）渲染链路完全缺失

- **PoB2 证据**：`Export/Scripts/statdesc.lua`（processStatFile）+ `Export/Scripts/mods.lua:1-5`（`loadStatFile("stat_descriptions.csd")`）；`Data/StatDescriptions/stat_descriptions.lua`（3.9MB）等 10 份。
- **pobr 位置**：无。grep crates/apps/tools/pipeline 无 stat_descriptions 处理；data/4.5.0.3.4 无对应 JSON；`pobr-data/src/stat.rs:38` 的 `StatDescription{id,text}` 仅为占位 struct、无使用者。

**影响**：PoB2 的 mods.lua 导出脚本第 4-5 行即依赖 statdesc，所有词缀池文件（ModItem 等）和技能描述都靠 statdesc 把 stat id + 数值渲染为英文文本，再喂给 ModParser。pobr 的 mods.json 只有 stat id + 掷值区间，无法渲染成可读/可解析文本：**词缀池 → modifier 的通路断裂**，crafting、暗金展示、按 mod id 注入装备词条、i18n 词条文本全部无从谈起。当前 pobr 仅靠 build XML 自带的英文文本行兜底，凡是需要"从数据生成文本"的场景（自建物品、词缀浏览）都被阻塞。**这是数据管线最大的单点缺口。**

**修复方向**：pipeline 下载 `stat_descriptions.csd`（及 skill/gem 等变体），适配器实现 csd 解析（条件区间 + 占位符模板），落 `stat_descriptions.json`（stat ids + 数值区间 → 文本模板）；框架侧实现模板渲染器，打通 mod id → 文本 → ModParser（或直接 id → modifier 短路通道）。

### Gap 2（🔴 high）SkillStatMap 被固化为框架内 Rust 启发式

- **PoB2 证据**：`Data/SkillStatMap.lua`（954 条目，含 div、PerStat/Multiplier/GlobalEffect tag、skill() data key，如 :1844 `active_skill_projectile_damage_+%_final_for_each_remaining_chain → PerStat:ChainRemaining`）+ `Modules/Data.lua:834-887`（statMap metatable 懒绑定）。
- **pobr 位置**：`crates/pobr-build/src/skill_stat_map.rs`（751 行后缀启发式，模块文档自述"只映射已知的无条件族…未知/条件型前缀返回 None"）+ `tools/pobr-data-adapter/src/skills.rs is_mappable_stat`（约 :375，入库白名单）。

**影响**：PoB2 把 stat → 内部 mod 的映射作为一张**可维护的数据表**（每条可带 div 换算、PerStat/Multiplier 等 tag、duration 等 skillData key）；pobr 把它重写成按后缀猜的 Rust 代码（`MappedStat` 仅有 mod_name/mod_type/scale 三元组，无条件 tag 表达能力）。后果：

1. 条件型/带 tag 的 stat 映射不出——如 Arc 的 `arc_damage_+%_final_for_each_remaining_chain → PerStat:ChainRemaining MORE`（该条在 PoB2 是 `Data/Skills/act_int.lua:70` 的每技能 statMap override，SkillStatMap.lua 本身也有大量同类 PerStat/div 条目）；
2. duration/AoE 等非伤害 skillData 不注入；
3. 适配器白名单先丢一遍、计算侧启发式再丢一遍，**双重静默丢失**；
4. 每次游戏加新 stat 需改框架代码重发版，违背"版本只更新 data JSON"的目标。

**修复方向**：把映射表 JSON 化为声明式 schema（`stat_id → {mod_name, type, div, tags[]}`），框架只留解释器；每技能 statMap override 走 Gap 3 的 overlay 通道。

### Gap 3（🔴 high）Export 模板人工策展层无系统性通道，已出现一次性手工补丁

- **PoB2 证据**：`Export/Scripts/skills.lua` directiveTable.set(:470)/flags(:782)/baseMod(:791)；`Export/Skills/act_int.txt:639`（FlickerStrikePlayer 的 `#baseMod mod("Speed","MORE",285,ModFlag.Attack)`）；`Data/Skills/act_int.lua` ArcPlayer 的 statSets[1].statMap(:70)/baseFlags/constantStats。
- **pobr 位置**：`tools/pobr-data-adapter/src/skills.rs:544`（skill_attack_speed_more 恒写 None）；`data/4.5.0.3.4/granted_effect_stat_sets.json` 中恰 1 条 `"skill_attack_speed_more": 285.0` 为手工合并；catalog.rs:297 字段注释自述"由 vendor Lua 抽取合并"。

**影响**：PoB2 的 Data/Skills/*.lua **不是纯 dat 导出**——Export 模板 txt 里手工维护每技能的 flags、baseMods 常量、stat-set 取舍与 statMap override。pobr 管线只消费 GGG dat，这一层知识系统性缺位；目前唯一的 baseMod（Flicker 的 285% 攻速 MORE）是绕过适配器直接改产物 JSON 合入的，**适配器重跑会把它冲掉——管线不可重现**。

**修复方向**：设计"人工 overlay JSON"层——版本目录下独立文件（如 `data/<版本>/overlay/skill_overrides.json`），适配器输出后做确定性 merge；否则随覆盖面扩大会积累越来越多不可再生的手工补丁。

### Gap 4（🔴 high）Misc.lua 全局常数与怪物等级表硬编码为 Rust 常量

- **PoB2 证据**：`Data/Misc.lua`（头部注释 "From DefaultMonsterStats.dat" / "From GameConstants.dat" / "From MinionGemLevelScaling.dat"，自动生成）+ `Modules/Data.lua:171` 起 data.misc。
- **pobr 位置**：`crates/pobr-data/src/monster.rs`（46.6K 常量数组）、`constants.rs`（8.3K）、`minion.rs`（25.1K）；`pipeline/config.json` 16 张表中无 DefaultMonsterStats/GameConstants/MinionGemLevelScaling。

**影响**：怪物 life/damage/armour/evasion/accuracy/ailmentThreshold/poiseThreshold 百级表、MonsterAccuracyBase 等 gameConstants、characterConstants 都是**随补丁变化的版本数据**，PoB2 由 miscdata.lua 自动从 dat 重新生成；pobr 把它们逐字写进 pobr-data 的 Rust 源码。数值当前正确（有对 Misc.lua 的逐项断言），但每次游戏版本更新都要人工改框架 crate 并重编译，**直接违背"框架稳定、每版本只换 data/<版本>/*.json"的项目核心目标**。

**修复方向**：pipeline/config.json 补下 DefaultMonsterStats、GameConstants、MinionGemLevelScaling 三张表，适配器落 `misc_constants.json` / `monster_tables.json`，pobr-gamedata 增加对应域；Rust 常量保留为测试断言基准后逐步退役。

### Gap 5（🔴 high）宝石 qualityStats 数据缺失，宝石品质对计算无效

- **PoB2 证据**：`Data/Skills/act_int.lua:17` 起 `ArcPlayer.qualityStats = {{"number_of_chains", 0.1}}`（act_int.lua 内共 154 处 qualityStats，每技能一份）。
- **pobr 位置**：catalog.rs SkillGemDef/SkillStatSetDef 无 quality 域；`pobr-build/src/build.rs:22` GemSkillRef 只有 gem_level 无 quality（XML 导入即丢弃品质）；`pobr-core/src/skill_source.rs:277` with_quality 全仓库无调用方；calc_orchestrator.rs 的 quality 引用全部是 item quality。

**影响**（核查后比原始描述更严重）：PoB2 每个技能携带品质 stat 及每 1% 的增量。pobr 不仅 catalog 没有这份数据，**连 build XML 解析层的 GemSkillRef 都没有 quality 字段——宝石品质在导入阶段就被丢弃**；skill_source 虽留了 quality_mods 注入口（with_quality）但全仓库零调用。导入 build 中 20 品质的宝石按 0 品质计算，对依赖品质词条的技能（链数、持续、伤害类品质）产生确定性 DPS 偏差。

**修复方向**：管线导出品质 stat 表（GGG 品质表）→ SkillStatSetDef 增加 quality_stats 字段 → XML 导入层 GemSkillRef 补 quality → orchestrator 接线 with_quality。

### Gap 6（🟡 medium）Gems 域字段缺口：additionalStatSet / additionalGrantedEffectId / weaponRequirements / GemEffects 表未导出

- **PoB2 证据**：`Data/Gems.lua`（IceNova 的 additionalStatSet1/2）；`Modules/Data.lua:901-980` setupGem 组装 grantedEffectList/additionalGrantedEffects/Vaal 映射。
- **pobr 位置**：catalog.rs:154-157（TODO 自述"GemEffects 表当前 pipeline 未导出，宝石→授予效果的直接连边暂缺"）；SkillGemDef 仅颜色/类型/属性需求。**核查澄清**：pipeline/config.json 的 SkillGems 表已下载 GemEffects FK 列，缺的是被指向的 GemEffects 目标表本身，FK 无从解析。

**影响**：pobr 当前靠 build XML 的 `<Gem skillId>`（= GrantedEffects.Id）直连授予效果，绕过了 gem→effect 连边缺失；但多 stat-set 技能（Ice Nova on Frostbolt 形态）、多授予效果宝石（Vaal 系等）、按宝石名建技能、武器类型限制校验（weaponRequirements）都没有数据支撑。Tier/naturalMaxLevel/tags 缺失影响等级上限与 UI。

**修复方向**：pipeline 下载 GemEffects 表，适配器解析 FK 补全 SkillGemDef（weapon_requirements/tags/tier/additional_stat_sets/additional_granted_effects）。

### Gap 7（🟡 medium）mods.json 缺 spawn weights、词缀文本与前后缀语义

- **PoB2 证据**：`Data/ModItem.lua`（~2550 行：affix 名、渲染文本 `"+(5-8) to Strength"`、weightKey/weightVal、modTags、tradeHashes；`Export/Scripts/mods.lua` 按 Domain/GenType 分流生成 10 个文件）。
- **pobr 位置**：`tools/pobr-data-adapter/src/mods.rs`（183 行，grep 确认零处 SpawnWeight/Families 处理——尽管 pipeline/config.json Mods 表已下载 SpawnWeight_Tags/Families 两列）。

**影响**：pobr 的 mods.json 保留了 domain/generation_type/tags/掷值，但**丢弃了已下载的 SpawnWeight_Tags/Families 列**，且无文本（依赖 Gap 1）。后果：词缀在哪些基底上合法、权重多少、词缀分组互斥（group/family）都不可知——pobr-item（custom item 编辑）和未来 craft 模拟的直接前置缺口。tradeHashes 则是 pobr-trade 真实接入的前置。

**修复方向**：适配器消费已下载的两列，ModDef 补 `spawn_weights[{tag,weight}]` / `family` / `affix_kind`（前后缀语义化）/ `trade_hashes`。

### Gap 8（🟡 medium）base_items.json 缺 req 与 socketLimit

- **PoB2 证据**：`Data/Bases/wand.lua`（每基底 req={...}、socketLimit=3、implicit 文本行、quality=20）。
- **pobr 位置**：catalog.rs:33 BaseItemDef（字段核实：id/name/item_class/drop_level/width/height/tags/implicits(mod id)/mod_domain/weapon/armour——无 req/socket_limit）。

**影响**：PoB2 基底带穿戴需求和符文插槽上限（`Export/Scripts/bases.lua` 从 ComponentAttributeRequirements 等表 join）。pobr 缺这两字段：装备需求校验、符文（ModRunes）数量上限无数据；implicit 是 mod id，落到计算还需经 Gap 1 的文本渲染或直接 id→modifier 通道。

**修复方向**：pipeline 补下 ComponentAttributeRequirements 等表，BaseItemDef 补 `req{str,dex,int,level}` / `socket_limit`。

### Gap 9（🟡 medium）Minions/Spectres/Bosses 数据域基本缺失

- **PoB2 证据**：`Data/Minions.lua`(43K)/`Spectres.lua`(623K)/`Bosses.lua`/`BossSkills.lua`；`Modules/Data.lua:775-830`（boss armour/evasion 均值与 boss 配置文案由 Bosses 数据统计）。
- **pobr 位置**：`crates/pobr-data/src/minion.rs`（25.1K 硬编码 Rust）、setup_env.rs 的 boss 常数。

**影响**：召唤物/灵体的 modList、skillList、生命/伤害系数在 PoB2 是完整数据域；pobr 只硬编码了少量 minion 数据，Spectres 完全缺失，boss 的 armour/evasion 均值是写死的而非由 boss 数据统计。召唤 build 的 parity 受限于此；且 minion.rs 同样属于"数据进框架"反模式。

**修复方向**：落 `minions.json` / `spectres.json` / `bosses.json` + `boss_skills.json`，boss 均值在 loader 装配阶段统计（对应 Data.lua:775-830 的加工逻辑留在框架）。

### Gap 10（🟡 medium）Uniques/ 暗金数据库缺失

- **PoB2 证据**：`Data/Uniques/*`（31 文件，含 Special/Generated/New），`Modules/Data.lua:1054-1060` 装载（行号已核实）。
- **pobr 位置**：无（pobr-data/src/item.rs 仅有 `ItemRarity::Unique` 枚举值，无暗金数据）。

**影响**：PoB2 的暗金库是手工维护的物品文本块（含 variant 标记，Special/Generated 是程序生成式暗金）。pobr 完全没有：从 trade/名字导入暗金、variant 切换、装备库浏览均不可用。calc parity 上 build code 自带原文可兜底，故定 medium，但这是"手工数据 overlay"需要 JSON 化的代表性域。

**修复方向**：直接定义 `uniques.json` schema 并写一次性 Lua→JSON 转换器，归入 overlay 通道（Gap 3 同一机制）。

---

## 数据 vs 逻辑切分建议（核心关注点）

PoB2 的数据/逻辑光谱可分**四层**，pobr 需要分别对待：

### (1) 纯生成数据 —— 应 100% JSON 化

头部标 "automatically generated" 的 Data/ 文件：Gems、Skills/*、Bases/*、Mod* 词缀池 10 文件、Misc（怪物表 + 三组 constants）、Costs、Essence、Minions/Spectres、WorldAreas、QuestRewards、FlavourText、ModScalability、QueryMods/TradeSiteStats、StatDescriptions。

pobr 已 JSON 化其中约 5 个域的"计算核心面"（gems/skills/bases/mods/costs），但 **Misc 与 Minions 被错误地搬进了框架代码**（monster.rs 46.6K、minion.rs 25.1K、constants.rs 8.3K、campaign.rs 的惩罚常数）——虽然集中在 pobr-data 这一零逻辑 crate、比散落在 calc 里好，但仍要重编译才能换版本，是当前**最直接违背分离目标**的部分。

### (2) 人工策展数据 —— 应设计为独立 overlay JSON

PoB2 以手工 Lua/模板维护的部分：SkillStatMap.lua（954 条 stat→mod 映射）、Export 模板里的 #baseMod/#flags/#set 指令与每技能 statMap override、Uniques/、Rares、Modules/Data.lua 内嵌表（cursePriority、keystones、nonDamagingAilment、weaponTypeInfo、unarmedWeaponData、jewelRadii、highPrecisionMods、data.misc 中非 gameConstants 派生的 magic numbers）。

这一层**不在 GGG dat 里**、跨版本相对稳定但会随机制改动更新。pobr 当前的处理是反模式双重奏：

- 要么重写成框架代码（skill_stat_map.rs 启发式）；
- 要么绕过管线手改产物 JSON（skill_attack_speed_more，数据中恰 1 条、适配器重跑即丢）。

**正确形态**：`data/<版本>/overlay/` 下的独立 JSON（`skill_stat_map.json`、`skill_overrides.json`、`uniques.json`、`misc_overrides.json`），适配器输出后做**确定性 merge**，框架只留 schema 与解释器。

### (3) 装配/加工逻辑 —— 留在框架（定位正确）

Modules/Data.lua 的 statMap metatable 绑定、setupGem 连边、boss 均值统计、itemBases 索引构建；Export/Scripts 的外键解析/反范式化。pobr 对应物（pobr-gamedata loader、build_data.rs、pobr-data-adapter）定位正确。ModCache 这类 Lua 性能补偿层在 Rust 侧**无需对应物**（设计性差异，非缺口）。

### (4) 框架常量 —— 留在代码合理

Global.lua 的 ModFlag/KeywordFlag 位枚举是 PoB 内部语义而非游戏数据，pobr-data/src/modifier.rs 的 bitflags 是对的；SkillType 枚举源自 ActiveSkillType 表，pobr 用字符串直传更稳。

### 目标 schema 清单（catalog.rs 增量）

**新表**：

| JSON 文件 | 内容 | 对应缺口 |
|---|---|---|
| `stat_descriptions.json` | stat id + 值 → 文本模板 | Gap 1（根） |
| `skill_stat_map.json` | 声明式映射：mod_name/type/div/tags | Gap 2 |
| `skill_quality_stats.json` | 每技能品质 stat + 增量 | Gap 5 |
| `uniques.json` | 暗金库（含 variant） | Gap 10 |
| `misc_constants.json` | gameConstants/characterConstants/monsterConstants | Gap 4 |
| `monster_tables.json` | 怪物百级表 | Gap 4 |
| `minions.json` / `spectres.json` | 召唤物/灵体 | Gap 9 |
| `bosses.json` + `boss_skills.json` | boss 数据与技能 | Gap 9 |
| `quest_rewards.json` | 任务奖励清单 | Gap 12 |
| `world_areas.json` | 区域等级 | Gap 13 |
| `essences.json` | 精髓 craft 数据 | Gap 13 |
| `mod_scalability.json` | catalyst/品质可缩放性 | Gap 11 |
| `trade_stat_map.json` | QueryMods 对应（PoB mod → trade API stat id） | Gap 13 |

**字段级增量**：

- `BaseItemDef` 补 `req{str,dex,int,level}` / `socket_limit`
- `SkillGemDef` 补 `weapon_requirements` / `tags` / `tier` / `additional_stat_sets` / `additional_granted_effects`（需下载并导出 GemEffects 表——SkillGems 的 GemEffects FK 列已在 pipeline/config.json 下载，缺的是目标表）
- `SkillLevelDef` 补 `level_requirement`（已核实现仅 level/cooldown_ms/attack_time_ms/cost_amounts/attack_speed_multiplier/base_multiplier）
- `SkillStatSetDef` 补 `quality_stats`，并把 `skill_attack_speed_more` 一类 vendor 补丁迁入 overlay 通道
- `ModDef` 补 `spawn_weights[{tag,weight}]` / `family` / `affix_kind`（前后缀语义化）/ `trade_hashes`
- XML 导入层 `GemSkillRef` 补 `quality` 字段

**pipeline/config.json 新增下载**：DefaultMonsterStats、GameConstants、MinionGemLevelScaling、GemEffects、宝石品质 stat 表、ComponentAttributeRequirements、`stat_descriptions.csd` 等。

---

## 附录：核查说明

核查范围：全部 5 条 high + 3 条 medium（Gap 6 Gems 字段、Gap 7 spawn weights、Gap 8 base req）+ 结构性陈述抽查（Export 行号、Modules/Data.lua 行号、文件规模）。逐条结论：

- **【high#1 statdesc】成立。** Export/Scripts/mods.lua:1-5 确有 `loadStatFile("stat_descriptions.csd")`；Data/StatDescriptions/ 下 stat_descriptions.lua 3.9M 等 10 份文件存在。pobr 侧全局 grep（crates/apps/tools/pipeline）无任何处理；唯一发现 pobr-data/src/stat.rs:38 有 StatDescription{id,text} 占位 struct 但零使用者，已补入说明（不改变结论）。保留 high。
- **【high#2 SkillStatMap】成立，例证出处微修。** SkillStatMap.lua 实测 954 条；skill_stat_map.rs 751 行、模块文档自述"只映射已知的无条件族"、MappedStat 仅 mod_name/type/scale 无 tag 表达能力，全部属实。修正点：报告举的 Arc 例（`arc_damage_+%_final_for_each_remaining_chain → PerStat:ChainRemaining`）实际位于 Data/Skills/act_int.lua:70 的每技能 statMap override 而非 SkillStatMap.lua 本身；但 SkillStatMap.lua:1844 有同构条目（`active_skill_projectile_damage_+%_final_for_each_remaining_chain`），论点不受影响，已标注准确出处。保留 high。
- **【high#3 人工策展层】成立且证据链完整。** directiveTable.skill(:126)/set(:470)/flags(:782)/baseMod(:791)/mods(:798) 行号全部精确命中；Export/Skills/act_int.txt:639 找到 `#baseMod mod("Speed","MORE",285,ModFlag.Attack)`，向上回溯确认属于 #skill FlickerStrikePlayer；skills.rs:544 恒写 None、granted_effect_stat_sets.json 中恰 1 条 `"skill_attack_speed_more": 285.0`、catalog.rs:297 注释自述 vendor 手工合并——"重跑即丢"断言成立。保留 high。
- **【high#4 Misc 硬编码】成立。** Misc.lua 头部确为 "automatically generated"，分段注释 "From DefaultMonsterStats.dat"/"From GameConstants.dat"/"From MinionGemLevelScaling.dat"；pipeline/config.json 16 张表中无这三张；monster.rs 46.6K / minion.rs 25.1K / constants.rs 8.3K 实测（原报告写 45.6K/24.5K，按实测微调）。保留 high。
- **【high#5 qualityStats】成立且比原描述更严重。** act_int.lua 154 处 qualityStats、ArcPlayer 的 `{"number_of_chains",0.1}` 原文命中；pobr 侧 catalog 零 quality 字段、skill_source.rs:277 with_quality 全仓库零调用方、calc_orchestrator 的 quality 全是 item quality——均属实。新发现：pobr-build/src/build.rs:22 GemSkillRef 只有 gem_level，XML 导入层连宝石品质数值都不解析（导入即丢），已补强描述。保留 high。
- **【medium#6 GemEffects】基本成立，一处澄清：** pipeline/config.json 的 SkillGems 表其实已下载 "GemEffects" FK 列，缺的是 GemEffects 目标表本身（catalog.rs:154-157 TODO 原话"GemEffects 表当前 pipeline 未导出"），已修正措辞。保留 medium。
- **【medium#7 spawn weights】成立。** mods.rs（183 行）grep 零处 SpawnWeight/Families；config.json Mods 表确已下载 SpawnWeight_Tags/Families 两列。保留。
- **【medium#8 base req/socketLimit】成立。** BaseItemDef 实测字段无 req/socket_limit，implicits 为 mod id。保留。
- **【抽查 medium#10 Uniques / low#12 QuestRewards】** Modules/Data.lua:1054-1060 装载 Uniques 行号命中（Uniques/ 实为 31 个文件，含 Special 子目录，已微调）；campaign.rs 实测恰 135 行、纯硬编码。均成立。

事实性修正（非 gap 条目）：pob2_structure 中 Export/Main.lua 826 行 → 实测 875 行；pobr_status 中 data-adapter 2576 行 → 实测 1957 行（5 文件逐一 wc 验证）；ModItem.lua "2549 条" 改为 ~2550 行（实测 2554 行，条目数与行数近似）。无条目被删除或降级——所有 high 的 PoB2 引用行号经实际打开验证全部精确，pobr 侧缺失均经全局 grep 复核确认非"在别处实现"。
