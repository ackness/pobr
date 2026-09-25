# Calcs display comparison

Source review: 2026-09-25. PoB2 reference:
`ce566eac45ea8a86477f513c7ee65a1ebe60014e`; PoBR active data: `4.5.5.2`.

This is a **source/contract comparison, not a completed GUI walkthrough**.
No running PoB2 desktop UI was inspected. TODO 4.3 remains open for the
same-build visual check below; passing calculation tests cannot replace it.

## Observable differences

| Area | PoB2 | PoBR | Comparison rule |
| --- | --- | --- | --- |
| Selected skill | Calcs has its own group, active skill and part selections (`CalcsTab.lua:42–101`); MAIN and CALCS environments are built separately (`:494–512`). | Calcs uses the session's selected main group; clicking a per-group DPS row changes the main skill. | Align group, active ordinal, stat set, weapon set and part before comparing. The imported `statSetIndexCalcs` is not a separate PoBR selection. |
| Buff mode | Four Calcs modes: UNBUFFED, BUFFED, COMBAT, EFFECTIVE (`CalcsTab.lua:12–16,165–166`). | Calculation uses the session's configuration flags rather than a separate Calcs-only mode. | Match `mode_buffs`, `mode_combat`, `mode_effective` and enemy inputs, not just the visible skill name. |
| Actor | Calcs can select minion stats (`CalcsTab.lua:101–103,434`). | The panel uses the returned display catalog; it has no equivalent independent actor selector. | A displayed minion summary is not proof of full per-minion actor parity. |
| Breakdown | Formula-specific sections and breakdowns (`Modules/CalcSections.lua`). | `calculate.rs::BREAKDOWN_MOD_NAMES` exposes a fixed list of modifier buckets. `breakdown_for` sums raw BASE/INC values and lists raw modifiers; `CalculationSession::mods_named` filters by name, not active calculation conditions. | These lists are **not** a replayable evaluated formula. Conditional modifiers, per-stat scaling, caps, conversions and derived values can prevent raw totals from reconstructing final output. |
| MORE multipliers | Formula-specific multiplier breakdown. | Individual MORE rows are listed; the summary exposes only BASE and INC totals, not the evaluated MORE product. | Do not sum MORE rows or infer an effective product from raw rows alone. |
| Damage basis | Hit, DoT and combined fields have distinct definitions. | `main_skill.hit_dps = output.dps`, `dot_dps = total_dot_dps`, `combined_dps = combined_dps`; hit components are non-crit, player-side, before enemy mitigation. | `TotalDPS` is hit DPS, not automatically combined or all-skill DPS. Compare like-for-like fields. |
| Per-group DPS | Main-skill and Full DPS configuration have different purposes. | The panel recalculates groups and lets the player select one. Weapon-set orchestration is described in `CLAUDE.md`. | Group rows do not establish a sustainable rotation or justify summing all displayed damage. |
| Attribution | Modifier/formula breakdown is not the same as source removal. | `analysis.rs::attribution_impl` recomputes after removing equipment, disabling skill groups and removing utility slots, returning baseline minus variant. | These are removal marginals, not additive shares or a complete passive/jewel attribution census. Nonlinear interactions mean percentages need not sum to 100%. |
| Formatting | Field-specific presentation. | `statDisplay.ts` uses field formats in the sidebar; Calcs bucket values use `float2`, rounding values ≥1000 to integers. Raw row values show at most two decimals. Null/nonfinite values render as infinity. | Compare unrounded JSON before treating a displayed rounding or missing-value difference as a formula bug. |

PoBR source anchors:

- `web/src/components/calcs/CalcsPanel.tsx`: bucket rendering, group selection,
  removal-marginal display and stale-result handling.
- `apps/pobr-wasm/src/build_api/calculate.rs`: breakdown fields, whitelist and
  main-skill damage basis.
- `apps/pobr-wasm/src/build_api/analysis.rs`: removal variants and delta sign.
- `crates/pobr-core/src/calc/session.rs::mods_named`: raw bucket membership.
- `crates/pobr-build/src/build.rs::GemSkillRef`: imported stat-set boundary.
- `web/src/lib/statDisplay.ts`: formatting and null handling.

## Remaining GUI acceptance

Use one committed attack build, one spell/DoT build and one minion build with
the same supported data/vendor version. Record the exact fixture, configuration,
weapon set, selected group/ordinal/stat set and buff mode. Capture both Calcs
screens and unrounded corresponding output fields. Check Life/Mana/ES,
resistances, hit chance, crit, action rate, hit DPS, DoT and combined DPS;
expand one conditional modifier and one MORE modifier. Classify each difference
as selection, actor, formula support, raw-versus-evaluated breakdown or display
rounding. Only after this visual evidence exists should TODO 4.3 be checked off.
