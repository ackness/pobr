# 02 · 环境 / 来源装配 (Setup)

**Rust 模块**：`pobr-core::calc` — `setup_env.rs` / `env` / `actor` / `skill_source.rs` / `item.rs` / `item_text.rs` / `passive.rs`
**对照**：PoB2 `CalcSetup.lua` / `ConfigOptions.lua` / `Misc.lua` / `Item.lua` / `CalcTools.lua`
**agent-docs 交叉**：§二 CalcSetup：环境与来源装配

## 总评

总体方向正确，来源接入（item/passive/gem 三通道带 SourceId 归因）的范式与 PoB2 的 modList 装配等价；怪物缩放表/档位常量（accuracy/armour/evasion/抗性/DamageTaken/DPSMult）已对照 PoB2 `Misc.lua` + `ConfigOptions.lua` 落地。归因/解析骨架本身无正确性问题。但存在一处会导致 Boss 进攻数值系统性偏高的 CRITICAL 漏算，及若干 HIGH/MEDIUM 装配语义问题。

---

## 02-01 · Boss 元素穿透（Pinnacle +3% / Uber +8%）完全未注入玩家 modDB — CRITICAL

> **✅ 已修复（2026-06-09）**：`setup_env.rs::setup_enemy` 新增 `inject_boss_penetration`，`defaults.pen != 0` 时向 `env.player.mod_db` 注入 `ElementalPenetration BASE = pen`（fire/cold/lightning 共享读取，不碰混沌/物理），归因 `EnemyConfig:pinnacle_boss_pen`。回归测试见 `tests/enemy_mod_db.rs`（Pinnacle=3 / Uber=8 / None=0）。pobr-core 601 + pobr-build 117 测试全过，无 parity 回归。

**PoB2 行为**：`ConfigOptions.lua` `enemyIsBoss` 段：Pinnacle/Uber 给 `enemyLightning/Cold/FirePen` 设占位 `data.misc.pinnacleBossPen(=15/5=3)` / `uberBossPen(=40/5=8)`，再由 `enemyFirePen/...` apply 注入 enemyModList 的 `<Element>Penetration`；PoB2 把 boss 自带穿透折进玩家有效伤害（减抗）。PoBR 文档自述 `tier.pen()` 应注入 player modDB 的 `<Element>Penetration BASE`。

**PoBR 现状**：`setup_env.rs:83-176` `inject_enemy_mods` 注入了 accuracy/evasion/armour/抗性/DamageTaken/boss debuff 抗，但从未读取或注入 `defaults.pen` / `tier.pen()`。`EnemyTierDefaults::compute`（`monster.rs:614-635`）算出了 pen 字段，`setup_env.rs:58-80` `setup_enemy` 只构造 enemy actor、对 player.mod_db 零注入。grep 全仓 setup 路径无任何 `<Element>Penetration` 写入玩家侧（仅 `offence.rs:705` 在读取阶段消费）。文档注释（`setup_env.rs:14`）声称会注入但代码缺失。
- `crates/pobr-core/src/calc/setup_env.rs:58`
- `crates/pobr-core/src/calc/setup_env.rs:83`
- `crates/pobr-core/src/calc/setup_env.rs:14`
- `crates/pobr-data/src/monster.rs:569`

**修复方案**：在 `setup_enemy`（或 `session.setup_enemy`）中，当 `defaults.pen != 0` 时向 `env.player.mod_db` 注入 `FirePenetration/ColdPenetration/LightningPenetration BASE = pen`（或统一 `ElementalPenetration BASE`），归因 `SourceKind::EnemyConfig.id="pinnacle_boss_pen"`。注意是注入 **player** 而非 enemy db（`offence.rs:679-716` 从玩家 db 读穿透）。补一条回归测试：Pinnacle 档下 player `ElementalPenetration` sum == 3。

---

## 02-02 · setup_enemy 直接覆写 env.enemy，破坏 enemyDB:AddList 增量装配语义 — HIGH

**PoB2 行为**：`CalcSetup.lua:682-691`：enemyDB 先 `initModDB` + `NewMod(Accuracy...)`，随后 `env.enemyDB:AddList(build.configTab.enemyModList)` 与 `partyTab.enemyModList` 增量叠加——所有 enemy mods（boss 抗、物理减伤 `enemyPhysicalReduction`、用户自定义 enemy 词条、曝光等）汇入同一 modDB，顺序无关、可叠加。

**PoBR 现状**：`setup_env.rs:77-79` `let mut enemy = Actor::new(...); inject_enemy_mods(...); env.enemy = enemy;` —— 直接用新 Actor **整体替换** `env.enemy`，丢弃此前可能已注入 `env.enemy.mod_db` 的任何 mod。`calc_orchestrator.rs:493` 在 `setup_enemy` 之后才追加曝光（顺序勉强 OK），但任何在 `setup_enemy` 之前注入 enemy 的来源（或多次调用 `setup_enemy`）会被静默清空。
- `crates/pobr-core/src/calc/setup_env.rs:77`
- `crates/pobr-build/src/calc_orchestrator.rs:493`

**修复方案**：改为「先保留/合并已有 `env.enemy.mod_db`，再 `add_mod` 增量注入」而非整体替换 actor；或明确文档化 `setup_enemy` 必须最先调用且仅一次，并把 base 标量与 mod 注入解耦。同时确认 config 的 `enemyPhysicalReduction`（`ConfigOptions.lua:2144` `PhysicalDamageReduction BASE val`）有进入 enemy db——当前 setup 段未见注入物理减伤。

---

## 02-03 · Boss 自身 debuff-抗 mod 缺少 Condition:Effective 门控 — MEDIUM

**PoB2 行为**：`ConfigOptions.lua:2000-2087`：`CurseEffectOnSelf`/`ExposureEffectOnSelf`/`SlowEffectOnSelf` MORE -50、`PoiseThreshold` MORE 500、`Condition:Unique/RareOrUnique` 等全部带 `{ type='Condition', var='Effective' }` 条件——只有在「敌人处于被有效作用」口径下才生效。

**PoBR 现状**：`setup_env.rs:141-172` 把这些 boss debuff-抗注入为**无条件** MORE/Flag（`push_enemy_number` 不附带任何 `ModTag::Condition`）。`ModTag::Condition` 机制存在（`modifier.rs:39-146`），但 setup 未使用。当下游以 `mode_effective=false` 求值时，PoB2 会抑制这些 mod，Rust 不会，导致非有效口径下诅咒/曝光/减速对 boss 的有效度被错误削弱。
- `crates/pobr-core/src/calc/setup_env.rs:141`
- `crates/pobr-core/src/modifier.rs:39`

**修复方案**：给 boss debuff-抗 mod 附加 `ModTag::Condition{var:"Effective", negated:false}`，并确保 `CalcConfig` 在 `mode_effective` 时置 `Condition:Effective=true`。若 PoBR 暂不建模 Effective 口径切换，至少在文档标注此为有意简化（恒等于 `Effective=true`）。

---

## 02-04 · Standard Boss +30 元素抗被硬注入为不可覆盖 BASE（应为 UI 占位）— MEDIUM

**PoB2 行为**：`ConfigOptions.lua:1997-2014` Boss 档：`defaultEleResist=30` 只通过 `varControls['enemyFireResist']:SetPlaceholder(30)` 设为**占位值**，真正注入是后续 `enemyFireResist` 控件 apply 的用户值（可为 0 或任意）。即 Boss 的 30 抗是默认而非强制。Pinnacle/Uber 同理占位 50。

**PoBR 现状**：`setup_env.rs:96-118` 当 `elemental_resist!=0` 直接 `push_enemy_number` `FireResist/ColdResist/LightningResist BASE = 30/50`，无法被「用户配置的 enemy 抗为 0」覆盖（因 `setup_enemy` 还会覆写整个 enemy db，见 02-02）。对于希望模拟「破抗后 boss 0 抗」或自定义 boss 抗的场景会偏差。
- `crates/pobr-core/src/calc/setup_env.rs:96`
- `crates/pobr-data/src/monster.rs:525`

**修复方案**：把档位元素抗作为「默认值」而非强制注入：若调用方提供了显式 enemy 抗配置则用配置值，否则回退档位默认。配合 02-02 的增量装配修复一并处理。

---

## 02-05 · item quality 局部 more 映射过粗：防具应分 armour/evasion/ES；首饰/腰带未建模 — MEDIUM

**PoB2 行为**：`Item.lua` `BuildModListForSlotNum`：武器 quality 作用于 physical min/max（`*(1+quality/100)`，1751-1756）；防具 quality 分别作用于 `armourData.Armour/Evasion/EnergyShield` 各自字段（1812-1819），而非一个统一 LocalDefencesMore。首饰/腰带 quality 经催化剂影响对应词条强度。

**PoBR 现状**：`item.rs:48,174-192` 把所有防具 quality 统一注入单一 `LocalDefencesMore` More modifier，依赖下游对 armour/evasion/ES 三者一致放大；首饰/腰带（`SlotCategory::Accessory`）`quality_mod_name` 返回 None，完全跳过（`item.rs:81`，已标 TODO）。若某防具只有 armour 无 evasion，统一 more 仍正确（0 不放大），但 ES 基底是否与 armour 走同一 `LocalDefencesMore` 通道需核对 `defence.rs`；首饰催化剂缺失是真实功能缺口。
- `crates/pobr-core/src/item.rs:174`
- `crates/pobr-core/src/item.rs:48`
- `crates/pobr-core/src/item.rs:81`

**修复方案**：确认 defence 管线 `LocalDefencesMore` 是否同时正确放大 armour/evasion/ES 的 per-slot 基底（与 PoB2 分字段一致）；首饰/腰带催化剂 quality 建模列为后续（当前 None 是安全降级，保持 TODO 即可）。优先级低于穿透/装配问题。

---

## 02-06 · 支援宝石 more 隔离仅按 SkillTypes 交集，未覆盖 exclude/add skill types — LOW

**PoB2 行为**：`CalcTools.lua` `canGrantedEffectSupportActiveSkill`：除 `requireSkillTypes` 交集外，还检查 `addSkillTypes` 与 `excludeSkillTypes`（被排除类型则不能支援），并有 `minionTypes` 等附加条件。

**PoBR 现状**：`skill_source.rs:379-391` `can_support` 只实现 `require_skill_types` 交集判定，`SupportGemSpec(170-219)` 无 exclude/add skill types 字段。对当前已覆盖的支援宝石够用，但对带排除类型的支援宝石（不少）会错误地判为可支援。
- `crates/pobr-core/src/skill_source.rs:379`
- `crates/pobr-core/src/skill_source.rs:201`

**修复方案**：扩展 `SupportGemSpec` 增加 `exclude_skill_types`/`add_skill_types`，`can_support` 加入排除判定（active 命中 exclude 即 `Err`）。属功能完备性增量，非数值方向错误，可随支援宝石数据完善推进。
