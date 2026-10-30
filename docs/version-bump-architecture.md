<a id="why-version-bumps-look-like-100k-line-changes-and-the-v003-plan-to-fix-it"></a>

# 数据升版为什么会有十万行 diff：v0.0.3 改造记录

本文保存 v0.0.3 的方案和交付记录，不是当前升版操作手册。
五项改造在 **v0.0.3-dev（2026-07-17）** 交付：

| 项目 | 交付位置 |
|------|----------|
| P0-1 | `pobr_gamedata::test_pins` 与 `data/<ver>/generated/test_pins.json`；历史确认入口 `POBR_BLESS_PINS=1`，后来已不再自动运行 |
| P0-2 | `pipeline/diff-vendor-calcs.sh`；回看 0.5.4b vendor 更新时，它直接指出 Mageblood 变化，无需逐构筑排查 parity |
| P1-3 | `data/overlay-common/special_mods.json`，由 `GameData::special_mods` 按 id 覆盖合并 |
| P1-4 | `pipeline/bump-version.sh` |
| P2-5 | `.gitattributes` |

2026-09-26 的实现补充：`bump-version.sh` 查询官方最新补丁，已经活动时直接退出。
品质平衡更新按稳定的效果 / stat ID 继承已确认决定，语义作用域变化仍需审查。
候选数据、审计和 parity 全部通过后才推进 `data/CURRENT`。
再生成使用独立候选目录、schema 3 文件哈希、严格原始列检查和导出来源记录；失败保留正式快照。
条目数 pins 和数值 golden 不自动确认或改写。基础数据 crate 不内嵌活动版本，运行时读取 CURRENT。
vendor 树由 `GameVersions.lua` 选择；物品授予的插槽从已加载树解析 id，不写死赛季节点号。
离线回归覆盖未知补丁版本、节点重编号和推进失败。

下文的自动确认 pins、vendor tarball 和验收顺序是**原方案**，不能直接当作现行行为。
当前命令、明确的 vendor / golden 决定和失败保护见 [pipeline/README.md](../pipeline/README.md)。
当前 `.gitattributes` 标记生成文件并关闭文本 diff，未设置原方案中的 `merge=ours`；
生成文件应重新生成，不能因此忽略合并检查。

<a id="1-anatomy-of-the-v001--v002-diff-the-evidence"></a>

## 1. v0.0.1 → v0.0.2 的 diff 构成

发布 diff 是 `672 files, +101,068 / -25,386`，拆分如下：

| 类别 | 行数 | 是否人工编写 | 是否由升版引起 |
|------|------|--------------|----------------|
| `data/*/generated/`（parsed_mods、special_vendor、coverage） | +91.8k / -14.5k | 否，`regen-all.sh` 产物 | 是 |
| 18 份 `meta.json` golden 重采 | ±1.8k | 否，`recapture_golden.py` 产物 | 是 |
| Web / WASM `build_api` 与前端 | 约 +5k | 是 | 否，同分支的功能开发 |
| 删除旧 mod-parser | -5k | 删除代码 | 否，同分支的清理 |
| overlay 人工规则（迁移 27 条 special_mods 并新增） | +1.2k | 是 | 是 |
| 数量断言和镜像测试修复（parser_rules 数量、覆盖率小数、灵体生命、tier 测试、precompile PATCH 等） | 约十个文件中的数十处 | 是 | 是 |
| 公式适配（Mageblood 约 300 行与六个 parity 修复提交） | 约 1–2k | 是 | 是 |

升版本身约需 **2–3k 行人工修改**，不是 10k 以上。
大 diff 主要来自提交的生成文件，以及混在发布分支上的其他功能和清理。
这 2–3k 人工成本仍会反复发生，需要分开处理公式变化与维护杂项。

<a id="2-root-causes"></a>

## 2. 当时的成本来源

<a id="2a-intrinsic-cost--engine-formula-drift-cannot-be-architected-away"></a>

### 2a. 游戏公式改变仍需实现

当时项目主要按 PoB2 计算行为迁移 Rust 实现。0.5.4b 改变闪避 / 护甲缩放、异常幅度，
PoB2 又加入 Mageblood legacy 模型；这些都需要人工适配。
公式改动越大，工作越多；纯数据补丁没有这部分公式工作。
这段描述是当时的工程背景，项目现行的独立实现方向见
[与 PoB2 的关系](maintainer-guide.md#与-pob2-的关系逐步独立)。

可减少的是发现问题的成本。当时先重采 golden，看到 parity 从 96% 降到 82%，
再逐构筑用 oracle 排查；Mageblood 就是这样找到的。应先生成变化清单，
在修改参考值前识别需要适配的计算。

<a id="2b-self-inflicted-cost--everything-else"></a>

### 2b. 可以消除的重复维护

1. 数据再生成就使固定数量 / 小数断言失败，例如 `real_data_section_counts`
   202/682→201/687、覆盖率 `0.801168`、灵体生命 3.0→2.2、precompile PATCH 和
   tier-degradation 版本字面量。逐处修改没有提供额外验证信息。
2. 27 条人工特殊规则曾从 `4.5.0.3.4` 手抄到 `4.5.4.3`。
   它们多是跨版本语义修正，应由共享目录继承。
3. `regen-all.sh` 已做抽取，但 `gen-zh-cn.mjs`、`web pnpm sync-data`、vendor tarball
   替换与 `.pob2-version.txt`、golden 重采、pins 修复仍靠手工步骤。
4. 91.8k 行生成 JSON 按普通代码显示，淹没需要审查的人工改动。

<a id="3-target-cost-model-the-contract-v003-ships"></a>

## 3. v0.0.3 的目标成本

| 补丁类型 | 原方案目标 |
|----------|------------|
| 纯数据，没有公式变化 | 一个命令、CI 通过、一次小范围审查；按小时而非按天计 |
| 平衡调整，公式或常量改变 | 一个命令和 vendor 变化清单；按清单修引擎，每项保留 parity 防退步检查 |
| 大版本，新增机制 | 按功能开发处理，不算作普通升版成本 |

<a id="4-v003-work-plan-ranked-by-leverage"></a>

## 4. v0.0.3 原方案

以下保留当时的具体计划；与后续实现不同的地方以开头说明和现行管线文档为准。

<a id="p0-1-kill-the-count-pins--snapshot-bless-instead-of-hand-pinned-literals"></a>

### P0-1：把数据数量断言移到每版快照

计划把精确数量和随数据变化的小数集中到
`data/<ver>/generated/test_pins.json`，用 `--bless` 生成，并作为 `regen-all.sh` 最后一步。
测试与快照比较；升版时只需审查一个生成文件，不再手改十处断言。
结构、schema、非空和只升不降的防退步基线仍写在测试代码里。
**自动 bless 是原计划，后来已取消。**

当时迁移清单：parser_rules `real_data_section_counts`、precompile 覆盖率基线、
precompile PATCH pin、灵体生命期望值、`mod_parser.rs` 任意属性期望值、
tier-degradation 版本字面量、依赖数据的 `contract_golden`。

<a id="p0-2-vendor-calc-delta-report--turn-engine-adaptation-into-a-checklist"></a>

### P0-2：用 vendor 计算差异生成排查清单

`pipeline/diff-vendor-calcs.sh <old-sha> <new-sha>` 检出两个固定版本，生成报告：

- `Modules/Calc{Offence,Defence,Perform,Setup}.lua`、`ModParser.lua`、
  `Modules/Data/` 的 `git diff --stat` 和修改片段，供核对 PoBR 对应实现。
- 分别运行 `extract-lua` 后比较 overlay JSON，列出新增或修改的 specialModList、解析规则和暗金。
- 报告放到 `devs/docs/audits/vendor-delta-<ver>.md`，作为第一阶段的引擎适配清单。

它包装已有 git diff 和抽取工具，不新增解析器。先读清单估算、排序，再决定如何重采 golden。

<a id="p1-3-version-independent-curated-overlay-dataoverlay-common"></a>

### P1-3：跨版本共享人工 overlay

`pobr-gamedata` 先加载 `data/overlay-common/`，再加载 `data/<ver>/overlay/`。
前者保存迁移的 27 条及后续人工语义修正，后者保存版本专用内容和大部分抽取产物。
新版本自动继承共享规则，仅真正随版本改变的修正放在版本目录。
原方案沿用“不再匹配新数据时给出警告”的处理；现行合并与严格检查见
[词条贡献指南](contributing-mods.md#7-validate-test-and-open-a-pr)。

<a id="p1-4-one-command-bump--pipelinebump-versionsh"></a>

### P1-4：统一升版脚本 `pipeline/bump-version.sh`

原计划把手工步骤串成：
`query-patch-version` → 配置 patch → `download-index` → `pathofexile-dat` →
vendor tarball 与 `.pob2-version.txt` → `regen-all.sh`（已有树快照继承、特殊规则、
预编译，加上原拟 P0-1 pins bless）→ `gen-zh-cn.mjs` → `web pnpm sync-data` →
`recapture_golden.py` → 定向检查（`driver.sh versions`、parity、gamedata）。
命令应可重复、分步骤恢复，复用 `soft_step`，最后打印人工决定清单。
重采参考值始终需要明确决定，不能自动接受新数值。

<a id="p2-5-mark-generated-files-in-gitattributes"></a>

### P2-5：标记生成文件

原方案拟使用 `data/**/generated/** linguist-generated -diff merge=ours`，
并类似处理 fixture `meta.json`，减少 PR 中约 90k 生成行的干扰。
实际属性没有采用 `merge=ours`；准确内容见根 `.gitattributes`。

<a id="explicitly-not-doing-yagni-revisit-only-with-evidence"></a>

### 当时未纳入的工作

- **把公式 / 常量全部移出 Rust。** 0.5.4b 多为闪避重做、Mageblood 模型等结构变化，
  常量文件不能省下实现；PoB2 也在 Lua 中写公式。某常量确实在至少两个补丁变过时再评估。
- **自动翻译 Lua 公式代码。** 这是研究项目；P0-2 的差异报告已能提供大部分发现价值
  （原方案估计为 90%，不是新的性能测量）。
- **停止提交生成数据。** 运行和部署需要它们，保留提交，用 `-diff` 降低审查噪声。

<a id="5-sequencing"></a>

## 5. 原验收顺序

P0-1、P0-2 独立，可任意顺序或并行做。P1-3、P1-4 接在 P0-1 后，P2-5 可随时做。
当时计划用下一个真实 GGG 补丁验收：运行 `bump-version.sh` 后，数据层人工 diff 应接近零，
引擎改动应能由差异报告逐项解释。这是当时的验收目标，不代表本文重新做过完整升版验证。
