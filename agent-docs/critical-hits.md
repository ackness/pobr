# 暴击机制 (Critical Hits)

暴击是击中时使伤害获得基于**暴击伤害加成 (Critical Damage Bonus)** 的伤害倍增器的效果[^mobalytics-crit]。暴击的额外伤害不是单独作为一个伤害包处理的，而是原始击中伤害的乘数。

**注意**：持续伤害 (damage over time) 不能暴击。但由暴击击中施加的 damaging ailment（如点燃/流血/中毒），其基础伤害取自那次暴击击中的伤害，因此会相应放大。

> 本文档把 PoE2 暴击拆成几个常被忽略的细分概念：**爆伤加成 (Critical Damage Bonus)**、**幸运/不幸 (Lucky/Unlucky)**、**重投 (Rerolling)**、**分岔暴击 (Bifurcated)**、**必然暴击 (Inevitable)**、**暴击弱点 (Critical Weakness，含光环版 Malice)**，以及它们在 PoB2 `CalcOffence.lua` 中的建模方式（见末尾「PoB2 计算实现」）。

## 暴击检定 (Critical Hit Check)

技能使用或被触发时，滚动一个 0 到 99.99 之间的随机数，作为该次技能使用的暴击**阈值**——技能的暴击几率必须**超过**该阈值才会暴击[^poe2wiki-crit]。

例如：施放法术滚到 12，则本次技能的所有击中都需要 ≥12.01% 的暴击几率才会暴击。

- **全有或全无**：同一次技能使用产生的所有击中通常共享同一次阈值滚动，要么全暴击、要么全不暴击。但条件型修饰词、buff/debuff 会让不同敌人的暴击几率不同（例如只有部分敌人身上有暴击弱点），从而打破全有或全无。
- **持续/引导技能例外**：Sustained（持续）技能与 Channelling（引导，如 `Incinerate`）按**每次击中 / 每个引导间隔**独立滚动暴击阈值，而不是整个技能滚一次。这种独立滚动**不算重投 (Rerolling)**。
- **怪物多投射物**：怪物的多投射物技能仅在技能使用时检查一次，因此所有击中要么全暴击、要么全不暴击。

## 基础暴击率来源

- **攻击 (Attacks)**：基础暴击率来自所用**武器底材**。空手攻击默认 **5%**[^pob2-deepwiki-crit]。例外——副手攻击（如 `Shield Charge`）、某些空手技能（如 `Shattering Palm`）使用**技能宝石**自带的基础暴击率，类似法术。
- **法术 (Spells)**：基础暴击率写在**技能宝石**上。常见默认值约 7%/12%/15%，按技能定义覆盖[^pob2-deepwiki-crit]。
- **双持武器**：
  - 同时使用两把武器的技能：每只手分别计算暴击几率与伤害，再合并为一次伤害实例。任一手暴击即视为该次合并击中为一次暴击；但这**不会**回头把没暴击的那只手缩放为暴击。
  - 独立使用两把武器的技能：每次击中独立计算暴击几率。

## 缩放暴击几率

有两类修饰词缩放暴击几率，**计算顺序**为：先加固定基础暴击率，再乘倍增器。

```
最终暴击率 = (基础暴击率 + Σ固定加成) × (1 + Σincreased/100) × Π(1 + more/100)
```

### 固定基础暴击率加成 (Flat / Base)

较稀有，在任何倍增器之前直接加到基础暴击率上：
- 被动天赋 notable（如 `Struck Through` 给攻击 +1% 基础暴击率）
- `Pillar of the Caged God` 等传奇（10% → 11%）
- **暴击弱点 (Critical Weakness)** 与 **Critical Strike** 类 debuff（见下文）

因为它们提高了"被倍增器相乘的基数"，**固定基础暴击率极其强大**——会让所有 increased 暴击几率更高效。

### 倍增器修饰词 (Increased / More)

更常见：
- `increased/reduced`（如 `Heartstopping`）加法叠加：基础 7% + 75% increased = `7% × 1.75 = 12.25%`
- `more/less`（如 `Charge Regulation`）彼此之间乘法叠加

### 暴击几率上限 (Crit Chance Cap)

最终暴击几率默认上限 **100%**（PoB2 中由 `CritChanceCap` 覆盖/求和），并取下限 0。超过 100% 即为"溢出"，无收益。

## 暴击伤害加成 (Critical Damage Bonus / 爆伤)

爆伤是暴击相对普通击中**额外**乘上的伤害倍率，等价于 PoE1 的 "critical strike multiplier / crit multi"。它是**独立于其它所有伤害倍率**的唯一乘数。

- **玩家与召唤物默认 +100%**，即暴击造成基础伤害的 **200%**（双倍）[^mobalytics-crit][^poe2wiki-crit]。
- **怪物的暴击额外伤害比玩家少 40%**（"40% less bonus critical damage"，0.1.0 起）[^poe2wiki-crit]。

### 计算顺序

1. 先把所有 `+#% to Critical Damage Bonus`（固定加成，如武器词条、`Sniper's Mark`、`Soul Core of Ticaba`）加到基础 **+100%** 上；
2. 再施加 `increased/reduced` 与 `more/less` 倍率[^poe2wiki-crit]。

### 局部 (local) vs 全局

武器上的 `+#% to Critical Damage Bonus`、以及嵌入武器的 `Soul Core of Ticaba`（`Martial Weapon: +5% to Critical Damage Bonus`）是**局部修饰词**，只对该武器造成的击中生效，不会加到法术/其它武器。

### 来源举例

- **`Sniper's Mark`**：被标记敌人受到的下一次暴击有 `(20-77)% increased Critical Damage Bonus`，并触发标记。
- **`Soul Core of Ticaba`**：嵌入武器 +5% 爆伤；嵌入身甲/盾"对你的击中减少 20% 爆伤"（防御向）。
- **`Battle-hardened`** 等：防御者可拥有"减少受到的暴击伤害加成"。

### 爆伤作用于持续伤害 (CritMultiplierAppliesToDegen)

特定词条可让一部分爆伤作用于持续伤害（PoB2 `BonusCritDotMultiplier = (爆伤BASE − 50) × CritMultiplierAppliesToDegen / 10000`）。默认持续伤害不享受爆伤。

## 暴击的特殊滚动行为

### 幸运 / 不幸 (Lucky / Unlucky)

- **幸运暴击几率 (Crit Chance Lucky)**：暴击阈值滚两次，取较好（较低阈值）的结果——等价于"暴击几率滚两次取暴击"。
- **不幸暴击几率 (Unlucky)**：滚两次取较差结果。
- 幸运与不幸会相互抵消。
- 等效暴击几率（概率视角）：`1 − (1 − c)²`，其中 `c` 为单次暴击几率。例如 30% → `1 − 0.7² = 51%`。

> 同理还有**幸运/不幸伤害 (Lucky/Unlucky Damage)**：伤害掷骰两次取更优/更差结果，二者也相互抵消。

### 重投 (Rerolling)

任何"对单次击中的暴击几率检定超过一次"的机制都算 **Rerolling**，包括使暴击几率变为 Lucky / Unlucky / Bifurcated / Inevitable 的效果。注意：Sustained/Channelling 按击中独立滚动**不算**重投。

### 分岔暴击 (Bifurcated Critical Hits)

用带此属性的武器击中时，暴击几率**滚两次**[^poe2wiki-bifurcate]：

- **任一次**成功 → 该击中为暴击，正常施加一份爆伤；
- **两次都**成功 → 该击中为暴击，爆伤**施加两次**（额外一份）；
- 若有效果使暴击几率为 Lucky，该 Luck 会**分别**作用于这两次滚动。

来源举例：长矛/连枷底材隐式 `Bifurcates Critical Hits`、`Tangletongue` 等传奇。

效果上，分岔既提升等效暴击率（`1 − (1 − c)²`），又因"两次都暴击"的概率额外提供一份爆伤（期望视角的 more 爆伤）。

### 必然暴击 (Inevitable Critical Hits)

带"必然暴击"的击中如果本可暴击但没滚出暴击，会**反复重投暴击几率直到成功**——因此**每次击中都暴击**[^poe2wiki-inevitable]。代价：每多重投一次，该击中**减少 30% 爆伤 (30% less Critical Damage Bonus per reroll)**。

PoB2 用几何级数计算期望爆伤倍率（第 N 次才暴击的概率 × 对应的爆伤档：100% / 70% / 40% / 10% …），并把结果折成一条 `CritMultiplier MORE` 负值，同时把暴击几率视为 100%。`Inevitable Criticals` 辅助宝石还提供"距上次必然暴击每秒 +暴击几率（有上限）"的递增。

## 暴击弱点 (Critical Weakness)

**可叠加的 debuff**：使对被影响目标的击中获得 **+0.5% 基础暴击几率/层**，最多 **20 层 = +10% 基础暴击几率**[^poe2db-critweakness][^poe2wiki-critweakness]。

- 该加成作用于**基础暴击几率**，在任何 increased/reduced 或倍率**之前**结算（与上文"固定基础暴击率加成"同一阶段）。
- 默认每层持续 **4 秒**；0.2.0 起每层**独立计时**（不再因新层刷新全部层），整体被削弱。

### 来源（含"暴击弱点光环"）

- **`Malice`（权杖技能 / presence 光环）**：对处于你 presence 内的敌人持续施加暴击弱点——这就是俗称的"**暴击弱点光环**"。0.2.0 起 `Malice` 取代了 `Eye of Winter` 成为主要来源。
- **`Reap`**（法术）：命中区域施加暴击弱点。
- **战吼 (Warcry)**：可"对敌人施加 3 层暴击弱点"。
- **标记 (Mark)**：可"消耗标记时对敌人施加 10 层暴击弱点"。
- 某些传奇（如头盔）`Enemies in Presence Gain Crit Weakness`。

## 其它与暴击联动的机制

- **`Blindside`（辅助）**：对**致盲 (Blinded)** 敌人更易暴击，且对致盲敌人的暴击造成更多伤害。
- **暴击触发双倍/三倍伤害**：`Double Damage Chance on Crit` / `Triple Damage Chance on Crit`——暴击时按概率叠加双倍/三倍伤害（三倍覆盖双倍）。
- **暴击斩杀 (Crit Culling)**：部分效果让暴击具有斩杀（按敌人稀有度的斩杀阈值）。
- **敌方受到的暴击倍率 (SelfCritMultiplier)**：`Sniper's Mark`、部分 debuff 让目标"受到的暴击伤害加成增加"，在玩家爆伤之上再乘敌方系数。

## 对暴击的防御

### 闪避：二次命中检定降级暴击

当一次会做命中检定 (accuracy check) 的击中滚出暴击时，会再做一次命中检定；若失败，暴击被**降级为普通击中**（击中本身仍连接，因为已通过首次检定）[^mobalytics-evasion]。

PoB2 直接体现为：`有效暴击几率 = 暴击几率 × 命中率`（见下）。因此高闪避（拉低攻击者命中率）能成比例削弱暴击。

### 阻止 / 保证暴击的优先级

- **`Resolute Technique`**：完全无法造成暴击。
- **`Sunder`** 等"保证暴击"效果会被 `Resolute Technique` 覆盖——**阻止暴击的效果总是优先于保证暴击的效果**[^poe2wiki-crit]。
- 部分辅助（如使法术"无法暴击换取 25% more 击中伤害"）也会移除暴击能力。

## PoB2 计算实现（`CalcOffence.lua`，核对基准）

以下变量/公式取自 [PathOfBuilding-PoE2 `src/Modules/CalcOffence.lua`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcOffence.lua) 暴击段，是 pobr 的回归基准：

**暴击几率（按结算顺序）**

```lua
-- 1) 基础求和（敌方 SelfCritChance，如暴击弱点，加在 base 上）
base = Sum("BASE","CritChance") + enemy:Sum("BASE","SelfCritChance")
inc  = Sum("INC","CritChance")  + enemy:Sum("INC","SelfCritChance")
more = More("CritChance")
output.CritChance = (baseCrit + base) * (1 + inc/100) * more   -- clamp 到 CritChanceCap(默认100) 与 0
output.PreEffectiveCritChance = output.CritChance

-- 2) 闪避降级：乘命中率
if mode_effective then output.CritChance = output.CritChance * output.AccuracyHitChance / 100 end

-- 3) 幸运暴击
if Flag("CritChanceLucky") then output.CritChance = (1 - (1 - c)^2) * 100 end

-- 4) 分岔暴击（对暴击几率再做一次 1-(1-c)^2）
if Flag("BifurcateCrit") then output.CritChance = (1 - (1 - c)^2) * 100 end

-- 5) 必然暴击：CritChance 置 100，并按几何级数折算 less 爆伤
if Flag("InevitableCriticalHits") then ... output.CritChance = 100 end
```

**爆伤（Critical Damage Bonus）**

```lua
extraDamage = Sum("BASE","CritMultiplier")/100          -- 默认含 +100% 基础
extraDamage = extraDamage * (1 + Sum("INC","CritMultiplier")/100) * More("CritMultiplier")
-- 分岔：两次都暴击的概率 bifurcateMultiChance = PreBifurcateCritChance^2 / 100，额外加一份爆伤
-- 敌方：+ enemy SelfCritMultiplier，再乘 (1 + enemy INC SelfCritMultiplier/100)
output.CritMultiplier = 1 + max(0, extraDamage)
```

**平均暴击效果（用于 DPS）**

```lua
c = output.CritChance / 100
output.CritEffect = (1 - c) + c * output.CritMultiplier
```

**关键稳定标识 / 旗标**：`CritChance`、`PreEffectiveCritChance`、`CritChanceCap`、`CritChanceLucky`、`BifurcateCrit`、`CritBifurcates`、`InevitableCriticalHits`、`CritMultiplier`、`NoCritMultiplier`、`CritEffect`、`CritMultiplierAppliesToDegen`、`BonusCritDotMultiplier`；敌方侧 `SelfCritChance`（暴击弱点等）、`SelfCritMultiplier`（标记等）。

## 对 pobr 实现的启示

当前 `pobr-core` 的暴击建模（`calc/offence.rs`）若要对齐 PoB2，需要在 `CalcConfig` / `ModName` / flags 层补齐：

- **基础暴击率分层**：区分 `base`（含敌方 `SelfCritChance` = 暴击弱点）/ `increased` / `more`，并实现 `CritChanceCap`（默认 100，可覆盖）。
- **有效暴击几率链**：`× 命中率` → 幸运 → 分岔 → 必然，逐步覆盖 `CritChance`，且保留 `PreEffectiveCritChance` 供归因/breakdown。
- **爆伤分层**：基础 +100%（玩家/召唤物）、怪物 −40%；固定加成先于 inc/more；局部武器爆伤只作用于对应武器；敌方 `SelfCritMultiplier`。
- **新 flags**：`CritChanceLucky` / `BifurcateCrit` / `InevitableCriticalHits` / `NoCritMultiplier`。
- **归因 (TraceGraph)**：暴击弱点（光环/debuff）、标记、致盲等来源都应能回溯到 `SourceId`，这正是 pobr 相对 PoB 的增量价值。

---

## 参考来源

[^mobalytics-crit]: Mobalytics — PoE 2 Guide: Critical Hits Explained. https://mobalytics.gg/poe-2/guides/critical-hits
[^mobalytics-evasion]: Mobalytics — PoE 2 Guide: Evasion Explained. https://mobalytics.gg/poe-2/guides/evasion
[^pob2-deepwiki-crit]: Path of Building for PoE2 DeepWiki — CalcOffence / Critical Hit Calculation. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
[^poe2wiki-crit]: PoE2 Wiki — Critical hit. https://www.poe2wiki.net/wiki/Critical_hit
[^poe2wiki-bifurcate]: PoE2 Wiki — Bifurcated Critical Hits. https://www.poe2wiki.net/wiki/Bifurcated_Critical_Hits
[^poe2wiki-inevitable]: PoE2 Wiki — Critical hit §Inevitable Critical Hits. https://www.poe2wiki.net/wiki/Critical_hit
[^poe2db-critweakness]: PoE2DB — Critical Weakness. https://poe2db.tw/us/Critical_Weakness
[^poe2wiki-critweakness]: PoE2 Wiki — Critical Weakness. https://www.poe2wiki.net/wiki/Critical_Weakness
[^pob2-calcoffence]: PathOfBuilding-PoE2 — `src/Modules/CalcOffence.lua`（暴击几率/爆伤/分岔/必然/幸运段）。https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Modules/CalcOffence.lua
</content>
</invoke>
