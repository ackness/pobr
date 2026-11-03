# 数据现状

最后核对：2026-11-03 UTC。本页原地更新：状态变化时直接改写，不追加历史。
长期设计与规则见[数据架构](data-architecture.md)，命令见[数据管线](../pipeline/README.md)。

## 当前数据锁与托管条件

- 活动目标为 `0.5.5.5`／international。消费产物的 tag + commit + SHA-256、两个 owner 的 publication、
  期望 manifest、运行身份与紧凑 release 身份都以[数据锁](../pipeline/data.lock.json)为准。
- RePoE 发布序号 4，仅推进 `ordinary`：从 RePoE 已发布包保留既有成员，并加入固定暗金 codec 来源；
  游戏 part 仍是序号 3 携带 `trade_crafting_sources.json` 的版本。翻译库为序号 2。
  PoBR 不消费 `ordinary`，本次数据锁 generation 由 2 升至 3，运行文件与 runtime／release 身份不变。
- 序号 4 已经由本地 latest 控制器经 PR／Full 双门禁采用（[PoBR #251](https://github.com/ackness/pobr/pull/251)），
  本地导入与组合校验通过，尚未部署。托管提案与采用因凭据写权限失败，不能据此宣称无人值守链路已打通。

| 托管条件 | 状态 |
| --- | --- |
| master 严格 required `PR gate`，对管理员生效 | 已启用 |
| 仓库 secret `DATA_SYNC_TOKEN` | 已配置；实测推分支和合并 PR 均返回 HTTP 403，需修正该 PAT 对 `ackness/pobr` 的写权限后重验 |
| owner 发布后的 `owned-data-published` 通知 | 未接线：两个 owner 都不发送，按 6 小时轮询 |

本次实测从 [RePoE #25](https://github.com/ackness/repoe/pull/25) 发布开始：
新 handoff `0f68afc3…` 经全新 Git 对象库取回并逐项核对摘要，再登记序号 4。
托管[提案运行](https://github.com/ackness/pobr/actions/runs/41607711817)在推分支时返回 403，
[采用回调](https://github.com/ackness/pobr/actions/runs/41608424131)在合并 PR 时返回
`Resource not accessible by personal access token`。本地使用已有写权限的 `gh` 身份运行同一控制器，
生成候选 `9af1fa94…`，通过同一 head 的 [PR gate](https://github.com/ackness/pobr/actions/runs/41607979674)
与 [Full gate](https://github.com/ackness/pobr/actions/runs/41607981042)，重核来源、基准锁与门禁后采用为
`ebf5b299…`。没有跳过控制器的接受条件。

Full CI 验证固定组合和精简包，用同次运行的独立 WASM 验证三个构筑、`+100 Life`、
保存重开及诊断保留。旧锁和新锁离线重放得到相同的 87 个运行文件；旧锁重放也通过该 WASM 的相同检查，
计算输出和诊断与 CI 精简候选一致。采用后 `data:prepare`、`data:check`、
`verify-composition` 通过，`data:latest check` 返回 `no-new-publication`。
这验证了元数据更新、门禁、受控采用和旧锁恢复能力；本次未做线上锁回退，也未覆盖游戏字节实际变化、
新补丁迁移或浏览器交互验收。

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
| `data:latest resolve --new-target` 生成显式新补丁候选（`target_change` 收据，永非自动采用） | 已完成；尚无真实新补丁演练 |

adapter 的 30 份 JSON 策略已完整迁入 RePoE，内容与迁移前一致；Cargo 依赖、工具链、任务与 CI 也由 RePoE 维护。
PoBR 留下的 11 份自有 shared 规则／构筑配置属于引擎语义，继续随本仓库代码维护，不作为领域适配策略迁移。

## 领域现状

下表的数量描述最近一次固定产物，不是全游戏覆盖率。

| 领域 | 已接入普通消费者 | 剩余缺口与关闭条件 | 契约 |
| --- | --- | --- | --- |
| 基底／词缀／制作 | 源 Stats／Mods 全八槽、Families、基底数值与继承 tags；制作七表投影保留原值诊断 | 13 个 charm 缺 Flag／Unknown／MergeMode 的完整解释，完整 packet 迁移 0/13；socket、subtype、权重与资格未全部迁源 | [基底](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-base-items.md#remaining-source-and-consumer-contracts)、[制作](https://github.com/ackness/repoe/blob/master/RePoE/docs/pobr_crafting_catalog.md) |
| 技能／品质 | effect→stat-set→等级／品质、gem 与 effect 关系、可证倍率；77 项旧 implicit 注入已移除 | `stat_set_index` 仍是 PoB 1-based 编号；7 项旧 implicit 惰性保留；corpse／speed／area、虚拟倍率与品质作用域按实际输出逐项处理 | [技能覆盖](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-skill-overrides.md)、[品质](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/policies/gem-quality/README.md) |
| 天赋 | 4913 节点中 4823 使用 CSD、51 混合、39 整文本兼容；72 个 variant 中 70 使用 CSD | 39 个兼容节点缺当前 scope 描述、owner 歧义、buff 表示或 compound；placement／armour、Requirement／QuestFlag 需独立依据与范围珠宝、加点消费者验证 | [天赋树](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-passive-tree.md) |
| 符文／魂核 | 305 项中 296 项使用目标 CSD | 9 项整项兼容：5 项缺 compound 上下文，4 项缺当前 CSD；需按精确资源补完整 packet | [符文](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-socketables.md) |
| 描述／tier | plain-display 政策、完整分支与原始条件、显示缩放与 lookup、TierIndex 区间反解 | 37 个 scoped 兼容 ID：8 个 owner 歧义、13 个缺关系 handler、16 个无普通分支；25 项未渲染保留诊断 | [描述](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-stat-descriptions.md) |
| actor／常量／关系 | 649 个 actor 的 27 个字段、技能有序引用、怪物抗性与成长、图腾、玩家与徒手常量、Boss 数值、珠宝槽与装备授予技能 | OT 的 merge／`set_`／Stats 作用域与 `mod_list`；目录成员、别名、上限与 effect→variety 等策展需要独立依据 | [actor](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-actors.md#observed-three-root-ot-draft) |
| 暗金 | 65 个授予技能关系中 4 个有显式变体绑定；RePoE 已从固定兼容目录投影 `unique_relations` 领域候选 | 关系候选尚未进入游戏 part，仍需 DTO、物品编辑与计算消费者接入；兼容关系不等于官方事实认证 | [暗金关系候选](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-uniques.md) |
| PoB2 解释与 buff | 各域保留项已按字段登记 | parser／config／special、部分 stat 映射与关系仍用社区解释；buff 的 15 处引用上下文未证等价，保持 `guard_eligible=false` | [PoB2 依赖](pob2-differences.md#数据来源现状与退出顺序) |
| 语言／显示 | 繁中与简中 FK v3 及紧凑侧表接入普通后端；两种语言各 1073 个 gem 中 1053 个源译、20 个英文阻断；职业与升华路由、打造名称、辅助说明、审核补译导入已接线 | 简中客户端补丁与服别未确认；真实审核补译 0 条；45 个候选是否有当前 UI 收益需先核 caller；国服数值未认证 | 翻译库的语言发布 |
| 新发布与持续更新 | 序号 4 已验证发布、候选、精确 head 的 PR／Full、最终包消费、本地控制器采用及旧锁离线恢复 | 修正托管 PAT 写权限后重验无人值守提案与采用；再用实际改变游戏 part 的发布验证运行数据刷新和完整回滚 | [最新发布同步](../pipeline/latest/latest-data.md) |
| 玩家验收与性能 | 完整 CI 用同次 WASM 消费当前数据锁的紧凑发行包；正式 Web 与独立 WASM 包仍发布逐文件快照，紧凑包仅 CI 验证 | 下一次新数据用普通 Web 入口验证导入、技能／辅助、装备／打造、加点、保存重开与回滚；在目标设备测冷加载、首次计算、内存与包体 | [WASM 测量](wasm-package.md#runtime-performance-comparison) |

## 已知例外

无。来源台账已归位：原料与采集记录迁入 [RePoE `sources/`](https://github.com/ackness/repoe/tree/master/sources)，语言策展记录迁入[翻译库 `sources/`](https://github.com/ackness/poe2-zh-cn-stats/tree/main/sources)，`pipeline/imports/sources.lock.json` 只保留 PoBR 消费范围的接受决定。

## 近期顺序

1. 修正 `DATA_SYNC_TOKEN` 对 `ackness/pobr` 的写权限；下一次真实 owner 发布重验托管提案与采用。
   运行数据刷新、真实新补丁和完整回滚仍需对应的新生产输入，不重复登记 bootstrap 或制造空序号更新。
2. 对影响现有 BD 的 charm、actor、描述与分支问题，先复用已有原料和诊断补语义或精确资源，再做最小的消费者修改。
3. 对新数据锁补普通 Web 流程验收；翻译优先接实际 caller，并保留英文或空值回退。
4. 固定目标设备与冷热输入测量后，再针对已测瓶颈优化。

"约 80% 体验即可进入后续优化"是阶段取舍，不是允许 20% 的数值误差，也不是已测得的覆盖率。
进度按能力报告分母、实测结果和阻塞项。

修订前的阶段收据、代号与证据矩阵保留在[固定旧版](https://github.com/ackness/pobr/blob/0fb6f913cc3fac980f9d84e855b26103860d6b96/docs/data-architecture.md#5-当前已有数据与实际缺口)，
更早的版本见[该版本之前的文档](https://github.com/ackness/pobr/blob/23341f06f6e462c4899751d53f93d87dedcc8c96/docs/data-architecture.md#51-剩余字段与关闭条件)。
