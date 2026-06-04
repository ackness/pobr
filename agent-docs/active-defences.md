# 进阶 / 主动防御 (Active & Advanced Defences)

本文档收录 PoE2（0.5.0）防御面那些**未被既有文档覆盖**的进阶 / 主动概念：**翻滚闪避 (Dodge Roll)**、**守护吸收 (Guard / Molten Shell)**、**规避 (Avoidance)**、**减少受到的暴击额外伤害**、**防御向 Keystone**、以及一批 PoE1 有而 PoE2 **已移除**的机制（Spell Suppression / Acrobatics 等，逐一核实）。

> 本文与既有文档**互补、不重复**，交叉引用：
> - 闪避 / 偏转命中公式见 [evasion.md](./evasion.md)；护甲减伤见 [armour.md](./armour.md)；ES 充能 / CI 见 [energy-shield.md](./energy-shield.md)；符咒护佑见 [runic-ward.md](./runic-ward.md)；格挡见 [block.md](./block.md)；抗性 / 最大抗性见 [resistances.md](./resistances.md)；眩晕 / 重眩晕见 [stun.md](./stun.md)。
> - 伤害承受**顺序**（转移 → Guard → ES → 生命 → 符咒护佑）见 [damage-defence-order.md](./damage-defence-order.md)。
> - **「不被击中几率」乘法链**、**Max Hit / EHP**、**承受伤害乘数**、**Fortify / Onslaught / Elusive** 等增益已在 [recovery-charges-buffs.md](./recovery-charges-buffs.md) §4 覆盖，本文只补它们没讲的「翻滚 / 守护 / 规避 / 暴击减伤 / Keystone」并交叉引用。
> - 末尾「PoB2 计算实现」给出核对过的真实变量 / 旗标名，是 pobr 的回归基准。

> **0.5.0 / 0.3.0 关键背景**：PoE2 取消了 PoE1 的「攻击 vs 法术」防御二分，怪物伤害改以 tag（AoE / projectile / strike / ground）区分[^mmojugg-monster]。这直接导致 **Spell Suppression、Acrobatics 被移除**（见 §6），并使闪避 / 翻滚的适用范围被重新定义。

---

## 一、翻滚闪避 (Dodge Roll)

翻滚是 PoE2 的**核心主动防御**，所有角色默认拥有，绑定空格（默认），**不占技能位、无冷却、无消耗**，**不被视为技能**（因此不受技能速度 / 施法速度修饰词影响）[^poe2wiki-dodgeroll][^mobalytics-dodgeroll]。

### 1.1 无敌帧 (i-frames) 与可躲避范围

- 翻滚动画**前半段**提供 **avoidance frames（无敌帧）**：保证躲避此期间**可被躲避的**击中；**后半段**角色恢复可受击状态、且移动距离变短[^poe2wiki-dodgeroll][^sportskeeda-dodge]。
- 默认**仅 Projectile 与 Strike 可被翻滚躲避**；**持续伤害 (DoT)、地面效果、范围 (Area) 击中默认无法躲避**[^mobalytics-dodgeroll]。Boss 大型 AoE / 砸地必须靠**移出范围**而非原地翻滚。
- **翻滚躲避 ≠ 闪避 (Evasion)**：被翻滚躲掉的击中**不算 Evaded**，闪避值与翻滚**完全不交互**[^mobalytics-dodgeroll]（与 [evasion.md](./evasion.md) 的熵值 / 命中检定无关）。但被翻滚躲掉的击中和被闪避的击中一样**在伤害计算第 1 步即被丢弃**，因此也不会施加流血等需要命中连接的 ailment。

### 1.2 距离 / 速度 / 与 ActionSpeed 的关系

- 默认翻滚移动 **3.7 米**[^poe2wiki-dodgeroll]。
- **翻滚全程总距离 = 同时长内正常行走的距离**——前半段快、后半段慢，平均速度等于移动速度。因此**狂按翻滚不会比走路更快**[^mobalytics-dodgeroll][^mmojugg-dodge]。
- **翻滚速度 / 距离按移动速度 (Movement Speed) 缩放**；翻滚不受技能速度影响，但受 **Action Speed**（行动速度，如冰缓 / Temporal Chains 拖慢，Tailwind 加速；见 [recovery-charges-buffs.md](./recovery-charges-buffs.md) §3.4）整体影响。
- **「+N 米翻滚距离」类修饰词**（如 `Surefooted Sigil` 护身符 +1m、`Lioneye's Glare`）**不延长动画时长**，而是在同样时间内覆盖更长距离——等价于翻滚的**速度乘区**[^poe2wiki-dodgeroll]。
- **取消动画**：翻滚可取消几乎任何当前动画（唯独不能取消另一次翻滚），常用于砍掉手雷 / 盾冲等长后摇，提高有效机动。
- **冲刺 (Sprint)**：长按翻滚键会先翻滚再进入冲刺，大幅提升移速直到取消（PoB2 `Data/Misc.lua`: `dodge_roll_sprint_minimum_time_ms = 100`、`player_allow_dodge_roll_cancel = 1`）。

### 1.3 改变翻滚行为的词条 / Keystone

- **`Bulwark`（Keystone）**：**移除翻滚的无敌帧**，改为「翻滚期间受到的击中伤害 **30% less**」——把翻滚从「躲避」变成「减伤」，用于硬抗本来无法躲避的 Boss 大招[^poe2wiki-dodgeroll][^mobalytics-dodgeroll]。
- **`Dodge Roll Avoids all Hits`**（如传奇靴 `Ab Aeterno`）：让翻滚能躲避**原本无法躲避的范围击中**（功能上类似旧 Acrobatics 之于闪避）。
- **`Unwavering Stance`（Keystone）**：**无法翻滚或冲刺**（换取不会被轻眩晕），与翻滚互斥。
- **`Dodge Roll passes through Enemies`**（如传奇靴）：翻滚穿过敌人；翻滚本身会把角色碰撞体积设为 0，可从夹缝中滚出包围。

---

## 二、守护吸收 (Guard) 与 Molten Shell

PoE2 的 **Guard** 是一个**增益 (Buff)**（不是 PoE1 那样的技能类型）：提供一个**优先于生命 / 能量护盾**吸收击中伤害的缓冲池[^poe2db-guard][^mobalytics-guard]。

### 2.1 Guard buff 核心规则

- **仅吸收击中 (Hit) 伤害**，不与 DoT 交互[^mobalytics-guard]。
- **同时只能有一个 Guard buff**。再获得 Guard 时，**保留 magnitude 更高者**；被丢弃的低 magnitude buff 会**按其 magnitude 比例刷新**保留 buff 的持续时间（但不超过其原始基础时长）[^poe2db-guard]。
- **Guard 上限 = 最大生命的 200%**[^poe2db-guard]。
- Guard 池在伤害承受顺序中位于 **ES / 生命之前**（见 [damage-defence-order.md](./damage-defence-order.md) §8 步骤 3「守护」）。

### 2.2 Guard 来源

- **Guard 技能 (Guard skills)**：带 `Guard` tag、**共享一条冷却**。代表是 **Molten Shell**：施放时附加额外护甲，并把一部分击中伤害从该 buff 扣除；buff 到期 / 耗尽时按累计吸收量向周围爆发火焰伤害[^poe2db-moltenshell]。
  - **Molten Shell 的吸收量与护甲挂钩**（护甲越高吸收越多），因此是护甲流的主动配套。
  - `Vaal Molten Shell` 在 PoB2 中作为**独立 guard 槽**处理（可与普通 guard 并存）。
- **战吼 / 升华**：`Fortifying Cry`（按范围内敌人 Power 授予 Guard）、Warbringer 的 `Encase in Jade`（消耗 Jade 层按最大生命授予巨额 Guard）。
- **药剂前缀 / 词条**：「Also grants N Guard」「生命药剂回复的一部分授予 Guard」「超额生命恢复转为 Guard」。

### 2.3 Guard vs 其它「伤害吸收 (Damage Absorption)」

> **重要区分**：**所有 Guard 都是伤害吸收，但并非所有伤害吸收都是 Guard**[^poe2db-guard][^mobalytics-guard]。
> - **Guard buff**：受「单 buff、200% 生命上限、magnitude 覆盖」规则约束。
> - **非 Guard 吸收**：如 Witchhunter 的 **`Sorcery Ward`**、**`Encase in Jade`**（物体）等——同样在生命 / ES 前吸收，但**不占 Guard 槽、不受 Guard 专属规则限制**。PoB2 把这些与 Guard 一起归入「Damage Absorption」承受层（见 [damage-defence-order.md](./damage-defence-order.md) §8）。

---

## 三、规避 (Avoidance)

**规避 (Avoidance)** = 「N% 几率**完全避免**某种击中 / 异常 / 控制」。与抗性（reduce magnitude / duration）不同：规避是**全有或全无的几率检定**，命中即 0 影响、未命中即正常。

### 3.1 规避「伤害 / 击中」

- **`N% chance to Avoid all Damage from Hits`**：对**所有**击中按几率完全免伤，**默认上限 75%**（PoB2 `data.misc.AvoidChanceCap = 75`）。`Elusive` buff 提供 15% × effect 的此项（见 [recovery-charges-buffs.md](./recovery-charges-buffs.md) §3.3）。
- **`Avoid <Type> Damage`**（按伤害类型）/ **`Avoid Projectiles`**：同样 75% 上限。注意：存在「按类型规避」时，PoB2 用 `specificTypeAvoidance` 标记并使 `AvoidProjectiles` 不再重复叠进通用链（避免双计）。
- 这些层与闪避 / 翻滚一起进入「**命中前不被击中几率**」乘法链（详见 [recovery-charges-buffs.md](./recovery-charges-buffs.md) §4.3，本文不重复公式）。

### 3.2 规避「异常 / 控制」

PoE2 实际存在的规避（已在 0.5.0 stat_descriptions 核实）：

| 规避项 | 真实 stat 文本 | 上限 |
|--------|---------------|------|
| 眩晕 | `% chance to Avoid being Stunned` | 100% |
| 点燃 | `% chance to Avoid being Ignited` | 100% |
| 感电 | `% chance to Avoid being Shocked` | 100% |
| 冰缓 | `% chance to Avoid being Chilled` | 100% |
| 冰冻 | `% chance to Avoid being Frozen` | 100% |
| 中毒 | `% chance to Avoid being Poisoned` | 100% |
| 流血 | `% chance to Avoid Bleeding` | 100% |
| 元素异常（全部）| `% chance to Avoid Elemental Ailments` | 100% |

- **「Avoid being X」与「X Immune」等价于 100% 规避**（PoB2：`Flag(ailment.."Immune")` → 直接置 100）。
- **眩晕的隐式规避**：**受击时身上有任意能量护盾 → 50% 几率避免眩晕**（PoB2 `CalcDefence.lua` 注释明确，把 `notAvoidChance × 0.5`）。这是 ES 角色「天然抗眩晕」的根因，且独立于 `AvoidStun` 词条。
- **联动旗标**：`ShockAvoidAppliesToElementalAilments`（`Stormshroud`：感电规避也作用于全元素异常）、`SpellSuppressionAppliesToAilmentAvoidance`（`Ancestral Vision`，遗留路径，见 §6）。
- **核实不存在**：PoE2 **没有**「Avoid being Critically Hit / 规避暴击」这一项。对暴击的防御走**「减少受到的暴击额外伤害」**（§4）和**闪避二次命中检定降级暴击**（见 [evasion.md](./evasion.md) §闪避与暴击 / [critical-hits.md](./critical-hits.md) §对暴击的防御）。

### 3.3 规避 vs 抵抗（reduced effect / duration）

- **规避**：几率完全免疫（上表）。
- **抵抗**：降低异常的**强度 (magnitude)** 或**持续时间 (duration)**，如「reduced Effect of Chill on you」「reduced Ignite Duration on you」（与异常**阈值 (Ailment Threshold)** 体系配合，见 [ailments.md](./ailments.md)）。
- 两者**独立叠加**：可同时「30% 几率避免被点燃」+「点燃对你的持续时间 −40%」。

---

## 四、减少受到的暴击额外伤害 (Reduced Extra Damage from Critical Hits)

这是 PoE2 对暴击的**专属减伤层**，作用于**暴击相对普通击中多出来的那部分伤害（爆伤 bonus）**，而非整体伤害。

- 真实 stat：`You take N% reduced Extra Damage from Critical Hits`，以及极致版 `Take no Extra Damage from Critical Hits`（= 把暴击降级为普通击中的伤害）[^poe2db-critweakness]。
- **PoB2 建模**（`CalcDefence.lua`）：
  ```lua
  output.CritExtraDamageReduction = min( Sum("BASE","ReduceCritExtraDamage"), 100 )   -- 上限 100%
  output.EnemyCritEffect = 1 + enemyCritChance/100 * (enemyCritDamage/100) * (1 - CritExtraDamageReduction/100)
  ```
  即它只缩放敌人爆伤项 `enemyCritDamage`，**不影响**敌人的基础击中伤害。100% 时等效「不吃暴击额外伤害」。
- **来源举例**：`per Endurance Charge`、`if you've taken a Critical Hit Recently`、被诅咒 / 中毒敌人相关（双向：increased/reduced 都有）。
- 它也会按比例削弱**暴击施加的异常**强度（PoB2 `enemyCritAilmentEffect` 同样乘 `(1 − CritExtraDamageReduction/100)`）。

---

## 五、防御向 Keystone（PoE2 0.5.0 现存）

PoE2 0.5.0 共 **33 个 Keystone**（另 8 个 Timeless Jewel Keystone）[^poe2db-keystone]。下表只列**防御 / 生存相关**且**经核实存在**的项；与既有文档已详述的（CI 见 [energy-shield.md](./energy-shield.md)）只标注交叉引用。

| Keystone | 效果（0.5.0 核实文本） | 防御意义 |
|----------|----------------------|---------|
| **Chaos Inoculation** | 最大生命变 1；免疫混沌伤害与流血 | 见 [energy-shield.md](./energy-shield.md) §CI |
| **Eldritch Battery** | 所有 ES 转为法力；法力消耗翻倍 | ES 当法力池；放弃 ES 作为受击缓冲 |
| **Mind Over Matter** | 所有伤害先从法力承受；法力恢复速率 −50% | 法力当额外血池（承受顺序见 [damage-defence-order.md](./damage-defence-order.md) §8） |
| **Iron Reflexes** | 所有闪避值转为护甲 | 闪避流并入护甲减伤（见 [armour.md](./armour.md)/[evasion.md](./evasion.md)） |
| **Zealot's Oath**（即 PoB 旗标 `ZealotsOath`）| 生命再生溢出转 ES（`Excess Life Recovery from Regeneration applied to ES; ES does not Recharge`）| ES 改由再生而非充能恢复 |
| **Resolute Technique** | 精准翻倍；**永不暴击** | 不是防御，但「无法暴击」总是优先于「保证暴击」（见 [critical-hits.md](./critical-hits.md)） |
| **Unwavering Stance** | **无法被轻眩晕**；无法翻滚 / 冲刺 | 眩晕免疫换机动（与翻滚互斥，§1.3） |
| **Bulwark** | 翻滚无法躲避伤害；翻滚期间受击 **30% less** | 把翻滚改成减伤层（§1.3） |
| **Blood Magic** | 无法力；技能法力消耗转为生命消耗 | 释放装备保留 / 法力层，但失去 MoM 协同 |
| **The Agnostic**（Timeless / 信仰）| DoT 绕过你的 ES；非满生命时每秒牺牲 1% 最大法力回复等量生命 | 法力当生命恢复池 |
| **「Take 50% less Damage over Time if started recently / 50% more if not」** | （DoT 反应型 Keystone）| 对持续伤害的条件减伤层 |
| **Avatar of Fire** | 75% 伤害转火；不造非火伤害 | 主要进攻向，附带统一伤害类型 |

> **0.5.0 重做提示**：`Ancestral Bond`、`Trusted Kinship`、`Vaal Pact` 等 Keystone 在 0.5.0 被重做[^poebuilds-050][^game8-050]；ES 充能相关被动被大量从「Recharge Rate」改为「faster Recharge Start」。具体数值实现时以一手数据为准。

---

## 六、PoE1 有、PoE2 已移除 / 退化的机制（核实）

为避免照搬 PoE1，以下逐一标注 PoE2 实际状态：

- **法术压制 (Spell Suppression)** —— ❌ **PoE2 普通构建已移除**。因 PoE2 取消「攻击 vs 法术」防御二分，Spell Suppression（半化法术伤害）失去存在基础[^mmojugg-monster][^mobalytics-mechanics]。
  - **注意**：PoB2 代码里**仍保留** `SpellSuppressionChance` / `SuppressionChanceCap=100` / `SuppressionEffect=50` 等变量，但这些只能由 **Timeless Jewel（Glorious Vanity 等）转化出的遗留小点**或个别遗留路径触发，**不在常规天赋树 / 装备词缀池中**[^poe2db-suppression]。
  - ⚠️ **既有 [block.md](./block.md) 的「法术压制」一节按 PoE1 语义描述，对 0.5.0 不准确**——常规 PoE2 没有 50% 半化法术的可堆叠层；应以本节为准。
- **Acrobatics（Keystone）** —— ❌ **0.3.0 移除**。0.3.0 起「闪避对**除红闪 Boss 技能外的所有击中**生效」，等于把旧 Acrobatics 的能力内置给了所有闪避角色，故 Keystone 被删（同时闪避公式被下调以平衡，见 [evasion.md](./evasion.md)）[^thegamer-acro][^poe2wiki-acro][^sportskeeda-evasion]。
  - PoB2 残留 `ConvertSpellSuppressionToSpellDodge`（"Acrobatics"）属遗留代码。
- **Attack/Spell Dodge（几率躲避，stat 型）** —— ⚠️ **退化**。PoB2 仍有 `AttackDodgeChance` / `SpellDodgeChance`（`DodgeChanceCap=75`），但常规 PoE2 已无对应词缀来源，主要是遗留 / Timeless Jewel 路径；玩家面对的「躲避」是**主动翻滚**（§1）而非这套几率 stat。
- **Deflection（偏转）** —— ✅ **存在但经历反复**：曾随「移除攻击/法术二分」一度被取消，后由 `The Third Edict` 重新引入，0.5.0 公式再调（DEX/闪避流减伤层）。**命中型偏转公式见 [evasion.md](./evasion.md) §偏转**，本文不重复（PoB2：`DeflectionChanceCap=95`、`DeflectEffect`）。

---

## PoB2 计算实现（核对基准）

变量 / 旗标取自 [PathOfBuilding-PoE2 `dev`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) 的 `src/Modules/CalcDefence.lua`、`CalcPerform.lua`、`src/Modules/Data.lua`、`src/Data/Misc.lua`，是 pobr 的回归基准：

**caps（`Modules/Data.lua` 的 `data.misc`）**
```lua
DamageReductionCap = 90        -- 物理减伤 / 护甲减伤硬上限
MaxResistCap = 90;  EnemyMaxResist = ...;  ResistFloor = -200
EvadeChanceCap = DefaultMaxEvadeChancePercent   -- 闪避上限(见 evasion.md)
DodgeChanceCap = 75            -- stat 型 dodge（遗留）
AvoidChanceCap = 75            -- 规避伤害/击中上限
BlockChanceCap = 90            -- 见 block.md
SuppressionChanceCap = 100; SuppressionEffect = 50   -- 遗留，常规不可用
DeflectionChanceCap = 95; DeflectEffect = ...        -- 偏转(见 evasion.md)
ArmourRatio = 10; NegArmourDmgBonusCap = 100; MinStunChanceNeeded = 20
```

**规避（`CalcDefence.lua`）**
```lua
output["Avoid"..type.."DamageChance"] = min(Sum("BASE","Avoid"..type.."DamageChance"), AvoidChanceCap)
output.AvoidAllDamageFromHitsChance  = min(Sum("BASE","AvoidAllDamageFromHitsChance"), AvoidChanceCap)
output[ailment.."AvoidChance"] = Flag(ailment.."Immune","ElementalAilmentImmune") and 100
    or floor(min(Sum("BASE","Avoid"..ailment, "AvoidAilments", "AvoidElementalAilments")
                 + (shockAppliesToAll and Sum("BASE","AvoidShock") or 0), 100))
-- 眩晕：notAvoidChance = StunImmune and 0 or 100 - min(Sum("BASE","AvoidStun"),100)
--      受击时有 ES → notAvoidChance *= 0.5  （ES 50% 避免眩晕，注释明确）
-- 联动旗标：ShockAvoidAppliesToElementalAilments(Stormshroud)、
--          SpellSuppressionAppliesToAilmentAvoidance(Ancestral Vision，遗留)
```

**暴击额外伤害减免（`CalcDefence.lua`）**
```lua
CritExtraDamageReduction = min(Sum("BASE","ReduceCritExtraDamage"), 100)
EnemyCritEffect = 1 + enemyCritChance/100 * (enemyCritDamage/100) * (1 - CritExtraDamageReduction/100)
-- 异常侧 enemyCritAilmentEffect 同样乘 (1 - CritExtraDamageReduction/100)
```

**Guard（`CalcPerform.lua` / `CalcDefence.lua`）**
```lua
-- buff 合并：单一 guard 槽；"Vaal Molten Shell" 独立槽
guards = {}; if name == "Vaal Molten Shell" then 独立 else 取 magnitude 高者 end
modDB.conditions["AffectedByGuardSkill"] = true; ["AffectedBy"..guardName] = true
-- 承受顺序里的吸收：sharedGuardAbsorb / <type>GuardAbsorb，按 GuardAbsorbRate(<=100) 扣
sharedGuardAbsorbRate = min(Sum("BASE","GuardAbsorbRate"), 100)
sharedGuardAbsorb     = calcLib.val(modDB, "GuardAbsorbLimit")
-- guard 上限 = 200% 最大生命（poe2db）
```

**翻滚 / Bulwark（`Data/Misc.lua` 常量 + Keystone 注入）**
```lua
player_allow_dodge_roll_cancel = 1; dodge_roll_sprint_minimum_time_ms = 100
-- 基础翻滚距离 3.7m（一手），随 Movement Speed/ActionSpeed 缩放，距离词条=速度乘区
-- Bulwark: 移除翻滚无敌帧 → 翻滚期间 DamageTakenWhenHit MORE -30%（旗标/条件实现）
-- Elusive: NewMod("AvoidAllDamageFromHitsChance","BASE", floor(15*effect), "Elusive")
```

**命中前不被击中几率（`CalcDefence.lua` ~2018，详见 recovery-charges-buffs §4.3，此处仅指针）**
```lua
MeleeNotHitChance = 100 - (1-Evade)*(1-EffectiveAttackDodge)*(1-AvoidAllDamageFromHits)*100
-- ProjectileNotHitChance 额外乘 (1 - AvoidProjectiles)（若无 specificTypeAvoidance）
```

---

## 对 pobr 实现的启示

对照 `pobr-core`（`mod_db.rs` / `config.rs::CalcConfig` / `calc/defence.rs` / `trace.rs`）落地建议：

1. **翻滚是「时间窗口型主动防御」，DPS / EHP 计算里应作为可配置开关而非常驻层。**
   - 翻滚的无敌帧不进入稳态 EHP；建议作为 `CalcConfig` 的一个**情景开关**（如 `Condition:DodgeRolling`），用于评估 `Bulwark`（注入 `DamageTakenWhenHit` MORE −30）/「Dodge Roll Avoids all Hits」等条件型词条。距离 / 速度只影响机动，不进伤害公式——但「翻滚距离词条 = 速度乘区」这一语义若做机动模拟需注意。

2. **规避独立成层，三类上限分清。**
   - `AvoidAllDamageFromHits` / `Avoid<Type>` / `AvoidProjectiles`：BASE 求和后 `min(_, 75)`；异常规避 `min(_, 100)`，`Immune` 旗标直接置 100。
   - **ES 50% 避免眩晕**要作为隐式规则（受击时 ES>0 → `AvoidStun` 等效 +50% 几率，乘法），不要漏。
   - 规避（几率全免）与抵抗（reduced effect/duration）是**两套机制**，分别建模、独立叠加。

3. **暴击额外伤害减免要只作用于「爆伤项」。**
   - 实现 `CritExtraDamageReduction = min(Σ ReduceCritExtraDamage, 100)`，仅乘敌人爆伤 bonus（`EnemyCritEffect` 公式），**不要**误乘基础击中伤害；并让它同步缩放暴击施加的异常。这与攻击侧 `critical-hits.md` 的爆伤是**对偶**关系。

4. **Guard / 伤害吸收作为承受顺序里 ES 之前的独立池。**
   - 建模为：单一 Guard 槽（magnitude 覆盖 + 比例刷新时长）、上限 200% 生命、`GuardAbsorbRate`（按类型 / shared）、`Vaal Molten Shell` 独立槽；并把 `Sorcery Ward` / `Encase in Jade` 归入「非 Guard 吸收」同层但不占 Guard 槽。承受顺序见 [damage-defence-order.md](./damage-defence-order.md) §8。

5. **Keystone 用「旗标 → 注入若干 Modifier / 改写承受管线」实现，便于归因。**
   - 如 `IronReflexes`（闪避→护甲转换）、`MindOverMatter`（承受顺序插入法力层）、`Bulwark`（翻滚减伤）、`ZealotsOath`（再生转 ES）。每个 Keystone 的影响都应能在 `TraceGraph` 回溯到该 `SourceId`——这是 pobr 相对 PoB 的增量价值。

6. **不要照搬 PoE1：Spell Suppression / Acrobatics / stat 型 Dodge 在常规 PoE2 不存在。**
   - 即使 PoB2 代码保留这些变量，pobr 数据管线应只在 Timeless Jewel / 遗留路径暴露它们，默认词缀池不含；闪避对「除红闪 Boss 外所有击中」生效（0.3.0+）应作为闪避的默认语义（见 [evasion.md](./evasion.md)）。

---

## 参考来源

[^poe2wiki-dodgeroll]: PoE2 Wiki — Dodge roll. https://www.poe2wiki.net/wiki/Dodge_roll
[^mobalytics-dodgeroll]: Mobalytics — PoE 2 Guide: Dodge Roll Explained. https://mobalytics.gg/poe-2/guides/dodge-roll-mechanic
[^sportskeeda-dodge]: Sportskeeda — PoE2 dodge-roll system & i-frames. https://www.sportskeeda.com/mmo/path-exile-2-poe2-dodge-roll-system-iframes
[^mmojugg-dodge]: MMOJUGG — Path of Exile 2 New Dodge Roll Overview. https://www.mmojugg.com/news/path-of-exile-2-new-dodge-roll-overview.html
[^poe2db-guard]: PoE2DB — Guard. https://poe2db.tw/us/Guard
[^mobalytics-guard]: Mobalytics — PoE 2 Guide: Guard Explained. https://mobalytics.gg/poe-2/guides/guard
[^poe2db-moltenshell]: PoE2DB — Molten Shell. https://poe2db.tw/us/Molten_Shell
[^poe2db-keystone]: PoE2DB — Keystone（Keystone Passive /33）. https://poe2db.tw/us/Keystone
[^poe2db-critweakness]: PoE2 stat_descriptions — `You take N% reduced Extra Damage from Critical Hits` / `Take no Extra Damage from Critical Hits`（vendor `Data/StatDescriptions/stat_descriptions.lua`）。
[^poe2db-suppression]: PoE2DB — Spell Suppression（仅 Timeless Jewel / 遗留来源）. https://poe2db.tw/us/Spell_Suppression
[^mmojugg-monster]: MMOJUGG — Path of Exile 2 Monster Damage System（移除攻击/法术二分、Spell Suppression & Deflection 移除）. https://www.mmojugg.com/news/path-of-exile-2-monster-damage-system.html
[^mobalytics-mechanics]: Mobalytics — PoE 2 Mechanics Guide（Spell Suppression no longer exists）. https://mobalytics.gg/poe-2/guides/mechanics
[^thegamer-acro]: TheGamer — PoE2 0.3.0 Removed the Acrobatics Keystone. https://www.thegamer.com/path-of-exile-2-acrobatics-keystone-removed/
[^poe2wiki-acro]: PoE2 Wiki — Acrobatics（已移除）. https://www.poe2wiki.net/wiki/Acrobatics
[^sportskeeda-evasion]: Sportskeeda — Evasion buff ahead of 0.3（闪避对除红闪外所有击中生效）. https://www.sportskeeda.com/mmo/path-exile-2-evasion-buff-0-3-launch-poe2
[^poebuilds-050]: poebuilds.net — PoE2 0.5.0 Patch Notes（Keystone 重做：Ancestral Bond/Trusted Kinship/Vaal Pact；ES Recharge 重构）. https://www.poebuilds.net/post/path-of-exile-2-0-5-0-patch-notes
[^game8-050]: Game8 — PoE2 0.5.0 Full Patch Notes & Summary. https://game8.co/games/Path-of-Exile-2/archives/601782
[^pob2-calcdefence]: PathOfBuilding-PoE2 — `src/Modules/CalcDefence.lua`（规避/暴击减伤/Guard 吸收/NotHitChance/caps 应用）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcDefence.lua
[^pob2-calcperform]: PathOfBuilding-PoE2 — `src/Modules/CalcPerform.lua`（Guard buff 合并、Elusive 注入 AvoidAllDamageFromHits）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcPerform.lua
[^pob2-data]: PathOfBuilding-PoE2 — `src/Modules/Data.lua`（`data.misc`：AvoidChanceCap=75、DodgeChanceCap=75、SuppressionChanceCap=100/Effect=50、DeflectionChanceCap=95、DamageReductionCap=90、MaxResistCap=90）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/Data.lua
[^pob2-misc]: PathOfBuilding-PoE2 — `src/Data/Misc.lua`（`player_allow_dodge_roll_cancel`、`dodge_roll_sprint_minimum_time_ms`）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Misc.lua
