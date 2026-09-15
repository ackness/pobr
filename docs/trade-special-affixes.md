# Special affixes in weighted market searches

## Sources

The implementation follows the pinned PoB2 commit
`ce566eac45ea8a86477f513c7ee65a1ebe60014e` for data version `4.5.5.2`:

- `src/Classes/TradeQueryGenerator.lua`: `InitMods`, `ProcessMod`,
  `canModSpawnForItemCategory`, and `GenerateModWeights`.
- `src/Data/ModItem.lua`, `ModVeiled.lua` (desecrated), `ModCorrupted.lua`,
  `ModJewel.lua`, `Essence.lua`, and `TradeSiteStats.lua`.

PoB2 supplements ordinary spawn weights with category overrides for perfect
essences, Genesis Tree crafts, and rune influences. Its current initialization
does not supply Alloy overrides. The game's same-version `EssenceMods`,
`Essences`, and `EssenceTargetItemCategories` tables provide these recipes,
including the actual alternatives in `OutcomeMods`. They also cover corrupted
essences. The tables are fetched through the existing GGG data pipeline;
`trade_crafting_sources.json` retains only modifier IDs, source kinds and item
classes. Recipe categories take precedence over the PoB2 essence fallback.

For example, `AlloyCastSpeedDamageAsExtraColdHybridOneHand1` belongs to Wands
and Foci; the stronger `AlloyCastSpeedDamageAsExtraColdHybrid1` belongs to
Staves. Their zero ordinary spawn weights must not exclude them from market
searches. Guessing eligibility from modifier names or granting them to every
weapon would produce incorrect categories.

## Search and score semantics

- `mods` remains the ordinary affix pool. `search_mods` adds alloy, essence,
  desecrated, breach, influence and corrupted sources with explicit category
  restrictions. Older catalogs without this optional field still work.
- Each official trade hash receives one probe and one query coefficient.
  Multiple hashes within a hybrid affix are independent searchable stats.
  Multiple exported lines within one hash are joined and scored once;
  `source_lines` also matches wrapped current-item exports.
- The scalar value follows PoB2: one placeholder uses that roll, two use their
  average, and a stat without placeholders is a one-unit presence flag. Fixed
  numbers in conditional descriptions are not included in the average.
  `value_indices` records these positions. More than two variable placeholders
  and inconsistent numeric templates are excluded instead of guessed.
- `trade_line` preserves official wording when it differs from the PoB export,
  including inverse wording/signs. Both spellings match the same ID and value.
- Crafted, fractured and desecrated explicit modifiers use `explicit.stat_*`,
  as in PoB2. Source-specific duplicates would count the same effect twice.
  Corruption uses `enchant.stat_*`; rune augments and ordinary implicits remain
  separate. Allocated-passive enchants use other trade IDs and are excluded.
- Removing an existing compound stat removes all its lines. Removing a counted
  enchant updates `Implicits:` so the next explicit modifier remains explicit.
- Current-item minimums still use the exact final query coefficients, including
  rounding and the 32-filter limit. Ambiguous or unknown relevant lines leave
  the score partial and omit the minimum filter.

PoB2 intentionally leaves its PoE2 pseudo-stat map empty: pseudo stats can mix
socket augment contributions into the query. This implementation also uses
actual namespaced hashes instead of replacing hybrids with pseudo totals.

## Calculation boundaries

Special search effects participate in the existing blank/current-item marginal
probes. Changed unsupported-effect diagnostics reject a probe, including a
removal that would otherwise unblock injection of the remaining item. This
prevents an unmodeled part of a compound effect from producing an inflated
coefficient. A parsed effect still depends on the engine's consumers and active
configuration; a zero contribution does not establish that the effect is useless.

The fixed vendor has no modifier text/hash records for `AlloyPuppetMasterChance1`
and `EssenceGrantedPassive`. Extraction reports and records these missing IDs in
`_meta.unmapped_crafting_mods`; it does not invent search IDs or gains.

Reference combination search and the manual affix editor continue to use
ordinary, base-compatible prefix/suffix pools. The special-source catalog is
not a crafting recipe simulator: it does not establish that several crafts can
coexist, their cost, probability or an optimal market purchase. Complete listed
items still need whole-build replacement comparison.

## Regeneration and checks

From the repository root, after exporting the same-version GGG tables:

```sh
python3 pipeline/extract-trade-crafting.py pipeline/tables/English data/4.5.5.2/overlay/trade_crafting_sources.json
luajit pipeline/extract-trade-catalog.lua vendor/PathOfBuilding-PoE2/src data/4.5.5.2/overlay/trade_catalog.json data/4.5.5.2/overlay/trade_crafting_sources.json
luajit pipeline/extract-trade-map.lua vendor/PathOfBuilding-PoE2/src data/4.5.5.2/overlay/trade_stat_map.json
python3 pipeline/test-trade-crafting.py
luajit pipeline/test-trade-stats.lua
pnpm --dir web test src/lib/tradeCatalog.test.ts src/lib/equipmentScore.test.ts src/lib/tradeOptimizer.test.ts
pnpm --dir web sync-data
pnpm --dir web build
pnpm --dir web exec playwright test e2e/trade-upgrades.spec.ts --grep 'alloy hybrid|imported PoB armour' --workers=1
```

`pipeline/regen-all.sh` runs both extraction stages in dependency order. The
catalog's optional third argument supplies the committed crafting sources when
regenerating from a checkout without raw GGG tables; omitting it gives only the
PoB2 source coverage.
