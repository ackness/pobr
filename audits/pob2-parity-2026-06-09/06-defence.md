# 06 · 防御与 EHP

**Rust 模块**：`pobr-core::calc` — `defence.rs` / `ehp.rs` / `survivability.rs` / `stat_boundary.rs`
**对照**：PoB2 `CalcDefence.lua` / `CalcOffence.lua`（leech）/ `Data.lua`
**agent-docs 交叉**：§六 CalcDefence（生命/护盾/护甲/闪避/抗性/EHP）

## 总评

总体方向正确，常量基本对齐 PoB2（ArmourRatio=10、ES 充能基础 12.5%/s、AvoidChanceCap=75、BlockChanceCap=90、Leech 上限/EffectiveMaxDamageForLeech=40000、充能默认 3 层/15s 等逐字核对一致）。命中/被命中公式（hit_chance / monster_hit_chance）、armour_reduction、reservation、regen+RecoveryRateMod、recoup、stat_boundary 与 PoB2 一致。

但 EHP/max-hit 子系统存在若干造成数值偏差的真实问题，集中在「承受乘数折算口径与 WhenHit 漏算」「元素走护甲时 armour DR 用 post-resist 而非 raw」「DR cap 与 flat DR 硬编码、漏 WhenHit/ElementalDamageReduction」。这些会让 EHP/max-hit 与 PoB2 面板系统性偏离（多数偏乐观）。ES 充能延迟的 Faster 词条把 BASE/INC 混用是方向性 bug。

---

## 06-01 · EHP/max-hit 漏算 DamageTakenWhenHit 承受乘数（受击专属减伤被忽略）— HIGH

**PoB2 行为**：PoB2 max-hit 路径用 `output[type..'AfterReductionTakenHitMulti'] = takenMult × spellSuppressMult × deflectMulti`，其中 `takenMult(=type..'TakenHitMult')` 来自 `applyDmgTakenConversion` 的 `baseTakenInc/baseTakenMore = Sum/More('DamageTaken', type..'DamageTaken', 'DamageTakenWhenHit', type..'DamageTakenWhenHit')`（`CalcDefence.lua:371-377, 2422-2443, 3609`）。即受击 EHP 必须包含 WhenHit 系列。

**PoBR 现状**：`perform.rs` 用于 max-hit 的 `damage_taken_mult` 闭包只取 `'DamageTaken'` / `'<Type>DamageTaken'`（+ 元素时 `'ElementalDamageTaken'`），完全不含 `*DamageTakenWhenHit`（`perform.rs:209-228`）。真正含 WhenHit 的 `taken_mult_for_type` / `calc_taken_multi_suite`（`defence.rs:476-592`）只写入 `taken_multi_*` 面板字段（`perform.rs:294-299`），从未折进 EHP/max-hit。
- `crates/pobr-core/src/calc/perform.rs:209-234`
- `crates/pobr-core/src/calc/defence.rs:476-506`
- `crates/pobr-core/src/calc/perform.rs:294-299`

**修复方案**：把 max-hit 的承受乘数统一改用 `taken_mult_for_type`（已含 WhenHit），删除 `perform.rs` 内重复且不全的 `damage_taken_mult` 闭包；让 EHP 与面板 `taken_multi_*` 共用同一函数，避免双账本与 WhenHit 漏算。注意 PoB2 还按攻击/法术分 `AttackTakenHitMult/SpellTakenHitMult`（2424-2431），若后续要分上下文需扩展。

---

## 06-02 · 元素走护甲（Armour applies to <Element>）时 armour DR 用 post-resist 伤害，PoB2 用 raw — HIGH

**PoB2 行为**：PoB2 计算 armour DR 百分比时用该类型 incoming(raw, 抗性前) 伤害：`armourReduct = armourReduction(effectiveAppliedArmour, damage)`，damage 为 `baseDmg×takenAs`（抗性前）；max-hit 推导同样写明 `armourDR = AppliedArmour/(AppliedArmour + ArmourRatio*RAW*DamageConvertedMulti)`，RAW 为抗性前（`CalcDefence.lua:2375, 3626`）。armour 与 resistance 是两个独立相乘的减伤层，armour% 不基于抗性后伤害。

**PoBR 现状**：`element_max_hit_with_armour` 把抗性后伤害喂给 armour_reduction：`post_resist = hit*res_taken; armour_part = 1 - armour_reduction(armour, post_resist)`（`ehp.rs:109-127`）。`post_resist < raw` → 估出的 armour DR% 偏高 → max-hit 偏乐观，与 PoB2 不一致。
- `crates/pobr-core/src/calc/ehp.rs:109-127`

**修复方案**：把 armour_reduction 的 hit 参数从 `post_resist` 改回 raw 的 hit（迭代中的当前 H），即 `armour_part = 1 - armour_reduction(armour, hit)`，再与 `res_taken` 相乘。这样 armour DR% 基于抗性前伤害，匹配 PoB2 的两层独立相乘模型。

---

## 06-03 · ES 充能延迟把 EnergyShieldRechargeFaster 的 BASE(秒) 与 INC(%) 来源混用 — MEDIUM

**PoB2 行为**：`rechargeBase = Override('EnergyShieldRechargeBase') or (EnergyShieldRechargeDelay + Sum('BASE','EnergyShieldRechargeFaster'))`；`output.EnergyShieldRechargeDelay = rechargeBase / (1 + Sum('INC','EnergyShieldRechargeFaster')/100)`（`CalcDefence.lua:1762-1763`）。BASE 词条是加到基础 4 秒上的秒数，INC 词条才是按 `(1+inc/100)` 缩短延迟，两者不同 ModType。

**PoBR 现状**：`calc_es_recharge` 只读 `EnergyShieldRechargeFaster` 的 BASE 之和，却把它当成百分比做 `delay = 4 / (1 + faster/100)`（`defence.rs:297-306`）。既漏了 BASE 应加到分子（`rechargeBase = 4 + faster_base`），又把 BASE 误用作 INC 的分母缩放，方向性错误。
- `crates/pobr-core/src/calc/defence.rs:295-306`

**修复方案**：按 PoB2 拆两路：`base_seconds = 4 + Sum(Base, 'EnergyShieldRechargeFaster')`；`delay = base_seconds / (1 + Sum(Inc, 'EnergyShieldRechargeFaster')/100)`。可选支持 `Override('EnergyShieldRechargeBase')`。同时把硬编码 `4.0` 对齐 `data.misc.EnergyShieldRechargeDelay=4`。

---

## 06-04 · 物理/各类型 DR 上限硬编码 0.9，未读 DamageReductionMax 词条 — MEDIUM

**PoB2 行为**：DR 上限可被词条提高：`output.DamageReductionMax = Max('DamageReductionMax') or data.misc.DamageReductionCap(角色=90)`；每类型 `output[type..'DamageReductionMax'] = min(Max(type..'DamageReductionMax') or Cap, 全局)`（`CalcDefence.lua:1862-1865`）。armour+flat DR 求和后 clamp 到该可变上限（2381, 396）。

**PoBR 现状**：`physical_taken_fraction_overwhelm` 与 `element_max_hit_with_armour` 把减伤上限写死 0.9（`clamp(0.0,0.9)` / `clamp(0.1,1.0)`），`physical_pdr_fraction` 也把 flat PDR clamp 0.9（`ehp.rs:57,117`；`perform.rs:669-677`）。无 DamageReductionMax 词条的 build 数值正确，但含 +Maximum Damage Reduction 的 build 会被错误截断。
- `crates/pobr-core/src/calc/ehp.rs:51-59`
- `crates/pobr-core/src/calc/ehp.rs:109-127`
- `crates/pobr-core/src/calc/perform.rs:669-677`

**修复方案**：在 perform 层把 `DamageReductionMax = (max('DamageReductionMax') or 90)` 及各类型上限读出，作为参数传入 `physical_max_hit_*`/`element_max_hit_*`，替换硬编码 0.9；保持默认 90 不回归。

---

## 06-05 · Flat 伤害减免漏 <Type>DamageReductionWhenHit 与 ElementalDamageReduction — MEDIUM

**PoB2 行为**：base 类型 DR = `Sum('BASE', type..'DamageReduction', isElemental and 'ElementalDamageReduction')`；WhenHit DR = `baseDR + Sum('BASE', type..'DamageReductionWhenHit')`，max-hit/受击路径优先用 WhenHit 值（`CalcDefence.lua:1867-1875, 378, 428, 3621`）。

**PoBR 现状**：`physical_pdr_fraction` 只 `Sum('PhysicalDamageReduction')`（`perform.rs:669-674`），没有 `PhysicalDamageReductionWhenHit`；`ResistanceSuite` 也没有元素 flat DR 字段，故 `ElementalDamageReduction` / `<Ele>DamageReductionWhenHit` 全部漏算。元素减伤只走 resist。
- `crates/pobr-core/src/calc/perform.rs:669-677`
- `crates/pobr-core/src/calc/ehp.rs:16-24`

**修复方案**：`physical_pdr` 改为 `Sum('PhysicalDamageReduction') + Sum('PhysicalDamageReductionWhenHit')`（受击口径）；`ResistanceSuite` 增加各元素 flat DR 字段，元素 max-hit 在 resist 之外再叠加 `(Base<Ele>DamageReduction + ElementalDamageReduction + WhenHit)` 的 flat DR 层，对齐 PoB2 reductMult。

---

## 06-06 · TakenMultiSuite 持续/反射上下文与 PoB2 三分法未完全对齐 — LOW

**PoB2 行为**：PoB2 区分 TakenHit / OverTime / Reflect，且 hit 进一步分 Attack/Spell/Average 上下文（`AttackTakenHitMult/SpellTakenHitMult`），并叠 `spellSuppressMult/deflectMulti`（`CalcDefence.lua:2422-2443`）。

**PoBR 现状**：`taken_mult_for_type` / `taken_mult_over_time` 结构正确（WhenHit vs OverTime 分开，元素叠 `ElementalDamageTaken*`），但只到 hit/overtime 两分，没有 Attack/Spell 细分，也没有 suppress/deflect 因子。`calc_taken_multi_suite` 的 `elemental_when_hit` 字段仅含纯 Elemental 贡献、与各类型字段语义重叠，易误用。
- `crates/pobr-core/src/calc/defence.rs:476-592`

**修复方案**：短期无需改（PoE2 已移除 spell suppression，deflect 用得少）；中期若要 Attack/Spell 分上下文 max-hit，再扩展 `*AttackTakenHitMult/*SpellTakenHitMult`。建议给 `elemental_when_hit` 字段加注释明确「不要与 fire/cold/lightning_when_hit 叠加」以防调用方重复计入。

---

## 06-07 · Leech 面板速率为近似模型，非 PoB2 LeechRateBase × instances — INFO

**PoB2 行为**：`LifeLeechInstanceRate = Life × data.misc.LeechRateBase(=0.02) × mod('LifeLeechRate')`；`LifeLeechRate = LifeLeechInstances × InstanceRate`；最终 `min(rate, MaxLifeLeechRate=Life×20%) × RecoveryRateMod`（`CalcOffence.lua:4706-4734, Data.lua:201`）。instance_total 受 `MaxLifeLeechInstance=Life×10%` 限。

**PoBR 现状**：`calc_leech` 用 `display_rate = min(instance_total × (max_rate/max_instance=2), rate_cap)` 反推（`survivability.rs:416-447`），不读 `LeechRateBase=0.02` 也不读 `LifeLeechRate` inc/more 与 instances 数。常量（20%/10%/40000）正确，但速率折算口径与 PoB2 不同。代码注释已标注为近似。
- `crates/pobr-core/src/calc/survivability.rs:416-447`
- `crates/pobr-core/src/calc/survivability.rs:383-397`

**修复方案**：若要面板与 PoB2 一致：引入 `LEECH_RATE_BASE=0.02`，`instance_rate = pool×0.02×(1+LifeLeechRate_inc/100)×more`，`total = instances×instance_rate`，再 `min(MaxRate)×RecoveryRateMod`。需 offence 侧传入 instances 与 LifeLeechRate 乘数；当前作面板近似可接受。
