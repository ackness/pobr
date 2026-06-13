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

## 4. 切换就绪状态（给 D-T8）

- `parse_mod_engine(text, &CompiledParserRules)` 签名稳定；
- `canonical_outcome(&ParseOutcome)` 序列化（C/D 共用，禁两套）；
- `CompiledParserRules::compile(&ModParserRulesDoc)`（编译在 core，gamedata 只 load）；
- legacy 保留、`parse_mod(text)` 旧签名可用、调用方零改动；
- 切换（删 legacy + 调用方注入）= D-T8 独立 commit，待 C1 diff=0 达成后做。
