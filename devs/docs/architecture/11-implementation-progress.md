# 实现进度同步

---

## 0. 维护规则

更新时间：2026-06-04（追加：TotalDPS trace 公式树、角色基础值 / 元素抗性惩罚 / 战役奖励 modifier 入口）

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

当前仍未完成：

- PoB 全量属性 parity matrix fixture。
- `AttributionReport`。
- Max resistance / overcap / floor。
- SkillUseTime。
- DamageComponent vector。
- 物品、天赋、技能来源接入。
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
- [ ] max resistance / floor / overcap。
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
- [ ] `AttributionReport`。
- [ ] direct/marginal/interaction attribution。

---

## 3. 当前实现步骤

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

