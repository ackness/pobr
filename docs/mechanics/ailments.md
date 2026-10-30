# 机制约定：异常、晕眩与敌人减益

本文说明伤害型异常、冰缓、感电、冻结、晕眩、钉住及命中效果。索引见[机制约定](README.md)，参考工具的差异见 [pob2-differences.md](../pob2-differences.md)。本文的数值视图描述一次计算，不表示引擎已经模拟了施加、消耗和再次施加的完整过程。

## 伤害型异常

流血、点燃和毒的潜在同时层数按“命中率 × 施加几率 × 持续时间 × 每秒攻击／命中机会数”估计。持续时间先应用持续时间加成（包括 `EnemyAilmentDuration`）和敌人减益过期速度，再除以伤害加速倍率。“Damaging Ailments deal damage #% faster”让同一份伤害更快结束，不增加该份伤害总量。实现见 `calc/perform.rs`，明细中的除数标为 `faster /N`。

Calcs 直接读取异常计算阶段留下的输入，不另算一遍伤害：

- 几率：`IgniteChancePerHit`、`BleedChance`、`PoisonChance`、`ShockChance`，单位为百分比。点燃取叠层筛选前的几率；流血和毒取各手别加权后的最终几率，不能施加的手别贡献零。
- 每种伤害型异常的 `Duration`、`EffMult`、`MagnitudeEffect`、`RollAverage`、`StacksMax`、`StackPotentialPercent`。持续时间用秒，效果与减免用倍率，取值分位和潜在层数比例用百分比。潜在层数不封顶；缺少命中频率估计时沿用满层回退。
- 来源区间：`Ignite{Fire,Cold,Lightning}{Min,Max}`、`BleedPhysical{Min,Max}`、`Poison{Physical,Chaos}{Min,Max}`。这些是非暴击命中伤害经过对应异常累积倍率后、选择高伤害实例前的值。

全局异常视图沿用最后一个可施加手别的输入，DPS 则沿用强／弱手合并；这两个口径不能混为一谈。没有该异常时，数值及倍率均报零。派生明细复用同一组输入和敌人减免项，必须能还原展示值。`ShockChance` 仍采用非暴击来源近似；`FreezeBuildupPct` 是每点伤害的冻结累积百分比，不是 `FreezeBuildupAvg` 的别名。

### Incision

[Incision 的游戏关键词](https://poe2db.tw/us/Incision)说明：每层使目标被命中时增加 10% 被施加流血的几率，最多 10 层；施加流血后清除全部层数。页面的 `incision_debuff` 也列出 `MaxStacks = 10`。因此，不能把它当作永久流血几率或自动满层。

PoBR 在有效模式读取 `multiplierIncisionStackCount`，写入敌人的 `Multiplier:IncisionStack`，上限 10。`calc::setup_env::apply_enemy_incision` 只有在角色拥有 `Condition:CanInflictIncision` 时，才把 `10 × 层数 × max(0, 1 + IncisionEffect INC / 100)` 加入敌人的 `SelfBleedChance`；最终施加几率仍受 100% 上限限制。移除来源后，导入配置中残留的层数不会生效。

`apply_X_incision_on_hit` 由 `skill_stats` 映射为施加能力；`incision_effect_+%` 放大每层几率。钉定 PoB2 的读法相同，因此这条规则不需要 `exporter` 标记。效果如何缩放每层几率尚无游戏内实测。Thousand Cuts 的每层 2% 物理承伤，以及“对受 Incision 影响敌人”的流血幅度，仍读同一层数或敌人 `MultiplierThreshold`，不再乘 `IncisionEffect`。

配置表示**这次命中前已经存在的层数**。引擎不推算积层、清层时机，也不由流血几率推算 `incisionConsumedRecently`。现有 worked 构筑没有 Incision；验证依据是 [Incision 构筑测试](../../crates/pobr-build/tests/skills/limits_reservation.rs)。

Crimson Assault 先把流血基础持续时间改为 1 秒，再应用持续时间 INC/MORE 和伤害加速。单实例伤害、保留输入与来源视图共用这一基础值。Shattering Spite 的 `active_skill_damaging_ailment_effect_+%_final` 通过异常效果施加 −50%，不会推算 Bloodstone Lance 数量或条件偷取。后者在钉定导出器中缺失，只在参考口径关闭；这两项不能解释所有存档中的 `BleedDPS` 差异。

## 冰缓、感电与冻结

`ShockDuration`、`ChillDuration` 对非玩家目标取 8 秒基础时间（`GameMechanicsConstantsDef::shock_base_duration` / `chill_base_duration`），`FreezeDuration` 取 4 秒（`freeze_base_duration`）。依据是[游戏关键词](https://poe2db.tw/us/Keywords)。统一公式在 `calc::ailment::ailment_duration_terms`：基础时间 × (1 + 持续时间 INC / 100) × MORE × 敌人减益持续倍率。

INC 合计攻击者的 `Enemy<X>Duration`、`EnemyAilmentDuration`、`EnemyElementalAilmentDuration` 与目标的 `Self<X>Duration`、`SelfAilmentDuration`、`SelfElementalAilmentDuration`。映射包括 `shock_duration_+%`、`chill_duration_+%`、`lightning_ailment_duration_+%`、`cold_ailment_duration_+%`、`freeze_duration_+%`、`chill_and_freeze_duration_+%`；`support_pure_shock_shock_duration_+%_final` 和 `active_skill_freeze_duration_+%_final` 走 MORE。

只有所选命中能够施加时才展示持续时间：感电要求非零几率；冰缓要求冰冷命中且没有 `CannotChill`；冻结要求 `FreezeBuildupAvg > 0`。引擎不由持续时间推算冻结速度或再次冻结的间隔。PoB2 没有对应的 `FreezeDuration` 输出。

冰缓与感电幅度由攻击者和目标两侧倍率相乘。`calc::ailment::effect_mod_factors` 分别读取攻击者的 `Enemy<X>Magnitude` / `AilmentMagnitude`、目标的 `Self<X>Magnitude` / `AilmentMagnitude`，每侧最低为零。`ChillEffectMod` / `ShockEffectMod` 展示乘积，中性值为 1。

攻击者倍率还按暴击占比 c 加权：`(1 − c) × factor + c × factor_with_CriticalStrike`。`AilmentsAreNeverFromCrit` 下 c 为零。这样，`shock_effect_+%_with_critical_strikes` / `chill_effect_+%_with_critical_strikes` 只影响暴击部分。来源伤害仍使用现有非暴击／暴击平均近似，没有分别求两种命中的最终幅度；**没有游戏内实测**。

`FreezeBuildupAvg` 表示一次成功命中平均积累了冻结门槛的百分之几：暴击加权、减免后的可冻结伤害 × `FreezeBuildupPct` × 目标 `FreezeBuildup` / `ImmobilisationBuildup` 倍率。默认来源是冰冷；`<Type>CanFreeze` 可允许其他伤害类型，例如 Pinnacle of Power。实现为 `calc::ailment::freeze_buildup_terms`，共用重度晕眩的减免后伤害。不命中或具有 `CannotFreeze` 时为零。

`active_skill_hit_damage_freeze_multiplier_+%_final` 提供攻击者的更多累积；Freezing Mark 的 `freezing_mark_hit_damage_freeze_multiplier_+%_final` 提供目标的 MORE。明细保留 `FreezeSourceHitAverage`、`FreezeBuildupPerDamage`、`EnemyFreezeBuildupMultiplier`；持续时间明细保留 `BaseAilmentDuration`、`AilmentDurationIncreased`、`AilmentDurationMore`、`EnemyDebuffDuration`，幅度明细保留 `AttackerChillMagnitude`、`TargetChillMagnitude`。

### 已有冻结差异与未决问题

下列数字是已有 worked 对照记录，不是本次文档审查重跑的结果。首领 poise 的 17×／9.38× 差异与重度晕眩相同，单这一项使 Pinnacle 上 PoB2 的累积约为 PoBR 的 1.81 倍。

| 构筑 | `FreezeBuildupAvg`：PoBR / PoB2 | 已知原因 |
|---|---|---|
| benchmark Deadeye Ice Shot `b6ce7b3a4329` | 9.035 / 12.128 | 比例 0.745 = 0.552 × 1.35；后者是 Rhoa 使 Minion Pact II 的 35% 更多伤害成立。每点伤害的 35% increased、25% more 和 Freezing Mark 倍率一致 |
| tuned Decompose `7505238a9c8d` | 0 / 1.079 | 毒云不命中；Chaotic Freeze 允许的是命中正在承受受辅助技能持续伤害的目标，不能让毒云本身变成命中 |
| tuned Oil Barrage `02b94461578f` | 0.440 / 1.548 | 比例 0.284 含 poise 0.552、冰伤减免 0.671，以及下述未决的 0.769。Topotante 按导入值穿透 15% 而非 25%，Pounce's Mark 为 0% 而非 32% 承伤；50% 更多冻结的辅助在 Spear Stab 组 |

2026-10-17 的 Oil Barrage 调查：主技能在第 9 组，辅助为 Nova Projectiles I、Salvo、Ricochet II、Fork、Armour Break III。[Tainted Strike](https://poe2db.tw/us/Tainted_Strike)（`criticals79`，节点 `22864`）的 `non_damaging_ailment_effect_+%_with_critical_strikes` 给暴击造成的非伤害异常 30% increased 幅度。PoB2 将其算入 `EnemyFreezeBuildup`，配合 Suppressing Fire（`44746`）的 40% more Immobilisation 得到 1.30 × 1.40 = 1.82。PoBR 解析到 `FreezeBuildup`，但累积查询未开启暴击条件，因此为 1.40。

当时的临时副本只增加一条相反的 30% reduced 词条，PoB2 从 1.547927978 降至 1.190713830，正好相差 1.30 倍；100% 暴击率、冰冷命中伤害和 DPS 不变。PoBR 两次都是 0.440092450。这个实验确认了差异来源，**没有证明“异常幅度”在游戏里应当放大冻结累积**。[游戏关键词](https://poe2db.tw/us/Keywords)分别定义了幅度和累积，没有明确两者等价。现有映射来自导出器；仍需明确游戏规则或客户端实测，不能据此修改引擎或新增 known-difference 豁免。

### 施加的减速幅度

`base_debuff_slow_magnitude_+%` 表示角色施加的减益中，减速部分的幅度；它不同于角色受到减速的 `base_slow_potency_+%`。游戏 `PassageUniqueAmanamuIncreasedDebuffSlowMagnitude` 的范围为 12–20。[Slow Magnitude Modifier](https://poe2db.tw/us/Slow_Magnitude_Modifier)限定它只改减速部分。

当前规则接到 `EnemyChillMagnitude`。例如基础 30% 冰缓，20% 冰缓幅度与 20% 减速幅度相加，得到 42%，再受[冰缓默认 50% 上限](https://poe2db.tw/us/Chill)限制。目标的减速效力另算；50% less 效力使 42% 变为 21%。命中估计与明确的 `ChillBase` / `ChillMinimum` 都使用它；已经表示最终幅度的敌人 `ChillVal` 和角色 `ChillOverride` 不再缩放。

两种 increased 相加是实现假设。[官方 0.2.0 补丁](https://www.pathofexile.com/forum/view-thread/3740562)明确了 Temporal Chains 的诅咒幅度与减速幅度相加，但没有直接证明冰缓的这一组合，**没有游戏内实测**。该词条不提供施加来源或覆盖率，只有冰伤也不会自动降低敌人行动速度；它不改自身减速、感电、冻结或持续时间。规则为 `exporter: "absent"`。Temporal Chains 的敌人减速部分、Hinder/Maim 的敌人移动速度仍缺计算模型，不能宣称已经一并支持。

## 晕眩、钉住与 poise

`HeavyStunBuildupAvg` 是一次成功命中的平均重度晕眩累积，占 poise（怪物累积门槛）的百分比。游戏 `KeywordPopups` Stun / HeavyStun（55 / 57）以造成的伤害为依据，Premitigation（422）另指减免前伤害。PoBR 复用命中 DPS 的逐类型减免后伤害，非暴击和暴击分别算护甲后再加权；只在暴击中出现的伤害也计入。它不乘命中率，交替双手按每次成功命中平均。

玩家的物理伤害与近战命中各给 50% more 累积，两者相乘；冰冷不积累重度晕眩（`KeywordPopups[3]`）。怪物的 100% 物理／33% 近战加成是怪物造成伤害的规则，不能套给玩家。

[Daze](https://poe2db.tw/us/Daze)使目标承受 50% more 晕眩累积。`conditionEnemyDazed` 只在有效模式成立。近期合并的 `daze_magnitude_+%` / `DazeMagnitude` 已由 `setup_env::scale_enemy_daze` 读取：若幅度增加 i（以比例表示），总倍率为 `1 + 0.5 × (1 + i)`；已有基础 1.5 倍，所以代码另乘 `(1 + 0.5 × (1 + i)) / 1.5 = 1 + i / 3`。50% increased Daze 幅度使总倍率从 1.5 变为 1.75，不改变 Daze 几率或持续时间。拥有施加来源不会自动开启敌人 Dazed 状态。幅度规则为 `exporter: "absent"`；比例缩放有回归测试，没有游戏内实测。

poise 计算读取 `GameConstants.HeavyStunDamageScale = 58 / 100 = 0.58`、对应等级的 `DefaultMonsterStats.PoiseThreshold`、角色的 `EnemyHeavyStunBuildup` / `EnemyImmobilisationBuildup`，以及敌人的累积和门槛修正。独特稀有度的 500% MORE（`Mods.MonsterUnique2`，即 6 倍）再乘 `MonsterVarieties.PoiseThreshold / 100`，均受 `Condition:Effective` 限制。

活动版本 0.5.5.4.2 的首领表由 `data/overlay-common/boss_poise_thresholds.json` 手工保留行 ID 和导出 SHA-256：普通 Boss 用 313（带 BossHealthBar 的 MAP 行众数）；Pinnacle/Uber 用 Xesht 行 1946 的 1700，即 17 倍。导出中没有独立 Uber variety。这是代表性默认值，不表示所有首领门槛一样。`base/enemy_presets.json` 中的 vendor Map Boss/Xesht poise 不用于此计算；常规导出支持这一列后应改为生成。

`CannotStun` / `CannotHeavyStun` 禁止累积。Mauling Stuns 与 Focused Thrust 共用距离处理，要求 `enemyDistance <= 米数 × 10`；未提供距离时沿用该处理器的零距离约定。

### 钉住

`PinBuildupAvg` 同样表示一次成功命中的减免后平均累积。游戏 `KeywordPopups[60]` 要求技能或效果明确允许伤害积累 Pin，不能让所有命中默认钉住。

| 来源 stat | 可积累的伤害 |
|---|---|
| `base_physical_damage_can_pin`、`support_pin_physical_damage_can_pin` | 物理 |
| `base_all_damage_can_pin` | 所有类型 |
| `local_all_damage_can_pin` | 该武器手别的所有命中类型 |
| `all_projectile_damage_can_pin` | 投射物命中 |
| `physical_spell_damage_can_pin_on_critical_hit`（Bone Chains） | 法术暴击的物理部分；非暴击及其他类型不计 |

这些映射为 `CanPin` / `<Type>CanPin`；`cannot_pin` 和“Cannot Immobilise”使结果为零。`calc::ailment::pin_terms` 分开读取非暴击和暴击伤害。比例为 `GameConstants[45].PinDamageScale = 420 / 100 = 4.2`，门槛共用 poise（`PinThresholdModifier[48]` 与 `HeavyStunThresholdModifier[29]` 都是 500）。角色的 `EnemyPinBuildup` / `EnemyImmobilisationBuildup`、目标的 `PinBuildup` / `ImmobilisationBuildup` 再缩放它；`active_skill_pins_as_though_dealt_damage_+%_final` 也接入这组累积修正。

重度晕眩的物理、近战各 50% more 不用于 Pin，因为 Stun / HeavyStun 文本只将其给晕眩；**没有游戏内实测**。Pin 辅助的 `support_pin_buildup_pin_instead_of_stun` 使重度晕眩累积为零，改为 Pin；PoBR 不像 PoB2 那样把技能的重度晕眩修正复制给 Pin。

`PinBuildupRequired` 通常为 100%；有 `pin_almost_pinned_enemies`（Constricting Command，`PinOnPrimed`）时，取 `KeywordPopups[400].PrimedPin` 的普通／魔法／稀有／独特 40%／50%／60%／70%。它允许命中已临近钉住的目标，但不自动授予累积来源。`PinDuration` 取 `GameConstants[46].PinDuration = 3000 / 1000 = 3` 秒，应用 `pin_duration_+%`、Pin 辅助的 MORE、目标 `SelfPinDuration` 和减益过期速度。没有 Pin 来源、不命中或禁止 Pin 时，两项均为零。它们不模拟命中序列、衰减或覆盖率；`conditionEnemyPinned` 仍由配置决定。

新增的阈值和时长规则为 `exporter: "absent"`；Bone Chains 为 `differs`，因为 PoB2 对整个技能判断一次后让所有伤害类型积累 Pin，而且使用减免前平均伤害。已有对照：Reap `3875ffe5d15e` 为 12.976 / 55.896（0.232 = poise 0.552 × 物理暴击份额 0.42）；Oil Barrage `02b94461578f` 为 6.615 / 17.082，Crossbow Shot `406c76f8e89a` 为 6.747 / 12.506，接近各自重度晕眩比例 0.396 / 0.541。Impending Doom `add19135df93` 为 0.364 / 0：Dark Consequences 的 3260 物理爆炸按游戏类型是 Spell，可积累 Pin，oracle 的 `skillFlags` 却为空。

## 破甲与命中效果视图

玩家 `AilmentThreshold` 从最大生命的一半起算，加固定值和最终最大 ES 的百分比，再应用门槛 INC/MORE。依据是[游戏关键词](https://poe2db.tw/us/Keywords)。结果保留小数：CI 的 1 生命给 0.5，PoB2 则向上取整为 1。游戏文字没有规定取整边界；这是精度约定，未实测。

玩家自己的 `FreezeThreshold` 见[防御门槛](defence.md#抗性减伤与门槛)，不要与敌人 `FreezeBuildupAvg` 混用。

`LifeOnKill` / `ManaOnKill` / `EnergyShieldOnKill` 是每次击杀的固定恢复加对应最终最大资源百分比，遵守禁止恢复条件。命中击杀必须属于玩家；图腾、陷阱、地雷等代理不返还给玩家。它们不假设击杀频率，也不处理持续伤害击杀归属。Cannibalism 的 `support_cannibalism_recover_%_maximum_life_on_kill` 仍未映射（II 的值为 3）；Amazon Twister `5355205a1743` 少的 81 是机制缺口，不是单位差异。

完全破甲会移除护甲，使非玩家目标承受 20% increased 物理命中伤害。`FullyBrokenArmourEffect` 展示效果修正后的百分比，只有配置完全破甲才影响命中。The Molten One's Gift（28542）将它扩展到火焰，Storm's Rebuke（22726）扩展到冰冷、闪电，stat 为 `fully_broken_armour_and_sundered_armour_you_inflict_also_applies_to_*`。同一效果只缩放一次，包括前者自带的 15% increased。

这是玩家施加到共享敌人的减益，召唤物命中也受益；仅属于召唤物的授予或效果加成不能修改它。即使敌人原本无护甲，仍需完全破甲状态。持续伤害不受影响；拥有词条不代表自动破甲或永久覆盖。

`calc::effect_views` 的下列字段只是展示估计，不自动开启 Fully Broken Armour、Dazed、Maimed、Blinded 等状态。**没有游戏内实测**。

- `ArmourBreakPerHit = (固定破甲 BASE + Σ 减免后伤害 × <Type>DamageAsArmourBreak / 100) × (1 + INC / 100) × MORE × 稀有度倍率`。伤害在暴击加权后、命中率之前；法术暴击破甲只用暴击份额 `c·m / (1 − c + c·m)`。关键词规定对普通怪 3 倍、魔法怪 2 倍；当前没有魔法敌人配置，非稀有／独特使用 3 倍。`HitsToFullyBreakArmour = ceil(破甲前敌人护甲 / 每次破甲)`；`TimeToFullyBreakArmour = 次数 / 每秒成功命中数`，后者为动作速率 × 重复次数 × 命中率。来源包括 `armour_break_amount_+%`、`_vs_enemies_affected_by_ailments`、`apply_X_armour_break_on_hit`、`armour_break_{physical,fire}_damage_%_dealt_as_armour_break`、`support_increased_armour_break_armour_break_amount_+%_final`。
- `DazeChance` 最高 100%。`DazeUptime` / `MaimUptime` / `BlindUptime` 以 8／4／4 秒为基础，再计时长和过期速度。假设成功命中均匀到达，令 `n = rate × duration`、`k = floor(n)`、`f = n − k`、p 为每击概率，覆盖率为 `f·(1 − (1 − p)^(k+1)) + (1 − f)·(1 − (1 − p)^k)`；p=1 时即 `min(1, rate × duration)`。`maim_on_hit_%`、`attacks_chance_to_blind_on_hit_%` 只给攻击，禁止或免疫使其为零。即使估计接近 100%，也不会自动开启敌人状态。
- `Maximum{Fire,Cold,Lightning}Infusions = 3 + 对应类型及通用追加值`，基础来自 `BaseMaximumInfusionCount`。`SurroundedRadiusMetres` 从 3 米起按面积加成缩放；`BaseRequiredEnemiesToBeConsideredSurrounded = 5`，不代表自动放置五个敌人。
- `CurseActivationDelay` 只在选中诅咒时读取：`curse_delay_duration_ms × (1 + delay increase − activation-speed increase) × MORE`。Whispers of Doom 的 `keystone_hex_master` 给 MORE 100（双倍延迟）。
- `RemnantsPerUse = 1 + chance_%_to_create_additional_remnant / 100`，仅适用于 `GeneratesRemnants`；Remnant Attraction 为 10%，Lingering Illusion 为 156%。产生数量不等于拾取数量，不能乘到 Into the Breach 配置的紫焰层数。显式拾取视图见[残骸快照](snapshots.md#箭袋残骸volatility-与-combo)；Elemental Equilibrium 和 Storm's Recollection 的拾取后效果仍未建模。

上述新增展示规则为 `exporter: "absent"`。独立的 `chaos_immunity` 设置 `ChaosImmune`，与 CI 共用 `defence::chaos_immune` 处理混沌命中和持续伤害（`differs`）；CI 的资源池转换仍只属于该基石。
