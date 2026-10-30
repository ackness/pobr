<a id="reference-sources-and-pob2-alignment-review"></a>

# 构筑参考来源与 PoB2 对齐检查（2026-09-12）

本文保留 2026-09-12 的资料检查和测试结果。下文的“最新”“活动版本”和页面行为均指当时，
不表示今天仍然如此。现行功能见 [Web 契约](web-contracts.md)，数据规则见
[维护者指南](maintainer-guide.md)。

本次暂时隐藏构筑参考入口，保留组件、匹配逻辑、公开历史样本和目录生成器。
未来玩家分享可复用 `BuildReference` 与现有装备比较流程；没有新增分享服务或上游接入。

<a id="candidate-data-sources"></a>

## 候选数据来源

| 来源 | 当时核验到的内容 | 对 PoBR 的用途 |
|------|----------------|----------------|
| [Mobalytics PoE2](https://mobalytics.gg/poe-2) | 0.5.5 指南、创作者构筑和规划器。[公开 widget 文档](https://mobalytics.gg/builds-widget-documentation)介绍 LoL 英雄与位置，不是 PoE2。 | 可以链接指南；本次没找到面向第三方的 PoE2 build API 文档，应先取得受支持的接入方式，再使用内部请求。 |
| [RePoE PoE2 导出](https://repoe-fork.github.io/poe2/) | `4.5.5.2` 的词缀、基底、技能、宝石、augments、stat 翻译 JSON。[项目说明](https://repoe-fork.github.io/)面向工具开发者，也链接 PoB 数据导出。 | 可离线核对数据或补充未来缺失的数据域；它是游戏定义，不是当季玩家构筑目录。保留现有 GGG 抽取流程与明确版本。 |
| [PoE2DB](https://poe2db.tw/us/) | 补丁、物品、词条和机制参考页；本次查看的页面与搜索中未发现公开第三方 API 文档。 | 人工交叉核对和来源链接；未提议自动导入构筑。 |
| [poe.ninja API](https://poe.ninja/docs/api) | 公开经济接口；构筑、角色和 PoB 接口明确标为内部使用，不开放第三方接入。 | 本次不采用自动构筑同步，也不采用定时离线抓取。 |

PoE2 Wiki 的 API sandbox 当时遇到反机器人验证，因此没有确认其程序访问能力。
未绕过验证，也未逆向内部接口。

未来分享构筑应保留来源、赛季 / 模式、数据版本和更新时间。
相似度只在明确选择的兼容参考组中比较；不能把旧样本得分显示成当季玩家排名。
这是当时记录的产品约束，不是新增后端契约。

<a id="current-upstream-alignment"></a>

## 当时的上游对齐情况

- `git ls-remote` 查询的上游 `HEAD` 与 `dev` 均为
  [`ce566eac45ea8a86477f513c7ee65a1ebe60014e`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/ce566eac45ea8a86477f513c7ee65a1ebe60014e)
  （9 月 10 日）。本地 vendor、活动解析器与技能 overlay 已使用该提交。
- GGG patch 协议返回 `4.5.5.2`，与当时的 `data/CURRENT` 和活动数据一致；RePoE 也列出同版本。
- [0.5.5 更新](adapting-to-0.5.5.md)已处理当时 Lua 导出的抽取兼容。
  本次查询没有可导入的更新 vendor 或游戏数据；保留 vendor 的本地运行时 shim。
- `overlay/special_mods.json` 是人工规则，保留历史来源；自动抽取位于
  `generated/special_vendor.json`，固定为 `ce566eac`。
  不能只凭人工层的旧来源字段，就判断漏了再生成。

<a id="calculation-checks-and-their-limits"></a>

## 当时的计算检查与边界

之后仓库已取消独立 golden 数据版本，测试和 PoB2 参考值跟随活动版本，
只保留活动快照与最近入库的两个版本。以下仍是本次原始结果，不能算作现行版本的验证。

未修改公式或数据，定向检查结果如下：

- `cargo test -p pobr-build --test parity ninja_parity:: -- --nocapture`：
  七项通过，含防御 / 攻击 / DoT 回归和未支持词条报告。
  **当时**有意使用 golden 数据 `4.5.4.8`，不是活动的 0.5.5 数据。
- `cargo test -p pobr-build --test parity special_parsemod_differential -- --ignored --nocapture`：
  真实 PoB2 parser 成功运行；版本专用人工层中七个可实例化样本的 modifier 名称集合一致。
  不覆盖完整共享 / 生成规则库，也不证明数值、条件或标签等价。
- `cargo test -p pobr-build --test parity multi_version:: -- --nocapture`：
  两项通过，覆盖当时五个已提交数据集（含 `4.5.5.2`）。只检查输出合理性与确定性，
  不证明活动版本的数值 parity。

本次有限检查未确认新的计算错误。未支持、部分解析、单独处理及没有 oracle 结论的条目
仍需保留诊断。回归通过或 JSON 更新不代表所有机制已实现。
后续数值修复应从可重现输入和同版本 PoB2 对照开始。

<a id="frontend-changes-and-validation"></a>

## 当时的前端改动与验证

- 导航和已保存标签恢复不再进入历史构筑参考；已有存档不变。
- 主内容最大宽度增至 2400px。视口至少 1800px 时，技能分两栏，装备增加槽位列数，
  计算区按可用宽度调整；窄屏规则保留。
- Web 构建和八个匹配单测通过。相关 Playwright 运行通过 17 项，覆盖
  320 / 768 / 1024 / 1440 / 1920 / 2560 / 3440px、有内容的流程、标签恢复和导航。
  参考功能隐藏期间，保留的六个面板流程显式跳过。
- 人工查看了宽屏、有内容的技能、装备和计算页面。
  未改 Rust / WASM 契约，未运行完整 workspace 或发布门禁。
