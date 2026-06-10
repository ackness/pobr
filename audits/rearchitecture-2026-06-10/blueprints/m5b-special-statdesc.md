# M5b 实施蓝图：special_mods 分批数据化 + statdesc 渲染链路

> 撰写：2026-06-11 · 规划者只读产出 · 状态：待实施
> 对应 roadmap 阶段：M5(b)（21-roadmap.md §M5）· 对应缺口：10-G2（specialModList 2085 条缺失，本阶段分批主体、M6 收尾长尾）、15-G1（statdesc 渲染链路缺失，R5 离线验证先行）
> 对应架构裁决：P4（special 切分）、P13（extract-lua 抽取方式）、P9（注入方式）、§5（DSL 硬边界）
>
> **本文自包含**：实施 agent 只读本文 + 代码即可开工。roadmap / 架构文档的关键原文已照录在 §0 与 §6。

---

## 0. 总纲与硬性纪律（原文照录）

### 0.1 本阶段任务（roadmap M5(b) 原文）

> **(b) special + statdesc**：`special_mods.json` 按 ninja 命中频率分批迁移（先 keystone/高频 unique 词条），handler 覆盖清单跑通（未映射告警）；statdesc 渲染链路——先离线验证（渲染结果 vs PoB2 导出文本逐行 diff 达标）才作为 mods.json 的 rendered_lines 生产列（R5 缓解）。

M5 验收门禁中与 (b) 相关的两句（roadmap 原文）：

> unsupported 词条率下降曲线纳入报表；special 迁移条目 oracle 抽样对拍。

### 0.2 统一门禁与执行纪律（roadmap §0 原文，每次合并回 master 适用）

> **门禁三件套**（每次合并回 master）：
> 1. `cargo test --workspace` + `clippy -D warnings` + `fmt --check` 全绿；
> 2. **ninja_parity 18-build 零回归**——防御 51% / 进攻 24%（@5% 容差）为底线不得倒退；阶段各自的提升目标见各节"验收"；
> 3. 涉及解析/数据的阶段：加 pob2-oracle 对拍或 generated 重生一致性校验。
>
> **执行纪律**（FINDINGS 04-02 教训制度化）：
> - **搬迁不变式**：纯搬迁（数据出代码、入 JSON）的 commit，parity baseline **逐值不变**（golden diff = 0）；搬迁与行为改动永远分两个 commit。
> - 行为修复必须附 PoB2 一手依据（源码行号/oracle 中间值）；baseline 更新独立 commit、显式审查。
> - 核心改动（mod_db/ModFlags/stat_map 引擎切换等）feature-gated **双跑对照**，diff 报告干净后才删旧码。

注：M5b 实施时的 ninja_parity 基线常量以届时 master 的 `crates/pobr-build/tests/ninja_parity.rs` 中 `BASELINE_DEF_HIT5` 等四个常量为准（写本文时为 111/117/23/31），不是 roadmap 中的历史百分比。

### 0.3 受限模板 DSL 硬边界（20-target-architecture.md §5 原文照录，写入 review checklist）

> special_mods/config effects 的占位符语言**硬边界**：
>
> - 允许：`$1..$n` 数值占位、字面量、`negate/clamp(min,max)/div/mult/base` 五种算子、target(player|enemy|minion)、受限谓词（字段引用 + eq/ne/gt/lt + and/or）。
> - 禁止：循环、递归、自由表达式、跨条目引用、字符串拼接求值。
> - 扩展闸门：新增任何 DSL 能力需 ≥20 个条目受益，否则该条目走 handler_id。
> - 监控：handler 条目数 <100；逼近 special 总量 10% 即判切分失败、回看 P4。
> - 元数据：未经 oracle 验证的条目带 `verified:false`，运行时照用但 parity 报告单列。

本蓝图在此边界内提出**一项 DSL 微扩展**（§3.2 的 `enums` 受限闭集映射），并按"≥20 条目受益"闸门给出论证；除此之外不得新增任何 DSL 能力，新需求一律走 handler_id。

### 0.4 开工前提

1. **M0–M4 已合并**（按 roadmap 顺序，M5 在 M3/M4 之后）。本蓝图对 M2/M3/M4 产物的依赖以"接口契约假设"形式列在 §2.6，实施前用 `git log` + 代码确认，不符则先调接口再开工。
2. **W3（RuntimeConstants/RuleSet 注入管道）已合并**：`pobr-gamedata::ruleset.rs` 的 `RuleSet` 聚合入口已存在（写本文时为骨架，字段全 `Option`），M5b 在其上扩 `special_mods` 域。
3. `luajit` 可用（`/opt/homebrew/bin/luajit` 或 `POBR_LUAJIT`），vendor PoB2 已完整检出（`vendor/PathOfBuilding-PoE2/src/` 含 Modules/Data/Export 三目录，写本文时已确认在位；`Modules/ModParser.lua` 642KB、`Data/StatDescriptions/stat_descriptions.lua` 3.9MB 均存在）。

---

## 1. 现状代码地图（实施 agent 起点）

| 资产 | 位置 | 现状 |
|---|---|---|
| modifier 文本解析器 | `crates/pobr-core/src/mod_parser.rs`（62.3K 单文件） | 手写窄子集；special 仅 `parse_keystone_special`（:486 起，CI / no-mana / EB / `Your X is N%` OVERRIDE / ES→Mana 转换等十余条硬编码）；主流程 :170-230 按"专用函数 → keystone special → parse_form → strip 链 → resolve_names"顺序；签名 `parse_mod(text: &str) -> Result<ParseOutcome, ParseError>` |
| handler 注册表骨架（M0 已落） | `crates/pobr-core/src/rules/registry.rs` | `HandlerRegistry`：`register/get/len/ids`，重复注册报错；`Handler = Box<dyn Fn(&[f64]) -> Vec<Modifier> + Send + Sync>`（骨架签名，注释明示接入 special 时按需扩参并同步全部 handler） |
| RuleSet 聚合入口（M0 骨架/W3 进行中） | `crates/pobr-gamedata/src/ruleset.rs` | `GameData::load_ruleset() -> RuleSet`；字段 `parser_rules/game_constants/config_catalog` 均 `Option`（W3 正把 game_constants 填实） |
| overlay merge 引擎（M0 已落） | `crates/pobr-gamedata/src/overlay.rs` | base→overlay 确定性 merge，规则单测锁定 |
| extract-lua 子命令（M0 已落） | `tools/sync-pob-catalog/src/extract_lua.rs` + `extract_skill_overrides.lua` | luajit 执行 vendor 序列化 → byte-stable JSON；`OverlayMeta`（vendor_commit / regen_command）模式可直接复制；现仅 skill_overrides 一种 kind |
| catalog schema | `crates/pobr-data/src/catalog/`（模块目录，M0 已拆） | 尚无 `parser_rules.rs`；`mod.rs` 列出全部子模块并 re-export |
| parity harness | `crates/pobr-build/tests/ninja_parity.rs` | 18 build（`examples/demo-bd-test/builds/*/`，每个含 code.txt / decoded.xml / meta.json）；`parity_no_regression` 门禁 + `parity_baseline_report` 报表；**尚无 unsupported 词条报表** |
| unsupported 收集 | `pobr-core::calc::session.rs:14,180`（`unsupported_modifier_texts`）；CLI 已曝露 | 但 pobr-build 的 `calc_orchestrator.rs::filter_parseable`（:1892）把硬失败词条**静默丢弃**（grep `unsupported` 在 pobr-build/src 零命中）——做语料统计必须绕开该过滤、自行分类 |
| pob2-oracle | `tools/pob2-oracle/{oracle.lua,run.sh}` | headless 引导 PoB2、dump build 级计算分解；**尚无 parseMod 级 differential 模式**（grep parseMod 零命中） |
| vendor specialModList | `vendor/PathOfBuilding-PoE2/src/Modules/ModParser.lua:2231-6150` | 2085 条；key = Lua pattern（小写、含 `(%d+)`/`(.+)` 捕获），value = 纯 mod 表或闭包；辅助构造器 `explodeFunc`(:2217) / `grantedExtraSkill`(:2155) / `triggerExtraSkill`(:2163)；:6151-6158 由 `data.keystones`（Modules/Data.lua:304-339，36 个关键石名）派生 `Keystone LIST` 条目；:6155-6158 给全表 key 加 `^...$` 锚定 |
| vendor statdesc | `Data/StatDescriptions/`（10 份：主文件 stat_descriptions.lua 3.9M、skill 688K、gem 582K 等 + Specific_Skill_Stat_Descriptions/） | 结构化 Lua 表：`{ [n] = { [lang] = { {limit={{min,max}...}, text="...{0}%...", [specs]}...}, stats={...} }, ["stat_id"]=n }`；渲染逻辑在 `Modules/StatDescriber.lua`（263 行：matchLimit + ~40 种 `spec.k` 数值变换）与 `Export/Scripts/statdesc.lua`（Export 侧 describeStats，渲染 `(5-8)` 区间形态） |
| vendor 渲染基准（diff 对照物） | `Data/ModItem.lua`(1MB) / ModItemExclusive(1.8M) / ModJewel / ModFlask / ModCharm / ModCorrupted / ModRunes / ModVeiled | key = GGG mod id（如 `"Strength1"`），值含**已渲染文本列**（如 `"+(5-8) to Strength"`，含多行词条）——这就是"PoB2 导出文本"，逐行 diff 的黄金参照 |
| pobr mods.json | `data/4.5.0.3.4/base/mods.json`（5.5M，16614 条） | 每条 `{id, mod_type, domain, generation_type, level, stats:[{stat_id,min,max}], tags}`；id 与 vendor ModItem key 同为 GGG mod id ——**diff 的 join key 现成** |
| 词缀池语料的另一面 | `data/4.5.0.3.4/base/passive_tree.json` / build XML item 文本 | 天赋节点 stats 与装备词条均以英文文本行入库/导入，经 parse_mod 解析——这是 unsupported 语料的来源 |

---

## 2. 接口契约（track 间唯一耦合面，先冻结再并行）

### 2.1 `overlay/special_mods.json` schema（v1）

落点 `data/<ver>/overlay/special_mods.json`，schema 类型落 `crates/pobr-data/src/catalog/parser_rules.rs`。顶层：

```json
{
  "_meta": {
    "schema": "special_mods/v1",
    "generator": "hand-curated + sync-pob-catalog check special-coverage",
    "vendor": "PathOfBuilding-PoE2",
    "vendor_commit": "<40-hex，对账基准>",
    "regen_command": "（手工策展域：此字段记录对账命令而非再生命令）"
  },
  "entries": [ SpecialTemplateDef... ]
}
```

`SpecialTemplateDef`（serde 类型，全部新字段 `#[serde(default)]`，遵守 R7 纪律）：

```jsonc
{
  // 稳定 id：snake_case，diff/报表/oracle 对拍引用用。条目重命名视为删+增。
  "id": "obliteration_explode_on_kill",

  // 匹配模式：Rust regex 语法（rust regex crate 子集：不用 look-around/反向引用）。
  // 引擎统一做：输入小写规范化（与现 parse_mod 的 rest 同一规范）、整行锚定（编译期包 ^...$，
  // 对照 PoB2 ModParser.lua:6155-6158 的同等处理）。捕获组按出现序 = $1..$n。
  // 数值捕获统一写 (\\d+(?:\\.\\d+)?)；词类捕获必须是显式闭集 (fire|cold|lightning|chaos|physical)，
  // 禁止 (.+) 开放捕获（开放捕获条目走 handler_id）。
  "pattern": "enemies you kill have a (\\d+)% chance to explode, dealing a (\\d+)% of their maximum life as (fire|cold|lightning|chaos|physical) damage",

  // vendor 对账元数据：原 Lua pattern 字面量（special-coverage 工具按它对 vendor key 做存在性 diff）
  "vendor_pattern": "enemies you kill have a (%d+)%% chance to explode, dealing a (.+) of their maximum life as (.+) damage",

  // 模板路径（与 handler_id 互斥；两者都缺 = 纯识别不产 mod 的"已知不支持"条目）
  "mods": [ ModTemplate... ],

  // handler 路径：真逻辑条目只记稳定 id；args 把捕获按序传给 handler
  "handler_id": null,
  "handler_args": ["$1", "$2", "$3"],

  // 元数据（§0.3 硬边界要求）
  "verified": false,            // oracle 对拍通过后置 true（Track D 流程）
  "batch": "S1",                // 批次标记：S0|S1|S2（§4.3），M6 长尾续编
  "source_note": "Obliteration（unique wand）"
}
```

`ModTemplate`：

```jsonc
{
  "name": "Damage",                      // ModName 字面量；或 {"enum": 3} 引用 enums 表（见下）
  "type": "MORE",                        // BASE|INC|MORE|FLAG|OVERRIDE|LIST（pobr ModType serde 名）
  "value": {"ref": "$1", "ops": [{"negate": {}}, {"div": 100}]},
  //  ^ 三态：数字字面量 | "$n" | {ref, ops[]}；ops 仅 negate/clamp(min,max)/div/mult/base 五种（§0.3）
  "flags": ["Attack"],                   // ModFlags 名列表（按届时 M4 扩位后的位名）
  "keyword_flags": [],
  "tags": [ {"type": "Condition", "var": "UsingWand"} ],
  //  ^ tag 结构 = pobr ModTag 的 serde 形态（M3/M4 扩到 ~20 种后），var/threshold 等值可用 "$n"
  "target": "player"                     // player|enemy|minion（缺省 player）；enemy → EnemyModifier LIST 包装（M3 通道）
}
```

**DSL 微扩展（需架构 review 确认，论证如下）**：条目级 `enums` 受限闭集映射——

```jsonc
"enums": { "3": { "fire": "Fire", "cold": "Cold", "lightning": "Lightning", "chaos": "Chaos", "physical": "Physical" } }
```

`ModTemplate.name`/`tags[].var` 可写 `{"enum": <捕获序号>}`，求值 = 在该闭集表里查捕获词得到**完整字面量**。这不是字符串拼接求值（禁止项）：每个可能输出都是表内显式字面量、闭集、零运算。受益条目 ≥20 的论证：explode 族（vendor :2233-2305，≥20 条变体）、`as extra X damage`/`X damage taken as Y` 族、按元素分支的抗性/伤害词条族——若无 enums，这些纯模板条目全部被迫按元素笛卡尔展开（条目数 ×4~5）或走 handler（直接冲击 <100 监控线）。**若 review 否决此扩展，回退方案 = 笛卡尔展开**（条目变多但语义不变，不阻塞）。

### 2.2 `rules/special_mod.rs` API（pobr-core）

```rust
// crates/pobr-core/src/rules/special_mod.rs
pub struct SpecialModRules { /* 编译后的条目：载入期 regex 编译 + RegexSet 预筛 */ }

impl SpecialModRules {
    /// 载入期编译；pattern 非法 / enums 引用越界 / ops 非白名单 → Err（fail fast，不静默）。
    pub fn compile(defs: &[SpecialTemplateDef], registry: &HandlerRegistry)
        -> Result<Self, SpecialCompileError>;

    /// 对单行（已小写规范化）做整行匹配。命中且可实例化 → Some(SpecialMatch)。
    /// handler_id 未注册 → 命中但产出空 mods + `unregistered_handler` 标记（报表用，不 panic）。
    pub fn try_match(&self, line: &str, registry: &HandlerRegistry) -> Option<SpecialMatch>;
}

pub struct SpecialMatch {
    pub entry_id: String,
    pub mods: Vec<Modifier>,     // 已实例化、已带 source 词条原文
    pub verified: bool,          // 透传给 parity 报表（verified:false 单列）
    pub unregistered_handler: Option<String>,
}
```

匹配优先级（对照 PoB2 parseMod :6389-6755 的顺序：jewelFunc → unsupported → **specialModList 锚定全行** → preFlag → form...）：在 `mod_parser.rs` 主流程中，special 查表插在**现 `parse_keystone_special` 调用点之前**（即 parse_form 之前、各专用函数之后；专用函数族是 pobr 既有行为，M5b 不重排以守搬迁不变式——见 §6 R2 落点）。

### 2.3 parse_mod 签名过渡（双签名，M6 收口）

```rust
// 既有签名保持不变（全部既有调用方零改动）：
pub fn parse_mod(text: &str) -> Result<ParseOutcome, ParseError>;       // = with_rules(text, None)
// 新增：
pub fn parse_mod_with_rules(text: &str, rules: Option<&SpecialModRules>)
    -> Result<ParseOutcome, ParseError>;
```

`ParseOutcome` 增 `#[serde 无关] special_meta: Option<SpecialMatchMeta>`（entry_id + verified，归因与报表用）。M6 重写 parser 为 `parse_mod(text, &ParserRules)` 时收掉双签名。

### 2.4 RuleSet / 注入链（P9）

```rust
// pobr-gamedata/src/ruleset.rs（在 W3 落地后的真实 RuleSet 上扩域）
pub struct RuleSet {
    /* W3 既有域不动 */
    pub special_mods: Option<Vec<SpecialTemplateDef>>,   // 新增；loader 容忍缺表（R7）
}
```

链路：`GameData::load_ruleset()` 读 `overlay/special_mods.json`（+ `generated/special_derived.json`，两表 entries 拼接、id 冲突报错）→ pobr-build 在 `CalcOrchestrator` 构建期 `SpecialModRules::compile` 一次 → 所有 ingest 路径（item/passive/gem/jewel）改走 `parse_mod_with_rules`。pobr-core 保持零 I/O。

### 2.5 `generated/special_derived.json`（keystone 派生）

schema 与 special_mods.json 同（`_meta.schema = "special_derived/v1"`）；由 `pobr-data-adapter` 新步骤从 `base/passive_tree.json` 的 keystone 节点名确定性派生（对照 vendor :6151-6158 语义：每个 keystone 名 → 一条 `{pattern: "^<name:lower>$", mods: [{name:"Keystone", type:"LIST", value:"<Name>"}]}`）。CI 走 M0 既有 "generated 重生一致" 防线。**注意**：vendor 用 `data.keystones`（Data.lua:304，36 条、排除 timeless-jewel 专属）而非全部树上 keystone 节点——adapter 派生时以 passive_tree.json keystone 节点为全集即可（超集无害：多出的 pattern 只是多识别几行），差异记录在 `_meta`。

### 2.6 对 M2/M3/M4 产物的契约假设（开工前逐条确认）

| 假设 | 来源阶段 | 不成立时的动作 |
|---|---|---|
| `rules/keystone_registry.rs` 存在，keystone 开关读 ModDb flag | M2 | Keystone LIST 展开（B-5）改为直接注入该 keystone 的效果 mods（过渡），登记回 M6 |
| `Modifier` 支持 LIST 值（EnemyModifier LIST 转发通道） | M3 | `target:"enemy"` 与 `Keystone LIST` 条目全部暂走 handler_id 或推迟到通道就绪 |
| ModTag ~20 种 + serde 形态稳定 | M3/M4 | ModTemplate.tags 仅允许届时已有变体；缺的 tag 类型条目标 `verified:false` + 留 S2 批次 |
| ModFlags 已扩位（武器类型位） | M4 | flags 字段仅允许旧 5 位名；武器类条目推后 |
| HandlerRegistry 签名可能已被 M3 config_interpreter 扩展 | M3 | B-2 以届时 master 签名为准统一扩参（`HandlerCtx { nums, words }`），同步更新全部已注册 handler |

---

## 3. 工作项分解

格式：每项 = {目标 / 涉及文件 / vendor 参照 / 新增表与 schema / 测试与 fixture / 预估规模}。规模单位 = 人日（1 agent 专注日）。

### Track A — 语料统计与 unsupported 报表（独立先行，产出选批依据）

**A-1 unsupported 语料抽取工具（pobr-cli 子命令 `corpus-report`）**
- 目标：从 18-build parity fixture 提取全量词条文本 → 三分类（Parsed / Unsupported / Err）→ 模板归一化 → 频率排序清单。这是"按 ninja 命中频率分批"的唯一事实来源。
- 方法（核心细化）：
  1. **语料源**：遍历 `examples/demo-bd-test/builds/*/code.txt`，走与 ninja_parity 完全相同的链路（`parse_build_from_code` + `BuildData`）收集词条：装备 implicit/explicit/enchant（经 `item_text::parse_item_text` 后的三段文本）、天赋节点 stats（`pobr-tree::collect_allocated_mods`）、jewel grant 行。**不走 `calc_orchestrator::filter_parseable`**（它把 Err 词条静默丢弃，:1892），直接对原始行调 `parse_mod` 分类。
  2. **分类**：`Ok(ParseStatus::Parsed)` / `Ok(ParseStatus::Unsupported)` / `Err(_)`。后两类合并为"缺口语料"。
  3. **归一化**：`\d+(\.\d+)?` → `#`、压缩连续空白、小写（与 parse_mod 的 rest 规范一致）、剥离 PoB2 方括号标记 `[...]`（与现 parser 同规则）。同模板异数值的行合并计数，保留 ≤3 条原文样本。
  4. **排序键**：(命中 build 数 desc, 总出现次数 desc, 模板字典序)。每行输出：`{template, builds_hit, total_count, samples[], status, sources{item|passive|jewel 计数}}`。
  5. **vendor 可解析性标注**（依赖 D-1，无 D-1 时此列留空跑通）：对每模板取一条原文样本喂 oracle parseMod——PoB2 能产非空 modList 的标 `pob2_parses: true`（= 迁移目标）；PoB2 也不解析的标 false（= out-of-scope，不计入覆盖率分母）。
- 涉及文件：`apps/pobr-cli/src/lib.rs`（新命令实现 + 导出）、`apps/pobr-cli/src/main.rs`（分发臂）；输出 JSON/markdown 到 stdout 或 `--out`（**不入 data/**，是报表不是数据）。
- 测试：固定 2 个 build 的 golden 报表片段（模板计数确定性）；空目录/坏 XML 报错路径。
- 规模：1.5 人日。

**A-2 ninja_parity 报表接入 unsupported 率曲线**
- 目标：`parity_baseline_report` 增打每 build 与聚合的 `词条总数 / parsed / unsupported / err` 计数与百分比（report-only，不进门禁断言），满足 M5 验收"unsupported 词条率下降曲线纳入报表"。实现上复用 A-1 的分类函数（从 pobr-cli lib 提为 pobr-build 可达的位置——放 `crates/pobr-build/src/corpus.rs`，pobr-cli 反向复用）。
- 涉及文件：`crates/pobr-build/src/corpus.rs`（新）、`crates/pobr-build/src/lib.rs`（导出）、`crates/pobr-build/tests/ninja_parity.rs`（报表段追加）。
- 测试：corpus.rs 单测（归一化/分类）；ninja_parity 报表跑通。
- 规模：1 人日（与 A-1 同 agent 串行，A-1 完成后小改）。

**A-3 vendor special 覆盖率对账（sync-pob-catalog `check --special-coverage`）**
- 目标：纯文本层枚举 vendor specialModList 的全部 pattern key（regex 抓 `ModParser.lua:2231-6150` 区段的 `\["((?:[^"\\]|\\.)*)"\]\s*=` 字面量；sync-pob-catalog 已有 regex 依赖、check 命令已是同风格），对照 special_mods.json 的 `vendor_pattern` 列做存在性 diff：输出 `vendor 总数 / 已迁移 / 未迁移`，附未迁移清单（供 M6 长尾批次直接消费）。同时校验 special_mods.json 内 `vendor_pattern` 在 vendor 中仍存在（漂移告警，R3 防线）。
- 涉及文件：`tools/sync-pob-catalog/src/lib.rs`（新模块 `special_coverage.rs`）、`src/main.rs`（check 分支扩参）。
- 测试：以内嵌小段 Lua 文本为 fixture 的抽取单测；对真实 vendor 跑通并断言总数 ≥2000（防区段定位失效静默归零）。
- 规模：1 人日。

### Track B — special 框架：schema + 解释器 + 接入（核心串联件）

**B-1 catalog schema（`parser_rules.rs`）**
- 目标：§2.1 的 `SpecialTemplateDef / ModTemplate / ValueExpr / TemplateTag` serde 类型 + `special_mods/v1`、`special_derived/v1` 文档类型。全部字段 `#[serde(default)]` / `Option`（R7）。
- 涉及文件：`crates/pobr-data/src/catalog/parser_rules.rs`（新）、`crates/pobr-data/src/catalog/mod.rs`（挂载 + re-export）。
- 测试：serde 往返、未知字段容忍、缺省值。
- 规模：1 人日。**本项是契约冻结点：合并后 C/D 才能动数据与对拍。**

**B-2 SpecialModRules 解释器（`rules/special_mod.rs`）**
- 目标：§2.2 API。载入期编译（`RegexSet` 预筛 + 逐条 `Regex`，整行锚定、输入小写规范）；实例化（`$n` 求值、五算子、enums 闭集查表、tags 占位填充、target 包装——enemy → EnemyModifier LIST（M3 通道）、minion → MinionModifier 同型）；handler 路由（`HandlerRegistry::get`，未注册 → 空 mods + 标记）。**DSL 单一实现（总架构评审裁决）**：`$n`/五算子/受限谓词的求值**复用 M3 落地的 `rules/value_expr.rs`**（config/special/parser 三处同一套受限语言，禁三套方言）；enums 闭集映射作为其受限扩展实现在 special_mod.rs 侧（M6 的 `:cap` 同理）。`HandlerRegistry` 签名扩为 `Fn(&HandlerCtx) -> Vec<Modifier>`，`HandlerCtx { nums: Vec<f64>, words: Vec<String> }`（同步改 registry.rs 既有测试）。
- vendor 参照：ModParser.lua:6362-6385（scan 的"最早+最长"语义本阶段**不需要**——special 是整行锚定匹配，无段切除）、:6155-6158（锚定）、:2155-2230（三个辅助构造器，作为 handler 实现参照）。
- 涉及文件：`crates/pobr-core/src/rules/special_mod.rs`（新）、`rules/mod.rs`、`rules/registry.rs`（签名扩参）。
- 测试：模板实例化逐变体（数值/算子链/enums/tags 占位/target 三态）；编译期错误路径（非法 regex / 越界 `$n` / 未知算子）；`verified:false` 透传。
- 规模：2.5 人日。

**B-3 mod_parser 接入 + keystone special 搬迁**
- 目标：`parse_mod_with_rules` 双签名（§2.3）；special 查表插入主流程（§2.2 优先级）；把 `parse_keystone_special` 的全部硬编码条目（CI / no-mana / EB / ES→Mana 两形 / `Your X is N%` 动态 OVERRIDE / 免疫短语）翻译为 special_mods.json 条目。
- **搬迁不变式执行法**：commit-1 = 接入 + 数据条目落库，但 special 命中后**仍 fall through 到旧 `parse_keystone_special` 双跑比对**（debug_assert / 单测枚举全部既有条目断言两路输出 Modifier 逐字段相等），golden diff=0；commit-2 = 删旧函数（行为等价已被 commit-1 测试锁定）。
- 涉及文件：`crates/pobr-core/src/mod_parser.rs`、`data/4.5.0.3.4/overlay/special_mods.json`（S0 批次的 keystone-effect 部分）。
- 测试：既有 mod_parser 测试全绿不改动（搬迁不变式的另一面）；新增 special 命中/不命中边界（前后缀多字符不得命中——锚定验证）。
- 规模：2 人日。

**B-4 RuleSet 扩域 + gamedata loader + orchestrator 注入**
- 目标：§2.4 链路。`pobr-gamedata` 新 domains loader（special_mods + special_derived，缺表 → None）；`RuleSet.special_mods`；`CalcOrchestrator` 构建期 compile 一次、全部 ingest 调用点切 `parse_mod_with_rules`；`session.rs` 的 unsupported 收集面不变。
- 涉及文件：`crates/pobr-gamedata/src/ruleset.rs`、`crates/pobr-gamedata/src/domains/`（新文件）、`crates/pobr-build/src/calc_orchestrator.rs`、`crates/pobr-build/src/build_data.rs`（若 BuildData 是注入载体，以届时形态为准）。
- 测试：loader 缺表/坏 JSON 路径；orchestrator 端到端——一条 special 词条从 XML 进、Modifier 出（带 SourceId 归因：special 条目沿用词条所在来源的 SourceId，`special_meta.entry_id` 进 breakdown 文本）。
- 规模：1.5 人日。

**B-5 Keystone LIST 通道（行为改动，独立 commit + PoB2 依据）**
- 目标：物品/天赋词条 `"<keystone name>"` 整行 → `Keystone LIST` mod（数据由 C-1 的 special_derived 提供）→ **展开复用 M3 已交付的 `calc/keystone_merge.rs` + `session.set_keystone_mods` 通道**（env_finalize 阶段 1/5，含 HashSet 去重与 `SourceKind::GrantedKeystone` 归因；总架构评审裁决：不得在 orchestrator 另建第二个展开点）。M2 `keystone_registry` 的开关经注入 mods 内含的 flag 自动接通（M3 蓝图 §8.2 既有约定）。
- 本项实际工作收窄为：①special/special_derived 条目产出 `Keystone LIST` mod（解析面，经 B-3 接入的 special 查表）；②确认 pobr-build 的 keystone_name→mods 注入表（M3-E2 已建）覆盖 special 来源；③端到端 fixture + 去重测试。
- vendor 参照：ModParser.lua:6151-6158（产 LIST）+ CalcPerform.lua:66-76（mergeKeystones，M3 已对照实现）。
- 涉及文件：`data/.../special_mods.json`（条目）、`crates/pobr-build/`（set_keystone_mods 喂数核验，预计零改动或小 patch）。
- 测试：fixture——含 `Malachai's Awakening` 型"物品授予关键石"的合成 build XML，断言 keystone 效果生效；重复授予去重（同 keystone 物品+树同时有 → 只生效一次，对照 PoB2 行为，开放问题 5 保留）。
- 规模：1 人日（评审后由 2 人日下调：展开通道复用 M3，不再新建）。

### Track C — 数据条目批次 + handler 首批 + 闸门测试

**C-1 special_derived 派生步骤（adapter）**
- 目标：§2.5。`pobr-data-adapter` 新增 `--emit-special-derived` 步骤（或并入主流程）：读 `base/passive_tree.json` keystone 节点 → 确定性产 `generated/special_derived.json`。
- 涉及文件：`tools/pobr-data-adapter/src/`（新 `special_derived.rs` + main.rs 接线）、`data/4.5.0.3.4/generated/special_derived.json`。
- 测试：派生确定性（同输入 byte-stable）；接入 M0 "generated 重生一致" CI 脚本（`devs/` 下既有 regen-check 清单加一行）。
- **与 M6 的衔接**：M6-T7 将把本步骤的生产迁入 `tools/precompile-mods`（统一 generated 层生产点）并扩展 per-gem chains/pierce 与 skill_names 段——schema 不变，迁移 commit 须对 keystone 段 byte 等价，adapter 步骤随之退役。
- 规模：1 人日。

**C-2 S1/S2 批次条目策展（主工作量）**
- 目标：按 A-1 报表排序迁移：
  - **S0**（随 B-3 落）：keystone-effect 特例 ~12 条 + special_derived ~36 条（C-1 自动）。
  - **S1**：缺口语料中 `builds_hit ≥ 2` 且 `pob2_parses: true` 的全部模板，预算 **60–100 条**。
  - **S2**：`builds_hit = 1` 但属 DPS/防御主通道（出现在 meta.json 偏差最大的 build）的模板，预算 **~50 条**。
  - 首波合计目标 **~150–200 条**（占 2085 的 ~8–10%）；其余长尾按 roadmap 留 M6（覆盖率驱动批次，A-3 清单直接消费）。
- 策展流程（每条）：vendor 原条目定位（A-3 清单给 pattern → 实施 agent 在 ModParser.lua grep 行号）→ 判定纯模板/真逻辑 → 写 JSON 条目（`vendor_pattern` 必填、`verified:false` 起步）→ Track D 对拍 → 通过则 `verified:true`。**纯表条目**（vendor value 无闭包）可直接照抄结构；**闭包条目**人工翻译模板，正确性完全交给 D 的 differential（"以运行时输出对拍为准、不以源码读得对为准"——P13 注 4）。
- 涉及文件：`data/4.5.0.3.4/overlay/special_mods.json`（独占）。
- 测试：每批次附 3–5 条代表性词条的 mod_parser 集成测试（fixture 行 → 期望 Modifier）；批次合并 commit 的 ninja_parity 报表中 unsupported 率必须下降（A-2 曲线即证据）。
- 规模：S1 2.5 人日 + S2 1.5 人日（条均 ~10 分钟：定位/翻译/对拍/登记）。

**C-3 handler 首批实现**
- 目标：缺口语料中无法入 DSL 的高频真逻辑条目。首批预算 **≤12 个 handler**：
  - `special:explode_on_kill`（explodeFunc 等价，ModParser.lua:2217-2230 参照；enemy-explosion 的计算消费若 calc 侧无落点，handler 产出 PoB2 同款 LIST mod 即可——值进 ModDb、消费缺口另登记，不在本阶段扩 calc）；
  - `special:granted_extra_skill` / `special:trigger_extra_skill`（:2155-2175 参照；与 M4 trigger_configs 接线对齐）；
  - 其余按 A-1 报表实际命中决定（DOUBLED 类若 M4 globalLimit 已落则入 DSL 不占 handler）。
- 涉及文件：`crates/pobr-core/src/rules/handlers/`（新目录，每 handler 一文件 + `mod.rs` 统一注册函数 `register_special_handlers(&mut HandlerRegistry)`）、`crates/pobr-build`（构建 registry 处调注册函数）。
- 测试：每 handler 单测（输入捕获 → Modifier 断言）；oracle 对拍样本。
- 规模：2 人日。

**C-4 闸门与覆盖清单测试（CI 原生）**
- 目标：把 §0.3 监控线落成测试，满足"handler 覆盖清单跑通（未映射告警）"：
  1. pobr-core 测试：读仓库 `data/4.5.0.3.4/{overlay/special_mods.json, generated/special_derived.json}`，`SpecialModRules::compile` 全量成功；所有 `handler_id` 均已注册（未注册 = 测试失败 + 打印未映射清单）；`registry.len() < 100`；handler 条目数 / special 总条目 < 10%。
  2. 条目唯一性：id 唯一、pattern 编译唯一（两条 pattern 等价字符串视为冲突报错）。
  3. `verified:false` 计数打印（报表，不断言）。
- 涉及文件：`crates/pobr-core/tests/special_mods_gate.rs`（新）。
- 规模：0.5 人日。

### Track D — pob2-oracle parseMod differential（验证基座）

**D-1 oracle parseMod 模式**
- 目标：`oracle.lua` 增 `--mode parsemod`：从 stdin（或 `--lines-file`）逐行读词条文本，headless 引导后调 `modLib.parseMod(line)`（两 pass，对照 ModParser.lua:7109-7133 cache 闭包的 order=1/2 语义），输出 JSONL：`{line, mods:[{name,type,value,flags,keywordFlags,tags}], unsupported:bool}`（flags/keywordFlags dump 为位名数组——oracle.lua 内做位→名反查，Global.lua 的 ModFlag 表就在 vendor）。
- 涉及文件：`tools/pob2-oracle/oracle.lua`、`run.sh`（透传模式参数）、`README.md`。
- **与 M6 的衔接**：M6 §5.3 的 parseMod differential **复用本模式**（模式名固定 `--mode parsemod`，M6 只做输出字段增量，不另起模式）。
- 测试：脚本级 smoke（3 条已知词条的期望输出，luajit 缺席时跳过——复用 extract_lua.rs 的 `luajit_available` 模式）。
- 规模：1.5 人日。

**D-2 Rust 侧 differential harness**
- 目标：对 special_mods.json 每条 entry：用 2–3 组样本值实例化 pattern（数值槽取 {1, 37, 100}，enum 槽逐值展开）→ 同一行喂 D-1 oracle 与 `parse_mod_with_rules` → 规范化比较（name 字符串等同；type 映射表；value 容差 1e-9；flags/keywordFlags 位名集合相等；tags 结构等同——pobr ModTag 与 PoB2 tag 的字段名映射表落在 harness 内、一处维护）。全 diff 通过 → 该条目可置 `verified:true`（**人工改 JSON + 独立 commit**，harness 只产报告不写数据，守"产物禁手改"语义的对偶：verified 是策展元数据、属人工列）。
- 涉及文件：`crates/pobr-build/tests/special_oracle_differential.rs`（`#[ignore]` 标注，本地/CI 手动跑——依赖 luajit，不进默认门禁）+ 报告输出。
- 测试：harness 自身的规范化比较单测（构造双侧 JSON）。
- 规模：2 人日。

### Track E — statdesc 抽取 + 渲染器 + 离线 diff（R5 主线，完全独立）

**E-1 statdesc 抽取（extract-lua 新 kind）**
- 目标：luajit `loadfile` 执行 `Data/StatDescriptions/*.lua`（10 份 + Specific_Skill 子目录按需），把返回 table 序列化为 `overlay/stat_descriptions.json`（§物理层归属见 §7 开放问题 1；本蓝图按 overlay/ 执行）。schema（`catalog/stat_descriptions.rs` 新增）：
  ```jsonc
  { "_meta": {"schema": "stat_descriptions/v1", ...},
    "scopes": [ { "name": "stat_descriptions", "parent": null,
        "descriptors": [ { "stats": ["stat_id_1", ...],
            "lang": [ { "limits": [["#","#"]],          // 每槽 [min,max]，"#"=无界，"!"=排除值
                        "text": "{0}% increased ...",
                        "specs": [{"k":"divide_by_one_hundred","v":1}],
                        "gem_quality": false } ] } ],
        "index": { "stat_id_1": 0 } } ] }
  ```
- vendor 参照：数据形态见 stat_descriptions.lua 头部（limit/text/stats 结构已确认）；scope/parent 链语义 StatDescriber.lua:13-39。
- 涉及文件：`tools/sync-pob-catalog/src/extract_statdesc.lua`（新引导脚本）、`extract_lua.rs`（扩 kind 分发；保持 byte-stable 纪律：Rust 侧统一排序/数字格式）、`main.rs`（参数）、`crates/pobr-data/src/catalog/stat_descriptions.rs`（schema）。**catalog/mod.rs 的挂载行由 B-1 一并预留（见 §4.2 共享文件归属）。**
- 测试：抽取确定性（重跑 byte-diff 零，进 M0 可再生 CI 清单）；3 条已知 descriptor 的 golden 断言。
- 规模：2 人日。

**E-2 渲染器（adapter 内，离线）**
- 目标：`tools/pobr-data-adapter/src/statdesc.rs`：`fn render_mod_lines(stats: &[(stat_id, min, max)], scope: &StatDescScope) -> Vec<RenderedLine>`，等价复刻两层 vendor 语义：
  1. **StatDescriber.lua:41-118 matchLimit + 描述选择**（depth/order 排序、gem_quality 行跳过、零值 stat 跳过）；
  2. **spec.k 数值变换全集**（StatDescriber.lua:120-230，~40 种：divide_by_one_hundred 族 / per_minute_to_per_second 族 / milliseconds_to_seconds 族 / negate / double / canonical_line 等——逐分支移植为 Rust enum，未知 k 报错不静默）；
  3. **Export 侧区间渲染**（`Export/Scripts/statdesc.lua` describeStats：min==max → `5`，否则 `(5-8)`；`{0}` / `{0:+d}` 占位格式）——diff 对照物是 ModItem.lua 的 Export 产物，必须用 Export 形态而非运行时单值形态。
- 涉及文件：`tools/pobr-data-adapter/src/statdesc.rs`（新）、`lib.rs/main.rs` 接线（新子命令 `statdesc-check`，见 E-3）。
- 测试：spec.k 逐变换单测（每种 ≥1 例）；matchLimit 边界（`#`/`!`/区间）。
- 规模：3 人日（渲染器是本 track 最大单件）。

**E-3 离线逐行 diff 验证（R5 闸门）**
- 目标与流程：
  1. `sync-pob-catalog extract-lua --kind mod-item-texts`：luajit 序列化 8 份 `Data/Mod*.lua`（Item/ItemExclusive/Jewel/Flask/Charm/Corrupted/Runes/Veiled）为 fixture `{mod_id: [text_lines]}`——**输出到 `--out` 指定的临时路径，不入 `data/`**（它是验证对照物不是运行时数据）。
  2. `pobr-data-adapter statdesc-check --catalog data/4.5.0.3.4 --vendor-texts <fixture>`：对 mods.json 与 fixture 的 **id 交集**逐条渲染、逐行 diff。报告：per-file 行一致率、不一致样本（双侧并排）、渲染失败（无 descriptor / 未知 spec.k）清单、仅单侧存在的 id 计数。
  3. **达标定义**：8 份文件 join 后的全部文本行 **diff = 0**，或剩余不一致逐条进显式白名单（`statdesc_diff_allowlist.json`，每条附原因注释）且白名单 ≤ 总行数 0.5%。
  4. 达标前 `rendered_lines` **不得**写入 mods.json（R5：「离线逐行 diff 达标才转生产列」）。
- 涉及文件：`tools/sync-pob-catalog/src/extract_moditem.lua`（新）、`tools/pobr-data-adapter/src/statdesc_check.rs`（新）。
- 测试：diff 器自身单测（构造双侧不一致样本）；对真实数据跑通并把首轮一致率写进 PR 描述（基线证据）。
- 规模：1.5 人日。

**E-4 转生产列（达标后，独立 commit）**
- 目标：adapter 主流程对 mods.json 开启 `rendered_lines: [String]` 列（`ModDef` 增 `#[serde(default)]` 字段）；重跑 pipeline regen-check（M0 防线）确认 byte-stable。**本阶段不接任何运行时消费方**（craft/uniques 渲染属 M5c/M6）——因此该 commit parity 必然逐值不变，commit message 注明。
- 涉及文件：`crates/pobr-data/src/catalog/mods.rs`（字段）、`tools/pobr-data-adapter/src/mods.rs`、`data/4.5.0.3.4/base/mods.json`（再生）。
- 测试：serde 兼容（旧 JSON 无该列仍可读）；regen 一致。
- 规模：1 人日。

---

## 4. 并行 track 切分

### 4.1 依赖与时序

```
契约冻结：B-1（schema）──┬──> C-1/C-2/C-3（数据与 handler）
                          ├──> D-2（differential harness）
                          └──> B-2 → B-3 → B-4 → B-5（框架链，组内串行）
A-1 → A-2 → A-3（组内串行；A-1 报表是 C-2 选批的前置）
D-1（独立，随时可做；A-1 的 pob2_parses 列与 C-2 的 verified 流程依赖它）
E-1 → E-2 → E-3 → E-4（组内串行，整组与 A/B/C/D 零文件交集——除 catalog/mod.rs 一行，见下）
```

必须串行的关键先后序：
1. **B-1 合并 = 契约冻结点**，之后 C/D/E 的 schema 引用才稳定（建议 B-1 单独快速 PR，第 1 天合）。
2. **B-3（接入）先于 C-2 条目生效**——条目可以先写（JSON 是死数据），但 parity 影响要等接入；C-2 的批次 commit 安排在 B-4 合并后，每批独立 commit 看 A-2 曲线。
3. **D-2 先于任何 `verified:true` 标记**。
4. **E-3 达标先于 E-4**（R5 闸门，硬序）。
5. B-5（Keystone LIST）是**行为改动**，排在 B 链最后、独立 commit + baseline 显式审查。

### 4.2 文件归属表（每 track 独占写；共享文件指定唯一责任人）

| 文件/目录 | 归属 | 说明 |
|---|---|---|
| `apps/pobr-cli/src/{lib,main}.rs` | **A** 独占 | corpus-report 命令 |
| `crates/pobr-build/src/corpus.rs`（新） | **A** 独占 | |
| `crates/pobr-build/tests/ninja_parity.rs` | **A** 独占（本阶段） | 仅追加报表段；基线常量更新按 §5.3 纪律走独立 commit，由触发该更新的 track 提出、A 审 |
| `tools/sync-pob-catalog/src/special_coverage.rs`（新） | **A** 独占 | |
| `tools/sync-pob-catalog/src/main.rs` | **共享：A 负责** | A 先落"check --special-coverage / extract-lua kind 分发骨架"两处分发臂（D0+1 合并）；E 之后只在 `extract_lua.rs` 与新 .lua 文件内活动 |
| `tools/sync-pob-catalog/src/extract_lua.rs` + `extract_statdesc.lua` + `extract_moditem.lua` | **E** 独占 | kind 分发臂骨架由 A 预留后 E 填实 |
| `crates/pobr-data/src/catalog/parser_rules.rs`（新） | **B** 独占 | |
| `crates/pobr-data/src/catalog/stat_descriptions.rs`（新） | **E** 独占 | |
| `crates/pobr-data/src/catalog/mod.rs` | **共享：B 负责** | B-1 一次性加两行挂载（parser_rules + stat_descriptions 的 `pub mod` + re-export），E 不碰此文件 |
| `crates/pobr-data/src/catalog/mods.rs` | **E** 独占（E-4 加 rendered_lines 字段） | |
| `crates/pobr-core/src/rules/{special_mod.rs, registry.rs, mod.rs}` | **B** 独占 | registry 签名扩参在 B-2 |
| `crates/pobr-core/src/rules/handlers/`（新目录） | **C** 独占 | B-2 合并后开工 |
| `crates/pobr-core/src/mod_parser.rs` | **B** 独占 | |
| `crates/pobr-core/tests/special_mods_gate.rs`（新） | **C** 独占 | |
| `crates/pobr-gamedata/src/{ruleset.rs, domains/*}` | **B** 独占 | 开工前确认 W3 已合并，rebase 其上 |
| `crates/pobr-build/src/calc_orchestrator.rs` | **B** 独占 | M5a（minion actor）/M5c（物品编辑态）同期也可能碰此文件——**跨蓝图冲突**，见 §6 风险 5 |
| `crates/pobr-build/tests/special_oracle_differential.rs`（新） | **D** 独占 | |
| `tools/pob2-oracle/*` | **D** 独占 | |
| `tools/pobr-data-adapter/src/{statdesc.rs, statdesc_check.rs}`（新） | **E** 独占 | |
| `tools/pobr-data-adapter/src/{special_derived.rs（新）, main.rs}` | **共享：C 负责** | adapter main.rs 的两处接线（C 的 emit-special-derived、E 的 statdesc-check 子命令）由 C 先落分发骨架，E 填实自己的分支体 |
| `data/4.5.0.3.4/overlay/special_mods.json`（新） | **C** 独占 | S0 批次条目内容由 B-3 提供文案、C 落盘（避免双写） |
| `data/4.5.0.3.4/overlay/stat_descriptions.json`（新） | **E** 独占（工具产物） | |
| `data/4.5.0.3.4/generated/special_derived.json`（新） | **C** 独占（工具产物） | |
| `data/4.5.0.3.4/base/mods.json` | **E** 独占（E-4 再生） | |

5 个 track 对应 5 个 worktree agent；A/D/E 第 0 天即可开工（A 的 main.rs 骨架 PR 最先合），B 第 0 天做 B-1 冻结契约，C 在 B-1 + A-1 之后进入主工作量。

### 4.3 批次定义（C-2 引用）

| 批次 | 内容 | 进入条件 | 预算 |
|---|---|---|---|
| S0 | keystone 派生（special_derived，自动）+ keystone-effect 特例搬迁 | 随 B-3/C-1 | ~48 条 |
| S1 | ninja 语料 `builds_hit ≥ 2` 且 `pob2_parses: true` 全量 | A-1 报表 + B-4 合并 | 60–100 条 |
| S2 | `builds_hit = 1` 的 DPS/防御主通道模板 | S1 合并、曲线确认下降 | ~50 条 |
| M6 长尾 | A-3 未迁移清单驱动 | 出本阶段范围 | 余量 ~1900 条 |

---

## 5. 门禁与验收

### 5.1 各 track 局部门禁

| Track | 局部门禁 |
|---|---|
| A | 报表确定性（同 fixture 同输出）；A-3 对真实 vendor 抽取条目数 ≥2000 断言 |
| B | B-3 搬迁双跑测试（旧 keystone special vs 数据条目逐字段相等）golden diff=0；B-5 行为 commit 附 CalcSetup 行号依据 + baseline 独立审查 |
| C | C-4 闸门测试常绿：handler 全注册、`len()<100`、handler 占比 <10%、id/pattern 唯一；每数据批次 commit 的 ninja_parity unsupported 率严格下降（A-2 报表数字写入 commit message） |
| D | differential harness 对 S0 批次全绿后才接受 S1 的 `verified:true`；oracle 输出与 README 用例 smoke |
| E | E-1 抽取 byte-stable（重跑 diff=0，纳入 M0 regen-check 清单）；E-2 spec.k 全分支测试；**E-3 达标线 = diff=0 或白名单 ≤0.5% 且逐条注明原因——未达标禁止 E-4** |

### 5.2 阶段整体验收（M5b 完成定义）

1. 门禁三件套全绿（§0.2 原文）。
2. ninja_parity **零回归**（基线四常量不降）；special 不设 parity 提升硬指标（M5 验收口径是曲线不是百分比），但 **unsupported 词条率曲线在报表中可见且相对开工基线下降**（roadmap 原文："unsupported 词条率下降曲线纳入报表"）。
3. **special 迁移条目 oracle 抽样对拍**（roadmap 原文）：S0+S1+S2 中 `verified:true` 占比 ≥80%；`verified:false` 条目在 differential 报告单列且每条附原因（oracle 不可跑 / PoB2 行为依赖未实现机制等）。
4. handler 覆盖清单跑通：C-4 测试在 CI 常绿，未映射 handler_id = CI 失败（"未映射告警"落成硬门禁）。
5. statdesc：E-3 diff 报告达标 + E-4 rendered_lines 已是 mods.json 生产列 + regen-check 通过。若 E-3 久攻不下（见 §6 R5 回退），允许 M5b 以"E-3 报告 + 白名单收敛计划"结项、E-4 顺延 M6——但必须显式登记，不得静默。
6. A-3 覆盖率报表数字写入收尾 commit：`vendor 2085 中已迁移 N（含 verified true/false 分布）`，作为 M6 长尾批次的起点基线。

### 5.3 baseline 纪律（roadmap §0 引用）

unsupported 率下降通常伴随 parity 命中变化：每个使命中数上升的批次 commit，按"baseline 更新独立 commit、显式审查"执行——批次数据 commit 不动基线常量，确认提升后由独立 commit 上调 `BASELINE_*`，permanent ratchet（注释原文：仅确认整体提升时上调，永不下调）。

---

## 6. 风险与回退（风险登记簿落点）

| 风险（roadmap 附 C 原文摘录） | 本阶段具体落点 | 缓解/回退 |
|---|---|---|
| **R1** 模板 DSL 复杂度膨胀（最大架构风险）："DSL 硬边界入 review checklist；≥20 条目闸门；handler 计数 <100 监控" | C-2 策展中会不断遇到"差一点就能模板化"的条目，诱惑加 DSL 能力；本蓝图已带一项 enums 微扩展 | §0.3 原文进 PR review checklist；enums 扩展单独 PR、附 ≥20 条受益清单实证；后续任何扩展同样走"清单实证 + 架构 review"；被拒条目一律 handler 或留长尾 |
| **R4** special 2085 条验证成本："分批 + verified:false + 长尾 Unsupported；不追求 100% JSON 化" | C-2 条均成本若超预算（>15 分钟/条）首波 150–200 条交付不完 | 批次可裁剪：S2 整批可砍、S1 按报表序砍尾；交付下限 = S0 + S1 的 builds_hit≥3 子集；**绝不**为赶量跳过 D 对拍（verified:false 照样可合，单列即可） |
| **R5** statdesc 渲染污染下游："离线逐行 diff 达标才转生产列；长期 stat_id 直通降权文本通道" | E-3 不达标的可能原因：spec.k 长尾、Export 与运行时渲染形态差异、mods.json 字段缺失（如 stat_order 影响行序） | 硬闸门已落 E-3→E-4 串行；行序不一致先按"行集合相等"降级比较并登记；达标失败 → E-4 顺延 M6、rendered_lines 保持不存在（无消费方，零污染）；diff 报告即首轮基线证据留档 |
| **R3** extract-lua 抽取正确性 / vendor 漂移 | E-1 statdesc 抽取、A-3 文本抓取、D-1 oracle 都依赖 vendor 当前 commit | OverlayMeta 记 vendor_commit（既有模式）；A-3 自带漂移告警；oracle differential 是终裁（P13 注 4）；vendor 更新时三件套全部重跑 |
| **R2** 搬迁破坏隐藏补偿 | B-3 keystone special 搬迁、special 查表插入主流程位置 | 双跑测试 + golden diff=0（B-3 已写入工作项）；special 插入点选在 parse_keystone_special 原位**之前紧邻**而非重排专用函数族顺序 |
| 跨蓝图并行冲突（M5 三 worktree） | `calc_orchestrator.rs`（B-4/B-5 vs M5a minion actor vs M5c 物品编辑态）；`catalog/mod.rs`（B-1 vs M5a/M5c 各自挂表） | M5 三蓝图的 orchestrator 改动各自走小 PR 快合、谁后合谁 rebase；catalog/mod.rs 同此；若 M5a/M5c 蓝图另有约定以总协调为准 |
| W3 进行中（开工时代码可能半切换） | ruleset.rs / RuntimeConstants 形态与本文 §1 描述不符 | §0.4 前提 2：B track 开工第一步 = 读届时 ruleset.rs 实形并按 §2.4 意图适配，契约以"RuleSet 上加 Option 域"不变式为准 |

---

## 7. 开放问题（实施前需裁决）

1. **stat_descriptions.json 物理层归属**：20 文档 §3.1 表把它列在 `base/`，但其抽取源是 vendor `Data/StatDescriptions/*.lua`（经 extract-lua），按 P1 层定义（base = pipeline+adapter 自 .dat 全自动再生）应落 `overlay/`。本蓝图按 **overlay/** 执行（与 CI 防线语义一致）；待 pipeline 直接下载 `stat_descriptions.csd` + adapter 自写 csd 解析后再迁 base/（彼时 vendor 抽取版可退役为对拍参照）。**需架构责任人确认或修订 20 文档 §3.1 表注。**
2. **enums 受限闭集映射**（§2.1）是否获准入 DSL：本蓝图已按 ≥20 条目闸门论证；若否决，回退 = 元素笛卡尔展开（条目膨胀 ~4 倍但零语义损失）。
3. **parse_mod 双签名过渡**（§2.3）vs 一步改全量调用方签名：本蓝图选双签名（M6 收口），代价是 M5b~M6 间存在两个入口；若总协调要求一步到位，B-3/B-4 规模各 +0.5 人日。
4. **zh-TW statdesc**：vendor 仅英文；i18n 边车的繁中渲染无 PoB2 对照基准，本蓝图判定 **out of scope**（不阻塞 R5 闸门），后续由 i18n 通道独立立项。
5. **`Keystone LIST` 重复授予去重的 PoB2 精确语义**（B-5 测试假设"只生效一次"）：实施时以 CalcSetup.lua 实读为准，若 PoB2 允许叠加则修正测试并附行号。

---

## 8. 体量汇总

| Track | 人日 | 关键交付 |
|---|---|---|
| A 语料与报表 | 3.5 | corpus-report + unsupported 曲线 + vendor 覆盖率对账 |
| B special 框架 | 9 | schema + 解释器 + parser 接入 + 注入链 + Keystone LIST |
| C 数据与 handler | 7.5 | special_derived + S0/S1/S2 条目 + handler 首批 + 闸门测试 |
| D oracle differential | 3.5 | parsemod 模式 + 对拍 harness |
| E statdesc | 7.5 | 抽取 + 渲染器 + 离线 diff + 转生产列 |
| 合计 | **~31 人日**（5 agent 并行下挂钟 ~9–10 天，C 为关键路径尾段） | |
