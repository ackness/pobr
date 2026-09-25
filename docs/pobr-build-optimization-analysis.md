# pobr-build 优化分析（refactor/pobr-build-optimization）

> 调研日期：2026-09-24。基线：`fix/audit-followups`（`bb61e0f`）。
> 目标：解决历史审计遗留的"编排层越界承载引擎语义"问题，同时不破坏 parity 门禁。

## 2026-09-25 implementation status

The sections below are the original investigation, not an unchecked current task list.
`BuildData` lookup methods now live in private `skills`, `equipment`, `passives`
and `rules` modules; its flat public fields and loading contract remain intact.

Wave 3 now has an opt-in `DataCalcCache::new(&data, &options, capacity)`:
immutable borrowing binds all data/options, bounded LRU entries compare complete
Build inputs structurally (including config placeholders), and trigger-source
results can be reused across calls. Compare/report/session paths remain uncached.
The existing text `CalcCache` is also bounded and checks structural equality;
its legacy `peek(u64)` remains hash-only. These APIs do not automatically cache
existing application calls, and no end-user speedup is claimed without measurement.

**Dependency-driven incremental recomputation is not implemented.** It needs
explicit input-to-stage read sets (including negative lookups), immutable stage
outputs and downstream invalidation across conditions, buffs, minions and
triggers. Validate full output, diagnostics and provenance against uncached
mutation sequences before enabling it; arithmetic TraceGraph edges alone are
not a computation dependency graph.

See [backlog reconciliation](backlog-reconciliation.md) for actual fixes,
existing implementations and remaining limits. Main-skill selection and Build
assembly stay in pobr-build; the historical placement table below is not a new
migration mandate.

## 1. 现状量化

`calc_orchestrator/` 目录 **11,688 行**（占 pobr-build src 的 57%）：

| 文件 | 总行 | 生产 | 测试 | 职责 |
|------|------|------|------|------|
| `mod.rs` | 3,869 | 1,121 | 2,748 | 入口 + stage_* 骨架 + 26 个 fn |
| `skill_resolve.rs` | 1,352 | 1,072 | 280 | 主技能解析 + minion spawn + gem 属性/品质/等级 + Kalandra + 槽位加成 |
| `buffs.rs` | 971 | 971 | 0 | herald/aura/curse BuffSpec + spirit reservation + warcry |
| `triggers.rs` | 872 | 664 | 208 | 触发链识别 + support 修正 + 子计算 |
| `collect.rs` | 750 | 750 | 0 | 角色基础/天赋/珠宝半径/keystone/物品·宝石收集 |
| `inject.rs` | 745 | 745 | 0 | 14 个 inject_* 阶段函数 |
| `stat_map.rs` | 567 | 567 | 0 | statmap 映射 + curse/debuff/exposure/buff 修正 |
| `weapon.rs` | 500 | 500 | 0 | 武器/徒手基础贡献 + 本地 mod 解析 |
| `skill_mods.rs` | 494 | 494 | 0 | 技能基础/DoT/尸爆/弩装填/品质/未选集 |
| `prepare.rs` | 421 | 421 | 0 | stage_resolve_main_skill/config/cfg/weapon_bases |
| `defence.rs` | 361 | 361 | 0 | 护甲/闪避/ES/ward/spirit/block 基础 |
| `conditions.rs` | 337 | 337 | 0 | skill_types→flags/conditions + 武器类型条件 |
| `granted_skills.rs` | 309 | 173 | 136 | 物品授予技能合成 |
| `sources.rs` | 104 | 104 | 0 | SourceWriter（写隔离） |
| `context.rs` | 36 | 36 | 0 | CalculationContext |

**关键观察**：
- `mod.rs` 生产代码仅 1,121 行，其中 26 个 fn；真正的骨架（`calculate_with_context` 阶段表）~140 行，已经干净。
- 所有子模块用 `use super::*` 全量导入，**没有显式依赖声明**——这是"越界"的结构性原因：任何模块都能摸到任何东西，没有边界约束。
- `skill_resolve.rs` 是**最大杂烩**：主技能解析、minion spawn（190 行）、gem 属性/品质/等级加成、Kalandra 反射、槽位加成缩放、文本清洗——至少 4 个独立职责挤在一起。
- `triggers.rs` 的 `support_modifiers`（604 行起）实际是**support gem 修正注入**，不是触发逻辑——命名误导。

## 2. 核心问题诊断

### 问题 A：引擎语义 vs 编排职责的边界在哪

历史审计说"orchestrator 承载了本该属于计算引擎的技能解析语义"。逐函数核对后，精确边界是：

| 属于**引擎语义**（应下沉到 pobr-core 或独立层） | 属于**编排**（留在 pobr-build） |
|---|---|
| `judge_group_supports`（support 兼容判定 + addSkillTypes 不动点，vendor CalcActiveSkill.lua:179-214） | 阶段调度顺序（`calculate_with_context` 的 stage_* 调用序列） |
| `resolve_main_skill` + `pick_group_main_skill`/`pick_group_chosen_active`（主技能选择，vendor SkillsTab 语义） | `SourceWriter` 写隔离 + `finish_sources` 消费 |
| `skill_base_modifiers`/`support_modifiers`/`trigger_modifiers`/`buff_skill_specs`（技能组 → 生效 Modifier 的翻译） | `Build` ↔ `Item`/`SocketGroup`/`PassiveTreeSpec` 的装配 |
| `weapon_contribution`/`dual_wield_off_hand_contribution`（武器基础 → WeaponContribution，vendor CalcSetup weaponData） | `stage_build_view` 的 Build 视图变换（Ring3 门控/granted skill 合成/品质折算/nameSpec 回填） |
| `resolve_skill_level_with_gem_bonus`/`gem_property_bonuses`/`additional_gem_levels`（gem 等级加成规则） | `BuildData::load` 的数据装配 |
| `combat_conditions`/`weapon_type_conditions`/`damage_keywords`（条件推导） | `inject_*` 的"把 Modifier 倒进 session" |
| `spawn_minions`（minion 识别 + MinionModifierEntry 提取） | `default_parser_rules` / `diagnose_tree_version` / `passive_jewel_state` 等入口 |

**本质**：pobr-core 的 `CalculationSession` 是"modifier 计算器"，但"给定一个技能组（gem 列表 + 数据表），算出该注入哪些 modifier"这一步——vendor 里叫 `CalcActiveSkill`——整个住在 pobr-build。这导致：
- pobr-core 脱离 pobr-build 算不出真实 build；
- 想单测技能解析语义必须拖 XML/BuildData 全家桶；
- `support.rs` 的 `judge_group_supports` 被 wasm `supportGroupsCompatibleJson` 直接调用，但它依赖 `BuildData`（含文件加载的 granted_effects），不能纯数据化。

### 问题 B：子模块无边界（`use super::*` 全量导入）

11 个子模块全部 `use super::*`，意味着：
- `buffs.rs` 能调用 `weapon.rs` 的 `parse_adds_with_suffix`，`triggers.rs` 能调用 `collect.rs` 的 `gate_parses`——**实际发生了**（triggers.rs:669 `use super::super::test_context` 甚至跨了两层）；
- 函数可见性是 `pub(crate)`，但"哪个模块该拥有哪个函数"没有约束；
- 新函数只能往"看起来最相关"的文件塞，越塞越杂（`skill_resolve.rs` 的 minion spawn 和 Kalandra 反射跟"技能解析"毫无关系）。

### 问题 C：`mod.rs` 测试与生产 2.4:1 倒挂

`mod.rs` 2,748 行测试 vs 1,121 行生产。测试质量高（parity 门禁 + 大量端到端断言），但**测试应该跟着被测代码走**——当前 `ring3_tests`、buff_spec 测试、trigger 测试、weapon 测试全堆在 `mod.rs` 末尾，找测试要翻 3,800 行文件。

### 问题 D：命名与职责错位

- `triggers.rs` 的 `support_modifiers`：support gem 的 statmap 修正注入，跟触发无关；
- `skill_resolve.rs` 的 `spawn_minions`：minion 装配，跟主技能解析无关；
- `collect.rs` 的 `resolve_granted_socket_jewels`/`gate_parses`/`filter_item_parseable`：物品过滤，跟"收集"关系弱；
- `inject.rs` 的 `per_slot_attribute_requirements`/`virtuous_mote_counts`：纯数据查询，不是注入。

## 3. 优化方案（分波次，parity 门禁护航）

### 第一波：零行为变化的纯重组（本分支可做）

目标：不改一行计算逻辑，只移动代码 + 收紧可见性，让边界显式化。

#### 1.1 按职责重排子模块（纯文件移动）

```
calc_orchestrator/
  mod.rs           — 入口 + stage_* 骨架（保持 ~1,100 行生产）
  context.rs       — CalculationContext（不动）
  sources.rs       — SourceWriter（不动）
  prepare.rs       — stage_resolve_main_skill/config/cfg/weapon_bases（不动）
  inject.rs        — inject_* 阶段函数（不动）
  skill/           — 新目录：技能组 → Modifier 的引擎语义
    mod.rs
    resolve.rs     — resolve_main_skill/pick_group_*/resolve_skill_level_*/gem_property_*/additional_gem_levels
    mods.rs        — skill_base_modifiers/support_modifiers/dot_flag/corpse_explosion/crossbow_reload/quality/unselected_set
    triggers.rs    — trigger_modifiers/config_trigger_modifiers/recognize_trigger_config/...
    buffs.rs       — buff_skill_specs/support_buff_specs/herald/warcry/spirit_reservation
    minions.rs     — spawn_minions（从 skill_resolve.rs 拆出）
  item/            — 新目录：装备/珠宝 → Modifier
    mod.rs
    weapon.rs      — weapon_contribution/dual_wield/unarmed/本地 mod 解析
    defence.rs     — defence_base/shield_block/spirit/ward/rolled_defence
    jewels.rs      — stage_inject_jewels/adorned/radius_jewel_grant_modifiers/keystone_mod_map
    filters.rs     — gate_parses/filter_parseable/filter_item_parseable/clean_item_text
  passives.rs      — resolve_passive_nodes/append_granted_passives/passive_effect_copies/keystone_mod_map（从 collect.rs 拆出）
  conditions.rs    — combat_conditions/weapon_type_conditions/damage_keywords/skill_type_bits/flags（不动）
  config.rs        — stage_inject_config_mods/resolve_config 调用（从 mod.rs 拆出）
  granted.rs       — granted_skills.rs + resolve_granted_socket_jewels + resolve_name_spec_gems（物品授予合成）
```

关键约束：**每个文件顶部用显式 `use` 替代 `use super::*`**，把跨模块依赖变成可读清单。

#### 1.2 `mod.rs` 测试外迁

`mod.rs` 的 2,748 行测试按被测对象拆到对应新模块（`skill/resolve.rs` 的测试跟到 `skill/resolve.rs` 的 `#[cfg(test)]`），`mod.rs` 只留入口级 smoke。

#### 1.3 `triggers.rs` 的 `support_modifiers` 归位

`support_modifiers`（604-664 行）移到 `skill/mods.rs`（它就是 support gem 修正注入），`triggers.rs` 只留触发链。

### 第二波：引擎语义下沉（立项再做，parity 门禁护航）

目标：让"技能组 → Modifier"脱离 `Build`/`BuildData`，变成纯数据入参。

#### 2.1 定义 `SkillEnv` 输入契约（pobr-core 新模块或独立 crate）

```rust
// pobr-core/src/skill_env.rs（或新 crate pobr-skill）
pub struct SkillEnv<'a> {
    pub gems: &'a [GemInput],           // (effect_id, level, quality, stat_set_index)
    pub effects: &'a dyn EffectLookup,  // granted_effects 查询接口
    pub stat_sets: &'a dyn StatSetLookup,
    pub gem_quality: &'a dyn QualityLookup,
    pub stat_map: Option<&'a StatMapCatalog>,
    pub parser_rules: Option<&'a CompiledParserRules>,
    // ... 纯数据，无 Build/XML/文件 I/O
}
```

`BuildData` 实现 `EffectLookup`/`StatSetLookup`/`QualityLookup` trait，编排层只做 `Build → SkillEnv` 的适配。

#### 2.2 迁移顺序（按依赖深度）

1. `judge_group_supports`（`support.rs`）→ `SkillEnv` 入参（它已经是半独立的，wasm 直接调用）；
2. `skill_base_modifiers`/`support_modifiers`/`dot_flag`/`corpse_explosion`/`crossbow_reload`/`quality`/`unselected_set`（`skill_mods.rs` + `stat_map.rs` 的映射层）→ `SkillEnv`；
3. `weapon_contribution`/`dual_wield`/`unarmed`（`weapon.rs`）→ 需要 `Build.items` 的装备视图 → 定义 `EquipmentView` trait；
4. `resolve_main_skill`/`pick_group_*`（`skill_resolve.rs`）→ `SkillEnv` + `SocketGroupView` trait；
5. `buff_skill_specs`/`herald`/`warcry`/`spirit_reservation`（`buffs.rs`）→ `SkillEnv` + `BuildView` trait；
6. `spawn_minions`（`skill_resolve.rs`）→ `SkillEnv` + `MinionLookup` trait；
7. `trigger_modifiers`/`config_trigger_modifiers`（`triggers.rs`）→ `SkillEnv` + `TriggerConfigLookup` + 子计算回调（最复杂，最后做）。

#### 2.3 `BuildData` 瘦身

`BuildData` 现在是"所有域的 HashMap 聚合"（1,217 行）。下沉后它变成**适配层**：持有 `GameData` 的查询结果，实现 `SkillEnv` 需要的 trait。`BuildData::load` 保留（编排层入口），但内部按 trait 分域组织。

### 第三波：增量计算与缓存（远期）

- `CalcCache` 已接线到 desktop，但 `calculate_with_data` 路径无缓存（`BuildSnapshot` 文档明确警告其 hash 不覆盖 gem level/quality/jewel radius 等）；
- 触发子计算（`triggers.rs` 的 source rate 子计算）无跨调用缓存，代码里自己记了 defer；
- 真正的增量计算（改一件装备只重算受影响部分）需要 TraceGraph 归因基建 + 依赖图，是大工程。

## 4. 风险与门禁

- **parity_no_regression** 是硬门禁：任何重组/下沉必须 value-for-value 等价；
- `mod.rs` 的 26 个 fn 和 stage_* 调用顺序是行为契约，第一波只动文件归属和 `use` 声明，不动调用序列；
- `use super::*` → 显式 `use` 的转换会暴露隐藏的跨模块依赖（如 `triggers.rs` 用 `collect.rs` 的 `gate_parses`），这些依赖要么归位要么显式声明；
- `Build` 类型目前被 13 个模块直接消费，下沉后应收敛到"编排层装配 + 引擎层 trait"两个面。

## 5. 验收标准（第一波）

- [x] `calc_orchestrator/` 无 `use super::*`（全部显式 `use`，仅 `tests.rs` 保留）；
- [x] `skill_resolve.rs` 拆为 `skill/resolve.rs` + `skill/minions.rs` + `item/kalandra.rs` + `item/slot_bonus.rs`；
- [x] `triggers.rs` 的 `support_modifiers` 移到 `skill/mods.rs`；
- [x] `mod.rs` 测试按被测对象外迁到 `tests.rs`，生产代码 1,089 行（≤1,200）；
- [x] `cargo test -p pobr-build --test parity parity_no_regression` 通过；
- [x] `cargo clippy --lib --tests -D warnings` 零告警。

## 6. 第二波执行记录（已完成，`2206fad`）

**目标**：引擎语义（技能组 → Modifier 的翻译）下沉到 `pobr_core::skill_env`，pobr-build 只留编排薄壳。

**已下沉的语义**（全部走 trait 抽象，零 `Build`/`BuildData` 依赖）：

| 域 | pobr-core 位置 | 关键 trait |
|---|---|---|
| support 兼容判定 + addSkillTypes 不动点 | `skill_env::support` (`judge_group_supports`) | `SupportJudgeLookup` |
| 技能组 → Modifier 纯翻译 | `skill_env::mods` (`skill_base_modifiers`, `dot_flag`, `corpse_explosion`, `crossbow_reload`, `mk_trigger_mod`) | `EffectLookup`, `StatMapLookup` |
| 技能等级/品质/宝石加成 | `skill_env::resolve` (`resolve_skill_level`, `gem_property_bonuses`, `additional_gem_levels`, `support_granted_gem_levels`, `pick_group_main_skill`) | `SkillLevelLookup`, `GemPropertyLookup`, `GemPropertyScanView`, `PassiveNodeLookup`, `GemDefLookup`, `ParserRulesLookup`, `CostTypeLookup` |
| 武器/双持/徒手基础贡献 | `skill_env::weapon` (`weapon_contribution`, `dual_wield_off_hand_contribution`, `unarmed_contribution`, `per_shield_defence_scale`) | `WeaponContributionLookup`, `WeaponItemLookup`, `EquipmentView` |
| Herald/aura/buff 名 + spirit 预留 | `skill_env::buffs` (`herald_skill_names`, `buff_skill_name`, `self_buff_offensive_modifiers`, `spirit_reservation_modifiers`) | `SocketGroupView`, `ReservationDb`/`ReservationLookup` |
| Buff/Warcry/Exposure specs | `skill_env::buff_specs` (`buff_skill_specs`, `support_buff_specs`, `warcry_skill_specs`, `support_modifiers`, `exposure_support_modifiers`, `group_judgement`) | `BuffEnv`, `WarcryEnv`, `StatMapCtx` |
| Stat-map 域通道 | `skill_env::stat_map` (`mapped_stat_modifiers`, `stat_map_data_mapped`, `curse_stat_modifiers`, `debuff_stat_modifiers`, `player_buff_stat_modifiers`, `classify_outcome`) + `buff_stat_map` 模块 | `StatMapCtx`（catalog+mode+record sink，替代 `CalculationContext`） |
| Minion 装配 | `skill_env::minions` (`spawn_minions`) | `MinionEnv`, `MinionSession`（`base_sum` + `add_minion_from_def`） |
| 触发链全链 | `skill_env::triggers` (`trigger_modifiers`, `config_trigger_modifiers`, `recognize_trigger_config`, `find_trigger_source_gem`, `source_cond_matches`, `base_rate_of`) | `TriggerEnv`, `TriggerCtx`, `TriggerSubCalc`（子计算回调） |

**关键设计**：
- `StatMapCtx { catalog, mode, records }`：statmap 引擎函数从消费 `CalculationContext` 改为消费该 ctx；`CalculationContext::stat_map_ctx()` 提供借用适配。
- `TriggerSubCalc`：触发源技能的子计算抽象为 trait 回调；编排层 `OrchestratorSubCalc` 实现（clone build → 指向源 gem → 一层深 `calculate_with_context`）。`group_index: Option<usize>` 表示 detached/测试组（无子计算，回退 `1/use_time`）。
- 组合 trait + `as_*()` upcast：`TriggerEnv`/`BuffEnv`/`WeaponContributionLookup` 等组合多个查询 trait，解决 Rust trait object 只能单 trait 的限制。
- `Build`/`BuildData` 实现所有 `*Lookup` trait（`build.rs`/`build_data.rs`），`Build` 另实现 `EquipmentView`/`SocketGroupView`/`GemPropertyScanView`。

**回归修复**（`2206fad`）：`pick_group_main_skill` wrapper 漏了 builder 路径（`gem_skills` 空、id 来自 `active_skill_id`）的引用回映射，导致 19 个 skills 集成测试主技能解析失败。

**验证**：`cargo test --workspace` 2291 全过；`parity_no_regression` 硬门禁通过；`clippy --all-targets` 零告警。

**仍未做（第三波/可选收尾）**：
- `resolve_main_skill`/`pick_group_chosen_active`/`stage_build_view` 等编排骨架留在 pobr-build（本就是编排职责，返回 `&SocketGroup` 绑死 Build 类型）；
- `BuildData` 按 trait 分域重组（2.3，纯内部结构，可选）；
- 增量计算/缓存（`calculate_with_data` 无缓存、触发子计算跨调用缓存）—— 依赖 TraceGraph 归因基建，远期工程。
