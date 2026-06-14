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
1. 建 `vendor→PoBR` alias 表（自举 legacy 词表 + 核对 358 name-only DIFF + structural 152 的聚合/PerStat 归一规则）。
2. extract-lua name_map 抽取期套 alias → 重生 mod_parser_rules.json。
3. 双跑重测：目标 C1 DIFF 收敛到 0（structural 分歧逐条归一或登记为 legacy bug 修正）。
4. DIFF=0 达成 → D-T8 切换 commit（默认开 parser-engine、调用方注入 CompiledParserRules、删 legacy）+ 全量门禁 + baseline 审查（若引擎修正了 legacy 的真 bug 致 parity 变动，独立 commit 显式审查）。

## 不阻塞项（切换决策之外可并行推进）
- special 长尾批次（F，overlay/special_mods.json 扩批，独立）。
- version-bump-drill.sh 扩展第 4 步 precompile 实跑（F，不依赖切换）。
- bench 三组补全（F-T10）。
- stat_id 双通道（E）部分前置核实（但实施依赖 B+D 切换后）。
