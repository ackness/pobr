# 数据现状

最后核对：2026-11-05 UTC。本页原地更新：状态变化时直接改写，不追加历史。
长期设计与规则见[数据架构](data-architecture.md)，命令见[数据管线](../pipeline/README.md)。

## 当前数据锁

- 活动目标为 `0.5.5.5`／international。消费产物的来源与 SHA-256、
  期望 manifest、运行身份与 CI 候选包身份都以[数据锁](../pipeline/data.lock.json)为准。
  两个 owner 包都来自 Release：游戏 `ackness/repoe` `game/0.5.5.5-2`，语言 `ackness/poe2-zh-cn-stats`
  `locale/0.5.5.5-1`，均由 `data:bump` 采用。`support_display` 与 `test_fixtures` 仍按 `handoff/*` tag + commit 拉取。
- 待发布：本分支的锁按本地模拟 Release 采用 `game/0.5.5.5-2`（包 `8938b99a…`，生产提交 `58a044fb…`）。
  正式采用须先合并契约 1.1 并打 `contract/1.1`，RePoE 改依赖该 tag 后发布，再对真实 Release 重跑 `data:bump`。
- `game/0.5.5.5-2` 是一次有意的游戏数据批次，计算结果不变，只改数据身份：
  新增 `base/active_skill_types.json`（290 行 `ActiveSkillType`，替代引擎的技能类型 sidecar）；
  `base/game_constants.json` 增加 `base_maximum_rage`（30，替代引擎常量）；
  `base/passive_tree.json` 为 4913 个节点中的 2106 个携带原生 stat packet（文件约 1.96 MB → 4.26 MB）；
  snapshot manifest 声明 `contract_version: 1`，并不再列出 `i18n/**` 与 `languages`。
- 运行 manifest 按 owner 组合：游戏 snapshot 只列游戏成员，`data:prepare` 加入语言包的 `i18n/**` 摘要，
  并由 `i18n/<语言>/` 目录推出 `languages`。只换语言发布不再需要新的游戏发布；
  仍列出语言成员的旧游戏 snapshot（`game/0.5.5.5-1` 及更早）被明确拒绝。锁的 `locale` 可为任意语言 tag，须由语言包提供。
- 加载器拒绝 schema 3 但未声明 `contract_version` 的 manifest，契约前例外已删除。
- RePoE 发布序号 7，`game` 角色序号 6：补回 14 个真实宝石的品质效果，
  按原生宝石关系验证五个 DNT 占位排除项。其他角色保持原有输入，翻译库为序号 3。
- 当前 runtime 为 `0ea72e06…`（此前 `e73febc0…`），release 为 `eda2e570…`，snapshot manifest 为 `54c7595a…`（固定测试绑定组合 manifest 的游戏部分 `e9804a9b…`，只换语言发布无需重新绑定）。
  Web 与独立 WASM 继续使用逐文件快照；紧凑候选包用于 CI 验证。
- 固定测试已在复核游戏成员差异后绑定新 manifest，召唤物和解析器的数值断言保持原值。
  数据采用需要同时检查这类显式测试绑定；必需的 `PR gate` 只证明快速检查通过。

品质发布策略在 [RePoE #29](https://github.com/ackness/repoe/pull/29)，
发布记录在 [RePoE #30](https://github.com/ackness/repoe/pull/30)。
保留 tag `handoff/quality-family-20261103` 指向 `99c14eb5…`；
全新远端对象库取回后，包、manifest 与 62 个成员摘要均通过核对。
游戏 part 只改变品质文件及其 snapshot manifest，其他 60 个游戏成员和 13 个语言摘要保持一致。
真实 `data:prepare` 已完成从 runtime `5e785e78…` 到 `e73febc0…` 的同补丁刷新，
保留两个本地维护文件。历史树可选属性修复及已发布的语言组合继续保留。

数据采用是一次人工审阅的锁修改：通过准确提交的 PR 检查与完整 CI，再确认合并后的主线完整 CI。
自动同步控制器（`pipeline/latest/`）已删除。真实新补丁迁移、线上回退与应用部署仍未验证。

## 仓库改造进度

| 项目 | 状态 |
| --- | --- |
| `pipeline/` 按功能分目录 | 已完成 |
| 设计与现状分为两份文档 | 已完成（本页） |
| 清理遗留入口：只依赖旧原表的 regen 检查段、原表与 vendor 再生入口、重复的 `data`／`data:fetch` 别名 | 已完成 |
| CI 缓存下载内容；导入后断言工作树干净 | 已完成（`mise run data:check`） |
| CI 不再重跑 owner 的普通领域与显示生产；精简发行包直接从已准备的快照组装 | 已完成 |
| 分域锁收敛为一份按 tag 引用两个 owner 发布的 JSON（`pipeline/data.lock.json`）；删除只为重放生产保留的锁、普通领域与显示生成脚本及候选任务 | 已完成 |
| 领域转换工具（`tools/pobr-data-adapter`、`pipeline/policies/`）迁往 RePoE；语言生产者迁往翻译库；退役 `data:tree:*`／`data:native:candidate` | 已完成（ackness/repoe#17、ackness/poe2-zh-cn-stats#8）；keystone 字节检查改在 RePoE |
| 删除入库的天赋树美术（`web/public/tree-art/`，533 个由 vendor 生成的文件）与生成任务；树查看器只画节点圆点和连线 | 已完成；节点美术以后作为独立功能接入 owner 发布 |
| 特殊制作目录的投影（原 `pipeline/imports/crafting_inputs.py`）迁往 RePoE，由游戏 part 携带；数据锁去掉 `crafting` 输入 | 已完成（ackness/repoe#21，发布序号 3）；输出字节、快照与运行／release 身份不变 |
| `data:prepare` 原位刷新完整的同补丁陈旧安装（保留本地 patch／维护文件），可用 `POBR_DATA_CACHE` 共享下载缓存 | 已完成 |
| 删除自动同步控制器 `pipeline/latest/` 与 `data-sync.yml`；换数据改为人工审阅的锁修改 | 已完成 |
| 引擎内嵌游戏表退出（4.2–4.4）：技能类型 sidecar、`BASE_MAXIMUM_RAGE`、`JEWEL_RADIUS_*` 改由游戏 part 提供 | 本分支完成，待 `game/0.5.5.5-2` 正式发布；保留项及理由见 `crates/pobr-data/tests/no_embedded_data.rs` |
| 构筑配置选项移入引擎 crate（2.6） | 本分支完成 |
| 快照 manifest 按 owner 组合；加载器强制 `contract_version`（2.5） | 本分支完成，待 `contract/1.1` 与 `game/0.5.5.5-2` 正式发布 |

adapter 的 30 份 JSON 策略已完整迁入 RePoE，内容与迁移前一致；Cargo 依赖、工具链、任务与 CI 也由 RePoE 维护。
PoBR 留下的自有规则属于引擎语义，继续随本仓库代码维护，不作为领域适配策略迁移：两份 shared 规则文件（`local_mods.json`、`special_mods.json`）仍在数据锁 `shared_files`；九份构筑配置选项已移入 `crates/pobr-gamedata/src/domains/config_options/` 并编译进引擎，计入引擎身份而不计入运行数据身份。

## 领域现状

下表的数量描述最近一次固定产物，不是全游戏覆盖率。

| 领域 | 已接入普通消费者 | 剩余缺口与关闭条件 | 契约 |
| --- | --- | --- | --- |
| 基底／词缀／制作 | 源 Stats／Mods 全八槽、Families、基底数值与继承 tags；制作七表投影保留原值诊断 | 13 个 charm 缺 Flag／Unknown／MergeMode 的完整解释，完整 packet 迁移 0/13；socket、subtype、权重与资格未全部迁源 | [基底](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-base-items.md#remaining-source-and-consumer-contracts)、[制作](https://github.com/ackness/repoe/blob/master/RePoE/docs/pobr_crafting_catalog.md) |
| 技能／品质 | effect→stat-set→等级／品质、gem 与 effect 关系、普通／额外品质原生作用域；424 个品质 owner；77 项旧 implicit 注入已移除 | `stat_set_index` 仍是 PoB 1-based 编号；7 项旧 implicit 惰性保留；五个 DNT 品质 owner 排除，Snipe 有越界作用域；未选分支 GlobalEffect、具体 stat 消费者仍有限制 | [技能覆盖](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-skill-overrides.md)、[品质](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/policies/gem-quality/README.md) |
| 天赋 | 4913 节点中 4823 使用 CSD、51 混合、39 整文本兼容；72 个 variant 中 70 使用 CSD | 39 个兼容节点缺当前 scope 描述、owner 歧义、buff 表示或 compound；placement／armour、Requirement／QuestFlag 需独立依据与范围珠宝、加点消费者验证 | [天赋树](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-passive-tree.md) |
| 符文／魂核 | 305 项中 296 项使用目标 CSD | 9 项整项兼容：5 项缺 compound 上下文，4 项缺当前 CSD；需按精确资源补完整 packet | [符文](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-socketables.md) |
| 描述／tier | plain-display 政策、完整分支与原始条件、显示缩放与 lookup、TierIndex 区间反解 | 37 个 scoped 兼容 ID：8 个 owner 歧义、13 个缺关系 handler、16 个无普通分支；25 项未渲染保留诊断 | [描述](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-stat-descriptions.md) |
| actor／常量／关系 | 649 个 actor 的 27 个字段、技能有序引用、怪物抗性与成长、图腾、玩家与徒手常量、Boss 数值、珠宝槽与装备授予技能 | OT 的 merge／`set_`／Stats 作用域与 `mod_list`；目录成员、别名、上限与 effect→variety 等策展需要独立依据 | [actor](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-actors.md#observed-three-root-ot-draft) |
| 暗金 | 65 个授予技能关系中 4 个有显式变体绑定；RePoE 已从固定兼容目录投影 `unique_relations` 领域候选 | 关系候选尚未进入游戏 part，仍需 DTO、物品编辑与计算消费者接入；兼容关系不等于官方事实认证 | [暗金关系候选](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-uniques.md) |
| PoB2 解释与 buff | 各域保留项已按字段登记 | parser／config／special、部分 stat 映射与关系仍用社区解释；buff 的 15 处引用上下文未证等价，保持 `guard_eligible=false` | [PoB2 依赖](pob2-differences.md#数据来源现状与退出顺序) |
| 语言／显示 | 繁中与简中 FK v3 及紧凑侧表接入普通后端；两种语言各 1073 个 gem 中 1053 个源译、20 个英文阻断；职业与升华路由、打造名称、辅助说明、审核补译导入已接线 | 简中客户端补丁与服别未确认；真实审核补译 0 条；45 个候选是否有当前 UI 收益需先核 caller；国服数值未认证 | 翻译库的语言发布 |
| 新发布与持续更新 | 序号 5 已发布真实历史树修复并验证同补丁安装刷新、组合与 WASM 消费 | 新快照须复核固定测试绑定，真实新补丁与线上回退仍待对应场景 | [锁驱动更新](../pipeline/runtime/locked-update.md) |
| 玩家验收与性能 | 完整 CI 用同次 WASM 消费当前数据锁的紧凑发行包；正式 Web 与独立 WASM 包仍发布逐文件快照，紧凑包仅 CI 验证 | 下一次新数据用普通 Web 入口验证导入、技能／辅助、装备／打造、加点、保存重开与回滚；在目标设备测冷加载、首次计算、内存与包体 | [WASM 测量](wasm-package.md#runtime-performance-comparison) |

`0.5.5.5` 品质来源核对：原始 429 个 owner（普通 442／额外 273 条）中，当前发布 424 个（437／273 条、340 个不同 stat ID）；已发布向量的 ID、顺序、倍率、通道和全部 82 条作用域记录均与固定原始表一致。`AddTypes`／`AddMinionTypes` 在本批 429 个 owner 中均为空，4 个辅助品质 owner 的作用域也均为空。原始 1191 个宝石投影为 1120 个，71 个省略项均标记为 DNT 占位，已发布宝石关系逐项一致。

生产端 v2 策略已替代历史 PoB 目录筛选：每个原始 owner 必须明确归入发布或排除，排除项须由 `SkillGems → GemEffects → BaseItemTypes` 的原生引用证明只关联 DNT 占位宝石。真实、混合、无关联或缺失引用均不能据此排除；旧 v1 策略仍能重放原产物。品质事实齐全不代表全部消费者已实现：ExplosiveConcoctionSecondPlayer 和 SiegeBallistaProjectileExplodePlayer 的有效品质原生分支仍没有兼容显示编号；Snipe 普通作用域 `[1,2,3]` 的位置 3 越界。这些选择边界与[具体计算消费者限制](mechanics/items.md#宝石等级品质与授予技能)继续保留，消费者不从本地原始表补造缺失行。

## 已知例外

无。来源台账已归位：原料与采集记录迁入 [RePoE `sources/`](https://github.com/ackness/repoe/tree/master/sources)，语言策展记录迁入[翻译库 `sources/`](https://github.com/ackness/poe2-zh-cn-stats/tree/main/sources)，`pipeline/imports/sources.lock.json` 只保留 PoBR 消费范围的接受决定。

## 近期顺序

1. 下一次真实 owner 发布按人工改锁流程采用；真实新补丁和线上回退仍需对应场景。
2. 对影响现有 BD 的 charm、actor、描述与分支问题，先复用已有原料和诊断补语义或精确资源，再做最小的消费者修改。
3. 对新数据锁补普通 Web 流程验收；翻译优先接实际 caller，并保留英文或空值回退。
4. 固定目标设备与冷热输入测量后，再针对已测瓶颈优化。

"约 80% 体验即可进入后续优化"是阶段取舍，不是允许 20% 的数值误差，也不是已测得的覆盖率。
进度按能力报告分母、实测结果和阻塞项。

修订前的阶段收据、代号与证据矩阵保留在[固定旧版](https://github.com/ackness/pobr/blob/0fb6f913cc3fac980f9d84e855b26103860d6b96/docs/data-architecture.md#5-当前已有数据与实际缺口)，
更早的版本见[该版本之前的文档](https://github.com/ackness/pobr/blob/23341f06f6e462c4899751d53f93d87dedcc8c96/docs/data-architecture.md#51-剩余字段与关闭条件)。
