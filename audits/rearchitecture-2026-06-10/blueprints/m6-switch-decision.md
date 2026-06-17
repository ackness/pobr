# M6.3 切换决策（owner 拍板项）

> 触发：M6-B parser 引擎重写完成后的双跑发现（m6-dualrun-report.md §3）。
> 状态：**待 owner 定夺**。在定夺前不执行 D-T8 切换 commit。
> 当前 master：M6 第一波（D7 precompile + C 对拍 + B 引擎）全合并，引擎 feature `parser-engine` 默认关、与 legacy 并存、行为中性、2021 tests 绿。

## 问题

蓝图 §5「双跑 diff=0 后翻开关」的前提是**新旧两侧产出同一套 ModName 词表**。实测不成立：

- **legacy**（M0–M5 手写 parser）产 **PoBR 自有词表**：`MaximumLife` / `Strength` / `ColdResistance` / `MaximumColdResistance`…
- **新引擎**（按蓝图 §1.2「vendor 名字字符串直落 StatId」忠实抽取）产 **vendor PoB2 词表**：`Life` / `Str` / `ColdResist` / `ColdResistMax`…
- **全部下游**（calc 引擎、mods.json、stat_map、归因、display）都消费 **PoBR 词表**。

C1（18-build）双跑：EQ=309 / DIFF=510（name-only 358 + structural 152）/ OLD_ONLY=41（`Allocates X`）/ UNSUP=874。DIFF 全是词表/语义分歧、**非引擎 bug**（引擎逐 form 单测 + 全语料零 panic 已证正确）。

直接翻开关 = 所有 ModName 改成 vendor 拼写 → 下游全部 miss → parity 崩。所以 M6.3 切换**必须先做词表归一**。

## 两条路

### A. 引擎输出后挂 vendor→PoBR 名归一层
- 在 `parse_mod_engine` 产出后、返回前，过一张 `vendor_name → PoBR StatId` 翻译表 + 聚合名展开（`AllResist`→三抗）+ PerStat/Multiplier tag 归一。
- **优点**：引擎保持「忠实 vendor」；翻译层独立可测、可逐条对照 legacy 收敛。
- **缺点**：多一层运行时映射；翻译表要覆盖 ~775 name_map 条目的 vendor→PoBR 对应（大部分可从 legacy 现有词表反向自举）。

### B. Track A 重抽 name_map 时直接映射到 PoBR StatId（推荐）
- 改 extract-lua 的 name_map 抽取：vendor 短语 → 不落 vendor 名，而落 PoBR canonical StatId（用一张 vendor→PoBR alias 表在抽取期归一）。
- **优点**：引擎仍是纯数据驱动 scan，输出直接是 PoBR 词表，**无运行时翻译层**；数据修一次、引擎零特判；与「计算内部只用稳定 StatId」的项目铁律一致。
- **缺点**：要先建 vendor→PoBR alias 表（同样从 legacy 词表自举）；mod_parser_rules.json 重生 + 双跑重测。
- **本质**：把分歧消化在数据生产期，而非运行期——更干净，符合 P3/P10 数据驱动终局。

两条路都需要同一张 **vendor→PoBR 别名表**（核心工作量），可从 legacy parser 现有的「短语→PoBR 名」映射反向自举 + 人工核对 358 条 name-only DIFF。区别只是这张表用在「运行期翻译」(A) 还是「抽取期归一」(B)。

## 推荐

**B**。理由：(1) 保持引擎纯数据驱动、无运行期特判；(2) 输出即 PoBR StatId，下游零改动即可切换；(3) alias 表是一次性数据资产，version-bump 时随 name_map 重抽自动复用；(4) 契合「计算内部只用稳定 ID」铁律。

落地序（B 路线的 D-T8 重定义）：
1. 建 `vendor→PoBR` alias 表（自举 legacy 词表 + 核对 358 name-only DIFF + structural 152 的聚合/PerStat 归一规则）。**[done]**
2. extract-lua name_map 抽取期套 alias → 重生 mod_parser_rules.json。**[done — 第一波]**
3. 双跑重测：目标 C1 DIFF 收敛到 0（structural 分歧逐条归一或登记为 legacy bug 修正）。
   **[第一波部分达成：name-only 358→1、structural 152→13、EQ 309→805；剩余 13+41
   OLD_ONLY 全属 special 通道 / EnemyModifier 包装 / 真 bug——见 m6-dualrun-report.md §2.5]**
4. DIFF=0 达成 → D-T8 切换 commit（默认开 parser-engine、调用方注入 CompiledParserRules、删 legacy）+ 全量门禁 + baseline 审查（若引擎修正了 legacy 的真 bug 致 parity 变动，独立 commit 显式审查）。

### D-T8 第二波就绪状态（M6.3 第一波交付后）

第一波（commit `2f7dbcc`→`cd58fed`，基线 `76dcefe`）已落地路线 B 的抽取期归一 +
引擎 C1–C6 结构归一，**行为中性**（parser-engine 默认关、未接调用方、parity 零回归）。
切换（第二波）前的**剩余必办**（按优先级）：

1. **special 通道接入**（最大块）：vendor `specialModList` 闭包条目——OLD_ONLY 41
   全是（`Allocates X` / `Gain Deflection` / `Charm Slots` / `Unwavering Stance` …）
   + 4 条 structural（`bypasses Energy Shield` / `for each type of Elemental Ailment`
   / `as Extra Damage of all Elements` / `Your Critical Damage Bonus`）。引擎
   `special_meta` 框架已在，需接 special 通道 dispatch。**[done — 第二波 2a]**
2. **EnemyModifier LIST 包装**（D8）：`Enemies in your Presence` / `Enemies you
   Curse take`——`apply_to_enemy`/`actor_enemy` 需 wrap 为 `EnemyModifier LIST`
   + inner 附 `Condition(Effective)` + 敌侧条件（注意 legacy `enemies you curse`
   用 `EnemyCursed`，data 用 `Cursed` + `mod_suffix:Taken`，包装时需对齐）。
   **[done — 第二波 2a：`prefix_enemy_condition` 加 `Enemy<X>` 前缀，`mod_suffix` 接入]**
3. **真 bug baseline 审查**（切换可能动 parity，独立 commit）：
   **[第二波 2a 已让引擎产 legacy-一致值使 DIFF=0；本波 parity 影响 0（未接调用方）。
   逐条 2b 审查见 m6-dualrun-report §2.6「4 真 bug 逐条处置」]**
   - `from Equipped Focus`：2a 从 `from equipped focus` 数据移除 `Condition(UsingFocus)`
     对齐 legacy。2b 若加回装备条件须同步消费侧 + parity 复核。
   - `per N Item ES on Equipped Helmet`：2a 改引擎数据 `OnHelmet`→`Onhelmet` 对齐
     legacy + 消费侧 slot_id（本就小写）——2b 无差异。
   - `Triggered Spells deal ... Spell Damage`：2a 当 inner 带 `SkillTypes(Triggered)`
     时保留 SPELL 位（`normalize_pobr_name(keep_spell)`）——SPELL 子集匹配语义等价。
   - `LifeGainAsEnergyShield`：2a 在 `GainAs` 后缀名回退 `MaximumLife`→`Life`——
     名与 legacy / 消费侧通道一致。
4. **回归门禁**：`c1_diff_zero_gate` 已从 `#[ignore]` 转**正式断言**（DIFF=0 且
   OLD_ONLY=0，进 CI）；`c1_converged_floor_gate` 收紧为 EQ≥860 下界。
   **[done — 第二波 2a]**

**第二波 2a 完成态**（commit `feat(m6-conv2): ...`，基线 `1652b80`）：C1 DIFF=0 /
OLD_ONLY=0 / EQ 805→860，行为中性、parity 零回归。**2b 切换**（默认开 +
调用方注入 `&CompiledParserRules` + 删 legacy）就绪——清单见 m6-dualrun-report §2.6
「2b 切换就绪清单」。

## 切换阻塞（2026-06-15 收尾波实测 —— owner 复核项）

D-T8 第三波（真正翻开关）实测**parity 回归，切换暂缓**。过程与根因如下。

**实测**（pobr-build `default = ["parser-engine"]`，orchestrator 经 BuildData 编译
`CompiledParserRules` 并 `set_parser_rules` 注入、minion 走引擎桥）：
- `parity_no_regression` 失败：`defensive core-8 @5% 132 → 129`。
- 逐 build 报告（engine-on vs engine-off diff）：远不止 3 个边界 stat——
  `druid-oracle-comet` 的 **Armour 1460 → 0**、**PhysDR 13 → 0**、TotalEHP 23312→21176、
  TotalDPS 63649→57640、TotalDotDPS 182→154；多 build 的 EHP/DPS/DoT 普遍下挫。
  即引擎接管后**系统性丢词条**，非取整噪声。

**根因（代码级确认）**：`mod_parser/legacy.rs:207-213` 的 `ParseCtx::parse`——
engine 分支直接 `return parse_mod_engine(text, engine)`，**不回退 legacy 手写专用解析器**。
引擎只覆盖数据驱动 `forms`/`name_map`/`special` 三表；legacy 经手写 specialized 函数族
（parse_form / parse_keystone_special / 各专用分支）覆盖更广的词条形态。
- C1 双跑（parser_dual_run.rs）DIFF=0 / OLD_ONLY=0 **只在 18-build 的 item+passive
  文本语料上成立**；
- 但全 calc 路径还 ingest 宝石授予效果 stat、光环/buff 展开、基底隐式等来源，
  其文本**不在 C1 语料**、也**未被数据表覆盖** → 引擎产 Unsupported 丢弃 → parity 崩。
- `corpus_unsupported_report`（仅跑 legacy/special 通道）engine-on/off 字节相同，
  佐证回归来自引擎 ingest 路径而非 legacy 度量。

**结论**：「C1 DIFF=0 ⇒ 切换 parity 中性」的前提**在全 ingest 路径上不成立**。
当前接线已落地、`parser-engine` 默认关、行为中性（master `parity_no_regression` 绿）。

### gap 量化（2026-06-15，POBR_DBG_UNSUPPORTED 全 ingest 实测）

用既有 `POBR_DBG_UNSUPPORTED=1` 仪表跑 `parity_baseline_report`（全 18 build
真实 ingest 路径）engine-on vs engine-off，对未支持词条集合取差：

- engine-unsupported 唯一模板 = **56**；legacy-unsupported = **121**（引擎反而比
  legacy 标记更少未支持——引擎"解析"了更多文本，部分进了**不同/错误**的 mod）。
- **OLD_ONLY 纯丢弃缺口（engine 丢、legacy 解）= 仅 10 个唯一模板**（可枚举、可逐条扩
  forms/special 表闭合）：`additional maximum Life equal to 100% of the Item Energy
  Shield on Equipped Body Armour` / `Archon Buffs also grant …`(×2) / `Empowered
  Attacks Gain 15% of Physical Damage as Extra Fire damage` / `Prevent +N% of Damage
  from Deflected Hits`(×2) / `Your Life cannot change while you have Energy Shield`
  / `Magnitude of Poison you inflict` / `Slowed by 20%` / 裸 `Damage`。

**关键反直觉发现（POBR_DBG_STAT 逐源核对，2026-06-15）**：parity 回归**根本不是
parser 问题**。对 `druid-oracle-comet`（Armour 1460→0）逐源 dump：
- `POBR_DBG_STAT=Armour`：engine 与 legacy **逐字节相同**——均 `Armour Base
  Number(328.0) tags=[SlotName("bodyarmour")] origin=base.Armour`。
- `POBR_DBG_STAT=Defences`：engine 与 legacy **逐字节相同**——同 2 条全局 `Defences
  Inc`（15 CanUseBondedModifiers / 30），值/tag/source/origin 全等。
- `POBR_DBG_DROPPED`：engine 与 legacy **逐字节相同**（均 123 条结构丢弃）。
- 即**四个诊断通道（unsupported / Armour mods / Defences mods / dropped）全逐字节
  相同，却算出 Armour 1460 vs 0**。⇒ 分歧在**未 dump 的某 ModName 名下** 或
  condition/multiplier/状态差异——须**全 ModDb dump 逐 mod diff**（现仅有按名 dump
  仪表，需加一个 dump-all 模式）+ defence.rs 本地 armour 计算逐步 trace 才能定位。

⇒ 回归源于**引擎 `ParseCtx` ingest 路径的更隐蔽行为差异**（疑似 local-mod 处理 /
slot 本地增伤 tag / ingest 顺序），而非"丢词条"或"错解析可见 mod"。这与 CLI 单条
`parse-mod` engine==legacy 完全自洽（单条解析没问题，但**整件装备 ingest 的本地 mod
处理在引擎 ctx 下产出了不同的不可见 mod / 计算路径**）。**全 ingest 逐文本 mod-输出
对照**（下一波）需对比的不止 parsed/unsupported，而是**每件来源 ingest 后玩家 ModDb
的完整 mod 集合 diff**（含 SlotName-tagged 本地 mod / 隐式折叠值）。

**这意味着 hybrid 回退 (b) 也不足以达 parity**：引擎对这些文本不标 Unsupported
（产出了 mod，只是 ingest 计算路径有别），fallthrough 永不触发 → (b) 只能救回 10 个
纯丢弃，救不了这类"ingest 路径分歧"。故无论 (a)/(b)，**引擎 ingest 路径的非丢弃分歧
是首要、独立的 root-cause 任务**，且规模/性质未知（可能是 ingest_item 在 engine ctx
下本地 mod 标 tag 或基值折叠的一处 bug，也可能是系统性的）。

**修正后的 fork 评估**：(a) 纯 A2 = 闭合 10 丢弃 + 根因修复非丢弃分歧（后者规模未知，
是真风险）；(b) hybrid = 即便保留 legacy fallthrough 仍达不到 parity（非丢弃分歧绕过
fallthrough）。⇒ **下一波无论如何先做"全 ingest 逐文本 mod-输出对照"**（非仅 parsed/
unsupported 状态对照），把非丢弃分歧逐条列出，再谈 fork。

**翻开关前必办（下一波）**：
1. **扩双跑语料到全 ingest 集**：把 engine-vs-legacy 逐文本对照从 item+passive
   扩到宝石/buff/基底隐式等**全部 ingest 来源**（即 orchestrator 实际喂给
   `parse_ctx` 的全集），重新统计真实 OLD_ONLY（legacy 解析、引擎丢弃）缺口。
2. **把 OLD_ONLY 驱到 0**：对缺口词条形态扩 `forms`/`name_map` 数据表（extract-lua
   重抽 + 工具再生 mod_parser_rules.json，禁手改），逐条让引擎产出 == legacy。
3. 全 ingest DIFF=0 / OLD_ONLY=0 达成后，才单 commit 翻 `default = ["parser-engine"]`
   + 跑 `parity_no_regression`（应零回归）。**若仍动 parity 报 owner，禁自行 bump baseline。**
4. （可选）A2 删 legacy 须在引擎全覆盖证实后；在此之前 legacy 是 feature-off 回退路径，
   不可删。

## 不阻塞项（切换决策之外可并行推进）
- special 长尾批次（F，overlay/special_mods.json 扩批，独立）。
- version-bump-drill.sh 扩展第 4 步 precompile 实跑（F，不依赖切换）。
- bench 三组补全（F-T10）。
- stat_id 双通道（E）部分前置核实（但实施依赖 B+D 切换后）。

## 收尾波状态（切换已翻默认后；删 legacy 序列）

切换早已翻默认（`pobr-build default=parser-engine`，fork-a 收官「最后一行 engine-behind」修完）。删 legacy 分三步：

- **1/3**（commit `e8b816e`）：解析输出共享类型迁出 `legacy.rs` → `mod_parser/outcome.rs`。
- **2/3**（commit `671b686`）：`ParseCtx` 迁出 `legacy.rs` → `mod_parser/dispatch.rs`，与 legacy 解耦。
- **3a/3**（commit `73c78e9`）：去 `parser-engine` feature 门控，引擎无条件编译，`aho-corasick` 无条件依赖，legacy 降为纯回退路径。三者皆**行为中性**、1540 tests 全绿。
- **3b（物理删 `legacy.rs` ~4085 行 + 迁测试到引擎）：未做，须本地环境完成。** 可回退探针实测引擎对 `mod_parser.rs` 单测有 **13/84 真实分歧**（PerStat/Multiplier tag 丢失、武器职业 bit 编码、聚合抗性展开等），云环境无 `vendor/` + luajit 无法重生规则/对照 vendor 裁决。**详见 `m6-delete-legacy-3b-handoff.md`（含逐条分歧 + 机械清单）。**
