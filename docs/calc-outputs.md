# 计算输出与明细契约

本文说明 CLI `calculate-build`、WASM `calculate_build_json` 的字段，以及 Calcs／侧栏的数值与单位。
页面行为见 [Web 契约](web-contracts.md)，机制规则见[机制约定](mechanics/README.md)。
新增字段先在 `display_catalog` 声明单位、PoB key 和明细种类。

文中的 BASE 是基础或附加数值，INC 是先相加的 increased／reduced 百分比，MORE 是逐项相乘的 more／less。
“有效模式”（effective）会计入敌人的减伤和状态。字段 id 是稳定接口名，界面翻译不能改变它。
DPS 指每秒伤害，EHP 指按所选敌人命中估算的有效生命；stat set 是同一技能效果的一组数值。

## CLI 与 WASM 响应

### 完整构筑与诊断

CLI 完整构筑和 marginal（逐来源边际贡献）结果带数据版本、树版本、未支持词条与 provenance。
调用方必须保留诊断，不能把未支持效果说成零效果。数据身份见[构筑模型](build-model.md#数据快照身份与会话)。

| CLI 字段／选项 | 含义 |
|---|---|
| `missing_items` | 导入器无法读取的已用物品；数值来自缺少这些物品的角色。 |
| `reference` | 与来源 PlayerStat 比较；主技能召唤物的 MinionStat 也列出，行名 Minion*，pob_key 为 `Minion:<stat>`。只在沿用导出条件时提供。 |
| `output.minions` | 每个创建的召唤物：怪物 id、召唤来源、所用 skill_id、等级、DPS、防御。 |
| `--breakdown <stats>` | 返回 breakdowns 和 main_skill，步骤／词条来自 pobr_build::breakdown。`Minion:<stat>` 选主技能召唤物，包含角色给它的词条。 |
| `--full-dps` | 返回 output.full_dps、full_dot_dps、full_dps_skills；请求 FullDPS／FullDotDPS 分解也会计算。未请求时字段缺省，不拿未算的零比参考。 |
| `output.ehp` | 实际 EHP 输入与过程量：逐类型敌人伤害、转换后承伤、资源池、命中次数、伤害概率与避开概率。 |
| `--reference-basis` | output 与分解改用参考口径，并标 reference_basis。 |
| `--usage` | 返回逐行 usage，始终用全部模型开启的游戏口径，不随 reference-basis 改变。 |

WASM `{ pob_code }` 请求把同类失败物品放入 `item_errors`。
`line_usage_json`（JS `lineUsageJson`）接受与完整计算相同的请求，返回 `{ usage, item_errors }`，不缓存。
台账状态见[诊断](diagnostics.md#输入在哪里丢失)。
TotalEHP／各类型 MaximumHitTaken 的敌人命中输入不能相加，所以不再列为词条行。
CalculationSession::enemy_hit_mods 可查敌人输入和来源；CLI 不输出 WASM 的 derived 明细。

### 召唤物与明细请求

WASM `calculateBuildJson` 的 minions 包含 index、minion_id、name、summoned_by、skill_id、level、
dps、life、armour、evasion、energy_shield，以及 stats、breakdowns、breakdown_descriptors。
每项是一个召唤物实例的快照，不乘数量。stats 包含火冰电混沌抗性、最大值和超上限值，
使用与角色相同的 *Resist／*ResistMax／*ResistOverCap id。四种最终抗性有逐步分解。
未建模值缺省，不补 0。

main_minion_index 由 reference_check::main_skill_minion 按提供效果的准确宝石和召唤来源定位。
同名召唤宝石各有结果，主组停用时无主召唤物。明细均带 actor。

| 请求字段 | 行为 |
|---|---|
| 省略 `breakdowns` | 角色 BREAKDOWN_STATS，加每个召唤物全部明细。 |
| `breakdowns: []` | 不生成角色或召唤物明细。 |
| `breakdowns: [id, ...]` | 先去重，只生成所选 id；未知 id 为参数错。 |
| `breakdown_actor` | `{kind: "player"}` 或 `{kind: "minion", index}`；index 属于本次响应，越界报错。 |
| `include_descriptors: true` | 内联角色 breakdown_descriptors；默认不复制。 |
| `config_context: true` | 额外计算并返回配置适用上下文；默认不返回。 |
| `full_dps: true` | 请求 Full DPS 汇总。 |

数值和 actor 身份不受明细选择影响，技能总览和 Full DPS 缓存忽略明细选择。
breakdownDescriptorsJson 无需游戏数据，每个引擎版本取一次；Web 的 getBackend()／descriptors.ts
在页面侧把同一份表附回响应，Worker 不反复复制。

| 明细种类 | 含义 |
|---|---|
| `steps` | 逐步计算，见 BREAKDOWN_STATS。 |
| `sources` | 一组对应计算阶段真实的一次 ModDb 查询，使用同样配置和 MORE 精度。 |
| `derived` | 复用实际输入的公式，不重跑机制；测试用输入重建输出。 |
| `none` | 当前没有可展示明细。 |

不能为凑明细把上游输入写成可相加的来源。测试还检查空选择／未请求 id 不生成明细。
配置上下文使用 calculate_with_data_session_tracked，在明细查询前取读取快照。
旧 benchmark 中跟踪最高慢 17.5%（Blood Mage），所以普通编辑不跟踪，只有配置页打开时请求。

## EHP 与防御字段

### 命中伤害与资源池

以下字段读取同一份 EhpTerms，不另算一套防御。类型包括 Physical、Fire、Cold、Lightning、Chaos。

| 字段 | 数值与单位 |
|---|---|
| `<类型>EnemyDamage` | 敌人加成与暴击后的伤害。 |
| `<类型>TakenDamage` | 承伤转换后、减免前的伤害。 |
| `<类型>TakenHit` | 减免后伤害。 |
| `<类型>TakenHitMult` | TakenHit／TakenDamage；分母为 0 时为 0。 |
| `<类型>EnergyShieldBypass` | 能量护盾旁路百分比。 |
| 元素 DamageReduction | 火冰电的减伤百分比。 |
| `totalEnemyDamage／totalTakenDamage／totalTakenHit` | 对应总量，只是已组装命中的展示。 |
| `ConfiguredDamageChance／ConfiguredNotHitChance／DamageTakenOnBlock` | 已求得的命中概率／格挡承伤项。 |
| `<类型>EnemyDamageMult／EnemyPen` | 敌人伤害倍率／穿透百分点；物理与混沌穿透输入为 0。 |
| `<类型>ResistTakenHitMulti` | 穿透后的抗性倍率，穿透不作用于负抗性。 |
| `<类型>AttackTakenHitMult／SpellTakenHitMult` | 攻击／法术承伤层，Average 类别取平均。 |
| `<类型>AfterReductionTakenHitMulti` | 所选命中类别承伤倍率 × 偏斜倍率。 |
| `<类型>BaseTakenHitMult` | 抗性 × 减伤 × 减伤后倍率，不含固定承伤。 |
| `<类型>DamageReductionMax` 及全局 `DamageReductionMax` | 对应减伤上限。 |
| `<类型>ManaEffectiveLife／MoMHitPool／TotalHitPool` | Mind over Matter 分担后的生命／命中池与总命中资源池。 |
| `LifeHitPool` | 可恢复生命计入生命流失防止后的池。 |
| 四种 ResistTotal | 截断后、抗性上下限之前的总值，复用 resolve_resistance。 |

Chaos Inoculation（CI，混沌免疫）下混沌承伤层为 0。
EhpTerms 保留敌人组装、MitigationCtx 减免快照和资源池。
EnemyDamage、EnergyShieldBypass、EnemyDamageMult、EnemyPen 只有聚合后值，无来源轨迹，明细为 none。
攻击／法术承伤为 sources，其余相应派生项为 derived。
资源池展示生命、MoM 魔力、能量护盾、Runic Ward／Aegis／Guard／友军等实际层。
没有反射伤害模型，不提供 TakenReflect。
Evergrasping、Runic Ward 与恢复的有意差异仍按[防御约定](mechanics/defence.md)解释。

### 防御持续伤害

| 字段 | 公式／边界 |
|---|---|
| 四种 ResistOverTime | 带 Dot 标志的 resolve_resistance；additional_*_resistance_against_damage_over_time_% 只进入此视图。 |
| 五种 TakenDotMult | max((1 − ResistOverTime/100) × (1 − 基础减伤/100) × 持续伤害承伤倍率, 0)。物理无抗性，CI 混沌为 0。 |
| 五种 TotalPool | MoM 后生命 + 按旁路计的能量护盾 + 未被旁路时的 Runic Ward；不含防止损失、护盾技能、Guard 等仅命中层。 |
| 五种 DotEHP | TotalPool／TakenDotMult，免疫时为无穷。 |
| `EHPSurvivalTime` | TotalNumberOfHits × 敌人攻击时间 ÷ 敌人行动速度。 |

护甲不减持续伤害，只读不分命中的额外减伤，受各类型上限约束。
net_recovery_terms 与这些字段共用 recovery::dot_taken，所以自身持续伤害的净恢复也读取持续伤害抗性、
元素基础减伤和 CI，而非命中抗性。四种持续抗性有 steps，其余为实际输入 derived。

敌人行动速度由 skill_use_time::action_speed_factor 读取敌人 ModDb，包含冰缓。
攻击时间只用显式 enemySpeed（毫秒，EnemyActionTime），未配置为 0。
游戏没有默认值，来源 700ms Placeholder 是导出工具假设，不采用。
PoB2 还使用自身行动速度修正，worked 样本中 enemySkillTime 多为 1.75／3.5s，不能直接比较；
output-coverage 此项不吻合不等于缺陷。

tuned Smith of Kitava Shield Wall fb03d7dce08e 的混沌持续倍率为 0.36，PoB2 0.34，
来自 Soul Core of Tacati 的[原文约定](mechanics/defence.md#资源池符文护盾与转换)。
Blood Mage Comet bc750d615159 的混沌抗性 55 对 65 沿用已有抗性差异；均无游戏内实测。
Infernalist Comet d7afa87a05ea 的生命／MoM 池随[符文原文差异](pob2-differences.md#已知的有意差异)低于 PoB2。
其他抗性差异与已有 TakenHit／ResistTotal 相同。

## Calcs 字段与伤害视图

### 等级、暴击、命中与恢复

| 字段 | 定义与限制 |
|---|---|
| `GemLevel` | 所选效果实际运行等级 = GemBaseLevel + GemBonusLevels，沿用[等级通道](mechanics/items.md#宝石等级品质与授予技能)。 |
| `PreEffectiveCritMultiplier` | Bifurcate／敌人暴击减免前，1 + max(0, 暴击伤害加成)，含基础、附加、INC、MORE、覆盖；不能暴击或禁用额外暴击伤害时为 1。CritMultiplierTerms 分 Primary／Secondary。 |
| `AccuracyHitChance` | 敌人格挡前的命中判定百分比；非攻击或 CannotBeEvaded／global_always_hit 为 100，输入为 Primary／SecondaryAccuracyChance。 |
| `AverageBurstHits` | 一次使用中的重复命中数，BurstTerms::hits，与 AverageBurstDamage 同源。 |
| `CullMultiplier` | 100／(100 − Culling Strike 阈值)，无机制时为 1，与 CullingDPS 同源。 |
| `ReservationDpsMultiplier` | 100／(100 − 敌人保留生命比例)，无机制时为 1，与 ReservationDPS 同源。 |
| `EnemyAilmentThreshold` | monster_scaling.json 的等级阈值乘敌人侧 INC／MORE。 |
| `EnemyCritChance` | 基础 × (1 + 增加) × (1 − 配置的暴击规避)，限 0–100；Always／Never 为 100／0，与 EnemyCritEffect 分开。 |
| `MaxLifeLeechRate／MaxManaLeechRate／MaxEnergyShieldLeechRate` | 每秒最大偷取速率，默认各最大池的 20%／20%／10%，再乘最大速率加成。MaxLifeLeechRatePercent 是生命比例。默认值沿用[恢复兼容约定](mechanics/defence.md#恢复偷取吸纳与充能护盾)，游戏未导出对应 stat。 |
| `DeflectEffect` | 偏转挡下伤害百分比 = 40 + BASE DeflectEffect，限 0–100；stat 为 base_damage_%_deflected，无游戏内实测。 |
| `SecondMinimalMaximumHitTaken` | 五种 `<类型>MaxHit` 的第二小值。 |
| `SelfChillEffect` | 玩家侧 (100 + INC) × MORE，与 SelfFreezeEffect／SelfPoisonEffect／SelfBlindDuration 同为[自身减益视图](mechanics/defence.md#自身减益减速与玩家一侧视图)。 |
| `EffectiveLootRarityMod` | (1 + LootRarity INC/100) × MORE；LootQuantity 仍无输出。护符按启用约定计入，未使用护符的 PoB2 不计，benchmark Amazon 为 1.15 对 1.0。 |

“increased amount of Mana Leeched” 改偷取量，不改速率上限，benchmark Oracle Lightning Bolt 为
212.73 对 PoB2 266.0，见[偷取差异](pob2-differences.md#已知的有意差异)。
当前每资源只允许一个偷取实例，不采用旧的每实例 10% 池上限，因此不输出 LifeLeechInstanceRate／MaxLifeLeechInstance。

### 药剂、护符与明细边界

两瓶药剂、三个护符（含未启用项）按槽位输出 `Flask<N>*`／`Charm<N>*`：总恢复、立即恢复、每秒持续恢复、
持续时间、最大充能、每次消耗、满充能使用次数、每秒充能。*ChargesGained 是获取倍率。
游戏 Flasks 的 LifePerUse／ManaPerUse／RecoveryTime（十分之一秒）与 ComponentCharges 的 MaxCharges／PerCharge
进入 base_items.json.utility，护符时间也用 RecoveryTime。品质提升药剂恢复或护符时间。
本地 local_* 只查该物品，不能放全局池。全局 FlaskEffect／*FlaskChargeGen 有来源查询，
槽位数值用实际输入公式；CLI utility_stats 与 WASM 同源。
只算单次使用和充能，不推触发／覆盖率，启用效果见[药剂护符](mechanics/items.md#药剂与护符)。

分组侧栏明细覆盖全部字段。NumberOfDamagingHits／NumberOfMitigatedHits 为 none，尚无耗尽模拟迭代轨迹。
最大承伤复用求解值、敌人倍率、资源池，不重跑求解器。sources 只解释查询，不保证等于含基础值的最终输出。
ChannelTimeToTrigger 为 Cast While Channelling 从空 Energy 到首次触发的秒数（总容量／持续获取速率，含增益），
不改变持续 DPS、重复频率或返还。目录不提供 CausticGroundDPS、MirageCausticGroundDPS、SustainableTrauma、TrapCooldown。
SkillDPS 逐技能信息由总览与 Full DPS 行承担。

### 平均伤害、区间与别名

TotalHitAvg 保留角色侧[减伤前伤害](https://poe2db.tw/us/Keywords)，包含暴击平均。
PoB2 AverageHit 含敌人减伤、未含命中率；AverageDamage 还计命中率，三者不是同一视图。
即使 catalog.pob_key 为 AverageHit，TotalHitAvg 仍排除参考吻合统计，不能改底层命中／DPS 来凑一致。
Comet dd7f512456b6、Living Bomb 9a0fb35e890a 的敌人元素倍率 0.5 解释约两倍差异；
Seismic Cry 3e588bde3126 的物理／冰霜倍率为 0.25／0.5。

AverageBurstDamage 再计一次使用的重复；CombinedAvg 与 WithIgniteDPS／WithBleedDPS／WithPoisonDPS
依技能用平均伤害或 DPS 组合。IgniteDamage 等是单异常 DPS × 有效时间，不能用叠层 DPS 再乘。
MainHandAccuracy／OffHandAccuracy 各用自己的作用域。
Duration、DurationSecondary、AuraDuration、ReserveDuration 读取四个游戏持续时间通道，通用加成只用一次。
范围内部单位除以 10 为 AreaOfEffectRadiusMetres。
PierceCount／ForkCount／ChainMax／SplitCount 共用禁止与转换规则；概率次数可为小数，无限穿透编码为 100。
FiringRate 是装填前射速，ReloadTime 是立即装填几率折算前的装填时间。

其他兼容别名为 ShockEffect → ShockEffectMod、AoeAreaMod → AreaOfEffectMod、
TotalTotemPlacementTime → TotemPlacementTime。除有意差异外，单位／默认须与对应 key 一致。
BlockEffect 是格挡阻止的命中伤害百分比，默认 100（[Blocking](https://poe2db.tw/us/Keywords) 阻止完整命中）；
内部 block_effect 仍是 EHP 的承伤份额，仅显示取补数。
ShockEffect／ShockEffectMod 是玩家和敌人两侧 `(1 + INC/100) × MORE` 的乘积，默认 1，
技能不能感电时为 0；它不是基础 20% 承伤或命中／阈值推导的幅度，原 shock_effect 另留。
AoeAreaMod 也是倍率，默认 1，即使基础半径未知；未知半径仍为 0。两者用数字单位，不是百分点。

Calcs 的 Physical／Fire／Cold／Lightning／Chaos + Min／Max 及 TotalMin／TotalMax，
是非暴击经敌人减伤后的区间。直接用已有分量乘保留的 EffMult，明细列减伤前界限／倍率，不再求减伤。
保留小数，PoB2 取整可能让小命中相对误差较大，见[精度差异](pob2-differences.md#已知的有意差异)。
攻击区间、EffMult、CritEffect 放 MainHand…／OffHand…，顶层同名为 0；法术用顶层。
DpsMultiplier 含技能、INC、MORE、手榴弹重复与弹幕因子，双持交替取平均、同时取和，不含 QuantityMultiplier。
Repeats 沿用重复数，TotalNumberOfHits 是死亡前承受敌人攻击次数（含避免命中），不是技能命中数。
StrikeTargets 不乘单目标伤害。

## 侧栏的机制、上限与倍率视图

以下由 calc/sidebar.rs、limit_display.rs 读取已计算结果，不另运行机制。

### 移动、光环与分类伤害

| 字段／机制 | 行为 |
|---|---|
| `MovementSpeedMod／ActionSpeedMod／EffectiveMovementSpeedMod` | 移动先含 ArmourTypes.IncreasedMovementSpeed 的装备惩罚；再乘行动速度，最终舍入至 0.001。负行动速度受 SlowEffectOnSelf 缩放，UnaffectedBySlows 去掉减速，正加成保留。 |
| `MovementSpeedWhileUsingSkill` | 只对 UsableWhileMoving／can_perform_skill_while_moving 技能，乘 movement_speed_+%_final_while_performing_action；Mobility／Deliberation 等只改使用时惩罚，不改角色常态速度。 |
| `PresenceRadiusMetres／PresenceRadius` | 基础 4m，自[0.2.0](https://www.pathofexile.com/forum/view-thread/3740562) 起，只读 Presence 范围／半径，如 unique_double_presence_radius，不读技能范围。原单位字段为米值 × 10，米字段独立保留下限。 |
| `AuraEffectMod／CurseEffectMod` | 选中光环／诅咒时，含全局与所选技能／辅助自身效果。SelectedMagnitude／SelectedCurseEffect 等只供显示，buff 已算过，不能再放大。 |
| `ActiveMinionLimit` | 所选召唤技能的 SelectedMinionLimit；无上限 stat 的同伴为 1。Wolf Pack 的 skill_wolf_pack_size 在 1／18／20 级为 3／5／6。 |
| `ReservationDPS` | Blood Price 的敌人 Presence 保留取最强一份，不相加；effective 下 CombinedDPS 乘 100／(100 − LifeReservationPercent)，此字段是额外部分，单次命中／异常不变。 |
| `BurningGroundDPS` | Ball Lightning、Incinerate、Oil Grenade、Spear of Solaris 的点燃地面形态取已有点燃；直接燃烧地面取技能 DoT。 |
| `DecayDPS` | Profane Decay 的 TriggeredDecayingHexPlayer 自身 DoT。 |
| `MirageDPS／MirageBurningGroundDPS` | skill_disabled_unless_cloned 的复制体视图，如 Mirage Archer。上述分类伤害已在结果中，不能再加 CombinedDPS。 |
| `ExplodeChance` | EnemyExplodeChance，最高 100%；还没有爆炸伤害计算。 |
| `CombinedAvgToMonsterLife` | CombinedAvg 占敌人等级生命表的百分比。 |
| `CritBifurcates` | 1 + c/(2 − c)，c 为一次暴击掷骰几率（含命中率、先算 Lucky），双持各算后平均。无机制或非 effective 为 0。比较用倍率，界面显示 (值 − 1)×100%，1.33333 为 33.3%。 |

### 上限、恢复与阈值

| 字段 | 定义 |
|---|---|
| `CharmLimit` | env_finalize::charm_slots 得到的可用护符栏。 |
| `EnemyCurseLimit／EnemyMarkLimit` | 玩家自己的 player_curse_limit／player_mark_limit，含 CurseLimitIsMaximumPowerCharges。内部 enemy_curse_limit 是所有施法者最大值，可能更高。 |
| `LightRadiusMod／LifeRecoveryRateMod／ManaRecoveryRateMod／EnergyShieldRecoveryRateMod` | 对应 `(1 + INC/100) × MORE`；恢复倍率与 Recoup 共用 recoup_recovery_rate 查询。LightRadius 每行因此算已读，即使没有转换。 |
| `GlobalGemAttributeRequirements／GlobalItemAttributeRequirements／GlobalWeaponAttributeRequirements` | 共用 global_requirement_modifiers 的各类需求倍率，含 GlobalAttributeRequirements，默认 1。 |
| `LowLifePercentage／LowestAttribute` | LowPoolThreshold × 100 = 35，与自动 Low Life 条件同源；最低属性是最终三属性最小值。 |
| `FreezeThresholdAdded` | 非负 BASE FreezeThreshold，包含“获得最大能量护盾的 N% 作为额外冰冻阈值”。 |
| `FreezeThresholdMod` | FreezeThreshold 的 `(1 + INC/100) × MORE`，默认 1。 |
| `FreezeThreshold` | `(AilmentThreshold + FreezeThresholdAdded) × FreezeThresholdMod`，derived 明细列这三个实际输入。 |

冰冻总阈值以角色异常阈值为基础，再加额外值和倍率。这是当前模型约定，尚无游戏内测量。
测试 limits_reservation.rs::freeze_threshold_views_read_the_additions_and_the_increases 验证
额外护盾份额、1.6 倍率和总公式。不要与敌人的 EnemyAilmentThreshold 混淆。

The Vertex 的“装备没有属性需求”是 100% less GlobalItemAttributeRequirements，
装备与武器倍率为 0，而未应用该行的 PoB2 为 1／3。
在已有 worked 样本中，这组字段的差异仅见已登记的 Spirit Walker Zekoa e071、Blood Mage Impending Doom add1。

MaximumChill／MaximumShock 来自 ailment_apply::maximum_terms；CurrentChill／CurrentShock
来自 Env::applied_ailments。基础最大为 Chill 50、Shock 100，加 ChillMax／ShockMax 或覆盖。
Shock 的 100 来自 PoB2 派生常数 SHOCK_MAX_EFFECT，未核对游戏。
有施加来源时当前值为 `floor(min(max(来源幅度, 敌人已有幅度), 最大值))`，无来源为 0。
PoB2 无条件加敌人基础 30 Chill，所以 worked 样本 CurrentChill 都不一致；PoBR 不虚构冰缓来源。
CurrentShock 两边为 0，最大值一致。
LowLifePercentage、LowestAttribute、四种异常视图及以下保留／持续／目标数为 derived，其余相应项用来源查询。

### 永久保留、满资源与打击目标

LifeCancellableReservation／ManaCancellableReservation 是技能保留可释放的资源百分比：
`max(100 − Σ BASE Extra<Pool>Reserved, 0)`，默认 100。
Beidat's Hand／Gaze／Will、Midnight Bargain 等永久保留进入总保留与低资源百分比，
Reservation Efficiency／ReservationMultiplier 只缩放技能部分，不能缩放永久部分。
没有额外永久魔力／精魄游戏来源可据此扩展，所以不读取 ExtraSpiritReserved。

DurationMod 用 duration_modifier_terms 的 `max((1 + INC/100) × MORE, 0)`，
合并 Duration／PrimaryDuration，与实际持续时间共用，默认 1。
PoB2 还混入异常持续和地雷持续；本引擎异常另算，PoE2 无地雷，所以可不同。
FullLifePercentage 为 full_life_threshold_%_override 的覆盖或 100，当前无物品／节点使用，无实测；
FullManaPercentage 对 full_mana_threshold_%_override 相同，Idle Hands 的 UniqueFullManaThreshold1 为 90。
两条映射 exporter-absent，都不自动设置 FullLife／FullMana；条件仍显式配置，CI 可提供 FullLife。

StrikeTargets 仅在 MeleeSingleTarget（Strike）时为 `1 + Σ BASE AdditionalStrikeTarget`，否则 0。
melee_attack_number_of_spirit_strikes 的额外附近目标不乘单目标 DPS；Slam 不是 Strike，见[关键词](https://poe2db.tw/us/Keywords)。
祖灵增幅读取明确攻击快照，见[快照约定](mechanics/snapshots.md#祖灵增幅)，额外目标仍不乘伤害。

## 精魄保留

SpiritReserved 显示启用技能请求的全部保留量，即使超过资源池。
SpiritUnreserved = Spirit − SpiritReserved 可以为负，计算器不会自动停技能以适配容量。
这是超用 BD 的报告方式，不表示游戏允许超额保留。

PoB2 显示的 SpiritReserved 截到 Spirit，但剩余值和逐技能分解未截。
tuned Bonestorm be0d16c86268 两边逐技能均为 100 + 54 + 60 + 30 = 244，
PoB2 总保留显示 137，双方剩余 −107。Blood Mage Comet bc750d615159 请求 355、池 200、剩余 −155。
output-coverage.py 仍直接比较该字段，并在截断对上列 oracle_before_pool_cap。
先比未截总量和逐技能，再查效率／辅助成本。
免费项链授予和物品技能去重仍是[独立差异](pob2-differences.md#已知的有意差异)，不能用去掉显示上限解释。

## Full DPS

`calc_orchestrator::full_dps::calculate` 由 DataOrchestratorOptions::full_dps 开启，默认关闭。
它把 PoB includeInFullDPS 组中的每个启用效果相加，不模拟轮转。
数量优先 groupCount，否则宝石 count，可有小数；非正或非有限数不计。
每个效果用自己的武器、天赋、辅助和触发环境；召唤物用该宝石创建的实例。
同一物品触发技能已有可编辑纳入组时，跳过自动重复行。

命中 DPS 乘数量相加。流血、点燃、毒共用各自层数上限，allocate_stacks 先分给单层伤害较高者。
同技能不可叠 DoT 取最强，可叠 DotCanStack 才乘数量。DoT 总上限与最强 Culling Strike 各只算一次。
FullDpsSkill 每行含数量、单实例命中 DPS、分配 DoT、贡献、未支持词条和错误；贡献和等于 FullDPS。

includeInFullDPS、groupCount、宝石 count、enableGlobal1/2 往返保留；WASM 为
include_in_full_dps、group_count、count。Web 技能页编辑，只在有组纳入时请求 full_dps: true。

## 不输出的字段

没有当前机制依据时不输出常数或猜测值：

| 字段 | 原因／替代 |
|---|---|
| `ActiveTrapLimit／ActiveMineLimit` | 0.5 数据无 Trap／Mine 技能类型或辅助，只有旧 Trappable／Mineable 与 Hazard。旧上限 stat 无当前引用，游戏常数无 PoB2 的 15。Hazard 另用 ActiveHazardLimit：所选技能的 Σ BASE HazardRuneLimit，Conductive Runes 为 20，其余 0，当前无词条／节点修改。 |
| `BloodCharges／BloodChargesMax／InspirationCharges／InspirationChargesMax` | 只余旧 stat，如 show_blood_charge_info、vaal_reap_additional_maximum_blood_charges、inspiration_charge_duration_+%，无当前使用。游戏球类型只有 Endurance／Frenzy／Power，PoB2 的 5 是默认设置。 |
| `TTakenReflect` | PoB2 的 `<Type>TakenReflectMult`／AnyTakenReflect 写死 false，不输出。游戏只有权重 0 的 ReflectDamageTakenEssence1、无当前暗金使用的戒指词缀，以及地图等词缀；Bramblejack 反射给攻击者，不能当玩家反射承伤减免。 |
| `OffensiveWarcryEffect／MaxOffensiveWarcryEffect／AilmentWarcryEffect／RallyingHitEffect` | 来自 PoE1 按友军数增伤的 Intimidating／Rallying Cry；当前 PoE2 无对应技能和 stat，Rallying Cry 只作为 LargeScarabBuff 显示名，旧快照恒 1，不能输出该常数。 |
| `LifeLeechInstanceRate／MaxLifeLeechInstance` | 旧 PoB2 单实例上限，当前规则不采用，见上文偷取。 |

图腾防御不沿用 PoB2 固定的 40 元素／20 混沌值，而按图腾宝石等级 stat 计算；
其他图腾字段边界见[召唤物与图腾约定](mechanics/minions.md)。
