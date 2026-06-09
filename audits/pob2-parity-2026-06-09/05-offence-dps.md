# 05 · 命中 / 暴击 / 异常 / DPS

**Rust 模块**：`pobr-core::calc` — `crit.rs` / `ailment.rs` / `offence.rs`
**对照**：PoB2 `CalcOffence.lua`（命中 L3681-3838 / 异常 L4841-5523 / 速度·DPS L5124-5192 等）
**agent-docs 交叉**：§五 命中/暴击/异常/DPS 组装（5.1 命中率 / 5.2 暴击 / 5.3 异常 / 5.4 DPS 总装）

## 总评

暴击管线（`crit.rs`）对 PoB2 `CalcOffence.lua` L3681–3838 对齐度非常高——cap、命中降级二次检定、Lucky/Bifurcate/Inevitable 几何级数 less 爆伤、敌方 SelfCrit*、NoCritMultiplier、`(100+ΣBASE)×(1+inc)×Πmore` 爆伤公式逐字对应，面板/归因双路径共用单一实现，几乎无方向性偏差。`offence.rs` 命中/速度/抗性/穿透/护甲链路基本忠实，但 traced 与非 traced 在「扣敌方格挡/分类型减伤」上口径分叉。异常子系统（`ailment.rs`）问题最集中：暴击加权用裸暴击率、magnitude 双重/错位计数、感电漏 effectMod、RollAverage 未接入。

---

## 05-01 · 异常暴击加权用裸暴击率，未做 over-stacking 修正（ailmentCritChance）— HIGH

**PoB2 行为**：`CalcOffence.lua` L5144：`local ailmentCritChance = 100 * (1 - m_pow(1 - output.CritChance / 100, m_max(globalOutput[ailment.."StackPotential"], 1)))`，再传入 `calcAilmentDamage(ailment, ailmentCritChance, hitAvg, critAvg)`（L5149）。即用于异常 base 加权的「暴击份额」是按叠层潜力放大后的「至少一层暴击」概率，而非单次命中暴击率。L4908–4914 的 `chanceFromHit/chanceFromCrit/baseVal` 用的就是这个放大后的 sourceCritChance。

**PoBR 现状**：`ailment.rs:91` `weighted_source_damage` 直接用 `source.crit_chance.clamp(0,1)`（单次命中暴击率 fraction），`AilmentSource.new`（`ailment.rs:54-73`）也只存原始 `crit_chance`。没有任何 `100·(1-(1-c)^max(SP,1))` 的 over-stacking 修正。`perform.rs:511` 取的 `crit_chance` 是单次命中值。
- `crates/pobr-core/src/calc/ailment.rs:85-105`
- `crates/pobr-core/src/calc/ailment.rs:51-73`
- `crates/pobr-core/src/calc/perform.rs:506-512`

**修复方案**：在异常 base 加权前按 PoB2 把单次暴击率经叠层潜力放大：`ailment_crit_chance = 1 - (1 - crit)^max(stack_potential, 1)`，再作为 `chance_on_crit` 的权重（fraction）。无叠层时 SP=1 退化为单次暴击率，保持向后兼容。需把 StackConfig/StackPotential 传入 `weighted_source_damage` 或在 `compute_damaging_ailment` 内先算好。

---

## 05-02 · 异常 magnitude 既叠 DoT 词条 inc/more 又叠 AilmentEffect，存在双重/错位计数 — HIGH

**PoB2 行为**：PoB2 异常 base = `calcAilmentDamage(...) * ailmentPercentBase`（L5146–5149），source 取自命中 pass 的 `output[type.."StoredHitMin/Max"]`（L4841–4856 → 设于 L4055–4056）。命中 pass 已经在 `dotCfg`（`ModFlag.Dot|Ailment` + `KeywordFlag.Bleed/Poison/…`，L5004–5005）下吸收了「增加 流血/中毒/持续 伤害」等 DoT 词条，因此 `calcMinMaxUnmitigatedAilmentSourceDamage` 里只再乘一个 `<Type><Ailment>Buildup` MORE，不再单独求 `BleedDamage/DamageOverTime`。最终 DPS 公式 `baseVal * effectMod(AilmentEffect) * rateMod * activeAilments * effMult`（L5190–5192）只有一处 AilmentEffect。

**PoBR 现状**：PoBR 走两层缩放：bleed/ignite/poison_instance 里 `scale_magnitude` 显式对 `[BleedDamage, AilmentDamage, PhysicalDamageOverTime, DamageOverTime]`（`ailment.rs:163-173/200-211/234-244`）做 inc 累加 + more 连乘；随后 `perform.rs:603` 的 `finalize_ailment_dps` 又乘 `ailment_effect_mod`（AilmentEffect MORE）。由于 PoBR 来源命中是裸 component_avg（未经 DoT-cfg 命中 pass），把 DoT 词条放在 `scale_magnitude` 本身可接受，但与额外 AilmentEffect 是否重叠取决于 `mod_parser` 把「增加 流血伤害」映射到哪个名字——若映射到 `BleedDamage` 又同时映射/聚合到 `AilmentEffect` 就双计；若 PoE2 的「增加 流血伤害」在 PoB2 实际归到 `AilmentEffect`，则 PoBR 的 `scale_magnitude` 名集映射错位。
- `crates/pobr-core/src/calc/ailment.rs:142-261`
- `crates/pobr-core/src/calc/ailment.rs:1078-1080`
- `crates/pobr-core/src/calc/perform.rs:596-607`

**修复方案**：核对 `mod_parser`/skill_stat_map 中 PoE2「增加/更多 流血·中毒·点燃·持续 伤害」「更多 异常效果」的目标 ModName，确保 `BleedDamage/DamageOverTime` 这套与 `AilmentEffect` 不会对同一词条双重命中。建议显式对照 PoB2 `ModParser.lua` 的 ailment 行（vendor 部分检出缺 ModParser.lua，需走 `gh` 取全量），并补一个 golden fixture（如「+X% 增加流血伤害」单词条）核验 PoBR 异常 DPS 与 PoB2 oracle 一致，避免 ×2 误差。

---

## 05-03 · 感电效果遗漏 AilmentMagnitude/EnemyShockMagnitude 的 effectMod — MEDIUM

**PoB2 行为**：`CalcOffence.lua` L5472：`Shock.effect = 50 * (damage/enemyThreshold)^0.4 * effectMod`，L5523 `output[ailment.."EffectMod"] = calcLib.mod(skillModList, cfg, "Enemy"..ailment.."Magnitude", "AilmentMagnitude") * calcLib.mod(enemyDB, cfg, "Self"..ailment.."Magnitude", "AilmentMagnitude")`，effectMod 进入 effect 函数。Chill 同理（L5479–5481 `incChill/moreChill`）。

**PoBR 现状**：`ailment.rs:270-280` `shock_effect` 签名只有 `(hit, threshold)`，公式 `50*ratio^0.4` 后直接 clamp[20,100]，没有 effectMod 入参；`shock_traced`（`ailment.rs:479-506`）调用时也未传任何 `AilmentMagnitude/EnemyShockMagnitude` 聚合。对比之下 `chill_traced`（`ailment.rs:779-804`）正确接了 effect_mod（`AilmentMagnitude/EnemyChillMagnitude`），所以是 Shock 单独漏了。
- `crates/pobr-core/src/calc/ailment.rs:270-280`
- `crates/pobr-core/src/calc/ailment.rs:479-506`

**修复方案**：给 `shock_effect` 增加 `effect_mod` 入参（默认 1.0），`shock_traced` 内按 `(1+Σinc/100)*Πmore` 聚合玩家 `EnemyShockMagnitude`/`AilmentMagnitude`（必要时再乘敌方 `SelfShockMagnitude`），仿 `chill_traced` 的做法连入 trace。注意 clamp 的 `min(20)/max(100)` 是 effectMod 之后再 clamp。

---

## 05-04 · 异常 base 命中未用 RollAverage（叠层位移滚动均值），只取 min/max 中点 — MEDIUM

**PoB2 行为**：`CalcOffence.lua` L5124–5126：`hitAvg = hitMin + (hitMax-hitMin)*ailmentRollAverage/100`，其中 RollAverage 在 StackPotential>1 时为 `(stacks-(max-1)/2)/(stacks+1)*100`，否则 50%（L5098–5104）。即异常 base 用的是按叠层潜力向高位偏移的滚动均值，而非简单 `(min+max)/2`。

**PoBR 现状**：PoBR 的来源命中走 `cross_type_source_hit`（`ailment.rs:1210-1233`）对每个 component 取 `(min+max)/2`，等价于固定 50% roll average，从不向高位偏移。`ailment.rs` 里虽然导出了 `roll_average/stack_potential`（`ailment.rs:981-1016`），但只服务于 `stacking_ailment_dps` 的线性叠层聚合，没有反馈到 base 命中区间的滚动均值。
- `crates/pobr-core/src/calc/ailment.rs:1210-1233`
- `crates/pobr-core/src/calc/ailment.rs:981-1016`
- `crates/pobr-core/src/calc/perform.rs:499-504`

**修复方案**：属「叠层维度尚未完整实现」的已知 gap（`perform.rs` 注释明确叠层延后）。完整 stacking 实现时，应让 `cross_type_source_hit` 暴露 min/max 而非直接给 avg，并按 `roll_average(stack_cfg)` 内插得到 hitAvg/critAvg，再喂给 `weighted_source_damage`。当前非叠层 build（max_stacks=1, SP≤1）下 RollAverage=50%，与 `(min+max)/2` 一致，故对单层 build 无偏差；标注为叠层完整化 TODO 即可。

---

## 05-05 · traced DPS 路径未扣敌方格挡/未做暴击命中降级/未做分类型减伤，与主路径分叉 — MEDIUM

**PoB2 行为**：`CalcOffence`：`HitChance = AccuracyHitChance*(1-enemyBlock/100)`，且 mode_effective 下暴击率乘 AccuracyHitChance 做二次检定（L3700）。命中与暴击在有效口径下都要吃敌方交互。

**PoBR 现状**：非 traced 主路径 `calculate_minimal_vs_enemy` 正确：扣 `enemy_block`（`offence.rs:264-269`）并把 `accuracy_hit_chance` 传入 `resolve_crit` 做降级（`offence.rs:275-282`）。但 traced 路径 `total_dps_traced`（`offence.rs:541-562`）用空 enemy_db、`hit_chance` 只算 `hit_chance(evasion, accuracy)` 不扣格挡，且 `cfg.mode_effective` 传入 `resolve_crit_traced` 时 enemy 为空——traced 与非 traced 在有效口径下给出不同 DPS。这是归因路径一致性缺口，会让 traced DPS 与面板 DPS 不一致；`total_dps_traced` 连 `enemy_damage_multiplier` 都没调用，分类型减伤完全缺失，traced DPS 对元素/混沌 build 会显著偏高。
- `crates/pobr-core/src/calc/offence.rs:456-615`
- `crates/pobr-core/src/calc/offence.rs:541-562`
- `crates/pobr-core/src/calc/offence.rs:264-291`

**修复方案**：若 traced 路径定位为「面板/旧三参口径」（无敌人减伤）则属设计取舍，建议在 `MinimalOutput`/文档明确 traced DPS 不含敌人交互，避免被误当成有效 DPS 的归因。若要与面板有效口径对齐，则需把 enemy_db、enemy_block、enemy_damage_multiplier 接入 `total_dps_traced`。

---

## 05-06 · flat_chance 对流血/中毒缺敌方 Self*Chance 的 inc/more（与 threshold 路径不对称）— LOW

**PoB2 行为**：PoB2 内禀几率聚合 `<Ailment>Chance` + `AilmentChance` 的 base/inc/more，并叠加敌方 `Self<Ailment>Chance`（base/inc）。

**PoBR 现状**：`flat_chance_traced`（`ailment.rs:510-548`）正确取了玩家 `<Ailment>Chance`/`AilmentChance` 的 base/inc/more 和敌方 `Self<Ailment>Chance` 的 BASE，但敌方 `Self*Chance` 的 INC/MORE 未纳入（`threshold_chance_traced` `ailment.rs:582-584` 对点燃/感电则把敌方 inc/more 纳入了，两条路径不对称）。
- `crates/pobr-core/src/calc/ailment.rs:510-548`
- `crates/pobr-core/src/calc/ailment.rs:582-584`

**修复方案**：对称化：`flat_chance_traced` 也把敌方 `Self<Ailment>Chance` 的 INC（必要时 MORE）并入 inc/more，与 `threshold_chance_traced` 保持一致。实际影响小（敌方 SelfBleedChance inc 罕见），标 LOW。

---

## 05-07 · DotDpsCap 的 Override 读取用 Sum(BASE) 近似，未走真正 Override 语义 — LOW

**PoB2 行为**：`CalcOffence` L5193 `m_min(uncapped, data.misc.DotDpsCap)`，且 cap 可被 `env.modDB:Override(nil,"DotDpsCap")` 重写（取 override 终值）。

**PoBR 现状**：`apply_dot_dps_cap`（`ailment.rs:1275-1284`）用 `player.sum(Base, "DotDpsCap")>0` 判定有无覆写，用求和值当 cap。若存在多条 DotDpsCap BASE 会相加而非取 override 终值；语义上应是 override 钳定。
- `crates/pobr-core/src/calc/ailment.rs:1275-1284`

**修复方案**：改用 `player.override_(cfg, "DotDpsCap")`（与 `crit_chance_cap` 的 override 处理一致）取终值，回退到 `DOT_DPS_CAP` 常量；避免多条 BASE 求和导致 cap 异常放大。影响面极小，标 LOW。
