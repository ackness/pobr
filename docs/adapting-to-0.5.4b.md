# Adapting PoBR to game 0.5.4b (data 4.5.4.3)

Status: **milestone kickoff** (2026-07-15). Runtime data is already on 4.5.4.3;
the calc **engine** still implements 0.5.0 formulas. This doc records the gap map
and the plan to advance the engine so parity can move to the 4.5.4.3 golden.

## Why this is an engine milestone, not a data task

The parity golden values in `examples/demo-bd-test/builds/*/meta.json` are the
PoB2 numbers captured from real poe.ninja builds at **game 0.5.0**. Data version
4.5.4.3 is **game 0.5.4b**, which rebalanced several defensive and ailment
formulas. Evidence the drift is real (not a config/oracle artifact):

- Re-capturing all 18 fixtures against the vendored 0.5.4b PoB2 (oracle) loads
  every build (0 failures) and shows build-specific drift: a caster with no
  evasion stacking (`monk-invoker-frost-bomb`) moves only 0.7%, while
  evasion/armour builds move 3–10×. A global config artifact would move all
  builds; a formula rework moves only the builds that use the reworked mechanic.
- PoBR fed 4.5.4.3 data still produces ~0.5.0 numbers (e.g.
  `huntress-ritualist` TotalDPS 68798 ≈ its old 0.5.0 golden 69700), while the
  0.5.4b oracle gives 101720 — PoBR's engine, not its data, is the lag.

Flipping `GOLDEN_PARITY_DATA_VERSION` to 4.5.4.3 with the re-captured golden
drops parity from ~96% to ~82% (defensive core-8 @5% 139→118). Those lost cells
are the adaptation work list below.

## Gap map (PoBR@4.5.4.3 vs oracle@0.5.4b)

Ratios are PoBR / golden; <1 means PoBR under-computes.

| Mechanic | Ratio | Representative builds | Note |
|----------|-------|-----------------------|------|
| Armour (value) | ~0.54× | `warrior-titan-shield-wall` (21201 vs 39546) | Titan armour scaling: gear 6239 ×(1+102% inc)=12603; oracle reaches 3.14× gear, PoBR only 1.68× — a missing more-armour / ascendancy source. |
| Evasion (value) | 0.32–0.60× | `huntress-ritualist`, `ranger-pathfinder-ice-shot`, `monk-martial-artist-twister` | 0.5.4b evasion rework; PoBR under-scales. |
| DeflectionRating | ~0.60× | `monk-martial-artist-twister` (13476 vs 22312) | New/reworked deflection scaling. |
| Some skill DPS | ~0.60× | `druid-oracle-ember-fusillade` (329k vs 549k) | Offence-side rebalance (per-build, not global). |
| Ailment (Ignite) DPS | large on ignite builds | several | 0.5.4b ailment magnitude change. |

Not a single systemic constant — each cluster is a distinct 0.5.4b formula/
mechanic change requiring its own fix against the current vendor Lua.

## Plan

**Phase 0 — adopt 4.5.4.3 as the parity target (baseline flip). ✅ DONE (4f02854).**
Re-captured all 18 fixture goldens via the 0.5.4b oracle, flipped
`GOLDEN_PARITY_DATA_VERSION`, re-pinned the mechanical mirrors, re-baselined
`parity_no_regression` / `panel_mode_no_regression` to the honest migration
numbers (def core-8 @5% 139→118, off 71→39, dot 26→9, panel 44→27, etc.), and
`#[ignore]`'d the armour/evasion/deflection/DPS canaries with per-mechanic
tracking notes. Repo green on 4.5.4.3; every Phase 1 fix ratchets these up and
un-ignores its canary.

**Phase 1+ — close gaps mechanic by mechanic**, each against the vendored 0.5.4b
Lua with the oracle's `defenceModList` / `intermediates` to pinpoint the exact
missing source.

**#1 target — Mageblood (common root cause, highest leverage). ✅ DONE.**
Diagnosed via `defenceModList`: the armour gap on `warrior-titan-shield-wall`
(`Mageblood` INC +219 Armour) and the evasion gap on `ranger-pathfinder-ice-shot`
(`Mageblood` BASE +2000, INC +150 Evasion) are the *same* mechanic. 9 of the 18
endgame fixtures wear Mageblood. PoB2 models it not as a flask channel but via the
belt's selected **variant lines** — each `Legacy of <X>` line and the
`All Mage's Legacies have N% increased effect per duplicate…` implicit.

Implemented to match vendor exactly:
- **Parse** (`rules/handlers/mageblood.rs`, `special:mageblood_legacy`): each
  `Legacy of <X>` → `LegacyOf<X>` BASE 1 + `MagebloodEquipped` FLAG (vendor
  `ModParser.lua:5554-5557`; captured mod name → handler like `granted_passive`).
  The `MagesLegacyEffect` implicit was already covered by the auto-extracted
  `special_vendor` batch.
- **Apply** (`calc/mageblood.rs`, env_finalize stage 2.5, vendor
  `CalcPerform.lua:66-142` table + `:1502-1528`): sum each `LegacyOf<X>` BASE =
  stacks; `totalDuplicates = Σ max(stacks-1,0)`;
  `globalEffect = 1 + totalDuplicates × MagesLegacyEffect/100`; each present
  legacy applies its effects **once**, `floor(globalEffect × base)`,
  source "Mageblood".

Result: def core-8 @5% 118→135, def 25-col @5% 343→393 / @10% 361→417; three
canaries (`physical_armour_block`/`cold_projectile_evasion_es`/`evasion_melee`)
un-ignored; `ranger-deadeye` Evasion 0.99x vs 0.5.4b golden 29774. The stale
`ninja-bd-deadeye.txt` embedded PlayerStats predate PoB2's Mageblood modeling
(res/evasion assertions on that old sample relaxed accordingly).

**#2 target — DeflectionRating. ✅ DONE (re-triage + Refraction buff).**
Re-triage after Mageblood showed the gap-map entry (`monk-martial-artist`
13476 vs 22312) was **downstream of the Evasion gap**: rating derives from
`Evasion × EvasionGainAsDeflection%` (CalcDefence.lua:1516-1522, formula
unchanged in 0.5.4b) and closed to 0.998x once Mageblood landed — the
deflection canary (`defence_panels_golden::deflection_matches_golden`)
re-verified against a fresh oracle run and un-ignored, no formula change.
One genuine deflection-side source remained: the **Refraction I/II support**
Refractive Plating buff (`sup_str.lua:5984/6023`,
`support_tempered_valour_deflection_rating_%_of_evasion_rating` BASE 20,
gated by `MultiplierThreshold ValourStacks/thresholdVar=
RefractionMinimumValour` — zero setters vendor-wide, so threshold is
statically 0). Admitted via the player-buff stat-map allowlist plus a
`MultiplierThreshold` arm in `translate_tag` (thresholdVar accepted only for
verified zero-setter vars). wolf-pack DeflectChance 0.80x→1.00x, pathfinder
0.93x→1.00x; def 25-col baseline 405→407 @5% / 428→429 @10%. The same buff's
`ArmourAppliesTo<El>DamageTaken` payloads stay reported (EHP-side lead:
wolf-pack TotalEHP 0.76x→0.81x, remainder likely there).

**#3 target — `ArmourAppliesTo<El>DamageTaken` (Refraction buff payload).
✅ DONE.** The consumption chain already existed end-to-end
(`calc::taken::armour_applies_pct` → `build_mitigation_ctx` → per-type
DamageReduction / MaximumHit / EHP, mirroring CalcDefence.lua:2361-2368
`percentOfArmourApplies` → `effectiveAppliedArmour`); tree-sourced percentages
flow through `mod_parser` ("X% of Armour also applies to Y damage taken").
The only gap was the player-buff stat-map allowlist: the Refractive Plating
buff's second stat key
(`support_tempered_valour_%_armour_to_apply_to_elemental_damage`,
`sup_str.lua:6019-6021`, three `ArmourAppliesTo{Fire,Cold,Lightning}
DamageTaken` BASE 30 mods with the same GlobalEffect + MultiplierThreshold
tags as the deflection payload) hit `UnknownModName`. Oracle attribution
(extended `defenceModList` name set) pins wolf-pack at tree 84 + buff 30 =
114% per element, `<El>EffectiveAppliedArmour` = 18580 × 1.14 = 21181.2.
Admitting the three names closes wolf-pack Fire/Cold/LightMaxHit
0.94x→0.96x (@5% hits) and TotalEHP 0.81x→0.88x; def 25-col baseline
407→410 @5% (429 @10% unchanged). The EHP remainder is **not** this channel:
Armour itself 0.98x (18169.78 vs 18580), ChaosMaxHit 0.87x (oracle
`ChaosEffectiveAppliedArmour` = 0 — chaos gap is elsewhere), and Life 1.11x
(PoBR overestimate 2973.6 vs 2674).

**#4 target — offence per-build DPS clusters. ✅ DONE (three sub-fixes).**
Re-triage first: the gap-map poster child `druid-oracle-ember-fusillade` was
already at TotalDPS 1.00x (closed as a side effect of earlier fixes; its
`canary_fire_spell_armour` re-verified and un-ignored). The real 0.5.4b
clusters (cells where PoBR ≈ old 0.5.0 golden while the 0.5.4b golden moved):

- **#4a Atziri's Communion → auto Low Life** (`huntress-ritualist-bow-shot`,
  `witch-abyssal-lich-detonate-dead`): 0.5.4b added
  `LifeReservePercentPerSpirit` (CalcDefence.lua:248-254) — the Communion
  support converts a persistent skill's Spirit reservation into Life percent
  reservation (0.66%/spirit), pushing heavy-reservation builds under the
  auto Low Life threshold (:335-350, unreserved ≤ 35%), which unlocks the
  "while on Low Life" mod family. Oracle `damageModList` pinned ritualist's
  missing 130 Damage INC to exactly the two LowLife-gated entries (tree
  +60, Direstrike buff +70 — the buff's stat-map entry already carried
  `Condition:LowLife`; only the condition was never true). Three generic
  consumption points: nameSpec-only gem resolution (lineage supports
  serialize without skillId/gemId), the spirit→life conversion branch in
  `spirit_reservation_modifiers`, and
  `CalculationSession::bridge_low_pool_conditions` (orchestrator 6e).
  Ritualist TotalDPS 0.68x→0.99x, TotalDotDPS 0.60x→1.00x,
  SpiritUnres/LifeUnres exact.
- **#4b Voices sinister jewel sockets** (ritualist, titan, abyssal-lich):
  the 0.5.4b Voices unique "Allocates 2 Sinister Jewel sockets"
  (ModParser 0.22.0 `GrantedPassive SinisterJewelSockets`,
  PassiveSpec.lua:1067-1090 alias order `voices_jewel_slot1..5` → 0_5 tree
  nodes 62152/26178/23960/39087/3367). PoBR's tree-jewel gate dropped
  jewels in never-allocated sinister sockets; `parse_items_and_slots` now
  admits the first N pinned sockets. Ritualist CritChance 27.84→29.10 and
  CritMultiplier 4.37→4.74, both exact (missing 13/37 INC matched the two
  sinister jewels line-for-line).
- **#4c grenade phrases un-preempted** (`ranger-deadeye-` /
  `mercenary-gemling-explosive-grenade`): vendor 0.22.0 added
  `not grantedEffect.fromItem` to the gem-name registration loop
  (ModParser.lua:6423) — "Grenade" (fromItem) no longer shadows the
  modFlagList phrases, so `grenade` / `for grenade skills` are live
  `SkillType.Grenade` tags again (run-parsemod verified). The PR#53-era
  extractor overrides (dead-entry / inert-SkillName rewrite, correct for
  0.21) were retired and `mod_parser_rules.json` + `parsed_mods.json`
  regenerated. Deadeye's 3×15% grenade-CDR tree nodes (oracle
  `extraModList` — new `ORACLE_EXTRA_STATS` tooling — Tree:21077/354/48429)
  now apply: Speed 0.164→0.254 (1.00x), TotalDPS 0.53x→0.83x; gemling
  Speed/AvgDamage/TotalDPS all 1.00-1.04x.

Aggregate: off @5% 46→56 (@10% 55→62), dot 11→14 (@10% 13→19), def 25-col
410→413 (@10% 429→434), core-8 138→140, panel off 27→40 / 30→41.

Remaining offence clusters after #4 (TotalDPS @5%, decomposed): the attack
AverageDamage family — smith-of-kitava 0.57x, monk-flicker 0.76x,
spirit-walker 0.78x (dot 0.23x), monk-twister 0.88x, titan 0.87x (its
CritMultiplier is a separate 0.5.4b "Bifurcated Crit Damage Bonus" MORE
mechanic, golden 5.00 vs PoBR 6.0) — plus blood-mage 0.87x, abyssal-lich
0.91x, pathfinder 0.90x, deadeye per-hit 0.83x (pre-0.5.4b shortfall,
documented in pob2_parity.rs), and frost-bomb 0.66x (golden unchanged
0.5.0→0.5.4b — a pre-existing cooldown-DPS gap, not a 0.5.4b regression).
These moved 6-46% in the 0.5.4b golden with per-build factors — no shared
constant; each needs its own oracle decomposition.
✅ (2026-07-17, 存量清扫 #7) frost-bomb TotalDPS 0.66x→1.00x：双根因 =
Archmage buff `DamageGainAsLightning`（80% per 100 Mana）未入玩家 buff
允收名单 + SkillType neg tag 不支持；以及 curse 链两缺口（EW 取数等级未吃
+spell-skill levels、技能局部 CurseEffect 段缺失）。见 commit 9075077。

**#5 target — ailment (ignite) magnitude. ✅ DONE (one root cause: Blazing
Critical global fire buff).** Re-triage first: the ailment *formula* did not
change in 0.5.4b — `IgniteChanceMultiplier`/`monsterAilmentThresholdTable`/
`defaultAilmentDamageTypes`/the `ailmentDPSUncapped` assembly are byte-identical
between vendors (only `skillData.dpsMultiplier` → `output.DpsMultiplier`, a
refactor PoBR already mirrors via `dps_end_factors`). Yet 16 of 18 dot goldens
moved (1.07×–22.4×). Oracle decomposition on the extreme mover
(`monk-martial-artist-flicker-strike`, dot 0.05x, golden 47→1054) pinned a
single fire-specific ×4 on the hit: PoBR's stored-Fire range was 0.21x oracle
while every other damage type sat at the uniform 0.85x hit gap. Per-source
tabulation (`ORACLE_EXTRA_STATS=DamageGainAsFire`) showed the missing 15%:
the **Blazing Critical** support (`sup_int.lua:959`) — 0.22.0 added a
`GlobalEffect effectType=Buff` tag to its
`support_blazing_crits_gain_%_fire_damage_with_attacks_on_critical_hit` stat,
turning the 15% `DamageGainAsFire` (Attack + Condition:CritRecently) from a
dead skill-local mod (old-vendor oracle: absent from the skill modList,
IgniteDPS 47.54 = PoBR's 47.55 exactly) into a global player buff ("imbue all
of your Attacks with Fire"). Since ignite chance ∝ fireAvg/threshold and
magnitude ∝ fireAvg, the buff amplifies IgniteDPS quadratically (×16 on
flicker). Exactly the three worst dot movers carry the gem (flicker,
spirit-walker, monk-twister).

Two generic consumption points (no per-gem code):
- `translate_player_buff_mod_name` allowlist admits `DamageGainAsFire`
  (consumer = the existing gain-as matrix in `calc::damage::buildGainTable`
  mirror; flags/Condition tags already translate).
- `support_buff_specs` now judges support compatibility against **additional
  granted effects** too (mirroring `buff_skill_specs`): Blazing Critical's
  host in these builds is Charged Staff, whose gem is Spell-typed — the
  compatible skill is its hidden Attack additional effect
  `ChargedStaffShockwavePlayer` (act_int.lua:3387).

Result: monk-twister TotalDotDPS 0.44x→0.98x, CombinedDPS 0.60x→0.96x,
AverageDamage/TotalDPS 0.60x→0.96x (all into the 5% band); flicker dot
0.05x→0.72x, spirit-walker 0.23x→0.87x (remainder = the attack AvgDamage
family's hit gap squared, not a dot-side mechanic). Baselines: off 56→58 @5%
(62→64 @10%), dot 14→16 @5% (19→21 @10%); def/panel unchanged (CritRecently
is combat-gated, panel mode unaffected). No ailment canary was left ignored
(the suite's only `#[ignore]` is the vendor-dependent oracle differential).

Dot-side leftovers, each triaged **not** a 0.5.4b ailment item:
- `smith-of-kitava` dot 0.20x: hit gap 0.57x squared explains 0.32; the extra
  residual is the un-modeled *uptime-scaled* Infernal Cry `DamageGainAsFire`
  12.04% — present and identical in both vendors (old-vendor oracle proves it
  predates 0.5.4b), i.e. a pre-existing warcry-uptime gap.
- `deadeye` 0.69x = hit 0.83x², `blood-mage` 0.79x ≈ hit 0.87x² (its ~16%
  per-hit shortfall is the long-registered pre-0.5.4b item), `titan` 0.91x /
  `abyssal-lich` 0.94x / `pathfinder` 0.92x track their hit gaps.
- `frost-bomb` dot 0.87x and `essence-drain` WithDotDPS 1.36x: goldens
  byte-identical across the flip — pre-existing gaps.
  ✅ (2026-07-17, #7) essence-drain 1.36x→1.00x：数据管线漏下
  `GrantedEffectStatSets.RemoveStats`（社区 schema 名 `IgnoredStats`）——
  ED 的 DoT set 本应移除主 set 的击中伤害 stat，PoBR 幻影击中 120.87 DPS。
  管线补列 + adapter 置零语义 + 55 效果重生成（commit 5edb741）。
  frost-bomb dot 随 #7-1 修复 0.87x→1.07x（@10% 带内）。
- `gemling` dot 1.10x over: oracle per-component shows fire stored 1.03x ×
  stacks 1.05x × slight crit-chance overshoot — downstream of its small
  hit-side overestimates, no dot-side mechanism.

**#6 target — the attack AverageDamage family. ✅ DONE (five generic-consumption-point
fixes; the whole family is inside the 5% band).** Per-build oracle decomposition
confirmed the #4 verdict (per-build factors, no shared constant) but found two
*shared* root causes hitting multiple builds plus per-build items:

- **#6a Bifurcate crit pipeline (Garukhan's Resolve builds: spirit-walker /
  pathfinder / titan).** Two halves:
  (i) the bifurcated extra-crit-damage weight is the *conditional* probability
  `(PreBifurcateCritChance²/100) / CritChance` (CalcOffence.lua:3823-3846 —
  identical in 0.21/0.22, i.e. a PoBR mis-port, not a 0.5.4b change; PoBR had
  the unconditional `pre²/10000`), and
  (ii) **0.22.0 added `CritMultiplier` to the weapon convert-to-local list**
  (Item.lua:1954-1961): an unflagged crit-damage-bonus mod on a weapon now
  gains `Condition:{Main,Off}HandAttack`, so Nebuloch's +29% no longer leaks
  into titan's Shield Wall (a non-weapon attack running the off-hand pass).
  Consumption points: `resolve_crit_multiplier` + new
  `CalculationSession::add_weapon_item` (orchestrator routes items whose base
  resolves as a weapon). Titan CritMultiplier 6.02→5.00 (golden exact,
  oracle PreEffectiveCritMultiplier 4.88 reproduced); spirit-walker /
  pathfinder CritMult 0.94x→1.00x/0.99x.
- **#6b enemyDistance placeholder feeds skillDist (0.22.0,
  CalcActiveSkill.lua:671 `configInput or configPlaceholder`).** The old vendor
  read only the explicit `<Input>` (what PoBR mirrored), so every DistanceRamp
  mod was dropped whole. Close Combat II's 30% MORE (ramp {10:1, 35:0},
  evaluated at the placeholder distance 20 → ×0.6 = 18%) now lands. Fallback
  chain mirrors ConfigTab: Input → XML Placeholder → catalog
  `defaultPlaceholderState`. flicker 0.838x→0.99x; smith one of three stacked
  segments. (Oracle probes added: `intermediates.SkillDist` +
  `MoreDamage_at{5,20,30,40}` — this is how the 30-vs-18 ramp base was pinned.)
- **#6c PerStat `statList` (smith/titan shield notable Tree:27687, "4%
  increased Attack Damage per 75 Item Armour and Evasion on Equipped
  Shield", ModParser.lua:1631).** The rule-driven parser rejected statList
  tag phrases whole. Normalized to a `|`-joined compound Multiplier var,
  summed at eval (ModStore.lua:445-452 semantics: Σstats then one
  floor(sum/div)). Oracle-pinned 88 INC; smith Physical pool scale lands at
  9.5899 = oracle exactly.
- **#6d hybrid mana→life cost + per-LifeCost mods (smith's Atalui's
  Bloodletting).** `base_skill_cost_life_instead_of_mana_%` →
  `HybridManaAndLifeCost_Life` turns `floor(manaBase × supportMult) × hybrid`
  into the Life cost base (CalcOffence.lua:2067/:2090-2104; mana chain tail
  `floor((1-hybrid)×ManaCost)`); statmap PerStat now admits
  `limit`/`limitTotal` so the support's `PerStat{stat=LifeCost, div=20,
  limit=40, limitTotal}` gain-as-physical maps; the orchestrator prefills
  `cfg.stats/multipliers[LifeCost]` from the new
  `CalculationSession::life_cost_snapshot` (vendor's cost-before-damage
  ordering). Oracle LifeCost 309 → floor(309/20)=15 → +30%
  DamageGainAsPhysical.
- **#6e per-leg enemy mitigation in the crit short-circuit path.** The
  short-circuit (identical legs) mitigated the non-crit leg and scaled by
  crit.effect — valid only for raw-independent mitigation; enemy armour DR
  depends on hit size (vendor pass 1 computes DR against the post-crit-mult
  hit, CalcOffence.lua:4395 blend). Reconstruct the crit leg as
  `non_crit × crit.multiplier` and blend per-leg; bit-identical when
  mitigation has no raw dependency. spirit-walker mitigated hit 42291 vs
  vendor AverageDamage 42318 (0.9994).

Aggregate: off @5% 58→71 (@10% 64→73), dot 16→25 (@10% 21→27), panel off
40→41/41→42; def unchanged. Per-build TotalDPS (effective): smith 0.575x→0.996x,
titan 0.874x→0.978x, flicker 0.838x→1.003x, spirit-walker 0.892x→0.980x,
monk-twister 0.958x→0.980x, pathfinder 0.904x→0.963x, ritualist →1.000x.

Remaining offence after #6, each triaged:
- `deadeye` 0.832x — the long-registered pre-0.5.4b per-hit shortfall
  (pob2_parity.rs); golden did move in the flip but the residual matches the
  old ledger.
  ✅ (2026-07-17, #10) 根因钉死：Deadeye 升华 Point Blank（Tree:41875）的
  「first 3.5m 20% more → 7m 后 0%」被引擎误析为平坦 ProjectileDamage MORE 20
  且带 leftover 被产线丢弃——所有分量统一少 ×1.20（oracle：MORE Damage 100 +
  DistanceRamp {35:0.2,70:0}，SkillDist=20 求值 +20%，ModParser.lua:2911-2913）。
  修复 = special 通道字面条目 ×2 + special 编译器 DistanceRamp tag 支持。
  AverageDamage/TotalDPS 0.83x→1.00x，连带 ignite 链 TotalDotDPS 0.69x→1.00x。
- `blood-mage` 0.880x / `abyssal-lich` 0.926x — Mageblood mod family
  (oracle pins blood-mage's missing `INC CritChance 107 'Mageblood'`; golden
  CritChance 72.45→92.1 and CritMult 5.34→5.87 moved with 0.5.4b while PoBR
  sits at the old CritMult 5.34 exactly). This is the Phase-1 standalone item.
- `frost-bomb` 0.661x — golden unchanged across the flip; pre-existing
  cooldown-DPS gap.
- `smith` TotalDotDPS 0.40x — hit is 0.996x now, so the dot residual is purely
  the un-modeled *uptime-scaled* Infernal Cry `DamageGainAsFire`
  (fire-source ratio 0.63² ≈ 0.40). Mechanism fully decomposed: uptime =
  `min((NumInfernalEmpowers/Speed)/(cooldown + warcryCastTime), 1) ×
  storedUses` (CalcOffence.lua:3229-3257), `NumInfernalEmpowers =
  floor(min(WarcryPower 20, cap 50)/per 10)` (CalcPerform.lua:2117-2131),
  gain-as = gem's `infernal_cry_exerted_attack_all_damage_%_to_gain_as_fire_%`
  (62 at smith's level) × uptime 19.41% = 12.04. Identical in both vendors —
  a pre-existing warcry-uptime gap; needs the warcry buff machinery
  (duration/cooldown/cast-time of a non-main skill + WarcryPower config),
  registered for its own slice.
  ✅ (2026-07-17, #9) warcry uptime 机器落地（pobr-core `calc::warcry` +
  编排层 `warcry_skill_specs`，全数据驱动零技能硬编码）：smith TotalDotDPS
  0.40x→1.06x（@10% 带内）。uptime/castTime/cooldown 对 oracle 逐位
  （19.4116% / 0.544218 / 6.27），注入 gain 12.035 = vendor "Uptime Scaled
  Infernal Cry" 条 bit-exact。连带修三处通道：敌人档位预设 player_mods
  （WarcryPower BASE 20）此前未注入 player db；statmap 直通族补
  WarcryPowerPer/Cap + InfernalExtraFireDamageMultiplier；parser 「skill
  speed」短语恢复 vendor 三名扇出（ModParser.lua:770 {Speed, WarcrySpeed,
  TotemPlacementSpeed}，此前 curated 收窄为单名致树上 17 INC WarcrySpeed
  丢失）。残余 +6% = smith 命中侧原有 +2% 高估（此前被缺失 gain 抵消遮蔽）
  经点燃平方放大；titan 同理暴露（TotalDPS 0.98→1.05，带内）。dot 基线
  @10% 31→32。
- `spirit-walker`/`monk-twister` last ~2%: "Barrage Repeats" MORE DPS
  (vendor `output.DpsMultiplier` via `calcLib.mod(..., "DPS")`) — repeat
  DPS bonus channel unwired.
  ✅ (2026-07-17, #10) 通道接通：Barrage buff 载荷（BarrageRepeats BASE 2 /
  BarrageRepeatDamage MORE -45 / SequentialProjectiles flag）经 buff 域允收
  名单入 player db，`dps_end_factors` 复刻 vendor 门控（CalcOffence.lua:962-976）
  与 MoreInternal 逐名桶百分比取整（ModList.lua:143：MORE 1.65 → ×1.02，
  oracle DpsMultiplier=1.02 逐位）。两 build TotalDPS 0.98x→1.00x。
  vendor else 分支（crossbow barrage 攻时惩罚）无语料覆盖，未实现（代码内登记）。
  同批（#10-3）smith/titan 命中高估根因 = 非伤害源武器上的裸「Adds N to M
  <type> Damage」泄漏进全局加法桶（vendor Item.lua:1923-1928 全类型折入
  weaponData 局部；titan Nebuloch 混沌 30-52 × added-effectiveness 3.47 =
  幻影 104-180 混沌 base，oracle ChaosMinBase=0）。剔除后 titan TotalDPS
  1.05x→1.01x、smith AverageDamage 1.02x→1.00x；titan 残余 ~0.9% 与
  Armour 面板 0.99x 同根（盾 rolled 1736 vs vendor 重算 1725 喂
  per-15-shield-armour phys base）+ 暴击率尾差，属防御侧通道。

Remaining elsewhere, re-triage against fresh `defenceModList` dumps:
the wolf-pack EHP remainder decomposed above
(Armour 0.98x / ChaosMaxHit 0.87x / Life 1.11x). Also ritualist TotalEHP
moved 1.04x→1.10x with #4a (低血 EHP 口径——vendor 只在显式
`conditionLowLife` config 下 cap `LifeRecoverable`，PoBR 的 EHP 消费端
尚未对齐该分支). Each is its own oracle-guided investigation; not all are
single formula constants (several are unmodeled unique/flask interactions).
✅ (2026-07-17, #7) 两项收口（commit 6bb0685）：
- ritualist TotalEHP 1.10x→1.00x：LifeRecoverable 口径是误诊（两侧本就一致，
  vendor 只在显式 config 下 cap）；真根因 = 策展条目 `also_grants_guard`
  （护符 "Also grants N Guard"）vendor 根本不解析（oracle AnyGuard=False），
  幻影 +491 共享 Guard 池已摘除。twister maxhit 族随之 1.06x→0.97x。
- wolf-pack：Life 1.11x→1.00x（Giant's Blood `HalvesLifeFromStrength`
  接消费端）、Armour 0.98x→1.00x（PerStat Spirit 分母改读最终池值 336 而非
  BASE 300）、Mana→0.99x + gemling Life/Mana/MaxHit 族→1.00x（overlay-common
  三色宝石阈值条目池名归一 MaximumLife/MaximumMana）。**未闭合余量** =
  companion ally-mitigation 层：vendor 把每个 hit pool 扩到
  `pool/(1−CompanionAllyDamageMitigation 10%)`（上限 TotalCompanionLife
  2262，CalcDefence.lua:3567-3595 allies 层）——PoBR 无 companion actor，
  maxhit 族诚实落在 ~0.82x（此前被幻影 Guard 掩蔽到 ~0.96x）。归 companion
  本体项目。
✅ (2026-07-17) `check-buff-refs` 15-drift re-review done: the recorded hashes
were pinned at vendor `2df5a743` (pre-0.21.0; never refreshed at a82a33b either),
so every buff "drifted" purely because `doActorMisc` shifted ~+75 lines
(:503-765 → :578-850) and the Arcane Surge block moved :1580-1591 → :1606-1617.
All 15 blocks were diffed against the pinned baseline: 14 byte-identical,
UnholyMight differs by one removed trailing space — zero semantic changes, so
no `buff_definitions` entry content changed and parity is untouched. Line
ranges re-pointed to the 0.22.0 (`ce8bffab`) locations, hashes refreshed via
`--write`, `_meta.vendor_commit` updated; check now reports 0 drift.

**#8 target — the blood-mage/abyssal "Mageblood crit family" residual. ✅ DONE
(one root cause, and it was not Mageblood).** The #6 ledger entry ("PoBR missing
`INC CritChance 107 'Mageblood'`") turned out to be a stale symptom: line-by-line
diff of `calc/mageblood.rs` against vendor `CalcPerform.lua:66-142` shows the
effects table complete (all 14 legacies, Diamond included), and the
per-duplicate implicit (`all mage's legacies have (%d+)% increased effect...`,
ModParser.lua:5558 → `MagesLegacyEffect` INC) is already covered by
`generated/special_vendor.json` (`vnd_all_mage_s_legacies_have_d_...`). A
per-mod dump of the live build confirmed `CriticalStrikeChance INC 107
src=Mageblood` present and oracle-exact. The *actual* gap, pinned by diffing
oracle `critModList`/`ORACLE_EXTRA_STATS` per-source against the PoBR ModDb:
both builds anoint their amulet with `{enchant}Allocates Zarokh's Gift` — a
*named jewel socket* node (0_5 tree 11184, `isJewelSocket` + `sinister`, alias
`DeliriumAnoint_ZarokhsGift_`). Vendor allocates it through the
`ResolveGrantedPassiveNodes` sockets-by-name fallback
(PassiveSpec.lua:1106-1114); PoBR's granted-passive path only matches Notables,
and the jewel gate in `xml_build.rs` only knew the Voices sinister-count
channel, so the socketed jewel was dropped whole (blood-mage: Pandemonium
Ornament — CritChance INC 24 + CritMult INC 25/28, exactly the CritMult
5.34-vs-5.87 delta; abyssal: an ES/defence jewel). Fix:
`NAMED_SOCKETS_0_5` in `xml_build.rs` — scan equipped items' three text
sections for `Allocates <name>`, match named sockets (0_5 tree has exactly
one: Zarokh's Gift), and treat the node as allocated for jewel inclusion,
same pinned-id pattern (and same tree-version ceiling) as
`SINISTER_SOCKETS_0_5`. Results: blood-mage TotalDPS 0.880x→1.00x (CritChance
88.5→92.1, CritMult 5.34→5.87, both golden-exact), abyssal-lich TotalDPS
0.926x→1.00x (CritChance/CritMult 1.00x; ES 12124→12437 vs golden 12434,
MaxHit family + TotalEHP 0.97-0.98x→1.00x). 18-build per-cell diff: only these
two builds moved. Baselines: off 73→76 @5% / 75→76 @10%, dot 27→31 @5% /
31→33 @10%; def counts unchanged (the abyssal def cells were already inside
the 5% band). The other 7 Mageblood wearers: no movement — their legacy
modelling was already complete.

**#11 — two legacy leftovers. ✅ DONE (one real root cause, one stale golden).**

- *druid EW "curse-slot over-application" (TotalDotDPS 1.06x)*: the #7 hypothesis
  (PoBR applying Elemental Weakness that vendor keeps out of the slot) was
  **wrong** — `POBR_DBG_CURSE` + `POBR_DBG_ENEMYMIT` show PoBR's single curse
  slot already resolves to Temporal Chains (EW excluded, IgniteEffMult 0.6 ==
  vendor). The real residual was the **Temporal Chains magnitude**: support
  judgement only enumerated gems whose *primary* granted effect is a support,
  so Blasphemy's support half (`SupportBlasphemyPlayer`, additional granted
  effect carrying `support_blasphemy_curse_effect_+%_final` → `CurseEffect
  MORE -41` @L19, act_int.lua:1126-1163) never entered the curse local-effect
  multiplier. TC applied at floor(-25×0.55)=-13% expire-slower vs vendor
  floor(-25×(1.1×0.59×0.5))=-8 (CalcPerform.lua:2422-2430 inc/more chain +
  Pinnacle `CurseEffectOnSelf MORE -50`), inflating ignite
  `debuffDurationMult` (CalcOffence.lua:1845) 1/0.87 vs 1/0.92. Fix: expand
  `judge_group_supports` candidates to each gem's granted-effect list
  (`gem_effects` overlay) routed by `is_support` — vendor CalcSetup gemList
  semantics — and carry the support *effect id* to all consumers. After:
  ignite duration 4.3478 == oracle, druid TotalDotDPS 1.06x→1.00x, monk
  frost-bomb 1.07x→0.99x, dot baseline 31→33 @5%; per-build curse slot
  occupancy unchanged across all 18 builds.
- *MARTIAL embedded-sample ES 1.12x (pob2_parity.rs)*: **stale golden, not a
  PoBR over-count.** The code is a `targetVersion="0_1"` (PoE2 0.1-era) export;
  feeding the same decoded XML to the current vendor oracle yields
  EnergyShield 7008 vs PoBR 7005.5 (0.9996x), while every era-stable stat
  (Life/Mana/Evasion/resists/crit) matches the embedded values exactly — only
  ES (and offence) drifted with 0.5.x data. Same class as the deadeye
  Mageblood/res stale-sample notes in that file; documented inline and
  guarded with a wide (0.15) tolerance against the embedded value plus tight
  guards on the era-stable stats. Authoritative ES gate remains ninja_parity's
  0.5.4b fixture goldens.

**#12 — companion allies defence layer. ✅ DONE (exact on twister; wolf-pack
residual is mitigation-side, not companion).** Vendor semantics (all pinned by
oracle on `mercenary-tactician-wolf-pack`):

- *Mitigation* (`CalcDefence.lua:2961-2965`): `CompanionAllyDamageMitigation =
  Σ BASE TakenFromCompanionBeforeYou + Σ BASE
  TakenFromCompanionBeforeYouFromDeflected × DeflectChance/100`. The 10% comes
  from Loyalty support's constant stat
  `companion_takes_%_damage_before_you_from_support` (SkillStatMap.lua:2559 →
  `TakenFromCompanionBeforeYou` BASE, GlobalEffect/Buff/unscalable); Wild
  Protector (twister's bear) carries the same stat on its own stat set.
- *Pool* (`CalcPerform.lua:3323-3370` `calcMinionLifePool`): when the player
  has the mitigation mod and no config Override, `TotalCompanionLife` = Σ life
  of minions whose granting skill has `SkillType.Companion` and not
  `MinionsAreUndamagable`. Oracle decomposition for the wolf: minion level 44
  (= `minionLevelTable[18+4]` — gem 18 + item "+4 to Level of all Minion
  Skills"), base life `floor(monsterAllyLifeTable[44]=2938 × minionData.life
  1.1) = 3231`, × Loyalty `support_trusty_companion_minion_life_+%_final`
  MORE −30 → **2262** (`TotalCompanionLife` golden).
- *Consumption*: max hit (`:3656-3663`) folds the layer with the shared
  poolProtected formula (`protected = L/(r)×(1−r)`; pool < protected ⇒
  pool/(1−r), i.e. ×1/0.9); EHP (`:493-495` + `reducePoolsByDamage:525-531`)
  drains `damage × r` capped by remaining companion life, not recoupable.

PoBR implementation (minimal slice — no companion actor abstraction; reuses
the existing minion actor + the existing generic `AllyLayer` machinery that
EHP/max-hit already shared):

- `TakenFromCompanionBeforeYou` admitted through the player-buff statmap
  allow-list (`stat_map_engine::translate_player_buff_mod_name`) — reaches the
  player ModDb via the existing support/active buff channels (Loyalty via
  `support_buff_specs`, Wild Protector via the active-skill Buff branch).
- Minion fixes feeding the life sum (all vendor-faithful, benefit every
  summoner): spawn level now includes `+N to Level` gem bonuses
  (`spawn_minions` → `additional_gem_levels` + `support_granted_gem_levels`);
  minion base life switched from the enemy `monsterLifeTable` to
  `monsterAllyLifeTable` (new `MONSTER_ALLY_LIFE_TABLE` const, vendor
  CalcPerform.lua:1046 floor semantics); Loyalty's −30% more minion life flows
  through a new narrow minion-domain statmap mapper
  (`map_minion_life_stat`, inner allow-list = Life → `MaximumLife`) into the
  group's `MinionModifierEntry` channel.
- `Actor::is_companion` marks companion minions at spawn (skill_types
  judgement); `perform` now runs `perform_minions` *before* `fill_mechanics`
  (vendor precedent: minion life pools are computed before player defence) and
  `inject_companion_life` writes `TotalCompanionLife` BASE into the player
  ModDb (Override-aware); `pool_setup::build_pool_state` emits the
  `companion` AllyLayer consumed by both EHP and max-hit paths.
- Not modelled: the `...FromDeflected × DeflectChance` term (no source in the
  18-build corpus; wolf-pack oracle mitigation is exactly 10 = pure hits
  term) and multi-companion stacking (no corpus; the sum loop is already
  general over `env.minions`).

Results: PoBR wolf pool 3817.0 vs oracle `<Type>TotalHitPool` 3826.67
(0.9975 — remaining 0.25% is the pre-existing Mana 761.2 vs 770 gap);
`huntress-spirit-walker-twister` closes exactly (5×MaxHit + TotalEHP
0.89-0.90x → 1.00x); wolf-pack moves MaxHit 0.82→0.90x, ChaosMaxHit
0.72→0.80x, TotalEHP 0.74→0.83x — the residual is a uniform ~10% per-type
taken-multiplier gap (armour-applies/mitigation side, tracked separately),
not the companion layer. Baselines: def 425→431 @5%, 434→439 @10%; off/dot/
core-8 unchanged; 18-build per-cell diff shows only twister and wolf-pack
moving.

**#13 — defensive residuals pinpoint fixes. ✅ DONE (wolf-pack + titan defensive
columns all exact).** Three independent root causes, each pinned by oracle:

- *wolf-pack per-type taken ~10% uniform gap = enemy Intimidated base pair.*
  Oracle `<X>EnemyDamageMult = 0.9` for all five types; enemyDB tabulate shows
  a single `Damage INC -10, source "Base"` — the actor-init conditional mod
  `Damage INC -10 { Condition: Intimidated }` (CalcSetup.lua:73-77, paired with
  `DamageTaken INC 10`), lit by the body-armour corrupted implicit "Enemies in
  your Presence are Intimidated" (flag reaches PoBR's enemy db but had no
  consumer). Note the Enfeeble curse payload is *not* the source — its MORE is
  gated `!Unique` and the Pinnacle enemy is Unique on both sides. Fix (all
  generic consumers): `setup_enemy` injects the Intimidated base pair (tag
  `EnemyIntimidated`); `env_finalize::bridge_enemy_condition_flags` bridges the
  enemy-db `Condition:Intimidated` FLAG into cfg `EnemyIntimidated` (vendor
  ModStore GetCondition semantics; allow-list = Intimidated only); orchestrator
  seeds `EnemyInPresence` true (CalcPerform.lua:524 default — PoBR does not
  model PresenceRadius vs enemyDistance). `EnemyDamageMult` was already wired
  as the max-hit end divisor (:3734-3771) and EHP damage-in multiplier — only
  the mod was missing. wolf-pack Phys/Fire/Cold/Light MaxHit 0.90x→1.00x,
  PhysDR 65.69→68.03 (oracle 68), TotalEHP 0.83x→(with the Mana fix) 1.00x.
- *wolf-pack Mana 761.2 vs 770 = The Adorned corrupted-magic-jewel scaling.*
  Oracle Int=139 vs PoBR 135: the six tree-socketed "Rallying Ruby of Valour"
  magic jewels (corrupted, enchant implicits +Int/+Dex/+chaos res) are scaled
  ×1.97 by The Adorned's "97% increased Effect of Jewel Socket Passive Skills
  containing Corrupted Magic Jewels" (CalcSetup.lua:944-948 sets the
  multiplier, :1342-1347 `ScaleAddList`, value = `trunc(round(v×scale, 2))`,
  ModStore.lua:70-79): +5 Int→9, +4 Dex→7. Fix: `Item.corrupted` field (item
  text `Corrupted` marker line, both parse paths) +
  `stage_inject_jewels` parses corrupted-magic-jewel texts and injects
  value-scaled modifiers (`adorned_corrupted_magic_jewel_inc` matches the
  two-physical-line wrapped text). Int 135→139 → Mana 770 exact; chaos resist
  uncapped 72→81 (capped 75) closes the ChaosMaxHit tail (17008 vs 17007).
  Not modelled: sinister/containJewelSocket socket exemptions, `unscalable`
  payloads (no corpus source).
- *titan Armour 0.985x = Runeforged base-armour display-line lag* (the #10-era
  "shield 1736 vs recomputed 1725" hypothesis was wrong — oracle
  `item.armourData.Armour = 1736` exactly, the shield was never the gap).
  Per-slot dump vs oracle `ArmourOn<Slot>`: gloves 96 vs 101, helmet 192 vs
  284, boots 58 vs 100 — three 0.5.4b-buffed Runeforged bases whose item-text
  `Armour:` lines lag the base DB (same family as the c705fe4 ES fix; helmet
  recompute 169×1.40×1.20=283.9→284 matches oracle exactly). Fix:
  `item_rolled_defence` now recomputes **all three** defences from the base DB
  when the base is known (vendor Item.lua:1994-1996, incl. `round()`), not
  just ES. titan Armour 38960→39546 exact, ES 54.67→55 exact, 5×MaxHit exact;
  fleet-wide the per-item `round()` also lands pathfinder Evasion
  0.98x→1.00x and twister DeflectChance 0.97x→1.00x exact.

Baselines: def 431→437 @5%, 439→445 @10% (the six wolf-pack cells);
off/dot/core-8 unchanged; 18-build per-cell diff shows no cell regressing.
titan TotalEHP 0.92x remains (separate residual, outside #13 scope).

**#14 — defensive 25-col long-tail triage. ✅ DONE (six clusters closed;
def 25-col 431→444 @5% / 439→444 @10%, core-8 142→144/144 = 100%).**

First full per-cell triage of the 19 cells outside the 5% band (never done
before). Full table at triage time (`~` = inside 10%, outside 5%; excluded =
wolf-pack ×6 + titan ×1, task #13's lane):

| build × col | ratio | root cause | vintage | status |
|---|---|---|---|---|
| druid-ember-fusillade PhysDR | 1.06x ~ | armour-DR **integer rounding**: vendor `<X>TakenHit` chain uses `calcs.armourReduction` = `round(armourReductionF)` (CalcDefence.lua:2402, Common.lua round = floor(x+0.5)); PoBR used the fractional variant everywhere. Golden PhysDR is always integer for this reason. | pre-existing (only visible at small DR) | ✅ fixed (699e9e3) |
| ranger-deadeye PhysDR | 1.20x | same | same | ✅ fixed (699e9e3) |
| abyssal-lich Life | 1.05x ~ | **Life→ES pool conversion** never deducted: 0_5-tree "Enhanced Barrier" (node 44299) gained `5% of Maximum Life Converted to Energy Shield` in 0.5.4b tree data; parser produced `LifeConvertToEnergyShield` BASE 5, defence matrix credited the ES side, but the pool-side `(1−conv/100)` (CalcDefence.lua:92) was explicitly deferred and never landed. Oracle: 1511×0.95×1.18 = 1693.83 vs golden 1694 exact. | 0.5.4b (node stats changed on 0_5 tree) | ✅ fixed (8b742d8) |
| abyssal-lich LifeUnres | 1.05x ~ | dependent (reservation is %-based) | — | ✅ same commit |
| essence-drain SpiritUnres | 0.86x (6 vs 7) | **Blasphemy per-curse rounding** drifted in 0.5.4b vendor: per-curse flat now folds into `baseFlat` *before* one efficiency-scaled round (`round(180/1.1)=164`, CalcDefence.lua:229-239); PoBR kept the old per-instance `round(60/1.1)=55×3=165`. Oracle `spiritReservedBreakdown` pins 164. | 0.5.4b (vendor code change) | ✅ fixed (fff5b2e) |
| gemling SpiritUnres | 3.33x (−60 vs −18) | **altQualityStats channel missing**: Gemling's "Advanced Thaumaturgy" (GemlingQuality flag) makes gems apply their alt quality stats (CalcTools.lua:147-152). Mirage Archer (alt `base_reservation_efficiency_+%` ×2 → 62% @q31) and Eternal Rage (alt `base_spirit_reservation_efficiency_+%` ×0.75 → 23%) reservation efficiencies were lost: 60→37, 100→81 (Δ42 = the whole gap). extract-lua gem-quality never transcribed `altQualityStats`. | pre-existing (data channel gap) | ✅ fixed (2896807) |
| smith Armour | 0.76x | **two dead provisioning inputs** (both mods parsed): ① `Multiplier:AllocatedConnectedNotable` — count of allocated tree.lua `applyToArmour=true` notables (12 exist, smith allocates 8 → +200×8 flat; CalcSetup.lua:840-841); the flag was absent from PoBR tree data. ② `StrRequirementsOn<slot>` PercentStat stats (CalcPerform.lua:1848-1857) — base-item attribute requirements absent from PoBR data (.dat bundle unavailable) → extended extract-bases vendor channel with `req = {str/dex/int}`. Oracle: Tree:9988 BASE 1600 + Tree:59589 152/120/161. Result 49096.16 vs 49096 exact. **Note**: the old "vendor↔PoBR tree skill mapping deadlock" memory was stale — `applyToArmour` is plain per-node vendor tree data, no mapping needed. | pre-existing | ✅ fixed (22e1b79) |
| smith PhysMaxHit / FireMaxHit / ColdMaxHit / LightMaxHit | 0.91-0.93x ~ | dependent on Armour | — | ✅ same commit (all 1.00x exact) |
| smith TotalEHP | 0.85→0.92x ~ | after Armour fix, residual = **EHP average block used 2 categories instead of 4**: vendor `EffectiveAverageBlockChance = (attack+projectile+spell+spellProjectile)/4` (CalcDefence.lua:1067) with `SpellProjectileBlock = max(spellBlock…, ProjectileBlock)` (:1013). Shield builds with 0 spell block: PoBR 13.65% vs vendor 20.475%. | pre-existing | ✅ fixed (3ab51dc), 99177 vs 99167.69 |
| titan TotalEHP | 0.92x ~ | same block-average root (shared consumption point; cell itself is #13's lane but the generic fix closes it) | pre-existing | ✅ same commit, 58898 vs 58904.30 |

Remaining after #14: the 6 wolf-pack cells (TotalEHP 0.83x, 4×ele/phys
MaxHit 0.90x, ChaosMaxHit 0.80x) — task #13's territory (per-type taken-mult
~10% uniform gap, see #12 notes).

Deliberate ceilings (marked in code): pool-conv applies to the whole
Maximum<res> BASE sum (vendor exempts `Extra<res>`; no dual-conversion build
in corpus); `reqMult` for attribute requirements held at 1 (no
reduced-requirements + armour-from-str build in corpus); alt quality stats
consumed only by the reservation-efficiency path so far (accessor is
general; offence-side alt stats wire up when a fixture demands).

**#15 — last three panel cells. ✅ DONE (off 78→80/80, dot 36→37/37 —
offensive and dot panels now full score alongside def 450/450).**

1. **gemling CritChance 1.20x → 1.00x (10.30 → 8.55 exact).** Root cause: the
   old 75a348e "keep inactive weapon-set nodes allocated for radius-jewel
   grants" mechanism was based on a misreading — in vendor, *every* mod on an
   `allocMode ≠ 0` node (the node's own stats **and** radius-jewel grants
   alike) gets `Condition: WeaponSet<N>` appended (CalcSetup.lua:222-223; the
   jewel-source branch :224-227 only fires for allocMode-0 nodes), so grants
   landing on inactive-set notables are dead. Oracle `critModList` Tabulate
   pins exactly one `7 @ Tree:32763` (INC total 71 = 14+10+7+10+20+10 →
   5 × 1.71 = 8.55). The 75a348e-era claim of 6×+7 matched the *old* golden
   (10.30); the 0.5.4b golden re-snap flipped it. Mechanism fully reverted
   (build.rs field + xml_build plumbing + collect.rs geometry merge). The same
   jewel's Small-node grants (crossbow damage) shrink too → gemling
   AvgDamage/TotalDPS 1.04x → 0.97x (still in band, sign flipped).
2. **gemling TotalDotDPS 1.10x → 0.95x (in band).** After the crit fix the
   residual decomposes exactly: ignite chance 0.0962 vs oracle 0.098636
   (0.9754) × stack potential 0.2603 vs 0.26691 (0.9752); both scale with the
   hit's fire damage, and 0.9754² ≈ 0.951 matches the dot ratio bit-for-bit.
   I.e. the dot residual is purely downstream of the pre-existing AvgDamage
   0.97x under-estimate (previously masked by the wrong 6× jewel grant); no
   independent dot-side gap.
3. **wolf-pack Speed 1.43x → 1.00x.** Two stacked causes: ① main-skill
   selection — Wolf Pack (`Minion`+`Companion`, neither Attack nor Spell)
   failed PoBR's is-damage-skill filter, so the designated `mainSocketGroup=4`
   yielded nothing and the fallback scan hijacked the main skill to the
   Blasphemy group's Temporal Chains (0.7 s cast → Speed 1.43). Vendor's
   `socketGroupSkillList` has **no** damage filter — fix: after
   `pick_group_main_skill` misses on the *designated* group, fall back to the
   `mainActiveSkill`-chosen active gem (`pick_group_chosen_active`;
   cross-group scan still damage-only). ② speed bucket — with Wolf Pack as
   main (castTime 1 s), PoBR's "untyped skill eats all three speed buckets"
   fallback consumed the weapon's `12% reduced Attack Speed` → 0.88. Vendor
   mod-matching requires the cfg to carry ModFlag.Attack/Cast for those mods
   to apply; untyped skills eat neither → `speed_names_for` now returns only
   `SkillSpeed` for non-attack/non-spell skills. Oracle mainOutput pins
   Speed = 1, Time = 1, CastRate = 1 (statSetLabel "Minion Info", player
   TotalDPS 0).

18-build per-cell diff after #15: every offensive/dot/defensive cell is a
hit (450/450, 144/144, 80/80, 37/37) — zero regressions by construction.
Panel-mode offensive ratchet re-measured 42/43 → 45/47 (@5%/@10%).

## Tooling

- `examples/demo-bd-test/tools/recapture_golden.py` — refresh fixture goldens
  against the currently vendored PoB2 (run after any vendor bump).
- `tools/pob2-oracle/run.sh <decoded.xml> out.json` — per-build 0.5.4b breakdown
  (`mainOutput` scalars + `intermediates` / `components` / `conversionTable`).
  `enemyMitigation.dealtMods` lists enemy outgoing-Damage scalers (Enfeeble /
  Intimidated-style feeds of `<X>EnemyDamageMult`); `ORACLE_DUMP_ITEMS="2,17"`
  dumps listed build items' parsed modLines + evaluated modList + armourData
  (provenance diffs for jewel-effect scaling / item defence recompute).
