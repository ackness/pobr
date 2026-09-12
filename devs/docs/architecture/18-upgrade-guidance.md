# Upgrade guidance: scoring, replacement checks, and bounded planning

The Upgrade page joins equipment, skill/support, and passive planning around the
current build, main skill, active weapon set, and selected objective. Suggestions
are evaluated alternatives, not a claim that the entire market or passive tree
has been solved globally.

## Decision contract

The balanced objective is `sqrt(DPS) * sqrt(EHP)`, giving equal importance to
relative changes in either quantity. The optional EHP floor is a hard constraint;
optional elemental resistance targets are prioritized lexicographically before
the scalar objective. A balanced improvement can still decrease DPS, so complete
replacement results show both quantities and explicitly identify a DPS decrease.

An equipment purchase follows three steps:

1. Generate affix weights and show the equipped item's score under the exact same
   official query. Use this Sum to screen candidates within the market budget.
2. Copy one complete candidate item and recalculate the build with that position
   replaced. Check DPS, EHP, resistance changes, requirements, and unsupported
   effects before deciding whether it satisfies the objective.
3. Apply the evaluated item to the build. Recalculate subsequent suggestions
   because interactions and the baseline have changed.

The visible paste card automatically validates the item base and required level,
then compares every compatible equipped position and allocated jewel socket in
one batch. It selects the best position under the shared objective, while users
can inspect another position before applying. Attribute and special equip
requirements still need manual verification; the calculation API itself accepts
permissive equipment snapshots. See
[calculation performance and copied items](19-calculation-performance-and-item-paste.md)
for the v0.0.14 performance measurements, new affix coverage and diagnostic rules.

The market Sum is a linear explicit-affix score. It does not include every base
property, implicit, augment, conditional effect, or interaction. EHP floors and
resistance priorities used by local planners are not equivalent to constraints
on the official market's Sum. Budget filters actual listings; hypothetical
reference combinations have no price and cannot prove budget optimality.

## Equipment scoring

Implementation: [tradeOptimizer.ts](../../../web/src/lib/tradeOptimizer.ts),
[equipmentScore.ts](../../../web/src/lib/equipmentScore.ts), and
[trade.ts](../../../web/src/lib/trade.ts).

`set_items` already replaced the selected equipment slot in v0.0.11. The scoring
problem was not a second equipped item accidentally remaining in that slot.
The old weighting formula averaged the gains from adding one full reference
roll to a blank item and adding another roll to the equipped item. Repeated
addition can undervalue an existing affix when accuracy or resistance is near a
cap. The old minimum Sum was a different quantity: half the scaled objective
difference between the current and blank items.

The revised estimator keeps the blank-item probe. For an already mapped current
affix, its second probe removes that affix and measures the realized contribution
per unit. An absent affix still uses addition to the current context. Probe items
retain quality; rolled Armour/Evasion/ES/Spirit/Ward totals are removed before
changing local affixes, because those totals already include the affixes being
varied. Complete reference combinations are evaluated as whole replacements,
respecting base eligibility, mod groups, and prefix/suffix limits. Combinations introducing a new unsupported effect are excluded from recommendations; existing baseline diagnostics remain visible.

`tradeQueryWeights` is shared by the query and equipped-item scorer: it removes
invalid/zero weights, deduplicates stat IDs, rounds coefficients to three decimal
places, and keeps at most 32 filters. `currentItemScore` only includes explicit
stats in that final query. Known effects omitted from the query contribute zero.
Implicit/augment/base properties remain outside that explicit Sum. Capitalization
differences between game and catalog wording do not change matching. Unknown or
ambiguous mappings and unresolved roll/variant text produce a partial score;
partial scores do not set a minimum filter. Complete scores can supply the
current-item minimum directly, without an arbitrary 50% conversion.

`scoreWarnings` reports partial coverage, detected nonlinear marginals,
base-dependent categories, and local constraints that are not represented by
the query. Even a fully mapped Sum is not a replacement DPS prediction. Removing
an affix is a local approximation, not a cure for multiplicative interactions.

## Manual affix simulation

The complete-item comparison includes a structured prefix/suffix editor backed
by all eligible tiers in the pinned PoB2 catalog. `roll_lines` retains each raw
numeric range without changing maximum-roll trade weights. Item level, base
spawn weights, affix groups, prefix/suffix capacity and character level constrain
the choices. Minimum, midpoint and maximum rolls can be previewed; the midpoint
is a range estimate, not a spawn-weighted crafting expectation or market price.

Only uniquely identified explicit affixes receive an editable tier. Hybrid and
merged stat lines can hide multiple affixes; conservative ambiguity detection
leaves these unresolved and never infers a free slot from their line count.
Unknown lines remain visible until explicitly replaced or removed. Corrupted,
twice-corrupted, sanctified, mirrored and unidentified items cannot use ordinary
affix simulation; fractured affixes are locked.

Draft edits invalidate Apply until the player recalculates the complete item.
The chosen destination slot and augment setup survive this recalculation.
Unchanged implicits, rune effects, quality and metadata are retained; stale local
defence and weapon property totals are removed so the engine derives them again.
Real-WASM browser tests verify local armour/evasion and bow DPS stay unchanged
when only an unrelated life affix changes, including after application.

## Real WASM comparison

The opt-in [upgradeBench.test.ts](../../../web/src/lib/upgradeBench.test.ts) uses
the generated WASM engine, not mocked DPS/EHP values. It stages the current web
data manifest and bridges `optimizeVariantsJson` into the existing evaluator.

```sh
cd web
POBR_UPGRADE_BENCH=1 pnpm exec vitest run src/lib/upgradeBench.test.ts
```

Ordinary unit-test runs skip this experiment. It requires existing WASM and
synchronized web data; it does not invoke Cargo or download player builds. Its
report is written to the ignored `.cache/upgrade-algorithm-bench.json` file
at repository root.

The recorded run used schema 4 and game data `4.5.5.2`. Each synthetic level-85
build has eight fixed real affixes selected from the catalog before observing
results. Every legal subset is generated on one fixed base at its available
maximum roll: 225 complete candidates per build. Both estimators use that same
reduced pool. The old estimator reproduces v0.0.11's addition formula, positive
weight selection, scale, and final query rounding; it is not a replay of the
entire old UI or a historical market snapshot.

Candidate legality in this experiment means base/domain eligibility and affix
groups/counts. The synthetic build inputs are calculation fixtures, not exported
playable characters; the experiment does not validate item/skill attribute
requirements, market availability, or price.

| Build | Current DPS | Current EHP | Old/new/oracle Top1 DPS | Old/new/oracle Top1 EHP | Top1 objective regret |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ice Shot, Twin Bow | 445.64 | 2529.63 | 627.67 | 2529.63 | 0% / 0% / 0% |
| Fireball, Sapphire Ring | 129.87 | 3697.37 | 157.89 | 3697.37 | 0% / 0% / 0% |

Top1 regret is `(best evaluated objective - selected objective) / best objective`.
The exhaustive oracle selects from the same 225 candidates. It proves the best
candidate only within that finite experiment.

| Build | Old misordered pairs | New misordered pairs | Comparable pairs | Higher Sum but worse objective, old → new | Higher Sum but lower DPS, old → new |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ice Shot, Twin Bow | 2060 | 1672 | 25200 | 35 → 11 | 35 → 11 |
| Fireball, Sapphire Ring | 553 | 564 | 24979 | 4 → 2 | 2 → 2 |

Pairs tied in either predicted or true objective are excluded from inversion
counts. Higher-Sum counts compare each candidate with the equipped item under
the same estimator. The bow ranking improves in this sample. The spell ranking
slightly worsens overall, despite fewer higher-Sum objective downgrades. Both
estimators still admit false upgrades. These results support retaining the
complete-replacement check, not claiming that removal marginals always beat
addition probes. The Fireball run assigns zero weight to added attack fire
damage; neither fixture produced unsupported-effect diagnostics.

Each estimator performs 18 affix-context evaluations per build. The exhaustive
oracle performs 225 whole replacements. One sequential run observed roughly
101/28/262 ms for old/new/oracle on the bow and 24/21/220 ms on the ring, after
about 668 ms of initialization. These are diagnostic observations, not a speed
comparison: ordering, parser caches, and JIT warmup differ between methods.

The deterministic synthetic regressions in
[tradeOptimizer.test.ts](../../../web/src/lib/tradeOptimizer.test.ts) expose
two additional boundaries:

| Synthetic mechanism | Current DPS | Old weighted Top1 | New weighted Top1 | Full-replacement optimum |
| --- | ---: | ---: | ---: | ---: |
| Capped accuracy | 820 | 600 | 850 | 850 |
| Local weapon multiplication | 40 | 35 | 35 | 40 |

The capped case uses six probes per estimator and reduces Top1 regret from 250
DPS to zero. In the multiplicative case both linear estimators retain a 5-DPS
regret; the legal combination search finds the exhaustive 40-DPS result in nine
evaluations. These are deliberately small mechanism tests, separate from the
real-WASM evidence above.

## Automatic supports

[supportOptimizer.ts](../../../web/src/lib/supportOptimizer.ts) generates the
eligible pool from pinned skill-type metadata, including postfix Boolean
requirements/exclusions and type additions contributed by other supports.
Candidate screening is permissive enough to retain type-enabling pairs, while
every complete combination receives an exact compatibility check. It enforces
level, socket capacity, overlapping families, source restrictions, and the
default lineage-copy limit across groups, including inactive weapon groups.
Unknown compatibility metadata is excluded with a count rather than guessed.

Players can search candidates by localized or English name, uncheck unwanted
supports, or exclude a gem directly from a recommendation. Exclusions use exact
skill IDs (tiers remain separate) and persist per active-skill set in the browser.
They apply before individual probes, current-set seeds and combination search;
an excluded equipped support cannot re-enter through a replacement seed. The
unchanged equipped build remains the comparison baseline. Editing exclusions
cancels pending work and invalidates results before another plan can be applied.
Restoring one or all exclusions does not modify the equipped build either.
An empty allowed pool still evaluates removal of excluded equipped supports;
with neither allowed nor equipped supports, the search is disabled.

Individual probes seed a bounded combination search that retains neutral
candidates and the current set. Complete group snapshots preserve active gems,
other groups, and weapon bindings. The applied snapshot is the evaluated one.
Ordinary support changes are skill adjustments without market purchase links;
lineage supports are explicitly identified. Metadata compatibility does not
establish that every support effect is modeled by the calculation engine.

The small exhaustive oracle in
[supportOptimizer.test.ts](../../../web/src/lib/supportOptimizer.test.ts)
uses controlled evaluator values, not live game DPS:

| Fixture, two support sockets | Best two isolated supports | Complete combination / exhaustive optimum | Oracle search space |
| --- | ---: | ---: | ---: |
| Individually neutral A+B synergy | 180 | 300 | 11 legal sets, including empty |
| A adds the type required by B | 170 | 260 | 5 legal sets, including empty |

Both tests verify that the bounded result equals the exhaustive optimum, uses
no more reported candidate evaluations than the oracle, and applies exactly the tested
gems. They also cover wrong skill types, final exclusions after type additions,
duplicate families, a survival floor, newly unsupported effects, cancellation,
and lineage availability. They do not prove that a pruned large pool retains
every useful combination or that an arbitrary five-support setup is globally
optimal. The support planner additionally evaluates an unchanged identity
snapshot for baseline unsupported-effect diagnostics; its candidate counter
excludes this identity call.

The real-engine [supportBench.test.ts](../../../web/src/lib/supportBench.test.ts)
uses six fixed catalog supports and two sockets for Ice Shot and Fireball. Each
has 22 legal complete sets including the empty set. Both isolated single-support
ranking and the new combination search reach the finite oracle optimum:

| Skill | Baseline DPS | Selected DPS | EHP before/after | New / isolated / exhaustive evaluations |
| --- | ---: | ---: | ---: | ---: |
| Ice Shot | 127.232 | 173.638 | 2529.633 | 21 / 7 / 22 |
| Fireball | 86.648 | 122.437 | 2529.633 | 21 / 7 / 22 |

These real cases show equivalence, not a universal advantage of combination
search. The synthetic interaction cases above explain why combination search
is still necessary. Independent application through the editor helper matches
both DPS and EHP exactly and preserves the other weapon group and its binding.
Irrelevant attack/spell added damage gives zero gain; relevant damage gives a
positive gain. No new unsupported effects occurred in these fixtures.

## Passive planning

[passivePlanner.ts](../../../web/src/lib/passivePlanner.ts) searches connected
paths from allocated nodes or the class start. Travel nodes consume the same
point budget as the target. Path unions deduplicate shared travel cost and can
find nodes that are only useful together. Reallocation considers refundable
terminal branches, rebuilds reachability after each refund, and replaces within
the refunded point budget. Filled jewel sockets and class roots are protected;
unexplained imported topology and weapon-set-exclusive passives disable unsafe
automatic refunds. Ascendancies and unsupported mastery paths are outside this
planner's ordinary passive-point scope.

The search is bounded to 512 evaluations, with bounded path probes and unions.
Shortest paths, beam selection, and terminal-branch refunds restrict the space.
The result is the best evaluated legal suggestion, not an optimal allocation
over the entire passive tree. Existing allocated travel-attribute choices are
preserved; newly considered travel nodes use the configured attribute choice.

After the first explicit search, the open planner tracks build edits. It hides
stale results immediately, cancels outstanding work, and waits 350 ms after the
latest build calculation before searching again. Collapsing or cancelling pauses
tracking. Application checks the exact evaluated request, connectivity, point
budget and attribute choices; it never applies a refreshed recommendation
automatically. Reallocation is optional and limits refunds to eight points.

Tree panning keeps the root SVG viewport fixed and translates its inner scene
at most once per animation frame. Memoized nodes and edges remain mounted, so
previously clipped edge nodes become visible during the drag. Pointer release
commits the viewBox and clears the temporary transform in one layout commit;
pointer cancellation restores the original view. Captured node events skip
hover updates while dragging. Browser regression tests cover edge visibility,
node-origin drags, no unintended allocation, cancellation and sub-pixel release
continuity; these do not establish a hardware-independent frame-rate guarantee.

[passivePlanner.test.ts](../../../web/src/lib/passivePlanner.test.ts) compares
small graphs with all legal connected allocations using synthetic evaluator
outputs:

| Fixture | Previous single-node interpretation | Legal planner result | Exhaustive result |
| --- | --- | --- | --- |
| Two useful nodes each need one travel node; budget 2 | Picks nodes 3 and 5, requiring 4 actual points and disconnected as proposed | Path 2→3; DPS 150, EHP 100 | Same; balanced score ≈122.47 |
| Two useful nodes share one travel node; budget 3 | Each node individually has zero DPS gain | Nodes 2,3,4; DPS 200, EHP 100 | Same; balanced score ≈141.42 |

A separate refund test replaces branch 2→3 with 4→5 within two refunded points,
improving synthetic DPS from 105 to 130 while EHP stays 100. Tests check the
connectivity and point count of every returned plan, protected sockets, unknown
imports, survival constraints, cancellation, and deterministic evaluation caps.

The real-engine [passiveBench.test.ts](../../../web/src/lib/passiveBench.test.ts)
uses a fixed eight-node Sorceress neighborhood, including an attribute travel
node. Three-point allocation enumerates seven legal candidates; the planner
uses eight evaluations including its baseline and reaches the same optimum:
DPS 82.880 → 97.950, EHP 2420.694 → 2441.670. The one-point refund case has two
legal candidate swaps; six planner evaluations reach the same optimum, DPS
97.950 and unchanged EHP 2420.694. Independently applied values have zero DPS/EHP
difference from the predictions, with existing Intelligence and new Strength
choices preserved. Full-tree cancellation and the 512-evaluation limit are also
checked. These are finite neighborhood results, not global tree optima.

Run all real-engine comparisons after building WASM and syncing data:

```sh
POBR_UPGRADE_BENCH=1 pnpm --dir web test src/lib/upgradeBench.test.ts src/lib/supportBench.test.ts src/lib/passiveBench.test.ts
```

The tree data now preserves `unlock_constraint` (200 nodes in data `4.5.5.2`).
The planner verifies the required ascendancy and prerequisite nodes, and protects
prerequisites of already unlocked nodes during refunds. Plans introducing a new
unsupported effect relative to the baseline are excluded with a visible count.

## Upstream references

The inspected PoB2 checkout is pinned by
[`vendor/.pob2-version.txt`](../../../vendor/.pob2-version.txt) to
`ce566eac45ea8a86477f513c7ee65a1ebe60014e`. The vendored directory is ignored;
the following links refer to the corresponding immutable upstream source:

- [`src/Classes/TradeQueryGenerator.lua`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/TradeQueryGenerator.lua): blank-item replacement probes, per-unit stat weights, query complexity, and the approximate historical minimum Sum.
- [`src/Modules/CalcActiveSkill.lua`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Modules/CalcActiveSkill.lua): support applicability and type additions while building active skills.
- [`src/Classes/PassiveSpec.lua`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/PassiveSpec.lua): allocation paths, dependencies, and reachability.
- [`src/Classes/TreeTab.lua`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/TreeTab.lua): passive power visualization and distance limits. A heatmap's node value is not a legal multi-point plan by itself.

PoBR's removal probes, shared objective, replacement verification flow, and
bounded support/path searches are adaptations in this repository. They should
not be described as an exact port of PoB2's optimizer.
