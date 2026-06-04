# 触发机制进阶 (Triggers)

本文档覆盖 PoE2（0.5.0）**触发 (Trigger)** 体系的「机制与速率」层：触发来源分类、元宝石的**能量 (Energy)** 量化模型、触发冷却与**触发速率上限 (Trigger Rate Cap)**、服务器帧节流、CoC / CWC 的 PoE2 对应物、触发技能的伤害/暴击上下文，以及多技能轮转 (rotation) 的建模。

> 本文与既有文档**互补、不重复**：
> - 元宝石的基础概念、Spirit 保留、敌人力量 (Monster Power) 概览见 [meta-gems.md](./meta-gems.md)，本文**补充能量获取/消耗的量化公式与触发速率**；
> - 冷却 (Cooldown)、消耗/保留、引导 (Channelling)、重复 (Repeat) 的技能侧机制见 [skill-mechanics.md](./skill-mechanics.md)，本文只讲**冷却如何门控触发速率**；
> - Spirit 保留池/效率/Presence 公式见 [recovery-charges-buffs.md](./recovery-charges-buffs.md)；
> - 触发技能的伤害/暴击计算沿用 [critical-hits.md](./critical-hits.md) / [damage-scaling.md](./damage-scaling.md)，本文只讲**触发上下文的差异**。
>
> 末尾「PoB2 计算实现」给出核对过的真实变量/旗标名，是 pobr 的回归基准。

> **POB2 状态警告**：POB2 更新日志明确指出元宝石/触发技能伤害计算「needs an entire overhaul」，对**能量驱动**的元宝石（Cast on X）触发速率支持目前**不完整**——`CalcTriggers.lua` 主要建模**冷却驱动**的触发（CoC 走模拟、CWC 走引导间隔），能量门控多为估算。实现时需以游戏内一手数据为准[^pob2-deepwiki-gems][^poe2db-energy]。

---

## 一、触发来源分类 (Trigger Sources)

PoE2 的触发可按**触发条件**与**实现机制**两个维度分类。

### 1.1 按触发条件

| 触发条件 | 代表元宝石 / 词条 | PoB2 stat 关键字 |
|---------|------------------|-----------------|
| **on Hit（命中）** | Cast on Hit（火法术）、`triggered_when_you_hit` 词条 | `gain_X_centienergy_per_monster_power_on_hit` |
| **on Crit（暴击）** | **Cast on Critical** (CoC) | `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit` |
| **on Kill（击杀）** | **Cast on Melee Kill**、**Cast on Minion Death** | `gain_X_centienergy_per_monster_power_on_melee_kill` |
| **on Ailment（异常）** | **Cast on Elemental Ailment**（点燃/感电/冰冻）、Cast on Ignite/Shock/Freeze | `gain_X_centienergy_per_monster_power_on_{ignite,shock,freeze}` |
| **on Block（格挡）** | **Cast on Block** | `gain_X_centienergy_on_block` |
| **when Hit / Hurt（受击/被伤害）** | **Barrier Invocation**（每损失 10 ES 得 1 能量） | `gain_1_energy_per_X_energy_shield_lost` 类 |
| **on Dodge（翻滚）** | **Cast on Dodge**（每移动 X 米得能量） | `gain_X_centienergy_per_unit_travelled_while_dodge_rolling` |
| **on Stun（眩晕）** | Cast on Melee Stun | `gain_X_centienergy_per_monster_power_on_{stun,heavy_stun}` |
| **on Charm Use** | Charm 触发类 meta | `gain_X_centienergy_per_charm_charge_used_on_using_charm` |
| **while Channelling（引导中）** | **Cast While Channelling** (CWC，能量版) | `triggeredWhileChannelling` / `triggerTime` |
| **at Interval（按间隔）** | Unleash the Elements 等 Buff 触发 | `..._base_trigger_frequency_ms` |

### 1.2 按实现机制（PoB2 内部三类）

PoB2 `isTriggered()` 把「被触发技能」识别为：`triggered` / `SkillType.Triggered` / `SkillType.InbuiltTrigger` / `triggeredByUnique` / `grantedEffect.triggered`。底层分三种机制：

1. **能量驱动 (Energy / Meta Gem)**：`SkillType.GeneratesEnergy` + `Triggers`，攒能量到上限触发（§二）。
2. **冷却驱动 (Cooldown-gated)**：触发本身有 base cooldown，由源技能速率 + 冷却共同门控（§三、§四）。CoC、CWC 属此类。
3. **物品内建触发 (InbuiltTrigger, `SkillType=50`)**：装备授予并自动触发的技能；该 flag 会**阻止触发宝石与 trap/mine/totem 再作用**（防止嵌套触发）[^pob2-global]。

---

## 二、能量 (Energy) 机制量化

> 与 [meta-gems.md](./meta-gems.md) §能量 交叉引用——此处补充**获取/消耗公式与最大能量**。

触发型元宝石用 **Energy** 计数器决定何时触发：**攒到最大能量 → 触发所有插槽法术 → 能量清零**[^poe2db-energy][^poe2wiki-coc]。每个产能技能**有自己独立的能量计数**；**被触发技能的直接效果不能产能**（防止自循环）[^poe2db-energy]。

### 2.1 最大能量 (Maximum Energy)

```
最大能量 = Σ(插槽法术基础施法时间 / 0.1s) × 10        -- 每 0.1s 基础施法时间 = 10 能量
```

- PoB2 stat：`generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`、`generic_ongoing_trigger_maximum_energy_is_total_of_socketed_skills`[^pob2-other]。
- 即「Has 10 maximum Energy per 0.1 seconds of base cast time of Socketed Spells」[^poe2wiki-coc]。基础施法时间越长 → 最大能量越高 → 越难触发。
- **关键陷阱**：计算最大能量（与产能消耗）时，**对「总使用时间」的修饰词按 2 倍计算**（"modifiers to Total use time are treated as though they were double the value"）[^poe2db-energy][^poe2wiki-coc]。
- 部分 Invocation（手动触发）用**固定最大能量**而非按施法时间，如 **Barrier Invocation 最大能量 500**[^game8-trigger]。

### 2.2 能量获取 (Energy Gain) — 0.5.0 重制核心

0.5.0 把能量获取与 **Monster Power**（敌人力量）和**异常强度**绑定，解决「清怪触发过频、Boss 触发不足」的旧问题[^poe2wiki-coc]。

**Cast on Critical 的能量公式**（最具代表性，0.5.0 起还看伤害）[^poe2wiki-coc]：

```
能量获取 = Monster Power × (暴击原始伤害(减免前) / 怪物异常阈值 Ailment Threshold)
```

- **要可靠触发，暴击伤害需约为怪物异常阈值的 10 倍**。异常阈值随怪物等级**指数增长**，故进高等级区域后若伤害没跟上，触发会突然失效[^poe2wiki-coc]。
- 与 PoE1 不同：**独立于怪物生命**，改看异常阈值。

**Cast on Ailment / Ignite 类**：`能量 = Monster Power × (点燃 magnitude / 阈值占比)`（点燃看强度，shock/freeze 多为定值）[^game8-trigger]。注意不同条件的 centienergy 基数差异很大（核对自 `act_int.lua`）：

| 条件 | centienergy/Monster Power | = 能量 |
|------|--------------------------|--------|
| Crit | 100 | 1 / Power |
| Ignite | 100（按异常阈值占比调整） | 1 / Power |
| Shock | 100 | 1 / Power |
| **Freeze** | **1000** | **10 / Power** |

> centienergy = 1/100 能量。Freeze 的基数是 Crit/Ignite/Shock 的 10 倍——冰冻触发显著更快[^pob2-actint]。

### 2.3 Monster Power（与 meta-gems.md 交叉，补充范围）

`Power = 内部生命乘数 × 稀有度系数`。内部乘数一般 **0.5–3**（弱小怪 ~0.5，大型威胁怪 ~3）；稀有度系数 **普通 1 / 魔法 2 / 稀有 5 / 独特固定 20**[^mobalytics-monsterpower][^poe2wiki-coc]。

- **设计意图**：一整波怪的**总 Power 常落在 15–20**（不论 Boss / 魔法群 / 稀有群 / 杂兵群），让触发在不同遭遇下一致。
- **推论**：只打**单个**弱小怪时产能约比满波少 **~30 倍**——单体触发慢、群怪触发快。

### 2.4 能量消耗与触发

- 达到最大能量 → **触发所有插槽法术，能量清零**[^poe2db-energy]。**一次命中只能触发一次**，溢出能量丢弃[^poe2wiki-coc]。
- **Invocation（手动触发）**可在能量够时一次**触发多次**（每次扣对应能量），如 Barrier Invocation；普通 Cast on X 每次触发只清零一次。
- **触发仍需支付资源成本**（通常法力）——但所有 Cast on X 的被触发法术带 `no_cost`，故元宝石侧免费，成本在元宝石的 Spirit 保留上[^pob2-other]。
- **gem 等级**只缩放 `energy_generated_+%`（1 级 +0% → 每级 +3%，20 级约 +57%）[^poe2wiki-coc][^pob2-actint]——等级**不改产能基数也不改最大能量**，只加快攒能量速度。

---

## 三、触发冷却 (Trigger Cooldown) 与服务器帧节流

### 3.1 触发速率上限 (Trigger Rate Cap)

冷却驱动型触发的速率上限取**「触发器冷却」与「被触发技能冷却」中较大者**，再受**服务器帧**节流[^pob2-calctriggers]：

```lua
-- CalcTriggers.lua, helmetFocusHandler / defaultTriggerHandler
icdrSkill        = calcLib.mod(modList, cfg, "CooldownRecovery")        -- 冷却恢复速率乘区
modActionCooldown = max( triggeredCD,  triggerCD / icdrSkill )           -- 取两者较大
rateCapAdjusted   = ceil(modActionCooldown × ServerTickRate) / ServerTickRate   -- 向上取整到帧
TriggerRateCap    = 1 / rateCapAdjusted
```

- **ICDR = Increased Cooldown Recovery Rate**（`CooldownRecovery` INC/MORE），作为**除数**缩短冷却：`实际冷却 = base / icdr`，提升触发上限。
- `triggerCD` = 触发宝石本身的冷却（`triggeredBy.grantedEffect.levels[lvl].cooldown`）；`triggeredCD` = 被触发技能的冷却（`skillData.cooldown`）。
- 被触发技能**无冷却**时，仅用 `triggerCD / icdr` 作为上限。

### 3.2 服务器帧节流 (Server Tick)

PoB2 `Data.lua`：`ServerTickTime = 0.033`，`ServerTickRate = 1/0.033 ≈ 30.3/s`[^pob2-data]。

- **所有触发冷却向上取整到服务器帧**：`ceil(cd × ServerTickRate) / ServerTickRate`。即触发只能发生在帧边界，真实冷却被"四舍五入"到下一帧。
- 例外：`skillData.ignoresTickRate` / `triggeredBy.ignoresTickRate` 的技能跳过取整（按真实值走，配合能量/充能式估算）[^pob2-calctriggers]。
- 这与冷却/持续/速度的帧上限一致——见 [skill-mechanics.md](./skill-mechanics.md) §冷却、[recovery-charges-buffs.md] 同款 `ServerTickRate`。

### 3.3 实际触发速率 = min(上限, 源速率)

```
SkillTriggerRate = min( TriggerRateCap, EffectiveSourceRate )
```

- **EffectiveSourceRate**：源技能（手动施放/攻击）的每秒次数；双持 `/2`、Unleash/多投射物等有专门修正[^pob2-calctriggers]。
- 即触发速率被「上限」和「你实际能多快制造触发条件」**双重门控**——伤害再高，若源攻速低或冷却长，触发也慢。

---

## 四、CoC / CWC 在 PoE2 的对应物

### 4.1 Cast on Critical (CoC) — 暴击触发

PoE2 的 CoC 是**能量驱动的元宝石**（不是 PoE1 的辅助宝石）[^poe2wiki-coc][^poe2db-coc]：
- ID `CastOnCriticalStrike`，tags `HasReservation/Meta/Persistent/Buff/GeneratesEnergy/Triggers`；**保留 100 Spirit**；`Socketed Skills deal 20% less Damage`；附带 `Buff grants (0-20)% increased Critical Hit chance`。
- 产能 = §2.2 公式（暴击伤害 / 异常阈值 × Monster Power）。PoB2 对 CoC 走 `calcMultiSpellRotationImpact` 模拟（§五）。

### 4.2 Cast While Channelling (CWC) — 引导触发

PoB2 `CWCHandler` 处理（仍为冷却/间隔驱动，非能量）[^pob2-calctriggers]：

```lua
adjTriggerInterval   = ceil(source.skillData.triggerTime × ServerTickRate) / ServerTickRate
triggerRateOfTrigger = 1 / adjTriggerInterval                           -- 引导每隔 triggerTime 触发一次
triggeredTotalCD     = cooldownOverride or max(triggeredCD, addsCastTime) / icdr
TriggerRateCap       = min( 1 / effCDTriggeredSkill, triggerRateOfTrigger )
```

- 由**引导间隔 `triggerTime`**（取整到帧）决定基准触发节奏，被触发技能冷却再 clamp。
- 0.5.0 也有**能量版 CWC**（"gains Energy while channelling"，到上限触发）作为元宝石[^poe2db-energy]。

### 4.3 `SpellCastTimeAddedToCooldownIfTriggered`

部分触发把**被触发法术的施法时间加进冷却**（`addsCastTime = baseCastTime / 施法速度乘区`），使施法慢的法术触发更慢[^pob2-calctriggers]。

---

## 五、多技能轮转与触发速率建模

多个法术插同一元宝石时，PoB2 用**确定性模拟** `calcMultiSpellRotationImpact` 估算每个技能的稳态触发速率[^pob2-calctriggers]：

```lua
triggerIncrement = 1 / sourceRate            -- 每次触发机会的间隔
SIM_TIME = triggerIncrement × 1000           -- 模拟 1000 次触发机会
-- 轮转: 每个触发机会找轮转中第一个"已脱离冷却"的技能触发它，否则浪费
skill.cd = max( cdOverride or (cd+addedCD)/icdr,  (triggerCD+addsCastTime)/icdr )
next_trig = ceil_b( floor_b(now, ServerTickTime) + skill.cd, ServerTickTime )   -- 冷却对齐到帧
```

- **轮转优先级**：每个触发机会按轮转顺序找第一个不在冷却的技能触发；都在冷却则**该次触发浪费**。这模拟了「多个长冷却法术轮流触发」的稀释。
- **触发几率 (trigger chance)**：用几何分布期望值 O(1) 折算——`rate = 1 / (SIM_TIME/count + triggerIncrement/chance×100 − triggerIncrement)`，把 `<100% chance to trigger` 摊进速率[^pob2-calctriggers]。
- **冷却对齐到帧**：冷却从当前帧起算、到冷却到期后的下一帧结束（`ceil_b(floor_b(...))`），这是触发速率出现"台阶"的根因。

### 触发伤害修饰 (`TriggeredDamage`)

触发宝石上的 `increased/more TriggeredDamage` 会被 `addTriggerIncMoreMods` 转成被触发技能的 `Damage INC/MORE`[^pob2-calctriggers]。

---

## 六、触发技能的伤害 / 暴击 / 上下文

被触发技能的伤害、暴击、命中**沿用标准 calc**（[critical-hits.md](./critical-hits.md) / [damage-scaling.md]），但有触发专属差异：

- **暴击独立滚动**：触发的击中按每次击中独立滚暴击阈值（与持续/引导同，不算 Rerolling）——见 [critical-hits.md](./critical-hits.md) §暴击检定。
- **DPS = 触发速率 × 单次伤害**：触发技能的 DPS 由 `SkillTriggerRate`（§三/§五）× 单次击中/暴击效果决定，而非自身施法速度。
- **`less Damage` 惩罚**：CoC 类「Socketed Skills deal 20% less Damage」是 `Damage MORE −20`。
- **Spirit 保留**：元宝石按宝石保留 Spirit（CoC=100），且作为持续增益可被持续增益辅助宝石支持——见 [meta-gems.md](./meta-gems.md) §精神保留、[recovery-charges-buffs.md](./recovery-charges-buffs.md) §保留效率。**成本倍乘 ≠ 保留倍乘**。

---

## PoB2 计算实现（核对基准）

变量/旗标取自 [PathOfBuilding-PoE2 `dev`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的 `src/Modules/CalcTriggers.lua`、`Modules/Data.lua` 与 `src/Data/Skills/*.lua`、`Global.lua`：

**触发速率（`CalcTriggers.lua`）**
```lua
ServerTickTime = 0.033;  ServerTickRate = 1/0.033 (≈30.3)        -- Data.lua
icdr             = calcLib.mod(modList, cfg, "CooldownRecovery")  -- ICDR 除数
modActionCooldown = max(triggeredCD, triggerCD / icdr)
TriggerRateCap   = 1 / (ceil(modActionCooldown × ServerTickRate)/ServerTickRate)
SkillTriggerRate = min(TriggerRateCap, EffectiveSourceRate)       -- 双门控
-- CWC: triggerRateOfTrigger = 1/ceil(triggerTime×ServerTickRate)/ServerTickRate
-- 模拟: calcMultiSpellRotationImpact(env, skillRotation, sourceRate, triggerCD, chance)
-- 触发伤害: Tabulate("INC"/"MORE", "TriggeredDamage") → Damage INC/MORE
-- 识别: isTriggered() = triggered / SkillType.Triggered / InbuiltTrigger / triggeredByUnique
```

**SkillType（`Global.lua`）**：`Triggerable=31`、`Triggers=32`、`Triggered=37`、`InbuiltTrigger=50`、`GeneratesEnergy`、`HasReservation=12`、`OngoingSkill`、`Meta`、`Persistent`、`Cooldown=91`。

**能量（`Data/Skills/act_int.lua` / `other.lua`，stat scope `meta_gem_stat_descriptions`）**
```lua
-- 最大能量
generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10
generic_ongoing_trigger_maximum_energy_is_total_of_socketed_skills
generic_ongoing_trigger_triggers_at_maximum_energy
-- 产能（centienergy = 1/100 能量；× Monster Power）
cast_on_crit_gain_X_centienergy_per_monster_power_on_crit         = 100
cast_on_ignite_gain_X_centienergy_per_monster_power_on_ignite     = 100
cast_on_shock_gain_X_centienergy_per_monster_power_on_shock       = 100
cast_on_freeze_gain_X_centienergy_per_monster_power_on_freeze     = 1000
-- 等级缩放（产能速度，非基数/上限）
energy_generated_+%   -- lvl1=0 → +3%/lvl → lvl20≈+57%
-- 被触发法术
triggered_by_generic_ongoing_trigger;  no_cost;  base_deal_no_damage(元宝石自身)
-- 保留/伤害惩罚
spiritReservationFlat = 100 (CoC);  Socketed Skills deal 20% less Damage
-- 公式（CoC, 0.5.0）：能量 = MonsterPower × 原始击中伤害 / 怪物异常阈值
```

---

## 0.5.0 关键变化小结

- **能量获取并入 Monster Power + 异常强度**：解决清怪/Boss 触发频率失衡；暴击伤害需 ~10× 怪物异常阈值才可靠触发，且阈值随等级**指数增长**[^poe2wiki-coc]。
- **CoC 0.5.0 起还看伤害**（不只看是否暴击）——低伤害暴击产能极少[^poe2wiki-coc]。
- **总使用时间修饰词在能量计算中按 2 倍处理**[^poe2db-energy]。
- Freeze 产能基数是 Crit/Ignite/Shock 的 **10 倍**[^pob2-actint]。
- CoC / 多数触发已是**能量驱动元宝石**（PoE1 的 CoC 辅助宝石模式不再）；CWC 有冷却/间隔版与能量版两种。

---

## 对 pobr 实现的启示

对照 `pobr-core`（`calc/offence.rs`、`config.rs::CalcConfig`、`mod_db.rs`、`trace.rs`），触发体系是一个**独立子系统**，建议：

1. **触发速率作为一等输出，DPS = 速率 × 单次效果。**
   - 新增 `TriggerRateCap` / `EffectiveSourceRate` / `SkillTriggerRate`，`SkillTriggerRate = min(cap, sourceRate)`；触发技能 DPS 用此速率而非自身施法速度。
   - `CooldownRecovery`（ICDR）作为除数；冷却 `ceil(cd × ServerTickRate)/ServerTickRate`（复用 `skill-mechanics.md` 已定的 `ServerTickRate = 1/0.033`）。

2. **能量模型按「centienergy + Monster Power + 异常阈值」量化。**
   - `max_energy = Σ(socketed base_cast_time / 0.1) × 10`，total-use-time 修饰词 ×2。
   - `energy_per_trigger = MonsterPower × 原始击中伤害 / 异常阈值`（CoC）；按条件取 centienergy 基数（Crit/Ignite/Shock=100、Freeze=1000）。
   - `energy_generated_+%` 走宝石等级；**触发速率 ≈ 产能速率 / max_energy**，受 §三上限 clamp。
   - 把 `MonsterPower`、`AilmentThreshold` 作为 `CalcConfig` 的敌人上下文（默认按一波 ~15–20 总 Power 估算单体场景）。

3. **多技能轮转用确定性模拟（可移植 `calcMultiSpellRotationImpact`）。**
   - 直接移植「1000 次触发机会 + 帧对齐冷却 + 几何分布折算触发几率」的 O(1) 估算，保证与 PoB2 数值一致（用 golden fixture 锁定）。

4. **新 SkillType / flags。**
   - `SkillType`：`Triggerable/Triggers/Triggered/InbuiltTrigger/GeneratesEnergy/Meta/Persistent/HasReservation`。
   - flags：`SpellCastTimeAddedToCooldownIfTriggered`、`ignoresTickRate`、`globalTrigger`。
   - `TriggeredDamage`(INC/MORE) → 注入被触发技能的 `Damage`。

5. **归因 (TraceGraph) 的增量价值**：把「触发速率」拆解到来源——源技能攻速、ICDR 词条、能量上限（插槽法术）、Monster Power 假设、各 less Damage 惩罚——都能回溯到 `SourceId`，这是 pobr 相对 PoB「触发为何这么快/慢」可解释性的核心增量。

> ⚠️ 因 POB2 对能量驱动元宝石伤害计算**尚不完整**，pobr 实现时不应盲目以 POB2 数值为唯一基准，需对照游戏内 / PoE2DB / Wiki 的一手能量公式，并保留来源说明。

---

## 参考来源

[^poe2db-energy]: PoE2DB — Energy（能量机制、独立计数、不能从触发技能直接效果产能、total use time ×2、Invocation 多次触发、Barrier Invocation 最大能量 500）。https://poe2db.tw/Energy
[^poe2db-coc]: PoE2DB — Cast on Critical（ID、tags、stat 列表）。https://poe2db.tw/us/Cast_on_Critical
[^poe2wiki-coc]: PoE2 Wiki — Cast on Critical Strike（能量公式 = MonsterPower × 原始伤害/异常阈值、10× 阈值、10 maximum Energy per 0.1s base cast time、20% less Damage、100 Spirit、等级能量进度、Monster Power 范围与一波总 Power 15–20、0.5.0 patch note）。https://www.poe2wiki.net/wiki/Cast_on_Critical_Strike
[^mobalytics-monsterpower]: Mobalytics — PoE 2 Guide: Monster Power（base Power 0.5–3、稀有度系数 1/2/5/独特 20）。https://mobalytics.gg/poe-2/guides/monster-power
[^mobalytics-meta]: Mobalytics — PoE 2 Meta Gems（Energy/Enemy Power/触发清零）。https://mobalytics.gg/poe-2/guides/meta-gems
[^game8-trigger]: Game8 — Trigger Explained / PoE2（各 Cast on X 能量值、Barrier Invocation 最大能量 500、Cast on Elemental Ailment 多条件、Cast on Minion Death 50 base energy × minion power）。https://game8.co/games/Path-of-Exile-2/archives/487670
[^pob2-deepwiki-gems]: Path of Building for PoE2 DeepWiki — Meta Skills 伤害计算「needs an entire overhaul」。https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^pob2-calctriggers]: PathOfBuilding-PoE2 — `src/Modules/CalcTriggers.lua`（isTriggered / TriggerRateCap / icdr / ServerTick 取整 / calcMultiSpellRotationImpact / CWCHandler / processAddedCastTime / addTriggerIncMoreMods）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcTriggers.lua
[^pob2-data]: PathOfBuilding-PoE2 — `src/Modules/Data.lua`（`ServerTickTime = 0.033`、`ServerTickRate = 1/0.033`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/Data.lua
[^pob2-actint]: PathOfBuilding-PoE2 — `src/Data/Skills/act_int.lua`（MetaCastOnCrit / CastOnElementalAilment 的 centienergy 常量：crit/ignite/shock=100、freeze=1000；energy_generated_+% 等级表；spiritReservationFlat=100）。
[^pob2-other]: PathOfBuilding-PoE2 — `src/Data/Skills/other.lua`（`generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time=10`、`maximum_energy_is_total_of_socketed_skills`、`triggers_at_maximum_energy`、`triggered_by_generic_ongoing_trigger`、`no_cost`）。
[^pob2-global]: PathOfBuilding-PoE2 — `src/Data/Global.lua`（SkillType 枚举：Triggerable=31/Triggers=32/Triggered=37/InbuiltTrigger=50）。
[^pob2-gems]: PathOfBuilding-PoE2 — `src/Data/Gems.lua`（Cast on Critical/Block/Dodge/Melee Kill/Minion Death/Elemental Ailment、Barrier Invocation、Energy Retention/Capacitor/Boundless Energy 支援宝石的 tags/meta/trigger）。
