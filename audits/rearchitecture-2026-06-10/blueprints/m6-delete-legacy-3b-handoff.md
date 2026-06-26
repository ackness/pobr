# M6 删 legacy 收尾：2/3+3a 已落地，3b（物理删除）手册

> 状态登记 + 3b 执行手册。承接 `m6-switch-decision.md`「切换阻塞」节之后的收尾波。
> 落地分支：`claude/stoic-ritchie-ub7jty`。

## 已落地（本会话，已 push）

| commit | 步骤 | 内容 | 验证 |
|---|---|---|---|
| `671b686` | **2/3** | `ParseCtx` 从 `legacy.rs` 迁出至独立 `mod_parser/dispatch.rs`（同 1/3 把输出类型迁到 `outcome.rs` 的范式）。调用方路径 `pobr_core::mod_parser::ParseCtx` 不变。 | 1540 tests，计数与基线逐项一致；clippy `-D warnings`/fmt clean |
| `73c78e9` | **3a** | 去 `parser-engine` feature 门控：引擎模块（scan/compiled/engine/forms/template/canonical）+ `ParseCtx.engine` + `session.parser_rules` + orchestrator 注入 + `build_data` 规则编译全部无条件编译；`aho-corasick` 转无条件依赖；`pobr-core` `test-rules` 改为仅 `["dep:serde_json"]`；`pobr-build` 删 default/parser-engine feature；`parser_dual_run` 去 `#![cfg]`；`mod_parser_bench` 去 `required-features`。legacy 降为「未注入引擎规则时」的纯回退路径。 | workspace check 0 warning；1540 tests（含 `parity_no_regression`）；clippy/fmt clean。**行为中性**（生产恒注入规则→走引擎；pobr-core 自测未注入→仍 legacy 回退） |

净效果：feature 矩阵复杂度归零，引擎是唯一生产解析器，`ParseCtx` 已与 legacy 解耦。**仅剩 `legacy.rs`（~4085 行）物理删除 + 把仍依赖 legacy 入口的测试迁到引擎 = 3b。**

## 为何 3b 不在本（云）环境完成

3b 会把所有「无注入规则」路径（含一批解析单测）强制走引擎。落地前做了**可回退探针**：把 `tests/parser/mod_parser.rs` 的 `parse_mod(text)` 临时改走 `parse_mod_engine(text, &rules)`（legacy 仍在），实测引擎对单测输入的覆盖。结果 **84 中 13 失败**——引擎与 legacy 在这些 parse 行为上**真实分歧**（数据完整：`overlay/mod_parser_rules.json` 389KB 已 commit，含 89×PerStat/199×Multiplier 规则；71/84 通过证数据可用）。

云环境无法就地修复/裁决这些分歧：
- `luajit`/`lua` **缺**→ 跑不了 `sync-pob-catalog extract-lua --what parser-rules`（`regen-all.sh:107`，从 vendor Lua 重生规则）。
- `vendor/PathOfBuilding-PoE2/` **不在仓库**（0 tracked files，本地检出物）→ 无 Lua 参考实现可对照/重抽。
- GGG patch CDN 对钉定 `4.5.0.3.4` 返回 **404**（旧补丁已下线；且 parser 规则本就非 GGG 下载，与 `.dat` 管线无关）。

故 3b 须在**本地环境**（有 `vendor/` + luajit）做：逐条对照 vendor `ModParser.lua` 裁决「引擎 bug（改数据/引擎）vs 测试该更新（引擎正确）」，禁在云端凭猜更新断言或 bump baseline（依 `m6-switch-decision.md` owner 门禁）。

## 探针发现的 13 项分歧（3b 必须逐条裁决）

行号指 `crates/pobr-core/tests/parser/mod_parser.rs`（探针前原始行）。

### A. 真实引擎能力/逻辑缺口（高优先——可能影响真实 build）
1. **PerStat/Multiplier tag 丢失**（最关键）：`+5 to maximum Mana per 10 Intelligence`(:166)、`+1 to Accuracy per Strength`(:181)、floor-count 变体(:635)——引擎产出 stat 但**不挂** `Multiplier{Intelligence/Strength,…}` tag → per-属性缩放静默失效。规则在 JSON 内（89×PerStat），疑引擎 form 匹配逻辑漏挂 tag。
2. **武器职业 keyword-flag/condition 编码**：one-handed bits(:902, `4` vs `17179869188`)、`UsingOneHandedMelee` condition 缺失(:762)、unarmed bit(:791, `16777220` vs `16777216` 多一位)、weapon-type attack-speed→condition(:419)。
3. **聚合抗性展开**：`all … Resistances`（含混沌）(:447)——引擎 2 mods vs legacy 4。
4. **gain-as per grenade**(:374)。
5. **bonded enabler → condition flag**(:685)——引擎 0 mods vs 1。
6. **PoB bracket markup 剥离**(:276)。

### B. 引擎设计语义（疑应改测试，非引擎 bug——仍需 vendor 确认）
7. `unknown_text_is_an_error_with_original_line`(:199)：引擎恒返回 `Ok(Unsupported)`，不返回 `Err`（引擎设计：永不报错）。测试期望 `Err`。
8. `pure_immunity_phrase_is_unsupported_not_error`(:529)：引擎 `Parsed`，测试期望 `Unsupported`（引擎更能解析，或误解析——需核对 vendor）。

> 复现探针：在 `mod_parser.rs` 顶部加一个 `parse_mod(text) -> Ok(parse_mod_engine(text, &rules))` 影子函数（规则用 `parser_dual_run.rs::load_rules` 同款 loader：`overlay/mod_parser_rules.json` + `overlay/special_mods.json` + `generated/special_derived.json`），跑 `cargo test -p pobr-core --test parser`。

## 分歧根因分析（本会话深挖，供本地定向修复）

> 用引擎源码 + 389KB 规则 JSON 深挖（无 vendor Lua，故下列「修复方向」须本地对照
> `vendor/.../Modules/ModParser.lua` 核实后再落地；尤忌凭猜改 tag 构造致静默错算）。

### PerStat/Multiplier（#1，最关键）— 已定位为「var 名未归一 + 缺数字 form」
- 规则编码：`per intelligence` → `tag_phrases` 里 `{"type":"PerStat","stat":"Int"}`；带数字
  divisor 走 `{"type":"Multiplier","div":"$1","var":...}`（JSON 有 73 处 `"div":"$1"`）。
- 引擎 `template.rs:178-192` `compile_tag` 已把 `PerStat`/`PercentStat` **归一为
  `ModTag::Multiplier{var:<stat>, div}`**（C2 归一，设计正确）。
- **gap (a) var 名未 vendor→PoBR 归一**：引擎产 `var:"Int"`，legacy/测试期望
  `var:"Intelligence"`（`data/.../overlay/vendor_name_aliases.json` 有属性别名）。修复：
  在 `compile_tag` 的 `PerStat`/`Multiplier` 分支对 `stat`/`var` 套属性别名（**仅 stat-attr
  名，勿误伤 `AxeItem`/`SummonedMinion` 等非属性 var**），或抽取期归一。
- **gap (b) `per N <stat>` 数字 form 缺失**：literal `per intelligence` phrase 不吃数字 →
  `per 10 Intelligence` 丢 `10`（div 落 1）。需补「`per (%d+) (stat)`」form 捕获 `$1` 作 div。
- **不破 c1 风险低**：c1 语料无此 per-stat 族（故 `c1_diff_zero_gate` 绿却仍分歧）；
  修后仍须重跑 c1 gate 确认 0。

### 武器职业 keyword-flag 位编码（#3/#7/#12/#13）
- bitmask 差（如 `4` vs `17179869188`、`16777220` vs `16777216`）+ `UsingOneHandedMelee`
  condition 缺。须对照 vendor `ModFlag`/`KeywordFlag` 位定义与 weapon-class → flag/condition
  映射，核引擎 `flag_phrases`/`flag_types` 表的位值与 condition 派生。

### 聚合抗性展开（#2）
- `all … Resistances`（含混沌）引擎 2 vs legacy 4 mods。核 `name_map` 对 `AllResist` 类聚合
  名的展开目标集（是否含 Chaos）+ vendor 语义。

### 设计语义类（#9 immunity / #11 unknown→Err）— 非引擎 bug，应改测试
- 引擎恒返回 `Ok(Unsupported)`（永不 `Err`），且对部分 immunity 短语更能解析。这是引擎
  既定契约（`m6-switch-decision` 已记），3b 时把 `unwrap_err()`/期望 `Unsupported` 的断言
  改判为引擎语义即可，无需动引擎。

> 其余（#4 gain-as-per-grenade、#5 bonded enabler、#10 bracket markup）未逐一深挖，
> 同法：先 `POBR_DBG` dump 引擎产物 vs legacy，对照 vendor `ModParser.lua` 定责。


## 3b 机械清单（分歧裁决后执行）

1. **删 `crates/pobr-core/src/mod_parser/legacy.rs`**（~4085 行）+ `mod.rs` 去 `pub mod legacy;` 与 `pub use legacy::{parse_minion_modifier, parse_mod, parse_mod_with_rules};`。
2. **`dispatch::ParseCtx` 收敛为引擎专用**：去 `rules`/`registry`/`none`/`with_rules`（special 已编译进 `CompiledParserRules::special`），`parse()` 仅 `parse_mod_engine`。决定无规则路径语义（建议：要求恒注入规则）。
3. **迁直连 legacy 入口的生产调用方**到引擎：`apps/pobr-cli`（`parse_mod_with_data`）、`pobr-build/src/corpus.rs`（`classify_line_with_rules`）、`tools/precompile-mods`、`pobr-core/src/mod_cache.rs`（`parse_or_insert`，无生产调用方，仅测试）。`calc_orchestrator.rs:1214` minion legacy 回退删除（仅留引擎 `extract_minion_modifier_entries` 路径）。
4. **`parse_minion_modifier` 替代**：引擎侧 `parse_mod_engine(text,&rules)` + `extract_minion_modifier_entries`（orchestrator 已有此路径）。
5. **迁测试到引擎**（约 15 文件）：`tests/parser/{mod_parser,mod_parser_m2_defence,parser_modcache_golden,special_mods_gate}.rs` + 用 `add_modifier_texts`/无 ctx ingest 便捷封装的 calc 测试（`engine/calc_session`、`golden/pob2_golden`、`offence/crossbow_reload`、`sources/env_finalize_*`、`defence/keystone_defence` 等）。建共享 dev-only 规则 loader（不引入新 feature gate；`serde_json` 已是 dev-dep）。按 A/B 裁决更新断言。
6. **删 `tests/parser/parser_dual_run.rs`**（legacy-vs-engine 对拍门禁，删 legacy 后失义）；`mod_parser_bench.rs` 去 legacy 臂或改纯引擎 bench。
7. **清理**：`test-rules` feature / `test_compiled_rules`（当前**无消费方**，可随共享 loader 一并删或保留）；各文件残留 `parser-engine` 字样的过时注释（item.rs/session.rs/build_data.rs/skill_source.rs/mod_cache.rs/calc_orchestrator.rs/engine.rs）。
8. **门禁**：`cargo nextest run --workspace`（或 `cargo test`）+ clippy `-D warnings` + fmt；`parity_no_regression` 不得回归。若 A 类裁决改了引擎/数据致 parity 变动 → 独立 commit + owner 审查，**禁自行 bump baseline**。
