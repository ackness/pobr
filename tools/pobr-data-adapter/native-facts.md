# Native facts domain candidate

This opt-in adapter consumes `repoe-native-facts/v1` from RePoE revision
`b6378322a80a0dc94a0f483c0031ab2df40e1334`. It creates a **partial INTERNATIONAL
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
the authenticated receipt. The six producer file fingerprints bind the supported
producer revision (the upstream v1 receipt itself has no Git revision field).
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
directory or dangling symlink, is refused and retained. An I/O interruption may
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
receipts and failure protection. `tests/fixtures/native-facts/actual-a-manifest.json`
is only the external receipt (SHA-256
`519364561e03e351b36c3aca0d9a986cf240e420183baa25d3c22ff1513f8257`), not game
payloads. Synthetic packages reuse its producer identity with small synthetic
payloads and recomputed test pins. Real-source smoke additionally requires two
fresh independent-process outputs and an independent comparison of all converted
relationships/stats to the canonical facts. This consumer does not certify the
producer's client decoding, complete foreign-key closure, other graphs, labels,
level data, translations, gameplay semantics or runtime usability.

For the pinned actual-a artifact, compare two fresh independent-process outputs:

```sh
mise exec -- python3 tools/pobr-data-adapter/tests/smoke_native_facts.py \
  --native-facts /path/to/actual-a \
  --candidate-a .cache/first-native-candidate \
  --candidate-b .cache/second-native-candidate
```

This independently checks 9,731 definitions, 5,152 instances, 5,957 graph stat
pairs, 61 class rows, 8,349 effects, 8,283 set identities, 429 qualities and all
61 source diagnostic records, with byte-identical outputs and the 31955 fifth
stat / Snipe row 69 facts. Raw group coordinates are compared exactly, without
a tolerance that could hide a parsing change.
