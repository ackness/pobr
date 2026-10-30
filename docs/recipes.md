<a id="change-recipes"></a>

# 常见改动步骤

本文列出常见小改动要改的文件、要跑的检查，以及留给批次门禁的验证。先按这里操作，
不必从头搜索整个仓库。通用约定见[维护者指南](maintainer-guide.md)，机制规则见
[机制约定](mechanics/README.md)，词条语法见[词条贡献指南](contributing-mods.md)。

**验证预算：每项改动最多编译一个小测试目标。** 不逐项运行
`cargo test -p pobr-core`（全部目标）、`--test parity`、WASM/CLI 套件、
`mise run accuracy`、`triage`、`diagnose`、`compare` 或覆盖率脚本。
这些命令会编译较大的 parity 测试程序或 CLI，部分还会重算整组构筑。
合并者在多个改动合入后运行一次 `mise run gate`。

<a id="find-what-already-exists-no-compilation"></a>

## 查找已有实现（不编译）

| 要查什么 | 命令 |
|----------|------|
| 一行文本的游戏 stat id、完整原文、来源、逐行使用记录和附近规则 | `mise run gap "<line or template>"`；新 worktree 先设置下文的 `POBR_BIN` 和 `POBR_CACHE_DIR`，避免编译 CLI 或重算记录 |
| 游戏数据里的技能、stat、基底、节点或暗金 | `mise run lookup <id or name>` |
| 哪些代码读取 modifier 名称 `X` | `rg -n '"X"' crates/pobr-core/src crates/pobr-build/src`；常见读取形式是 `named("X")`、`sum(.., "X")`、`flag(.., "X")` |
| 类似文本是否已有规则 | `rg -n -i '<a distinctive word>' data/overlay-common/special_mods.json` |
| 技能 stat 是否已有含义映射 | `rg -n '<stat_id>' crates/pobr-core/src/rules/skill_stats.rs` |
| 有哪些测试套件 | `mise run targets` |

<a id="add-one-game-line-item-passive-jewel-flaskcharm-text"></a>

## 添加一行游戏词条（物品、天赋、珠宝、药剂或护符）

1. 按上表查游戏 stat id 和现有读取方。
2. 根据词条选择位置：

   | 情况 | 修改位置 |
   |------|----------|
   | 已有 modifier 名称，只需提供数值或条件 | `data/overlay-common/special_mods.json` 中的条目 |
   | 技能或辅助宝石 stat | `crates/pobr-core/src/rules/skill_stats.rs` 的 stat id → modifier 映射，并用其中的 `exporter_rule` 记录来源 |
   | JSON 无法表达的逻辑 | `crates/pobr-core/src/rules/handlers/` 中的 handler，并在条目中填写 `handler_id` |
   | 能识别，且不影响任何计算输出 | `"mods": []`、`"batch": "S0"` |
   | 能识别，但机制尚未实现 | `"mods": []`、`"batch": "unmodelled"`，用 `source_note` 说明缺少什么计算 |

3. 写入条目：`id`、`pattern`（小写、匹配整行的 Rust 正则）、`mods`、
   `batch`（新人工规则用 `S2`）、`verified: false`、`game_source`（游戏 stat id）、
   `exporter`（PoB2 未实现用 `"absent"`，行为不同用 `"differs"`，相同可省略）和
   `examples`（游戏原文）。`source_note` 记录可追溯的来源或缺失机制。
   下一步会逐条匹配 `examples`，因此它们就是解析回归样本。
4. 运行 `mise run regen`：校验规则与示例，重生成
   `data/<version>/generated/parsed_mods.json` 和 `parse-coverage.json`，再测试
   precompile-mods。提交变化的生成文件，不手工编辑它们。
5. 只有 modifier 名称没有读取方，或读取方需要新行为时，才添加读取代码，并在
   `crates/pobr-core/tests/<suite>/` 最近的现有文件中补一个引擎测试，运行
   `cargo test -q -p pobr-core --test <suite> <test name>`。
6. 词条引入新的约定、假设或尚未实测的游戏行为时，在对应的
   [docs/mechanics/](mechanics/README.md) 文件写一段说明。与 PoB2 有意不同且已定位到
   构筑数值的情况，写入 [pob2-differences.md](pob2-differences.md) 和
   `devs/scripts/known-differences.json`。

到这里即可提交。逐行使用记录、accuracy 和 parity 的变化由批次门禁统一测量。

<a id="add-one-calcs-display-field"></a>

## 添加一个 Calcs 展示字段

1. 数值必须已经存在于 `OutputTable`（`crates/pobr-core/src/calc/output.rs`），
   或能从计算阶段保留的中间项得到。不要重复计算已有数值。
2. 依次修改：
   - `crates/pobr-core/src/display_catalog.rs`：添加
     `computed("<Id>", Cat::…, Vt::…, "<PoB key>")` 和 `"<Id>" => output.<field>` 分支。
   - 明细：在 `crates/pobr-core/src/calc/session/detail_views.rs` 添加派生公式，并把 id
     加入 `DERIVED_STATS`；公式输入必须能重建结果。也可使用与原计算相同的 ModDb 查询
     提供来源分组。`pobr_build::breakdown::detail::detail_kind` 必须与之对应。
   - 标签：`crates/pobr-i18n/locales/en-US/stats.toml`、
     `crates/pobr-i18n/locales/zh-TW/stats.toml`、`web/src/lib/detailLabels.ts`。
   - 不固定 catalog 的条目数。现有检查会验证活动数据版本上的 id 唯一性、标签、数值和明细连接。
3. 运行 `cargo test -q -p pobr-core --test golden display_catalog`，并用一个引擎用例断言
   数值正确、派生输入能重建它。
4. 只用两三个构筑交叉核对 PoB2，并复用门禁已编译的 CLI：
   `target/debug/pobr calculate-build --file <code.txt> --json` 对照
   `.cache/pob2-oracle/<hash>.json` 的 `calcsOutput.<PoB key>`。
   完整 `output-coverage.py` 留给批次验证。

<a id="implement-a-missing-mechanic"></a>

## 实现缺失机制

先按[静态模拟的范围与取舍](maintainer-guide.md#静态模拟的范围与取舍)确认实际需求，
再按上面的步骤实现。依据来自 `data/<version>/base/` 的游戏 stat、常量、词条文本或官方
patch notes，并写入对应[机制文档](mechanics/README.md)。
PoB2（`vendor/`）只用于交叉核对，不引用其行号来解释游戏规则。
所有没有游戏内实测的假设都要明确写出。

<a id="batches-and-the-gate"></a>

## 批量改动与门禁

同一领域的多个条目放在一个分支，依次实现，每项只跑相关检查。
最后一次规则修改后统一运行一次 `mise run regen`，再提交。

合并者合入多个分支后运行一次批次门禁：

```bash
mise run gate                    # 运行全部步骤，最后列出失败项
mise run gate --only core,parity # 修复后只重跑失败项
```

<a id="working-in-a-new-worktree"></a>

## 在新 worktree 中工作

新 worktree 的 `target/` 和 `.cache/` 为空。第一次测试会重新编译依赖；分析工具
（`gap`、`gaps`、`triage`、`compare`、`config:coverage`、`skills:sweep`、
`output-coverage.py`）默认会准备自己的 CLI，并重算逐行使用记录和 PoB2 结果。
先指向已有缓存的 checkout：

```bash
mise run warm /path/to/warm/checkout                         # 首次运行：写时复制其 target/debug
export POBR_BIN=/path/to/warm/checkout/target/debug/pobr     # 分析工具使用现有 CLI，不另行编译
export POBR_CACHE_DIR=/path/to/warm/checkout/.cache          # 共用逐行记录和 oracle 结果
```

`POBR_BIN` 运行的是那份 checkout 的引擎。用它读取游戏数据、旧记录和 PoB2 对照值；
本 worktree 的修改由定向测试和合并后的门禁验证。
