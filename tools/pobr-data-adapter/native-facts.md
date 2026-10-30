# Native facts domain candidate

By default this opt-in adapter consumes `repoe-native-facts/v1` from RePoE revision
`b6def2152dfc48d8ec49c7a237b2e973c3830e97`. It creates a **partial INTERNATIONAL
0.5.5.5 candidate**, never a runtime data manifest or production-ready package.
It does not update `data/`, `CURRENT`, defaults, calculation rules or PoB compatibility.
The producer contract is `RePoE/docs/native_facts.md` in that revision. The
explicit core-skills A0, gem/constraint A1 and items/actors B/C opt-ins are
documented below.

The separate [guarded runtime assembler](native-runtime.md) can project the v4
archive into queryable schema-4 runtime domains. It does not admit calculation
or change this mode's partial-candidate contract.

```sh
mise exec -- cargo run -p pobr-data-adapter -- \
  --native-facts /path/to/repoe-facts \
  --expected-manifest-sha256 <externally-trusted-64-lowercase-hex-digest> \
  --source-lock pipeline/sources.lock.json \
  --out .cache/new-native-candidate
```

All four arguments are required. This mode accepts no legacy adapter arguments.
The caller supplies the external manifest digest; calculating it from an untrusted
package and immediately accepting it provides no authentication. The source-lock
bytes, selected entry, decoder/schema pins and nine raw fingerprints must match
the authenticated receipt. The nine producer file fingerprints bind the supported
producer revision (the upstream v1 receipt itself has no Git revision field).
The decoder must name trusted source pin
`64703bc3c5eca107108244e02baf0dc2cd46669e62a6b5ba273f868aa9db0095` and exactly
its 117 source paths. Each source hash must match the pinned Git blob or its
upstream `ident` checkout expansion. The pin is retained as a non-game fixture,
verified by its own fixed digest, and included in the adapter fingerprint.
Payload paths must be exactly the eight physical tables plus `graph.json`,
`passives.json`, `skills.json` and `diagnostics.json`. Every size/hash is verified
before payload consumption. Extra paths (including empty directories), symlink
components, missing files and duplicate JSON keys anywhere are rejected.
Verified bytes are retained for parsing; the adapter never reopens mutable payloads.

`candidate.json` uses the adapter-local `native_facts/model.rs` schema
`pobr-native-domain-candidate/v1`, a reusable intermediate boundary for the next
domain conversion. No runtime `pobr-data` types are changed until a runtime consumer
and its semantics are justified. The candidate contains:

- All passive definitions keyed by stable game string ID, a separate source order,
  and ordered stat ID/value pairs with one-based slots and original value fields.
- Separate graph instances joined to definition IDs, directed edges and signed
  splines, groups and roots. Geometry fields retain their raw names/units; a raw
  radius index is not a physical radius. No global coordinates are calculated.
- Every class relation, including base/replacement membership flags when either
  definition has no default graph instance. No replacement node is instantiated.
- All effect and stat-set identities, with separate source orders and each effect's
  ordered set links, original fields/slots and explicitly named index hypothesis.
- Every quality row joined by effect ID, with separate main/alternate raw scopes,
  zero-based stat slots and integer permille values. Ordered sets join through the
  effect. An empty scope has no inferred policy. Null primary/additional links
  remain facts in the physical row; they do not become invented set identities.

Stable IDs are scoped by the input's game/realm/patch identity. Physical row numbers
appear only as evidence in JSON pointers; they are never game IDs. Maps support
stable-ID lookup; order arrays and ordered relation/stat arrays preserve source
order. Projected stat/value pairs, identities, graph membership and relationships
are checked against the verified physical tables; incomplete projections fail.
Each instantiated graph ID must join exactly one physical PassiveSkills row,
and the selected instance's row and string ID must identify that row. Duplicate
off-graph placeholder IDs remain permitted. The acquired Default tree row's
literal `PassiveSkillGraph` value plus `.psg` must equal `graph.raw_resource`;
this introduces no geometry interpretation or policy for unavailable graphs.

The complete RePoE package remains the canonical facts archive. `raw` and
`raw_stat`/`raw_character`/`raw_tree` references contain a payload path and JSON
pointer, resolved **only against the RePoE package with the candidate's
`input_manifest_sha256`**. Physical rows retain every unused value slot, unknown
field, null, empty string/array, flag, foreign row and metadata field. Table-level
columns/schema/validation and unprojected physical facts likewise remain in that
same sealed input inventory. The complete graph is referenced in addition to its
typed fields. This command deliberately does not duplicate roughly 41 MB of
physical tables into a second raw-copy pipeline. A future conversion requiring a
raw reference must obtain that exact canonical package and verify its manifest and
payload bytes again; a pointer is not a claim of independently understood semantics.
Absolute machine paths are never inserted into output.

`source-manifest.json` and `source-lock.json` retain their exact verified bytes.
`source-diagnostics.json` retains the entire diagnostics payload byte-for-byte,
including unknown diagnostics and occurrence order. In particular the actual
SnipePlayer quality row 69 keeps main scope `[1,2,3]`, alternate `[0]`, three sets,
and the out-of-range-under-hypothesis diagnostic. No fourth set, quality application,
label, display text, stat mapping, geometry semantics or PoB fallback is inferred.

`candidate-receipt.json` records input identity, measured counts, adapter source,
crate manifest and dependency-lock fingerprint, and every output payload's size/hash.
The adapter enables JSON `float_roundtrip` so raw PSG coordinates retain their exact
binary floating-point values. The receipt is written last. Input and
normalization failures create no output. Any existing output, even an empty
directory or dangling symlink, is refused and retained. The existing output parent
must not contain symlinks. Every existing ancestor is opened without following
symlinks; device/inode identities reject output inside the canonical input package
or the checkout's production `data/`, including filesystem case and normalization
aliases. A small `rustix` dependency supplies safe directory-relative syscalls.
Reservation (`mkdirat`), every exclusive payload creation (`openat`) and the final
receipt all use the retained directory handles. Replacing a parent with a symlink
cannot redirect those writes. Directory identities are checked when opening,
before and after reservation/writes, and before reporting success. Observed
namespace drift fails instead of reporting success at a misdirected path.

This output mode supports Linux and macOS; other targets return an explicit
unsupported-platform error before payload consumption. The correction batch was
tested on macOS arm64, including real case/Unicode aliases and a stopped-process
parent-symlink replay. Linux and other-target builds were not exercised in that
batch. This is not an OS sandbox against arbitrary actors moving already-open
directories, remounting namespaces, changing files, or changing paths after the
last check. It does not grant exclusive ownership of the surrounding filesystem.
An I/O interruption or detected namespace drift may
leave an incomplete directory; it must not be consumed without a valid completion
receipt and matching payload hashes. Retry at a fresh path. Output is deterministic
for identical inputs and adapter source; it has no timestamp or invocation path.

Focused regression check:

```sh
mise exec -- cargo test -p pobr-data-adapter --bin pobr-data-adapter native_facts::
mise exec -- cargo fmt -p pobr-data-adapter -- --check
mise exec -- cargo clippy -p pobr-data-adapter --bin pobr-data-adapter --tests -- -D warnings
```

Synthetic fixtures cover trust boundaries, retained verified bytes, stable ID joins,
fifth/zero/negative stats, empty/null/ordered metadata, both quality channels,
uninterpreted scopes, absent-base facts, diagnostic preservation, deterministic
receipts and failure protection, including alias and staged namespace-replacement
regressions. `tests/fixtures/native-facts/final-manifest.json` is only the final
external receipt (SHA-256
`30b42aa2b79e1810bf44f856b924d929a3e36fffe5a9afc264b7712c5dd2f968`), not game
payloads. Its nine producer hashes were checked against the final RePoE Git blobs.
Its source-lock hash remains
`e5ca7ea7dea4a0e220afcf0acac97d046cfbed3c0099e5074b2182fe50a43bde`.
The older `actual-a-manifest.json` (`5193…`) is a historical producer-rejection
fixture, not an accepted tool identity. Synthetic packages use the final profile
with small synthetic payloads and recomputed external test pins; alternate-package
consistency tests do not represent external authentication bypasses.
Real-source smoke additionally requires two
fresh independent-process outputs and an independent comparison of all converted
relationships/stats to the canonical facts. This consumer does not certify the
producer's client decoding, complete foreign-key closure, other graphs, labels,
level data, translations, gameplay semantics or runtime usability.

For the pinned final artifact, compare two fresh independent-process outputs:

```sh
mise exec -- python3 tools/pobr-data-adapter/tests/smoke_native_facts.py \
  --native-facts /path/to/actual-final \
  --candidate-a .cache/first-native-candidate \
  --candidate-b .cache/second-native-candidate
```

The smoke checker authenticates the manifest, requires the exact canonical path
inventory (rejecting extra directories and symlinks), and verifies **all twelve**
payload sizes/hashes before parsing any payload. All raw-reference and facts
comparisons use those retained bytes, including otherwise unused metadata. A
changed unused `Stat7Value` with the original manifest is rejected.

This independently checks 9,731 definitions, 5,152 instances, 5,957 graph stat
pairs, 61 class rows, 8,349 effects, 8,283 set identities, 429 qualities and all
61 source diagnostic records, with byte-identical outputs and the 31955 fifth
stat / Snipe row 69 facts. Every raw reference resolves in the sealed archive;
physical graph joins, Default resource, topology, effect links and quality channels
are cross-checked. Raw group coordinates are compared by binary64 bits, without
a tolerance that could hide a parsing change. The twelve final payloads are
byte-identical to the older correct archive; candidate facts change only in their
input manifest identity, with no new stat, quality or geometry policy.

## Explicit core-skills A0 consumption

`--profile core-skills-a0` selects the bounded `repoe-native-facts/v2` contract in
RePoE `aa2fb163aa422317db5c32031607eb199214e7b7`, documented there in
`RePoE/docs/native_core_skills.md`. Omitting the flag, or explicitly selecting
`--profile base-v1`, keeps the v1 contract above. A v2 package fails without the
v2 opt-in; a v1 package fails under that opt-in. There is no format fallback.
The A0 producer increment has been independently reviewed against its fixed raw
inputs. A later producer change requires a deliberate pin update. This review
does not promote the acquisition proposal or certify gameplay semantics.

```sh
mise exec -- cargo run -p pobr-data-adapter -- \
  --native-facts /path/to/actual-a --profile core-skills-a0 \
  --expected-manifest-sha256 e4bd693d45d8eaa4768d9a86c3e7e5ee84f1f4d8fc05112ae3b290d51bb901e5 \
  --source-lock /path/to/proposed-sources.lock.json \
  --out .cache/new-core-skills-candidate
```

The same external byte pin, schema and full 117-file decoder checks apply. The
ten producer SHA-256 values are fixed in the adapter and were independently
compared with that revision's Git blobs; the two changed/added values are for
`RePoE/native_facts.py` and `RePoE/native_skills.py`. Manifest claims and
recomputed self-hashes cannot replace these pins. The external manifest digest
binds a package; it does not authenticate acquisition of its upstream client.
The checked-in `core-skills-a0-manifest.json` fixture is that receipt only,
without physical game payloads.

A0 has exactly fifteen DAT resources and the Default PSG. In addition to the
base eight tables it requires `GrantedEffectLabels`, `ActiveSkills`,
`ActiveSkillType`, `CostTypes`, `GrantedEffectsPerLevel`,
`GrantedEffectStatSetsPerLevel` and `EffectivenessCostConstants`. Exactly 21
payloads are verified before any payload is parsed: fifteen physical tables,
the four base documents, `skill_core.json` and `source-lock.json`. The embedded
lock must equal the external file byte-for-byte, including unselected source
entries and whitespace. Source selection, its `profile: core-skills-a0`, raw
resource inventory, sizes/hashes and acquisition declaration must agree.

The selected source declares an acquisition status and a nonempty array of
`{kind,sha256}` evidence fingerprints. The supported declarations are
`unreviewed-acquisition-draft` and `reviewed-source-input`; both remain caller
declarations, preserved in `receipt.input.source_identity.acquisition`. The
actual `e4bd…` package and its observed raw hashes remain
**unreviewed-acquisition-draft**. Neither the producer pin, this conversion nor
a passing smoke test approves them or promotes them into the production lock.
Evidence files are not fetched or authenticated by this adapter.

The v2 candidate retains all base candidate fields, with explicit format
`pobr-native-domain-candidate/v2`, a typed `core_skills` collection and measured
`core_summary`. Its receipt uses `pobr-native-candidate-receipt/v2` and names the
input format/profile, producer revision, acquisition declaration and exact raw
fingerprints. Base counts remain in `summary`; the seven added counts are in
`core_summary`. Both formats remain partial with `production_ready: false` and
produce no runtime `manifest.json`.

`core_skills` contains source-ordered arrays for labels, stat-set labels,
active-skill types, active skills, effect metadata, cost types, interpolation
constants and every physical effect/stat-set level row. Definition and foreign
references use `{id,raw}`, where `id` is the stable game string and `raw` is a
canonical-input row reference. Physical row numbers on level facts identify
source occurrences only. They do not manufacture stable level IDs. Arrays keep
repeated owners, repeated levels, actor contexts, duplicate references, null
slots and source order.

All projected fields and raw pointers are checked against the physical tables,
including row-zero label references, null relations and empty label text as
separate facts. `ActiveSkills.GrantedEffect` remains a string. Costs retain the
entire ordered cost-type and amount arrays; unequal lengths produce `costs:
null`, without truncation or invented defaults. Stat-set levels retain separate
base, additional, float, interpolation and actor/level channels, even when
arrays have unequal lengths. Numeric channels use JSON numbers without unit
conversion; signs, zero, negative zero and exact binary64 float values round-trip.
The adapter checks the declared physical projection and creates stable references;
it does not interpolate, align unequal stat arrays or select effective levels.

Every core definition/level fact refers to its complete canonical physical row,
so cooldowns, reservation, flags, unknown fields, unprojected arrays and
column/schema metadata remain in the sealed RePoE package. Diagnostics are
copied byte-for-byte, including unresolved alignments and unknown diagnostic
shapes. No interpolation, units, quality application, geometry or gameplay
application policy is inferred. No production data, runtime types, engine,
WASM or source acquisition pipeline is changed.

V1 candidate serialization is unchanged. New runs accurately fingerprint the
new adapter implementation; this does not invalidate the historical `08a7…`
candidate receipt produced by its original implementation. The final b6
`30b42…` input profile and authentic earlier candidate outputs remain supported.
The historical `5193…` producer-rejection fixture keeps its existing meaning.

The same smoke checker accepts an explicit A0 profile:

```sh
mise exec -- python3 tools/pobr-data-adapter/tests/smoke_native_facts.py \
  --profile core-skills-a0 --native-facts /path/to/actual-a \
  --source-lock /path/to/proposed-sources.lock.json \
  --candidate-a .cache/core-skills-a --candidate-b .cache/core-skills-b
```

Generate the two candidates in fresh independent processes first. The smoke
checks all 21 payloads, all fifteen physical table envelopes and rows, every
core projection and stable/raw reference, the fixed ten producer hashes, full
decoder pin, raw source inventory, exact external/embedded/output lock bytes,
unreviewed source status, output hashes and byte determinism. The actual A0
increment contains 202 labels, 1,147 active skills, 290 types, 18 cost types,
3 constants, 34,300 effect-level rows and 41,557 stat-set-level rows, alongside
the unchanged base counts and 63 verbatim diagnostic records. These checks
certify this bounded conversion, not client decoding, acquisition approval,
complete foreign-key closure, numerical accuracy or production adoption.


## Explicit gems and constraints A1 consumption

`--profile gems-constraints-a1` selects `repoe-native-facts/v3` from RePoE
`c450fb8e3ad4e5792386c7c5bb81c90d85407e62`. Its bounded producer contract is
`RePoE/docs/native_gems.md`. Both producer and consumer increments passed
independent review. The fixed input remains an unreviewed acquisition draft,
partial and non-runtime.
A different producer needs a deliberate pin update. Both previous profiles
keep their own exact formats, producer pins and resource inventories.

```sh
mise exec -- cargo run -p pobr-data-adapter -- \
  --native-facts /path/to/actual-a --profile gems-constraints-a1 \
  --expected-manifest-sha256 b514c9fe28f2b49c9205b8686828fb5192c6d0dc101af00cfb138b3841ee5103 \
  --source-lock /path/to/proposed-sources.lock.json \
  --out .cache/new-gem-candidate
```

The output parent must already exist. A1 verifies exactly 39 DATs and the
Default PSG in the source inventory, and exactly 47 payloads before parsing:
39 complete physical tables, the four base documents, `skill_core.json`,
`source-lock.json`, `gems.json` and `constraints.json`. All eleven producer
file hashes were checked against the fixed Git blobs; the additional pinned
module is `RePoE/native_gems.py`. The external manifest digest, full decoder
source pin, schema, embedded/external byte-identical lock and explicit
acquisition declaration are checked with the same boundary as A0. The fixture
`gems-constraints-a1-manifest.json` contains only metadata, no physical tables.

The candidate and completion receipt use `/v3`. They retain the base and core
collections and add typed `gems`, `constraints` and twelve measured
`gem_summary` counts. Every declared projection field is reconstructed from
physical rows and compared before conversion. References carry the exact
canonical table and row. A stable string ID is included when the source table
has one; row-only tables such as `WieldableClasses` gain no invented ID.
`SkillGems` identity combines its physical row with the related `BaseItemTypes`
metadata ID. Multiple gem rows referring to one item remain separate rows.

Gem bindings retain source slots, shared effects, nulls, duplicates, primary
and additional effects, and the separate existing effect-to-stat-set links.
Variants, gem tags, multiple/empty support families, support metadata rows,
positive support associations and every experience row remain source facts.
Requirements and experience values retain exact JSON numeric channels.
Constraints retain ordered skill-type expressions, minion flags, weapon
restrictions, action/shape references and visibility flags. No support
expression, family conflict, eligibility, requirement formula, playable
filter, units or quality-scope policy is evaluated. Empty families and absent
positive associations do not mean that a gem supports nothing.

All source diagnostics remain verbatim, including missing support metadata,
flag disagreements and unresolved Snipe scope. Unknown physical fields and
unprojected auxiliary tables remain accessible in the pinned canonical input.
This candidate does not add a runtime manifest or approve source acquisition.
The maintained smoke checker accepts the same explicit profile and checks
all new scalar fields, relationships and their exact table/row evidence:

```sh
mise exec -- python3 -B tools/pobr-data-adapter/tests/smoke_native_facts.py \
  --profile gems-constraints-a1 --native-facts /path/to/actual-a \
  --source-lock /path/to/proposed-sources.lock.json \
  --candidate-a .cache/gem-a --candidate-b .cache/gem-b
```

## Explicit items and actors B/C consumption

`--profile items-actors-bc` selects `repoe-native-facts/v4` from RePoE
`e41401224a085a676c84b8f14b144f20fe37c79a`. Its producer contract is
`RePoE/docs/native_items_actors.md`. The producer
increment passed independent review. This is another explicit opt-in; previous
profiles retain their own input formats and pins. It produces a partial
`pobr-native-domain-candidate/v4` and `/v4` completion receipt, with
`production_ready: false`, without a runtime manifest or production update.

```sh
mise exec -- cargo run -p pobr-data-adapter -- \
  --native-facts /path/to/actual-a --profile items-actors-bc \
  --expected-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --source-lock /path/to/proposed-sources.lock.json \
  --out .cache/new-items-actors-candidate
```

B/C checks exactly 77 DAT resources plus the Default PSG, 87 sealed payloads,
and thirteen fixed producer file hashes. The external and embedded locks must
match byte-for-byte. The actual lock SHA-256 is
`d7a59a052109f411a62b1398ac844ff31ca8afd6d4158e3488446a19cb75743f`;
its acquisition declaration remains `unreviewed-acquisition-draft`. Hashes bind
the exact input and tools; they do not authenticate the client source or approve
gameplay semantics. The checked-in B/C manifest fixture is metadata only.

The candidate retains the base, core-skill and gem collections and adds typed
`items`, `actors` and sixteen measured `items_actors_summary` counts. Each new
projection is reconstructed from physical tables and checked against the producer
view before conversion. References preserve their exact canonical table/row,
adding a stable string ID only when one exists. Whole physical rows remain
available through the sealed input. In particular:

- Mods retain all eight nullable stat slots and separate regular/hardmode
  endpoints, flags, families, tags, weights and granted-effect/buff links.
- Bases retain metadata IDs, repeated names, direct tags, inheritance stems,
  implicit mods and seven component-owner joins. Charges join the exact metadata
  string, never a display name. Ambiguous or unresolved owners fail.
- SoulCores retain regular/bonded groups, category rows, unknown category values
  and constraints. Statless cores remain present. Orphan groups fail even if
  there are no cores. Utility buffs keep primary pairs, secondary raw values and
  separate granted flags/stats/conditions.
- Essence outcomes with unequal mod/weight arrays retain both arrays and
  `outcomes: null`. No uniform weights are invented.
- Constants retain exact IDs, including whitespace, and separate Value/Divisor
  numbers. The 100 ordered default-monster level rows keep their complete values.
  Monster types/varieties, ordered mod groups, effects, null links, intrinsic
  minion stats, scaling and spectre references remain facts.
- `SummonedSpecificMonsters.Id` is a numeric source field. It is retained as
  `id_raw`, without stringification, uniqueness inference or a new summon bridge.
  Empty physical tables may report actual row width zero in this profile;
  nonempty tables still require the exact declared physical width.

All source diagnostics are copied verbatim. B/C does not evaluate playable
catalogues, inherited tags, mod eligibility, SoulCore category/stacking rules,
units, actor identity bridges, interpolation or server formulas. Snipe scope and
geometry interpretations remain unresolved. It does not call Python at runtime.

Generate two fresh outputs, then run the differential verifier:

```sh
mise exec -- python3 -B tools/pobr-data-adapter/tests/smoke_native_facts.py \
  --profile items-actors-bc --native-facts /path/to/actual-a \
  --source-lock /path/to/proposed-sources.lock.json \
  --candidate-a .cache/items-actors-a --candidate-b .cache/items-actors-b \
  --repoe-source /path/to/repoe
```

Only this opt-in verifier requires a RePoE source checkout. It hashes the two
fixed pure projection modules before executing their verified buffers, without
loading package initialization or bytecode. It compares their converted views
with Rust using exact table/row references, JSON types and floating-point bits.
This intentionally reuses reviewed producer projection logic; it does not provide
an independent producer audit. The separate producer review covers that boundary.
The rest of the smoke check verifies the sealed inventory, source locks, complete
raw references, output hashes, byte determinism and the older domain collections.
The Rust adapter itself requires only the input package, external pin and lock.
