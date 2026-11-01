# Ordinary stat descriptions from RePoE

`--ordinary-domain stat-descriptions` writes `overlay/stat_descriptions.json`
for the existing `StatDescriptionsDef`, GameData loader and equipment TierIndex.
It consumes pinned RePoE CSD and a separately pinned legacy catalog. Source
acquisition/adoption remain separate from producing this domain candidate.

The producer owns each catalog scope's **own** descriptors. Global, passive and
aura are separate; literal includes resolve effective ownership when rendering,
without copying parent entries into child catalogs. A missing scope is an exact
source gap. Additional CSD roots do not become global equipment descriptions.
`Renderer::new`, `global` and `render` retain their existing caller contract;
`scope(document, path)` resolves an explicit literal include closure.

`single` retains representative numeric **1** as the existing capture convention,
not the actual scaled display of raw=1. Complete multiline descriptions stay one
template. Compounds retain the complete ordered member list and indexed
placeholder order. Unknown JSON fields are preserved. Known gem-quality and
table-only branches are excluded from ordinary display; unknown restrictions
reject the complete descriptor. `!n`, signed exact/range conditions, implicit
placeholders and known numeric formats follow the pinned CSD/PyPoE conventions.

Optional `descriptors` records retain **all ordinary branches**, raw integer
conditions and one composed display format per member:

```json
{
  "member_stats": ["regeneration_per_minute"],
  "branches": [{
    "template": "Regenerate {0} Life per second",
    "conditions": [{"min": null, "max": null, "not": null}],
    "formats": [{"multiplier": 1, "divisor": 60, "offset": 0,
                 "precision": 1, "fixed": false}]
  }]
}
```

The exact affine expression is `(raw * multiplier + offset) / divisor`.
Conditions apply in raw units **before** formatting, in source first-match order.
The last numeric handler owns precision: absent precision uses six significant
digits for fractions, zero truncates toward zero, and positive precision rounds
to that many decimals. Fixed formats preserve decimal zeroes. Coefficient
composition and supplied-value arithmetic are checked; invalid divisors,
precision, overflow and lossy wide inputs do not produce a value. Numeric formats
come from the existing pinned PyPoE `TQNumberFormat` definitions (revision and
reference hash in `passive_tree/render/number-cases.json`), not calculation laws.
`canonical_line` and `canonical_stat` retain their explicit marker policy and
validated arity/index; unknown relational handlers are never treated as numeric.

TierIndex uses descriptor metadata instead of legacy entries for those members.
It searches the exact forward display image over each mod's raw **integer** roll
range, preserving all raw values that round to a capture. Decimal captures are
compared as checked rationals, never guessed raw floats. Raw conditions and
previous-branch exclusion apply to the complete compound packet. If more than
one mod can produce the text, it returns no tier label. Old JSON without metadata
retains the old matching behavior. This affects display labels, not role effects.

Unsupported/ambiguous records, `no_description`, records with no ordinary branch
and legacy-only IDs retain explicit compatibility with complete source hashes.
A stale unsupported source record rejects until its binding is updated.
`no_description` never means effectless. The receipt separates source-owned
members from compatibility and preserves every equivalent owner's pointer/hash.
Original metadata is nested in the header; the old catalog is archived verbatim.

The optional `diagnostic_ownership: "explicit-no-description/v1"` policy moves
unique, single-member `no_description` records into the existing `unrendered`
diagnostic set. It cannot replace an old single, compound or descriptor, or choose
between duplicate owners. Each diagnostic binds the complete source record hash
in the receipt; newly displayed source text leaves the diagnostic set and follows
the normal descriptor path. Missing source records still require an explicit
compatibility binding. The shared effect renderer continues to reject these
stats in complete passive/socketable packets: a display diagnostic does not
authorize dropping a stat or prove its effect is zero. No DTO or calculation
rule changes are needed.

The optional `descriptor_equivalence: "ordinary-plain-display/v1"` policy also
coalesces duplicate owners whose English templates differ only in display markup
removed by the existing ordinary text consumer. Every other English semantic
field must match, including ordered members, branches, conditions, restrictions,
handlers and unknown fields. The receipt retains every original pointer and raw
slice hash with the equivalence policy name. Different wording or conditions
remain ambiguous; this policy does not select a first/last owner to resolve them.
The default shared renderer and earlier policies keep their existing behavior.
Complete compound packets still require every member.

An optional paired `--supplement` uses the existing verified selected-table
reader. Its manifest must equal `lookup_manifest_sha256` in the chosen policy.
The nine-table package resolves seven explicit PoE2 formatters: item class row,
1-based keystone/Mage's Legacy row, expansion-size row, Ultimatum HASH16, skill
Index → ordered SkillGems → BaseItemTypes, and PassiveSkillGraphId (including
the source signed-short alias). Physical column/FK contracts are checked.

`lookup_values` stores each handler's complete integer-key-to-display-literal
map once per scope; a branch's optional `lookups` array addresses those handler
names in member order. Missing/empty arrays mean all numeric. Lookup members
must use identity numeric formats. Raw labels, display markup conversion, source
row hashes, ordered joins and complete mapping hash remain in the receipt.
Unknown keys, empty names and missing tables never supply a numeric ID as text.

TierIndex only binds known keys inside the actual stat-set mod pool's raw ranges.
A missing pool creates no lookup templates. Each binding retains its raw key;
two possible keys with the same name are ambiguous even inside one mod. The
current source-backed path permits at most one lookup member per branch and
avoids a cross-product of whole tables. Descriptor storage is shared across its
templates. PoE1-only `affliction_reward_type` / `display_indexable_support`
policies remain unsupported; no PoE1 relation is guessed for PoE2.

Use `0.5.5.5-lookups-policy.json` plus the pinned nine-table package when enabling
these names; `0.5.5.5-policy.json` replays the numeric-only catalog. The oracle's
optional `--lookup-tables /absolute/supplement/tables.json` independently checks
the complete physical joins and the produced mapping hash as well as coverage.
`0.5.5.5-diagnostics-policy.json` additionally enables source-owned unrendered
diagnostics; pair it with the oracle's
`--diagnostic-ownership explicit-no-description/v1`. Omitting this field keeps
the earlier policies' behavior and compatibility bindings.
`0.5.5.5-plain-display-policy.json` additionally enables plain display equivalence;
use the same oracle arguments plus
`--descriptor-equivalence ordinary-plain-display/v1`. Its independent source
inventory owns two additional global descriptors, `gain_x_rage_on_hit` and
`bleeding_damage_on_self_taken_as_fire_instead`, while retaining their raw owners.
The declared ordinary Git inputs now select this policy. Actual six-domain
production and compact runtime composition preserve every existing descriptor
and text; 17 legacy singleton and 3 compound bindings still use compatibility.
The 37 scoped compatibility IDs are a separate count, not a zero-gap claim.

The policy pins payload, manifest, patch, old input and compatibility aggregate.
The ordinary reader verifies Stats and source lock without reading unrelated
tables. Stats membership does not invent/filter descriptions. English text remains
a matching key, with existing translation applied at display boundaries.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/F --facts-manifest-sha256 "$FACTS_SHA" \
  --version 0.5.5.5 --ordinary-domain stat-descriptions \
  --codec-source data/0.5.5.4.2/overlay/stat_descriptions.json \
  --codec-source-sha256 "$LEGACY_SHA" \
  --codec-policy pipeline/stat-descriptions/0.5.5.5-policy.json \
  --codec-policy-sha256 "$POLICY_SHA" \
  --item-descriptions /absolute/aura-closure/descriptions.json \
  --out /absolute/existing-parent/fresh-descriptions
```

Use actual pins; [ordinary input/output guards](ordinary-facts.md) apply. The
candidate contains the marker, overlay, old catalog, policy, manifests/lock and
receipt. Assembly and CURRENT are explicit coordinator steps. Frozen inputs are
never overwritten. Generated modifier rules are not regenerated by this command.

The independent stdlib oracle inventories source ownership and compares a real
producer's complete target membership and compatibility bindings:

```sh
mise exec -- python3 pipeline/stat-descriptions/source_oracle.py \
  --source /absolute/aura-closure/descriptions.json \
  --legacy data/0.5.5.4.2/overlay/stat_descriptions.json \
  --candidate /absolute/fresh-descriptions --out /absolute/fresh-oracle
```

Focused Rust regressions (run sequentially in the coordinator's warm target):

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::stat_descriptions::
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::passive_tree::render::
mise exec -- cargo test --locked -p pobr-item --lib tier::
```
