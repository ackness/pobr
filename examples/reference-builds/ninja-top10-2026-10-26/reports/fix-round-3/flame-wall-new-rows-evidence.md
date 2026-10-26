# Bounded attribution: three newly outside rows

Calculation revision: 2e45c66e. Read-only analysis of the two requested builds; no CLI, oracle execution, tests, builds, triage, holdout inspection, or source edits. This cache note is the only write. No registry classification changed.

## Sources

- Native after: `accuracy-round3-2026-10-26/after-builds/{gemling-legionnaire-08,shaman-08}.json`.
- Native before: `accuracy-round2-2026-10-26/quality-builds/{gemling-legionnaire-08,shaman-08}.json`.
- Cached oracle: `pob2-oracle/2cb238bed7e79480-e741db377c.json` and `pob2-oracle/9ccdc21291929d05-6dd50b5f34.json` (paths relative to parent `.cache`).
- Frozen XML decoded in memory from each build's `examples/reference-builds/ninja-top10-2026-10-26/builds/<slug>/code.txt`.
- Game snapshot `data/0.5.5.4.2/base/granted_effect_stat_sets.json`: FlameWallProjectileBuffPlayer L34 adds 71..107 fire; L15 adds 18..26. Both frozen builds explicitly configure flameWallAddedDamage=true.

## gemling-legionnaire-08 — AverageBurstDamage

Native 1,583,049.371746979; saved 1,396,978.10265; pinned 1,669,179.445. Native is +13.3196% versus saved, -5.1600% versus pinned. **Do not mark the entire row explained/known: a concrete native duplicate-buff defect remains.**

Flame Wall is correctly present at effective L34 Q22; the weaker L19 Q45 copy does not stack. Added fire 71..107 agrees with the game table and oracle. Spark's native lightning source is 103..1949; Archmage gain-as-lightning is 408%. Thus post-gain lightning is 812.92..10337.48 = 5.08*(103..1949) + 4.08*(71..107). After damage modifiers, noncritical fire average is 1298.154 and lightning average 82769.4192. Exact native burst formula:

`(1298.154 + 82769.4192) * (1 + 0.7551 * (16.301 - 1)) * 1.5 * 1 = 1583049.371746979`

The 1.5 is enemy hit mitigation (Rakiata's Flow resistance inversion); burst hit count is one, not 1.5. Before Flame Wall, cached TotalHitAvg was 971393.558494543 versus 1055366.24783132 after; the corresponding burst increase is 125959.0340051644. The old row being within tolerance did not establish correctness.

**Concrete engine cause:** `crates/pobr-core/src/skill_env/buff_specs.rs:403` deduplicates ordinary buffs with `seen.insert(skill_id)` before the strongest-copy merger can see later copies. Cached native usage contains enabled MetaCastOnCritPlayer L22 Q22 and L24 Q50. Native CritChance breakdown applies only `buff.MetaCastOnCritPlayer.cast_on_crit_global_critical_hit_chance_granted_+%` = 22 INC; cached oracle applies 50 INC. The existing quality map defines this alternate-quality stat at 1 per quality; raw effect metadata permits multiple ongoing instances. Existing `calc/buff_pass.rs` defines same-name strongest-modifier selection. This is first-copy suppression, independent of Flame Wall's own duplicate handling. No source edit or new execution was performed.

Native crit INC 739 becomes 767 if only the missing 28 INC is corrected: base 9% gives 78.03% rather than 75.51%. Arithmetic-only counterfactual burst is **1631672.1897857338**, an increase of 48622.818038755. This is not a measured patched result.

The remaining pinned differences are separately evidenced: frozen helmet Soul Core text gives 3% maximum Mana, while oracle `extraModList.INC.Mana` rebuilds Item:27 Soul Shelter as 5%. Native Mana=3905*1.75=6833.75; oracle Mana=6912 from 3905*1.77 (6911.85 before rounding). Crossing one 100-Mana step raises oracle Archmage gain-as from 408% to 414%, generic increased damage from 988% to 997% (Rathpith +6, passive +3), and crit INC by another 3 (Rathpith), giving oracle total crit INC770 / 78.3%. Preserve frozen rune text. Oracle also rounds hit endpoints and its effective crit multiplier: `(1962.75 + 126634.5) * (1 + .783 * (16.3 - 1)) = 1669179.445275`, matching cached 1669179.445. Native retains 16.301.

The older saved total is not fully reconstructed here; its exact historical input/rounding residual remains unresolved. Do not treat the row as wholly a reference-version difference or wholly a Flame Wall regression.

## shaman-08 — IgniteDPS and TotalDotDPS

Both native rows are **34.789730567**, saved/pinned both zero. Native Flame Wall effective L15 Q0 correctly adds 18..26 fire to Spark, matching game data and cached oracle. This is recipient hit fire damage; the implementation does not inject Flame Wall's lingering ignite.

The cached native ignite breakdown reproduces the result exactly:

`3228.74251572 * 0.20 * 0.5 * 0.10775009279189803 = 34.789730566997626`

Here source fire hit average is 366.366, critical average 3357.378024; conditional ignite critical share is 95.6992647556%, yielding source 3228.74251572. Application chance is 0.6969973% (not 69.69973%), duration 4.7619047619s, and hit rate 3.246428571/s. Existing active-stack formula is `0.006969973 * 4.7619047619 * 3.246428571 = 0.10775009279`. TotalDotDPS is solely this ignite. These are cached engine assumptions; this analysis adds no event history, uptime, or new rate.

**Pinned zero has a concrete oracle cause:** its cache reports positive fire, IgniteChance=0.7039954672%, IgniteDuration=4.819277108, Speed=3.285714286 and DpsMultiplier=1, but IgniteStackPotential=0. The only cached totem-limit modifier is `extraModList.OVERRIDE.ActiveTotemLimit = 2`, from SupportMetaTotemSpellTotemPlayer; no BASE totem limit is present. Vendor `CalcOffence.lua:1937-1938` correctly includes Override when displaying ActiveTotemLimit/TotemsSummoned=2. However, its ailment-stack calculation at `CalcOffence.lua:5264-5266` uses only `Override(TotemsSummoned)` or `Sum(BASE, ActiveTotemLimit, ActiveBallistaLimit)`, omitting the skill limit Override. There is no explicit TotemsSummoned config in the frozen XML. Thus it multiplies positive stack potential by zero. Vendor `Data/Skills/act_str.lua:18295` supplies precisely that OVERRIDE2 support limit. Zero is therefore not evidence that Flame Wall recipient fire cannot ignite.

No concrete native Flame Wall defect is established for these two rows. The exact game-correct numeric steady-state ignite under a chosen simultaneous-totem scenario is not established by this bounded audit: native uses its existing per-skill hit-rate stack convention, while the oracle zero is defective. Do not infer a new active-totem count or broaden a known classification from this note.
