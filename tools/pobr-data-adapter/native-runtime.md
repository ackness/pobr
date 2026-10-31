# Guarded native runtime assembly

`--native-runtime` projects a verified RePoE `items-actors-bc` v4 archive into
existing PoBR runtime domain types. It creates a **queryable schema-4 candidate**;
ordinary full-build calculation stays closed with `native-profile-not-admitted`. It does not read
vendor data, active snapshot contents, `CURRENT`, shared overlays or the network.

```sh
mise exec -- cargo run -p pobr-data-adapter -- \
  --native-runtime /path/to/native-v4-package \
  --expected-manifest-sha256 <separately-reviewed-facts-manifest-sha256> \
  --source-lock /path/to/exact-facts-source-lock.json \
  --descriptions /path/to/native-descriptions-package \
  --descriptions-manifest-sha256 <separately-reviewed-descriptions-manifest-sha256> \
  --official-tree /path/to/official-tree.json \
  --official-tree-sha256 <separately-reviewed-official-tree-sha256> \
  --compiler-revision <40-lowercase-hex-git-commit> \
  --out .cache/native-runtime-candidates/new-candidate
```

The caller supplies an existing parent and an absent output leaf. This directory
is itself the version root accepted by `GameData`, without `CURRENT`. Output must
stay outside production and input packages. Directory handles protect the
reserved root and base layer; this is not an OS sandbox against arbitrary
external directory moves. The compiler revision is a caller declaration; a
source implementation fingerprint is also recorded. Release tooling must check
the Git declaration and clean build independently. Hashing an unknown input and
immediately accepting it does not authenticate its source.

The established v4 verifier checks all 77 physical tables and their projections.
The supplemental description manifest is externally pinned; all three payload
hashes, source lock, realm and patch are checked. The large branch archive is
referenced, **not interpreted as an effective formatter**. Facts and descriptions
identify international client `0.5.5.5`; the official tree patch stays unknown.

Only official `x/y` coordinates are borrowed. Numeric node IDs, supplied non-null
slugs, directed edges, root links, group membership/order, orbit indices and all
class replacement pairs must agree with the client graph. Group centres must
equal client centres rounded to six significant decimal digits, the observed
export formatting. This is not a radius formula. Null slugs are placeholders;
client definitions are retained. Official names, numeric text and effects are
never imported.

The projection includes all 5,152 Default graph instances and the selected
Sorceress's base attributes. Complete single-stat ordinary passives can use the
existing native contract, bound to the exact source/patch/tree tag. Eligibility
uses the core's own mapper, never a second stat-to-modifier registry or fixed
node list. Amounts and node/stat identities come from source rows. The reviewed
fire-resistance node remains a required reference. Other nodes must match its
entire raw context apart from identity, name/icon, graph ID and the one stat/value
slot. Unknown fields and unused slots must match too; grants, choices, multiple
stats or changed context do not become partial packets. No display text is invented
when a native-only packet has no formatter. The reference index retains all 9,731 definitions and 61 class
relations; no missing Druid base becomes a phantom graph node.

Spark's base/gem, primary granted effect/stat-set, level-1 resolved stats and
costs use native stable-ID joins. `BaseResolvedValues` supplies stored damage;
`FloatStatsValues` remains evidence, never flat damage or a second effectiveness
multiplier. Constants and implicit stats stay separate. No PoB ordinal is
assigned. The cold-infused branch, other levels/quality/supports/equipment remain
outside this projection, with complete facts retained in the archive.
The selected normalized stat IDs must be unique across resolved, constant and
implicit channels; no duplicate/override policy is inferred. Physical signed-i32
scalar fields are checked before unit conversion, alongside the i32 stat arrays.
The non-allocatable official root connector is mandatory matching evidence.

General tree modifier text is absent pending effective CSD selection/formatting.
Ascendancy ownership, unlock constraints, class variants, attribute choices and
jewel geometry are unprojected. An empty field does not declare an unmodelled
mechanic effectless. `base/native_coverage.json` records these omissions, and the
candidate cannot enter legacy calculation. Packet coverage is counted from emitted
nodes and lists the required core rules. Unprojected nodes remain in the archive;
their absent packets do not mean their effects are globally irrelevant.

Sealed sidecars retain the transitive input lock, canonical raw references,
normalization decisions, required-rule declarations, acquisition status and tree
match evidence. `base/native_runtime.json` binds their digests into its assembly
key; `manifest.json` inventories every base file. All projected domain readers
and the schema-4 memory loader must pass before output reservation. The manifest
and final `assembly-receipt.json` follow base writes. An interrupted directory
without the final receipt is incomplete.

Sources remain unreviewed and `production_ready: false`. Ordinary calculation
stays blocked for both contract versions. No pool/accuracy/requirement formula,
server cadence, full DPS, build/save binding or CN numerical coverage is inferred.

## Opt-in player context (native contract v2)

Add all three flags to the command above:

```sh
  --player-context /path/to/repoe-native-player-context-package \
  --player-manifest-sha256 <external-manifest-byte-pin> \
  --player-source-lock-sha256 <external-player-source-lock-byte-pin>
```

Without these flags, the existing query-only v1 contract remains the default.
Its absent `player` extension is omitted from serialization and canonical hashes.
V2 still uses manifest schema4; it requires sealed `native_player_context.json`
and `native_actor_binding.json`, version2 normalization/rules, complete artifact
pins, and the allowed player plus exact parent source manifests. V2 artifacts
under v1 fail even when they are unlisted. Neither version enters `BuildData`
or `load_ruleset` calculation.

The player consumer binds the 14 fixed producer fingerprints at RePoE cf190d05,
reuses the 117-source decoder verifier, authenticates the complete eight-file
inventory against the external manifest pin and the separate external lock pin,
and compares the embedded parent manifest byte-for-byte with the verified v4
input. Player and parent game/realm/patch must agree. Reference origins resolve
against separate archives with exact table, physical row, stable ID and pointer.
Consumed physical tables and canonical retained metadata lines are cross-checked
against their projections. Every UTF-16 line/resource fingerprint and assignment
value span is checked; duplicate or ambiguous critical assignments fail.
This consumes a pinned package, without re-decoding DAT or claiming raw source
review. `tests/check_native_player_sources.py` is a read-only standard-library
receipt checker, not a Rust replay or source approval.

The selected identity is exactly `Metadata/Characters/Int/IntFourb` (Character7)
and AO `Metadata/Characters/Int/IntFourB.ao`. The IntFourB→Character→Parent AO
chain and independent Character.ot remain distinct. Character1's Witch override
cannot bind this actor. All six starting relations and twelve ordered choices
remain references; selected row2 options2779/1146 are never autogranted.

`NativePlayerContextDef` retains typed source facts/metadata/local physical tables
and all eight original JSON texts. The separate `NativeActorBinding` names
`selected-player-generic-ot-v1` for `ConditionalSkillHitV1`, with status
`conditional-model`. It normalizes only the sole plain generic OT
`base_critical_hit_damage_bonus` lexical token into a finite explicit extra-damage
bonus. The amount is source-derived. Critical glossary definitions and exact
assignment/path/UTF-16 provenance remain retained. Applying this separate OT
quantity to the selected actor is a PoBR assumption, never an inferred AO→OT
inheritance edge or effective metadata merge.

All99 plain Stats have explicit per-key dispositions in `player-stat-policy.json`;
unknown/scoped metadata, character fields and starting relationships remain
retained with local dispositions. Charges/rampage, PvP, delirium and other
excluded conditions remain inactive; defence, recovery, contact probability,
payment timing and ailment outputs remain outside this evaluation. An unknown
plain key fails source-policy normalization. Relevant unresolved inputs still
block the later build admission; an unused fact is never globally effectless.
The no-override policy excludes gear, supports, allocations, ascendancy, infusion,
buffs, charges, rampage, quests, PvP, delirium, events, movement, cutscenes and
external modifiers. Mana/use does not certify payment instant or affordability;
a successful damaging hit does not guarantee contact.

`pobr_gamedata::NativeData::load(&GameData)` loads immutable typed catalogs,
context and policy with private validity-bearing state. It returns v1 without
actor defaults, and rejects legacy data, native patches and malformed v2 inputs.
It does not construct `RuntimeConstants`/`BuildData` or call `load_ruleset`.
Sealed typed native catalogs reject duplicate JSON object keys, including escaped
aliases, before deserialization can overwrite a stable identity in a map.
Its identity getter returns the final `GameData` digest; the assembly key remains
separate. V2's receipt records that final identity outside the self-hashed
contract. Loading is not build admission, persistence or calculation authority.
