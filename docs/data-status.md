# 数据现状

最后核对：2026-11-15。本页原地更新：状态变化时直接改写，不追加历史。
长期设计与规则见[数据架构](data-architecture.md)，命令见[数据管线](../pipeline/README.md)。

活动目标为 `poe2`／`0.5.5.8`／international。包、manifest、运行身份与紧凑发行身份都以[数据锁](../pipeline/data.lock.json)为准，
换数据用 `mise run data:bump`。每个 owner Release 的 `receipt.json` 记录它的全部来源 pin，
见 [RePoE Releases](https://github.com/ackness/repoe/releases)（`game/<patch>-<n>`，流程见 [RELEASING.md](https://github.com/ackness/repoe/blob/master/RELEASING.md)）
与[翻译库 Releases](https://github.com/ackness/poe2-zh-cn-stats/releases)（`locale/<patch>-<n>`，流程见 [RELEASING.md](https://github.com/ackness/poe2-zh-cn-stats/blob/main/RELEASING.md)）；本页不复制哈希与发布记录。
PoBR 一侧的换数据、发版与回退步骤见 [RELEASING.md](../RELEASING.md)。

## 发布与采用

| 能力 | 状态 | 阻塞项 |
| --- | --- | --- |
| 游戏包 | `game/0.5.5.8-1`，数据锁从真实 Release 固定。包含英文宝石说明、技能类型表、原生 stat packet，以及按目标 facts 投影的可选物品类别英文成员 `overlay/item_class_names.json`（`contract/1.4`）；引擎内嵌数据的保留项见 `crates/pobr-data/tests/no_embedded_data.rs` | 无 |
| 语言包 | `locale/0.5.5.8-1`，数据锁从真实 Release 固定。沿用已登记的简中词条、宝石说明与繁中显示来源，简中和繁中均提供与目标游戏成员按稳定 ID、精确英文核对的 `ItemClasses.Name` 作用域。只换语言发布不需要新的游戏发布；锁的 `locale` 须是语言包提供的语言（当前支持英文、简中、繁中） | 无 |
| 测试夹具 | `test_fixtures` 按 `handoff/*` tag + commit 拉取，发布时的 `target` 仍为 `0.5.5.5`；换活动目标不重贴夹具身份 | 无 |
| 运行 manifest | 按 owner 组合：游戏 snapshot 只列游戏成员，`data:prepare` 加入语言包的 `i18n/**` 并推出 `languages`。仍列语言成员的旧游戏 snapshot（`game/0.5.5.5-1` 及更早）和未声明 `contract_version` 的 schema 3 manifest 都被拒绝 | 无（已生效；真实 Release 的采用见“游戏包”） |
| PoBR 自有规则 | `local_mods.json`、`special_mods.json` 在锁的 `shared_files`；九份构筑配置选项编译进 `crates/pobr-gamedata/src/domains/config_options/`，计入引擎身份 | 无 |
| 采用流程 | 当前锁由真实 Release 正常双 owner bump 生成并安装；合并前仍须精确 head 通过审阅、PR gate 与 Full CI，合并后确认主线 Full CI。固定测试绑定组合 manifest 的游戏部分，换游戏包时复核成员差异再重绑 | 本地安装与验收不等于数据 PR 已合并或 CI 已通过 |
| 新补丁迁移与回退 | `0.5.5.5` → `0.5.5.8` 已完成正常双包改锁、组合安装与消费者绑定迁移；正式包与已验组合的全部成员、运行及紧凑身份一致，复用已通过的四步组合验收和真实 UI 检查；正常 bump 重算 87 个 L2 构筑，数值无变化 | 线上回退仍未验收；数据 PR 与应用发版分别完成采用和上线 |
| 部署 | 锁合并到 master 只跑 Full CI；生产站点只由稳定 `v0.x.y` tag 部署（[RELEASING.md](../RELEASING.md)）。线上数据是该 tag 提交上的锁（`git show v0.x.y:pipeline/data.lock.json`），站点顶栏显示版本号 | 无 |
| 发行包 | Web 与独立 WASM 发布逐文件快照；紧凑发行身份已按当前锁重算并与本地已验组合一致，本地 release E2E 与 CI 仍须验证对应应用候选 | 见下表“玩家验收与性能” |

## 领域现状

数量描述当前锁的产物，不是全游戏覆盖率；细节与计数在各链接的 owner 文档里。
仍依赖的冻结输入及其退出顺序只维护在[与 PoB2 的差异](pob2-differences.md#数据来源现状与退出顺序)。

| 能力 | 状态 | 阻塞项 |
| --- | --- | --- |
| 基底／词缀／制作 | 来自游戏事实（[基底](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-base-items.md#remaining-source-and-consumer-contracts)、[制作](https://github.com/ackness/repoe/blob/master/RePoE/docs/release/pobr_crafting_catalog.md)） | charm 的部分字段缺完整解释；socket、subtype、权重与资格未全部迁源 |
| 技能／品质 | 等级、品质与 gem 关系使用原生作用域（[技能覆盖](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-skill-overrides.md)、[品质](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/policies/gem-quality/README.md)、[限制](mechanics/items.md#宝石等级品质与授予技能)） | `stat_set_index` 仍是 PoB 编号；少数品质分支越界或没有显示编号 |
| 天赋 | 普通树使用游戏描述，少数节点混合（[天赋树](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-passive-tree.md)） | 少数整文本兼容节点缺当前描述或关系；需经范围珠宝与加点验证 |
| 符文／魂核 | 大部分使用游戏描述（[符文](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-socketables.md)） | 少数整项兼容，需补完整 packet |
| 描述／tier | 完整分支、显示缩放与 tier 反解（[描述](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-stat-descriptions.md)） | 少数 scoped 兼容 ID 有 owner 歧义或缺 handler |
| actor／常量／关系 | actor、怪物、图腾、玩家常量、Boss 数值与装备授予技能（[actor](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-actors.md#observed-three-root-ot-draft)） | 作用域与目录策展需要独立依据 |
| 暗金 | RePoE 已投影关系候选（[暗金](https://github.com/ackness/repoe/blob/master/tools/pobr-data-adapter/docs/ordinary-uniques.md)） | 候选未进入游戏包，消费者未接入 |
| PoB2 解释与 buff | 保留项按字段登记 | 按[冻结输入清单](pob2-differences.md#数据来源现状与退出顺序)的顺序退出 |
| 语言／显示 | 简中与繁中接入，辅助说明与审核补译已接线（[翻译库](https://github.com/ackness/poe2-zh-cn-stats/tree/main/docs)；缺译统计 `mise run i18n:coverage`）；当前锁已接入 `ItemClasses.Name`，制作编辑器按稳定 ID 与精确英文显示类别。本地真实 UI 覆盖全部 26 类的三语标签、语言切换、草稿及保存重开，旧包缺成员的行为另有回归验证（[成员契约](data-architecture.md#item-class-names)、[显示行为](web-contracts.md#装备打造与暗金目录)） | 每种语言仍有 2 个技能 gem 名缺译（`mise run i18n:coverage`）；未知国服源客户端的版本与数值等价仍未认证，已接入的词条行、宝石说明、召唤物名及类别译文不扩大认证范围 |
| 玩家验收与性能 | 锁改变时完整 CI 跑 Web 冒烟；正式包继续用逐文件快照（[加载对比](wasm-package.md#web-data-load-comparison)） | 回滚需手动验证；真实玩家设备未测 |

原料与采集记录在 [RePoE `sources/`](https://github.com/ackness/repoe/tree/master/sources) 与[翻译库 `sources/`](https://github.com/ackness/poe2-zh-cn-stats/tree/main/sources)；`pipeline/imports/sources.lock.json` 只保留 PoBR 消费范围的接受决定。

## 近期顺序

1. 按可复现缺陷和明确的数据采用需求选择完整小批次；发版上线、线上回退等条件触发操作不阻塞无关应用修复发布。
2. 对影响现有构筑的缺口补语义或精确资源，再做最小的消费者修改；不因旧冻结清单或未知未来输入扩大范围，性能只针对实测瓶颈优化。
