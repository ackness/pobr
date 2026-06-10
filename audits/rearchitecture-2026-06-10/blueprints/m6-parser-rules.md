# M6 实施蓝图 — 解析规则全量数据化 + stat_id 直通

> 撰写：2026-06-11 · 基于 roadmap M6 节、架构裁决 P3/P4/P7/P10/P13/P18、审计 10-mod-system.md（10-G1/10-G2）
> 本文自包含：实施 agent 只需读本蓝图 + 代码即可开工。规模 ~4 人周，6 个 track。
> 战略目标一句话：**ModParser 六表入 JSON、mod_parser 重写为数据驱动 scan 引擎、建立 stat_id 第二通道、第二次 version-bump-drill 作为整个重构（M0–M6）的终局验收。**

---

## 0. 前置状态与硬约束

### 0.1 开工时已具备（M0–M5 交付）

- 三层数据目录 `data/4.5.0.3.4/{base,overlay,generated}` + manifest v2（三段 domains，`crates/pobr-gamedata/src/manifest.rs`）。
- overlay merge 引擎（`crates/pobr-gamedata/src/overlay.rs`）+ `RuleSet` 聚合入口（`crates/pobr-gamedata/src/ruleset.rs`——**注意：`ParserRules` 当前是空占位 struct（:15），`RuleSet.parser_rules: Option<ParserRules>`（:31）就是 M6 要填充的注入点**；M0-W3 正在把 `game_constants` 填实，M6 开工时假设该管道已存在且模式可照抄）。
- handler 注册表骨架（`crates/pobr-core/src/rules/registry.rs`）。
- `sync-pob-catalog extract-lua` 子命令（`tools/sync-pob-catalog/src/extract_lua.rs` + `extract_skill_overrides.lua`）：luajit 执行 vendor Lua → JSONL → Rust 侧排序/序列化保证 byte-stable，`_meta` 头记 vendor commit + regen_command。**M6 的六表抽取直接复用这套骨架**（resolve_luajit / OverlayMeta / byte-stable 纪律全部照抄）。
- CI 防线：`devs/scripts/regen-check.sh`（重跑 byte-diff 零）+ pobr-data 禁内嵌大数组 lint。
- pob2-oracle（`tools/pob2-oracle/oracle.lua`，headless 引导 PoB2 计算 dump）；ninja_parity 18-build 门禁（`crates/pobr-build/tests/ninja_parity.rs`，语料在 `examples/demo-bd-test/builds/*/decoded.xml`）。
- M5b 应已交付 `overlay/special_mods.json` 主体（specialModList 高频批次）+ `rules/special_mod.rs` 模板实例化（P4）。**M6 不重做 special，只做：①新 scan 引擎接入 special 通道；②覆盖率驱动的长尾批次收尾。**

### 0.2 执行纪律（roadmap §0 原文，逐条适用）

1. `cargo test --workspace` + `clippy -D warnings` + `fmt --check` 全绿；
2. **ninja_parity 18-build 零回归**——防御/进攻当期 baseline @5% 容差不得倒退；
3. **搬迁不变式**：纯搬迁（数据出代码、入 JSON）commit，parity baseline 逐值不变（golden diff=0）；搬迁与行为改动永远分两个 commit；
4. 行为修复必须附 PoB2 一手依据（源码行号 / oracle 中间值）；baseline 更新独立 commit、显式审查；
5. 核心改动 feature-gated **双跑对照**，diff 报告干净后才删旧码（本阶段的双跑 = 新旧 parser 全语料对照）。

### 0.3 依赖方向硬约束（P9）

pobr-core 保持零 I/O：新 parser 签名 `parse_mod(text, &CompiledParserRules)`，规则由 pobr-gamedata 加载、pobr-build 注入。不新增 crate（precompile-mods 是 `tools/` 下新 bin，工具不受依赖方向限制）。

---

## 1. vendor 六表逐一分析与 JSON schema 设计

源文件：`vendor/PathOfBuilding-PoE2/src/Modules/ModParser.lua`（7133 行 / 642KB）。各表的 Lua 形态已逐段核实（行号以当前 vendor 检出为准，抽取时以 luajit 执行结果为准、不依赖行号）。

### 1.1 formList（:62-154，91 条 Lua pattern → 27~28 种 form）

**Lua 形态**：`["^(%d+)%% increased"] = "INC"` —— key 是 Lua pattern（含 `^` 锚、`(%d+)`/`([%d%.]+)`/`(%a+)` 捕获、`%%` 转义、`%-` 区间写法），value 是 form id 字符串。无闭包。

实测 form id 集合（28 个，audit 口径 27——抽取时 assert 实测集合并以实测为准）：
`INC RED MORE LESS BASE GAIN LOSE GRANTS GRANTS_GLOBAL REMOVES CHANCE FLAG TOTALCOST BASECOST PEN REGENFLAT REGENPERCENT DEGENFLAT DEGENPERCENT DEGEN DMG DMGATTACKS DMGSPELLS DMGTHORNS DMGTHORNSBASE DMGBOTH OVERRIDE DOUBLED`

**JSON schema**（`forms` 段）：

```json
{ "pattern": "^(%d+)%% increased", "form": "INC" }
```

裁决：**pattern 原样保留 Lua pattern 语法**，不在抽取期翻译成 regex（翻译是引入静默偏差的最大风险源；Rust 侧实现 Lua pattern 子集匹配器，见 §2.2）。抽取期附带派生字段 `literal`（pattern 中最长的连续字面量片段，供 aho-corasick 预过滤，如 `"% increased"`）与 `anchored: true`（`^` 开头）。

### 1.2 modNameList（:157-961，776 条短语 → ModName / 名集 / 带 tag 的结构）

**Lua 形态**三种 value：
- 纯字符串：`["strength"] = "Str"`；
- 字符串数组：`["attributes"] = { "Str", "Dex", "Int", "All" }`；
- 带 tag 的表：`["mana cost of attacks"] = { "ManaCost", tag = { type = "SkillType", skillType = SkillType.Attack } }`（skillType 是数值枚举，**抽取必须反查 SkillType 表还原成名字字符串**）。

无闭包（已实测 `function(` 计数 = 0）。匹配方式：plain 子串匹配（`scan(line, modNameList, true)`）。

**JSON schema**（`name_map` 段）：

```json
{ "phrase": "mana cost of attacks",
  "names": ["ManaCost"],
  "tags": [{ "type": "SkillType", "skill_type": "Attack" }] }
```

`names` 恒为数组（单名也包一层）；tag 结构复用 §1.9 的 `TagTemplate`。pobr 侧 `ModName = StatId(String)`（`pobr-data/src/modifier.rs:8`），vendor 名字字符串直接落 StatId，无需映射表。

### 1.3 modFlagList（:964-1171，202 条短语 → ModFlag/KeywordFlag 位 + 可选 tag）

**Lua 形态**：`["with maces"] = { flags = bor(ModFlag.Mace, ModFlag.Hit) }`、`["with axes or swords"] = { flags = ModFlag.Hit, tag = { type = "ModFlagOr", modFlags = bor(ModFlag.Axe, ModFlag.Sword) } }`。无闭包。plain 匹配。

**抽取关键**：bor 出来的是位掩码数值——引导脚本遍历 `ModFlag`/`KeywordFlag` 表建「位 → 名字」反查表，把掩码**分解为名字数组**（P1 裁决：位枚举本身留 Rust，JSON 里只存名字，Rust 载入期 `ModFlags::from_names(&[...])` 还原位）。M4 已把 ModFlags 扩到 ~30 位（u64，`pobr-data/src/modifier.rs:34`），名字对齐 PoB2 `Data/Global.lua:222-259`；若个别 vendor flag 名在 pobr 枚举缺位，载入期报错（fail-fast，不静默丢）。

**JSON schema**（`flag_phrases` 段）：

```json
{ "phrase": "with maces", "flags": ["Mace", "Hit"] }
{ "phrase": "with axes or swords", "flags": ["Hit"],
  "tags": [{ "type": "ModFlagOr", "mod_flags": ["Axe", "Sword"] }] }
```

### 1.4 preFlagList（:1174-1421，行首前缀 → flags/tag/wrapper 指令，9 个闭包）

**Lua 形态**：key 是 `^` 锚定的 Lua pattern（含 `[hd][ae][va][el]` 这类字符类糅合 have/deal/heal 变体），value 是结构表，字段超出 flag/tag 还有**包装指令**：`addToMinion = true`、`addToMinionTag = {...}`、`playerTag = {...}`、`addToAura`、`newAura`、`addToSkill`、`applyToEnemy`、`actorEnemy` 等（消费见 §4 LIST 包装）。9 个闭包条目（如按捕获生成 SkillName tag）。

**JSON schema**（`pre_flags` 段）：

```json
{ "pattern": "^minions [cthd][ae][ukva][sel]e? ",
  "add_to_minion": true,
  "literal": "minions " }
{ "pattern": "^golems [hd][ae][va][el] ",
  "add_to_minion": true,
  "add_to_minion_tags": [{ "type": "SkillType", "skill_type": "Golem" }] }
```

字段全集（载入期映射到 Rust `PreFlagDef`）：`flags[] / keyword_flags[] / tags[] / player_tags[] / add_to_minion / add_to_minion_tags[] / add_to_aura / only_add_to_banners / new_aura / new_aura_only_allies / add_to_skill(TagTemplate) / apply_to_enemy / actor_enemy / handler_id?`。9 个闭包走 §1.9 探针推断，推不出的标 `handler_id`。

### 1.5 modTagList（:1424-2136，684 条 per-X/条件短语 → tag 模板，139 个闭包）

**Lua 形态**：纯表条目（`["per power charge"] = { tag = { type = "Multiplier", var = "PowerCharge" } }`）与闭包条目混杂。闭包绝大多数是「捕获值代入固定槽位」：

```lua
["per (%d+) rage"] = function(num) return { tag = { type = "Multiplier", var = "Rage", div = num } } end,
["per brand, up to a maximum of (%d+)%%"] = function(num) return { tag = { type = "Multiplier", var = "ActiveBrand", limit = tonumber(num), limitTotal = true } } end,
["per (%d+)%% (%a+) effect on enemy"] = function(num, _, effectName) return { tag = { type = "Multiplier", var = firstToUpper(effectName) .. "Effect", div = num, actor = "enemy" } } end,
```

即闭包的「逻辑」只有三类：数值捕获 → div/limit/threshold 槽位；字符串捕获 → `firstToUpper` 后拼进 var 名；二者组合。**这正是 P4 受限 DSL 的适用范围**。

**JSON schema**（`tag_phrases` 段，占位符 DSL 见 §1.9）：

```json
{ "pattern": "per (%d+) rage",
  "tags": [{ "type": "Multiplier", "var": "Rage", "div": "$1" }] }
{ "pattern": "per (%d+)%% (%a+) effect on enemy",
  "tags": [{ "type": "Multiplier", "var": "$2:cap+Effect", "div": "$1", "actor": "enemy" }] }
```

注意条目还可能携带 `tagList`（多 tag）与 `newAura` 等 misc 字段，schema 与 pre_flags 共用字段全集。

### 1.6 specialModList 与数据派生区（不入 mod_parser_rules.json）

- specialModList 2085 条 → **已在 M5b 入 `overlay/special_mods.json`**（P4：`{pattern, mods[占位符模板], handler_id?, verified}`）。M6 只做接入与长尾：新 parser 第一步查 special（全行锚定 `^...$`——vendor 在 :6154-6158 给所有 special key 包了 `^..$`，scan 命中且剩余文本为空才采用，语义照搬）。
- 加载期派生条目（:6151-6158 keystone → `Keystone LIST`；:6302-6361 per-gem chains/pierce/projectile、skillNameList/preSkillNameList）→ **入 `generated/special_derived.json`**（架构 §3.3）。**与 M5b 的衔接（总架构评审）**：keystone 派生段已由 M5b-C1 经 pobr-data-adapter 产出——M6-T7 把生产**迁移**进 precompile-mods（统一 generated 层生产点，adapter 步骤退役）并扩展 per-gem chains/pierce 与 `skill_names` 段；迁移 commit 对 keystone 段 byte 等价（搬迁不变式）。schema 同 special_mods + `skill_names` 段。M6 范围内 skillNameList/preSkillNameList 的消费（parseMod 的 order=1/2 两 pass 技能名扫描）实现在引擎里，数据由 generated 表供给。
- `unsupportedModList`（:6161，目前仅 `mirrored`）→ mod_parser_rules.json 的 `unsupported` 段（现 parser 硬编码 `["mirrored","split"]`，`mod_parser.rs:63`——`split` 是 pobr 自加项，迁表时保留并加注释来源）。

### 1.7 小查找表（:6166-6293 + 派生）

| 表 | 形态 | JSON 段 | 说明 |
|---|---|---|---|
| suffixTypes | 短语→后缀名（GainAsFire/ConvertToCold/LifeLeech…） | `suffix_types` | plain 匹配；BASE/GAIN/LOSE/GRANTS 族 form 用 |
| dmgTypes | physical→Physical 等 5 条 | `damage_types` | DMG 族 form 用 |
| penTypes | 短语→\*Penetration | `pen_types` | PEN form 用 |
| resourceTypes | 短语→资源名（数组值存在） | `resource_types` | 注意 vendor 加载期把 `maximum X` 变体就地补进表 |
| regenTypes/degenTypes/costTypes/baseCostTypes | `appendMod(resourceTypes, "Regen"/...)` 派生 | `regen_types`/`degen_types`/`cost_types_map`/`base_cost_types` | **抽取期落最终展开形态**（luajit 执行后表已含派生项，照 dump，保证 byte-stable 且 Rust 不复刻派生逻辑） |
| flagTypes | 短语→`Condition:X` 字符串或 mod 结构（hexproof 特例） | `flag_types` | FLAG form 用；value 双形态：`{ "condition": "Condition:Phasing" }` 或 `{ "mod": {name,value,type} }` |
| 既有小查表 | high_precision_mods.json（**M4 W-A2 落表**）/ local_mods.json（**M5c WI-C1 落表**）——总架构评审裁决：M6 只消费；开工核实存在性，缺失即向对应阶段返工，**不并入 T-A** | 独立 overlay 文件 | mod_db round 例外 / 局部词条白名单 |

### 1.8 `overlay/mod_parser_rules.json` 顶层 schema 与 catalog 类型

```json
{
  "_meta": { "schema": "mod_parser_rules/v1", "generator": "sync-pob-catalog extract-lua",
             "vendor_commit": "…", "extracted_files": ["Modules/ModParser.lua"], "regen_command": "…" },
  "forms":          [ … 91 条 … ],
  "name_map":       [ … 776 条 … ],
  "flag_phrases":   [ … 202 条 … ],
  "pre_flags":      [ … ~250 条 … ],
  "tag_phrases":    [ … 684 条 … ],
  "suffix_types":   [ … ], "damage_types": [ … ], "pen_types": [ … ],
  "resource_types": [ … ], "regen_types": [ … ], "degen_types": [ … ],
  "cost_types_map": [ … ], "base_cost_types": [ … ], "flag_types": [ … ],
  "unsupported":    [ "mirrored" ]
}
```

- 排序纪律：每段按 pattern/phrase 字典序排（Lua pairs 无序 → 必须显式排序才 byte-stable）。
- Rust schema 落 `crates/pobr-data/src/catalog/parser_rules.rs`（架构 §2.1 已点名）：`ModParserRulesDoc / FormDef / NameMapDef / FlagPhraseDef / PreFlagDef / TagPhraseDef / TagTemplate / ValueExpr`。全部字段 `#[serde(default)]`/`Option`（R7 纪律）。
- manifest v2 `overlay` 域登记 `mod_parser_rules`，按域记 schema 版本。

### 1.9 抽取方法：luajit 执行 + 探针式闭包模板推断

复用 `extract_lua.rs` 骨架，新增引导脚本 `extract_mod_parser_rules.lua` + 子命令分支：

1. **执行而非正则啃源码**（P13）：用 pob2-oracle 的 headless 引导方式（`LUA_PATH` 注入 `runtime/lua/`）加载 ModParser.lua 所需依赖（ModTools、Data 的 SkillType/ModFlag/KeywordFlag 表），执行到表构建完成；dump 的是**加载后的最终表**（含 resourceTypes 的 maximum 变体、regen/cost 派生表）。
2. **位掩码 → 名字**：遍历 ModFlag/KeywordFlag/SkillType 表建反查，序列化时分解。
3. **闭包 → 模板（探针推断）**：对闭包 value 条目，用 2 组哨兵捕获值调用（数值哨兵 73/97，字符串哨兵 `"qzx"`/`"wvk"`），对两次输出做结构 diff：
   - 字段值 == 哨兵数值 → 该槽位 = `"$n"`；== 哨兵×k / 哨兵/k → `"$n:mult(k)"/"$n:div(k)"`（实际预期极少）；
   - 字符串字段含 `firstToUpper(哨兵)` 形态 → `"$n:cap"` 拼接模板；
   - 两次输出去哨兵后结构不一致（条件分支闭包）→ **推断失败，条目落 `handler_id`**（`tag_phrase:<pattern 的 stable hash 前 12 位>`），并写进抽取报告。
   - 所有探针推断条目带 `"inferred": true`；oracle differential（T-C）覆盖到的条目转 `verified: true`。
4. **DSL 硬边界**（架构 §5 原文适用）：占位符仅 `$1..$n`、算子仅 `negate/clamp/div/mult/base` + `:cap`（字符串首字母大写拼接，为 tag_phrases 新增，**该扩展受益条目 ~139 个闭包中的大多数，远超 ≥20 条目闸门**）；禁循环/递归/自由表达式。handler 计数监控沿用 <100 总闸门（与 special 共享计数）。
5. 抽取自检：assert 各段条目数（91/776/202/…，与 vendor 实测一致，容差 0）；assert form id 集合；闭包推断成功率入报告。

预估闭包分布：tag_phrases 139、pre_flags 9、其余 0（已实测）。预期 handler 条目 ≤15。

---

## 2. scan.rs 引擎设计

新模块目录 `crates/pobr-core/src/mod_parser/`：`mod.rs`（编排）+ `scan.rs` + `forms.rs` + `template.rs` + `legacy.rs`（旧实现整体平移，双跑期保留）。

### 2.1 「最早 + 最长」匹配语义的精确定义

vendor `scan()`（ModParser.lua:6362-6385）逐字语义：

1. 对小写化的 line，在 pattern 表的**每个** pattern 上做 `find`；
2. 取胜者按序比较：`start 更小` > `start 相等且 end 更大` > `start、end 都相等且 #pattern 更长`；
3. 命中后返回 value、捕获组、以及**切除命中段后的剩余文本**（`line[1..start-1] ++ line[end+1..]`，保留原大小写）；未命中返回 nil + 原文。

**PoBR 确定性补充**：vendor 在三个 tie-break 全相等时由 Lua `pairs()` 的哈希序决定——未定义行为。PoBR 加第四级 tie-break：pattern 字典序小者胜，**并在双跑/oracle diff 中专项观察是否有语料行落入该歧义档**（落入即记录进 `parser_ambiguity_report`，人工对 vendor 实际行为裁决后用 `priority` 字段固定）。

`plain` 变体（modNameList/flagList/suffix/pen/cost 等）= 纯子串查找，无 pattern 语法。

### 2.2 Lua pattern 子集匹配器（`scan.rs` 内部）

formList/preFlagList/tagList 的 pattern 实测只用到 Lua pattern 的一个子集：
`^` 锚、字面量、`%d %a %D %s` 类、`%%`/`%-`/`%.` 转义、字符类 `[...]`（含字面量集合如 `[hd][ae][va][el]`）、量词 `+ - * ?`（`-` 是惰性）、捕获 `(...)`（最多 5 个，引擎按 vendor 同样上限 cap1..cap5）、`$` 锚（special 用）。

实现：手写小型匹配器（~300 行，回溯式，输入已小写化），**不引入 regex crate 做翻译**——理由：(a) Lua `-` 惰性量词与 regex 语义差异是静默错误源；(b) pattern 才 ~1300 条且行短（<200 字符），性能由预过滤兜住；(c) 匹配器本身是 L4 框架语义，跨版本稳定。逐 pattern 单测从 vendor 实例反推（fixture：每种语法元素至少 3 条真实 pattern + 命中/不命中/捕获断言）。

### 2.3 aho-corasick 索引（R6 性能落点）

载入期把 `ParserRules`（serde 数据）编译为 `CompiledParserRules`（含索引，存 `OnceCell`/构造期完成）：

- **plain 表**（name_map 776 / flag_phrases 202 / suffix / pen / cost…）：每表一个 `AhoCorasick`（`MatchKind::LeftmostLongest`——该语义 = 「最早开始、同起点取最长」，与 §2.1 对 plain 子串的语义**完全等价**，因为 plain 等长不同 end 不可能存在）。命中即得 (start, end, pattern_id)，零逐条扫描。
- **pattern 表**（forms 91 / pre_flags / tag_phrases 684）：用抽取期派生的 `literal` 字段建一个混合 AhoCorasick（LeftmostFirst 仅做候选过滤）：line 命中某 literal 才对对应 pattern 跑 Lua 匹配器；literal 长度 <3 或纯类元素的 pattern 进「always-check 桶」（预估 <40 条）。anchored pattern 只在 pos 0 试。
- 新增依赖：`aho-corasick`（workspace 级，pobr-core）。
- special_mods（全行锚定）：模式数 2085+，但全行匹配 → 先按「首词 → 候选 special 桶」哈希分桶，桶内逐条匹配（special 多以固定词开头；纯 pattern 开头的进 always 桶）。

### 2.4 与现 parser 的语义差异清单（双跑 diff 的预期来源，逐项处置）

| # | 差异 | 现 parser（`mod_parser.rs`） | 新引擎 | 处置 |
|---|------|------------------------------|--------|------|
| D1 | 匹配策略 | 固定顺序 strip_prefix/strip_suffix 链 | 位置无关「最早+最长」 | 词序变体行从 Unsupported → Parsed（增益，oracle 抽样验证） |
| D2 | 预处理 | `strip_pob_brackets`（`[内部名\|显示名]` 取显示名）+ `normalize_spaces` | **保留为引擎 pre-pass**（PoB2 输入文本已在上游清洗，pobr 的树/词条入库形态带括号标记，是 PoBR 侧必需差异） | 不变，写进 mod.rs 注释 |
| D3 | tag 扫描次数 | 各专用函数零散剥离 | parseMod 同款：modTagList 扫两次（:6442-6457，支持双 tag 词条） | 双 tag 行为增益 |
| D4 | form 覆盖 | 5 种（INC/RED/MORE/LESS/BASE）+ 专用函数（伤害区间/转换/gain-as/override/keystone special） | 27~28 种全量 | PEN/REGEN/CHANCE/FLAG/DOUBLED 等整类从丢失 → 产出（行为增益，须 oracle 验证后按域放行） |
| D5 | 特判前缀 | `bonded:`/`Gain the benefits of Bonded…`/`Has +N per level`/`has ` 前缀等 PoBR 手写特例（mod_parser.rs:76-117） | 逐条核对 vendor 归属：`has `→preFlagList 无语义前缀；bonded→vendor specialModList/ModCache；per-level→special_mods | 迁移为数据条目或 special；**任何一条迁移前后输出必须逐字节一致**（搬迁不变式） |
| D6 | 失配输出 | `ParseStatus::Unsupported` + 原文收集 | 同款保留；另比照 vendor「匹配 form 但 name 失配返回空表 `{}`」语义 → 映射为 `Parsed{mods:[]}+unparsed` 与 Unsupported 两态，对齐 vendor 三态（nil / {} / modList） | 引擎契约写进 ParseOutcome 文档 |
| D7 | 产物形态 | `Modifier{name=StatId, mod_type, value, flags, kw, tags}` | 同结构（模板实例化产出） | 双跑 diff 的比较单位 |
| D8 | LIST 包装 | 无（minion/aura/enemy 词条多数 Unsupported） | parseMod 末段 ExtraAuraEffect/ExtraAura/MinionModifier/ExtraSkillMod/EnemyModifier LIST 包装（:6680-6750） | M3/M5 已建 LIST 转发通道的直接对接；M6 范围 = 解析产出正确的 LIST mod，消费侧不动 |

---

## 3. forms.rs：27~28 种 form 求值 enum

`enum Form`，每 variant 一个 `≤20 行` 的求值分支（架构 §2.2 原文约束），输入 `(caps, 剩余文本, rules)`，输出 `(mod_names, mod_type, values, suffix, extra_tags, 继续扫描指令)`。逐 form 对照 vendor dispatch（:6460-6655）：

| form | name 来源 | value/type | 备注 |
|------|-----------|------------|------|
| INC / RED | name_map | INC，RED 取负 | |
| MORE / LESS | name_map | MORE，LESS 取负 | |
| BASE / GAIN / LOSE | name_map + suffix_types 扫描 | BASE，LOSE 取负 | suffix 拼接进 ModName（GainAsFire 等） |
| GRANTS / REMOVES | name_map + suffix | BASE（REMOVES 取负）+ extra tag `Condition:{Hand}Attack` | **local 语义**：`{Hand}` 占位符由 item ingest 上下文实例化 |
| GRANTS_GLOBAL | name_map + suffix | BASE | |
| CHANCE | name_map | BASE | |
| TOTALCOST / BASECOST | cost_types_map / base_cost_types（先扫）→ name_map 清尾 | BASE | name 失配 → 空表态 |
| PEN | pen_types（plain，失配→空表态）→ name_map 清尾 | BASE | 产 \*Penetration |
| REGENFLAT / REGENPERCENT | `regen_types[cap2]` | BASE（PERCENT 加 `Percent` 后缀） | cap2 是资源名捕获 |
| DEGENFLAT / DEGENPERCENT / DEGEN | `degen_types[cap2]` / `dmgTypes[cap2]+"Degen"` | BASE | |
| DMG / DMGATTACKS / DMGSPELLS / DMGBOTH / DMGTHORNS / DMGTHORNSBASE | `dmgTypes[cap3]` → `{X}Min/{X}Max` 双 mod | BASE 区间 | ATTACKS/SPELLS/BOTH 在无显式 flag 时默认补 KeywordFlag.Attack/Spell/bor 二者；THORNS 补 ModFlag.Thorns；THORNSBASE 值恒 {1,1} |
| FLAG | 先扫 flag_types（失配→nil 态）再 name_map | FLAG（或 flag_types 条目内嵌 mod 的 name/type/value，hexproof 特例） | 产 `Condition:X` FLAG mod —— M3 已接 GetCondition Flag 回退的消费侧 |
| OVERRIDE | name_map | OVERRIDE | |
| DOUBLED | name_map | **双 mod**：`{Name} MORE 100` + `Multiplier ChanceTo{Name}Doubled limit=100/limitTotal` 结构（vendor :6618-6655） | 依赖 M4 已落的 globalLimit 聚合；若 M4 未交付 globalLimit，DOUBLED 暂保 Unsupported 并在覆盖率报表单列（不阻塞切换） |

`template.rs`：模板实例化（`$n` 代入捕获、`:cap` 拼接、negate/clamp/div/mult/base 算子求值、TagTemplate → ModTag、misc 指令 → LIST 包装）。与 M5b `rules/special_mod.rs` 的占位符求值器**共用同一实现**：公共求值器即 **M3 起建、M5b 已复用的 `rules/value_expr.rs`**（总架构评审裁决的单一受限 DSL 实现），T-B 直接复用并补 `:cap`，禁止第二套求值器。

---

## 4. parseMod 编排（mod_parser/mod.rs）

照搬 vendor parseMod（:6389-6755）主序，签名 `pub fn parse_mod(text: &str, rules: &CompiledParserRules) -> Result<ParseOutcome, ParseError>`：

1. PoBR pre-pass（D2：括号剥离 + 空白归一 + 小写视图）；
2. unsupported 表查询；
3. special 通道：scan(special_mods ∪ special_derived)，命中且剩余为空 → 模板/handler 实例化返回；
4. pre_flags 扫描（行尾补空格的 vendor 怪癖照搬：`line = line .. " "`）；
5. preSkillName 扫描（order=2 时机控制：对外暴露 `parse_mod_ordered(text, order, rules)`，默认两 pass 编排在引擎内完成——vendor cache 闭包对每行跑 order=1/2 两遍取先成功者，引擎等价实现）；
6. formList 扫描，失配 → Unsupported；
7. modTagList 扫描 ×2；
8. 按 form 分发（§3）→ name/suffix/value；
9. skillNameList 扫描（order 控制）+ modFlagList 扫描；
10. 合并 flags/keywordFlags/tagList → 生成 Vec<Modifier>；
11. misc 包装（addToAura/newAura/addToMinion/addToSkill/applyToEnemy → LIST mod）；
12. 剩余文本非空白 → `unparsed: Some(...)`（vendor 返回 `line:match("%S") and line` 同义）。

兼容垫片：旧签名 `parse_mod(text)` 在双跑期保留，内部走 legacy；切换 commit 后删除，五个调用方（`passive.rs:61 / item.rs:132 / skill_source.rs:407,440,539 / calc/session.rs:75` + item_text）改为接收注入的 `&CompiledParserRules`（沿 W3 的 `&GameConstants` 注入路径同款改法，由 session/orchestrator 持有 RuleSet）。

---

## 5. 新旧双跑 harness 与全语料

### 5.1 语料来源（四层，去重后入 `tools/precompile-mods` 的 corpus collector）

| 来源 | 提取方式 | 量级 | 用途 |
|------|----------|------|------|
| C1：18-build XML | `examples/demo-bd-test/builds/*/decoded.xml` 的 Items 文本块逐行 + Skills/Config 自由文本 | ~2-3k 行 | **门禁主语料**（roadmap 验收点名「18-build 语料 parse diff=0」） |
| C2：passive_tree.json 全节点 stats | `data/4.5.0.3.4/base/passive_tree.json` 每节点 `stats[]` 文本行 | ~10k 行 | 全量树词条覆盖率 + stat_id 通道底座 |
| C3：vendor ModCache.lua | luajit 加载 `Data/ModCache.lua`（1MB，6598 行）dump 全部 key（词条原文）**与 value（PoB2 预解析 mod 表）** | ~6k 行 | **语料 + 离线 golden 双重身份**：PoB2 对每行的期望输出免 oracle 直接可得 |
| C4：QueryMods.lua（二期） | luajit dump trade 词条文本 | ~10k+ 行 | 覆盖率报表扩面，不进 diff=0 门禁 |

### 5.2 diff=0 判定的精确定义

- **比较单位**：`canonical_outcome(text) = 排序后的 Vec<Modifier> 的规范 JSON`——Modifier 按 `(name, mod_type, 序列化 tags, flags, kw, value)` 排序；f64 用 serde_json 最短往返表示；`source`（原文）参与、`origin`（SourceId）剔除（双跑两侧来源构造不同）。status/unparsed 一并入比较。
- **五态裁决**（对每行）：

| 态 | 定义 | 处置 |
|---|------|------|
| EQ | 旧、新都 Parsed 且 canonical 相等 | 通过 |
| DIFF | 都 Parsed 但产物不同 | **切换阻塞**。逐条 oracle/ModCache 仲裁：新对旧错 → 行为修复 commit（附 vendor 行号/ModCache 值）+ baseline 独立审查；新错 → 修引擎/数据 |
| OLD_ONLY | 旧 Parsed 新 Unsupported | **切换阻塞**（新 parser 必须是旧能力超集） |
| NEW_ONLY | 旧 Unsupported 新 Parsed | 增益。C3 行直接对 ModCache 值断言；非 C3 行按 form 分组抽样 ≥10%/组走 oracle |
| UNSUP | 双 Unsupported | 计入覆盖率缺口，不阻塞 |

- **门禁口径**：C1（18-build）语料 DIFF=0 且 OLD_ONLY=0（经裁决的行为修复按纪律剥离到独立 commit 后重跑归零）；C2/C3 的 DIFF/OLD_ONLY 清零或逐条 whitelist（whitelist 文件入库、注明 vendor 依据，目标为空）。
- **实现**：`crates/pobr-core/tests/parser_dual_run.rs`（feature `parser-dual-run` 下编译 legacy）+ 报告生成器输出 `target/parser-diff-report.json`（按五态 × 语料源 × form 分组计数 + 明细）。ModCache golden 断言单列 `tests/parser_modcache_golden.rs`，fixture 为抽取落盘的 `tests/fixtures/modcache_golden.json`（self-contained，CI 不依赖 luajit）。

### 5.3 oracle parseMod differential（roadmap 点名项）

**复用 M5b-D1 已交付的 `oracle.lua --mode parsemod`**（不另起模式名）：stdin 喂文本行集合，调 vendor `modLib.parseMod` 逐行 dump mod 表 JSON；M6 仅做输出字段增量（如 order=1/2 双 pass 标记）。T-C 用它对 NEW_ONLY/DIFF 行做终裁（P13：「oracle differential test 是最终裁判」）。比较时做字段名映射（vendor mod 表 → pobr Modifier 的 canonical 形态）。

---

## 6. precompile-mods 与 generated/parsed_mods.json + 覆盖率 CI

### 6.1 工具形态

新 bin `tools/precompile-mods/`（workspace member；依赖 pobr-core + pobr-gamedata + serde_json）：

```
cargo run -p precompile-mods -- --data data/4.5.0.3.4 [--corpus-extra <file>] [--report]
```

流程：加载 RuleSet（含 ParserRules）→ 枚举语料（C1 路径写死为 examples 约定 + C2 从 data 内自举 + special_derived 展开输入）→ 逐行 `parse_mod` → 写两个产物：

- `generated/parsed_mods.json`：`{ "_meta": {...含 rules 文件 hash...}, "entries": [{ "text": "...", "status": "parsed|unsupported", "mods": [canonical Modifier] }] }`，text 字典序排、byte-stable；
- `generated/special_derived.json`：§1.6 的派生展开（keystone LIST / per-gem / skill_names）。

### 6.2 运行时消费（热路径零解析，R6 兜底）

- pobr-gamedata 新增 `parsed_mods` 域懒加载 → `ParsedModCache`（text → Vec<Modifier> 的 HashMap）；
- `pobr-core/src/mod_cache.rs` 从「形同虚设」改为正式入口：`lookup_or_parse(text, &CompiledParserRules, &ParsedModCache)`；五个 ingest 调用方统一走它；
- cache miss（用户自定义词条）回退在线 parse_mod——bench 验证在线路径也 ≤10% 退化（§9），cache 只是加速不是正确性依赖。

### 6.3 覆盖率 CI 报表（roadmap 验收点名项）

- precompile-mods `--report` 输出 `parse 覆盖率 = parsed / total`，按语料源 × form 分组 + Unsupported 明细 top-N；
- CI 工作流加步骤：①`regen-check.sh` 扩展进 generated 重生一致校验（precompile 重跑 == 已提交，架构 §4 防线 ③）；②覆盖率**棘轮**：当前覆盖率写进 `devs/ci/parse-coverage-baseline.json`，PR 不得降低（升高时同 PR 更新 baseline）；
- special 长尾收尾（10-G2）由该报表驱动：Unsupported top-N 中属 special 模式者按批次补 `special_mods.json`（每批独立 commit、oracle 抽样、`verified` 标注——R4 纪律）。

---

## 7. stat_id 双通道（P10，passive_tree 先行）

### 7.1 映射表方案

- 新表 `generated/stat_modifier_map.json` + `overlay/stat_modifier_overrides.json` 双层：
  - generated 层**由文本通道自举**：对 C2 语料（树节点 stats 行），节点词条在 base 数据里可回溯到 stat_id + 数值（树导出 / .dat PassiveSkills 列）。precompile-mods 把「该行 parse 出的 Modifier 模板（数值参数化为 `$v`）」按 stat_id 聚类；同一 stat_id 的所有文本实例必须归出**同一**模板，归不出（同 id 多模板 / 文本歧义）→ 进 `conflicts` 段待人工；
  - overlay 层放人工裁决的 override（schema 同 M1 `skill_stat_map.json` 条目：`stat_id → [{mod_name, mod_type, flags, kw_flags, tags[], div/mult/base}]`，**直接复用 `catalog/stat_map.rs` 的 StatMapEntry 类型**，不另发明 schema）。
- **前置核实（T-E 第一项工作）**：确认 pipeline 能给出 per-node stat_id+值（`passive_tree.json` 现仅存文本行，`catalog/tree.rs:49 stats: Vec<String>`）。若 .dat join 短期拿不到 → 降级方案：通道二以「规范化文本行」为 key（text-keyed），等 pipeline 补列后无损升级为 id-keyed（映射表结构不变，仅 key 字段换）。
- 注入路径：`pobr-tree` 收集 allocated node 的 (stat_id, value) → `pobr-core::passive::ingest_passive_nodes_v2` 查映射表实例化 Modifier（带同样的 SourceId 归因）。feature `stat-id-channel` 门控。

### 7.2 双通道 diff 与按域切换

- harness：`crates/pobr-build/tests/stat_channel_diff.rs`——18-build 每 build 分别用文本通道 / stat_id 通道构建 ModDb，对 `iter_mods` 的 canonical 序列化 diff + 对 OutputTable 顶层指标 diff；
- 切换闸门（roadmap 原文）：「**stat_id 通道 diff<0.1% 后才允许按域切换**」——口径：18-build 全部 build 的 mod 条目级 diff 行数 / 总条目数 < 0.1% 且 OutputTable 关键指标逐值相等；
- M6 只切 passive_tree 域；mods.json（装备词缀）域留待 M7+statdesc（rendered_lines 依赖 M5b），蓝图仅保证映射表 schema 对两域通用。

---

## 8. version-bump-drill.sh（P18，第二次演练 = 终局验收）

**在 M3 T5-F 交付的第一版 `devs/scripts/version-bump-drill.sh` 基础上扩展**（非新建：补第 4 步 precompile 实跑、5d 覆盖率断言，移除 M3 版的 precompile 占位 skip），内容契约：

```bash
#!/usr/bin/env bash
# version-bump-drill：架构分离目标的唯一可执行验收（P18）。
# 用法：version-bump-drill.sh <new_poe_version> [--vendor-ref <git ref>] [--dry-run]
set -euo pipefail
# 0) 前置断言：工作区干净（git status --porcelain 为空）
# 1) pipeline：按 tools 配置对 <new_poe_version> 下载 .dat 导出（缺源时 --dry-run 跳过、用现版本重放）
# 2) adapter：cargo run -p pobr-data-adapter -- --poe-version <v> → data/<v>/base/
# 3) extract-lua 全套：cargo run -p sync-pob-catalog -- extract-lua（skill_overrides + mod-parser-rules
#    + 既有各 overlay 抽取器，--vendor-root 指向 --vendor-ref 检出）→ data/<v>/overlay/
# 4) precompile：cargo run -p precompile-mods -- --data data/<v> → data/<v>/generated/ + 覆盖率报表
# 5) 终局断言（任一失败 = 演练失败，失败项登记下一阶段数据化清单）：
#    a. git diff --quiet -- crates/ apps/ tools/   ← Rust 零改动
#    b. cargo build --workspace                     ← 零改动编译通过
#    c. POBR_DATA_VERSION=<v> cargo test -p pobr-build --test ninja_parity -- --nocapture
#       （parity 集"可运行"——允许数值漂移，不允许 panic/加载失败）
#    d. 覆盖率报表生成成功，新增 Unsupported 列入报告（不设阈值，供下阶段排期）
# 6) 输出 drill-report.md：四步耗时 / 覆盖率 delta / 新 Unsupported top-N / drift diff 摘要
```

第二次演练的执行口径：若 0.6 版本数据不可得，用「当前版本 + vendor 新 commit」做半演练（3/4/5 步全真，1/2 步重放）——演练有效性以「extract-lua/precompile 对 vendor 漂移的吸收能力」为主要观测点。`ninja_parity` 需要支持 `POBR_DATA_VERSION` 环境变量选版本目录（T-F 顺带接线，现为写死路径则改之）。

---

## 9. bench ≤10% 方案（R6）

- 新增 `crates/pobr-core/benches/mod_parser_bench.rs`（criterion）三组：
  1. `parse_corpus_legacy` vs `parse_corpus_engine`：同一 1000 行混合语料（从 C1/C2 固定抽样、入 fixture）逐行解析吞吐——**门禁：engine ≤ 1.10 × legacy 中位耗时**（roadmap「parse bench 退化 ≤10%」的操作化）；切换删 legacy 后该 bench 留 engine 绝对值监控；
  2. `compile_rules`：ParserRules → CompiledParserRules（含 aho-corasick 构建）一次性成本，目标 <80ms（载入期、非热路径，超标仅警告）；
  3. `ingest_with_parsed_cache`：18-build 单 build 全量 ingest，cache 命中路径 vs 纯解析路径——验证 parsed_mods 兜底（预期 cache 路径快 ≥5×，未达不阻塞、记录）。
- `mod_db_bench` 维持既有门禁无回归。
- 若 1 超标的优化顺位：① literal 预过滤桶细化（双字 literal）；② plain 表 AC automaton `dense` DFA；③ 候选 pattern 按历史命中频率排序。全部用尽仍超 → parsed_mods 缓存兜底 + 在 PR 里显式记录绝对耗时供裁决（roadmap 括号注记的本意）。

---

## 10. 任务分解（工作项总表）

| ID | 目标 | 涉及文件（新增 ➕ / 修改 ✎） | vendor 参照 | 测试/fixture | 规模 |
|----|------|------------------------------|-------------|---------------|------|
| T1 | catalog schema：`parser_rules.rs`（FormDef/NameMapDef/FlagPhraseDef/PreFlagDef/TagPhraseDef/TagTemplate/ValueExpr） | ➕`crates/pobr-data/src/catalog/parser_rules.rs` ✎`catalog/mod.rs` ✎`catalog/manifest.rs`（overlay 域登记） | §1 全部 schema | schema 往返单测 + 手写 mini fixture（每段 ≥3 条） | 2d |
| T2 | extract-lua 六表抽取：引导脚本（headless 加载 ModParser、位掩码反查、探针闭包推断、各段排序 dump JSONL）+ Rust 子命令分支 | ➕`tools/sync-pob-catalog/src/extract_parser_rules.{rs,lua}` ✎`main.rs` ✎`lib.rs` | ModParser.lua:62-154/157-961/964-1171/1174-1421/1424-2136/6161-6293；oracle.lua 的引导方式 | 条目数 assert（91/776/202/…）；闭包推断率报告；byte-stable 重跑测试；落盘 `data/4.5.0.3.4/overlay/mod_parser_rules.json` | 4d |
| T3 | scan 引擎：Lua pattern 子集匹配器 + 最早最长语义 + AC 索引 + CompiledParserRules | ➕`crates/pobr-core/src/mod_parser/scan.rs` ➕`compiled.rs` ✎workspace Cargo.toml（aho-corasick） | ModParser.lua:6362-6385 | pattern 匹配器逐语法元素单测；tie-break 专项测试；歧义报告 | 3d |
| T4 | forms + template：27~28 form 求值 enum + 占位符实例化（与 special_mod 共用 value_expr） | ➕`mod_parser/forms.rs` ➕`template.rs` ✎`rules/value_expr.rs`（提公共或复用 M5b 实现） | ModParser.lua:6460-6655（form dispatch）/:6680-6750（LIST 包装） | 每 form ≥3 条真实词条 fixture（输入→canonical 输出） | 3d |
| T5 | parseMod 编排 + legacy 平移 + 双跑 feature | ✎`mod_parser.rs`→➕`mod_parser/{mod.rs,legacy.rs}` ✎`lib.rs` | ModParser.lua:6389-6755 | 现有全部 parse 测试在 legacy 与 engine 双跑通过 | 2d |
| T6 | 语料收集 + 双跑 harness + ModCache golden + oracle parse-mods 模式 | ➕`crates/pobr-core/tests/parser_dual_run.rs` ➕`tests/parser_modcache_golden.rs` ➕`tests/fixtures/modcache_golden.json` ✎`tools/pob2-oracle/oracle.lua`（parse-mods）➕语料收集器（precompile-mods 内 corpus 模块） | Data/ModCache.lua、Main.lua:128 | 五态报告生成；C1 diff=0 断言入 CI | 4d |
| T7 | precompile-mods 工具 + parsed_mods/special_derived 产物 + 覆盖率棘轮 CI | ➕`tools/precompile-mods/`（新 member）✎根 Cargo.toml ✎`devs/scripts/regen-check.sh`（generated 校验）➕`devs/ci/parse-coverage-baseline.json` | ModParser.lua:6151-6158/6302-6361（派生区） | 重生一致测试；覆盖率报表 golden | 3d |
| T8 | 运行时接线：gamedata parsed_mods/parser_rules 域 + RuleSet 填实 + mod_cache 复活 + 五调用方注入改造 + 切换删旧 | ✎`pobr-gamedata/src/{ruleset.rs,domains/}` ✎`pobr-core/src/mod_cache.rs` ✎`passive.rs/item.rs/item_text.rs/skill_source.rs/calc/session.rs` ✎`pobr-build`（orchestrator 注入） | — | 注入路径集成测试；切换 commit 跑全量门禁 | 3d |
| T9 | stat_id 通道：映射自举 + overlay override + 双通道 diff harness + passive_tree 域切换 | ➕`generated/stat_modifier_map.json` ➕`overlay/stat_modifier_overrides.json` ✎`pobr-tree`（stat_id 收集）✎`pobr-core/src/passive.rs`（v2 ingest）➕`pobr-build/tests/stat_channel_diff.rs` | P10；catalog/stat_map.rs（M1） | diff<0.1% 报告；18-build 双通道 OutputTable 逐值 | 4d |
| T10 | version-bump-drill.sh + drill 演练 + bench 三组 + special 长尾收尾批次 | ✎`devs/scripts/version-bump-drill.sh`（扩展 M3 第一版）➕`pobr-core/benches/mod_parser_bench.rs` ✎`ninja_parity.rs`（POBR_DATA_VERSION）✎`overlay/special_mods.json`（长尾批次） | P18 | drill-report；bench 门禁；长尾批次 oracle 抽样 | 3d |

---

## 11. 并行 track 切分

### 11.1 六个 track 与归属

| Track | 工作项 | 可并行起点 | 关键产出 |
|-------|--------|-----------|----------|
| **A 数据抽取** | T1 + T2 | 立即 | mod_parser_rules.json + catalog 类型 |
| **B 引擎** | T3 + T4 + T5 | 立即（用 T1 的 mini fixture 先行，不等真实抽取） | mod_parser/ 模块 + 双跑 feature |
| **C 对拍** | T6 | 立即（ModCache golden 与 oracle 扩展不依赖 A/B） | 五态报告 + golden fixture |
| **D 管线与接线** | T7 + T8 | T7 骨架立即；T8 等 B 引擎可用 | precompile-mods + 运行时注入 |
| **E stat_id** | T9 | 前置核实立即；实施等 B+D | 双通道 + passive_tree 切换 |
| **F 收尾** | T10 | 串行尾部（单 agent） | drill + bench + 长尾 |

### 11.2 文件归属表（每文件唯一写者；冲突即违规）

| 文件/目录 | 独占写者 | 说明 |
|-----------|---------|------|
| `crates/pobr-data/src/catalog/parser_rules.rs`、`catalog/mod.rs`、`catalog/manifest.rs` | A | B 只读消费 |
| `tools/sync-pob-catalog/**` | A | |
| `data/*/overlay/mod_parser_rules.json` | A（工具产物） | 禁手改 |
| `crates/pobr-core/src/mod_parser/**`（含旧 mod_parser.rs 的迁移） | B | |
| `crates/pobr-core/src/rules/value_expr.rs` | B | M3 起建、M5b 已复用的公共求值器；B 只做 `:cap` 增量扩展并知会 config/special 维护者 |
| `crates/pobr-core/src/lib.rs` | B | D 需要的 export 由 B 代加（接口契约 §11.3） |
| `crates/pobr-core/tests/parser_dual_run.rs`、`parser_modcache_golden.rs`、`tests/fixtures/modcache_golden.json` | C | |
| `tools/pob2-oracle/oracle.lua` | C | |
| `tools/precompile-mods/**`、根 `Cargo.toml`（member 注册）、`devs/ci/parse-coverage-baseline.json` | D | |
| `crates/pobr-gamedata/**` | D | M0-W3 合并后才动 ruleset.rs |
| `crates/pobr-core/src/mod_cache.rs`、`passive.rs`、`item.rs`、`item_text.rs`、`skill_source.rs`、`calc/session.rs` 的 parse 调用点 | D（T8 切换 commit 集中改） | B 不碰调用方 |
| `crates/pobr-build/**`（orchestrator 注入 + stat_channel_diff.rs） | D（orchestrator）/ E（stat_channel_diff.rs） | 两文件集不相交 |
| `crates/pobr-tree/**`、`pobr-core/src/passive.rs` 的 v2 新增段 | E | passive.rs 双写者风险：E 只**新增** ingest_passive_nodes_v2 函数，不改既有函数体；T8 切换 commit 时 D 统一收口 |
| `devs/scripts/version-bump-drill.sh`、`regen-check.sh` 扩展、benches/ | F（regen-check 的 generated 段由 D 写、F 复核） | |
| `overlay/special_mods.json` 长尾批次 | F | |

### 11.3 track 间接口契约（先冻结后开工）

1. **A→B**：`catalog::parser_rules` 类型集（T1 第 1 天冻结 v1；字段只增不改，serde default 兜底）。B 在真实抽取落盘前用 `tests/fixtures/mini_parser_rules.json`（A 提供，每段 ≥3 条手抄 vendor 条目）开发。
2. **B→C/D**：`parse_mod(text, &CompiledParserRules) -> ParseOutcome` 签名 + `canonical_outcome` 序列化函数（落 `mod_parser/canonical.rs`，C 的 diff 与 D 的 parsed_mods 共用同一实现，**禁止两套序列化**）。
3. **B→D**：`CompiledParserRules::compile(&ModParserRulesDoc) -> Result<Self, CompileError>`（gamedata 不做编译，只 load + merge，编译在 core——保 P9 边界）。
4. **D→E**：`ParsedModCache` 查询 API + RuleSet 注入路径；E 的 v2 ingest 走同一注入。
5. **C→全体**：五态报告 JSON schema（diff 报告是 B 修复迭代与 F 验收的共同输入）。

### 11.4 串行序与里程碑

```
W1        W2          W3            W4
A ████████░
B ██████████████░          (W3 起进入 diff 修复循环)
C ████████████░░░░░░░      (golden 先行，W2 末出首份五态报告)
D ░░████████████████░      (T7 骨架→T8 等切换闸门)
E ░░░░░░██████████████
F ░░░░░░░░░░░░██████████
里程碑：
M6.1 (W1末) 契约冻结 + mini fixture 引擎跑通 + mod_parser_rules.json 首版落盘
M6.2 (W2末) 真实规则过引擎，首份五态报告；C1 语料 DIFF/OLD_ONLY 清单出炉
M6.3 (W3中) C1 diff=0 达成 → 切换 commit（T8）：删 legacy、调用方注入、parity 全量门禁
M6.4 (W4)   stat_id passive_tree 切换 + drill 演练 + bench 门禁 + 长尾批次 → 阶段验收
```

硬串行点：**M6.3 切换 commit 必须单独、完整过门禁三件套 + 双跑 diff 报告附 PR**；E 的域切换与 F 的 drill 在 M6.3 之后。

---

## 12. 门禁与验收

### 12.1 各 track 局部门禁

| Track | 局部门禁 |
|-------|----------|
| A | 条目数 assert 全过；重跑 byte-diff 零（接入 regen-check.sh）；闭包推断失败条目均有 handler_id 且计入 <100 总闸门 |
| B | pattern 匹配器单测全绿；每 form ≥3 fixture；现有 parse 测试（tests/mod_parser.rs 等）legacy/engine 双跑全绿 |
| C | modcache_golden 全过（whitelist 目标为空）；oracle parse-mods 可重复运行 |
| D | generated 重生一致 CI 绿；覆盖率棘轮就位；切换 commit 全量门禁 |
| E | diff<0.1% 报告达标才许切；切换后 18-build OutputTable 逐值不变（纯通道切换 = 搬迁不变式适用） |
| F | drill 5a-5d 全过；bench 三组达标 |

### 12.2 阶段整体验收（roadmap M6 验收原文，逐条对应）

> 「全部 parse 测试通过 + 18-build 语料 parse diff=0 + 解析覆盖率入 CI 报表 + parity 零回归 + parse bench 退化 ≤10%（parsed_mods 缓存兜底）；stat_id 通道 diff<0.1% 后才允许按域切换。**第二次 version-bump-drill**：新版本只跑 pipeline→adapter→extract-lua→precompile 四步，Rust 零改动编译通过、parity 集可运行；固化为 `devs/scripts/version-bump-drill.sh`。」

对应：parse 测试 = B/C 门禁；diff=0 = §5.2 C1 口径；覆盖率报表 = §6.3；parity 零回归 = 切换 commit 与其后每次合并跑 ninja_parity，baseline @5% 不倒退（搬迁部分逐值不变）；bench = §9；stat_id = §7.2；drill = §8。

---

## 13. 风险与回退（R# 落点）

| 风险 | 本阶段落点 | 缓解与回退 |
|------|-----------|------------|
| **R3 抽取正确性 / vendor 漂移** | 探针推断误判条件型闭包；luajit 加载 ModParser 的依赖面（需 Data/ModTools 环境）比 skill_overrides 大 | luajit 执行而非正则（P13）；探针双哨兵一致性检查；`inferred:true` 元数据 + oracle differential 终裁；CI drift diff（extract-lua 重跑 vs 已提交）；回退：推断失败条目一律 handler_id，宁多勿错 |
| **R6 性能 / 数据体积** | 全表扫描 ~2000 pattern；mod_parser_rules.json 预计 ~1-2MB、parsed_mods.json ~3-5MB | AC 索引 + literal 预过滤 + parsed_mods 零解析热路径 + bench ≤10% 门禁（§9 优化顺位）；体积走懒加载（gamedata 按域），必要时 bincode 边车后置 M7 |
| **重写已验证 parser 的回归面**（roadmap M6 风险栏首项） | D5 特判迁移、D4 新 form 放量 | 双跑全语料 diff=0 才切换（这正是 M6 后置的原因）；legacy feature-gated 保留至切换后一个稳定周期再物理删除；ModCache golden 提供离线万级断言 |
| **R1 DSL 膨胀** | tag_phrases 的 `:cap` 扩展、misc 字段全集 | `:cap` 受益 ~139 条 >> 20 条闸门，合规；其余不新增算子；handler 总数（special+tag+preflag）<100 监控入 review checklist |
| **R4 special 长尾验证成本** | T10 收尾批次 | 覆盖率报表驱动分批 + verified:false + 长尾留 Unsupported，**不追求 100%**（roadmap 附 C 原文） |
| **与 M0-W3 的合并冲突** | ruleset.rs / domains/ 双方都要动 | M6 的 D track 对 gamedata 的改动**等 M0 合 master 后 rebase 开工**；蓝图按「注入管道已存在」假设写，若 W3 形态有出入以 master 实际为准、契约 3 相应顺延 |
| **歧义 tie-break 与 vendor pairs 未定义行为** | §2.1 第四级 tie-break | parser_ambiguity_report 专项观察；命中歧义的语料行以 oracle 实测裁决并用 priority 固定 |

---

## 14. 实施前待裁决问题

1. ~~M0 小查表是否已落盘~~ **已裁决（总架构评审 2026-06-11）**：`high_precision_mods.json` 归 M4 W-A2、`local_mods.json` 归 M5c WI-C1；M6 只消费。开工核实存在性即可，缺失向对应阶段返工，不并入 T-A。
2. **M5b special_mods.json 实际交付形态**：T-B 的 special 通道接入假设 `rules/special_mod.rs` + 占位符求值器已存在。若 M5b 延期/形态不同，B track 需保留 legacy keystone special 路径并把 special 接入降级为 M6 内迭代项（影响 M6.3 范围）。
3. **passive_tree 的 per-node stat_id 数据源**：pipeline 现状是否可从 .dat（PassiveSkills/树导出）拿到 node→(stat_id,value)；拿不到则 T9 走 text-keyed 降级方案（§7.1），id-keyed 顺延到 pipeline 补列后。
4. **探针式闭包模板推断**是否接受为 extraction 策略（替代 ~148 个闭包的逐条人工策展）；接受则 verified 升级依赖 oracle differential 覆盖，不接受则 T2 工期 +3d。
5. **DOUBLED form 的 globalLimit 依赖**：M4 验收含 EvalMod tag 第二批，但 globalLimit 跨 mod 累计是否实际落地需开工核实；未落地则 DOUBLED 保 Unsupported 单列（§3 表注），不阻塞切换但覆盖率报表须标注。

---

*实施期间发现蓝图与 master 现状不符时：以 master 为准、在 PR 中注明偏差并回写本蓝图（追加「偏差记录」节，不改正文）。*
