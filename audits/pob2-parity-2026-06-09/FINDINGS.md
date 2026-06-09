# FINDINGS — PoB2 Parity 审查全量 finding 总表

> 审查日期：2026-06-09 · 对照基准：vendor PoB2 Lua 源码（CalcSetup/CalcPerform/CalcOffence/CalcDefence/CalcTriggers/ModList/ModStore/Global/ConfigOptions/Item 等）
> 详细内容见各子系统报告（01–06）。ID 命名规则：`<子系统编号>-<序号>`。

## 修复状态（更新于 2026-06-09）

CRITICAL + 全部 10 条 HIGH 已落地（每条对照 PoB2 源码二次核实后修复并补回归测试）。全 workspace `cargo test`(956 passed) + `clippy -D warnings` + `fmt --check` 全绿，无 parity 回归。

### 第三波·启用工作（更新于 2026-06-10）

第二波 defer 的 partly finding 经 **5 个 worktree 隔离 agent 并行实现各自的启用工作**（参考 vendor PoB2 一手实现），逐个合并回 master、每次合并跑 ninja_parity 门禁。全部合入后 `cargo test --workspace`**997 passed** + clippy + fmt 全绿、**ninja_parity 无回归**（baseline 未动）。

| ID | 启用工作 | 状态 |
|----|---------|------|
| 01-04 | keyword_flags 解析接线（`for poison`/`of curse auras` 等 + DoT 位常量）→ `matches_context` 真正生效 | ✅ 启用 |
| 02-03 | boss debuff 消费侧（曝光乘 `ExposureEffectOnSelf` 后折抗）+ `Condition:Effective` 由 mode_effective 派生门控 | ✅ 启用 |
| 03-01/03-02 | build 层 `trigger_modifiers`/`in_group_trigger_source_rate` 注入触发冷却+源速率（内建触发主技能）→ 触发面板从恒 0 打通 | ✅ 启用（support-gem 触发链路/CWC 数据 defer） |
| 03-06 | CWC 分支改走 `calc_multi_spell_rotation` 单技能路径 | ✅ 启用 |
| 05-01 | `estimate_active_stacks`（命中×施加×持续×速度）→ SP>1 → over-stacking 暴击放大真正生效 | ✅ 启用（多 hit/冷却/图腾分支 defer） |
| 05-04 | `cross_type_source_hit_at_roll` 双 pass：RollAverage 高位偏移 | ✅ 启用 |
| 05-05 | `calculate_minimal_traced_vs_enemy`：traced DPS 串敌方格挡/减伤/命中降级，对齐 panel | ✅ 启用 |
| 05-07 | `apply_dot_dps_cap` 改用常量（PoB2 无 DotDpsCap-Override 机制），移除 modDB 读取 | ✅ 启用 |
| 06-06 | `taken_mult_for_type` 支持 Attack/Spell hit-source 上下文（PoB2 Average 默认）；反射 defer（PoB2 自身禁用） | ✅ 启用 |
| 01-06 | config `defaultState` 导入（XML 省略=PoB2 默认值，如 DemonForm/BypassCD） | ✅ 启用 |
| 02-05 | 移除零消费者死 modifier（per-type quality 已在 orchestrator 正确）；催化剂 defer（缺 mod-tag 层+Item catalyst 字段） | ✅ 归位 |

> 三波合计：1 CRITICAL + 10 HIGH + 6 MED/LOW confirmed + 11 项启用工作落地，4 条经验/复核 rejected（04-02/05-06/03-05/04-03），余 03-04-Repeats/02-06-add_skill_types/触发 support 链路/催化剂等 defer（缺更深数据模型，已逐条记录缺什么）。

### MEDIUM/LOW 第二波（更新于 2026-06-09）

23 条 MEDIUM/LOW 经并行核查（对照 PoB2 一手源码）：**7 confirmed · 15 partly · 1 rejected**。已落地 **6 条 confirmed**（含回归测试），全 workspace `cargo test`(966 passed) + clippy + fmt 全绿、ninja_parity 无回归。

| ID | sev | 状态 | 说明 |
|----|-----|------|------|
| 01-03 | MED | ✅ 已修复 | Multiplier/PerStat 倍率加 `floor(base/div+0.0001)`（PoB2 EvalMod） |
| 01-05 | LOW | ✅ 已修复 | 新增 `ModDb::get_multiplier`（PoB2 GetMultiplier 语义）+ perform_minions 收口 |
| 03-04 | MED | ✅ 已修复 | DPS 路径加服务器帧 cap（30.3×Repeats，非引导）+ 冷却 cap 乘 Repeats |
| 05-03 | MED | ✅ 已修复 | 感电 magnitude 补 `EnemyShockMagnitude`/`AilmentMagnitude` effectMod |
| 06-03 | MED | ✅ 已修复 | ES 充能延迟拆 BASE(秒,分子)/INC(%,分母)，支持 Override |
| 06-04 | MED | ✅ 已修复 | 减伤上限参数化（`DamageReductionMax` 词条），去 perform 提前 0.9 clamp |
| 04-02 | MED | ❌ 否决（经验证据） | 审查建议去掉 gain-source base fallback；实测 **ninja_parity 进攻 @5% 从 23 跌到 22**，fallback 实为 load-bearing（conv_min 某路径未携对角线留存）→ **保留 fallback**，理论分析被一手 parity 数据反驳 |
| 05-06 | LOW | ⊘ rejected（核查） | flat_chance 敌方 SelfXChance —— 核查判定不成立 |
| 01-04 | LOW | ✅ 已修复 | 补 `KeywordFlags::matches_context`（PoB2 MatchKeywordFlags 三段：空集恒真/MatchAll 子集/否则 ANY）。当前全 NONE 下恒真、零行为变化、为 keyword 接线就绪 |
| 03-05 | LOW | ⊘ rejected（复核） | 审查称 AoE 把 BASE AreaOfEffect 加入基径为臆造——**错**。PoB2 CalcOffence.lua:429 确实 `baseRadius += Sum("BASE","AreaOfEffect")`，PoBR 是逐字移植，**移除反而破坏 parity** → 不改 |
| 04-03 | LOW | ⊘ rejected（复核·核心） | 审查称 AttackDamage/SpellDamage 独立 ModName 是缺陷——核查证 PoBR 解析名与聚合名两侧自洽、与 PoB2 聚合期等价 → 命名约定差异，不改；仅"补复合名解析"属可选增量，defer |
| 02-04, 04-04 | MED/LOW | ⊘ defer（无补丁） | 核查判 partly 但无安全增量补丁 |
| 01-06, 02-03, 02-05, 02-06, 03-03, 03-06, 05-04, 05-05, 05-07, 06-06 | MED/LOW | ◷ defer·补丁就绪 | 10 条 vetted 补丁存档，**当前均零数值/parity 影响**（latent/基础设施/跨层）：05-07(PoB2 无 DotDpsCap-Override 机制)、05-04(需活跃叠层模型)、05-05(traced-vs-enemy 重构)、06-06(攻击/法术·反射 takenMult 需跨函数 plumb)、02-03/02-05/02-06/03-03/01-06(消费侧/build 层未接线)。待各自启用依赖落地时逐条 parity-gated 接入 |

> **关键教训**：parity harness 是理论分析的经验仲裁者。04-02 的一手 PoB2 公式分析看似正确，但 ninja_parity 真实 build 数据表明 PoBR 的 `conv_min` 在某转换路径未正确携带对角线留存，原 fallback 在补那个隐藏缺口——直接按"PoB2 理论"去掉反而倒退。后续每条落地都应跑 ninja_parity 验证。

| ID | sev | 状态 | 修复要点 |
|----|-----|------|----------|
| 02-01 | CRITICAL | ✅ 已修复 | `setup_env` 注入 Boss 元素穿透 `ElementalPenetration` 到玩家 db |
| 01-01 | HIGH | ✅ 已修复 | `more()` 改逐 modName `round(modResult,2)` 后跨名连乘（PoB2 ModList MoreInternal），四条 more 路径同步 |
| 01-02 | HIGH | ✅ 已修复 | ModFlags 改子集语义 `is_subset_of`（PoB2 ModList） |
| 02-02 | HIGH | ✅ 已修复 | `setup_enemy` 改就地增量装配，不再整体替换 enemy actor |
| 03-01 | HIGH | ◐ core 就绪 | 触发末端乘 triggerChance（`trigger_chance_multiplier`）；真正生效待 build 层注入触发上下文 |
| 03-02 | HIGH | ◐ core 就绪 | 新增 `TriggerSourceRate` 注入通道（PoB2 EffectiveSourceRate）；待 build 层接线 |
| 04-01 | HIGH | ✅ 已修复 | `scale_with_path` 补 Min/Max`<Type>`Damage 分 min/max MORE 乘区 + parser 名映射 |
| 05-01 | HIGH | ◐ core 就绪 | `ailment_crit_chance` over-stacking 公式 + 去 stack_potential clamp；放大待 05-04 活跃叠层模型 |
| 05-02 | HIGH | ✅ 已修复 | 异常 magnitude 从 PoE1 幻影名集改为 PoE2 `AilmentMagnitude` |
| 06-01 | HIGH | ✅ 已修复 | EHP/max-hit 复用 `taken_mult_for_type`，计入 DamageTakenWhenHit |
| 06-02 | HIGH | ✅ 已修复 | 元素走护甲时 armour DR 改用 raw（抗性前）伤害 |

> 图例：✅ 完整修复（现网即生效）；◐ core 侧已就绪（公式/通道已对齐 PoB2，因当前模型或 build 层未注入而暂为 no-op，待对应依赖落地即生效）。
> **注意**：03-01/03-02 需 build/orchestrator 层注入触发数据（源速率/触发上下文）才在真实 build 生效；05-01 需 05-04 活跃叠层模型才产生 over-stacking 放大。三者均向后兼容、对现网输出零影响（已分别补正向回归测试锁定 core 侧行为）。
> CRITICAL + 全部 10 条 HIGH 均已落地；其余 MEDIUM/LOW（含触发链路的 build 层接线、05-04 活跃叠层）留待后续波次。

## 总表

| ID | 子系统 | severity | 标题 | Rust 位置 |
|----|--------|----------|------|-----------|
| 01-01 | Mod 解析与 ModDb 聚合 | HIGH | MORE 聚合缺少 PoB2 的逐-mod round(modResult, 2) 精度归一 | `crates/pobr-core/src/mod_db.rs:161` |
| 01-02 | Mod 解析与 ModDb 聚合 | HIGH | ModFlags 匹配用 intersects（任一重叠），PoB2 要求 mod.flags 是 cfg.flags 子集 | `crates/pobr-core/src/modifier.rs:137` |
| 01-03 | Mod 解析与 ModDb 聚合 | MEDIUM | PerStat/Multiplier 缺少 m_floor(base/div+0.0001) 整数化 | `crates/pobr-core/src/modifier.rs:174` |
| 01-04 | Mod 解析与 ModDb 聚合 | MEDIUM | KeywordFlags 不支持 MatchAll，仅实现默认 any 语义 | `crates/pobr-core/src/modifier.rs:141` |
| 01-05 | Mod 解析与 ModDb 聚合 | LOW | GetMultiplier 不消费 modDB 内的 Multiplier:X，依赖编排层预解析 | `crates/pobr-core/src/config.rs:104` |
| 01-06 | Mod 解析与 ModDb 聚合 | LOW | 条件默认状态全部为 false，未对齐 PoB2 ConfigOptions 的 defaultState | `crates/pobr-core/src/config.rs:100` |
| 02-01 ✅ | 环境/来源装配 (Setup) | CRITICAL | Boss 元素穿透（Pinnacle +3% / Uber +8%）完全未注入玩家 modDB（**已修复 2026-06-09**） | `crates/pobr-core/src/calc/setup_env.rs:58` |
| 02-02 | 环境/来源装配 (Setup) | HIGH | setup_enemy 直接覆写 env.enemy，破坏 enemyDB:AddList 增量装配语义 | `crates/pobr-core/src/calc/setup_env.rs:77` |
| 02-03 | 环境/来源装配 (Setup) | MEDIUM | Boss 自身 debuff-抗 mod 缺少 Condition:Effective 门控 | `crates/pobr-core/src/calc/setup_env.rs:141` |
| 02-04 | 环境/来源装配 (Setup) | MEDIUM | Standard Boss +30 元素抗被硬注入为不可覆盖 BASE（应为 UI 占位） | `crates/pobr-core/src/calc/setup_env.rs:96` |
| 02-05 | 环境/来源装配 (Setup) | MEDIUM | item quality 局部 more 映射过粗：防具应分 armour/evasion/ES；首饰/腰带未建模 | `crates/pobr-core/src/item.rs:174` |
| 02-06 | 环境/来源装配 (Setup) | LOW | 支援宝石 more 隔离仅按 SkillTypes 交集，未覆盖 exclude/add skill types | `crates/pobr-core/src/skill_source.rs:379` |
| 03-01 | 编排 (Perform/Trigger) | HIGH | 冷却驱动触发未走 rotation 模拟、未乘 triggerChance | `crates/pobr-core/src/calc/perform.rs:393` |
| 03-02 | 编排 (Perform/Trigger) | HIGH | 触发 source_rate 误用被触发技能自身速率；build 层未注入触发数据致输出恒为 0 | `crates/pobr-core/src/calc/perform.rs:389` |
| 03-03 | 编排 (Perform/Trigger) | MEDIUM | 技能速率 ActionSpeed 缺 floor/cap 与 TemporalChains 分离，且对攻击技能无条件施加 | `crates/pobr-core/src/calc/skill_use_time.rs:88` |
| 03-04 | 编排 (Perform/Trigger) | MEDIUM | 服务器帧速率上限缺 Repeats 因子，且 DPS 路径完全未施加帧 cap | `crates/pobr-core/src/calc/skill_use_time.rs:104` |
| 03-05 | 编排 (Perform/Trigger) | LOW | AoE 计算把 BASE AreaOfEffect 加入基础半径，PoB2 calcRadius 无此项 | `crates/pobr-core/src/calc/skill_mechanics.rs:74` |
| 03-06 | 编排 (Perform/Trigger) | LOW | CWC 分支 skill_trigger_rate 直接取 cap，未经 calcMultiSpellRotationImpact | `crates/pobr-core/src/calc/perform.rs:401` |
| 04-01 | 伤害核心 (转换/gain/inc-more) | HIGH | 缺失 Min<Type>Damage / Max<Type>Damage 的分 min/max MORE 乘区 | `crates/pobr-core/src/calc/damage.rs:250` |
| 04-02 | 伤害核心 (转换/gain/inc-more) | MEDIUM | gain-as-extra 源量在「base 被完全转换走」时错误回退到原始 base | `crates/pobr-core/src/calc/damage.rs:557` |
| 04-03 | 伤害核心 (转换/gain/inc-more) | LOW | AttackDamage/SpellDamage 用独立 ModName，而非 PoB2 的 Damage + ModFlag | `crates/pobr-core/src/calc/damage.rs:269` |
| 04-04 | 伤害核心 (转换/gain/inc-more) | LOW | per-type base 用 round，PoB2 gain 源用 floor | `crates/pobr-core/src/calc/damage.rs:587` |
| 04-05 | 伤害核心 (转换/gain/inc-more) | INFO | base 附加伤害未计入敌人 Self<Type>Min/Max | `crates/pobr-core/src/calc/damage.rs:158` |
| 05-01 | 命中/暴击/异常/DPS | HIGH | 异常暴击加权用裸暴击率，未做 over-stacking 修正（ailmentCritChance） | `crates/pobr-core/src/calc/ailment.rs:85` |
| 05-02 | 命中/暴击/异常/DPS | HIGH | 异常 magnitude 既叠 DoT 词条 inc/more 又叠 AilmentEffect，存在双重/错位计数 | `crates/pobr-core/src/calc/ailment.rs:142` |
| 05-03 | 命中/暴击/异常/DPS | MEDIUM | 感电效果遗漏 AilmentMagnitude/EnemyShockMagnitude 的 effectMod | `crates/pobr-core/src/calc/ailment.rs:270` |
| 05-04 | 命中/暴击/异常/DPS | MEDIUM | 异常 base 命中未用 RollAverage（叠层位移滚动均值），只取 min/max 中点 | `crates/pobr-core/src/calc/ailment.rs:1210` |
| 05-05 | 命中/暴击/异常/DPS | MEDIUM | traced DPS 路径未扣敌方格挡/未做暴击命中降级/未做分类型减伤，与主路径分叉 | `crates/pobr-core/src/calc/offence.rs:541` |
| 05-06 | 命中/暴击/异常/DPS | LOW | flat_chance 对流血/中毒缺敌方 Self*Chance 的 inc/more（与 threshold 路径不对称） | `crates/pobr-core/src/calc/ailment.rs:510` |
| 05-07 | 命中/暴击/异常/DPS | LOW | DotDpsCap 的 Override 读取用 Sum(BASE) 近似，未走真正 Override 语义 | `crates/pobr-core/src/calc/ailment.rs:1275` |
| 06-01 | 防御与 EHP | HIGH | EHP/max-hit 漏算 DamageTakenWhenHit 承受乘数（受击专属减伤被忽略） | `crates/pobr-core/src/calc/perform.rs:209` |
| 06-02 | 防御与 EHP | HIGH | 元素走护甲时 armour DR 用 post-resist 伤害，PoB2 用 raw（pre-resist） | `crates/pobr-core/src/calc/ehp.rs:109` |
| 06-03 | 防御与 EHP | MEDIUM | ES 充能延迟把 EnergyShieldRechargeFaster 的 BASE(秒) 与 INC(%) 来源混用 | `crates/pobr-core/src/calc/defence.rs:295` |
| 06-04 | 防御与 EHP | MEDIUM | 物理/各类型 DR 上限硬编码 0.9，未读 DamageReductionMax 词条 | `crates/pobr-core/src/calc/ehp.rs:51` |
| 06-05 | 防御与 EHP | MEDIUM | Flat 伤害减免漏 <Type>DamageReductionWhenHit 与 ElementalDamageReduction | `crates/pobr-core/src/calc/perform.rs:669` |
| 06-06 | 防御与 EHP | LOW | TakenMultiSuite 持续/反射上下文与 PoB2 三分法未完全对齐 | `crates/pobr-core/src/calc/defence.rs:476` |
| 06-07 | 防御与 EHP | INFO | Leech 面板速率为近似模型，非 PoB2 LeechRateBase × instances | `crates/pobr-core/src/calc/survivability.rs:416` |

## Severity 计数

| Severity | 数量 |
|----------|------|
| CRITICAL | 1 |
| HIGH | 10 |
| MEDIUM | 14 |
| LOW | 10 |
| INFO | 2 |
| **合计** | **37** |
