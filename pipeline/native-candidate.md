# Pinned native candidate replay

`mise run data:native:candidate` is PoBR's bounded unified ingress from existing,
versioned producer artifacts. It calls the established
[`--native-runtime` assembler](../tools/pobr-data-adapter/native-runtime.md),
offline, with an explicitly supplied prebuilt adapter. It does not regenerate
raw acquisition, RePoE export, descriptions or translation phases. It neither
builds nor downloads tools, discovers sources, reads vendor/PoB2/default packs,
promotes source locks, publishes data nor changes `CURRENT`.

The [example recipe](native-candidate-example.json) fixes the current partial
international `0.5.5.5` artifacts. Obtain its SHA-256 independently from the
coordinator/reviewed handoff; the required `--recipe-sha256` authenticates exact
bytes before JSON parsing. Computing a new hash of unknown bytes is not source
approval. This example still references unreviewed acquisitions.

```sh
# The output parent must already exist; the output leaf must be absent.
mise run data:native:candidate \
  --recipe pipeline/native-candidate-example.json \
  --recipe-sha256 2786beb91a6fd04517282ac10f2311eeaa1be07f05a1b2d7bdc0c23fa706cd38 \
  --facts /local/artifacts/facts-v4 \
  --source-lock /local/artifacts/facts-v4/source-lock.json \
  --descriptions /local/artifacts/descriptions-v1 \
  --official-tree /local/references/official-tree.json \
  --compiler /local/tools/pobr-data-adapter \
  --out .cache/native-candidates/new-candidate
```

Every local binding is required. None comes from the recipe, PATH discovery,
environment variables or a latest/default selector. The command executes one
fixed argument list with no shell and an empty child environment. It has no
network operations; enforce OS network denial independently when proving an
offline replay. The pinned executable is trusted code, not sandboxed by this
Python command.

## Recipe v1 contract

The root object has exactly `schema`, `target`, `facts`, `descriptions`,
`official_tree` and `compiler`. Every nested object below is also closed:
missing/extra keys, duplicate JSON keys at any depth, non-JSON numbers and
incorrect types are errors. There are no arbitrary commands, executable
locations, machine paths or timestamps. SHA-256 pins are 64 lowercase hex
characters; the declared Git revision is 40 lowercase hex characters.

| Object | Required fields and values |
| --- | --- |
| root | `schema: "pobr-native-candidate-recipe/v1"` |
| `target` | `game: "poe2"`, `realm: "INTERNATIONAL"`, numeric dotted `game_patch`, `input_profile: "sorceress-spark-level1-q0-v1"`, `output_profile: "spark-single-hit-cast-cost-v1"`, `source_status: "unreviewed-acquisition-draft"`, `partial: true` |
| `facts` | Exact `manifest_sha256` and `source_lock_sha256` for `repoe-native-facts/v4` |
| `descriptions` | Exact `manifest_sha256` and `source_lock_sha256` for `repoe-native-descriptions/v1` |
| `official_tree` | Exact `sha256`; `game_patch: null` (unknown) |
| `compiler` | Exact `binary_sha256`, `revision_declaration` and expected assembler `implementation_sha256` |

The two producer manifests must identify the recipe's exact game/patch,
international realm, partial/nonproduction state and unreviewed acquisition
status. Their complete file inventories, sizes and byte hashes, both source
locks, the official tree and executable are checked before invoking the adapter.
The external facts source-lock binding must match both the recipe and embedded
lock bytes. The descriptions source lock is bound inside its package.

The adapter retains its existing source/decoder contracts, physical decoding,
joins, tree matching, domain projection and loader checks. Python adds envelope
verification, not another decoder. It checks the complete fixed profile's
17-domain schema-4 inventory, every output byte hash and JSON document, projected
target/profiles, input/compiler/acquisition cross-links, partial coverage and
the assembler's final receipt. The expected implementation fingerprint must
match the sealed input lock and assembly receipt. Input, recipe, executable and
orchestrator fingerprints are checked again before completion.

## Completion and provenance

The output is an isolated wrapper:

```text
new-candidate/
  recipe.json                  # exact input recipe bytes
  runtime/                     # unchanged sealed assembler namespace
    base/*.json
    manifest.json
    assembly-receipt.json
  orchestration-receipt.json    # atomically installed last
```

The deterministic orchestration receipt inventories `recipe.json` and **all**
runtime files, including the runtime manifest and assembler receipt. It records
the recipe SHA, target/status, assembly key, binary/implementation pins and
relative orchestrator source fingerprints. It contains no local bindings or
timestamps; relocating identical inputs preserves all output bytes.
`revision_declaration` stays a caller declaration; `clean_build_attested: false`
explicitly records that hashing this prebuilt binary does not prove a clean Git
build. No source approval or clean-build attestation is inferred.

Only an absent output with an existing parent is accepted, outside production
data and input/tool directories; symlink components and special files are
rejected. Failures retain any incomplete wrapper for diagnosis and never issue
the orchestration receipt. Even a successful assembler receipt alone is not
orchestration completion. A complete wrapper requires a valid final receipt and
an exact matching file inventory, with no unlisted files/directories. Replays
always need a new output leaf; this command never repairs or overwrites one.

These are queryable partial catalogs. Calculation admission remains closed,
sources remain unreviewed/nonproduction, and neither gameplay correctness nor
CN numerical coverage is established. Later source approval, consumer admission
and publication belong to separate coordinated work.

Focused stdlib checks (no Rust build or package installation):

```sh
mise exec -- python3 pipeline/test-native-candidate.py
mise exec -- python3 pipeline/test-data-snapshot.py
```
