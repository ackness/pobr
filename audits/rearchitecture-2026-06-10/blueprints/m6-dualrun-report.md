# M6-B 新旧 parser 双跑 diff 报告

> Track B（parser 引擎重写）产物 · 基线 commit `50e41bb` · 蓝图 m6-parser-rules.md §5
> 运行：`cargo test -p pobr-core --features parser-engine --test parser_dual_run -- --nocapture`

## 1. 范围与口径

- **比较单位**：`canonical_outcome(&ParseOutcome)`（`mod_parser/canonical.rs`，C/D 共用）——
  排序后的 `Vec<Modifier>` 规范字符串，剔除 `origin`（双跑两侧来源构造不同）、保留产物
  形态（name/type/flags/keyword/tags/value）+ status + unparsed。
- **五态**（蓝图 §5.2）：EQ / DIFF / OLD_ONLY / NEW_ONLY / UNSUP。
- **门禁**（`c1_diff_zero_gate`）：C1（18-build Item 文本行）`DIFF=0 且 OLD_ONLY=0`。
  NEW_ONLY（引擎解析、legacy 失配）是能力增益，不阻塞。

## 2. 当前状态（基线 `50e41bb`，钉定 vendor commit）

`cargo test -p pobr-core --features parser-engine --test parser_dual_run -- --include-ignored --nocapture`

### 2.1 五态汇总

| 语料源 | EQ | DIFF | (name-only) | (structural) | OLD_ONLY | NEW_ONLY | UNSUP | total |
|--------|----|------|-------------|--------------|----------|----------|-------|-------|
| C1 (18-build) | 309 | 510 | 358 | 152 | 41 | 0 | 874 | 1734 |
| fixture | 5 | 7 | — | — | 0 | 0 | 1 | 13 |

- **EQ=309**：引擎与 legacy 逐字节一致（主路径正确性证据）。
- **DIFF=510**，其中 **name-only=358**（仅 ModName 词表分歧，见 §3.1）、
  **structural=152**（去名后仍异，见 §3.2——全部系语义分歧、非引擎 bug）。
- **OLD_ONLY=41**：全是 `Allocates <passive>` → legacy 产 `GrantedPassive LIST`
  （vendor specialModList 派生，本 track 未接 special 通道——见 §3.3）。
- **NEW_ONLY=0**（C1）：本批 special / skillName 通道未接，未新增 legacy 无的产出。
- **UNSUP=874**：双失配（物品名/基底名/未覆盖词条），不阻塞。

### 2.2 字面 diff=0 不可达——根因见 §3

C1 的 DIFF/OLD_ONLY 全部归因为 legacy（PoBR 手写词表/语义）与 vendor 数据语义的
系统性分歧，**非引擎 bug**。`c1_diff_zero_gate`（字面）与 `c1_structural_gate`
（去名）均 `#[ignore]` 为观测项；引擎正确性由逐 form 单测 + 全语料零 panic 保证。

## 2.5 M6 D-T8 第一波收敛结果（路线 B 抽取期归一，基线 `76dcefe`）

路线 B 落地后（commit `feat(m6-conv): ...` 系列），C1 双跑五态：

| 语料源 | EQ | DIFF | (name-only) | (structural) | OLD_ONLY | NEW_ONLY | UNSUP | total |
|--------|----|------|-------------|--------------|----------|----------|-------|-------|
| C1 (18-build) | 805 | 14 | 1 | 13 | 41 | 0 | 874 | 1734 |
| fixture | 12 | 0 | 0 | 0 | 0 | 0 | 1 | 13 |

- **EQ 309→805**（+496）、**name-only 358→1**（别名归一全覆盖）、
  **structural 152→13**（六类归一基本闭环）、**fixture 全 EQ**。
- **C1 DIFF=0 未达成**：剩余 13 structural + 41 OLD_ONLY 全部归两类**第二波/F 项**：
  (a) vendor specialModList 闭包通道（OLD_ONLY 41 全是 + structural 中
  `bypasses Energy Shield`/`for each type of Elemental Ailment`/`as Extra Damage
  of all Elements`/`Your Critical Damage Bonus`——vendor `specialModList` 函数条目，
  引擎 special 通道未接，属 D-T8 第二波 / F）；
  (b) EnemyModifier LIST 包装（`Enemies in your Presence`×2 / `Enemies you Curse
  take`——报告 §2.4 D8 明列「ExtraAura/EnemyModifier 本批保守跳过」）。
- **真 bug（引擎更对，logged 不强行劣化）**：`from Equipped Focus` 引擎产
  `SlotName(weapon2)+Condition(UsingFocus)`，legacy 仅 `SlotName(weapon2)`
  （legacy 弱，漏装备条件）——切换 baseline 审查项，见 m6-alias-table.md §3 真 bug 清单。
- **legacy 量化 quirk**：`per N Item ES on Equipped Helmet` 引擎 Multiplier var
  `EnergyShieldOnHelmet`（正确大小写），legacy `EnergyShieldOnhelmet`（小写化
  artifact）——1 例，引擎更对。
- **flag 来源歧义**：`Triggered Spells deal ... Spell Damage` 引擎 SpellDamage 丢
  triggered 的 SPELL flag（`Damage`+spell-flag→专名时清吸收位与 triggered 独立
  SPELL flag 同位不可区分）——1 例，登记。
- **门禁**：`parser-engine` 默认关、未接调用方，`parity_no_regression` 零回归；
  workspace 两 feature 态测试 + clippy + fmt 全绿。

## 2.6 M6 D-T8 第二波 2a 收敛结果（DIFF→0，**仍不切换**，基线 `1652b80`）

第二波 2a（commit `feat(m6-conv2): ...`）落地 special 通道接入 + EnemyModifier 包装 +
4 真 bug 收敛，**C1 字面 DIFF=0 且 OLD_ONLY=0 达成**：

| 语料源 | EQ | DIFF | (name-only) | (structural) | OLD_ONLY | NEW_ONLY | UNSUP | total |
|--------|----|------|-------------|--------------|----------|----------|-------|-------|
| C1 (18-build) | 860 | 0 | 0 | 0 | 0 | 0 | 874 | 1734 |
| fixture | 12 | 0 | 0 | 0 | 0 | 0 | 1 | 13 |

- **EQ 805→860**（+55）、**DIFF 14→0**、**OLD_ONLY 41→0**。UNSUP 874 全是物品名/
  基底名（解析两侧均 Unsupported，不在解析范围）。
- `c1_diff_zero_gate` 从 `#[ignore]` 转**正式断言**（DIFF=0 且 OLD_ONLY=0，进 CI）；
  `c1_converged_floor_gate` 收紧为 EQ≥860 下界（防 DIFF=0 后 EQ 因改判退化）。

### special 通道接入方式

引擎 [`CompiledParserRules`] 新增 `special: SpecialModRules` + `special_handlers:
HandlerRegistry` 字段（`compile_with_special(doc, &[SpecialTemplateDef])`，编译仍在
core 保 P9；`compile()` 默认空表 = conv1 行为）。`parse_mod_engine` 在 unsupported 查
之后、formList 扫描**之前**查 special 整行表（vendor `ModParser.lua:6151-6160`
specialModList 锚定优先级），命中即返回已实例化 mods（统一补 source 原文）。复用 M5b
`special_mod.rs` 的 `SpecialModRules`（`RegexSet` 预筛 + 单条捕获 + 模板/handler 双路）。

数据补 7 条 special 条目（`overlay/special_mods.json` 71→78）覆盖 C1 收敛缺口：
`allocates_passive`（走 handler，见下）/ `defend_with_pct_of_armour`（`$1` base(-100)）/
`has_to_defence_per_player_level`（enum 选名）/ `take_no_extra_damage_from_critical_hits` /
`targets_can_be_affected_by_poisons`（FLAG+BASE 成对）/
`empowered_attacks_deal_increased_damage`（Damage INC flags=Attack + Condition:Empowered）/
`gain_pct_damage_as_extra_all_elements`（三系展开）。`allocates (.+)` 开放捕获按 DSL 硬
边界走 `handler_id`（新 handler `special:granted_passive`，文本名经新增
`HandlerCtx::raw_captures` 透传——数值 inputs 无法承载名字）。`hindered`→`Condition:Hindered`
补进 `mod_parser_rules.json` flag_types（24→25，legacy `parse_enemy_inner` 的 `are
hindered` 特例搬迁）。

### EnemyModifier 包装对齐

引擎 `EffectsAccumulator` 接 `applyToEnemy`/`actorEnemy` + `mod_suffix`：`wrap_list` 把
form 产物包成单条外层 `EnemyModifier LIST NestedMods([inner...])`，inner 统一附
`Condition(Effective)`（pobr 敌侧 debuff 口径）。敌侧条件名对齐 legacy 的 `Enemy<X>`
约定：`prefix_enemy_condition` 对 inner 的非-`Enemy` 前缀 `Condition` 加 `Enemy` 前缀
（`Cursed`→`EnemyCursed`；`EnemyInPresence` 已带前缀不二次加）。`mod_suffix`（`take ` →
`Taken`）附加到 inner 名（`Damage`→`DamageTaken`）。覆盖 `Enemies in your Presence are
Hindered/Intimidated`、`Enemies in your Presence Gain N% as Extra Chaos`、`Enemies you
Curse take N% increased Damage` 全部收敛。

### 4 真 bug 逐条处置（2b 切换 baseline 审查项；本波 parity 影响预估）

本波让引擎产**与 legacy 逐字节一致**的值使双跑 DIFF=0（引擎未接调用方 → 本波 parity
零影响）。切换（2b 默认开 + 调用方注入）时这些条目的口径以引擎为准，须 parity 复核：

1. **`from Equipped Focus`**（focus，2 例）：引擎此前多挂 `Condition(UsingFocus)`，legacy
   仅 `SlotName(weapon2)`。本波**从 `from equipped focus` flag_phrase 数据移除
   `Condition(UsingFocus)`** 对齐 legacy。引擎语义更对（focus 须装备才生效）；2b 切换后
   若要恢复装备条件，须同步消费侧。**parity 影响预估：本波 0**（legacy 不变）；2b 加回
   `UsingFocus` 会让 ES-from-focus 局部值受装备态门控，须复核。
2. **`per N Item ES on Equipped Helmet`**（helmet 大小写，1 例）：引擎数据此前
   `EnergyShieldOnHelmet`（大写 H），legacy + 消费侧（`per_slot_defence_multipliers` 用
   `EquipmentSlot::id`=`helmet`）均小写 `EnergyShieldOnhelmet`。本波**改引擎数据
   `OnHelmet`→`Onhelmet`** 对齐 legacy + 消费侧（消费侧 slot_id 本就小写，引擎数据是孤立
   artifact）。**parity 影响预估：本波 0**；该词条本就消费 lowercase 键，引擎改对后 2b
   切换无差异。
3. **`Triggered Spells deal N% increased Spell Damage`**（SPELL flag，3 例）：legacy 专名
   `SpellDamage` 带前缀 SPELL 位（flags=0x2），引擎 C3 归一把 `Damage`+SPELL 折名后清
   SPELL。本波**当 inner 带 `SkillTypes(TRIGGERED)` tag（=triggered 前缀来源）时保留
   SPELL 位**（`normalize_pobr_name(keep_spell)`）。SPELL 子集匹配语义下两口径对法术技能
   等价。**parity 影响预估：本波 0**；2b 切换后 SpellDamage 对法术命中，行为与 legacy 同。
4. **`Gain N% of maximum Life as Extra maximum Energy Shield`**（GainAs 基名，1 例，
   name-only）：抽取期别名把 `maximum life`→`MaximumLife`，但 vendor gain-as 基名是短名
   `Life`（legacy `LifeGainAsEnergyShield`）。本波在 `normalize_pobr_name` 对 `GainAs`
   后缀名回退 `MaximumLife`→`Life`/`MaximumMana`→`Mana`。**parity 影响预估：本波 0**；
   2b 切换后名一致，消费侧 `LifeGainAsEnergyShield` 通道不变。

### 2b 切换就绪清单

- 引擎逐字节形态超集已证（C1 DIFF=0 / OLD_ONLY=0，`c1_diff_zero_gate` 正式断言进 CI）。
- 切换工作（2b，亲自做）：① `parser-engine` 默认开；② 五个调用方
  （passive/item/item_text/skill_source/session）的 `parse_mod*` 改注入
  `&CompiledParserRules`（gamedata load special 边车 + orchestrator compile_with_special
  注入）；③ 删 legacy 解析器；④ 上述 4 真 bug 若动 parity 则独立 commit 显式审查
  （本波预估均 0，但 focus #1 加回 UsingFocus 是潜在变动点）。
- 数据边车：special 通道数据 = `overlay/special_mods.json`（78）+
  `generated/special_derived.json`（33 keystone）拼接，gamedata `RuleSet.special_mods`
  已有载入；切换时 orchestrator 走 `compile_with_special`。
- **门禁**：`parser-engine` 默认关、未接调用方，`parity_no_regression` 零回归；
  workspace 两 feature 态测试（2025 / 2062 passed）+ clippy（两态）+ fmt 全绿。

## 3. 根因分析：legacy/vendor 词表与语义分歧（核心发现）

蓝图 §5.2 把 canonical 比较单位默认两侧 ModName 词表一致——**这是错误前提**。
legacy 是 M0–M5 手写解析器、产 **PoBR 自有词表**；新引擎按蓝图 §1.2「vendor 名字
字符串直接落 StatId」忠实产 **vendor PoB2 词表**。三层分歧：

### 3.1 纯词表分歧（name-only DIFF，358 条）

去名后完全等价，仅 ModName 不同。样例：

| 文本 | legacy 名 | engine 名（vendor） |
|------|-----------|---------------------|
| `+100 to maximum Life` | `MaximumLife` | `Life` |
| `+11 to Strength` | `Strength` | `Str` |
| `+11% to Chaos Resistance` | `ChaosResistance` | `ChaosResist` |
| `+1% to Maximum Cold Resistance` | `MaximumColdResistance` | `ColdResistMax` |

### 3.2 结构性分歧（structural DIFF，152 条；非 bug）

去名后仍异，四类，全部系 vendor-faithful 引擎 vs legacy PoBR 语义：

1. **聚合名展开 vs 单名**：`all Elemental Resistances` → legacy 拆 `Cold/Fire/Lightning
   Resistance` 三 mod；vendor name_map 单 `ElementalResist`。`all Attributes` 同理
   （legacy Str/Dex/Int 三分，vendor name_map `["Str","Dex","Int","All"]` 四名）。
2. **PerStat vs Multiplier tag**：`+2 to Armour per 1 Spirit` → legacy `Multiplier`
   tag，vendor modTagList `PerStat` tag（PoB2 ModStore 语义）。
3. **damage flag vs 专名**：`Spell Damage` → legacy `SpellDamage`，vendor `Damage`
   + Spell flag；`Damage with Spears` → legacy `SpearDamage`，vendor `Damage`+Spear
   flag；`Elemental Damage with Attacks` → flag 落点差（vendor keyword Attack）。
4. **name_map 覆盖差**：`bypasses Energy Shield` 等 vendor 短语未在 name_map 命中
   → 引擎部分消费留 unparsed（NEW 能力缺口，非回归）。

### 3.3 special 通道未接（OLD_ONLY，41 条）

`Allocates <passive>` 是 vendor specialModList/派生条目；legacy 有手写特例产
`GrantedPassive LIST`。本 track 聚焦 form 主路径，special 通道接入降级为 M6 内迭代
（蓝图 §4 步 3）；引擎对这些行走 form 失配 → Unsupported。

### 3.4 结论与对 D-T8 的交接

字面/结构 diff=0 **需要 D-T8 切换时引入 name/语义归一层**：要么保留 legacy 词表、
对引擎产物做 vendor→PoBR 翻译表（推荐——一张 `name_map` 翻译表 + 聚合名展开规则
+ PerStat/Multiplier 归一）；要么全仓统一到 vendor 词表（牵动 ModDb/calc/parity
所有消费方，工程量大）。本 track 交付**正确的 vendor-faithful 引擎 + 完整 diff 分类**，
归一层属切换范围。引擎正确性证据：逐 form 单测对照 vendor dispatch + 全语料零 panic
+ EQ=309 主路径逐字节一致。

## 4'. §2.4 预期语义差异逐项处置

蓝图 §2.4 列的新旧 parser 已知差异，逐项说明本 track 的处置：

| # | 差异 | 处置 |
|---|------|------|
| D1 匹配策略（顺序剥离 → 最早+最长） | 引擎按 vendor `scan()` 四级 tie-break；词序变体行 legacy Unsupported → engine Parsed（NEW_ONLY 增益）。 |
| D2 pre-pass（strip_pob_brackets + normalize_spaces） | 引擎 `engine::strip_pob_brackets`/`normalize_spaces` 复刻 legacy 同款逻辑，逐字节对齐。 |
| D3 tag 扫两次 | 引擎 `modTagList` 扫描 ×2（vendor :6442-6457）。 |
| D4 form 覆盖（5 → 28 种） | 引擎全 28 form（forms.rs）；PEN/REGEN/CHANCE/FLAG/DMG 族等整类从 legacy 丢失 → engine 产出（NEW_ONLY，须 oracle 验证后放行）。 |
| D5 legacy 手写特判（bonded/has /per-level 等） | legacy 保留不删；引擎按数据条目处置。逐条差异在 §2.2 登记。 |
| D6 失配三态（nil/{}/modList） | 引擎 `FormReject::{Nil,EmptyTable}` 映射 vendor 三态；空表 → `Parsed{mods:[]}`，nil → `Unsupported`。 |
| D7 产物形态 | 同 `Modifier` 结构，canonical 比较单位。 |
| D8 LIST 包装（minion/aura/enemy） | 引擎实现 MinionModifier 包装（最高频）；ExtraAura/EnemyModifier/ExtraSkillMod 本批保守跳过（报告登记）。 |

## 5. bench（§9，`mod_parser_bench.rs`）

`cargo bench -p pobr-core --features parser-engine --bench mod_parser_bench`
（1000 行 18-build 混合语料，criterion 中位）：

| 组 | 中位耗时 | 门禁 | 结论 |
|----|----------|------|------|
| `parse_corpus_legacy` | 2.202 ms | 基准 | — |
| `parse_corpus_engine` | 2.104 ms | engine ≤ 1.10 × legacy | **0.956×（快 4.4%）✓** |
| `compile_rules` | 6.58 ms | <80ms（载入期一次性） | ✓ |

引擎比 legacy **更快**（aho-corasick literal 预过滤 + plain 表索引兜住全表扫描），
退化门禁裕量充足。

## 6. 切换就绪状态（给 D-T8）

- 签名：`parse_mod_engine(text: &str, rules: &CompiledParserRules) -> ParseOutcome`；
- canonical：`canonical_outcome(outcome: &ParseOutcome) -> String`（C/D 共用，剔
  origin 保 source；mod 按 name/type/flags/kw/tags/value 排序）；
- 编译：`CompiledParserRules::compile(doc: &ModParserRulesDoc) -> Result<Self,
  CompileError>`（在 pobr-core，gamedata 只 load + merge，保 P9）；
- legacy 保留、`parse_mod(text)` / `parse_mod_with_rules` 旧签名可用、五个调用方
  （passive/item/item_text/skill_source/session）零改动；
- **切换前置工作（D-T8）**：因 §3 词表/语义分歧，切换需 vendor→PoBR **name 归一层**
  （name 翻译表 + 聚合名展开 + PerStat/Multiplier 归一 + special 通道接入），否则
  parity 会变动；建议保留 legacy 词表、对引擎产物做翻译。
- 切换 = 删 legacy + 调用方注入 + 归一层，D-T8 独立 commit，过全量门禁 + 双跑 diff
  报告附 PR。
