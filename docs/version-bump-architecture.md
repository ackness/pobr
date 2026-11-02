# 数据版本与覆盖层

PoBR 将应用版本、基础库版本和游戏数据版本分开。RePoE 独立存储／处理游戏事实，翻译库独立存储／处理语言；PoBR 只跟踪代码、锁、来源元数据和自有规则。版本目录由 `mise run data:prepare` 按固定输入临时组合，供构建和实物测试使用，不提交完整快照或把旧 PoBR artifact 当回退。

数据目录的职责如下：

| 路径 | 职责 |
| --- | --- |
| `data/CURRENT` | 运行时默认使用的游戏数据版本 |
| `data/<version>/base/` | 临时导入并适配的该版本游戏数据 |
| `data/<version>/overlay/` | 该版本的补充、修正与 PoB2 转换的兼容数据，随 owner 包导入 |
| `data/<version>/generated/` | 从固定输入生成的派生数据；maintenance 不进 runtime manifest |
| `data/<version>/i18n/` | 独立语言产物的临时运行投影，保留来源和精确身份 |
| `data/overlay-common/` | 十一份入 Git 的 PoBR 自有规则，以及七份临时导入的外部事实／兼容输入；按 loader 各自读取 |
| `data/<version>/patch/` | manifest 清单之外的补丁层，镜像版本目录的相对路径 |

<a id="p1-3-version-independent-curated-overlay-dataoverlay-common"></a>

## 共享规则与版本覆盖

源码中的旧标记 **P1-3** 指共享覆盖层约定：稳定的语义规则放在
`overlay-common/`，不随每次数据升版复制。支持按稳定键合并的数据域由版本层覆盖同键，
其余保留共享项；整文件回退的数据域在版本文件存在时使用版本文件，缺失时读取共享文件。
具体选择以 `pobr-gamedata` 各域的加载器为准，不能把一种合并方式推广到所有文件。

## 当前实现与后续工作

三仓库的固定输入、普通六域生产、完整／紧凑组包、当前 `0.5.5.5` 本地采用与旧版恢复均已接通。
两 owner 发布索引和 PoBR latest 控制器已合并；默认构建仍重放锁，发布索引只负责发现候选。
自动采用还缺专用凭据与一次真实新发布的全链演练，owner 新原料生产也不由索引自动完成。
字段／语义和玩家流程的剩余范围统一维护在[数据现状](data-status.md)，
托管启用条件见[latest](../pipeline/latest/latest-data.md#verified-rollout-status-2026-11-02-utc)，不在本页复制阶段待办。

## 开发入口

- 数据版本选择、manifest 校验、身份摘要及快照生命周期见[构筑模型](build-model.md#数据快照身份与会话)。
- 当前／旧版的临时导入及完整身份校验见[构建数据](../pipeline/runtime/build-data.md)；候选适配、回滚保护和离线的 `mise run data:check` 见[数据管线](../pipeline/README.md)。
- 语言产物由翻译库生成并发布，PoBR 经[数据锁](../pipeline/data.lock.json)导入。代码合并不自动切换 CURRENT 或认证来源。
- 历史 Git 树的[独立输入包](../pipeline/tree/tree-inputs.md)只用于诊断候选，不能据其旧版本标签切换活动数据。
- 当前工具、缓存和验证范围见[开发工作流](development-workflow.md)。

本页说明当前目录职责；操作步骤和 schema 细节由上述文档分别维护。
