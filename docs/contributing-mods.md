<a id="contributing-modifiers-overlay-channel"></a>

# 通过 overlay 添加词条规则

大多数词条可以只改 JSON，无需写 Rust。overlay 是加载到游戏原始数据之上的修正数据。
本文说明 `data/<version>/overlay/*.json` 的解析规则格式；定义以
[`parser_rules.rs`](../crates/pobr-data/src/catalog/parser_rules.rs) 中的
`ModParserRulesDoc` 和 `SpecialModsDef` 类型为准。添加或重命名字段、值表达式运算符、
或移动规则表时，同一个 PR 必须更新本文。共享规则始终使用 `data/CURRENT` 指向的活动版本验证。

<a id="1-the-big-picture"></a>

## 1. 一行词条怎样进入计算

PoBR 把 `35% increased Fire Damage` 这样的文本解析成 `Modifier`，再由计算引擎汇总。
解析分两层：

1. **通用解析器**（`overlay/mod_parser_rules.json`）来自 PoB2 的
   `Modules/ModParser.lua`。它处理 `N% increased X`、`+N to X`、`N% more X`、
   按属性缩放和条件后缀等常见句式。通常无需修改它，见第 3 节。
2. **特殊规则**（`overlay/special_mods.json`）是人工维护的整行匹配表，处理通用语法
   无法表达的暗金、基石和特殊措辞。新增人工词条通常放在这里，见第 4 节。

两层都读不懂的行记为 `ParseStatus::Unsupported`，保留在诊断和覆盖率报告中，
不会因此崩溃。`pobr-build::build_data` 加载规则，再由
`CompiledParserRules::compile_with_special` 编译。特殊模板在使用前就拒绝未知字段、
非法运算和不支持的作用域。提交前用第 7 节的 `--check` 校验最终合并的规则和通用解析器格式。

<a id="2-the-overlay-directory"></a>

## 2. 规则文件与覆盖顺序

`data/<version>/overlay/` 保存对 `.dat` 自动导入结果的补充和修正。本文涉及：

| 文件 | 来源与语法 | 何时修改 |
|------|------------|----------|
| `special_mods.json` | 人工维护；Rust 正则与受限值表达式 | 添加或修复整行特殊词条 |
| `mod_parser_rules.json` | 从 vendor Lua 抽取；Lua pattern | 维护者修改通用语法时使用，见第 3 节 |

**新增人工规则通常放在 `data/overlay-common/special_mods.json`。**
`pobr-gamedata` 按两层合并：

1. `data/overlay-common/special_mods.json` 是跨版本共享的人工规则。
   不随补丁变化的语义修正放在这里，新版本会自动继承。
2. `data/<version>/overlay/special_mods.json` 只放确实因游戏版本而异的规则。
   相同 `id` 会整条替换共享条目，并保留原位置；新 `id` 追加到末尾。
   这里不做字段级递归合并。

`regen-all.sh` 只把上一版的版本专用文件带到候选版本，供维护者对照新 vendor 审查。
设计背景见[版本升级记录的 P1-3](version-bump-architecture.md#p1-3-version-independent-curated-overlay-dataoverlay-common)。

共享目录还包含 `buff_definitions.json`、`high_precision_mods.json`、`local_mods.json`、
`vendor_name_aliases.json` 等人工数据。`buff_definitions` 与特殊规则一样按 `id` 合并；
`high_precision_mods` 和 `local_mods` 则优先使用版本目录文件，缺失时才用共享文件，
不会逐字段混合。非版本专用的修改应放在共享文件。
其他 overlay（基底修正、暗金、宝石效果、诅咒、stat 描述等）使用 `pobr-data` 中各自的格式，
不属于本文的解析规则检查范围；也可以通过 JSON 贡献，后续按实际需求扩展校验。

**仅本地使用的修正无需提 PR。** 把文件放在
`data/<version>/patch/<same-relative-path>`，例如 `patch/overlay/uniques.json`。
加载时对象递归合并；两个数组都由带 `id` 的对象组成时按 `id` 合并，其他数组整体替换。
patch 先覆盖版本文件，之后才合并共享层与版本层；共享文件没有对应的版本 patch。
实现见 `crates/pobr-gamedata/src/lib.rs` 的 `GameData::load_json_at`。

<a id="3-mod_parser_rulesjson--the-generic-grammar-reference"></a>

## 3. 阅读通用语法 `mod_parser_rules.json`

**不要为新增词条手改这个文件。** 它从 vendor Lua 再生成，手改内容会被覆盖：

```bash
cargo run -p sync-pob-catalog -- extract-lua --what parser-rules --vendor-root "$PWD/vendor/PathOfBuilding-PoE2/src" --out "data/$(cat data/CURRENT)/overlay/mod_parser_rules.json"
```

具体命令也记录在 `_meta.regen_command`。扩展通用语法需要修改上游 Lua 或抽取器，
由维护者处理。本节用于查明已有文本为什么这样解析。

顶层字段都是行数组；其中的 `pattern` 使用 **Lua pattern**，例如 `(%d+)`、`%%`。
读取代码是 `crates/pobr-core/src/parse/mod_parser/compiled.rs`。

| 字段 | vendor 表 | 用途 |
|------|-----------|------|
| `forms` | `formList` | 匹配句式、提取数值，确定 `INC` / `BASE` / `MORE` / `PEN` / `DMG` 等类型 |
| `name_map` | `modNameList` | 普通子串 → 一个或多个 `ModName`，以及可选附加效果 |
| `flag_phrases` | `modFlagList` | 短语 → `ModFlag` / `KeywordFlag` 位与可选标签 |
| `pre_flags` | `preFlagList` | 行首 pattern → 标志、标签或包装指令 |
| `tag_phrases` | `modTagList` | 按 X 缩放或条件后缀 → 标签模板 |
| `suffix_types` | `suffixTypes` | 扫描 BASE / GAIN / GRANTS 的后缀 |
| `damage_types` | `dmgTypes` | DMG 使用的伤害类型 |
| `pen_types` | `penTypes` | PEN 使用的穿透目标 |
| `regen_types` / `degen_types` | `regenTypes` / `degenTypes` | 资源回复与流失名称 |
| `cost_types_map` / `base_cost_types` | `costTypes` / `baseCostTypes` | 技能消耗名称 |
| `flag_types` | `flagTypes` | FLAG 短语 → `Condition:X` 或内嵌 modifier |
| `unsupported` | `unsupportedModList` | vendor 中登记为跳过的文本 |
| `unsupported_pobr_extra` | — | PoBR 额外登记的跳过文本 |

`flags`、`keyword_flags`、`tags`、`player_tags`、`add_to_minion`、`mod_suffix` 等
共用效果字段通过 `RuleEffectsDef` 平铺到 `name_map`、`flag_phrases`、`pre_flags`、
`tag_phrases` 各行；完整字段以 `parser_rules.rs` 为准。

pattern 行的 `literal` 和 `anchored` 是抽取器生成的索引信息：前者记录供
aho-corasick 预筛选使用的最长字面子串，后者记录 `^` 锚定。无需手写。

<a id="4-special_modsjson--where-you-add-a-modifier"></a>

## 4. 在 `special_mods.json` 添加规则

除版本专用修正外，条目放在 `data/overlay-common/special_mods.json`。
每项是 `parser_rules.rs` 定义的 `SpecialTemplateDef`。解析器把输入转成小写，
按整行匹配 `pattern`，编译时自动加上 `^…$`。这里用 **Rust 正则**：
支持 `(\d+)` 捕获和分支，不支持前后查找或反向引用。
分支必须放在显式分组内，例如 `(fire|cold)`，保证锚点覆盖所有分支。
检查通过不等于 Lua pattern 到正则的语义转换一定正确。

下面是现有 FLAG 规则的写法。`source_note` 保留旧条目的历史出处；
旧格式如 `ModParser.lua:NNNN` 只说明抽取出处。
新增机制应使用游戏 stat、词条或官方说明，不把 Lua 行号当作游戏依据。

```json
{
  "id": "eternal_life_while_energy_shield",
  "pattern": "your life cannot change while you have energy shield",
  "vendor_pattern": "your life cannot change while you have energy shield",
  "mods": [
    { "name": "EternalLife", "type": "FLAG", "value": true }
  ],
  "verified": false,
  "batch": "fork-a",
  "source_note": "ModParser.lua:3144 (Lich specialModList)"
}
```

数值捕获与封闭词集 `enums` 的写法：

```json
{
  "id": "has_to_defence_per_player_level",
  "pattern": "has \\+(\\d+) to (armour|evasion|energy shield) per player level",
  "mods": [
    { "name": { "enum": 2 }, "type": "BASE", "value": "$1" }
  ],
  "enums": {
    "2": {
      "armour": "ArmourPerLevel",
      "evasion": "EvasionPerLevel",
      "energy shield": "EnergyShieldPerLevel"
    }
  },
  "verified": false,
  "batch": "S2"
}
```

| 字段 | 要求 | 含义 |
|------|------|------|
| `id` | 必填 | 稳定的 snake_case id；改名相当于删除再新增 |
| `pattern` | 必填 | 整行 Rust 正则；捕获按 `$1..$n` 编号 |
| `mods` | 与 `handler_id` 二选一；识别空结果见下文 | 输出的 `ModTemplateDef` 列表 |
| `handler_id` | 与非空 `mods` 互斥 | JSON 无法表达的 Rust handler，见第 6 节 |
| `handler_args` | handler 所需参数 | 按顺序转交捕获，例如 `["$1","$2"]` |
| `enums` | 可选 | `{"<capture#>": {"<word>": "<literal>"}}`，把封闭词集映射到完整名称 |
| `vendor_pattern` | 可选 | 原 Lua pattern；抽取器据此跳过人工规则已覆盖的 vendor 行 |
| `exporter` | 默认 `same` | PoB2 未实现用 `absent`，行为不同用 `differs`；`absent` 规则在参考口径中排除 |
| `game_source` | `absent` / `differs` 必填 | 游戏 stat id、原文或来源链接 |
| `verified` | 新条目填 `false` | 只有 oracle 差分确认后才设 `true`；不改变运行时是否使用规则 |
| `batch` | 必填 | 维护批次，例如 `S0` / `S1` / `S2` / `unmodelled` |
| `source_note` | 非 S0 条目需有可追溯说明 | 暗金或基石名称、游戏依据；`unmodelled` 必须说明缺少的机制 |
| `examples` | 格式允许省略；新人工规则应填写 | 真实原文，用于逐规则匹配和重叠 / oracle 审计 |

`"mods": []` 必须分清两种含义：

- `"batch": "S0"`：确认不影响任何受支持计算输出，例如 `experience_gain`
  （经验获得速度，stat `experience_gain_+%`）。诊断标为 `no_effect`。
- `"batch": "unmodelled"`：文本已识别，但机制没实现。不得输出 modifier 或指定 handler；
  `source_note` 必须说明缺少什么。它仍报告为未建模，不算 `no_effect`。

其他批次必须生成 modifier 或指定 handler。约束由
`crates/pobr-gamedata/tests/mods/load_special_mods.rs` 验证
（测试目标命令为 `cargo test -p pobr-gamedata --test mods`）；不能用 S0 隐藏未实现机制。

`mods[]` 中的 `ModTemplateDef`：

```jsonc
{
  "name": "FireDamage",        // literal ModName, or {"enum": 2}
  "type": "BASE",              // BASE | INC | MORE | FLAG | OVERRIDE | LIST
  "value": "$1",               // see the value forms below
  "flags": ["Attack"],         // optional ModFlag names
  "keyword_flags": ["Hit"],    // optional known KeywordFlag names
  "tags": [ { "type": "Condition", "var": "Combat" } ],
  "target": "player"           // optional; other targets require an explicit LIST wrapper
}
```

<a id="5-placeholder--value-mini-language"></a>

## 5. 捕获占位符与值表达式

`$1..$n` 引用从 1 开始编号的捕获组。两处格式略有不同。

<a id="5a-value-in-a-mods-template-special_modsjson"></a>

### 5a. `special_mods.json` 的 `mods[].value`

| 格式 | 示例 | 结果 |
|------|------|------|
| 数值常量 | `50` | 固定 `50` |
| 布尔常量 | `true` | 用于 `"type":"FLAG"` |
| 捕获 | `"$1"` | 捕获到的数值 |
| 表达式 | `{ "ref": "$1", "ops": [ {"negate":{}}, {"div":100} ] }` | 从左到右执行运算 |
| 内嵌 modifier | `{ "mods": [ … ] }` | 对应 vendor 的 `LIST { mod = mod(...) }` |
| 列表对象 | `{ "Key": "…" }` | 保留元数据；没有通用解释器读取，校验列为未参与计算字段 |

值运算只支持以下五种，统一由 `crates/pobr-core/src/rules/value_expr.rs` 求值：

| 运算 | JSON | `v →` |
|------|------|-------|
| negate | `{"negate":{}}` | `-v` |
| clamp | `{"clamp":{"min":0,"max":100}}` | `min(max(v,min),max)` |
| div | `{"div":100}` | `v / 100` |
| mult | `{"mult":10}` | `v * 10` |
| base | `{"base":6}` | `v + 6` |

<a id="5b-inline-placeholders-in-mod_parser_rulesjson-tag-fields--handler_args"></a>

### 5b. 通用规则标签字段与 `handler_args` 的内联占位符

标签字段字符串和 handler 字符串参数由
`crates/pobr-core/src/parse/mod_parser/template.rs` 解释：

| 格式 | 示例 | 结果 |
|------|------|------|
| 捕获 | `"$1"` | 捕获 1 |
| 首字母大写并拼接 | `"$2:cap+Effect"` | `firstToUpper($2) . "Effect"`，例如 `FrenzyEffect` |
| 数值运算 | `"$1:mult(10)"` / `"$1:div(N)"` / `"$1:base(N)"` | 捕获 × / ÷ / + `N` |

<a id="dsl-hard-boundary"></a>

### 表达式的限制

允许 `$n`、上述五个值运算符、`:cap` / `:mult` / `:div` / `:base`、`enums` 封闭词集、
`target(player)` 和显式内嵌 modifier 包装。禁止循环、任意递归、自由表达式、跨条目引用、
任意值的字符串拼接。需要这些逻辑时使用第 6 节的 handler。
新增表达式能力必须能帮助至少 20 个条目，否则使用 handler。

<a id="6-when-json-isnt-enough--a-handler"></a>

## 6. JSON 无法表达时使用 handler

需要分支逻辑、跨域 `LIST` 内容或 PoB2 闭包构造逻辑时，用 Rust handler。
条目只记录稳定 id 和捕获参数：

```json
{
  "id": "allocates_passive",
  "pattern": "allocates (.+)",
  "handler_id": "special:granted_passive",
  "handler_args": ["$1"],
  "batch": "S2"
}
```

注册位置：

- `special:<name>`：`crates/pobr-core/src/rules/handlers/`，每个 handler 一个小模块，
  接入该目录 `mod.rs` 的 `register_special_handlers`。最小完整例子是
  `handlers/explode.rs`：读取 `ctx.inputs`，返回 `HandlerOutcome::player_mods(...)`。
- `config:<name>` / `buff:<name>`：属于 `overlay/config_options.json` /
  `buff_definitions.json` 的另一条入口，在
  `crates/pobr-build/src/handlers.rs::build_registry` 注册。

handler 注册数必须 `< 100`，特殊 handler 条目占全部特殊规则的比例必须 `< 10%`，
由 `crates/pobr-core/tests/parser/special_mods_gate.rs` 检查。
接近上限说明规则拆分需要调整，应优先改成模板。

<a id="7-validate-test-and-open-a-pr"></a>

## 7. 校验、测试与提交 PR

### 第一步：校验最终合并的 JSON

先运行这个范围较小的检查：

```bash
cargo run -p precompile-mods -- --data "data/$(cat data/CURRENT)" --check
```

它检查通用解析器格式与编译，以及 common、版本、patch、derived、vendor 合并后的特殊规则。
`special_vendor.json` 必须存在；缺失的生成数据应再生成，不能跳过。
人工规则之后依次追加 derived 和 vendor 规则，这两层的重复 id 是错误。
原始文件或 patch 内重复 id 也会失败。删除版本覆盖后，共享条目会重新生效，
所以必须检查整个有效快照。

特殊规则校验包括内嵌 modifier、已注册 handler、捕获与 enum 引用、有限数值、
除法与 clamp 边界、flags、keyword flags、标签字段和类型、角色作用域。
不能丢弃未知条件或作用域来让规则生效。`verified:false` 不会放宽检查。
运算始终从左到右执行，取负或 clamp 之后的运算也不能跳过。

`--check` 把 JSON 报告写到 stdout，可重定向到本地文件。报告记录引擎包版本、数据版本、
每个输入文件的 SHA-256（含 common 和 patch）、生效顺序、被替换来源及未参与计算的元数据。
`--report` 在写解析产物前也执行这些检查。

检查的边界：

- `StatId` 不是封闭列表。名称字面值和 enum 语法合法，不代表有代码读取它。
  新规则要说明读取方与计算阶段，数值测试覆盖开启、关闭和边界情况。
- 跨角色条件读取 `CalcConfig.actor_multipliers`，由调用方提供角色快照。
  本检查不能证明完整召唤物 Build 上下文已注入，或召唤物 DPS 已支持。
- 结构化 LIST 表和空名称的 PoB 配置发现标记会列为未参与计算的元数据。
  能识别文本不代表会计算。
- 通用解析器平铺效果另有校验限制；特殊规则的严格标签检查不覆盖整套通用语言。
- `refresh-modifiers.sh` 审计全部来源样本和规则示例。每行的 `special_rule_id` 关联
  `rule_validation.effective_rules` 中的规则来源；`matching_special_rules` 按首次命中
  顺序列出候选，含被前面规则遮住的条目。这不证明所有可能输入都不会匹配多条正则。
  规则来源与装备、天赋的 `SourceId` 归因各自保留。

完整例子是共享规则 `body_armour_grants_reduced_crit_damage_bonus_100`。
它为兼容覆盖保留旧 id，但数值已改为捕获，不再固定为 100。
PoB2 的 `ItemCondition { Body Armour, NORMAL }` 对应现有的构筑条件
`NormalBodyArmourEquipped`。读取方是 `defence::calc_crit_extra_reduction`，由
`perform::fill_mechanics` 调用；它把额外暴击伤害减免之和限制到 0..100，再用于防御输出和 EHP。

[完整 Build 回归](../crates/pobr-build/tests/skills/equipment_affix_effects.rs)在活动数据上
覆盖不穿胸甲、普通、魔法、稀有、暗金胸甲，六个 roll 值及物品 `SourceId`。
预编译检查测试把解析规则 id 关联到共享文件哈希；WASM 契约和浏览器往返测试覆盖加载与装备编辑。
这些断言和 parser-oracle 一致，只能证明所测范围，不能代替同版本的完整 Build 数值对照。

### 第二步：重生成解析语料

改动影响已有语料的解析结果时，重生成产物，例如新增特殊规则开始匹配某条已有原文：

```bash
cargo run -p precompile-mods -- --data "data/$(cat data/CURRENT)" --report
```

发布产物使用 `data/CURRENT` 的活动版本，并提交变化的
`generated/parsed_mods.json` 和 `parse-coverage.json`。活动版本的覆盖率产物有 golden
检查，过期会导致 CI 失败；旧版本不为无关的解析器修改重生成。
运行时 `ParseCtx` 使用编译后的规则；`parsed_mods.json` 是离线回归产物，
当前不作为运行时缓存加载。`ModCache` 是内存缓存，寿命只能覆盖一个不可变规则快照。

### 第三步：运行相关测试

日常可直接运行 `mise run regen`，它合并上述校验、重生成和
`cargo test -p precompile-mods`，并逐规则检查 `examples`。
只有名称新增读取方或读取逻辑改变时，才补引擎用例，见
[添加一行词条](recipes.md#add-one-game-line-item-passive-jewel-flaskcharm-text)。
`parity_no_regression`、`reference_report` 由 `mise run gate` 每批统一测量，不逐条重复。

### 第四步：提交 PR

说明新增或修复的词条、游戏依据、`source_note` 来源、`verified` 状态，以及检查和测试结果。
`verified: true` 仅用于 oracle 差分已确认的条目；无法确认精确数值时保持 `false`。
规则仍按正常路径运行，只会在 parity 报告中单独标记。新增代码和注释不要用 Lua 行号解释机制；
旧条目的历史来源不必伪装成新的游戏验证。
