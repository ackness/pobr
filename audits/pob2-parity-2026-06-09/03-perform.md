# 03 · 编排 (Perform / Buff / 技能时序 / 触发)

**Rust 模块**：`pobr-core::calc` — `perform.rs` / `skill_use_time.rs` / `skill_mechanics.rs` / `trigger.rs`
**对照**：PoB2 `CalcPerform.lua` / `CalcOffence.lua` / `CalcTriggers.lua`
**agent-docs 交叉**：§三 CalcPerform 全局编排 + §五 5.4 速度与 DPS 总装

## 总评

`perform.rs` 的 fill 编排骨架（offence → defence → mechanics → ailment → minions）清晰。`skill_use_time` / `skill_mechanics`（冷却、消耗、投射物、AoE）/ `trigger`（冷却驱动、能量、CWC、多技能轮转）的纯函数实现对照 vendor 公式系数大体正确：冷却 storedUses 取整、消耗分步 floor/ceil + costEfficiency、投射物 projBase/projMore、CWC 帧对齐间隔与 ICDR 除法都与 PoB2 源码一致。但「触发编排」这一核心环节存在方向性缺陷（HIGH），速率 cap/ActionSpeed 有边界场景偏差（MEDIUM），AoE 引入了臆造入基径（LOW）。

---

## 03-01 · 冷却驱动触发未走 rotation 模拟、未乘 triggerChance — HIGH

**PoB2 行为**：`CalcTriggers.lua` `defaultTriggerHandler` 默认分支（L796-805）：`output.SkillTriggerRate = calcMultiSpellRotationImpact(env, triggeredSkills, EffectiveSourceRate, triggerCD or triggeredCD, triggerChance, actor)`，其中 `triggerChance`（L716-776）`= 100 × sourceHitChance/100 × sourceCritChance/100 ×（triggerOnCrit 的 chanceToTriggerOnCrit）`。只有 `skillData.ignoresTickRate` 的单技能特例才退化为 `m_min(TriggerRateCap, EffectiveSourceRate)`（L782），且仍乘 overlaps。即触发速率不仅受 cap 与源速率门控，还须乘命中率/暴击率/触发几率并经轮转/冷却对齐折算。

**PoBR 现状**：`perform.rs:393-398` `fill_trigger` 在 `trigger_cd>0` 时直接调用 `resolve_trigger_rate_traced → trigger.rs:120-149 resolve_trigger_rate`，仅做 `skill_trigger_rate = min(cap, source_rate)`，既不乘 triggerChance（hit_chance×crit_chance×trigger_chance），也不调用本文件已实现的 `calc_multi_spell_rotation`（`trigger.rs:472`）。等于把 PoB2 的非默认 `ignoresTickRate` 特例当成了通用路径。
- `crates/pobr-core/src/calc/perform.rs:393-405`
- `crates/pobr-core/src/calc/trigger.rs:120-149`
- `crates/pobr-core/src/calc/trigger.rs:472-548`

**修复方案**：在 `fill_trigger` 的冷却驱动分支改为：先算 cap 与有效源速率，再乘 `triggerChance = sourceHitChance × sourceCritChance（若 triggerOnCrit）× explicitTriggerChance`，最后对多被触发技能调用已有的 `calc_multi_spell_rotation` 求稳态速率（chance<1 的几何折算已支持）。triggerChance 所需 hit_chance/crit_chance 可取自 `env.player.output`（命中率/暴击率已算出）。单技能且 ignoresTickRate 时才保留 `min(cap, sourceRate×overlaps)` 特例以对齐 L782。

---

## 03-02 · 触发 source_rate 误用被触发技能自身速率；build 层未注入触发数据致输出恒为 0 — HIGH

**PoB2 行为**：`CalcTriggers.lua` 中 `EffectiveSourceRate` 来自 `findTriggerSkill` 找到的「触发源技能」的 cast/attack rate（L427 `useCastRate ? cast rate : attack rate`），并对双持/multiHit/unleash/repeats 等做修正（L432-465）。触发源与被触发技能是两个不同 skill，源速率取自源技能的 `output.Speed`。

**PoBR 现状**：`perform.rs:389` `source_rate = env.player.output.effective_action_rate`，这是主技能（即被触发的那个技能）自己的有效行动速率，而非触发源技能的速率——语义错位。此外 build 层（`crates/pobr-build/src`）完全没有写入 `TriggerCooldownBase` / `CWCTriggerTime` / 触发源速率，grep 无任何命中，意味着 `fill_trigger` 的 `trigger_cd>0` / `cwc_trigger_time>0` 分支在真实 build 中永不进入，`trigger_rate_cap` / `skill_trigger_rate` 恒为 0。
- `crates/pobr-core/src/calc/perform.rs:375-406`
- `crates/pobr-build/src/calc_orchestrator.rs`

**修复方案**：触发链路需 build/orchestrator 层提供：触发源技能有效速率（attack/cast rate）、触发宝石冷却（`TriggerCooldownBase`）、被触发技能冷却（`TriggeredSkillCooldown`）、触发几率/triggerOnCrit。建议在 Env/Actor 上增加触发上下文字段（`source_rate`、`trigger_cd`、`triggered_cd`、`trigger_chance`、`is_cwc`），由 orchestrator 从宝石/插槽数据填充，`fill_trigger` 改读该上下文而非主技能 action_rate。在未接入前应在文档/注释明确标注触发面板为占位（当前注释只说「无词条保持 0」，未点明 build 层从不注入）。

---

## 03-03 · 技能速率 ActionSpeed 缺 floor/cap 与 TemporalChains 分离，且对攻击技能无条件施加 — MEDIUM

**PoB2 行为**：`CalcPerform.lua:922-944` `actionSpeedMod = 1 + (max(-TemporalChainsEffectCap, TemporalChainsActionSpeed) + ActionSpeed)/100`，随后 `max(MinimumActionSpeed/100, _)` 下限、`min((100-MaximumActionSpeedReduction)/100, _)` 上限；`UnaffectedBySlows` 时只取正向值。`CalcOffence.lua:2831-2851` `ActionSpeedMod` 只在 selfCast / totem 末端乘 `output.Speed`；攻击的速度由 `source.AttackRate`（武器层）承载，不在此处再乘整 ActionSpeedMod。

**PoBR 现状**：`skill_use_time.rs:88,101-102` `total_action_speed = Sum(INC, ActionSpeed)`，`action_factor = 1 + total_action_speed/100`，对所有技能无条件相乘，没有 `MinimumActionSpeed` 下限、没有 `MaximumActionSpeedReduction` 上限、没有把 `TemporalChainsActionSpeed` 单独 cap，也无 `UnaffectedBySlows` 正向过滤。`offence.rs:248` 的 DPS 路径同样无条件乘 action_speed_mod。
- `crates/pobr-core/src/calc/skill_use_time.rs:88-122`
- `crates/pobr-core/src/calc/offence.rs:242-249`

**修复方案**：抽一个与 PoB2 `actionSpeedMod` 等价的辅助函数：`mod = 1 + (max(-TemporalChainsEffectCap, Sum(INC,TemporalChainsActionSpeed)) + Sum(INC,ActionSpeed))/100`，再 `max(MinimumActionSpeed/100,·)`、`min((100-MaximumActionSpeedReduction)/100,·)`，并支持 `UnaffectedBySlows` 的 SumPositiveValues 路径；skill_use_time 与 offence 共用之。Temporal Chains / 高 slow 场景下当前实现会偏离 PoB2。

---

## 03-04 · 服务器帧速率上限缺 Repeats 因子，且 DPS 路径完全未施加帧 cap — MEDIUM

**PoB2 行为**：`CalcOffence.lua:2863-2865`：`if not Channel then output.Speed = m_min(output.Speed, data.misc.ServerTickRate * output.Repeats)`。即非引导技能的最终速率被 `ServerTickRate × Repeats` 截断（Repeats 来自多重打击/技能重复，默认 1）。这是作用在最终 DPS 速率上的硬上限。

**PoBR 现状**：`skill_use_time.rs:104-110` 帧 cap 为 `server_rate`（无 Repeats），且只写入 `effective_action_rate`（仅供 trigger 用）。`offence.rs` 的 DPS action_rate（`offence.rs:249` / `598` `apply_cooldown_cap`）只施加冷却 cap，没有任何 `ServerTickRate` 帧 cap——高攻速 build 的 DPS 不会被 30.3/s 帧率截断，与 PoB2 不符。`Repeats`（multistrike/技能重复）在整个 core 中未建模（grep 无命中）。
- `crates/pobr-core/src/calc/skill_use_time.rs:104-110`
- `crates/pobr-core/src/calc/offence.rs:241-249`
- `crates/pobr-core/src/calc/offence.rs:833-854`

**修复方案**：在 offence DPS 速率末端（`apply_cooldown_cap` 之后）追加非引导技能的 `min(rate, ServerTickRate × Repeats)` 帧 cap；引入 `Repeats`（默认 1，由 multistrike/skillRepeats 词条注入）。`skill_use_time` 的帧 cap 也应乘 Repeats 以与 DPS 路径一致。在 Repeats 未接入前至少补上 `ServerTickRate` 单帧 cap，避免极高攻速 DPS 失真。

---

## 03-05 · AoE 计算把 BASE AreaOfEffect 加入基础半径，PoB2 calcRadius 无此项 — LOW

**PoB2 行为**：`CalcOffence.lua:161-162` `calcRadius(baseRadius, areaMod) = m_floor(baseRadius × m_floor(100×sqrt(areaMod))/100)`；`areaMod = round(round(incArea×moreArea,10),2)`。baseRadius 仅为技能基础半径，不掺入任何 `Sum(BASE, AreaOfEffect)`；AoE 修正全部走 areaMod 乘区。

**PoBR 现状**：`skill_mechanics.rs:74` `effective_base = base_radius + extra_base + db.sum(Base, AreaOfEffect)`，把 BASE AreaOfEffect 直接加进入基径再走 calcRadius——PoB2 没有此通道。另 `area_mod` 用 4 位 round 而非 PoB2 的 2 位 round（calcRadius 台阶可能差一格）。
- `crates/pobr-core/src/calc/skill_mechanics.rs:66-80`

**修复方案**：移除 `effective_base` 中的 `db.sum(Base, AreaOfEffect)`（除非确有 PoE2 词条按基径加值的来源，否则属臆造）；并把 `area_mod` 对齐 PoB2 的 `round(round(inc×more,10),2)` 两步取整，使半径台阶与 PoB2 一致。影响面小（BASE AoE 罕见），故 LOW。

---

## 03-06 · CWC 分支 skill_trigger_rate 直接取 cap，未经 calcMultiSpellRotationImpact — LOW

**PoB2 行为**：`CalcTriggers.lua` `CWCHandler`（L262-263）：`TriggerRateCap = m_min(1/effCDTriggeredSkill, triggerRateOfTrigger)`，随后 `SkillTriggerRate = calcMultiSpellRotationImpact(env, triggeredSkills, triggerRateOfTrigger, 0)`——即被触发技能间仍走轮转模拟，多技能 CWC 时各技能速率被轮转分摊。

**PoBR 现状**：`perform.rs:401-404` CWC 分支把 `skill_trigger_rate` 直接等于 `cwc.trigger_rate_cap`（`trigger.rs:599-638` `calc_cwc_trigger_rate` 只算单技能 cap）。单被触发技能时与 PoB2 等价，但多被触发技能轮转分摊未建模。`adds_cast_time` 也硬编码为 0（`perform.rs:402` 注释承认 build 层未注入）。
- `crates/pobr-core/src/calc/perform.rs:399-405`
- `crates/pobr-core/src/calc/trigger.rs:599-638`

**修复方案**：CWC 多被触发技能时应把 `channelling_rate` 作为 source_rate 喂给 `calc_multi_spell_rotation` 分摊到各技能（对齐 L263）；单技能可保留取 cap 的快路径。`adds_cast_time` 需 build 层按被触发法术 base_cast_time/cast_speed 注入（`spell_cast_time_added_to_cooldown` 已实现，仅缺接线）。
