# M6 D-T8 第二波 2b 切换——就绪复核与阻塞项（owner 审查）

> 基线：`bb9b179`（fix(m6-regen) 合并后）。状态：**未切换、parity 未动、未 bump、未改任何生产代码**。
>
> 本波（2b）动手前对「2b 切换就绪清单」做实现级复核，发现清单**遗漏一类硬约束**，
> 使「默认开 parser-engine + 五调用方注入 + 删 legacy」无法在不引入 **owner 级架构
> 决策**的前提下达成。引擎本身已验证正确（C1 DIFF=0 gate 在本 worktree 实跑 PASS），
> 阻塞**不在引擎**，而在调用图与 pobr-core 零 I/O 铁律的冲突。逐项如下，请 owner 拍板后续。

## 0. 已验证（无疑项）

- `cargo nextest run -p pobr-core --features parser-engine --test parser_dual_run -E
  'test(c1_diff_zero_gate)'` → **PASS**：引擎对 18-build 语料 + fixture 与 legacy 逐字节一致。
- 引擎是数据驱动 `parse_mod_engine(text, &CompiledParserRules)`，规则来自
  `data/.../overlay/mod_parser_rules.json` + special 边车，gamedata loader
  (`mod_parser_rules()`) 已存在。

## 1. 阻塞项 A：零 I/O 墙——无参 `parse_mod(text)` 删不掉、也无干净替身

`crates/pobr-core` 是零 I/O crate（CLAUDE.md「I/O 收口在 pobr-gamedata 一处；pobr-core
维持零 I/O」），且 `pobr-data`/`pobr-core` **本体不依赖 `serde_json`**（pobr-data
Cargo.toml 明注「crate 本体保持零 I/O、不依赖 serde_json」，pobr-core 仅 dev-dep）。

引擎入口**必须**外部注入编译后的 `CompiledParserRules`；pobr-core 自己读不了那份
JSON，也无任何嵌入（已确认：无 `include_str!` / 无 `LazyLock<CompiledParserRules>`
/ 无 `Default for CompiledParserRules`）。

而无参 `parse_mod(text)`（= legacy `parse_mod_with_rules(text, None, None)`）的调用方
**远不止清单列的 5 个**：

| 类别 | 位置 | 删 legacy 后的处置 |
|------|------|--------------------|
| 内部递归 | `mod_parser/legacy.rs`：`parse_minion_modifier`、`bonded:` 前缀、`during effect` 等递归 `parse_mod` | 递归宿主随 legacy 一起删，无替身 |
| pobr-core 内部 | `mod_cache.rs:33`、`item.rs:254`（flask `during effect`）、`skill_source.rs:549/661`、`calc/session.rs:152`（`add_modifier_texts`） | 零 I/O，拿不到 rules |
| pobr-build | `corpus.rs:37`、`calc_orchestrator.rs:5286/5345/5354/5693`（probe）、`tests/config_*` | |
| apps | `apps/pobr-cli/src/lib.rs:165/197`、`apps/pobr-wasm/src/session.rs`（经 `add_modifier_texts`） | I/O 层可改 |
| tools | `tools/precompile-mods`（注释自承「切换后改 `parse_mod(text, &CompiledParserRules)`」） | |
| 测试 | `mod_parser.rs` / `mod_parser_m2_defence.rs` / `keystone_defence.rs` / `resistance_cap.rs` / `pob2_golden.rs` / `taken_as.rs` / `parser_modcache_golden.rs` / `calc_session.rs` / `defence_panels.rs` … ~10 文件、几十处 | 大面积 |

**核心问题**：删 legacy 后无参 `parse_mod` 没有后端。三种解法各有 owner 级代价：

- **(A1) 嵌入**：pobr-core 用 `include_str!` 内嵌 `mod_parser_rules.json` + special，
  建 `static DEFAULT_RULES: LazyLock<CompiledParserRules>`，无参 `parse_mod(text)` =
  `parse_mod_engine(text, &DEFAULT_RULES)`。
  - 代价：① pobr-core **新增运行时 `serde_json` 依赖**（反序列化嵌入 JSON）——破
    pobr-data/pobr-core「本体不依赖 serde_json」的明确不变式；② 数据 source-of-truth
    复制一份进 pobr-core，破「数据收口 gamedata 一处」P9 意图（version-bump 时
    pobr-core 也得重编）。`include_str!` 本身是编译期、不破「运行时零 I/O」，但破上述两条。
  - 收益：~20 调用方零改；五调用方仍可注入 gamedata 版覆盖默认。
- **(A2) 全量穿线**：把 `&CompiledParserRules` 穿到每个无参调用方（含
  `mod_cache`/`corpus`/CLI/wasm/precompile + 全部测试，测试各自从磁盘 load rules）。
  - 代价：~20 文件、几十处签名改动 + 阻塞项 B/C；回归面广，清单只列 5 个，其余全是隐含工作。
  - 收益：纯净，与零 I/O + 数据驱动 P3/P9/P10 终局一致——是「干净切换」本义。
- **(A3) 留薄 legacy**：legacy 仅作无参 `parse_mod` 后端，五调用方走引擎。
  - 与「删 legacy」目标直接冲突，仅兜底，不推荐。

清单「2b 切换就绪清单」只写五调用方 + 删 legacy，**未对上述 ~20 个无参调用方给出
处置**——这是 owner 需先定的路线（A1 / A2 / A3）。

## 2. 阻塞项 B：`parse_minion_modifier` 语义不对齐

- orchestrator `calc_orchestrator.rs:1182` 消费 `parse_minion_modifier(text) ->
  Option<Vec<MinionModifierEntry>>`，收进召唤物自己的 ModDb（结构体
  `MinionModifierEntry { inner, minion_type }`）。
- 引擎侧 minion 是数据驱动 form（`Minions ` 前缀 → `addToMinion` → `wrap_list` 产
  **`MinionModifier LIST` modifier**），输出是 `Modifier`，**不是**
  `Vec<MinionModifierEntry>`。
- 删 legacy = 删 `parse_minion_modifier`；orchestrator minion 收集须改走引擎的
  `MinionModifier LIST` 产物 + 下游消费侧（`MinionSpawn.minion_modifiers`）对齐新形态。
  清单未列，且直接影响 minion build parity。

## 3. 阻塞项 C：orchestrator 注入是「有 special 才注入」

`calc_orchestrator.rs:462`：
```rust
if let Some(special_rules) = &data.special_rules {
    session.set_special_rules(special_rules.clone(), Some(data.special_registry.clone()));
}
```
切换后 session 必须**恒有** `&CompiledParserRules`（否则引擎无法解析任何词条）。现行
「缺表才回退、有表才注入」的 R7 容错，在删 legacy 后变成「缺表 = 完全无法解析」。须
改为 gamedata 恒 load `mod_parser_rules.json` 编译 `CompiledParserRules`、orchestrator/
session 恒注入；`BuildData::new`（无数据目录的测试构造，`build_data.rs:466`
`special_rules: None`）路径也需 rules 来源。缺表策略（fail-fast vs 回退）需 owner 定。

## 4. parity 影响

**本波未改任何生产代码 → parity 严格零变动，baseline 未动、未 bump。** 4 真 bug
（focus UsingFocus / helmet 大小写 / Triggered SPELL / LifeGainAsES）在 2a 已让引擎
产 legacy-一致值，预估切换后 0 影响（m6-dualrun-report §2.6）；但该预估**只能在引擎
确实接管全部生产解析路径后由 `parity_no_regression` 实测确认**——接管依赖 A/B/C 先落地。

## 5. 建议

倾向 **(A2 全量穿线)**——与零 I/O + 数据驱动终局一致，是「干净切换」本义；需 owner
确认愿担 ~20 文件穿线 + B（minion 形态迁移）+ C（恒注入 + 缺表策略），并把它们追加进
2b 清单。若要更快落地、可接受 pobr-core 多一份编译期数据副本 + 运行时 serde_json，则
**(A1 嵌入)** 调用方改动最少。**(A3)** 不满足删 legacy，不推荐。

请 owner 在 A1 / A2 间拍板并确认 B、C 纳入 2b 范围。拍板后即可执行（每步独立 commit，
每 commit 跑 workspace + clippy + fmt + `parity_no_regression`）。

---
**未 bump 声明**：本波 parity 零变动（未改生产代码），未触碰任何 baseline / 门禁常量。
