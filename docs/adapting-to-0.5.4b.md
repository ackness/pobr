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

Remaining elsewhere, re-triage against fresh `defenceModList` dumps:
ailment magnitude (#5) and the wolf-pack EHP remainder decomposed above
(Armour 0.98x / ChaosMaxHit 0.87x / Life 1.11x). Also ritualist TotalEHP
moved 1.04x→1.10x with #4a (低血 EHP 口径——vendor 只在显式
`conditionLowLife` config 下 cap `LifeRecoverable`，PoBR 的 EHP 消费端
尚未对齐该分支). Each is its own oracle-guided investigation; not all are
single formula constants (several are unmodeled unique/flask interactions).
Also pending from the vendor bump itself: `check-buff-refs` reports 15
`vendor_ref` line-hash drifts in `data/overlay-common/buff_definitions.json`
(e.g. OnslaughtFlask/ShapersPresence/UnholyMight) — the 4.5.4.3 upgrade swapped
the vendor without the manual buff re-review; each drifted buff needs its
vendor lines re-checked for semantic changes, then `--write` to refresh.

## Tooling

- `examples/demo-bd-test/tools/recapture_golden.py` — refresh fixture goldens
  against the currently vendored PoB2 (run after any vendor bump).
- `tools/pob2-oracle/run.sh <decoded.xml> out.json` — per-build 0.5.4b breakdown
  (`mainOutput` scalars + `intermediates` / `components` / `conversionTable`).
