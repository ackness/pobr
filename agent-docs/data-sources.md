# POE2 资料查询资源指南

本文档汇总了 Path of Exile 2 各类游戏资料的查询网站与工具，按类别整理以便快速定位所需信息。

## 官方资源

| 网站 | 内容 | 语言 |
|------|------|------|
| [Path of Exile 2 官网](https://pathofexile2.com/) | 官方公告、补丁说明、联赛信息、商城 | 英文 |
| [官方论坛](https://www.pathofexile.com/forum) | 开发者更新、补丁讨论、Bug 报告 | 英文 |
| [官方 Discord](https://discord.gg/pathofexile) | 实时社区讨论、官方公告 | 英文 |

> **提示**：官方补丁说明是机制变更的最权威来源，建议在查阅社区资料前先确认官方补丁内容[^maxroll-050-patchnotes]。

## Wiki 与数据库

| 网站 | 内容 | 语言 |
|------|------|------|
| [PoE2 Wiki (poe2wiki.net)](https://www.poe2wiki.net/wiki/Path_of_Exile_2_Wiki) | 综合 Wiki：物品、技能、机制、任务、地图 | 英文 |
| [PoE2DB (poe2db.tw)](https://poe2db.tw/) | 数据库：物品、技能、天赋树、通货 | 中文/英文 |
| [Fextralife Wiki](https://pathofexile2.wiki.fextralife.com/) | 综合 Wiki：攻略、Build、物品、地图 | 英文 |
| [PoEDB](https://poedb.tw/) | 数据驱动的数据库：掉落、词缀、怪物 | 中文/英文 |

> **区别**：poe2wiki.net 更偏向机制文档，poe2db.tw 更偏向可搜索的数据库，poedb.tw 更偏向原始数据提取[^poe2wiki-home][^poe2db-home]。

## 构建规划与伤害计算

| 网站/工具 | 内容 | 说明 |
|-----------|------|------|
| [Path of Building PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) | PoE2 离线构建规划工具 | PoBR 计算 parity、QuestRewards、Build Code、UI 输出字段的主参考[^pob-poe2-repo] |
| [Path of Building Community](https://github.com/PathOfBuildingCommunity/PathOfBuilding) | PoE1 离线构建规划工具 | 仅作为历史架构和 PoE1 差异参考[^pob-repo] |
| [Maxroll Build Guides](https://maxroll.gg/poe2/build-guides) | 各职业/升华 Build 指南 | 含装备、天赋、技能配置 |
| [Mobalytics Guides](https://mobalytics.gg/poe-2/guides) | 机制讲解、Build 指南、通货攻略 | 深度机制文章较多[^mobalytics-home] |

> **与 PoB-PoE2 的核对**：实现计算、导入导出、章节奖励和展示字段时优先看 PathOfBuildingCommunity/PathOfBuilding-PoE2 的 `src/Modules/CalcSetup.lua`、`CalcPerform.lua`、`ConfigOptions.lua` 和 `src/Data/QuestRewards.lua`。PathOfBuildingCommunity/PathOfBuilding 是 PoE1 版本，适合参考架构，不适合作为 PoE2 数值最终基线[^pob-poe2-deepwiki][^pob-deepwiki]。

## 通货与经济

| 网站 | 内容 |
|------|------|
| [Currency.poe2.trade](https://www.pathofexile.com/trade) | 官方交易网站：通货、装备搜索 |
| [poe.ninja](https://poe.ninja/) | 经济数据分析：通货比率、热门 Build、物品价格 | 
| [Maxroll Currency](https://maxroll.gg/poe2/resources) | 通货使用建议、稀有度分级 |

## 技能与宝石数据

| 网站 | 内容 |
|------|------|
| [poe2db.tw 技能数据库](https://poe2db.tw/us/SkillGems) | 技能宝石基础属性、等级成长 |
| [poe2wiki.net Gems](https://www.poe2wiki.net/wiki/Spirit_gem) | 宝石类型、机制说明 |
| [SkyCoach Gems Guide](https://skycoach.gg/blog/path-of-exile-2/articles/gems-guide) | 宝石系统入门指南[^skycoach-gems] |

## 天赋/被动树

| 网站 | 内容 |
|------|------|
| [PoE2DB 天赋树](https://poe2db.tw/) | 可交互的被动天赋树（含搜索） |
| [poe2passive.com](https://poe2passive.com/) | 被动树模拟器、路径规划 |

## 游戏数据解包

| 工具 | 用途 | GitHub |
|------|------|--------|
| ggpk-tool | 现代化的 POE2 GGPK 提取与解析 | [juddisjudd/ggpk-tool](https://github.com/juddisjudd/ggpk-tool) |
| VisualGGPK2 | 图形界面浏览/导出 GGPK 内容 | [aianlinb/VisualGGPK2](https://github.com/aianlinb/VisualGGPK2) |
| ggpkviewer | Rust 实现的 GGPK 解析工具集 | [shadr/ggpkviewer](https://github.com/shadr/ggpkviewer) |
| PoET | Python 命令行提取工具 | [jcmoyer/PoET](https://github.com/jcmoyer/PoET) |

> 详细使用方法与 .dat 文件解析说明见 [content-ggpk.md](content-ggpk.md)。

## 社区与攻略

| 平台 | 内容 |
|------|------|
| [Reddit r/PathOfExile2](https://www.reddit.com/r/PathOfExile2/) | 社区讨论、攻略分享、问题解答 |
| [Maxroll.gg](https://maxroll.gg/poe2) | 新闻、补丁说明、机制讲解、Build 指南[^maxroll-home] |
| [YouTube / Twitch](https://www.twitch.tv/directory/game/Path%20of%20Exile%202) | 实况、Build 展示、联赛开荒 |
| [Sportskeeda Guides](https://www.sportskeeda.com/mmo) | 防御/抗性/属性入门指南[^sportskeeda-defense] |

## 数据应用方向

不同资源适合解决不同类型的问题：

- **查机制/公式** → poe2wiki.net、Mobalytics 深度文章、官方补丁说明
- **查具体数值** → poe2db.tw、poedb.tw、解包后的 .dat 文件
- **规划 Build / 核对输出** → Path of Building PoE2、Maxroll Build Guides、PoE2DB 天赋树
- **查通货价值** → poe.ninja、官方 trade 网站
- **获取原始数据** → ggpk-tool 解包、社区维护的 schema

---

## 参考来源

[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^poe2wiki-home]: PoE2 Wiki. https://www.poe2wiki.net/wiki/Path_of_Exile_2_Wiki
[^poe2db-home]: PoE2DB. https://poe2db.tw/
[^mobalytics-home]: Mobalytics — PoE 2 Guides. https://mobalytics.gg/poe-2/guides
[^pob-poe2-repo]: PathOfBuildingCommunity/PathOfBuilding-PoE2. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^pob-poe2-deepwiki]: PathOfBuildingCommunity/PathOfBuilding-PoE2 DeepWiki. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^pob-repo]: PathOfBuildingCommunity/PathOfBuilding. https://github.com/PathOfBuildingCommunity/PathOfBuilding
[^pob-deepwiki]: Path of Building DeepWiki — Calculation Engine. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding
[^skycoach-gems]: SkyCoach — PoE 2 Gems Guide. https://skycoach.gg/blog/path-of-exile-2/articles/gems-guide
[^sportskeeda-defense]: Sportskeeda — PoE 2 Defense Guide. https://www.sportskeeda.com/mmo/exile-2-poe2-defense-resistance-guide-energy-shield-armor-evasion
[^maxroll-home]: Maxroll — Path of Exile 2. https://maxroll.gg/poe2
