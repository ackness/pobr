# 差距分析与实现计划（2026-06-05）

> 本文由一次 9-agent 并行差距分析（sonnet 探索 + 代码实读 + agent-docs/doc12 对照）综合而成，
> 用于驱动后续逐波实现。**权威性低于代码本身**——执行时仍以 `agent-docs/*` 一手事实、
> PoB2 Lua（`gh api repos/PathOfBuildingCommunity/PathOfBuilding-PoE2/contents/src/Modules/<f>.lua --jq .content | base64 -d`）、
> 现有 Rust 代码三者交叉验证为准。原始结构化缺口数据：本次会话 workflow 输出（`pobr-gap-analysis` run）。
>
> 注意：`11-implementation-progress.md` 已明显落后于实际代码，不要当权威；本文取代它作为当前规划基线。

## 0. 总览

差距分析共识别 **122 个缺口**（27 个正确性 bug、47 S / 56 M / 19 L），覆盖 doc12 的 P1–P7：
暴击(P1)、敌人 modDB+命中(P2)、异常(P3)、伤害缩放/转换(P4)、防御/恢复/充能/格挡/EHP(P5/P6)、
技能功能+辅助宝石(P6)、触发+召唤物(P7)、来源接入(物品/解析/天赋树)、PoB parity/Build/应用层。

### 关键工程约束（决定并行边界）

- `offence.rs`(被 23 个缺口触及) 与 `perform.rs`(14) 是**编排中心**；calc 核心高度耦合，必须大体**串行**。
- `hit_chance` 仅定义在 `defence.rs:40`，被 offence(攻击命中) 与 defence(被命中) **共用**——单一 owner。
  PoE2 中"你打怪"与"怪打你"命中公式不同，修复可能要拆分方向。
- calc 核心的**集成测试文件**（`tests/calc_minimal.rs`/`tests/perform_fill.rs`）的数值断言被
  crit/damage/ailment/defence 共享 → **深度并行会撞测试文件**。
- 真正安全并行的边界 = **独立 crate / 独立测试文件**的流：`pobr-tree`(radius_jewel)、
  `pobr-core` sources(item_text/item)、parity/build/app、triggers/minions(greenfield 新文件)。
- 共享常量收口：所有新增 `GameConstants` 先在 `pobr-data/src/constants.rs` 一次性加好，
  避免并行 agent 撞 constants.rs。

### 额度/恢复协议（用户约定）

- 用户 coding plan 约 **2026-06-05 等待约 1h 后重置**；sonnet 小任务"基本没限制"，opus 重活等重置。
- 编排原则：**正确性 bug 优先**；重活(L 级 opus 重构)排在额度充裕时；
  撞额度时用 `ScheduleWakeup`/后台 sleep 等重置后 `Workflow resumeFromRunId` 续跑（已完成 agent 走缓存）。

---

## 1. Wave 0 — 正确性 bug（PoE2 口径对齐，优先）

让现有输出先正确。按**文件/测试归属**分车道，避免并行冲突。**先做 constants 收口（step 0）**。

### Step 0（串行，先做）：constants 收口
在 `pobr-data/src/constants.rs` 一次性新增（值需对照 agent-docs + PoB2 验证）：
- `PLAYER_BASE_CRIT_DAMAGE_BONUS = 100.0`（PoE2 基础爆伤 +100% → 2.0；**待 PoB2 CalcOffence.lua 核对** 100 vs 200 表述，base=0 应得 2.0）
- 格挡硬上限 `BLOCK_CHANCE_CAP = 90.0`（现 survivability 写死 75）
- 感电下界 `BASE_SHOCK_MAGNITUDE = 20.0`（现写死 5）
- 必要时：`CRIT_CHANCE_CAP = 100.0`、`SERVER_TICK_RATE`（已有 `SERVER_TICK_SECONDS=0.033`）

### Lane A — calc-core 数值耦合 bug 簇（**串行，一个 agent**）
拥有 `offence.rs` + `damage.rs` + `defence.rs::hit_chance` + `ailment.rs`(shock) + `ehp.rs`(CI)
及其集成测试 `tests/{calc_minimal,damage_components,ailment,ehp}.rs`。逐个修：
- `crit-base-bonus-poe2`[S]：爆伤 150→PoE2 口径(base=0→2.0)；同步 `calc_minimal.rs:111` 期望值。
- `crit-multiplier-inc-more`[S]：爆伤补 `Inc`/`More` 乘区（现只读 Base，"暴击伤害加成增加"词条完全不生效）。
- `hit-chance-formula`[S]：`defence.rs:40` 改 PoE2 线性命中公式 `1-0.95*DE/(DE+4*AA)`（现为 PoE1 幂次近似），按攻防方向核对是否需拆分。
- `spell-must-hit`[S]：法术必中分支（offence 不应对法术套用攻击命中率）。
- `traced-dps-physical-only-divergence`[M]：`total_dps_traced` 与 `calculate_minimal` 伤害管线统一（traced 路径目前只用物理模名）。
- `missing-elemental-damage-modname-group`[S]：`calculate_components` 补 `ElementalDamage` 聚合组（火/冰/电共享 increased）。
- `damage-conversion-chain-order-wrong`[S]：`DAMAGE_TYPES` 顺序改 Phys→Light→Cold→Fire→Chaos。
- `added-damage-effectiveness-missing`[S]：added effectiveness（`AddedDamage` MORE，只乘外部 flat 不乘技能自带 base）。
- `shock-min-clamp-bug`[S]：感电下界 5→20%。
- `ehp-chaos-inoculation-wrong`[S]：CI build 混沌池 EHP（依赖 keystone 处理，可先特判 CI）。

### Lane B — survivability.rs（并行，独立文件+测试）
- `block-chance-cap-wrong`[S]：格挡上限 75→90。
- `spell-suppression-wrong-existence`[S]：PoE2 已移除法术压制——删除/改为 no-op（核对 active-defences.md/block.md）。
- `reservation-wrong-model`[M]：保留模型 PoE1 法力% → PoE2 Spirit 池。
- **约束**：只改 `survivability.rs` 内部函数 + `tests/survivability.rs`；**不动 perform.rs/perform_fill.rs**（归 Lane A/Wave1）。

### Lane C — pobr-tree radius_jewel.rs（并行，独立 crate）
- `radius-jewel-wrong-constants`[S]：三档半径常量对齐 PoE2 真实坐标系（查 PoB2 tree data / passive_tree_meta.json）。
- `radius-jewel-multiplier-not-applied`[S]：欧氏距离比较乘 `PassiveTreeJewelDistanceMultiplier`。

### Lane D — pobr-core sources（并行，独立测试文件）
- `item-text-range-tier-marker-not-stripped`[S]：剥离 `{range:N}` / `(tier: N)` 注释再喂 mod_parser。
- `item-quality-not-modeled-as-more-local`[M]：物品 quality → "more 局部物理/防御" modifier 注入。
- **约束**：只动 `item_text.rs`/`item.rs` + `tests/{item_text,item_source}.rs`。

> Wave 0 验收：`cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check` 全绿；
> 每个 bug 有最小单元测试；关键数值有 PoB2/agent-docs 出处注释。

---

## 2. Wave 1 — 地基（解锁最多下游，opus，大体串行于 calc 核心）

- **P2 enemy.mod_db 接线**（`enemy-mod-db-not-read`,M）：`setup_env` 注入敌人(怪物等级缩放表 + Boss 四档)；
  offence 读 `enemy.mod_db`（受到增伤/抗性/护甲/破甲）；曝光取最强(min_of)。解锁暴击弱点/曝光/诅咒/破甲/受伤 debuff。
- **P1 resolve_crit 重构**：抽 `resolve_crit(player,enemy,cfg,hit_chance,base_crit,mode_effective)`；
  接 `crit-mode-effective`(命中降级) + flags(`CritChanceLucky`/`BifurcateCrit`/`InevitableCriticalHits`/`NoCritMultiplier`) + `crit-enemy-selfcrit` + cap 可查询。
- **P4 伤害转换链**：固定 Phys→Light→Cold→Fire→Chaos、技能转换先于全局、>100%归一、increased 沿途类型累积(双重 dip)、gain-as-extra、穿透/Overwhelm；`DamageComponent` 加 `kind:Hit/Dot`+沿途类型集。
- **P3 异常施加概率/阈值**（`ailment-chance-threshold-missing`,M）：阈值表(EnemyAilmentThreshold/PoiseThreshold)、施加几率、从指定 hit 分量派生。

## 3. Wave 2+ — 功能建出（多数可按子系统并行，注意 perform.rs 串行收口）

- **防御扩展**(P5/P6)：ES recharge、avoidance、taken 乘数、充能乘数封顶、偷取/再生/Recoup(0.5.0)、BuffEffect、Max Hit/EHP 完整。
- **技能功能**(P6)：`SkillUseTime` 替换 offence 简化 action_rate(`offence-action-rate-not-replaced`)、辅助宝石 mana mult/more 隔离/skill-type gating/level&quality 缩放、AoE √area、投射物(Split/Pierce/Fork/Chain)、冷却/消耗。
- **触发/召唤物**(P7,greenfield)：`Env.minions:Vec<Actor>`、player→minion 三通道、召唤物内禀(必中/爆伤+70/怪物缩放)、触发速率上限 `1/(ceil(cd×TickRate)/TickRate)`。
- **来源/天赋树**：`PassiveTreeSpec` mastery 选择字段 + 自动装配、pobr-item 职责厘清、CLI parse-item、珠宝 socket gating。
- **parity/build/app**：PoB 全量 parity matrix fixture + CI gate、10 个 PoB 导出 golden fixtures + 容差、应用层补全。

## 3.9 执行记录

### Wave 0 ✅（2026-06-05 完成）
3 车道并行(sonnet) 全部落地，全 workspace **332 测试通过 / 0 失败 / clippy+fmt 全绿**。已核验：
hit_chance 攻防双公式逐字匹配 PoB2 `CalcDefence.lua:32-44`；爆伤 `1+(100+ΣBase)/100×(1+inc)×more`(base=0→2.0)；
感电 clamp[20,100]%；BLOCK_CHANCE_CAP=90/PLAYER_BASE_CRIT_DAMAGE_BONUS=100/SHOCK_MIN_EFFECT=20。
- Lane A：crit 150→2.0、crit inc/more、hit_chance(攻/防拆分 `monster_hit_chance`)、spell-must-hit、traced 统一、
  ElementalDamage 组、转换链顺序 Phys→Light→Cold→Fire→Chaos、added effectiveness(最小切片)、shock 20%、EHP CI 特判、block cap 90。
- Lane C：radius_jewel 常量对齐 PoB2 `data.jewelRadii["0_1"]` outer×1.2、新增 VeryLarge 档。
- Lane D：item_text 剥离 `{range}`/`(tier)` 注释、item quality→局部 more(武器物理/护甲防御)。
- Defer 到后续：spell-suppression 完整移除(已 inert)、reservation→Spirit、radius Variable 档、催化剂品质、完整转换链。

### Wave 1a ✅ enemy.mod_db 地基
根依赖，解锁曝光/穿透/受伤链/SelfCrit/异常阈值/Overwhelm 等 ~12 下游。范围：怪物等级缩放表+EnemyTier(数据)、
setup_env 注入 enemy.mod_db、offence 读取(受伤链/抗性/护甲/曝光 min_of/CannotBeEvaded)、mode_effective(面板 vs 有效 DPS)开关。

**Step 1 ✅**：`pobr-data::monster`（怪物缩放表 5 张 + `EnemyTier`/`EnemyTierDefaults` 聚合，纯数据零 I/O）。

**Step 2 ✅（calc 接线）**：178 pobr-core 测试 / 380 workspace 测试通过、clippy+fmt 全绿，新增 18 用例。落地：
- `CalcConfig::mode_effective`（默认 `false`=面板/裸 DPS 口径；`true`=有效 DPS 引入敌人交互）。
- `calc/setup_env.rs`：`setup_enemy(env, level, tier)` 把 accuracy/evasion/armour/元素抗/ChaosResist/Uber DamageTaken
  /Boss 通用 debuff 抗性(`CurseEffectOnSelf/ExposureEffectOnSelf/SlowEffectOnSelf MORE -50`、`PoiseThreshold +500`)
  /`Condition:Unique|RareOrUnique|PinnacleBoss` 注入 enemy.mod_db（全部 `SourceKind::EnemyConfig` 归因）；
  `env_with_enemy` 便捷构造；`reduce_enemy_exposure` 曝光取最强→`*Resist BASE -magnitude`。
- `ModDb::max_of`（曝光 `ExposureMin`/取最强语义，`exposure-min-of-aggregation`）。
- `offence::calculate_minimal_vs_enemy(player, enemy, cfg, input)`（旧 `calculate_minimal` 三参委托空敌人，向后兼容）：
  enemy 受伤链(`DamageTaken` 通用+`<Type>DamageTaken` INC/MORE)、抗性/护甲减伤、`CannotBeEvaded`/敌方 `CannotEvade` 满命中短路、
  敌方格挡扣命中——**均仅在 `mode_effective=true` 生效**。
- `perform` 改读 `&env.enemy.mod_db` + enemy.mod_db 的 Evasion BASE 派生命中率（回退标量）。
- **向后兼容**：空 enemy.mod_db + 默认面板口径下输出与历史一致，160 原 pobr-core 测试全部继续通过。
- **Defer**：穿透(`tier.pen()`)注入 player 侧 `<Element>Penetration`（依赖玩家伤害减抗末端，留待伤害管线 wave）；
  玩家施加 debuff(诅咒/破甲/凋萎/曝光来源)的具体注入(留 `reduce_enemy_exposure` hook，下游 wave 填)；
  resolve_crit 读 enemy `SelfCritChance/SelfCritMultiplier`(归 P1 暴击 wave)。

### Wave 1b ✅ 暴击管线完成（commit 2d242e4 内）
新建 `calc/crit.rs`：`resolve_crit`/`resolve_crit_traced`/`CritOutcome` 消双轨重复。对照 PoB2 `CalcOffence.lua:3620-3838`：
flags(`CritChanceLucky`/`BifurcateCrit`/`InevitableCriticalHits` 几何级数/`NoCritMultiplier`)、可查询 `CritChanceCap`、
敌方 `SelfCritChance/SelfCritMultiplier`(暴击弱点)、traced inc/more 归因(`ModDb::flag_origin`)。flags gate 到 `mode_effective`。399 测试。

### Wave 1c ✅ 伤害转换链 + 穿透 + Overwhelm（commit cb6301a）
对照 PoB2 `CalcOffence.lua` processDamageConversion/buildGainTable/conversionTable：
- `DamageComponent` 富化 `kind(Hit/Dot)`/`source`/`type_path`(Copy→Clone)；`ConversionRules`(skill/global 两阶段+>100%归一+fold)；
  gain-as-extra；按 `type_path` 双重 dip；无转换 mod 走快速路径逐字回归。
- 有效 DPS 减伤末端(仅 mode_effective)：元素/混沌穿透 `max(resist-pen,0)`、Overwhelm(`EnemyPhysicalDamageReduction` clamp[0,75])。420 测试。
- **待决**：double-dip 按 agent-docs(来源+目标)实现，PoB2-dev 主循环仅目标——golden 对齐时需决策。
- 校正：`EnemyTier.pen()` 是"敌人穿透玩家"(防御侧)，非玩家穿透，未注入 offence。

### Wave 1d ✅ 异常施加几率 + 阈值表（commit b37395c）
对照 PoB2 `CalcOffence.lua` calcAilmentDamage/effMult + `CalcSetup.lua` AilmentThreshold：
- 数据：`MONSTER_AILMENT_THRESHOLD_TABLE`/`MONSTER_POISE_THRESHOLD_TABLE`(各100项, 抄 Misc.lua)+异常常量。
- 施加几率管线(几率派生型 点燃/感电 + 内禀型 流血/中毒)、effMult(敌抗+DamageTakenOverTime, 物理无视抗)、暴击加权、
  玩家阈值=maxLife×0.5(修 bug)、TraceGraph 归因。perform fill 改"几率×magnitude 期望值"口径。450 测试。
- **defer**：冰冻/电击 Poise 积累(非伤害异常)、叠层 CanStack(L)、AilmentEffect/Faster/Slower 维度、跨类型施加、DotDpsCap、crit ailment mode。

### ✅ Wave 1 地基完成（2026-06-05）
enemy modDB → 暴击 → 伤害转换/穿透/Overwhelm → 伤害型异常(几率/阈值/effMult/暴击加权) 全部落地并提交。
332 → **450 测试**(+118)，全程 clippy+fmt 全绿、PoB2 Lua 逐字核对、向后兼容(新机制 gate 到 mode_effective)。
**剩余 = Wave 2+**（防御扩展/技能功能 SkillUseTime/触发召唤 greenfield/来源天赋/PoB parity matrix+golden）。

### Wave 2 批次1 ✅（commits eb50627·f331ef3·522b2c8）
4 子系统并行内部逻辑 → 串行集成 → 验证。**450 → 550 测试**。
- **parity**：PoB catalog fixture(215 display_stats/1092 output_keys, 从 PoB2 Lua gh 生成, `devs/fixtures/pob/parity/`)+ `check_pobr_parity` CI gate；天赋树 `PassiveTreeSpec.mastery_effects` 玩家选择注入。
- **防御扩展**：ES recharge(12.5%/s, 4s delay, ZealotsOath)、avoidance(乘法叠加+cap)、taken 乘数(WhenHit/OverTime)、暴击额外伤害减免/EnemyCritEffect。
- **辅助宝石**：skill_source 4 TODO 全清(mana mult/more 隔离/skill-type gating/level&quality)+ SkillTypes 扩展。
- **召唤物+触发** greenfield：`calc/minion.rs`(独立 Actor+三通道+内禀必中/爆伤+70/怪物缩放)、`calc/trigger.rs`(速率上限)；集成 `Env.minions:Vec<Actor>` + `perform_minions` 多 Actor 复用同管线。
- **defer(Wave2 批次2+)**：防御 charges/leech/regen/Recoup/BuffEffect/完整MaxHit；技能 AoE/投射物/cooldown/cost；召唤物 limit/MinionDef schema/跨Actor trace/ally-buff 缩放；触发 energy/轮转/CWC/rate-wiring；parity golden(10 build);pobr-item/CLI parse-item；剩余异常(冰冻电击Poise积累/叠层/AilmentEffect维度/跨类型/DotDpsCap)。

### Wave 2 批次2 ✅（commits 654ce4b·a40edd4）
4 子系统并行 → 集成 → 验证。**550 → 670 测试**。
- **防御恢复**(survivability.rs)：充能乘数封顶(Power/Frenzy/Endurance)、偷取(0.5.0 单实例三上限)、再生增强(XRecoveryRate)、Recoup(8s/4s)。
- **剩余异常**(ailment.rs)：冰缓 effect clamp[30,50]、冰冻/电击 Poise 积累、叠层权重平均 DPS(替换单层简化)。
- **技能功能**(skill_mechanics.rs)：AoE √area、投射物数量+行为(Split→Pierce→Fork→Chain)、冷却、消耗(mana/life 分步取整)。
- **应用层**：CLI parse-item 打通(调 item_text/ingest_item)、golden 回归 harness(2 份真实 PoB2 ninja build 端到端快照基线)。
- 集成：OutputTable +24 字段(手写 Default 中性) + fill_skill_mechanics + fill_ailments 扩展 + 充能/偷取/Recoup，TraceGraph 贯穿。
- **defer(Wave2 批次3+)**：召唤物完整编排/MinionDef 入库 schema/跨Actor trace/ally-buff 缩放；触发 energy/轮转/CWC/rate-wiring；parity golden(10 build 真实 PoB2 数值对齐)；AilmentEffect/Faster/Slower 维度+跨类型施加+DotDpsCap；BuffEffect；投射物距离衰减；技能基础参数入库。

### Wave 2 批次3 ✅（commits 862d25b·359d1b3）
**670 → 801 测试**。
- **召唤物完整化**：pobr-data `MinionDef` 入库 schema(life/damage/attackTime/crit/resist+SkillTypes, 4 代表性常量)、`MinionData::from_def` 桥接、数量上限(Multiplier:SummonedMinion)、per-minion 乘数、`Env::add_minion_from_def` + perform_minions 真实底材 + 跨Actor 通道。
- **触发完整**：能量驱动(Cast on X)、多技能轮转(确定性模拟)、CWC(triggerTime 取整)；`fill_trigger` 冷却驱动接入 perform。
- **异常维度**：AilmentEffect/Faster/Slower、跨类型施加(`<Type>Can<Ailment>`)、DotDpsCap=(2³¹-1)/60。
- **i18n+显示**：输出 stat 显示文本(en-US 80 key canonical + zh-TW 44 译文)、display_catalog 映射、golden 扩展。

### 🎯 calc 机制阶段基本完成（2026-06-05）
Wave 0 + Wave 1 + Wave 2(批次1-3)：**332 → 801 测试**(+469)，21 commits。PoBR 计算核心从最小闭环推进到**覆盖 doc12 P1-P7 全机制族的完整 PoE2 战斗引擎**(暴击/伤害转换/异常/防御恢复/召唤物/触发/技能功能 + enemy modDB + parity 框架 + i18n)。

### Wave 3 批次1 ✅ build-layer 集成（2026-06-05）
**801 → 834 测试**(+33，含先前 `BuildData`/`calculate_with_data`/`setup_enemy` 一批)。打通「一份 PoB Build Code → 完整计算」的端到端生产路径，**不再依赖测试内手写 XML 抽取**。
- **`pobr-build::build_data::BuildData`**：把 `GameData` 按域投影为内存索引(节点表/宝石表/职业属性)，唯一 I/O 入口，调用方一次加载多次复用。
- **`pobr-build::calc_orchestrator::calculate_with_data`**：端到端归因编排——角色基础(职业起始属性→`CharacterBase`)→装备(`add_item` 槽位+段落归因)→天赋树(`collect_allocated_mods`→节点归因)→技能宝石(active/support 分类+source 注册)→敌人(`setup_enemy`)→额外文本；逐件 `filter_parseable` 容错(PoB skip-and-collect)。`CalculationSession::setup_enemy` 经 session 暴露。
- **`pobr-core::item_text::parse_pob_xml_item`**：解析 PoB Build XML 内嵌 `<Item>` 文本块(**无 `--------` 分隔**，按 `Implicits: N` 计数切分；`{enchant}{rune}`/`{fractured}`/`{desecrated}` 前缀剥离；`Rune:`/`Sockets:` 等 XML 专有元数据行跳过)，复用既有 `classify_mod_lines`/`strip_pob_annotations`。
- **`pobr-build::xml_build::{parse_build, parse_build_from_code}`**：生产 XML→Build 解析器——`<Tree activeSpec>`选中`<Spec nodes>`、`<Items activeItemSet>`选中`<ItemSet>`的`<Slot name itemId>`→`EquipmentSlot`映射(枚举外槽/空槽忽略、武器组按`useSecondWeaponSet`切换)、`<Skills activeSkillSet>`下每个`<Skill>`→一个 `SocketGroup`(启用态+启用 gem)。单件解析失败跳过该件不中止。
- **CLI `calculate-build`**：`pobr calculate-build --file <code> [--data-dir ..] [--enemy-tier ..] [--panel]`，decode→parse_build→BuildData::load→calculate_with_data，输出 Build 摘要+关键标量 JSON。真实 deadeye 验证：154 节点/9 装备/9 宝石组、life 2376、抗性 75、命中率 0.82(vs Pinnacle)、爆伤 2.0。
- **e2e 重构**：`tests/e2e_real_build.rs` 改用生产 `parse_build_from_code`，删除 ~250 行测试内手写抽取(DRY)；两真实 fixture(Deadeye/Martial Artist)端到端回归。
- **已知切片(deferred)**：宝石→modifier 文本(分等级 stat set 未导出，DPS/技能伤害=0)；物品基底防御值(`Armour:`/`Evasion:` 非词条文本，armour/evasion 局部计算未接)；`masteryEffects` 选择；JewelSocket 内嵌珠宝；第二武器组独立 Spec。

### Wave 3 批次2 ✅ 宝石数据通道 Phase 2+3 — 真实 DPS 解锁（2026-06-06，commit `ae9676f`）
**834 → 837 测试**。打通「PoB `<Gem skillId>`+等级 → stat-set 基础伤害 → `<Type>DamageMin/Max` BASE → offence 伤害分量 → **DPS**」，端到端计算首次产出真实技能 DPS（此前恒 0）。
- **解阻 = `GrantedEffectStatSets*` 表重下**（此前误记为外部阻塞，实测 GGG PoE2 CDN + npm 均可达）：`node download-index.mjs && npx -y pathofexile-dat@15`；偶发 socket 中断的大 bundle 手动 curl（`Folders/` 前缀）落缓存再跑；config.json 列名按 PoE2 schema variant(`validFor=2`)修正(`BaseResolvedValues/FloatStats/AdditionalStats`，无 `DamageEffectiveness`)。
- **伤害解析**：`FloatStats[i]↔BaseResolvedValues[i]`(+`AdditionalStats↔AdditionalStatsValues`) = 每级已解析 min/max；FireballPlayer L1=8/12、L20=224/336（对齐 vendor `Data/Skills/act_int.lua`）。
- **入库 + 注入**：`granted_effect_stat_sets.json`(2068 效果) → `pobr-gamedata::skill_stat_sets()` → `BuildData.skill_stat_sets` → `ResolvedSkillLevel.base_damage` → `damage_stat_to_mod`(`<source>_<min|max>_<base|added>_<type>_damage`, source∈spell/secondary/attack)。Phase 2(action rate/cost/cooldown)随同提交。
- **defer**：per-gem/support 宝石 more 倍率(`resolve_gems` 传空文本)、武器伤害(attack 技能仍 0)、DoT per-minute、多主技能 use_time。

**剩余工作 → 见 [`14-remaining-work-recheck.md`](14-remaining-work-recheck.md)**（2026-06-06 8-agent 实地重核，55 项分 P0–P3 + 并行编排建议；下列为粗粒度入口）：
1. ✅ ~~宝石数据通道 DPS~~（Wave 3 批次2 完成；剩 per-gem/support 注入 = recheck P0-2）。
2. **数据管线**：minion/aura/unique/CostTypes 入库（recheck P2-6，独立流，解锁最多下游）。
3. **真·PoB2 golden 数值对齐**：parity catalog 全 `Planned`、无 golden 回归(recheck P0-1，外部阻塞)。⚠️含 Wave1c 双重 dip 分歧决策。
4. **buff/aura ingest 系统**：BuffEffect/光环/玩家 debuff→enemy（recheck P1-1，单点解锁面最大）。
5. **silent bug**：`cross_type_source_hit` 从不调用(recheck P0-3,S)、异常叠层 Ignite/Shock/Chill 未实现(P1-2)、minion ally-buff 无缩放(P0-4)。
6. **应用层 GUI**(pobr-desktop egui，recheck P3-3)。

## 4. 编排映射（并行/串行）

| Wave | 串行流(calc 核心) | 可并行流(独立 crate/测试) | 模型 |
|------|------------------|--------------------------|------|
| 0 | Lane A(calc bug簇) | Lane B(surv)/C(tree)/D(sources) | sonnet |
| 1 | enemy modDB→resolve_crit→转换链→异常阈值 | — | opus |
| 2+ | perform.rs 收口 | 防御/技能/触发召唤/来源/parity 各子系统 | 混合 |
