# Declared ordinary producer inputs

[The requirements](ordinary-inputs.requirements.json) select six existing ordinary
adapter domains and their fact paths. [The lock](ordinary-inputs.lock.json) records
every consumed compatibility, policy, CSD and supplemental file with its bytes and
SHA-256. CSD units include the sibling manifest that the existing reader consumes.
The target game, patch and realm are exact; `global` is not an alias for
`international`. Author-machine paths are not part of either contract.

Run from the repository root:

```bash
mise exec -- python -m pipeline.runtime.ordinary.ordinary_inputs plan
mise exec -- python -m pipeline.runtime.ordinary.ordinary_inputs verify \
  --local .cache/ordinary-portable-inputs --out .cache/ordinary-verified
mise exec -- python -m pipeline.runtime.ordinary.ordinary_inputs execute \
  --local .cache/ordinary-portable-inputs --out .cache/ordinary-produced \
  --adapter target/debug/pobr-data-adapter
```

Local inputs use `<unit-key>/<member-name>` from the lock. Only those files are
read; no search, local hint or historical pathname fallback occurs. All declared
bytes, the complete portable source handoff and exact manifest identities are
checked before any producer starts. Each domain has a fixed allowed/required
argument profile; independent source and policy hashes must match the lock.
Domain-specific row joins, policy semantics and conversion remain the existing
Rust adapter's responsibility. `verify` authenticates inputs; it does not run
Rust or validate the resulting game behavior.

The F3 source-lock correction leaves physical facts unchanged but changes their
manifest identity. Four policies therefore carry an explicit `policy_rebinding`
declaration plus their original policy bytes. Preflight ties the old/new hashes
to the verified source handoff, checks the one allowed target-manifest pointer,
and rejects any other JSON-value change. Original policies are preserved; a stale
binding fails before the first domain runs.

Production runs sequentially and records the binary hash, argv, exit status and
stdout/stderr for each command. A successful exit is insufficient: the wrapper
checks the candidate marker, target, source identity, selected fact inventory,
archived policy/source bytes, expected runtime output paths and every receipt
file's bytes/SHA. It rechecks inputs and completed domains before writing
`completed.json` last. Failures retain logs and partial domain directories without
a completion marker. Existing output directories are never overwritten.

## Independent owner inputs and local policies

The v2 lock uses `packages` to pin one or more independent owner ZIP inputs. Each
uses the existing `GitInputs` repository/full revision/path/bytes/SHA, embedded
manifest pin and expanded size contract. The current RePoE publication uses
`repoe-data-publication/v1`; its original 29 source/CSD/codec/reference members
keep their exact paths and bytes. The old whole package in `ackness/pobr` is no
longer accepted or fetched. No complete data package is published in PoBR refs.

The ten codec-policy files are not stored in the external ordinary package.
`ordinary_policy.py` materializes them from pinned small PoBR policy sources,
the 5402-byte tree policy template and the external retained version binding
`retained-policy-bindings/passive-tree-v4.json`. That binding contains the original
version-specific compatibility/exclusion/class/ascendancy and metadata values;
it is explicitly retained compatibility, not new official source approval.
Nested JSON insertion order is preserved so all ten existing policy byte pins,
including the original and rebound tree policy, remain unchanged. A changed
source, mapping, pointer or output pin fails before any producer starts.

A package's target and exact original member pins are checked before local
policies are written. Duplicate members across owners, missing owner inputs,
wrong targets, unexplained extra paths and mismatched policy source pins reject.
`publication-provenance/ordinary-manifest.json` retains the previous package's
source inventory offline. Full version mappings are absent from PoBR Git;
only their small interpretation template and precise external pins remain.

```sh
mise run data:ordinary execute --adapter target/debug/pobr-data-adapter \
  --out .cache/ordinary-produced
mise run data:ordinary execute --adapter target/debug/pobr-data-adapter \
  --offline --out .cache/ordinary-replayed
# An explicit unpublished local owner handoff uses the same exact pins:
mise run data:ordinary verify --package repoe=/absolute/path/ordinary-inputs.zip \
  --out .cache/ordinary-verified
```

Local-only/pending packages cannot silently fetch an unpublished remote. Missing
or damaged offline cache entries reject; no checked-in or historical snapshot is
consulted. Original v1 envelopes remain readable; new runs require the pinned verifier described below. A
v1 package owned by `ackness/pobr` is explicitly rejected. Its legacy `pack`
command is an explicit local diagnostic, not the production storage contract.

The actual adapter still runs six domains sequentially; the wrapper validates
its receipts and writes completion last. Full CI uploads only business outputs,
receipts, logs and the small handoff, then the runtime assembler applies the
eight declared replacements. Expanded sources, CSD, external mapping and policy
materialization remain disposable build evidence. Ordinary production does not
change CURRENT, source acceptance, the site or adopted runtime. Local input
verification and synthetic process tests do not prove remote CI or game behavior.

## Pinned upstream envelope verifier

The lock's `source_verifier` field fixes `ackness/repoe`, one full Git revision
and the exact bytes/SHA of `RePoE/__init__.py`, `RePoE/source_io.py` and
`RePoE/source_rebinding.py`. New verification or production requires all three;
there is no installed-package, old PoBR collector or unpinned fallback. Historical
completed artifacts can still be read with their own original lock.

The importer acquires these three small code files through the same Git/cache
boundary as other pinned inputs. It checks the complete inventory before running
any initializer, executes verified byte snapshots in a temporary private package,
and removes that namespace afterwards. The package has no filesystem import
search path. The upstream verifier checks the full nested portable source envelope;
no HTTP, index discovery or decoder module is part of its code closure.

For an explicit unpublished local handoff, `--verifier-library <directory>` points
to a directory containing exactly that three-file closure. The same lock pins and
symlink/inventory checks apply. `--offline` never fetches missing code: prefill the
cache or provide this explicit local directory. All code files, input bytes and
completed domains are rechecked before `verified-inputs.json` or `completed.json`
is written. A changed verifier leaves no completion marker. The receipt records
`source_verifier` with its exact library identity.

Outputs-only CI artifacts keep that small provenance receipt. The runtime
assembler compares it with the declared lock without executing the verifier or
shipping library code to players. Full facts, acquisition receipts, external
mapping and verifier code remain disposable build evidence.
