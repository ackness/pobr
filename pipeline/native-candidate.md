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

These are queryable partial catalogs. Ordinary full-build calculation remains closed,
sources remain unreviewed/nonproduction, and neither gameplay correctness nor
CN numerical coverage is established. [Bounded consumer admission](../docs/native-skill-hit.md),
source approval and publication are separate coordinated paths.

## Actor recipe v2

The [player example](native-player-candidate-example.json) uses
`pobr-native-candidate-recipe/v2` and requires an additional `player` object with
exact `manifest_sha256` and `source_lock_sha256` pins for a
`repoe-native-player-context/v1` package. Supply its local directory explicitly
with `--player-context`; v1 forbids that binding, and v2 requires it. V2 uses the
same input profile and source status, with output profile
`conditional-skill-hit-v1`. Unknown fields or mixed-version profiles fail.

The player manifest, complete inventory and source lock are verified before
execution. Its embedded parent manifest must byte-match the facts pin, and its
declared parent game, patch, numerical realm and source-lock identity must agree.
Changing a patch label or re-pinning a different actor parent does not substitute
for that relationship. All three player assembler flags come from these explicit
bindings; no actor or policy is selected through a local default.

V2 verifies a 19-domain runtime inventory, including the separate sealed player
context and actor-policy artifacts. It checks their external source/parent pins,
artifact/context hashes, v2 input-lock and contract links, conditional-model
policy, evaluation and v2 assembly receipt. Rust still owns source interpretation
and `NativeData` validation. Python checks the shape and links of the assembler's
reported final data identity; it does not independently recompute the Rust
identity algorithm or grant calculation authority.

The completion marker is `pobr-native-candidate-orchestration/v2` and includes that
reported `data_identity` separately from the assembly key. Both source status and
`production_ready: false` remain unchanged; `calculation_admitted: false` denotes
the assembler's guarded candidate, not a complete Build calculation. The player
policy retains the explicit generic-OT applicability assumption. Nothing here
certifies actor pools, requirements, effective cadence, full DPS or CN data.

```sh
mise run data:native:candidate \
  --recipe pipeline/native-player-candidate-example.json \
  --recipe-sha256 4d2a54b1b62cfee62547526f22d88c74702dec0a7e37965e3f1524175567634c \
  --facts /local/artifacts/facts-v4 \
  --source-lock /local/artifacts/facts-v4/source-lock.json \
  --descriptions /local/artifacts/descriptions-v1 \
  --player-context /local/artifacts/player-context-v1 \
  --official-tree /local/references/official-tree.json \
  --compiler /local/tools/pobr-data-adapter \
  --out .cache/native-candidates/new-player-candidate
```

The example's digest identifies its tracked bytes, not source approval. Compiler
pins identify a particular prebuilt artifact; another platform/build needs its
own reviewed recipe. `revision_declaration` and byte integrity do not establish
a clean-build attestation. The old example and v1 runtime bytes remain supported.

Focused stdlib checks (no Rust build or package installation):

```sh
mise exec -- python3 pipeline/test-native-candidate.py
mise exec -- python3 pipeline/test-data-snapshot.py
```
