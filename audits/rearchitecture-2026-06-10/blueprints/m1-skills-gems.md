# M1 实施蓝图 — 技能/宝石数据链路

> 撰写：2026-06-11 · 基于 21-roadmap M1 节 / 20-target-architecture（P2/P5/P9/P13）/ 18-skills-gems.md / 15-data-pipeline.md
> 本文档**自包含**：实施 agent 只读本蓝图 + 代码即可开工。所有 vendor 行号与 pobr 现状均已于 2026-06-11 在 `m0/data-foundation` 分支上重新核实。
>
> **前置假设**：M0 已收尾——三层目录 `data/4.5.0.3.4/{base,overlay,generated}`、manifest v2、overlay merge（`pobr-gamedata/src/overlay.rs`）、`GameData::load_ruleset()` 聚合入口（`pobr-gamedata/src/ruleset.rs`，W3 正在把 calc 常量切到注入的 `RuntimeConstants`）、`sync-pob-catalog extract-lua` 子命令（luajit + 引导脚本，产物 byte-stable）、CI 防线（`devs/scripts/regen-check.sh` + pobr-data 禁内嵌数据 lint）均已存在。本蓝图按"注入管道已存在"撰写：M1 新增的数据域照搬 M0 模式（catalog schema → gamedata loader → pobr-build 注入 → pobr-core 纯函数收引用）。

---

## 0. 阶段目标与统一门禁（roadmap 原文摘录）

**目标**：打通宝石品质、statmap、support 适用性三条断裂链路——进攻 parity 最大的系统性低估来源。

**阶段验收**（roadmap M1 节原文）：

> 进攻 parity 24% → **≥40%@5%**；quality-20 宝石 fixture；statmap 双跑 diff 报告干净；oracle 对拍 statmap 抽样。

**统一门禁三件套**（roadmap §0 原文，每次合并回 master 适用）：

> 1. `cargo test --workspace` + `clippy -D warnings` + `fmt --check` 全绿；
> 2. **ninja_parity 18-build 零回归**——防御 51% / 进攻 24%（@5% 容差）为底线不得倒退；
> 3. 涉及解析/数据的阶段：加 pob2-oracle 对拍或 generated 重生一致性校验。

**执行纪律**（roadmap §0 原文，FINDINGS 04-02 教训制度化）：

> - **搬迁不变式**：纯搬迁（数据出代码、入 JSON）的 commit，parity baseline **逐值不变**（golden diff = 0）；搬迁与行为改动永远分两个 commit。
> - 行为修复必须附 PoB2 一手依据（源码行号/oracle 中间值）；baseline 更新独立 commit、显式审查。
> - 核心改动（mod_db/ModFlags/stat_map 引擎切换等）feature-gated **双跑对照**，diff 报告干净后才删旧码。

当前 baseline 常量（`crates/pobr-build/tests/ninja_parity.rs:270-273`，回归门禁断言对象）：

```
BASELINE_DEF_HIT5 = 111   BASELINE_DEF_HIT10 = 117
BASELINE_OFF_HIT5 = 23    BASELINE_OFF_HIT10 = 31
```

parity 跑法：`cargo test -p pobr-build --test ninja_parity -- --nocapture`（18 个 build 在 `examples/demo-bd-test/builds/*/`，黄金值 = 各 build `meta.json::player_stats`）。oracle 对拍：`tools/pob2-oracle/`（`oracle.lua` + `run.sh`，luajit headless 跑 PoB2 取中间值）。

**对应 high 缺口**：18-G1（quality 四层）、18-G2（support 裁决）、18-G3 / 15-G2（SkillStatMap）、18-G4（多 statSet）、15-G5（qualityStats 数据）、15-G3 部分（vendor 抽取列可再生化）、18-G6/18-G7（reservation/SupportManaMultiplier 族）。

**范围澄清（总架构评审 2026-06-11，对 roadmap 附 A 的归属修订）**：roadmap 附 A 把 16-G4（BaseItemDef 缺列）与 14-G5 注记的 `granted_effects.minion_list` 标为「M1 落库」；八份蓝图统一裁决为**消费阶段自带落库**——`block_chance`/`spirit` → M2 Track D；`reload_time_ms` → M4 W-D2；`flask{}/charm{}` → M3 T4-D2；`minion_list` 外键 → M5a Track A3（overlay 边车，非 .dat 列）。**M1 不做上述各列**。例外：`granted_effect_levels.level_requirement` 由 M1 T4 落库（同表同批改列成本最低），M5a 只消费。

---

## 1. 现状核验结论（蓝图撰写时逐项验证，实施 agent 可直接信赖）

### 1.1 「core 归因 API 已就绪」——✅ 验证成立，但有一个关键限定

`crates/pobr-core/src/skill_source.rs` 已具备且**全仓（非测试）无调用方**：

| API | 位置 | 语义 |
|---|---|---|
| `SupportGemSpec::with_quality(quality, mods)` | skill_source.rs:277 | 设 quality + quality_mods |
| quality_mods 注入 | skill_source.rs:588-634 | 逐条注为 `SourceKind::GemQuality` 归因（id = `gem.<id>.q<Q>`） |
| `SupportGemSpec::with_mana_multiplier(mult)` | skill_source.rs:248 | 注入消费在 :526 |
| `can_support(require, active_types)` | skill_source.rs:379 | **仅位集交集**，无表达式/exclude/不动点 |

**关键限定**：parity 主路径（ninja_parity → `calculate_with_data`）走的是 `calc_orchestrator.rs` 自有管线（`skill_base_modifiers` :1409 / `support_modifiers` :1606 → `mapped_stat_modifiers` :1703 → `skill_stat_map.rs`），**不经过** `skill_source::ingest_*`（那是 `CalculationSession`/CLI 最小路径）。所以「orchestrator 接 with_quality」的真实落点是：**orchestrator 在 stat 取数阶段做 `buildSkillInstanceStats` 等价（quality stats 叠加进 per-level stats），归因用 `SourceKind::GemQuality`**；`skill_source` 路径作为第二消费方同步接线（保持 CLI 路径语义一致）。蓝图 Track-1 按此写。

### 1.2 重大发现：crit/attspd/cost/reservation 全部是 .dat 列，可走 base/ 通道再生

对照 `vendor/PathOfBuilding-PoE2/src/Export/spec.lua` 与 `Export/Scripts/skills.lua:226-295` 逐行核实：

| PoB2 level 字段 | .dat 来源（spec.lua 列名） | 换算 |
|---|---|---|
| `manaMultiplier` | `GrantedEffectsPerLevel.CostMultiplier` | `CostMultiplier - 100`（=100 时省略） |
| `spiritReservationFlat` | `GrantedEffectsPerLevel.SpiritReservation` | 原值（=0 省略） |
| `reservationMultiplier` | `GrantedEffectsPerLevel.ReservationMultiplier` | `- 100`（=100 省略） |
| `storedUses` | `GrantedEffectsPerLevel.StoredUses` | 原值 |
| `attackSpeedMultiplier` | `GrantedEffectsPerLevel.AttackSpeedMultiplier` | 原值 |
| `critChance` | `GrantedEffectStatSetsPerLevel.AttackCritChance`（OffhandCritChance 覆盖） | `/ 100` |
| `baseMultiplier` | `GrantedEffectStatSetsPerLevel.BaseMultiplier`（已下载） | `/10000 + 1` |
| `manaReservationPercent` | PoE2 导出脚本中**已注释掉**（skills.lua:253-254） | **不入库** |

⇒ 18-G8「3912 个 crit_chance + 3578 个 attack_speed_multiplier 不可再生」的修复路径比审计预估更简单：**给 `pipeline/config.json` 补列即可**（adapter `RawGrantedEffectPerLevel` 的 serde 字段已声明，列一出现即解析）。`overlay/skill_overrides.json` 通道收窄为真正不在 .dat 的部分（statSet `baseMods`，如 Flicker `mod("Speed","MORE",285)`）。

### 1.3 support 类型表达式：AND/OR/NOT 是 ActiveSkillType 表的行

已核实 `pipeline/tables/English/ActiveSkillType.json`（278 行）含 `'AND'`/`'OR'`/`'NOT'` 三行。GrantedEffects 的类型列是**后缀表达式 token 流**（FK 索引序列，AND/OR/NOT 即特殊行）。现有 `GrantedEffectDef.allowed_active_skill_types: Vec<u32>` 保留了原始索引但**无消费方**（仅测试构造），可安全改为**解析后的名字 token 数组**。

pipeline 现状（`pipeline/config.json` GrantedEffects 列）：`AllowedActiveSkillTypes`（= PoB2 spec `SupportTypes`，require 语义）与 `AddedActiveSkillTypes`（= `AddTypes`）**已下载**；缺 exclude 列（PoB2 spec 列 6 `ExcludeTypes`）、`SupportGemsOnly`（列 7）、`CannotBeSupported`（列 9）。社区 schema（pathofexile-dat）对这三列的命名需在 W0 下载时验证（见 §6 开放问题 Q1）。

### 1.4 多 statSet / meta gem 的数据路径

- `additionalStatSet1/2`（Gems.lua 149 处）源自 **`GrantedEffects.AdditionalStatSets` 列**（Export/Scripts/skills.lua:914-918），列值是指向**另一行 GrantedEffects 的 Key 列表**（如 IceNova → `IceNovaPlayerOnFrostbolt`），即"附加形态 = 另一个 granted effect 的 statSet"。
- `additionalGrantedEffectId1`（162 处）源自 **`GemEffects.AdditionalGrantedEffects` 列**（spec.lua gemeffects 列 10）。
- `SkillGems.GemEffects` 列**已下载**（FK 索引，如 `[1]`），目标表 `GemEffects` 未下载。GemEffects 表 spec 列：`Id`/`Name`/`GrantedEffect`(Key)/`Tags`/`AdditionalGrantedEffects`(list Key)/`SecondarySupportName` 等。
- XML 侧 statSet 选择属性：`<Gem statSetIndex="N">`（vendor `Classes/SkillsTab.lua:354` 读、:489 写；另有 `statSetIndexCalcs`，calcs 页独立选择，M1 不做）。

### 1.5 PoB2 框架逻辑的精确参照（Rust 侧需按语义复刻的全部函数）

| 函数 | vendor 位置 | 规模 | 语义要点 |
|---|---|---|---|
| `doesTypeExpressionMatch` | Modules/CalcTools.lua:61-82 | ~20 行 | 后缀栈机：遇 OR/AND 弹一合一、NOT 取反栈顶、普通 token 压入 `skillTypes[token] or false`；最后栈内**任一**为真即匹配 |
| `canGrantedEffectSupportActiveSkill` | Modules/CalcTools.lua:84-110 | ~25 行 | 顺序：cannotBeSupported → supportGemsOnly（无 gemData 拒）→ fromItem 特例（M1 跳过）→ exclude 表达式命中拒 → isTrigger 且非玩家 actor 拒（玩家 build 恒不触发，M1 跳过）→ require 表达式（空 = 接受） |
| addSkillTypes 两遍 + 不动点 | Modules/CalcActiveSkill.lua:179-210 | ~30 行 | pass1：兼容 support 把 addSkillTypes 并入 activeSkill.skillTypes，不兼容的进被拒名单；repeat-until 重扫被拒名单直到一轮无新增；pass2：兼容名单注入 effectList。保证 support 插槽顺序无关 |
| `buildSkillInstanceStats` | Modules/CalcTools.lua:138-200 | ~60 行 | ① quality>0 时 `stats[stat] += math.modf(rate × quality)`（**截断取整 trunc，非 floor**）；② 逐 stat 按 statInterpolation 取值（1=直读/2=按 actorLevel 线性插值/3=effectiveness 插值，M1 只做 1，2/3 留 M5a）；③ constantStats 叠加 |
| `mergeSkillInstanceMods` | Modules/CalcActiveSkill.lua:82-141 | ~60 行 | statMap 查表 → 注入值 = `map.value or statValue × (map.mult or 1) × scalar / (map.div or 1) + (map.base or 0)`；**选中 set 全量 merge + baseMods**；未选 set 仅 merge **global** stat（isGlobalEffect 判 mod tag）与 global baseMods |
| support level 字段消费 | Modules/CalcActiveSkill.lua:686-700 | ~15 行 | `manaMultiplier` → `SupportManaMultiplier` MORE；`reservationMultiplier` → `ReservationMultiplier` MORE；`manaReservationPercent` → skillData；`spiritReservationFlat` → `ExtraSpirit` BASE |
| SkillStatMap 数据形态 | Data/SkillStatMap.lua（954 条）+ Data.lua:835-847（metatable 懒挂） | 数据 | 每条 = `stat_id → { mod/flag/skill 构造器列表..., div?, mult?, base?, value? }`；per-statSet 覆盖 ~390 处嵌在 Data/Skills/*.lua 的 statMap 字段 |
| quality 数据导出 | Export/Scripts/skills.lua:304-313 | 数据 | `GrantedEffectQualityStats` 按 GrantedEffect 取行，`{stat.Id, StatValues[i]/1000}`；**support 宝石不导出 qualityStats**（`if not (skillGem and granted.IsSupport)`） |

### 1.6 pobr 现状关键位置速查

| 位置 | 现状 |
|---|---|
| `pipeline/config.json` | 15 张表；GrantedEffects 8 列；无 GemEffects/GrantedEffectQualityStats/SupportGems |
| `tools/pobr-data-adapter/src/skills.rs`（~700 行） | `is_mappable_stat`（:374，入库白名单）、`adapt_stat_sets`（:442，只走主 StatSet 外键）、`RawGrantedEffectPerLevel`（:81，已声明 CritChance 等 serde 字段但列不在表里） |
| `crates/pobr-data/src/catalog/skills.rs` | `SkillGemDef`（无 effect 外键）/`GrantedEffectDef`（单 stat_set、`allowed_active_skill_types: Vec<u32>` 无消费方）/`SkillLevelDef`（无 reservation 族）/`SkillStatSetDef`（单 set、`skill_attack_speed_more` 手补字段） |
| `crates/pobr-build/src/build.rs:22` | `GemSkillRef { skill_id, gem_level }`——无 quality / 无 statSetIndex |
| `crates/pobr-build/src/xml_build.rs:791-805` | 只取 `skillId`+`level` 属性 |
| `crates/pobr-build/src/build_data.rs:298` | `effect_stats(skill_id, gem_level)`——无 quality / 无 set 选择 |
| `crates/pobr-build/src/calc_orchestrator.rs:1606` | `support_modifiers`：只查 `is_support==true` 即全量注入，**无任何兼容裁决** |
| `crates/pobr-build/src/skill_stat_map.rs`（751 行） | 后缀启发式（待删） |
| `crates/pobr-core/src/calc/skill_mechanics.rs:541` | cost 公式 doc 明确"不含 SupportManaMultiplier defer" |
| `tools/sync-pob-catalog/src/extract_lua.rs` + `extract_skill_overrides.lua` | extract-lua 机制就绪：最小 stub（SkillType/ModFlag/KeywordFlag 自映射 metatable + mod/flag/skill 构造器捕获为纯表）+ JSONL → Rust 侧 byte-stable 序列化。**抽 SkillStatMap 可直接复用此机制** |
| `examples/demo-bd-test/builds/`（18 个） | sorceress-stormweaver-comet 的 decoded.xml 含 15 个 `quality="20"` 宝石（quality fixture 既有素材） |

---

## 2. 工作项分解

### W0（预备，串行先行）：pipeline 扩列扩表 + adapter 模块拆分

**目标**：一次性把 M1 全部新增 .dat 列/表下载落盘并验证列名；把 `skills.rs`（~700 行）拆成模块目录，给后续 5 个 track 划出互不冲突的文件边界。

**涉及文件**：
- `pipeline/config.json`（独占改一次）
- `pipeline/tables/English/*.json` `pipeline/tables/Traditional Chinese/*.json`（重新下载产物）
- `tools/pobr-data-adapter/src/skills.rs` → 拆为 `tools/pobr-data-adapter/src/skills/{mod.rs, gems.rs, effects.rs, levels.rs, stat_sets.rs, quality.rs}`（纯搬迁，不改行为）

**pipeline/config.json 变更清单**（全部 M1 需求汇总到一个 commit）：

```jsonc
// 改列（既有表）
{ "name": "GrantedEffects", "columns": [ ...现有 8 列,
    "ExcludedActiveSkillTypes",   // PoB2 spec 列6 ExcludeTypes；社区 schema 名以实际下载为准（Q1）
    "CannotBeSupported",          // spec 列9
    "SupportGemsOnly",            // spec 列7
    "AdditionalStatSets" ] },     // spec 中 AdditionalStatSets（list Key → GrantedEffects）
{ "name": "GrantedEffectsPerLevel", "columns": [ ...现有,
    "CostMultiplier", "StoredUses", "SpiritReservation",
    "ReservationMultiplier", "AttackSpeedMultiplier",
    "PlayerLevelReq" ] },        // M5a 召唤物选级依赖（level_requirement），随本批一并下载
{ "name": "GrantedEffectStatSetsPerLevel", "columns": [ ...现有,
    "AttackCritChance", "OffhandCritChance" ] },
{ "name": "GrantedEffectStatSets", "columns": [ ...现有, "LabelType" ] },  // 多 set label
// 新表
{ "name": "GemEffects", "columns": ["Id","Name","GrantedEffect","Tags",
    "AdditionalGrantedEffects","SecondarySupportName"] },
{ "name": "GrantedEffectQualityStats", "columns": ["GrantedEffect","GrantedStats","StatValues"] }
// （可选，Gap10 low）SupportGems: ["SkillGem","Family"] —— 仅当顺手；不阻塞
```

**验收**：下载成功、新列非空计数报告（写进 commit message）：`AttackCritChance` 非空数应 ≈ 3912 的来源量级、`CostMultiplier ≠ 100` 行数 ≈ sup_*.lua manaMultiplier 出现量级（64+）。**若社区 schema 缺列/列名不同**：以 `npx pathofexile-dat` 实际可用 schema 为准修正列名；确实不存在的列改走 extract-lua 兜底通道（从 `Data/Gems.lua`/`Data/Skills/*.lua` 抽，见 Q1）。

**规模**：0.5–1 天。**此项完成前其他 track 不得动 pipeline/config.json。**

---

### Track-1（T1）：gem quality 四层打通（18-G1 / 15-G5）

**目标**：宝石品质从 .dat 到 DPS 全链路生效，q20 宝石不再被静默丢弃。

**工作项**：

| # | 内容 | 文件 | 参照 |
|---|---|---|---|
| T1.1 | adapter 出 `base/gem_quality_stats.json`：`effect_id → [{stat, per_quality_rate}]`；rate = `StatValues[i]/1000`；support 效果跳过（对齐导出条件） | `tools/pobr-data-adapter/src/skills/quality.rs`（新） | Export/Scripts/skills.lua:304-313 |
| T1.2 | catalog schema：`GemQualityStatDef { effect_id, stats: Vec<QualityStat { stat, per_quality_rate }> }` | `crates/pobr-data/src/catalog/skills.rs`（追加 struct，不动既有） | 20-target §3.1 gem_quality_stats.json |
| T1.3 | gamedata 懒加载域 `gem_quality_stats()`（照搬 M0 既有域模式） | `crates/pobr-gamedata/src/domains/`（新文件） | — |
| T1.4 | Build 模型：`GemSkillRef` 加 `quality: u32`（default 0）；`SocketGroup.active_gem_quality` 同步 | `crates/pobr-build/src/build.rs` | — |
| T1.5 | XML 导入：解析 `<Gem quality="N">` 属性 | `crates/pobr-build/src/xml_build.rs:791-805` | SkillsTab.lua（quality attrib） |
| T1.6 | 取数接线：`BuildData` 持有 quality 表；`effect_stats` 加 quality 参数，按 `stats[stat] += trunc(rate × quality)` 叠加（**trunc 截断，对齐 `math.modf`**），叠加项归因标记区分（见 T1.7） | `crates/pobr-build/src/build_data.rs:298` | CalcTools.lua:140-146 |
| T1.7 | 归因：quality 叠加产生的 stat 增量经 `mapped_stat_modifiers` 注入时用 `SourceKind::GemQuality`（id=`gem.<id>.q<Q>`，与 skill_source.rs:633 既有约定一致）。实现建议：`effect_stats` 返回 `(stat, value, source_kind)` 或拆 `effect_quality_stats` 单独取（避免 base/quality 混在一个值里丢归因粒度——PoBR 增量资产，20-target §1.1） | `build_data.rs` + `calc_orchestrator.rs`（skill_base_modifiers/support_modifiers/aura_buff_modifiers 三个取数点） | — |
| T1.8 | `skill_source.rs` 路径同步：CLI/Session 侧 `ingest_gem_leveled` 调用方（如有）喂 quality；本项很薄，主要是 doc 与单测对齐 | `crates/pobr-core/src/skill_source.rs`（只补测试/doc） | — |

**测试与 fixture**：
- 单测：构造 Fireball 等价 quality 表（rate=0.5），q20 → 叠加 +10（trunc 验证：rate=0.55, q19 → trunc(10.45)=10）。
- 集成：`sorceress-stormweaver-comet`（15×q20）双跑前后 DPS 变化方向断言 + oracle 对拍该 build 的 quality stat 中间值（oracle.lua 取 `buildSkillInstanceStats` 输出）。
- ninja_parity：行为 commit（quality 生效）与数据 commit（表入库，逐值不变）分开；baseline 提升后独立 commit 更新。

**规模**：2–3 天。

---

### Track-2（T2）：skill_stat_map.json 抽取 + stat_map_engine + 双跑切换（18-G3 / 15-G2，P5）

**目标**：954 条全局映射 + per-statSet 覆盖出代码入 overlay；框架只留 ~60 行 merge 引擎；双跑 diff 干净后删 `skill_stat_map.rs`（751 行）与 adapter 白名单。

#### T2.1 extract-lua 抽取 `overlay/skill_stat_map.json`

- **文件**：`tools/sync-pob-catalog/src/extract_lua.rs`（扩 `--what stat-map`）+ 新引导脚本 `extract_skill_stat_map.lua`（复用现有 stub：`SkillType/ModFlag/KeywordFlag` 自映射 + mod/flag/skill 构造器捕获——`extract_skill_overrides.lua:30-50` 已有同形实现）。
- **抽取源**：`Data/SkillStatMap.lua`（954 条全局）+ `Data/Skills/{act_str,act_dex,act_int,sup_str,sup_dex,sup_int,other}.lua` 各 statSet 的 `statMap` 字段（~390 处 per-set 覆盖；minion/spectre 留 M5a）。
- **JSON schema**（定义于 `crates/pobr-data/src/catalog/stat_map.rs`，新文件）：

```jsonc
// overlay/skill_stat_map.json
{ "_meta": { "schema": "skill_stat_map/v1", "vendor_commit": "...", "regen_command": "..." },
  "global": {
    "<stat_id>": {
      "div": 1000.0,          // 可选；value/mult/base 同
      "mods": [ {
        "kind": "mod" | "flag" | "skill_data",   // 对应 mod()/flag()/skill() 构造器
        "name": "FireMin",                        // PoB2 内部 ModName 原样保留（翻译层在框架，见 T2.2）
        "mod_type": "BASE" | "INC" | "MORE" | "FLAG" | "LIST" | "OVERRIDE",
        "flags": ["Attack", ...],                 // ModFlag token 名（stub 自映射保证是名字）
        "keyword_flags": [...],
        "tags": [ { "type": "PerStat", "stat": "ChainRemaining", ... } ]  // 原样纯表化
      } ]
    } },
  "per_stat_set": {            // per-set 覆盖边车（同文件，避免两文件 drift）
    "<granted_effect_id>": { "<set 序号或 label>": { "<stat_id>": { ...同上... } } } }
}
```

- **抽取保真原则**：tags/嵌套表**原样**纯表化落 JSON，不在抽取期做任何语义筛选——筛选/不支持判定是引擎（框架）的职责，这样 vendor 更新时 overlay drift diff 才有意义。
- **确定性**：排序（stat_id 字典序）+ 数字最短往返 + `_meta.vendor_commit`，照搬 skill_overrides/v1 模式。

#### T2.2 `rules/stat_map_engine.rs`（pobr-core，纯函数，~60 行核心 + 翻译表）

- **文件**：`crates/pobr-core/src/rules/stat_map_engine.rs`（新；`rules/` 目录若 W3 尚未建则本 track 建 `rules/mod.rs`）。
- **签名**（P9 注入风格，零 I/O）：

```rust
pub struct StatMapCatalog { /* 由 pobr-data::catalog::stat_map 反序列化聚合，含 global + per_set */ }
pub fn map_stat(
    catalog: &StatMapCatalog,
    effect_id: &str, set_key: Option<&str>,   // per-set 覆盖优先，miss 落回 global
    stat: &str, stat_value: f64,
) -> MappedOutcome   // Mapped(Vec<Modifier>) | SkillData(key,value) | Unsupported(原因) | Unknown
```

- **merge 公式**（CalcActiveSkill.lua:112 逐字对齐）：`value.unwrap_or(stat_value * mult.unwrap_or(1.) * scalar / div.unwrap_or(1.) + base.unwrap_or(0.))`。`scalar`（checkForScalarMultiplier，:53-66）依赖 mod_db 反查，M1 固定 1.0 并把含 scalar 需求的条目归 `Unsupported`（统计上报）。
- **ModName 翻译层**：PoB2 名 → PoBR 名（如 `FireMin`→`FireDamageMin`、`Speed`(Attack flag)→`AttackSpeed`、`Damage`→`Damage`）。这是**框架语义（L4，P2 判据：只随机制变不随版本变）**，以 Rust 常量表落在本文件；未知名字归 `Unknown` 上报。初版翻译表从 `skill_stat_map.rs` 现有映射反推 + 双跑 diff 驱动补全。
- **tag 求值第一批**：`无 tag`、`Condition`、`Multiplier/PerStat`（映射到 PoBR 既有 `Tag` 体系能表达的子集）。其余 tag（actor/GlobalEffect/…）条目整条归 `Unsupported`——**宁可跳过不可错算**，与 legacy「保守跳过」口径一致，保证双跑可比。
- **flag/skill_data**：`flag` → ModType::Flag；`skill_data`（PoB2 `skill("duration",…)` 类）→ M1 仅接 `duration` 等 offence 已消费的少数 key，其余 `Unsupported` 统计。

#### T2.3 双跑对照框架

- **开关**：`OrchestratorOptions` 加 `stat_map_mode: StatMapMode { Legacy, Data, Compare }`（运行时枚举而非 cargo feature——18 build 双跑在同一进程内完成，报告好做；默认 `Legacy` 保 baseline 不动）。
- **接缝**：`calc_orchestrator.rs::mapped_stat_modifiers`（:1703）内部按 mode 分发：Legacy 走 `skill_stat_map::map_skill_stats`，Data 走 `stat_map_engine::map_stat`，Compare 两边都跑、记录 diff、**输出取 Legacy**（Compare 不改变计算结果，纯观测）。
- **L1 映射级 diff**（穷举）：新增 `crates/pobr-build/tests/statmap_dual_run.rs`（`#[ignore]`，手动跑）——枚举 `granted_effect_stat_sets.json` 全量 distinct stat_id × 两引擎，分类计数：`both_equal / both_diff(值或 ModName 不同) / legacy_only / data_only / both_absent`，明细落 `target/statmap-diff/L1.jsonl` + 汇总打印。
- **L2 端到端 diff**：同测试文件，18 个 ninja build 分别以 Legacy/Data 跑 `calculate_with_data`，逐 OutputTable stat 字段 diff，**按 build 分组**输出（roadmap R5 缓解原文："双跑 + 按 ninja build 分组 diff"）`target/statmap-diff/L2-<build>.md`。
- **diff 报告形态**：markdown 汇总（分类计数表 + 按 build 的字段级偏移表 + Unsupported/Unknown 清单），人工 review 后把结论摘要登记到 `audits/rearchitecture-2026-06-10/blueprints/m1-statmap-switch-log.md`（切换审查记录，含 PoB2 依据引用）。
- **oracle 抽样**：≥50 条 stat（覆盖 div/mult/base/value/per-set 覆盖/分类型 final 各形态），`tools/pob2-oracle/oracle.lua` 跑 PoB2 `mergeSkillInstanceMods` 取注入后 modList，对拍 `stat_map_engine` 输出（名字经翻译层归一后比对 value/type/flags）。

#### T2.4 切换与删除（严格串行的收尾）

**删旧码（`skill_stat_map.rs` 751 行 + legacy 过滤）的前置条件**，全部满足才执行：

1. L1 报告 `legacy_only` 集合为空（或逐条附 PoB2 依据证明 legacy 为误映射）；
2. L2 报告 18 build 全部 review：每处输出变化要么 = 0，要么是"修对"且附 SkillStatMap.lua 条目出处；
3. oracle 抽样 ≥50 条全部一致；
4. 默认 mode 切 `Data` 的行为 commit + ninja baseline 更新 commit（独立、显式审查）已合并。

之后**纯删除 commit**：删 `skill_stat_map.rs`、删 `StatMapMode::Legacy/Compare` 分支（或保留 Compare 作长期对照工具——建议保留枚举、删 legacy 实现）、删消费侧白名单过滤（见 T5 与 §3 全量入库的搬迁不变式说明）。

**依赖**：T2.1–T2.3 可立即开工（先对现有已入库 stat 子集建立 diff 基线）；**穷举意义上的 L1 与最终切换依赖 T5.3 全量 stat 入库**（见串行序 §3）。

**规模**：4–6 天（抽取 1.5 / 引擎 1 / 双跑框架 1.5 / 切换审查与删除 1+）。

---

### Track-3（T3）：support 适用性裁决（18-G2）

**目标**：doesTypeExpressionMatch 栈机 + canGrantedEffectSupportActiveSkill + addSkillTypes 不动点全语义落地，orchestrator 注入前裁决。

**工作项**：

| # | 内容 | 文件 | 参照 |
|---|---|---|---|
| T3.1 | adapter：GrantedEffects 三个类型列解析为**名字 token 数组**（FK 索引 → ActiveSkillType.Id，保留 `"AND"/"OR"/"NOT"` 与顺序）；`CannotBeSupported`/`SupportGemsOnly` 布尔入库 | `tools/pobr-data-adapter/src/skills/effects.rs` | spec.lua grantedeffects 列 3/5/6/7/9 |
| T3.2 | schema：`GrantedEffectDef` 改造——`allowed_active_skill_types: Vec<u32>` **替换为** `require_skill_types: Vec<String>`（token 流）；新增 `add_skill_types: Vec<String>` / `exclude_skill_types: Vec<String>` / `cannot_be_supported: bool` / `support_gems_only: bool`。（旧字段无消费方，§1.3 已核实，直接替换 + regen） | `crates/pobr-data/src/catalog/skills.rs`（GrantedEffectDef 段，**T3 独占**） | 18-skills schema 建议 |
| T3.3 | 求值器：`rules/skill_type_expr.rs`——`fn matches(expr: &[String], active: &HashSet<String>) -> bool` 后缀栈机（≤30 行，含空栈防御：弹空按 false） | `crates/pobr-core/src/rules/skill_type_expr.rs`（新） | CalcTools.lua:61-82 |
| T3.4 | 裁决器：重写 `skill_source.rs::can_support` 为 PoB2 全语义 `fn can_support(effect: &SupportJudgeInput, active_types: &HashSet<String>) -> bool`（cannotBeSupported / supportGemsOnly / exclude / require 四段；fromItem 特例与 minionTypes 标注 defer M5）。位集交集旧实现删除 | `crates/pobr-core/src/skill_source.rs:379` | CalcTools.lua:84-110 |
| T3.5 | 不动点：orchestrator 组装阶段——对每个 socket group：取 active 技能 `skill_types` 为种子集合 → pass1 兼容 support 的 `add_skill_types`（注意 addSkillTypes 是普通 token 列表非表达式）并入集合、不兼容进被拒名单 → repeat-until 被拒名单一轮无新增 → 产出该组**兼容 support 名单**缓存 | `crates/pobr-build/src/calc_orchestrator.rs`（新 fn `judge_group_supports`，~50 行） | CalcActiveSkill.lua:179-210 |
| T3.6 | 接线：`support_modifiers`（:1606）/`resolve_skill_level_with_gem_bonus` 等所有按 `is_support` 全量注入处改为只注入兼容名单；被拒 support 完全不参与（数值/manaMultiplier 都不吃，对齐 PoB2 拒收） | calc_orchestrator.rs | — |

**测试与 fixture**：
- 栈机单测：用真实 token 流（从下载表挑带 AND/OR/NOT 的条目，如近战类 support 的 exclude 表达式）+ 手工构造边界（空表达式=不限制、纯 NOT、多栈残留任一真）。
- 不动点单测：构造「support A 加 Triggered 类型、support B require Triggered」，AB 与 BA 两种插槽顺序裁决结果一致。
- 集成：挑一个 ninja build 手动塞一个不兼容 support（修改后的 XML fixture），断言其倍率不进 DPS。
- ninja_parity：T3.6 是**行为 commit**（此前误注入的不兼容 support 被摘除，部分 build DPS 会降——这是修对，附 PoB2 依据逐 build 说明）；T3.1/T3.2 数据列入库是**搬迁 commit**（逐值不变，因为新列此时无消费方）。

**规模**：3–4 天。

---

### Track-4（T4）：等级表字段族 + mana_multiplier/cost/spirit 接线（18-G6 / 18-G7 / 18-G8 数据面）

**目标**：reservation/cost 全族数据列入库；SupportManaMultiplier 进 cost 公式；Spirit 预留汇总；crit/attspd 列转正为可再生。

**工作项**：

| # | 内容 | 文件 | 参照 |
|---|---|---|---|
| T4.1 | adapter：`granted_effect_levels.json` 消费新列——`mana_multiplier = CostMultiplier-100`（=100 → None）、`spirit_reservation_flat`、`reservation_multiplier = ReservationMultiplier-100`、`stored_uses`、`level_requirement = PlayerLevelReq`（M5a createMinionSkills 选级依赖，本阶段只落库不消费）；`attack_speed_multiplier`/`crit_chance` 改从表列直读（**替代 skill_overrides merge 来源**） | `tools/pobr-data-adapter/src/skills/levels.rs` + `stat_sets.rs`（AttackCritChance/OffhandCritChance → crit_chance，`/100`，Offhand 覆盖） | Export/Scripts/skills.lua:226-295；§1.2 换算表 |
| T4.2 | schema：`SkillLevelDef` 补 `mana_multiplier: Option<f64>` / `spirit_reservation_flat: Option<f64>` / `reservation_multiplier: Option<f64>` / `stored_uses: Option<u32>` / `level_requirement: Option<u32>`（全部 serde default） | `crates/pobr-data/src/catalog/skills.rs`（SkillLevelDef 段，**T4 独占**） | — |
| T4.3 | 再生一致性验收：用新列重生 `granted_effect_levels.json`，diff 现有 3912 crit + 3578 attspd 值——**逐值一致才能合并**（证明可再生通道等价于历史手补）；skill_overrides.json 边车收窄为 `skill_attack_speed_more`（baseMods 仍非 .dat），更新 `extract_skill_overrides.lua` 不再抽 crit/attspd | data/4.5.0.3.4/base + overlay/skill_overrides.json + `tools/sync-pob-catalog/src/extract_skill_overrides.lua` | 搬迁不变式 |
| T4.4 | SupportManaMultiplier 接线：`support_modifiers` 对兼容 support（依赖 T3 名单；T3 未合并前临时按现状全量）注入 `Modifier("SupportManaMultiplier", More, mana_multiplier)`（SupportGem 归因）；`skill_mechanics.rs` cost 公式乘入 `Π(1 + SupportManaMultiplier/100)`（删 :541 defer 注释） | calc_orchestrator.rs + `crates/pobr-core/src/calc/skill_mechanics.rs` | CalcActiveSkill.lua:689-691 |
| T4.5 | Spirit 预留汇总：所有启用持续型（光环/buff）效果的 `spirit_reservation_flat × Π(1+ReservationMultiplier/100)` 聚合 → OutputTable 新增 `spirit_reserved` 聚合字段（口径对照 PoB2 Reservation 段）；超载只**报告不拦截**（与 PoB2 一致：照算并标红）。**与 M2 的边界**：spirit 池本值（base_items.spirit→calc_spirit_pool）、`spirit`/`spirit_unreserved` 字段与 ReservationEfficiency 归 M2 W0.2/Track D；本项只做技能侧预留聚合。若 M1∥M2 并行，`survivability.rs`/`skill_mechanics.rs` 预留段以 M1 先合并、M2-D rebase 补 efficiency 为序 | calc_orchestrator.rs + pobr-core calc（survivability/预留段） | CalcActiveSkill.lua:692-700 |

**测试与 fixture**：
- 单测：cost = base × Π(SupportManaMultiplier)（正负倍率各一条）；spirit 汇总含 reservation_multiplier。
- oracle：挑 1 个带 cost 倍率 support 的 ninja build 对拍 mana cost 中间值。
- ninja_parity：T4.1–T4.3 搬迁 commit（逐值不变，T4.3 的 diff 校验就是证明）；T4.4/T4.5 行为 commit。

**规模**：2–3 天。

---

### Track-5（T5）：多 statSet 入库 + gem↔effect 外键 + 全量 stat 入库（18-G4 / 18-G5 / T2 数据前提）

**目标**：一个 effect 多 statSet（带 label）建模；SkillGems→GemEffects→GrantedEffects 外键链打通；删 `is_mappable_stat` 白名单全量入库（statmap 数据前提）。

**工作项**：

| # | 内容 | 文件 | 参照 |
|---|---|---|---|
| T5.1 | adapter：GemEffects 表解析；`SkillGemDef` 补 `granted_effect_id: Option<String>` / `additional_granted_effect_ids: Vec<String>`（来自 GemEffects.AdditionalGrantedEffects） | `tools/pobr-data-adapter/src/skills/gems.rs` | Export/Scripts/skills.lua:898-925；spec gemeffects |
| T5.2 | adapter + schema：`granted_effect_stat_sets.json` 改多 set——`SkillStatSetDef { effect_id, sets: Vec<StatSetDef { set_id, label, constant_stats, levels, skill_attack_speed_more }> }`；主 set = GrantedEffects.StatSet，附加 set = GrantedEffects.AdditionalStatSets 指向的 effect 的主 set（IceNova → IceNovaPlayerOnFrostbolt 行已核实在已下载表中）；`GrantedEffectDef` 补 `additional_stat_set_ids: Vec<String>`（字段由 T3 owner 协调加入，见 §3 文件归属） | `tools/pobr-data-adapter/src/skills/stat_sets.rs` + catalog/skills.rs（SkillStatSetDef 段，**T5 独占**） | 18-G4；Gems.lua:11-12 |
| T5.3 | **全量 stat 入库**：删 `is_mappable_stat` 白名单（adapter 端不再过滤任何 stat）。**搬迁不变式保障**：同 commit 在 `pobr-build` 消费侧加等价过滤（`mapped_stat_modifiers` 入口处套用从 adapter 平移过来的同一谓词，置于 legacy 路径），保证 ninja parity 逐值不变；该消费侧过滤随 T2.4 删 legacy 时一起删除 | skills/stat_sets.rs + calc_orchestrator.rs + data regen | 15-G2 修复方向 |
| T5.4 | Build 模型 + XML：`GemSkillRef` 补 `stat_set_index: Option<u32>`；xml_build 解析 `<Gem statSetIndex>`（`statSetIndexCalcs` 忽略，M1 不做 calcs 页独立选择） | build.rs + xml_build.rs | SkillsTab.lua:354/489 |
| T5.5 | 消费：`effect_stats(skill_id, gem_level, quality, set_index)`——选中 set（缺省主 set）全量取数；**未选 set 的 global-only merge 本 track 不做**（保守跳过，PoB2 语义依赖 statmap mod 的 GlobalEffect tag，T2 数据落地后由 W-J 联合项补，见 §3） | build_data.rs | CalcActiveSkill.lua:124-140 |
| T5.6 | meta gem 展开（18-G5，medium）：orchestrator 把 `additional_granted_effect_ids` 展开为同组附加技能参与（resolve_main_skill 现有"跳 meta 壳"逻辑改为按外键正向解析）。若时间紧可降级为只入库不消费（数据先行） | calc_orchestrator.rs:668 | CalcSetup.lua:1716 |

**测试与 fixture**：
- 入库断言：IceNova effect 含 ≥2 个附加 set 且 label 非空；任一 support（如 Pinpoint Critical）的曾被过滤 stat 出现在 JSON。
- XML round-trip：`statSetIndex="2"` 解析 → 选中第 2 set 的 stats 进入计算。
- ninja_parity：T5.2/T5.3 是搬迁 commit（多 set 入库但缺省仍选主 set + 消费侧过滤兜底 ⇒ 逐值不变）；T5.4/T5.5 行为 commit（18 build 的 decoded.xml 若含 statSetIndex 会改变取数——先 grep 样本统计命中数写入 commit message）。

**规模**：3–4 天。

---

### W-J（联合收尾，串行末段）：未选 set 的 global-only merge + 阶段验收

- 依赖 T2（statmap tags 含 GlobalEffect 信息）+ T5（多 set 模型）。实现 `isGlobalEffect` 等价（CalcActiveSkill.lua:68-80：mod tag 含 global 语义判定），`effect_stats`/merge 路径对未选 set 仅注入 global mod。
- 阶段整体验收（§5）。
- **规模**：1–2 天。

---

## 3. 并行 track 切分与文件归属

### 3.1 执行顺序

```
W0（pipeline 扩列 + adapter 拆模块，串行，0.5–1d）
   ├─→ T1 quality（独立，2–3d）          ┐
   ├─→ T3 support 裁决（独立，3–4d）      ├─ 三者并行
   ├─→ T4 等级字段族（独立，2–3d）        ┘
   ├─→ T2.1–T2.3 statmap 抽取/引擎/双跑框架（与上并行开发，4d）
   └─→ T5 多 set + 全量入库（3–4d；T5.2 的 GrantedEffectDef 字段需与 T3.2 协调，建议 T3 先合并）
T5.3 合并后 → T2 穷举 L1 + L2 终版 diff → T2.4 切换与删除（严格串行）
T2 + T5 合并后 → W-J global-only merge → 阶段验收
```

必须串行的先后序（违反即返工）：
1. **W0 → 一切**（pipeline 表就位 + skills.rs 拆分后才有干净文件边界）；
2. **T3.2 → T5.2**（GrantedEffectDef 两家都改：T3 是 owner，T5 的 `additional_stat_set_ids` 在 T3 合并后追加）；
3. **T5.3 → T2.4**（全量 stat 入库是穷举双跑与删旧码的数据前提）；
4. **T3.6 → T4.4 终态**（SupportManaMultiplier 只对兼容 support 注入；T4 先行合并时按现状全量注入并留 TODO，T3 合并后一行改为兼容名单）；
5. **(T2 ∧ T5) → W-J**。

### 3.2 文件归属表（每文件唯一写 owner；其他 track 需要改动时走 owner 协调或等其合并后 rebase）

| 文件 | Owner | 其他 track 的接触方式 |
|---|---|---|
| `pipeline/config.json` + `pipeline/tables/**` | **W0**（之后冻结） | 任何后续需求重开一个 W0 式独占 commit |
| `tools/pobr-data-adapter/src/skills/gems.rs` | T5 | — |
| `tools/pobr-data-adapter/src/skills/effects.rs` | T3 | — |
| `tools/pobr-data-adapter/src/skills/levels.rs` | T4 | — |
| `tools/pobr-data-adapter/src/skills/stat_sets.rs` | T5 | T4 的 crit_chance 改读 stat-set 列：函数级切块，T4 写 `crit_from_statset_levels` 独立函数，T5 调用点合并时对齐 |
| `tools/pobr-data-adapter/src/skills/quality.rs` | T1 | — |
| `crates/pobr-data/src/catalog/skills.rs` | 按 struct 分段：`SkillGemDef`→T5、`GrantedEffectDef`→**T3**、`SkillLevelDef`→T4、`SkillStatSetDef`→T5、新 `GemQualityStatDef`→T1 | 各段 serde default 字段追加互不冲突；同段双改走 owner |
| `crates/pobr-data/src/catalog/stat_map.rs`（新） | T2 | — |
| `crates/pobr-gamedata/src/domains/`（各新域文件） | 各 track 一文件一域（quality→T1、stat_map→T2） | 新文件零冲突；`lib.rs`/`manifest` 注册行冲突小，rebase 解决 |
| `crates/pobr-core/src/rules/{skill_type_expr.rs}` | T3 | — |
| `crates/pobr-core/src/rules/{stat_map_engine.rs}` | T2 | `rules/mod.rs` 先到先建 |
| `crates/pobr-core/src/skill_source.rs` | T3（can_support 重写） | T1 只动 doc/测试段 |
| `crates/pobr-core/src/calc/skill_mechanics.rs` | T4 | — |
| `crates/pobr-build/src/build.rs`（GemSkillRef） | **T1 先行**（quality 字段）；T5 在 T1 合并后追加 stat_set_index | 字段追加，序无强约束但约定 T1 先 |
| `crates/pobr-build/src/xml_build.rs` | T1 先行（quality 属性）；T5 追加 statSetIndex | 同上 |
| `crates/pobr-build/src/build_data.rs`（effect_stats） | **T1 先行**（quality 参数）；T5 追加 set_index 参数 | 签名两次演进，见 §3.3 契约 |
| `crates/pobr-build/src/calc_orchestrator.rs` | 函数级分割：`mapped_stat_modifiers`→T2、`support_modifiers`→T3（T4 在其内加 manaMultiplier 行，T3 合并后 rebase）、`judge_group_supports`(新)→T3、quality 取数点→T1、`resolve_main_skill`/meta 展开→T5、Spirit 汇总(新 fn)→T4 | 大热点：**约定只在自有函数内改**，公共调用点（`calculate_with_data` 主流程）每 track ≤3 行接线，冲突 rebase 自解 |
| `crates/pobr-build/src/skill_stat_map.rs` | T2（最终删除） | 冻结：M1 期间任何人不得再往启发式加映射 |
| `crates/pobr-build/tests/statmap_dual_run.rs`（新） | T2 | — |
| `tools/sync-pob-catalog/src/extract_lua.rs` + 新 lua 脚本 | T2（stat-map 抽取）；T4 改 `extract_skill_overrides.lua`（收窄 crit/attspd） | 两个不同 `--what`，文件不同 |
| `crates/pobr-build/tests/ninja_parity.rs`（baseline 常量） | 任何行为 commit 的 baseline 更新都是**独立 commit** | 多 track 同时提升时按合并序逐个 bump |
| `data/4.5.0.3.4/**`（regen 产物） | 每个含 regen 的 PR 独占 regen（merge 前重跑 `devs/scripts/regen-check.sh`） | 禁手改（M0 铁律） |

### 3.3 track 间接口契约（先定后动，变更需双方确认）

```rust
// C1（T1→T5→W-J 渐进演进，每步向后兼容加参）：
BuildData::effect_stats(&self, skill_id: &str, gem_level: u32) -> Vec<SkillDamageStat>            // 现状
  → effect_stats(&self, skill_id, gem_level, quality: u32) -> EffectStats                          // T1 后
  → effect_stats(&self, skill_id, gem_level, quality, set_index: Option<u32>) -> EffectStats       // T5 后
// EffectStats 区分 base/quality 两段（归因粒度），各段 Vec<SkillDamageStat>

// C2（T3 产出，T3.6/T4.4 消费）：
fn judge_group_supports(group: &SocketGroup, data: &BuildData) -> GroupSupportJudgement
// { compatible: Vec<usize/*gem_skills 下标*/>, final_skill_types: HashSet<String> }

// C3（T2 产出）：
enum StatMapMode { Legacy, Data, Compare }   // OrchestratorOptions 字段，默认 Legacy
fn stat_map_engine::map_stat(&StatMapCatalog, effect_id, set_key, stat, value) -> MappedOutcome

// C4（T1/T5 共享 Build 模型）：
struct GemSkillRef { skill_id: String, gem_level: u32, quality: u32 /*T1*/, stat_set_index: Option<u32> /*T5*/ }

// C5（W0 产出，全员消费）：adapter 模块边界 = gems/effects/levels/stat_sets/quality 五文件，
//     mod.rs 只做编排与共享 Raw 类型。
```

---

## 4. 每 track 局部门禁

所有 track 通用：`cargo test --workspace` + `clippy -D warnings` + `fmt --check`；含 regen 的 commit 跑 `devs/scripts/regen-check.sh`（byte-diff 零）；搬迁 commit 与行为 commit 分离、baseline 更新独立 commit。

| Track | 局部门禁（merge 进集成分支前必须绿） |
|---|---|
| W0 | 下载产物落盘 + 新列非空计数报告进 commit message；adapter 拆分后全量重生 byte-diff 零 |
| T1 | trunc 语义单测；q20 fixture（stormweaver-comet）oracle 对拍；行为 commit 后 ninja 进攻命中不降 |
| T2 | 引擎单测（merge 公式四参全覆盖）；L1/L2 diff 报告生成可重复；切换四前置条件（§T2.4）核对单 |
| T3 | 栈机/不动点单测；不兼容 support 拒收集成测试；行为 commit 附逐 build PoB2 依据 |
| T4 | T4.3 再生 diff 逐值一致（3912+3578 值）；cost 公式 oracle 对拍 |
| T5 | IceNova 多 set 入库断言；T5.3 消费侧过滤兜底下 ninja 逐值不变；statSetIndex round-trip |
| W-J | global-only 语义单测（对照 CalcActiveSkill.lua:124-140 构造双 set 用例） |

**阶段整体验收**（= roadmap M1 验收原文）：
1. ninja_parity 进攻 **≥40%@5%**（即 OFF_HIT5 ≥ 36/90 量级，以实际 DPS 比较项总数换算），防御 ≥ 现 baseline（111/117）不降；
2. quality-20 宝石 fixture 进 CI（常跑，非 ignore）；
3. statmap 双跑 diff 报告干净 + 切换日志（m1-statmap-switch-log.md）归档；
4. oracle 对拍 statmap 抽样 ≥50 条全过；
5. `skill_stat_map.rs` 与 `is_mappable_stat`（含消费侧兜底过滤）已删除，`grep -r is_mappable_stat` 零命中。

---

## 5. 风险与回退（roadmap R# 落点）

| 风险 | 落点 | 缓解 / 回退 |
|---|---|---|
| **R5 前置：statmap 切换与隐藏补偿耦合倒退**（roadmap M1 风险原文："双跑 + 按 ninja build 分组 diff"） | T2.4 切换 commit | Compare 模式纯观测不改输出；L2 按 build 分组定位补偿点；切换后若 ninja 倒退，**回退 = OrchestratorOptions 默认值改回 Legacy 一行**（删旧码前 Legacy 始终在）；删旧码后回退 = revert 删除 commit |
| **GemEffects/Excluded 列外键质量未知**（roadmap 原文："adapter 端外键完整性校验报表"） | W0 / T3.1 / T5.1 | adapter 加外键完整性校验：悬空 FK 计数进 stderr 报表，>0 时列入 commit message；社区 schema 缺列时降级 extract-lua 兜底（Q1） |
| 全量 stat 入库（T5.3）意外改变 legacy 行为 | T5.3 | 消费侧平移同一谓词兜底（§T5.3），commit 内 ninja 逐值校验；谓词平移用同一函数体复制 + 双处单测锁定 |
| T3 行为 commit 摘除误注入 support 导致进攻命中**下降**（修对但 baseline 倒退） | T3.6 | 逐 build 列出被拒 support 与 PoB2 依据；若个别 build 因"过算抵消欠算"而掉出容差，记录到补偿清单（与 R5 同一报告机制），**不回滚正确行为**，在阶段验收口径中说明 |
| quality Alt 列（AltStats/AltApplyToStatSets/StatSetIndex）语义未实现造成个别宝石品质错算 | T1.1 | 第一版只取 GrantedStats/StatValues 主列（与 PoB2 导出行为一致——其也只读这两列）；Alt 列原样入库不消费，标 TODO |
| `calc_orchestrator.rs` 多 track 热点冲突 | 全部 | §3.2 函数级归属 + 主流程每 track ≤3 行接线约定；集成分支每日 rebase |
| extract-lua 抽 SkillStatMap 的 tag 纯表化失真（闭包/函数值条目） | T2.1 | 抽取器遇函数值字段记 `"_unextractable": true` 原样上报，引擎归 Unsupported；oracle 抽样兜底正确性 |

---

## 6. 开放问题（实施前需裁决/验证）

| # | 问题 | 建议默认 |
|---|---|---|
| Q1 | 社区 schema（pathofexile-dat）中 GrantedEffects 的 exclude/cannotBeSupported/supportGemsOnly/AdditionalStatSets 列与 GemEffects/GrantedEffectQualityStats 表的确切命名是否可用——**W0 第一天即验证**；缺列时走 extract-lua 从 `Data/Gems.lua`/`Data/Skills/*.lua` 抽（数据等价，但通道从 base/ 变 overlay/，需在 manifest 域归属上确认） | 先试 dat 列；缺则 overlay 兜底 |
| Q2 | ModName 翻译层（PoB2 名→PoBR 名常量表）放 Rust 框架是否符合 P2 判据——本蓝图判 **是**（名字随机制不随版本），但属于"数据带逻辑拍平"的边界条目，建议架构 owner 复核一句话确认 | 留框架（L4） |
| Q3 | 未选 statSet 的 global-only merge 推迟到 W-J（T2+T5 之后）期间，多 set 技能未选 set 的 global mod 暂缺——对 18 build 影响面需在 T5.4 行为 commit 时实测统计；若命中显著可把 W-J 提前 | 按序执行 |
| Q4 | `StatMapMode::Compare` 是否长期保留为对照工具（建议保留——后续 M3 config_interpreter / M6 parser 双跑可复用同模式） | 保留枚举与报告框架 |

---

## 附：规模汇总

| 项 | 预估 |
|---|---|
| W0 | 0.5–1 天 |
| T1 quality | 2–3 天 |
| T2 statmap | 4–6 天 |
| T3 support 裁决 | 3–4 天 |
| T4 等级字段族 | 2–3 天 |
| T5 多 set/外键/全量入库 | 3–4 天 |
| W-J + 阶段验收 | 1–2 天 |
| 合计 | ~16–23 人天（5 agent 并行下日历 ~1.5–2 周），与 roadmap ~3 人周一致 |
