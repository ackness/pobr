# 文档导航

按你要完成的工作选择入口。这里列出当前可用的说明和契约；具体命令以
`mise.toml`、接口类型与回归测试为准。

<a id="current-guides-and-contracts"></a>

## 使用 PoBR

| 想做什么 | 入口 |
| --- | --- |
| 了解功能、导入自己的 BD | [项目介绍（中文）](../README.zh-CN.md) / [English](../README.md) |
| 在本地运行 Web 应用 | [Web 安装与使用（中文）](../web/README.zh-CN.md) / [English](../web/README.md) |
| 了解不同导入格式保留什么 | [导入格式](import-formats.md)、[WeGame 分享链接](wegame-import.md) |
| 理解换装、市集与珠宝搜索 | [Web 功能约定](web-contracts.md#upgrade-guidance)、[市集词缀](trade-special-affixes.md)、[珠宝搜索](jewel-search.md) |
| 查看界面与示例输入 | [截图与来源](screenshots/README.md) |
| 报告问题或贡献改动 | [贡献指南](../CONTRIBUTING.md)、[安全报告](../SECURITY.md) |

## 集成计算引擎

| 想了解什么 | 文档 |
| --- | --- |
| 使用独立 WASM 包 | [下载、校验、运行与测量](wasm-package.md) |
| Build、导入、存档、数据快照和计算会话 | [构筑模型](build-model.md) |
| JSON / CLI 输出、单位与解释字段 | [计算输出](calc-outputs.md) |
| 页面操作、持久化与 API 消费约定 | [Web 契约](web-contracts.md) |
| 当前支持的计算范围和假设 | [机制约定](mechanics/README.md)、[PoB2 差异](pob2-differences.md) |

## 开发与维护

从[贡献指南](../CONTRIBUTING.md)开始；架构、依赖方向和跨主题约定在
[维护者指南](maintainer-guide.md)，给代码代理的简短入口是 [AGENTS.md](../AGENTS.md)。

| 工作 | 文档 |
| --- | --- |
| 环境、定向测试、缓存、CI、review 与发版 | [开发工作流](development-workflow.md) |
| 改一条词条、一个输出或一种机制 | [改动配方](recipes.md)、[词条规则指南](contributing-mods.md) |
| 排查数值差异、输入损失和未支持效果 | [诊断与逐行台账](diagnostics.md) |
| 升级游戏数据、候选校验与生成来源 | [数据管线](../pipeline/README.md)、[数据层结构](version-bump-architecture.md)、[宝石品质](../pipeline/gem-quality/README.md) |
| 使用参考引擎与抽取工具 | [PoB2 oracle](../tools/pob2-oracle/README.md)、[catalog 同步](../tools/sync-pob-catalog/README.md) |
| 测量性能或发布包体积 | [按需性能检查](development-workflow.md#慢构建与按需报告)、[WASM 测量](wasm-package.md#runtime-performance-comparison) |

## 资料与文档维护

[机制背景资料](../agent-docs/README.md)保留源码和测试引用的早期推导与出处，
不作为当前行为的契约。当前实现先看 `docs/mechanics/`、游戏数据和回归测试。

当前约定按主题维护；行为改变时直接修正对应段落。个人调研、实施计划和单机测量留在
忽略的本地目录，避免与开发者需要的当前说明混在一起。
