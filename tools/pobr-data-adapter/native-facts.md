# Native facts domain candidate

This opt-in adapter consumes `repoe-native-facts/v1` from RePoE revision
`b6def2152dfc48d8ec49c7a237b2e973c3830e97`. It creates a **partial INTERNATIONAL
0.5.5.5 candidate**, never a runtime data manifest or production-ready package.
It does not update `data/`, `CURRENT`, defaults, calculation rules or PoB compatibility.
The producer contract is `RePoE/docs/native_facts.md` in that revision.

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
