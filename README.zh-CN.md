# PoBR — 流放之路 2 在线 BD 规划工具

[English](README.md) | **简体中文**

[![PoBR：轻量、快速的流放之路 2 在线 BD 规划工具。浏览器即开即用，比较装备与辅助宝石，规划天赋升级。基于 Rust 与 WebAssembly，开源免费。](.assets/pobr-preview-zh-CN.jpg)](https://pobr.win)

**换装备、搭辅助宝石、点天赋之前，先看看你的 BD 会提升多少。**

PoBR（Path of Building in Rust）是一款开源的 **流放之路 2（Path of Exile 2 / PoE2）
在线 BD 规划工具**。在浏览器里导入 PoB2 代码或 WeGame 分享链接，查看伤害与防御，
比较下一步升级。使用 Rust + WebAssembly 计算，支持简体中文、繁体中文和英文界面。

**[打开在线版](https://pobr.win)** ·
[Pages 备用地址](https://pobr-web.pages.dev) ·
[更新记录](https://github.com/ackness/pobr/releases) ·
[反馈问题](https://github.com/ackness/pobr/issues)

> **测试版：** 已可体验，游戏机制覆盖仍不完整。比较升级收益前，请核对未支持效果和战斗配置。

## 现在的界面

| 装备总览与实时角色属性 | 完整物品换装比较 |
| --- | --- |
| [![装备布局与实时伤害、防御、抗性数据](docs/screenshots/equipment.png)](docs/screenshots/equipment.png) | [![示例戒指与两个已装备戒指的换装比较](docs/screenshots/item-comparison.png)](docs/screenshots/item-comparison.png) |
| **查看技能组与辅助宝石** | **查看天赋树与已分配节点** |
| [![当前技能组与辅助宝石操作](docs/screenshots/support-upgrades.png)](docs/screenshots/support-upgrades.png) | [![当前天赋树与已分配节点](docs/screenshots/passive-tree.png)](docs/screenshots/passive-tree.png) |

截图于 2026 年 10 月 30 日通过 ego-lite 从线上 v0.0.43 拍摄，使用公开回归样本，
界面语言为简体中文。它们展示功能，不是 BD 推荐。
[截图来源和更多页面](docs/screenshots/README.md)。

## 你可以用它做什么

- **买装备前比较换装。** 粘贴从市集复制的完整物品，在适用的装备位置比较每秒伤害（DPS）、
  有效承伤量（EHP）与抗性变化。
- **为主技能挑辅助宝石。** 比较已计算的整套搭配，排除不想用的辅助，并预览等级、品质升级。
- **规划接下来的天赋点。** 寻找包含过路点成本的连通路线，预览节点变化，再决定是否应用。
- **按自己的 BD 搜装备。** 生成带词缀权重、角色等级要求和预算筛选的 PoE2 官方市集链接。
- **看懂数值来源。** 编辑装备、技能与战斗配置，同时查看伤害分解和词条来源。

## 用你的 BD 试一试

1. [打开在线版](https://pobr.win)，点击顶栏「BD 库」；空白概览也可以点击「导入或创建 BD」。
2. 粘贴 **PoB2 代码**或 **WeGame PoE2 分享链接**并导入。也支持其他已适配的构筑分享链接、
   游戏内规划器 / 国服的 `.build` 文件，或新建角色。
3. 核对主技能、战斗配置与未支持效果，再到「提升」「技能」或「天赋树」比较改动。

网站首页就是工作台。「设置」可以切换游戏风格、分组、紧凑三种显示方式，为单独页面指定样式，
或调整语言、主题和间距。显示方式不改变计算结果。

修改会自动保存在当前浏览器中，不同构筑和阶段分别保留。导入构筑代码默认新增构筑；
只有想覆盖当前阶段时，才将导入位置选为当前阶段。同一域名下的标签页会协调 BD 库的变化。
**`pobr.win` 和 `pobr-web.pages.dev` 的本地存档各自独立**，换域名时请用文件迁移。

你可以用 JSON 分享一个构筑及其全部阶段，也可以导出 PoB2 代码。下载工作区备份可保留
所有构筑、阶段和编辑器中尚未应用的草稿，方便留存或换设备。恢复工作区备份会在确认后
替换整个本地构筑集合，恢复其他备份前请先备份当前工作区。

不同导入格式携带的信息不同，游戏内规划器文件不包含完整角色装备。
[查看各导入格式能保留什么](docs/import-formats.md)。

PoBR 计算的是选定条件下的 BD，不会重放一场战斗，也不会自动推算技能循环、触发历史或增益覆盖率。
EHP 取决于配置的敌人和来伤，不能保证角色在任何场景下都能存活。
[机制与计算假设](docs/mechanics/README.md)说明了具体边界。

PoBR 是以 [Path of Building for PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
为计算参考的独立实现，尚未覆盖全部 PoB2 机制。推荐范围限于已计算的候选与已建模效果；
市集权重用于初筛，不保证找到最优购买方案，购买前请比较完整物品的换装收益。

## 参与改进与关注更新

发现计算差异或未支持词条？欢迎[提交 Issue](https://github.com/ackness/pobr/issues/new)，
附上可公开分享的最小复现构筑、相关物品或技能，以及预期值与实际值。界面反馈和翻译修正也很有帮助。

想参与开发，可以从[贡献指南](CONTRIBUTING.md)或[添加词条规则](docs/contributing-mods.md)开始。
如果 PoBR 对你有用，可以 **Star 收藏仓库**；希望收到发版通知，可选择 **Watch → Custom → Releases**。

## 开发者入口

Rust 计算引擎由 WebAssembly 应用和 CLI 共用，支持词条离线预编译，并通过 `TraceGraph`
和 `AttributionReport` 提供计算追踪与来源归因。计算使用稳定 ID，与显示翻译分离；游戏数据
使用版本化 JSON 快照。PoB2 parity 测试保护已记录的回归基线，不代表所有机制均已覆盖。

计算在浏览器内完成；WeGame 和市集 HTTP 适配由 Pages Worker 提供。
WASM/JSON API 与 CLI 仍在迭代，可能随版本变更。

### 本地运行

使用最新稳定版 [mise](https://mise.jdx.dev/getting-started.html) 安装并执行项目固定的工具。
在仓库根运行：

```bash
mise bootstrap --only packages,tools,task
mise exec -- cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
mise run verify build codec           # 按需定向测试与 lint
mise run targets                      # 只列测试目标，不编译
```

Web 的 WASM、数据准备与 Vite 启动见 [Web 开发说明](web/README.zh-CN.md)。
[开发工作流](docs/development-workflow.md)统一说明定向验证、CI、review 与发版；
可选性能测量和解读限制见 [WASM 包说明](docs/wasm-package.md#runtime-performance-comparison)。

Rust 使用 **edition 2024**；应用与工具继承 workspace 发布版本，七个内部库独立维护版本。

## 文档

| 想做什么 | 从这里开始 |
| --- | --- |
| 找文档 | [文档目录](docs/README.md) |
| 参与开发 | [贡献要求](CONTRIBUTING.md)、[AGENTS.md](AGENTS.md)、[维护者指南](docs/maintainer-guide.md) |
| 添加或修复词条 | [词条指南](docs/contributing-mods.md)、[常见改动步骤](docs/recipes.md) |
| 使用 JSON 接口 | [构筑模型](docs/build-model.md)、[计算输出](docs/calc-outputs.md)、[WASM 包](docs/wasm-package.md) |
| 核对机制 | [机制约定](docs/mechanics/README.md)、[有意保留的差异](docs/pob2-differences.md) |

工作区包含七个 Rust 库、两个应用和四个 Rust 工具，React / TypeScript 界面在 `web/`。
维护者指南说明了架构与数据流。`agent-docs/` 中的历史研究资料需要结合当前游戏数据和测试使用，
不能当作当前实现的说明。

## 参考与致谢

- [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)（MIT）提供部分参考数据，
  并用于回归比较。游戏数据和词条原文是规则依据；`vendor/` 中固定版本的 Lua 用于交叉核对，
  与游戏证据冲突时不默认 Lua 正确。[仍依赖哪些数据](docs/pob2-differences.md)。
- [poe2db.tw](https://poe2db.tw/) 与 PoE2 Wiki——游戏机制与文本翻译的查证来源。

## License

代码以 [MIT](LICENSE) 协议发布。

本项目与 Grinding Gear Games 无任何关联，亦未获其背书。`data/` 下的游戏数据
派生自 Path of Exile 2 客户端资源，版权归 Grinding Gear Games 所有，仅用于
构建计算的互操作目的（与 Path of Building 社区惯例一致）。
