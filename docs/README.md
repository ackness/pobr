# 文档目录

先按要做的事找文档。日常开发从[维护者指南](maintainer-guide.md)和
[AGENTS.md](../AGENTS.md)开始；提交要求见[贡献指南](../CONTRIBUTING.md)。

<a id="current-guides-and-contracts"></a>

## 当前指南

这些文档描述当前实现，功能改变时需要一起更新。

| 想了解什么 | 文档 |
| --- | --- |
| 本地开发、架构和验证要求 | [维护者指南](maintainer-guide.md) |
| 游戏规则如何计算、哪些地方用了假设 | [机制约定](mechanics/README.md) |
| CLI / WASM 返回什么，界面数值是什么意思 | [计算输出](calc-outputs.md) |
| BD、存档、数据快照和计算会话如何组织 | [构筑模型](build-model.md) |
| 还依赖 PoB2 的哪些数据，哪些结果有意不同 | [PoB2 差异](pob2-differences.md) |
| 计算不对、词条没生效时怎么查 | [诊断工具](diagnostics.md) |
| PoB2、WeGame、游戏规划器等格式能保留什么 | [导入格式](import-formats.md) |
| 测试、缓存、CI、部署和发版 | [开发工作流](development-workflow.md) |
| 改一条词条、一个计算字段或机制的具体步骤 | [常见改动步骤](recipes.md) |
| 添加或修复词条解析规则 | [词条指南](contributing-mods.md) |
| 界面操作、升级建议和 BD 管理的行为要求 | [Web 行为约定](web-contracts.md) |
| 市集特殊词缀从哪里来、怎么评分 | [市集特殊词缀](trade-special-affixes.md) |
| 珠宝搜索支持什么、有哪些限制 | [珠宝搜索](jewel-search.md) |
| WeGame 分享链接如何导入 | [WeGame 导入](wegame-import.md) |
| 如何使用独立 WASM 发布包 | [WASM 包](wasm-package.md) |
| 如何升级游戏数据版本 | [数据版本升级](version-bump-architecture.md) |
| 当前界面长什么样、截图怎么拍的 | [线上截图](screenshots/README.md) |

其他目录还有 [Web 安装指南](../web/README.zh-CN.md)、[数据管线](../pipeline/README.md)
和[宝石品质生成说明](../pipeline/gem-quality/README.md)。
`agent-docs/` 保存机制研究资料，结论需要结合当前游戏数据和测试核对。

<a id="dated-records"></a>

## 历史记录

以下是有日期的历史记录。它们保留当时的测量、排查结论和版本背景，不能当成当前版本的承诺。
先看文档顶部的日期、版本或提交，再决定能否用于眼前的问题。

| 记录 | 当时查了什么 |
| --- | --- |
| [WASM 性能（英文）](wasm-performance.md) / [中文](wasm-performance.zh-CN.md) | 2026-10-11 的耗时和体积；2026-10-12 补充测量、诊断和 PoB2 对照 |
| [0.5.5c 数据适配](adapting-to-0.5.5c.md) | 2026-09-26，数据版本 `4.5.5.3` |
| [0.5.5 数据适配](adapting-to-0.5.5.md) | 2026-09-11，数据版本 `4.5.5.2` |
| [Calcs 展示比较](calcs-display-comparison.md) | 2026-09-25，与 PoB2 计算面板的比较 |
| [参考样本与引擎检查](reference-sources-and-engine-review.md) | 2026-09-12，构筑参考来源及 PoB2 对照 |

历史文件保留原路径，代码注释和脚本仍会引用它们。
