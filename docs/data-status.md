# 数据现状

最后核对：2026-11-02 UTC。本页原地更新：状态变化时直接改写，不追加历史。
长期设计与规则见[数据架构](data-architecture.md)，命令见[数据管线](../pipeline/README.md)。

## 当前数据锁与托管条件

- 活动目标为 `0.5.5.5`／international。消费产物的 tag + commit + SHA-256、两个 owner 的 publication、
  期望 manifest、运行身份与紧凑 release 身份都以[数据锁](../pipeline/data.lock.json)为准。
- 两个 owner 的发布序号都还是 1（bootstrap 登记）。真正新的发布还没有走过提案、门禁到采用的全链。
- 旧版 `0.5.5.4.2` 由冻结的[回滚锁](../pipeline/runtime/build-data-0.5.5.4.2.lock.json)恢复，不改变 `CURRENT`。
- 当前数据锁已经本地采用并通过主线完整 CI，尚未部署。

| 托管条件 | 状态 |
| --- | --- |
| master 严格 required `PR gate`，对管理员生效 | 已启用 |
| 仓库 secret `DATA_SYNC_TOKEN` | 未配置，无人值守的提案与采用未启用 |

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

## 领域现状

下表的数量描述最近一次固定产物，不是全游戏覆盖率。

| 领域 | 已接入普通消费者 | 剩余缺口与关闭条件 | 契约 |
| --- | --- | --- | --- |
| 基底／词缀／制作 | 源 Stats／Mods 全八槽、Families、基底数值与继承 tags；制作七表投影保留原值诊断 | 13 个 charm 缺 Flag／Unknown／MergeMode 的完整解释，完整 packet 迁移 0/13；socket、subtype、权重与资格未全部迁源 | [基底](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-base-items.md#remaining-source-and-consumer-contracts)、[制作](../pipeline/imports/crafting-inputs.md) |
| 技能／品质 | effect→stat-set→等级／品质、gem 与 effect 关系、可证倍率；77 项旧 implicit 注入已移除 | `stat_set_index` 仍是 PoB 1-based 编号；7 项旧 implicit 惰性保留；corpse／speed／area、虚拟倍率与品质作用域按实际输出逐项处理 | [技能覆盖](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-skill-overrides.md)、[品质](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/policies/gem-quality/README.md) |
| 天赋 | 4913 节点中 4823 使用 CSD、51 混合、39 整文本兼容；72 个 variant 中 70 使用 CSD | 39 个兼容节点缺当前 scope 描述、owner 歧义、buff 表示或 compound；placement／armour、Requirement／QuestFlag 需独立依据与范围珠宝、加点消费者验证 | [天赋树](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-passive-tree.md) |
| 符文／魂核 | 305 项中 296 项使用目标 CSD | 9 项整项兼容：5 项缺 compound 上下文，4 项缺当前 CSD；需按精确资源补完整 packet | [符文](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-socketables.md) |
| 描述／tier | plain-display 政策、完整分支与原始条件、显示缩放与 lookup、TierIndex 区间反解 | 37 个 scoped 兼容 ID：8 个 owner 歧义、13 个缺关系 handler、16 个无普通分支；25 项未渲染保留诊断 | [描述](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-stat-descriptions.md) |
| actor／常量／关系 | 649 个 actor 的 27 个字段、技能有序引用、怪物抗性与成长、图腾、玩家与徒手常量、Boss 数值、珠宝槽与装备授予技能 | OT 的 merge／`set_`／Stats 作用域与 `mod_list`；目录成员、别名、上限与 effect→variety 等策展需要独立依据 | [actor](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-actors.md#observed-three-root-ot-draft) |
| 暗金 | 65 个授予技能关系中 4 个有显式变体绑定 | 需要一份固定的独立 unique→base→mods／roll／variant 关系，再接物品编辑与计算消费者 | — |
| PoB2 解释与 buff | 各域保留项已按字段登记 | parser／config／special、部分 stat 映射与关系仍用社区解释；buff 的 15 处引用上下文未证等价，保持 `guard_eligible=false` | [PoB2 依赖](pob2-differences.md#数据来源现状与退出顺序) |
| 语言／显示 | 繁中与简中 FK v3 及紧凑侧表接入普通后端；两种语言各 1073 个 gem 中 1053 个源译、20 个英文阻断；职业与升华路由、打造名称、辅助说明、审核补译导入已接线 | 简中客户端补丁与服别未确认；真实审核补译 0 条；45 个候选是否有当前 UI 收益需先核 caller；国服数值未认证 | 翻译库的语言发布 |
| 新发布与持续更新 | owner 索引、latest 控制器、显式采用、失败恢复与数据锁／运行身份 CAS | owner 完成一次有实际收益的新生产与发布；配置凭据后用它验证提案、精确 head 的 PR／Full、最终包消费、采用和完整回滚 | [最新发布同步](../pipeline/latest/latest-data.md) |
| 玩家验收与性能 | 完整 CI 用同次 WASM 消费当前数据锁的紧凑发行包 | 下一次新数据用普通 Web 入口验证导入、技能／辅助、装备／打造、加点、保存重开与回滚；在目标设备测冷加载、首次计算、内存与包体 | [WASM 测量](wasm-package.md#runtime-performance-comparison) |

## 已知例外

| 例外 | 退出条件 |
| --- | --- |
| `web/public/tree-art/` 入库的天赋树美术由 vendor 生成 | RePoE 发布客户端树美术后改为导入 |
| 冻结的 `0.5.5.4.2` 回滚锁不经数据锁 | 两个 owner 发布对应的 recovery 描述后改为第二份数据锁 |

## 近期顺序

1. 由 owner 完成一次有实际收益的新生产与发布，配置 `DATA_SYNC_TOKEN` 后用它演练提案、门禁、采用与回滚。不要重复登记 bootstrap。
2. 对影响现有 BD 的 charm、actor、描述与分支问题，先复用已有原料和诊断补语义或精确资源，再做最小的消费者修改。
3. 对新数据锁补普通 Web 流程验收；翻译优先接实际 caller，并保留英文或空值回退。
4. 固定目标设备与冷热输入测量后，再针对已测瓶颈优化。

"约 80% 体验即可进入后续优化"是阶段取舍，不是允许 20% 的数值误差，也不是已测得的覆盖率。
进度按能力报告分母、实测结果和阻塞项。

修订前的阶段收据、代号与证据矩阵保留在[固定旧版](https://github.com/ackness/pobr/blob/0fb6f913cc3fac980f9d84e855b26103860d6b96/docs/data-architecture.md#5-当前已有数据与实际缺口)，
更早的版本见[该版本之前的文档](https://github.com/ackness/pobr/blob/23341f06f6e462c4899751d53f93d87dedcc8c96/docs/data-architecture.md#51-剩余字段与关闭条件)。
