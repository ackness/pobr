# 数据现状

最后核对：2026-11-05 UTC。本页原地更新：状态变化时直接改写，不追加历史。
长期设计与规则见[数据架构](data-architecture.md)，命令见[数据管线](../pipeline/README.md)。

活动目标为 `0.5.5.5`／international。包、manifest、运行身份与紧凑发行身份都以[数据锁](../pipeline/data.lock.json)为准，
换数据用 `mise run data:bump`。每个 owner Release 的 `receipt.json` 记录它的全部来源 pin，
见 [RePoE Releases](https://github.com/ackness/repoe/releases)（`game/<patch>-<n>`，流程见 [RELEASING.md](https://github.com/ackness/repoe/blob/master/RELEASING.md)）
与[翻译库 Releases](https://github.com/ackness/poe2-zh-cn-stats/releases)（`locale/<patch>-<n>`）；本页不复制哈希与发布记录。

## 发布与采用

| 能力 | 状态 | 阻塞项 |
| --- | --- | --- |
| 游戏包 | 锁指向 `game/0.5.5.5-2`，目前按本地模拟 Release 写入。这一批计算结果不变：新增 `base/active_skill_types.json`，`base/game_constants.json` 增加 `base_maximum_rage`，天赋树节点携带原生 stat packet；引擎不再内嵌这些表（保留项见 `crates/pobr-data/tests/no_embedded_data.rs`） | 合并契约 1.1 并打 `contract/1.1`；RePoE 改依赖该 tag 后正式发布 `game/0.5.5.5-2`；再对真实 Release 重跑 `data:bump` |
| 语言包 | `locale/0.5.5.5-1`，从 Release 采用。只换语言发布不需要新的游戏发布 | 无 |
| 辅助显示 | `support_display` 仍按 `handoff/*` tag + commit 拉取；`data:bump` 已能在语言 Release 带附件时一并替换 | 翻译库在 `locale/0.5.5.5-2` 发布辅助显示附件 |
| 测试夹具 | `test_fixtures` 按 `handoff/*` tag + commit 拉取，换目标时保留 | 无 |
| 运行 manifest | 按 owner 组合：游戏 snapshot 只列游戏成员，`data:prepare` 加入语言包的 `i18n/**` 并推出 `languages`。仍列语言成员的旧游戏 snapshot（`game/0.5.5.5-1` 及更早）和未声明 `contract_version` 的 schema 3 manifest 都被拒绝 | 随 `game/0.5.5.5-2` 正式发布生效 |
| PoBR 自有规则 | `local_mods.json`、`special_mods.json` 在锁的 `shared_files`；九份构筑配置选项编译进 `crates/pobr-gamedata/src/domains/config_options/`，计入引擎身份 | 无 |
| 采用流程 | 人工审阅的锁修改：精确 head 通过 PR gate 与 Full CI，合并后再确认主线 Full CI。固定测试绑定组合 manifest 的游戏部分，换游戏包时需复核成员差异再重绑 | 无 |
| 新补丁迁移与回退 | 流程已写在[锁驱动更新](../pipeline/runtime/locked-update.md)，尚未用真实新补丁走过 | 一次真实新补丁：两个 owner 一起发布并 bump，验证线上回退 |
| 部署 | 锁合并到 master 只跑 Full CI；生产站点只由稳定 `v0.x.y` tag 部署（[发布与采用](data-architecture.md#5-更新采用与发布)） | 新数据上线须在 `data:bump` PR 里升 patch 版本，合并后推 tag |
| 发行包 | Web 与独立 WASM 发布逐文件快照；紧凑发行包只在 CI 中由同次 WASM 消费验证 | 见下表“玩家验收与性能” |

仓库改造（`pipeline/` 分目录、锁收敛为一份、adapter 与语言生产迁往 owner 仓库、删除 `pipeline/latest/` 等）已完成，过程见 git 历史与[固定旧版](https://github.com/ackness/pobr/blob/0fb6f913cc3fac980f9d84e855b26103860d6b96/docs/data-architecture.md#5-当前已有数据与实际缺口)。

## 领域现状

数量描述当前锁的产物，不是全游戏覆盖率。仍依赖的冻结输入及其退出顺序只维护在
[与 PoB2 的差异](pob2-differences.md#数据来源现状与退出顺序)，本表不重复。

| 能力 | 状态 | 阻塞项 |
| --- | --- | --- |
| 基底／词缀／制作 | Stats／Mods 全八槽、Families、基底数值与继承 tags 来自目标事实；制作七表投影保留原值诊断（[基底](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-base-items.md#remaining-source-and-consumer-contracts)、[制作](https://github.com/ackness/repoe/blob/master/RePoE/docs/pobr_crafting_catalog.md)） | 13 个 charm 缺 Flag／Unknown／MergeMode 的完整解释；socket、subtype、权重与资格未全部迁源 |
| 技能／品质 | effect→stat-set→等级／品质、gem 与 effect 关系、普通／额外品质的原生作用域已接入；旧 implicit 注入已移除（[技能覆盖](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-skill-overrides.md)、[品质](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/policies/gem-quality/README.md)） | `stat_set_index` 仍是 PoB 1-based 编号；7 项旧 implicit 惰性保留；品质的选择边界见表下 |
| 天赋 | 普通树使用 CSD，部分节点混合（[天赋树](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-passive-tree.md)） | 39 个整文本兼容节点缺当前 scope 描述、owner 歧义、buff 表示或 compound；placement／armour、Requirement／QuestFlag 需独立依据，并经范围珠宝与加点消费者验证 |
| 符文／魂核 | 大部分使用目标 CSD（[符文](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-socketables.md)） | 9 项整项兼容：5 项缺 compound 上下文，4 项缺当前 CSD；需按精确资源补完整 packet |
| 描述／tier | plain-display 政策、完整分支与原始条件、显示缩放与 lookup、TierIndex 区间反解（[描述](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-stat-descriptions.md)） | 37 个 scoped 兼容 ID：8 个 owner 歧义、13 个缺关系 handler、16 个无普通分支；25 项未渲染保留诊断 |
| actor／常量／关系 | actor 字段、技能有序引用、怪物抗性与成长、图腾、玩家与徒手常量、Boss 数值、珠宝槽与装备授予技能（[actor](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-actors.md#observed-three-root-ot-draft)） | OT 的 merge／`set_`／Stats 作用域与 `mod_list`；目录成员、别名、上限与 effect→variety 等策展需要独立依据 |
| 暗金 | RePoE 从固定兼容目录投影 `unique_relations` 候选（[暗金关系候选](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/ordinary-uniques.md)） | 候选尚未进入游戏 part，仍需 DTO、物品编辑与计算消费者接入；兼容关系不等于官方事实认证 |
| PoB2 解释与 buff | 各域保留项已按字段登记 | 按[冻结输入清单](pob2-differences.md#数据来源现状与退出顺序)的顺序退出；buff 的引用上下文未证等价，保持 `guard_eligible=false` |
| 语言／显示 | 繁中与简中 FK v3 及紧凑侧表接入普通后端；职业与升华路由、打造名称、辅助说明、审核补译导入已接线（[翻译库 `docs/`](https://github.com/ackness/poe2-zh-cn-stats/tree/main/docs)） | 每种语言 20 个 gem 名英文阻断；简中客户端补丁与服别未确认；真实审核补译 0 条；45 个候选是否有当前 UI 收益需先核 caller；国服数值未认证 |
| 玩家验收与性能 | 完整 CI 用同次 WASM 消费当前锁的紧凑发行包（[WASM 测量](wasm-package.md#runtime-performance-comparison)） | 下一次新数据用普通 Web 入口验证导入、技能／辅助、装备／打造、加点、保存重开与回滚；在目标设备测冷加载、首次计算、内存与包体 |

品质的剩余边界：五个 DNT 占位 owner 按 `SkillGems → GemEffects → BaseItemTypes` 的原生引用排除；
Snipe 普通作用域 `[1,2,3]` 的位置 3 越界；ExplosiveConcoctionSecondPlayer 与 SiegeBallistaProjectileExplodePlayer 的有效品质分支没有兼容显示编号。
消费者不从本地原始表补造缺失行，具体限制见[宝石等级、品质与授予技能](mechanics/items.md#宝石等级品质与授予技能)。

## 已知例外

无。原料与采集记录在 [RePoE `sources/`](https://github.com/ackness/repoe/tree/master/sources)，语言策展记录在[翻译库 `sources/`](https://github.com/ackness/poe2-zh-cn-stats/tree/main/sources)，`pipeline/imports/sources.lock.json` 只保留 PoBR 消费范围的接受决定。

## 近期顺序

1. 完成上面“发布与采用”的阻塞项，正式采用 `game/0.5.5.5-2` 与 `locale/0.5.5.5-2`。
2. 对影响现有 BD 的 charm、actor、描述与分支问题，先复用已有原料和诊断补语义或精确资源，再做最小的消费者修改。
3. 对新数据锁补普通 Web 流程验收；翻译优先接实际 caller，并保留英文或空值回退。
4. 固定目标设备与冷热输入测量后，再针对已测瓶颈优化。

"约 80% 体验即可进入后续优化"是阶段取舍，不是允许 20% 的数值误差，也不是已测得的覆盖率。
进度按能力报告分母、实测结果和阻塞项。
