# 计算编排（Setup/Perform/环境）

> 重架构审计 · 2026-06-10 · 领域 11
> 对照对象：PoB2（vendor/PathOfBuilding-PoE2/src）Modules/Calcs.lua + CalcSetup.lua + CalcPerform.lua + CalcTools.lua
> pobr 对应层：`crates/pobr-build/src/calc_orchestrator.rs` + `crates/pobr-core/src/calc/{perform,env,actor,setup_env,session}.rs`
> 本报告各条缺口均已逐一打开两侧源码核查（见附录），且与 `audits/pob2-parity-2026-06-09/FINDINGS.md` 的 37 条已知 finding 不重叠。

---

## PoB2 代码结构（结构地图）

PoB2 该领域由 4 个文件组成，总规模约 300KB / 5500 行。

| 文件 | 规模 | 职责 |
|------|------|------|
| `Modules/Calcs.lua` | 34KB | 顶层调度：buildOutput / FullDPS 多技能合并 / 增量重算闭包 |
| `Modules/CalcSetup.lua` | 89KB | `initEnv` 环境装配：actor、buffMode、基础 modDB 注入、来源收集 |
| `Modules/CalcPerform.lua` | 168KB / 3440 行 | `perform`：环境终结（buff/curse/charge/misc 持续写 modDB）+ defence→offence 编排 |
| `Modules/CalcTools.lua` | 10KB | 纯工具：`calcLib.mod/val`、宝石等级校验、stat 插值 |

### Calcs.lua（顶层调度）

- `calcs.buildOutput(build, mode)`（L396）是 UI 入口：`initEnv → calcs.perform(env)` 得主技能输出，再经 `calcs.calcFullDPS`（L152）对 FullDPS 列表里每个技能各起一个 fullEnv（`buildActiveSkill` L370，靠 `GlobalCache.cachedData[mode][uuid]` 复用）合并多技能 DPS。
- MAIN 模式下还扫描全部 mod 生成 `conditionsUsed/multipliersUsed/perStatsUsed`（L463-605，驱动 Config 页只显示相关选项）。
- `getCalculator/getNodeCalculator/getMiscCalculator`（L73-150）提供增量重算闭包（搭配 CalcSetup 的 `wipeEnv`(L327) + `specCopy` parent-链缓存），支撑天赋节点 hover 预览。

### CalcSetup.lua（initEnv 环境装配）

`calcs.initEnv` 按顺序：

1. 建 env + player/enemy 双 actor（actor 互持 `enemy` 引用）；
2. buffMode 三态 → `mode_buffs/mode_combat/mode_effective`（L582-605）；
3. **基础 modDB 注入块**（L608-684，约 70 条 NewMod）：职业 Str/Dex/Int、Life/Mana per level、Accuracy per level、抗性惩罚 -60、Totem 抗性 40/20、MaximumRage/Fortification、EnemyCurseLimit/MarkLimit、ProjectileCount 1、Tailwind/SoulEater/Rampage、重击积累 MORE 等——大半由 `data.characterConstants/gameConstants` 驱动；
4. enemyDB 初始（L685-687：monsterAccuracyTable + `Condition:AgainstDamageOverTime`）+ configTab/partyTab modList 增量 AddList；
5. 天赋节点 modList、装备 modList/radius jewel、宝石 → activeSkillList、flask/charm 收集（`env.flasks/env.charms` L560-561）。

### CalcPerform.lua（perform 阶段顺序）

`calcs.perform(env, skipEHP)`（L955）阶段树：

```
perform(env)
├─ mergeKeystones（L961，第一次）
├─ minion modDB 初始化 + 玩家跨通道注入
├─ applyEnemyModifiers（L1107-1111：player/minion/enemy 各转发一次 "EnemyModifier" LIST → enemyDB）
├─ Banner / Warcry
├─ Mageblood 特判（L1386）→ flask/charm 合并（L1429-1663，mergeFlasks/mergeCharms L1657-1658）
├─ mergeKeystones（L1661 后，第二次：flask 授予的 keystone）
├─ doActorAttribsConditions（player/minion；函数体 L137-453）
│   ├─ 武器/护甲槽条件：UsingShield/UsingHelmet/DualWielding/OffHandIsEmpty 等
│   ├─ mode_combat 条件自动置位（L242-260：AttackedRecently 等）
│   ├─ 属性双 pass 计算（L380-443，"Calculate twice because of circular dependency"）
│   └─ Presence（L444）
├─ herald/aura 计数
├─ Combine buffs/debuffs（L1831-2945）
│   ├─ 遍历 activeSkillList 按 buff.type 九类分发：Buff/Guard/Warcry/Aura/AuraDebuff/Debuff/Curse/CurseBuff/Link
│   ├─ aura 自身效果乘 (1+Σ AuraEffect..AuraEffectOnSelf/100)×More×Magnitude（L2103-2105）
│   ├─ 党员 allyBuffs 取强
│   └─ curse：determineCursePriority（L454）+ limit（L2829-2833）+ mark/hex 槽位合并（L2835+）
├─ Apply buff/debuff modifiers（L2947-2984：buffs→modDB、minionBuffs→minionDB、debuffs/curseSlots→enemyDB）
├─ extra auras / AffectedByAuraMod
├─ mergeKeystones（L3055，第三次：buff 授予的 keystone）
├─ doActorCharges + doActorMisc（L3068-3074）
│   └─ doActorMisc（L503-765）：内建 buff 语义表——Fortify(L523-539)/Onslaught(L541-573)/
│      Fanaticism/UnholyMight(L577-581)/Adrenaline(L589-596)/Convergence(L597-600)…
│      flag→具体 mod 展开，整段由 mode_combat 门控（L510）
├─ 非伤害异常施加到 enemyDB（L3076-3180）
│   └─ Chill/Shock 取 max → 精度截断（L3164-3165）→ 生成 mod 写 enemyDB（L3166-3167）
│      → Condition:AlreadyX 防 minion 双重施加（L3169）→ Multiplier:ChillEffect/ShockEffect（L3173-3180）
├─ enemy 侧 charges/misc（L3183-3184）
├─ calcs.defence（L3192）→ Apply exposures（L3214-3247）→ buildDefenceEstimations（L3249）→ calcs.offence（L3255）
├─ minion 的 defence/offence（L3259-3264）
└─ party 导出 / 宝石等级 / 缓存
```

公共工具：`calcs.actionSpeedMod`（L922-944）是带 TemporalChains cap 与上下限的行动速度公式。

**数据流总括**：initEnv 装配（静态来源→modDB）→ perform 前半段是**环境终结**（buff/curse/charge/misc 继续写 modDB/enemyDB）→ 后半段固定顺序 defence→exposure→EHP→offence 读取聚合。

---

## pobr 实现现状

pobr 把 PoB2「initEnv + perform 前半段」的职责拆到了 build 层：

- **`crates/pobr-build/src/calc_orchestrator.rs`（2600+ 行）** 做来源装配：character base（:1742）、武器贡献（含 unarmed_contribution :1255）、装备防御、宝石/辅助 stat 映射、天赋、aura 静态注入 `aura_buff_modifiers`（:1638）、触发词条注入、属性/资源 multiplier 单 pass 回填（:555-565）、武器/持握条件 `weapon_type_conditions`（:1008，含 DualWielding 派生 :1062）、条件蕴含 `apply_condition_implications`（:994）。经 `pobr-core::calc::session::CalculationSession`（纯来源注入 + cfg 回填入口）落进单一 player ModDb。

- **core 侧编排 `perform.rs`（817 行）**：充能 multiplier → `calculate_minimal_vs_enemy`（offence，含 life/抗性/命中/DPS，:55）→ `calc_defence`（:58）→ `fill_mechanics`（技能时间/EHP/预留/恢复/格挡/ES 充能/规避/承伤/暴减/充能/偷取/recoup/技能机制/触发）→ `fill_ailments`（双 pass 异常 + 叠层）→ `perform_minions`（每个召唤物独立 Actor 复用同管线，跨 actor trace 边）。

- **`env.rs`**：`Env{player, enemy, cfg, minions}`；**`actor.rs`**：`Actor{mod_db, level, base, output, breakdown}`（无 PoB2 的 actor 互持 enemy 引用 / itemList / activeSkillList）。

- **`setup_env.rs`（271 行）** 只做敌人装配：档位缩放、Boss debuff 抗（已带 `Condition:Effective` 门控）、boss 穿透注玩家、曝光归约——上轮审计 02-01/02-02/02-03 已修复。

**总体评价**：pobr 是单技能、单 pass、读侧聚合的「静态装配 → 纯函数 fill」模型，与 PoB2「perform 期间持续写 modDB 的环境终结」模型是结构性差异。具体缺位：buff/curse 编排阶段、doActorMisc 内建 buff 表、非伤害异常 enemy 侧施加、flask/charm（`xml_build.rs:680` 显式忽略 Charm/Flask 槽）、party、FullDPS 多技能框架均未落地；mode 只有 `mode_effective` 一维（`config.rs:27`）。

---

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|------|--------|------|-----------|-----------|------|
| 1 | buff/aura/curse 的 perform 编排阶段整体缺失（含 curse 系统） | 🔴 high | missing | CalcPerform.lua:1831-2984（九类分发；aura 乘区 L2103-2105；determineCursePriority L454；curse limit L2829-2833） | calc_orchestrator.rs:1638 `aura_buff_modifiers`（静态原值直注）；curse 全仓无对应 | aura 效果乘区与诅咒系统完全缺位，影响面最大 |
| 2 | doActorMisc 内建 buff 语义表缺失（Onslaught/Fortify/Adrenaline 等） | 🔴 high | missing | CalcPerform.lua:503-765（flag→mod 展开 + BuffEffectOnSelf 缩放；mode_combat 门控 L510） | 无（src 零命中，仅测试手工构造） | buff flag 无消费者，数值增益静默丢失 |
| 3 | 非伤害异常（Chill/Shock）对 enemyDB 的施加与 Current/Maximum 口径缺失 | 🟡 medium | partial | CalcPerform.lua:3076-3180（取 max→截断→写 enemyDB→AlreadyX→Multiplier 更新） | perform.rs:695-706 `fill_ailments`（仅写面板字段 `output.shock_effect`） | 感电增伤乘区在 offence 路径无任何消费点 |
| 4 | 阶段顺序与 PoB2 相反：先 offence 后 defence，无 defence→offence 数据依赖通道 | 🟡 medium | design | CalcPerform.lua:3192-3255（defence→exposure→EHP→offence） | perform.rs:55-58（offence :55 先于 defence :58） | PerStat 词条只能一阶近似；exposure 顺序契约靠文档 |
| 5 | buffMode 三态（mode_buffs/mode_combat）缺失，只有 mode_effective 一维 | 🟡 medium | partial | CalcSetup.lua:582-605；CalcPerform.lua:242-260 / L510 | config.rs:27（仅 mode_effective）；AttackedRecently src 零命中 | 战斗条件自动置位与口径切换无法表达 |
| 6 | EnemyModifier LIST 转发通道缺失 | 🟡 medium | missing | CalcPerform.lua:486-500 `applyEnemyModifiers` + 调用点 L1107-1111 | 无（全仓仅 minion.rs:213 注释提及） | 装备/天赋上的敌方 debuff 词条全部无效 |
| 7 | initEnv 基础 modDB 注入块仅小部分对齐（~70 条 base mod 大半缺失） | 🟡 medium | partial | CalcSetup.lua:608-684 + enemyDB 初始 L685-687 | character.rs 硬编码常量 + calc_orchestrator.rs:1742 | Totem 抗性/MaximumRage/EnemyCurseLimit/ProjectileCount 基线零命中 |
| 8 | doActorAttribsConditions 条件派生不全 + 属性单 pass 无迭代 | 🟡 medium | partial | CalcPerform.lua:137-453（护甲槽条件 L190-195；属性双 pass L380-443） | calc_orchestrator.rs:1008/:994/:555（DualWielding 已派生 :1058-1063） | 护甲槽/属性比较条件缺；属性互喂只一阶收敛 |
| 9 | flask/charm 合并阶段缺失 | 🟡 medium | missing | CalcPerform.lua:1386/1429-1663；CalcSetup.lua:560-561 | 无计算路径；xml_build.rs:680 显式丢弃 Charm/Flask 槽 | flask/charm 完全不进计算 |
| 10 | 多技能 FullDPS / GlobalCache / 增量 calculator 框架缺失 | 🟢 low | missing | Calcs.lua:73-150/152-368/370-393；CalcSetup.lua:327 wipeEnv | calc_orchestrator.rs（单主技能，无 per-skill env 缓存） | 不影响单技能 parity，但是性能叙事主场景 |
| 11 | mergeKeystones 二次合并（buff/flask 授予的 keystone）缺失 | 🟢 low | missing | CalcPerform.lua:66-79 + 调用点 L961/L1661/L3055 | 无（passive ingest 一次性） | 词条授予 keystone 的通路不存在 |

**统计：🔴 high ×2 · 🟡 medium ×7 · 🟢 low ×2**

---

## 缺口详述

### Gap 1 🔴 buff/aura/curse 的 perform 编排阶段整体缺失（含 curse 系统）

- **PoB2 证据**：CalcPerform.lua:1831-2984。每个 active skill 的 buff 按 buff.type 九类分发（Buff/Guard/Warcry/Aura/AuraDebuff/Debuff/Curse/CurseBuff/Link）。aura 自身要乘 `(1 + Σ(AuraEffect + AuraEffectOnSelf + BuffEffect…)/100) × More × Magnitude` 效果乘区（L2103-2105）后才 merge 进 modDB，并与 allyBuffs 取强、置 AffectedByAura 系列条件；curse 走 `determineCursePriority`（L454）优先级排序 + EnemyCurseLimit/MarkLimit 截断（L2829-2833）+ CurseEffect 缩放后写 enemyDB；最后 Apply 阶段（L2947-2984）统一落库。
- **pobr 现状**：`calc_orchestrator.rs:1638 aura_buff_modifiers` 把 granted-effect stat **原值**逐条 `Modifier::number` 直接注入玩家 db，无任何效果乘区；grep `CurseEffect` 全仓仅 `setup_env.rs:192`（boss CurseEffectOnSelf）与测试。
- **影响**：任何 `% increased Aura Effect`（天赋大点常见）/ `BuffEffectOnSelf` 词条对 aura 数值零作用；诅咒类技能（Despair/Enfeeble 等）对敌 DPS 完全无贡献，诅咒上限/互斥也无从表达。这是当前编排域对真实 build 影响面最大的缺口。
- **修复方向**：在 core 层引入「buff 编排阶段」——active skill 携带 buff 描述（类型/Magnitude/作用对象），perform 在 offence/defence 之前按类型分发：aura 路径计算效果乘区后注入 player db（带归因 SourceId）；curse 路径排序 + limit 截断后注入 enemy db。buff 描述本身应来自 granted_effect 数据（见数据切分节）。

### Gap 2 🔴 doActorMisc 内建 buff 语义表缺失

- **PoB2 证据**：CalcPerform.lua:503-765。config/词条置起的 buff flag（`Flag:Onslaught` 等）由 doActorMisc 统一展开成具体 modifier 并乘 `BuffEffectOnSelf`。例如 Onslaught（L541-573）→ Speed INC 2×effect(Attack) + 2×effect(Cast) + WarcrySpeed + MovementSpeed INC effect，其中 `effect = floor(10 × (1 + OnslaughtEffect + BuffEffectOnSelf))`；Fortify（L523-539）→ DamageTakenWhenHit MORE `-floor(effectScale × stacks)`；另有 UnholyMight（L577-581）、Adrenaline（L589-596）、Convergence（L597-600）、Tailwind/Elusive 等。整段由 `env.mode_combat` 门控（L510）。
- **pobr 现状**：grep `Onslaught/Fortif/Adrenaline` 在 crates/apps/tools 的 src 零命中；仅 `tests/mod_db_traced.rs:30` 与 `tests/defence_ext.rs:285` 把它们当普通 mod 文本手工构造（验证的是消费端，不是展开端）。
- **影响**：即使 mod_parser 解析出 `You have Onslaught` 类 flag，也没有消费者把它变成具体数值 mod；Fortify 的承伤减免、Adrenaline 的全套增益同理静默丢失。带这类机制的 build 进攻/防御都会系统性偏低。
- **修复方向**：实现 perform 期的 misc-buff 展开阶段：读取 flag → 查 buff 定义表 → 按 `BuffEffectOnSelf` 缩放 → 注入 mod（带 buff 名作 SourceId）。buff 定义表强烈建议直接做成 `buff_definitions.json`（见数据切分节），避免重走 PoB2 的 260 行 if-chain 老路。

### Gap 3 🟡 非伤害异常（Chill/Shock）对 enemyDB 的施加缺失

- **PoB2 证据**：CalcPerform.lua:3076-3180。ailment 的 Val/Base/Override/Minimum 汇总 → `output.CurrentX = floor(min(max(override, ΣXVal), MaximumX) × 10^precision)/10^precision`（L3164-3165）→ 生成 ActionSpeed/DamageTaken mod 写 enemyDB（L3166-3167）→ `Condition:AlreadyX` 防 minion 双重施加（L3169）→ `Multiplier:ChillEffect/ShockEffect` 更新（L3173-3180）。
- **pobr 现状**（grep 实证）：`perform.rs:695-706 fill_ailments` 中 shock magnitude 只落到玩家面板字段 `output.shock_effect`；offence/damage 路径**没有任何**把感电幅度折进敌方承伤的消费点（mod_parser 把 `against shocked enemies` 解析为 EnemyShocked 条件 tag 属于另一类玩家词条门控，不是幅度增伤）。
- **影响**：感电=DamageTaken INC、冰缓=ActionSpeed INC 的乘区整体缺失；config「敌人已被感电 X%」与技能自施加的取强合并、Maximum clamp、防重复施加都没有建模。感电 build 的有效 DPS 会整体缺掉感电增伤乘区。
- **修复方向**：在 fill_ailments 之后（或拆出独立阶段）把最强 ailment 幅度按 PoB2 口径（max → clamp → 截断）转为 enemy db 的 modifier，由 offence 的敌方承伤乘区自然消费；default/max/precision 参数入 `non_damaging_ailments.json`。注意 pobr 当前 offence 先于 ailment（见 Gap 4），需要一并调整阶段顺序。

### Gap 4 🟡 阶段顺序与 PoB2 相反，无 defence→offence 数据依赖通道

- **PoB2 证据**：CalcPerform.lua:3192-3255 固定顺序 `calcs.defence`（L3192）→ Apply exposures（L3214）→ buildDefenceEstimations（L3249）→ `calcs.offence`（L3255）。防御先算，offence 可直接读 output.Life/ES/Mana（PerStat 词条如 "per 100 maximum Life"、leech 上限等依赖它）；曝光在 defence 后、offence 前折入敌抗。
- **pobr 现状**：`perform.rs:55-58` 中 `calculate_minimal_vs_enemy`（offence）先行（:55），`calc_defence` 在后（:58）。这些依赖被改为 build 层单 pass 预回填 `cfg.multipliers`（calc_orchestrator.rs:555-565 set_multiplier Strength/Life/Mana…）。
- **影响**：任何在 perform 期间才能确定的池子值（buff/charge 改属性后的 Life）无法被 PerStat 词条引用，存在一阶近似误差；exposure 要求调用方在 perform 前手动 `apply_enemy_exposure`（session.rs:145 文档注明「须在 setup_enemy 之后调用」），顺序契约靠文档而非编排保证。
- **修复方向**：把 minimal 拆为 **pools → defence → offence** 三段以对齐 PoB2 依赖方向；exposure 折算收进 perform 编排内部，消除手动调用契约。这一改动也是 Gap 1/2/3 落地的前置（它们都需要在 offence 之前有「环境终结」阶段位）。

### Gap 5 🟡 buffMode 三态缺失

- **PoB2 证据**：CalcSetup.lua:582-605，EFFECTIVE/COMBAT/BUFFED/NONE 四档映射到 `mode_buffs/mode_combat/mode_effective` 三个独立布尔。消费侧如 CalcPerform.lua:242-260（AttackedRecently/CastSpellRecently/UsedMovementSkillRecently 由主技能 skillFlags 自动置真）、L510（doActorMisc 整段门控）。CALCS 页可独立切换四档口径。
- **pobr 现状**：`config.rs:27` 仅 `mode_effective` 一个开关；grep `AttackedRecently` 在 src 零命中。
- **影响**：未来落地 buff/curse 编排时无法表达「有 buff 无战斗条件」等口径；`if you've attacked recently` 这类大量词条没有自动置位机制。
- **修复方向**：CalcConfig 增加 `mode_buffs/mode_combat` 两个布尔；perform 的 buff/charge/misc 阶段按位门控；主技能 skill_types 派生战斗条件（attack→AttackedRecently 等）在 cfg 回填时自动置位。

### Gap 6 🟡 EnemyModifier LIST 转发通道缺失

- **PoB2 证据**：CalcPerform.lua:486-500 `applyEnemyModifiers`——`Tabulate "EnemyModifier"` → `enemyDB:AddMod`，带 appliedEnemyModifiers 去重缓存；调用点 L1107-1111 对 player/minion/enemy 三方各转发一次。
- **pobr 现状**：grep `EnemyModifier` 全仓仅 `pobr-data/src/minion.rs:213` 文档注释，core/build 编排零消费。
- **影响**：PoB2 把「Enemies have/take …」类词条统一解析为携带 EnemyModifier LIST 的玩家 mod，perform 开头批量转发进 enemyDB。pobr 没有这个通道，敌侧 mod 只能由 setup_enemy/apply_enemy_exposure 等专用入口注入——装备/天赋上很常见的敌方 debuff 词条（-res、增加承伤）全部无效。
- **修复方向**：mod_parser 解析敌方向词条为带 `EnemyModifier` 语义的 list mod；perform 开头加转发 pass（player db → enemy db，保留原 SourceId 归因），minion 侧同理。

### Gap 7 🟡 initEnv 基础 modDB 注入块仅小部分对齐

- **PoB2 证据**：CalcSetup.lua:608-684 约 70 条 NewMod：Life/Mana per level、ManaRegen PerStat、Accuracy per level、TotemFire/Cold/Lightning 40 + Chaos 20 Resist、MaximumRage/Fortification、EnemyCurseLimit/MarkLimit、ProjectileCount BASE 1、Physical/EnemyHeavyStunBuildup MORE、Tailwind/SoulEater/Rampage per-multiplier、WeaponSwapSpeed；enemyDB 初始在 L685-687（monsterAccuracyTable + Condition:AgainstDamageOverTime）。
- **pobr 现状**：核心几条已对齐（commit f23e88f：属性/等级派生，硬编码在 `pobr-core/src/character.rs` 的 LIFE_PER_LEVEL/MANA_PER_LEVEL/ACCURACY_PER_LEVEL 等常量 + calc_orchestrator.rs:1742 character_base）。但 grep 确认 TotemFireResist/MaximumRage/MaximumFortification/EnemyCurseLimit 在 src 零命中；ProjectileCount 有消费端（skill_mechanics.rs calc_projectile_count）但 build 层无 BASE 1 基线注入（仅测试手工注入）。
- **影响**：这些「角色固有基线」是后续 totem/rage/fortify/诅咒上限/投射物机制的输入基线，缺了会让对应机制即使实现了也拿不到默认值。且 PoB2 这些值来自 `data.characterConstants/gameConstants`（游戏数据），pobr 现在硬编码在 Rust 常量里，违背本项目数据/框架分离目标。
- **修复方向**：建 `character_constants.json` + `base_player_mods.json`（见数据切分节），perform/orchestrator 从数据装配玩家基线，逐条补齐 70 条注入表。

### Gap 8 🟡 doActorAttribsConditions 条件派生不全 + 属性单 pass 无迭代

- **PoB2 证据**：CalcPerform.lua:137-453——武器条件（L143-241，含 OffHandIsEmpty/UsingFocus/countsAsAll1H/WieldingDifferentWeaponTypes）；护甲槽条件 UsingHelmet/UsingBodyArmour/UsingGloves/UsingBoots（L190-195）；mode_combat 条件（L242-260）；属性显式双 pass 计算（L380-443，注释 "Calculate twice because of circular dependency"）+ DexHigherThanInt 等比较条件；Presence（L444）。
- **pobr 现状**（核查后修正）：覆盖了 UsingShield、主要武器类型条件、单/双手近战分类；**DualWielding 已派生**（calc_orchestrator.rs:1058-1063，副手为武器基底时置真——原始报告称未派生有误，已修正）；空手有 unarmed_contribution 武器数据。但护甲槽条件、OffHandIsEmpty/UsingFocus、countsAsAll1H/WieldingDifferentWeaponTypes、属性比较条件（DexHigherThanInt/TwoHighestAttributesEqual/LowestAttribute）均未派生；`apply_condition_implications`（:994）仅覆盖 Ignited→Burning、Frozen→Chilled 两条，远少于 ConfigOptions implyCond 全集。
- **影响**：依赖这些条件的词条（如 "while wearing a Helmet"、"if Dexterity is higher than Intelligence"）静默不生效；「+X to Str per Y Int」这类属性互喂词条只能一阶收敛（PoB2 双 pass 才能解环依赖）。
- **修复方向**：补齐槽位/持握/属性比较条件派生（可表驱动）；属性计算改双 pass（第一遍算基础属性 → 回填 multiplier → 第二遍重算），与 Gap 4 的 pools 阶段一并落地。

### Gap 9 🟡 flask/charm 合并阶段缺失

- **PoB2 证据**：CalcPerform.lua:1386（Mageblood 特判）/ 1429-1663（Merge flask modifiers，mergeFlasks/mergeCharms L1657-1658，minion 侧再合并 L2753）；initEnv 收集 `env.flasks/env.charms`（CalcSetup.lua:560-561，收集逻辑 L1015/L1027）。
- **pobr 现状**：无计算路径——grep flask/charm：pobr-core 仅 `mod_parser.rs:998` 的 UsingFlask 条件 tag；`pobr-build/src/xml_build.rs:680` 显式把 Charm/Flask 槽名忽略不入枚举。
- **影响**：PoE2 的 charm/flask（Ruby Flask 类抗性、Silver Flask 类 Onslaught 来源、charm 常驻词条）在 PoB2 经 flask effect 乘区缩放后合入 modDB（doActorMisc 的 Onslaught 还专门读 Silver Flask 的 flaskData.effectInc）。pobr 物品管线只处理装备槽，导入时即丢弃 flask/charm，对走 charm 抗性/增益的 build，防御与 parity 都会偏差。
- **修复方向**：xml_build 保留 Charm/Flask 槽 → item ingest 识别 flask/charm 基底与词条 → perform 新增 merge 阶段（flask effect 乘区缩放 + 激活态门控，配合 mode_combat）。flask/charm 基底数据需要入库。

### Gap 10 🟢 多技能 FullDPS / GlobalCache / 增量 calculator 框架缺失

PoB2 对 FullDPS 列表中每个技能单独起 env 计算再合并（DoT 去重、minion 并入），用 GlobalCache（UUID 索引）复用；天赋树节点 hover 的快速增量重算靠 `wipeEnv` + modDB.parent 链拷贝。pobr 是单技能单 env，多技能合计与节点增益预览都没有框架位。属功能性缺口，不影响单技能 parity，但它是 pobr「性能优势」叙事的主要兑现场景（多技能并行天然适合 Rust 并行化）——建议在 Env 设计期预留 per-skill 只读快照。

### Gap 11 🟢 mergeKeystones 二次合并缺失

PoB2 在 perform 内三次 merge keystone（L961/L1661/L3055，`env.keystonesAdded` 去重）：装备/flask/buff 词条可授予 keystone（如 "You have Iron Reflexes"），授予发生在 buff 应用之后所以需要再 merge。pobr 的天赋 keystone 在 build 层一次注入，词条授予 keystone 的通路（解析出 `Keystone:X` 后查 keystone 定义注入其 mod）不存在。

---

## 数据 vs 逻辑切分建议

这是用户的核心关注点：PoB2 把数据硬编码成 Lua 过程式代码，PoBR 的目标是框架稳定、每个版本只更新 `data/<版本>/*.json`。本领域的切分如下。

### 应 JSON 化的「数据」（随版本更新）

| # | 数据对象 | PoB2 现状 | 建议表 |
|---|----------|-----------|--------|
| 1 | 角色常量（life_per_level、mana_per_level、accuracy_rating_per_level、base_critical_hit_damage_bonus、base_max_fortification、BaseMaximumRage、抗性惩罚、ServerTickRate、MaxEnemyLevel…） | `data.characterConstants/gameConstants/data.misc`，CalcSetup.lua:608-678 消费 | `character_constants.json` |
| 2 | initEnv 基线 mod 注入表（~70 条 NewMod 的 name/type/value/flags/tags） | CalcSetup.lua:608-684 硬编码为 Lua 调用 | `base_player_mods.json` |
| 3 | 等级缩放表（monsterAccuracyTable/monsterArmourTable/monsterEvasionTable/mapLevelLifeMult…） | `data.*` 数组，CalcSetup.lua:685 / CalcPerform 多处消费 | `monster_scaling.json` |
| 4 | 内建 buff 定义（Onslaught/Fortify/Adrenaline/UnholyMight/Convergence/Tailwind… 的 mod 展开表 + 基准幅度 + 是否吃 BuffEffectOnSelf） | doActorMisc 260 行 if-chain（CalcPerform.lua:503-765），**最典型的「数据类代码」** | `buff_definitions.json` |
| 5 | 非伤害异常参数（chill/shock 的 default/max/precision） | `data.nonDamagingAilment`（CalcPerform.lua:674/1162/3077 消费） | `non_damaging_ailments.json` |
| 6 | 召唤物基表（lifeTable 乘数、minionData.modList、playerMinionIntrinsicStats） | CalcPerform.lua:989-1018 消费 | `minions.json` |
| 7 | weaponTypeInfo（武器类型→条件 flag/melee/oneHand/range） | doActorAttribsConditions L157/L198 消费 | `weapon_types.json` |
| 8 | config 项的 defaultState / enemy preset 数值 | ConfigOptions.lua（上轮审计 01-06 已做导入逻辑，数值表仍内嵌） | `config_options.json` |
| 9 | unique 特判（Mageblood L1386 / Dancing Dervish / The Iron Mass…散落 perform 主流程） | 按名字 match 硬编码在编排主流程 | 理想形态：`unique_id → 行为 flag + 参数` 数据 + 少量通用机制逻辑 |

### 留在框架的「逻辑」（跨版本稳定）

- perform 的阶段顺序本身；
- buff 九类分发与效果乘区公式（aura 的 inc/more 组合、ally 取强）；
- curse priority/limit 算法；
- attributes 双 pass 迭代与条件派生规则；
- `actionSpeedMod` 公式（CalcPerform.lua:922-944）；
- `applyEnemyModifiers` 转发机制；
- charges 解析；exposure 取强归约。

这些跨版本稳定，pobr 已正确地把同类内容写成 Rust 纯函数——方向正确，需要补的是上表的数据底材。

### PoB2 是如何混在一起的（反面教材）

CalcSetup/CalcPerform 把上表 1/2/4/9 全部硬编码为过程式 Lua：基线 mod 是 70 行 NewMod 调用、buff 表是 260 行 if-chain、unique 行为按名字 match 散布在编排主流程里——导致每个版本 diff 时数据变更和逻辑变更不可区分。这正是 PoBR 重架构要避免的。

### pobr 自身的违例与 schema 补齐清单

`data/4.5.0.3.4/` 现有 base_items/mods/stats/granted_effect_*/skill_gems/passive_tree/cost_types，对照本领域还缺，且 **pobr 自己也有两处违背数据分离目标的硬编码**需迁出：

- `character_constants.json` —— 当前硬编码在 `pobr-core/src/character.rs` Rust 常量，应迁出；
- `base_player_mods.json` —— 让 perform 从数据装配玩家基线，而非散落在 character.rs + orchestrator；
- `monster_scaling.json` —— 当前在 `pobr-data/src/monster.rs` 以 Rust 数组硬编码，同样应入 `data/<版本>/`；
- `buff_definitions.json` —— 落地 Gap 2（doActorMisc）的前置数据；
- `non_damaging_ailments.json` —— 落地 Gap 3 的前置数据；
- `minions.json` —— minion.rs 已有结构体但底材数据未入库；
- `weapon_types.json`；
- `config_options.json` —— FINDINGS 01-06 已做导入逻辑，数值表仍内嵌。

`catalog.rs` 的 `DataManifest` 机制已就绪，以上各表按域懒加载补进 `pobr-gamedata` 即可，**不需要动框架层**。

---

## 附录：核查说明

核查范围：2 条 high 全部 + 7 条 medium 全部 + 2 条 low 抽查，共 11 条逐一打开两侧源码验证。先读 `audits/pob2-parity-2026-06-09/FINDINGS.md` 确认无重复——本报告各条均不与 37 条已知 finding 重叠（FINDINGS 的 02-xx/03-xx 聚焦 setup_env 敌人装配与触发，未覆盖 buff/curse/doActorMisc/flask 编排域）。

逐条核查结论：

1. **Gap 1 buff/aura/curse（high）**：成立。PoB2 侧 Combine buffs(1831)/Apply(2947)/aura 乘区(2103-2105 实读确认 `(1+inc/100)×more×Magnitude`)/determineCursePriority(454)/curse limit(2829-2833) 全部实存。pobr 侧 aura_buff_modifiers（实际函数定义在 :1638）逐行读过——确为 granted-effect stat 原值直注、无任何 AuraEffect/BuffEffectOnSelf 乘区；grep CurseEffect 全仓仅 setup_env.rs:192（boss 抗性）与测试。保留 high。
2. **Gap 2 doActorMisc（high）**：成立。实读 L503-600，Fortify/Onslaught/Adrenaline/UnholyMight/Convergence 的 flag→mod 展开 + BuffEffectOnSelf 缩放确如描述（行号微调：Fortify L523-539、Onslaught L541-573、Adrenaline L589-596）。pobr 侧 grep 零命中，测试验证的是消费端不是展开端。保留 high。
3. **Gap 3 感电/冰缓（medium）**：成立且比原文更确定。实读 L3155-3185 确认 CurrentX 精度截断/enemyDB AddMod/Condition:AlreadyX(L3169)/Multiplier:ShockEffect 全实存；pobr 侧 grep 确认 shock magnitude 仅写面板字段，offence/damage 无幅度→敌承伤消费点——原 detail 中的犹疑已改为确定性结论。保留 medium。
4. **Gap 4 阶段顺序（medium/design）**：成立。perform.rs:55/58 实证 offence 先于 defence；PoB2 L3192→3214→3249→3255 顺序实证；orchestrator:555-565 预回填、session.rs:145 exposure 手动调用契约均实证。保留。
5. **Gap 5 buffMode（medium）**：成立。CalcSetup L582-605 三态、CalcPerform L242-260 自动置位、L510 门控均实读确认；pobr config.rs 仅 mode_effective、AttackedRecently src 零命中。保留。
6. **Gap 6 EnemyModifier（medium）**：成立。applyEnemyModifiers L486-500 实读（含去重缓存细节）、调用点 L1107-1111 实读；pobr 全仓 grep 仅 minion.rs:213 注释。保留。
7. **Gap 7 initEnv 基线（medium）**：成立。L600-690 整块实读，~70 条 NewMod 确认；pobr 侧 TotemFireResist/MaximumRage/EnemyCurseLimit src 零命中；额外查证 ProjectileCount——有消费端但 build 层无基线注入（仅测试手工注入）。enemyDB 初始行号修正为 L685-687。保留。
8. **Gap 8 条件派生（medium）**：**部分有误，已修正**。原 detail 称「DualWielding 派生族未派生」——实读 calc_orchestrator.rs:1058-1063 证明 DualWielding 本体已派生（副手为武器基底时置真），空手也有 unarmed_contribution 数据；真正缺的是护甲槽条件、OffHandIsEmpty/UsingFocus、countsAsAll1H/WieldingDifferentWeaponTypes、属性比较条件。已改写 detail 并补 PoB2 双 pass 属性计算（L382 "Calculate twice" 实读确认）。severity 维持 medium（缺口仍实质存在）。
9. **Gap 9 flask/charm（medium）**：成立。Mageblood L1386、Merge flask L1429、mergeFlasks/mergeCharms L1657-1658、env.flasks/charms CalcSetup L560-561 全实证；pobr 侧补强证据：xml_build.rs:680 显式忽略 Charm/Flask 槽名。保留。
10. **Gap 10 FullDPS（low，抽查）**：成立。getCalculator L73、calcFullDPS L152、buildActiveSkill L370、wipeEnv CalcSetup:327 全实证。保留。
11. **Gap 11 mergeKeystones（low，抽查）**：成立。函数 L66、调用点 L961/L1661/L3055 实证（原报告 L1660 修正为 L1661）。保留。

**修改汇总**：无删除、无降级；1 条实质修正（Gap 8 的 DualWielding 误判）；3 处行号修正（doActorMisc 各 buff 行号、enemyDB 初始 L685-687、mergeKeystones L1661）；Gap 3 detail 由猜测改为 grep 实证的确定结论；Gap 9 pobr_ref 补 xml_build.rs:680 证据。PoB2 结构图与 pobr 现状文本已同步这些行号与事实修正（aura_buff_modifiers :1638、perform.rs :55/:58、DualWielding 已派生）。
