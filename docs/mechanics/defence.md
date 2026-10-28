# 机制约定：防御、资源与恢复

资源池与转换、抗性与减伤、恢复与偷取、充能事件、自身减益与减速。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 资源池、符文护盾与转换

Runic Ward absorbs damage after Life reaches 1. Life-loss prevention (Grasping Wounds, `life_loss_prevented_%`) applies only to the Life share, not damage absorbed by Ward. In `pool_damage::reduce_pools`, spend the Ward share before the Life-only overkill cap: with 1000 Life, 500 Ward, 25% prevention and a 1500-damage hit, 1332 damage brings Life to 1 (333 deferred), and 168 consumes Ward, leaving 332. Previously, PoBR and pinned PoB2 discarded damage as overkill before spending Ward; at 1 Life a large hit could consume only 1 Ward. Frozen Blood Mage ranks 01/02/04/05/07/08 all combine Ward and 25% prevention, so agreement with PoB2 did not establish correctness. Source: Runic Ward game text (damage taken from Ward before Life while at 1 Life), Grasping Wounds node 56162. The interaction is tested arithmetically, **not in game**; deferred-loss timing remains governed by the existing simulator assumptions.

物品的符文护盾（Runic Ward）是基底的值加上物品自身词条（隐式与显式）里的 “+N to maximum Runic Ward”（游戏 stat `local_ward`，只出现在暗金上，例如 The Brass Dome），再乘物品自己的 “N% increased Runic Ward”（`local_ward_+%`）和品质；镶嵌的合金和武器上的同一文本是 `base_maximum_ward`，加给角色（`item::defence::item_ward_modifiers`）。“Gain X% of maximum Life as Extra maximum Runic Ward”（`base_maximum_life_%_to_gain_as_maximum_ward`）按加成之前的生命乘比例，再按符文护盾的加成缩放，与其他 “Gain … as Extra …” 的资源转换相同（`calc::defence`）；这一口径与 PoB2 相同，**没有游戏内实测**。从生命、魔力转换或获得的量取最近的整数；各件装备的防御按栏位名的顺序累加，同一构筑的总数不随物品的读入顺序在 .5 处变化。

The tuned Smith of Kitava Shield Wall (`fb03d7dce08e`) boots retain the imported
Soul Core of Tacati's explicit +11% Chaos Resistance, rather than substituting
the current socketable table's +13%. With the item's +53%, Chaos resistance is
64% and a 386 Chaos hit deals 138.96 (PoB2 substitutes 66% and takes 131.24).
With 2162 Runic Ward, including The Brass Dome's local +991 Ward, PoBR's
ChaosMaxHit is 8001 and TotalEHP is 35908.717951478; the older code references
are 2468 and 11096.445514375. This follows the imported-text and local-Ward
rules above, rather than the exporter version's result.

Widow's Reign (`reserve_life_instead_of_loss_from_damage_for_x_ms`, in the active
and retained `base/stats.json` / `overlay/stat_descriptions.json`) replaces
Life lost to damage with reservation. The current item has a 3000 ms release
window; `overlay/uniques.json` retains the old 5000 ms variant (the
[game item text](https://poe2db.tw/us/Widows_Reign) confirms the current value).
`pool_setup` reads `DamageInsteadReservesLife`; `PoolState.life_reserved_by_damage`
retains the amount reserved across hits, after ES, MoM and other absorbing
layers. It is separate from maximum Life, ordinary missing Life (for example
Life spent on costs), and configured skill/permanent reservation. Damage consumes
reservation capacity even with ordinary Life missing; it clamps current Life to
the remaining capacity instead of subtracting from it again. Fully consuming
that capacity is lethal, including the last point under CI; chaos immunity and
ES bypass retain their normal gates. `life_recovery_cap` / `recover_life` allow
ordinary missing Life to recover but never remove reservation or revive a dead
pool. The EHP loop applies that ceiling to recovery on block, including its
accelerated batches; damage remains eligible for recoup, whose recovery cannot
release reserved Life. Reservation efficiency still affects only skill
reservation, not reservation caused by damage.

`LifeRecoverable` is the fresh snapshot's configured unreserved Life; each
subsequent pool state supplies its reduced recovery ceiling. Skill reservations
remain enabled, and permanent reservation (for example Beidat's 25%) is never
available. PoB2 instead uses its entire cancellable share for Widow's initial
pool and permits ordinary recovery in its hit loop. We do not infer that the
player switches off skills: change the configured reservations explicitly to
model that choice. Neither the static snapshot nor the consecutive-hit loop
infers a damage-free 3/5-second window, cancels reservation automatically, or
simulates a release/recovery event. Callers can provide an explicit current
`PoolState.life_reserved_by_damage` and current `life`; these are engine state,
not new UI controls. The DoT pool remains Life/MoM/ES/Ward without hit-only
Guard or loss prevention; there is no elapsed-time DoT/recharge simulation.
Recovery panels remain potential recovery rates for ordinary missing resources,
not a claim that regeneration/leech offsets damage reservation. No game-client
expiry, rounding or deferred-Life-loss interaction has been measured; the
reservation state uses the existing continuous pool precision and loss-prevention
ordering. No new comparison key or display label is introduced.

Direct total Energy Shield, such as Discipline, converts into a direct Life or
Mana total after the destination's increases/multipliers. It is not a new base
pool and is not fed back into the defence conversion matrix. Converted totals
retain their source attribution. Damage sharing is bounded to the incoming
damage and the companion's finite pool. Life prevention consumes an exactly
exhausted boundary as dead, rather than retaining a floating-point residue;
legitimate positive sublethal Life is preserved. Incoming critical damage uses
the incoming Hit context, independently of the player's selected skill.

## 抗性、减伤与门槛

Smith of Kitava 的 “Modifiers to Fire Resistance also grant Cold and Lightning Resistance at 50% of their value” 与 “Modifiers to Maximum Fire Resistance also grant Maximum Cold and Lightning Resistance”（`modifiers_to_fire_resistance_also_apply_to_cold_lightning_resistance_at_%_value` / `modifiers_to_maximum_fire_resistance_apply_to_maximum_cold_and_lightning_resistance`）把火抗的每条词条按比例、最大火抗的每条词条按全值加给冰抗和电抗；元素抗性惩罚不是火抗的词条，不计入；“all Elemental Resistances” 已经给冰电抗，不再转换其展开后的火抗分量。先合计可转换的词条并乘比例，保留半点等小数，到最终总抗性与最大抗性时才向零截断，再计算超上限抗性（`calc::offence::resolve_resistance`）。Smith of Kitava 的 “Body Armour grants” 词条（Kitavan Engraving 的 `body_armour_grants_x_maximum_life_+%`、Leather Bindings 的 `body_armour_grants_x_life_regeneration_rate_per_minute_%`）只在装备普通稀有度胸甲时生效。同一类天赋（Tribute to Utula、Support Straps、Tantalum Alloy、Flowing Metal、Molten Symbol）给出的精魄、力量、晕眩门槛、抗性、最大火抗、护甲作用于元素伤害、物理命中改为火焰承受也受这一条件控制。咒符效果（`charm_effect_+%`）只放大启用的咒符。`temporary_minion_limit_+`（“Temporary Minion Skills have +# to Limit”）只提高带 `Duration` 类型的召唤技能的上限（`skill_env::minions`，`TemporaryMinionLimit`），不影响常驻召唤物。Sacrosanctum 的 `life_recoup_also_applies_to_energy_shield` 把生命吸纳的比例也加给能量护盾吸纳，两者各按自己的回复速度与时长；Igniferis 的 `life_recovery_from_regeneration_is_not_applied` 让生命再生不再回复生命，原始再生值仍供转移类机制读取（`calc::survivability`）。

Crystallised Immunities uses the game stats `immune_to_chill_if_majority_blue_supports_socketed`, `immune_to_ignite_if_majority_red_supports_socketed` and `immune_to_shock_if_majority_green_supports_socketed` (Crystallised Immunities, node 5332). Count socketed support gems across enabled groups using the official `SkillGems.GemColour` values: strength/red, dexterity/green and intelligence/blue. A majority means **strictly more than half of all counted supports**; empty sets, ties and a mere plurality grant no immunity. The existing `ChillImmune` / `IgniteImmune` / `ShockImmune` path gives 100% avoidance. PoB2 accepts a strict plurality instead; these rules carry `exporter: "differs"`. Sources: `base/stats.json`, `base/passive_tree.json` and `base/skill_gems.json`.

Strong Chin (current passive 10998,
`stun_threshold_+_from_lowest_of_base_helmet_evasion_rating_and_armour`) already
uses the generated parser's `StunThreshold` BASE modifier and the existing
`LowestOfArmourAndEvasionOnhelmet` snapshot. The retained game text says
“Evasion and Armour on your Helmet”: the minimum is taken after the helmet's
local flat/increased modifiers and quality, before global defence increases.
Known bases use current game base values; imported defence displays are only a
fallback for unknown bases. Missing either defence contributes zero. Ordinary
Stun Threshold increases/multipliers then apply to the added amount.

Projectile Bulwark (current passive 19236,
`defend_with_%_armour_against_ranged_attacks`) says “Defend with 120% of Armour
against Projectile Attacks”. It uses the configured incoming enemy
`enemyDamageType = Projectile` category, independently of the player's skill.
Melee, spell, spell-projectile, untyped and unspecified/Average hits receive no
conditional benefit. It changes effective applied Armour in the shared
mitigation snapshot used by damage taken, EHP, max-hit and existing details;
character Armour and borrowed Evasion/ES terms remain separate. The percentage
uses the existing strongest-`ArmourDefense` convention rather than multiplying
several “defend with” percentages; this stacking interaction has no in-game
measurement here. The current pinned exporter has no projectile-specific rule,
so that source is exporter-absent. No new condition, event or display is added.

Skin of the Loyal's four uncapped-resistance flags add the corresponding final
uncapped resistance as an increased Armour, Evasion, Energy Shield or Ailment
Threshold modifier. These are ordinary increases, not multipliers. They read
the buff/charge resource snapshot before defence conversion, avoiding a
self-referential conversion loop. Companion-in-Presence defaults on only when
an enabled companion skill exists; explicit true/false remains a selected
condition. Hinder applied by an active curse also supplies EnemyHindered.
Enemy modifiers gated by an applied mark or curse are forwarded again after
curse-slot selection. This enables Marked for Sickness only when a mark takes
effect, without duplicating modifiers already active before the buff pass.

## 恢复、偷取、吸纳与充能护盾

“Physical Damage prevented Recouped as Life” (`physical_damage_prevented_recouped_as_life_%`, [Made to Last](https://poe2db.tw/us/Made_to_Last) 5%, Wandering Reliquary 50%) uses an explicit convention: prevented damage is the Physical damage entering the physical reduction stage minus that leaving it, counting Armour and additional physical damage reduction after the cap and overwhelm. This follows [Iron Ward's game text](https://poe2db.tw/us/Iron_Ward), which describes damage prevented by Armour or other sources of damage reduction; it is an assumption, **没有游戏内实测 / no in-game test**. Damage shifted to another type before reduction and later damage-taken multipliers do not count; blocked and deflected hits contribute nothing, including partially damaging blocks. Evaded hits never enter the landed-hit loop. The [December 2024 bug report](https://www.pathofexile.com/forum/view-thread/3661840) and [November 2025 discussion](https://www.pathofexile.com/forum/view-thread/3882155) are verification leads, not a developer ruling on this convention. The mitigated EHP loop accumulates prevented damage alongside ordinary damage-taken recoup; `calc::survivability::life_recoup_terms` combines both using Life's 8-second duration (4 seconds with either Life/global flag) and LifeRecoveryRate, and also supplies the `LifeRecoupRate` detail. The parser rule is `exporter: "absent"`, so reference-basis calculations omit it. The Chaos counterpart (Forsaken Bangle, in [flasks and charms](items.md#药剂与护符)) reuses this definition with Chaos resistance in place of Armour, and the Mana side uses `mana_recoup_terms`.

恢复（`pobr-core/src/calc/recovery.rs`）分三层：原始再生；经过再生限制与直接恢复后的恢复（`LifeRegenRecovery` 等，含 Runic Ward 与怒气，来自 `base_ward_regeneration_per_minute` “Regenerate 1 Runic Ward per second”、`base_rage_regeneration_per_minute` 等游戏 stat）；扣掉构筑自身损耗后的净恢复（`NetLifeRegen`、`NetManaRegen`、`NetEnergyShieldRegen`、`NetWardRegen`、`TotalNetRegen`）。角色固有的魔力再生是 `character_inherent_mana_regeneration_rate_per_minute_%` = 240（每秒 4% 最大魔力），“No inherent Mana Regeneration” 只去掉这一份（`calc_orchestrator::inject::inject_character_base`）。自身损耗是资源流失（“Lose … per second”，不经减免）、自己受到的类型化持续伤害（经抗性与承伤修正）和主技能的每秒消耗（百分比消耗按资源池换算）：生命、魔力、能量护盾各扣自己的消耗，`NetWardRegen` 是 Runic Ward 的恢复减去它的每秒消耗，`TotalNetRegen` 是这四种净恢复之和，怒气消耗只进入 `TotalBuildDegen`；敌人的持续伤害不计入。Berserk 的 `skill_base_life_loss_%_per_minute_per_rage_while_not_losing_rage_to_apply` = 6 是每点怒气每秒流失 0.1% 最大生命。偷取与命中获得（`LifeLeechGainRate`、`LifeLeechGainPerHit` 等）按[游戏关键词](https://poe2db.tw/us/Keywords)使用一秒基础持续时间，不再把每次偷取量限制为资源池的 10%。[0.5 补丁说明](https://www.pathofexile.com/forum/view-thread/3932540)中的单次命中 40,000 伤害上限已实现：命中总伤害超过上限时，各伤害类型按相同比例缩小，再按各自的偷取比例计算，不能用额外元素伤害补足物理偷取。`base_life_leech_amount_+%` / `base_mana_leech_amount_+%` 的“increased amount of Life Leeched” / “increased amount of Mana Leeched”增加偷取量；`base_life_leech_rate_+%` 等“Leech Life 1% faster”加快速率，不增加总量。最大偷取速率先应用增加/减少与更多/更少修正，再限制实例速率；生命和魔力每秒 20%、能量护盾每秒 10% 的默认上限仍是项目保留的兼容约定，并非 0.5 补丁说明确认的游戏常量。按主技能平均命中频率估计实例覆盖时间，优先计算恢复速率最高的实例，较慢实例只填补其过期后留下的时间，不模拟随机暴击的时间序列；命中获得与瞬间偷取仍按每次命中相加。PoB2 在没有自身类型化持续伤害时不给出净恢复字段，PoBR 照常计算。

Equipped Focus/Shield defence increases (`energy_shield_from_focus_+%`,
`shield_armour_evasion_energy_shield_+%`) add to global increases only for the
matching off-hand item's locally computed defences. The curated Focus rule is
marked `exporter: differs`: the exporter applies it to any off-hand, whereas
the game text requires a Focus (`UsingFocus`). Recharge rate stats
(`energy_shield_recharge_rate_+%`, `virtual_energy_shield_recharge_rate_+%`)
read `EnergyShieldRecharge`, faster start (`energy_shield_delay_-%_while_affected_by_archon`)
reads `EnergyShieldRechargeFaster`. Thawing Charm's
`charm_energy_shield_recharge_starts_when_used` follows the enabled-charm
convention: an enabled, in-budget charm is in effect (`UsingCharm`), so
`EnergyShieldRechargeBase` is 0 and recharge starts immediately; charm
uptime is not simulated (exporter-absent).

Scarred Faith's “Energy Shield does not Recharge” (`keystone_alternate_es_recovery`,
[game text](https://poe2db.tw/us/Scarred_Faith)) sets recharge rate, amount and
start delay to zero; its separate regeneration prohibition makes net ES
regeneration zero when there are no other gains or drains. Recharge is not
included in the current EHP hit-pool simulation. Resonance (`keystone_charge_cycle`
and `gain_*_charges_instead_of_*`, [game text](https://poe2db.tw/us/Resonance))
redirects Power → Endurance → Frenzy → Power once. With no charge-gain event
model, configured counts above their minima stand in for incoming gains;
minima and maxima stay with their original type, incoming gains merge and
are capped by the destination maximum. On-gain proc effects, duration and
consumption timing are not simulated.

Mystic Harvest (`life_leech_also_recovers_based_on_elemental_damage_types`)
extends the damage funding an existing Life-leech percentage to elemental
damage; it grants no percentage and does not remove attack/spell restrictions.
Essentia's `convert_all_life_leech_to_energy_shield_leech` keeps that damage
eligibility while redirecting the base leech to ES. The game's
[Stat Conversion](https://poe2db.tw/us/Stat_Conversion) wording applies
destination modifiers: ES amount, speed, recovery, instant share and restrictions,
not Life modifiers. Life on hit is separate and remains Life recovery.
The panel and sustained recovery share typed damage eligibility and the existing
whole-hit damage ceiling. The panel remains a raw average-hit single-instance
view; it need not equal sustained, recovery-rate-scaled or instant recovery.

`LifeRegenAppliesToManaRegen` and `ManaRegenAppliesToRageRegen` share only
increases/reductions of the source regeneration rate. Source BASE, MORE and
recovery modifiers do not transfer. Detail groups retain the original sources
and conditions and include only those INC contributions.

Eldritch Will's `maximum_life_mana_and_energy_shield_+%` uses the resource
names consumed by the pool calculation: `MaximumLife`, `MaximumMana` and
`EnergyShield`. The shorter `Life` and `Mana` INC aliases are not pool inputs.
`MovementSpeedAppliesToEnergyShieldRecharge` similarly shares only MovementSpeed
INC into recharge rate, preserving source conditions and the same detail rows.
It does not transfer base speed, MORE factors, action speed or movement penalties,
and does not establish recharge uptime.

Oisin's `mana_leech_from_elemental_instead` uses the existing elemental Mana-leech
eligibility and excludes Physical damage from that resource's leech. It grants no
percentage, keeps source attack/spell restrictions, and preserves independently
eligible Chaos leech. The whole-hit ceiling still includes all damage before
eligibility weighting. Life/ES conversion and the panel's existing conventions
are unchanged.

## 充能与资源事件

Charge durations (`PowerChargesDuration`, `FrenzyChargesDuration`,
`EnduranceChargesDuration`) display seconds: the [game's Charges keyword
text](https://poe2db.tw/us/Charges) gives a 15-second base, multiplied by
`(1 + sum of generic and kind-specific increased duration / 100)` and the
product of generic and kind-specific more/less duration modifiers, clamped at
zero. Fractional seconds are retained because the game text specifies no
whole-second rounding; unlike the oracle's whole-second floor, 3% reduced
duration displays 14.55 seconds rather than 14. The in-game tick rounding
boundary has not been measured. `charge_duration_+%` uses the existing three-kind
parser rule; `base_charge_duration_+%` applies generically, and
`frenzy_charge_duration_+%_per_frenzy_charge` uses the resolved Frenzy Charge
count. Details share the calculation's terms. No charge uptime or expiry is
simulated, and duration changes no other output.

Conduit (`keystone_conduit`, passive 33979: “If you would gain a Charge,
Allies in your Presence gain that Charge instead”) uses the same snapshot
convention: configured counts above their minima represent incoming gains,
so the player's Power, Frenzy and Endurance counts stay at their minima,
including with Resonance. Minimum charges are treated as always held, not
gained, and remain with the player; this convention has no in-game test.
Allies receiving the charges and their effects remain unmodelled: there is
no ally/minion charge-gain model. The rule declares `exporter: "absent"`,
so reference-basis calculations omit it.

Event Horizon
(`lose_%_of_life_and_energy_shield_when_you_use_a_chaos_skill`, passive
`chaos33_`: 3%, [game text](https://poe2db.tw/us/Event_Horizon)) subtracts
3% of each final maximum pool per selected player-used Chaos skill use from
net Life/ES recovery and includes it in `TotalBuildDegen`. It uses the existing
cost-use frequency (one payment per repeat sequence), excludes triggered and
proxy actions, and is neither mitigated damage nor a cost affected by cost
modifiers. Other skills in a rotation and channel start/stop events are not
simulated. These charge and loss rules, and the previously unrecognized Scarred Faith
regeneration wording, declare `exporter: "absent"`.
Thaumaturgical Dynamism (`ascendancy_gemling_charge_generator_buff_display`)
reports a five-second generation interval and 0.2 generation events per second, from game
`BuffTemplates.AscendancyGemlingChargeGenerator`'s
`gain_random_charge_every_X_ms_from_total_socketed_gem_attribute_requirements = 5000`
([buff data](https://poe2db.tw/us/Thaumaturgical_Dynamism)). Random charge
selection is biased by socketed gem attribute requirements, but the [game keyword](https://poe2db.tw/us/Keywords) supplies no numeric probability formula.
`calc::charge_generation` reports the total event rate, directed to allies by Conduit;
multiple grants do not stack. It leaves configured charge counts unchanged and does
not simulate consumption, per-type probabilities, extra on-gain chains or uptime.
These event views have no invented PoB reference key.

## 自身减益、减速与玩家一侧视图

Player-side views `SelfFreezeEffect`, `SelfPoisonEffect` and `SelfBlindDuration` follow the other `Self*` views: nonnegative `(100 + INC) * MORE` percentages. Freeze effect reads `SelfFreezeEffect` (game `non_damaging_ailment_effect_+%_on_self`, also feeding Chill and Shock); Poison effect reads `SelfPoisonEffect` (`unaffected_by_damaging_ailments` gives 0). Blind has no self-duration game stat (`blind_duration_+%` is outgoing), so `SelfBlindDuration` is the 100% base times `DebuffExpirationModifier / 100`: Blind is a debuff. Incoming freeze, poison and blind remain unmodelled, so these views change no other output.

Generic player slows already represented by negative `ActionSpeed` modifiers now use one calculation for skill rates, warcry timing, proxy placement, channel timing and movement (`calc::skill_use_time::action_speed_factor`). The [game Slow keyword](https://poe2db.tw/us/Keywords) defines separate slows as multiplicative; each penalty is scaled by `SlowEffectOnSelf` (`base_slow_potency_+%`, for example Mail Coat’s 30% reduction), and `UnaffectedBySlows` (`slows_have_no_potency_on_you`, Wanderlust / Relentless Pursuit) removes those penalties while preserving positive speed bonuses. A 50% slow with 30% reduced potency is a 35% slow. The potency text rules are `exporter: "absent"`; reference mode omits them. Existing custom modifiers can supply an explicit action-speed slow; self Temporal Chains and movement-only debuffs remain unmodelled, and ordinary equipment / skill movement penalties are unchanged. `SkillSpeed`, `WarcrySpeed` and `TotemPlacementSpeed` already have separate consumers: a partial ledger row on a build without a warcry or totem is expected, not a missing speed bonus. Every negative `ActionSpeed` modifier counts as a slow. That holds because the only player sources today are debuffs: self Chill, Temporal Chains and Maddening Presence. The game defines Slows as "modifiers from Debuffs" and keeps non-debuff reductions under separate stats (`action_speed_-%`, `overencumbered_action_speed_+%_final`). A new non-debuff source of reduced Action Speed must therefore use a separate name, or slowing potency would scale it.

Player Shock and explicitly configured Chill magnitudes are scaled by
`SelfShockEffect` / `SelfChillEffect` (`calc::player_debuffs`), using game stats
`shocked_effect_on_self_+%` and `chill_effectiveness_on_self_+%` from
`base/stats.json`; the passive tree includes Stormwalker's 40% Shock reduction
and 40% Chill reduction nodes. Shock scales only the configured damage-taken
contribution (the existing imported config supplies 15%, so 40% reduction gives
9%); the new Shock reduction rule is exporter-absent. Chill requires both the
player's Chilled state and an explicit `conditionChilledEffect` / `ChillVal`:
50% Chill with 40% reduced effect is a 30% action-speed slow, then subject to
slowing potency. No incoming hit or default Chill magnitude is inferred.
Effect reductions cannot make a debuff beneficial. `CurseEffectOnSelf`, the six
`Self<Ailment>Duration` fields, and `CorruptedBloodImmunity`, `MaimImmunity` and
`HinderImmunity` display player-side modifier views. Curse effect and durations
are percentages of base: `(100 + INC) * MORE` (curse effect is floored at zero).
Durations combine the ailment-specific and all-ailment modifiers, plus
elemental-ailment modifiers for Ignite, Chill, Freeze and Shock. Cool Headed,
Warm Blooded and Strong Hearted supply their game-data 50% less duration through
the same views.
Immunity views reuse the existing `*Immune` modifiers, including “cannot be
inflicted” text. `DebuffExpirationModifier` is `10000 / (100 + expiration rate)`
percent (game `debuff_time_passed_+%`); each duration view is multiplied by
`DebuffExpirationModifier / 100`. `ExposureEffectOnSelf`, `WitherEffectOnSelf`,
`SelfBleedEffect` and `SelfIgniteEffect` likewise expose nonnegative
`(100 + INC) * MORE` views from
`exposure_effect_on_you_+%`, `withered_effect_on_self_+%`,
`base_bleeding_magnitude_+%_on_self` and `ignite_effect_on_self_+%`. The other
views use `curse_effect_on_self_+%`, `self_bleed_duration_+%`,
`base_all_ailment_duration_on_self_+%` and the three
`immune_to_*` stats. Incoming curses, ailments, Corrupted Blood, Maim and Hinder
remain unmodelled, so these views change no other output. Enemy curses read the
enemy database separately. Generic `PhysicalDegen` has neither ailment identity
nor duration, so duration and immunity lines cannot alter it.
