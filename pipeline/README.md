# PoBR 数据管线（pipeline）

本目录声明 PoBR 构建所需的固定输入：按唯一数据锁临时导入 RePoE 游戏数据包与翻译库语言产物，
封存为运行目录与精简发行包。把 RePoE 事实转换成运行 JSON 的
[`pobr-data-adapter`](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/README.md) 及其领域政策在 RePoE。
PoBR 只导入固定产物：[特殊制作目录](https://github.com/ackness/repoe/blob/master/RePoE/docs/pobr_crafting_catalog.md)也由 RePoE 投影后放进游戏 part。
完整输入与 `data/<poe-version>/` 运行目录仅临时物化；
尚未迁移的字段以 PoB2／社区兼容数据的形式随 RePoE owner 包固定导入，不称为已从客户端自主再生。
PoBR 不从 PoB2 vendor Lua 或本地原表导出生产数据。

当前存储职责：游戏数据归RePoE、语言数据归翻译库，PoBR只提交代码、唯一数据锁 [`data.lock.json`](data.lock.json)、来源元数据及小型合成夹具；CI构建和需要实物的测试在临时目录导入数据并生成发布资产。
四份完整快照、七份外部shared、历史大policy及完整语言／API样本已迁出当前Git tree；无旧PoBR数据artifact回退。本地活动目标保持0.5.5.5，Git历史不改写。详见[存储边界](../docs/data-architecture.md#4-存储边界)。

[最新完整发布同步](latest/latest-data.md)通过两owner发布索引发现候选，冻结为新的数据锁，再由精确head的PR／Full门禁与实际最终包消费验证后条件采用。普通构建继续按不可变锁重放；索引bootstrap、官方版本观察和owner源码更新均不代表新数据已生成或已采用。
两 owner 索引及 PoBR 控制器均已合并；严格 PR gate／管理员保护已配置，专用同步凭据与真实新发布的端到端采用仍待完成。
当前核对与优化顺序以[数据现状](../docs/data-status.md)和 [latest 启用状态](latest/latest-data.md#verified-rollout-status-2026-11-02-utc)为准。

## 功能目录

| 目录 | 内容与入口 |
| --- | --- |
| [data.lock.json](data.lock.json) | 唯一数据锁：两个 owner 的固定 tag、commit、路径与 SHA-256，目标、自有规则 pin 与期望身份 |
| [runtime/](runtime/build-data.md) | 按锁临时导入、快照封存、精简发行包和显式采用；`data:prepare`、`data:update`、`data:pack` |
| [imports/](imports/client-sources.md) | 固定 Git 传输、来源接受记录、辅助说明投影、数据归属检查 |
| [latest/](latest/latest-data.md) | owner 通道、候选发现与 CI 后条件采用；`data:latest` |
| [diagnostics/](diagnostics/README.md) | 数据版本差异、描述来源核对与词条审计；不自动采用数据 |

各功能的说明、锁和测试与实现放在一起。Python 从仓库根目录以模块运行，
例如 `mise exec -- python3 -m pipeline.runtime.runtime_pack --help`；日常优先使用上表中保持稳定的 mise 任务。

JSON 的职责各不相同：`data.lock.json` 是唯一的生产数据声明；CI 与本地准备都只读它。
测试夹具只供测试读取，不能代替真实来源或运行验收。

历史来源收据中的路径按记录时的 Git 版本解释，不为整理目录重写原始 pins、命令或接受范围。
当前文件定位与组合清单则使用本页列出的路径。

RePoE 在 2026-11-03 截断 fork 前历史后，活动锁、回滚锁和官方版本观察模块采用迁移后的 Git 提交；
两个 owner 的发布声明推进到 sequence 2，数据包字节、运行时身份与发行身份保持不变。
历史收据中的原始 producer ID 保留，通过 RePoE 的[提交映射](https://github.com/ackness/repoe/blob/master/publication/history-rewrite-20261103.json)
定位字节相同的源码；它们不作为新的运行时下载位置。详见[历史迁移说明](https://github.com/ackness/repoe/blob/master/RePoE/docs/history.md)。

临时脚本、报告、下载、测试目录和物化运行包放在忽略的 `.cache/`；
本目录不保留已退休 exporter 的依赖安装或临时输出。

## 活动数据来源与生产

| 域 | 当前来源 | 生产与保留边界 |
| --- | --- | --- |
| 物品基底／词缀／Stat／技能宝石 | 固定自有RePoE客户端DAT事实 | [普通领域adapter](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-facts.md)接回原DTO；未拥有字段及旧编解码仍保留兼容 |
| 宝石品质 | 目标GrantedEffectQualityStats／GrantedEffects／Stats事实 | [品质生成说明](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/policies/gem-quality/README.md)；计算仍按固定effect-wide策略，不认证全部细分scope |
| 词条显示文本 | 固定客户端CSD＋明确保留的兼容文本 | [普通描述生产者](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-stat-descriptions.md)、树／符文renderer；未表达项保留诊断或整项回退 |
| 被动天赋树 | 目标DAT／Default PSG事实与固定兼容字段 | 普通树已迁源；placement／armour等未拥有语义保留 |
| 特殊制作目录 | 独立RePoE七表中立事实，RePoE按PoBR目录政策筛选／聚合后放进游戏 part | [目录投影](https://github.com/ackness/repoe/blob/master/RePoE/docs/pobr_crafting_catalog.md)保留八槽与未表达原值诊断；来源语言缺失保持unknown；不代表完整效果或概率 |
| 规则／目录／翻译 | PoBR自有规则、随owner包导入的PoB2／社区兼容目录、固定语言库与客户端文本投影 | 逐项记录来源和适用边界；保留输入不代表全部自主生产，SC客户端身份与CN数值不因中文显示而认证 |

`.dat` 存的是 **id / 数值 / 外键关系**（规范化数据库表）；显示文本在 `StatDescriptions` 里。
列名/表名见 [poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema)。

## 固定来源

独立客户端候选的可重复流程是[固定补丁索引定位](imports/client-index.md) →
[未审阅原始采集](imports/client-source-draft.md) → 来源审阅与固定 →
[严格重放](imports/client-sources.md) → RePoE 事实导出与其 `pobr-data-adapter` 领域适配。
索引定位、采集、重放与来源包生成由独立RePoE实现并测试；PoBR只导入 owner 已发布的固定产物。这些步骤不自动提升来源锁，也不切换活动数据。历史 Git 树也由 RePoE 独立保存和发布，PoBR 不再直接获取官方原始树。

[sources.lock.json](imports/sources.lock.json)保留历史官方树输入及固定客户端资源位置、原始SHA-256、大小、decoder／schema和采集证据。
`acquisition_acceptances`分别接受F77、六DAT、91 tree／IT、10 CSD、12语言原生资源，以及精确六OT／两描述DAT字节；
`retained_input_acceptances`另记locale／shared／compat三类有界保留决定。各项只接受明确scope和pins，不认证全部机制、SC客户端身份或分发许可。
采集、领域解释、消费者验证、本地采用及远端发布仍分开；完整状态见[数据现状](../docs/data-status.md)。
PoB2 的测试参考版本独立登记在 [devs/reference-sources.json](../devs/reference-sources.json)，
`mise run vendor` 不再读取生产 parser 元数据选择参考版本。

PoBR 不再提供历史官方树的诊断候选入口（原 `data:tree:source`／`data:tree:candidate`，已退役）。
RePoE 的 `tree:input` 导出固定的历史 Git 树，`pobr-data-adapter --tree` 可直接生成树域候选。
`bd87e651` 输入中 Druid 的两条悬空 `overridePairs`（`19680`→`40837`、`55194`→`57601`）仍会被严格
adapter 拒绝；原隔离清单保留在 RePoE adapter 的 `policies/tree-corrections/`，没有执行入口。

## 固定客户端原始资源与事实候选

原始客户端采集、索引定位及来源包生成请在独立RePoE运行其`client:source`、`client:draft`、`client:locate`、`source:rebind`任务。
入口、默认strict及显式diagnostic边界见[客户端资源获取](imports/client-sources.md)。

RePoE 原生事实包另由 RePoE 的 `pobr-data-adapter --native-facts` 验证外部 manifest 摘要后形成
PoBR 领域候选，见[候选契约](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/native-facts.md)；schema 4 分支已冻结。当前只接通部分国际服事实，
保留完整原始事实引用和诊断；它不是运行时包，也未完成品质、几何或全部关系的语义验证。
活动数据仍含随 owner 包导入的 PoB2 兼容字段，不能宣称数据已全部退出 PoB2。

### 普通完整 BuildData 候选

RePoE 的 `pobr-data-adapter --ordinary-facts` 已将固定自有 RePoE 事实接回原普通 DTO，见
[普通生产入口与领域命令](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-facts.md)。技能、基底、树、符文、
描述、actor 与基础字段的目标来源和固定兼容按各领域 receipt 分开，不能将单个文件全部算作自主。
这些领域输出经既有 schema-3 组包和封存后可由普通 GameData/BuildData 消费；
树域候选与 schema-4 受限原生候选是不同入口，不代表普通完整路径的现行覆盖范围。

活动组合的身份、各领域保留兼容的分母和关闭条件统一维护在[数据现状](../docs/data-status.md#领域现状)，本页不复制阶段记录。

选定 locale 的显示侧表 `i18n/<locale>/display_texts.json` 由翻译库生成，随语言发布导入，接入普通 Web
`translateLines`：精确来源查询、跨表同名冲突和缺译保留，不改原始构筑文本或计算。
侧表已列入当前活动manifest；全部字段UI消费、真实审核译文及CN语义仍分别验收。审核补译入口只消费精确绑定的已审核记录，当前真实包为0条，草稿不生效。

原生候选的 recipe 重放入口（原 `data:native:candidate`）已退役；`--native-runtime` 随 adapter 冻结在
RePoE，见[原生运行组装](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/native-runtime.md)。

## 运行时数据组合封存

运行包的版本文件与共享规则可独立封存并核验，见[运行时数据组合封存](runtime/runtime-bundle.md)。
精简发行包由 `mise run data:pack build` 直接从已准备的 owner 快照组装，发行身份记录在数据锁的 `release_id`；
PoBR 不再在 CI 中重跑普通领域生产或显示生成。

## 词条审计

审计已准备数据中的词条规则，并与固定版本的 PoB2 parser 对照：

```bash
mise exec -- bash pipeline/diagnostics/modifier-audit.sh
```

命令先严格校验完整有效规则，再用完整来源语料对照 PoB2，并与上一份审计比较。
它只做审计：不提取或改写规则，不下载游戏数据、不切换游戏版本，也不修改数值 golden。
没有 vendor 时用 `--offline`，报告明确不含 PoB2 对照。
可用 `--data data/<version>` 指定数据，`--baseline <audit.json>` 指定历史审计。

语料来源包括 StatDescriptions 的稳定 stat ID、装备/珠宝/药剂/护符词缀、特殊制作词缀、
基底隐式、传奇装备各变体、天赋树，以及中文导入实际输出的英文模板。
数值范围抽取低/高样本，复合描述使用不同的占位值；特殊规则的 `examples` 也进入语料，无法渲染的条目另计数。
这是**解析样本覆盖率**，包含地图和怪物描述，不能当作玩家机制或 DPS 覆盖率。

产物：

- `data/<version>/generated/modifier-audit.json`（已准备的忽略目录）：可重现的完整审计与来源，作为下一次比较的快照；不进入 Web 下载清单。
- `.cache/modifier-audit/<version>/validation.json`：严格校验报告，记录 common / version / patch / derived / vendor 文件的 SHA-256、有效规则顺序和覆盖来源；审计的 `rule_validation` 内嵌同一信息。
- `.cache/modifier-audit/<version>/current.delta.json`：退化、新增缺口、已解决和移除条目。
- 同目录的 `corpus.txt` / `oracle.jsonl`：本次 PoB2 对照的输入与输出。

报告中的 `pobr_gap` 表示 PoB2 完整解析而 PoBR 未完整解析，优先检查 PoBR 规则、枚举、标签映射。
`upstream_gap` 表示 PoB2 也不能完整解析，需查游戏机制并实现对应计算，不能靠空规则消除提示。
`recognized_empty` 单列零 modifier 的规则；有残余文本或丢失条件标签也不计为完整支持。
`no_effect` 是登记为不影响任何计算输出的 S0 规则（`"mods": []`、`"batch": "S0"`）匹配的行，与 `parsed` 一样计为完整支持：从 `parsed` 变成 `no_effect` 不算退化。
没有 oracle 时缺口为 `uncompared_gap`。`mod_names` 便于定位计算消费者，但识别出名称不证明消费者已实现。

每条样本的 `special_rule_id` 关联规则文件来源，`matching_special_rules` 按优先级列出所有候选，多个 ID 表示后续规则被遮蔽。该检查只覆盖样本，不证明任意文本都无重叠。结构化 LIST 等未参与计算的字段在 `non_effective_fields` 单列；现有装备 / 天赋的 `SourceId` 不由规则来源替代。严格校验与合并契约见[维护者规则说明](../docs/contributing-mods.md#7-validate-test-and-open-a-pr)。

以前完整解析的文本退化，或同一来源移除旧措辞后新增无法解析的措辞，会使命令失败。
失败时保留 `.cache` 报告，**不覆盖上一份已通过的审计快照**；新增机制缺口单独列出。
既有示例角色覆盖率及数值 parity 门禁仍独立保留。

PoB2 本身把两层工作分开：`Data/StatDescriptions` 等从游戏数据导出文本，
`Modules/ModParser.lua` 将文本映射到 modifier，`Modules/Calc*.lua` 消费这些 modifier。
PoBR 沿用这个边界：从 PoB2 转换的解析规则（`mod_parser_rules.json`、`special_vendor.json`）随 RePoE owner 包固定导入，
PoBR 的修正放在 `data/overlay-common/special_mods.json`，更新数据后自动继承；真正的新机制仍需要实现和数值回归测试。

### 中文词典来源

先运行`mise run setup`准备精确工具和frozen Web依赖；独立语言消费者使用`mise exec --`，不需要PoBR内的npm导出器依赖。

基础名词与既有游戏模板使用 [poe2-en-cn-dict 的存档](https://github.com/vagrant-soul/poe2-en-cn-dict)；新增词条与 PoBR 自维护模板使用下面的独立公开词表。
2026-10-12 原 `addohm/poe2-en-cn-dict` 仓库、API 和固定提交原文件均返回 404；
公开接口无法区分删除与转私有。存档提交 `288d8b9d999c5ea5664149e408847156dbd83ff4`
与原固定提交 `28d683c99600eb407b4e014ccaf8247532fb4607` 的十个生成输入逐字节一致。
这是下载来源恢复，不是中文数据升版；`_meta.json` 继续保留真实来源提交和 2026-07-06 的生成时间。
国际服导出提供繁中；完整游戏表的新版官方简中仍需相应 WeGame 客户端数据，不能用繁转简冒充。独立词表中的腾讯交易文本不等于完整客户端名词表。
当前构建从独立翻译库的固定产物导入，执行 `mise run data:prepare`；完整词典和元数据保持原字节，不在PoBR内重新生成或跟踪上游HEAD。
目标、Git修订、tag、manifest及成员摘要见[数据锁](data.lock.json)，临时导入及回滚见[build-data](runtime/build-data.md)。
简中生成、下载与合并在翻译库；PoBR 只导入 owner 已发布的固定语言文件，不解析来源 HEAD。现有 processed 包的精确恢复不代表全部客户端自主再生。

### 独立词表与人工翻译维护

[ackness/poe2-zh-cn-stats](https://github.com/ackness/poe2-zh-cn-stats)维护交易模板、语言来源、人工修正及固定语言产物。
更新先在翻译库生成／审阅并发布不可变产物，再由PoBR更新精确锁及验证显示、导入和计算契约；运行时不联网取词典。
现有简繁保留产物不证明官方客户端身份、国服数值或全部译文质量；原生成版本、未知字段及冲突诊断不因目标版本改变而重贴标签。

维护职责已迁移：

- 原 `EXTRA_PAIRS` 的 11 条模板及 `Maelstrom → 漩涡` 名字补丁位于独立库的 `curated.json`，保留原词句、字符串／嵌套占位符、参数顺序和不可变出处。PoBR 不再保存第二份硬编码补丁列表。
- 给已有交易 ID 补译，修改独立库的 `translations.json`；需要明确覆盖现有译文时使用 `overrides.json`；不属于交易目录的模板和已人工核验的多参数模板使用 `curated.json`。
- 技能标签、物品结构行及导入语法仍由 PoBR 按上下文解释，不把 UI 文案或计算规则混入词条表。

辅助定性说明及其`GemEffects.SupportText`配对处理已归翻译库独立存储和维护。
`data.lock.json`的`support_display`固定外部manifest、资产和目标catalog摘要；`mise run data:prepare`
在构建／实物测试前导入，物化忽略的`web/src/lib/supportDescriptions.zh-CN.json`，不改GameData快照身份。
当前保留资产的原生成版本仍为0.5.5.4.2；与目标目录SHA完全相同才允许使用，UI仍核`skill_id`和完整英文。
换目录须重新由翻译库配对并更新锁，不能重贴旧版本标签。投影代码与合成测试归翻译库；PoBR验证导入pins、目录绑定和显示回退。
繁体仍是字形转换回退，不宣称官方繁中译文；这些说明不是当前等级／品质数值或机制认证。

自动模板仅支持已验证的零／单数值槽，以 `{0:d}` 避免误进入字符串捕获；多值参数、双向歧义、数值／符号不符等由独立库列为排除项。PoBR 再按实际翻译器的匹配骨架检查新模板与既有中英文模板、其他新模板的冲突，冲突不按输入顺序选第一条。审核过的迁移模板保留原行为，旧模板与七份名词表不因接入而删除。

同一英文模板在旧词典与独立词表中的中文不一致时，采用独立词表的官方译法：只替换中文措辞，保留旧条目的英文模板和各参数槽的格式（符号显示、字符串捕获行为不变）。仅在新模板无歧义、英文除参数格式和空白外完全相同、且新中文不与其他既有模板冲突时替换；人工审核模板、含换行的旧模板、仅字面数值不同的模板不替换。被替换的条目列在 `translation-audit.json` 的 `replaced_legacy`。

`generated/translation-audit.json` 记录新增、替换、重复、排除和稀有名补丁采用情况；它属于维护报告，不进入运行时 manifest 或浏览器包。`i18n/zh-CN/_meta.json.translation_source` 记录实际版本；原 `source` 字段仍指基础名词和模板的真实来源。

首次接入 `0.5.5.4.2` 保留全部 26,074 条原模板和七份原名词表，增加 1,822 条无冲突模板，合计 27,896 条。其中 17 条原模板的中文改用官方译法，总数不变。翻译可用不等于机制已计算：新增英文语料仍由 modifier audit 报告解析缺口，不通过补译掩盖。


## 固定输入、版本与候选

当前输入只由[数据锁](data.lock.json)声明（导入细节见[build-data](runtime/build-data.md)）；
`data/CURRENT`只选择本地运行版本。
PoBR不再安装`pathofexile-dat`、下载原始索引或导出官方原表。官方版本发现和资源采集在独立RePoE执行，
语言在翻译库生成；成功观察到一个补丁号也不能认证全部原料、机制或改变CURRENT。

```sh
mise run data:version                   # 本地选中版本
mise run data:prepare                   # 临时导入当前固定目标，不更改 CURRENT
mise run data:check                     # 离线检查：跟踪数据归属，导入后工作树干净
mise run data:pack build --out .cache/runtime-pack   # 从已准备的快照组精简发行包
mise run data:update --lock pipeline/data.lock.json --out .cache/update-review
```

`data:check` 不联网、不导入：先由 `pipeline.imports.check_data_ownership` 确认 `data/` 只跟踪 `CURRENT`
与锁中声明的自有共享规则，再断言导入后没有未忽略的新文件或被改动的跟踪文件。导入产物的忽略规则集中在根
`.gitignore` 的同一区块。CI 中每个导入数据的 job 先恢复 `.cache/data-import-downloads` 的内容寻址缓存
（命中后仍逐项校验摘要），导入后运行同一检查。

完整候选及显式本地采用见[锁驱动更新](runtime/locked-update.md)。采用须同时指定`--activate`和预期CURRENT，
保留隔离的版本／GameData／parity门禁、原子选择、失败回滚及同版本目标patch；默认只准备候选。
shared变更须另行迁移；不会自动修改vendor pin、来源接受、数值golden、构建锁或部署。

schema-3 manifest封存默认运行字节；维护报告和本地patch不进入玩家包。
完整游戏／语言输入、运行目录与缓存均为临时或忽略内容，不进入PoBR Git／data artifact refs。

## 扩展 / 升版

- 新PoE2版本：在独立RePoE观察并取得精确来源、导出和发布游戏输入；翻译库独立准备语言。
  PoBR审阅并更新`pipeline/data.lock.json`，先用`data:update --lock <lock> --out <fresh>`准备候选，
  验证来源／消费者／CI后再显式选择CURRENT。计算语义、vendor参考与golden更新分别维护。
- 新数据域：在RePoE固定schema／事实导出器中补齐来源，由其`pobr-data-adapter`接既有DTO；PoBR验证数据契约和消费者；
  不再通过PoBR Node导出器或修改旧配置将历史数组冒充新目标。
- stat-set 标签由 `GrantedEffectStatSets.Label` → `GrantedEffectLabels.Text` 直接连接，
  以原始 `_index` 查外键，不按数组位置或显示文字猜测；有引用却缺表、缺行或行号重复时拒绝适配。
  标签为空的旧快照仍可从 `overlay/stat_set_labels.json` 回退；该兼容表继续提供 PoB 构筑码的
  stat-set 编号，不能覆盖已存在的原生标签。此能力不自动更新活动快照。
- 怪物基础抗性由 `MonsterVarieties.MonsterType` → `MonsterTypes.MonsterResistances` →
  `MonsterResistances.Fire1/Cold1/Lightning1/Chaos1` 导出到 `base/monster_resistances.json`，
  按 monster variety id 索引。使用普通稀有度的基础值（0.5 每列只有一个值），合并类型链接的
  全部抗性记录，保留负数弱点；空链接为零。该文件的 `_meta` 保存原始导出收据与三张输入表的
  SHA-256；普通 BuildData 已将其写入 minion／spectre 定义并用于基础抗性计算，
  不代表其它召唤物语义已全部迁移或认证。
- `GemEffects.GrantedEffectDisplayOrder` 在社区 dat-schema 中尚未命名（2026-10-16 核对），
  不能按此列名导出。显示顺序继续由 owner 包中的兼容 `overlay/gem_effects.json` 提供；
  不为未命名列猜测 schema override。
- **历史资源可用性**：已观察到部分旧补丁资源返回 404，不能据此断言所有旧资源必然下线。
  固定归档可离线重放已保存的 DAT；未取得的整表／资源保持来源缺口。采集和新表生产归 RePoE。
  owner 包中的 PoB2 兼容字段可作为显式保留依赖，不能在采集失败时静默兜底为目标补丁的官方事实。

## 列名陷阱：社区 schema vs PoB2 spec.lua（M1-W0 2026-06-11 核验）

社区 [poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema) 与 PoB2 `Export/spec.lua`
对**同一物理列**有不同命名。下表用于解释 DTO 兼容字段；新导出在 RePoE 固定物理列／schema，再由 adapter 明确映射。历史核验对照（PoE2 段，validFor=2）：

| 表 | 社区 schema 列名 | PoB2 spec.lua 名（语义） |
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
