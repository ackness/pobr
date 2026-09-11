# Calculation performance and copied item comparisons

## Performance evidence

`web/scripts/bench-calc.mjs` measures real release WASM on the three committed
PoB builds below. It stages the same `4.5.5.2` data into independent old/new
modules, warms both, alternates execution order, and compares complete JSON
outputs. Distinct trailing JSON whitespace bypasses endpoint response caching.
The batch replaces a ring with 16 different complete items; it requests either
one or 16 display fields, including the same baseline calculation.

The performance-only changes were measured before intentional affix and
diagnostic behavior changes. Nine samples per scenario produced these medians
on the development machine; they are observations, not cross-device guarantees.
All nine old/new scenarios returned equivalent complete outputs.

| Committed build | Single old/new (ms) | 16 items, one stat old/new (ms) | 16 items, 16 stats old/new (ms) |
| --- | ---: | ---: | ---: |
| monk-invoker-frost-bomb | 13.218 / 7.797 | 158.959 / 77.073 | 160.735 / 77.111 |
| mercenary-tactician-wolf-pack | 12.224 / 8.367 | 177.531 / 111.342 | 178.645 / 111.644 |
| ranger-deadeye-explosive-grenade | 12.380 / 6.528 | 170.714 / 77.839 | 172.769 / 77.704 |

Single calculations took 32-47% less time and batches 38-55% less time. The main
cost was rebuilding a lowercased name index of the entire passive tree for each
gem-property scan. Granted-notable resolution now scans only when a grant is
present, preserving case-insensitive matching, duplicate suppression and the
lowest-ID notable rule. Common item lines avoid character-by-character cleanup
when no PoB annotations are present. Sparse display extraction reads the
requested output directly instead of allocating all 105 display fields for each
field. Percentage-unit and unknown-ID behavior remain unchanged. No shared
mutable cache, prepared-build snapshot, or extra dependency was introduced.

Run after building WASM and synchronizing web data:

```sh
node web/scripts/bench-calc.mjs
node web/scripts/bench-calc.mjs --reference .cache/reference-wasm --iterations 9
```

The optional reference directory must contain an independent compatible
`pobr_wasm.js` and `pobr_wasm_bg.wasm`. Reference equality is appropriate for
behavior-preserving changes. New calculation effects or new diagnostics can
legitimately change the output and fail that comparison. Reports are written to
ignored `.cache/`; timing experiments are outside ordinary tests and CI.

## Modifier coverage

The following source-to-output paths now have real equipment/jewel regression
coverage, alongside the existing parser and parity suites:

- `Leeches N% of Physical Damage as Life/Mana` parses to the same typed names as
  PoB2. Physical leech consumers read the physical hit after conversion. Weapon
  sources are scoped to their attacking hand and cannot leech from a spell or
  the other weapon's hit. Proxy attacks (totems, traps and mines) do not grant
  ordinary player leech; explicit leech-to-player transfer remains unmodeled.
  The existing strongest-single-instance recovery panel
  remains an approximation; this is not a sustained recovery simulation.
- `Gain additional Stun Threshold equal to N% of maximum Energy Shield` accepts
  every numeric roll, using the existing PercentStat calculation and final ES.
  The numeric capture stays in PercentStat.percent with BASE=1, preserving
  Adorned/item-effect truncation before stat multiplication. This changes stun
  threshold, not raw DPS or EHP.
- `Lightning Damage from Hits also Contributes to Flammability and Ignite
  Magnitudes` enables the existing lightning ignite-source flag. It adds the
  lightning source to the modeled ignite magnitude without multiplying hit DPS;
  it does not implement a separate flammability buildup simulation.
- Ordinary socketed jewels participate in bonuses gained from an equipped
  quiver, including the existing corrupted-magic Adorned scaling. The quiver
  must actually be equipped.

These paths follow pinned PoB2 `ModParser.lua`, `Item.lua`, `CalcOffence.lua`,
`CalcPerform.lua` and the current trade catalog. They do not imply that every
recognized modifier has a complete calculation consumer.

Known remaining gaps include active Guard, Meta Skill Energy and Seal event
models, Ward regeneration, self-debuff expiry and separate Thorns damage.
These effects must not be substituted with invented main-skill DPS or permanent
life. A zero main-DPS change alone does not prove an effect is unsupported.

Rejected actual equipment and jewel modifier lines now reach session
unsupported diagnostics. The strict item gate still excludes partially parsed
modifiers from the numeric calculation; reporting a gap must not silently
inject its understood prefix. Clipboard metadata is parsed separately and is
not counted as an unsupported effect. A replacement with unsupported effects
needs review because the engine may understate either benefits or drawbacks.
GemProperty text and the verified two-line Adorned effect are already consumed
by dedicated orchestration paths and are not falsely reported as parser gaps.

## Paste-to-compare flow

The Upgrade page has one visible complete-item paste card. The input preserves
original effect text, normalizes structural clipboard headers, identifies the
base/category and required level, and uses the backend item classifier to show
actual affix lines. It evaluates every compatible equipped position and
allocated jewel socket in one batch. Rings are compared against both current
rings; each candidate replaces exactly one position.

Positions use the existing shared objective, EHP floor and optional elemental
resistance priority. Users can select a position and inspect baseline versus
replacement DPS, EHP and resistances. No item is applied until the explicit
apply action. Build, weapon context or input changes invalidate stale results
and cancel outstanding work. Objective changes re-rank the existing complete
outputs without unnecessary recalculation. The current comparison objective is
shown beside the paste input, with a link to its controls. Unsupported lines
remain visible.

This is a local whole-build comparison, independent of the market's linear Sum
and without importing listing JSON or accessing a market login. Attribute and
special equip requirements remain a player check. Single-slot weapon previews
reject incompatible weapon-family changes that require a coordinated setup.
