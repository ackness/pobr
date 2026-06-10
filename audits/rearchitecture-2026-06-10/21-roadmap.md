# 21 — 分阶段路线图（M0–M7）

> 撰写日期：2026-06-10 · 配套架构方案见 [20-target-architecture.md](20-target-architecture.md)（裁决条目以 P# 引用）
> 排序逻辑：**M1–M4 按 parity 提升斜率排（B 案），M0 与 M6 按数据-框架分离硬目标排（A 案）**。设计保证：任一阶段中断，项目都处于"比现状更分离、parity 不低于现状"的稳态。
> 体量估计：合计 ~27 人周。阶段内按 worktree 并行（沿用 5-agent 惯例），每张 JSON 表独立可合并、独立过门禁。

---

## 0. 统一门禁与执行纪律（每阶段适用）

**门禁三件套**（每次合并回 master）：

1. `cargo test --workspace` + `clippy -D warnings` + `fmt --check` 全绿；
2. **ninja_parity 18-build 零回归**——防御 51% / 进攻 24%（@5% 容差）为底线不得倒退；阶段各自的提升目标见各节"验收"；
3. 涉及解析/数据的阶段：加 pob2-oracle 对拍或 generated 重生一致性校验。

**执行纪律**（FINDINGS 04-02 教训制度化）：

- **搬迁不变式**：纯搬迁（数据出代码、入 JSON）的 commit，parity baseline **逐值不变**（golden diff = 0）；搬迁与行为改动永远分两个 commit。
- 行为修复必须附 PoB2 一手依据（源码行号/oracle 中间值）；baseline 更新独立 commit、显式审查。
- 核心改动（mod_db/ModFlags/stat_map 引擎切换等）feature-gated **双跑对照**，diff 报告干净后才删旧码。

**缺口 ID 约定**：`<领域文档号>-G<n>`，如 `13-G2` = 13-defence.md 缺口清单第 2 条。本目录 high 缺口汇总（01-gap-summary）按同一 ID 体系聚合，下文映射可直接对照。

---

## M0 地基：三层目录 + 数据出代码 + 可再生性收口（~2.5 人周）

**目标**：建立 `base/overlay/generated` 三层物理目录与全部 CI 防线（P1/P8/P9），把"版本更新只动 JSON"做成最小可信闭环；框架内最大块的内嵌数据表（monster/constants/campaign）出代码。

**具体任务**：

- manifest v2（三段 domains）+ pobr-gamedata overlay merge loader（merge 规则单测锁定）+ RuleSet 聚合入口骨架；handler_id 注册表骨架。
- L1 常量落库：`game_constants.json`（三段）/`character_constants.json`/`monster_scaling.json`/`non_damaging_ailments.json`/`weapon_types.json`/`unarmed_data.json`/`jewel_radii.json`/`base_player_mods.json`/`enemy_presets.json`；`monster.rs`/`constants.rs`/`campaign.rs` 降级 fallback；calc 全部魔数改读注入常量（pobr-core 签名改收 `&GameConstants`，P9）。
- overlay 通道建立：sync-pob-catalog 增 `extract-lua` 子命令（luajit 执行 vendor 序列化，P13）；把 `skill_attack_speed_more` 手补与 crit_chance/attack_speed_multiplier（3912+3578 个 vendor 抽取值）固化为可重复抽取步骤（`skill_overrides.json` 通道）。
- 小查表数据化（P3 的 M0 部分）：`high_precision_mods.json`/`local_mods.json` 白名单 + mod_parser 查表段改读数据。
- CI：①"重跑 pipeline+adapter+merge → byte-diff 零"；②"pobr-data 禁内嵌大数组" lint；③catalog.rs 拆 `catalog/` 模块目录。
- 顺带接线（数据已就位的纯接线）：resistancePenalty（campaign 既有表）+ enemyIsBoss XML 读取。

**验收门禁**：parity **逐值不变**（纯搬迁 golden diff=0，接线两项为独立行为 commit 单独审 baseline）；data/ 全量可再生 CI 绿；`cargo test` 全绿。

**对应 high 缺口**：15-G3（Export 手工补丁无通道）、15-G4（Misc 常量/百级表硬编码）、19-G3（enemyIsBoss 不读 XML）、14-G3（minions 数据化——本阶段完成 schema+常量侧，数据入库在 M5a）、10-G1（部分：查表段先行）。

**风险**：R2 搬迁破坏隐藏补偿（缓解：搬迁不变式逐值校验）；extract-lua 首次落地的正确性（缓解：先抽小表 + oracle 对拍）。

---

## M1 技能/宝石数据链路（~3 人周）

**目标**：打通宝石品质、statmap、support 适用性三条断裂链路——进攻 parity 最大的系统性低估来源。

**具体任务**：

- gem quality 四层打通：pipeline 下载 GrantedEffectQualityStats → `gem_quality_stats.json` → XML 导入 `GemSkillRef.quality` → orchestrator 接 `with_quality`（core 归因 API 已就绪）。
- `skill_stat_map.json`（extract-lua 抽 954 条 + per-statset 覆盖边车）+ pobr-core `rules/stat_map_engine.rs`（~60 行 merge 公式）；**双跑对照**（新引擎 vs skill_stat_map.rs 启发式）diff 干净后删 751 行旧码 + 删 adapter `is_mappable_stat` 白名单（全量 stat 入库）。
- support 适用性：granted_effects 补 require/exclude/add_skill_types token 表达式列 → doesTypeExpressionMatch 求值器 + addSkillTypes 不动点循环 → orchestrator 注入前调 can_support。
- mana_multiplier/reservation 全族数据列 + cost 接线；多 statSet（additional_stat_set_ids）入库与 Build 模型；GemEffects/SupportGems 外键表下载打通。

**验收门禁**：进攻 parity 24% → **≥40%@5%**；quality-20 宝石 fixture；statmap 双跑 diff 报告干净；oracle 对拍 statmap 抽样。

**对应 high 缺口**：18-G1（quality 四层皆空）、18-G2（support 裁决断裂，含 FINDINGS 02-06 的扩大版）、18-G3 / 15-G2（SkillStatMap 错放框架）、18-G4（多 statSet 丢弃）、15-G5（qualityStats 缺失）、16-G4（部分：base_items 数据列随本阶段 adapter 扩展落库）。

**风险**：R5 前置——statmap 切换若与隐藏补偿耦合会倒退（缓解：双跑 + 按 ninja build 分组 diff）；GemEffects 外键质量未知（缓解：adapter 端外键完整性校验报表）。

---

## M2 防御机制（~4 人周）

**目标**：补齐防御侧三大结构缺失（扣池状态机/keystone 开关/taken-as），EHP 口径切 PoB2（P11），防御 parity 冲 80%。

**具体任务**：

- `calc/pool_damage.rs`：reducePoolsByDamage 扣池状态机（allies→aegis→guard→ward→ES bypass→MoM→loss-prevention→life）+ 参数化 poolProtected 原语（MoM/Guard/Aegis/Ward bypass/SoulLink 复用同一公式）。
- `rules/keystone_registry.rs`：CI 接线（消灭 perform 写死 false——parser 早已能解析该 flag）、EB(EnergyShieldProtectsMana)、IronReflexes、防御转换矩阵（resourceList ConvertTo + Unbreakable/DoubleBodyArmourDefence 翻倍 flag）。
- taken-as shift 管线（防御侧 damageShiftTable）；Block 基底（base_items.block_chance，源 ShieldTypes）+ BlockEffect + max 体系；Spirit 池聚合（base_items.spirit）；Deflection；Stun 体系（常量已在 M0 入库）。
- EHP 口径切 numberOfHitsToDie×单击伤害；旧 lowest-max-hit 保留为附加指标；baseline 更新独立 commit。

**验收门禁**：防御 parity 51% → **≥80%@5%**；MoM/CI/taken-as 类 fixture；EHP 口径切换的 baseline diff 显式审查。

**对应 high 缺口**：13-G1（taken-as 管线）、13-G2（扣池状态机）、13-G3（MoM/EB/bypass）、13-G4（EHP 口径）、13-G6（资源转换矩阵 + keystone）、16-G4（部分：block_chance/spirit 消费侧）。

**风险**：扣池状态机是新的有序可变过程，与"calc 纯函数"约定的张力（缓解：状态机封装为局部纯函数 `fn reduce_pools(pools, hit) -> PoolsAfter`，不写 Env）；**禁止顺手改归因结构**（P17——双 pass RFC 属 M4）。

---

## M3 编排：config/buff/aura/curse/敌方（~4 人周）

**目标**：用 config_interpreter + buff_expander 两个数据解释器一次性消灭编排层的启发式硬编码；补 aura/curse 整个 perform 阶段。

**具体任务**：

- `config_options.json`（extract-lua 抽 542 条）+ `rules/config_interpreter.rs`：消灭 parse_config 前缀启发式、count 型 condition、customMods、implyCond ~60 处、DEFAULT_TRUE_CONDITIONS 硬编码（P6）。
- `buff_definitions.json` + `rules/buff_expander.rs`（doActorMisc 等价）；buffMode 三态；flask/charm 合并。
- aura/curse perform 阶段（`calc/buff_pass.rs`）：九类分发、aura 效果乘区（inc/more 组合、ally 取强）、curse priority/limit；EnemyModifier LIST 转发通道。
- 非伤害异常消费闭环：敌方施加循环 + shock→enemy DamageTaken、Condition:Chilled/Shocked + 配置输入联动。
- EvalMod tag 扩展第一批（actor/limitActor 引用——aura/curse 跨 actor 词条的前提）；mergeKeystones 二次合并。

**验收门禁**：进攻 **≥55%** / 防御 **≥85%**；含 aura/curse 的 build 不再系统性偏低；config 导入用真实 XML fixture 回归；**第一次 version-bump-drill 演练**（P18）——发现的"必须改代码"项登记进 M5/M6 清单。

**对应 high 缺口**：11-G1（buff/aura/curse 阶段缺失）、11-G2（doActorMisc 表缺失）、19-G1（ConfigOptions apply 语义）、19-G2（customMods）、14-G6（异常消费闭环）、10-G3（部分：actor 系 tag）。

**风险**：R1 DSL 膨胀首次实战——config effects 是第一个大规模受限 DSL（缓解：§5 硬边界 + ≥20 条目闸门 + handler 计数监控）；aura/curse 是历史 parity 偏差的最大未知数，阶段目标可能需要按 build 分组重排（缓解：按 ninja build 命中频率优先实现高频 aura）。

---

## M4 进攻深水区（~5 人周）

**目标**：MH/OH 与暴击双 pass、全乘区补齐、技能 DoT、触发数据接线——进攻 parity 冲 70%。

**具体任务**：

- **归因 RFC 先行**（P17）：pass = TraceGraph 子图、combineStat = 合并节点；评审通过后实现 `calc/hand_pass.rs`（MH/OH 双 pass + combineStat）与 `calc/crit_pass.rs`（暴击/非暴击双聚合）。
- ModFlags 扩位（~30 位，weapon_types.json 驱动派生，feature-gated 双跑切换）；canDeal 门控。
- Double/Triple Damage 乘区（ScaledDamageEffect 全乘区）；技能 DoT（granted_effect_stat_sets 的 dot 基值族 + dotIs* 旗标）；dpsMultiplier/quantityMultiplier 接入 TotalDPS；弩 reload 模型（base_items.reload_time_ms/bolt_count）；LuckyHits。
- `trigger_configs.json` 接线：源速率用计算后攻速（修 14-G2）、命中/暴击折入触发几率、CoC 链路（parity 口径走 PoB2，能量模型 feature-gated，P12）。
- EvalMod tag 第二批（PerStat 读 output/globalLimit）+ mod_db 写侧原语（ReplaceMod/ConvertMod/ScaleAddMod）。

**验收门禁**：进攻 **≥70%@5%**；弩/CoC/双持 fixture；`mod_db_bench` 无回归；ModFlags 双跑 diff 干净后切换。

**对应 high 缺口**：12-G1（MH/OH 双 pass）、12-G2（ModFlags 位宽）、12-G3（暴击双 pass）、12-G4（Double/Triple 乘区）、12-G5（技能 DoT）、12-G6（dpsMultiplier/quantityMultiplier）、14-G1（触发 configTable）、14-G2（触发源速率）、10-G3（余下 tag）、16-G4（部分：弩 reload 消费侧）。

**风险**：R8 双 pass × 归因模型冲突（缓解：RFC 前置 + 评审为合并前置条件）；R6 性能——双 pass 让热路径计算量近似翻倍（缓解：bench 门禁 + 只读快照并行铺垫）。

---

## M5 minion / 物品编辑态 / special 分批数据化（~6 人周，3 个 worktree 并行）

**目标**：三条独立战线并行——召唤物链路、special_mods 分批迁移 + statdesc、pobr-item 落地 + 树字段。

**具体任务**：

- **(a) 召唤物**：`minions.json`/`spectres.json` 入库（MonsterVarieties 反范式化）+ minion actor 编排 + createMinionSkills（走 granted_effects.minion_list 外键）+ mirage 框架（`mirage_configs.json`）。
- **(b) special + statdesc**：`special_mods.json` 按 ninja 命中频率分批迁移（先 keystone/高频 unique 词条），handler 覆盖清单跑通（未映射告警）；statdesc 渲染链路——先离线验证（渲染结果 vs PoB2 导出文本逐行 diff 达标）才作为 mods.json 的 rendered_lines 生产列（R5 缓解）。
- **(c) 物品编辑态 + 树**：pobr-item 落地（variant 门控/applyRange + `mod_scalability.json`/`uniques.json`/`runes.json`/`catalysts.json`/武器局部 mod 结构化结算/Weapon2 局部词条）；树字段消费（is_attribute/options/isSwitchable ReplaceNode/WeaponSet 条件/`node_effect.rs` 节点效果缩放管线/Grants Skill）。

**验收门禁**：召唤/幻影 build 扩入 parity 集（先建 baseline 后入门禁）；radius 珠宝 attribute 误计数专项回归；BuildRaw 往返等价 golden fixture（P16，编辑态无 parity 可依）；unsupported 词条率下降曲线纳入报表；special 迁移条目 oracle 抽样对拍。

**对应 high 缺口**：14-G3（minions 数据入库）、14-G4（minion build 接线）、14-G5（createMinionSkills）、15-G1（statdesc 链路）、10-G2（specialModList——本阶段分批主体，M6 收尾）、16-G1（variant 门控）、16-G2（applyRange/ModScalability）、16-G3（武器局部 mod）、17-G1（武器组）、17-G2（节点效果缩放管线）、17-G3（isSwitchable）。

**风险**：R4 special 2085 条验证成本（缓解：分批 + verified:false 元数据 + 长尾留 Unsupported）；R5 statdesc 渲染污染下游（缓解：离线 diff 达标才转生产列）；R9 编辑态无 parity（缓解：BuildRaw 往返契约）；三线并行的合并冲突（缓解：每表独立文件、按域分 worktree）。

---

## M6 解析规则全量数据化 + stat_id 直通（~4 人周，战略战役）

**目标**：兑现 P3 终局——ModParser 六表入 JSON、parser 重写为数据驱动 scan 引擎；建立 stat_id 第二通道；**第二次 version-bump-drill = 终局验收**。

**具体任务**：

- extract-lua 抽 ModParser 六表 → `mod_parser_rules.json`（forms 91→27 form_id / name_map 776 / flag_phrases 202 / pre_flags / tag_phrases 684；special 已并入 special_mods.json）。
- mod_parser 重写为模块目录：`scan.rs`（最早+最长匹配，载入期建 aho-corasick 索引）+ `forms.rs`（27 种 form 求值 enum）+ `template.rs`；签名 `parse_mod(text, &ParserRules)`。
- tools/precompile-mods 产 `generated/parsed_mods.json` + 解析覆盖率 CI 报表；pob2-oracle 对 18-build 全词条做 parseMod differential test。
- stat_id→Modifier 映射表（P10 双通道），按域（先 passive_tree 再 mods）跑双通道 diff 报告。

**验收门禁**：全部 parse 测试通过 + 18-build 语料 parse diff=0 + 解析覆盖率入 CI 报表 + parity 零回归 + parse bench 退化 ≤10%（parsed_mods 缓存兜底）；stat_id 通道 diff<0.1% 后才允许按域切换。**第二次 version-bump-drill**：新版本只跑 pipeline→adapter→extract-lua→precompile 四步，Rust 零改动编译通过、parity 集可运行；固化为 `devs/scripts/version-bump-drill.sh`。

**对应 high 缺口**：10-G1（六表硬编码——终局解决）、10-G2（special 收尾：覆盖率驱动的长尾批次）。

**风险**：R3 抽取正确性/vendor 漂移（缓解：oracle differential 终裁 + CI drift diff）；R6 性能（缓解：aho-corasick + parsed_mods 缓存 + bench 上限）；重写已验证 parser 的回归面（缓解：新旧 parser 双跑全语料 diff=0 才切换——这正是把它放到 M6、语料与 oracle 工具齐备之后的原因）。

---

## M7 长尾与超越（持续，不设总验收）

树分配/寻路（alloc.rs BFS）、FullDPS 多技能（兑现并行性能叙事）、treeVersion 迁移、display_stats/calc_sections 数据化完善 + minionDisplayStats/extraSaveStats、trade_stat_map、能量元宝石双口径 fixture（P12 超越模式）。每项独立 parity-gated 合入。

---

## 附 A. high 缺口 → 阶段映射总表

| 缺口 ID | 领域 | 摘要 | 主责阶段 | 备注 |
|---------|------|------|----------|------|
| 10-G1 | mod 系统 | ModParser 六表硬编码 Rust | **M6**（M0 先行小查表） | P3 节奏裁决 |
| 10-G2 | mod 系统 | specialModList 2085 条缺失 | **M5b**（M6 收尾长尾） | 按 ninja 命中频率分批 |
| 10-G3 | mod 系统 | EvalMod tag 5/20 种 | **M3**（actor 系）+ **M4**（PerStat/globalLimit） | P7 |
| 11-G1 | 编排 | buff/aura/curse perform 阶段缺失 | **M3** | 影响面最大 |
| 11-G2 | 编排 | doActorMisc 内建 buff 表缺失 | **M3** | buff_definitions.json |
| 12-G1 | 进攻 | MH/OH 双 pass + combineStat | **M4** | 归因 RFC 前置（P17） |
| 12-G2 | 进攻 | ModFlags 仅 5 位 | **M4** | feature-gated 双跑 |
| 12-G3 | 进攻 | 暴击/非暴击双 pass | **M4** | 同 RFC |
| 12-G4 | 进攻 | Double/Triple Damage 乘区 | **M4** | |
| 12-G5 | 进攻 | 技能 DoT 缺失 | **M4** | 数据列 M1 已入库 |
| 12-G6 | 进攻 | dpsMultiplier/quantityMultiplier | **M4** | |
| 13-G1 | 防御 | taken-as 转换管线 | **M2** | |
| 13-G2 | 防御 | reducePoolsByDamage 扣池 | **M2** | pool_damage.rs |
| 13-G3 | 防御 | MoM/EB/ES bypass | **M2** | poolProtected 原语 |
| 13-G4 | 防御 | EHP 口径不同 | **M2** | P11，baseline 独立 commit |
| 13-G6 | 防御 | 资源转换矩阵 + keystone | **M2** | keystone_registry |
| 14-G1 | 触发/召唤/异常 | 触发 configTable 61 项 | **M4** | trigger_configs.json |
| 14-G2 | 触发/召唤/异常 | 触发源速率用基础 use_time | **M4** | |
| 14-G3 | 触发/召唤/异常 | Minions/Spectres 未 JSON 化 | **M0**（schema/通道）+ **M5a**（入库） | |
| 14-G4 | 触发/召唤/异常 | 召唤物未接 build 链路 | **M5a** | |
| 14-G5 | 触发/召唤/异常 | createMinionSkills 缺失 | **M5a** | minion_list 外键 M1 入库 |
| 14-G6 | 触发/召唤/异常 | 非伤害异常消费闭环 | **M3** | |
| 15-G1 | 数据管线 | statdesc 渲染链路缺失 | **M5b** | R5 离线验证先行 |
| 15-G2 | 数据管线 | SkillStatMap 固化为启发式 | **M1** | = 18-G3，双跑切换 |
| 15-G3 | 数据管线 | Export 手工策展无通道 | **M0** | skill_overrides.json |
| 15-G4 | 数据管线 | Misc 常量/百级表硬编码 | **M0** | 搬迁不变式 |
| 15-G5 | 数据管线 | 宝石 qualityStats 缺失 | **M1** | = 18-G1 数据面 |
| 16-G1 | 物品 | variant 词条无门控 | **M5c** | |
| 16-G2 | 物品 | applyRange/ModScalability 缺失 | **M5c** | mod_scalability.json |
| 16-G3 | 物品 | 武器局部 mod 覆盖不全 | **M5c**（local_mods.json M0 先行） | |
| 16-G4 | 物品 | BaseItemDef 缺关键字段 | **M1**（落库）→ **M2/M4**（消费） | block/spirit→M2，reload→M4 |
| 17-G1 | 天赋树 | 武器组节点未解析 | **M5c** | |
| 17-G2 | 天赋树 | 节点效果缩放管线缺失 | **M5c** | node_effect.rs |
| 17-G3 | 天赋树 | isSwitchable ReplaceNode 缺失 | **M5c** | 需 pipeline 补 .dat 表 |
| 18-G1 | 技能/宝石 | gem quality 四层皆空 | **M1** | |
| 18-G2 | 技能/宝石 | support 适用性裁决断裂 | **M1** | |
| 18-G3 | 技能/宝石 | SkillStatMap（同 15-G2） | **M1** | |
| 18-G4 | 技能/宝石 | 多 statSet 未建模 | **M1** | |
| 19-G1 | 配置/展示 | ConfigOptions apply 语义未建模 | **M3** | config_interpreter |
| 19-G2 | 配置/展示 | customMods 不导入 | **M3** | |
| 19-G3 | 配置/展示 | enemyIsBoss 不读 XML | **M0** | 数据已就位的纯接线 |

## 附 B. parity 目标轨迹（@5% 容差，ninja_parity 18-build）

| 阶段 | 进攻 | 防御 | 关键事件 |
|------|------|------|----------|
| 现状 | 24% | 51% | — |
| M0 | 24%（逐值不变） | 51%（逐值不变） | 可再生性 CI 上线 |
| M1 | **≥40%** | 51% | statmap 双跑切换 |
| M2 | ≥40% | **≥80%** | EHP 口径切换 |
| M3 | **≥55%** | **≥85%** | 第一次 version-bump-drill |
| M4 | **≥70%** | ≥85% | 归因 RFC + 双 pass |
| M5 | 召唤/物品 build 扩入 parity 集 | — | unsupported 率下降曲线 |
| M6 | parse diff=0、零回归 | 零回归 | **第二次 version-bump-drill（终局验收）** |

## 附 C. 风险登记簿（与 20 文档 §5/§6 联动）

| ID | 风险 | 触发阶段 | 缓解（已写入流程） |
|----|------|----------|--------------------|
| R1 | 模板 DSL 复杂度膨胀（最大架构风险） | M3/M5b/M6 | DSL 硬边界入 review checklist；≥20 条目闸门；handler 计数 <100 监控 |
| R2 | "理论正确"重构破坏隐藏补偿（已发生一次，FINDINGS 04-02） | 全阶段 | 搬迁不变式；parity 仲裁；feature-gated 双跑；baseline 独立 commit |
| R3 | extract-lua 抽取正确性 / vendor 漂移 / 部分检出 / 许可 | M0/M3/M6 | luajit 执行而非正则；CI drift diff；oracle 终裁；overlay 头部记 commit；许可与 vendor 同等对待 |
| R4 | special 2085 条验证成本 | M5b/M6 | 分批 + verified:false + 长尾 Unsupported；不追求 100% JSON 化 |
| R5 | statdesc 渲染污染下游（mods 文本列/parsed_mods/craft） | M5b | 离线逐行 diff 达标才转生产列；长期 stat_id 直通降权文本通道 |
| R6 | 性能回退与数据体积（data/ 将达 ~30MB+） | M4/M6 | aho-corasick 索引；parsed_mods 零解析热路径；懒加载+分片；bench ≤10% 上限；必要时 bincode 边车 |
| R7 | schema 频繁演化（七个阶段都改 catalog） | 全阶段 | 新字段一律 `#[serde(default)]`/Option；manifest 按域记 schema 版本；loader 容忍缺表；不保证旧 data 前向兼容 |
| R8 | 双 pass × 归因模型冲突（PoBR 核心卖点的最大模型扩展） | M4 | RFC 前置评审；禁止 M2 顺手改归因 |
| R9 | pobr-item 编辑态无 parity 依据 | M5c | BuildRaw 往返等价契约 + golden fixture |
| R10 | 工作量 ~27 人周，期间可能发 0.6 大版本 | 全阶段 | M0 后即可低成本吸收数值型补丁；机制型按 P2 判据增量排期；M6/M7 可后置不损失 M0–M5 收益 |
| R11 | "零回归"与"提升"目标张力（M1–M4 既修对又改输出） | M1–M4 | 行为修复附 PoB2 一手依据 + oracle 中间值；baseline 更新独立 commit 显式审查 |
