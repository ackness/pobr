# Historical official tree inputs

RePoE owns acquisition, storage and packaging of the fixed historical Git tree.
PoBR imports its immutable package for a diagnostic tree-domain adapter. This is
separate from the current ordinary DAT/PSG tree and from runtime adoption.

The [input lock](tree-inputs.lock.json) uses
`pobr-historical-tree-input-lock/v1`: a source id and one RePoE Git package with
full revision, archive path/bytes/SHA-256, manifest bytes/SHA-256 and expanded size.
Publication metadata distinguishes a local handoff from a remotely available
object; local-only pins do not prove remote CI can acquire it.
The pinned package's commit is kept alive by RePoE's
`handoff/historical-official-tree-0.5.5` tag; consumers fetch its full revision, not the tag.

The ZIP contains exactly `data.json` and `manifest.json`. The latter has schema
`repoe-official-tree-input/v1`, status `historical-diagnostic-candidate`,
`production_ready=false`, the original source declaration, raw file pin, plan pin
and producer declarations. PoBR requires that declaration to equal the selected
[source entry](../imports/sources.lock.json), except for its owned `corrections` policy.
Producer pins are package-bound metadata; this importer does not execute the
producer or turn its declarations into source or semantic certification.

```sh
mise run data:tree:source
mise run data:tree:source --offline
mise run data:tree:candidate --offline --out .cache/historical-tree-candidate
```

`tree_inputs.py` uses the existing GitInputs/acquire/unpack path. It checks package
ownership, immutable revision, exact ZIP membership, expanded size, manifest and
raw pins, source identity, diagnostic status and the import receipt. Paths with
parent traversal or symlinks fail. An existing corrupt import fails without
repair. A missing import can be reconstructed from the complete, pinned archive
cache offline; a bare old `.cache/sources/<sha>/data.json` cannot replace a RePoE
package. There is no direct upstream HTTP, latest or alternate-source fallback.

Downloads remain in `.cache/data-import-downloads/<archive-sha256>`. Verified raw,
manifest and completion receipt are temporarily materialized under
`.cache/tree-inputs/<lock-sha256>/`. Lock/source declarations are checked again
before returning. No full data or archive enters PoBR Git or its artifact refs.

The tree candidate still uses PoBR's existing adapter, exact quarantine policy,
output guards and deterministic domain receipt. Its provenance now includes the
independent input package and importer/lock fingerprints; the imported raw,
manifest and receipt are rechecked after adaptation. The original bytes and two
Druid orphan `overridePairs` remain in the source package. Only the pinned
diagnostic normalization removes those pairs; nodes and replacement definitions
remain. Other missing references still fail. This is not a new semantic decision.

The retained source is GGG export `bd87e651`, upstream version `0.5.5`, with
unknown game-patch applicability. Its original rights scope remains unreviewed
for product redistribution. No fresh Git-origin observation is claimed by
repackaging old fixed bytes. The candidate has no runtime manifest, cannot become
a full portable pack, and never changes CURRENT, source acceptance or the current
ordinary tree. `--offline` also reaches the existing Cargo invocation; actual
adaptation needs the declared Rust tools, not a hidden PoB2 preparation step.
