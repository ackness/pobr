# PoE1 兼容代码移除 + PoB2 缺口审计（2026-06-10）

> 决策：本项目仅面向 **PoE2（0.5.0）**。不再为 PoE1 机制付出兼容成本。
> 本目录记录本次「移除 PoE1 兼容」的范围与依据，并附 PoB2 后续实现计划。

## 1. 背景与方法

多 agent 并行排查（33 个子 agent，三阶段）：

1. **Map-PoE1** — 分区扫描 build/xml、data/constants、ailment/crit、offence/defence、parser、tests 六个区域；
2. **Verify-PoE1** — 对每个移除候选交叉验证 `vendor/PathOfBuilding-PoE2`（PoB2 上游 Lua），判定 `safe-remove` / `remove-with-care` / `keep` / `needs-poe2-data-first`；
3. **Gaps-PoB2** — 对照 vendor `CalcOffence/Defence/Perform/ActiveSkill/Setup/Triggers/Mirages` 找未实现特性（见 [`pob2-implementation-plan.md`](./pob2-implementation-plan.md)）。

**关键结论**：计算引擎本身**早已是 PoE2-only**——offence/defence/crit/ailment 区域只找到「对照 PoE1 差异」的文档注释（保留，属有用文档），无任何真正运行的 PoE1 公式分支。PoE1 残留高度集中在两组。

## 2. 本次移除范围

### G1 — GameVersion / PoE1 XML 识别机制（彻底移除）

PoB2 上游（`Build.lua`）只接受 `PathOfBuilding2` 根元素，对旧的 `PathOfBuilding`（PoE1）根**直接拒绝**，无任何向后兼容。据此彻底移除版本区分机制：

| 文件 | 移除内容 |
|------|----------|
| `crates/pobr-data/src/build_config.rs` | 删除整个 `GameVersion` 枚举（`Poe1`/`Poe2`） |
| `crates/pobr-build/src/xml_serde.rs` | 删除 `ParsedBuildHeader.pob_major` 字段；根元素改为**仅接受 `PathOfBuilding2`**，PoE1 根 → `XmlError::NotPobRoot` |
| `crates/pobr-build/src/xml_build.rs` | 删除 `pob_major==1 → Poe1` 分支与 `with_game_version` 调用、import |
| `crates/pobr-build/src/build.rs` | 删除 `Build.game_version` 字段 + `with_game_version()` |
| `crates/pobr-build/src/build_config.rs` | 删除 `BuildConfig.game_version` 字段 + `with_game_version()` |
| `crates/pobr-build/src/snapshot.rs` | 删除 `game_version_tag` 字段/函数及其内容哈希分量 |
| `apps/pobr-cli/src/lib.rs` | 删除 `BuildSummary.game_version` 输出字段 |
| 测试 | `poe1_root_recognized`/`poe1_root_maps_to_poe1_version` 改为断言 PoE1 根被拒绝；清理引用 `with_game_version`/`pob_major`/`game_version` 的断言 |

> 影响面：snapshot 内容哈希不再含 version tag → 缓存 key 变化（仅影响计算缓存命中，不影响结果正确性）。

### G2 — 法术压制 Spell Suppression（彻底移除）

PoE2 普通构建已移除法术压制（`agent-docs/active-defences.md §六`、`block.md §法术压制`）。PoB2 仅为导入旧 build round-trip 保留 inert 计算；本项目 PoE2-only，无需保留。

| 文件 | 移除内容 |
|------|----------|
| `crates/pobr-core/src/calc/survivability.rs` | 删除 `suppression_chance()` 函数 + 文档提及 |
| `crates/pobr-core/src/calc/mod.rs` | 删除 `suppression_chance` 重导出 |
| `crates/pobr-core/src/calc/output.rs` | 删除 `OutputTable.spell_suppression_chance` 字段 + 默认值 |
| `crates/pobr-core/src/calc/perform.rs` | 删除填充 `spell_suppression_chance` 的代码块 |
| `crates/pobr-core/src/display_catalog.rs` | 删除 `SpellSuppressionChance` 展示目录项 + 取值映射 |
| `crates/pobr-i18n/locales/{en-US,zh-TW}/stats.toml` | 删除 `SpellSuppressionChance` 文本键（两侧同步以满足 i18n lint） |
| 测试 | `survivability.rs::suppression_chance_clamps_at_100` 删除；`perform_fill.rs` 测试改名 `perform_fills_block_chance` 并去掉压制断言 |

> `SpellSuppressionChance` 词条文本仍能被 `mod_parser` 解析进 ModDb（不报错），只是不再展示/计算——导入含该词条的旧 build 不会崩溃。

## 3. 明确**保留**（不在本次移除）

| 项 | 位置 | 理由（来自 verify 阶段） |
|----|------|------|
| Boss 护甲/闪避占位常量 `PINNACLE/UBER_ARMOUR/EVASION_MEAN` | `crates/pobr-data/src/monster.rs` | `needs-poe2-data-first`：这是 **PoE2 占位数据**（PoB2 自身 `Bosses.lua` 当前仍沿用同一 PoE1 遗留 boss 名单计算这些均值）。删除会让敌方档位防御计算缺数据。待 GGG/PoB2 发布正式 PoE2 boss 名单后按新数据重算，而非现在删。 |
| PoE1↔PoE2 差异对照注释 | `constants.rs`、`ailment.rs`、`defence.rs` 等 | 这些注释（如「护甲系数 *5 vs *10」「爆伤 +50% vs +100%」）是**有价值的 PoE2 设计依据文档**，非可执行的兼容代码。 |
| `spell_block_chance` 字段/机制 | `output.rs` 等 | 法术格挡在 PoE2 仍存在，非 PoE1 残留。 |

## 4. 验证（worktree `chore/drop-poe1-compat`）

全部 4 个 CI gate 通过：

- `cargo fmt --check` ✓
- `cargo clippy --workspace --all-targets -- -D warnings` ✓
- `cargo test --workspace` ✓（全绿，无 failed）
- `cargo run -p lint-i18n` ✓（`OK (no extra keys)`）
