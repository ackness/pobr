# M3 编排实施蓝图：config / buff / aura / curse / 敌方

> 撰写：2026-06-11 · 规划者只读产出 · 对应 roadmap M3 节（21-roadmap.md）+ 领域审计 11-calc-orchestration.md / 19-config-build-display.md + 架构裁决 20-target-architecture.md（P6/P7/P9/P13、§5 DSL 硬边界）
> **本文自包含**：实施 agent 只需读本蓝图 + 代码即可开工，无需回读 roadmap/审计文档。所有 vendor 行号均已在撰写时实读核对（vendor commit 见 `vendor/.pob2-version.txt`）。

---

## 0. 阶段定位与目标

**一句话**：用 `config_interpreter` + `buff_expander` 两个数据解释器一次性消灭编排层的启发式硬编码；补上 PoB2 perform 前半段「环境终结」（buff/aura/curse/敌方异常持续写 modDB）整个阶段；做第一次 version-bump-drill 演练。

**阶段验收（roadmap 原文）**：

> 进攻 **≥55%** / 防御 **≥85%**；含 aura/curse 的 build 不再系统性偏低；config 导入用真实 XML fixture 回归；**第一次 version-bump-drill 演练**（P18）——发现的"必须改代码"项登记进 M5/M6 清单。

**对应 high 缺口**：11-G1（buff/aura/curse 阶段缺失）、11-G2（doActorMisc 表缺失）、19-G1（ConfigOptions apply 语义）、19-G2（customMods）、14-G6（异常消费闭环）、10-G3（部分：actor 系 tag）。
另捎带：11 号审计 Gap 5（buffMode 三态）、Gap 6（EnemyModifier 通道）、Gap 9（flask/charm 合并）、Gap 11（mergeKeystones 二次合并）、19 号审计 Gap 4（count 型 condition）、Gap 6（implyCond ~60 处）。

**体量**：~4 人周，按 5-agent worktree 并行（T0 地基串行先行 + T1–T5 并行）。

### 0.1 起点假设（实施前必须逐条确认）

本蓝图按「M0 收尾 + M1/M2 已交付」的状态写。开工前用下表自检，未达成项见各 track 的降级预案：

| # | 假设 | 校验方法 | 未达成时 |
|---|------|----------|----------|
| 1 | RuntimeConstants 注入管道已存在：`CalcConfig.constants`（pobr-core/src/config.rs 已有字段）+ `CalculationSession::set_constants`，calc 魔数已改读注入常量（M0-W3） | grep `RuntimeConstants` + 跑 `cargo test -p pobr-core` | 各 track 新代码仍**只准**读 `cfg.constants`，不得新增魔数；缺的常量按 §13 流程补入 `game_constants.json` |
| 2 | `data/<ver>/{base,overlay,generated}` 三层 + manifest v2 + overlay merge（`pobr-gamedata/src/overlay.rs`）+ `GameData::load_ruleset()` 骨架（`ruleset.rs`，`ConfigCatalog` 占位 `None`）已就绪 | 已实读确认存在 | — |
| 3 | `pobr-core/src/rules/{mod.rs,registry.rs}` handler 注册表骨架已就绪（`HandlerRegistry`，签名 `Box<dyn Fn(&[f64]) -> Vec<Modifier>>`，注释明示「后续按需扩上下文参数」） | 已实读确认存在 | — |
| 4 | extract-lua 通道已打样：`tools/sync-pob-catalog/src/extract_lua.rs`（luajit + stdin 引导脚本 + JSONL + Rust 侧排序/byte-stable 组装 + `_meta` 记 vendor commit/regen_command） | 已实读确认存在 | — |
| 5 | `non_damaging_ailments.json` / `enemy_presets.json` / `base_player_mods.json` / `character_constants.json` 等九表已入库（M0） | `ls data/4.5.0.3.4/base/` 已确认 | — |
| 6 | enemyIsBoss / resistancePenalty XML 接线已完成（M0）：`xml_build.rs` parse_config 已识别两项 → `EnemyTier`/`CampaignProgress` | 已实读确认（xml_build.rs:193-205） | — |
| 7 | **M1 交付**：gem quality 链路、`skill_stat_map.json` + `rules/stat_map_engine.rs`、support 适用性、多 statSet；进攻 parity ≥40% | 跑 ninja_parity 看报告 | aura buff 的 stat 提取仍走现 `map_aura_buff_stat` 启发式，T3 的 BuffSpec 提取层多一个适配分支 |
| 8 | **M2 交付**：`calc/pool_damage.rs`、`rules/keystone_registry.rs`、taken-as、防御 parity ≥80% | 同上 | T5 的 mergeKeystones 改为只接「词条→keystone flag」，开关消费留待 keystone_registry 落地；防御 ≥85 目标按 M2 实际终点顺延口径（在 PR 中显式声明） |
| 9 | flask/charm 基底数据列（base_items.json `flask{}`/`charm{}`）随 M1 adapter 扩展落库（16-G4） | `python3` 查 base_items.json 字段 | **撰写时未落**（实查 UtilityFlask/LifeFlask/ManaFlask 条目存在但无 flask 数据列）——T4 自带 adapter 增列工作项（已计入 T4 预估） |

### 0.2 关键 vendor / pobr 代码地图（实施 agent 速查）

| 主题 | vendor 参照 | pobr 现状 |
|------|------------|-----------|
| ConfigOptions 大表 | `Modules/ConfigOptions.lua`（2323 行；**542** 个 `{ var =` 条目、**517** 个 apply 闭包、**60** 行 implyCond——三数均 grep 复核）；section：General(:112)/Skill(:208)/Combat(:796)/EffectiveDPS(:1620)/EnemyStats(:1958)/Custom(:2277)/QuestRewards(:57 动态) | `pobr-build/src/xml_build.rs::parse_config`（:156-247）：condition*(boolean)/use*Charges/DEFAULT_TRUE_CONDITIONS(:123)/multiplier*(number)/enemyIsBoss/resistancePenalty/quest* 分支；count 型 condition、customMods、enemy 数值覆盖、implyCond（仅 orchestrator :988 两条）全缺 |
| apply 模板例 | check 直注 :133-135（conditionMoving→FLAG）；count+clamp :117-119（CurrentManaPercentage `m_max(m_min(val,100),0)`）；count 双 mod :120-131（conditionStationary→Multiplier+条件 FLAG，含 boolean 旧版兼容）；SkillData LIST :114-116；写 enemyModList :1961-1962（带 `Condition:Effective` tag）；customMods :2278-2296（逐行 parseMod）；真逻辑：enemyIsBoss :1963-2120 / presetBossSkills :2170-2249 / questRewards :56-108 | — |
| doActorMisc 内建 buff | `Modules/CalcPerform.lua:503-765`：整段 `env.mode_combat` 门控(:510)；Fortify :523-539（stacks 模型）；Onslaught :541-573（`effect=floor(10×(1+ΣINC(OnslaughtEffect,BuffEffectOnSelf)/100))`→Speed INC 2×e(Attack)+2×e(Cast)+WarcrySpeed 2×e+MovementSpeed e；Silver Flask 特判读 flaskData.effectInc）；Fanaticism :574-580；UnholyMight :581-585（Multiplier:UnholyMightMagnitude 100 + DamageGainAsChaos 0.3×effect per-multiplier）；ChaoticMight；Adrenaline :589-596（Damage INC floor(100e)/Speed 25e/MS 25e/PDR 10e）；Convergence :597-600（ElementalDamage MORE floor(30e)）；HerEmbrace 等后续条目至 :765 | 全仓 src 零命中（buff flag 无消费者） |
| buff/aura/curse 编排 | CalcPerform.lua:1831-2984：九类分发（Buff/Guard/Warcry/Aura/AuraDebuff/Debuff/Curse/CurseBuff/Link）；aura 自身乘区 :2103-2105（`inc=Σ(AuraEffect,BuffEffect,BuffEffectOnSelf,AuraEffectOnSelf,AuraBuffEffect,SkillAuraEffectOnSelf)`，`mult=(1+inc/100)×More(同名集合)×calcLib.mod(Magnitude)`；ScaleAddList(buff.modList, mult)；`AffectedByAura`/`AffectedBy<名>` 条件）；ally 取强（`allyBuffs[...].effectMult/100 <= mult` 才用自己的）；curse priority `determineCursePriority` :454-485（base+socket×SocketPriorityBase+slot+source 四段相加，数据表 `data.cursePriority` 在 `Modules/Data.lua:274`）；curse limit :2829-2833（`EnemyCurseLimit`(+CurseLimitIsMaximumPowerCharges 特例)+`EnemyMarkLimit`，mark/hex 槽位分开填，priority 高者替换）；CurseEffect 缩放（非 mark 乘 enemyDB More CurseEffectOnSelf）；Apply 落库 :2947-2984 | `calc_orchestrator.rs:1633 aura_buff_modifiers`：granted-effect stat **原值直注**玩家 db，零乘区；curse 全仓无 |
| EnemyModifier 转发 | CalcPerform.lua:486-500 `applyEnemyModifiers`（Tabulate "EnemyModifier" LIST → enemyDB:AddMod，appliedEnemyModifiers 去重）；调用点 :1107-1111（player/minion/enemy 三方各一次） | 零实现（mod_parser/mod_db 均无 EnemyModifier） |
| 非伤害异常施加 | CalcPerform.lua:3076-3180：`ShockVal`(enemyDB)/`ShockBase`/`ShockOverride`/`ShockMinimum`(modDB) → magnitude 乘 `Enemy<X>Magnitude`×`AilmentMagnitude`(skill)×`Self<X>Magnitude`(enemyDB) → `Current<X> = floor(min(max(override,ΣVal),Maximum<X>)×10^prec)/10^prec`（prec 来自 `data.nonDamagingAilment`，已入库 `non_damaging_ailments.json`）→ 生成 mod 写 enemyDB（Shock→`DamageTaken INC {Condition:Shocked}`；Chill→`ActionSpeed INC -num {Condition:Chilled}`；Bonechill→ColdDamageTaken）→ `Condition:Already<X>` 防 minion 双重施加 → `Multiplier:ChillEffect/ShockEffect` 取增量更新(:3173-3180) | `perform.rs fill_ailments`（offence 之后）只写面板字段；enemy db 无施加点；offence 的 DamageTaken 消费链**已存在**（offence.rs:685-720，mode_effective 门控） |
| buffMode 三态 | CalcSetup.lua:582-605（EFFECTIVE→buffs+combat+effective；COMBAT→buffs+combat；BUFFED→buffs；NONE 全关）；mode_combat 条件自动置位 CalcPerform.lua:242-260（非 triggered/trap/mine/totem 的主技能：attack→`AttackedRecently`、spell→`CastSpellRecently`、Movement→`UsedMovementSkillRecently`、minion 非 duration→`UsedMinionSkillRecently`、Vaal→`UsedVaalSkillRecently`…） | `config.rs` 仅 `mode_effective` 一维 |
| mergeKeystones | CalcPerform.lua:66-76（Tabulate LIST "Keystone" → keystoneMap 查 modList 注入，`env.keystonesAdded` 去重）；调用点 :961/:1661/:3055（树后/flask 后/buff 后三次） | passive ingest 一次性，词条授予 keystone 通路不存在 |
| flask/charm | CalcSetup.lua:560-561 收集 env.flasks/charms；CalcPerform.lua:1386 Mageblood 特判、:1429-1663 merge（mergeFlasks/mergeCharms :1657-1658） | `xml_build.rs:722` 槽名枚举外显式忽略 Charm/Flask |
| EvalMod actor tag | PoB2 ModStore EvalMod 的 `actor`/`limitActor` tag：把 Multiplier/PerStat 的读取上下文切到 `env.player`/`env.minion`/parent | `modifier.rs ModTag::Multiplier{var,div,limit}` 无 actor 维度；`perform_minions` 已有「玩家值注入 minion cfg」先例（SummonedMinion） |

---

## 1. 总体设计决策（各 track 共同遵守）

### D1 「环境终结」阶段框架（perform 内新增固定阶段位）

PoB2 模型是「perform 前半段持续写 modDB，后半段 defence→offence 只读聚合」。pobr 现为「build 层静态装配 → perform 纯 fill」。M3 在 **pobr-core `perform.rs` 开头、offence/defence 之前**插入一个 `env_finalize(env)` 调度段，固定阶段顺序如下（对照 PoB2 perform 阶段树，省略 M3 不做的 Banner/Warcry/party）：

```
perform(env)
├─ env_finalize(env)                       ← M3 新增（T3 负责框架，各 track 挂阶段）
│   1. merge_keystones(env)                  // T5：词条授予 keystone（含 flask/buff 授予，幂等去重）
│   2. forward_enemy_modifiers(env)          // T3：player(+minion) db 的 EnemyModifier LIST → enemy db
│   3. merge_flasks_charms(env)              // T4：flask/charm 词条按激活配置合入（mode_combat 门控）
│   4. buff_pass(env)                        // T3：九类分发（aura 乘区 / curse priority+limit / debuff→enemy）
│   5. merge_keystones(env)                  // T5：第二次（buff/flask 授予的 keystone）
│   6. expand_misc_buffs(env)                // T2：doActorMisc 等价（flag → buff_definitions → mods）
│   7. apply_nondamaging_ailments(env)       // T4：Chill/Shock → enemy db
├─ （既有）charge multiplier 回填 / ES→Mana / offence / defence / fill_mechanics / fill_ailments / minions
```

要点：
- 每个阶段是 `pub fn xxx(env: &mut Env)` 的**局部纯过程**（只写 `env.player.mod_db`/`env.enemy.mod_db`/`env.cfg.conditions`），不引共享可变状态；写入的 modifier 一律带 `SourceId` 归因（新 SourceKind 见 D4）。
- **不在 M3 调整 offence/defence 先后序**（11 号审计 Gap 4 的 pools→defence→offence 重排属 M2/M4 范畴）；M3 所有新机制都发生在两者之前，与现序无冲突。
- 各阶段默认**空转兼容**：无 buff spec / 无 flask / 无 EnemyModifier 时输出逐值不变（这是各 track 的搬迁不变式锚点）。
- 阶段 1-7 的函数分别住在各 track 独占文件里，`perform.rs` 只有 7 行调用——把共享文件冲突压到最小（见 §2 文件归属表）。

### D2 受限 DSL 硬边界（架构 §5 全文，写入每个 PR 的 review checklist）

config effects / buff 定义的占位符语言硬边界：

- **允许**：`$1..$n`（M3 内 config 只有单输入，记为 `input`）数值占位、字面量、`negate / clamp(min,max) / div / mult / base` 五种算子、`target(player|enemy|minion)`、受限谓词（字段引用 + `eq/ne/gt/lt` + `and/or`）。
- **禁止**：循环、递归、自由表达式、跨条目引用、字符串拼接求值。
- **扩展闸门**：新增任何 DSL 能力需 **≥20 个条目受益**，否则该条目走 handler_id。
- **监控**：handler 条目总数 <100；config 域 handler 逼近 542×10%≈54 即判切分失败、回看裁决 P4/P6（用测试断言锁死，见 T1-A6）。
- **元数据**：未经 oracle 验证的条目带 `verified:false`，运行时照用但 parity 报告单列。

Review checklist（每个含 DSL/数据表的 PR 必须逐项勾选）：
1. [ ] 没有新增 DSL 算子；若有，PR 描述里列出 ≥20 个受益条目。
2. [ ] handler 计数断言测试仍通过（config ≤54 / buff ≤8 / 总数 <100）。
3. [ ] 新表/新条目带 `verified` 字段；oracle 对拍报告附在 PR。
4. [ ] overlay 产物未手改（重跑 extract 命令 byte-diff = 0；人工策展表改了源头 JSON 并更新 `_meta`）。
5. [ ] 注入路径带独立 SourceKind 的 SourceId。
6. [ ] 搬迁 commit 与行为 commit 分离；行为 commit 附 PoB2 行号/oracle 中间值。

### D3 双跑与 baseline 纪律（roadmap §0 原文，逐字适用）

> - **搬迁不变式**：纯搬迁（数据出代码、入 JSON）的 commit，parity baseline **逐值不变**（golden diff = 0）；搬迁与行为改动永远分两个 commit。
> - 行为修复必须附 PoB2 一手依据（源码行号/oracle 中间值）；baseline 更新独立 commit、显式审查。
> - 核心改动（mod_db/ModFlags/stat_map 引擎切换等）feature-gated **双跑对照**，diff 报告干净后才删旧码。

M3 的双跑点（三处）：
1. **config_interpreter vs parse_config 旧路径**（T1-A5）：口径＝「旧路径能产出的 conditions/multipliers/global_texts/标量项，新解释器必须逐值一致（旧⊆新）」；新解释器**多出来**的覆盖（count 型 condition、implyCond、enemy 覆盖、customMods）属行为提升，按独立行为 commit 入，每项附 ConfigOptions.lua 行号。
2. **buff_pass aura 乘区 vs aura_buff_modifiers 静态直注**（T3-C5）：18-build 中无 AuraEffect 词条的 build 必须逐值不变；有 AuraEffect/BuffEffectOnSelf 词条的 build 差异即修复目标，逐 build 列 diff 报告。
3. **ModValue 扩 NestedMods**（T3-C4，动 mod_db 核心载荷类型）：现有全部 mod_db 测试 + `mod_db_bench` 无回归为门禁；该 commit 内禁止任何行为改动。

### D4 归因 SourceKind 扩展（统一在 T0 落，避免各 track 撞 pobr-data）

`pobr-data/src/source.rs` 的 `SourceKind` 新增四个变体（一次性 commit）：`ConfigOption`（"config.<var>"）、`Buff`（"buff.<id>" / "aura.<skill_id>" / "curse.<skill_id>"）、`Flask`（"flask.<slot>"）、`GrantedKeystone`（"keystone.<name>"）。架构通用原则：「所有新数据表注入路径携带 SourceId……数据化反而强化归因粒度」。**M3 禁止改 TraceGraph/归因结构本身**（P17，双 pass RFC 属 M4）。

### D5 buffMode 三态语义（T2 定义，全员消费）

`CalcConfig` 增 `mode_buffs: bool` / `mode_combat: bool`（默认 **false**，与现有 `mode_effective` 默认一致——保证未显式置位的既有调用方逐值不变）。pobr-build 编排入口（`calculate_with_data`）对 MAIN 口径显式置三者为 true（PoB2 非 CALCS 模式恒 EFFECTIVE，CalcSetup.lua:585-590）。门控关系：buff_pass 整体吃 `mode_buffs`；doActorMisc 等价段、战斗条件自动置位、flask/charm 合并吃 `mode_combat`；敌侧 debuff/curse 维持既有 `mode_effective`（`Condition:Effective`）口径。
**parity 注意**：ninja_parity/golden 现走 orchestrator，置 true 后 mode_combat 派生条件（AttackedRecently 等）会让一批词条首次生效——这是**行为 commit**，与字段引入（搬迁 commit，默认 false 零影响）分离。

---

## 2. Track 划分、文件归属、串行序

### 2.1 Track 总览

| Track | 内容 | 预估 | 依赖 |
|-------|------|------|------|
| **T0 接口地基**（串行，先行 2-3 天） | SourceKind 扩展；CalcConfig buffMode 字段（默认 false）；`env_finalize` 空阶段框架；`BuffSpec`/session 注入 API 契约；handler 注册聚合点 | 0.3 人周 | 无 |
| **T1 config 链路** | `config_def.rs` schema + `extract-config-options` 抽取（探针法归纳 apply）+ `config_options.json` + gamedata 装载 + `rules/config_interpreter.rs` + xml_build 切换双跑 | 1.2 人周 | T0 |
| **T2 buff 定义/expander** | `buffs.rs` schema + `buff_definitions.json`（人工归纳 doActorMisc）+ `rules/buff_expander.rs` + mode_combat 条件自动置位 | 0.7 人周 | T0 |
| **T3 buff_pass aura/curse + EnemyModifier** | `calc/buff_pass.rs` 九类分发 + aura 乘区 + curse priority/limit + `curse_priority.json` + EnemyModifier LIST 解析/转发 + 替换 aura_buff_modifiers 直注 | 1.2 人周 | T0；C5 在 T2 合并后收尾 |
| **T4 异常闭环 + flask/charm** | `calc/ailment_apply.rs`（Chill/Shock→enemy db）+ flask/charm 槽位/数据列/merge 阶段 | 0.8 人周 | T0；flask 槽位 patch 排 T1 的 xml_build 重构合并后 |
| **T5 tag 扩展 + mergeKeystones + drill** | `ModTag` actor/limitActor 维度（**先行**，T3 依赖）+ keystone 二次合并 + `version-bump-drill.sh` 第一版 | 0.6 人周 | T0；E1 在 T3 主体前完成 |

### 2.2 文件归属表（独占写权；未列文件 = 不许动）

| 文件/目录 | 归属 | 说明 |
|----------|------|------|
| `crates/pobr-data/src/source.rs`、`config.rs` 的 buffMode 字段、`calc/env_finalize.rs`（新，空框架）、`calc/session.rs` 的 BuffSpec API 段 | **T0** | 地基 commit，T0 合并后冻结接口 |
| `crates/pobr-data/src/catalog/config_def.rs`（新） | T1 | |
| `tools/sync-pob-catalog/src/extract_config_options.lua`（新）、`extract_lua.rs`（扩展段）、`lib.rs`/`main.rs` 子命令注册行 | T1 | main.rs 注册行 append-only |
| `crates/pobr-gamedata/src/`（config 域 loader + `ruleset.rs` 的 `ConfigCatalog` 填充） | T1 | ruleset.rs 其余字段不动 |
| `crates/pobr-core/src/rules/config_interpreter.rs`、`rules/value_expr.rs`（新） | T1 | value_expr 是 config/special(M5b)/parser(M6) 共用的五算子+谓词求值器（见 §4.4） |
| `crates/pobr-build/src/xml_build.rs` | **T1** | T4 的 flask/charm 槽位保留改动在 T1 合并后以独立小 patch 进（≤30 行） |
| `crates/pobr-build/src/handlers.rs`（新，handler 注册聚合点） | T1 建骨架 | T2/T3 各自在**自己文件**里 `pub fn register_buff_handlers(reg)`，T1 聚合点逐行 append 调用 |
| `data/<ver>/overlay/config_options.json` | T1（工具产物） | 禁手改 |
| `crates/pobr-data/src/catalog/buffs.rs`（新）、`crates/pobr-core/src/rules/buff_expander.rs`（新）、`data/<ver>/overlay/buff_definitions.json` | T2 | |
| `crates/pobr-core/src/calc/buff_pass.rs`（新）、`calc/env.rs`（BuffSpec 存放字段）、`calc_orchestrator.rs` 的 aura/buff 注入段（:1633 一带） | T3 | |
| `crates/pobr-core/src/mod_parser.rs` | **T3**（EnemyModifier/keystone 授予词条解析段） | T5 需要的 `Keystone` LIST 解析由 T3 代写（T5 提供测试用例） |
| `crates/pobr-core/src/mod_db.rs`（NestedMods 消费段） | T3 | 改动须走 D3-双跑点 3 |
| `data/<ver>/overlay/curse_priority.json` | T3（工具产物） | |
| `crates/pobr-core/src/calc/ailment_apply.rs`（新）、flask 相关 item ingest 段（`pobr-core/src/item.rs`/`item_text.rs` 的 flask 分支）、`tools/pobr-data-adapter` flask 数据列段 | T4 | |
| `crates/pobr-core/src/modifier.rs`（ModTag 扩展）、`calc/keystone_merge.rs`（新）、`devs/scripts/version-bump-drill.sh`（新） | T5 | modifier.rs 在 E1 完成后冻结 |
| `crates/pobr-core/src/calc/perform.rs` | **T3** | 只接受 env_finalize 调度行级改动；其余 track 通过自己模块的 `pub fn` 被调度 |
| `crates/pobr-data/src/catalog/mod.rs`、`crates/pobr-core/src/lib.rs`/`calc/mod.rs` 模块声明行 | 各 track append 自己的一行 | 单行 append 冲突 git 可自动解，约定按字母序插入 |
| `crates/pobr-build/tests/ninja_parity.rs` baseline | 任何行为 commit 的 baseline 更新独立 commit | 全员只读，更新走显式审查 |

### 2.3 串行序

```
T0（2-3 天，单 agent）
 ├─ 合并后 → T1 / T2 / T3 / T4 / T5-E1 并行开工
T5-E1（ModTag actor/limitActor，~2 天）→ 合并 → T3-C2/C3 才能进 aura/curse 跨 actor 词条
T2 合并 → T3-C5（替换 aura_buff_modifiers 双跑）+ T2-B4 行为 commit（mode_combat 置位）
T1 合并 → T4 的 flask/charm 槽位 patch；T1-A5 双跑删旧码
全部合并 → T5-F（version-bump-drill 演练）→ 阶段验收
```

### 2.4 track 间接口契约（T0 冻结，变更需全员同步）

1. **`BuffSpec`**（pobr-core `calc/session.rs`，T0 定义）：
   ```rust
   pub enum BuffKind { Buff, Guard, Warcry, Aura, AuraDebuff, Debuff, Curse, CurseBuff, Link }
   pub struct BuffSpec {
       pub name: String,            // buff 名（PoB2 buff.name，AffectedBy<名> 条件用）
       pub kind: BuffKind,
       pub skill_id: String,        // 来源技能（归因 + curse priority socket 计算）
       pub mods: Vec<Modifier>,     // buff 携带词条（granted_effect stat 经 statmap/映射产物）
       pub magnitude: f64,          // 默认 1.0（PoB2 calcLib.mod Magnitude 的来源值）
       pub slot: Option<String>,    // socket group 槽名（curse priority）
       pub socket_index: u32,       // 组内宝石序（curse priority，cap 8）
       pub is_mark: bool,
       pub ignore_curse_limit: bool,
   }
   // session API（T0 落签名，T3 实现体）：
   pub fn add_buff_skill(&mut self, spec: BuffSpec);
   pub fn set_keystone_mods(&mut self, map: BTreeMap<String, Vec<Modifier>>); // T5 消费
   ```
   pobr-build（T3）从 granted_effects 数据构造 BuffSpec；分类规则：`skill_types` 含 Aura→Aura、含 Mark→Curse(is_mark)、granted_effect 的 buff 语义列（M1 statmap 边车）→其余类。M3 实际实现 Aura/Curse/Debuff 三类的消费，其余 kind 进框架但暂走「原值直注」兼容路径（行为与现状一致）。
2. **`ConfigOutcome`**（T1 定义，pobr-build 消费）：
   ```rust
   pub struct ConfigOutcome {
       pub player_mods: Vec<Modifier>,
       pub enemy_mods: Vec<Modifier>,
       pub conditions: HashMap<String, bool>,
       pub multipliers: HashMap<String, f64>,
       pub custom_mod_lines: Vec<String>,      // customMods 原文，由 build 层喂 mod_parser
       pub scalars: ConfigScalars,             // resistancePenalty/enemyIsBoss/enemyLevel…
   }
   ```
3. **handler 注册**：`pobr-build/src/handlers.rs::build_registry() -> HandlerRegistry`，T1 建；T2 暴露 `buff_expander::register_handlers(&mut HandlerRegistry)`，T1 聚合点调用。M3 内若需扩 handler 签名（加 `&CalcConfig` 上下文），由 T1 改 `rules/registry.rs` 并通知（registry.rs 注释已预留此演化）。
4. **buffMode 字段名**：`cfg.mode_buffs` / `cfg.mode_combat`（T0 落字段 + builder 方法，语义见 D5）。

---

## 3. T0 接口地基（串行先行）

| 项 | 内容 | 文件 | 验收 |
|----|------|------|------|
| T0-1 | SourceKind 增 `ConfigOption`/`Buff`/`Flask`/`GrantedKeystone` 四变体（D4） | `pobr-data/src/source.rs` | 编译 + 既有测试全绿；逐值不变 |
| T0-2 | CalcConfig 增 `mode_buffs`/`mode_combat`（默认 false）+ `with_mode_buffs`/`with_mode_combat` | `pobr-core/src/config.rs` | 默认 false → 全输出逐值不变（golden diff=0） |
| T0-3 | `calc/env_finalize.rs` 空框架：7 个阶段位的调度函数，各阶段先落 no-op stub；perform.rs 头部插一行 `env_finalize(env)` | `calc/env_finalize.rs`（新）、`perform.rs`（1 行） | no-op → 逐值不变 |
| T0-4 | `BuffSpec`/`add_buff_skill`/`set_keystone_mods` 签名落 session（实现体 `todo!`→ 先存 Env 字段不消费）；Env 增 `player.buff_skills: Vec<BuffSpec>`、`keystone_mods: BTreeMap<...>` | `calc/session.rs`、`calc/env.rs` | 存而不消费 → 逐值不变 |

T0 整体一个 PR、一个搬迁 commit，golden diff=0。

---

## 4. T1 config 链路（`config_options.json` + `rules/config_interpreter.rs`）

### 4.1 A1 — schema：`pobr-data/src/catalog/config_def.rs`

**目标**：542 条目的声明式类型。字段直接镜像 ConfigOptions schema + effects DSL：

```rust
pub struct ConfigOptionDef {
    pub var: String,
    pub input_type: ConfigInputType,        // Check/Count/List/Integer/Float/Text（vendor 计数：check×328/count×146/list×30/integer×5/float×1/text×1）
    pub section: String,
    pub label: Option<String>,
    pub default: Option<ConfigDefault>,     // defaultState(bool|number) / defaultIndex(usize) / defaultPlaceholderState
    pub list_options: Vec<ListOption>,      // {val, label}
    pub visibility: ConfigVisibility,       // if_cond/if_flag/if_mult/if_enemy_cond/if_skill_data（M3 只入库不消费——UI 用）
    pub imply_conditions: Vec<String>,      // implyCond + implyCondList 展开（vendor 共 60 行、含 6 处 implyCondList）
    pub effects: Vec<ConfigEffect>,
    pub handler_id: Option<String>,         // 真逻辑条目（目标 ≤54）
    pub verified: bool,
}
pub struct ConfigEffect {
    pub target: EffectTarget,               // Player | Enemy
    pub name: String,                       // ModName，如 "Condition:Moving" / "Multiplier:StationarySeconds"
    pub mod_type: String,                   // FLAG/BASE/INC/MORE/OVERRIDE/LIST
    pub value: EffectValue,                 // Literal(f64) | Input { mult, div, base } | Clamp{min,max} | Negate | FromListVal
    pub emit_if: Option<EffectPredicate>,   // 受限谓词：{ on: Input, op: Gt|Ge|Lt|Le|Eq|Ne, rhs: f64 }（conditionStationary 的 >0 才发 FLAG）
    pub tags: Vec<EffectTag>,               // 受限 tag：Condition{var,neg}/Multiplier{var,div,limit}/Effective（→Condition:Effective）
    pub flags: Vec<String>,                 // ModFlag 名（attack/spell…），解释器侧映射位枚举
    pub list_value: Option<ListEffectValue> // SkillData LIST 等键值载荷（如 corpseLife）
}
```

DSL 与 D2 边界一一对应；`Input{mult,div,base}` + `Clamp` + `Negate` 即架构允许的五算子。**测试**：serde 往返 + 每变体一个构造用例。**预估**：~250 行 + 测试。

### 4.2 A2 — extract-config-options 抽取（探针法归纳 apply 闭包）

**目标**：`cargo run -p sync-pob-catalog -- extract-config-options --vendor-root vendor/PathOfBuilding-PoE2/src --out data/<ver>/overlay/config_options.json`，byte-stable、`_meta` 记 vendor commit（沿用 `extract_lua.rs` 既有 OverlayMeta/assemble 模式）。

**vendor 参照**：`Modules/ConfigOptions.lua` 全文件；headless 引导参考 `src/HeadlessWrapper.lua` 与 pob2-oracle 的现成引导（tools/pob2-oracle）。

**引导脚本 `extract_config_options.lua` 的归纳算法**（核心难点，apply 是闭包不可序列化——用**调用拦截 + 多探针拟合**）：

1. **环境**：复用 pob2-oracle headless 引导加载 `data`/`modLib` 等真实依赖后 `require ConfigOptions`（P13：luajit 执行，不正则啃源码）。
2. **RecordingModList**：实现 `NewMod(name, type, value, source, ...)` 拦截，把每次调用记为 `{target, name, type, value, tags=序列化(...)}`；tags 里的 table 按 `{type=..., var=..., div=..., limit=..., neg=...}` 白名单字段序列化，出现白名单外字段 → 本条目标记 `handler_id`。
3. **探针调用**：
   - `check` 型：`apply(true, ml, eml, stubBuild)` 一次 → effects 全为字面量。
   - `count/integer/float` 型：探针 `val ∈ {17, 37}`（互质、避开常见 clamp 端点）。两组记录按 (name,type,tags) 配对：value 相同 → `Literal`；线性拟合 `v = a·val + b`（a/b 取最短小数）→ `Input{mult:a, base:b}`；再补探针 `{-5, 250}` 探 clamp 边界（输出在 0/100 等处饱和 → `Clamp{min,max}` 包裹）。两组 effects **条数不同**（如 conditionStationary 只在 val>0 时发 FLAG）→ 对缺失项尝试 `emit_if: input > 0`（用 val=0 探针验证），失败 → handler_id。
   - `list` 型：对每个 `list[i].val` 各调一次；若 effects 仅 value 随 val 变 → `FromListVal`；结构不同 → handler_id。
   - `text` 型（仅 customMods）：不探针，直接落 `handler_id: "config:custom_mods"`（消费侧专用通道，见 A4）。
4. **逃逸检测**：stubBuild/env 以 `__index` 元表记录任何字段读取；apply 读了 build/env/skillModList → handler_id（真逻辑：enemyIsBoss/presetBossSkills/questRewards 及读 output 的少数条目）。`m_max/m_min/m_floor` 等白名单函数正常放行。
5. **implyCond/implyCondList、schema 字段**：直接从条目 table 序列化（这部分是纯数据，零归纳）。
6. 输出 JSONL → Rust 侧按 var 排序、统一数字格式、组装 `_meta`（`regen_command` 写 canonical 相对路径，照抄 skill_overrides 模式）。

**正确性裁判（oracle 对拍，P13「不以源码读得对为标准」）**：新增 `check-config-options` 子命令——对全部 542 条目，把归纳出的 effects 在 Rust 侧按同一探针值实例化，与 luajit 真实 apply 的 RecordingModList 输出 diff（探针集：check=true；count={0,1,17,37,100,250}；list=全选项）。diff=0 的条目 `verified:true`；diff≠0 自动降级 handler_id 并入报表。**该对拍跑在 CI 的 luajit 可用分支**（无 luajit 时 skip，沿用 M0 模式）。

**预估**：Lua 引导 ~300 行 + Rust 侧 ~250 行 + 对拍 ~150 行；产物预期 ~490 条模板化 + ~50 条 handler_id（架构预估 10%）。**这是 T1 风险最大项**，先用 General/Combat 两个 section 打样跑通再扩全量。

### 4.3 A3 — gamedata 装载 + RuleSet.ConfigCatalog 填充

`pobr-gamedata` 增 config 域懒加载（overlay 目录、无 base 侧、不需 merge）；`ruleset.rs` 的 `ConfigCatalog` 从占位空结构改为真实类型 `pub struct ConfigCatalog { pub options: Vec<ConfigOptionDef>, pub by_var: HashMap<String, usize> }`，`load_ruleset()` 接通。loader 容忍缺表（R7：表不存在 → `config_catalog: None`，消费方回退旧路径）。**预估**：~120 行。

### 4.4 A4 — `rules/config_interpreter.rs`

**目标**：纯函数 `interpret(catalog: &ConfigCatalog, inputs: &RawConfigInputs, registry: &HandlerRegistry) -> ConfigOutcome`。

- `RawConfigInputs` = xml_build 读出的原始 `<Input name bool|number|string>` 三型键值（A5 改 xml_build 产出它）。
- **DSL 单一实现（架构 §5，总架构评审裁决）**：`Input{mult,div,base}`/`Clamp`/`Negate` 与受限谓词的**求值器**不得内联在 interpreter——落独立模块 `crates/pobr-core/src/rules/value_expr.rs`（T1 起建）。M5b special_mods 与 M6 parser 模板按裁决**必须复用此实现**（M5b 加 enums 闭集、M6 加 `:cap`，均为该模块的受限扩展）；config/special/parser 三处是同一套受限语言，禁三套方言。`ConfigEffect.EffectValue` 仅是 schema 形态，求值统一走 value_expr。
- 求值序：每条目取「显式输入 else default」→ check=false/None 直接跳过 → effects 逐条实例化（算子按 schema 语义；`Effective` tag → `ModTag::Condition{var:"Effective"}`）→ target 分流 player/enemy → `Condition:`/`Multiplier:` 前缀的 FLAG/BASE 同时回填 `conditions`/`multipliers` 表（保持现有 cfg 通道兼容）→ `imply_conditions` 展开（仅当条目值为真；蕴含写入 conditions，不覆盖显式 false）→ handler_id 条目查 registry，未注册 → 记入 `unhandled` 报表字段（不 panic）。
- **defaultState 语义**取代 `DEFAULT_TRUE_CONDITIONS` 硬编码（xml_build.rs:123-131 七条删除）与 `DEFAULT_QUEST_STAT_REWARDS`（quest 默认领取从 catalog default 读出）。
- **customMods**：`handler` 不做解析（core 的 mod_parser 调用属 build 层职责）——interpreter 把 text 原文按行 StripEscapes 后放 `custom_mod_lines`，pobr-build 喂 `session.add_modifier_texts`（source=Custom；不可解析行自然落 `ParseStatus::Unsupported` 可见性通道）。vendor 参照 ConfigOptions.lua:2278-2296。
- **第一批 config handlers**（注册在 pobr-build/handlers.rs，包装既有逻辑而非重写）：`config:enemy_is_boss`（既有 EnemyTier 接线包装）、`config:preset_boss_skills`（M3 先 stub 告警，boss_skills.json 属 M5+）、`config:resistance_penalty`（既有 CampaignProgress 包装）。

**所有 SourceId**：`SourceKind::ConfigOption, "config.<var>"`。
**测试**：每算子单测；imply 展开（含「显式 false 不被蕴含覆盖」）；count 型 conditionStationary 端到端（number=5 → Multiplier:StationarySeconds=5 + Condition:Stationary=true；number=0 → 只有 Multiplier）；customMods 行通道。**预估**：~400 行 + ~300 行测试。

### 4.5 A5 — xml_build 切换与双跑

xml_build 的 `parse_config` 重构为「只产 `RawConfigInputs`」；解释走 interpreter。**双跑**（D3 点 1）：保留旧 parse_config 为 `#[cfg(test)]` 或临时私有函数，集成测试对 ninja 18-build + 既有 XML fixture 跑两路，断言旧产出 ⊆ 新产出且交集逐值相等；diff 报告（新增覆盖项清单）附 PR。报告干净后删旧分支（独立 commit）。**新增覆盖项的 parity 影响 = 行为 commit**：count condition / implyCond / enemy 数值覆盖 / customMods 各自独立 commit + baseline 显式审查。

**真实 XML fixture**（阶段验收要求）：构造 `crates/pobr-build/tests/fixtures/config_*.xml` 至少覆盖：count 型 stationary、implyCond 链（UsedSkillRecently 族）、enemy 抗性覆盖 + enemyIsBoss=None、customMods 多行（含一行不可解析）、list 型选项。**预估**：~200 行改动 + fixture。

### 4.6 A6 — 监控断言

`handlers.rs` 测试：`assert!(config_handler_count <= 54)`、`assert!(registry.len() < 100)`；catalog 加载测试：`verified:false` 条目数入 parity 报表（sync-pob-catalog check 扩展或测试打印均可）。

---

## 5. T2 buff 定义 / expander / buffMode

### 5.1 B1 — schema：`pobr-data/src/catalog/buffs.rs`

```rust
pub struct BuffDef {
    pub id: String,                       // "Onslaught"
    pub trigger_flag: String,             // mod_db 的 Flag 名（多数同 id）
    pub mode_gate: BuffModeGate,          // Combat（doActorMisc 整段 :510 门控）
    pub base_magnitude: f64,              // Onslaught=10 / Adrenaline=25(speed)… 见 mods[].value_per_effect 设计
    pub effect_inc_stats: Vec<String>,    // 吃哪些 INC 缩放：["OnslaughtEffect","BuffEffectOnSelf"]
    pub rounding: Rounding,               // Floor（PoB2 m_floor(base×(1+inc/100))）
    pub mods: Vec<BuffModTemplate>,       // {name, mod_type, value_per_effect(系数), flags, tags}
    pub conditions_set: Vec<String>,      // 附带置位条件（HerEmbrace→condList["HerEmbrace"]）
    pub handler_id: Option<String>,
    pub verified: bool,
    pub vendor_ref: String,               // "CalcPerform.lua:541-573"（人工策展可追溯性）
}
```

效果公式（框架逻辑，留 Rust）：`effect = round(base_magnitude × (1 + Σ INC(effect_inc_stats)/100))`，每条 mod 值 = `value_per_effect × effect`（Onslaught Speed INC = 2×effect）。

### 5.2 B2 — `buff_definitions.json`（人工归纳 + oracle 对拍）

**抽取方式偏离声明**：doActorMisc 是 260 行过程式 if-chain（CalcPerform.lua:503-765），**不是可执行序列化的数据表**，无法走 extract-lua 通道——本表以**人工归纳**落 overlay，每条带 `vendor_ref` 行号 + `verified` 字段，正确性以 oracle 对拍（构造带该 flag 的最小 build，对拍 PoB2 输出的 Speed/Damage 等中间值）为准。`_meta` 记 vendor commit；CI drift 防线降级为「vendor 文件该行段 hash 变化时告警」（sync-pob-catalog check 加一个行段 hash 对账）。**此偏离需架构确认**（见 §12 待裁决 Q1）。

**第一批条目**（模板可表达，按 ninja 命中频率优先）：Onslaught（基本形，:541-573，**不含** Silver Flask 分支）、Adrenaline（:589-596）、Convergence（:597-600）、UnholyMight（:581-585，Multiplier:UnholyMightMagnitude + per-multiplier DamageGainAsChaos——验证 ModTag::Multiplier 路径）、ChaoticMight、Fanaticism（:574-580，selfCast 门控 → `emit_if` 谓词或 handler）、HerEmbrace、Tailwind、Elusive。
**handler_id 条目**（真逻辑）：`buff:fortify`（stacks 模型读 FortificationStacks/MaximumFortification，:523-539）、`buff:onslaught_flask`（Silver Flask flaskData 读取，依赖 T4 flask merge，M3 末接）。buff handler 预算 ≤8。

### 5.3 B3 — `rules/buff_expander.rs`

纯函数 `expand_misc_buffs(db: &ModDb, cfg: &CalcConfig, defs: &[BuffDef], registry: &HandlerRegistry) -> Vec<Modifier>`：`cfg.mode_combat` 门控整段 → 逐 def 查 `db.flag(trigger_flag)` → 公式展开 → SourceId `Buff, "buff.<id>"`。env_finalize 阶段 6 接线（T3 调度点调用，写回 player.mod_db）。
**测试**：逐 buff 数值单测对 PoB2 公式（含 floor 行为：OnslaughtEffect 23% + BuffEffectOnSelf 10% → effect=floor(10×1.33)=13 → Speed INC 26）；mode_combat=false 零输出；flag 未置零输出。**预估**：expander ~150 行 + 测试 ~250 行。

### 5.4 B4 — mode_combat 战斗条件自动置位

vendor 参照 CalcPerform.lua:242-260（见 §0.2 表）。落点：pobr-build 编排在主技能解析处按 skill_types 派生（attack→`AttackedRecently`、spell→`CastSpellRecently`、Movement→`UsedMovementSkillRecently`、minion→`UsedMinionSkillRecently`、Vaal→`UsedVaalSkillRecently`），triggered/trap/mine/totem 豁免；写入 cfg.conditions，`mode_combat` 门控。**独立行为 commit**（一批 "recently" 词条首次生效，baseline 显式审查）。**预估**：~60 行 + 测试。

---

## 6. T3 buff_pass：aura / curse / EnemyModifier

### 6.1 C1 — BuffSpec 提取（pobr-build）

把 `calc_orchestrator.rs` 现有 aura 注入段（:1633 `aura_buff_modifiers` 与 self_buff 系）改为构造 `BuffSpec` 经 `session.add_buff_skill` 注入；分类规则见 §2.4 契约 1。**过渡期双注入禁止**：C5 切换前 buff_pass 对 Aura kind 空转（feature flag `buff-pass-aura`），保证不双计。curse 技能识别：granted_effect `skill_types` 含 Mark/Curse 系 type token（M1 的 token 表达式列）。**预估**：~200 行。

### 6.2 C2 — aura 路径（calc/buff_pass.rs）

vendor 参照 CalcPerform.lua:2090-2120（实读核对）：

```
inc  = skill_db.sum INC  ["AuraEffect","BuffEffect","BuffEffectOnSelf","AuraEffectOnSelf","AuraBuffEffect","SkillAuraEffectOnSelf"]
more = skill_db.more 同名集合 × magnitude
mult = (1 + inc/100) × more
每条 buff mod：value × mult 后并入 player.mod_db（同名 merge 相加，对应 mergeBuff/ScaleAddList）
条件置位：AffectedByAura、AffectedBy<去空格名>
```

M3 口径简化（与 PoB2 的差异显式记录在模块文档）：(a) `skill_db` = player.mod_db + 该 skill 自带 mods（pobr 无 per-skill modlist 分层，AuraEffect 全局聚合——与 PoB2 差异在「只对某 aura 生效的 AuraEffect」词条，少见，命中时按 build 修）；(b) ally 取强：`allyBuffs` 参数恒空（party 未落地），分支保留。`auraCannotAffectSelf`（granted_effect 数据列，缺列时默认 false）。ScaleAddList 的取整语义对照 `Classes/ModList.lua::ScaleAddList`（实施时实读：对 value 乘 mult 后 `m_floor(x+0.5)` 类取整——逐字对齐）。
SourceId：`Buff, "aura.<skill_id>"`；trace 边保留原 stat 来源（buff mod 的 origin 不丢弃，scale 记入 raw_text）。

### 6.3 C3 — curse 路径 + `curse_priority.json`

- **新增 overlay 小表 `curse_priority.json`**：`data.cursePriority`（Modules/Data.lua:274 起的纯数据表：per-curse 基值 + `SocketPriorityBase` + 槽名权重 + `CurseFromAura`/`CurseFromEquipment`）。走 extract-lua（真·数据表，luajit 可序列化；并入 T1 的引导脚本框架，新 schema `curse_priority/v1`）。
- priority 算法（框架逻辑，对照 determineCursePriority :454-485）：`base + min(socket_index,8)×SocketPriorityBase + slot_weight(去" (Swap)") + source_weight`（aura 源→CurseFromAura；装备源→CurseFromEquipment；Ring 2/3 的装备隐式诅咒折回 Ring 1 权重）。
- limit（:2829-2833）：`EnemyCurseLimit = override(CurseLimitIsMaximumPowerCharges→PowerChargesMax) else sum BASE EnemyCurseLimit`；`EnemyMarkLimit` 同；curse/mark 槽位**分开**按 priority 填（同名比 priority 高者替换，低者跳过），`ignore_curse_limit` 条目槽位外追加。
- CurseEffect 缩放：`mult = (1 + Σ INC(CurseEffect,BuffEffect)/100) × more`；非 mark 再乘 `enemy_db.more(CurseEffectOnSelf)`；mods 缩放后写 **enemy.mod_db**，带 `Condition:Effective` 门控对齐现有敌侧口径。`EnemyCurseLimit` 基线值依赖 `base_player_mods.json`（M0 已入库，确认含 EnemyCurseLimit BASE 1——缺则补，属数据 commit）。
- 输出面板字段：`output.EnemyCurseLimit`、curse 槽占用列表（display_catalog M3 不扩，仅 OutputTable 字段）。

### 6.4 C4 — EnemyModifier LIST 通道

1. `ModValue` 增 `NestedMods(Vec<Modifier>)`（D3-双跑点 3：独立 commit，mod_db 全测试 + bench 无回归）。
2. mod_parser：敌方向词条（`Enemies ... have/take ...`、`Nearby Enemies ...`、`Enemies you Curse ...` 带条件）解析为 `Modifier{ name:"EnemyModifier", mod_type: List, value: NestedMods([inner]) }`，inner 为正常解析的敌侧 mod（`Condition:Effective` tag 视语义附带）。**范围控制**：M3 只迁移 mod_parser 中现有「Enemy 前缀 ModName」词条中语义为「写敌方 db」的子集 + ninja 18-build 语料中 Unsupported 的敌方向高频词条（先列清单再动手，每条附 PoB2 ModParser 对应行为依据）。
3. `forward_enemy_modifiers(env)`（env_finalize 阶段 2）：player.mod_db（+ minions）的 EnemyModifier LIST → enemy.mod_db，保留原 SourceId，按 mod 身份去重（对照 applyEnemyModifiers :486-500 的 cache 语义；pobr 单次 perform 内以 HashSet<指纹> 实现）。

### 6.5 C5 — 替换 aura_buff_modifiers 直注（双跑收尾）

T2/T3 主体合并后：开 `buff-pass-aura` 路径、关静态直注 → 18-build 双跑 diff 报告（D3 点 2）→ 无 AuraEffect 词条 build 逐值持平、有词条 build 差异附 PoB2 依据 → baseline 行为 commit → 删旧函数。

**T3 测试**：aura 乘区 fixture（20% inc AuraEffect + aura 给 100 ES → 120）；curse priority 表驱动单测（vendor 数值样例）；limit 截断（3 curse 入 1 limit → priority 最高者）；mark/hex 分槽；EnemyModifier 转发归因（SourceId 穿透 enemy db 聚合）；NestedMods serde/mod_db 回归。**预估**：buff_pass ~450 行 + parser 段 ~150 行 + 测试 ~400 行。

---

## 7. T4 非伤害异常闭环 + flask/charm

### 7.1 D1 — `calc/ailment_apply.rs`（env_finalize 阶段 7）

对照 CalcPerform.lua:3076-3180（公式见 §0.2 表），M3 范围 = Chill + Shock 两类（PoE2 主干；Scorch/Sap/Brittle 不在 PoB2-PoE2 该段）：

1. 来源聚合：enemy_db `<X>Val`（config 敌人状态项经 T1 enemy effects 注入）∪ player_db `<X>Base/<X>Override/<X>Minimum`（词条），magnitude 乘 `Enemy<X>Magnitude`/`AilmentMagnitude`（skill 侧）×`Self<X>Magnitude`（enemy 侧）。
2. `Current<X> = floor(min(max(override, Σ Val), Maximum<X>) × 10^prec)/10^prec`；`Maximum<X>` = override else `non_damaging_ailments.json` 的 max + 词条 `<X>Max` BASE（常量读 `cfg.constants`，禁新魔数）。
3. 写 enemy db：Shock → `DamageTaken INC Current {Condition:Shocked}`；Chill → `ActionSpeed INC -Current {Condition:Chilled}` + Bonechill 分支（`ColdDamageTaken INC`）；置 `Condition:Shocked/Chilled`（来自 Override 来源时）+ `Condition:AlreadyShocked/AlreadyChilled` 防 minion 重复；`Multiplier:ChillEffect/ShockEffect` 取增量更新（:3173-3180）。ChillCanStack/ShockCanStack 分支 M3 暂不实现（按 build 命中再补，模块文档记差异）。
4. **消费验证**：offence 的 DamageTaken 链已存在（offence.rs:685-720）——shock 闭环在 `mode_effective` 下自动生效。enemy ActionSpeed 的消费点 M3 缺（敌方出手速度只影响 EHP 估，列入模块文档已知差异）。
5. 与既有 `fill_ailments`（offence 后面板/DoT 计算）职责切分：本阶段**只消费 Base/Override/Val 词条与 config**（与 PoB2 同——PoB2 该段也不依赖本次 perform 的 DPS），面板 magnitude 估算留 fill_ailments 不动。
6. **顺带闭环**：config `conditionEnemyShocked` 系条目在 T1 落库后产出 enemy `ShockVal`，本阶段把它折成 DamageTaken——这正是「配置输入联动」的端到端线。

**测试**：config 感电 20% → 有效 DPS ×1.20 的端到端 fixture；override 与 Val 取 max；Maximum clamp + 精度截断逐值（prec 来自数据）；Already 置位后二次施加无效。**预估**：~250 行 + 测试 ~200 行。

### 7.2 D2 — flask/charm 合并（mode_combat 门控）

依赖现状：base_items.json 无 flask 数据列（§0.1 假设 9 实查未落）。工作项：
1. **adapter 增列**（tools/pobr-data-adapter）：UtilityFlask/LifeFlask/ManaFlask（+charm 基底，确认 PoE2 .dat 的 charm item_class 后补）落 `flask{duration, charges, buff_stats[]}/charm{...}` 列——L1 数据，搬迁 commit。
2. **xml_build 槽位**：`EquipmentSlot` 枚举外的 Charm/Flask 槽保留为新 `utility_slots: Vec<(String, ItemRef)>`（T1 合并后小 patch，≤30 行）。
3. **item ingest flask 分支**（pobr-core item.rs/item_text.rs）：flask/charm 词条文本解析复用 mod_parser；utility flask 的常驻 buff 词条（Onslaught during effect 等）→ Flag mod + flask effect inc 词条。
4. **merge 阶段**（env_finalize 阶段 3）：激活态门控 = config `useFlask`/charm 默认常驻（PoB2 charm 在 combat 模式默认生效；flask 需 config 勾选——对照 ConfigOptions Combat section 的 flask 条目，T1 落库后用 config 条件门）+ flask effect 乘区缩放后并入 player db。SourceId `Flask, "flask.<slot>"`。Mageblood 类特判不做（PoE2 无）；`buff:onslaught_flask` handler 在此之后接通（T2 协作）。

**范围声明**：M3 验收只要求「charm/flask 词条进入计算且吃 effect 乘区」，魔药充能/持续时间模型不建。**预估**：adapter ~150 行 + ingest ~200 行 + merge ~120 行 + 测试。

---

## 8. T5 tag 扩展 + mergeKeystones + version-bump-drill

### 8.1 E1 — ModTag actor/limitActor（**先行，T3 依赖**）

`ModTag::Multiplier` 增字段 `actor: Option<ActorRef>`、`limit_var: Option<String>`、`limit_actor: Option<ActorRef>`（`ActorRef = Player|Parent|Minion`）；`ModTag::Condition` 增 `actor: Option<ActorRef>`。求值通道：`CalcConfig` 增 `actor_multipliers: HashMap<String, f64>`（"父 actor 视角"只读快照——minion 求值时由 perform_minions 注入玩家值，玩家求值时为空；沿用现 SummonedMinion 注入先例并一般化）。`effective_number`/`matches` 按 actor 维度取数：`actor=None` → 现行为**逐字不变**（搬迁 commit golden diff=0）；`Some(Parent|Player)` → 查 actor_multipliers。
mod_parser 端 M3 只接 aura/curse 域需要的词条（如 minion 吃 `per X of your Y`），按 ninja 语料命中排。**预估**：~150 行 + 测试。

### 8.2 E2 — mergeKeystones 二次合并

- mod_parser：`You have <KeystoneName>` 类词条 → `Modifier{name:"Keystone", List, value: Text(name)}`（解析代码由 T3 在 mod_parser 统一落，T5 给用例）。
- pobr-build：从 passive_tree.json 构造 `keystone_name → Vec<Modifier>`（节点 stat 文本走既有 passive ingest 解析路径），经 `session.set_keystone_mods` 注入。
- `calc/keystone_merge.rs`：env_finalize 阶段 1 & 5 各跑一次——Tabulate player db 的 Keystone LIST → 未注入过（`HashSet<String>` 去重，等价 env.keystonesAdded）的查 map 注入其 mods，SourceId `GrantedKeystone, "keystone.<name>"`。对照 CalcPerform.lua:66-76。
- 与 M2 `rules/keystone_registry.rs` 的关系：本项只负责「词条→keystone 的 mod 注入」；CI/EB 等机制开关仍由 keystone_registry 读 flag 裁决（注入的 mods 里含对应 flag 即自动接通）。

**测试**：装备词条授予 Iron Reflexes → 其 keystone mods 注入一次；树已点同名 keystone → 不重复；buff 授予（阶段 5）时点验证。**预估**：~120 行 + 测试。

### 8.3 F — `devs/scripts/version-bump-drill.sh` 第一版（P18）

**目标**：把「版本更新只动 JSON」做成可执行演练。第一版覆盖 M3 时点已存在的管线步：

```bash
devs/scripts/version-bump-drill.sh --data-export <dir> --vendor <dir> --version <ver>
# 1. pipeline 下载校验（占位：校验输入目录表清单完整性）
# 2. cargo run -p pobr-data-adapter -- ... → data/<ver>/base/
# 3. extract-lua 全部已注册抽取：skill_overrides + config_options + curse_priority
# 4. precompile：占位 skip（M6）
# 5. 校验：A) 产物重跑 byte-diff=0（复用 devs/scripts/regen-check.sh 逻辑）
#         B) cargo build --workspace 零改动编译通过
#         C) cargo test -p pobr-build --test ninja_parity 可运行（不要求达标，要求不 crash）
# 6. 产出报告 audits/rearchitecture-2026-06-10/drill-findings-m3.md：
#    每个「必须改 Rust 才能吸收」的发现 = 一条登记（→ M5/M6 数据化清单）
```

**演练执行**：对**当前版本输入重放**（无新版本时的演练形态）+ 人工模拟一处数值变更（改 vendor 某 aura 数值 → 重跑 → 确认只有 overlay JSON diff、Rust 零改动、parity 集可跑）。发现项登记是验收物，不要求清零。**预估**：脚本 ~150 行 + 一次演练（0.5 天）。

---

## 9. 门禁与验收

### 9.1 每 PR 局部门禁（全 track 一致）

1. `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check` 全绿；
2. ninja_parity 18-build **零回归**（防御/进攻以 M2 末实际 baseline 为底线不得倒退；搬迁 commit 逐值不变 golden diff=0）；
3. 涉及 overlay 表：重跑 extract 命令 byte-diff=0 + `_meta` vendor commit 与 `.pob2-version.txt` 一致；
4. D2 的 review checklist 六项勾选。

### 9.2 阶段整体验收（roadmap M3 节原文为准）

> 进攻 **≥55%** / 防御 **≥85%**；含 aura/curse 的 build 不再系统性偏低；config 导入用真实 XML fixture 回归；**第一次 version-bump-drill 演练**（P18）——发现的"必须改代码"项登记进 M5/M6 清单。

操作化：
- parity：ninja_parity @5% 容差，进攻 ≥55%、防御 ≥85%（若 M2 终点 <80%，按「M2 终点 + M3 预期增量 ≥+5pp 防御 / +15pp 进攻」口径顺延并在验收 PR 显式声明）；
- 「aura/curse build 不再系统性偏低」：18-build 中含 aura/curse 的子集单独列 diff 分布，中位偏差从系统性负偏移收敛到 ±5% 带内或给出逐 build 解释；
- config fixture：§4.5 清单全绿；
- 双跑三点（D3）全部出过 diff 报告且旧码已删；
- handler 计数断言（D2）在 CI 常驻；
- drill 报告落盘 + 登记项转入 M5/M6 清单。

---

## 10. 风险与回退（R# 落点）

| 风险 | 本阶段具体落点 | 缓解/回退 |
|------|----------------|----------|
| **R1 DSL 膨胀**（最大架构风险，config effects 是第一个大规模受限 DSL） | A2 归纳时被「再加一个算子就能多收 N 条」诱惑 | §1-D2 硬边界 + ≥20 条目闸门 + handler 计数断言测试（A6）锁死；扩算子必须独立 PR 列受益清单 |
| **R3 extract-lua 正确性**（探针法归纳错：非线性 apply 被拟合成线性 / clamp 漏检） | A2 | 三类探针 + 逃逸检测保守降级 handler；**oracle 对拍 542 条全量**为正确性裁判（不以「源码读得对」为准）；`verified:false` 单列报表；先两个 section 打样 |
| **aura/curse 是历史 parity 偏差的最大未知数**（roadmap 原文） | C5 切换后含 aura build 可能出现非预期大 diff | 按 ninja build 命中频率优先实现高频 aura；C5 diff 报告逐 build 归因（TraceGraph 直接可用）；阶段目标必要时按 build 分组重排（roadmap 授权） |
| **R2 隐藏补偿**（aura 静态直注当前可能恰好补偿了别处低估） | C5 删旧码 | feature flag 双跑、diff 干净才删；删码独立 commit 可单独 revert |
| **R11 零回归 vs 提升张力**（mode_combat 置位/implyCond/count condition 都让一批词条首次生效） | B4、A5 行为 commit | 每个覆盖扩展独立行为 commit + PoB2 行号依据 + baseline 显式审查；一次只开一类 |
| **R7 schema 演化** | config_def/buffs 新表 | 新字段一律 `#[serde(default)]`；manifest 按域记 schema 版本；loader 缺表回退旧路径（A3） |
| **NestedMods 动核心载荷** | C4 | 独立 commit、mod_db 全测试 + bench 门禁、零行为改动 |
| **多 track 共享 perform.rs/xml_build.rs** | 全程 | §2.2 归属表：perform.rs 仅 T3 写、其余 track 经模块 pub fn 被调度；xml_build 仅 T1 写、T4 patch 串行 |

---

## 11. 实施前待裁决（open questions）

1. **Q1 buff_definitions.json 的抽取方式偏离 P13**：doActorMisc 是过程式 if-chain，无法 luajit 序列化抽取——本蓝图按「人工归纳 + vendor_ref 行号 + oracle 对拍 + 行段 hash drift 告警」落 overlay。需架构确认此为 overlay 通道的认可例外（否则唯一替代是 M6 式行为级对拍工具，成本不成比例）。
2. **Q2 M2 终点防御 parity 若未达 80%**：M3 的 ≥85% 目标是否按 §9.2 顺延口径执行（需阶段 owner 拍板）。
3. **Q3 handler 签名扩展时机**：现 `Handler = Fn(&[f64]) -> Vec<Modifier>` 无法表达 enemyIsBoss（需写 enemy 侧 + 读 tier）。T1 计划扩为 `Fn(&HandlerCtx) -> HandlerOutcome`（含 player/enemy 双向量 + 标量设置）——这是对 M0 骨架的破坏性签名变更（registry.rs 注释已预留），是否需要与正在进行的 W3 工作协调合并窗口。
4. **Q4 charm 基底的 .dat 表名**：实查 base_items.json 无 Charm item_class，PoE2 charm 在 .dat 中的 item_class 名需 pipeline 侧确认（可能需补下载表）——T4-D2 第 1 项的前置侦察。

---

## 12. 工作项→缺口→验收 对照总表

| 工作项 | 缺口 | 关键验收物 |
|--------|------|-----------|
| T1 A1-A6 | 19-G1/19-G2/19 号 Gap4/Gap6 | config_options.json（~490 模板 + ≤54 handler）+ oracle 对拍报表 + 真实 XML fixture + 双跑 diff 报告 + DEFAULT_TRUE_CONDITIONS/前缀启发式删除 |
| T2 B1-B4 | 11-G2、11 号 Gap5 | buff_definitions.json（首批 ~10 条 + ≤8 handler）+ 逐 buff 数值单测 + mode_combat 行为 commit |
| T3 C1-C5 | 11-G1、11 号 Gap6 | buff_pass.rs + curse_priority.json + aura/curse fixture + aura_buff_modifiers 删除 + EnemyModifier 端到端归因测试 |
| T4 D1-D2 | 14-G6、11 号 Gap9 | shock→DPS 端到端 fixture + flask/charm 词条入计算 + adapter flask 列 |
| T5 E1/E2/F | 10-G3（actor 系）、11 号 Gap11 | actor tag 单测 + keystone 授予 golden + version-bump-drill.sh + drill-findings-m3.md |

## 13. 常量补充流程（全 track 适用）

M3 新公式需要的任何常量（curse limit 基线、ailment max/precision 等）：先查 `data/<ver>/base/game_constants.json`/`non_damaging_ailments.json`/`base_player_mods.json` 是否已有 → 有则经 `cfg.constants`/对应注入表读取；没有 → 在对应 JSON + `pobr-data` fallback 同步补（搬迁 commit，逐值测试），**禁止在 calc 代码里写裸数字**。
