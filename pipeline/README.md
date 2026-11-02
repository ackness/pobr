# PoBR 数据管线（pipeline）

本目录声明 PoBR 构建所需的固定输入，将 RePoE 游戏事实、翻译库语言产物和 PoBR 自有政策
适配、组合为既有 DTO／运行 schema。完整输入与 `data/<poe-version>/` 运行目录仅临时物化；
尚未迁移的字段继续保留明确固定的 vendor／社区来源，不称为已从客户端自主再生。

当前存储职责：游戏数据归RePoE、语言数据归翻译库，PoBR只提交代码、需求／版本锁、来源元数据及小型合成夹具；CI构建和需要实物的测试在临时目录导入数据并生成发布资产。
四份完整快照、七份外部shared、历史大policy及完整语言／API样本已迁出当前Git tree；无旧PoBR数据artifact回退。本地活动目标保持0.5.5.5，Git历史不改写。详见[存储边界](../docs/data-architecture.md#1-架构决策与三仓库职责)。

[最新完整发布同步](latest-data.md)通过两owner发布索引发现候选，冻结完整组合，再由精确head的PR／Full门禁与实际最终包消费验证后条件采用。普通构建继续按不可变锁重放；索引bootstrap、官方版本观察和owner源码更新均不代表新数据已生成或已采用。

## 活动数据来源与生产

| 域 | 当前来源 | 生产与保留边界 |
| --- | --- | --- |
| 物品基底／词缀／Stat／技能宝石 | 固定自有RePoE客户端DAT事实 | [普通领域adapter](../tools/pobr-data-adapter/ordinary-facts.md)接回原DTO；未拥有字段及旧编解码仍保留兼容 |
| 宝石品质 | 目标GrantedEffectQualityStats／GrantedEffects／Stats事实 | [品质生成说明](gem-quality/README.md)；计算仍按固定effect-wide策略，不认证全部细分scope |
| 词条显示文本 | 固定客户端CSD＋明确保留的兼容文本 | [普通描述生产者](../tools/pobr-data-adapter/ordinary-stat-descriptions.md)、树／符文renderer；未表达项保留诊断或整项回退 |
| 被动天赋树 | 目标DAT／Default PSG事实与固定兼容字段 | 普通树已迁源；placement／armour等未拥有语义保留，历史官方Git树候选入口另列下文 |
| 特殊制作目录 | 独立RePoE七表中立事实，PoBR筛选／聚合 | [必需临时输入](crafting-inputs.md)保留八槽与未表达原值诊断；来源语言缺失保持unknown；新元数据已本地采用，不代表完整效果或概率 |
| 规则／目录／翻译 | PoBR自有规则、固定vendor／社区目录、固定语言库与客户端文本投影 | 逐项记录来源和适用边界；保留输入不代表全部自主生产，SC客户端身份与CN数值不因中文显示而认证 |

`.dat` 存的是 **id / 数值 / 外键关系**（规范化数据库表）；显示文本在 `StatDescriptions` 里。
列名/表名见 [poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema)。

## 固定来源与独立树候选

独立客户端候选的可重复流程是[固定补丁索引定位](client-index.md) →
[未审阅原始采集](client-source-draft.md) → 来源审阅与固定 →
[严格重放](client-sources.md) → RePoE 事实导出与 PoBR 领域适配。
索引定位、采集、重放与来源包生成由独立RePoE实现并测试；PoBR只按固定三文件验包接口验证输入，见[ordinary接入](ordinary-inputs.md)。这些步骤不自动提升来源锁，也不切换活动数据。历史 Git 树也由 RePoE 独立保存和发布，PoBR 不再直接获取官方原始树。

[sources.lock.json](sources.lock.json)保留历史官方树输入及固定客户端资源位置、原始SHA-256、大小、decoder／schema和采集证据。
`acquisition_acceptances`分别接受F77、六DAT、91 tree／IT、10 CSD、12语言原生资源，以及精确六OT／两描述DAT字节；
`retained_input_acceptances`另记locale／shared／compat三类有界保留决定。各项只接受明确scope和pins，不认证全部机制、SC客户端身份或分发许可。
本地活动版本现为`0.5.5.5`，完整双语runtime `dba0615e`；旧`0.5.5.4.2`由独立回滚锁按需恢复。
采集、领域解释、消费者验证、本地采用及远端发布仍分开；完整状态见[架构现状](../docs/data-architecture.md#5-当前已有数据与实际缺口)。
下方`data:tree:source`／`data:tree:candidate`按[独立历史树输入锁](tree-inputs.md)消费 RePoE 包，不是当前普通树的再生流程。
PoB2 的测试参考版本独立登记在 [devs/reference-sources.json](../devs/reference-sources.json)，
`mise run vendor` 不再读取生产 parser 元数据选择参考版本。

```bash
mise run data:tree:source
mise run data:tree:source --offline
mise run data:tree:candidate --offline --out .cache/tree-proof/first
mise run data:tree:candidate --offline --out .cache/tree-proof/second
```

历史诊断入口固定的 `bd87e651` 输入有已确认的来源缺口：Druid 的 `overridePairs` 引用了
`nodes` 中不存在的 `19680`、`55194`（分别指向替换项 `40837`、`57601`）。
这些记录在已核对的多个官方版本中均悬空；是否属于游戏中已删除的节点尚未证实。
直接将原始文件交给严格 adapter 仍会拒绝生成；旧完整再生成链也需要先解决其来源问题。

上面的诊断候选入口通过 [固定修正清单](tree-corrections/bd87e6512c92b868542eddfb1ba4ea8b6dc2da36.json)
隔离这两条无法应用的映射。清单同时绑定源提交、原始哈希和精确旧值，并进入来源 lock；
只有基础节点在节点表、替换表、边、分组和解锁引用中均不存在时才可应用。
原始缓存与替换定义完整保留，不补造节点、位置或新 ID。其他缺失引用继续由 adapter 报错。
候选 receipt 保存 `quarantined_override_pairs`、未解决状态、修正文件与规范化输入的摘要，
也记录预处理代码摘要并检查生成期间是否变化。这只允许继续研究已有图，不计作完整来源验收。
最终确认需要匹配补丁的客户端 `.psg`、`PassiveSkills` 与 `ClassPassiveSkillOverrides`；
本候选没有因此获得生产发布资格。

导入命令按[精确 Git 锁](tree-inputs.lock.json)获取 RePoE 的 `data.json`／`manifest.json` 包，校验归属、原来源身份、ZIP／成员 pins 和完整成员集合，临时物化到 `.cache/tree-inputs/<lock-sha256>/`。原 `.cache/sources/` 裸文件不能代替该包，损坏的导入不会联网修复。
`--offline` 不联网；缺包、错 pin 或下载失败即失败，不回退到官方 URL、latest 或其它版本。
候选命令用 Rust adapter 生成 `base/passive_tree.json` 与 `base/passive_tree_meta.json`，
记录来源 lock、adapter/数据类型与构建输入的摘要；重放结果不包含机器路径或当前时间。
Cargo 使用 `--locked`；`--offline` 同时禁止源下载和 Cargo 联网，不会为生成候选更新 lockfile。
这些任务不准备 vendor、LuaJIT 或 PoB2 oracle。

`--out` 必须是不存在且位于正式 `data/` 之外的路径。候选只含树域，带有
`_domain_candidate.json`，没有产品 manifest；封存和发布入口也会拒绝此标记。
当前固定输入的上游说明是 0.5.5，其与产品活动补丁的精确适配关系尚未核实，`game_patch` 保持 null。
生成时目录使用上游版本作为 adapter 标签，不表示已确认客户端补丁。
这些命令不会覆盖活动数据或修改 `CURRENT`。生产切换还需补丁适配、完整域/消费者与数值验证。
当前 native variants 仍沿用基础节点的拓扑和升华归属；替换后的独立升华归属、通用属性选择与
vendor 推导的 armour 标记尚未迁移。历史树 adapter 继续保留，不能将此候选入口视为完整替代。

## 固定客户端原始资源与事实候选

原始客户端采集、索引定位及来源包生成请在独立RePoE运行其`client:source`、`client:draft`、`client:locate`、`source:rebind`任务。
入口、默认strict及显式diagnostic边界见[客户端资源获取](client-sources.md)。

RePoE 原生事实包另由 `pobr-data-adapter --native-facts` 验证外部 manifest 摘要后形成
PoBR 领域候选，见[候选契约](../tools/pobr-data-adapter/native-facts.md)。当前只接通部分国际服事实，
保留完整原始事实引用和诊断；它不是运行时包，也未完成品质、几何或全部关系的语义验证。
候选消费与旧生产再生入口仍分别存在，不能宣称生产链已退出 PoB2。

### 普通完整 BuildData 候选

`pobr-data-adapter --ordinary-facts` 已将固定自有 RePoE 事实接回原普通 DTO，见
[普通生产入口与领域命令](../tools/pobr-data-adapter/ordinary-facts.md)。技能、基底、树、符文、
描述、actor 与基础字段的目标来源和固定兼容按各领域 receipt 分开，不能将单个文件全部算作自主。
这些领域输出经既有 schema-3 组包和封存后可由普通 GameData/BuildData 消费；
上方独立树域与 schema-4 受限原生候选仍是不同入口，不代表普通完整路径的现行覆盖范围。

本地活动快照`data/0.5.5.5`有67个清单文件及manifest，加18份未变shared；manifest `73b2d77e`／完整runtime `dba0615e`。制作目录现为必需的独立事实投影；92条业务记录与原基线相同，仅来源元数据改变。
它保留简繁两种词典和历史树。精确7e4d0c23完整CI将当前组合投影为同一SC紧凑release `9932ea87`／runtime `ec4f3776`（82个虚拟文件）；产物归档摘要与block身份已核验，新来源锁保留在构建收据。
六域双次生产、356条开发输入的TypeScript／WASM API流程及三对CLI结果沿用各自固定证据；完整双语候选另核一个CLI／WASM构筑和采用／回滚演练。
本地默认CLI读取新身份，Web同步86个运行文件；本地复用固定CLI，未重编译Rust／WASM或重跑原356条API流程。精确7e4d0c23的完整CI另执行Rust分片、真实WASM／Web与运行组包，E2E未选。
描述、继承tags、actor字段及保留兼容的分母和关闭条件见[架构现状](../docs/data-architecture.md#5-当前已有数据与实际缺口)。
本地CURRENT采用已完成；PoBR218／219／221、翻译库5和RePoE7／8已合并，仍未部署。新增制作投影的独立审阅语言P2已修复，artifact已发布；新组合经fresh远端Git、禁网重放与默认固定CLI等价检查后本地采用。PR221轻量及完整CI通过；原浏览器／WASM与356语料证据保留原身份。

语言库的完整显示目录可经[显示侧表投影](native-display-texts.md)接入普通 Web
`translateLines`：精确来源查询、跨表同名冲突和缺译保留，不改原始构筑文本或计算。
侧表已列入当前活动manifest；全部字段UI消费、真实审核译文及CN语义仍分别验收。审核补译入口只消费精确绑定的已审核记录，当前真实包为0条，草稿不生效。

已有版本化事实、CSD 描述和官方树可通过[固定 recipe 的原生候选统一入口](native-candidate.md)
与显式绑定的预编译 adapter 离线组包：`mise run data:native:candidate`。
该入口只编排现有 `--native-runtime`，封存 recipe 与完成收据；不获取原始来源、不发布或切换活动数据。

## 运行时数据组合封存

运行包的版本文件与共享规则可独立封存并核验，见[运行时数据组合封存](runtime-bundle.md)。
PoBR 所需外部表/显示字段及固定 Git 产物分别由 JSON 需求与锁声明；
目前已接通原生显示候选的 CI 获取/校验和精简投影，见[定向数据导入](data-imports.md)。
[普通领域输入声明](ordinary-inputs.md)另行固定事实归档、CSD、政策和supplement，
Full CI从Git获取压缩输入后顺序调用既有adapter生成六域候选；原始大表不进入玩家块。
需求 JSON 的 `runtime_pack.ordinary` 指定六域可替换的八个路径；独立 CI 组包任务核对
完成收据／领域收据及业务字节后，与固定保留输入、显示候选合成三块。
[精确提交663d9055的完整CI](https://github.com/ackness/pobr/actions/runs/41376591565)已通过六域生产、真实WASM／Web、声明运行组包及Full gate；部署仍未执行，本地完整快照采用见上文。
该入口保存已存在的完整运行组合，不替代下述领域来源与发布验收。

## 词条更新与回归检查

普通解析修复或同步当前固定版本的 PoB2 规则：

```bash
mise exec -- bash pipeline/refresh-modifiers.sh
```

命令依次提取普通规则与 `specialModList`、严格校验完整有效规则、用完整来源语料对照 PoB2、检查上一份审计，
最后更新预解析缓存。它不下载游戏数据、不切换游戏版本，也不修改数值 golden。
只检查已有数据和引擎时用 `--audit-only`；没有 vendor 时用 `--offline`，报告明确不含 PoB2 对照。
可用 `--data data/<version>` 指定数据，`--baseline <audit.json>` 指定历史审计。

语料来源包括 StatDescriptions 的稳定 stat ID、装备/珠宝/药剂/护符词缀、特殊制作词缀、
基底隐式、传奇装备各变体、天赋树，以及中文导入实际输出的英文模板。
数值范围抽取低/高样本，复合描述使用不同的占位值；特殊规则的 `examples` 也进入语料，无法渲染的条目另计数。
这是**解析样本覆盖率**，包含地图和怪物描述，不能当作玩家机制或 DPS 覆盖率。

产物：

- `data/<version>/generated/modifier-audit.json`：可重现的完整审计与来源，作为下一次比较的快照；不进入 Web 下载清单。
- `.cache/modifier-audit/<version>/validation.json`：严格校验报告，记录 common / version / patch / derived / vendor 文件的 SHA-256、有效规则顺序和覆盖来源；审计的 `rule_validation` 内嵌同一信息。
- `.cache/modifier-audit/<version>/current.delta.json`：退化、新增缺口、已解决和移除条目。
- 同目录的 `corpus.txt` / `oracle.jsonl`：本次 PoB2 对照的输入与输出。

报告中的 `pobr_gap` 表示 PoB2 完整解析而 PoBR 未完整解析，优先检查提取器、枚举、标签映射。
`upstream_gap` 表示 PoB2 也不能完整解析，需查游戏机制并实现对应计算，不能靠空规则消除提示。
`recognized_empty` 单列零 modifier 的规则；有残余文本或丢失条件标签也不计为完整支持。
`no_effect` 是登记为不影响任何计算输出的 S0 规则（`"mods": []`、`"batch": "S0"`）匹配的行，与 `parsed` 一样计为完整支持：从 `parsed` 变成 `no_effect` 不算退化。
没有 oracle 时缺口为 `uncompared_gap`。`mod_names` 便于定位计算消费者，但识别出名称不证明消费者已实现。

每条样本的 `special_rule_id` 关联规则文件来源，`matching_special_rules` 按优先级列出所有候选，多个 ID 表示后续规则被遮蔽。该检查只覆盖样本，不证明任意文本都无重叠。结构化 LIST 等未参与计算的字段在 `non_effective_fields` 单列；现有装备 / 天赋的 `SourceId` 不由规则来源替代。严格校验与合并契约见[维护者规则说明](../docs/contributing-mods.md#7-validate-test-and-open-a-pr)。

以前完整解析的文本退化，或同一来源移除旧措辞后新增无法解析的措辞，会使命令失败。
失败时保留 `.cache` 报告，丢弃候选目录，**不覆盖正式数据和上一份已通过的审计快照**；新增机制缺口单独列出。
既有示例角色覆盖率及数值 parity 门禁仍独立保留。

PoB2 本身把两层工作分开：`Data/StatDescriptions` 等从游戏数据导出文本，
`Modules/ModParser.lua` 将文本映射到 modifier，`Modules/Calc*.lua` 消费这些 modifier。
PoBR 沿用这个边界：提取器自动吸收可表达的规则，人工修正放在 `data/overlay-common/special_mods.json`，
更新数据后自动继承；真正的新机制仍需要实现和数值回归测试。

### 中文词典来源

先运行`mise run setup`准备精确工具和frozen Web依赖；独立语言消费者使用`mise exec --`，不需要PoBR内的npm导出器依赖。

基础名词与既有游戏模板使用 [poe2-en-cn-dict 的存档](https://github.com/vagrant-soul/poe2-en-cn-dict)；新增词条与 PoBR 自维护模板使用下面的独立公开词表。
2026-10-12 原 `addohm/poe2-en-cn-dict` 仓库、API 和固定提交原文件均返回 404；
公开接口无法区分删除与转私有。存档提交 `288d8b9d999c5ea5664149e408847156dbd83ff4`
与原固定提交 `28d683c99600eb407b4e014ccaf8247532fb4607` 的十个生成输入逐字节一致。
这是下载来源恢复，不是中文数据升版；`_meta.json` 继续保留真实来源提交和 2026-07-06 的生成时间。
国际服导出提供繁中；完整游戏表的新版官方简中仍需相应 WeGame 客户端数据，不能用繁转简冒充。独立词表中的腾讯交易文本不等于完整客户端名词表。
当前构建从独立翻译库的固定产物导入，执行 `mise run data:prepare`；完整词典和元数据保持原字节，不在PoBR内重新生成或跟踪上游HEAD。
目标、Git修订、manifest及成员摘要见[构建锁](build-data.lock.json)，临时导入及回滚见[build-data](build-data.md)。
旧简中生成、下载与合并实现已迁至翻译库；PoBR的dictionary／历史兼容生成只调用固定独立producer，当前完整候选／bump则导入owner已发布的固定语言文件，均不解析来源HEAD。九个 SC 成员与 140+2 分层按固定历史输入生成，TW 与紧凑 display 继续独立组合，见[语言生成契约](locale-generation.md)。部分游戏 vendor 再生工具仍待按域退出，现有 processed 包的精确恢复不代表全部客户端自主再生。

### 独立词表与人工翻译维护

[ackness/poe2-zh-cn-stats](https://github.com/ackness/poe2-zh-cn-stats)维护交易模板、语言来源、人工修正及固定语言产物。
更新先在翻译库生成／审阅并发布不可变产物，再由PoBR更新精确锁及验证显示、导入和计算契约；运行时不联网取词典。
现有简繁保留产物不证明官方客户端身份、国服数值或全部译文质量；原生成版本、未知字段及冲突诊断不因目标版本改变而重贴标签。

维护职责已迁移：

- 原 `EXTRA_PAIRS` 的 11 条模板及 `Maelstrom → 漩涡` 名字补丁位于独立库的 `curated.json`，保留原词句、字符串／嵌套占位符、参数顺序和不可变出处。PoBR 不再保存第二份硬编码补丁列表。
- 给已有交易 ID 补译，修改独立库的 `translations.json`；需要明确覆盖现有译文时使用 `overrides.json`；不属于交易目录的模板和已人工核验的多参数模板使用 `curated.json`。
- 技能标签、物品结构行及导入语法仍由 PoBR 按上下文解释，不把 UI 文案或计算规则混入词条表。

辅助定性说明及其`GemEffects.SupportText`配对处理已归翻译库独立存储和维护。
`build-data.lock.json`的`support_display`固定外部manifest、资产和目标catalog摘要；`mise run data:prepare`
在构建／实物测试前导入，物化忽略的`web/src/lib/supportDescriptions.zh-CN.json`，不改GameData快照身份。
当前保留资产的原生成版本仍为0.5.5.4.2；与目标目录SHA完全相同才允许使用，UI仍核`skill_id`和完整英文。
换目录须重新由翻译库配对并更新锁，不能重贴旧版本标签。投影代码与合成测试归翻译库；PoBR验证导入pins、目录绑定和显示回退。
繁体仍是字形转换回退，不宣称官方繁中译文；这些说明不是当前等级／品质数值或机制认证。

自动模板仅支持已验证的零／单数值槽，以 `{0:d}` 避免误进入字符串捕获；多值参数、双向歧义、数值／符号不符等由独立库列为排除项。PoBR 再按实际翻译器的匹配骨架检查新模板与既有中英文模板、其他新模板的冲突，冲突不按输入顺序选第一条。审核过的迁移模板保留原行为，旧模板与七份名词表不因接入而删除。

同一英文模板在旧词典与独立词表中的中文不一致时，采用独立词表的官方译法：只替换中文措辞，保留旧条目的英文模板和各参数槽的格式（符号显示、字符串捕获行为不变）。仅在新模板无歧义、英文除参数格式和空白外完全相同、且新中文不与其他既有模板冲突时替换；人工审核模板、含换行的旧模板、仅字面数值不同的模板不替换。被替换的条目列在 `translation-audit.json` 的 `replaced_legacy`。

`generated/translation-audit.json` 记录新增、替换、重复、排除和稀有名补丁采用情况；它属于维护报告，不进入运行时 manifest 或浏览器包。`i18n/zh-CN/_meta.json.translation_source` 记录实际版本；原 `source` 字段仍指基础名词和模板的真实来源。

首次接入 `0.5.5.4.2` 保留全部 26,074 条原模板和七份原名词表，增加 1,822 条无冲突模板，合计 27,896 条。其中 17 条原模板的中文改用官方译法，总数不变。翻译可用不等于机制已计算：新增英文语料仍由 modifier audit 报告解析缺口，不通过补译掩盖。


## 固定输入、版本与候选

当前输入由[build-data](build-data.md)、[ordinary](ordinary-inputs.md)和[显示导入](data-imports.md)的独立锁声明；
`data/CURRENT`只选择本地运行版本。`config.json`仍绑定历史原表兼容诊断的旧输入，不是latest或活动指针。
PoBR不再安装`pathofexile-dat`、下载原始索引或导出官方原表。官方版本发现和资源采集在独立RePoE执行，
语言在翻译库生成；成功观察到一个补丁号也不能认证全部原料、机制或改变CURRENT。

```sh
mise run data:version                   # 本地选中版本
mise run data:prepare                   # 临时导入当前固定目标，不更改 CURRENT
mise run data:regen --out .cache/ordinary-produced --adapter target/debug/pobr-data-adapter
mise run data:update --lock pipeline/build-data.lock.json --out .cache/update-review
```

`data`和`data:fetch`是当前固定输入prepare的简短入口；不会发现最新版本或重贴旧数据标签。
`data:regen`只生成已声明的普通领域候选，完整封包仍使用`data:pack`，不把六域结果冒称全部自主数据。
完整候选及显式本地采用见[锁驱动更新](locked-update.md)。采用须同时指定`--activate`和预期CURRENT，
保留隔离的版本／GameData／parity门禁、原子选择、失败回滚及同版本目标patch；默认只准备候选。
shared变更须另行迁移；不会自动修改vendor pin、来源接受、数值golden、构建锁或部署。

## 历史本地 DTO 兼容诊断

既有`data:adapt`／`data:adapt:reference`及`data:legacy:regen`保留，用于明确提供的历史原表和固定vendor，
不再负责下载或安装上游导出器。它们不是当前独立来源生产链，旧输入须保持原patch、schema与来源收据。
适配和规则计算仍属PoBR；不能因低层adapter可读取一个数组就把它登记为新的官方事实。
这些命令仍在隔离候选中运行审计及回归，检查原目录和配置未并发改变后才替换本地目标，
`data:adapt`不隐式运行PoB2，reference入口则明确要求已固定的参考环境。

schema-3 manifest封存默认运行字节；维护报告和本地patch不进入玩家包。
`manifest`子命令只封装已核对的本地修改，不替代来源或游戏语义验收。
完整游戏／语言输入、运行目录与缓存均为临时或忽略内容，不进入PoBR Git／data artifact refs。

## Vendor calc-delta 报告（`diff-vendor-calcs.sh`）

版本升级后，用它把「这次补丁改了哪些计算公式 / 数据」变成一份 triage 清单，
取代「翻黄金 → 看 parity 暴跌 → 逐 build 考古」的发现流程。

```bash
pipeline/diff-vendor-calcs.sh <old-sha> <new-sha> [--out <file>]
# 默认输出：devs/docs/audits/vendor-delta-<new-sha[:8]>.md
```

- 两个 vendor pin 各做一次 shallow git checkout 到 `.cache/vendor-delta/<sha>/`
  （gitignore，命中缓存即跳过；用 git checkout 而非 codeload tarball——headless
  抽取的 `HeadlessWrapper.lua` 引导需要完整工作树，tarball 缺文件会导致
  `modLib.parseMod missing`）。
- 报告三节：**Calc 模块 diff**（`Modules/Calc*.lua` + `ModParser.lua` 的 diffstat +
  折叠 hunks）、**数据模块 diff**（`Data/` + `Modules/Data*.lua` 只给按改动量排名的
  diffstat）、**抽取产物 diff**（对两个 pin 跑 `extract-lua --what
  special-mods|parser-rules|uniques`，对生成 JSON 做条目级 add/remove/change 汇总）。
- 软降级（沿用 `regen-all.sh` 的 `soft_step` 精神）：某个 `--what` 在旧 pin 上因
  结构不兼容跑失败时，报告注明缺失原因，脚本不中止。
- 旋钮：`MAX_HUNK_LINES`（单文件 hunk 超此行数只留 diffstat，默认 800）、
  `DATA_TOP`（数据文件排名条数，默认 40）。

## 扩展 / 升版

- 新PoE2版本：在独立RePoE观察并取得精确来源、导出和发布游戏输入；翻译库独立准备语言。
  PoBR审阅并更新对应需求与完整owner锁，先用`data:update --lock <lock> --out <fresh>`准备候选，
  验证来源／消费者／CI后再显式选择CURRENT。计算语义、vendor参考与golden更新分别维护。
- 新数据域：在RePoE固定schema／事实导出器中补齐来源，在PoBR领域adapter中接既有DTO和消费者；
  不再通过PoBR Node导出器或修改旧配置将历史数组冒充新目标。
- stat-set 标签由 `GrantedEffectStatSets.Label` → `GrantedEffectLabels.Text` 直接连接，
  以原始 `_index` 查外键，不按数组位置或显示文字猜测；有引用却缺表、缺行或行号重复时拒绝适配。
  标签为空的旧快照仍可从 `overlay/stat_set_labels.json` 回退；该兼容表继续提供 PoB 构筑码的
  stat-set 编号，不能覆盖已存在的原生标签。此能力不自动更新活动快照。
- 怪物基础抗性由 `MonsterVarieties.MonsterType` → `MonsterTypes.MonsterResistances` →
  `MonsterResistances.Fire1/Cold1/Lightning1/Chaos1` 导出到 `base/monster_resistances.json`，
  按 monster variety id 索引。使用普通稀有度的基础值（0.5 每列只有一个值），合并类型链接的
  全部抗性记录，保留负数弱点；空链接为零。该文件的 `_meta` 保存原始导出收据与三张输入表的
  SHA-256；它目前只提供数据，不改变召唤物计算。
- `GemEffects.GrantedEffectDisplayOrder` 在 `pathofexile-dat@15.2.0` 使用的社区 schema 中
  尚未命名（2026-10-16 导出验证），不能按此列名导出。显示顺序继续由
  `overlay/gem_effects.json` 提供；不为未命名列猜测 schema override。
- **CDN 只保留当前补丁**：GGG patch CDN 会下线旧版本，旧补丁请求返回 404。`.cache/`
  里已缓存的 bundle 可继续离线导出**既有表的全部列**（整张 `.datc64` 在同一 bundle 里）；
  但**新增整表**若其 bundle 未缓存则无法补下——这类表记录在 `config.json` 的
  `_tablesUnavailableForPinnedPatch`，数据改走 `sync-pob-catalog extract-lua` 兜底
  （vendor Lua → `overlay/`），版本升级重下时再移回 `tables` 数组。

## 列名陷阱：社区 schema vs PoB2 spec.lua（M1-W0 2026-06-11 核验）

`pathofexile-dat` 用 [poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema) 的列名；
PoB2 `Export/spec.lua` 对**同一物理列**有不同命名。`config.json` 必须用社区名下载，
adapter 落库时按 PoB2 语义重命名。已核验对照（PoE2 段，validFor=2）：

| 表 | 社区 schema 列名（下载用） | PoB2 spec.lua 名（语义） |
|----|--------------------------|--------------------------|
| GrantedEffects | `SupportsGemsOnly` | `SupportGemsOnly`（多个 s） |
| GrantedEffects | `ExcludedActiveSkillTypes` | `ExcludeTypes` |
| GrantedEffects | `AllowedActiveSkillTypes` | `SupportTypes`（require 语义） |
| GrantedEffects | `AddedActiveSkillTypes` | `AddTypes` |
| GrantedEffects | `AdditionalStatSets` | 同名；**FK 目标是 GrantedEffectStatSets**（非 GrantedEffects） |
| GrantedEffectsPerLevel | `Reservation` | `SpiritReservation` |
| GrantedEffectsPerLevel | `EffectOnPlayer` | `ReservationMultiplier`（默认 100，已用全表默认值佐证） |
| GrantedEffectStatSets | `Label` | `LabelType`（FK → GrantedEffectLabels） |
| GrantedEffectStatSetsPerLevel | `SpellCritChance` | **`AttackCritChance`**（主暴击列，整体前移一位） |
| GrantedEffectStatSetsPerLevel | `AttackCritChance` | **`OffhandCritChance`**（副手覆盖列） |
| SkillGems | `ItemExperienceType` | `GemLevelProgression`（FK → ItemExperiencePerLevel） |
| ItemExperiencePerLevel | `ItemCurrentLevel` / `Level` | `Level` / `PlayerLevel` |

暴击两列的错位已用 overlay `skill_overrides.json` 的 201 条 crit_chance 全量对拍验证
（199 条直接命中 `SpellCritChance/100`；2 条位于 AdditionalStatSets 指向的附加 set，同列命中）。
另注：PoE2 的 `GrantedEffectsPerLevel` **没有** `PlayerLevelReq` 列（PoE1 才有）；等级需求
走 `SkillGems.ItemExperienceType → ItemExperiencePerLevel` 链（PoB2 `Export/Scripts/skills.lua:240`）。

Target-language proper names can be adapted from the independent translation repository
with the [native name sidecar adapter](native-name-translations.md). It preserves
exact locale/source identity and does not activate data.
