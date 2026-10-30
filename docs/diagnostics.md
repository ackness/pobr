# 诊断工具与逐行台账

先批量找共同原因，再看单个构筑。流程概要见[维护者指南](maintainer-guide.md#排查数值差异诊断闭环)
和 [AGENTS.md](../AGENTS.md#accuracy-workflow)。Modifier 是供计算读取的词条，ModDb 是保存这些词条的数据库，stat 是游戏属性标识。
本文说明报告含义；参数与缓存见
[开发工作流](development-workflow.md#慢构建与按需报告)。

## 输入在哪里丢失

导入成功和词条能解析都不保证它已参与计算。引擎分别记录四个环节：

| 环节 | 可能丢失的内容 | 记录位置 |
|---|---|---|
| XML → Build | 被拒物品、没有模型的装备栏 | ImportReport::skipped_items／unmodelled_slots |
| 文本 → Modifier（计算词条） | 未解析行，或只解析一部分、仍有余文的行 | unsupported_modifier_texts；台账 unparsed |
| 宝石 stat → Modifier | stat-map 有条目但引擎拒绝，或根本无条目 | Diagnosis::unmapped_stats／unknown_stats |
| Modifier → 计算 | 名字从未查询、倍率无人赋值、条件无人设置 | unread_modifiers／unprovided_multipliers／unset_conditions |

未支持文本包括物品、珠宝、`Tree:<节点>: <行>` 和 `Config: <行>`。
无映射的宝石 stat 多数是显示／动画资料，需按所属宝石确认，不能一律视为机制缺陷。
缺倍率也包括 `actor: player` 的角色快照倍率，如每个非 Idol 增幅物的计数。

`pobr_build::diagnose` 走普通计算管线，额外开启 ModDb／CalcConfig 读取跟踪和 Compare 映射记录，
返回上述问题及实际主技能／等级。选错技能或等级不会丢输入，所以单列。
`diagnosis_does_not_change_the_result` 对入库构筑核对结果逐位相同；跟踪更慢，只用于诊断。

### 每行词条的状态

`pobr_build::usage::line_usage`（CLI `calculate-build --usage [--json]`，WASM `line_usage_json`）
回答每行输入用在哪里。它列物品各区段、肢体、药剂／护符、珠宝、天赋、任务奖励、自定义块、配置、宝石 stat，
带来源 id、持有者名、解析出的 modifier 和状态。

| 状态 | 含义 |
|---|---|
| applied | 已计入，used_in 列出进入的 stat。 |
| partial | 一行中只有部分 modifier 被使用；不等于允许应用有未解析余文的文本。 |
| inactive | 名字有读取方，但本次条件、标志或状态不满足。reason 说明原因。 |
| unread | 没有查询读取该名字，或宝石等级／品质文本没有提升任何技能。 |
| outside | 在 ModDb 外消费，如物品本地防御／伤害、宝石等级／品质、珠宝半径；reason 写明读取方。 |
| no_effect | 已确认不影响任何输出的 S0，或本构筑无人询问的配置项。 |
| unmodelled | 规则认得文本，但机制尚未建模，不产生 modifier。 |
| unparsed | 没有完整解析整行，不应用任何已解析片段。 |
| no_modifier | 解析出 modifier 却没有存入；需要让台账解释原因。 |

inactive 可带 `read_gate: { state, config_option }`，无控件时 config_option 为 null。
`condition_configs` 列引用条件的配置入口，包含已设 false 或由计算推导的条件。
给召唤物的 MinionModifier 按召唤物是否读取内部名字判断，没有召唤物时为 unread。
`Condition:<X>`／`Multiplier:<X>` 经条件表消费：有其他词条等待 X，或计算问过 X
（CalcConfig::read_variables）时算读取。

编排用 `SourceWriter::note_line`／`LineFate` 记录 ModDb 外的去向。
新增本地词条剥离或按文本读取时须登记消费者，否则台账会误报 unparsed／no_modifier。
宝石等级／品质行还列明提升了哪些技能。
AdditionalRingSlot、GemlingQuality 由构筑预处理读天赋原文，分别决定第三戒指栏和附加品质效果。
只有对应无条件天赋行且实际启用时才标 outside；其他来源同名 flag 不能据此消除未读提示。

### 条件未设置不一定是缺陷

计算会在条件成立时才写入某些值，其他 BD 没有这些条件是正常现象。
`calc/session.rs::CONDITIONS_SET_BY_A_PASS` 登记计算阶段条件，如主副手、怒气、战斗、暴击分支、
敌人冰缓／感电、诅咒／标记、Elusive、Surrounded、武器组、内建 buff、AffectedBy*、Using*Flask／Using*Charm。
`diagnosis.rs::SET_BY_THE_ORCHESTRATION` 登记由构筑决定的条件，如武器类型、盾、主技能、首领、
同伴、普通胸甲、辅助颜色。新增条件须登记，避免其他构筑误报。

其余条件有配置入口时显示 `config: off`，由用户决定。没有任何来源时才是缺输入线索。
未读也只是线索：攻击不读法术伤害是正常的。跨全部已用构筑的读取并集可区分“本次用不上”和“所有样本都没读”。

## 批量排查与缺口工具

### triage、diagnose 与 compare

1. `mise run triage` 一次计算 benchmark、tuned 与钉定 PoB2。可指定组或构筑目录；拒绝 holdout。
   它按共同原因汇总，先修影响多个构筑的原因，整批后再跑。
2. 只有 triage 解释不了的行，才用 `mise run diagnose <构筑>`。
   第 1 节导入损耗，第 2 节参考差异，第 3–9 节丢输入；第 6 节含未设置条件，配置可设置者另列 config: off。
3. 看公式用 `mise exec -- cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown <Stat,...>`，
   加 --reference-basis 用参考口径，--json 输出 JSON，--usage 查看逐行去向。
   查游戏记录用 `mise run lookup <id 或名字>`。
4. 再用 `mise run compare <构筑> [Stat ...]` 对照 PoB2 的总值、比值、BASE／INC／MORE 和逐来源词条。
   省略 stat 时取参考值中不一致的行；ActionRate 优先对 HitSpeed，否则 Speed。
   召唤物为 `Minion:<stat>`。更细值可用 tools/pob2-oracle/run.sh。
5. 以游戏文本和数据判断谁对，不把参考或 PoB2 当真值。有意差异记录到
   [PoB2 差异](pob2-differences.md)与 devs/scripts/known-differences.json。

triage 按三方数值把差异行分为：

| 分类 | 含义 |
|---|---|
| engine | PoB2 与构筑码参考值一致，PoBR 不同。 |
| both | 三方都不同。 |
| reference | PoBR 与当前 PoB2 一致，来源参考来自另一版本，通常无需改引擎。 |
| known | 已登记的有意差异；PoB2 额外保留护身符授予技能精魄也自动归入。 |

engine／both 的汇总包括：按来源／类型对齐的词条差异、已存未生效词条、伤害因子、敌人抗性／穿透／承伤、
资源池、逐技能精魄，以及按模板合并的未解析行。EHP／最大承伤还比敌人伤害、转换、资源池、命中次数，
并列 PoB2 `enemyMitigation.gainMods` 的额外伤害来源。
每个构筑还列本地未读且与差异 stat 同词的名字（如 EffectiveBlockChance／ReplaceShieldBlock），
以及等级／品质不同的技能（PoBR usage.skills，PoB2 skillLevels）。
这些变化可能不丢输入，不能只看跨构筑高频缺口。

triage 对每个构筑收集 usage，另汇总未建模、所有样本均未读的名字、无人设置条件与拒绝的宝石 stat，
不要求构筑一定有数值差异。PoB2 缓存在 `.cache/pob2-oracle/`，与 compare 共用；
重跑只重算 PoBR，--refresh 重算两边。同类问题反复逐个排查时，应补工具输出。

### gaps 与 gap

`mise run gaps` 合并当前 generated/modifier-audit.json 和 benchmark／tuned 的逐行台账。
数字替换为 # 形成模板。先按丢输入的构筑数，再按来源（unique、passive、mods、compound、jewel_node、bases）
优先级和数量排序。纯 translation 不入队，队列只收有审计来源的游戏文本。
它是诊断清单，不是必须实现的待办；是否建模仍按实际输出需要判断。

| 审计分类 | 含义 |
|---|---|
| pobr_gap | PoB2 能解析，PoBR 未完整处理。 |
| upstream_gap | 两边都不完整。 |
| recognized_empty | 识别但没有 modifier。 |
| no_effect | 已登记无输出的条目，供复查。 |
| ledger_gap | 仅在构筑台账见到；可用研究卡查询，不是审计队列来源。 |

gaps 与 triage 共用所有 worked 构筑（可用于调查的 benchmark／tuned 样本）的读取并集，按实际去向另分桶：

| 桶 | 判定 |
|---|---|
| dropped | 未解析、未建模、解析后未存、所有样本均未读，或无已知状态／配置／技能要求可解释的 inactive。 |
| unused | 名字在其他 worked 构筑读过，本构筑用不上。 |
| config-gated | inactive 有配置入口，可由玩家设值。 |
| state-gated | 由已登记的构筑／计算状态决定，优先于配置分类，含 AffectedBy*。condition_states 列出这些条件。 |
| skill-mismatch | 所选技能不满足 flag、keyword、SkillType；条件与阈值不放这里。 |
| handled | 其余已处理输入。 |

命中技能的命中词条用带 Hit 的真实上下文判断。例如 Thin Ice 应报告 EnemyFrozen 配置条件，而非缺 Hit。
同一行可同时进入多个桶，标题显示模板／构筑对数，排序只用 dropped 构筑数。
默认隐藏只有条件限制、技能不匹配、未使用或已处理的行；未出现在构筑中的审计缺口仍以 0 构筑保留。
--all 展示全部桶，--kind 按来源过滤，--limit 只限文本行数，--json 保存完整所选队列与变体／台账。
命令默认构建一次 CLI，不运行 PoB2，可用 `--binary <pobr>` 复用程序。
usage 缓存为 `.cache/build-usage/`，按构筑码、程序内容、脚本及数据路径／大小／mtime 失效，--refresh 强制重算。
两条工具不读 holdout。

`mise run gap "<模板或文本>"` 输出数字变体、stat_descriptions.json 对应的单 stat 或 compound 全部成员，
逐个核对 base/stats.json，列来源完整文本、节点名、携带该行的 worked 构筑、台账桶与原因。
引擎名和 special／mod_parser 候选只是关键字近似，不证明支持；poe2db 链接是未联网验证的猜测。
再用 `mise run lookup <稳定 id>` 查游戏记录。

### 技能与输出覆盖

`mise run skills:sweep` 对活动数据每颗非辅助宝石（含 meta）的所有非辅助主动效果测试，
按 GemEffects 顺序保留来源宝石和主效果，以 mainActiveSkill／mainActiveSkillCalcs 选择。
行名为 主效果/所选效果@序号，主附效果分计，装填不能代替射击。
合成构筑用 90 级 Witch、20 级零品质宝石和普通武器，比较 PoBR 与钉定 PoB2。
分类为 agree、known、differs、pobr_zero、pob2_zero、both_zero、error、pob2_other_skill，
纯指令／占位另列。来源或附加效果在 vendor 缺失不算吻合。

both_zero 不能证明 buff、无载荷幻影、无事件触发已实现；召唤物只看一个主召唤物。
known 只适用于所选 stat set 的显示／无伤害规则、已写明的模型或可在 0.1% 内重建 DPS 比值的范围取整。
保留值和原因，排除技能族的未解释差异排名。平均伤害显示只豁免 CombinedDPS，其余指标仍比。
平均命中因口径不同只展示，不分类；速率优先 HitSpeed，并比 firing_rate／reload_time。
每效果只测首个 stat set，不穷举辅助、载荷、尸体、充能、目标或战斗条件，不混轮转和过热。

全扫描写 `.cache/skill-sweep/report.json`／report.txt，--skill 默认写 report-selected.json／txt，
显式 --json／--text 优先。保留完整轮次再做定向。oracle 缓存键含 XML、vendor commit 和 wrapper。
该扫描不读 holdout。

`mise exec -- python3 devs/scripts/output-coverage.py` 用已构建的 target/debug/pobr，按 display_catalog.pob_key
对照 benchmark、tuned、demo 的 oracle 完整输出，拒绝 holdout。
逐字段报告有限值样本数、非零样本、5%／10% 吻合数和最差构筑。
--oracle-cache 只读 compare 共用缓存，--refresh-missing 补缺。
没有样本不等于通过，大量零值吻合也不证明非零机制正确。

## 读取方、登记与解析边界

### 完整解析与规则分类

编译时递归校验特殊规则的 flag、tag 字段／对象、捕获、handler、运算。
precompile-mods --check 校验运行时合并后的全规则，输出哈希、顺序和来源；
refresh-modifiers.sh 在审计／发布前执行。StatId 是开放名，解析通过不证明计算支持。
见[规则验证说明](contributing-mods.md#7-validate-test-and-open-a-pr)。

词条审计先用 strip_display_markup 展开游戏链接／样式，再去物品注记。
`[Thorns|Thorns]`、`[Strength]` 是词条内容，不能删术语。
text_channel 的 gem_property／granted_skill 说明有其他文本入口，不证明授予技能已实现。

每条接入路径都经 ParseOutcome::into_applicable，只在 is_complete 时应用整行。
例如 `1% more Life Flask Recovery`／`2% more Mana Flask Recovery` 可能只读成最大生命／魔力，
余下 Flask Recovery；这时整行记入未支持和 unparsed，不能应用错误片段。
物品、药剂护符、天赋、宝石／辅助文本、extra_modifier_texts、珠宝、本地需求都遵守此规则。
仅显示残留要修解析规则，不在接入处放行。
`1% increased speed of Recoup Effects` 现在已有独立规则，不再作为部分解析反例。
dropped_pre_flag_tags（审计 Partial）仍是另一项诊断。

`damage_+%_per_active_curse_on_self` 按 multiplierCursesOnSelf／Multiplier:CurseOnSelf 缩放；
无计数时 inactive 并指向该配置。零倍率的 Multiplier tag 结合目录寻找计数配置入口。
数字 MultiplierThreshold 也可报告配置入口，但要求目录有 setter，且只设这一边界就能满足其余要求。
因此 Cadiro 未指定箭矢计数是 config-gated，不是缺模型。
未知 setter、无关技能／条件、冲突阈值、非有限值和已存 0／false 仍按实际原因分类。
标签静态或动态所有权不改变 JSON 字符串字段。
普通／可移除 Power、Frenzy、Endurance 计数共用实际输入映射，指向 `use<Kind>Charges`，
`override<Kind>Charges` 设置启用后的计数。

### 新读取方怎样登记

按名字消费用 ModDb::named(name) 或聚合查询，不用 iter_mods().filter(名字)，后者不记录读取。
数据库外消费登记 diagnosis.rs::READ_OUTSIDE_THE_DATABASE 并注明消费者。
仅某状态读取的名字登记 READ_UNDER_A_STATE，给出读取方、状态和配置 id，
如受减速时的 slow potency，或等待 conditionAffectedByArchon 的共享加成。
每个登记必须在 tests/skills/line_usage.rs 用关闭／开启状态验证 inactive → applied，不能拿登记掩盖未实现。

任务奖励和自定义行解析失败时为 `Config:<行>`，其他行照常生效，None 不产生行。
所有输出均不受影响的规则可用 mods: []、batch: S0（如经验获取）；
non_s0_has_traceable_provenance 要求其他批次有 modifier 或 handler，unmodelled 除外。
未实现机制不能用 S0 隐藏，应保留诊断。
认得但未建模用 mods: []、batch: unmodelled，source_note 写缺的模型，如逐命中破甲、残片计数、击杀速率。
台账显示 unmodelled，审计为 recognized_empty，缺口队列仍列出。

## 配置覆盖

`mise run config:coverage [--json <文件>]` 按分区报告 read、handler、implied、unread、no_effect。
它复用 benchmark／tuned 的 usage 读取并集及 condition_reads、multiplier_reads、referenced_variables 和条件清单，
列缺技能、无精确引用、未注册 handler、stub、上下文限制，以及空 effects 中的 scalar 读取／缺失机制。
--binary 复用 CLI，--refresh 重算，不读 holdout。
这只说明读取覆盖；静态引用、handler 注册、近似名字都不证明选项改变数值。
已穿透／连锁次数不能当穿透／连锁上限别名。

not_in_game 从 `(read + handler) / options` 的相关分母排除。
config_relevance.py 核对所需技能的游戏效果 id／名称及宝石物品名；其他控件只有在基础游戏或
继承的 overlay 物品／stat 描述都无机制词证据时才排除。overlay 来自导出工具，不是独立游戏来源。
歧义和旧 stat 留在分母，这不证明可玩性。运行该脚本可再生成版本化 Web 筛选，工作流测试检查与快照一致。
ShockVal 等动态名字线索不算观察到的读取。

detonateDeadCorpseLife 可给现有物理／混沌尸体伤害份额
（corpse_explosion_monster_life_permillage_physical／_chaos）提供非负有限生命，包含 0。
缺失、负值或非数字继续用敌人等级表；不改敌人生命，不给无关技能加尸体伤害。

## 参考口径、导出工具规则与语料

### 两种计算口径

ninja_parity、reference_report、golden_canary 使用 DataOrchestratorOptions::reference_conditions()：
against_reference_enemy() 选 EFFECTIVE、Pinnacle、全部引擎模型，
on_reference_basis() 再关闭导出工具没有的模型。
参考口径让 meta 内法术按自身速率，箭雨每到允许间隔就命中，球／风暴始终覆盖目标（periodic_uptime = false）。
新增独立模型把开关加入 on_reference_basis，单条词条用下面的 exporter 标记；模型仍需游戏数值测试。

`diagnose` 第 2 节用参考口径，丢输入部分用全部模型开启的 against_reference_enemy，避免误报只被新模型读取的词条。
CLI output 用游戏口径，energy_trigger 列容量、事件来源和频率；reference 可另算并标
at_own_rate_of_use、rain_hits_whenever_allowed、periodic_hits_assume_sustained。
箭雨 output.rain 保留落箭／命中间隔及每几箭命中一次。

### exporter 规则

自有规则填 game_source 并声明 exporter: absent（导出工具未实现）或 differs（口径不同），
缺省与 vnd_* 为 same。gamedata 校验 special_mods，stat-id 规则登记在 skill_stats.rs::exporter_rule，
stat id 本身是来源。规则元数据随 modifier 共享，换 origin 或嵌套 payload 也保留。
on_reference_basis 设置 exporter_absent_rules = false，在入库和 buff 展开前去掉 absent，保留 differs。
游戏口径、Web、CLI output／usage 保留全部；reference.exporter_absent_rules_omitted 标明排除。
单条 absent 不逐构筑登记 known 或降基线，其他有意差异才写差异文档和清单。

accuracy／triage 的 beyond PoB2 固定其余参考模型，每次恢复一条 absent 规则，统计出现构筑、
数值改变构筑和行数，不用 5% 阈值。多规则影响同一行不能相加；differs 两边都开，模式变化为 0。
holdout 只报计数，不报身份。影响统计不证明正确性。

### 语料与基线

- examples/demo-bd-test/builds/ 是 2026-06 上季首周的回归语料，不覆盖后续物品／机制。
- examples/wasm-benchmark/builds/ 的 8 个和 reference-builds/tuned/ 用于调查修复。
  tuned 含每升华三份，来自 2026-10-13 及后续两批，在 10-15 合并。
- reference-builds/holdout/ 是 2026-10-15 第二次抓取的每升华另一份，只测量，
  不运行 triage／diagnose／compare，不按其差异挑修复。accuracy 默认报三组总数。

reference_agreement_does_not_regress 要求每组吻合构筑数／行数不低于 reference_report.rs 基线。
主技能召唤构筑的伤害看该召唤物 TotalDPS。提升后上调，不能为通过下调；
升数据版需按 ninja_parity 重录，其他参考错误必须有游戏依据并记录影响。
门禁防回退，不证明真值；新增展示字段使更多 PlayerStat 行进入比较，也不等于计算更准。
新目标／赛季用 builds:fetch，旧 holdout 转 tuned，新批作 holdout，见[参考构筑说明](../examples/reference-builds/README.md)。
