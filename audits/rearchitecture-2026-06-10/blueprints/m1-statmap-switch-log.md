# M1 statmap 切换审查记录（T2.4 终稿归档）

> 蓝图 T2.3/T2.4 的切换审查档案：Legacy（`pobr-build/src/skill_stat_map.rs` 751 行
> 后缀启发式）→ Data（`overlay/skill_stat_map.json` + `pobr-core::rules::stat_map_engine`）。
> 本文件登记双跑 diff 终版结论 + oracle 抽样结果 + T2.4 四前置条件核对单 +
> 切换/删除 commit 序列（§6）。
> vendor 基准：PathOfBuilding-PoE2 @ `2df5a7433dd2`（overlay `_meta.vendor_commit`）。

## 0. T2.4 前置条件状态（全绿，切换已执行）

- [x] **前置条件 1**：L1 `legacy_only` 39 行，**逐条已附 PoB2 依据证明 legacy 误映射 /
  超映射**（§2.2 三类裁决，全部核实）——按验收口径（为空 **或** 逐条有据）达成。
- [x] **前置条件 2**：L2 18 build 全部 review：10 个 build 逐字段一致；8 个 build
  的每处变化均为"修对"且附 SkillStatMap / Data/Skills 条目出处（§3）。
- [x] **前置条件 3**：oracle 抽样 **71/71 PASS**（≥50 达标；§4）。
- [x] **前置条件 4**：默认 mode 切 `Data` 的行为 commit（`bf71975`）+ ninja baseline
  独立更新 commit（`02bbf58`，显式审查）已合并——commit 序列与实测结果见 §6。

## 1. 双跑口径（终版）

- **Legacy 探针** = `legacy_stat_filter::is_mappable_stat`（T5.3 消费侧平移的原 adapter
  白名单）∧ `map_skill_stats`，与 `calc_orchestrator::legacy_mapped_stat_modifiers`
  同一谓词链。
- **Data 探针** = `stat_map_engine::map_stat(catalog, effect_id, set_key=None, stat, value)`；
  `set_key=None` → 默认 set `"1"` 的 per-statSet 覆盖（PoB2 缺省 statSetIndex=1：
  `SkillsTab.lua:354`、`CalcActiveSkill.lua:166-171`；18 个 ninja build 的
  decoded.xml `statSetIndex` 全为 `"nil"` = 默认），miss 落回全局表（`Data.lua`
  statMap metatable `__index` 链）。
- **L1 行集**（T5.3 全量入库后穷举）：global 行 = 存在 ≥1 个无 per-set 覆盖 carrier
  effect 的 distinct stat（1961 行，附 carrier 注记）；per-set 上下文行 = 存在默认
  set "1" 覆盖的 (effect, stat) 对（476 行）；总 2437 行。
- orchestrator 三取数点（skill_base / quality / support）已传 effect id（Data/Compare
  通道消费；Legacy 不消费）。**归属说明**：该接线（`mapped_stat_modifiers` 增
  `effect_id` 参数）在共享 worktree 并行下被并入 T5.2 commit `aad8301`，实现属
  T2b（蓝图 §3.2 `mapped_stat_modifiers`→T2 归属）。
- 报告产物：`target/statmap-diff/L1.jsonl` / `L1-summary.md` / `L2-<build>.md` /
  `oracle-report.md`；重跑命令见 `crates/pobr-build/tests/statmap_dual_run.rs` 模块头
  （`l2_runtime_compare_records` 为 L2 差异的取数点级定位工具）。

## 2. L1 终版结论（前置条件 1）

分类计数（2437 行）：`both_equal 98` / `data_only 106` / `both_diff 4` /
`legacy_only 39` / `both_absent 2190`。data 侧 Unsupported 分布：
`unknown_mod_name 386`（PoBR 未消费名，如 AilmentMagnitude/AreaOfEffect 族——切换
门槛外，引擎按"宁可跳过"上报）、`tag 61`、`mod_type 60`（MinionModifier LIST 族，
M5a）、`skill_data_key 38`、`flags 32`、`keyword_flags 4`、`missing_mod_type 3`。

### 2.1 本轮（T2b）引擎补全（行为改动，逐条 PoB2 依据）

| # | 补全 | PoB2 依据 | 说明 |
|---|---|---|---|
| E1 | `set_key=None` → 默认 per-set `"1"` 覆盖 | `SkillsTab.lua:354`（`statSetIndex or 1`）、`CalcActiveSkill.lua:166-171` | 解锁 per-statSet 覆盖族（support `*_final` 等 29 条 legacy_only 根因 → both_equal/data_only） |
| E2 | ModFlag 直译（Attack/Spell/Melee/Projectile/Area → `ModFlags` 附着） | `Data/Global.lua:213-249` 位值；匹配语义 `ModList.lua` 子集判定 = PoBR `Modifier::matches` `is_subset_of` | 例：gain-as with attacks（`SkillStatMap.lua:1110-1117` `mod("DamageGainAs<T>","BASE",nil,ModFlag.Attack)`）。legacy 丢 flag 全局注入（法术也吃 = 过算）；Data 附 flag = 修对。子集外 token（Hit/Dot/Thorns/Weapon…）仍整条 Unsupported |
| E3 | KeywordFlag 直译（已移植位 Aura/Curse/Hit/Ailment/Poison/Bleed/Ignite/各 Dot） | `Data/Global.lua:251-292` 位值；ANY 语义 `MatchKeywordFlags` ↔ `matches_context` | — |
| E4 | **KeywordFlag.Attack/Spell → 等价 `ModFlags::ATTACK/SPELL` 门控** | PoB2 cfg.keywordFlags 与 PoBR cfg.flags 同由技能类型派生（`calc_orchestrator.rs:1125-1129`），ANY-keyword 与 flag-子集对单 keyword 等价；例 Elemental Armament `sup_str.lua:2825-2827` `mod("ElementalDamage","MORE",nil,0,KeywordFlag.Attack)` | 修复 4 个 ninja build（deadeye/twister/gemling/pathfinder）的 +25% MORE 缺失 |
| E5 | ActorCondition(actor=enemy) → `Enemy<Var>` Condition | `SkillStatMap.lua:1119` + PoBR 敌方条件命名约定 `mod_parser.rs:950-964`（EnemyBurning） | legacy 无条件注入（过算）；Data 条件门控 = 修对 |
| E6 | 直通名补 `WarcrySpeed` / `TotemPlacementSpeed`（惰性作用域名） | `SkillStatMap.lua:554-557`（skill_speed 三 mod 条目）、`:2400-2401` | legacy 把 `summon_totem_cast_speed_+%` 误并入 `CastSpeed`（图腾放置速度 ≠ 施法速度，误映射）；惰性名上的冗余 Warcry/Totem keyword 安全丢弃（无消费方） |
| E7 | orchestrator Data/Compare 通道 effect 上下文接线（三取数点） | 同 E1 | Legacy 通道不消费；默认 mode 仍 Legacy，ninja baseline 不动 |

### 2.2 legacy_only 39 行逐类裁决（全部有据）

**a) skillData 通道族（4 行；Data 侧 Unsupported(skill_data_key) = 正确保守）**

`off_hand_weapon_{min,max}imum_{physical,fire}_damage`：vendor 走
`skill("setOffHand…")` skillData 通道（`SkillStatMap.lua:2123-2147`），不是玩家
modifier。legacy 注入 `<Type>DamageMin/Max` BASE 属误映射；physical 两条
（carriers：FortifyingCryShockwave/Nightfall*/ResonatingShield 等 7 effect）已被
`calc_orchestrator::is_off_hand_weapon_base_stat` 在 skill-base 取数点剔除（由
`non_weapon_attack_contribution` 作为武器 source 消费——L1 legacy 探针不含该调用点
过滤，实跑两边一致）；fire 两条（carriers：MagmaSpray*）legacy 命中时会错位注入主手
伤害。off-hand skillData 通道接入随后续（M4 偏移记录）。

**b) minion 域族（2 行；Data 侧 Unsupported(mod_type LIST) = 正确保守，M5a 接手）**

`minion_base_physical_damage_%_to_convert_to_lightning`（carriers：
TriggeredLivingLightning*）/ `minion_skill_physical_damage_%_to_convert_to_fire`
（carriers：RagingSpirits*）：vendor 包 `MinionModifier LIST`
（`SkillStatMap.lua:2439,2625`），作用于 minion actor；legacy 注入**玩家**转换词条
属误映射。

**c) vendor 无 statMap 条目族（26 行；Data 侧 Unknown = 对齐 PoB2）**

已逐条 grep 核实（`Data/SkillStatMap.lua` + `Data/Skills/*.lua` @ `2df5a743`）：这些
stat 仅出现在 stats 数组/等级表（`{ "<stat>", value }` 行），无任何
`["<stat>"] = { mod(...) }` statMap 条目 ⇒ PoB2 `mergeSkillInstanceMods` 不注入任何
mod；legacy 的后缀猜测注入 = 超出 PoB2 的映射（误映射）。按 carrier 分两亚类：

- **monster/exile 专属 effect**（玩家 build 不可达）：GABogGiant*、
  ExplosiveTeleportSandDjinn、SirenBossWaterSpout、GeneticsScientist*、
  CarnivorousPlantOrbProjectile、MPSBoneRabbleBurningArrow、
  EssenceDrainRogueExileWitch2、PainOfferingRogueExileWitch1、
  MMSHellscapeDemonEliteTripleMortar（both_diff 行）。
- **player effect 但 vendor 未实现映射**：ArchonOfChayulaPlayer、
  MantraOfDestructionPlayer、DarkTempest/ManaTempestPlayer、IceTippedArrowsPlayer、
  SupportSeeRedPlayer、SupportPotentialPlayer、SupportSlamAftershocksPlayer、
  SupportFusilladePlayer、SupportTitanicArrowsPlayer、SupportMetaTotemSpellTotemPlayer、
  SupportMetaCastOn{Crit,Death,MeleeKill}Player（trigger_meta_gem_damage）、
  WalkingCalamityPlayer、WitchHunterMarkPlayer、Wyvern{Rend,FlameBreath,Devour}Player。
  PoB2 自身不注入 ⇒ 切换后消失属对齐 PoB2（parity 基准 P0 判据）。

**d) tag/keyword 第一批之外族（7 行 per-set；Data 侧 Unsupported = 正确保守）**

| 行 | vendor 依据 | 裁决 |
|---|---|---|
| SupportCloseCombatPlayer{,Two}::support_close_combat_…_from_distance ×2 | `sup_dex.lua:1238-1240`（DistanceRamp tag）+ `ModStore.lua:557-560`（`cfg.skillDist` nil → mod 不参与）+ `CalcActiveSkill.lua:642`（skillDist 仅 effective mode 才有） | **非 effective 模式下 PoB2 同样不吃该 mod**——Data 跳过 = 对齐；legacy 无条件 +30% MORE = 过算误映射。effective-mode 的 DistanceRamp 求值随 M3 config（skillDist）接入 |
| DemonFormPlayer::demon_form_grants_cast_speed | `other.lua:4384-4386`（GlobalEffect Buff tag + Condition DemonForm） | buff 域（W-J/M3）；legacy 无条件注入 = 过算 |
| PainOfferingPlayer::pain_offering_attack_and_cast_speed | `act_int.lua:15492-15494`（GlobalEffect Buff tag） | 同上 |
| SupportFerocityPlayer::skill_consume_frenzy_charge_… | `sup_dex.lua:2226-2230`（MultiplierThreshold + scalar=ConsumedFrenzyChargeEffect） | scalar 是 T2.2 既定边界（固定 1.0 整条跳过）；legacy 无条件 MORE = 过算 |
| SupportUrgentTotemsPlayerThree::totem_skill_{attack,cast}_speed ×2（global 行） | `SkillStatMap.lua:611-616`（`KeywordFlag.Totem` 门控） | PoBR `KeywordFlags` 未移植 TOTEM 位（pobr-data 改动不在本 track 文件归属），保守跳过；legacy 丢门控注入 = 误映射（仅当 support 只配图腾技能时数值巧合相等）。defer：移植 TOTEM/WARCRY 位后直译 |

### 2.3 both_diff 4 行（全部修对）

`skill_speed_+%`（global，carrier=monster effect）与
`SupportMultishotPlayer{,Two}::support_scattershot_skill_speed_+%_final`：vendor 同
条目三 mod（`SkillStatMap.lua:554-557` / `sup_dex.lua:3157-3161`——Speed +
WarcrySpeed(KeywordFlag.Warcry) + TotemPlacementSpeed），Data 全注入（后两个为惰性
名，无消费方不改变计算）；legacy 只出 SkillSpeed。`summon_totem_cast_speed_+%`
（carriers=SupportUrgentTotems*）：vendor = `TotemPlacementSpeed INC`
（`SkillStatMap.lua:2400-2401`），legacy 误并 CastSpeed——Data 修对。

## 3. L2 终版结论（前置条件 2）

18 build 双跑（Legacy vs Data，`mode_effective=false`，Pinnacle）。**10 个 build
逐字段一致**：ember-fusillade / bow-shot / spirit-walker-twister / wolf-pack /
frost-bomb / titan-shield-wall / abyssal-lich-detonate-dead / **monk-twister /
gemling-grenade / pathfinder-ice-shot**（后三个 + deadeye 的 Elemental Armament
缺失由 E4 修复归零/收敛）。8 个 build 有差异，每处已用
`l2_runtime_compare_records` 定位到具体 (stat, 取数点) 并逐条引证：

| build | 字段偏移 | 根因（取数点级） | vendor 依据 | 裁决 |
|---|---|---|---|---|
| druid-oracle-comet | dps/hit −30%，ignite −51% | data_only：`support_spell_cascade_damage_+%_final` → Damage MORE −30（legacy 漏注入 Spell Cascade 的 less 惩罚） | `sup_int.lua:7917-7919` | 修对（吃到 −30% less） |
| sorceress-stormweaver-comet | dps/hit −30% | 同上（SupportSpellCascadePlayer） | 同上 | 修对 |
| sorceress-disciple-of-varashta-comet | dps/hit −30% | 同上 | 同上 | 修对 |
| sorceress-chronomancer-essence-drain | dps/hit +35% | data_only：`support_slow_cast_spell_damage_+%_final` → Damage MORE +35（Considered Casting） | `sup_int.lua:2117-2119` | 修对 |
| witch-blood-mage-coiling-bolts | dps/hit +35%，ignite +82% | 同上 | 同上 | 修对 |
| ranger-deadeye-explosive-grenade | dps/hit −39%，ignite −64% | data_only：`support_multiple_damage_+%_final` −25（Multishot less）+ `base_reduce_enemy_lightning_resistance_%` → LightningPenetration +30；both_diff：scattershot 三 mod（后两惰性） | `sup_dex.lua:3154-3156` / `SkillStatMap.lua:929-931` / `sup_dex.lua:3157-3161` | 修对（净值=less 惩罚 > 穿透收益） |
| monk-martial-artist-flicker-strike | dps/hit −23.08%，ignite −41% | legacy_only：`support_close_combat_…_from_distance`（DistanceRamp，§2.2d）剔除 ×1.30 | `sup_dex.lua:1238-1240` + `ModStore.lua:557-560` | 修对（= PoB2 非 effective 行为） |
| warrior-smith-of-kitava-shield-wall | dps/hit −23.08%，ignite −41% | 同 flicker（Close Combat）+ data_only：off_hand per-15-shield PerStat 条目（cfg 无 `ArmourOnWeapon 2` 乘子 → 贡献 0，惰性） | 同上 + `SkillStatMap.lua:2115-2122` | 修对 |

> 风险登记（R5 机制）：Close Combat 两 build 与 Spell Cascade 三 build 在 ninja
> 基准（PoB2 网站口径）下若以 effective 模式计算，切换后会进补偿清单——T2.4 切换
> commit 时按 L2 表逐 build 核对 OFF_HIT5 变化方向，不回滚正确行为。
>
> **切换实测兑现（§6 commit ①）**：8 个变化 build 与本表逐一对应、无表外变化；
> OFF_HIT5 唯一掉出项 = deadeye（1.02x→0.77x，Multishot less 假命中拆除）已按
> 本预案进补偿清单；stormweaver 1.32x→0.92x 反向**进** @10%（修对即收敛）。

## 4. oracle 抽样（前置条件 3）

**71/71 PASS**（global 59 + per-set 12；探针值 240）。桶覆盖：plain 8 / div 8 /
mult 4 / base 3 / value 8 / multi-mod 8 / Condition 8 / ActorCondition 3 /
Multiplier+PerStat 8 / flags 8 / conversion gain-as 5 / skill_data（伤害基值 +
duration）8 / per-set 覆盖 12（计数有交叠）。对拍法 = vendor
`calcs.mergeSkillInstanceMods`（`CalcActiveSkill.lua:82`）以合成 statSet +
`extraStats` 受控注入逐 stat 跑真实 merge，注入后 modList 的名字/flags/keyword/tag
经引擎翻译层（`translate_mod_name`/`translate_tag`）归一后与
`stat_map_engine::map_stat` 输出做多重集相等比较（值容差 1e-6）。
报告：`target/statmap-diff/oracle-report.md`；重跑：
`POBR_POB2_SRC=<vendor>/src cargo test -p pobr-build --test statmap_dual_run --
--ignored --nocapture oracle_statmap_sampling`。

## 5. 遗留 / 移交

- per-set 覆盖默认 set `"1"`；XML `statSetIndex` 显式选择（T5.4 已入模型）与引擎
  set_key 的接线已在切换时核对：18 个 ninja build 的 decoded.xml `statSetIndex`
  全为 nil（默认），`mapped_stat_modifiers` 当前恒传 `set_key=None`（= 默认
  set "1"，与 PoB2 缺省一致），无行为差；显式 set_key 接线随 W-J 多 set
  global-only merge 落地。
- `GlobalEffect` tag 条目（buff 族）/ scalar 条目 / DistanceRamp（effective mode）/
  KeywordFlag TOTEM·WARCRY 位 / MinionModifier LIST（M5a）/ setOffHand* skillData
  通道：按 §2.2 分类 defer，均有上报分类可追踪。光环/Mark 自身 buff 的两个
  legacy 存留映射（GlobalEffect Buff 族的临时通道）已迁 `buff_stat_map.rs`
  存活，buff 域数据化（W-J/M3）后退役。
- `unknown_mod_name 386`：PoBR 未消费的 PoB2 名（AilmentMagnitude / AreaOfEffect /
  Duration / EnemyIgniteChance 族等）——随 M4 进攻深化逐名接入，引擎只需扩直通表。
- **进攻 @5% 缺口**（切换后 22/80=27.5%，阶段验收目标 ≥40%@5%）：补偿清单 =
  effective 模式 DistanceRamp/skillDist（M3 config）、buff 域（W-J/M3）、
  unknown_mod_name 直通表扩展（M4）、grenade 冷却吞吐 / Mirage 数据（M4/M5a）。

## 6. T2.4 切换/删除 commit 序列（2026-06-11 执行归档）

严格串行、独占 commit 序列（蓝图 T2.4；W-J 接线 commit 等切换 commit 合并后进行）：

| # | commit | 内容 | 结果 |
|---|---|---|---|
| ① | `bf71975` | **行为**：`DataOrchestratorOptions.stat_map_mode` 默认 Legacy→Data（一行回退点）；catalog 随 `BuildData::load` 加载、`calculate_with_data` 缺省回退（全部既有调用方自给自足） | 防御 114/120 逐值不变；进攻 hit5 23→22 / hit10 32 不变；8 个变化 build 与 §3 L2 表逐一对应、全部修对 |
| ② | `02bbf58` | **baseline 独立更新**（显式审查）：DEF 111/117→114/120（锁棘轮）、OFF_HIT10 31→32（stormweaver 进 @10%）、OFF_HIT5 23→22（**已审查例外**：deadeye 假命中拆除，依据 `sup_dex.lua:3154-3156` + `SkillStatMap.lua:929-931`，进补偿清单、不回滚正确行为——§3 风险登记预案兑现） | 门禁恢复全绿 |
| 补 | `4cc2d19` | 切换后续：pob2_parity 嵌入式旧样本（deadeye）容差放宽（同一补偿结，0.817x→0.613x） | — |
| ③a | `f1d9851` | **搬迁**：`map_aura_buff_stat`/`map_self_buff_offensive_stat` 原样平移 `buff_stat_map.rs`（aura/self-buff 注入路径不在 statmap 双跑范围，数据引擎对 GlobalEffect Buff 族按保守 Unsupported——删整文件前迁出存活） | 纯移动零行为 |
| ③b | `0c634b4` | **纯删除**：`skill_stat_map.rs`（751 行）+ `legacy_stat_filter.rs`（T5.3 消费侧兜底）+ `StatMapMode::Legacy` 变体/分支；Compare 保留为长期观测框架（蓝图 §6 Q4 + 00-index §2.2，语义改为 Data 计算 + outcome 记录，输出与 Data 一致）；statmap_dual_run.rs 收敛（删 L1/L2 legacy 双跑，保留 Compare 契约门禁 + 定位工具 + oracle 对拍） | 删后 ninja 逐值不变（114/120/22/32）；源码 `grep -r is_mappable_stat` 零命中（剩余命中均为 audits/ 历史审计文档） |

回退路径（蓝图 §5 R5）：删旧码前 = `#[default]` 移回 Legacy 一行；删旧码后 =
revert `0c634b4`（删除 commit 独立可逆）。
