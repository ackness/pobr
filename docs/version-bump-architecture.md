# 数据版本与覆盖层

PoBR 将应用版本、基础库版本和游戏数据版本分开。RePoE 独立存储／处理游戏事实，翻译库独立存储／处理语言；PoBR 只跟踪代码、锁、来源元数据和自有规则。版本目录由 `mise run data:prepare` 按固定输入临时组合，供构建和实物测试使用，不提交完整快照或把旧 PoBR artifact 当回退。

数据目录的职责如下：

| 路径 | 职责 |
| --- | --- |
| `data/CURRENT` | 运行时默认使用的游戏数据版本 |
| `data/<version>/base/` | 临时导入并适配的该版本游戏数据 |
| `data/<version>/overlay/` | 该版本的补充与修正 |
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

## 开发入口

- 数据版本选择、manifest 校验、身份摘要及快照生命周期见[构筑模型](build-model.md#数据快照身份与会话)。
- 当前／旧版的临时导入及完整身份校验见[构建数据](../pipeline/build-data.md)；候选适配和回滚保护见[数据管线](../pipeline/README.md)。
- 语言生成由[独立 producer](../pipeline/locale-generation.md)完成，新目标需显式匹配的 generation lock；代码合并不自动切换 CURRENT 或认证来源。
- 当前工具、缓存和验证范围见[开发工作流](development-workflow.md)。

本页说明当前目录职责；操作步骤和 schema 细节由上述文档分别维护。
