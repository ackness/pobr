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

## Vendor calc-delta 报告（`diff-vendor-calcs.sh`）

版本升级后，用它把「这次补丁改了哪些计算公式 / 数据」变成一份 triage 清单，
取代「翻黄金 → 看 parity 暴跌 → 逐 build 考古」的发现流程。

```bash
pipeline/diff-vendor-calcs.sh <old-sha> <new-sha> [--out <file>]
# 默认输出：devs/docs/audits/vendor-delta-<new-sha[:8]>.md
```

- 两个 vendor pin 各做一次 shallow git checkout 到 `.cache/vendor-delta/<sha>/`
  （gitignore，命中缓存即跳过；用 git checkout 而非 codeload tarball——headless
  抽取的 `HeadlessWrapper.lua` 引导需要完整工作树，tarball 缺文件会导致
  `modLib.parseMod missing`）。
- 报告三节：**Calc 模块 diff**（`Modules/Calc*.lua` + `ModParser.lua` 的 diffstat +
  折叠 hunks）、**数据模块 diff**（`Data/` + `Modules/Data*.lua` 只给按改动量排名的
  diffstat）、**抽取产物 diff**（对两个 pin 跑 `extract-lua --what
  special-mods|parser-rules|uniques`，对生成 JSON 做条目级 add/remove/change 汇总）。
- 软降级（沿用 `regen-all.sh` 的 `soft_step` 精神）：某个 `--what` 在旧 pin 上因
  结构不兼容跑失败时，报告注明缺失原因，脚本不中止。
- 旋钮：`MAX_HUNK_LINES`（单文件 hunk 超此行数只留 diffstat，默认 800）、
  `DATA_TOP`（数据文件排名条数，默认 40）。

## 扩展 / 升版

- 新 PoE2 版本：更新 `config.json` 的 `patch`，重跑三步，`data/` 下生成新版本目录，`diff` 审查。
- 新数据域：在 `config.json` 的 `tables` 增表/列，并在 `pobr-data-adapter` 增对应适配器。
- **CDN 只保留当前补丁**：GGG patch CDN 会下线旧版本（M1-W0 时 4.5.0.3.4 已 404）。`.cache/`
  里已缓存的 bundle 可继续离线导出**既有表的全部列**（整张 `.datc64` 在同一 bundle 里）；
  但**新增整表**若其 bundle 未缓存则无法补下——这类表记录在 `config.json` 的
  `_tablesUnavailableForPinnedPatch`，数据改走 `sync-pob-catalog extract-lua` 兜底
  （vendor Lua → `overlay/`），版本升级重下时再移回 `tables` 数组。

## 列名陷阱：社区 schema vs PoB2 spec.lua（M1-W0 2026-06-11 核验）

`pathofexile-dat` 用 [poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema) 的列名；
PoB2 `Export/spec.lua` 对**同一物理列**有不同命名。`config.json` 必须用社区名下载，
adapter 落库时按 PoB2 语义重命名。已核验对照（PoE2 段，validFor=2）：

| 表 | 社区 schema 列名（下载用） | PoB2 spec.lua 名（语义） |
|----|--------------------------|--------------------------|
| GrantedEffects | `SupportsGemsOnly` | `SupportGemsOnly`（多个 s） |
| GrantedEffects | `ExcludedActiveSkillTypes` | `ExcludeTypes` |
| GrantedEffects | `AllowedActiveSkillTypes` | `SupportTypes`（require 语义） |
| GrantedEffects | `AddedActiveSkillTypes` | `AddTypes` |
| GrantedEffects | `AdditionalStatSets` | 同名；**FK 目标是 GrantedEffectStatSets**（非 GrantedEffects） |
| GrantedEffectsPerLevel | `Reservation` | `SpiritReservation` |
| GrantedEffectsPerLevel | `EffectOnPlayer` | `ReservationMultiplier`（默认 100，已用全表默认值佐证） |
| GrantedEffectStatSets | `Label` | `LabelType`（FK → GrantedEffectLabels） |
| GrantedEffectStatSetsPerLevel | `SpellCritChance` | **`AttackCritChance`**（主暴击列，整体前移一位） |
| GrantedEffectStatSetsPerLevel | `AttackCritChance` | **`OffhandCritChance`**（副手覆盖列） |
| SkillGems | `ItemExperienceType` | `GemLevelProgression`（FK → ItemExperiencePerLevel） |
| ItemExperiencePerLevel | `ItemCurrentLevel` / `Level` | `Level` / `PlayerLevel` |

暴击两列的错位已用 overlay `skill_overrides.json` 的 201 条 crit_chance 全量对拍验证
（199 条直接命中 `SpellCritChance/100`；2 条位于 AdditionalStatSets 指向的附加 set，同列命中）。
另注：PoE2 的 `GrantedEffectsPerLevel` **没有** `PlayerLevelReq` 列（PoE1 才有）；等级需求
走 `SkillGems.ItemExperienceType → ItemExperiencePerLevel` 链（PoB2 `Export/Scripts/skills.lua:240`）。
