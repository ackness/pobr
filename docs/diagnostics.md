# 诊断工具与逐行台账

排查流程的概要见[维护者指南的“排查数值差异”](maintainer-guide.md#排查数值差异诊断闭环)与 [AGENTS.md](../AGENTS.md#accuracy-workflow)。本文说明每个工具输出的含义、台账的状态与登记清单、配置覆盖，以及参考口径和回归语料的细节。命令的参数与缓存见[开发与 CI 工作流](development-workflow.md#慢构建与按需报告)。

## 输入在哪里丢失

`AdditionalRingSlot` 和 `GemlingQuality` 由现有构筑预处理读取天赋文本，分别决定第三戒指栏与宝石附加品质效果，不依赖 ModDb 查询。诊断只在来源为对应的无条件天赋行、且实际预处理判定启用时将其列为 `outside`；自定义词条或其他来源的同名 flag 不能据此消除未读取提示。

一个输入可能在四处丢失，引擎必须把每一处都记下来，而不是让数值悄悄偏小：

| 环节 | 丢失方式 | 记录位置 |
|------|----------|----------|
| 导入（XML → `Build`） | 物品块被拒、物品所在装备槽没有模型 | `ImportReport::skipped_items` / `unmodelled_slots` |
| 文本 → Modifier | 没有规则能解析的词条行（物品、珠宝、已分配天赋 `Tree:<节点>: …`、配置行 `Config: …`）；只读懂一部分、留有未解析余文的行（`Parsed` 但 `unparsed` 非空）同样不应用，按原文记入 | `unsupported_modifier_texts`（台账 `unparsed`） |
| 宝石 stat → Modifier | stat map 有条目但引擎拒绝（未知 mod 名、不支持的 tag）；完全没有条目的 stat 多数是展示或动画数据，按所属宝石列出供查找缺失的机制 | `Diagnosis::unmapped_stats` / `unknown_stats` |
| Modifier → 计算 | 存入的名字从未被查询；依赖的 `Multiplier` 没有任何地方赋值（含从角色快照读取的 `actor: player` 类，例如 “per non-Idol Augment in your Equipment”）；等待的条件不在条件表里，也没有计算阶段推导它 | `Diagnosis::unread_modifiers` / `unprovided_multipliers` / `unset_conditions` |

`pobr_build::diagnose` 运行同一条计算管线并返回以上全部，外加实际计算的主技能及其等级（技能或等级选错不会丢输入，所以单独列出）。它打开 `ModDb::track_reads` 与 `CalcConfig::track_reads` 并以 Compare 模式记录 stat 映射，结果与普通计算逐位相同（`diagnosis_does_not_change_the_result` 对全部已入库构筑断言），但更慢，只用于排查。

逐行台账 `pobr_build::usage::line_usage`（CLI `calculate-build --usage [--json]`、WASM `line_usage_json`）在同一次诊断上回答“构筑的每一行词条用上了没有、用在哪里”：计算收到的每一行（物品各区段、肢体、药剂与护符、珠宝、天赋节点、任务奖励与自定义词条、配置项，以及宝石 stat）列一次，带来源 id、持有者名字（节点名、物品基底）、解析出的每个 modifier 和它的去向：`applied`（被计数；`used_in` 列出它进入的 stat）、`partial`、`inactive`（名字被读取但没被计数，或已验证的读取方只在本次未到达的状态下运行；`reason` 给出缺的条件或标志，例如 “needs Attack”；后者另带 `read_gate: { state, config_option }`，没有配置入口时 `config_option` 为 null；台账的 `condition_configs` 另列被引用条件的配置入口，包含已设置为 false 或由计算阶段推导而不在 unset 列表里的条件）、`unread`（没有任何查询问过这个名字）、`outside`（在 ModDb 之外消费：物品自身的防御 / 武器伤害、宝石等级与品质、珠宝半径，`reason` 写明读取方；宝石等级与品质行还写明提升了哪些技能，一个都没提升时为 `unread`）、`no_effect`（登记为不影响任何输出的 S0 条目，或本构筑无人询问的配置项）、`unmodelled`（有规则识别但不产生 modifier，机制未建模）、`unparsed`、`no_modifier`（解析出 modifier 却没有存入，台账应当学会说明原因）。给召唤物的词条（`MinionModifier`）按召唤物自己的计算是否读取了它给出的名字判断，构筑没有召唤物时为 `unread`。`Condition:<X>` / `Multiplier:<X>` 类 modifier 经条件表消费：有 modifier 等待 `<X>`、或计算阶段问过 `<X>`（`CalcConfig::read_variables`）时算作用上。行的去向由编排在写入来源时记录（`SourceWriter::note_line`，`LineFate`）；在 ModDb 之外拿走或丢弃词条行的新代码（剥离本地词条、按文本读取）要同时调用 `note_line` 并写明读取方，否则台账会把这些行报成未解析或没有 modifier。`mise run triage` 带 `--usage` 运行每个构筑，把未建模的行、没有任何构筑读取的名字、无人设置的条件和被拒绝的宝石 stat 跨构筑汇总，不论该构筑有没有不一致的行。

条件不在条件表里时，有两类不是线索：计算阶段自己推导、成立时才写入的（暴击的一半、主副手、怒气与战斗、敌人的冰缓与感电、诅咒与标记、Elusive、Surrounded、所用武器组、内置 buff 附带的条件、`AffectedBy*`、`Using*Flask` / `Using*Charm`，见 `calc/session.rs::CONDITIONS_SET_BY_A_PASS`），以及编排按构筑本身设置、换一个构筑才会成立的（武器类型、盾、主技能的战斗条件、首领、同伴、普通胸甲、辅助宝石颜色，见 `diagnosis.rs::SET_BY_THE_ORCHESTRATION`）。其余条件中有配置项能设置的标为 “config: off”，由用户决定是否打开；没有任何来源的才是丢掉的输入。新增由计算阶段或编排设置的条件时登记进对应清单，否则其他构筑会误报它。

## 批量排查与缺口工具

排查顺序（先批量、后单个；完整流程见 [AGENTS.md](../AGENTS.md#accuracy-workflow)）：

1. `mise run triage` 一次计算基准组与 tuned 组的全部构筑和钉定的 PoB2（也可给组目录或构筑目录），把与构筑码参考值不一致的每一行按今天的 PoB2 分类：`engine`（PoB2 与参考值一致而 PoBR 不一致）、`both`（三方都不同）、`reference`（PoBR 与 PoB2 一致，参考值出自另一版导出工具，不用改）、`known`（`devs/scripts/known-differences.json` 已解释的有意差异；PoB2 额外保留护身符授予技能的精魄也自动归入）。随后把 engine / both 行的线索跨构筑汇总：按来源与类型对齐后只有一边有或数值不同的词条、PoBR 有却没生效的词条、伤害因子与敌人的抗性 / 穿透 / 承伤、最大承伤与 EHP 背后 PoB2 的资源池、精魄总量与逐技能保留，以及没有规则能解析的词条（按模板合并，按涉及的构筑数排序）；再从每个构筑的逐行台账汇总计算丢掉的输入：有规则识别却未建模的行、存入后没有任何构筑读取的名字（先列没有读取方的）、无人设置的条件、被拒绝的宝石 stat。EHP 与最大承伤的行另列两边逐类型的敌人伤害、承受的部分、资源池和命中次数中不同的项，以及 PoB2 一侧敌人获得的额外伤害与它的来源（oracle 的 `enemyMitigation.gainMods`）。每个构筑另列本构筑存入却没有读取、名字与不一致的行同词的词条（例如 `EffectiveBlockChance` 旁的 `ReplaceShieldBlock`）：只有少数构筑有的名字排不进跨构筑的清单。每个构筑另列两边按不同等级或品质计算的技能（PoBR 取自 `--usage` 账本的 `skills`，PoB2 取自 oracle 的 `skillLevels`）：技能等级改变 buff、光环、诅咒的数值，却不丢任何输入，其他线索都指不出它。按涉及构筑最多的原因成批修复，整批修完再跑一次（PoB2 一侧按构筑码缓存，与 `compare` 共用）。holdout 组只用 `mise run accuracy` 测量，`triage` 拒绝它。
2. 分诊解释不了的行，再单独看那个构筑：`mise run diagnose <构筑>`。第 1 节是导入损耗，第 2 节是与构筑码自带参考值的差异，第 3–9 节是丢掉的输入（第 6 节是等待无人设置的条件的词条，配置项能设置的条件另列为 “config: off”）。多数差异在这里就能对上原因。
3. 看一个 stat 怎么算出来：`cargo run -p pobr-cli -- calculate-build --file <code.txt> --breakdown <Stat,...>` 打印计算步骤和读到的每条词条（`--reference-basis` 按参考值口径，`--json` 输出 JSON；加 `--usage` 列出没有完全用上的每一行及原因）；技能、stat、基底、天赋或暗金在游戏数据里的记录用 `mise run lookup <id 或名字>`，不必逐个打开数据文件。
4. 清单解释不了的差异，用 `mise run compare <构筑> [Stat ...]` 对照钉定的 PoB2：每个 stat 两边的总值与比值、BASE / INC / MORE 合计，以及按来源（天赋节点、装备槽、技能、角色基础与属性、配置、任务奖励）对齐后只有一边有或数值不同的词条。不给 stat 时比较构筑码参考值中不一致的行。`ActionRate` 在 PoB2 有 `HitSpeed` 时与它比较（命中频率，不是施放速率），没有时才用 `Speed`；召唤物的总值以 `Minion:<stat>` 一并列出。PoB2 的结果按构筑码缓存在 `.cache/pob2-oracle/`，重跑只重算 PoBR（`--refresh` 两边都重算）。更细的中间值仍可直接用 `tools/pob2-oracle/run.sh`。
5. 参考值不是真值。构筑码内的 `<PlayerStat>` 是导出工具当时的结果（版本未知，可能早于当前数据）；PoB2 自身也有假设和缺陷。两边不一致时，以游戏自己的词条文本和数据判断谁对，再决定改哪一边。导出工具缺失的词条用 `exporter: "absent"` 与游戏来源登记，让参考口径自动排除，不降低基线、不按构筑登记 known differences；导出工具口径不同的规则用 `"differs"` 保留并报告。其余确认为有意差异的行才写进 [pob2-differences.md](pob2-differences.md) 与 `devs/scripts/known-differences.json`。

同一类行反复需要逐个构筑人工排查时，说明分诊或诊断的输出缺了东西：扩充工具，让下一轮批量运行直接给出这类线索。

找下一批机制缺口时用 `mise run gaps`：把当前数据的 `generated/modifier-audit.json` 与 benchmark、tuned 的 `--usage` 台账按数字替换为 `#` 的模板合并，先按丢掉输入的构筑数，再按 unique、passive、mods、compound、jewel_node、bases 的来源优先级及来源数排序。纯 translation 来源不入队；`pobr_gap` 是 PoB2 能解析而 PoBR 未完整处理的线，`upstream_gap` 是两边都不完整，`recognized_empty` 是识别却未产出 modifier，`no_effect` 是已登记不影响输出的条目（供审查，不代表待实现）；队列只收录有审计来源的游戏文本；研究卡也可查询只见于构筑台账的文本，此时标为 `ledger_gap`。队列与 triage 共用全部 worked 构筑的读取并集：`dropped` 只计未解析、未建模、解析后未存入、没有任何 worked 构筑读取的名字，以及没有已登记状态、配置入口或技能范围要求解释的 inactive；名字在别的 worked 构筑读过则归 `unused`（本构筑用不上），有配置入口的 inactive 归 `config-gated`，已登记状态的 inactive 归 `state-gated`（包括 `SET_BY_THE_ORCHESTRATION` 与 `CONDITIONS_SET_BY_A_PASS` 中由构筑或计算阶段决定的条件，如武器类型、辅助宝石颜色与严格多数，以及整个 `AffectedBy*` buff／光环条件族；台账用 `condition_states` 列出被引用的这些条件，研究卡解释为 `condition set from the build: <name>`；这些条件即使有配置入口也归 `state-gated`），所选技能不满足的 flag、keyword 或 SkillType 要求归 `skill-mismatch`，条件与阈值不归入此桶；命中词条在技能确实产生命中时复用带 `Hit` 的命中上下文解释，所以 Thin Ice 报未满足的 `EnemyFrozen` 配置条件，而不是缺少 `Hit`。其余已处理的输入归 `handled`。标题打印各桶的模板／构筑对数，同一行同时有缺口与门控时会进入多个桶；排序只使用 `dropped` 的构筑数。默认隐藏只有门控、技能不匹配、未使用或已处理输入的模板，未在构筑出现的审计缺口仍以零构筑数保留；`--all` 同时显示这些行及每桶计数。`--kind` 按来源过滤，`--limit` 只限制文本行数，`--json` 保存所选队列的全部模板、变体、审计字段与构筑台账。命令默认构建一次 CLI，不运行 PoB2；可用 `--binary <pobr>` 显式复用已构建程序。台账与 `triage` 共用 `.cache/build-usage/`，按构筑码、程序内容、收集脚本及数据文件路径/大小/修改时间失效；`--refresh` 强制重算。两条命令只读 worked 组，不读 holdout。

从队列选中一行后运行 `mise run gap "<模板或文本>"`，一次列出数字变体、从 `overlay/stat_descriptions.json` 反查的单 stat 或 compound 全部成员（逐项确认 `base/stats.json` 是否存在）、每个来源的完整词条文本（含暗金其他行与咒符触发隐式）、节点名、每个携带它的 worked 构筑及台账去向和桶（包含门控原因）、引擎名字和 special/mod_parser 近似规则。引擎与规则候选仅按关键词匹配，不证明支持；暗金与节点的 poe2db 链接标为猜测，未经联网验证。确认游戏含义时可再把卡片上的稳定 id 交给 `mise run lookup <id>`。

已入库构筑覆盖不到的技能用 `mise run skills:sweep`（`devs/scripts/skill-sweep.py`）：对活动数据里每颗非辅助宝石（含 meta）的所有非辅助主动效果，按 `GemEffects` 显示顺序枚举，保留来源宝石与主效果，以 `mainActiveSkill` / `mainActiveSkillCalcs` 选择效果；行标识为 `主效果/所选效果@序号`，主／附加效果分别计数，弹药装填不能代替射击覆盖。最小合成构筑使用 90 级 Witch、20 级零品质宝石、普通武器基底，同时计算 PoBR 与钉定 PoB2，按技能族汇总 `agree` / `known` / `differs` / `pobr_zero` / `pob2_zero` / `both_zero` / `error` / `pob2_other_skill`；纯指令与占位效果另列。扫描不读 holdout；两边实际效果均须核对，vendor 缺少效果或附加效果的来源宝石不算吻合。`both_zero` 不证明 buff、无载荷幻影、无事件触发或召唤物已实现；召唤物只看一个主召唤物，不代表整支召唤物队伍。`known` 只按所选 stat set 的平均伤害显示／无伤害 stat、本指南列明的技能模型或可重算解释 DPS 比值的伤害区间取整分类，保留两边数值与原因、从技能族的未解释差异排名中排除；平均伤害显示只豁免 `CombinedDPS`，其余字段仍须吻合，取整须在 0.1% 内解释 DPS。平均命中因减伤口径不同只展示、不分类；速率优先对照 PoB2 的 `HitSpeed`，并对照 CLI JSON 的 `firing_rate` / `reload_time`（`FiringRate` / `ReloadTime`）。每效果只测第一个 stat set，不穷举辅助、触发载荷、尸体、充能、目标与战斗条件，不混合轮转或过热。完整扫描写 `.cache/skill-sweep/report.json` / `report.txt`，`--skill` 定向扫描默认写 `report-selected.json` / `report-selected.txt`，显式 `--json` / `--text` 优先；保留完整轮次再做定向扫描。结果缓存在 `.cache/skill-sweep/`，oracle 缓存键含 XML、vendor commit 与 wrapper 内容。

侧栏字段的覆盖与数值吻合分开衡量：`python3 devs/scripts/output-coverage.py` 用已构建的 `target/debug/pobr`，按 `display_catalog` 的 PoB key 逐字段对照 oracle 的完整输出（默认 benchmark、tuned 与 demo 三组，拒绝 holdout），给出每个字段两边都有有限值的构筑数、非零样本数、5% / 10% 内的吻合数和最差的构筑；`--oracle-cache` 只读已有缓存（键与 `compare-build.py` 相同），`--refresh-missing` 补算缺少的。没有样本不算验证通过，大量零值吻合也不证明非零的机制正确。

## 读取方、登记与解析边界

特殊 Modifier 规则在编译时递归校验 flags、标签字段/作用对象、捕获、handler 和运算。
`precompile-mods --check` 校验运行时合并后的完整集合，并输出文件哈希、顺序和规则来源；
`refresh-modifiers.sh` 在审计和发布产物前执行此检查。`StatId` 仍是开放名称，解析通过不证明计算支持。
覆盖语义与边界见[维护者规则说明](contributing-mods.md#7-validate-test-and-open-a-pr)。
词条审计（`tools/precompile-mods/src/audit.rs`）先用运行时的显示文本清理（`mod_parser::engine::strip_display_markup`）展开游戏链接与样式，再去掉物品注记：`[Thorns|Thorns]`、`[Strength]` 这类术语是词条的一部分，删掉会改变机制。
审计的 `text_channel`（`gem_property` / `granted_skill`）标出由其他文本入口读取的行（宝石等级与品质、授予技能），只说明这一行有去向，不说明授予的技能都已实现。

A line the parser reads only in part is not applied. A parse that returns `Parsed`
with a non-empty `unparsed` remainder (for example “2% more Mana Flask Recovery”,
which is read as 2% more maximum Mana, “1% more Life Flask Recovery” as 1% more
maximum Life, and “1% increased speed of Recoup Effects” as a `LocalEffect` increase)
used to be applied by ingest as a wrong modifier. Every ingest path now
goes through `ParseOutcome::into_applicable`: item sections, flasks and charms,
passive nodes, gem and support-gem stat texts, free `extra_modifier_texts`, jewel
texts and local requirement lines apply a line only when `ParseOutcome::is_complete`
holds; otherwise the original line is kept in `unsupported_modifier_texts` and
the usage ledger reports it as `unparsed`, exactly like a line no rule matched. A
remainder that is only display residue is fixed in the parser rule, not tolerated at
ingest; the benchmark and tuned groups had none. Silently dropped pre-flag tags
(`dropped_pre_flag_tags`, audit class `Partial`) are a separate, unchanged diagnostic.

“#% increased Damage per Curse on you” (`damage_+%_per_active_curse_on_self`)
scales by the PoBR config input `multiplierCursesOnSelf`
(`Multiplier:CurseOnSelf`); with no count the line is inactive with that config
gate. A reader blocked by a numeric `MultiplierThreshold` also reports a
config gate when the catalog declares its multiplier setter and setting that
one boundary satisfies every other requirement. Cadiro's unspecified arrow
therefore remains inactive/config-gated, rather than an implementation gap.
Unknown setters, unrelated skill/condition gates, conflicting thresholds,
non-finite inputs and stored zero/false keep their existing classification.
The calculation and its actual unmet reason are unchanged.
The ledger finds count gates from the modifier's zero-scaling
`Multiplier` tags and the config catalog. Static gate labels are borrowed;
dynamic labels and config ids are owned, with the same JSON string fields.
Plain and removable Power/Frenzy/Endurance charge counts share the calculation's
input mapping, so their zero-scaling ledger gates point to the catalog's matching
`use<Kind>Charges` option (with `override<Kind>Charges` setting the enabled count).

“未被读取”是线索不是结论：攻击技能不读法术伤害是正常的。报告用全部已入库构筑的读取并集区分“任何构筑都不读（没有读取方）”和“本构筑的技能用不上”。

新增按名字消费 modifier 的代码用 `ModDb::named(name)` 或聚合查询，不要用 `iter_mods().filter(名字)`：后者不算读取，诊断会把有读取方的名字报成没有。确实在数据库之外消费的名字（按文本扫描、构筑结构）登记在 `diagnosis.rs::READ_OUTSIDE_THE_DATABASE` 并写明消费者。只在某状态下读取的名字登记在旁边的 `READ_UNDER_A_STATE`，写明实际读取方、状态及可设置它的配置 id；例如 slow potency 只在角色受减速且没有减速免疫时读取，Archon 共享加成等待 `conditionAffectedByArchon`。每个登记名字必须在 `tests/skills/line_usage.rs` 中用状态关闭／开启的构筑验证 `inactive` → `applied`，新增登记没有 fixture 时测试失败；不能用登记掩盖没有读取方的机制。

配置行指任务奖励与自定义词条。其中解析不了的行以 `Config: <行>` 记入未支持清单，其余行照常生效；任务奖励选项的 “None”（未领取）不是词条行。对任何计算输出都没有影响的词条在 `overlay-common/special_mods.json` 里登记为只识别、不产生 modifier 的条目（`"mods": []`、`"batch": "S0"`，例如游戏 stat `experience_gain_+%` 的 “5% increased Experience Gain”；除 `unmodelled` 外的其他批次必须产生 modifier 或挂 handler，`pobr-gamedata` 的 `non_s0_has_traceable_provenance` 检查），它不是丢失的输入；尚未实现的机制不能用这种条目消掉，必须留在未支持清单里。规则能识别、但机制尚未建模的行登记为 `"batch": "unmodelled"`（同样 `"mods": []`，`source_note` 写明缺的模型，例如 “per-hit Armour Break is not modelled”、“remnant count not modelled”、“kill rate not modelled”）：台账报 `unmodelled`，审计报 `recognized_empty`，缺口队列照常列出它，不会当作无效果隐藏。

## 配置覆盖

`mise run config:coverage [--json <文件>]` 按配置分区报告 `read` / `handler` / `implied` / `unread` / `no_effect`：复用 benchmark、tuned 的 `.cache/build-usage/` 读取并集，条件与倍率另用台账的 `condition_reads`、`multiplier_reads`、`referenced_variables` 和计算阶段条件清单；列出未覆盖技能、引擎及活动数据中没有精确名字引用的项、未注册 handler、stub / context gate，以及空 effects 中已有 scalar 读取方与丢失的机制。`--binary <pobr>` 复用已构建 CLI，`--refresh` 重算台账；不读 holdout。这是读取覆盖，静态名字引用与 handler 注册都不证明选项改变了数值；近似名字不是别名，已穿透／连锁次数不能映射为穿透／连锁上限。

Config coverage also reports `not_in_game` and excludes it from the relevant
`(read + handler) / options` denominator. `devs/scripts/config_relevance.py`
checks required skills against game effect IDs/displayed names and skill-gem
item names; other controls are excluded only when their mechanic words lack
base-game evidence or inherited overlay item/stat-description text. The overlay
text is exporter-derived, not an independent game-data source. Ambiguous matches
and legacy stats stay in the denominator; this is not proof of playability.
`python3 devs/scripts/config_relevance.py` regenerates the versioned Web filter,
and workflow tests check it against the snapshot. Static computed-name leads
(such as `ShockVal`) are not observed reads. `detonateDeadCorpseLife` supplies
an explicit nonnegative finite corpse Life, including zero, to existing corpse
damage shares (`corpse_explosion_monster_life_permillage_physical` / `_chaos`);
absent, negative or nonnumeric values retain the enemy-level life table. It
does not change enemy Life or add corpse damage to unrelated skills.

## 参考口径、导出工具规则与语料

与构筑码参考值比较的入口（`ninja_parity`、`reference_report`、`golden_canary`）统一用 `DataOrchestratorOptions::reference_conditions()`，它由两部分组成：`against_reference_enemy()`（EFFECTIVE 口径、Pinnacle 首领，引擎的模型全部开启）和 `on_reference_basis()`（关掉导出工具没有的模型：meta 宝石里的法术按自身速率；箭雨每当敌人能再被命中时就算一次命中；球与风暴始终覆盖目标，`periodic_uptime = false`）。PoBR 新增一个导出工具没有的模型时，把它的开关加进 `on_reference_basis()`；独立词条规则用下述 coverage 字段，不逐构筑降基线。模型本身由游戏数值的测试验证，不靠参考值。`mise run diagnose` 的第 2 节按参考口径比较，其余各节（丢失的输入、读取并集）来自 `against_reference_enemy()` 下的计算，否则只有该模型读取的词条会被误报成“没有读取方”。CLI `calculate-build` 的 `output` 是游戏口径（`output.energy_trigger` 给出能量上限、充能技能和触发频率），`reference` 块在这种构筑上改按自身速率比较并以 `at_own_rate_of_use` 标明；主技能是箭雨时 `output.rain` 给出落箭间隔、命中间隔和每几支箭命中一次，`reference` 块按导出工具的计法比较并以 `rain_hits_whenever_allowed` 标明；主技能是球或风暴时以 `periodic_hits_assume_sustained` 标明。

PoBR 自有词条规则声明 `exporter: "absent"`（导出工具未实现）或 `"differs"`（已实现但口径不同），并填 `game_source`（游戏 stat id、原文或链接）；缺省与抽取的 `vnd_*` 规则都是 `"same"`。`overlay-common/special_mods.json` 由 gamedata 验证；stat-id 规则在 `rules/skill_stats.rs::exporter_rule` 登记，stat id 本身就是来源。modifier 持有共享的规则元数据，替换物品/宝石 origin 时仍保留，嵌套 payload 也一样。`on_reference_basis()` 设置 `exporter_absent_rules = false`，在写入数据库、展开 buff 之前去掉 absent；differs 保留。游戏口径、Web、CLI `output` 与 `--usage` 保留全部规则，CLI `reference.exporter_absent_rules_omitted` 标明比较时的排除。

`mise run accuracy` 与 triage 的 “beyond PoB2” 按规则列出出现的构筑数、改变的构筑数与行数：固定其余参考模型，每次只恢复一条 absent 规则，按数值变化计数（不是 5% 吻合阈值）；多规则影响同一行时不能相加。differs 在两种口径都开启，列出但模式变化为零。holdout 只输出计数，不输出构筑与行的身份。这些数值用于观察实现影响，正确性仍由游戏数据的回归测试保证。

`examples/demo-bd-test/builds/` 是回归门禁语料，构筑抓取于 2026-06（上一赛季开服第一周），不包含之后的物品和机制；门禁全绿只说明这批构筑没有回退。当前赛季的构筑分三组，都用构筑码自带的参考值衡量：`examples/wasm-benchmark/builds/`（8 个）和 `examples/reference-builds/tuned/`（每个升华三个：2026-10-13 抓的一批和之后的两批 holdout，都在 2026-10-15 并入）用来找差异、修引擎；`examples/reference-builds/holdout/`（每个升华另一个，2026-10-15 第二次抓取）只测量，不对它运行 `diagnose` 或 `compare`，也不按它的差异挑选要修的内容，它的总数回答“没人调过的构筑上有多准”。`mise run accuracy` 不带参数打印三组的总数。`reference_agreement_does_not_regress` 断言每组的吻合数（伤害吻合的构筑数——主技能召唤召唤物的构筑看召唤物的 `TotalDPS`——和吻合的行数）不低于 `reference_report.rs` 里记录的基线。改动提升了吻合数就上调基线，不为通过而下调；数据版本变化使数字移动时按 `ninja_parity` 的做法重录。导出工具未实现的词条规则标为 `exporter: "absent"`，由参考口径自动排除，游戏实现与参考基线分别验证；其他参考值被游戏数据证明有误时，在基线旁记录具体依据与影响。这个门禁防的是退步，不证明正确：参考值可能本身有误，构筑也可能因为错误的原因吻合。新增的展示字段让构筑码里更多 `<PlayerStat>` 行进入比较，行数随之变化不代表原有计算更准。需要新一轮目标或新赛季开始时，用 `mise run builds:fetch` 重新抓取（见 [reference-builds](../examples/reference-builds/README.md)）：原来的 holdout 并入 tuned，新抓的一批作 holdout。
