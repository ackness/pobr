# 机制约定：异常、晕眩与敌人减益

伤害型异常、冰缓／感电／冻结、晕眩与钉住、破甲与命中效果视图。索引与写法见[机制约定](README.md)，与 PoB2 的差异另见 [pob2-differences.md](../pob2-differences.md)。

## 伤害型异常

伤害型异常（流血、点燃、毒）的同时层数 = 命中率 × 施加几率 × 持续时间 × 每秒命中数，持续时间是基础时间乘持续时间加成（含 “increased duration of Ailments you inflict”，`EnemyAilmentDuration`）与敌人身上效果的过期速度，再除以 “Damaging Ailments deal damage #% faster”：伤害总量不变、造成得更快，每份结束得更早（`calc/perform.rs`，分解里写作 “faster /N”）。

Calcs additionally exposes `IgniteChancePerHit`, `BleedChance`, `PoisonChance`, `ShockChance`; each damaging ailment's `Duration`, `EffMult`, `MagnitudeEffect`, `RollAverage`, `StacksMax` and `StackPotentialPercent`; `Ignite{Fire,Cold,Lightning}{Min,Max}`, `BleedPhysical{Min,Max}` and `Poison{Physical,Chaos}{Min,Max}`. These read the retained ailment-stage inputs, without another damage calculation. Chances, roll percentiles and stack potential are percentages; durations are seconds; effect and mitigation values are multipliers. Source ranges are non-critical stored hit damage after the damage type's ailment buildup multiplier, before the high-roll selection. Ignite chance is measured before over-stacking; bleed/poison chance is the final weighted application chance. Global damaging-ailment views use the last applying hand, while bleed/poison chances average all hand passes (a hand that cannot apply contributes zero); DPS keeps its existing stronger/weaker-hand merge. An absent ailment reports zero, including its multipliers. Stack potential is uncapped and uses the existing full-stack fallback when no hit-rate estimate exists; duration includes enemy debuff expiration and division by faster damage. Derived details reuse those terms (and the shared enemy-mitigation terms) and must reconstruct each displayed value. `ShockChance` retains the existing non-critical-component source approximation. `FreezeBuildupPct` remains a percentage per damage point, not an alias for `FreezeBuildupAvg`.

Incision (`apply_X_incision_on_hit`, [Incision support](https://poe2db.tw/us/Incision): "Supported Skills inflict
Incision on Hit, which causes Enemies to be increasingly more likely to be inflicted with Bleeding. All Incision is
removed from a target on inflicting it with Bleeding") is read from the configured stack count
(`multiplierIncisionStackCount`, enemy `Multiplier:IncisionStack`, at most 10, effective mode only). Each stack adds
10% to the enemy's `SelfBleedChance`, the input every Bleed already reads (`calc::setup_env::apply_enemy_incision`),
so the chance is limited to 100% by the ordinary chance rules. It counts only while the character can inflict Incision
(`Condition:CanInflictIncision`), so a stale stack input left in an imported build does nothing once the source is
removed. "N% increased effect of Incision" (`incision_effect_+%`, `IncisionEffect`) scales that chance. The 10% per
stack and 10-stack maximum come from the Incision keyword text on poe2db and the config tooltip of the pinned
exporter, which agree; the game data holds no constant for either. The pinned PoB2 applies the same rule
(`CalcPerform.lua`: `10 * (1 + IncisionEffect INC / 100)` per stack, limit 10), so the rule needs no `exporter`
attribute; that the effect modifier scales the per-stack chance is therefore the exporter's reading, **not an in-game
measurement**. The Incision support's `apply_X_incision_on_hit` is claimed in `skill_stats` as
`Condition:CanInflictIncision`; the extracted map entry (a `GlobalEffect` flag) was rejected as an unknown modifier
name, so a build with only the support never counted its stacks. Thousand Cuts ("Enemies you apply Incision to take 2%
increased Physical Damage per Incision") and the "against Enemies affected by Incision" Bleeding magnitude lines
were already read from the same stack count (`MultiplierThreshold` on the enemy) and are not scaled by the effect
modifier. Stacks are removed by the Bleeding they cause; the configured count is the number present when the hit
lands, and no stack build-up, consumption timing or `incisionConsumedRecently` is inferred from the chance. No
worked build carries Incision, so this is covered by a build-level test only.

Crimson Assault replaces Bleeding's base duration with one second before the
existing duration INC/MORE and faster-damage modifiers. Instance damage, retained
terms and source views use the same resolved base. Shattering Spite's native
`active_skill_damaging_ailment_effect_+%_final` applies its -50% through damaging
ailment effect; it does not infer Bloodstone Lance counts or conditional leech.
The pinned exporter omits this stat, so it is disabled only for reference-basis
comparison. Neither change explains every remaining saved BleedDPS difference.

## 冰缓、感电与冻结

`ShockDuration` and `ChillDuration` (seconds) use the [game keyword](https://poe2db.tw/us/Keywords) base of 8 seconds on non-players (`GameMechanicsConstantsDef::shock_base_duration` / `chill_base_duration`), times `1 + increases / 100` and the more multipliers, then the enemy's debuff expiration (`calc::ailment::ailment_duration_terms`). The increases add the player's `Enemy<X>Duration`, `EnemyAilmentDuration` and `EnemyElementalAilmentDuration` (skill stats `shock_duration_+%`, `chill_duration_+%`, `lightning_ailment_duration_+%`, `cold_ailment_duration_+%`, `support_pure_shock_shock_duration_+%_final` now pass the stat map) to the target's own `Self<X>Duration`, `SelfAilmentDuration` and `SelfElementalAilmentDuration`. They are reported only when the selected hit can shock (non-zero shock chance) or chill (cold hit damage, no `CannotChill`). Shock and chill magnitude now multiply the attacker's `Enemy<X>Magnitude` / `AilmentMagnitude` factor by the target's `Self<X>Magnitude` / `AilmentMagnitude` factor (`calc::ailment::effect_mod_factors`, each floored at zero); `ChillEffectMod` and `ShockEffectMod` display that product (neutral 1). `FreezeBuildupAvg` is the crit-weighted, mitigated hit damage that builds Freeze (Cold, plus any type flagged `<Type>CanFreeze`, e.g. Pinnacle of Power) times `FreezeBuildupPct` times the target's `FreezeBuildup` / `ImmobilisationBuildup` multiplier, as a percentage of the Freeze threshold (`calc::ailment::freeze_buildup_terms`); it reuses Heavy Stun's mitigated per-hit damage and is zero when the skill does not hit or the player `CannotFreeze`. `active_skill_hit_damage_freeze_multiplier_+%_final` (more Freeze buildup) and Freezing Mark's curse payload `freezing_mark_hit_damage_freeze_multiplier_+%_final` (the target's `FreezeBuildup` MORE) now reach these terms. The boss poise threshold difference recorded for `HeavyStunBuildupAvg` (Xesht 17× against PoB2's 9.38×) applies unchanged, so PoB2's value is about 1.81× PoBR's on Pinnacle bosses. Worked comparisons beyond that factor: benchmark Deadeye Ice Shot `b6ce7b3a4329` is 9.035 vs 12.128 (0.745 = 0.552 × 1.35, Minion Pact II's 35% more damage with the Rhoa; the per-damage buildup, 35% increased × 25% more × Freezing Mark's target MORE, agrees). Tuned Acolyte of Chayula Decompose `7505238a9c8d` is 0 vs 1.079: the poison cloud deals no hit, and Chaotic Freeze's text gives Chaos damage Freeze buildup only "for Hits against targets taking damage over time from Supported Skills", while PoB2 builds Freeze from the cloud's Chaos damage. Tuned Tactician Oil Barrage `02b94461578f` is 0.440 vs 1.548 (0.284): 0.552 for poise, 0.671 for the Cold mitigation already registered for its DPS (Soul Core of Topotante's 15% penetration against PoB2's 25%, Pounce's Mark 0% against 32% taken), and a 0.769 residual traced to Tainted Strike's critical-hit ailment-magnitude line (investigation below). The Freeze support that gives 50% more sits in the Spear Stab group, not Oil Barrage's. Derived details give `BaseAilmentDuration`, `AilmentDurationIncreased`, `AilmentDurationMore`, `EnemyDebuffDuration`; `AttackerChillMagnitude`, `TargetChillMagnitude`; and `FreezeSourceHitAverage`, `FreezeBuildupPerDamage`, `EnemyFreezeBuildupMultiplier`, which reconstruct each value. `FreezeDuration` (seconds) uses the same terms with the Freeze keyword's base ("lasts 4 seconds by default", `freeze_base_duration`); `freeze_duration_+%`, `chill_and_freeze_duration_+%` and `cold_ailment_duration_+%` reach it as `EnemyFreezeDuration` INC, `active_skill_freeze_duration_+%_final` as its MORE, with the target's `SelfFreezeDuration`. It is reported only when the selected hit builds Freeze (`FreezeBuildupAvg` > 0); how long the buildup takes and refreeze timing are not modelled, and PoB2 has no corresponding output. The attacker magnitude factor is weighted by the hit's critical share c (its crit chance, 0 under `AilmentsAreNeverFromCrit`): `(1 - c) * factor + c * factor_with_CriticalStrike` (`calc::ailment::effect_mod_factors`), so "increased Magnitude of Shock / Chill you inflict with Critical Hits" (`shock_effect_+%_with_critical_strikes`, `chill_effect_+%_with_critical_strikes`) applies to the critical share of Shock and Chill magnitude and of `ShockEffectMod` / `ChillEffectMod`. Chill and Shock still use the existing non-critical / crit-averaged source damage; weighting the factor rather than computing a separate critical magnitude is an approximation with no in-game test.

Oil Barrage investigation (2026-10-17, tuned `02b94461578f`): the selected
`OilBarragePlayer` group is group 9; its supports are Nova Projectiles I, Salvo,
Ricochet II, Fork and Armour Break III. [Tainted Strike](https://poe2db.tw/us/Tainted_Strike)
(game passive `criticals79`, node `22864`; stat
`non_damaging_ailment_effect_+%_with_critical_strikes`) grants 30% increased
non-damaging ailment magnitude with critical hits. PoB2 tabulates it as
`EnemyFreezeBuildup` INC 30 under `CriticalStrike` and `WeaponSet2`; together
with Suppressing Fire (`44746`)'s 40% more Immobilisation buildup this gives
1.30 × 1.40 = 1.82. PoBR's existing parser maps the line to `FreezeBuildup`,
but its buildup query leaves the critical condition inactive, giving 1.40.
In a temporary copy of this worked build, adding only “30% reduced Magnitude
of Non-Damaging Ailments you inflict with Critical Hits” cancels that modifier:
PoB2's `FreezeBuildupAvg` falls from 1.547927978 to 1.190713830 (exactly 1.30×
within output precision), while its 100% crit chance, Cold hit damage and DPS
stay unchanged. PoBR stays at 0.440092450 in both copies.
**OPEN gameplay question:** does this magnitude stat modify Freeze buildup?
The [game keywords](https://poe2db.tw/us/Keywords) define magnitude as the stats
a buff/debuff applies and Freeze buildup as a contribution from Cold hits;
they do not establish that equivalence. The existing parser mapping is
exporter-derived, not independent game proof. This isolates the residual's
source but does not justify changing the engine or declaring the oracle wrong.
Game-client measurement or an explicit game rule remains required; no new
known-differences exemption is added, and the poise ([below](#晕眩钉住与-poise)) and Cold-mitigation conventions
remain unchanged.

Outgoing “Debuffs you inflict have N% increased Slow Magnitude” is
`base_debuff_slow_magnitude_+%` (active game `Mods` entry
`PassageUniqueAmanamuIncreasedDebuffSlowMagnitude`, range 12–20), distinct
from incoming `base_slow_potency_+%`. The game's
[Slow Magnitude Modifier](https://poe2db.tw/us/Slow_Magnitude_Modifier)
applies only to the slowing component of an inflicted debuff. The rule
currently feeds `EnemyChillMagnitude`, reusing the existing Chill magnitude
sum, trace and derived details: 30% Chill with 20% increased Chill magnitude
and 20% increased Slow magnitude becomes 42%, capped at the game's default
50% maximum ([Chill keyword](https://poe2db.tw/us/Chill)).
The additive Chill/Slow aggregation is an inference from both increases
modifying the same outgoing slowing magnitude, supported by the official
[0.2.0 patch notes](https://www.pathofexile.com/forum/view-thread/3740562)
explicitly making Curse Magnitude and Slow Magnitude additive for Temporal
Chains. Neither that example nor the retained `Stats` rows explicitly proves
the Chill/Slow pair; this pairing has not been validated by an in-game
measurement. Enemy slowing potency then scales that capped reduction
separately (50% less gives a 21% slow). This applies to the hit magnitude
estimate and explicit player
`ChillBase` / `ChillMinimum` sources. Enemy `ChillVal` and player
`ChillOverride` already specify final magnitudes and remain unchanged.
The line supplies no source, condition or uptime; cold damage alone still
does not write an enemy action-speed penalty. It changes neither incoming
slows, Shock/Freeze magnitude, nor debuff duration/expiration. Inactive
sources leave enemy modifiers and conditions untouched. The rule is
exporter-absent and omitted on the reference basis. **Remaining scope:**
[Temporal Chains](https://poe2db.tw/us/Temporal_Chains) has separate slow
and effect-expiration components; its enemy slow payload is not yet
consumed, and Hinder/Maim movement reductions have no enemy movement model.
These need their own consumers before this rule can cover their slowing
components. No in-game timing or uptime measurement is assumed.

## 晕眩、钉住与 poise

`HeavyStunBuildupAvg` reports the selected successful hit's average enemy Heavy
Stun buildup as a percentage of poise. The game's `KeywordPopups` entries
Stun / HeavyStun (55 / 57) base buildup on damage dealt; Premitigation (422)
distinguishes damage before enemy mitigation. The calculation reuses the
hit-DPS pass's per-type mitigated damage, with armour applied separately to
non-critical and critical hits before crit weighting. Damage types present
only on critical hits remain in the average. It does not multiply
by hit chance; alternating hands are averaged per successful hit. Player
physical damage and melee hits each give 50% more buildup, multiplicatively;
cold contributes zero (`KeywordPopups[3] Cold`). The monster 100% physical /
33% melee bonuses describe damage dealt by monsters. Dazed enemies receive
50% more Stun Buildup (`KeywordPopups[252]`); `conditionEnemyDazed` is
Effective-gated, as are the other enemy configuration debuffs.

The shared poise formula uses `GameConstants.HeavyStunDamageScale = 58 / 100`
(0.58), the level's `DefaultMonsterStats.PoiseThreshold`, player
`EnemyHeavyStunBuildup` / `EnemyImmobilisationBuildup`, and enemy buildup and
threshold modifiers. Boss poise combines the unique-rarity 500% MORE
(`Mods.MonsterUnique2`) with `MonsterVarieties.PoiseThreshold / 100`, both
behind `Condition:Effective`. The 0.5.5.4.2 game rows are manually curated in
`data/overlay-common/boss_poise_thresholds.json`, including row IDs and export
SHA256: 313 for Boss (the mode among MAP rows with BossHealthBar), 1700 for
Pinnacle/Uber (Xesht, row 1946; no distinct Uber variety). These are representative
tier defaults, not a claim that all bosses share a threshold. The adapter should
generate this domain when that column joins the regular export. The vendor
Map Boss/Xesht Poise entries in `base/enemy_presets.json` remain unused.

`CannotStun` / `CannotHeavyStun` suppress buildup. The derived detail reuses the
retained calculation terms. Mauling Stuns uses `enemyDistance <= metres * 10`,
sharing Focused Thrust's handler and ledger gate; unset distance retains the
zero-distance convention.

`PinBuildupAvg` reports the selected successful hit's average enemy Pin buildup
as a percentage of the Pin threshold. The game's `KeywordPopups[60]` says that
"Certain skills and effects allow damage to build up Pinned", so buildup is
opt-in per damage type: a hit builds Pin only from the types that an applying
source enables. The sources are the selected skill's
`base_physical_damage_can_pin` (Physical) and `base_all_damage_can_pin`, a
weapon's `local_all_damage_can_pin` (that hand's hits only),
`all_projectile_damage_can_pin` (projectile hits only),
`physical_spell_damage_can_pin_on_critical_hit` (Bone Chains: the Physical part
of a critical spell hit only; the non-critical leg and other types add nothing)
and the Pin support's `support_pin_physical_damage_can_pin`. `cannot_pin` and
"Cannot Immobilise" force 0. They are the `CanPin` / `<Type>CanPin` flags;
critical-only sources reuse Heavy Stun's separate critical and non-critical
mitigated legs (`calc::ailment::pin_terms`). The scale is
`GameConstants[45].PinDamageScale = 420 / 100` (4.2) over the same poise
threshold as Heavy Stun (`PinThresholdModifier` [48] and
`HeavyStunThresholdModifier` [29] are both 500, so the boss poise rows above
apply unchanged). Player `EnemyPinBuildup` / `EnemyImmobilisationBuildup` (Pin
support, "increased Pin Buildup", `active_skill_pins_as_though_dealt_damage_+%_final`)
and the enemy's `PinBuildup` / `ImmobilisationBuildup` scale it. Heavy Stun's
50% physical and melee multipliers are not applied: the game text gives them
only for Light Stun chance and Heavy Stun buildup (`KeywordPopups[55]` / [57]).
That is a convention, **no in-game test**. A skill with
`support_pin_buildup_pin_instead_of_stun` (the Pin support) builds no Heavy Stun
(`HeavyStunBuildupAvg` is 0) and its damage counts toward Pin; the support's
buildup modifiers are the Pin ones, and PoBR does not copy the skill's Heavy
Stun modifiers to Pin as PoB2 does. `PinBuildupRequired` reports the buildup
threshold for the selected hit: normally 100%, or the target's Primed-Pin
threshold when `pin_almost_pinned_enemies` (Constricting Command) grants
`PinOnPrimed`. Game `KeywordPopups[400].PrimedPin` gives 40% / 50% / 60% / 70%
for Normal / Magic / Rare / Unique enemies. This rule can Pin an already
primed enemy without granting the skill a source of buildup. `PinDuration`
uses `GameConstants[46].PinDuration = 3000 / 1000` (3 seconds), the inflicted
`pin_duration_+%`, the Pin support's duration MORE, target `SelfPinDuration`
and enemy debuff expiry rate. Both views are zero without a Pin source,
when the skill cannot hit, or when Pin is prohibited. These are application
requirements and duration views, with derived inputs that rebuild each value;
no hit-sequence, buildup decay or uptime is inferred, and `conditionEnemyPinned`
stays a configured state. The new rules are exporter-absent. PoB2 pins from unmitigated hit
averages and evaluates `Physical Spell Critical Hits build Pin` once against the
skill, so a physical spell pins with every damage type; that rule declares
`exporter: "differs"`. Worked comparisons against the pinned oracle: Chronomancer
Reap `3875ffe5d15e` 12.976 vs 55.896 (0.232 = 0.552 from the Xesht 17 / 9.38
poise factor, as for Heavy Stun, times 0.42 because PoBR pins only the Physical
critical portion while PoB2 adds Cold, Fire and Lightning); Tactician Oil Barrage
`02b94461578f` 6.615 vs 17.082 (0.387) and Crossbow Shot `406c76f8e89a` 6.747 vs
12.506 (0.539) follow their `HeavyStunBuildupAvg` ratios (0.396 and 0.541), so the
remaining gap there is the shared poise and mitigated-versus-unmitigated source
damage, not Pin; Blood Mage Impending Doom `add19135df93` is 0.364 vs 0, because
the 3260 Physical part of the Dark Consequences frog explosion (a Spell by game
skill type) pins in PoBR while the oracle reports empty `skillFlags` for it.

## 破甲与命中效果视图

Player `AilmentThreshold` starts at half maximum Life, adds flat threshold and
percentages of final maximum Energy Shield, then applies threshold increases
and multipliers ([game keyword text](https://poe2db.tw/us/Keywords)).
The display retains fractional threshold: Chaos Inoculation's 1 Life gives
0.5, rather than PoB2's 1 from rounding percentage contributions upward.
The keyword specifies half Life and no integer ceiling; this is an intentional
precision convention, without an in-game measurement of the rounding boundary.
`LifeOnKill`, `ManaOnKill` and `EnergyShieldOnKill` report flat recovery plus
percentages of their final maximum pool per kill, respecting recovery blocks;
the selected skill's hit kills must belong to the player. Totems, traps, mines
and other proxies report zero player recovery, consistent with the keyword's
separate entity using the skill. They do not assume a kill frequency or model
damage-over-time kill attribution. Cannibalism support's
`support_cannibalism_recover_%_maximum_life_on_kill` remains an unmapped buff
stat (Cannibalism II gives 3 in `base/granted_effect_stat_sets.json`); the
Amazon Twister `5355205a1743` missing 81 is a mechanic gap, not a different
unit or default. `DazeChance` reports the selected hit's
chance, capped at 100%. Freeze buildup reads `FreezeBuildup` alongside the
existing enemy/immobilisation modifiers. Fully Broken Armour removes Armour
and makes non-player targets take 20% increased Physical damage from Hits;
`FullyBrokenArmourEffect` displays that percentage after effect modifiers,
and it affects hits only when the enemy is configured as fully broken.
These fields expose source details through the display catalog.

The Molten One's Gift (passive 28542) extends the same Fully Broken Armour
percentage to Fire Hits; Storm's Rebuke (22726) extends it to Cold and Lightning
Hits (`fully_broken_armour_and_sundered_armour_you_inflict_also_applies_to_*`
in the active game's `base/stats.json`, with the wording in `base/passive_tree.json`).
The existing effect modifiers scale each granted type once, including the
15% increased effect on The Molten One's Gift. This is the player's inflicted
debuff on the shared enemy, so minion Hits benefit too; minion-only grants and
effect modifiers do not alter it. The existing Fully Broken Armour state is
required even when the enemy has no Armour. Damage over time is unaffected,
and neither breaking Armour nor its uptime is inferred from possessing a grant.

On-hit effect views (`calc::effect_views`, all reported only: configured enemy states such as Fully Broken Armour, Dazed, Maimed or Blinded still decide every damage and defence effect; **没有游戏内实测 / no in-game test**). Shared terms feed both each value and its derived detail:

- `ArmourBreakPerHit` = (flat `ArmourBreakPerHit` BASE + Σ dealt damage × `<Type>DamageAsArmourBreak` %) × (1 + INC / 100) × MORE × rarity. Dealt damage is the selected hit's average after enemy mitigation (crit-weighted, before hit chance); “Break Armour on Critical Hit with Spells” uses only the critical share c·m / (1 − c + c·m). KeywordPopups ArmourBreak: “Players Break 3 times Armour against Normal Monsters and 2 times Armour against Magic Monsters”; there is no Magic enemy configuration, so non-Rare/Unique enemies use 3×. `HitsToFullyBreakArmour` = ⌈enemy Armour before break ÷ per hit⌉, `TimeToFullyBreakArmour` = hits ÷ landed hits per second (action rate × repeats × hit chance). Sources: `armour_break_amount_+%` / `_vs_enemies_affected_by_ailments` (Exposed Wounds), `ArmourBreakerPlayer`'s `apply_X_armour_break_on_hit`, `armour_break_{physical,fire}_damage_%_dealt_as_armour_break`, `support_increased_armour_break_armour_break_amount_+%_final`.
- `DazeUptime` / `MaimUptime` / `BlindUptime` use KeywordPopups durations (Daze 8 s, Maim 4 s, Blind 4 s) times duration modifiers and enemy debuff expiration. With evenly spaced landed hits, n = rate × duration, k = ⌊n⌋, f = n − k and per-hit chance p, uptime = f·(1 − (1 − p)^(k+1)) + (1 − f)·(1 − (1 − p)^k); at 100% chance this is min(1, rate × duration). Attack-only chances (`maim_on_hit_%`, `attacks_chance_to_blind_on_hit_%`) apply to attacks; `Cannot*` and enemy immunity give zero. Turning a ≥99.9% uptime at 100% chance into the enemy state automatically is a possible follow-up, not implemented.
- `Maximum{Fire,Cold,Lightning}Infusions` = 3 (GameConstants `BaseMaximumInfusionCount`, KeywordPopups “up to 3 of each Infusion”) + per-type and all-type additions. `SurroundedRadiusMetres` is the 3 m Surrounded radius (KeywordPopups; `BaseRequiredEnemiesToBeConsideredSurrounded` = 5) scaled by `SurroundedArea` like other areas.
- `CurseActivationDelay` is the selected curse's `curse_delay_duration_ms` × (1 + delay increase − activation-speed increase) × MORE; Whispers of Doom (`keystone_hex_master`, KeywordPopups HexMaster “Double Activation Delay of Curses”) is MORE 100. The view and its readers need a curse as the selected skill (a registered read gate).
- `RemnantsPerUse` = 1 + `chance_%_to_create_additional_remnant` / 100 (Remnant Attraction 10%; Lingering Illusion's 156%) for skills with `GeneratesRemnants`. Into the Breach's configured collected Purple Flame stacks are not a creation rate, so extra creation chance does not multiply them; remnant collection, Arcane / Empowering Remnants, Elemental Equilibrium and Storm's Recollection still need a collection model.

The new rules are `exporter: "absent"`. Standalone “Immune to Chaos Damage” (game stat `chaos_immunity`) sets `ChaosImmune`, which `defence::chaos_immune` shares with Chaos Inoculation for chaos hits and chaos damage over time (`exporter: "differs"`); the CI pool rerouting stays tied to the keystone.
