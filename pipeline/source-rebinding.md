# Draft source evidence rebinding and portable ordinary input

`source_rebinding.py` makes a new, explicitly **draft** source lock and facts
manifest from an externally pinned existing facts package and an externally
pinned evidence recipe. It runs offline using the Python standard library and
the existing client-source primitives. It performs no acquisition, decoding,
source admission, ordinary domain production, runtime assembly or publication.

```sh
mise exec -- python pipeline/source_rebinding.py create \
  --source /absolute/existing-facts \
  --recipe .cache/rebinding/recipe.json --recipe-sha256 <full-sha256> \
  --out .cache/rebinding/fresh-a

mise exec -- python pipeline/source_rebinding.py verify \
  --archive .cache/rebinding/fresh-a/ordinary-input.zip \
  --handoff .cache/rebinding/fresh-a/handoff.json --handoff-sha256 <full-sha256> \
  --out .cache/rebinding/extracted

mise exec -- python pipeline/test-source-rebinding.py
```

Output directories must be absent and disjoint from inputs. Inputs, output paths
and archive entries reject symlinks, traversal, case collisions and file/directory
collisions. This assumes local files without concurrent writers, following the
[client-source acquisition contract](client-sources.md). There is no JSON-size
cutoff: external byte/hash pins and the declared exact archive inventory bound
reads and expansion. The input's native facts and acquisition limits still apply
when the existing readers are invoked.

## Recipe and checks

The recipe schema is `pobr-source-rebinding-recipe/v1`:

| Field | Contract |
| --- | --- |
| `original` | `manifest` and `lock`, each with exact `bytes` and lowercase `sha256` |
| `evidence` | Portable filename → `{path, bytes, sha256}` for each explicitly selected retained record |
| `sources` | Every original source key → `{resource_count, dat_table_count, supplements}`; each supplement has `kind` and `evidence` filename |
| `groups` | `evidence` names the group-membership JSON; `counts` and `receipts` map the same group keys to expected counts and acquisition receipt filenames |
| `replacement` | `group`, `online`, `offline`, `selection`, `ranges`, `original_bc`, `range_count` identify the accepted replacement boundary |
| `retired` | Entries with exact original `{kind, sha256}`, replacement evidence filename and replacement `kind` |
| `excluded_domains` | Explicit limits and ordinary inputs supplied separately |

Groups must be disjoint and cover exactly the selected source resources. Receipt
raw pins must equal the original lock and manifest. Receipt game, realm and patch
must agree; acquisition locations must agree with every original location field.
The replacement online/offline raw observations and selection must agree, and
offline evidence must record zero HTTP. Each replaced BC range must exist in the
original receipt and preserve URL, final URL, start, length, total, ETag and hash.
Its new `http-range` provenance stays separate from the original cache-copy record.
These are consistency checks of accepted retained observations, not fresh network
observations or independent source acceptance by the producer.

Original root/source fields are retained. The new lock adds the exact missing BC
location tuples from the selected acquisition records, computes scope from actual
resource paths, and retains the previous scope/acquisition under `rebinding`.
Unavailable original receipt identities are explicitly retired there; their
replacement hashes become new acquisition evidence. Every active evidence hash
must have a bundled file. The old missing receipts are neither recovered nor
relabeled. Other source-specific context records are an explicitly pinned closure,
not an instruction to recursively read every historical path mentioned in them.

The manifest's lock descriptor, source-lock hash, selected-source hash and
acquisition evidence are rebound together. Selected-source hashing uses sorted,
compact UTF-8 JSON plus a newline, matching the native reader. All other manifest
fields and **all other payload bytes** are preserved. This does not assert that
the original exporter ran again with the new metadata. Exact original manifest
and lock bytes remain in the archive for the before/after identity boundary.

## Scope of the F77 input

The retained F77 manifest `56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0`
contains 77 physical DAT tables and nine projections. Its selected source has
78 resources: 77 DAT and the default PSG. The ancestor source's 40 resources are
39 DAT plus PSG, an exact subset of those 78. The two source objects do not imply
118 acquired resources. Both old scope strings describing eight tables are stale.

The producer checks the manifest inventory and each table's raw-resource metadata
against the lock, then joins acquisition records across base9, A0's seven, A1's 24
and BC's 38. It enumerates additional resources in those receipts separately;
the four snipe CSD resources in the BC receipt are outside F78. The accepted F1/F2
replacement boundary supplies A1's new observations and BC's 14 new range records.
Its acceptance record is evidence about those replacements only.

The coordinator's F3 decision is recorded separately in
[`sources.lock.json`](sources.lock.json), under
`acquisition_acceptances.f77-0.5.5.5-international`. It accepts the bounded
78-resource acquisition and corrected evidence binding at manifest `050fcf93`
and lock `63905d13`; full pins, the immutable Git transport and independent review
identity are in that entry. Paths for the facts, source lock and evidence are
relative to the inner source archive. The outer producer archive also contains
unaccepted supplemental inputs, so its transport pin is not a blanket approval.
Historical draft statuses and frozen bytes remain unchanged. The producer cannot
create this acceptance, and a different source pin does not inherit it. This is
an acquisition decision, not a runtime release gate or CURRENT adoption; later
sources and target applicability still need their own decisions.

## Portable interface

`ordinary-input.zip` is sorted and uses fixed ZIP timestamps, regular-file modes
and DEFLATE level 9. Identical inputs produce identical bytes in the pinned Python
environment; cross-zlib byte equivalence is not asserted. `handoff.json` records
its path, full byte/hash pin, expanded bytes/files, input-manifest pin and new
facts manifest/lock pins. Authenticate the handoff hash before verification.
ZIP expansion verifies exact member names, modes, byte counts, hashes, draft
status, scope, receipt bindings and payload equivalence before creating an output.

The archive has this interface:

```text
input-manifest.json                 # exact portable file/hash inventory
facts/manifest.json                 # new draft envelope
facts/source-lock.json              # new draft evidence/scope binding
facts/tables/*.json                 # unchanged full physical F77 tables
facts/*.json                        # unchanged nine F77 projections
provenance/original-manifest.json   # original exact metadata bytes
provenance/original-source-lock.json
evidence/*                         # explicitly selected, verbatim records
rebinding.json                     # old/new boundary, equivalence and exclusions
```

An import CI caller can pin the ZIP and handoff at an immutable Git revision,
verify/extract it, and pass `<extracted>/facts` plus
`handoff.facts_manifest.sha256` to ordinary producers. No local acquisition paths
need to be followed. The original absolute path strings inside frozen receipts
remain historical provenance, with portable paths/pins supplied by the new binding.
The lock's `rebinding.evidence_files` paths are relative to the portable archive
root, alongside `facts/`, rather than relative to the lock file's directory.
Provenance remains outside player DTOs. No runtime import, data-import lock, CI or
mise wiring is changed by this producer.

The bundled closure retains source locations, recorded raw pins, original and
replacement receipts, selected index/location metadata, and replacement acceptance.
It excludes raw DAT/PSG bodies, compressed response bodies, full indexes and decoder
environments; it supports offline ordinary facts input, not acquisition replay.
Supplemental tables/metadata, locale/CSD and player-context exports, ordinary
producer outputs/receipts, prepared policies/templates, compatibility runtime
paths, shared overlays and translations remain separate inputs. Their admission
and target applicability do not follow from this artifact. Existing ordinary
producer receipts remain bound to their original manifest; rebinding this input
does not retroactively change frozen downstream receipts.

For integration, use the new `create`/`verify` CLI directly or import
`verify(archive_path, authenticated_handoff, optional_absent_output)` from
`source_rebinding`. The existing lightweight `client_sources.load_source` and
`native_candidate.package` can check the new lock and complete facts envelope.
Those checks do not run the Rust physical decoder or ordinary domain producers.
The focused tests cover count/scope drift, receipt pin replacement, payload changes,
mixed identities, path attacks, deterministic replay and larger retained JSON.
