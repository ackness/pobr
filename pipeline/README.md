# PoBR 数据管线（pipeline）

把 **GGG 官方游戏数据（`.dat` 表）** 抽取为 JSON，再适配为 PoBR 自有的最小 JSON schema
（落在仓库根的 `data/<poe-version>/`）。GGG 数据提供物品与技能的原始定义，
固定提交的 PoB2 Lua 提供解析规则、机制映射及补充目录；不在仓库存放大体积原始数据。

## 数据来源（真源）

| 域 | 真源 | 取法 |
|----|------|------|
| 物品基底 / 词缀 / Stat / 技能宝石 | 游戏 `Content.ggpk` 里的 `.dat` 表（GGG） | `pathofexile-dat` 按版本从 CDN 只下需要的表 bundle |
| 宝石品质（从 `4.5.5.2` 起） | GGG `GrantedEffectQualityStats` / `GrantedEffects` / `Stats`，范围由版本收据固定 | `pobr-data-adapter --gem-quality`，见[品质生成说明](gem-quality/README.md) |
| 词条显示文本 | `Metadata/StatDescriptions/*.txt`（GGG） | 同上，作为 `files` 导出 |
| 被动天赋树 | GGG 官方 `github.com/grindinggear/poe2-skilltree-export` 的 `data.json` | 直接取 `data.json`（不取图集） |

`.dat` 存的是 **id / 数值 / 外键关系**（规范化数据库表）；显示文本在 `StatDescriptions` 里。
列名/表名见 [poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema)。

## 词条更新与回归检查

普通解析修复或同步当前固定版本的 PoB2 规则：

```bash
bash pipeline/refresh-modifiers.sh
```

命令依次提取普通规则与 `specialModList`、用完整来源语料对照 PoB2、检查上一份审计，
最后更新预解析缓存。它不下载游戏数据、不切换游戏版本，也不修改数值 golden。
只检查已有数据和引擎时用 `--audit-only`；没有 vendor 时用 `--offline`，报告明确不含 PoB2 对照。
可用 `--data data/<version>` 指定数据，`--baseline <audit.json>` 指定历史审计。

语料来源包括 StatDescriptions 的稳定 stat ID、装备/珠宝/药剂/护符词缀、特殊制作词缀、
基底隐式、传奇装备各变体、天赋树，以及中文导入实际输出的英文模板。
数值范围抽取低/高样本，复合描述使用不同的占位值；无法渲染的条目另计数。
这是**解析样本覆盖率**，包含地图和怪物描述，不能当作玩家机制或 DPS 覆盖率。

产物：

- `data/<version>/generated/modifier-audit.json`：可重现的完整审计与来源，作为下一次比较的快照；不进入 Web 下载清单。
- `.cache/modifier-audit/<version>/current.delta.json`：退化、新增缺口、已解决和移除条目。
- 同目录的 `corpus.txt` / `oracle.jsonl`：本次 PoB2 对照的输入与输出。

报告中的 `pobr_gap` 表示 PoB2 完整解析而 PoBR 未完整解析，优先检查提取器、枚举、标签映射。
`upstream_gap` 表示 PoB2 也不能完整解析，需查游戏机制并实现对应计算，不能靠空规则消除提示。
`recognized_empty` 单列零 modifier 的规则；有残余文本或丢失条件标签也不计为完整支持。
没有 oracle 时缺口为 `uncompared_gap`。`mod_names` 便于定位计算消费者，但识别出名称不证明消费者已实现。

以前完整解析的文本退化，或同一来源移除旧措辞后新增无法解析的措辞，会使命令失败。
失败时保留 `.cache` 报告与待检查的规则改动，**不覆盖上一份已通过的审计快照**；新增机制缺口单独列出。
既有示例角色覆盖率及数值 parity 门禁仍独立保留。

PoB2 本身把两层工作分开：`Data/StatDescriptions` 等从游戏数据导出文本，
`Modules/ModParser.lua` 将文本映射到 modifier，`Modules/Calc*.lua` 消费这些 modifier。
PoBR 沿用这个边界：提取器自动吸收可表达的规则，人工修正放在 `data/overlay-common/special_mods.json`，
更新数据后自动继承；真正的新机制仍需要实现和数值回归测试。

### 中文词典来源

中文导入使用 [poe2-en-cn-dict](https://github.com/addohm/poe2-en-cn-dict) 的游戏文本对照。
普通再生成复用 `_meta.json` 中的固定提交；主动更新或精确重放：

```bash
node pipeline/gen-zh-cn.mjs --refresh
node pipeline/gen-zh-cn.mjs --version <version> --ref <full-commit-sha>
```

所有文件从同一提交下载，下载完整后才启用缓存；失败不会混入半份新版词典。
离线输入可用 `--dict <directory>`，其元数据明确没有已验证的上游提交。
`bump-version.sh` 在联网升级时刷新词典，并在词典生成和完整词条审计通过后才推进活动版本标记。
词典缓存与下载失败测试运行 `node --test pipeline/test-dictionary-source.mjs`，已接入 CI；
升级失败保护由 `python3 devs/scripts/test_workflows.py` 覆盖。

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
仓库保存配置、脚本、说明、受审的来源收据，以及产出的 `data/<version>/` 最小适配数据。
上面的 `--raw` 命令不生成品质域；品质单独使用[独立生成命令](gem-quality/README.md#独立生成)。

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

- 新 PoE2 版本：**`pipeline/bump-version.sh` 一条命令**（查补丁号 → 下载 → 树/vendor 对齐 →
  regen-all（含 test-pin bless）→ 推进 CURRENT/DATA_VERSION → zh-CN/web 同步 → 定向验证），
  末尾打印剩余人工决策（golden 翻转、引擎 delta triage）。分步等价操作见脚本头注释。
  品质域要求预先准备该版本的[来源收据](gem-quality/README.md#更新版本或原始表)；
  缺失或输入字节变化时会在 regen-all 中止，核对完成后用 `--patch <version> --skip-download` 重跑。
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
