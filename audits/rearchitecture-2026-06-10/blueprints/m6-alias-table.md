# M6.3 vendor→PoBR 别名表 + 分歧归一规格

> M6.3 切换前置**数据资产**（A/B 两路共同前置，见 `m6-switch-decision.md`）。
> 基线 commit `2f6e4a0`。纯数据/分析，**不接线、不改引擎、不改抽取、零行为变更**。
> 数据产物：`data/4.5.0.3.4/overlay/vendor_name_aliases.json`（schema
> `vendor_name_aliases/v1`，零消费）+ `pobr-data::catalog::vendor_name_aliases`
> （零消费 serde 形状 + 往返单测）。
> 输入证据：双跑报告 `m6-dualrun-report.md` §3（358 name-only DIFF + 152
> structural DIFF + 41 OLD_ONLY）。

## 1. 自举方法

按**同一触发短语**对齐两侧 parser 的「短语→ModName」映射：

- **PoBR 侧**：`crates/pobr-core/src/mod_parser/legacy.rs::parse_name`（M0–M5 手写
  parser 的短语→PoBR StatId 映射，155 个短语条目）。
- **vendor 侧**：`data/4.5.0.3.4/overlay/mod_parser_rules.json` 的 `name_map` 段
  （vendor 短语→vendor ModName，775 条，`{phrase, names[]}` schema）。

对任一短语 P，若两侧都把 P 解析出**单一** ModName，则
`engine 名(vendor) → legacy 名(PoBR)` 即一条别名。多名（聚合）短语与 vendor 用
`Damage`/`Speed` 泛名 + flag dispatch 的短语**不入纯别名表**——它们是结构性分歧
（§3），登记在 JSON 的 `structural_deferrals`。

### 自举结果

| 桶 | 数量 | 说明 |
|----|------|------|
| 共享短语（单名两侧） | 114 | 自举证据短语 |
| → distinct vendor 名 | **76** | 别名表条目（多短语收敛同名） |
| ┣ real-rename（vendor≠PoBR） | **20** | 切换时下游 miss 的根因子集 |
| ┗ identity（vendor==PoBR） | 56 | 名相同，切换零影响 |
| 共享多名短语（聚合，不入别名） | 1 | `skill speed`（§3 cat 1） |
| legacy-only 短语（engine 走 flag dispatch / 异短语） | 40 | §3 cat 3 主体（damage flag） |

## 2. 别名表覆盖率（358 name-only DIFF）

**结论：358 name-only DIFF 涉及的 distinct stat 名 100% 被别名表覆盖。**

- name-only DIFF 的定义即「去名后逐字节等价、仅 ModName 不同」（dual-run
  `name_blind(lc)==name_blind(ec)`）。两侧都 Parsed ⇒ 短语在两侧 name_map 都命中
  ⇒ 该 vendor→PoBR 对必在自举的 114 共享短语内。
- 双跑报告 top-220 明细捕获的 name-only 样例 8/8 命中（`Life→MaximumLife`、
  `Str→Strength`、`Int→Dexterity/Intelligence`、`ChaosResist→ChaosResistance`、
  `ColdResistMax→MaximumColdResistance`、`LightningResistMax→…`、`Mana→MaximumMana`
  等），无 MISS。
- 358 是**语料实例数**（同一对在多件装备/多个数值上重复，如 `+N to maximum Life`
  跨 N），distinct **名对**远少于 358，全部落在 20 real-rename 内。

### 20 条 real-rename（vendor → PoBR，切换核心）

| vendor_name | pobr_stat_id |
|-------------|--------------|
| `ChaosResist` | `ChaosResistance` |
| `ChaosResistMax` | `MaximumChaosResistance` |
| `ColdResist` | `ColdResistance` |
| `ColdResistMax` | `MaximumColdResistance` |
| `CritChance` | `CriticalStrikeChance` |
| `CritMultiplier` | `CriticalStrikeMultiplier` |
| `Dex` | `Dexterity` |
| `ElementalResistMax` | `MaximumAllElementalResistances` |
| `EnemyBleedDuration` | `BleedDuration` |
| `EnemyFreezeBuildup` | `FreezeBuildup` |
| `EnemyIgniteDuration` | `IgniteDuration` |
| `EnemyPoisonDuration` | `PoisonDuration` |
| `FireResist` | `FireResistance` |
| `FireResistMax` | `MaximumFireResistance` |
| `Int` | `Intelligence` |
| `Life` | `MaximumLife` |
| `LightningResist` | `LightningResistance` |
| `LightningResistMax` | `MaximumLightningResistance` |
| `Mana` | `MaximumMana` |
| `Str` | `Strength` |

其余 56 条 identity（`Armour`/`Evasion`/`EnergyShield`/`FireDamage`/`Accuracy`/
`BlockChance`/`AilmentChance`/`CooldownRecovery`… 全量见 JSON）切换时词表不变，
保留在表内是为「全名集可枚举 + version-bump 重抽时一次性核对」。

> **人工核对项（0 条阻塞）**：自举对 358 name-only 全覆盖，无需人工补录。
> `EnemyBleedDuration→BleedDuration` 等四条「敌侧时长去 Enemy 前缀」与既有 statmap
> 归一（`stat_map_engine` 的 `EnemyPoisonDuration→PoisonDuration`）一致，已校验。

## 3. structural 152 条四类归一规格（形式化，不实现）

按双跑报告 §3.2 四类分类（top-220 明细完整捕获全部 152 structural + 41
OLD_ONLY）。实测分布：

| 类 | §3.2 名称 | 实测计数 | 真 bug 归属 |
|----|-----------|----------|-------------|
| C1 | 聚合名展开 vs 单名 | 85 | 引擎需补（展开规则） |
| C2 | PerStat vs Multiplier tag | 2 | 引擎需补（tag 归一） |
| C3 | damage flag vs 专名 | 31 | 引擎需补（flag→专名 + DoesNotApply 等价） |
| C4 | name_map 覆盖差 | 4 | 引擎需补（NEW 能力缺口，非回归） |
| C5* | DamageType tag 缺失 | 28 | **引擎需补**（PoBR 计算依赖 tag；§3.2 未单列） |
| C6* | SlotName vs Condition（`from Equipped X`） | 3 | 部分 **legacy 弱**、部分引擎缺口 |
| **合计** | | **152** | |

\* C5/C6 在原 §3.2 四类外，是 top-220 明细暴露的细分；归并入「需归一规格」。

### C1 — 聚合名展开（85 条）

- **现象**：`+N% to all Elemental Resistances` → legacy 拆 `FireResistance` /
  `ColdResistance` / `LightningResistance` 三 mod；engine vendor name_map 单
  `ElementalResist`。`+N to all Attributes` → legacy `Strength/Dexterity/Intelligence`
  三分；engine `["Str","Dex","Int","All"]` 四名。双类型组合（`Strength and
  Intelligence`、`fire and cold resistances`）、`skill speed`（→`Speed`+
  `WarcrySpeed`+`TotemPlacementSpeed`）同理。
- **归一规则（规格）**：
  ```
  expand(vendor_names) =
    for each vendor_name n in entry:
      if n in AGGREGATE_EXPANSION:           // 数据表：聚合名 → PoBR 子名集
        emit each child PoBR StatId (复制该 mod 的 type/value/flags/tags)
      else:
        emit alias(n)                        // 走 §2 别名表
    去重（同 PoBR 名合并）
  ```
  需要的数据：聚合展开表 `vendor聚合名 → [PoBR 子名]`，例：
  `ElementalResist → [FireResistance, ColdResistance, LightningResistance]`、
  `ElementalResistMax`(聚合形) → 三 max、`All → [Strength, Dexterity, Intelligence]`、
  `StrInt → [Strength, Intelligence]`（vendor 把 `Str and Int` 解析为 `Str/Int/StrInt`，
  其中 `StrInt` 是 vendor「组合属性」名——PoBR 无此名，需展开为两 base 或丢弃组合腿）。
- **真 bug 归属**：引擎忠实落 vendor 聚合名是**对的**（vendor 在 ModStore 侧自带
  `ElementalResist`→三抗的展开逻辑）；PoBR 下游没有这层 ModStore 展开，故归一层
  须在数据生产/翻译期把聚合名展开成 PoBR 子名。**非 legacy bug，引擎需补展开。**

### C2 — PerStat vs Multiplier tag（2 条）

- **现象**：`+2 to Armour per 1 Spirit` / `+1 to maximum Mana per 2 Item Energy
  Shield on Equipped Helmet` → legacy `Multiplier{var=Spirit,div=1}` tag；engine
  `PerStat{stat=Spirit,div=1}` tag（vendor ModStore `PerStat` 语义）。
- **归一规则（规格）**：`PerStat{stat,div,limit,...}` ↔ `Multiplier{var=stat,
  div,limit,...}` 字段一一对应改写（tag 变体名翻译，载荷不变）。计算侧
  `effective_number(cfg)` 当前只识别 `Multiplier`，故归一方向 = `PerStat → Multiplier`
  （或反向，由 A/B 选定的消费侧决定）。
- **真 bug 归属**：vendor 用 `PerStat` 是其原生 tag；PoBR 用 `Multiplier` 等价表达。
  **非 bug，tag 名归一即可。**

### C3 — damage flag vs 专名（31 条）

- **现象**：`Spell Damage` → legacy `SpellDamage`，engine `Damage`+SPELL flag
  (`0x2`)；`Damage with Spears` → legacy `SpearDamage`，engine `Damage`+Spear flag
  (`0x10000004`)；`Damage with Crossbows` → `Damage`+`0x4000004`；`Elemental
  Damage with Attacks` → legacy `ElementalDamage`+Attack flag (`0x1`)，engine
  `ElementalDamage`+Attack **keyword** (`0x10000`)；`Magnitude of Poison you
  inflict` → legacy `AilmentMagnitude`+POISON kw (`0x200000`)，engine 无 kw；
  `Poison on Hit with Attacks`、`Spell Hits Gain … as Extra` 同属 flag/kw 编码差。
- **归一规则（规格）**：vendor 用「泛 `Damage` 名 + flag/keyword」表达作用域，PoBR
  用「专名」表达。两套等价，需双向规则表：
  ```
  // vendor (Damage, flagset) → PoBR 专名
  (Damage, SPELL)            → SpellDamage
  (Damage, Spear)            → SpearDamage
  (Damage, Crossbow)         → CrossbowDamage
  (Damage, Bow)              → BowDamage
  (Damage, Mace)             → MaceDamage
  (Damage, Quarterstaff)     → QuarterstaffDamage
  (Damage, Area)             → AreaDamage
  (Damage, Grenade)          → GrenadeDamage
  // 同时去掉被专名吸收的 flag/keyword 位（避免双重 gating）
  // ElementalDamage：Attack flag(0x1) ↔ Attack keyword(0x10000) 二选一（消费侧统一）
  // AilmentMagnitude：POISON/BLEED/IGNITE keyword 归一（legacy 带 kw，engine 不带 → 引擎需补）
  ```
- **真 bug 归属**：vendor 的 `Damage`+flag 是其 ModStore form dispatch 正解；PoBR 专名
  是历史选择。**非 legacy bug**；但 `AilmentMagnitude` 的 POISON/BLEED/IGNITE
  keyword、`PoisonChance`/`Elemental Damage with Attacks` 的 flag/kw 落点
  **引擎需补**（当前引擎漏挂作用域 keyword，归一前会丢 gating）。

### C4 — name_map 覆盖差（4 条，引擎能力缺口）

- **现象**（engine 留 `unparsed`，部分消费）：
  - `12% of Damage taken bypasses Energy Shield` → engine `DamageTaken` BASE 12 +
    unparsed `bypasses Energy Shield`；legacy `ChaosEnergyShieldBypass` 等专名展开。
  - `21% increased Damage for each type of Elemental Ailment on Enemy` → engine
    `Damage` INC + unparsed 尾缀；legacy 整行展开 5 条按敌方异常条件化 Damage INC。
  - `Gain 5% of Damage as Extra Damage of all Elements` → engine `Damage` BASE +
    unparsed `as Extra Damage of all Elements`；legacy 展开 Fire/Cold/Lightning 三条。
  - `Your Critical Damage Bonus is 250%` → engine unparsed `Your` 前缀；legacy
    `CritMultiplier` OVERRIDE 250。
- **归一规则（规格）**：这些是 engine name_map / form 覆盖缺口，**非别名问题**——
  需在 engine 侧补对应 name_map 短语 / form handler（B 路线在 extract-lua 补；A 路线
  仍需 engine 补，翻译层无法凭空生成缺失的 mod）。
- **真 bug 归属**：**引擎需补**（NEW 能力缺口，legacy 更全；非 legacy bug）。

### C5 — DamageType tag 缺失（28 条，引擎需补）

- **现象**：`123% increased Physical Damage` → legacy `PhysicalDamage` INC +
  `DamageType(Physical)` tag；engine `PhysicalDamage` INC，**无** DamageType tag。
  Fire/Cold/Lightning/Physical 各类伤害 INC 普遍如此。
- **归一规则（规格）**：归一层据 PoBR StatId 补 DamageType tag——
  ```
  attach_damage_type(name) = match name {
    PhysicalDamage→Physical, FireDamage→Fire, ColdDamage→Cold,
    LightningDamage→Lightning, ChaosDamage→Chaos, _→none
  }
  ```
  （与 legacy `damage_type_for_name` 同表。）
- **真 bug 归属**：**引擎需补**。PoBR 计算的伤害分桶依赖 DamageType tag；若 B 路线
  落 PoBR 名而不补 tag，伤害归桶会错。注：vendor 不挂此 tag 是因 vendor 用
  `Damage`+flag 体系（C3），名本身即类型信息；PoBR 专名体系需 tag 显式标注。

### C6 — SlotName vs Condition（`from Equipped X`，3 条）

- **现象**：`44% increased Energy Shield from Equipped Focus` → legacy
  `EnergyShield` INC + `SlotName(weapon2)` tag；engine `EnergyShield` INC +
  `Condition(UsingFocus)` tag。`from Equipped Body Armour` → legacy
  `SlotName(bodyarmour)`，engine **空 tags**（丢失槽位限定）。
- **归一规则（规格）**：
  - `from Equipped Focus` 类：`Condition(UsingX)` ↔ `SlotName(slot)` 语义不等价
    （condition = 「装备了 X 时全局生效」；SlotName = 「仅该槽位的局部值」）。
    PoB2 此处实际是**装备条件**（vendor `Condition:UsingFocus`），**legacy 的
    SlotName 解读是弱化近似** —— 这是 legacy 行为偏差，引擎更接近 vendor。
  - `from Equipped Body Armour` 引擎落空 tags = **引擎缺口**（漏挂条件）。
- **真 bug 归属**：**混合**——`from Equipped Focus` 引擎更对（legacy 弱）；
  `from Equipped Body Armour` 引擎漏挂条件（引擎需补）。

### 附：OLD_ONLY 41 条（`Allocates <passive>`，special 通道）

- legacy 手写特例产 `GrantedPassive LIST`；engine 未接 special 通道 → form 失配
  Unsupported。**非词表问题**，属双跑报告 §3.3 / 蓝图 §4 步 3 的 special 通道接入，
  不在本别名表范围（D-T8 切换的并行项）。

### 真 bug 清单（归一时须显式审查 baseline 的项）

| 项 | 类 | 归属 | 切换动作 |
|----|----|------|---------|
| `from Equipped Focus` → SlotName | C6 | **legacy 弱**（引擎 `UsingFocus` 更对） | 切到引擎语义 → 可能改 parity，独立 commit 审查 |
| `AilmentMagnitude` 漏 POISON/BLEED/IGNITE kw | C3 | 引擎需补 | extract/engine 补 keyword |
| `Elemental Damage with Attacks` flag↔kw | C3 | 引擎需补（落点统一） | 消费侧 gating 统一 |
| C4 四条 name_map/form 缺口 | C4 | 引擎需补 | extract-lua 补短语/form |
| C5 DamageType tag 缺失 ×28 | C5 | 引擎需补 | 归一层补 tag |
| `from Equipped Body Armour` 落空 tags | C6 | 引擎需补 | extract/engine 补条件 |

> 除 `from Equipped Focus`（legacy 弱、切换后引擎更对、parity 可能变动）外，其余均为
> 「引擎当前缺、归一/补齐后趋同 legacy」——补齐方向收敛，不应使 parity 下降。

## 4. A/B 两路接线点（供 owner 定夺后 D-T8 直接用）

两路都消费**同一张** `vendor_name_aliases.json`，区别仅在消费时机：

### A 路线 — 引擎产物运行期翻译层

- **接线点**：`parse_mod_engine(text, rules) -> ParseOutcome` 产出后、返回前，新增
  `normalize_engine_outcome(outcome, &aliases, &expansion_rules)`：
  1. 每个 mod 的 `name`：查别名表 `vendor → PoBR`（§2）；
  2. 聚合名（C1）：按展开表拆成多 mod；
  3. tag 归一：`PerStat→Multiplier`（C2）、补 `DamageType`（C5）、`from Equipped`
     条件（C6）；
  4. damage flag→专名（C3）：按 (Damage, flagset) 规则改名 + 清吸收位。
- **数据接入**：`CompiledParserRules` 旁挂编译后的 alias/expansion 表（pobr-core 编译，
  gamedata 只 load + merge，保 P9）。
- **改动面**：仅 pobr-core parser 出口 + 一张翻译表；引擎本体保持 vendor-faithful。
- **C4 缺口**：翻译层无法生成缺失 mod，仍需 engine name_map 补（A 路线也躲不开）。

### B 路线 — extract-lua 抽取期归一（决策文档推荐）

- **接线点**：`sync-pob-catalog extract-lua --what parser-rules` 写 `name_map` 时，
  对每个 `{phrase, names}` 的 `names` 套别名表归一为 PoBR StatId：
  1. real-rename：`Life→MaximumLife` 等直接改 `names`；
  2. 聚合名：`names:["ElementalResist"]` → 展开为 `["FireResistance",...]`（或保留
     聚合名 + 在 form 消费侧展开，二选一，与 PoBR resolve_names 对齐）；
  3. C5 DamageType tag、C3 damage flag→专名、C2 PerStat→Multiplier 在 form/tag 抽取
     期一并归一（须扩展 extract-lua 的 tag/form 转录逻辑）。
- **产物**：重生 `mod_parser_rules.json`（`name_map`/`tag_phrases`/`forms` 直接是
  PoBR 词表），引擎**无运行期翻译层**，下游零改动即可切换。
- **改动面**：extract-lua 抽取逻辑 + 重生 JSON + 双跑重测（目标 C1 DIFF→0）。
- **契合**：把分歧消化在数据生产期，符合 P3/P10 数据驱动终局、「计算内部只用稳定
  StatId」铁律。

### 两路共用前提（本资产已就绪）

- §2 别名表（76 条，含 20 real-rename）：两路的 name 归一基础。
- §3 四类归一规格：两路的 tag/聚合/flag 归一规则来源。
- C4 缺口：两路都需 engine name_map/form 补齐（不可由翻译/抽取归一凭空生成）。

## 5. 边界与门禁

- **未改**：`mod_parser/**`、extract-lua、任何调用方、`parser-engine` feature、
  `ninja_parity` baseline。新增 catalog 类型零消费（仅 serde 往返单测）。
- **门禁**：纯数据资产无消费方 ⇒ `parity_no_regression` 必然零回归；
  `cargo nextest run --workspace` + clippy + fmt 全绿。
