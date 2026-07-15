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

**#1 target — Mageblood (common root cause, highest leverage).** Diagnosed via
`defenceModList`: the armour gap on `warrior-titan-shield-wall` (`Mageblood` INC
+219 Armour) and the evasion gap on `ranger-pathfinder-ice-shot` (`Mageblood`
BASE +2000, INC +150 Evasion) are the *same* mechanic. Every poe.ninja endgame
fixture wears Mageblood, which keeps all equipped **magic utility flasks**
permanently active (granite→armour, jade→evasion, ruby/sapphire/topaz→res, …).
PoBR explicitly does not model this (`env_finalize.rs:214-218`: Mageblood
`CalcPerform.lua:1387-1403` + the `MagicUtilityFlaskEffect` rarity channel are
declared unimplemented). Implementing it closes a large slice of the armour /
evasion / resistance gaps across multiple builds at once — do this first.

Remaining after Mageblood, re-triage against fresh `defenceModList` dumps:
DeflectionRating scaling, offence per-build DPS clusters, ailment magnitude.
Each is its own oracle-guided investigation; not all are single formula
constants (several are unmodeled unique/flask interactions).

## Tooling

- `examples/demo-bd-test/tools/recapture_golden.py` — refresh fixture goldens
  against the currently vendored PoB2 (run after any vendor bump).
- `tools/pob2-oracle/run.sh <decoded.xml> out.json` — per-build 0.5.4b breakdown
  (`mainOutput` scalars + `intermediates` / `components` / `conversionTable`).
