# 整合清单：沙箱实现 → 真实仓库

> 来源：在一个**旧快照沙箱仓库**（`/Volumes/personal_folder/Codes/games/pobr`，下称 SANDBOX）里独立完成了一批实现（6 个提交，`cargo test` 363 通过）。
> 经只读对比，**本仓库（REAL）是权威且更先进的版本**：核心计算、来源接入（item/passive/gem ingest）、归因（`attribute()`）、以及 SANDBOX 完全没有的 **GGG 数据管线**（`pobr-gamedata` / `tools/pobr-data-adapter` / `data/` / `pipeline/` / `vendor/`）都更完整。
> 因此**不是拷贝，而是有选择地把 SANDBOX 的增量移植 + 适配到 REAL 的 API**。本文件不改任何代码，仅供逐项点选。
>
> 生成时间：2026-06-04。SANDBOX 代码完整保留在该沙箱仓库，可作移植参考。

---

## 0. 一句话结论

REAL 为权威。SANDBOX 真正值得移植的增量集中在三处：①REAL 计算层**还没有**的机制模块（`skill_use_time` / `ailment` / `ehp` / `survivability`）；②REAL 仍是**空占位**的上层 crate（`pobr-build` 的 Build Code 编解码、`pobr-trade`、`pobr-i18n`、`pobr-tree`）；③一批**纯增量**的 `pobr-data` 类型与 `display_catalog`。SANDBOX 的归因 / setup / active_skill / raw item 解析**被 REAL 更好的设计取代，不移植**。

---

## 1. 总体优先级（按价值÷工作量）

| 优先 | 项目 | 为什么 | 工作量 |
|------|------|--------|--------|
| **P0** | `pobr-build::build_code`（XML↔zlib↔URL-safe base64 解码） | 直达你最初目标：用 `examples/demo-bd-test/ninja-bd-*.txt` 真实 PoB2 code 做 decode 验证。纯编解码、几乎不依赖 data/core API，移植后可立刻验证字节级兼容 | low |
| **P0** | 计算机制：`skill_use_time` / `ailment` / `ehp` / `survivability` | REAL 计算层完全没有；纯新增、只依赖 `ModDb`/`CalcConfig`/常量；高价值 | low–med |
| **P1** | `pobr-data` 纯增量：`GameConstants`/`DamageRange`/`SkillCost`、`build_config`(ViewMode/Bandit/GameVersion)、`BoundarySpec`/`StatTextKey`、`damage.rs`、枚举 serde、`DisplayStatDefinition::computed/planned`+`DisplayStatValue` | 大多 `new`/纯追加，给上面机制与 catalog 打底 | low |
| **P1** | `display_catalog`（强类型展示字段目录 + parity 投影） | 推进 PoB 全量属性覆盖目标；依赖 OutputTable 字段，需先扩展 output | low–med |
| **P2** | `damage_vector` 的转换/gain/**double-dip** 增强 | REAL 已有简版 `calc/damage.rs`（仅 type+min+max），SANDBOX 增加转换归一化、gain-as-extra、increased double-dip、source/kind 分桶——是增强而非替换，需在 REAL `DamageComponent` 上扩展 | med |
| **P2** | `stat_boundary` 的 floor/missing/通用 `bounded_stat` | REAL OutputTable 已有 `max_*_resistance`/`*_over_cap`，但无 floor/missing/通用边界抽象——补齐而非重做 | med |
| **P2** | `pobr-trade` / `pobr-i18n` / `pobr-tree` 实现填入占位 | REAL 是空占位；`trade`/`i18n` 依赖少、好移植；`tree` 需与 `catalog.rs::PassiveNodeDef` 对齐 | med |
| **P3** | `mod_db` 的 `more_traced`/`flag_traced`/`override_traced`、`sync-pob-catalog` 的 `diff`/fixture、`tools/lint-i18n`、`apps/*` | 锦上添花；apps 依赖上层 crate 落定后再做 | low–med |
| **不移植** | SANDBOX 的 `attribution`(compute_attribution/BuildSnapshot)、`calc/setup.rs`、`calc/active_skill.rs`、`pobr-item` raw 解析、`item.rs` 的 ItemSlot/Item 重构 | **被 REAL 取代**：`attribute()`+`filtered()`、`ingest_item/passive/gem`、`item_text.rs`、`EquipmentSlot` 更成熟 | — |

> **建议第一批就做 P0**：`build_code` decode + 4 个机制模块 + 其 P1 data 依赖。这样既达成 ninja-code 验证的最初目标，又把 REAL 计算层缺的机制补上，且几乎不触碰 REAL 已有核心（低冲突）。

---

## 2. 逐项清单

图例：`new`=REAL 没有可新增 · `overlap`=已有等价物可少量适配 · `conflict`=已有但 API/语义不同需重写或二选一 · `skip`=被 REAL 取代不移植。

### 2.1 pobr-data

| 组件 | 状态 | REAL 对应 | 建议 | 风险 | 工作量 |
|------|------|-----------|------|------|--------|
| `DamageSource`/`DamageKind`/`AilmentType`/`AilmentCategory`/`ElementalType` 枚举 | new | 无 | 合并进 `constants.rs` | 低 | low |
| `GameConstants`+`poe2()` / `DamageRange` / `SkillCost(Kind)` | new | 无 | 整体移植；REAL 的 `ARMOUR_RATIO` 等可收归 `GameConstants` | 低 | low |
| `DamageType`/`ClassId` serde + `is_elemental()`/`ELEMENTAL` | conflict | 同名无 serde/无方法 | 给 REAL 补 derive 与方法（字段一致，纯加法） | 低 | low |
| `damage.rs`：`DamageComponent`/`AilmentInstance`/`DebuffInstance`/`HitDamageResult` | new | 无（注意与 `calc::DamageComponent` 同名，见 2.2） | 新建文件；依赖上面枚举 + `SkillTypes` serde | 中（依赖链） | low |
| `stat.rs`：`BoundarySpec`/`StatTextKey` | new | 仅 `StatId`/`StatDescription` | 追加；`BoundarySpec::resistance()` 用 REAL 命名 `DEFAULT_MAX_RESISTANCE`/`HARD_MAX_RESISTANCE` | 低 | low |
| `build_config.rs`：`ViewMode`/`BanditChoice`/`GameVersion` | new | 无 | 新建文件 + lib 注册 | 低 | low |
| `display_stat.rs`：`computed()`/`planned()`/`with_*` 构造器 + `DisplayStatValue`/`TraceMode`/`AttributionGroup`/`AttributionMode` | overlap | 核心 struct/enum **逐行相同**；缺这些方法与类型 | 纯追加方法 + 4 个类型 | 低 | low |
| `skill.rs`：`SkillTypes`/`SkillFlags` serde(transparent) + 新常量(MELEE/PROJECTILE/...) + `SkillId` 方法 | conflict | 同名缺 serde、仅 3 常量 | 补 serde（注意 `CalcConfig` 用 `SkillTypes`，验证序列化）+ 追加常量 | 中 | low |
| `passive_tree.rs`：`NodeKind`/`GroupId`/`JewelKind`/`NodeData`/`spec.ascendancy_class` | conflict | 极简(NodeId+spec)，且 `catalog.rs` 已有更完整 `PassiveNodeDef`/`PassiveNodeKind` | **以 `catalog.rs::PassiveNodeDef` 为权威**；`pobr-tree` 计算层对齐它，避免两套并存 | 中 | med |
| `item.rs`：`ItemSlot`(15,含 Flask)/`ItemImplicitKind`/`Requirements`/`EquippedItem`/`Item` 重构 | conflict | REAL 用 `EquipmentSlot`(10)、`Item` 三字段(implicit/enchant/modifier_texts) | **以 REAL 为准**；如需 Flask 槽，给 REAL `EquipmentSlot` 扩展而非引入 `ItemSlot`。SANDBOX 的 `Item` 重构**不移植** | 高 | med |
| `gem.rs`：`GemKind`/`GemTag`/`SkillLevelEffect`/`SocketGroup` + `Gem.is_support`→`GemKind` | conflict | `Gem` 用 `is_support:bool`，`SkillEffect` 较简 | 追加 `GemTag`/`SkillLevelEffect`/`SocketGroup`（new，好）；`is_support→GemKind` 是 PoE2 语义扩展，**可选**，会牵动 core 调用点 | 中 | med |
| `source.rs` / `modifier.rs` / `game_data.rs` | overlap | 一致 | 直接用 REAL，不动 | 无 | — |
| `catalog.rs`（DataManifest/BaseItemDef/.../PassiveNodeDef/...） | real_only | REAL 独有（数据管线 schema） | 保留；作为 item/passive 的权威数据定义 | — | — |

### 2.2 pobr-core 计算机制（注：失败切片，由主控直接读 REAL 源码补全）

REAL `calc/`：`actor/breakdown/damage/defence/env/error/offence(22K)/output/perform/session`；`OutputTable` 已含 `max_*_resistance`/`*_resistance_over_cap`/`damage_components`；`mod_db` 有 `sum_traced`/`filtered`（无 more/flag/override_traced）。

| 组件 | 状态 | REAL 对应 | 建议 | 风险 | 工作量 |
|------|------|-----------|------|------|--------|
| `calc/skill_use_time.rs`（speed bucket + action speed 独立乘区 + server frame cap + `is_channelling`） | new | 无 | 直接移植，只依赖 `ModDb`/`CalcConfig`/常量 | 低 | low |
| `calc/ailment.rs`（bleed/ignite/poison magnitude + shock + corrupted blood） | new | 无 | 直接移植；poison 基数=物理+混沌（已对照 ailments.md 修正） | 低 | low |
| `calc/ehp.rs`（各类型 max hit + EHP=lowest） | new | 无 | 移植；`reference_hit` 近似限制已注明 | 低–中 | low |
| `calc/survivability.rs`（预留/恢复/格挡/法术抑制） | new | 无 | 直接移植，纯 `ModDb` 查询 | 低 | low |
| `calc/stat_boundary.rs`（uncapped/max/final/overcap/**missing**/floor + traced） | overlap | REAL OutputTable 已算 `max_*`/`over_cap`，但无 floor/missing/通用 `bounded_stat` | 把 REAL 现有抗性边界重构为调用 `bounded_stat`，补 floor/missing | 中 | med |
| `calc/damage_vector.rs`（转换归一化 + gain-as-extra + **increased double-dip** + source/kind 分桶） | conflict | `calc/damage.rs`（`DamageComponent{type,min,max}` + 简单 per-type inc/more） | 在 REAL `damage.rs` 上**增强**：加转换/gain/double-dip；REAL `DamageComponent` 需扩展或并入 `pobr-data::damage::DamageComponent`（解决同名） | 中 | med |
| `OutputTable` 字段扩展（skill_use_time/effective_action_rate/bleed/ignite/poison/shock/各 max_hit/total_ehp/reservation/regen/block/suppression） | conflict | REAL `OutputTable` 字段较少 | 按机制移植顺序逐步加字段（纯加法，注意 `From<&MinimalOutput>` 同步） | 中 | med |
| `mod_db` `more_traced`/`flag_traced`/`override_traced`/`iter_mods` | new | 仅 `sum_traced`/`filtered` | 追加（归因增强用）；REAL 用 `filtered` 做 recompute，`iter_mods` 可省 | 低 | low |
| `calc/perform.rs` 编排扩展 | conflict | REAL 有自己的 perform | 按移植的机制逐步在 REAL perform 末尾追加 fill 阶段 | 中 | med |
| `display_catalog.rs` | new | 无 | 移植；依赖 OutputTable 扩展后字段；Computed/Planned + parity self-check | 低–中 | low |
| `benches/mod_db_bench.rs`（criterion，5000 mod） | new | 无 | 移植；workspace 加 criterion dev-dep | 低 | low |

### 2.3 pobr-core 归因 + 来源接入

| 组件 | 状态 | REAL 对应 | 建议 |
|------|------|-----------|------|
| `attribution`（`compute_attribution`/`BuildSnapshot`/`AttributionRequest`/`AttributionEntry`/`AttributionReport`） | **skip** | `attribute<F:FnMut>()` + `AttributionReport::direct()` + `session.snapshot()` + `mod_db.filtered()` | **不移植**。REAL 的闭包 recompute 设计更灵活、已与 session 成闭环。三种口径（direct/marginal/interaction）语义一致。SANDBOX 的 `without_source` 过滤逻辑可作 REAL recompute 闭包实现参考 |
| `AttributionGroup`/`AttributionMode` | overlap | REAL 已有（带 `wants_marginal/wants_interaction` 方法） | 用 REAL 的 |
| `calc/setup.rs`（build_player_mods 统一聚合） | **skip** | `ingest_item`/`ingest_passive_nodes`/`ingest_gem` 三个分域 + `session.add_item/add_passive_nodes/add_gem` | **不移植**。REAL 分域 ingest 更细、support 父子 source 关联更完整 |
| `calc/active_skill.rs`（mana_multiplier/兼容性） | **skip(参考)** | `skill_source::ingest_gem` | 不移植结构；其中 mana_multiplier 与 skill-type 兼容性逻辑可作 REAL `skill_source` TODO 的**实现参考** |
| `pobr-core::item_text` / `passive` / `skill_source` | real_only | — | 保留 REAL，作为数据管线必要环节 |

### 2.4 上层 crate + apps + tools（注：失败切片，由主控直接读 REAL 占位补全）

REAL 上层 5 crate 均为**空占位**（`lib.rs` 仅 doc 注释）；apps/cli·desktop 为 360–383B main.rs stub、wasm 314B lib.rs stub；tools 有 `sync-pob-catalog`(无 diff) 与独有 `pobr-data-adapter`。SANDBOX 的上层实现**依赖 SANDBOX 的 pobr-data/pobr-core API**，移植主要工作量在**重定向到 REAL API**。

| 组件 | 状态 | 建议 | 风险/工作量 |
|------|------|------|-------------|
| `pobr-build::build_code`（decode/encode） | new(填占位) | **P0 优先**。几乎不依赖 data/core；移植后用 ninja code 验证字节级兼容（SANDBOX 已做 padding 容错 + zlib + 防 bomb） | 低 |
| `pobr-build` 其余（Build 状态/xml_serde/orchestrator/cache/comparison/import_detect） | new(填占位) | orchestrator/comparison 依赖 SANDBOX 的 CalculationSession/OutputTable，需重定向到 REAL；`import_detect`/`build_config` 低依赖可先移 | 中–高 |
| `pobr-trade`（trait + 离线 mock + TradeQuery/Item） | new(填占位) | 依赖少，直接移植适配 REAL `pobr-data` | 低 |
| `pobr-i18n`（Translator/en-US·zh-TW/fallback/stat_text） | new(填占位) | 依赖 `StatId`/`StatTextKey`；移植适配 | 低–中 |
| `pobr-tree`（PassiveTree/collect_allocated_mods/radius jewel） | new(填占位) | 需与 REAL `catalog.rs::PassiveNodeDef` 对齐节点数据结构（不要引入 SANDBOX 的 `NodeData` 重复） | 中 |
| `pobr-item`（raw_text/sections/crafter） | **skip/重叠** | REAL `pobr-core::item_text` 已有 PoB 兼容物品文本解析；占位注释也提示需厘清职责边界。**不直接移植**，必要时把 SANDBOX 的 section 切分/CustomItemDraft 思路并入 REAL 既有路径 | 中 |
| `tools/sync-pob-catalog` 的 `diff_catalogs`/`write_self_contained_fixture`/`check_against_fixture` | new | REAL 已有 collect/write/read，无 diff；追加 | 低 |
| `tools/lint-i18n` | new | 依赖 `pobr-i18n` 落定后再做 | 低 |
| `apps/pobr-cli`/`pobr-wasm`/`pobr-desktop` | new(填 stub) | 依赖上层 crate 落定后做；优先级最低 | 中 |

---

## 3. 推荐分批移植（落地后每批独立提交）

1. **批 A（P0，达成 ninja-code 目标）**：移植 `pobr-build::build_code`（decode/encode + import_detect）→ 写一个读 `examples/demo-bd-test/ninja-bd-*.txt` 的测试，验证 decode 出合法 XML（字节级兼容验证）。**几乎零冲突**。
2. **批 B（P0/P1，补 REAL 计算机制）**：先移 `pobr-data` 纯增量（GameConstants/DamageRange/SkillCost/枚举+serde/BoundarySpec/build_config/damage.rs），再移 `skill_use_time`/`ailment`/`ehp`/`survivability` + OutputTable 对应字段扩展 + perform fill 阶段。
3. **批 C（P1）**：`display_catalog` + `display_stat` 构造器/类型；`mod_db` traced 方法。
4. **批 D（P2）**：`stat_boundary`(floor/missing 重构 REAL 抗性) + `damage_vector` double-dip 增强（在 REAL `damage.rs` 上）。
5. **批 E（P2）**：`pobr-trade` / `pobr-i18n` 填占位 + `tools` 增强。
6. **批 F（P3）**：`pobr-tree`（对齐 catalog）、`pobr-build` 编排/comparison、`apps/*`。

每批都需在 REAL 仓库 `cargo test --workspace` + `clippy --all-targets -D warnings` + `fmt` 全绿后提交。

---

## 4. 与 REAL 既有设计冲突的硬决策（需你拍板）

1. **物品槽位**：保留 REAL `EquipmentSlot`（建议）还是采用 SANDBOX `ItemSlot`(含 Flask)？影响 `pobr-core` 多处引用。
2. **`Gem.is_support: bool` → `GemKind`**：是否采纳（PoE2 有 Spirit/Meta gem）？会牵动 core 调用点。
3. **天赋节点权威定义**：统一到 `catalog.rs::PassiveNodeDef`（建议）；`pobr-tree` 不要再引入 SANDBOX `NodeData`。
4. **`DamageComponent` 同名**：REAL `calc::DamageComponent{type,min,max}` vs SANDBOX `pobr-data::damage::DamageComponent{source,kind,type,min,max,skill_types}`——建议统一到 `pobr-data` 富版并让 `calc` 复用。

---

## 5. 不可由本对比验证的项

- SANDBOX 与 REAL 的 `pobr-data` 字段级差异已逐文件核对；但 **REAL 数据管线产物（`data/`/`vendor/` 的真实 GGG 数据）** 与 SANDBOX 的自构造 fixture 不可比，移植机制模块时应改用 REAL 真实数据做测试。
- Build Code **字节级兼容**只有用 `examples/demo-bd-test` 的真实 PoB2 code 实测才能确认（批 A 即验证）。
