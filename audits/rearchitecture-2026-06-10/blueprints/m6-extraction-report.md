# M6 前置：ModParser 规则表抽取质量报告

> 生成：2026-06-11 · pre-m6-rules track（M6 蓝图 §1 的 A track 前置部分：抽取 + schema + drift 工具；**不含 scan 引擎**）
> vendor：PathOfBuilding-PoE2 @ `2df5a7433dd2f1609e2fad8a6c3c917f923fe34f`（2df5a74 Fix crash when importing a character that uses Facebreaker gloves）
> 产物：`data/4.5.0.3.4/overlay/mod_parser_rules.json`（380 KB，schema `mod_parser_rules/v1`）
> 工具：`cargo run -p sync-pob-catalog -- extract-lua --what parser-rules`；drift 防线：`sync-pob-catalog parser-rules-drift`

## 1. 抽取方式（蓝图 §1.9 的实现形态）

- **执行而非啃源码（P13）**：用 pob2-oracle 同款 headless 引导（`dofile HeadlessWrapper.lua`，cwd = vendor `src/`、`LUA_PATH` = `runtime/lua`、`CI=true`）加载完整 PoB2 环境，再经 `debug.getupvalue` 从 `modLib.parseMod`（cache 包装）→ 内层 `parseMod` 的 **upvalue** 直接取**加载后的最终表**。相比蓝图设想的"最小依赖面加载 ModParser.lua"更保真：regen/cost 派生表、resourceTypes 的 `maximum X` 变体等加载期副作用全部天然就位，且零行号依赖。
- **位掩码 → 名字（P1）**：遍历全局 `ModFlag` / `KeywordFlag` 表建「单 bit → 名」反查（复合掩码逐 bit 分解为原语名，bit 升序输出；掩码不保留 vendor `bor(...)` 书写序——bit 序确定性等价）。tag 内 `skillType` 数值经 `SkillType` 反查为名字（键落 `skill_type`）。未映射 bit / 未知枚举值一律 fail-fast，本次零命中。
- **闭包 → 模板（双哨兵探针）**：数值哨兵 73/97（多槽位用素数族 73,79,83… / 97,103,107…），字符串哨兵 qzxa/wvka 族；槽位类型从 pattern 捕获组分类（含 `%d` → 数值，字母类 → 字符串，`.+` 等先字符串后数值重试）。两轮输出做联合结构 diff：哨兵衍生值还原为 `$n` / `$n:cap` / `$n:negate` / `$n:base(c)` / `$n:mult(k)` / `$n:div(k)` 占位符，字符串拼接段用 `+` 连接并用 B 哨兵反向实例化验证；结构不一致 / 不可表示 → 落 `handler_id` 兜底。
- **确定性**：Lua 侧 JSONL 忠实输出，Rust 侧统一排序（每段按 pattern/phrase 字典序）+ serde_json 序列化 + 计数自检（钉定 vendor commit 容差 0，其它 commit 降级为告警以便 version-bump 演练吸收漂移）。重抽 byte-diff = 0 已由 `parser-rules-drift` 与 `tools/sync-pob-catalog/tests/extract_parser_rules.rs` 双重验证。

## 2. 抽取结果总量

| 段 | 条目数 | 蓝图估值 | 干净抽出 | 探针推断 | handler 兜底 | 丢弃 |
|---|---:|---:|---:|---:|---:|---:|
| forms | 91 | 91 | 91 | — | — | 0 |
| name_map | 775 | 776 | 775 | — | — | 0 |
| flag_phrases | 202 | 202 | 202 | — | — | 0 |
| pre_flags | 219 | ~250 | 210 | 9（闭包 9/9 全成） | 0 | 0 |
| tag_phrases | 682 | 684 | 538 | 141（闭包 144 中） | 3 | 0 |
| suffix_types | 40 | — | 40 | — | — | 0 |
| damage_types | 5 | 5 | 5 | — | — | 0 |
| pen_types | 6 | — | 6 | — | — | 0 |
| regen_types / degen_types / cost_types_map / base_cost_types | 各 32 | — | 各 32（含 maximum 变体派生展开） | — | — | 0 |
| flag_types | 24 | — | 24（含 hexproof 内嵌 mod 特例） | — | — | 0 |
| unsupported | 1（`mirrored`） | 1 | 1 | — | — | 0 |
| unsupported_pobr_extra | 1（`split`，pobr 自加，独立段保 drift 纯净） | — | — | — | — | — |

**闭包合计：153（preFlag 9 + tagList 144）→ 推断成功 150（98%）/ handler 3（预算 ≤15，余量充足）。丢弃 0。**

form id 集实测 28 种（与蓝图 §1.1 一致）：`INC RED MORE LESS BASE GAIN LOSE GRANTS GRANTS_GLOBAL REMOVES CHANCE FLAG TOTALCOST BASECOST PEN REGENFLAT REGENPERCENT DEGENFLAT DEGENPERCENT DEGEN DMG DMGATTACKS DMGSPELLS DMGTHORNS DMGTHORNSBASE DMGBOTH OVERRIDE DOUBLED`。

## 3. handler 兜底条目（3 条，全在 tag_phrases）

| handler_id | pattern | 失败原因（vendor 行为） |
|---|---|---|
| `tag_phrase:232c0dcf75c8` | `per (%d+) rampage kills` | `limit = 1000 / num`——倒数缩放超出五算子 DSL（蓝图 §1.9 预期内的"宁多勿错"） |
| `tag_phrase:7080fc5630f8` | `if you have a (%a+) (%a+) in (%a+) slot` | 条件分支闭包：`slot == "right" and 2 or slot == "left" and 1`，哨兵下 boolean 拼接报错 |
| `tag_phrase:35f924fd331e` | `if both equipped ([%a%s]+) have a?n? ?([%a%s]+) modifiers?` | 闭包对捕获做去尾（`itemSlotName:sub(1, #n-1)` 复数→单数）变换，模板语法不可表示 |

三条均需 M6-B 在 `rules/registry.rs` 注册 Rust handler（id 即上表；全局 handler 台账 <100，当前本表贡献 3）。

## 4. 探针推断的算子使用分布（value_expr 扩展依据）

对 150 条 inferred 条目内的占位符字符串统计（按出现次数）：

| 形态 | 次数 | 说明 |
|---|---:|---|
| 纯 `$n` | 136 | 捕获直代 div/limit/threshold 等槽位 |
| `$n:cap`（拼接段） | 17 | `firstToUpper(捕获)` 拼进 var 名（如 `$2:cap+Effect`）——**蓝图 §4.2 裁决批准的 `:cap` 扩展实测受益 17 处/16 条条目**，低于蓝图预估 ~139（多数闭包实为纯数值代入），但拼接类条目无 `:cap` 即全部退 handler，扩展仍然必要 |
| `$n:base(c)` | 12 | 加性偏移（`threshold = num - 1` 六属性阈值族等） |
| `$n:mult(k)` | 2 | 整数倍缩放 |
| `$n:div(k)` / `$n:negate` | 0 | 本表未用到（special_mods 侧可能用到，算子保留） |

## 5. 与蓝图的偏差记录（蓝图尾注："以实际为准、回写偏差"）

1. **计数修正**：modNameList 实测 **775**（蓝图 776）、modTagList 实测 **682**（蓝图 684）、闭包实测 **9+144=153**（蓝图 9+139）。抽取自检（`PINNED_SECTION_COUNTS`）按实测钉定。
2. **`resource_types` 段不入库**：vendor 加载完成后 `parseMod` 仅消费其四个派生展开表（regen/degen/cost/base_cost），原始 `resourceTypes` 非 parseMod upvalue、运行时不可达也无消费方。schema 文档已注明；若 M6 主波发现需要可从派生表无损还原。
3. **抽取通道**：未走"最小 stub 加载 ModParser.lua"（其加载段依赖 `modLib.createMod`、`data.gems` 等大面环境），改走 headless 全量引导 + upvalue 走读（§1）。对 drift 工具与 drill 的影响：重抽需要完整 vendor 检出（`src/` + `runtime/lua`），CI 无 vendor 时集成测试按既有约定跳过。
4. **tag 字段键名**：蓝图 schema 例中点名的三个转换键落 snake_case（`skill_type` / `mod_flags` / `keyword_flags`，值已反查/分解为名字），**其余 tag 字段键名 vendor camelCase 原样转录**（`limitTotal` / `varList` / `globalLimitKey` …）——忠实转录优先，消费侧（M6-B）按原文匹配。
5. **新发现字段**：preFlagList 含蓝图 §1.4 未列的 `modSuffix`（4 条非闭包 + 闭包产出若干，如 `^take ` → `"Taken"`）；modNameList 表条目除 names/tags 外还携带 `flags`/`keywordFlags`/`addToMinion`/`addToSkill`。schema `RuleEffectsDef` 已按实测字段全集建模（name_map/flag_phrases/pre_flags/tag_phrases 四段共用，serde flatten）。
6. **anchored 非全锚**：preFlagList 有 4 条不带 `^`（`against you`、`allies in your presence `、`allies in your presence [hgd][ae][via][enl] `、`attacks with energy blades `），忠实保留——M6-B scan 引擎不得假设 pre_flags 全锚定。
7. **literal 派生**：91 条 forms 中 2 条无字面段（`^(%d+)`、`^([%+%-][%d%.]+)%%?`）→ 引擎 always-check 桶；pre_flags/tag_phrases literal 覆盖 100%，无 <3 字符短 literal。等长 run 取先出现者（确定性 tie-break，实现于 `derive_pattern_meta`）。

## 6. 已知局限（M6 主波须知）

- **探针推断的条件盲区**：双哨兵同落同一分支的条件闭包（如阈值判断 `if num > 50`）会被误判为无条件模板。本表 150 条 inferred 均带 `inferred: true` 元数据，按蓝图 §1.9 由 M6-C 的 oracle parseMod differential 终裁后升级 `verified`（字段届时增补）。
- **`{Hand}` 占位符**：flag_phrases/tag_phrases 中 `var = "{Hand}Attack"` 一类条目按 vendor 原样转录，实例化语义（item ingest 上下文代入）属 GRANTS/local 消费侧（蓝图 §3 表注）。
- **掩码 bit 序 vs 书写序**：`flags: ["Hit", "Mace"]`（bit 升序）≠ vendor 书写序 `bor(Mace, Hit)`——语义等价（位或交换律），对拍时注意集合比较。
- 本 track **未做**：scan 引擎 / CompiledParserRules / RuleSet 填实（M6-T3/T8）、special 通道、skillNameList/preSkillNameList（M6-T7 generated 层）、oracle differential（M6-C）。

## 7. 防线与测试清单

| 防线 | 位置 |
|---|---|
| 重抽 byte-diff（drift） | `sync-pob-catalog parser-rules-drift --vendor-root <src> --committed data/4.5.0.3.4/overlay/mod_parser_rules.json`（byte 等价 + 分段增/删/改摘要，退出码 1 即漂移） |
| 重抽 == 已提交（CI，缺 luajit/vendor 自动跳过） | `tools/sync-pob-catalog/tests/extract_parser_rules.rs` |
| 计数 / form id 集自检（钉定 commit 容差 0） | `tools/sync-pob-catalog/src/extract_parser_rules.rs::self_check` |
| schema 往返 + 形态钉值（mini fixture，A→B 契约） | `crates/pobr-data/tests/parser_rules_schema.rs` + `tests/fixtures/mini_parser_rules.json` |
| 消费侧计数 + 逐字段抽样钉值 + handler 预算 | `crates/pobr-gamedata/src/domains/parser_rules.rs`（tests） |
| literal/anchored 派生单测 | `tools/sync-pob-catalog/src/extract_parser_rules.rs`（tests） |
