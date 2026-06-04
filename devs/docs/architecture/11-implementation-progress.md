# 实现进度同步

---

## 0. 维护规则

更新时间：2026-06-04（追加：阶段六来源接入三件套 —— 物品（含 implicit/explicit/enchant section 区分）、天赋树节点、技能宝石（主动 / 辅助）的 modifier 来源与归因入口，并行实现后合并；最大抗性 / 硬上限 / over-cap 计算边界原语）

每次实现后必须同步更新本文件：

- 已完成项使用 `[x]`。
- 正在做但未验收完成使用 `[~]`。
- 未开始使用 `[ ]`。
- 每个完成项必须能对应到代码、测试、工具或文档证据。
- 不能把概念设计当作实现完成。

---

## 1. 当前总状态

当前 PoBR 已进入计算核心原型阶段。已具备：

- Rust workspace。
- `pobr-data` / `pobr-core`。
- PoB display/output/breakdown catalog 扫描工具。
- Modifier / ModDb / parser 的最小可测试实现。
- 最小计算闭环。
- SourceId 与 TraceGraph 基础。
- `Life` / `Mana` / 三元素抗性 / `TotalDPS` 的 traced calculation。
- 角色基础值 / 元素抗性惩罚 / 战役奖励的 modifier 入口（`character` / `campaign` 模块）。
- 物品来源接入（`item` 模块）：装备词条按 section（implicit / explicit / enchant）→ 带 `SourceKind::ItemImplicit` / `ItemAffix` / `ItemEnchant` + 槽位归因的 modifier。
- 天赋树来源接入（`passive` 模块）：已分配节点词条 → 带 `SourceKind::PassiveNode` / `AscendancyNode` 归因的 modifier。
- 技能宝石来源接入（`skill_source` 模块）：主动 / 辅助宝石词条 → 带 `SourceKind::SkillGem` / `SupportGem` 归因的 modifier（辅助宝石 `with_parent` 链接被支援技能）。

当前仍未完成：

- PoB 全量属性 parity matrix fixture。
- SkillUseTime。
- 辅助宝石 mana multiplier / more 倍率被支援技能隔离 / skill type gating（`skill_source` 已留结构化 TODO）。
- 伤害转换 / gain-as-extra / 分类型 flat added damage 的 parser（`damage.rs` 已留 TODO；架构已就位）。
- 天赋节点词条由 `PassiveTreeSpec` 直接承载（当前经独立 `AllocatedNode` 装配）。
- 物品 section 由 raw item 文本解析自动切分（当前由调用方分字段提供）。
- PoB Build Code 导入/导出。
- 多语言 runtime crate。
- GUI / CLI / WASM 应用层。

---

## 2. 已完成清单

### Workspace 与基础类型

- [x] Root Cargo workspace。
- [x] `crates/pobr-data`。
- [x] `crates/pobr-core`。
- [x] `tools/sync-pob-catalog`。
- [x] 基础 ID：`StatId` / `ModName` / `ItemBaseId` / `NodeId` 等。
- [x] 基础 enum：`DamageType` / `ClassId` / `ModType` / `ModFlags` / `KeywordFlags` / `SkillTypes`。
- [x] 最小数据结构：`Item` / `Gem` / `SkillEffect` / `PassiveTreeSpec` / `GameData`。

### Modifier 与 ModDB

- [x] `Modifier` / `ModValue` / `ModTag`。
- [x] `Modifier::matches` 支持 flags、keyword flags、condition、damage type、skill type。
- [x] `ModDb::add_mod` / `add_list`。
- [x] `ModDb::sum`。
- [x] `ModDb::more`。
- [x] `ModDb::flag`。
- [x] `ModDb::override_`。
- [x] `ModDb::list`。
- [x] `ModList` parent sum。
- [x] 条件、flags、damage type、multiplier limit 测试。
- [ ] ModDB benchmark。

### Modifier Parser

- [x] 英文基础 form：base / increased / reduced / more / less。
- [x] 攻击/法术 preflag 第一版。
- [x] condition tag：full life。
- [x] multiplier tag：charge scaling。
- [x] damage type tag。
- [x] unsupported line 保留。
- [x] parser cache 第一版。
- [x] 当前已解析核心 stat：life、mana、resistance、accuracy、armour、evasion、energy shield、基础 damage/speed。
- [ ] PoB 全量 modifier cache 生成工具。
- [ ] 多语言反向 modifier parser。

### 最小计算核心

- [x] `Env` / `Actor` / `OutputTable` / `BreakdownTable`。
- [x] `CalculationSession`。
- [x] `perform`。
- [x] life / mana。
- [x] fire/cold/lightning resistance 简化计算。
- [x] armour / evasion / energy shield。
- [x] hit chance。
- [x] armour reduction。
- [x] average hit。
- [x] crit chance / crit multiplier 简化计算。
- [x] attack/action rate 简化计算。
- [x] DPS 简化计算。
- [x] max resistance / floor / overcap（默认 75 / 硬上限 90 / 可由 `Maximum<Element>Resistance` + `MaximumAllElementalResistances` 提升 / 负抗性无下限 / over-cap 输出，见 `offence.rs::resolve_resistance`）。
- [x] DamageComponent vector（`calc/damage.rs::DamageComponent` + `calculate_components`，按 5 种伤害类型拆分非暴击击中分量，纯物理路径回归一致，见 `tests/damage_components.rs`）。
- [ ] SkillUseTime。
- [ ] DamageComponent vector。
- [ ] EHP / max hit。
- [ ] ailment / debuff DoT。

### PoB Parity Catalog

- [x] `DisplayStatId`。
- [x] `DisplayStatCategory`。
- [x] `ParityStatus`。
- [x] `PobCatalog`。
- [x] `sync-pob-catalog scan`。
- [x] `sync-pob-catalog check`。
- [x] 抽取 `BuildDisplayStats.lua` display stats。
- [x] 抽取 `CalcSections.lua` output/breakdown 引用。
- [x] 抽取 `CalcOffence.lua` / `CalcDefence.lua` / `Calcs.lua` output 写入。
- [x] 抽取 `Data.lua` power stat list。
- [ ] 提交固定 PoB catalog fixture。
- [ ] PoB catalog CI gate。

### Source Attribution 与 Trace

- [x] `SourceKind`。
- [x] `SourceId`。
- [x] `ModifierSource`。
- [x] `Modifier::with_origin`。
- [x] `ModContribution`。
- [x] `ModDb::contributions`。
- [x] `TraceGraph` / `TraceNode` / `TraceEdge`。
- [x] `TraceOperation`。
- [x] `ModDb::sum_traced`。
- [x] `TraceGraph::source_ancestors`。
- [x] `calculate_minimal_traced` 第一版。
- [x] traced outputs：`Life` / `Mana` / `FireResist` / `ColdResist` / `LightningResist`。
- [x] traced DPS formula tree（`TotalDPS`）。
- [ ] `TraceMode`。
- [x] `AttributionReport`（`attribution.rs`：`AttributionRequest` / `AttributionReport` / `AttributionEntry`，依架构文档 10 §6-7，`ModDb::filtered` 驱动 marginal 重算，见 `tests/attribution.rs`）。
- [x] direct/marginal/interaction attribution（direct 读 trace 祖先链 / marginal = final − without_source / interaction = final − baseline − Σmarginal）。
- [ ] `AttributionRequest.build: BuildSnapshot` / `selected_skill`（高层类型未实现，当前用重算闭包替代）。

### 来源接入（阶段六）

- [x] 物品：`EquipmentSlot`（10 个稳定槽位 ID）。
- [x] 物品：`ingest_item(slot, &Item)` —— 词条文本 → 带 `SourceKind::Item` + `slot` + `raw_text` 归因的 modifier。
- [x] 物品：无法解析词条收集进 `ItemIngest::unsupported`（不报错）。
- [x] 物品：`CalculationSession::add_item(slot, &Item)` 注入最小计算闭环。
- [x] 物品：贡献可回溯到具体装备槽（`contributions` origin slot / source id 测试）。
- [x] 物品：implicit / explicit / enchant section 区分（`ItemModSection` → `ItemImplicit` / `ItemAffix` / `ItemEnchant`，`SourceId.id = item.<slot>.<section>`）。
- [x] 天赋树来源接入（`passive::ingest_passive_nodes` + `AllocatedNode`，`SourceKind::PassiveNode` / `AscendancyNode`，`session.add_passive_nodes`）。
- [x] 技能宝石来源接入（`skill_source::ingest_gem` + `GemModSource`，主动 `SkillGem` / 辅助 `SupportGem` + `with_parent`，`session.add_gem` / `add_skill_gem` / `add_support_gem`）。
- [x] raw 物品文本块解析（`item_text::parse_item_text` → 切分 implicit/explicit/enchant 段为 `Item`，喂入 `ingest_item`）。
- [ ] 辅助宝石 mana multiplier / more 倍率被支援技能隔离 / skill type gating（结构化 TODO 见 `skill_source.rs`）。
- [ ] 天赋节点词条由 `PassiveTreeSpec` 自动装配（当前由调用方经 `AllocatedNode` 提供）。

---

## 3. 当前实现步骤

### 已完成：阶段六来源接入三件套（并行实现 + 合并）

物品 section 区分 / 天赋树 / 技能宝石三个来源由三个子任务并行实现（各自独立
worktree 提交），再合并到 master。三者范式对称于物品来源接入：来源 → `parse_mod`
解析词条 → 挂 `SourceKind` + 稳定 `SourceId` + `raw_text` 归因 → 注入 `ModDb`，
无法解析词条收集进 `unsupported`。合并后 CI gate（test/fmt/clippy）全绿，67 测试通过。

- 物品 section：`pobr-core/src/item.rs::ItemModSection`，`Item` 增 `implicit_texts` / `enchant_texts`，测试 `tests/item_source.rs`。
- 天赋树：`pobr-core/src/passive.rs::ingest_passive_nodes` + `AllocatedNode`，测试 `tests/passive_source.rs`。
- 技能宝石：`pobr-core/src/skill_source.rs::ingest_gem` + `GemModSource`，测试 `tests/skill_source.rs`。
- 入口：`CalculationSession::add_item` / `add_passive_nodes` / `add_gem`（+ `add_skill_gem` / `add_support_gem`）。

### 已完成：物品来源接入（阶段六第一切片）

实现见 `pobr-data/src/item.rs::EquipmentSlot`、`pobr-core/src/item.rs::ingest_item`、
`calc/session.rs::CalculationSession::add_item`，测试见 `tests/item_source.rs`。

填上了原 `add_modifier_texts` 路径「解析丢归因」的洞：装备词条经 parser 解析后，
统一挂上 `SourceKind::Item` + 槽位 `slot` + 原始 `raw_text` 归因，复用既有
`with_origin` / `contributions` / `sum_traced` 基建，使最终输出可 source-level
回溯到具体装备槽。

- [x] `EquipmentSlot`（武器 ×2 / 头 / 身 / 手 / 脚 / 项链 / 戒指 ×2 / 腰带），`id()` 稳定字符串。
- [x] `ingest_item` 解析词条并挂槽位归因，`stat_id` / `mod_type` 由 `with_origin` 回填。
- [x] 无法解析词条收集进 `unsupported`。
- [x] `add_item` 端到端注入最小计算（life inc/base 管线可验证）。
- [x] 贡献回溯断言（origin slot == 槽位、source id == `item.<slot>`）。
- [ ] section 区分（implicit / explicit / enchant）需 `Item` 先拥有词条分段。

### 已完成：DPS trace formula tree

实现见 `calc/offence.rs::total_dps_traced` + `more_factor_traced`，测试见
`tests/calc_minimal.rs::traced_minimal_calculation_links_total_dps_to_damage_speed_and_accuracy_sources`。

- [x] `TotalDPS` traced output。
- [x] 追踪 base hit average（`CharacterBase` source node）。
- [x] 追踪 damage INC modifier sum。
- [x] 追踪 damage MORE factor（`more_factor_traced`，逐项 source）。
- [x] 追踪 crit average factor（crit chance / multiplier BASE）。
- [x] 追踪 total hit average。
- [x] 追踪 action rate（speed INC / MORE）。
- [x] 追踪 hit chance（accuracy 链 + `enemy.evasion` EnemyConfig source）。
- [x] 追踪 `TotalDPS final = total_hit_avg * action_rate * hit_chance`。
- [x] 测试可从 `TotalDPS` 回溯到装备 / 天赋 / 支援宝石 / 敌方配置来源。

### 已完成：角色基础值 / 元素抗性惩罚 / 战役奖励 modifier 入口

实现见 `character.rs`（`CharacterBase`）与 `campaign.rs`（`CampaignProgress` /
`CampaignReward` / `CampaignState`），入口经 `CalculationSession::add_modifiers`
注入；测试见 `tests/character_base.rs` 与 `tests/campaign.rs`。资料对照
`agent-docs/attributes.md` 与 `agent-docs/campaign-rewards.md`。

- [x] `CharacterBase`：等级 + 属性派生 life/mana/accuracy 的 `BASE` modifier（`CharacterBase` 归因）。
- [x] `CampaignProgress`：元素抗性惩罚表（Act1=0 … Endgame=-60），作用于火/冰/电三抗、混沌不降，`CampaignReward` 归因 `campaign.resistance_penalty`。
- [x] `CampaignReward`：固定抗性奖励（Head of the Winter Wolf / Sisters of Garukhan / The Flame Core），稳定 source id `campaign.<reward>`。
- [x] `CampaignState`：进度惩罚 + 奖励扁平化为单一 modifier 列表。
- [x] `CalculationSession::add_modifiers`：入口注入并参与最小计算闭环（life / 元素抗性可端到端验证）。
- [ ] 属性派生 bonus 改由 total 属性（树 + 装备）的 inherent attribute pass 计算。
- [ ] 纹身 / Venom Draught / Seven Pillars 等多选 / 可重选奖励、Spirit / threshold 类 modifier。
- [ ] `Rakiata's Lesson` 等机制型奖励（护甲作用于元素击中 / 偏转 / ES recharge delay）。

---

## 4. 验证命令

当前统一验证命令：

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'pobr-cargo-target'; cargo fmt --check
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'pobr-cargo-target'; cargo test --workspace
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'pobr-cargo-target'; cargo clippy --workspace --all-targets -- -D warnings
```

