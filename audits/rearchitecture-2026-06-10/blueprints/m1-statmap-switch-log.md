# M1 statmap 切换审查记录（草稿，T2b）

> 蓝图 T2.3/T2.4 的切换审查档案：Legacy（`pobr-build/src/skill_stat_map.rs` 751 行
> 后缀启发式）→ Data（`overlay/skill_stat_map.json` + `pobr-core::rules::stat_map_engine`）。
> 本文件登记双跑 diff 终版结论 + oracle 抽样结果 + T2.4 四前置条件核对单。
> vendor 基准：PathOfBuilding-PoE2 @ `2df5a7433dd2`（overlay `_meta.vendor_commit`）。

## 0. 状态

- [ ] T2.4 前置条件 1：L1 `legacy_only` 集合为空，或逐条附 PoB2 依据证明 legacy 误映射（§2）
- [ ] T2.4 前置条件 2：L2 18 build 全部 review，每处变化 =0 或"修对"附出处（§3）
- [ ] T2.4 前置条件 3：oracle 抽样 ≥50 条全部一致（§4）
- [ ] T2.4 前置条件 4：默认 mode 切 Data 的行为 commit + baseline 更新 commit（**不在 T2b 范围**，T2.4 执行）

## 1. 双跑口径（终版）

- **Legacy 探针** = `legacy_stat_filter::is_mappable_stat`（T5.3 消费侧平移的原 adapter
  白名单）∧ `map_skill_stats`，与 `calc_orchestrator::legacy_mapped_stat_modifiers`
  同一谓词链。
- **Data 探针** = `stat_map_engine::map_stat(catalog, effect_id, set_key=None, stat, value)`；
  `set_key=None` → 默认 set `"1"` 的 per-statSet 覆盖（PoB2 缺省 statSetIndex=1：
  `SkillsTab.lua:354`、`CalcActiveSkill.lua:166-171`），miss 落回全局表
  （`Data.lua` statMap metatable `__index` 链）。
- **L1 行集** = `granted_effect_stat_sets.json`（T5.3 全量入库后）distinct stat 的
  global 行 + 存在 per-set 覆盖的 `(effect, stat)` 上下文行。
- 报告产物：`target/statmap-diff/L1.jsonl` / `L1-summary.md` / `L2-<build>.md` /
  `oracle-report.md`（重跑命令见 `crates/pobr-build/tests/statmap_dual_run.rs` 模块头）。

## 2. L1 终版结论（前置条件 1）

（待终版重跑后填写——T5.3 全量入库 merge 后执行。）

### 2.1 本轮（T2b）引擎补全（行为改动，逐条 PoB2 依据）

| # | 补全 | PoB2 依据 | 说明 |
|---|---|---|---|
| E1 | `set_key=None` → 默认 per-set `"1"` 覆盖 | `SkillsTab.lua:354`（`statSetIndex or 1`）、`CalcActiveSkill.lua:166-171` | 解锁 per-statSet 覆盖族（support `*_final`、demon form 等 29+ 条 legacy_only 的根因） |
| E2 | ModFlag 直译（Attack/Spell/Melee/Projectile/Area → `ModFlags`） | `Data/Global.lua:213-249` 位值；匹配语义 `ModList.lua` 子集判定 = PoBR `Modifier::matches` `is_subset_of` | 例：`non_skill_base_all_damage_%_to_gain_as_*_with_attacks`（`SkillStatMap.lua:1110-1117` `mod("DamageGainAs<T>","BASE",nil,ModFlag.Attack)`）→ gain-as 名 + ATTACK flag。legacy 丢 flag 全局注入（法术也吃，过算）；Data 侧附 flag 为修对 |
| E3 | KeywordFlag 直译（已移植位：Aura/Curse/Hit/Ailment/Poison/Bleed/Ignite/各 Dot） | `Data/Global.lua:251-292` 位值（PoBR `KeywordFlags` 注释逐位对齐） | ANY 匹配语义两边一致（`MatchKeywordFlags` ↔ `matches_context`） |
| E4 | ActorCondition(actor=enemy) → `Enemy<Var>` Condition | `SkillStatMap.lua:1119`（`…gain_as_fire_with_attacks_vs_burning_enemies`）+ PoBR 既有敌方条件命名约定 `mod_parser.rs:950-964`（EnemyBurning） | legacy 无条件注入（过算）；Data 侧条件门控为修对 |
| E5 | 直通名补 `WarcrySpeed` / `TotemPlacementSpeed`（惰性作用域名，PoBR 无消费方） | `SkillStatMap.lua:554-557`（`skill_speed_+%` 同条目三 mod）、`:2400-2401`（`summon_totem_cast_speed_+%` → `TotemPlacementSpeed` INC） | legacy 把 `summon_totem_cast_speed_+%` 误并入 `CastSpeed`（误映射：图腾放置速度 ≠ 施法速度）；`skill_speed_+%` 整条目可映射后不再 legacy_only |
| E6 | 惰性作用域名上的冗余 KeywordFlag（Warcry/Totem）安全丢弃 | `SkillStatMap.lua:556`（WarcrySpeed 名即 Warcry 作用域） | 仅限 `SCOPE_NAMED_INERT` 白名单，无消费方不会错算 |
| E7 | orchestrator Data/Compare 通道带 effect 上下文（三取数点传 effect id） | 同 E1 | Legacy 通道不消费（后缀启发式与 effect 无关），默认 mode 仍 Legacy，ninja baseline 不动 |

### 2.2 legacy_only 残留逐条裁决

预期残留类别（终版 L1 跑完后逐条核对清单）：

**a) skillData 通道族（Data 侧 Unsupported(skill_data_key) = 正确保守）**

- `off_hand_weapon_{min,max}imum_{physical,fire}_damage`：vendor 走
  `skill("setOffHand…")` skillData 通道（`SkillStatMap.lua:2123-2147`），不是玩家
  modifier。legacy 把它注入 `<Type>DamageMin/Max` BASE 属误映射；physical 两条已被
  `calc_orchestrator::is_off_hand_weapon_base_stat` 在 skill-base 取数点剔除（由
  `non_weapon_attack_contribution` 作为武器 source 消费），fire 两条 legacy 在命中
  时会重复/错位注入。

**b) minion 域族（Data 侧 Unsupported(mod_type LIST) = 正确保守，M5a 接手）**

- `minion_base_physical_damage_%_to_convert_to_lightning` /
  `minion_skill_physical_damage_%_to_convert_to_fire`：vendor 包
  `MinionModifier LIST`（`SkillStatMap.lua:2439,2625`），作用于 minion actor。
  legacy 直接注入**玩家**转换词条属误映射。

**c) vendor 完全无 statMap 条目族（已逐条 grep 核实 @ `2df5a743`）**

下列 stat 在 `Data/SkillStatMap.lua` 与 `Data/Skills/*.lua` 中**只出现在 stats
数组/等级表**（`{ "<stat>", value }` 行），无任何 `["<stat>"] = { mod(...) }`
statMap 条目 ⇒ PoB2 `mergeSkillInstanceMods` 对它们不注入任何 mod；legacy 的
后缀猜测注入 = 超出 PoB2 的映射（误映射），Data 侧 Unknown = 对齐 PoB2：

`active_skill_base_physical_damage_%_to_gain_as_fire`、
`archon_of_chayula_physical_and_chaos_damage_+%_final`（other.lua:1507,1565 仅 stats 行）、
`bleeding_monsters_attack_speed_+%`、
`mantra_of_destruction_grant_all_damage_%_to_gain_as_chaos_with_attacks`（act_int.lua:15127 仅 stats 行）、
`non_skill_base_all_damage_%_to_gain_as_{cold,lightning}_with_spells_from_buff`、
`non_skill_base_{cold,physical}_damage_%_to_convert_to_chaos`、
`non_skill_base_physical_damage_%_to_convert_to_cold`、
`non_skill_base_physical_damage_%_to_gain_as_fire`（spectre.lua:4538 仅 stats 行，spectre 域）、
`shearing_bolts_non_skill_base_physical_damage_%_to_convert_to_cold`（act_dex.lua:3889）、
`skill_consume_{frenzy,power}_charge_to_gain_*_final`、
`trigger_meta_gem_damage_+%_final`（act_int.lua:2185 / act_str.lua:2496 仅 stats 行）、
`walking_calamity_non_skill_base_all_damage_%_to_gain_as_fire`（act_str.lua:20273,20331）、
`witch_hunter_mark_attack_and_cast_speed_+%`、
`wyvern_devour_*`（other.lua:13616,13673，monster 技能域）、
`support_{dual_cascade_aftershocks,fusillade,multiple,spell_totem,titanic_arrows,…}_*_final`
（对应技能/support 的 statMap 未给条目，仅 stats 行）。

> 解读：PoB2 自身也未实现这些 stat 的注入（或属 buff/展示域）。legacy 注入它们
> 会让 PoBR 偏离 PoB2 参照（方向上可能"更对游戏"，但 parity 基准是 PoB2——蓝图
> P0 判据）。切换后这些注入消失属**修对**（对齐 PoB2），若 ninja 个别 build 因此
> 掉容差，按 R5 机制记录补偿清单不回滚。

## 3. L2 终版结论（前置条件 2）

（待终版重跑后按 build 填写：每 build 列差异字段 + 归因到 §2.1 的 E# 或 §2.2 条目。）

## 4. oracle 抽样（前置条件 3）

（待跑：`POBR_POB2_SRC=<vendor>/src cargo test -p pobr-build --test statmap_dual_run
-- --ignored --nocapture oracle_statmap_sampling`，结果表见
`target/statmap-diff/oracle-report.md`，此处登记汇总。）

## 5. 遗留 / 移交

- per-set 覆盖目前只接默认 set `"1"`；XML `statSetIndex` 显式选择的 set_key 接线随
  T5.4/T5.5 多 statSet 模型（蓝图 §3 串行序）。
- `GlobalEffect` tag 条目（demon form / pain offering / 光环类 buff 族）引擎整条
  Unsupported——属 buff 系统域（W-J global-only merge + M3），不在 statmap 切换
  门槛内，但 L2 中相关 build 若有差异需引用本条说明。
- scalar（`checkForScalarMultiplier`，`CalcActiveSkill.lua:53-66`）固定 1.0，含
  scalar 条目 Unsupported（蓝图 T2.2 既定边界）。
