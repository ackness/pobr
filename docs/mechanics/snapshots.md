# 机制约定：配置输入与事件快照

配置项与敌人配置，以及按显式快照计算的事件：祖灵增幅、Archon、地面与护符事件、箭袋、残骸、Volatility、Combo。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 配置项与敌人配置

配置项进入计算的方式各不相同，有 handler id 不等于选项已生效。破甲（`conditionEnemyArmourBroken` 与破甲量 `multiplierArmourBreak`）由 `calc::armour_break::apply_armour_break` 在有效 DPS 口径下从敌人护甲中扣掉所填的量，没填时全部扣掉，“You can Break enemy Armour to below 0” 时下限为原护甲的相反数；没勾选时数值框里的残留不生效。致盲与 Elusive 的效果覆盖（`overrideBuffBlinded` / `overrideBuffElusive`）按百分数读取，不超过角色能施加的致盲、能得到的 Elusive 效果（`calc::blind`、`rules::buff_expander`）。Inexorable Critical 按配置的距上次暴击秒数（`SecondsSinceInevitableCrit`，不到 4 秒时另置 `CritRecently`）乘每秒的加成，受 `support_inevitable_criticals_*_cap` 限制（`calc::crit::apply_inexorable_critical`）。Dark Pact 的目标骷髅生命（`darkPactSkeletonLife`）乘 `skeletal_chains_aoe_%_health_dealt_as_chaos_damage` 为基础混沌伤害，Herald of Ash 的过量伤害（`hoaOverkill`）乘每分钟的燃烧比例 `herald_of_ash_burning_%_overkill_damage_per_minute` 为火焰持续伤害（`calc::configured_damage`）。

敌人受击配置（`calc_orchestrator::enemy_config::modifiers`）以 OVERRIDE 覆盖等级与敌人类别的预设：五种伤害（`enemyPhysicalDamage` 等）、三种元素穿透、暴击率、暴击伤害和物理压制（`enemyPhysicalOverwhelm`）；显式的 0 同样覆盖，负数按 0，非有限值忽略；来源标为 `EnemyConfig`、id `config.<输入名>`，被覆盖的预设行在 TotalEHP 与最大承伤的敌人输入（`CalculationSession::enemy_hit_mods`）里保留但不计入。`enemyDamageType` 选择命中类别（平均、无类型、近战、投射物、法术、法术投射物；持续伤害不是命中），决定闪避、格挡与承伤的上下文（`calc::taken::HitCategory`）；`EHPUnluckyWorstOf` 按 1 / 2 / 4 次中最差的结果估计防御概率。`enemySpeed` 只在 Energy 格挡事件缺少来袭频率时提供回退，不是 EHP 的通用伤害倍率；`enemyDamageRollRange` 和持续伤害的防御视图还没有读取方。敌人等级只解析一次（`calc_orchestrator::skill::mods::resolved_enemy_level`），尸体生命与敌人的减伤共用：调用方显式给的等级优先，其次构筑码 `enemyLevel` 的 Input、Placeholder；都没有时普通怪物和普通首领跟随角色等级，Pinnacle / Uber 取该档的默认等级（82），最后限制在该档的最低等级与敌人最高等级（85）之间。尸体爆炸按这一等级取怪物生命表（82 级 32956，85 级 36012），例如 Detonate Dead 的 `corpse_explosion_monster_life_permillage_physical` = 107 是 10.7%。

## 祖灵增幅

Ancestral Boost snapshots (`calc/session/preparation::bridge_ancestral_boost`) resolve one shared state after enabled buffs are assembled. The [game keyword](https://poe2db.tw/us/Keywords) gives a boosted Strike two additional targets and a boosted Slam 30% more damage and 25% increased area. The active native `GrantedEffectStatSets` supply Ancestral Call I/II/III's `ancestral_call_spirit_strike_interval_ms` (5000/3000/1000 ms) and Fist of War I/II/III's `ancestral_slam_interval_duration` (6000/4000/14000 ms, also milliseconds); no rank interval comes from vendor constants. `double_ancestral_boost_effect` doubles the bonus once (60% more Slam damage, 50% increased area), not the whole attack or another copy of its support modifiers. [Call III](https://poe2db.tw/us/Ancestral_Call_III)'s `support_ancestral_call_damage_+%_final` is an unconditional 30% less damage on the supported skill, including unboosted uses.

The default game calculation is an unboosted attack. `conditionAncestrallyBoosted=true` (or the existing `warcryMode=MAX`) selects an available boosted use **only with a source**. Alternatively, `multiplierAncestralBoostElapsedSeconds` supplies elapsed seconds since that support last boosted this skill: before its native interval the attack is unboosted, at/after it the next use is boosted. A supplied timer takes precedence over the requested maximum; negative/non-finite values are not ready. Explicit `conditionAncestrallyBoosted=false` selects an unboosted attack even with an active Cry. These inputs are intentionally raw API controls: XML `<Input name="conditionAncestrallyBoosted" boolean="true"/>`, or JSON `config_inputs: {"conditionAncestrallyBoosted": true, "multiplierAncestralBoostElapsedSeconds": 5}` (and `"conditionAncestralCryActive": true` for an active Cry). `config_resolve` makes raw JSON edits authoritative over imported XML state. Native callers may instead set `Build.config.conditions["AncestrallyBoosted"]` / `["AncestralCryActive"]` and `Build.config.multipliers["AncestralBoostElapsedSeconds"]` when no raw override is present. Reads are tracked for config diagnostics, but these controls have no dedicated Web/catalog entries or new display labels yet; the existing Max Hit control remains available. They describe one attack, never a repeated-use frequency or an average DPS rotation. No timer, first-use readiness, automatic uptime, cooldown-recovery scaling, or rate of boost consumption is inferred. The existing `AVERAGE` selection therefore leaves this snapshot unboosted; it is not a claim that an averaged rotation has been solved.

[Ancestral Cry](https://poe2db.tw/us/Ancestral_Cry)'s enabled buff additionally needs `conditionAncestralCryActive=true`, meaning the player declares that the buff is currently active. Its native `ancestral_cry_empowered_attacks_strike_X_additional_enemies` gives the same two-target boost; the skill text applies the boost to both melee Strikes and Slams. Call/Fist and Cry share one boost, not independent damage/target multipliers. `ancestral_cry_non_skill_physical_to_fire_convert_%` converts 100% of physical damage on these boosted attacks through the non-skill conversion channel. Cry's additional quality effect (enabled by Advanced Thaumaturgy) grants its global fire-damage bonus with the same explicit buff state; ordinary quality only extends duration per consumed charge. Triggered attacks (including Volcanic Steps/Eruption), totem/proxy attacks and unrelated skills never acquire this player-use boost. Cry's native eight-second duration, 50% more duration per removable Endurance Charge, Glory generation, charge consumption and volcanic trigger frequency do not imply uptime; expiry and rotations are not simulated. There is no in-game timing test. `DataOrchestratorOptions::ancestral_boost_snapshots=false`, selected by `on_reference_basis`, preserves the previous supported-Strike snapshot; new Cry and Call III rules are exporter-absent, and the new Slam bonus only applies to game snapshots. Reference compatibility is tested separately from the game behavior.

## Archon

Archon shared bonuses use the explicit `conditionAffectedByArchon` configuration
(default off; possessing a source does not establish uptime). The game's
[Archon keyword](https://poe2db.tw/us/Archon_Buff) defines a 10-second duration
and a 20-second recovery period; `ArchonDuration` and `ArchonRecoveryPeriod`
report these nominal timings after `archon_duration_+%`,
`archon_delay_expires_x%_faster` and `archon_no_recovery_period` (Dominion).
Invigorating Archon's movement and elemental-resistance grants are scaled by
Archon effect plus buff effect on self; Energising Archon's conditional recharge
start bonus is not part of the buff and is not scaled. Rules are exporter-absent.
The Command-use gain chance is recognized but does not imply an active buff;
proc timing, uptime, and intrinsic payloads of individual Archon types are not
modelled by this shared-bonus state. Nature's Archon needs direct BuffTemplates
verification before its intrinsic damage is implemented: the keyword says 25%
more damage, while the displayed `ArchonNature` template uses
`plant_skill_damage_+% = 25` (increased).

## 地面与护符事件

Olroth's Resolve “Gain Guard equal to Current Runic Ward for 10 seconds when
Effect ends” (`local_unique_flask_gain_%_of_current_ward_as_guard_when_flask_effect_ends`,
[game text](https://poe2db.tw/us/Olroths_Resolve)) retains local event metadata
for equipped flasks, including inactive slots. Config `olrothGuard` supplies
Guard still present from an earlier use, after any intervening damage, within
the ten-second lifetime; unset/zero grants none. This is an explicit snapshot,
not an estimate from current/maximum Ward or flask uptime. Reusing the flask
does not by itself remove existing Guard. The strongest Guard grant wins
(including charm Guard), capped at 200% maximum Life, and feeds EHP/max-hit
pools without changing Ward. Flask effect, quality and flask duration do not
scale the end-event metadata. `Flask<N>GuardAfterEffect` / `GuardDuration` show
the input capped for that slot and the nominal lifetime, with reconstructible
details and no invented PoB reference keys. The rule is exporter-absent; there
is no in-game timing test.

Beira's Anguish (official TW 貝伊拉的悲痛) creates Ignited Ground (點燃地面)
for 4 seconds, igniting as though dealing Fire damage equal to 500% maximum
Life (`UniqueCharmIgniteEnemiesInPresence1`, current game Mods). Current
[game Keywords](https://poe2db.tw/us/Keywords) give a default ground radius of
2 metres and require non-hit ground to build Flammability (易燃) to 50% before
Igniting; the [current item text](https://poe2db.tw/us/Beiras_Anguish) replaced
its pre-0.3 immediate nearby Ignite. Per-charm `IgnitedGroundDuration`,
`IgnitedGroundRadiusMetres`, `IgnitedGroundSourceDamage`,
`IgnitedGroundIgniteDPS` and `IgnitedGroundIgniteDuration` report potential
from currently equipped sources, even inactive charms. Current maximum Life
is the creation snapshot, including reserved Life. This is independent of the
selected skill's hit, crit, attack/spell modifiers and local supports. Shared
global Ignite magnitude, rate, duration and enemy resistance/taken apply;
hit-only penetration does not. Charm quality/effect/buff duration do not scale
these on-use event properties; no in-game timing test has been performed.

`conditionBeiraIgniteActive` explicitly declares an already established,
currently active ground Ignite in combat; merely equipping/enabling the charm
never grants DPS. `BeiraIgniteDPS` is the strongest such charm source.
`CombinedIgniteDPS` and total DoT use the strongest of that and the selected
skill's ordinary Ignite, never their sum. Optional Full DPS includes that charm
source once in the shared Ignite limit, independent of included skill counts.
When an included/selected skill supports
multiple simultaneous Ignites, cross-source stacking is unverified: the Beira
value remains separate and is not added to total damage. No use frequency,
buildup time, location, expiry or uptime is inferred. The item rule is
exporter-absent; reference basis excludes the new source. Calcs labels and
configuration help are application copy using the verified official TW terms;
CN copy is contextual, using the pinned `i18n/zh-CN/words.json` item name
`贝拉的苦楚` and the matching `stat_lines.json` term
燃烧地面; these dictionaries combine trade and PoE2DB sources and are not
claimed to be a retained official CN KeywordPopups table. Headhunter remains
unchanged at the owner's request.

Consecrated Ground (`calc::consecrated_ground`) uses the game
[ConsecratedGround keyword](https://poe2db.tw/us/Keywords): an actor explicitly
standing on it regenerates 5% of its current maximum Life per second and has
50% reduced Curse effect (`life_regeneration_rate_per_minute_%`,
`curse_effect_on_self_+%`, buff `ground_consecration`). Config
`conditionOnConsecratedGround` and native `OnConsecratedGround` /
`Condition:OnConsecratedGround` share the combat snapshot; duplicate grants
apply once. Minions require `conditionMinionsOnConsecratedGround` or their
own condition flag, never the player's position. Sanguis Heroum's
`UniqueCharmCreateConsecratedGround1` in the active game's `base/mods.json`
and [item text](https://poe2db.tw/us/Sanguis_Heroum) supply
`charm_create_consecrated_ground_when_used_duration_ms = 4000` and
`charm_create_consecrated_ground_when_used_radius = 14` (1.4 metres). The local
`Charm<N>ConsecratedGroundDuration` / `ConsecratedGroundRadiusMetres` views
retain these per-use properties on inactive equipped charms too; they neither
establish that ground exists nor place any actor on it. Charm effect, quality
and charm duration do not scale this separate ground event. This source rule
is exporter-absent; the base ground benefit is implemented by PoB2 too.
Ground ownership and an ally's ground effect are not inputs, so
`ConsecratedGroundEffect` is deliberately not consumed: the user's own-ground
modifiers must not scale an ally's ground. No use frequency, uptime, lingering
effect or in-game timing measurement is assumed. TW labels use the game's
[奉獻地面 keyword](https://poe2db.tw/tw/Keywords) and
[嗜血英雄／止血護符 text](https://poe2db.tw/tw/Sanguis_Heroum); surrounding UI
phrasing is application copy, not an official translation. Retained official
tables provide English and TW only; the new CN labels/help are contextual
application wording, not an official CN translation.

## 箭袋、残骸、Volatility 与 Combo

Cadiro's Gambit (`calc::cadiro_arrows`) uses the retained active unique,
`UniquePerandusArrows1` and stat `each_arrow_fired_gains_random_perdandus_prefix`.
The game's linked keywords define [Crescendo](https://poe2db.tw/us/Crescendo_Arrows)
as 6 additional chains and 60% increased damage per prior chain,
[Splinter](https://poe2db.tw/us/Splinter_Arrows) as splitting to 6 targets,
[Reversing](https://poe2db.tw/us/Reversing_Arrows) as piercing all targets and
returning, [Diamond](https://poe2db.tw/us/Diamond_Arrows) as guaranteed critical
hits with 60% increased critical damage bonus,
[Covetous](https://poe2db.tw/us/Covetous_Arrows) as 600% increased rarity on its
kills, and [Blunt](https://poe2db.tw/us/Blunt_Arrows) as 600% increased stun buildup.
The single `cadiroArrow` selection maps to native multiplier `CadiroArrow`:
0/unset is neutral, and 1–6 follow that order. Explicit zero/false/invalid input
retires an imported selection. Only the selected attack's `skill_can_fire_arrows`
stat and Bow weapon flags qualify; spells, crossbows, minions, proxy skills and
non-arrow secondary stat sets do not inherit the effect. Combat mode is required.
Crescendo reuses `skillChainCount` (native `Chain`) for the hit's prior chains,
capped by available chains, defaulting to the initial hit. This is increased
damage, not MORE damage or additional single-target hits. Diamond preserves
the first accuracy check, bypasses the critical evasion reroll, and yields to
`NeverCrit`. The source's fixed effects do not stack from duplicate grants.
`CadiroArrowReturn` and `CadiroArrowKillRarity` retain source details and have no
invented PoB keys; return geometry/timing and loot outcomes remain unmodelled.
No equal probabilities, average DPS, all-six simultaneous effects, or extra
return/chain hits are assumed. The new source rule is exporter-absent and
excluded on the reference basis; the ordinary quiver attack speed is unchanged.
TW and CN arrow names use their respective game KeywordPopups
([TW](https://poe2db.tw/tw/Crescendo_Arrows),
[CN](https://poe2db.tw/cn/Crescendo_Arrows), and corresponding five linked names);
these differ from the abbreviations in the item line. Surrounding control/help
wording is application copy. No in-game timing or geometry test is claimed.

Remnant pickup views (`calc::remnant_pickup`) keep the collector's Arcane
Remnants recovery separate from the creator's Empowering Remnants effect grants.
The active game's `base/passive_tree.json` nodes 46124 / 40985 and `base/stats.json`
provide `remnant_recover_%_mana_on_pickup` (3% maximum Mana per collection) and
`created_remnants_have_%_chance_to_duplicate_pick_up_results` (15% chance for
**Remnants you create** to grant their effects twice; also visible in the
[current passive text](https://poe2db.tw/us/Empowering_Remnants)). The older generic
`stat_descriptions` overlay says a pickup counts as another pickup, but that is
not the current passive wording and does not establish that the collector's
separate recovery triggers twice. Accordingly Arcane recovery is not multiplied
by duplication chance or Remnant effect. Instant on-pickup recovery is not Mana
regeneration and is not scaled by recovery-rate modifiers.

`remnantPickupsPerSecond` / native `RemnantPickupsPerSecond` counts the player's
collections from any creator. `createdRemnantPickupsPerSecond` /
`CreatedRemnantPickupsPerSecond` counts collections of the player's own created
Remnants, including collections by another recipient; another creator's chance
is unknown and is never substituted. `remnantPickupMissingMana` /
`RemnantPickupMissingMana` is an optional deficit immediately before each pickup:
recovery retains the maximum-Mana percentage basis, but cannot exceed that
deficit or unreserved Mana capacity. Full Mana or an explicit zero deficit gives
zero recovery. `CannotGainMana` also suppresses recovery; `NoManaRegen` does not
block this instant event. Without a deficit, displays explicitly say **potential** recovery;
none of these views is added to regeneration, sustain, or EHP. A supplied rate
assumes that same deficit before each event, not simulated spending between them.
Missing or zero rates do not infer collection from skill use. XML, JSON raw inputs
and native multipliers agree; explicit false, zero or invalid raw input retires
an imported value. Config reads are tracked even when absent. Combat is required
for event outputs; source percentage/chance properties remain inspectable.

Expected intrinsic effect-grant counts are `1 + clamp(chance, 0, 100) / 100` for
one collected own-created Remnant; this is event metadata, not buff magnitude,
duration, stacks, DPS, or a recursive pickup chain. Potential grants per use reuse
`effect_terms.remnants_per_use()` and explicitly require **all created Remnants
to be collected**; the view does not establish that any are collected. Products
saturate only at the largest finite representable number, recorded in details.
The two rules are exporter-absent and excluded on the reference basis. No in-game
timing or duplication interaction test is claimed. Retained official TW skill
names `mana_remnants` / `life_remnants` are 魔力痕跡 / 生命痕跡; the new labels and
help use 痕跡, with the remaining wording being contextual application copy.
The CN labels/help are contextual UI copy, not an official CN translation.

Volatility restoration (`calc::volatility_events`) uses active game stat
`volatility_refresh_%_chance`, Instability passive 43944 (25%), and
`maximum_volatility_allowed` (Controlled Chaos: 30). The game's
[Volatility keyword](https://poe2db.tw/us/Volatility#VolatilityRef) defines a
200-stack default maximum, a four-second explosion timer reset by each new
stack, and at most one gain per 0.1 seconds. The explosion consumes its stacks;
on a successful refresh the equivalent exploded amount is regained. The keyword
also describes 100 Physical self-damage per stack and ten seconds of Volatile
Power, but these are not simulated by this restoration view. The explicit
`volatilityExplosionQuantity` / native `VolatilityExplosionQuantity` snapshot
must be a finite positive whole number; it is capped by the final maximum,
which Controlled Chaos replaces rather than increases (also clarified in the
[0.3.0 patch notes](https://poe2db.tw/us/Version_0.3.0)). Expected restoration per
explosion is capped quantity times the summed refresh chance, clamped to 0–100%.
The optional `volatilityExplosionsPerSecond` / native
`VolatilityExplosionsPerSecond` is a supplied rate of eligible explosions of
that same quantity. Without it, the per-second view is zero; invalid rates and
overflow are rejected. No rate is inferred from attack speed or the resetting
timer, and restored stacks never feed back into an infinite-chain calculation,
even at 100% chance. No current stack count, damage, defence, Volatile Power,
expiry, or average uptime is changed. Without a source, all views are zero. With a player source, chance and cap
remain potential metadata even with absent/zero/invalid quantity or in noncombat
mode; all event outcomes remain zero. Minion actors are excluded. Explicit raw false,
zero or invalid values retire imported native settings; absent inputs preserve
them, and both controls are tracked even when absent. Both new source rules are
exporter-absent and excluded on the reference basis. Fields have retained
formula inputs, source groups, and no invented PoB keys. Retained game names
[易爆](https://poe2db.tw/tw/Volatility) / [无常](https://poe2db.tw/cn/Volatility)
supply the TW/CN term; complete labels and help are contextual application copy.
No in-game timing or gain-cooldown interaction test is claimed; callers supply
eligible events rather than assuming refresh bypasses any gain restrictions.

`calc::combo` reports `ComboMaximum` and `ComboGainPerEligibleEvent` for the selected
receiving skill. The current [Combo keyword](https://poe2db.tw/us/Combo) describes a
counter on some skills, built by striking enemies with the same bound weapon;
weapon swaps lose it. [Tempest Bell](https://poe2db.tw/us/Tempest_Bell) and
[Mantra of Destruction](https://poe2db.tw/us/Mantra_of_Destruction) explicitly build
it with other skills. Current `base/granted_effect_stat_sets.json` gives required
amounts 4 and 8; Combo Finisher I/II require 3/5, and Culmination I/II and Vruun's
Inevitability give an explicit maximum of 10. Required amounts serve as capacity
only without an explicit maximum; they are never added to that separate cap.
Distinct requirements use the greatest threshold, not their sum. Signed quality
changes first adjust the active skill's own requirement (for example Mantra);
Finisher requirements and explicit maxima each use their greatest contribution.
Supported receivers come through the existing support eligibility checks.

This is one successful eligible event **starting from an empty counter**, with
only global gain modifiers, not the receiving skill's own hit/cast rate.
`additional_combo_gain_on_hit` ([Martial Adept, passive 41751](https://poe2db.tw/us/Martial_Adept))
adds its integer count once to that event. `additional_combo_gain_chance_%` says
“chance to build an additional Combo”: its contributions add to one probability
bounded to 0–100%, adding exactly one Combo on success. Therefore with fixed bonus
`a`, chance `p` as a fraction and capacity `c`, expected gain is
`(1-p) × min(c, 1+a) + p × min(c, 2+a)`. Count inputs are nonnegative integers.
This additive, nonrecursive interpretation follows the wording and the shared
stat IDs; the combined interaction has **not been measured in game**. There is
no overflow mechanic in those descriptions. No held counter, decay, uptime,
event frequency, automatic spending, recovery or DPS bonus is inferred.
Practiced Combo modifies the generating strike, not the receiver: its support
bonus is not borrowed from a different skill or included in this global-only view.

The Finality keyword forbids Combo gain. `conditionFinality` / native `Finality`
is an explicit current snapshot; false/off retires an imported true state, and
an explicit native condition overrides a `Condition:Finality` modifier flag.
Absent a receiver, outside combat, during Finality, or for unsupported proxy /
minion receivers, generation is zero; inactive paths do not read global gain
modifiers. The checkbox only models this gain restriction, not Finality's other
bonuses. New generation rules are exporter-absent; reference mode preserves the
existing Culmination selected-stack multiplier but excludes these generation
views. Calcs formulas retain their inputs and have no invented PoB output keys.
`連擊` and `終極` are the corresponding current game TW keyword terms
([TW keyword](https://poe2db.tw/tw/Combo)); full labels/help are contextual UI copy.
CN uses `连击` and `绝杀` from the corresponding retained
`i18n/zh-CN/stat_lines.json` gain-Combo and gain-Finality entries.
