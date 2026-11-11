# PoBR 数据管线（pipeline）

本目录按唯一数据锁 [`data.lock.json`](data.lock.json) 临时导入 RePoE 的游戏包与翻译库的语言包，
封存为运行目录与精简发行包。

PoBR 不生产数据。把 RePoE 事实转换成运行 JSON 的
[`pobr-data-adapter`](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/README.md) 及其领域政策在 RePoE，
[特殊制作目录](https://github.com/ackness/repoe/blob/master/RePoE/docs/release/pobr_crafting_catalog.md)也由 RePoE 投影后放进游戏包；
尚未迁移的字段以 PoB2／社区兼容数据的形式随 owner 包固定导入，不称为已从客户端自主再生。

三仓库职责与存储边界见[数据架构](../docs/data-architecture.md)，当前接入状态见[数据现状](../docs/data-status.md)，
换数据、发版与回退的步骤见 [RELEASING.md](../RELEASING.md)。

## 功能目录

| 目录 | 内容与入口 |
| --- | --- |
| [data.lock.json](data.lock.json) | 唯一数据锁：两个 owner 的 Release 附件（或旧 `handoff/*` 的 commit + 路径）与 SHA-256，目标、自有规则 pin 与期望身份 |
| [runtime/](runtime/build-data.md) | 按锁临时导入、快照封存与精简发行包（`data:prepare`、`data:pack`）；按另一份锁的本地选择与回退演练（`data:update`，见[锁驱动的本地选择](runtime/locked-update.md)） |
| [imports/](imports/sources.lock.json) | Release 附件与固定 Git 传输、`data:bump`、测试夹具导入、数据归属检查；台账仅存消费范围接受决定 |
| [diagnostics/](diagnostics/README.md) | 数据版本差异、描述来源核对与词条审计；不自动采用数据 |

各功能的说明、锁和测试与实现放在一起。Python 从仓库根目录以模块运行，
例如 `mise exec -- python3 -m pipeline.runtime.runtime_pack --help`；日常优先使用上表中保持稳定的 mise 任务。

`data.lock.json` 是唯一的生产数据声明，CI 与本地准备都只读它。
测试夹具只供测试读取，不能代替真实来源或运行验收。

历史来源收据中的路径按记录时的 Git 版本解释，不为整理目录重写原始 pins、命令或接受范围。
临时脚本、报告、下载、测试目录和物化运行包放在忽略的 `.cache/`。

## 活动数据来源与生产

| 域 | 当前来源 | 生产与保留边界 |
| --- | --- | --- |
| 物品基底／词缀／Stat／技能宝石 | 固定自有RePoE客户端DAT事实 | [普通领域adapter](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-facts.md)接回原DTO；未拥有字段及旧编解码仍保留兼容 |
| 宝石品质 | 目标GrantedEffectQualityStats／GrantedEffects／Stats事实 | [品质生成说明](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/policies/gem-quality/README.md)；计算仍按固定effect-wide策略，不认证全部细分scope |
| 词条显示文本 | 固定客户端CSD＋明确保留的兼容文本 | [普通描述生产者](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-stat-descriptions.md)、树／符文renderer；未表达项保留诊断或整项回退 |
| 被动天赋树 | 目标DAT／Default PSG事实与固定兼容字段 | 普通树已迁源；placement／armour等未拥有语义保留 |
| 特殊制作目录 | 独立RePoE七表中立事实，RePoE按PoBR目录政策筛选／聚合后放进游戏 part | [目录投影](https://github.com/ackness/repoe/blob/master/RePoE/docs/release/pobr_crafting_catalog.md)保留八槽与未表达原值诊断；来源语言缺失保持unknown；不代表完整效果或概率 |
| 规则／目录／翻译 | PoBR自有规则、随owner包导入的PoB2／社区兼容目录、固定语言库与客户端文本投影 | 逐项记录来源和适用边界；保留输入不代表全部自主生产，SC客户端身份与CN数值不因中文显示而认证 |

## 来源与 owner 工具

原料来源、采集证据与语言策展归 owner 仓库：游戏侧见 RePoE
[`sources/`](https://github.com/ackness/repoe/tree/master/sources)，语言侧见翻译库
[`sources/`](https://github.com/ackness/poe2-zh-cn-stats/tree/main/sources)。
[sources.lock.json](imports/sources.lock.json) 只保留 PoBR 消费范围的接受决定（shared、compat 两类保留输入），
不认证全部机制、SC 客户端身份或分发许可。PoB2 的测试参考版本独立登记在
[devs/reference-sources.json](../devs/reference-sources.json)。

客户端采集、官方版本发现、DAT 列名与 schema 映射、事实导出、领域候选（普通事实、原生事实、天赋树）和输入包
`inputs/<patch>-<n>` 都在 RePoE，入口见其[客户端资源获取](https://github.com/ackness/repoe/blob/master/RePoE/docs/client/client-sources.md)与上文的 adapter 说明。
这些步骤不改 PoBR 的锁，也不切换活动数据。活动数据仍含随 owner 包导入的 PoB2 兼容字段，按字段的现状见
[与 PoB2 的差异](../docs/pob2-differences.md#数据来源现状与退出顺序)。

## 运行时数据组合封存

运行包的版本文件与共享规则可独立封存并核验，见[运行时数据组合封存](runtime/runtime-bundle.md)。
精简发行包由 `mise run data:pack build` 直接从已准备的 owner 快照组装，发行身份记录在数据锁的 `release_id`；
CI 不重跑 owner 的领域生产或显示生成。

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

## 语言数据与宝石说明

语言包由[翻译库](https://github.com/ackness/poe2-zh-cn-stats)生成并发布为 `locale/<patch>-<n>` Release，
PoBR 按锁导入，不在本仓库重新生成、下载词典或跟踪上游 HEAD；运行时不联网取词典。
交易模板、译文、人工补译与 `translation-audit.json` 都在翻译库维护，见[翻译库 README](https://github.com/ackness/poe2-zh-cn-stats#readme)；
翻译问题在那里修正。技能标签、物品结构行及导入语法仍由 PoBR 按上下文解释，不把 UI 文案或计算规则混入词条表。
翻译可用不等于机制已计算：新增英文语料仍由词条审计报告解析缺口。

选定 locale 的显示侧表 `i18n/<locale>/display_texts.json` 随语言包导入，接入普通 Web `translateLines`：
精确来源查询、跨表同名冲突和缺译保留，不改原始构筑文本或计算。审核补译只消费精确绑定的已审核记录，草稿不生效。

宝石说明的英文来自游戏包的可选 `overlay/gem_descriptions.json`，译文来自语言包
`display_texts.json` 的 `GemEffects.SupportText` 与 `ActiveSkills.Description` 作用域。
Web 的宝石选择器与辅助推荐共用说明面板，按稳定 ID 和完整英文查询译文；没有英文文件时不显示说明。
文件形状与来源约定见[数据架构](../docs/data-architecture.md#3-发布与导入契约)，显示与语言回退见
[Web 契约](../docs/web-contracts.md#宝石天赋与珠宝规划)。

## 固定输入、版本与候选

当前输入只由[数据锁](data.lock.json)声明（导入细节见[build-data](runtime/build-data.md)）；
`data/CURRENT` 只选择本地运行版本。PoBR 不安装表导出工具，也不下载原始索引或导出官方原表；
成功观察到一个补丁号也不能认证全部原料、机制或改变 CURRENT。

```sh
mise run data:bump ackness/repoe game/0.5.5.5-3   # 把锁改到 owner 的 Release，校验 SHA256SUMS 后导入
mise run data:version                   # 本地选中版本
mise run data:prepare                   # 临时导入当前固定目标，不更改 CURRENT
mise run data:check                     # 离线检查：跟踪数据归属，导入后工作树干净
mise run data:pack build --out .cache/runtime-pack   # 从已准备的快照组精简发行包
mise run data:update --lock <另一份锁> --out .cache/update-review   # 只在本地：准备候选或演练回退
```

`data:check` 不联网、不导入：先由 `pipeline.imports.check_data_ownership` 确认 `data/` 只跟踪 `CURRENT`
与锁中声明的自有共享规则，再断言导入后没有未忽略的新文件或被改动的跟踪文件。导入产物的忽略规则集中在根
`.gitignore` 的同一区块。CI 中每个导入数据的 job 先恢复 `.cache/data-import-downloads` 的内容寻址缓存
（命中后仍逐项校验摘要），导入后运行同一检查。

`data:update` 不改数据锁，不是采用路径：它按另一份锁准备完整候选，或经 `--activate` 与预期 CURRENT
在本地原子选择它（保留隔离的版本／GameData／parity 门禁、失败回滚及同版本目标 patch），见
[锁驱动的本地选择](runtime/locked-update.md)。它不会修改 vendor pin、来源接受、数值 golden 或部署。

schema-3 manifest 封存默认运行字节；维护报告和本地 patch 不进入玩家包。
完整游戏／语言输入、运行目录与缓存均为临时或忽略内容，不进入 PoBR Git／data artifact refs。

## 扩展 / 升版

新补丁、只换语言、发版与回退的步骤只写在 [RELEASING.md](../RELEASING.md)。`data:bump` 的校验规则：

- **目标**：目标改变时必须在一次调用中 bump 两个 owner，只 bump 一个会被拒绝。命令原子替换锁与 `data/CURRENT`；
  随后的 `data:prepare` 失败时恢复原字节并以非零退出。
- **测试夹具**：可选的 `test_fixtures` 是与补丁无关的 API 响应样本；换目标时保留，条目加 `target` 记录它们发布时的目标；缺省时跳过。
- **receipt 绑定**：receipt 的 `package`（ZIP 与根 manifest 的字节数与 SHA-256）存在时必须与下载的包一致；
  `repoe-release-receipt/v2` 必须带它；旧 receipt 不带时仍可读。
- **契约**：新游戏快照未声明 `contract_version` 时拒绝，报错指向 RePoE 声明中 snapshot-manifest job 的
  `contract_version: "adapter"`（见 RePoE 的 [RELEASING.md](https://github.com/ackness/repoe/blob/master/RELEASING.md)）。
- **绑定游戏包或补丁的消费者**：`crates/pobr-gamedata/tests/fixtures/repository-test-pins.json`（任何新游戏包都要重绑
  `game_manifest_sha256`）、`web/src/lib/configGameRelevance.json`、`devs/ci/parse-coverage-baseline.json` 与示例 golden
  （如 `examples/demo-bd-test/panel-golden.json` 的 `data_version`）在 bump PR 中一起迁移，做法见
  [RELEASING.md](../RELEASING.md)；计算语义、vendor 参考与数值 golden 的更新分别维护。
- **新数据域**：在 RePoE 的固定 schema／事实导出中补齐来源，由其 `pobr-data-adapter` 接既有 DTO；
  PoBR 定义数据类型（[数据契约版本](../docs/data-architecture.md#31-数据契约版本)），验证契约与消费者。
  采集失败时，owner 包中的 PoB2 兼容字段只能作为显式保留依赖，不能静默充当目标补丁的官方事实。
