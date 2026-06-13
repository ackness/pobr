# M5a 实施蓝图 — 召唤物链路（minions / spectres / createMinionSkills / mirage / parity 扩集）

> 阶段：roadmap M5(a) ｜ 对应缺口：14-G3（Minions/Spectres 未 JSON 化）、14-G4（召唤物未接 build 链路）、14-G5（createMinionSkills 缺失）、14-#9（minion modDB 装配缺注入项，medium）、14-#7（CalcMirages 幻影域，medium）
> 撰写：2026-06-11 ｜ 本文自包含：实施 agent 只读本蓝图 + 代码即可开工，无需回读 roadmap/审计。
> 体量：M5 总 ~6 人周三线并行，本蓝图为其中 (a) 线，估 ~2 人周，内部再切 5 个 track。

---

## 0. 前置状态与全局纪律

### 0.1 已交付（M0，可直接依赖）

- 三层数据目录 `data/4.5.0.3.4/{base/, overlay/, generated/}` + `manifest.json` v2（三段 domains）。
- overlay merge 引擎（`crates/pobr-gamedata/src/overlay.rs`）+ RuleSet 聚合入口骨架（`crates/pobr-gamedata/src/ruleset.rs`，W3 正在把九张常量表接入 `load_ruleset`，**本蓝图按"注入管道已存在"假设写**：calc 侧常量经 `&GameConstants`/catalog 类型注入，pobr-core 保持零 I/O）。
- handler 注册表骨架（`crates/pobr-core/src/rules/registry.rs`）。
- `sync-pob-catalog extract-lua` 子命令（`tools/sync-pob-catalog/src/extract_lua.rs` + 内嵌引导脚本 `extract_skill_overrides.lua`）：luajit 在最小 stub 环境执行 vendor Lua，JSONL 输出，Rust 侧排序 + byte-stable 序列化，`_meta` 头记 vendor commit 与 regen 命令。**这是本阶段数据入库的主通道，照此模式扩展。**
- 九张常量表已落 `base/`（含 `monster_scaling.json`：life/damage/armour/evasion/accuracy/ailment_threshold/poise_threshold/**ally_life/ally_damage**——召唤物基线两张表已在库）。
- CI：`devs/scripts/regen-check.sh`（重跑管线 byte-diff 零）+ pobr-data 禁内嵌大数组 lint。
- vendor 检出：`vendor/PathOfBuilding-PoE2/src/` 当前 **Data/、Modules/、Export/ 均在**（含 CalcMirages.lua / CalcActiveSkill.lua / CalcPerform.lua / Data/Minions.lua / Data/Spectres.lua / Export/Scripts/skills.lua），无需 gh 远程取文件。

### 0.2 pobr 召唤物现状（greenfield 半成品，本阶段的起点）

| 位置 | 状态 |
|---|---|
| `crates/pobr-data/src/minion.rs`（24.5K） | `MinionDef` schema 已就绪（字段对齐 Minions.lua）+ **4 条手抄常量构造函数**（`minion_def_zombie` :261 / `minion_def_raging_spirit` :296 / `minion_def_skeletal_warrior` :331 / `minion_def_skeletal_storm_mage` :366）+ Spectre 占位 :405。**未挂 catalog/manifest/loader/adapter 任何一处** |
| `crates/pobr-core/src/calc/minion.rs`（617 行） | 纯函数就绪：`derive_minion_base_stats`（等级表×归一化乘数、虚拟武器、爆伤 30+70、CannotBeEvaded）、三通道注入（`MinionModifierEntry` / 盟友 buff / `AttributeInfusion`）、`build_minion_context_from_def` :427、`write_summoned_minion_multipliers`。**注意 :39 `MINION_LEVEL_TABLE` 硬编码 const、`derive_minion_base_stats` 读 `pobr_data::monster::MonsterScalingRow::at_level`（嵌入表，W3 正切注入）** |
| `crates/pobr-core/src/calc/env.rs` | `add_minion` :33 / `add_minion_from_def` :48 API 就绪，**仅测试可达** |
| `crates/pobr-core/src/calc/perform.rs:90` | `perform_minions`：env.minions 跑 `calculate_minimal_vs_enemy` + `calc_defence` 出 `MinionOutput` 快照（含跨 actor trace 边、`Multiplier:SummonedMinion`/`MinionPresenceCount` 注入） |
| `crates/pobr-build/` | **grep minion 零命中**：orchestrator 不识别召唤物宝石、XML 导入不读 `skillMinion` 属性、召唤物面板恒空 |
| `MinionDef.skill_list` | 存而不用（calc 侧零读取）——法术召唤物 DPS 无来源 |
| mod_parser | 不产 MinionModifier 包裹（`Minions deal X% increased Damage` 进不了召唤物 ModDb） |
| mirage | 全无（仅 `pob2_parity.rs:137` / `skill_stat_map.rs:394` 注释提及） |

### 0.3 全局纪律（roadmap §0 原文要点，每个 track 适用）

1. 门禁三件套：`cargo test --workspace` + `clippy -D warnings` + `fmt --check` 全绿；**ninja_parity 18-build 零回归**（以合并时 master 上已记录的 baseline 为底线不得倒退；M0 时点为防御 51% / 进攻 24% @5% 容差，M1–M4 合入后以届时 baseline 为准）；涉及解析/数据的改动加 pob2-oracle 对拍或 generated 重生一致性校验。
2. **搬迁不变式**：纯搬迁（数据出代码、入 JSON）的 commit，parity baseline **逐值不变**（golden diff = 0）；搬迁与行为改动永远分两个 commit。
3. 行为修复必须附 PoB2 一手依据（源码行号 / oracle 中间值）；baseline 更新独立 commit、显式审查。
4. `data/<版本>/` 任何文件禁手改；overlay 产物只许工具再生，`_meta` 头记 vendor commit；新表注入路径携带独立 `SourceKind` 的 `SourceId`（归因粒度随数据化增强）。

---

## 1. 核心取舍：minions.json / spectres.json 的入库路线

### 1.1 两条路线

**路线 A（roadmap §3.1 终局形态）：pipeline 补 .dat 表 + adapter 反范式化 → `base/`**

- pipeline `config.json` 补下载 `MonsterVarieties`、`DefaultMonsterStats`、`MonsterResistances`、`MonsterTypes`、`GrantedEffects` 关联（minion 技能 join）等表；`tools/pobr-data-adapter` 复刻 PoB2 `Export/Scripts/minions.lua` 的反范式化逻辑。
- 关键困难：**Minions.lua 里的归一化乘数（`life = 0.7`、`damage = 0.75`）不是 .dat 现成列**，是 PoB2 导出器拿 MonsterVarieties 数值除以 DefaultMonsterStats 基准算出来的派生量；`monsterTags`/`skillList`/`modList`（含 `mod(...)` 构造）也要走完整的 export 模板逻辑。等于把 PoB2 的 minions 导出器在 Rust 重写一遍，工作量大、且任何口径偏差直接破坏 parity（基准恰恰是 PoB2 的产物）。

**路线 B（本蓝图采纳，M5a 执行）：extract-lua 执行 vendor `Data/Minions.lua` / `Data/Spectres.lua` → `overlay/`**

- 两份文件头部自注 "automatically generated, do not edit"——它们**本身就是 .dat 的确定性投影**，是 PoB2 计算引擎的实际输入，即 parity oracle 的事实数据源。
- 文件入参形如 `local minions, mod = ...`（Spectres 为 `local minions, mod, flag = ...`），用 M0 已验证的 extract-lua 模式（luajit + stub 构造器 + JSONL）执行即可逐字段忠实抽取，零反范式化逻辑。
- 符合 P13 裁决（luajit 执行而非正则啃源码）与 R3 缓解（CI drift diff + oracle 终裁）。

### 1.2 裁决

**M5a 走路线 B**，理由按优先级：① parity 门禁以 PoB2 输出为基准，用 PoB2 自己的数据文件保证数值逐字段一致，消除"反范式化口径偏差"这一整类风险；② extract-lua 通道 M0 已经打通且 byte-stable，边际成本最小；③ 路线 A 的全部价值（脱离 vendor、纯 .dat 再生）由 version-bump-drill（P18）守门——若未来演练发现 vendor 更新不及时/漂移，再触发路线 A 迁移，迁移时两路产物 diff=0 即可无感切换。

**物理归宿**：按 P1 的生产工具定层原则（base = pipeline+adapter 产出；overlay = extract-lua 产出），落 `overlay/minions.json` + `overlay/spectres.json`。注意 20-target-architecture §3.1 表格把这两张表列在 base/（按"逻辑上属 L1"分类）——本蓝图按"物理层跟生产工具走"执行，**已列入 §6 开放问题**，若总裁决倾向 base/ 则只改落盘路径与 manifest 段位，工作项不变。

### 1.3 granted_effects.minion_list 外键的依赖检查（必做的第一步）

roadmap 附 A 写"minion_list 外键 M1 入库"，但**截至本蓝图撰写（M0 收尾时点）该列不存在**，且来源有一个关键事实：

- `minionList` **不在 .dat**。它来自 PoB2 Export 模板的手工指令 `#minionList <minion>...`（`vendor/.../src/Export/Scripts/skills.lua:771-776` directiveTable.minionList；`Export/Skills/act_dex.txt:431` 等），属 L2 策展数据——与 M0 已建的 `skill_overrides.json` 通道（承接 Export 指令类手补）同族。
- 现状核实：`crates/pobr-data/src/catalog/skills.rs::GrantedEffectDef` 无 `minion_list` 字段；`data/.../base/granted_effects.json` 含 MinionMeleeStep 等 minion granted effect 条目（.dat GrantedEffects 全量导出，**召唤物自身技能的 levels/stat_sets 数据已在库**），但无宝石→召唤物的连边。

**归属定案（总架构评审 2026-06-11）**：M1 蓝图已明确**不含** `minion_list`（roadmap 附 A 的「M1 入库」注记作废，见 M1 蓝图 §0 范围澄清）——**A3 必做**，用 extract-lua 抽 `Data/Skills/*.lua` 的 `minionList`/`addMinionList` 落 `overlay/granted_effect_minions.json` 边车，gamedata merge 阶段拼进 `GrantedEffectDef`；A0 的检查保留为防御性核验。`granted_effect_levels.level_requirement` 列已划归 **M1 T4 落库**（PlayerLevelReq，评审修订后 M1 蓝图已含）——A0 仍核验存在性，A4 的附带兜底项预计可免。

---

## 2. 工作项分解

### Track A — 数据入库管线（minions / spectres / 外键边车 / 常量补表）

#### A0 依赖检查与 schema 冻结（串行起点，~0.5 天）

- **目标**：核实 M1–M4 合入后的真实状态，冻结本阶段 schema 契约。
- **检查清单**：
  1. `GrantedEffectDef` 是否已有 `minion_list`（无 → A3 全做）；
  2. `SkillLevelDef` 是否已有 `level_requirement`（无 → A4 补 pipeline 列）；
  3. `ruleset.rs::GameConstants` 是否已是真实 catalog 类型（W3 产物），minion 侧常量走同一注入面；
  4. `xml_build.rs` 是否仍无 `skillMinion` 解析（无 → B1 全做）。
- **产出**：在本文件追加一节「A0 检查结果」（蓝图文档是唯一允许写的例外），列每项的实际状态与对应工作项的增删。

#### A1 MinionDef schema v2 迁入 catalog（~1 天）

- **目标**：`MinionDef` 从 `pobr-data/src/minion.rs` 迁入 `catalog/actors.rs`（20-doc §2.1 目标布局），扩齐 vendor 字段，挂 `DataManifest`。
- **涉及文件**：`crates/pobr-data/src/catalog/actors.rs`（新建）、`catalog/mod.rs`（导出）、`catalog/manifest.rs`（domains 注册 `minions`/`spectres`/`granted_effect_minions`/`mirage_configs`）、`crates/pobr-data/src/minion.rs`（类型移走后保留 re-export 过渡，4 条手抄构造函数暂留供 A6 双跑）。
- **vendor 参照**：`Data/Minions.lua`（32 条）/ `Data/Spectres.lua`（593 条，同 schema；key 为完整 metadata 路径如 `Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre`）。
- **schema 增量**（全部 `#[serde(default)]`，R7 纪律）：

  ```text
  MinionDef 现有字段保持 +
    attack_range: f64            // attackRange
    accuracy: f64                // accuracy（默认 1）
    base_movement_speed: f64     // baseMovementSpeed
    weapon_type1: Option<String> // weaponType1（虚拟武器类型，ModFlags 派生用）
    weapon_type2: Option<String>
    spawn_location: Vec<String>  // spectre 专用，召唤物为空
    mod_list: Vec<MinionModDef>  // ← 新增结构化类型，见下
  MinionModDef {                 // 对应 Lua mod("StunDuration","OVERRIDE",3,0,0) / flag(...)
    name: String, mod_type: String, value: serde_json::Value,
    flags: u64, kw_flags: u64,
    tags: Vec<serde_json::Value>,   // 罕见，先原样保留
    comment: Option<String>,        // Lua 行尾 stat 注释，备查
  }
  ```

- **测试**：serde 往返 + 4 条手抄常量逐字段等于 schema v2 构造（搬迁不变式的类型层）。
- **规模**：~400 行（schema + 文档注释 + 测试）。

#### A2 extract-lua 抽 Minions.lua / Spectres.lua → overlay（~2 天）

- **目标**：`overlay/minions.json`（32 条）+ `overlay/spectres.json`（593 条），byte-stable、`_meta` 头齐全、regen-check 接入。
- **涉及文件**：`tools/sync-pob-catalog/src/extract_lua.rs`（扩子命令分支 `--kind minions|spectres`）、`tools/sync-pob-catalog/src/extract_minions.lua`（新建引导脚本：提供 `minions` 空表 + `mod(...)`/`flag(...)` 记录型 stub——返回携带全部入参的 table 而非求值，`dofile` 后逐条 JSONL 输出）、`tools/sync-pob-catalog/src/main.rs`（CLI 接线）、`data/4.5.0.3.4/overlay/minions.json`、`overlay/spectres.json`、`data/4.5.0.3.4/manifest.json`（overlay 域登记）、`devs/scripts/regen-check.sh`（追加两条 regen 命令）。
- **vendor 参照**：`Data/Minions.lua` 头部 `local minions, mod = ...`；`Data/Spectres.lua` 头部 `local minions, mod, flag = ...`（stub 需同时提供 `flag`）；`modList` 内有真实 `mod(...)` 调用（如 Spectre 的 `mod("StunDuration", "OVERRIDE", 3, 0, 0)`）与纯注释行——stub 必须把 mod 构造忠实序列化为 `MinionModDef`，注释行丢弃（vendor 注释非数据）。
- **数字格式**：沿用 extract_lua.rs 现行「Rust 侧统一最短往返表示」约定，保证重跑 byte-diff 零。
- **测试**：
  1. 单测：抽取产物中 `RaisedZombie`/`SummonedRagingSpirit`/`RaisedSkeletonWarriors`/`RaisedSkeletonStormMage` 四条逐字段等于 `pobr_data::minion` 手抄常量（**这是搬迁不变式的逐值校验**，发现 diff 以 vendor 为准并记录）；
  2. 条目计数断言（32 / 593）；
  3. regen-check 全量重跑 byte-diff 零。
- **规模**：Lua 引导 ~120 行 + Rust ~250 行 + 产物 JSON。

#### A3 granted_effect → minion_list 外键边车（~1 天）

- **目标**：`overlay/granted_effect_minions.json`：`{effect_id → {minion_list: [String], add_minion_list: [String], minion_uses: [String]?, minion_has_item_set: bool?}}`，gamedata merge 进 `GrantedEffectDef`。
- **涉及文件**：`tools/sync-pob-catalog/src/extract_skill_overrides.lua`（扩输出字段：执行 `Data/Skills/*.lua` 时每个 skill 条目已在 stub 环境内成表，新增提取 `minionList`、support 的 `addMinionList`、`minionUses`、`minionHasItemSet`、skillData 内 `minionLevel`/`minionLevelIsPlayerLevel`/`minionLevelIsEnemyLevel`/`minionDamageEffectiveness`）、`extract_lua.rs`（新输出文档类型）、`catalog/skills.rs`（`GrantedEffectDef` 补 `#[serde(default)] minion_list: Vec<String>` 等字段——**merge 后内存形态**，base JSON 不变）、`crates/pobr-gamedata/src/overlay.rs` 或新 `domains/granted_effect_minions.rs`（merge 接线）。
- **vendor 参照**：`Export/Scripts/skills.lua:207-211`（minionList 写出格式）、`Data/Skills/minion.lua`（被引技能定义所在）、`Data/Skills/act_*.lua`（召唤宝石条目，如 act_int.lua 的 Raise Zombie 含 `minionList = { "RaisedZombie" }`）。抽取范围：`act_dex, act_int, act_str, minion, sup_*`（support 的 addMinionList）——按现有 `DEFAULT_SKILL_FILES` 扩列。
- **测试**：Raise Zombie → `["RaisedZombie"]`、Skeletal Storm Mage 召唤技 → 对应骷髅条目等 ≥5 条断言；merge 后 `BuildData` 查询接口单测。
- **规模**：~300 行。

#### A4 minion 侧常量/等级表补库（~1 天）

- **目标**：把召唤物链路仍硬编码的常量迁入数据并走注入面（搬迁不变式：本项为纯搬迁 commit）。
- **清单**：

  | 数据 | vendor 出处 | 现 pobr 位置 | 目标 |
  |---|---|---|---|
  | `minionLevelTable`（40 项宝石等级→怪物等级） | `Data/Misc.lua:16` | `calc/minion.rs:39 MINION_LEVEL_TABLE` const | `base/monster_scaling.json` 扩 `minion_level` 段（或 game_constants monster 段，与 W3 落点对齐） |
  | `mapLevelLifeMult`（66+ 区域生命乘数，hostile minion 用） | `Data/Misc.lua:327` | 无 | 同上 |
  | `SpectreBeastDamageFixup = 1.25` | `Data/Misc.lua`（misc 常量） | 无（hiddenDamageFixup 派生输入） | `base/game_constants.json` |
  | `base_critical_hit_damage_bonus`（怪物 30）+ playerMinionIntrinsicStats（70） | `Modules/CalcPerform.lua:1007` 引 monsterConstants | `calc/minion.rs` const 30/70 | `base/game_constants.json` monster 段（确认 W3 是否已迁，已迁则只切消费点） |
  | HeavyStunBuildup 两常量（`physical_hit_damage_stun_multiplier_+%_final_from_ot` 等） | `CalcPerform.lua:1013-1014` 引 monsterConstants | 无 | 同上 |

- **涉及文件**：`crates/pobr-data/src/catalog/monster_scaling.rs` 与/或 `catalog/game_constants.rs`（schema 扩段）、`tools/pobr-data-adapter` 或 extract-lua（按 W3 已为九表选定的生产通道——Misc.lua 系常量走哪条就跟哪条）、对应 `data/.../base/*.json`、`calc/minion.rs`（`minion_level_from_gem_level` 改收注入表、删 const——**与 A6 同 commit 节奏：先双跑后删**）。
- **附带**：若 A0 查出 `granted_effect_levels` 缺 `level_requirement`：`pipeline/config.json` `GrantedEffectsPerLevel` 补 `PlayerLevelReq` 列 + `tools/pobr-data-adapter/src/skills.rs` 导出 + `catalog/skills.rs::SkillLevelDef` 补字段。
- **测试**：逐值锁定单测（40 项等级表逐项、fixup=1.25、30+70）；regen byte-diff 零。
- **规模**：~350 行。

#### A5 gamedata 懒加载域 + BuildData 暴露（~1 天）

- **目标**：运行时读得到。`GameData` 新增 `minions()`/`spectres()`/`granted_effect_minions()` 懒加载域；`pobr-build::BuildData` 聚合为查询 API。
- **涉及文件**：`crates/pobr-gamedata/src/domains/minions.rs`、`domains/spectres.rs`、`domains/granted_effect_minions.rs`（新建，模式照抄 `domains/monster_scaling.rs` 等 ~700B 小文件）、`domains/mod.rs`、`crates/pobr-gamedata/src/manifest.rs`（v2 校验）、`crates/pobr-build/src/build_data.rs`（字段 + `minion_def(id)` / `effect_minion_list(effect_id)` / `resolve_minion_skill(...)` 查询方法）。
- **接口契约**（冻结给 Track B/C）：

  ```rust
  // BuildData 新增（B/C 只经这三个入口消费数据）
  pub fn minion_def(&self, id: &str) -> Option<&MinionDef>;          // minions 优先，miss 落 spectres
  pub fn effect_minion_list(&self, effect_id: &str) -> &[String];    // merge 后外键
  pub fn minion_constants(&self) -> &MinionConstants;                // A4 常量聚合视图
  ```

- **测试**：repo data 加载冒烟（RaisedZombie 命中、字段抽查）；manifest 缺表容忍（loader 返回空域不 panic，R7）。
- **规模**：~300 行。

#### A6 删 4 条手抄常量（搬迁收尾，~0.5 天）

- **目标**：兑现 P8「删 pobr-data 内嵌数值表」。A2 的逐值校验单测绿后，`minion.rs:261-430` 四个构造函数 + Spectre 占位删除，依赖它们的测试改读 fixture JSON（`crates/pobr-core` dev-dependency 不引 gamedata——用 `include_str!` 内嵌小 fixture 或测试内联构造）。
- **门禁**：独立 commit；parity 逐值不变（这 4 条当前无运行时消费方，理论 diff=0，跑 ninja_parity 确认）。

### Track B — build 链路接线（XML 导入 → orchestrator 识别 → MinionModifier 通道）

#### B1 XML 导入召唤物字段（~1 天）

- **目标**：Build 模型承载召唤物选择信息。
- **涉及文件**：`crates/pobr-build/src/xml_build.rs`（gem 元素属性解析）、`crates/pobr-build/src/build.rs`（`Gem` 结构补字段；`Build` 补 `spectre_list: Vec<String>`）。
- **vendor 参照**：`Modules/Build.lua:418-427`（`srcInstance.skillMinion = value.minionId` / `skillMinionCalcs` / `skillMinionItemSet`）、`:464-472`（`skillMinionSkill`、statSet 查找表）；spectreList 持久化在 build XML 的 Spectre 区段（`Build.lua:45 self.spectreList`，**实际元素名待 E1 抓到真实召唤 build 的 decoded.xml 后核实**——现有 18 个 fixture 无召唤 build）。
- **新字段**（全 Option/Vec，向后兼容）：`skill_minion`、`skill_minion_calcs`、`skill_minion_skill: Option<u32>`、`skill_minion_item_set`、`Build.spectre_list`。
- **测试**：手工构造 XML 片段往返单测；E1 真实 fixture 落地后补集成断言。
- **规模**：~200 行。

#### B2 orchestrator 识别召唤物宝石 → env.add_minion_from_def（~2 天，本 track 主体）

- **目标**：导入召唤 build 后 `OutputTable.minions` 非空且数值对齐 PoB2 装配语义。
- **涉及文件**：`crates/pobr-build/src/calc_orchestrator.rs`（新增 `minion_spawns(build, data) -> Vec<MinionSpawn>` 与接线段；参考既有 `trigger_modifiers` :1498 的组织方式）、`crates/pobr-core/src/calc/env.rs`（`add_minion_from_def` 签名按需扩 item-set/弓箭袋通道参数，保持纯函数）。
- **vendor 参照**（逐条对齐）：
  - 召唤物列表判定 `CalcActiveSkill.lua:846-857`：effect 名以 `Spectre` 开头 → 取 `build.spectreList`（monsterDamage=true）；以 `Companion` 开头 → beastList（M5a 不做 beast，留 TODO+Unsupported 标记）；否则取 `grantedEffect.minionList[1]`；support 的 `addMinionList` 追加。
  - 选型 `:866-877`：XML `skillMinion` 在列表内则用之，否则取第 1 个。
  - 等级判定 `:891-896`：`minionLevelIsEnemyLevel → env.enemyLevel`；`minionLevelIsPlayerLevel → min(characterLevel, cap)`；`skillData.minionLevel` 显式值；默认 `minionLevelTable[gem_level]`；clamp [1,100]。
  - limit → `Multiplier:SummonedMinion`：`CalcPerform.lua:1183-1191`（core `write_summoned_minion_multipliers` 已有，orchestrator 负责喂 limit 源值——limit stat 经 `base_number_of_<x>_allowed` 族聚合，先按 ModDb 既有 multiplier 通道接线）。
- **接口契约**（冻结给 Track C）：

  ```rust
  /// orchestrator 产出、env 消费的中间结构（pobr-build 内部）
  struct MinionSpawn<'d> {
      def: &'d MinionDef,
      minion_level: u32,            // 已按四规则判定 + clamp
      gem_level: u32,
      is_monster_damage: bool,      // spectre/beast 路径（hiddenDamageFixup 输入）
      minion_modifiers: Vec<MinionModifierEntry>,  // B3 产出
      infusion: AttributeInfusion,                  // B3 产出
      source: SourceId,             // SourceKind::Gem，归因贯穿
  }
  ```

- **测试**：合成 build（手写 XML：Raise Zombie + 等级 N）→ `OutputTable.minions[0]` 的 life/armour/虚拟武器逐值断言（用 PoB2 同参数面板值做 golden，oracle 跑一次记录进测试注释）；spectreList 路径同法。
- **规模**：~450 行。

#### B3 MinionModifier / 属性灌注通道（~1.5 天）

- **目标**：玩家词条 `Minions deal/have …` 进召唤物 ModDb；`StrengthAddedToMinions` 族 flag 生效。
- **涉及文件**：`crates/pobr-core/src/mod_parser.rs`（前缀段新增 minion 包裹：命中后产出**包裹标记**而非平铺 mod——建议 `Modifier` 增 `addressed_to: Option<ActorTarget>` 或沿用现有 LIST 通道产 `MinionModifierEntry`，以 mod_db LIST 形态存储；选型须与 W3 后的 modifier.rs 现状对齐，原则：**不改聚合内核，只加包裹层**）、`crates/pobr-build/src/calc_orchestrator.rs`（收集 LIST → `MinionSpawn.minion_modifiers`）。
- **vendor 参照**：`Modules/ModParser.lua:1203-1208` preFlag：`^minions ` / `^minions [cthd]ave|deal|take|use ` → `addToMinion = true`；`:1138 ["minion"]`；`CalcPerform.lua:1676` 消费语义（`value.type` 限定 → `MinionModifierEntry.minion_type`，core 已实现匹配）。属性灌注 flag 词条（`StrengthAddedToMinions` 等）出自天赋/装备 special 词条——M5a 只接**消费侧**（orchestrator 读 player ModDb flag → `AttributeInfusion`，对照 `CalcPerform.lua:1063-1075`），flag 生产归 M5b special_mods 批次。
- **测试**：`parse_mod("Minions deal 20% increased Damage")` → 包裹形态断言；端到端：玩家挂该词条 → minion DPS 增长、玩家自身 DPS 不变；归因断言（该词条 SourceId 出现在 minion 输出的 trace 上）。
- **规模**：~350 行。

#### B4（可选，时间富余才做）minion itemSet / 弓+箭袋通道

- `CalcPerform.lua:1031-1060`（`minionUseBowAndQuiver`、itemSet AddList）。次级通道，缺省明确标 Unsupported 并在 parity 报告单列。不阻塞验收。

### Track C — createMinionSkills + minion modDB 装配补全

#### C1 createMinionSkills 等价：召唤物主技能解析（~2 天，14-G5 主体）

- **目标**：法术系召唤物 DPS 有来源；melee 系叠加技能自身 damage multiplier。
- **涉及文件**：`crates/pobr-core/src/calc/minion.rs`（新增 `resolve_minion_skills(def, minion_level, effects: &MinionSkillData) -> Vec<MinionActiveSkill>` 纯函数；`MinionSkillData` 为注入的数据视图，零 I/O）、`crates/pobr-build/src/build_data.rs`（组装 `MinionSkillData`：skill_list id → granted_effects/levels/stat_sets 三表查询，复用既有 `resolve_skill_level` 路径）。
- **vendor 参照**：`CalcActiveSkill.lua:1049-1119 createMinionSkills`——逐条语义：
  1. `minionData.skillList` 过滤「数据中存在的技能」（pobr：granted_effects 含 MinionMeleeStep 等，已核实在库）；
  2. `ExtraMinionSkill` LIST 追加（带 minionList 限定）——通道预留，生产方在 M5b special；
  3. 空列表兜底 `MeleeAtAnimationSpeed`；
  4. 选级：`levels[].level_requirement <= minion.level` 取最高（依赖 A0/A4 的 level_requirement 列）；
  5. `minionSkill.skillData.damageEffectiveness = 1 + (minionDamageEffectiveness or 0)/100`（:1104，来自 A3 抽出的 skillData 键）;
  6. 主技能选择：XML `skillMinionSkill` 索引，缺省第 1 个（clamp）。
- **测试**：RaisedSkeletonStormMage + 宝石等级 N → 解析出 `ArcSkeletonMageMinion`，选级正确、spell 基伤来自 stat_sets 的 golden 断言（oracle 取 PoB2 中间值）；空 skillList 兜底断言。
- **规模**：~400 行。

#### C2 minion 主技能喂 offence 管线（~1.5 天）

- **目标**：`perform_minions` 不再只算虚拟武器：minion main skill 走与玩家同构的 minimal offence（spell → stat_sets 基伤；attack → 虚拟武器 × 技能 multiplier）。
- **涉及文件**：`crates/pobr-core/src/calc/perform.rs`（`perform_minions` :96-158 段重构：`MinimalInput::from(minion.base)` 之上叠 `MinionActiveSkill` 的 base damage / multiplier / use_time）、`crates/pobr-core/src/calc/output.rs`（`MinionOutput` 补 `main_skill_name`/`main_skill_dps`）、`crates/pobr-core/src/calc/minion.rs`（`MinionContext` 补 `active_skills: Vec<MinionActiveSkill>`）。
- **vendor 参照**：`CalcPerform.lua:964-971`（build minion skills 时机：keystone 合并后、modDB 装配前）；minion 走完整 offence 的事实（召唤物之后复用同一套 offence/defence 管线作为独立 actor）。
- **测试**：melee（Zombie：`base_damage_ignores_attack_speed=true`，攻速只进 DPS 不进每击）与 spell（Storm Mage Arc：基伤来自 stat_sets）两类 golden；DPS 对 oracle 中间值 @5%。
- **规模**：~350 行。

#### C3 minion modDB 装配补全（14-#9，~1 天）

- **目标**：对齐 `CalcPerform.lua:983-1075` 装配清单中缺失项。
- **逐项**（vendor 行号均 CalcPerform.lua）：
  - `hiddenDamageFixup`：`Damage MORE = round(allyDamage[lv]/damageTable[lv] × SpectreBeastDamageFixup, 2) - 1`（`CalcActiveSkill.lua:907` 附近；仅 monsterDamage=spectre/beast 路径非零）→ `minion.rs` 装配函数补注入，输入全部来自 A4 数据；
  - `mapLevelLifeMult`：hostile minion 生命 ×（:989-991）；
  - `PhysicalHeavyStunBuildup` / `EnemyHeavyStunBuildup` MORE（:1013-1014，常量来自 A4）；
  - `CritMultiplier = 怪物30 + 内禀70`（:1007，已实现——只把常量来源切到注入）；
  - `ProjectileCount BASE 1`（:1011 附近）；
  - `minionData.modList` 注入（:1017-1019，A1/A2 的 `mod_list` 字段 → Modifier 转换，OVERRIDE/FLAG 类型映射 mod_db 既有语义）。
- **涉及文件**：`crates/pobr-core/src/calc/minion.rs`（`build_minion_context_from_def` 扩参/扩装配）、`env.rs`（透传）。
- **测试**：Spectre 条目（Lightless Abomination：life=3、armour=0.4、fireResist=75、StunDuration OVERRIDE 3）端到端装配断言；hiddenDamageFixup 数值表驱动测试（取 3 个等级点对 oracle）。
- **规模**：~300 行。

### Track D — mirage 框架 + mirage_configs.json（14-#7）

#### D1 子环境重算框架（~2 天）

- **目标**：「复制主动技能 → 隔离 env 注入修正 → 重跑 perform → 写回主面板」的稳定逻辑落 `crates/pobr-core/src/calc/mirage.rs`（新文件）。
- **vendor 参照**：`Modules/CalcMirages.lua:22-54 calculateMirage`（copyActiveSkill → `usedByMirage` 标记防递归 → `mirageUses = storedUses` → preCalc 注入 → `calcs.perform(newEnv)` → postCalc 写回）、`:56 calcs.mirages` 入口（mainSkill 已是 mirage 或 disabled 则跳过）。
- **设计**：pobr 无 copyActiveSkill 概念，等价物 = 以当前 env 的只读快照构造子 `Env`（player ModDb clone + 主技能输入克隆 + 注入 mirage MORE 修正），跑 `perform` 取子 OutputTable。**与 M4 触发源速率的子计算机制（GlobalCache 等价物）同族**：A0 检查 M4 是否已落子计算入口，已落则复用其 env-clone 原语，未落则本项自建最小版（仅克隆、不缓存）并在文档标注供 M4 回收。归因：子图以 `SourceKind::Mirage` 挂回主 TraceGraph（不改归因结构，P17 红线——mirage 输出作为独立 output 节点，不合并进玩家 DPS 聚合节点）。`SourceKind::Mirage` 变体为 `pobr-data/src/source.rs` 的单行 enum 追加（D 提 PR、A 守门 review，沿 M3-T0 四变体先例）。
- **测试**：合成场景（玩家 Bow 技能 + 注入 count=2 / less=−25% more）→ mirage DPS = 单发 DPS × (1−0.25) × 2 的闭式断言；递归防护（mirage 技能不再触发 mirage）。
- **规模**：~350 行。

#### D2 mirage_configs.json + 数据消费（~1.5 天）

- **目标**：5 类配置数据化：`overlay/mirage_configs.json`。
- **vendor 参照**：`CalcMirages.lua` 五分支——Mirage Archer（`skillData.triggeredByMirageArcher`，count=`MirageArcherMaxCount`，less damage=`MirageArcherLessDamage`，less speed=`MirageArcherLessAttackSpeed`，源技能条件=武器为 Bow、未被 mirage 使用）、Saviour Mirage Warriors、Tawhoa's Chosen（:178）、Sacred Wisps、General's Cry。
- **schema**（`catalog/triggers.rs::MirageConfigDef`，对照 14 审计切分建议第 5 条）：

  ```json
  {
    "mirage_id": "mirage_archer",
    "trigger": { "skill_data_flag": "triggeredByMirageArcher" },
    "source_skill_filter": { "weapon_type": "Bow", "exclude_used_by_mirage": true },
    "count_stat": "MirageArcherMaxCount",
    "less_damage_stat": "MirageArcherLessDamage",
    "less_attack_speed_stat": "MirageArcherLessAttackSpeed",
    "uses_stored_uses": false,
    "handler_id": null
  }
  ```

  真特殊分支（Tawhoa 的 slam 选择 / General's Cry 的尸体模型）走 `handler_id`（注册进 `rules/registry.rs`，遵守 20-doc §5 计数监控）。
- **生产方式**：5 条配置由 sync-pob-catalog 子命令**内嵌生成**（数据写在工具源码常量里 → 工具落盘），满足「overlay 禁手改、只许工具再生」；vendor drift 由 check 子命令对 CalcMirages.lua 做行哈希提醒（粗粒度即可）。此取舍列 §6 开放问题。
- **涉及文件**：`catalog/triggers.rs`（新建/扩展）、`tools/sync-pob-catalog/src/`（生成 + drift 提醒）、`data/.../overlay/mirage_configs.json`、`domains/mirage_configs.rs`、`crates/pobr-build/src/calc_orchestrator.rs`（识别 mirage 触发条件 → 调 D1 框架；**该文件归 Track B 所有，D 的接线段排 B2 合并之后**）。
- **测试**：Mirage Archer 端到端（E1 的 mirage build fixture）；config 反序列化 + handler 覆盖告警单测。
- **规模**：~400 行。

### Track E — parity 扩集与 baseline 建立

#### E1 召唤/幻影 build fixture 抓取（~1 天）

- **目标**：`examples/demo-bd-test/builds/` 新增 ≥3 个召唤 build（建议：Witch 系 minion 主 C（zombie/skeleton mage）、Spectre 主 C、混合 minion+自身 DPS 各一）+ ≥1 个 Mirage Archer 系 build；含 `code.txt`/`decoded.xml`/`meta.json`（PoB2 黄金值）。
- **流程**（沿用既有工装）：poe.ninja poe2 builds 选 0.5.0 联赛真实角色 → `examples/demo-bd-test/tools/ingest_ninja.py` 抓取 → `make_fixture.py` 产 decoded.xml + meta.json；**meta.json 的 player_stats 必须含 Minion 面板键**（PoB2 导出含 `MinionDPS` 类键，确认 make_fixture 透传；缺则补 oracle 导出步骤）。同时用真实 decoded.xml 核实 B1 的 spectreList/skillMinion 元素名。
- **产出即契约**：fixture 落地后 B1/B2/C 的集成测试统一引用，不再各自手写 XML。

#### E2 baseline 建立与入门禁流程（~1 天，行为接线完成后执行）

按 roadmap M5 验收原文「**召唤/幻影 build 扩入 parity 集（先建 baseline 后入门禁）**」与「unsupported 词条率下降曲线纳入报表」，固化为四步：

1. **报告期**：新 fixture 进 `ninja_parity` 的发现列表但**不计入** `parity_no_regression` 断言（harness 加 build 级 allowlist 或按目录前缀 `minion-` 隔离统计），跑 `--nocapture` 出首份对照报告；
2. **修复期**：Track B/C/D 行为 commit 逐项缩差，每个行为修复附 PoB2 行号或 oracle 中间值；
3. **定基线**：聚合命中率稳定后，更新 `ninja_parity` baseline 常量为实测值，**独立 commit、显式审查**（diff 里同时贴报告摘要）；
4. **入门禁**：移出 allowlist，新 build 与原 18-build 同受零回归约束。原 18-build 在全程任何 commit 都不得倒退（这是硬底线，与新 build 的 baseline 建立解耦）。

- **涉及文件**：`crates/pobr-build/tests/ninja_parity.rs`（统计分组/allowlist 机制 + 基线常量）、`examples/demo-bd-test/builds/*`。
- **oracle 对拍**：`tools/pob2-oracle` headless 跑同 build，抽 Minion.Life / Minion.TotalDPS / hiddenDamageFixup 中间值各 ≥3 点写进测试注释（行为修复的一手依据）。

---

## 3. 并行 track 切分

### 3.1 拓扑与串行点

```
A0(依赖检查, 0.5d) ──► A1(schema) ──► A2 ∥ A3 ∥ A4 ──► A5(loader/BuildData) ──► A6(删手抄)
                          │
                          ▼ schema 冻结 + MinionSpawn/三接口契约
        B1(XML) ──► B2(orchestrator 接线) ──► B3(MinionModifier)   [B4 可选]
                          │ MinionSpawn 契约
        C1(createMinionSkills) ──► C2(喂 offence) ──► C3(modDB 补全)
        D1(子环境框架) ──► D2(configs)  ── 接线段排 B2 之后
        E1(fixture 抓取, 可最先做) ……………… E2(baseline, 全部行为合并后收口)
```

- **必须串行**：A0→A1→A5（数据契约链）；B2 之后才轮到 D2 的 orchestrator 接线段；E2 是全阶段收口。
- **可并行**：A2/A3/A4 互不重叠；B 与 C 在 A1 schema 冻结后即可用「4 条手抄常量 + 手写 XML」并行开发（A5 落地后切真实数据，一行 import 切换）；D1 与 B/C 无文件交集；E1 第一天就可做（还能反哺 B1 的元素名核实）。
- **建议人力**：5 个 worktree agent 对应 5 个 track；A 是关键路径，先投人。

### 3.2 文件归属表（每文件唯一写者；越界改动必须经该 track 的 PR）

| 文件/目录 | 独占写者 | 说明 |
|---|---|---|
| `crates/pobr-data/src/catalog/actors.rs`、`catalog/mod.rs`、`catalog/manifest.rs`、`catalog/skills.rs`（minion 字段）、`catalog/monster_scaling.rs`、`catalog/game_constants.rs`、`catalog/triggers.rs` | **A**（triggers.rs 的 MirageConfigDef 段由 D 起草、A 合入——schema 统一归 A 守门） | schema 单点 |
| `crates/pobr-data/src/minion.rs` | **A** | A1 迁移 + A6 删除 |
| `tools/sync-pob-catalog/**` | **A**（D2 的生成子命令例外：D 写、A review） | extract-lua 通道 |
| `pipeline/config.json`、`tools/pobr-data-adapter/**` | **A** | 仅 level_requirement 兜底时动 |
| `data/4.5.0.3.4/**`（产物） | **A**（D2 的 mirage_configs.json 由 D 的工具生成） | 禁手改，工具产出 |
| `crates/pobr-gamedata/**` | **A** | 域 loader + merge |
| `crates/pobr-build/src/xml_build.rs`、`build.rs` | **B** | |
| `crates/pobr-build/src/calc_orchestrator.rs` | **B** | D2 接线段在 B2 合并后由 D 提交、B review |
| `crates/pobr-build/src/build_data.rs` | **共享：A 建查询 API（A5），C 加 `MinionSkillData` 组装段** | 按函数切分：A 负责数据字段与三接口，C 只新增 `minion_skill_data()` 一个函数，先后合入 |
| `crates/pobr-core/src/mod_parser.rs` | **B**（仅 minion 前缀段） | 热点文件，B 内单 commit |
| `crates/pobr-core/src/calc/minion.rs` | **C**（A4 的常量切注入段例外：A 提 PR、C review） | |
| `crates/pobr-core/src/calc/perform.rs`、`output.rs`、`env.rs` | **C** | D1 不碰 perform：mirage 入口由 C 在 perform 预留一个 `fn run_mirage_pass(...)` hook（空实现），D 只填 mirage.rs |
| `crates/pobr-core/src/calc/mirage.rs`（新） | **D** | |
| `crates/pobr-core/src/rules/registry.rs` | **D**（注册 mirage handler） | |
| `examples/demo-bd-test/**`、`crates/pobr-build/tests/ninja_parity.rs` | **E** | baseline 常量只许 E 改 |
| `devs/scripts/regen-check.sh` | **A** | |

### 3.3 track 间接口契约（变更需双方签字）

1. **A→B/C：schema v2 字段清单**（A1 文末冻结）+ `BuildData` 三接口（§2 A5 代码块）。
2. **B→C：`MinionSpawn`**（§2 B2 代码块）——B 产、orchestrator 内消费时调 C 的装配函数；字段增删走 PR 互审。
3. **C→D：`run_mirage_pass` hook 签名**：`fn run_mirage_pass(env: &Env, configs: &[MirageConfigDef], out: &mut OutputTable)`（C 预留空壳，D 填实）。
4. **C/D→E：`MinionOutput` / mirage 输出键名**（`main_skill_dps` 等）一经 E 的报告引用即冻结。

---

## 4. 门禁与验收

### 4.1 每 track 局部门禁

| Track | 局部门禁 |
|---|---|
| A | 工作区三件套全绿；**搬迁不变式**：A2/A4/A6 均为纯搬迁 commit，ninja_parity 逐值不变（golden diff=0）；regen-check 重跑 byte-diff 零；A2 的 4 条逐值校验单测（手抄 vs 抽取）绿 |
| B | 三件套；B1/B3 解析往返单测；B2 合成 build 端到端 golden（oracle 中间值注明）；**原 18-build 零回归**（召唤接线不得扰动非召唤 build——orchestrator 新段必须 gate 在 `effect_minion_list 非空`） |
| C | 三件套；C1/C2/C3 各自 golden（oracle 取数 ≥3 点）；原 18-build 零回归 |
| D | 三件套；D1 闭式断言 + 递归防护；handler 计数告警单测；原 18-build 零回归 |
| E | fixture 完整性（code/decoded/meta 三件齐 + Minion 面板键存在）；baseline commit 独立且附报告摘要 |

### 4.2 阶段整体验收（roadmap M5 验收原文逐条落点）

| roadmap 原文 | 本蓝图落点 |
|---|---|
| 「召唤/幻影 build 扩入 parity 集（**先建 baseline 后入门禁**）」 | E2 四步流程；完成判据：≥3 召唤 + ≥1 mirage build 在门禁内、`parity_no_regression` 含其 baseline |
| 「unsupported 词条率下降曲线纳入报表」 | 报表机制 owner = **M5b A-2**（`pobr-build/src/corpus.rs` + ninja_parity 报表段）；E2 复用其分类函数只追加 minion build 维度计数，不另写一套（M5b A-1/A-2 第 0 天即可开工，建议先行合并）。`ninja_parity.rs` 共享文件按段分工：M5b-A 管报表段、M5a-E 管 baseline/allowlist 段，后合并者 rebase；Companion/beastList、minion itemSet（B4 未做时）必须以 Unsupported 显式计数而非静默丢弃 |
| 「special 迁移条目 oracle 抽样对拍」 | 属 M5b；M5a 对应物 = minion 中间值 oracle 对拍（E2） |
| parity 附 B「M5 行」 | 进攻/防御总命中率**不低于 M4 收口值**（≥70%/≥85%，以届时实测 baseline 为准），新增召唤 build 单列统计 |
| 14-G3/G4/G5 关闭判据 | G3：minions/spectres JSON 在库 + 手抄常量删除 + manifest/loader/adapter 三处挂接；G4：真实召唤 build `OutputTable.minions` 非空且 life/防御 @5% 命中；G5：法术召唤物（Storm Mage 类）DPS 来自技能数据且对 oracle @5% |

### 4.3 双跑要求

- A4 的 `minion_level_from_gem_level` 切注入：保留 const 一个过渡 commit，测试断言「注入表 == const 表」逐项相等后删 const（微型双跑）。
- C2 改 `perform_minions`：虚拟武器旧路径保留为 fallback（`MinionContext.active_skills` 为空时走旧路径），新旧共存一个版本周期，mirage/技能路径稳定后再评估删除——避免半成品数据（skill_list 引用了不在库的技能 id）把已能算的 melee 召唤物打回 0。

---

## 5. 风险与回退（风险登记簿 R# 在本阶段的具体落点）

| R# | 本阶段落点 | 缓解/回退 |
|---|---|---|
| R2 隐藏补偿 | ① `derive_minion_base_stats` 同时吃「monster_scaling 注入表（W3 改造中）+ minionLevelTable（A4 搬迁）」，两处口径耦合，若 W3 的 rounding 与手抄表有微差会在 minion 侧放大；② 4 条手抄常量与 vendor 当前值可能已漂移（手抄注明 2025-06） | A2 逐值校验单测先行，发现 diff **以 vendor 为准**、diff 记录进 commit message；A4/A6 严格分 commit；与 W3 的合并顺序：A4 等 W3 的 game_constants 通道合入 master 后 rebase |
| R3 extract-lua 正确性 / vendor 漂移 | Spectres.lua 的 `modList` 含真实 `mod(...)`/`flag(...)` 构造与 tag table，stub 序列化若丢参会静默错数据；Minions/Spectres 是 auto-generated，vendor bump 时条目会整体换血 | stub 把 mod 全部入参原样记录（含 tag 原始 JSON），消费侧不识别的形态显式 Unsupported；CI drift diff（check 子命令对两文件行哈希）；oracle 终裁（E2 中间值） |
| R6 数据体积 | spectres.json 预计 ~700KB+（vendor Lua 624K） | 懒加载域（不进默认热路径）；ninja_parity 仅在含 spectre 的 build 触发加载；体积入 regen-check 报表观察，不设硬限 |
| R7 schema 演化 | MinionDef 扩 7 字段、GrantedEffectDef 扩 minion_list、SkillLevelDef 扩 level_requirement | 全部 `#[serde(default)]`；manifest 按域记 schema 版本；loader 容忍缺表（A5 单测锁定） |
| R8/P17 归因 | mirage 子环境重算是「子图归因」的又一实例，禁止顺手改 TraceGraph 结构 | D1 设计已定：mirage 输出为独立节点 + `SourceKind::Mirage` 边，不合并进玩家 DPS 聚合；若发现必须改归因结构，停下出 mini-RFC（援引 P17 流程） |
| 三线并行合并冲突（M5 总风险） | 本蓝图内部 5 track 已做文件归属表；对 M5b/M5c：本阶段碰 `mod_parser.rs`（B3 前缀段）与 M5b special 批次潜在相邻 | B3 限定在前缀（preFlag）段单 commit；与 M5b 约定：special 批次不动前缀段；worktree 合并顺序 A→B→C→D→E |
| 能量元宝石口径（14-#12 / P12） | 本阶段不触碰：trigger.rs 能量模型维持悬空，召唤/触发交叉场景一律走 PoB2 口径 | 已有裁决，写明防止实施 agent 顺手接线 |

**整体回退**：每 track 独立可合并、独立可 revert——A 合入后即使 B/C 未完成，数据在库无消费方、零行为影响；B2 的 orchestrator 段有 `effect_minion_list 非空` gate，revert 单 commit 即回到召唤面板恒空的现状，原 18-build 不受牵连。

---

## 6. 开放问题（实施前需总裁决，不阻塞 A0–A1 启动）

1. **minions/spectres 物理归宿**：本蓝图按 P1「生产工具定层」落 `overlay/`；20-doc §3.1 表格按「逻辑 L1」列在 `base/`。二选一需总架构拍板（影响 manifest 段位与 regen-check 条目，代码量 ~10 行）。建议：overlay 先行，version-bump-drill 触发路线 A 时迁 base。
2. **mirage_configs 的「工具生成」纪律**：5 条配置实质是人工从 Lua 闭包转写，本蓝图用「数据内嵌进 sync-pob-catalog 源码 → 工具落盘」满足 overlay 禁手改的字面要求。若总裁决认为这是绕规，备选：承认其为 L2 手工策展特例（同 `item_tag_special.json` 的「手工数据，标注维护来源」先例）。
3. **`Multiplier:SummonedMinion` 的 limit 源值口径**：PoB2 limit 来自 `base_number_of_<x>_allowed` stat 聚合（含天赋/装备加成），pobr 当前 `write_summoned_minion_multipliers` 收一个标量。B2 先用「技能数据基值 + ModDb 该 stat 聚合」近似，是否需要完整 `output[limit]` 反查（PoB2 CalcPerform:1190 用 output 值）待首份 parity 报告定。
4. **Companion/beastList**：M5a 显式不做（标 Unsupported），排期归属（M5a 尾巴 vs M7 长尾）待 E2 报告看 ninja 命中频率。
5. **W3 合流时点**：A4 的常量落点（monster_scaling 扩段 vs game_constants monster 段）依赖 W3 最终形态，A0 检查时与 W3 agent 对齐一次即可。

---

## A0 检查结果（2026-06-13/14 实施实测，基线 50cbfe9 + pre-M5a 数据 0050b73）

> 续作实施者核实：pre-M5a 数据前置（`0050b73`）已把 §1 路线 B 的全部产物 + actors.rs schema 落库，本阶段实际起点远超蓝图 §0.2 描述的「greenfield 半成品」。

**数据/schema 现状（A1/A2/A3/D2 数据面已就绪）**：
- `catalog/actors.rs` 已建：`MinionEntryDef`（入库 v2 全字段 + `mod_list` 结构化）、`MinionModDef`、`LuaValueDef`、`GrantedEffectMinionDef`、`MinionsDef`/`GrantedEffectMinionsDef`。
- 四份 overlay 数据已生成且 `_meta` 齐全：`minions.json`（32 条）、`spectres.json`（**591** 条，非蓝图预估 593——vendor `Data/Spectres.lua` 593 赋值块中 2 个 key 重复，运行时去重后 591，已在 `load_minions::spectres_count` 锁定）、`granted_effect_minions.json`（31 条边车）、`mirage_configs.json`（5 条，schema `catalog/triggers.rs::MirageConfigDef`）。vendor commit `2df5a74`。
- gamedata 域 loader 已建：`domains/{minions,spectres,granted_effect_minions,mirage_configs}.rs`（缺文件返回 `Option::None`，向后兼容）。

**A0 检查清单逐项**：
1. `GrantedEffectDef.minion_list`：**本阶段已补**（`minion_list`/`add_minion_list`/`minion_uses`/`minion_has_item_set`，merge 后内存形态，base JSON 不变）——commit `7400232`（承接 WIP `84777bc`）。
2. `SkillLevelDef.level_requirement`：未单独核验（C1 选级依赖项，C 未做时不阻塞）；A4 兜底项预计可免（蓝图 §1.3 已注 M1 T4 落库）。
3. `ruleset::GameConstants`：minion 侧常量仍走 `calc/minion.rs` const（`MINION_LEVEL_TABLE`/30/70），A4 搬迁未做。
4. `xml_build.rs` 的 `skillMinion` 解析：**仍无**（B1 未做）。

**minion_list 外键核验（§1.3 必做第一步，结论）**：32 minions + 591 spectres = 623 个 minion id；`granted_effect_minions.json` 31 条边车的 `minion_list`/`add_minion_list` 引用全部命中（**零悬空外键**）。`RaiseZombiePlayer→[RaisedZombie]`、`RagingSpiritsPlayer→[SummonedRagingSpirit]`、`ManifestWeaponPlayer.minion_uses=["Weapon 1"]`+`minion_has_item_set` 等抽样均正确。

**本阶段实际交付（2 commit，门禁四件套全绿）**：
- `7400232` feat(m5a): A1/A3/A5 数据接线——`MinionDef::from_entry` 桥接 + `granted_effects()` 加载期 merge 边车 + `BuildData.{minion_def, effect_minion_list, minions}` 查询 API（A5 三接口契约其二；`minion_constants` 视图未建——A4 未做）。
- `9b5a918` feat(m5a): B2 orchestrator `spawn_minions`——识别召唤宝石（`effect_minion_list` 非空 gate）→ `Env.minions` → `OutputTable.minions` 非空。**G4 核心关闭**：Raise Zombie L20 build 产出召唤物快照、level=40（minionLevelTable[20]）、life>0 且与 core 派生口径一致；非召唤 build minions 恒空（gate 正确，18-build 零回归）。

**残项（未做，按依赖序）**：
- **A4/A6**：minion 常量搬迁（`MINION_LEVEL_TABLE`/30/70/`SpectreBeastDamageFixup`/`mapLevelLifeMult` → 数据注入面）+ 删 4 条手抄常量。依赖 W3 game_constants 形态。
- **B1**：`xml_build.rs` 解析 `skillMinion`/`skillMinionSkill`/spectreList（依赖 E1 真实 fixture 核实元素名）。
- **B3**：mod_parser `^minions ` 前缀包裹（vendor ModParser.lua:1204-1205 `addToMinion`）→ orchestrator 收集喂 `spawn_minions` 的 `minion_modifiers`（首版传空）+ 属性灌注消费侧（`StrengthAddedToMinions` → `AttributeInfusion`）。**热点文件 mod_parser，须单 commit 谨慎**。
- **C1/C2/C3**：createMinionSkills（法术召唤物主技能解析 → spell 基伤 / melee multiplier）+ 喂 offence + modDB 装配补全（hiddenDamageFixup/mapLevelLifeMult/ProjectileCount/`minionData.modList` 注入）。**G5 未关闭**：当前 spell 召唤物（Storm Mage）仅算虚拟武器近似，无技能基伤来源。
- **D1/D2**：mirage 子环境重算框架 + 5 类配置消费（数据已在库，消费接线未做）。
- **E1/E2**：召唤/幻影 build fixture 抓取（需 poe.ninja，headless 不可达）+ parity 扩集与 baseline 建立。**B2 已使 `OutputTable.minions` 非空，但未扩入 ninja_parity 报表**（无真实召唤 fixture）。

**limit 口径（§6.3）**：B2 `spawn_minions` 按 `base_sum(limit_stat)` 取玩家 BASE 之和、缺则兜底 1，是近似实现；`ActiveMinionLimit` MORE 乘区与 Override 口径、`output[limit]` 反查待首份召唤 parity 报告定。
