# 技能与宝石

> 领域范围：gem 数据模型 / support 适用性 / 等级·品质 progression / statSet→mod 映射 / meta·spirit gem
> 审计日期：2026-06-10　|　所有引用均已打开实际代码核查（见文末附录）

## PoB2 代码结构

PoB2 在该领域 = **三层数据文件 + 两个消费模块 + 一个导出脚本**。

### 数据层（全部自动生成，文件头标注 "automatically generated, do not edit"）

| 文件 | 体积 | 内容 |
|------|------|------|
| `Data/Gems.lua` | 501K（~1000 颗宝石） | 宝石身份表，key = Metadata 路径。每条含 `name`/`variantId`/**`grantedEffectId`**（指向技能定义）/**`additionalStatSet1/2`**（全库 149 处，已核实；替代 PoE1 skill part 的"附加形态"，如 IceNova → IceNovaPlayerOnFrostbolt + IceNovaColdInfusedPlayer，Gems.lua:11-12）/**`additionalGrantedEffectId1`**（162 处，已核实；meta gem 附带的第二效果）/`tags`/`gemType`/`weaponRequirements`（如 Leap Slam 限锤）/`reqStr·Dex·Int`/`Tier`/`naturalMaxLevel` |
| `Data/Skills/{act_str,act_dex,act_int,sup_*,minion,spectre,other}.lua` | 合计 ~5MB | granted effect 定义 `skills["FireballPlayer"]`，含 `skillTypes`、`castTime`、**`qualityStats`**（每 1% 品质给的 stat，如 Fireball act_int.lua:7234-7236 `spell_skills_fire_2_additional_projectiles_final_chance_% 0.5`）、**`levels`**（40 级表：cost/critChance/levelRequirement/manaMultiplier/reservationMultiplier/**spiritReservationFlat**（act_int.lua 精确 1319 处）/attackSpeedMultiplier/damageMultiplier/PvPDamageMultiplier/storedUses）、**`statSets`** 数组（详见下）。support 条目额外有 **`requireSkillTypes` / `addSkillTypes` / `excludeSkillTypes`**（后缀表达式 token 流，可含 `SkillType.AND/OR/NOT`）、`gemFamily`、`isLineage`、`ignoreMinionTypes` |
| `Data/SkillStatMap.lua` | 105K（**954 条**全局映射，已精确核实） | stat id → 内部 modifier 模板（`mod`/`flag`/`skill` 三种构造器 + `div`/`mult`/`base`/`value` 参数 + `PerStat` 等 tag），经 `Data.lua:835-847` 以 metatable 懒加载挂到每个 `statSet.statMap` |

`statSets` 数组的结构（PoE2 用它完全取代 PoE1 的多 part）：

```text
statSets[i]
├── label                       # 如 Fireball 有 "Projectile" / "Explosion" 两套
├── baseEffectiveness / incrementalEffectiveness
├── baseFlags                   # area / projectile 等，按 set 独立
├── constantStats
├── stats 列表 + 按 gem level 的数值矩阵（含 statInterpolation 1/2/3）
├── per-set statMap 覆盖        # 如 Arc `arc_damage_+%_final_for_each_remaining_chain`
│                               #   → mod("Damage","MORE", PerStat ChainRemaining)
│                               #   （act_int.lua:69-72，全库 statMap 出现 ~390 处）
└── baseMods                    # 手工 directive 注入的常量 mod
```

### 消费层（框架逻辑）

| 模块 | 关键函数 | 职责（均已逐行核实） |
|------|----------|----------------------|
| `Modules/CalcTools.lua` | `canGrantedEffectSupportActiveSkill`（:84-110） | support 兼容裁决：cannotBeSupported / supportGemsOnly / fromItem 特例 / excludeSkillTypes 后缀表达式 / isTrigger / requireSkillTypes 后缀表达式 |
| | `doesTypeExpressionMatch`（:60-81） | AND/OR/NOT 栈式求值器 |
| | `validateGemLevel` | 等级 clamp 到 naturalMaxLevel |
| | `buildSkillInstanceStats`（:138 起） | 取数+插值：品质 stat `math.modf(rate×quality)` + statInterpolation==3 的 effectiveness 插值 + ==2 的按 actorLevel 线性插值 + constantStats 叠加 |
| `Modules/CalcActiveSkill.lua` | `createActiveSkill`（:181-210） | 两遍处理 support：pass1 对兼容 support 做 **addSkillTypes 不动点循环**保证 support 顺序无关；pass2 收编兼容 support 进 effectList |
| | `mergeSkillInstanceMods` | 对选中 statSet 全量 merge、对未选 statSet 只 merge global stat；statMap 查表后按 `value or statValue×mult×scalar/div+base` 注入 |
| | `buildActiveSkillModList`（:689-700） | support level 字段消费：manaMultiplier→SupportManaMultiplier MORE / reservationMultiplier→ReservationMultiplier MORE / manaReservationPercent / spiritReservationFlat→ExtraSpirit BASE；另有多 trigger 冲突禁用、storedUses、active level 字段消费（critChance/damageMultiplier/attackTime/attackSpeedMultiplier/cooldown） |
| `Modules/CalcSetup.lua` | :1716（已核实） | `gemData.additionalGrantedEffects` 展开——meta gem 一颗宝石产出多个技能 |

### 导出脚本

`Export/Scripts/skills.lua`：从 `.dat` 生成上述 Data 文件。关键外键：`GrantedEffects.SupportTypes/AddTypes/ExcludeTypes` → require/add/excludeSkillTypes；**`GrantedEffectQualityStats`**（:304-313，已核实，StatValues/1000）→ qualityStats；`SupportGems.Family` → gemFamily；手工 directive（`#addSkillTypes`、statMap 覆盖、baseMods）混入生成结果。

### 数据流

```text
Gems.lua（宝石→效果索引）
  → Data/Skills（效果定义 + 等级矩阵）
  → SkillStatMap + per-set statMap（stat→mod 模板）
  → createActiveSkill（support 兼容裁决）
  → buildSkillInstanceStats（取数 + 插值 + 品质）
  → mergeSkillInstanceMods（注入 skillModList）
  → CalcOffence / CalcPerform
```

## pobr 实现现状

pobr 已打通"主技能 + support 数值 → 真实 DPS"的最小链路，但宝石域的数据模型明显比 PoB2 薄。

### 数据管线

- `pipeline/config.json` 下载：`GrantedEffects`（仅 Id/IsSupport/ActiveSkill/StatSet/CastTime/CostTypes/AllowedActiveSkillTypes/AddedActiveSkillTypes 共 8 列，已核实）、`GrantedEffectsPerLevel`（仅 Cooldown/CostAmounts/AttackTime + Level/GrantedEffect，磁盘表已核实只有 6 个 key）、`GrantedEffectStatSets(PerLevel)`、`SkillGems`（含 GemEffects 列）、`ActiveSkills`、`CostTypes`。**无 GrantedEffectQualityStats、无 SupportGems 表**。
- `tools/pobr-data-adapter/src/skills.rs`（699 行）适配为 4 个 JSON：
  - `skill_gems.json`：仅 id/gem_type/颜色/需求，GemEffects 列不解析、无 granted_effect 关联；
  - `granted_effects.json`：is_support/active_skill/cast_time/skill_types（位集化）/stat_set/cost_types；AddedActiveSkillTypes 已下载且 121 行非空，但 `RawGrantedEffect` 无对应 serde 字段；
  - `granted_effect_levels.json`：cooldown/attack_time/cost_amounts + crit_chance（3912 值）/attack_speed_multiplier（3578 值）/base_multiplier——后三者来自一次性 headless PoB 抽取 merge，当前 pipeline 表中对应列为 0，**不可再生**；
  - `granted_effect_stat_sets.json`：constantStats + 分级 stats，经 `is_mappable_stat`（skills.rs:374）白名单过滤后入库；只跟 `GrantedEffects.StatSet` 主链接，additional set 丢弃。

### 计算侧

- `crates/pobr-core/src/skill_source.rs`（644 行）：`ingest_active_gem`/`ingest_support_gem`/`ingest_gem_leveled`，带 SourceId 归因（PoBR 增量）；`can_support`（:379）仅实现 require∩active 交集语义；`SupportGemSpec` 有 `mana_multiplier`（`with_mana_multiplier`）和 quality/quality_mods（`with_quality`）接口，但 grep 全仓（crates/apps/tools，排除测试）无任何调用方喂数据——**核心层是就绪的空架子**。
- `crates/pobr-build/src/skill_stat_map.rs`（751 行，已核实）：把 PoB SkillStatMap 重写成 Rust 后缀启发式，条件型 stat 保守跳过。
- `calc_orchestrator.rs`：`resolve_main_skill`（:152 跳 meta 壳选伤害技能）、`skill_base_modifiers`、`support_modifiers`（:1611，只检查 `is_support==true` 即全量注入，不调 `can_support`）、`aura_buff_modifiers`/`self_buff_offensive_modifiers`、`resolve_skill_level_with_gem_bonus`。
- Build 模型：`GemSkillRef = {skill_id, gem_level}`（build.rs:22-28）；`xml_build.rs:743-765` 只解析 `<Gem skillId level enabled>`——**quality 属性不解析**（已核实样本 build sorceress-stormweaver-comet 的 decoded.xml 有 15 个 `quality="20"` 宝石被丢弃）。

### 覆盖度评估

| 已对齐（有 ninja_parity 门禁兜底） | 整块空白 |
|------|------|
| 等级 progression（整数档）、cost/cooldown、support 数值倍率、光环防御 buff、attackSpeedMultiplier/critChance | 品质、多 statSet、support 兼容裁决、spirit 预留、SupportManaMultiplier、meta gem 展开 |

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|------|--------|------|-----------|-----------|------|
| 1 | 宝石品质（quality）链路缺失：数据表、Build 模型、XML 导入、orchestrator 接线四层皆空 | 🔴 high | missing | skills.lua:304-313；CalcTools.lua:138-146；act_int.lua:7234-7236 | pipeline/config.json；build.rs:22；xml_build.rs:743-765；skill_source.rs:277 | core 层有未接线的归因 API；样本 build 15 颗 q20 宝石被静默丢弃 |
| 2 | support 适用性裁决未在 build 路径执行：无 exclude/后缀表达式/addSkillTypes 不动点，orchestrator 注入前不调 can_support | 🔴 high | partial | CalcTools.lua:84-110 / :60-81；CalcActiveSkill.lua:181-210 | skill_source.rs:379；calc_orchestrator.rs:1611；pipeline/config.json | 数据列、语义、接线三层断裂；FINDINGS 02-06 低估了范围 |
| 3 | SkillStatMap 被实现为 Rust 后缀启发式而非数据表，adapter 端 is_mappable_stat 二次白名单造成不可恢复的数据丢失 | 🔴 high | design | Data/SkillStatMap.lua（954 条）；Data.lua:835-847；act_int.lua:69-72 | skill_stat_map.rs（751 行）；skills.rs:374-413 | 把数据错放进框架，与"数据进 JSON、框架稳定"目标直接相悖 |
| 4 | 多 statSet / additionalStatSet（PoE2 的 skill-part 等价物）未建模：每个效果只入库主 statSet | 🔴 high | missing | Gems.lua:11-12（149 处）；act_int.lua statSets；CalcActiveSkill.lua | catalog.rs:190；skills.rs:466-475；Build 模型 | additional set 行已在磁盘表中却被 adapter 丢弃；多形态技能只能算默认形态 |
| 5 | meta gem / additionalGrantedEffects 不展开，且 skill_gems.json 与 granted effect 间无关联键（需补 GemEffects 中间表） | 🟡 medium | missing | Gems.lua additionalGrantedEffectId1（162 处）；CalcSetup.lua:1716 | catalog.rs:150-159；calc_orchestrator.rs:152-157 | 脱离 PoB2 XML 自建 build 时"宝石→效果"无数据可查 |
| 6 | spirit 预留与 reservation 全族数据列缺失（spiritReservationFlat / reservationMultiplier / manaReservationPercent） | 🟡 medium | missing | act_int.lua spiritReservationFlat（1319 处）；CalcActiveSkill.lua:689-700 | catalog.rs SkillLevelDef；pipeline/config.json；cost_types.json | Spirit 超载的非法配置也照算 |
| 7 | SupportManaMultiplier 数据与消费双缺：被支援技能 cost 不受 support 倍率影响 | 🟡 medium | partial | CalcActiveSkill.lua:689-691；sup_int.lua（64 处） | skill_source.rs:248；skill_mechanics.rs:539；granted_effect_levels.json | API 空架子 + cost 公式明确 defer |
| 8 | statSet baseMods 与 crit/attack-speed 等 vendor 抽取列不可再生——重跑 adapter 必静默丢数据 | 🟡 medium | design | CalcActiveSkill.lua baseMods；skills.lua directive | skills.rs:541-545 / :95-102；pipeline 磁盘表；git dc03599/c290b79 | 3912+3578 个已 merge 值在版本更新时会静默丢失 |
| 9 | gem 等级 progression 细节：naturalMaxLevel 缺失 + statInterpolation 2/3 插值未实现 | 🟢 low | partial | CalcTools.lua validateGemLevel / :147-196；Gems.lua | build_data.rs:195；skill_gems.json | 整数档查表已对；minion 域开工前需补插值 |
| 10 | gemFamily / weaponRequirements 等宝石级约束未入库（合法性校验缺数据） | 🟢 low | missing | sup_str.lua gemFamily；Gems.lua weaponRequirements；CalcActiveSkill.lua getWeaponFlags | 无（pipeline 无 SupportGems 表） | 纯数据问题，逻辑很薄 |

## 缺口详述

### Gap 1 🔴 宝石品质（quality）链路缺失（missing）

**PoB2 证据**：`Export/Scripts/skills.lua:304-313`（GrantedEffectQualityStats → qualityStats，StatValues/1000）；`Modules/CalcTools.lua:138-146` `buildSkillInstanceStats`（`stats[stat] += math.modf(rate × quality)`）；`Data/Skills/act_int.lua:7234-7236`（Fireball qualityStats 每 1% 品质 0.5）。

**pobr 位置**：`pipeline/config.json`（无 GrantedEffectQualityStats 表，已核实）；`crates/pobr-build/src/build.rs:22` `GemSkillRef`（无 quality 字段）；`crates/pobr-build/src/xml_build.rs:743-765`（只取 skillId/level，不取 quality 属性）；`crates/pobr-core/src/skill_source.rs:277` `with_quality`（API 存在但全仓无调用方）。

**影响**：PoB2 的宝石品质是 per-gem 的 qualityStats 表（每 1% 品质给定量 stat，走与等级 stat 同一条 statMap 管线）。pobr 现状：core 层 skill_source.rs 已有 `with_quality`/`quality_mods` + `SourceKind::GemQuality` 归因 API（空架子），但数据层（GrantedEffectQualityStats 未下载、catalog 无字段）、Build 模型（GemSkillRef 无 quality）、XML 导入（quality 属性丢弃）、orchestrator（无人调 with_quality）**四层全断**。已核实样本 ninja build（sorceress-stormweaver-comet）有 15 颗 quality=20 的宝石被静默丢弃——任何带品质宝石的终局 build DPS 系统性偏低，品质给附加投射物/暴击等机制 stat 时偏差更大，是 ninja_parity 进攻侧残余误差的稳定来源之一。

**修复方向**：① pipeline 补下载 GrantedEffectQualityStats 表 → adapter 出 `gem_quality_stats.json`；② `GemSkillRef` 加 `quality` 字段，xml_build 解析 `quality` 属性；③ orchestrator 在构造 gem spec 时调用已就绪的 `with_quality` 接线。

### Gap 2 🔴 support 适用性裁决未在 build 路径执行（partial）

**PoB2 证据**：`Modules/CalcTools.lua:84-110` `canGrantedEffectSupportActiveSkill`（cannotBeSupported/supportGemsOnly/excludeSkillTypes/requireSkillTypes/isTrigger）+ `:60-81` `doesTypeExpressionMatch`（SkillType.AND/OR/NOT 栈式求值）；`Modules/CalcActiveSkill.lua:181-210`（addSkillTypes 两遍 + 被拒 support 不动点 repeat-until 循环，保证插槽顺序无关）。

**pobr 位置**：`crates/pobr-core/src/skill_source.rs:379` `can_support`（仅位集交集，唯一调用方 `ingest_support_gem`:516 在 pobr-build/apps 无调用方）；`crates/pobr-build/src/calc_orchestrator.rs:1611` `support_modifiers`（只检查 `is_support==true` 即全量注入）；`pipeline/config.json` GrantedEffects 列（无 ExcludedActiveSkillTypes；AddedActiveSkillTypes 已下载、121 行非空，但 adapter `RawGrantedEffect` 无对应 serde 字段）。

**影响**：三层断裂全部核实——(1) 数据层：ExcludedActiveSkillTypes 列未下载；AddedActiveSkillTypes 下载了但不解析、不入库；(2) 语义层：PoB2 的 require/exclude 是含 AND/OR/NOT 的后缀表达式 token 流（栈机求值），位集交集无法表达组合条件；(3) 接线层：orchestrator 注入主组 support 数值前完全不做兼容性检查——把法术 support 插在攻击技能组里照样吃满倍率，与 PoB2 直接拒收的行为分叉。FINDINGS 02-06 只记了"more 隔离未覆盖 exclude/add"（LOW，defer），低估了缺口范围：缺的是整个裁决器 + 数据列 + addSkillTypes 不动点（影响 Triggered 等类型追加后的连锁兼容判断）。

**修复方向**：① pipeline 补 ExcludedActiveSkillTypes 列、adapter 解析 AddedActiveSkillTypes；② GrantedEffectDef 的 skill_types 改为表达式 token 数组（不能塌成位集）；③ 在 Rust 框架实现 doesTypeExpressionMatch 栈机 + canGrantedEffectSupportActiveSkill + addSkillTypes 不动点循环（PoB2 侧合计 <80 行）；④ support_modifiers 注入前调裁决器。

### Gap 3 🔴 SkillStatMap 被实现为代码而非数据表（design）

**PoB2 证据**：`Data/SkillStatMap.lua`（954 条显式映射，已精确核实；含 div/mult/base/value 与 PerStat 等 tag）；`Modules/Data.lua:835-847`（metatable 懒加载挂接）；`Data/Skills/act_int.lua:69-72`（per-statSet 覆盖：`arc_damage_+%_final_for_each_remaining_chain` → `mod("Damage","MORE", PerStat ChainRemaining)`，全库 statMap 出现 ~390 处）；`mergeSkillInstanceMods`（merge 公式 `value or statValue×mult×scalar/div+base`）。

**pobr 位置**：`crates/pobr-build/src/skill_stat_map.rs`（751 行后缀模式匹配）；`tools/pobr-data-adapter/src/skills.rs:374-413` `is_mappable_stat`（入库白名单）。

**影响**：PoB2 把 stat→mod 映射当**数据**（954 条全局表 + per-set 覆盖），框架只有一个通用 merge 引擎；pobr 把它写成两层**代码**：adapter 端 is_mappable_stat 决定哪些 stat 入库（不在白名单的 stat 在 JSON 里根本不存在），calc 端 skill_stat_map.rs 再按后缀猜映射。后果：(a) 每补一族 stat 要同时改 adapter + 重新生成数据 + 改映射代码三处（commit c290b79 "support stat-set 过度过滤"修复即此架构的直接产物，is_mappable_stat 的 doc 注释自己记录了"无 stat → 丢整个 set → 进攻 parity 大面积塌陷"的事故）；(b) 条件型/per-skill stat（Arc 按剩余链数、duration、AoE、投射物数量等）永久丢弃，无法增量启用；(c) 后缀启发式有误判面（靠"保守跳过"兜底 = 系统性少算）。与项目"数据进 JSON、框架稳定"的目标直接相悖——**这正是最该 JSON 化的一块**。

**修复方向**：把 SkillStatMap 抽成 `skill_stat_map.json`（schema 见下节），adapter 全量入库 stat（撤销 is_mappable_stat 过滤），框架只保留 ~60 行通用 merge 引擎 + tag 语义枚举。

### Gap 4 🔴 多 statSet / additionalStatSet 未建模（missing）

**PoB2 证据**：`Data/Gems.lua:11-12` additionalStatSet1/2（全库 149 处；IceNova → "IceNovaPlayerOnFrostbolt"/"IceNovaColdInfusedPlayer"）；`Data/Skills/act_int.lua`（Fireball statSets "Projectile"/"Explosion" 两套，各自独立 baseFlags/constantStats/levels；label 全文件 23 处）；`Modules/CalcActiveSkill.lua`（`activeEffect.statSet.index` 选择 + 未选 set 仅 merge global stat）。

**pobr 位置**：`crates/pobr-data/src/catalog.rs:190` `GrantedEffectDef.stat_set`（单 FK）；`tools/pobr-data-adapter/src/skills.rs:466-475` `adapt_stat_sets`（只遍历 GrantedEffects 链接的主 set）；Build 模型无 part/statSet 选择字段。

**影响**：PoE1 的多 part 技能在 PoE2 完全由多 statSets 取代（Fireball 直击/爆炸、Ice Nova 自身/Frostbolt 引爆是不同 set，baseFlags 和数值矩阵都不同）。pobr 只入库 GrantedEffects.StatSet 指向的那一套：已核实 IceNovaPlayerOnFrostbolt / IceNovaColdInfusedPlayer 两行**确实存在于已下载的 GrantedEffectStatSets 表中**，但 adapt_stat_sets 的链接循环只走主外键，additional set 被丢弃；PoB2 XML 里的 statSet/skillPart 选择属性导入时也不读。所有多形态技能只能算默认形态，且 area 等 baseFlags 取错 set 时伤害 flag 跟着错。

**修复方向**：① SkillStatSetDef 改造为"一个 effect 多 set（带 label/base_flags）"；② SkillGemDef 补 `additional_stat_set_ids[]`；③ Build 模型 + xml_build 增加 statSet 选择字段；④ merge 引擎实现"选中 set 全量、未选 set 仅 global"的语义。

### Gap 5 🟡 meta gem / additionalGrantedEffects 不展开，gem→effect 无关联键（missing）

**PoB2 证据**：`Data/Gems.lua` additionalGrantedEffectId1（全库 162 处）+ `gemType="Meta"`；`Modules/CalcSetup.lua:1716`（gemData.additionalGrantedEffects 逐个展开为额外技能，行号已核实）。

**pobr 位置**：`crates/pobr-data/src/catalog.rs:150-159` `SkillGemDef`（无 granted_effect 关联字段；TODO 注释自述 "GemEffects FK 指向的 GemEffects 表当前 pipeline 未导出"）；`calc_orchestrator.rs:152-157` 注释承认仅靠 resolve_main_skill 跳过 meta 壳。

**影响**：pobr 的宝石身份表（skill_gems.json）和技能定义表（granted_effects.json）之间没有外键。已核实：SkillGems.GemEffects 列**已下载在磁盘表中**但 adapter `RawSkillGem` 不解析；且该列是指向独立 GemEffects 中间表的 FK 索引，目标表本身也未加入 pipeline——完整打通需要补下载 GemEffects 表 + adapter 解析两步。当前靠 PoB2 XML 每个 `<Gem>` 自带 skillId 才能工作；一旦要脱离 PoB2 XML 自建 build（项目最终目标），从"玩家插了什么宝石"到"授予哪些效果"这一步无数据可查。meta gem 附带的第二 granted effect 也不会出现在技能列表中，依赖该效果数值的组直接漏算。

**修复方向**：① pipeline 补下载 GemEffects 中间表；② adapter 解析 SkillGems.GemEffects → SkillGemDef 补 `granted_effect_id` + `additional_granted_effect_ids[]`；③ orchestrator 按 additional effects 展开多技能（对应 CalcSetup.lua:1716 语义）。

### Gap 6 🟡 spirit 预留与 reservation 全族数据列缺失（missing）

**PoB2 证据**：`Data/Skills/act_int.lua` spiritReservationFlat（全文件 1319 处，已精确核实）；`Modules/CalcActiveSkill.lua:689-700`（manaMultiplier→SupportManaMultiplier MORE、reservationMultiplier→ReservationMultiplier MORE、manaReservationPercent、spiritReservationFlat→ExtraSpirit BASE，已逐行核实）。

**pobr 位置**：`crates/pobr-data/src/catalog.rs` `SkillLevelDef`（字段仅 level/cooldown_ms/attack_time_ms/cost_amounts/attack_speed_multiplier/base_multiplier/crit_chance，无任何 reservation 字段）；`pipeline/config.json` GrantedEffectsPerLevel 仅 3 数据列；`data/4.5.0.3.4/cost_types.json`（18 个条目无 Spirit，已核实）。

**影响**：PoE2 的常驻 buff/光环/被动 minion 体系全靠 Spirit 预留，是 build 可行性的硬约束（Spirit 不够 = 光环开不出来）。pobr 有 Spirit 总量聚合但每个 buff gem 预留多少完全无数据：aura_buff_modifiers 现在无条件注入所有已启用光环的 buff——Spirit 超载的非法配置也照算。reservationMultiplier（support 改预留）同缺。

**修复方向**：补下载 GrantedEffectsPerLevel 对应列（或从 vendor Lua 抽取）→ SkillLevelDef 补 `spirit_reservation_flat`/`reservation_multiplier`/`mana_reservation_percent` → orchestrator 做预留汇总与可行性校验。

### Gap 7 🟡 SupportManaMultiplier 数据与消费双缺（partial）

**PoB2 证据**：`Modules/CalcActiveSkill.lua:689-691`（`NewMod("SupportManaMultiplier","MORE", level.manaMultiplier)`，行号已核实）；`Data/Skills/sup_int.lua`（manaMultiplier 64 处）。

**pobr 位置**：`crates/pobr-core/src/skill_source.rs:248` `with_mana_multiplier`（注入 API 存在但 grep 全仓无非测试调用方，已核实）；`crates/pobr-core/src/calc/skill_mechanics.rs:539`（cost 公式 doc 注释明确"不含 SupportManaMultiplier defer"）；granted_effect_levels.json 无 mana_multiplier 字段。

**影响**：core 层 API 是空架子：orchestrator 从不设置 mana_multiplier，cost 计算路径文档也明确跳过该乘区。PoE2 中 support 的 cost 倍率普遍存在（正负皆有），主技能 mana cost 因此系统性偏差，连带影响 mana 维持/EB 类机制评估。

**修复方向**：补数据列（SkillLevelDef.mana_multiplier）+ orchestrator 接线 with_mana_multiplier + skill_mechanics cost 公式乘入——三步都已留 TODO 但无一落地。

### Gap 8 🟡 vendor 抽取列不可再生：重跑 adapter 必静默丢数据（design）

**PoB2 证据**：`Modules/CalcActiveSkill.lua`（`modList:AddList(set.baseMods)`）；`Export/Scripts/skills.lua` 手工 directive 注入 baseMods/statMap；FireballPlayer 等 levels 内 critChance 列。

**pobr 位置**：`tools/pobr-data-adapter/src/skills.rs:541-545`（skill_attack_speed_more 恒 None，注释承认"vendor Lua 合并，适配阶段留空"）；`skills.rs:95-102` `RawGrantedEffectPerLevel`（serde 已声明 CritChance/AttackSpeedMultiplier/BaseMultiplier 字段）vs `pipeline/tables/English/GrantedEffectsPerLevel.json`（34153 行仅 6 个 key，CritChance 非空计数 = 0，已核实）；git dc03599/c290b79。

**影响**：已核实 data/4.5.0.3.4/granted_effect_levels.json 里有 3912 个 crit_chance、3578 个 attack_speed_multiplier 值，而 pipeline 磁盘表完全没有这些列——这些值来自一次性 headless PoB 抽取（commits dc03599/c290b79），抽取脚本不在仓库内。adapter 的 RawGrantedEffectPerLevel 其实已声明对应 serde 字段（列若出现即可解析），但 pipeline/config.json 不下载这些列，下个游戏版本重跑 download+adapter 会**静默把这些列全置 None**，技能基础暴击/Flicker 类攻速倍率塌回缺省。statSet baseMods（PoB2 手工补的常量 mod，如 Flicker Speed MORE）则完全没有入库通道。

**修复方向**：把"vendor Lua 抽取"固化为可重复的管线步骤（如扩展 sync-pob-catalog），或确认 GGG dat 有对应列后加进 pipeline/config.json（adapter 已能解析，缺的只是让列出现在表里）；否则数据更新流程是断的。

### Gap 9 🟢 naturalMaxLevel 缺失 + statInterpolation 2/3 未实现（low，简述）

整数宝石等级查表 pobr 已对（BaseResolvedValues 是预解析值）。缺：(a) naturalMaxLevel 不入库（grep 全 crates 0 命中），超 21+ 的非法等级 clamp 行为与 PoB2（`validateGemLevel`）分叉；(b) interpolation 2/3 只在非整数 actorLevel 场景（minion 随怪等、物品授予技能）生效——当前 pobr 不算 minion 所以影响小，但 minion 域开工时必须补 incrementalEffectiveness/damageIncrementalEffectiveness（已下载未入库）。

### Gap 10 🟢 gemFamily / weaponRequirements 未入库（low，简述）

Aftershock I/II/III 同族同插不叠（`gemFamily`/`isLineage`）、攻击技能武器类型门控（拿法杖的 Leap Slam 应禁用，`getWeaponFlags`）这类合法性约束 pobr 完全无数据支撑（pipeline 全表清单无 SupportGems，已核实）。当前 parity harness 用的是合法 build 故不暴露，但作为 build 编辑器必须能拒绝/提示非法组合；纯数据问题，逻辑很薄。

## 数据 vs 逻辑切分建议

### 本质是数据、PoB2 以 Lua 承载的（共 ~5.6MB，占 src/Data 的大头）

1. **Gems.lua（501K）**——宝石身份/索引表。纯数据，pobr 已有 skill_gems.json 对应但字段残缺（无 granted_effect 关联/additional 系列/naturalMaxLevel/weaponRequirements）。
2. **Data/Skills/\*.lua（~5MB）**——granted effect 定义 + 40 级数值矩阵 + statSets。纯数据（自动生成），pobr 已拆成 granted_effects / granted_effect_levels / granted_effect_stat_sets 三个 JSON，**方向正确**。
3. **SkillStatMap.lua（105K，954 条）+ per-statSet statMap 覆盖（~390 处）**——**本次审查的核心判定：它是数据，不是逻辑**。每条就是 `stat_id → {mod_name, mod_type, flags, keyword_flags, tags(PerStat/Condition…), div, mult, base, value}` 的声明式记录；PoB2 框架里消费它的只有一个 ~60 行的通用 merge 函数（`value or statValue×mult×scalar/div+base` 公式）。pobr 当前把它实现成 751 行 Rust 后缀启发式 + adapter 端 is_mappable_stat 白名单，是**把数据错放进了框架**——每个版本新 stat 都要改两处代码并重生成数据，且条件型映射永久丢失。
4. **手工 directive 混入的 baseMods / addSkillTypes 补丁、critChance/attackSpeedMultiplier 等 PoB 侧补全列**——半手工数据，需要独立的"PoB2 vendor 抽取"管线步骤承载（现状是一次性脚本 merge，已核实 pipeline 磁盘表无这些列、不可再生，见 Gap 8）。

### 本质是逻辑、应留在 Rust 框架的（PoB2 侧合计 <500 行）

| 函数 | 规模 | 职责 |
|------|------|------|
| `doesTypeExpressionMatch` | ~20 行 | 后缀表达式求值器（栈机） |
| `canGrantedEffectSupportActiveSkill` | ~25 行 | support 兼容裁决 |
| `createActiveSkill` 的 addSkillTypes 不动点循环 | ~30 行 | support 顺序无关性 |
| `buildSkillInstanceStats` | ~60 行 | 品质 stat 叠加 + 三种 statInterpolation 插值 |
| `mergeSkillInstanceMods` | ~60 行 | statMap 查表 + 公式注入 + 未选 set 的 global-only merge |
| `validateGemLevel` | 数行 | 等级 clamp |

pobr 对这些逻辑已有弱化版（can_support / resolve_skill_level / mapped_stat_modifiers），需**按 PoB2 语义补强而非重设计**；且 core 层已有的归因 API（with_quality/with_mana_multiplier/ingest_support_gem）是接线就绪的，缺口主要在数据列与 orchestrator 接线。

### PoB2 现在怎么"混在一起"（JSON 化时要拍平的三处）

- 自动生成的 Data/Skills 里嵌着手工 statMap 覆盖和 baseMods（导出脚本 directive 机制）；
- SkillStatMap 的 mod 构造器闭包（mod/flag/skill）让数据文件携带了构造逻辑；
- metatable 懒加载（Data.lua:835-847）把全局映射动态挂到每个 statSet。

这三处"数据带逻辑"在 JSON 化时都要拍平成**纯声明式记录 + tag 枚举**。

### pobr 当前 JSON schema（catalog.rs）还缺的表/字段清单

**新表**

| 表 | schema | 说明 |
|----|--------|------|
| `skill_stat_map.json` | `stat_id → [{mod_name, mod_type, flags, kw_flags, tags[], div, mult, base, value?}]` | 取代 skill_stat_map.rs 启发式 + is_mappable_stat 白名单；tag 语义枚举留框架 |
| `gem_quality_stats.json` | `effect_id → [{stat, per_quality_rate}]` | 源表 GrantedEffectQualityStats，需加进 pipeline/config.json |
| per-statSet statMap 覆盖边车 | 挂在 stat_sets 或独立文件 | vendor Lua 抽取 |

**SkillGemDef 补字段**：`granted_effect_id`、`additional_granted_effect_ids[]`、`additional_stat_set_ids[]`、`natural_max_level`、`tags[]`、`weapon_requirements[]`、`gem_family[]`（源：SkillGems.GemEffects 列已下载，但其 FK 目标 GemEffects 中间表 + SupportGems 表均需补下载）。

**GrantedEffectDef 补字段**：`require_skill_types` 改为**表达式 token 数组**（保留 AND/OR/NOT，不能塌成位集）、`exclude_skill_types[]`（需补下载 ExcludedActiveSkillTypes 列）、`add_skill_types[]`（已下载 121 行未解析）、`cannot_be_supported`、`support_gems_only`。

**SkillLevelDef 补字段**：`mana_multiplier`、`reservation_multiplier`、`mana_reservation_percent`、`spirit_reservation_flat`、`stored_uses`。

**SkillStatSetDef 改造**：一个 effect 多 set（带 label/base_flags）、去掉 is_mappable_stat 过滤改为全量入库（落地与否交给 skill_stat_map.json）、补 `incremental_effectiveness`/`damage_incremental_effectiveness`（插值用，已下载未入库）、`base_mods[]`。

**管线**：把 crit_chance/attack_speed_multiplier 等 vendor 抽取固化为可重复步骤（adapter 的 RawGrantedEffectPerLevel 已能解析这些列，缺的是让列出现在 pipeline 表里——扩展 sync-pob-catalog 或补 config.json 列），否则版本更新时 3912+3578 个已 merge 值会静默丢失。

---

## 附录：核查说明

核查范围：全部 4 条 high + 全部 4 条 medium + 抽查 2 条 low，共 10 条全部打开实际代码验证（vendor PoB2 Lua、pobr crates/tools/pipeline、磁盘数据表、样本 build XML、git log）。

**high 4 条全部查实，保留 severity**

1. quality 链路（Gap 1）：PoB2 三处引用逐行核实（skills.lua:304-313 GrantedEffectQualityStats/StatValues/1000、CalcTools buildSkillInstanceStats math.modf(rate×quality)、act_int.lua:7234-7236 Fireball qualityStats）；pobr 侧 pipeline 无该表、GemSkillRef 无字段、xml_build.rs 确实只取 skillId/level。**修正一处**：原报告称"计算四层皆空"不完全准——pobr-core skill_source.rs:277 有 with_quality/quality_mods + SourceKind::GemQuality 归因 API（grep 全仓无调用方，空架子），已在详述中补充该 nuance（不影响 severity：数据/Build/XML/接线四层仍全断）。影响断言经验证成立：样本 build sorceress-stormweaver-comet decoded.xml 实测 15 个 quality="20" 宝石被丢弃。
2. support 裁决（Gap 2）：canGrantedEffectSupportActiveSkill（实际 :84 起，原报告 :85 偏 1 行，已修正为 :84-110）、doesTypeExpressionMatch 栈机、CalcActiveSkill :181-210 不动点 repeat-until 循环全部逐行核实；can_support 确为纯交集（skill_source.rs:379）且其唯一调用方 ingest_support_gem 在 build 路径无人调用（grep 证实）；support_modifiers（实际定义 :1611，原报告 :1612）只查 is_support；pipeline GrantedEffects 实际 8 列无 Excluded、AddedActiveSkillTypes 非空行数实测恰为 121。全部成立。
3. SkillStatMap 设计问题（Gap 3）：954 条精确核实（grep `\["` = 954）；Data.lua metatable 实际在 :835-847（原报告 834-875 范围偏宽，已修正）；skill_stat_map.rs 实测 751 行；is_mappable_stat 在 skills.rs:374 起；Arc per-set statMap 在 act_int.lua:69-72 逐字核实；c290b79 commit message"修复 support stat-set 过度过滤"+ is_mappable_stat doc 注释自述事故，佐证"过滤丢 set"断言。per-statSet statMap 出现次数实测 ~390（原称 ~360，已修正）。成立。
4. 多 statSet（Gap 4）：additionalStatSet 149 处、additionalGrantedEffectId 162 处均精确复现；IceNova 两个 additional set 的行实测**确实存在于已下载的 GrantedEffectStatSets 表中**（被 adapt_stat_sets 主链接循环丢弃）——报告关键断言直接验证成立；GrantedEffectDef 单 stat_set FK、Build 模型无选择字段均核实。成立。

**medium 4 条全部查实**

5. meta gem（Gap 5）：CalcSetup.lua:1716 行号精确命中。**修正一处**：SkillGems.GemEffects 列确实已下载在磁盘表中且 adapter 不解析，但 catalog.rs TODO 注释揭示更深一层——GemEffects 列是指向独立 GemEffects 中间表的 FK，该目标表本身也未加入 pipeline，完整打通需两步（补下载中间表 + adapter 解析），已写入详述。
6. spirit 预留（Gap 6）：act_int.lua spiritReservationFlat 实测恰 1319 处；CalcActiveSkill :689-700 四个 reservation 字段消费逐行核实；SkillLevelDef 字段清单核实无 reservation；cost_types.json 18 条无 Spirit。成立。
7. SupportManaMultiplier（Gap 7）：PoB2 :689-691 精确命中；with_mana_multiplier 全仓 grep（排除测试）确无调用方；skill_mechanics.rs:539 doc 注释明确 defer。成立。
8. vendor 抽取不可再生（Gap 8）：**查实并强化**——pipeline/tables/English/GrantedEffectsPerLevel.json 实测 34153 行仅 6 个 key、CritChance 非空 = 0，而 data JSON 实测 3912 个 crit_chance + 3578 个 attack_speed_multiplier（与报告数字一致），证实重跑必丢；**修正一处**：adapter 的 RawGrantedEffectPerLevel 其实已声明这三列的 serde 字段（原报告未提），所以断点在"列不出现在 pipeline 表"而非 adapter 代码，修复路径比原描述多一个选项（补 config.json 列），已写入详述；skill_attack_speed_more 恒 None 注释核实。

**low 2 条抽查**

9. Gap 9：natural_max_level 全 crates grep 0 命中、buildSkillInstanceStats interpolation 2/3 代码核实存在，成立。
10. Gap 10：pipeline 全表清单核实无 SupportGems 表，成立。

**结论**：无删除、无降级。10 条全部成立；修正 4 处细节（Gap 1 补 core 层空架子 API、Gap 2/3 行号与计数微调、Gap 5 补 GemEffects 中间表层次、Gap 8 补 adapter 已可解析的事实与更简修复路径），并把已验证的精确计数（121/149/162/954/1319/3912/3578/15×q20）写回报告增强可信度。原报告质量高：所有行号引用误差 ≤1 行，所有计数与实测一致。
