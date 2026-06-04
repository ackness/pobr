# PoBR 数据管线（pipeline）

把 **GGG 官方游戏数据（`.dat` 表）** 抽取为 JSON，再适配为 PoBR 自有的最小 JSON schema
（落在仓库根的 `data/<poe-version>/`）。**不使用 PoB 的生成 Lua，不在仓库存放大体积原始数据。**

## 数据来源（真源）

| 域 | 真源 | 取法 |
|----|------|------|
| 物品基底 / 词缀 / Stat / 技能宝石 | 游戏 `Content.ggpk` 里的 `.dat` 表（GGG） | `pathofexile-dat` 按版本从 CDN 只下需要的表 bundle |
| 词条显示文本 | `Metadata/StatDescriptions/*.txt`（GGG） | 同上，作为 `files` 导出 |
| 被动天赋树 | GGG 官方 `github.com/grindinggear/poe2-skilltree-export` 的 `data.json` | 直接取 `data.json`（不取图集） |

`.dat` 存的是 **id / 数值 / 外键关系**（规范化数据库表）；显示文本在 `StatDescriptions` 里。
列名/表名见 [poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema)。

## 版本钉定

`config.json` 的 `"patch"` 钉定 PoE2 补丁版本（以 `4.` 开头 → 自动走 `patch-poe2.poecdn.com`）。
当前 PoE2 版本可向 GGG patch 协议服务器查询：

```bash
# patch.pathofexile2.com:13060，握手 [0x01,0x07]，返回形如 https://patch-poe2.poecdn.com/4.5.0.3.4/
node query-patch-version.mjs   # 见本目录
```

> **不需要下载完整的 `Content.ggpk`。** pathofexile-dat 只按 `config.json` 点名的表从 CDN 取对应 bundle。
> 也可在 `config.json` 用 `"steam"` 指向本地 PoE2 安装目录，完全离线。

## 运行（再生成数据）

```bash
cd pipeline
# 1) 预热索引缓存（弹性分块下载，规避大文件单流中断）：
node download-index.mjs
# 2) 抽取 .dat → 原始 JSON（产物在 ./tables/，已 gitignore）：
npx -y pathofexile-dat@15
# 3) 适配原始 JSON → PoBR 最小 JSON（落到 ../data/<version>/）：
cargo run -p pobr-data-adapter -- --raw ./tables --out ../data --patch <version>
```

`./.cache/`（~113MB bundle 索引）、`./tables/`、`./files/` 均为中间物，**已 gitignore，不入库**。
仓库只保存 `config.json`、脚本、本 README，以及第 3 步产出的 `data/<version>/*.json`（最小适配数据）。

## 扩展 / 升版

- 新 PoE2 版本：更新 `config.json` 的 `patch`，重跑三步，`data/` 下生成新版本目录，`diff` 审查。
- 新数据域：在 `config.json` 的 `tables` 增表/列，并在 `pobr-data-adapter` 增对应适配器。
