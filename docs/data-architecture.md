# 统一数据架构与维护设计

状态：**架构设计，尚未全量实施**。最后修订：2026-11-01。

本设计承接项目所有者关于数据完整、准确、可用、规范、高效的要求。当前生产行为仍以
[数据管线](../pipeline/README.md)、[构筑模型](build-model.md)和代码为准。
本文规定目标架构、来源和验收条件，不以设计条目代表实现完成。
普通数据生产者已接回完整 BuildData；0.5.5.5 已本地采用，完整数据由两个独立仓库发布并在 CI／实物测试前临时导入。身份与验收边界见第 5 节。
原生事实导出与受限 CLI/WASM 的较早验证另见第 12 节。上述交付不等于整库正确、完整来源独立或线上发布。

## 1. 架构决策与三仓库职责

游戏事实和语言数据分别由自有RePoE与翻译库独立存储、处理和发布；PoBR维护接入、领域适配与计算能力，上游更新作为可评估的输入；
上游未及时实现的字段和修正由我们维护，不以 PoB2 或 RePoE 当前覆盖范围限制产品能力。

| 仓库 | 唯一主要职责 | 边界 |
| --- | --- | --- |
| [ackness/repoe](https://github.com/ackness/repoe) | 客户端资源解码、游戏事实和关系导出、通用字段补全、版本化数据产物及来源记录 | 不实现 PoBR 的伤害计算、敌人场景默认值或构筑配置语义 |
| [ackness/poe2-zh-cn-stats](https://github.com/ackness/poe2-zh-cn-stats) | 国服/国际服词条模板、必要的术语补充、匹配来源和人工修正 | 不以翻译替代游戏 stat 身份；现有词条库不冒充完整游戏名词库 |
| [ackness/pobr](https://github.com/ackness/pobr) | 统一接入、领域规范化、数据筛选、Rust 语义与计算、玩家使用流程和数据包发布 | Rust/Web 消费 PoBR 数据契约，不直接拼接多个站点的私有格式 |

本地工作根为 `/Users/claw/codes`，两个数据仓库位于其同名目录。
PoBR 的开发仍可在独立 worktree 中进行，不要求切换或覆盖主 checkout。
仓库之间靠固定版本的产物和收据连接，正式构建不依赖某台机器上的相对目录或未提交文件。

推荐先在自有 RePoE 仓库发布带来源收据的版本化事实数据资产，不另建第四个长期数据镜像仓库。
翻译库保留现有快照；PoBR 发布自己的运行包。源码修订、事实数据修订、翻译快照和最终运行包身份分别记录。
若沿用上游已发布数据作初始输入，明确标记为固定社区产物，不声称已由我们从客户端完整重生。

**存储边界（用户于2026-11-01再次明确）**：PoBR源码仓库不保存完整游戏表、语言词典、版本快照或原始／领域数据归档；仅保留代码、契约、需求／版本锁、必要来源元数据和小型合成夹具。
CI构建按精确修订与摘要从RePoE、翻译库导入数据，在临时目录适配、验证并生成CLI／WASM／Web发布资产；测试需要实物数据时同样临时导入，可复用忽略的本地缓存。
客户端索引定位、范围采集、原料重放及来源包生成也归独立RePoE；PoBR不保留重复collector。ordinary构建仅临时导入固定的三文件纯验包闭包，完成后收据保留代码身份，运行包不携带该代码。缺失路径的显式diagnostic不产成功selection，来源接受与语义／运行采用仍分开。
旧Node原始索引／表导出器及其安装依赖已退出；版本候选由完整外部owner锁准备，本地采用另显式指定预期CURRENT并运行原有消费者门禁。历史raw DTO／vendor诊断仍单列，不当作当前来源生产；见[锁驱动更新](../pipeline/runtime/locked-update.md)。
构建产物可随应用分发，但不因此成为PoBR Git内的数据副本；PoBR自己的artifact ref也不能作为外部事实／语言库的替代。
当前Git tree已移除四份完整快照、七份外部shared文件、四份历史大policy映射、辅助说明表和十一份完整API测试响应；Git历史不改写。
当前与最小旧版回滚均有两owner的独立固定产物，ordinary大映射归RePoE、语言加工归翻译库；PoBR保留小template、十一份自有规则、锁与导入代码。历史snapshot兼容事务仍保留，但语言加工已调用固定翻译producer，原始下载及隐式升版入口已退出；当前完整版本使用[锁驱动更新](../pipeline/runtime/locked-update.md)。
[临时prepare](../pipeline/runtime/build-data.md)拒绝缺失、错误摘要或未声明输入；旧PoBR artifact／历史快照不再作生产回退。旧artifact对象的历史存在不代表仍被现行锁消费。

[最新完整发布同步](../pipeline/latest/latest-data.md)从RePoE `master`和翻译库 `main`的发布索引发现候选，
将两owner、每个领域修订及全部消费锁冻结到`pipeline/latest/data-composition.json`。
首份组合登记现有0.5.5.5输入，不代表新采集或新的来源接受。后续同补丁更新也须保留序号和前驱，
经精确head的PR／Full门禁、完整与紧凑投影核对及同次WASM消费后，才可条件采用。
定时流程需要专用凭据和远端严格分支保护；提交接线不代表这些托管条件已经启用。
owner尚未完成新数据生产时保留旧锁并报告缺口，不用版本字符串、源码合并日期或部分新表冒充完整更新。

**统一的是数据身份、生产入口、输出契约和维护责任。** 原始事实可能来自多个必要来源，但每个事实域
只有一个明确生产者；核对源不会在运行时自动覆盖主源，补充项必须有出处和优先级。
同一字段若出现不同值，先核对服别、补丁、单位和作用域，再按证据决定；不做多数投票或最后写入者获胜。
人工修正绑定实体/字段、适用版本、原值预期、理由及证据；升级后原值不匹配时重新审查，不能无条件套用。

### 1.1 跨仓问题归属与共同交付

统一架构不要求合并仓库。修复放在拥有该事实或契约的一层，其他仓库只更新固定产物与消费接线：

| 问题 | 首要修改位置 | 下游责任 |
| --- | --- | --- |
| DAT/CSD/PSG 解码、原始槽位、关系、字节范围或未知字段丢失 | RePoE 及其固定 schema/解码依赖 | PoBR 更新生产者与产物 pin，并验证领域转换 |
| 交易模板配对、译文冲突、人工补译、客户端描述的语言候选 | 翻译库；原始 CSD 转录缺陷先在 RePoE 修复 | PoBR 更新固定语言产物、匹配与显示回退，不复制第二张人工工作表 |
| 稳定 ID 适配、单位/作用域解释、stat 映射、构筑准入和数值 | PoBR adapter / Rust 规则与构筑层 | CLI/WASM/Web 验证同一契约和来源归因 |
| 已有译文未消费、属性标签遗漏、语言切换或显示分组 | PoBR 显示接线 | 保留原始导入文本；不为 UI 缺陷改写事实或重做翻译采集 |

翻译来源分为交易模板、客户端原始描述、固定名词/说明词典和人工策展，不能互相冒充。
现有独立翻译库不是完整游戏汉化包；PoBR 可将已固定外部词典转换为只读显示资产，
但须保留 source revision、原文、文件摘要、生成命令和缺译原因。新增人工补译归回翻译库。
按技能 ID 连接时仍核对完整原文/版本，不能将同名、近似文本或字形转换当作同一来源证据。

跨仓交接至少包含：生产者提交、产物摘要/schema、游戏补丁与服别（未知明确保留）、
改变的字段/关系/语义、消费方所需变更、已跑检查和未验证范围。生产者变更后重新绑定消费收据；
源码合并、事实发布、PoBR 采用、活动数据切换是不同动作，不因上游合并自动执行后续动作。
对已有来源缺陷，应修复拥有它的仓库，而不是在 PoBR 永久增加无出处的补丁值。

## 2. 数据究竟来自哪里

### 2.1 已核实的 RePoE 获取链

核对源码版本为 `aa9fa7c394b18a7a89f913f86703112b3f9d20f1`。
[run_parser.py](https://github.com/repoe-fork/repoe/blob/aa9fa7c394b18a7a89f913f86703112b3f9d20f1/RePoE/run_parser.py)
接收本地 `Content.ggpk` 路径；默认通过
[util.py](https://github.com/repoe-fork/repoe/blob/aa9fa7c394b18a7a89f913f86703112b3f9d20f1/RePoE/parser/util.py)
向 `ggpk.exposed/version` 查询资源地址，再交给 PyPoE 的文件系统和 relational reader。
[README](https://github.com/repoe-fork/repoe/blob/aa9fa7c394b18a7a89f913f86703112b3f9d20f1/README.md)
明确说明使用 PyPoE 解析游戏资源，表结构规格来自社区 dat-schema。

因此，已核对的主导出链是**解析官方客户端资源**；从 CDN 服务器下载客户端文件，不能解释为读取游戏服务端数据库。
没有证据表明 RePoE 掌握完整服务端公式或隐藏规则。社区 schema 及其字段命名也需要验证。

这条主线之外还有实际网络输入：[stat_translations.py](https://github.com/repoe-fork/repoe/blob/aa9fa7c394b18a7a89f913f86703112b3f9d20f1/RePoE/parser/poe2/stat_translations.py)
会请求实时 `api/trade2/data/stats`，即使客户端来自本地文件；随后用整段文本、拆行和数字替换匹配，将 trade ID 附到 CSD 结果。
所以固定客户端与工具还不足以让当前所有模块离线重放，文本匹配也不能直接认证 trade ID 与游戏 stat 的对应关系。
目标生产将**纯客户端描述导出**与**可选交易关联补充**拆开。后者记录 URL、服别/语言、采集时间、原始响应与摘要；
交易目录不提供明确客户端补丁时保留未知。匹配结果先作为候选关联，核验后才进入正式映射。

已发布 JSON 另在 [repoe-fork/poe2](https://github.com/repoe-fork/poe2)，与导出器源码仓库是不同产物。
其[固定工作流](https://github.com/repoe-fork/poe2/blob/75885a099f2e9be1619aa3c793fe6ab0754a511f/.github/workflows/export.yml)
定期检查游戏版本，运行 PyPoE schema import 和 RePoE，再发布数据。
它对 RePoE/PyPoE checkout 未固定 revision；版本发现与实际导出还会分别查询资源地址。
我们的生产流程必须固定一次发现结果及实际输入，不能仅凭 `exported-version.txt` 宣称完全可重放。

### 2.2 推荐来源顺序与使用方式

| 需要的数据 | 主来源/获取方式 | 补充与核对 |
| --- | --- | --- |
| 表内 ID、数值和关系 | 官方 CDN 对应补丁资源，或同补丁本地客户端；由自有 RePoE 导出 | 固定社区原始表导出可作过渡输入；另一路解码抽查关键字段 |
| 天赋图成员、拓扑和布局 | 客户端 PSG、被动定义与替换表 | GGG 官方 skilltree-export 核对布局、文本和关系；精确补丁未确认时只作旁证 |
| 描述、继承属性和对象定义 | 客户端 CSD、`.it`、`.ot` 等资源，由相应模块解析 | 官方文本、维护中的社区解析工具及具体游戏观测 |
| 客户端不完整公开的内容 | 有出处的有限策展与游戏证据 | 官方 patch notes、受控实测；PoB2 为交叉核对，不是真值 |
| 交易目录和描述关联 | 官方 API 的固定原始响应快照，独立于客户端定义 | 文本匹配只产生候选；不把采集时间当游戏补丁，不将 trade ID 冒充游戏 stat ID |
| 简中词条和术语 | 自有翻译库及经核实的国服资源/词典 | 保留每条来源与歧义；不以繁转简冒充国服译文 |

官方公开 API 另用于其实际支持的角色、交易等功能，不当作完整游戏定义或公式接口。
GGG 的[数据导出说明](https://www.pathofexile.com/developer/docs/data)目前只将天赋树列为
PoE2 游戏数据导出的例外。官方来源的客户端字节与官方支持的数据接口是不同的来源类别；
客户端解析依赖的 schema、解码器及字段语义仍需自行验证，不能据此承诺受支持的服务端全量导出。
第三方网站只有在来源、版本、许可和接入方式明确时才进入候选；同源转存不增加独立证据。

旧补丁资源可能不再可下载。原始输入、schema 和导出工具的可核实归档因此属于持续维护能力。
本次对旧补丁索引的一次探测返回 404；这不证明所有旧资源均不存在，也不允许改用最新资源后贴旧版本标签。

## 3. 一条生产入口、分清数据层次

正式生产顺序为：**确定来源锁 → 取得原始资源 → 自有 RePoE 导出 → PoBR 规范化与补充
→ 按产品范围筛选 → 审计候选 → 发布固定数据包**。
翻译包在规范化后按稳定身份合入显示/输入边界，PoB2 对照在验证阶段独立运行。

| 层次 | 保存内容 | 职责和限制 |
| --- | --- | --- |
| 原始证据层 | 实际消费的客户端文件、社区过渡输入、schema、工具版本和字节摘要 | 保留原貌，可离线重放；大文件不塞进 Git 或浏览器数据包 |
| RePoE 事实层 | 游戏定义、原始 stat/值、完整关联、发布状态证据、资源路径 | 不按 PoB 覆盖过滤，不混入引擎假设；保留未识别信息的诊断 |
| PoBR 领域层 | 面向构筑的物品、技能、天赋、actor、描述等规范化模型 | 稳定 ID、单位、条件与作用域明确；消除上游格式差异 |
| 规则与场景层 | Rust stat 语义、用户条件、敌人/EHP 场景、必要人工模型 | 与客户端事实分开，分别记录游戏依据、近似与支持范围 |
| 语言与兼容层 | 显示文本、国服输入模板、PoB 构筑码索引/别名 | 语言切换不改变计算身份；兼容编号不主导原生模型 |
| 发布层 | 已筛选的数据包、manifest、来源收据和验证结论 | CLI/WASM/Web 使用同一身份；缺陷候选不能直接替换活动包 |

统一入口负责编排现有职责，不建设单个无边界的大脚本。缓存、报告和发布包各有位置。
物理目录迁移服从领域契约；现有 `base/overlay/generated` 暂时保留兼容读取，不能把目录改名当成完成架构统一。
原生结构化事实直接进入领域适配与计算，不重新解析其显示文本；外部装备原文等文本导入继续保留解析、原文及损失诊断。

## 4. 目标数据结构与身份规则

目标按领域组织，而非按“来自哪家工具”组织：物品与词缀、技能与品质、天赋与珠宝、角色与召唤物、
描述与本地化、规则与场景、兼容映射。复用当前有价值的类型、快照和加载器，逐域调整。

- **主键**：优先使用游戏稳定字符串 ID；表行号只用于本次导出连接外键。树 graph ID、游戏定义 ID、交易 stat ID、PoB 编号各自保留，不能互换。
- **stat**：保留原始 stat ID、数值、缩放单位和所属来源；原始值与规范化值之间的转换可追踪。不能仅剩英文描述再反向猜数值。
- **关系**：显式表达 gem→effect→stat-set→等级/品质，item→base/mod/augment，召唤技能→实体→技能/成长等关联。
- **条件**：对象、技能分支、装备部位和条件作用域与数值同时保留，不能把条件项扁平化为全局加成。
- **树**：区分“被动定义存在”和“该补丁的图内实例存在”；节点、替换、别名、拓扑与几何属于同一树身份。
- **语言**：稳定 ID 与显示模板分开。中文原文、参数顺序、值转换和歧义可追踪；缺译回退不改计算身份。
- **状态**：区分未知、缺失、不适用、尚未支持与已验证，避免统一默认成 0、空列表或不生效。

一份包同时标识游戏、服别、客户端补丁、树修订、领域 schema、导出/适配版本和翻译修订。
游戏 ID 的稳定性不假定跨所有历史版本永久成立：身份带游戏/服别/修订上下文，重命名或替换另存兼容关系。
相同客户端补丁可以因数据修复产生新的数据修订，不能覆盖既有发布物。
来源匹配、转换完整性、游戏/计算证据分开记录，不压缩成一个含糊的 `verified`。

技能分支迁移须同时处理选择与规则查询。目前构筑模型及 `pobr-core::skill_env` 的
`stat_set_index` 仍传递 PoB 构筑码的 **1-based 兼容编号**：`BuildData::select_stat_set`
通过 `StatSetDef::vendor_set_index` 选择分支，抽取规则的 per-set key 也使用该编号。
它既不是客户端表行号，也不是原生分支 ID。目标是在导入边界通过固定兼容映射解析为
`GrantedEffectStatSets.Id`，内部选择、等级数据与自有规则均按此 ID 查询；导出时再还原兼容编号。
这需要连同辅助、buff 和未选分支的作用域一起迁移，不能只修改 XML 解析或重命名字段。
原始编号继续保留以支持未解析构筑的往返；映射缺失、冲突和缺少证据的默认分支必须显式诊断。
在该迁移完成前，不能将原生事实候选的接入宣称为技能选择已脱离 vendor。

翻译库主表仍以**完整交易 ID＋英文分支**为键，不能直接按游戏 stat ID 连接。
其编号模板是独立构建产物；trade ID→游戏 stat 的对应关系只有证据充分时才建立，不以数字后缀或近似文本猜测。
缺少游戏名词时保留独立、固定来源的名词边车，未来补入自有链路；不复制第二份人工词条工作表。

国际服与国服的数值差异以实际输入核对：可证相同的领域可共享，差异领域按服别保留。
国服资源未核实时明确标记验证边界，不能因有中文文本就宣称已经支持国服全部数值。

## 5. 当前已有数据与实际缺口

当前设计分支已在本地采用 **0.5.5.5**；`data/CURRENT` 和默认 CLI／Web 数据同步均选中该版本，CURRENT本身不提供数据回退。
完整数据已迁往独立RePoE／翻译仓库，由精确锁临时组合；PoBR tracked `data/`只有CURRENT和十一份自有规则。原`7793f93b`入库快照的过渡存储已撤出当前tree，历史不改写。
当前完整快照有67个manifest文件、简繁两种词典及18份共享JSON；manifest SHA为`73b2d77e669743ced67df5cf6e0245f79e333bb9b2a481b796da7099d8f63a06`，
完整runtime身份为`dba0615e55471570b22cb7759b3906b1db9e78629a2a6846a743e070708f0e72`。制作目录改为必需的独立七表事实投影，92条业务记录不变，来源元数据更新。
旧`0.5.5.4.2`由独立回滚锁恢复runtime `dcfc32c2`，可用`--keep-current`安装作兼容测试；旧完整目录不再入库。独立数据artifact已按授权发布；源码接线已由PoBR218／219／221、翻译库5及RePoE7／8合并。设计分支使用独立干净发布历史，未部署。

自主RePoE事实和CSD已通过领域adapter接回普通DTO、GameData／BuildData及原计算路径；保留构筑码导入、装备、技能、天赋、配置和存档能力。
六域最新输出与N4显示已封入SC紧凑release `9932ea87`／runtime `ec4f3776`；原2a03ad79／d7ac23c5基线与当前73b2d77e／dba0615e组合均投影到这一紧凑身份。精确7e4d0c23的完整CI重新生成同一release，下载产物核验完整归档摘要及全部block身份通过；新来源锁留在构建收据中。
既有356条开发输入的真实TypeScript／WASM API流程和三对CLI结果按原固定代码／产物身份复用；初次2a03ad79／d7ac23c5基线另验证真实CLI／WASM构筑及采用／回滚。当前制作组合以固定CLI验证默认身份与业务／诊断等价，完整CI另验证实际WASM和组包。
root从远端Git重新取得当前及回滚输入，恢复相同manifest／runtime／bundle；无数据路径或版本覆盖的CLI计算与候选结果完全相同，Web默认同步86个运行文件。
初始2a03ad79／d7ac23c5基线的真实本地浏览器完成Druid／Oracle导入、计算、等级／笔记编辑、刷新恢复及打造exact-ID的“蓝玉”；该阶段复用原WASM，Web757通过／8跳过及两项多版本构筑证明保持原身份。
PR221精确7e4d0c23的[完整CI](https://github.com/ackness/pobr/actions/runs/41348105556)通过三个Rust分片、fmt／Clippy、automation、Web／真实WASM／独立打包、声明运行组包和Full gate；E2E未选。它不认证全部浏览器／BD、准确率、CN数值、许可或所有用户patch组合。

本轮root已分别接受精确91项tree／IT资源、10项CSD和12项语言原生资源，加入原F77及六DAT采集决定。
[sources.lock](../pipeline/imports/sources.lock.json)的`retained_input_acceptances`另记录locale／shared／compat三类有界保留决定，逐项状态和精确pin生效，不改原五项采集接受；另增的六OT／两描述关系DAT仅按各自精确字节范围接受。
它们固定旧字典、140项native-selected与2项literal增量、v3显示、空审核补译包、18份shared及兼容输入，保留原来源版本／社区解释和未解决状态。
三个vendor抽取表在固定版本重现；crafting历史执行输入仍未知。新增七表中立事实与[必需目录投影](../pipeline/imports/crafting-inputs.md)已在禁网本地组合中验证：89源行／92条业务记录与旧基线一致，八槽／源权重分别保留，null stat 的非零区间保持诊断。新来源元数据manifest `73b2d77e`／runtime `dba0615e`已由发布artifact2c664da4恢复并本地采用，fresh远端Git、禁网重放和默认固定CLI等价验证通过；语言声明缺失保持unknown。独立审阅发现的English默认P2已修复；不覆盖原保留接受记录，也不把本次本地验证说成完整CI／全部语义认证。buff引用检查仍exit 1（15处窗口唯一定位，refs未改），parser检查exit 0但有closure／count警告。
SC客户端身份、CN数值、分发许可和未知机制仍未认证；有界接受不代表全部自主迁源。

审核补译已集成到PoBR importer及显示生产者：官方优先，补译须审核并精确绑定catalog、版本、区服、英文和完整context。
真实包仍为0条，两个草稿不生效；新增入口未改变显示sidecar业务字节。原catalog与335MB verification只作离线构建输入，不进入玩家包。
下表列出当前迁移结果；第5.1节列出真实缺口，历史阶段与验收范围单列在第5.2节。

| 领域 | 已完成接线 | 仍需处理或明确保留的边界 |
| --- | --- | --- |
| 物品基底/词缀 | 目标事实生成普通基底、Stats/Mods，保留八槽值域及 Families；已撤销目标匹配基底的六类旧数值覆盖，继承tags已集成并实际生产 | 部分护符效果及未拥有字段仍需证据，generation weights/条件分别核对；继承仍使用单列的固定社区解释 |
| 技能/分支 | 效果、逐级数值、stat-set、gem/effect 关系及可证 BaseMultiplier 已迁源；本批实际移除77项raw-bound implicit旧注入，复用普通技能转换 | 遗留7项implicit覆盖因当前codec无对应effect而不注入，28个目标set已有源stat；历史分支身份仍未知。corpse／speed／area及虚拟效果等解释继续保留，旧导出器编号不是目标raw身份 |
| 品质 | 从目标事实重生 main/alternate 品质数值并复用既有消费 | 原始 scope 保留，但计算仍按 effect-wide 兼容策略；未认证更细作用域语义 |
| 天赋 | 目标图、解锁FK与完整文本接原普通树；实际4913节点为4823 CSD、51 mixed CSD／ClientStrings、39整文本兼容；72 variants为70 CSD、1 mixed、1整文本兼容 | placement／armour及未拥有字段仍按固定策略保留；177 native packet是优化通道，不代表普通树覆盖上限。mixed中的ClientStrings尚缺独立语言绑定 |
| 符文/魂核 | 305目标条目及一个历史拼写alias接原普通槽位/限制/效果消费；296条目使用目标CSD，并接自有翻译库选中描述 | 9条整项兼容：5缺compound上下文、4缺当前CSD；不猜零、不为清空列表引入机制 |
| 描述 | [普通stat-descriptions生产者](../tools/pobr-data-adapter/ordinary-stat-descriptions.md)的可选plain-display政策实际拥有global／passive／aura的11004／214／24个render-descriptor member ID；保留完整分支、raw条件、格式及lookup，TierIndex消费缩放反解。另由source维护25项unrendered诊断，不增加可渲染文本 | 新政策旧10442个singleton中17个、279个compound中3个仍兼容；37个scoped兼容ID是另一分母。当前声明式Git输入及release9932ea87已接入这两项等价descriptor；原诊断政策19／3及39项、R／旧lookup政策64项保留各自历史身份。tier仍依赖tags、spawn weights及原始roll范围；已匹配样本不证明全目录tier覆盖 |
| 暗金 | 现有文本、roll/变体目录保留；部分装备授予技能关系已由目标表生成 | 完整暗金→词缀归属及roll范围的独立来源仍缺失；RePoE名称/素材目录不能替代它 |
| actor/常量/关系 | 怪物抗性/成长、图腾、召唤目录、玩家基础/徒手/异常常量、Boss数值、珠宝槽及装备技能关系已逐批迁源；本批实际为649个现有actor拥有27字段 | mod_list／目录策展与Boss选择、模型假设仍分列。Flag0／NoReservation保留明确社区解释，不伪称官方字段语义；本批actor业务值与旧候选相同 |
| 翻译/双服 | N4普通backend已实际消费TW及SC FK v3目录，两语言各1073 gem为1053源译／20英文阻断；12职业／23升华路由及literal／null回退通过。审核补译、打造基底exact-ID名称及独立辅助说明资产已接线；官方优先、草稿不生效 | 路由数不等于全部译出。真实审核运行包395B／0条，两个真实草稿仍未生效；非空行为仅合成用例。辅助说明保留原0.5.5.4.2生成身份，须精确catalog／skill_id／英文匹配；其它scoped说明、CN身份／数值及真实审核译文仍待 |

后续清单按字段归属维护：未迁游戏事实、固定社区解释、PoBR计算规则及历史编解码分别处理。
旧阶段文件清单中的“remaining”不自动代表当前尚未实现；已迁字段不重复立项。
本地候选验证按各批固定代码、包和消费身份记账，不将历史356 API/3 Web的结果直接移用到后继组合。

### 5.1 剩余字段与关闭条件

下表从 `fbe50261` 基线逐项对账至下列固定集成与消费产物。计数来自固定输入和收据，
不是编译白名单、覆盖率或新增机制清单。保留依赖只有取得对应字段的证据并验证真实消费者后才能退出；
不能只刷新兼容 hash、删诊断或将未知填零。收据代号见本节末的定位表。

| 字段／域 | 当前实现与固定证据 | 下一项可执行工作与关闭条件 |
| --- | --- | --- |
| 描述：新候选17旧singleton、3旧compound及37个scoped兼容ID | [plain-display政策](../pipeline/policies/stat-descriptions/0.5.5.5-plain-display-policy.json)把 `gain_x_rage_on_hit` 和 `bleeding_damage_on_self_taken_as_fire_instead` 的重复定义按既有显示标记投影归为等价；其余英文语义字段必须完全相同，receipt保留全部raw pointer/hash。实际source target为11027／215／25，其中25项是原诊断政策的unrendered。剩37项＝8歧义owner＋13缺PoE2关系handler＋16无ordinary分支。15项描述／11项renderer测试通过；实际新候选10文件A/B同字节、独立全源oracle匹配，非_meta目录仅新增两个descriptor，所有旧文本不变。旧诊断政策重放conversion及非_meta目录与既有普通领域产物相等。证据在协调者`.cache/description-owner-semantics-20261101/`；原诊断批`.cache/source-diagnostics-20261101/`保持原身份 | ae39a0ff已绑定新政策至Git a9ed05f8，六域生产、双次组包及实际TS/WASM／CLI消费通过；集成证据见`.cache/plain-display-integration-20261101/`。继续按binding补真正歧义owner、PoE2关系handler或ordinary分支；新两表观察仅有空support表及单行rawkey0 reward，第三表未匹配，不能按ID后缀或ordinal补关系，13 handler仍保留；25项诊断仍effectless=false，whole-packet renderer默认行为不变。旧R64项及WASM证据不重标；不借PoE1关系、删诊断或将未知条件当等价。D的681／279／1303仍是历史计数 |
| 描述：分支／缩放反解与tier | R已接可选descriptor DTO：raw条件先于显示转换，ordered affine格式为 `(raw * multiplier + offset) / divisor`；九表lookup单列。原JSON无metadata时行为保留。[TierIndex](../crates/pobr-item/src/tier.rs)只读global scope，用原始整数roll区间的正向显示像反解，遵守first-match与完整compound；同名key／多mod歧义不给tier | 已通过描述8／tier9等聚焦测试；新WASM实际对开发324／352分别标出24行／41行tier。仍需按具体消费者验证其它scope与未覆盖装备；标签受tags、spawn weights、domain及roll pool约束，跨行hybrid／不可滚词缀等不能据描述覆盖率推断 |
| 符文：5项 compound 上下文 | S 的整项 fallback：四个 `RuneEnhance*` 缺 `local_weapon_no_physical_damage`，Spiteful缺 `attacks_inflict_bleeding_on_hit`。J新增调用方回归区分缺成员、变化后拒绝和明确源值0，R的12项socketable测试已通过；真实packet缺口未变 | 从目标资源取得缺成员或缺省语义的证据，再接[完整packet渲染](../tools/pobr-data-adapter/src/ordinary_facts/socketables.rs)。89份 `.it` 及已持有packet未建立默认0依据；测试通过不能使5项整项兼容清零 |
| 符文：4项当前 CSD 缺记录 | S 分别记录 `TalismanFox` 的 `display_additional_socketable_mods_for_idols`、`IdolSpecial7` 的 `companions_gain_onslaught_when_you_parry`、`RuneOfTheAncients26` 的 `rage_effect_+%`、`RuneWarpingAdditionalSuffix` 的 `local_maximum_mods_allowed_+`；ID前缀均为 `Metadata/Items/SoulCores/` | 原九文件及J报告的27个精确top-level CSD搜索均无这些ID的完整定义；不是全客户端／所有嵌套scope搜索。沿[有界定位／采集](../pipeline/imports/client-index.md)补确切资源与语义。SoulCores两个描述列均null，Stats标记／ID后缀不能授权静默省略 |
| 天赋：39整节点、1个variant整文本兼容 | J的v4与DESCRIPTION helper已集成；R实际4913节点＝4823 CSD＋51 mixed＋39 whole，72 variants＝70 CSD＋1 mixed＋1 whole。文本字节变化为9个base entry及1个variant-bearing entry，owner变化可不改文本；110／2是T旧阶段，59只是J原来源候选 | tree/render 53项测试、真实双producer及三代表API已通过。任一buff、未知stat／template失败仍整节点fallback；4987旧binding保持并不代表当前fallback数。本轮39节点首阻断为30个当前effective scope缺描述、3个owner歧义、3个未完整表示buff、2个无匹配分支、1个缺compound成员；另1variant缺描述。30是节点数，不是缺stat ID数。三buff的PassiveSkills→BuffTemplates→BuffDefinitions物理FK已存在，缺完整效果表示／值与适用解释，不再列为原始采集缺口；27top搜索不扩称全客户端。mixed中CSD已可选译，ClientStrings另缺语言绑定，不能按英文相同借用 |
| 天赋：解锁身份 | R已实际生产UnlockedBy物理FK→GraphId有序列表、VisibleForAscendancy物理FK→稳定Id，接原[DTO](../crates/pobr-data/src/catalog/tree.rs)／[allocation](../crates/pobr-tree/src/allocation.rs)的all-prerequisites语义；J的200个constraint源／旧DTO对照与本批测试分开记账 | 已编译、聚焦回归及普通API运行；未知constraint扩展仍保留，Requirement仍为opaque ClientStrings，不能生成predicate。200相等不是AND语义依据；三例level编辑也不等于完整玩家加点／variant验收 |
| 天赋：placement、armour、资格及未知字段 | T／J分列目标group中心和prepared偏移／坐标absence；几何／group rounding函数与base字节相同，12个true `apply_to_armour`仍prepared，239图实例仍逐定义hash排除 | 取得物理radius／offset单位、armour成员资格及条件字段依据，验证范围珠宝／涂油等原消费者。不得按坐标／stat名称猜armour，或将图中存在当可分配性；Requirement／QuestFlag／IsAnointmentOnly等未拥有上下文仍受guard |
| 基底：继承 tags | V最终actual-c已接原 `BaseItemDef.tags` 并由R实际生产；5496源行连接82 roots／89份物品 `.it`，保留284字段／71个Base.tag字面occurrence。显式 PyPoE66d 社区策略按child-first顺序取并集，再与DAT tags合并为排序去重的DTO set；不认证官方merge law，不涉及actor `.ot` | 基底19项测试及双producer通过：4902条实际table tags变化；按原loader的table＋overlay并集比较，3134集合变化、1943原空集合获tag，不再只是I预测，但此比较本身不是第二次Rust loader执行。v2仍拒绝mixed同名ownership，保留未resolved／未匹配；tier与三API仅覆盖本批样本 |
| 基底：socket、subtype、charm 及未匹配入口 | B 的[覆盖层转换](../tools/pobr-data-adapter/src/ordinary_facts/base_items/compatibility.rs)已撤六类目标数值覆盖；V／R仍保留其它JSON和5个未匹配入口，未解释 `.it` 的其它字段 | socket_limit／sub_type被WASM augment编辑器直接消费，须保留并验证容量及buckler／quarterstaff筛选；13个charm已有UtilityBuff→BuffDefinition关系（5数值／8GrantedFlags）；全13含未解释的Flag1／Flag4／Unknown0／Unknown3及MergeMode=3，固定schema／PyPoE只证明物理关系和较窄的显示配对，0/13完整packet迁移，12历史文本保留，不能再立重复raw导出。详见[基底契约](../tools/pobr-data-adapter/ordinary-base-items.md#remaining-source-and-consumer-contracts)。不把tags交付扩为rarity／socket／charm／mod eligibility全迁源。Gold Ring `FourRing10` 的旧表层tags虽为 `[]`，旧[loader](../crates/pobr-gamedata/src/domains/base_item_overrides.rs)还合入default/ring及两个 `genesis_tree_*`；继承后为default/ring，不能用旧表层空值代表运行时集合 |
| 共享 buff 引用与上下文 | RePoE [固定来源对账](https://github.com/ackness/repoe/blob/e3cabf9bd008240e12a173c7233c2c48e56eb822/RePoE/docs/buff_source_reconciliation.md)认证ce8／bb52两个提交、两文件全文、15个引用窗口及7个完整scope；14项局部仅注释变化、Arcane分支及前序相同，保留定义字节不变 | perform／defence／reservation的上游条件有变化，整体等价未确立，`guard_eligible=false`、`semantics_verified=false`。验证器默认exit2；evidence-only exit0仅说明证据成立。PoBR原15处漂移失败及兼容保留，不把局部搬移改称完整语义通过 |
| prepared 技能／品质解释 | K旧实物的94项解释含84 implicit／8 corpse／1 speed／1 area，另有虚拟倍率及10个未绑定codec决策。V v2已集成，R实际移除77项source-bound implicit旧注入；其它entry含重复次数精确保留。primary／selected sets的ImplicitStats／IgnoredStats证据与原ordinary-skills转换继续使用 | override 15项回归及双producer已通过，原普通技能保留对应flags；只读跟进确认7项旧implicit当前惰性，28个目标set已从源事实获得对应stat；移除它们只属政策清理，不代表计算能力迁移。8 corpse、1 speed、1 area及虚拟EnemyExplode解释仍保留，历史branch codec不能按raw顺序推导。品质effect-wide分类仍为固定策略；规则按实际输出及游戏依据处理 |
| actor：有序技能及4个prepared字段 | V v3与root supplement／六表选择及fingerprint已集成；R实际为原649个条目（32 minion／617 spectre）拥有27字段，复用原[DTO](../crates/pobr-data/src/catalog/actors.rs)与[minion](../crates/pobr-gamedata/src/domains/minions.rs)／[spectre](../crates/pobr-gamedata/src/domains/spectres.rs) loader。84＋2694个技能引用按物理FK→GrantedEffects.Id保留顺序／重复／空列；实际业务值与旧候选相同，身份metadata改变 | actor最终20项回归与双producer通过。category用MonsterCategories.Tag→Tags.Id显式映射，不用Name／Unknown0，无Id／Type列；weapon用ItemClasses.Id显式映射，未知武器类拒绝。Flag0仍固定column8社区解释，收据明确official_semantics_certified=false；业务值相同不认证官方语义或所有召唤玩法 |
| actor／玩家／场景未拥有字段 | V保留membership／aliases／limits／extra_flags策展；accuracy=1是固定兼容规则，不是缺失客户端倍率。六组DAT Mods已存在；RePoE独立draft现另存BearCompanion／LivingLightning／SummonedRagingSpirit三root的6份OT、11172B原料及中立事实，fresh远端62成员核验和OS拒网重放通过，已另限精确采集字节接受，未迁移mod_list。spawn_location仍属于区域／遭遇策展；P 已接37+16基础绑定，其它常量及Boss选择分列 | 三个OTC候选在固定index未匹配；其它catalog root尚不在此闭包。合并／set_操作／Stats作用域、DAT Special_Mods适用与OT优先级仍需独立依据，不能把-50／40同值或Boolean直接当旧DTO数值的证明；详见[actor契约](../tools/pobr-data-adapter/ordinary-actors.md#observed-three-root-ot-draft)。649项InheritsFrom不是单个root，89份物品 `.it`不能代替actor。模型只按真实输出需要修改；不把141个Character.ot assignment称为已求值，不为迁移计数新增模型或伪造raw字段 |
| minion关系／skill-stat-map规则 | V确认IgnoreMinionTypes原已迁源；effect→variety aliases、minion_list／add_minion_list、minion_uses／minion_has_item_set仍为策展。skill-stat-map属于stat→modifier解释及历史scope，不是客户端数值表；132个目标缺失stat ID和不可抽取项保留诊断 | 完整技能→variety／装备关系仍需独立来源，8个skeletal effect共享MinionType物理row8却有不同alias，DisplayMinionMonsterType为0行，单列不能闭合。minion_uses主手借用有消费者，support-added与独立item-set模型仍deferred，不为迁flag新增模型。规则只按原消费者的实际需求和语义证据修改；不得以raw行齐全、替换hash或删诊断宣布自主 |
| 暗金：完整词缀归属、roll／变体 | U 的[授予技能接线](../tools/pobr-data-adapter/src/ordinary_facts/item_grants/links.rs)已核65关系，4个显式变体仅迁 `skill_name`／`granted_effect_id`；61关系未绑定暗金。variant membership／game_mod_id selectors／名称仍兼容 新补ItemVisualIdentity实表复核：17227行中87行有119个Stats FK，但449个stash→visual连接对应UniqueStat1..4全null，不能当词缀归属；完整row/hash见协调者`.cache/source-followup-20261101/unique-visual-proof.json`（43e4daca）。 | 取得目标 unique→base→完整 mods／roll／variant 的固定来源，保留全部 stat 槽与适用条件，再接原暗金／物品编辑消费者。UniqueStash 名称素材、89字面文件和这65关系都不能替代完整归属；NoReservation 仍是显式社区解释 |
| 语言：TW／SC目录与投影 | N的TW目录仍为58572字段／18列；N4真实SC FK v3为58588字段／18列，实际lookup为25097 translated／33491 fallback，输出1649082B侧表381d827a。三个现有caller索引为BaseItemTypes.Name 5503、Characters.Name 12、Ascendancy.Name 37；未使用scope仍在翻译库 | 全部查询decision及冲突保留离线，所有literal／null blockers／aliases仍参与，重复源身份拒绝。E4的TW1627852B及strict-v2 SC1507852B在N4旧版兼容运行中逐字节保持；E／N3更早产物及各自A/B／单次投影身份不重标。L的[三份ID名称侧表](../pipeline/native/native-name-translations.mjs)及静态support保留；N旧名称23822／旧v124409不重标。新SC侧表已按精确pin有界接受并纳入当前活动快照；完整来源客户端身份及全部UI消费仍未认证 |
| 语言：普通backend literal与其它scoped说明 | E／E2的null blocker及absent兼容规则保持；TW17260个literal key为15456可译／1804未决、579跨scope／alias组。N4 SC同为17260个key，15713可译／1547 null、415跨scope／alias组；N3旧9725／7535／1805仍属strict-v2阶段。这些分母不是scoped字段记录数，placeholder／未知语法不在此臆造渲染 | N4实际[普通backend](../web/src/api/wasmBackend.ts)＋原674654ef WASM在OS禁网下验证15类TW／11类SC literal及各24个null回退；E／E2／N3／E4既有结果不改身份。其它scoped说明caller、逐字段UI及浏览器验收仍待，未新增计算／BD结论 |
| 语言：普通gem／职业／升华名称 | N4继续复用E3的gemCatalog／ClassNames。gem沿既有skill_gems＋gem_effects边保留与WASM当前English相容的候选base，译名须一致；升华用exact ID＋English，职业用Characters.Name内唯一当前English及null blocker。已归属组的缺项／拒绝／改名／歧义保留英文，未归属保留旧路径 | N4两语言实际各1073 gem＝1053源译／20英文阻断、0未归属；各12职业／23升华路由核对，不称全部译出，SC Duelist／Shadow仍英文。物品打造基底选择器现另用已有BaseItemTypes.Name精确ID／当前英文，已拥有但null／空译文／英文不符强制英文，未拥有保留原路径；不改变生成物品原文或计算输入。非name字段、英文、原WASM目录及共享alias顺序不变。E3 Web23项／tsc及N3／E4旧SC全阻断保持原阶段；未实跑浏览器／数学／新BD，20回退也不自动成为官方缺译 |
| 语言：CSD比较与选中渲染 | H `94e36c1`的raw v1不变，固定九文件20737pairs／1149排除／6诊断，27top24506／1664／6498，31唯一文件union24521 scoped pairs；原242个implicit排除中241配对，余1字面冲突保留。R按同编号／escape／-d契约与固定e0c00bb7、新树receipt新增2个无歧义literal，2907选择保留／诊断 | mixed中ClientStrings／名称缺绑定仍诊断，不借相同英文或TW名词；CSD配对数不证明专名、九表语言UI或CN数值覆盖 |
| 语言：观察SC与目标资源／记录绑定 | N4已实际fetch Windows RePoE61fe85cc／翻译e61f1332，按Git inventory／manifest／blob固定v3 source dc001605及catalog manifest0ef819f9。原verification Buffer须同时匹配catalog／manifest pins及六模块闭包；真实createCatalogLookup逐root重新计算strict-v2及完整可达FK结构，再核catalog投影，不只接受版本guard | N3的source9ae5f1ee／catalog bbdf750a及raw 5同／4不同是固定旧阶段，旧7同摘要仍不可用。v3仍保留typed列、稳定key、当前英文和未声明FK的严格值比较；observed patch／realm保持null，target0.5.5.5／international独立。实物、文本适用与正式来源准入分开，不认定国服或CN数值等价 |
| 语言：显式FK绑定与剩余拒绝 | N4的pinned FK结构策略已消费ItemVisualIdentity／ModType／Stats依赖与原始strict-v2父证据；v3仍拒绝BaseItemTypes 8、Mods 7、Characters 1个binding group，共16组对应26个display字段拒绝decision；AlternateQualityTypes 26组全部接受。N3原4817／10271／12／26拒绝数只描述旧strict-v2 | 缺key、真实stat范围／英文变化及结构不等仍拒绝；声明边以有序stable-key完整可达结构比较，其它物理值保持strict，不按ordinal相异直接判语义。PyPoE66d1 Default locale fallback仍为固定社区解释，official_client_semantics_verified=false；不是官方merge／客户端语义认证。来源准入及其它尚未拥有字段不因FK交付而关闭 |
| 语言：官方缺译／补译工作台 | 独立0e7778c4已接v3：真实工作台58588当前条目、45 eligible，保留117160条prior observations。21项库测试与原真实导出证据复用；紧凑审核包395B／0条、SHA563015ee，两真实草稿仍不生效。PoBR 9f3f2022消费该包，真实58588决策为25097 official／33491 fallback，输出381d827a与原N4逐字节同 | source-review／拒绝绑定不能当官方缺译。718B单条补译只作明确标注的合成验证：真实库接受精确query，跨区服拒绝、英文或完整context不符回退；不自动批准Freezing Pulse草稿，不认证旧网站描述的当前机制 |
| 语言：补译版本／区服隔离 | PoBR显式reviewed-only入口绑定catalog原bytes／SHA、原v3 verification Buffer、独立紧凑包及workbench单模块pin；官方六模块与catalog.producer精确guard不变。查询传精确patch／realm／locale／table／field／ID／English／完整英文context，官方先返回、审核补译其次。933c40e3补拒v2＋补译及缺声明bytes的守卫，19 Node／12 Python定向测试通过 | 目前PoBR补译仅v3 international／zh-CN，旧v1／v2官方查询保留。SC观察patch／realm仍null；china／international与zh-CN／zh-TW独立，target-missing不是国服专属。换catalog／patch／context须复核，未知／内部placeholder／草稿不可生效；来源与CN数值另验 |
| 交付：声明式显示候选导入 | 需求JSON与Git lock固定九表／18字段／官方六模块，并显式启用独立审核包。补译artifact 5d019466（runtime.zip 1585B／989293a6）已发布到翻译库handoff/reviewed-supplements-20261101；workbench固定0e7778c4，两新增Git对象均由fresh Git获取并核bytes／SHA。d12b85bd的六域八替换路径及完成收据guard保持 | 9f3f2022真实禁网import 25秒、19／11定向测试与automation通过；root后继933c40e3的守卫由19／12定向测试验证。root用既有六域outputs与新显示import在OS拒网下组包4.451秒，玩家四文件及CLI ZIP全同9932ea87／ec4f3776；新构建收据绑定审核输入。未重跑普通producer／WASM／356 API。对应代码已随PR218合并，精确fa66514a轻量及完整CI通过；该历史局部实测仍不扩称全Browser／HTTP验收，本地活动采用见第5.2节 |
| 交付：必需制作事实与新组合 | RePoE74c48815的七表中立导出已由PR8合并，缺失语言保留unknown。PoBR221改用v2锁与[八槽目录投影](../pipeline/imports/crafting-inputs.md)，game part拒绝旧成品交集，v1回滚独立保留；实际89行生成92条目录，未改变业务值 | fresh Git、OS禁网重放、默认固定CLI及精确7e4d0c23完整CI通过；来源元数据更新完整组合73b2d77e／dba0615e／afc78a77，CI紧凑release仍9932ea87／ec4f3776。7条权重诊断与null stat的-20/-20保留；原旧执行身份、全部效果／概率、客户端身份和许可不因此认证 |
| 交付：独立语言生成与历史入口退出 | 翻译库629be2d9提供离线语言producer，PoBR已删除旧生成／下载／模板合并实现；历史dictionary／regen／adapt事务调用[独立生成锁](../pipeline/imports/locale-generation.lock.json)和[临时适配层](../pipeline/imports/locale-generation.md)，当前完整版本的bump／update按owner构建锁导入，不再解析语言来源HEAD | 固定15输入实际生成九个SC文件，27896＋140＋2＝28038且逐字节等于当前基线；原候选manifest2a03ad79／runtime d7ac23c5保留原身份。TW三文件与compact display仍独立组合，旧metadata保留身份；新输入须显式固定预期输出及适用目标。历史原表／vendor再生事务仅作显式本地兼容诊断，PoBR不保留第二套语言生成实现；历史复现不认证新客户端语义 |
| 交付：当前加载与体积 | [loadDataPack](../web/src/api/dataPack.ts)兼容原快照及[三块release](../pipeline/runtime/runtime-bundle.md)：core27515708B、rules15006508B、SC locale6243071B、manifest14142B，共48779429B／本地gzip-9合计5315211B；CLI确定性ZIP5315593B归档相同块。56845333B catalog及335031638B verification仅编译校验 | 当前release9932ea87／runtime ec4f3776为固定保留输入＋最新六域＋N4显示；相较a788ae27／dcc1ff4a，82虚拟文件仅stat_descriptions的两个descriptor及manifest变化，其余逐字节同。原树177个来源摘要变化、62d40368及旧体积保持各批身份；大小不含Web JS、WASM、素材及HTTP开销，非首屏／复访或峰值内存实测 |
| 交付：共享运行schema与必要可选块 | ae39a0ff的当前TS backend＋原674654ef WASM在OS禁网下重新完成356开发输入的导入／计算／level编辑／保存重开及旧包切回恢复，业务结果全同固定R；355 PoB码可编辑语义同，唯一.build仅保留既有tree_version null→0_5变化，计算和notes不变。历史树、966trade gems、1073 gem标签1053源译／20阻断、12class／23asc路由核同 | 复用compiled663b的CLI实际验证开发1／324／352三对输出，除各自固定runtime digest全同；未新编译Rust/WASM。原八个组包边界回归及automation保留d12b身份；本批14项编排契约测试通过，六领域28.630秒、356输入API26.753秒。两次完整六文件组包同字节，所有CLI物化bytes等于玩家块。非Browser／HTTP／准确度／holdout／生产回滚；审核补译的新入口及空包组包见上行，非空真实译文及远端发布另验；本地活动采用见第5.2节 |
| 交付：独立存储与临时导入 | [当前v2 build-data锁](../pipeline/runtime/build-data.lock.json)绑定RePoE game／独立crafting facts 2c664da4、翻译库a4bd9b5b；game part不再携带crafting成品，由PoBR必需投影；[ordinary锁](../pipeline/runtime/ordinary/ordinary-inputs.lock.json)绑定RePoE aed5f734，大映射外置而十份policy仍按原字节物化。辅助说明5046a18a归翻译库，十一份历史API响应dbd35717归RePoE，均为published固定Git产物；测试响应不冒充当前事实 | PoBR已移除四完整快照／外部shared七份／四大policy映射及完整显示、测试表。当前与回滚fresh远端取回后身份不变，缺包／错pin拒绝，无tracked快照或旧PoBR artifact回退。processed兼容表的原producer和适用限制保留，存储迁出不等于全部从客户端自主再生 |
| 交付：历史 Git 树原始输入 | RePoE 独立生成 `repoe-official-tree-input/v1` 的原样输入包；[PoBR 导入锁](../pipeline/tree/tree-inputs.lock.json)固定 archive／manifest／成员 pins，[临时导入器](../pipeline/tree/tree-inputs.md)取代直接上游 HTTP 和旧裸缓存回退。PoBR 仍拥有 quarantine、DTO 与领域候选政策 | 原始 bd87e651 JSON 为5140821B／b52be9c4，upstream0.5.5、game_patch null及原rights不变，两条Druid orphan定义完整保留。独立审阅无P0–P3，artifact26cceab3已发布；fresh远端Git验包／四producer blobs与离线重放通过，旧裸cache及本地源库读取由OS拒绝。现有24项snapshot／9项import fixture及完整automation通过，不代表Rust新生产、语义或CURRENT采用，当前普通F77／PSG树不变 |
| 来源准入／新鲜度／回滚／CURRENT | [采集契约](../pipeline/imports/client-source-draft.md)与[sources.lock](../pipeline/imports/sources.lock.json)分别固定七项raw采集接受和三类保留输入决定。原F77、六DAT、91 tree／IT、10 CSD和12语言原生资源决定保持；另限六OT及两描述关系DAT（共8资源／11282B）精确字节接受，缺失OTC／第三DAT及语义不随之接受；raw字节、物理解码、领域解释及文本适用分开。完整候选身份、缓存采用／回滚及CURRENT替换失败恢复已验证，独立旧版锁另实际恢复dcfc32c2且不改变CURRENT | 本地已选0.5.5.5；旧目录由外部锁临时恢复。SC身份、完整unique、未知机制及许可不随之认证。2026-11-01 22:01 UTC官方discovery实际返回0.5.5.5，189B观察响应SHA256 `4d7c3b2576f363d689f34a6cb70b4d77b8d1ea347fa039c17b1dff84d5d68e84`；这仅证明该时点端点公布的版本，不认证全部原料或独立客户端build。13:34超时及10:39历史响应保留原身份；正式上线前仍另核新鲜度、CI及部署回滚；代表性本地浏览器已验，未重跑未变化的大输入生产及356 API，不把本地演练写成生产回滚 |

H的比较投影按固定PyPoE66d1规则：每模板next index从0开始；显式k使用k并重置next为k+1，自动槽使用next后递增，
包括向后显式重置。仅默认／`d`／`-d`／`+d`格式可配对，编号须在stat arity内；raw模板、format、token、span和分支引用不改写。
转义括号不消耗编号，但原lexer语法诊断仍阻止配对；条件／限制／有序handler／字面数字／歧义等原边界不放宽。

N4的显示catalog契约要求manifest与catalog版本相同；v1允许三／五／六模块，v2允许五／六模块，v3必须六模块。
v3须与manifest的producer闭包相同并校验原verification bytes／SHA后独立重建绑定；v1／v2保留独立pin的兼容lookup，不强制与历史producer相同，真实旧输出已核同。
E2／N2的原三／五模块证据仍配各自冻结源码，不能混用修订或闭包。
target identity与未知observed-client patch／realm分别保存；接收格式、合成候选或resource／record文本适用不认证完整来源身份及CN数值。

既有批次的固定证据定位在交接保留的 `.cache/builddata-migration-stage/`；后继I／V、J、N、H分别位于所属owner交接的
`.cache/ordinary-field-ownership/`、`.cache/tree-packets/`、`.cache/remaining-native-language/`、`.cache/remaining-native-language-placeholder/`。
集成实物R位于协调者 `.cache/all-data-completion-20261101/integration/`，绑定本批源码、实际命令、产物及消费证据。
显示实物E／E2分别位于协调者 `.cache/all-data-completion-20261101/display-consumption/`、`display-v2/`；原E的14项及R的368项闭包保持固定。
普通名称消费E3实物位于协调者 `.cache/all-data-completion-20261101/scoped-names/`，不改E的数据或旧WASM收据。
真实SC的N3实物位于协调者 `.cache/all-data-completion-20261101/windows-sc/git-received/`；工作台W位于翻译仓 `.cache/native-workbench-20261101/`，以actual-b为最终结果。
显示体积与消费E4实物位于协调者 `.cache/runtime-data-footprint-20261101/`，沿用原目录／WASM，保留E／N3旧阶段身份。
声明式导入与实际FK v3的N4实物位于协调者 `.cache/data-imports-20261101/`，成功Git输入为actual-git-b，禁网重放为offline-a；路径不同的receipts不称同字节。
翻译库N2实物位于其owner的 `.cache/native-catalog-v2/`；九表raw pin对照另存协调者 `.cache/all-data-completion-20261101/windows-sc/nine-tables/target-raw-comparison.json`。
来源锁集成位于协调者 `.cache/source-lock-integration-20261101/`（REPORT19f6ece5／closure9185d634）；运行包集成位于 `.cache/runtime-pack-integration-20261101/`（REPORT0abe7571／closure23ea35dd／matrixffc8b950／CLI proofdd33adbd）。后者3592项证据及17源码pins保全，2492个保存的响应hash已复核；未重标旧R或WASM构建。
核心Git交付与fresh-cache导入实物位于 `.cache/runtime-git-publication-20261101/`（REPORTfb3862e6／closure9a43d220／proof63e2e4eb），原集成阶段pending陈述保持历史身份；首次sandbox临时目录配置失败另存，产品代码未改。
最终六域组合实物位于 `.cache/runtime-ordinary-integration-20261101/`（REPORT2d6e0975／closurefba65a96），3711项本批证据与11源码pins保全；六域30.214s、显示23.670s、组包A/B及当前356 API／三对CLI均属clean d12b85bd，WASM和CLI仍各自既有编译身份。此前普通编排审查截至17c24810无阻断，仅只读，不能代替此组合审查。
下列 SHA 前缀用于查找原件，不是新的验证结果；
详细报告与旧失败产物保留原阶段身份，不改原报告。正式产物仍须使用完整 SHA-256 pin。

| 代号 | 固定文件与 SHA-256 前缀 |
| --- | --- |
| D | `remaining-stat-descriptions/final-a/producer-receipt.json`：`362843c98cc23736`；REPORT：`2e930dc59035bee6` |
| S／T | `remaining-csd/socketables-final-a/producer-receipt.json`：`ad03cd44b01f70a5`；`remaining-csd/ascendancy-tree-final-a/producer-receipt.json`：`7bfe24116d4520ac`；共同 REPORT：`6ba15ba3bd565155`；九项明细：`remaining-csd/remaining-socketables.json` |
| B／P | `remaining-base-overrides/REPORT.md`：`b79f67d16d49fa92`；`remaining-player-baselines/REPORT.md`：`c48d11d4c5601321`，后者 `actual-a/producer-receipt.json`：`4db46a309189014e` |
| K | `skill-overrides/REPORT.md`：`39dfe440af9827a9`；`skill-overrides/actual-a/data/0.5.5.5/producer-receipt.json`：`9e8dc9b411a0959c` |
| A | `actors/REPORT.md`：`8a4a3edd872a44ac`；`actors/minions-actual-a/data/0.5.5.5/producer-receipt.json`：`a42efc7e3008990b`；对应 `spectres-actual-a` 收据：`e4b7ce3565339d56` |
| U | `remaining-slot-skill-links/REPORT.md`：`109b5c1feffdfb4e`；`remaining-slot-skill-links/item-grants-a/producer-receipt.json`：`c301bbe71537a63f`；NoReservation 原解释报告 `remaining-item-grants/REPORT.md`：`422c15079d566d25` |
| L | `remaining-language-source/REPORT.md`：`bd9eaa3f9b1df5e9`；`remaining-language-source/consumer-final-a/receipt.json`：`50a90defb34a9af6` |
| I | 首批报告原文现作为V的`REPORT.md`尾部完整保留，该段SHA仍为`275707ccfe658588`，不是当前整份文件摘要；`source-oracle.json`：`7d62c053aa31a943`（明确非Rust执行）；首批patch身份`310fdf2dc53f4d21`已由V替代。RePoE actual-b manifest：`0dc9d41abc3dd21f`，payload：`178ab9f28cf5bd73`；源码head：`95f0dec69f30bcf707770c49c4ba7d39ac61fea0` |
| V | 最终`REPORT.md`：`de7355b07d3c29cd`；完整`integration.patch`：`cb3412db29a2ba02`；`policy-pins.json`：`5cc45596d7f73c2c`，`field-ownership.json`：`9120e16f2076014c`；actor observation：`010fddc4835e773c`，prepared oracle：`16496784c8909355`（均非Rust执行）。RePoE actual-c manifest：`f8642243fc1d04ce`，payload仍`178ab9f28cf5bd73`；MonsterCategories supplement：`c0f2a23f64a71ed4`；源码head：`73a631d8374acef9640cfdd3f8550325bfa9cdf2` |
| J | `REPORT.md`：`9ac3bad08eb10c2b`；`handoff.json`：`15c4f18d9d365995`；source-unlocks proof：`e039e06a9342f70b`，source-nonstat proof：`3dc32777a44a6e84`；8表supplement manifest：`54e72ff9f137275d`；源码head：`42bfbf9315a21ead8288b05cbeb31a46805641a3` |
| N | `REPORT.md`：`74100de12f1cdbba`；`handoff.json`：`f7edaa10f173c0b6`；`INTEGRATION.md`：`dafe8afff9b4afe3`；candidates manifest：`811c1b8314194fdf`；TW catalog manifest：`3f66561485638c36`，27top SC candidate：`012fe6c77b63b0c4`；翻译库head：`e006be4281b59e8137bd4d2fc979156b5527f771` |
| H | `REPORT.md`：`72244255fef252e6`；`handoff.json`：`f926ae915cde5ab5`；`source-proof.json`：`d9f19d7818030830`；新manifest：`7d15152a751000dc`，固定九文件candidate：`e0c00bb708ff50f0`，27top candidate：`f6f8ad1c0fc80b0a`；翻译库head：`94e36c127e8089d206bce83816aeed3b05d0995a`；N旧catalog／name及候选／报告完整保留 |
| R | `REPORT.md`：`94dc197a015381e5`；368项`closure.json`：`e6b24e6ec51b0250`；实际adapter：`12eb4d38cdeacda8`／clean `9481d61f`，`producer-replay.json`固定六域A/B全部51文件；描述receipt：`aeb2af553d3ca68f`，树receipt：`f681e34eb725dfd3`。新WASM：`674654efee7169f9`／clean `7c688522c0537ccb2f0e8307259290bd0b0cd290`，`wasm/build-receipt.json`：`19a0676237852441`；`package/bd/proof.json`绑定三例API及runtime；不转移历史f4身份 |
| E | head：`a2145a5360912fe64496d217a2ad2577d182ce6e`；`REPORT.md`：`1fa0b16649f29358`；14项`closure.json`：`9926ade15bfaf656`；侧表：`ff1ca6fd8f3addab`，receipt：`6897c730e08a6214`；`projection-proof.json`：`d38adaa7b53dd161`，`backend-proof.json`：`baa33e6d86811f8f`，临时consumer manifest：`c81b7008a94eac7d`。生产源码／七个运行模块摘要与固定head核同，WASM仍为R的`674654ef` |
| E2 | head：`ad830ae133e05a36fc99cf11f96176336fe0e5d0`；`REPORT.md`：`587d4ef718e6bb58`，8项closure：`9bc9e71efde26bad`；`blocker-proof.json`：`c80139700f6c0b52`，v1重放receipt：`271bd74b081c46c2`。RePoE canonical：`49852b2e79e0e9846df542896f8bb3f5177bc49c`，六blob同worker6927d80；Mac ingest：`104796ce61b4dc6f`，九英文target-lock：`3b0ccf519d722992`；无实际SC candidate／新WASM身份 |
| N2 | head：`590b6ecdd0cf312097e9cce0ed03824b2bb00945`，七blob同canonical1213517；`REPORT.md`：`7cb4e07b3428ab78`，`handoff.json`：`ead1695a1e29f8c1`，五模块`import-closure.json`：`36908d00c2880f99`；合成输入manifest：`90417fe20cd7070b`，输出catalog：`3ed4af8372a59974`／manifest：`525f4b3acff30b91`，`synthetic-proof.json`：`c15e32710159d186`，作者30-test log：`8fade12fe5e9d584`；root九表raw比较：`3e2a75f15d0fb9b8`。仅2个合成字段，不是实际SC目录 |
| E3 | head：`4fc0299f45e69957f23ab77980ec0ae349bdb54b`，tree：`f5b7cda8e877c0e3276f2ba4f65c755117f3a246`；`REPORT.md`：`971bd8f024221f0d`，12路径`identity.json`：`bc4346cd074eaf01`，9项closure：`176d9bcd72dec898`；`runtime-tw-b/proof.json`：`203a53b08d7cc834`绑定9个实际运行模块，consumer manifest与E的c81b7008字节相同，WASM仍compiled7c688／674654ef；初alias探针失败日志保留 |
| N3 | `REPORT.md`：`a1941b7798939027`，24项closure：`444d522ebdd31542`；Windows heads：RePoE1d047dba／翻译565a46be，本地source-only修复：1130d00f／c43c0d22；source manifest：`9ae5f1ee55174b82`，catalog manifest：`bbdf750ac33a2e9a`，侧表：`3d81de1f5ff7717e`，`projection-proof.json`：`74b645ebc5697f67`，`binding-gap-inventory.json`：`1a1ac4764cf04fb2`；实际backend proof：`a8b5e19c0482cfc2`，consumer manifest：`15c6229bc7ea9221`，PoBR代码aab0e62d／旧WASM身份分列 |
| E4 | head：`695656fbac61ffeac5f022d5a365331467837330`，tree：`e436e429f9fbdb15c8f030b6ec1792611b76ae9c`；`REPORT.md`：`104a965fa9bbee48`，15产物＋9源码closure：`6ff52b44c2b6a8ef`，`proof.json`：`3991e5538b58eef2`；TW／SC侧表：`ae273e6c2b1e8bd0`／`e017befeb7282715`；`backend/proof.json`：`c5dd7aaf8d0f4070`，`backend-comparison.json`：`2ee5e1e934d8264a`，consumer manifest：`018f163201b132e4`；两个脚本log保留，12项unit仅观察terminal结果，无独立raw log |
| N4 | head：`ef8b5e5bd6a3276f7e573ffe3942c0f2f1f1ec3a`，tree：`8a6f8998031adaebfe210d2f5325117a2ff09c54`；`REPORT.md`：`8169c6b6e86ea6c9`，30项closure：`44c6086b4f8a8aa5`，15源码identity：`b123a7c41f400fc8`；requirements／lock：`b0e1a6e6301257f9`／`ee4b80268dce34cc`；RePoE61fe85cc／翻译e61f1332及source／catalog manifests：`dc00160526a114c0`／`0ef819f9628ae634`；runtime：`381d827ab1345bf7`，receipt：`2d32813300b3f9b9`，replay proof：`39eccd7299b46f33`；backend proof／consumer manifest：`3bfde11a725a9d8c`／`48a854c499f9de50`，WASM仍R身份 |
| W | head：`0fa3db232146ddd543d80b68eca3e943bae52353`，tree：`cb248ef73e755bec0f75c23b82918efb7c845516`，parent c43c0d22；`REPORT.md`：`1cbe9078f7fe88d3`，16项closure：`5be914179b1ceaff`，`actual-proof.json`：`7168a0315d76b940`；final B manifest：`b17fa14b942d7c02`，worklist：`d5fea560510b01e8`，regional-candidates：`a82997da1a700988`，edits：`1d15a1031bd3b341`；17-test log：`10daa1ce280404cd`，非PoBR补译消费或采用证据 |

### 5.2 源码、候选、采用与玩家验收分别记账

当前状态以本节第一行及第5.1节为准。其余固定阶段行记录当时源码、输入与未跑范围，不把旧“未采用”重新列作当前待办。

| 固定阶段 | 源码与候选 | 实际消费／玩家证据 | 采用状态 |
| --- | --- | --- | --- |
| 初始外部导入／完整双语采用基线 | CI接线90f10562已集成为032c8205，root240a8a9修复两处旧数据测试依赖。独立game／language与PoBR十一份自有规则恢复67个清单文件＋manifest／18份shared：manifest `2a03ad79`／runtime `d7ac23c5`／bundle `a6075854`；SC紧凑release仍为另一身份9932ea87／ec4f3776 | root实际fresh远端Git恢复当前与旧版，默认CLI等同候选，Web同步86文件；真实浏览器导入Druid／Oracle、编辑等级／笔记、刷新恢复及打造“蓝玉”通过。Web757通过／8跳过，workflow通过，core buff18／工具buff3／实际多版本构筑2项通过。复用CLI `1cd9e614`（compiled `663b798f`）和WASM `674654ef`（compiled `7c688522`），未重编译WASM或重跑356条 | **本地已采用0.5.5.5，完整数据已迁出PoBR当前Git tree**；旧版由独立两owner锁恢复dcfc32c2。数据artifact已发布，PoBR218／219、翻译库5和RePoE7源码已合并；精确b8b980fa完整CI通过三Rust分片、Web／WASM／组包与automation，源仓各自检查通过。未部署；非全部浏览器、准确度、CN语义或许可认证 |
| 历史75b整包 | `75b4615b`／PR206；63 snapshot +18 shared，manifest `2fbe3ea72e9a167537be83c08a9818b64988ce72cae4db4083d284a83c40cb38`，runtime `3239739ec53903ed7f55b673da8e1993bac0b830d09c4eed3f5673f5cc6086f2` | 356导入／编辑计算／保存重开，355原PoB码计算；3类代表真实Web及旧包恢复。非全部 opaque 字段、浏览器或准确度认证 | 本地过渡验证；未采用 |
| PR215普通描述基线 | 作者 `8c856e86`，PR215已合并 `a5ef8dd4`；66 snapshot +18 shared，manifest `548ad7e63da1fd5a6ed978dbd03d758ae1c7ab475af339d94c905c0fc14b834b`，runtime `4cc89713bad0dc046abcf4297927604407a14c3407e997d9d55c5e93bd6239f9` | D的9761模板／compat核对及开发0、324普通API流程属于此前组合；本轮R以它作旧候选对照及恢复对象，不把原结果移用到新代码 | 历史基线保留；来源draft，CURRENT仍 `0.5.5.4.2` |
| FIELDS／TREE交接至本地集成 | V的`73a631d8`／25路径、J的`42bfbf93`／9路径已全部集成；I首批报告、b/c来源和完整patch替代关系保留。DOCS `a977fe1`已集成为 `b4f73166`，文档blob相同 | owner阶段的stdlib预测与J的59来源候选保持原身份；已写回归及真实producer现由R提供运行证据。7unbound、策展／规则／codec及未获源字段仍保留 | 本地集成完成；本批源码未发布，采用／完整玩家验收未完成 |
| R阶段集成候选／真实WASM | R：adapter clean `9481d61f`实际六域各两次，全部51文件A/B同字节；新66 snapshot +18 shared，manifest `2db649bf022084dc33701a8db33e82b9062174b7d0e0a8cc08c457ed78a28228`，runtime `c85dd98cf85e087bf34f7a45baaa03231f7a4d2265585bb1c970d74edb021f4c`；新WASM `674654ef`编译于clean `7c688522` | 聚焦Rust136项：描述8、tree/render53、socket12、base19、actor20、override15、tier9；tests-enabled Clippy初needless-borrow由9481最小修后通过，失败日志保留。OS禁网的新WASM实际开发0／324／352导入字段、计算、level编辑／保存重开均通过，0数值／诊断增减／物品错误；另以324恢复旧候选，tier在324／352分别标24／41行 | 本地候选及三例普通API通过；非新Web／全356／accuracy／holdout／来源批准／生产回滚验收。完整unique、CN及其它未拥有字段不消失；CURRENT不变 |
| 语言库初批历史交付 | N的`e006be4`／21路径与固定语言candidate保留；本批PoBR消费见R及下一行 | 作者Node26.13.0下32聚焦测试通过；当时更广62 case通过，1个suite缺cheerio无法加载。此阶段14原CSD测试与compiler／strictJson旧blob相同，历史失败不改；R的后继全100通过另记 | N的历史证据不是PoBR UI／完整CN／数值／来源准入或采用证明 |
| 语言implicit增量与本批消费 | H的`e006be4→94e36c1`／3路径及新CSD candidate保留；R的PoBR `7c688522`接相同编号契约与mixed中的CSD选择，原catalog／name不变 | H作者20项changed-CSD测试保留；R另有实际JS13项及翻译库canonical `98985ef`全100项通过，初e006缺cheerio的历史失败不改。三仓mise latest请求／精确锁独立；RePoE隔离Python3.15.2的继承9＋locale7及CLI smoke通过，旧decoder环境／历史证据不重标 | 已有本地selected语言消费，新增2 literal／2907保留或诊断；ClientStrings／九表语言UI／SC专名／CN数值及采用仍未完成，CURRENT不变 |
| 显示目录投影／backend消费及v2准备 | E的`a2145a53`之后，E2 `ad830ae1`修复候选漏阻断并接v2闭包；N2翻译实现590b6ec已集成canonical1213517。E的内存consumer manifest仍为 `c81b7008a94eac7dbfbcecb673dece24e894f7423c36bbef8b5a52727173643b`，原66＋18封存组合不改 | E真实backend＋WASM15译文／12回退及E2 Node10／合成3阻断＋1译文保持各自身份。N2实际Node26.13下30项通过；合成90417fe2输入的A/B三文件同，只查2 fixture字段，observed身份null；root另报30项不叠加覆盖 | 本行合成及TW证据不改身份；N3另已接收真实SC并消费。其它scoped说明／UI、最终组合及采用仍待，N2本身不提供PoBR产品证明 |
| 普通scoped名称TW API | E3 `9a114b7f→4fc0299f`／12路径本地提交；同一E TW数据与consumer manifest，TS代码身份单列，旧WASM及封存包不重标 | Web23项／tsc及全1073 gem、12职业、23升华scope核对通过；非name字段、英文与原WASM目录保持。3个共享secondary alias仍沿原首owner，首次探针误断言单owner失败，仅修脚本后通过，产品规则未改 | 本行是TW固定证据，实际SC另见N3；名称API及选择器／摘要／技能集已接线，浏览器／数学／新BD未验 |
| 真实SC Git实物／compact双语言API | N3已接source9ae5f1ee／catalog bbdf750a并集成source-only修复，其单次SC投影及内存manifest `15c6229bc7ea92214b02e8d7b19caa940142d12db583c627a8af6990a678d0a5`不重标。E4只改producer／test／topic doc三路径，compact候选另绑定manifest `018f163201b132e41135dc67811da4f3807675db29b0cc88504093c268796fdc`；WASM仍compiled7c688／674654ef | N3独立全投影／Mac13项及Windows作者5＋44项／1平台skip保持原身份。E4 Node26.13下12项focused通过（terminal观察，无另存unit log）；真实两投影的全部literal／三scope相同，实际当时TS＋旧真实WASM在OS禁网下所有原结果数组相同：TW1053／20、SC0／1073 gem源译／阻断，SC12职业英文／23升华源译，各1073 gem／12职业／23升华、15 TW／11 SC literal字段及各24 null回退 | 本行是strict-v2固定阶段，后继N4已收到并消费v3；不重标旧产物。共享运行pack、完整SC覆盖、Browser、计算／新BD、来源批准及最终封存／回滚／采用未完成，CURRENT不变 |
| 声明式Git导入／真实SC FK v3 | N4本地15路径已实现requirements／独立Git lock／importer／CI候选与外部完整root hook。成功Git与禁网fresh replay的RePoE22＋catalog4＋library6文件及runtime同字节，receipts路径差异保留；新SC1649082B／381d827a，内存consumer manifest `48a854c499f9de50d2d2d93f9c04679747a49620a6bf0a379f0abf1a9194cbf2`，仍用原compiled7c688／674654ef WASM | workflow批次通过，后续Git／兼容及最终pin-shape guard由Python8／Node15最终聚焦测试覆盖；旧v1 TW／v2 SC runtime同字节。已有只读Astra xhigh无blocking findings，末次guard在其后由最终8项覆盖。实际禁网TS backend两locale各1073 gem＝1053源译／20英文阻断、0未归属，12职业／23升华路由、15 TW／11 SC literal及各24 null回退通过；非name字段／英文／原WASM目录不变，初fixture／TW兼容／TLS失败保留 | CI接线仅本地验证，远端CI未跑；只上传显示runtime＋receipts，站点仍旧完整快照。非Browser／数学／新BD或来源批准；核心／CSD／规则／兼容组包、agent补译runtime、最终回滚与CURRENT采用仍待 |
| 独立补译／区服工作台 | W的0fa3db2／8路径本地提交，原官方catalog／五模块／query不改。实际B从固定TW／SC catalog形成117160字段、90 eligible与16 target-missing字段观测；A只用于历史，输入／规则不同，不作为同输入重放 | Node26.13的17项（9新＋8既有）及真实离线CLI／list通过；2条Freezing Pulse WebsiteDescription分别为zh-CN／zh-TW草稿，effective均null，已提交可读views与final B匹配 | 官方优先和精确版本／区服审核边界已实现；草稿不是机制认证，区域候选不等于国服专属。PoBR未接补译API，无agent译文产品消费／浏览器或采用证明 |

“39兼容文件”是75b阶段的文件分类，不能作为现行剩余任务数；24个领域输出内部当时也有保留字段。
“64符文 fallback”同属早期阶段，现行为9项。后续新候选按字段收据重新绑定，不改这些历史统计。
此前各阶段的冻结f4 WASM仍编译于 `7abfe3af`；R另建clean `7c688522`的新WASM并绑定本批API证据，未给f4重标身份。
完成本地提交、协调者集成、源码合并、生成候选、活动采用和玩家验收必须分别确认，不能仅据 owner 的完成消息推进状态。

迁移清单还必须包含共享 `overlay-common/`：自有规则、buff、局部词缀、物品授予技能、图腾关联等
不全在版本目录的域清单中，不能因盘点时遗漏而丢失现有能力。

词缀 family 的入库字段为 `families: Option<Vec<String>>`：缺列或 null 保留为未知并省略序列化，
显式空数组保留为 `[]`；旧运行时快照继续读取。原始外键按 `ModFamily._index` 连接到 `Id`，
保留列表顺序，不把表内位置或 `ModType.Name` 当作 family 身份。实际引用缺表、悬空或指向空 ID 时，
adapter 报出词缀 ID、来源字段和目标；不因暂时没有计算读取方而丢弃关系或补造空列表。
导出配置已补齐 `ModFamily.Id`，必需列检查覆盖 `Mods.Families` 与目标表的 `_index`/`Id`；
旧 raw 导出没有实际引用时可省略目标表，缺列仍进入现有 drift/strict-columns 流程。
该切片没有引入 family 互斥、计算或物品选择行为，也未刷新活动包。
字段依据为本地核对的[固定 schema](https://github.com/ackness/repoe/blob/64e894e2963707dc93bcc383bb57651bd200f444/schema/poe2.json)
（`Mods.Families` 引用 `ModFamily`，其键为 `Id`）；
[RePoE 的导出](https://github.com/ackness/repoe/blob/64e894e2963707dc93bcc383bb57651bd200f444/RePoE/parser/poe2/mods.py)
也将 family ID 列表与 mod type 分开。这里没有认证该 schema 与当前活动补丁的客户端解码。

完整JSON经Git交付，线程仅传head／path／hash；显示投影与工作台已取消固定字节上限（PoBR d0261f8e／翻译1bc54fed），仍核manifest声明bytes、stat／read实长及SHA；134217729B容量夹具不算v3真实目录或全source重放，原aab／0fa运行身份保留。

## 6. 筛选原则：减少交付负担，保留完整事实

“全”的目标是目标游戏版本面向玩家 BD 构建所需的完整定义、数值和关联，不是把客户端全部素材送入浏览器。
收录范围不能由 PoB/RePoE 是否实现，或当前代码有没有 reader 单独决定。

1. 原始证据保留，不因产品暂时不用就改写源文件。
2. 按公开的产品范围选择目录，再保留全部依赖闭包；怪物/物品授予技能、隐藏 stat 和附加分支即使不直接展示，也可能是计算依赖。
3. 在默认运行包中排除确定无关的游戏、未选语言/素材及内部占位；每条排除有理由，发布/未发布状态需要证据，不能只信上游手工列表。
4. 玩家可遇到而引擎尚不支持的定义保留身份和诊断，不能为缩小包或提高覆盖率而静默删除。
5. 历史构筑需要独立历史身份或明确转换；默认包可按需加载历史数据，不拿当前版本悄悄代算。
6. 筛选结果有可审阅清单；定义数量和 stat/关系丢失分别检查，数量相同不能证明完整。

## 7. stat 修复与数值正确性

推荐修复的是**读取全部实际 stat 槽、正确连接外键并保留条件**，不是为已发现节点手填数值。
固定版本 RePoE 的天赋导出只读取前四个值；本地按匹配客户端表检查，确认有后续槽遗漏。
节点 `31955` 的第五项格挡恢复生命，原始表、官方树和当前 PoBR 均有依据；该缺陷在 RePoE 导出，不能误报为当前 PoBR 也缺值。
证据包括[固定生成器](https://github.com/repoe-fork/repoe/blob/aa9fa7c394b18a7a89f913f86703112b3f9d20f1/RePoE/parser/poe2/passives.py)、
[同补丁原始表](https://github.com/repoe-fork/dat-export/blob/50fc9064269439013aea9cbd36ba59475882570f/current/poe2/heuristics/csv/PassiveSkills.csv)
和 [GGG 树](https://github.com/grindinggear/poe2-skilltree-export/blob/bd87e6512c92b868542eddfb1ba4ea8b6dc2da36/data.json)。
GGG 树精确客户端补丁仍未知，故这里只用于该项旁证，不作整包等价认证。

自有 RePoE 已改为从实际 Stats 列表和对应 StatNValue 读取；多槽、负值、零值、空列表、缺值和重复 ID 均有回归检查。
原生 schema reader 另对悬空外键报错；真实客户端、图成员和其他元数据仍需独立验收。
若数值在客户端没有公开，保留“未知/策展/假设”状态，并附游戏证据，不能写成自动提取得到。

当前已有的 Druid 两条固定 ID 隔离属于**未决诊断候选策略**，不代表已恢复真实节点。
它既不能扩大成通用忽略，也不能替代原始图成员资格核验。

## 8. 持续更新与发布流程

每次游戏版本、导出器/schema 修正或翻译变化都可触发候选，不只检查游戏版本号。

1. **发现并固定**：发现补丁后只解析一次目标来源；固定服别、资源、schema、RePoE/PyPoE、补充内容、所用 API 原始响应和翻译版本。第三方版本发现服务只提供线索。
2. **取得证据**：下载实际需要的官方资源或读取本地客户端，校验并归档；缓存损坏/目标资源不可得时失败，保留现有生产包。
3. **导出与规范化**：用自有 RePoE 固定修订生成事实数据，再由 PoBR adapter 形成领域包。记录全部输入、生产者和转换摘要。
4. **合入补充**：规则、策展与翻译各走固定来源，记录冲突及处理依据；旧版本人工值不能无说明改标签沿用。
5. **审阅差异**：检查新增、删除、改值、条件、关联、排除范围和翻译变化；同一游戏版本的数据修正也审计这些差异。
6. **验证候选**：执行本节之后的五项验收；PoB2 保留为回归与交叉对照，有意差异以游戏依据解释。
7. **发布并采用**：为数据产物生成不可变修订。PoBR 显式更新来源锁、完成消费者验证后采用；保留上一套可回滚身份。

三个仓库独立发布，但 PoBR 的来源锁绑定一次实际验证过的组合。
翻译未变化可复用旧修订并检查覆盖，不要求为每个游戏补丁制造空翻译提交。
数据修复也不要求同步升级 Rust 引擎，除非 schema/语义或消费者行为确实改变。

现有[ordinary导入](../pipeline/runtime/ordinary/ordinary-inputs.md)、[显示导入](../pipeline/imports/data-imports.md)和[构建prepare](../pipeline/runtime/build-data.md)
已分别接收固定目标、数据／翻译修订与在线／离线模式，形成认证输入、显示收据或完整运行组合。
来源发现、各域再生、差异报告和生产采用尚未收敛为一个自动入口，仍是独立动作。
描述主体仅消费客户端资源；需要交易关联时，导出模块读取事先固定的 API 快照，不能旁路联网取得当前目录。
离线模式缺少该快照时，明确拒绝依赖它的补充步骤，或按预先选择的纯客户端范围生成候选；不能静默减少承诺的产物范围。
定期检查可以自动生成候选，失败时保留上一套已验证数据；不能自动追踪上游 HEAD 并直接切换活动版本。
发布产物使用固定摘要校验，远端同名资产发生变化也拒绝使用；本地调试模式的未提交内容不能冒充正式来源锁。

### 8.1 发布与回滚的完整单位

最终发布身份封存**版本数据、实际使用的共享规则、翻译、兼容映射**的固定字节，并声明领域 schema 及消费者兼容要求。
源码可以继续维护共享目录，发行包必须携带或固定引用可取回的对应内容，不能在加载时读仓库当前 `overlay-common/` 代替已验证版本。
当前 `GameData::identity()` 标识运行时字节；新增的[组合封存入口](../pipeline/runtime/runtime-bundle.md)
将版本文件、共享规则、翻译与兼容表保存在独立目录中，收据固定完整清单及默认运行时身份。
它已通过真实构筑的 Rust CLI 等价读取验证；封存字节不替代新数据的来源、机制和发布验收。

用户 `patch/` 属于单独的本地覆盖身份，不改写官方发行包；计算 provenance 同时识别发行组合、有效 patch 和引擎/构筑条件。
回滚恢复完整发行组合并核验身份，不能只回退版本目录或 `CURRENT`。patch 在回滚后重新校验；不兼容时拒绝该组合并保留原文件，不能静默删除或套用。
现有加载与身份行为见[构筑模型](build-model.md#数据快照身份与会话)；版本 manifest 仍是 schema 3，
封存包在其外固定共享层，用户patch继续单独管理。本地0.5.5.5采用已完成，当前记录完整runtime `dba0615e`与封存bundle `afc78a77`；原2a03ad79／d7ac23c5／a6075854组合及其既有验收证据仍按原身份保留。
当前锁和[旧版锁](../pipeline/runtime/build-data-0.5.5.4.2.lock.json)分别从两个数据owner恢复完整组合；root已fresh取回旧版runtime `dcfc32c2`／bundle `6768c476`，并用`--keep-current`实际安装且保持CURRENT为0.5.5.5。
原缓存切换演练的正常回退及替换失败恢复保留原证据。引擎／WASM未重建，不能把本地演练扩称远端生产组合回滚；不兼容用户patch及部署组合仍须分别验收。

## 9. 上游更新与停更时的维护责任

上游是可同步的来源，我们的 fork 是可独立维护的生产者。通用导出修复优先保持小范围、便于回馈上游；
未合入的修复留在自己的修订中，升级时检查是否仍需要，避免重复覆盖上游新行为。
不能以某个导出缺陷单独断定上游已经停更，也不能把持续维护押在上游承诺上。

独立维护还包括 PyPoE、schema 和原始资源可用性，fork RePoE 本身不足以自动解决这些依赖。
先固定可用版本；有实际格式变化再维护窄补丁或评估替代读取模块，不预先 fork 整个生态。
原始 stat 的含义与 Rust 规则仍由 PoBR 负责，导出器不能替代游戏机制研究。

RePoE 代码按其许可维护；其游戏数据/素材权利属于 GGG，非商业用途不自动消除使用限制。
PyPoE 的许可需保持独立工具边界并在相关分发时核实，不将其实现直接复制进 Rust 核心。
原始大文件归档和公开产物的可分发范围分别记录。

## 10. 五项质量目标与验收

| 目标 | 验收依据 | 不能代替验收的信号 |
| --- | --- | --- |
| 全 | 目标补丁的定义清单、所有 stat 槽/分支、关联闭包、历史/服别边界；排除项有出处，未支持项保留诊断 | 文件存在、条目数量相近、PoB 已收录 |
| 准确 | 固定原始字节和 schema；关键字段独立核对；转换、单位、作用域和受影响计算有证据 | hash 一致、同源网站都相同、只与 PoB 一致 |
| 可用 | 数据加载后可搜索/选择/编辑/计算/保存/重开；CLI/WASM/Web 保持契约；真实输入的损失可见 | exporter 运行成功、目录可浏览 |
| 规范 | 一套领域 schema、稳定 ID、单位与空值语义、明确 provenance/版本/服别、兼容迁移 | 换目录名、把 Lua 改成 JSON |
| 高效 | 测量发行包体积、冷加载、峰值内存、首次/非缓存计算和批量换装；按域加载、构建期索引、避免运行时文本反解 | 只测热缓存、只引用 Rust 语言性能 |

数值与性能容差按实际字段、显示舍入和目标设备制定，并记录可重跑基线；当前没有证据支持固定准确率百分比或随意承诺毫秒目标。
先测当前包作为基线，实施前为目标设备确定可接受预算；后续比较同输入、同环境，冷/热路径分别报告。
影响受支持输出的来源/数值疑点阻止对应候选激活；范围外信息保留明确状态，不为过门禁假造模型或降低基线。
目录收录、字段转换和计算支持分别验收：可以保留尚未支持的真实定义，但不能把它标为已正确计算。

## 11. 分批迁移及退出条件

以下为长期领域路线；当前实施优先级和近期产品验收以第 13.1 节为准，不要求全部领域独立后才交付可用版本。

| 批次 | 设计/实施范围 | 可验收结果 |
| --- | --- | --- |
| A：来源与契约 | 完整盘点当前版本域、共享规则、翻译与兼容；确定来源锁、领域身份、筛选清单；首个域替换前建立性能基线/预算 | 每个现有能力都有明确归属与来源缺口；用一个树节点和一个多分支技能固定最小领域契约及单位/条件边界 |
| B：天赋试点 | 自有 RePoE 全 stat/元数据修正；原始表/PSG、官方树并行核对；保留历史与替换身份 | 原始 stat/条件不截断，几何/拓扑和珠宝消费者通过；未决 Druid 映射有真实处理依据 |
| C：技能与物品 | 形态、品质 scope、基底继承/固有值、词缀 family/条件、augment 数值 | 逐域替代旧生产者，编辑到计算的实际流程验证，不扩大永久 overlay 拼接 |
| S：语义与规则（随各域推进） | 描述、自有 stat→modifier 映射、文本解析、配置目录及 trigger/mirage 等语义 | 按游戏证据扩展现有自有规则路径；相应 parser/stat-map/config/special 产物可以自主生成和验证，才能退出旧 vendor 生产路径 |
| D：actor 与策展 | 召唤关联、常量、敌人场景、暗金完整词条/变体 | 事实与模型分离，证据缺口明确；现有支持保持，新增覆盖按真实需求验收 |
| E：双服与持续维护 | 国服来源、翻译覆盖、服别差异、独立数据修订和性能复测 | 固定来源可重放；一次真实版本更新及完整组合回滚演练；双服事实与语言边界可解释 |

同一域通过验收后才退出旧写入路径；历史导入和 PoB2 参考继续保留。
上述是依赖顺序，不表示一次性并行重写；每批保持可运行，不把源格式迁移与大量机制变化混在一起。

S 属于 PoBR 的语义责任，RePoE 只提供事实和描述，不把 PoB Lua 逻辑转写后当成独立游戏依据。
当前 `mod_parser_rules`、`special_vendor`、`skill_stat_map`、`config_options` 等都要在清单中逐项归属；
按[现有依赖与退出顺序](pob2-differences.md)迁移，不另建通用解释器，也不立即移除仍需保留的 vendor。
每域标明“自主生产”“固定社区输入、依赖仍存在”“仅参考/兼容”之一；保留固定外部输入是明确的阶段状态，不能据此宣布该域已自主更新。
只有目标生产范围再生不再读取/执行 vendor 时，才能宣称相应生产依赖已经退出；PoB2 oracle 与历史兼容仍可保留。

## 12. 当前证据、限制与待决事项

普通完整路径的现状见第5节；下文保留原生事实／受限 profile 等各自固定阶段的能力与边界，
不将早期阶段的未跑项重新列为当前普通路径待办，也不将其证据升级到后继组合。

当前可复用：版本化快照、manifest 字节校验、GameData 加载与身份、诊断、独立翻译 pin、PoB2 oracle。
活动数据的文件完整性已检查；部分关键源字段已作固定版本比对。这些证据尚不足以认证整库和游戏计算。
本地活动包已升级为`0.5.5.5`，完整双语身份与采用证据见第5节；线上发布未改变。旧`0.5.5.4.2`已有独立固定数据包可恢复，其完整客户端再生和PSG来源仍未闭合。

自有 RePoE 的开发流程已迁到 mise + uv + Ruff，固定 schema 的 1,286 张原生表通过结构验证；
这不是 1,286 张真实表均已解码的结论。国际服 `0.5.5.5` 的 77 张原始 DAT 和 Default PSG 已固定候选来源字节，
可从归档压缩块离线重建，并形成保留物理字段、未知信息和关系诊断的 RePoE 部分事实候选。
指定实物已独立核对物理字段和全部图实例，保留 9,731 个定义、5,152 个实例、5,957 项图 stat 和 61 条职业替换。
其中 Druid 的两条基础定义不属于该 Default 图，定义本身仍存在；这不证明历史删除或旧补丁等价。
Snipe 品质作用域、物理半径、其他图和未获取关联仍有明确诊断。生产者工具身份、畸形 DAT 校验和图实例验收已加固并独立复核；
当前消费入口固定该生产者及完整 decoder 源码身份，不沿用旧候选的工具收据。

PoBR 的[原生事实消费入口](../tools/pobr-data-adapter/native-facts.md)已接通外部摘要校验和领域候选转换，
完整原始元数据继续引用固定的 RePoE 包，诊断原字节保留。该中间候选没有运行时 manifest，不能直接激活。
同补丁的label、ActiveSkills、费用和逐级七张表曾分批采集并离线重放；后继采集接受以第5节及来源锁的精确资源清单为准，不沿用早期“尚未审阅”状态。
这批事实导出及物理字段增量复核已完成，PoBR 通过显式 v2 选项生成包含标签、成本和逐级原始通道的领域候选。
宝石/辅助约束另有 24 张表完成原始采集、物理解码和离线重放；显式 v3 事实及消费增量已实现并通过独立审查。
该候选保留物品身份、多效果、变体、辅助家族、经验行和有序约束，不解释可用目录或辅助适配语义。
显式 v4 已接通装备与 actor 事实，生产者和消费端增量均通过独立审查：保留 16,784 行词缀的全部八槽、
5,496 行物品基底及组件关系、313 行魂核、121 行常量、100 行怪物等级表及召唤关联。
87 个输入 payload 的完整性、原始引用、两进程确定性和旧候选兼容均有实物验证；仍不解释继承、单位、目录可用性或召唤身份。
物品的 82 个继承入口及七个父文件已完成固定候选采集和离线重放；完整字面闭包不代表已验证继承合并与词缀适用规则。
选定技能描述的九个 CSD 已补齐字面 include 闭包，包含原始简体中文文本；来源批准与分支选择、渲染和翻译消费分别验收。
每批仍需核对实际物理宽度、已获取关联，并单独验证单位和应用语义。
PoBR 已有原生分支 label 读取，vendor 仍用于旧包标签回退和构筑码编号兼容；生产者、完整运行时消费和独立规则尚未全部迁完。
原生天赋 stat 已有一个受控的完整 Rust 计算路径，验证了无 parser catalog 的火抗计算、来源诊断与效果增幅。
其[版本／上下文／加载与 fallback 边界](build-model.md#可选-native-天赋-stat)已明确；新构建 WASM 的公开 API
已验证该受控包的原始值变更、单次应用、语义回退、畸形包拒绝及旧活动包兼容。
原生包已有独立的[受约束组包入口](../tools/pobr-data-adapter/native-runtime.md)：从固定事实投影完整树图和选定 Spark 分支，
生成带来源索引、覆盖说明及必需 native 契约的 schema-4 包，并通过现有领域加载器检查。
通用树描述、职业替换等尚未全部投影，普通 `BuildData` 的原生全量计算准入保持关闭。
显式 v2 契约已加入固定角色事实及独立的 actor 适用政策；
[原生构筑文档](native-build-document.md)保存完整 Build，并绑定精确数据集、角色、技能分支和政策身份。
[原生 CLI](native-cli.md)重开文档后重新准入，通过共享 Rust 核心计算
[受限条件命中](native-skill-hit.md)。真实固定包已验证保存／重开、输入变化传播、错误输入拒绝和来源保留。
原有配置继续限定指定一级角色和一级零品质 Spark，无装备、无辅助、无加点；角色通用 OT 的适用仍是显式模型假设。
新增显式伤害天赋 profile 可选择普通、无条件、未变换的单 stat 伤害 INC 节点，沿用同一共享计算路径；
已验证增删、逆序选择、保存重开及真实 CLI/WASM 完整结果一致。来源物理行身份冲突、重复或不支持的混合选择会整体拒绝，
合法来源引用换序仍保留原始证据顺序。选择只表示条件比较中假定这些节点有效，点数、连通性和角色进度合法性尚未认证。
新增通用主法术 profile 复用同一领域类型、stat 映射及 Rust 条件命中核心，支持显式选择 Spark、Fireball、Frostbolt
及源表唯一配对的精确宝石等级，初始实物包含三技能各 40 个等级。等级来自源表，不代表角色等级、需求或可获得性认证。
保存重开、普通伤害 INC 天赋、旧 profile 输出及实际 CLI/WASM 完整结果均已验证，并通过独立阶段审查。
主投射物与爆炸等附加分支分开；缺少证据的投射物数量或寿命明确返回不可用，不从通用默认值补造。
该 profile 仍限定角色 7／等级 1、品质 0、无装备或辅助的受限条件命中，不表示原生完整构筑准入已开放。
受限[原生 WASM 接口](native-wasm.md)已验证同一文档的重新准入、完整结果与 CLI 一致，
以及普通/原生模式切换、缓存清理、失败初始化保留状态和普通翻译接口拒绝。
这不是 Web 编辑流程验收。资源初始化、总命中率、需求、持续施放、完整 DPS、原生 Web 流程及国服数值尚未完成。
该受限profile的运行证据不提供额外来源批准；当前精确来源决定及普通活动快照以第5节为准。

现有真实 WASM 已建立固定产物身份下的预热计算观测；[测量入口](../web/scripts/bench-calc.mjs)另记录
文件读取与校验、初始化各阶段、首次计算及进程峰值 RSS。首次响应从 Node 脚本内开始计时，
文件系统缓存不受控；这不是浏览器冷加载、WASM 单体内存、准确率或性能 SLA 的认证。
翻译库现有测试及原始响应摘要/离线再生校验通过，PoBR 本地读取的模板摘要与当前活动包一致。
当前翻译快照仍有未解决词条、冲突和来源诊断；通过结构校验不能宣称翻译或国服支持已经全覆盖。

Windows WeGame 客户端的一个 Spark CSD 已与固定国际服资源逐字节相同；
该项只认证这个描述文件，不能推定客户端补丁、全包同一性或国服数值相等。

仍需在实施前后分别确认：公开数据产物的分发范围；国服原始资源的可持续来源；新领域 schema 的向后兼容方式；
目标设备和性能预算；暗金完整策展的长期证据来源。它们是具体设计/来源决策，不靠默认值掩盖。

阶段集成后按风险统一独立审查，线程和模型遵循当次用户授权；不为每个 helper 或小修另开审计。
小修由负责人核对证据并运行聚焦回归；验证预算遵循[开发工作流](development-workflow.md)。
审查通过不冒充来源批准、完整游戏正确性或全部产品验收。

## 13. 原始目标、阶段边界与持续核对

产品目标始终是面向玩家、以 Rust 高效计算、兼容国际服与国服的 BD 构建工具。
数据、模型和验证应能自主维护；脚本语言按任务选择，vendor 中固定的 PoB2 继续用于对照、回归和格式兼容。
独立性不等于删除所有 Lua，也不等于把 PoB2 规则冻结成 JSON。

| 原始要求 | 长期验收点 | 当前阶段不能代替的部分 |
| --- | --- | --- |
| 完整事实与关系，筛选后仍保留未支持身份 | 面向目标补丁的 BD 目录、关系/作用域及依赖闭包 | 表数、字段数、已支持一个技能 |
| 官方字节和可持续自主更新 | 固定工具/schema/原始输入，离线重放和一次真实升版 | 自洽 hash 或一次导出成功 |
| 单一消费入口和自主语义 | RePoE → PoBR 领域 ID → Rust → CLI/WASM/Web；逐域退出 vendor 生产依赖 | 同一 Spark 结果或跨端一致性 |
| 安全采用与回滚 | 目标补丁验收、不可变组合发布、完整回滚和用户 patch 单独验证 | 合并源码、绿色单项 gate |
| 真实可用、双服、性能 | 选择/编辑/计算/保存/重开流程、独立游戏依据、服别对照、目标设备测量 | 有中文、微基准、仅预热或进程结束内存 |

Spark 等受限场景是共享数据与计算路径的验证样本，不是最终支持范围；不得另建按技能硬编码的平行引擎。
后续增量应说明改善哪个实际 BD 操作/输出，并区分事实已获取、语义已实现、构筑已准入与玩家流程已验收。
例如伤害天赋可计算，不代表点数、连通性或角色进度的加点合法性已经认证。
暂不模拟的事件收益仍保留诊断，不为清空缺口表而扩大为无需求的时间线模拟。

### 13.1 当前优先级：自主数据接回既有完整 BD 流程

当前先交付可持续的数据源迁移：自主获取和维护数据，并灵活适配目标游戏版本。
保留现有构筑码导入、领域类型、GameData/BuildData、stat 映射、计算核心及玩家操作流程；
以领域 adapter 消除原始来源格式差异。仅当实际字段、单位、作用域或行为冲突证明必要时修改消费者。
每项算法或核心架构变更须说明具体错误、受影响构筑、既有路径为何不足和聚焦回归；
单个技能的精细机制、新模拟模型和非迁移必需的规则完善不阻塞本阶段。

第 12 节的受限原生 profile 是保留的验证工具，不是替代普通构筑导入的产品入口。
不继续逐技能扩展该入口来替代完整 BD 迁移，也不通过取消其守卫把不完整候选当完整运行包。
已有来源、可重放产物、稳定 ID、通用映射和审计修复继续复用，不推倒重做。

自主领域生产者已接入本地完整运行包及BuildData，0.5.5.5已本地采用；独立数据存储、临时构建导入及最小旧版恢复已接通。当前按第5.1节继续真实字段／语义缺口及远端发布验收，沿用普通CLI/WASM/Web路径。
核对目标补丁及运行包各域的现状后，逐域替换生产者；尚未替换的固定社区输入/兼容规则可作为明确的过渡依赖。
其来源版本和适用性单列，不能因混合来源便宣称完全自主，也不能把跨补丁旧值重新贴成新补丁事实。
同补丁完整运行包不足时，分别交付已完成的适配接线及精确来源缺口；不以受控样本成功冒充实际采用，
也不无限扩展旁支研究来绕过这个缺口。优先可取得且能一致验证的目标版本。

本阶段验收使用已收集的真实 BD 码和现有 benchmark/tuned/examples 语料：

- 先固定代码、包和语料清单，记录现有完整路径基线；覆盖已有技能、辅助组合、装备、天赋和配置，不只选 Spark。
- 将全部可用的开发语料列入矩阵，明确实际执行数、跳过数及原因；大批验证统一安排，开发中使用有代表性的聚焦集。
  Holdout 只按既有 accuracy 流程测量，不用于诊断、挑选修复或调整规则。
- 分别记录解码/导入成功、装备/技能与辅助/天赋/配置保留、可计算、未支持诊断、保存导出重开结果。
  能返回数字不等于完整支持；不能静默丢掉未映射数据或把未知值归零以提高通过率。
- 同补丁同语义比较迁移前后结果；新补丁的游戏改值、旧参考差异与迁移缺陷分开报告，不追求错误的数值全等。
- 用普通 Web 入口完成真实导入、编辑、计算、保存及重开，并检查旧包和回滚路径；源码合并不自动切换 CURRENT。

用户提出的“约 80% 体验即可进入后续优化”是阶段取舍方向，不是允许数值误差 20%，也不是已测得的覆盖率。
进度先报告上述能力的分母、实测结果和阻塞项；未建立覆盖矩阵前不再用一个主观总百分比代表迁移完成度。
完整来源独立、双服数值认证和剩余机制仍是长期目标，各自验收，不能全部变成近期可用版本的前置重写任务。

### 13.2 协调与交接

长任务在阶段开始、交接及发布前，重读本设计与所改领域契约，对照最初需求和最新用户决定；
发现冲突时区分实际实现、设计目标和临时限制，并修正文档。已被取代的状态直接更新，不累积互相矛盾的结论。
新线程收到固定 base/head、相关仓库与负责人、输入身份、验收范围和未闭环问题；缺少早期上下文时先询问拥有者。
并行写入保持隔离，跨仓契约修改告知直接消费者；长期状态记录用于定位证据，不替代代码、测试或来源实物。

已通过验收的阶段独立提交 PR，处理问题后逐步合并，不等待整个迁移形成一个大 PR。
尚未验收的后续阶段留在独立分支；合并不自动启用候选。完成交接且没有待办的线程可归档/settled，
但固定 Git 引用与后续仍依赖的证据先保留。远程资源按独立任务边界使用，不能以增加线程数替代串行集成责任。
