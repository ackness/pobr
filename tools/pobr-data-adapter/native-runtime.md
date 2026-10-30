# Guarded native runtime assembly

`--native-runtime` projects a verified RePoE `items-actors-bc` v4 archive into
existing PoBR runtime domain types. It creates a **queryable schema-4 candidate**;
calculation stays closed with `native-profile-not-admitted`. It does not read
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
Sorceress's base attributes. One ordinary passive gets the existing whole-packet
native contract, bound to the exact source/patch/tree tag. Its amount is read
from the source. The reference index retains all 9,731 definitions and 61 class
relations; no missing Druid base becomes a phantom graph node.

Spark's base/gem, primary granted effect/stat-set, level-1 resolved stats and
costs use native stable-ID joins. `BaseResolvedValues` supplies stored damage;
`FloatStatsValues` remains evidence, never flat damage or a second effectiveness
multiplier. Constants and implicit stats stay separate. No PoB ordinal is
assigned. The cold-infused branch, other levels/quality/supports/equipment remain
outside this projection, with complete facts retained in the archive.

General tree modifier text is absent pending effective CSD selection/formatting.
Ascendancy ownership, unlock constraints, class variants, attribute choices and
jewel geometry are unprojected. An empty field does not declare an unmodelled
mechanic effectless. `base/native_coverage.json` records these omissions, and the
candidate cannot enter legacy calculation.

Sealed sidecars retain the transitive input lock, canonical raw references,
normalization decisions, required-rule declarations, acquisition status and tree
match evidence. `base/native_runtime.json` binds their digests into its assembly
key; `manifest.json` inventories every base file. All projected domain readers
and the schema-4 memory loader must pass before output reservation. The manifest
and final `assembly-receipt.json` follow base writes. An interrupted directory
without the final receipt is incomplete.

Sources remain unreviewed and `production_ready: false`. This establishes
assembly/integrity/domain loading, not selected actor-template binding, resource
initialization formulas, critical/timing outputs, build/save/export binding,
general gameplay accuracy, China numerical coverage or browser performance.
Complete the coordinated admission checks before enabling native calculation;
copying legacy defaults is not a substitute.
