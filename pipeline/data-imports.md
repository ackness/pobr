# Declarative external data imports

[`data-requirements.json`](data-requirements.json) declares what PoBR consumes;
[`data-imports.lock.json`](data-imports.lock.json) pins which published Git inputs
supply it. The importer downloads the two repositories' archives and the exact
translation implementation into `.cache/`, validates their identities, then
projects the declared display views. No developer-specific checkout path is
required in either file.

The display importer covers target-bound native text. Complete build/test data is
prepared separately by [`build_data.py`](build-data.md) from the independently
published game and language owners. [`runtime_pack.py`](runtime-bundle.md)
combines that exact temporary snapshot, ordinary producer outputs and the display
projection into three verified runtime blocks. The site build uses the exact
prepared adopted snapshot. Generating a candidate does not change `CURRENT`,
certify a source client's patch/realm, or admit a source.

## Configuration

- `target` fixes the game's patch and realm independently of observed clients.
- `repoe.tables` names stable keys and physical display fields required by the
  consumer. Both inflection fields remain in the declared 18-field contract.
- `translation.scopes` selects per-ID runtime indexes. Currently these are base
  item, character and ascendancy names, which have concrete Web callers.
- `translation.literals` preserves all catalog conflict participants. All
  applicable literal texts can be displayed, but incomplete/ambiguous names
  still block old dictionary fallback even if their per-ID scope is omitted.
- `translation.supplement` is optional. `{"mode": "reviewed-only"}` is the only
  enabled mode: the lock then also pins one compact reviewed-supplement package
  (full Git commit, path, archive and manifest bytes/SHA, expanded inventory) and
  the library's `src/translation-workbench.mjs` at its own Git revision/bytes/SHA.
  The six-module official library closure and `catalog.producer` guard do not
  change; the module is materialized next to them and never listed in
  `library.files`. Omitting the mode requires the lock to omit both pins.
- The lock fixes full Git commits, archive paths, SHA-256 and declared sizes,
  expanded inventories, manifests and every executable library module. Updating
  requirements does not silently advance a source revision.
- `runtime_pack` declares the complete release format, three blocks and projection.
  Its build input lock preserves the exact game/language assembly and retained
  compatibility ownership. It does not claim retained compatibility was exported
  as official RePoE facts.

The importer checks the RePoE/catalog binding, target identity, requested fields,
and producer closure. V3 requires the pinned original verification sidecar and
the library's independent full binding reconstruction. Unknown source identity
stays unknown; accepted text applicability is restricted to the declared target.
V2 retains its original strict comparison. Regional or version differences stay
rejected rather than being classified as missing official translations.

With a reviewed supplement the importer additionally requires that every runtime
source is exactly the current target catalog identity (patch, realm, locale,
catalog SHA-256 and verification pin) and forwards the compact package's manifest
pin and the separate workbench module pin in `display-request.json`. A package
built for another catalog, revision or scope rejects the whole import. The
workbench revision is pinned independently of the catalog archive because it is a
newer library commit; both stay reproducible full commits. The supplied package
may legitimately contain zero reviewed records: that publishes the unchanged
official projection rather than approving a draft.

## Running and replaying

```sh
mise run data:imports --out .cache/external-display-a
mise run data:imports --offline --out .cache/external-display-b
```

The first command fetches shallow fixed Git commits over HTTPS and only the requested blobs,
without checking out either repository. The second uses only
the verified content-addressed download cache. A missing offline input or damaged
cache entry fails; it never selects another revision or falls back to a local
library checkout. Every output directory must be fresh. `--requirements`,
`--lock` and `--cache` can select explicit alternate configurations/cache paths.

Archive members must exactly match the pinned manifest inventory and their byte
hashes. There is no fixed JSON size ceiling. Declared bytes, actual bytes and
SHA-256 still have to match. ZIP data is checked before publication; paths cannot
escape the staging directory. Runtime output remains only
`runtime/display_texts.json`. `runtime/receipt.json` keeps decisions offline;
source tables, FK witnesses and `verification.json` remain build inputs, and so do
the reviewed-supplement package, its workbench module and the large catalog
sidecar. Players never download them.

For an independently assembled **complete** runtime snapshot, Web synchronization
accepts an external root using the existing manifest and loaders:

```sh
POBR_DATA_ROOT="$PWD/.cache/complete-release/data" mise exec -- pnpm --dir web sync-data
```

An explicitly supplied root must exist and pass the same snapshot validation;
it does not fall back to repository data. The display candidate above is not
such a complete root. The [portable runtime bundle](runtime-bundle.md) remains
the shared integrity contract for CLI/WASM snapshots. Replacing all checked-in
game payloads uses the two independently owned inputs in the build data lock;
source/adoption validation remains separate. See the temporary build-data and
runtime bundle topics for preparation and materialization.

## Verification

`mise run check:workflows` includes import/archive/cache failure cases,
projection and v3 sidecar contracts, and external-root synchronization tests.
The supplement contract adds pin, missing-module, producer-mismatch, v3-Buffer,
exact-context and bound-to-another-catalog cases that must publish nothing. The
full CI Web job additionally runs the actual pinned imports. Compact display
and complete runtime candidates are
uploaded independently; build-time source archives and verification tables are
excluded from `web-dist`.
