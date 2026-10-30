# 机制约定：防御、资源与恢复

本文说明资源池、抗性、减伤、恢复和玩家自身减益。索引见[机制约定](README.md)，参考值差异见 [pob2-differences.md](../pob2-differences.md)。EHP 的连续受击计算与静态恢复面板各有边界，不能把面板上的潜在恢复率直接当作战斗中一直生效的回复。

## 资源池、符文护盾与转换

### Runic Ward

Runic Ward 在生命降至 1 后吸收伤害；[官方 0.5.0 说明](https://www.pathofexile.com/forum/view-thread/3932540)确认了这个触发位置。Grasping Wounds（节点 56162，`life_loss_prevented_%`）只延后生命部分的损失，不保护 Ward。

`pool_damage::reduce_pools` 先扣 Ward，再对生命部分限制过量伤害。例如 1000 生命、500 Ward、25% 损失延后承受 1500 伤害：其中 1332 使生命降到 1，333 被延后；剩余 168 扣 Ward，留下 332。此前 PoBR 与钉定 PoB2 都先丢弃过量伤害，可能让 1 生命时的大命中只扣 1 Ward。冻结语料 Blood Mage 01/02/04/05/07/08 都有此组合，参考值一致不能证明正确。上述交互有算术测试，**没有游戏内实测**；延后损失的时序仍沿用现有模型。

装备 Ward 的计算顺序是：基底 + 物品隐式／显式 `local_ward`，再乘本地 `local_ward_+%` 和品质。例如 The Brass Dome 的本地 Ward 只属于胸甲；合金和武器上的同类文本则是角色 `base_maximum_ward`。实现见 `item::defence::item_ward_modifiers`。

`base_maximum_life_%_to_gain_as_maximum_ward` 以生命加成前的基数获得额外 Ward，再乘 Ward 修正；与现有资源 gain-as 转换和 PoB2 一致，未实测。从生命、魔力转换或获得的量取最近整数。装备防御按栏位名称排序后累加，避免物品读入顺序改变 .5 附近的结果。

已有 tuned Shield Wall `fb03d7dce08e` 对照保留 Tacati 的导入 +11% 混沌抗性，不改用现表 +13%。加装备 +53% 得 64%；386 混沌命中扣 138.96，PoB2 的 66% 则扣 131.24。包括 Brass Dome 本地 +991 在内的 Ward 为 2162，PoBR `ChaosMaxHit = 8001`、`TotalEHP = 35908.717951478`；旧构筑码参考值为 2468／11096.445514375。这是输入和本地 Ward 规则的结果，不是本次重测。

### Widow's Reign

`reserve_life_instead_of_loss_from_damage_for_x_ms` 把伤害导致的生命损失改为生命保留。[当前物品文本](https://poe2db.tw/us/Widows_Reign)给 3000 ms 释放窗口，`overlay/uniques.json` 还保留 5000 ms 旧变体。`pool_setup` 读取 `DamageInsteadReservesLife`；`PoolState.life_reserved_by_damage` 在 ES、MoM 等吸收层之后记录伤害保留量。

伤害保留与最大生命、因消耗而缺失的普通生命、技能／永久保留分开。即使普通生命已缺失，伤害仍占用保留容量，然后把当前生命压到剩余容量内，不重复扣生命。容量耗尽即死亡，CI 的最后一点也不例外；混沌免疫和 ES 穿透仍照常处理。

`life_recovery_cap` / `recover_life` 可恢复普通缺血，但不解除保留，也不能复活。EHP 连续受击及其批量加速步骤，对格挡恢复使用同一上限。伤害仍可产生吸纳，吸纳不能解除保留；保留效率只改变技能保留。

初始 `LifeRecoverable` 是配置中未保留的生命；后续状态扣掉新增的伤害保留。技能继续开启，Beidat 等永久保留也不能使用。PoB2 改用全部可取消保留的生命作为初始容量，并允许受击循环中的普通恢复；PoBR 不假设玩家关闭技能，要模拟这一选择必须直接改配置。

静态计算和连续受击循环都不推算无伤害的 3／5 秒窗口，不自动释放保留。调用方可显式提供当前 `life_reserved_by_damage` 和 `life`，但没有新增 UI 控件。持续伤害仍使用 Life/MoM/ES/Ward，不使用仅限命中的 Guard 或损失延后，也没有按时间推进的 DoT／充能过程。恢复面板描述普通缺失资源的潜在回复，不表示再生或偷取能抵消伤害保留。释放时机、取整和延后生命损失的交互均未做客户端实测；内部沿用连续精度和原有损失延后顺序。

### 其他资源转换边界

Discipline 这类直接增加总 ES 的效果，在转换为生命或魔力时，放在目标资源的 INC/MORE 之后。它不重新成为基础池，也不回流到转换矩阵；来源仍可追踪。

同伴分担不超过来袭伤害或同伴剩余生命。生命恰好耗尽时判死，避免浮点残余造成不死循环；真实的正数剩余生命仍保留。来袭暴击使用来袭 Hit 上下文，不受玩家所选技能类型影响。

## 抗性、减伤与门槛

Smith of Kitava 的 `modifiers_to_fire_resistance_also_apply_to_cold_lightning_resistance_at_%_value` 把火抗词条按指定比例给冰、电抗；`modifiers_to_maximum_fire_resistance_apply_to_maximum_cold_and_lightning_resistance` 则全额复制最大火抗词条。剧情元素抗性惩罚不属于火抗词条；全元素抗性已经给过冰、电，也不重复复制。

先合计可复制的词条，再乘比例，保留半点等小数；到最终抗性和最大抗性时才向零截断，最后求超上限抗性（`calc::offence::resolve_resistance`）。

“Body Armour grants” 系列只在穿普通稀有度胸甲时生效，包括 Kitavan Engraving（`body_armour_grants_x_maximum_life_+%`）、Leather Bindings（`body_armour_grants_x_life_regeneration_rate_per_minute_%`），以及 Tribute to Utula、Support Straps、Tantalum Alloy、Flowing Metal、Molten Symbol 的精魄、力量、晕眩门槛、抗性、最大火抗、护甲作用于元素伤害、物理命中改以火焰承受。

Crystallised Immunities（5332）读取 `immune_to_chill_if_majority_blue_supports_socketed`、`immune_to_ignite_if_majority_red_supports_socketed`、`immune_to_shock_if_majority_green_supports_socketed`。按启用组中辅助宝石的 `SkillGems.GemColour` 统计：红／力量、绿／敏捷、蓝／智力。某色必须**严格超过全部辅助的一半**，空集、平局或仅数量最多都不够。效果进入 `ChillImmune` / `IgniteImmune` / `ShockImmune`，给 100% 避免。PoB2 用严格相对多数，规则为 `exporter: "differs"`。依据为活动版本 `base/stats.json`、`base/passive_tree.json` 和 `base/skill_gems.json`。

Strong Chin（10998）的 `stun_threshold_+_from_lowest_of_base_helmet_evasion_rating_and_armour` 读取 `LowestOfArmourAndEvasionOnhelmet`：取头盔本地固定值、增加值与品质之后，角色全局防御加成之前的护甲／闪避较低值，再给 `StunThreshold` BASE。已知基底用当前游戏基础值，未知基底才用导入防御显示值；缺任何一项都取零，随后照常应用晕眩门槛 INC/MORE。

Projectile Bulwark（19236，`defend_with_%_armour_against_ranged_attacks`）按 120% 护甲抵御投射物攻击。它检查来袭配置 `enemyDamageType = Projectile`，不检查玩家技能。近战、法术、法术投射物、无类型和 Average 均不给加成。效果改用于减伤的护甲，供承伤、EHP、最大承伤和明细共用；不修改面板护甲或借用的闪避／ES。多条“defend with”沿用最强 `ArmourDefense`，不相乘；此叠加交互未实测。钉定导出器没有该专属规则，标为 `absent`。

Skin of the Loyal 的四个未封顶抗性标志，把对应最终未封顶抗性作为护甲、闪避、ES 或异常门槛的 increased，不能当 more。读取 buff／充能后的资源快照，但在防御转换之前，避免自身循环。`CompanionInPresence` 只有启用同伴技能时才默认开启，显式 true/false 优先。生效诅咒提供的 Hinder 会设置 `EnemyHindered`；依赖标记／诅咒的敌人词条在诅咒槽选择后再转发一次，不重复已有词条，因此 Marked for Sickness 要有实际生效的标记。

### 玩家冻结门槛

当前代码已同时展示三个字段（`calc::sidebar`、`calc::limit_display`）：

- `FreezeThresholdAdded`：非负的固定追加量，包括最终最大 ES 的百分比，stat 为 `freeze_threshold_+` / `gain_%_maximum_energy_shield_as_freeze_threshold_+`。
- `FreezeThresholdMod`：`freeze_threshold_+%` 等 INC/MORE 的倍率，中性值 1。
- `FreezeThreshold = (AilmentThreshold + FreezeThresholdAdded) × FreezeThresholdMod`，派生明细保留这三个输入。

[Freeze Threshold 关键词和 Icebreaker 文本](https://poe2db.tw/us/Freeze_Threshold)确认了门槛及“50% 最大 ES 作为额外门槛”；[官方 0.2.0 补丁](https://www.pathofexile.com/forum/view-thread/3740562)确认玩家基础异常门槛为一半最大生命。它们**没有直接证明上述冻结总值以最终 `AilmentThreshold` 为基数，也没有证明追加量与增加量的顺序**。这是当前显示模型的假设，未做游戏内实测。字段只展示玩家防御属性，不改变 DPS、EHP 或敌人的冻结累积。回归见 [limits_reservation.rs](../../crates/pobr-build/tests/skills/limits_reservation.rs)。

## 恢复、偷取、吸纳与充能护盾

### 再生与净恢复

`calc/recovery.rs` 将恢复分为三层：原始再生、经过限制与直接恢复后的 `LifeRegenRecovery` 等、扣自身损耗后的 `NetLifeRegen` / `NetManaRegen` / `NetEnergyShieldRegen` / `NetWardRegen`。四者之和为 `TotalNetRegen`；怒气消耗只进 `TotalBuildDegen`。

Ward 与怒气再生来自 `base_ward_regeneration_per_minute`、`base_rage_regeneration_per_minute`。角色固有魔力再生 `character_inherent_mana_regeneration_rate_per_minute_% = 240`，即每秒 4% 最大魔力；“No inherent Mana Regeneration”只移除这部分。Igniferis 的 `life_recovery_from_regeneration_is_not_applied` 阻止生命再生实际回复，原始值仍供转移机制读取。

自身损耗包括不经减伤的资源流失、经抗性和承伤修正的自身类型化 DoT、主技能每秒消耗。百分比消耗先按资源池换算，各资源扣自己的消耗；敌人 DoT 不进净再生。Berserk 的 `skill_base_life_loss_%_per_minute_per_rage_while_not_losing_rage_to_apply = 6` 是每点怒气每秒流失 0.1% 最大生命。PoB2 在没有自身类型化 DoT 时可能不输出净恢复，PoBR 仍输出。

`LifeRegenAppliesToManaRegen` 和 `ManaRegenAppliesToRageRegen` 只复制来源再生的 INC，不复制 BASE、MORE 或恢复修正；明细保留条件和来源。Eldritch Will 的 `maximum_life_mana_and_energy_shield_+%` 使用池计算实际读取的 `MaximumLife`、`MaximumMana`、`EnergyShield`；`Life` / `Mana` INC 别名不参与资源池。

### 偷取

[官方 0.5.0 补丁](https://www.pathofexile.com/forum/view-thread/3932540)改为每种资源同一时刻只让恢复速率最高的偷取实例回复，并给用于偷取计算的单次总命中伤害设 40,000 上限。超出时所有伤害类型等比例缩小，再按各自可偷取比例计算；额外元素伤害不能补足物理偷取。

PoBR 采用[游戏关键词](https://poe2db.tw/us/Keywords)的一秒基础时间，不再把单次偷取量限制为资源池的 10%。`base_life_leech_amount_+%` / `base_mana_leech_amount_+%` 增加总量，`base_life_leech_rate_+%` 等加快速率，不增加总量。最大速率先应用 INC/MORE，再限制实例。生命／魔力每秒 20%、ES 每秒 10% 的默认速率上限仍是项目兼容约定，**不是该补丁确认的常量**。

持续恢复按主技能平均命中频率估计覆盖，最快实例先回复，较慢实例只补其过期后的空档；不模拟随机暴击的时间序列。命中获得与瞬间偷取逐次相加。`LifeLeechGainRate` / `LifeLeechGainPerHit` 等面板仍是平均命中的原始单实例值，不必等于含恢复速率、瞬间部分或覆盖估计的持续回复。

Mystic Harvest（`life_leech_also_recovers_based_on_elemental_damage_types`）让已有生命偷取比例也可由元素伤害提供，但不赠送比例，也不解除攻击／法术限制。Essentia（`convert_all_life_leech_to_energy_shield_leech`）保留伤害类型资格，将基础生命偷取转给 ES；按[Stat Conversion](https://poe2db.tw/us/Stat_Conversion)使用目标 ES 的总量、速度、恢复、瞬间比例与禁止条件，生命命中获得仍回复生命。

Oisin 的 `mana_leech_from_elemental_instead` 使用元素魔力偷取并排除物理；不赠送比例，保留攻击／法术限制和独立允许的混沌偷取。40,000 上限仍先计整个命中，再筛选资格。

### 吸纳（Recoup）

[PoE2 Recoup 关键词](https://poe2db.tw/us/Recoup)给出：受到命中后，在 8 秒内恢复指定比例的伤害。不要套用 PoE1 的默认 4 秒。`4SecondRecoup` 或对应资源的 `4SecondXRecoup` 可把基时改为 4 秒。当前 `recoup_speed_+%` / `RecoupSpeed` 已作用于生命、魔力、ES：

`持续时间 = 基础时间 / ((1 + RecoupSpeed INC / 100) × MORE)`。

正速度下，总量不变、回复更快；实现遇到非正倍率时回退基础时间。速度规则为 `exporter: "absent"`。各资源再分别应用 `LifeRecoveryRate`、`ManaRecoveryRate`、`EnergyShieldRecoveryRate`；恢复速率与 Recoup 速度不是同一项。

普通生命吸纳对全部受伤类型生效。`PhysicalLifeRecoup`、`FireLifeRecoup`、`ColdLifeRecoup`、`LightningLifeRecoup` 只加到对应类型；“元素伤害吸纳为生命”展开为后三种，不能再乘全部受伤。ES 的 `recoup_%_elemental_damage_as_energy_shield` 同理展开为 `Fire/Cold/LightningEnergyShieldRecoup`。`LifeRecoupRate` / `EsRecoupRate` 明细与实际计算共用逐类型输入。[Sacrosanctum 的文本](https://poe2db.tw/us/Sacrosanctum)对应 `life_recoup_also_applies_to_energy_shield` 当前只复制通用 `LifeRecoup` 比例，ES 保留自己的时间和恢复速率；代码没有把新增的类型限定生命吸纳一并复制，不能写成全部吸纳效果均已转移。

“防止的物理伤害吸纳为生命”（`physical_damage_prevented_recouped_as_life_%`，[Made to Last](https://poe2db.tw/us/Made_to_Last) 5%，Wandering Reliquary 50%）使用单独的防止量：进入物理减伤阶段的伤害，减去护甲、额外物理减伤、上限及压制处理后的伤害。转为其他类型承受的部分和后续承伤倍率不算；格挡、偏斜不计，部分承伤格挡也不计，闪避不进入成功受击循环。

该定义参考 [Iron Ward](https://poe2db.tw/us/Iron_Ward) 对护甲或其他减伤的描述，是实现假设，**没有游戏内实测**。[2024 年 bug report](https://www.pathofexile.com/forum/view-thread/3661840)和[2025 年讨论](https://www.pathofexile.com/forum/view-thread/3882155)只是待验证线索，不是开发者裁定。EHP 循环累计防止量，`survivability::life_recoup_terms` 与普通／类型限定吸纳相加，共用生命时长和恢复速率。规则为 `absent`。

Forsaken Bangle 的混沌防止量沿用这一边界，但用混沌抗性与减伤，分别回复生命和魔力；具体假设见[药剂与护符](items.md#药剂与护符)。实现与回归见 [survivability.rs](../../crates/pobr-core/src/calc/survivability.rs)、[recovery.rs](../../crates/pobr-core/tests/defence/recovery.rs)。

### 能量护盾充能

`energy_shield_from_focus_+%` 和 `shield_armour_evasion_energy_shield_+%` 只对匹配副手装备的本地防御追加全局 increased。Focus 规则为 `differs`：游戏要求 `UsingFocus`，导出器却用于任何副手。

`energy_shield_recharge_rate_+%` / `virtual_energy_shield_recharge_rate_+%` 读取 `EnergyShieldRecharge`；`energy_shield_delay_-%_while_affected_by_archon` 读取 `EnergyShieldRechargeFaster`。`MovementSpeedAppliesToEnergyShieldRecharge` 只复制移速 INC，保留条件和明细，不复制基础移速、MORE、行动速度或移动惩罚，也不建立充能覆盖率。

Thawing Charm 的 `charm_energy_shield_recharge_starts_when_used` 沿用“启用且未超槽位的护符效果当前生效”约定：`UsingCharm` 下 `EnergyShieldRechargeBase = 0`，视为立即开始充能，不模拟护符触发或覆盖率（`absent`）。[Scarred Faith](https://poe2db.tw/us/Scarred_Faith) 的 `keystone_alternate_es_recovery` 则将充能量、速率、延迟都置零；其独立禁止再生条款使没有其他得失时净 ES 再生为零。当前 EHP 受击资源池不计充能。

## 充能与资源事件

`PowerChargesDuration` / `FrenzyChargesDuration` / `EnduranceChargesDuration` 从[Charges 的 15 秒](https://poe2db.tw/us/Charges)起算，乘通用与对应类型的 `(1 + INC / 100) × MORE`，最低零。`charge_duration_+%` 走三类规则，`base_charge_duration_+%` 走通用规则；`frenzy_charge_duration_+%_per_frenzy_charge` 用已解析的狂怒球数。保留小数：3% reduced 得 14.55 秒，PoB2 向下取整为 14。游戏 tick 边界未实测，不模拟到期或覆盖率。

[Resonance](https://poe2db.tw/us/Resonance)（`keystone_charge_cycle`、`gain_*_charges_instead_of_*`）将 Power → Endurance → Frenzy → Power 转向一次。没有获球事件模型，配置中超过最小值的球数被当作本次获得量；最小值和最大值不换类型，转入量合并并受目标上限限制。

Conduit（33979，`keystone_conduit`）把获得的球改给 Presence 友军，沿用上述快照：玩家三类球只留最小值，与 Resonance 同用也如此。最小球视为一直持有而非新获得，这一交互未实测。友军实际获球及其效果未建模；规则为 `absent`。两者都不模拟获得触发、消耗或持续时间线。

“近期消耗耐力球”的新计数与持有球数分开，见[配置输入](snapshots.md#配置项与敌人配置)。旧的暴击球近期消耗条件仍用可移除球数近似，见[装备与天赋](items.md#暗金珠宝与天赋文本)，不能把两种约定当成同一个事件模型。

[Event Horizon](https://poe2db.tw/us/Event_Horizon)（`chaos33_`，`lose_%_of_life_and_energy_shield_when_you_use_a_chaos_skill`）按玩家亲自使用的所选混沌技能，每次流失最终最大生命和 ES 的 3%。沿用费用频率，每个重复序列付一次；触发和代理不计。它进入净恢复及 `TotalBuildDegen`，不是可减免伤害，也不受费用修正。轮转中的其他技能、引导起止未模拟。此规则与 Scarred Faith 新识别的禁止再生文本均为 `absent`。

Thaumaturgical Dynamism（`ascendancy_gemling_charge_generator_buff_display`）从 `BuffTemplates.AscendancyGemlingChargeGenerator` 的 `gain_random_charge_every_X_ms_from_total_socketed_gem_attribute_requirements = 5000` 得到每 5 秒一次、每秒 0.2 次事件（[buff 数据](https://poe2db.tw/us/Thaumaturgical_Dynamism)）。`calc::charge_generation` 只显示总事件率；Conduit 可改为友军接收，重复授予不叠加。宝石属性需求影响随机球种，但关键词没有概率公式，因此不推算各类概率、当前球数、额外触发链或覆盖率，也不伪造 PoB 对照 key。

## 自身减益、减速与玩家一侧视图

`SelfFreezeEffect` / `SelfPoisonEffect` 等自身幅度视图使用非负 `(100 + INC) × MORE` 百分比。`non_damaging_ailment_effect_+%_on_self` 影响冻结、冰缓、感电；`unaffected_by_damaging_ailments` 可使毒效果为零。Blind 没有已知自身时长 stat，`blind_duration_+%` 是向外施加；`SelfBlindDuration` 因而为 100% × `DebuffExpirationModifier / 100`。这些视图本身不模拟来袭冻结、毒或致盲。

`calc::skill_use_time::action_speed_factor` 统一处理技能、战吼、代理放置、引导和移动中的负 `ActionSpeed`。按[游戏 Slow 定义](https://poe2db.tw/us/Keywords)，不同减速相乘，每项先用 `SlowEffectOnSelf`（`base_slow_potency_+%`）缩放；例如 50% 减速、30% reduced 效力，实际减速 35%。`UnaffectedBySlows`（`slows_have_no_potency_on_you`，Wanderlust / Relentless Pursuit）移除减速但保留正面速度。效力文本规则为 `absent`。

自定义词条可显式给行动速度减速；自身 Temporal Chains、仅影响移动的减益尚未建模，装备／技能移动惩罚不改。当前把所有负 `ActionSpeed` 当减速，是因为现有来源均为减益（自身 Chill、Temporal Chains、Maddening Presence）。游戏将非减益行动减速放在 `action_speed_-%`、`overencumbered_action_speed_+%_final` 等独立 stat；未来接入时必须另用名字，避免错误乘减速效力。`SkillSpeed`、`WarcrySpeed`、`TotemPlacementSpeed` 有各自读取者，无对应技能时台账部分不生效属于正常条件限制。

`calc::player_debuffs` 用 `SelfShockEffect` / `SelfChillEffect` 缩放明确配置的自身感电／冰缓。`shocked_effect_on_self_+%` 与 `chill_effectiveness_on_self_+%` 来自游戏 stat，Stormwalker 等节点提供 40% reduced。感电只缩放配置承伤：现有导入值 15% 经减少后为 9%（新规则 `absent`）。冰缓同时要求 Chilled 状态和明确 `conditionChilledEffect` / `ChillVal`；50% 基础经 40% reduced 为 30%，再受减速效力影响。不猜来袭伤害或默认幅度，效果降低也不能反向变成增益。

`CurseEffectOnSelf`、六种 `Self<Ailment>Duration`、`CorruptedBloodImmunity`、`MaimImmunity`、`HinderImmunity` 都是自身修正视图。时长合计专属、全部异常，以及点燃／冰缓／冻结／感电的元素异常修正；Cool Headed、Warm Blooded、Strong Hearted 的 50% less 走同一计算。

`DebuffExpirationModifier = 10000 / (100 + expiration rate)`，单位为百分比（`debuff_time_passed_+%`），再乘到自身时长。`ExposureEffectOnSelf`、`WitherEffectOnSelf`、`SelfBleedEffect`、`SelfIgniteEffect` 读取 `exposure_effect_on_you_+%`、`withered_effect_on_self_+%`、`base_bleeding_magnitude_+%_on_self`、`ignite_effect_on_self_+%`。其余来源包括 `curse_effect_on_self_+%`、`self_bleed_duration_+%`、`base_all_ailment_duration_on_self_+%` 和 `immune_to_*`。

这些百分比／免疫视图没有一并实现来袭诅咒、异常、腐化之血、Maim 或 Hinder；敌人诅咒另读敌人数据库。通用 `PhysicalDegen` 没有异常身份或时长，因此不能被流血时长／免疫词条修改。
