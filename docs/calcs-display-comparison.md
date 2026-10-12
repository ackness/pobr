# Calcs display comparison

Source review: 2026-09-25. PoB2 reference:
`ce566eac45ea8a86477f513c7ee65a1ebe60014e`; PoBR active data: `4.5.5.2`.

This is a **source/contract comparison, not a completed GUI walkthrough**.
No running PoB2 desktop UI was inspected. The same-build visual check below
remains open; passing calculation tests cannot replace it.

## Observable differences

| Area | PoB2 | PoBR | Comparison rule |
| --- | --- | --- | --- |
| Selected skill | Calcs has its own group, active skill and part selections (`CalcsTab.lua:42–101`); MAIN and CALCS environments are built separately (`:494–512`). | Calcs uses the session's selected main group; clicking a per-group DPS row changes the main skill. | Align group, active ordinal, stat set, weapon set and part before comparing. The imported `statSetIndexCalcs` is not a separate PoBR selection. |
| Buff mode | Four Calcs modes: UNBUFFED, BUFFED, COMBAT, EFFECTIVE (`CalcsTab.lua:12–16,165–166`). | Calculation uses the session's configuration flags rather than a separate Calcs-only mode. | Match `mode_buffs`, `mode_combat`, `mode_effective` and enemy inputs, not just the visible skill name. |
| Actor | Calcs can select minion stats (`CalcsTab.lua:101–103,434`). | The panel uses the returned display catalog; it has no equivalent independent actor selector. | A displayed minion summary is not proof of full per-minion actor parity. |
| Breakdown | Formula-specific sections and breakdowns (`Modules/CalcSections.lua`). | `calculate.rs::BREAKDOWN_MOD_NAMES` exposes a fixed list of displayed stats, keyed by display stat id. `CalculationSession::stat_mod_names` maps each stat to the modifier names its calculation stage aggregates (Life and Mana read `MaximumLife` / `MaximumMana`; a resistance reads both spellings plus the shared `ElementalResist`; `ActionRate` reads the speed names the inspected skill uses; crit reads `CriticalStrikeChance` / `CriticalStrikeMultiplier`; Armour, Evasion and Energy Shield read their own name plus the shared `ArmourAndEvasion` / `Defences`). `breakdown_for` lists every stored modifier of that name set through `CalculationSession::evaluated_stat_mods`: each row has `active` and, for an active BASE/INC/MORE row, the applied value (per-stat scaling and shared caps included). `base_total`, `inc_total` and `more_total` come from one `sum` / `more` query over the whole name set and count active rows only. The context is the session config; `ActionRate`, `CritChance` and `CritMultiplier` use the main-hand pass config, so the weapon's own lines count as they do in the calculation. Each breakdown also has `formula`: the steps from the starting value to the displayed stat (`base`, `flat`, `enemy_flat`, `converted`, `increased`, `more`, `added_time`, `action_speed`, `override`, `cap`, `cooldown_cap`, `server_tick_cap`, `floor`, `bonus_to_multiplier`; for the defences and Spirit `gear_adjusted`, `gained`, `slot_scaled`, `flat_after`, `round`; for crit `hit_chance`, `lucky`, `bifurcate`, `inevitable`, `inevitable_penalty`, `bifurcate_bonus`, `enemy_increased`), each with its operand and running result. The steps come from the terms the stage itself aggregates (`offence::pool_terms`, `resolve_resistance`, `action_rate_terms`, `crit::crit_chance_terms` / `adjust_crit_chance` / `crit_multiplier_terms`, `defence::defence_resource_terms`, `defence_panels::spirit_terms`) through `CalculationSession::stat_formula`, which returns them only when they end at the displayed value. A stat with a formula gets a breakdown even without modifier lines (a weapon's base crit chance, a skill's base rate). | `formula` covers Life, Mana, the four resistances, Energy Shield, Armour, Evasion, Spirit, `ActionRate`, `CritChance` and `CritMultiplier`; on the 18 fixture builds every one of these that has a breakdown has a formula. A defence formula shows the equipped items' values inside the `flat` step and the extra from increases scoped to one item slot as one `slot_scaled` amount, not per slot. `formula` is `null` for Accuracy and Movement Speed (no display stat to end at), and for values the steps do not model: an enemy-side increase to crit chance, `NeverCrit` / `NoCritMultiplier`, or two hands with different values combined. For those the page shows the BASE/INC/MORE totals with a note that further effects can apply. An off-hand pass can match weapon-specific rows differently. |
| MORE multipliers | Formula-specific multiplier breakdown. | Individual MORE rows are listed; `more_total` is the evaluated product of the active rows over the stat's name set. | Factors the formula multiplies in separately (for example action speed on top of attack/cast speed) are not part of `more_total`; where a `formula` exists they are their own step. |
| Damage basis | Hit, DoT and combined fields have distinct definitions. | `main_skill.hit_dps = output.dps`, `dot_dps = total_dot_dps`, `combined_dps = combined_dps`; hit components are non-crit, player-side, before enemy mitigation. | `TotalDPS` is hit DPS, not automatically combined or all-skill DPS. Compare like-for-like fields. |
| Independent skill metrics | Main-skill and Full DPS configuration have different purposes. | The panel evaluates every active effect, including support-granted effects, without requiring a main selection. Selecting a row chooses that exact effect for detailed inspection. Weapon-set orchestration is described in `docs/web-contracts.md`. | Rows do not establish a sustainable rotation or justify summing all displayed damage. Utility zeroes remain visible; unavailable metrics are null with diagnostics, not zero DPS. |
| Attribution | Modifier/formula breakdown is not the same as source removal. | `analysis.rs::attribution_impl` recomputes after removing equipment, disabling skill groups and removing utility slots, returning baseline minus variant. | These are removal marginals, not additive shares or a complete passive/jewel attribution census. Nonlinear interactions mean percentages need not sum to 100%. |
| Formatting | Field-specific presentation. | `statDisplay.ts` uses field formats in the sidebar; Calcs bucket values use `float2`, rounding values ≥1000 to integers. Raw row values show at most two decimals. Null/nonfinite values render as infinity. | Compare unrounded JSON before treating a displayed rounding or missing-value difference as a formula bug. |

Socket-group attribution labels list the group's saved gems in source order,
using localized names and retained source names for unresolved slots. The variant
removes the entire group, not just its first gem or selected active gem. The label
does not assert that every listed gem is enabled or contributes to the result.

PoBR source anchors:

- `web/src/components/calcs/CalcsPanel.tsx`: bucket rendering, group selection,
  removal-marginal display and stale-result handling.
- `apps/pobr-wasm/src/build_api/calculate.rs`: breakdown fields, whitelist and
  main-skill damage basis.
- `apps/pobr-wasm/src/build_api/analysis.rs`: removal variants and delta sign.
- `crates/pobr-core/src/calc/session.rs::evaluated_mods_named` / `named_totals`: bucket rows as
  the calculation context treats them, and the matching totals.
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
rounding. Only this visual evidence closes the walkthrough.
