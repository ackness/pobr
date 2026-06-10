# 防御计算（护甲/闪避/ES/减伤/EHP）

> 重构审计 · 2026-06-10 · 领域 13
> 范围：护甲/闪避/ES 三围聚合、抗性、格挡/闪避/偏斜（deflect）、承伤管线、扣池顺序、MoM/EB、Stun、EHP/max hit。
> 前置：本文与 `audits/pob2-parity-2026-06-09/FINDINGS.md`（06-01~06-07 点状修复）互补，聚焦其未覆盖的**未建管线层**缺口。

---

## PoB2 代码结构（结构地图）

PoB2 防御域只有一个文件 `Modules/CalcDefence.lua`（4285 行，229KB，行数已核实），内部分四层：

```
CalcDefence.lua
├── 1. 顶部纯函数原语 (L25-70)
│   ├── hitChance            进攻侧 acc×1.25/(acc+eva×0.3)，clamp 5-100
│   ├── monsterHitChance     防御侧 1-0.95E/(E+4A)（与进攻侧公式不对称）
│   ├── deflectChance        PoE2 新增：100-(acc/(acc+deflection×0.12)×150-50)，cap 95
│   └── armourReductionF     armour/(armour+raw×ArmourRatio(=10))，支持护甲破负值
│
├── 2. 受击管线 helper (L351-748)
│   ├── applyDmgTakenConversion (L355)   <X>DamageTakenAs<Y> 词条 → shiftTable，进伤拆多类型
│   ├── takenHitFromDamage      (L421)   单击承伤入口：EffectiveAppliedArmour DR + flat DR
│   │                                    + overwhelm + 抗性 + takenFlat
│   │                                    + AfterReductionTakenHitMulti(taken×suppress×deflect)
│   └── reducePoolsByDamage     (L460)   扣池状态机，顺序固定：
│                                        盟友(frost shield/spectre/totem/soul link)
│                                        → aegis(分型+共享) → guard(AbsorbRate%)
│                                        → ward(含 WardBypass, L568-573)
│                                        → ES(chaos 双倍 L582，per-type bypass，EternalLife L588-594)
│                                        → MoM mana 池(L585-586) → preventedLifeLoss → life → overkill
│                                        按伤害类型逆序遍历(L578)
│
├── 3. calcs.defence(env, actor) (L749-2003)  面板防御值
│   ├── Action Speed (758)
│   ├── 抗性 (816)        转换词条注入、Melding、base×calcLib.mod INC 乘区、
│   │                     Dot 抗、图腾抗、m_modf 截断、floor -200 / cap 90
│   ├── Block (960)       盾 armourData.BlockChance 基底(974-979)、
│   │                     BaseBlockChanceMax 体系(961-965)、四分型、
│   │                     lucky/unlucky 幂、BlockEffect
│   ├── 主防御 (1140)     六槽位 armourData 逐槽聚合
│   │                     + DoubleBodyArmourDefence/EnergyShieldToWard 等 flag(1297-1311)
│   │                     + resourceList 五元 ConvertTo 转换矩阵(L1301，>100% 归一化)
│   │                     + 四分型 Evasion + EvadeChance(cap 95) + DeflectionRating
│   ├── Dodge/Suppression 转换 (1509)
│   ├── 池与预留 (1570)   doActorLifeManaSpirit 统一算 Life/Mana/Spirit；
│   │                     doActorLifeManaSpiritReservation 含
│   │                     ReservationMultiplier/Efficiency/BloodMagic、Darkness
│   ├── 恢复系 (1593-1860) leech caps、regen×RecoveryRateMod、ES recharge(750%/min+延迟)、
│   │                     recoup 全矩阵 + pseudo recoup、ward recharge
│   ├── Damage Reduction (1862)  DamageReductionMax 词条化、
│   │                     ArmourAppliesToPhysicalDamageTaken BASE 100 隐式注入(1863)、
│   │                     percentOfEvasion/EnergyShieldApplies 同型合成
│   │                     effectiveAppliedArmour(2336-2362)、per-type flat DR
│   └── 移速/规避/格挡回复/异常免疫 (1878-2003)
│
└── 4. calcs.buildDefenceEstimations(env, actor) (L2005-4285)  EHP/生存估算
    ├── not-hit chance (2015-2037)   evade×dodge×avoid 连乘四分型 + EHPUnluckyWorstOf 平方/四次方
    ├── 敌人进伤装配 (2040)          configInput enemy<X>Damage/Pen/Overwhelm + EnemyCritEffect
    ├── taken 乘区矩阵 (2247)        hit/Attack/Spell/Reflect/Dot 五口径
    ├── 单击承伤 (2309-2524)
    ├── stun (2525-2643)             阈值可基 ES/Mana/CI 前 Life、
    │                                ES>totalTakenHit 且非 EB 才避晕减半(2554-2557)、
    │                                SelfStunChance、tick 取整时长
    ├── 池整备 (2643-2976)           LifeRecoverable/LowLife cap、preventedLifeLoss、
    │                                ES bypass(2707-2723)、MoM/EB 嵌套保护公式(2726-2820)、Guard、盟友
    ├── numberOfHitsToDie (2978-3153) 循环调 reducePoolsByDamage 含每击间恢复、WardNotBreak 短路
    ├── NumberOfMitigatedDamagingHits (3247)
    ├── TotalEHP = hits × 单击进伤 (3322)
    ├── recoup/petrified degen/净恢复 (3346-3491)
    ├── max hit (3540-3601)          ward(bypass poolProtected)/aegis/guard/盟友折进 TotalHitPool，
    │                                每条转换分支解二次方程求 RAW(3643-3656)，
    │                                多分支 useConversionSmoothing 近似
    └── EHP vs dots/degen 收尾 (3764+)
```

**数据流**：modDB（ModCache 生成的词条）→ Sum/More/Flag/Override 聚合 → output 表字段；常量来自 `data.misc`（`Modules/Data.lua` L175-218，大半引用 `Data/Misc.lua` 的 GGG gameConstants/characterConstants 导出）；物品防御基底来自 `item.armourData`（含 BlockChance、按 actor.level 取值）。

---

## pobr 实现现状

pobr 防御实现分四个文件共约 1770 行，由 `perform.rs::fill_mechanics`（L140-380）统一编排写入 OutputTable：

| 文件 | 行数 | 已实现 | 关键偏差/缺失 |
|------|------|--------|---------------|
| `defence.rs` | 738 | `scaled_defence_stat`（L178-215）per-slot 聚合语义对齐（槽位 base ×(全局 inc+槽位 inc)× more 连乘，缩放名集 Armour/ArmourAndEvasion/Defences 对齐）；`hit_chance`/`monster_hit_chance`（L152-168）公式逐字对齐；`calc_es_recharge`（750%/min+延迟，06-03 修复后对齐）；`calc_avoidance`；`taken_mult_for_type`（hit/Attack/Spell/OverTime 口径，06-06 后对齐）；`calc_crit_extra_reduction`/`enemy_crit_effect` | 无任何 ConvertTo 转换；avoid_stun 的 ES 条件用 ES>0（应为 ES>totalTakenHit 且非 EB）；L20/L329 自标注 `gap: ehp-no-avoidance-layer` |
| `ehp.rs` | 352 | per-type max hit：物理走 `armour_reduction` 自洽定点迭代（L104-174，数学上等价 PoB2 quadratic 的无转换特例）+ pdr_flat + overwhelm + 可变 DR cap（06-04）；元素经 `armour_applies_to_element` [bool;3]（06-02）；CI 选项已实现 | `total_ehp` = 各类型 max hit 取最低（L292-306），与 PoB2 hits×damage 口径不同；混沌池用 es×0.5 近似；CI 被 perform.rs:193 写死 false |
| `survivability.rs` | 609 | reservation（L34-47，flat+pct 钳位）、regen×RecoveryRate、block cap 90、三充能（PoB2 UseXCharges 口径）、leech（0.5.0 单实例 + 40000 截断 + 三层 cap）、recoup（8s/4s + RecoveryRateMod） | reservation 无 efficiency/multiplier（spirit 预留另在 `skill_mechanics.rs:658-689`，已含 ReservationMultiplier more）；recoup 基数是 life×常数估计、词条矩阵不全 |
| `stat_boundary.rs` | 70 | 通用 floor/max/overcap 原语，抗性边界用它 | — |

**总体评估**：面板"静态防御值"（三围、抗性 cap、命中/被命中、ES 充能、taken 乘区、avoidance、block 基础值）覆盖度约六成且公式多已逐行对照；但 PoB2 防御域的**受击动态管线**（taken-as 转换 → EffectiveAppliedArmour 合成 → 扣池顺序 → MoM/EB/Guard/Ward/Aegis → numberOfHitsToDie → TotalEHP）整体缺失（grep MindOverMatter/Bypass/Aegis/Guard/Deflect 在计算路径零命中，已核实），EHP 板块只是单击短板近似。PoE2 特有机制中：护甲对非物理（boolean 近似）、格挡（仅 cap）、deflect/ward/spirit 池/stun 体系未实现。FINDINGS 06-01~06-07 修复的是已有路径上的点状偏差，本轮发现的缺口集中在未建的管线层。

---

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|------|--------|------|-----------|-----------|------|
| 1 | 承伤 taken-as 转换管线（damageShiftTable）完全缺失 | 🔴 high | missing | CalcDefence.lua:355-416、:421-449 | 无（damage.rs::apply_shift 仅进攻侧） | 防御侧 `<X>DamageTakenAs<Y>` 全缺，受击只能单类型近似 |
| 2 | 击中扣池顺序管线 reducePoolsByDamage 缺失 | 🔴 high | missing | CalcDefence.lua:460-678 | ehp.rs:239-319（静态 life+es 池） | allies→aegis→guard→ward→ES→MoM→loss-prevention→life 状态机全缺 |
| 3 | MoM / EnergyShieldProtectsMana(EB) / per-type ES bypass 全缺 | 🔴 high | missing | CalcDefence.lua:2707-2723、:2726-2820 | 无（全 workspace grep 零命中） | MoM build 的 max hit/EHP 漏掉整个 mana 池贡献 |
| 4 | TotalEHP 口径不同：PoB2 是 hits×单击伤害，pobr 是 lowest max-hit | 🔴 high | design | CalcDefence.lua:2978-3153、:3247、:3322、:2015-2037 | ehp.rs:292-306 | 两者量纲都叫 EHP 但语义不同，parity 对不上 |
| 5 | max hit 缺 hit-pool 扩展层与转换精确解 | 🟡 medium | partial | CalcDefence.lua:3540-3601、:3643-3656 | ehp.rs:104-174 | 池只有 life+es，缺 ward/aegis/guard/allies 各层 poolProtected |
| 6 | 防御资源转换 resourceList 管线缺失（ConvertTo/翻倍 flag） | 🔴 high | missing | CalcDefence.lua:1301-1384、:1160-1246、:1410-1417 | defence.rs:178-215（无转换分支） | 五元转换矩阵 + Unbreakable/IronReflexes 等 keystone 全缺 |
| 7 | ArmourAppliesTo<X>DamageTaken 应为 BASE 百分比，pobr 实现为 boolean 重定向 | 🟡 medium | incorrect | CalcDefence.lua:1862-1863、:2336-2362；ModParser.lua:2519-2544 | perform.rs:200-204 + ehp.rs:263-264 | [bool;3] 无法表达部分适用；"also applies" 变体下错误清零物理护甲 |
| 8 | Block 模型过简：缺盾基底/BlockChanceMax/BlockEffect/lucky/格挡回复 | 🟡 medium | partial | CalcDefence.lua:960-1058、:1901-1914 | perform.rs:268-272 + survivability.rs:129-131 | 仅 Σ BASE clamp 90；数据侧 ArmourBaseStats 也缺 block_chance 字段 |
| 9 | Evade 细分与上限缺失（四分型/EvadeChanceCap=95/BASE/luck） | 🟡 medium | partial | CalcDefence.lua:1396-1404、:1421-1466、:1509-1569 | defence.rs:152-168 | 怪物命中公式本体对齐，但分型/cap/flag 全缺 |
| 10 | Deflection（PoE2 特有防御层）完全缺失 | 🟡 medium | missing | CalcDefence.lua:48-54、:1487-1506、:2434 | 无（defence.rs:589 注释承认略去） | 评级→几率→DeflectEffect(40%) 减伤全套零实现 |
| 11 | Spirit 池缺失 + 预留管线缺 efficiency | 🟡 medium | partial | CalcDefence.lua:73-126、:172-350、:128-141 | survivability.rs:34-47；skill_mechanics.rs:658-689 | 无 Spirit 池本值聚合；ReservationEfficiency 全缺（spirit 路径 Multiplier 已有） |
| 12 | Stun 系统缺失 + ES 隐式避晕条件错误 | 🟡 medium | partial | CalcDefence.lua:2525-2643（:2554-2557） | defence.rs:437-453 | 仅 avoid_stun 一值且 ES 条件用 ES>0；阈值/几率/时长体系全缺 |
| 13 | 抗性层缺转换/Melding/Dot 抗/图腾抗/INC 乘区/下限 -200 | 🟢 low | partial | CalcDefence.lua:819-864、:866-883、:885-941 | offence.rs:143-164 | 各项影响面较窄但都是面板可见值 |
| 14 | Ward 全套缺失（池/Bypass/NotBreak/RechargeDelay） | 🟢 low | missing | CalcDefence.lua:1144-1273、:568-573、:1849-1860、:2990 | 无（catalog.rs:81-82 字段已有，calc 零消费） | 数据已就位、只缺消费逻辑，实现成本低 |
| 15 | Recoup 缺 per-damage-type、pseudo recoup 与真实承伤基数 | 🟢 low | partial | CalcDefence.lua:1777-1848、:3346+ | survivability.rs:546-609 + perform.rs:345-350 | 公式骨架对，词条矩阵不全、基数为估计值 |
| 16 | Chaos Inoculation 已实现但未接线（perform 写死 false） | 🟢 low | partial | CalcDefence.lua:85、:120-123、:2537-2539 | perform.rs:193 | 解析（mod_parser.rs:514）与 EhpOptions 均已就位，只差一行接线 |

**统计**：🔴 high 5 条 · 🟡 medium 7 条 · 🟢 low 4 条。

---

## 缺口详述

### Gap 1（🔴 high）承伤 taken-as 转换管线（damageShiftTable）完全缺失

- **PoB2 证据**：`CalcDefence.lua:355-416 applyDmgTakenConversion`、`:421-449 takenHitFromDamage`——`<Source>DamageTakenAs<Target>`/`<Source>DamageFromHitsTakenAs<Target>`/`ElementalDamageTakenAs<X>` 的 BASE 求和成 shiftTable，按转换后类型分别过抗性/护甲/taken 乘区再求和。
- **pobr 现状**：无。perform.rs/ehp.rs 均按原始伤害类型单路径减伤；damage.rs 的 `apply_shift` 仅覆盖进攻侧伤害转换，可作骨架复用，但防御侧零接线。
- **影响**：【已核查成立】PoE2 大量防御构筑依赖 "X% of Physical Damage taken as Fire" 类词条（Lightning Coil 系、Cloak of Flame 系、天赋）。全局 grep 确认 pobr 没有任何防御侧 taken-as/damage_shift 实现，max hit 与 takenHit 都按单一类型计算——对这类 build 的 max hit/EHP 系统性算错（通常严重低估物理 max hit）。
- **修复方向**：在受击侧实现与进攻侧同构的 shift 矩阵（可复用 `damage.rs::apply_shift` 的归一化骨架），并把它作为 `takenHitFromDamage` 等价入口的第一步——这是整个受击管线的结构入口，缺它则后续各层只能是近似。

### Gap 2（🔴 high）击中扣池顺序管线 reducePoolsByDamage 缺失

- **PoB2 证据**：`CalcDefence.lua:460-678`——含 frost shield/spectre/totem/soul link 先扣、aegis 分型+共享、guard 按 AbsorbRate 比例、ward 含 WardBypass（:568-573 已核实）、ES 对 chaos 双倍 `esDamageTypeMultiplier=2`（:582，除非 ChaosNotDoubleESDamage）、EternalLife（:588-594）、MoM mana 池（:585-586 `min(lifeHitPool/(1-MoMEffect)-lifeHitPool, mana)`）、preventedLifeLoss 分段；按伤害类型逆序遍历（:578）。
- **pobr 现状**：`ehp.rs:239-319 calc_ehp_with_opts`——静态 life+es 池，混沌按 es×0.5 近似。
- **影响**：【已核查成立】PoB2 的击中生存计算是一个有严格顺序的资源扣减状态机；pobr 只有 "pool/taken" 静态除法。chaos 对 ES 双倍伤害用 es×0.5 近似在纯池下等价，但 bypass、guard、aegis、ward、allies、loss prevention 各层全部缺失（grep aegis/guard/frost_shield/soul_link 在 pobr calc 下零命中）。这是 EHP 板块与 PoB2 面板差异的**最大结构性来源**。
- **修复方向**：实现一个有序扣池状态机（输入：分类型 takenHit；状态：各池余量），各保护层用统一的 poolProtected 原语挂接（见"数据 vs 逻辑切分建议" §4），顺序固定为 PoB2 的层次。Gap 3/4/5 都依赖它。

### Gap 3（🔴 high）MoM / EnergyShieldProtectsMana(EB) / per-type ES bypass 全缺

- **PoB2 证据**：`CalcDefence.lua:2707-2723` ES bypass（`<X>EnergyShieldBypass` Override/BASE，clamp 0-100，MinimumBypass；UnblockedDamageDoesBypassES→100）；`:2726-2820` MoM（sharedMindOverMatter=`DamageTakenFromManaBeforeLife` clamp 100、per-type `<X>DamageTakenFromManaBeforeLife`、EnergyShieldProtectsMana 时 manaProtected 公式、sharedMoMHitPool/sharedManaEffectiveLife）。
- **pobr 现状**：无。grep MindOverMatter/DamageTakenFromManaBeforeLife/EnergyShieldBypass 在全 workspace 计算路径零命中；mana 不进任何伤害池。
- **影响**：【已核查成立，PoB2 行号逐段比对】MoM 是核心防御 keystone：mana 按比例先于 life 扣减，poolProtected = sourcePool/(MoM%)×(1-MoM%)，hitPool = max(LifeHitPool-protected,0)+min(LifeHitPool,protected)/(1-MoM%)。EB 让 ES 保护 mana 再保护 life，公式链更深（:2735-2746）。pobr 对任何 MoM build 的 max hit/EHP 直接错误（漏掉整个 mana 池贡献）。
- **修复方向**：词条名（DamageTakenFromManaBeforeLife 等）属数据、入解析表；池公式属框架逻辑，作为 poolProtected 原语的两个实例（MoM、EB 嵌套）实现，挂在 Gap 2 的状态机上。

### Gap 4（🔴 high）TotalEHP 口径不同：PoB2 是 numberOfHitsToDie×单击伤害，pobr 是 lowest max-hit

- **PoB2 证据**：`CalcDefence.lua:2978-3153 numberOfHitsToDie`（迭代扣池直到 life=0，含每击间 recovery，WardNotBreak 短路）、`:3247 NumberOfMitigatedDamagingHits`（ConfiguredDamageChance=blockEffect×suppressionEffect×deflectMulti×(1-avoid)）、`:3322 TotalEHP = TotalNumberOfHits × totalEnemyDamageIn`；`:2015-2037` 四分型 NotHitChance（evade×dodge×avoid 连乘 + EHPUnluckyWorstOf 平方/四次方）。
- **pobr 现状**：`ehp.rs:292-306`——total_ehp = 各类型 max hit 取 min。
- **影响**：【已核查成立，全部行号实证】PoB2 的 EHP 是"以配置的敌人伤害模拟连续受击、计入闪避/格挡/不被击中概率与每击恢复后能吃几下"的期望值；pobr 的 total_ehp 是单击短板。两者量纲都叫 EHP 但语义不同，parity 对不上。defence.rs:20/:329 已自标注 `gap: ehp-no-avoidance-layer`——calc_avoidance 算出的规避值无任何消费者乘进 EHP。
- **修复方向**：先落地 Gap 1/2 的管线，再实现 numberOfHitsToDie 模拟循环（输入敌人进伤预设，迭代调用扣池状态机+每击间恢复），最后 TotalEHP = hits×单击进伤、并接入 not-hit/mitigation 概率层。在管线就绪前，可先把现有口径在 OutputTable 中改名/标注，避免 parity 误比。

### Gap 6（🔴 high）防御资源转换 resourceList 管线缺失（ConvertTo/GainAs/翻倍类 flag）

- **PoB2 证据**：`CalcDefence.lua:1301-1384`——Armour/Evasion/EnergyShield/Life/Mana 五元 `<X>ConvertTo<Y>` per-slot 转换矩阵、>100% 归一化（已核实 resourceList 定义与 conversionRate 循环）；`:1160-1246` DoubleBodyArmourDefence、EnergyShieldToWard 等翻倍/转移 flag（:1297-1311 已核实）；`:1410-1417 CappingES`。
- **pobr 现状**：`defence.rs:178-215 scaled_defence_stat`——只有 per-slot base × inc/more，无任何转换；全局 grep ConvertTo/GainAs 仅命中进攻侧 damage.rs 与 minion 的 LifeConvertToEnergyShield，防御三围零转换。
- **影响**：【已核查成立】PoE2 天赋/装备大量出现 "Convert X to Y"/防御转移类词条与身甲翻倍 keystone（Unbreakable/DoubleBodyArmourDefence/IronReflexes 在 pobr 全 workspace grep 零命中）。pobr 防御三围聚合完全没有转换矩阵，这类 build 的 armour/evasion/ES 直接错。
- **修复方向**：转换矩阵本身是通用逻辑（与伤害转换同构，pobr damage.rs 已有类似实现可复用骨架），词条名是数据。在 `scaled_defence_stat` 的 per-slot 聚合之后、全局 inc/more 之前插入五元转换步骤，并补 keystone flag 的消费分支。

### Gap 5（🟡 medium）max hit 缺 hit-pool 扩展层与转换精确解

- **PoB2 证据**：`CalcDefence.lua:3540-3601`——TotalHitPool 依次叠 ward（含 WardBypass poolProtected，:3546-3550）、aegis（:3556）、guard AbsorbRate（:3558-3560）、frost shield、spectres/totems、soul link；`:3643-3656`——armour DR 下解 RAW 的 quadratic：a=ArmourRatio×convMulti×(1-flatDR+overwhelm)，含 noDRMaxHit/maxDRMaxHit 上下界 + useConversionSmoothing。
- **pobr 现状**：`ehp.rs:104-174`——定点迭代解 H×taken(H)=pool，池只有 life+es。
- **影响**：【已核查成立】pobr 的自洽迭代在数学上等价于 PoB2 的 quadratic（无转换时），这点没问题；缺的是 hit pool 的所有扩展层（每层都是 poolProtected = pool/(rate)×(1-rate) 同构公式）以及多类型转换时的 smoothing。对带 guard 技能（PoE2 常见 Scavenged Plating 等）或 aegis 的 build，max hit 偏低。
- **修复方向**：复用 Gap 2/3 的 poolProtected 原语扩展 TotalHitPool；转换分支接 Gap 1 的 shiftTable 后按 PoB2 的二次方程精确解或保留迭代法（数学等价即可），多分支时补 smoothing。

### Gap 7（🟡 medium）ArmourAppliesTo<X>DamageTaken 应为 BASE 百分比，pobr 实现为 boolean 重定向

- **PoB2 证据**：`CalcDefence.lua:1862-1863` `NewMod("ArmourAppliesToPhysicalDamageTaken","BASE",100)`；`:2336-2362`——percentOfArmourApplies=Sum(BASE) 可取 50% 等部分值，元素再叠 ArmourAppliesToElementalDamageTaken；另有 EvasionAppliesTo/EnergyShieldAppliesTo 同型，合成 effectiveAppliedArmour；`ModParser.lua:2519-2544`——"instead of physical" 变体额外注入 `flag("ArmourDoesNotApplyToPhysicalDamageTaken")`，"also applies"/"N% of armour applies" 变体物理保留隐式 100。
- **pobr 现状**：`perform.rs:200-204`（ArmourAppliesToFire/Cold/Lightning flag）+ `ehp.rs:263-264`（any_redirect 时物理护甲清零）。
- **影响**：【已核查成立，细节修正】PoB2 语义：每类型有 0-100+ 的"护甲适用百分比"，物理默认 100，元素词条按 BASE 叠加（如 50% 部分适用），且 Evasion/ES 也能按百分比当护甲用。pobr 的 [bool;3] 模型无法表达部分适用；其"任一元素重定向→物理失去护甲"仅与 "instead of physical" 文本变体一致（PoB2 该变体经 ModParser 注入 ArmourDoesNotApplyToPhysicalDamageTaken flag，**非负值注入**——原报告此处有误已修正），对 "also applies"/"N% of armour applies" 变体（物理仍保有 100%）会错误清零物理护甲、压低物理 max hit。
- **修复方向**：把 [bool;3] 改为按伤害类型的百分比（`[f64;5]`，物理隐式 100，BASE 叠加），区分 "instead" flag 与 "also applies" 加法词条两条解析路径，并扩展到 EvasionAppliesTo/EnergyShieldAppliesTo 合成 effectiveAppliedArmour。

### Gap 8（🟡 medium）Block 模型过简

- **PoB2 证据**：`CalcDefence.lua:960-1058`——:974-979 Weapon 2/3 armourData.BlockChance 基底（已核实）、:961-965 BaseBlockChanceMax+BlockChanceMax/Override（已核实）、Projectile/SpellProjectile 分型、CannotBlockAttacks/CannotBeBlocked、Effective*BlockChance 的 lucky/unlucky 幂运算、BlockEffect→DamageTakenOnBlock；`:1901-1914` recovery on block（LifeOnBlock/ManaOnBlock/EnergyShieldOnBlock）。
- **pobr 现状**：`perform.rs:268-272`（BlockChance/SpellBlockChance BASE 求和 + cap 90，仅此而已）+ `survivability.rs:129-131`。
- **影响**：【已核查成立】pobr 的 block 只有 Σ BASE clamp 90：没有盾基底值（数据侧 `catalog.rs:76-83 ArmourBaseStats` 也缺 block_chance 字段，已核实）、没有 max 体系（PoE2 BlockChanceMax 可被词条改）、没有 BlockEffect（格挡承伤百分比，PoE2 格挡非全免）、没有 inc 乘区。mod_parser 也无 "chance to block" 文本模式。对持盾 build 的 block 值与 EHP 中 block 层全部缺失/偏差。
- **修复方向**：① 数据侧给 ArmourBaseStats 补 block_chance 字段（GGG ShieldTypes.dat）；② mod_parser 补 block 文本模式；③ 实现 base+inc 乘区 + BlockChanceMax 体系 + BlockEffect 承伤折减，并把 block 概率/效果接入 Gap 4 的 mitigation 层。

### Gap 9（🟡 medium）Evade 细分与上限缺失

- **PoB2 证据**：`CalcDefence.lua:1396-1404`（MeleeEvasion/ProjectileEvasion/SpellEvasion/SpellProjectileEvasion 各吃独立 inc）；`:1421-1466`（EvadeChance=100-(monsterHitChance-BASE EvadeChance)×enemy HitChance、EvadeChanceMax/data.misc.EvadeChanceCap=95、CannotEvade/AlwaysEvade/UnluckyEvade、EnemyAccuracyDistancePenalty）；`:1509-1569` Dodge 层。
- **pobr 现状**：`defence.rs:152-168 monster_hit_chance`（公式正确，已核实 `1-0.95E/(E+4A)` clamp [0.05,1.0]）+ perform 只产出 chance_to_be_hit 单值。
- **影响**：pobr 的怪物命中公式本体对齐，但缺：BASE EvadeChance 加项、敌方 HitChance 乘区、四分型 evasion（PoE2 法术也可被闪避，SpellEvasion 是独立乘区）、95% cap 的显式词条提升（EvadeChanceMax Max 语义）、CannotEvade 等 flag。EHP 的 not-hit 层（Gap 4，:2015-2037 四分型 NotHitChance 已核实）依赖这些分型值。
- **修复方向**：把单值 evade 拆为 Melee/Projectile/Spell/SpellProjectile 四分型（各自独立 inc 乘区），补 BASE EvadeChance、EvadeChanceMax（cap 95 入 game_constants 数据）与相关 flag，作为 Gap 4 not-hit 层的输入。

### Gap 10（🟡 medium）Deflection（PoE2 特有防御层）完全缺失

- **PoB2 证据**：`CalcDefence.lua:48-54 deflectChance`（`100 - (accuracy/(accuracy+deflection×0.12)×150-50)`，cap data.misc.DeflectionChanceCap=95）；`:1487-1506`（DeflectionRating = BASE + Evasion/Armour GainAsDeflection、DeflectIsLucky、DeflectEffect=40 基础）；`:2434` deflectMulti 乘进 AfterReductionTakenHitMulti。
- **pobr 现状**：无。`defence.rs:589` 注释承认"deflect 罕用，按 1.0 略去"（已核实）；全 workspace grep Deflect 仅此一处注释。
- **影响**：【已核查成立】Deflection 是 PoE2 0.2+ 新防御属性（部分盾/词缀提供），PoB2 已完整实现：评级→几率公式→DeflectChance==100 时按 DeflectEffect（基础 40%）减伤进 takenHit。pobr 零实现，词条也未解析。对 deflect build 承伤全错。
- **修复方向**：实现 DeflectionRating 聚合（含 Evasion/Armour GainAsDeflection）→ deflectChance 公式 → deflectMulti 进 taken 乘区链；常量 40/95 入 game_constants 数据表；mod_parser 补对应文本模式。

### Gap 11（🟡 medium）Spirit 池缺失 + 预留管线缺 efficiency

- **PoB2 证据**：`CalcDefence.lua:73-126 doActorLifeManaSpirit`（Life/Mana/Spirit 统一 base×(1-conv)+extra ×inc×more，Override，CI→Life=1）；`:172-350 doActorLifeManaSpiritReservation`（ReservationMultiplier more-floor4、ReservationEfficiency inc/more 除法、ExtraXReserved、BloodMagicReserved、Companion/Spectre spirit 特例）；`:128-141 doActorDarkness`。
- **pobr 现状**：`survivability.rs:34-47` reservation（flat+pct 钳位，无 efficiency/multiplier）；`skill_mechanics.rs:658-689 calc_spirit_reservation`（base×ReservationMultiplier more+ExtraSpirit，**已实现**）；OutputTable 仅 spirit_reserved（output.rs:144），无 Spirit 池本值。
- **影响**：【已核查，描述修正】Spirit 是 PoE2 核心资源，PoB2 把它与 Life/Mana 同管线计算池值+预留。pobr 没有 Spirit 池聚合（base 来自职业/装备 +inc/more）。预留侧修正：ReservationMultiplier 在 spirit 路径已实现（skill_mechanics.rs:672-678，原报告称全缺有误）；仍缺的是 ReservationEfficiency（PoE2 词条常见，grep 全 workspace 零命中）以及 Life/Mana 预留路径的 efficiency/multiplier。spirit 不足判定（技能能否启用）因无池值而无法做。
- **修复方向**：① 把 Spirit 纳入与 Life/Mana 同构的池聚合管线（base+inc/more+Override）；② 预留侧补 ReservationEfficiency（除法语义）并把 multiplier/efficiency 推广到 Life/Mana 路径；③ 有池值后实现 spirit 不足时技能禁用判定。

### Gap 12（🟡 medium）Stun 系统缺失 + ES 隐式避晕条件错误

- **PoB2 证据**：`CalcDefence.lua:2525-2643`——StunThreshold 可基于 ES/Mana/CI 前 Life + AddESToStunThreshold（:2529-2552 已核实）；`:2554-2557` `if output.EnergyShield > output["totalTakenHit"] and not env.modDB:Flag(nil,"EnergyShieldProtectsMana")` 才 ×0.5；SelfStunChance=StunBaseMult(200)×有效伤/阈值、PhysicalTakenHit×0.25 加权；Stun/BlockDuration 按 ServerTickRate 上取整。
- **pobr 现状**：`defence.rs:437-453`（avoid_stun：ES>0 即 ×0.5，已核实）；无 StunThreshold/StunDuration/SelfStunChance。
- **影响**：【已核查成立，条件补充】pobr 只有 avoid_stun 一个值，且 ES 减半条件用 ES>0；PoB2 是 ES 大于本次总承伤**且未点 EnergyShieldProtectsMana(EB)** 才生效——pobr 两个条件都缺。整个阈值/几率/持续时间体系未实现，面板 Stun 区块无法对齐。
- **修复方向**：先修 ES 避晕条件（依赖 totalTakenHit，即 Gap 1/2 的承伤管线产出 + EB flag）；再实现 StunThreshold/SelfStunChance/Duration 体系——PoE2 0.5 的 light/heavy stun 常量在 `Data/Misc.lua:44-53` 是现成数据，应随 game_constants JSON 化。

### Gap 13（🟢 low）抗性层缺转换/Melding/Dot 抗/图腾抗/INC 乘区/下限 -200

`CalcDefence.lua:819-941` vs `offence.rs:143-164 resolve_resistance`（base BASE 求和 + max 75/90，无 INC、无 floor、无 Override、无 dot/totem 变体）。FINDINGS 已覆盖 boundary 框架（stat_boundary.rs 有 floor 能力但 resolve_resistance 没用 -200 下限），本轮新增：抗性 INC 乘区、XResConvertToY 转换词条、Melding 类 keystone（ElementalResistMaxIsHighestResistMax→OVERRIDE）、DoT 用独立抗性值（ailment DPS 承伤会用错）、m_modf 小数截断。各项影响面较窄但都是面板可见值。

### Gap 14（🟢 low）Ward 全套缺失

`CalcDefence.lua:1144-1273`（per-slot 聚合+EnergyShieldToWard，:1297-1311 已核实）、`:568-573`（扣池层含 WardBypass，已核实）、`:1849-1860 WardRechargeDelay`、`:2990 WardNotBreak` 全挡判定（已核实）。pobr 数据侧 `catalog.rs:81-82 ArmourBaseStats.ward` 字段已有但 calc 零消费；`item_text.rs:396` 仅识别 "Ward:" 标签。当前版本 ward 物品稀少故 low，但数据已就位、只缺消费逻辑，实现成本低。

### Gap 15（🟢 low）Recoup 缺 per-damage-type、pseudo recoup 与真实承伤基数

`CalcDefence.lua:1777-1848`（`<DamageType><Resource>Recoup` 全矩阵 + Sacrosanctum ReplaceMod + PhysicalDamageMitigated<R>PseudoRecoup 吃 regen inc/more）、`:3346+`（基数用 damageTakenThatCanBeRecouped，即 reducePoolsByDamage 实际入池伤害）vs `survivability.rs:546-609`（单一 LifeRecoup 名）+ `perform.rs:345-350`（基数=life×常数估计）。公式骨架对（total/duration×rateMod、4s flag），但词条矩阵只有 3 个名字，且基数与受击管线脱钩（依赖 Gap 2）。

### Gap 16（🟢 low）Chaos Inoculation 已实现但未接线

`CalcDefence.lua:85/:120-123/:2537-2539` vs `perform.rs:193 chaos_inoculation: false`（已核实）。ehp.rs 的 CI 选项（ES 当血池、混沌免疫）已实现并有测试，且 `mod_parser.rs:514` **已能解析** ChaosInoculation flag（原报告称需"补 flag 解析"有误——解析已就位），但 perform 的 fill_mechanics 硬编码 false，从未从 ModDb 读该 flag。任何 CI build 的 EHP/max hit 走非 CI 路径，chaos max hit 算成有限值。真正只差一行接线（`db.flag(cfg, "ChaosInoculation")`）即可启用——建议作为本领域第一个落地项。

---

## 数据 vs 逻辑切分建议

### 1. 纯数值常量（应 JSON 化，pobr 当前硬编码在 Rust）

PoB2 的防御常量集中在 `Modules/Data.lua:175-218` 的 `data.misc` 表，且很多直接引用 `Data/Misc.lua` 的 GGG 导出常量（gameConstants/characterConstants/monsterConstants）：

| 常量 | 值 | 来源 |
|------|----|----|
| ArmourRatio | 10 | data.misc |
| DamageReductionCap | characterConstants["maximum_physical_damage_reduction_%"] | GGG 导出 |
| EvadeChanceCap | gameConstants["DefaultMaxEvadeChancePercent"]=95 | GGG 导出 |
| DeflectionChanceCap | 95 | data.misc |
| DeflectEffect | gameConstants["BasePercentDamageDeflected"]=40 | GGG 导出 |
| AvoidChanceCap | 75 | data.misc |
| MaxResistCap / ResistFloor | 90 / -200 | data.misc |
| LowPoolThreshold | 0.35 | data.misc |
| StunBaseMult + light/heavy stun 全套 | 200 等 | Misc.lua:44-53、185-220 |
| WardRechargeDelay、ES 充能 | character_inherent_energy_shield_recharge_rate_per_minute_%=750 | GGG 导出 |

这些每个 patch 都可能变（GGG 直接导出），**本质是数据**。pobr 现状：散落硬编码在 `crates/pobr-data/src/constants.rs`（ARMOUR_RATIO/RESIST_FLOOR/BLOCK_CHANCE_CAP 等 Rust 常量）、`defence.rs:247-249`（ES_RECHARGE_RATE_PER_MINUTE_BASE=750、delay=4s）、`survivability.rs:327-341`（leech 上限）、`perform.rs:191`（DR cap 默认 90 内联）。

**建议**：新增 `data/<ver>/game_constants.json`（分 character/monster/game 三段，schema 进 `catalog.rs`），`pobr-data-adapter` 从 GGG 导出生成，运行时经 `pobr-gamedata` 注入 calc——版本更新零代码改动。

### 2. 物品防御基底（已部分 JSON 化，有字段缺口）

PoB2 每槽位 `item.armourData`（armour/evasion/ES/ward/**BlockChance**，`GetArmourDataValue(name, actor.level)` 还带等级缩放）。pobr `catalog.rs:76-83 ArmourBaseStats` 已有 armour/evasion/energy_shield/ward，**缺 BlockChance**（GGG ShieldTypes.dat）——这直接阻塞 Block 段的 `baseBlockChance`（CalcDefence.lua:974-980，已核实 Weapon 2/3 armourData.BlockChance 取值）按盾基底取值。

**建议**：ArmourBaseStats 补 `block_chance: Option<f64>` 字段，adapter 从 ShieldTypes.dat 反范式化。

### 3. 敌人伤害/配置预设（数据，pobr 完全没有对应表）

buildDefenceEstimations 的进伤来源 `env.configInput["enemy<Type>Damage"]`/`Pen`/`Overwhelm` 与 configPlaceholder 默认值、boss 预设、enemyDamageMult 来自 ConfigOptions/敌人数据（CalcDefence.lua:2068-2085）。这是"按敌人等级/类型查表"的数据。

**建议**：JSON 化为 `enemy_damage_presets`——pobr 已有 monster.rs/setup_env 的部分敌人模型，可在该 schema 上扩展 per-type damage/pen/overwhelm 默认列。

### 4. 词条→ModName 映射（数据）vs 聚合机制（逻辑）

PoB2 的 `Data/ModCache.lua`（自动生成）把文本词条映射到 `DeflectEffect`、`ArmourAppliesTo<X>DamageTaken`、`DamageTakenFromManaBeforeLife`、`GuardAbsorbRate/Limit`、`<X>EnergyShieldBypass`、`LifeLossPrevented` 等 ModName——这层是数据，pobr 对应 mod_parser/mods.json，补齐方向是把这批防御 ModName 纳入解析表（JSON 驱动而非 Rust 正则逐条加）。

而 CalcDefence 的**机制公式属于逻辑、留在框架**：

- armour DR 公式；
- `poolProtected = pool/(rate)×(1-rate)` 的 X-protects-Y 通用模式——MoM/Guard/FrostShield/SoulLink/WardBypass 全部复用同一公式（CalcDefence.lua:2746/2837/3546-3550，ward bypass poolProtected 已核实）——pobr 可实现为**一个参数化的 `protected_pool(source, rate, protected)` 原语**，由数据 flag 决定挂哪些层；
- reducePoolsByDamage 的扣池顺序；
- max-hit 二次方程解（:3643-3650 已核实 a/b/c 系数与 `data.misc.ArmourRatio` 引用）。

### 5. Keystone 行为（开关是数据、行为是逻辑）

MoM/EB(EnergyShieldProtectsMana)/CI/EternalLife/Unbreakable/IronReflexes/DoubleBodyArmourDefence/WardNotBreak 等在 PoB2 全部表现为 ModCache 注入的 flag/BASE 词条 + CalcDefence 里的分支。pobr 正确的切分是：天赋树 JSON（passive_tree.json 已有）给出词条 → 解析为 flag → 框架内一组**有限的、稳定的** keystone 分支逻辑。当前 pobr 连消费侧分支都缺（见缺口清单），且 `perform.rs:193` 把 chaos_inoculation 写死 false——而 `mod_parser.rs:514` 其实已能解析 ChaosInoculation flag，属于"逻辑侧没接数据开关"的典型例子。

### 总结

该领域 PoB2 的"数据混在代码里"主要是 **(a)** Data.lua/Misc.lua 常量、**(b)** ModCache 词条映射、**(c)** 敌人配置默认值三块；pobr 的 JSON schema 还缺 game_constants 表、ArmourBaseStats.block_chance 字段、enemy per-type damage 预设列。计算管线本身（扣池顺序/防御公式/keystone 分支）是逻辑，应留 Rust，但要**以数据 flag 为开关、以通用 poolProtected 原语为骨架**来实现，避免逐 unique 硬编码。

---

## 附录：核查说明

核查范围与结论（共逐条打开验证 9 条：全部 5 条 high + 4 条可疑 medium/low）：

- **high #1（taken-as 管线）**：成立。打开 CalcDefence.lua:350-470 实证 applyDmgTakenConversion（实际起于 :355，原写 356，微调）与 takenHitFromDamage（:421，原写 422）。pobr 侧全局 grep（crates/apps/tools）"TakenAs/taken_as/damage_shift" 仅命中进攻侧 damage.rs::apply_shift（伤害转换，非防御承伤），防御侧确实零实现。pobr_ref 补注 damage.rs 骨架可复用的事实。
- **high #2（reducePoolsByDamage）**：成立。实证函数起于 :460（原写 461），ward WardBypass（:568-573）、chaos ES 双倍 esDamageTypeMultiplier=2（:582）、EternalLife（:588-594）、逆序遍历（:578）均逐行核实。pobr ehp.rs 通读确认只有静态 life+es 池；grep aegis/guard/frost_shield/soul_link 在 pobr calc 零命中。
- **high #3（MoM/EB/bypass）**：成立。实证 :2707-2723 ES bypass 段与 :2726+ MoM 段（含 EnergyShieldProtectsMana manaProtected 公式、poolProtected 公式），与描述逐字相符。pobr grep MindOverMatter/DamageTakenFromManaBeforeLife/EnergyShieldBypass 全 workspace 零命中（仅 ailment 的 bypasses_es 字段与 CooldownBypass，均无关）。
- **high #4（TotalEHP 口径）**：成立。实证 :2015-2037 四分型 NotHitChance+EHPUnluckyWorstOf、numberOfHitsToDie helper 起于 ~:2978、:3247 NumberOfMitigatedDamagingHits、:3322 TotalEHP=hits×damage（原写 3321，差 1 行）。pobr ehp.rs:292-306 取 min 实证。detail 中"defence.rs 注释自标 gap: ehp-no-avoidance-layer"经 grep 证实（defence.rs:20/:329）。
- **high #6（resourceList 转换）**：成立。实证 :1301-1320 五元 resourceList+ConvertTo 矩阵（原写 1300-1384）、:1160-1175 DoubleBodyArmourDefence/EnergyShieldToWard。pobr grep ConvertTo/GainAs/Unbreakable/IronReflexes/DoubleBodyArmour：仅命中进攻侧伤害转换与 minion LifeConvertToEnergyShield，防御三围零转换；scaled_defence_stat（defence.rs ~178-215）通读确认无转换分支。
- **medium #7（ArmourAppliesTo）**：成立但修正一处细节：原 detail 称 instead 语义"由 ModParser 另行注入负值"——实证 ModParser.lua:2519-2524 是注入 `flag("ArmourDoesNotApplyToPhysicalDamageTaken")` 而非负 BASE，已改写。核心断言（BASE 百分比可部分叠加、物理隐式 100、also-applies 变体下 pobr 错误清零物理护甲）经 :1863 与 :2336-2362 + ModParser 2525-2544 双侧证实，维持 medium/incorrect。
- **medium #12（Stun ES 条件）**：成立并补强：实证 :2554-2557 条件为 `ES > totalTakenHit AND not EnergyShieldProtectsMana`，pobr defence.rs:437-453 用 ES>0 且无 EB 排除——比原描述还多缺一个条件，已写入。
- **medium #11（Spirit）**：部分有误，已修正：原 detail 称预留"缺 ReservationEfficiency 与 ReservationMultiplier"——实证 skill_mechanics.rs:672-678 的 spirit 路径**已实现 ReservationMultiplier more**；ReservationEfficiency 确实全 workspace 零命中、Spirit 池本值确实缺（output.rs 仅 spirit_reserved）。标题与 detail 改写，维持 medium/partial。
- **low #16（CI 接线）**：成立但修正：perform.rs 实际行号 193（原写 194）；且 mod_parser.rs:514 已解析 ChaosInoculation flag，原 detail 称需"补 flag 解析"有误——只差消费侧一行接线，已改写。

另抽查：#5（:3540-3560 TotalHitPool ward/aegis/guard 层 + :3643-3656 quadratic 实证）、#8（:960-985 盾基底/BlockChanceMax 实证；catalog.rs:76-83 ArmourBaseStats 确无 block_chance；mod_parser 无 block 文本模式，detail 补注）、#10（defence.rs:589 deflect 注释实证，grep Deflect 全 workspace 仅此注释）、#15 Ward（:568-573/:2990 实证，catalog ward 字段在 :81）。均成立。

无删除、无降级条目；所有 severity 维持原判（high 5 条全部查实为真实结构性缺失/口径差异，影响断言成立）。修正集中在：3 处行号微调（356→355、3321→3322、194→193）、#7 instead 语义机制描述、#11 ReservationMultiplier 误报、#16 flag 解析已存在的事实，并在多条 detail / PoB2 结构 / pobr 现状中补入"已核实"的具体行号证据。
