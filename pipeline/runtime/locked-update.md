# Locked data candidates and explicit local selection

Game acquisition and export belong to RePoE; language production belongs to the
translation repository. PoBR consumes a complete [build-data lock](build-data.md)
with exact owner revisions, archive/member pins, snapshot and runtime identities.
A patch number alone is insufficient. [Latest complete publication discovery](../latest/latest-data.md)
is separate from this immutable local preparation/selection command. PoBR has
no downloader, table exporter or vendor-based upgrade path.

```sh
# Prepare a fresh complete candidate. CURRENT and installed data remain unchanged.
mise run data:update --lock pipeline/data.lock.json --out .cache/update-review
# Optional --patch only checks the lock's target; it cannot relabel old data.
mise run data:update --lock pipeline/data.lock.json \
  --patch 0.5.5.5 --out .cache/update-review-2 --offline
# Explicit local selection, including a same-version refresh:
mise run data:update --lock pipeline/data.lock.json --out .cache/update-select \
  --activate --expected-current 0.5.5.5
```

`--lock` and a fresh `--out` are required. `--activate` and `--expected-current`
must occur together. Before preparation, a mismatched requested patch or current
marker fails. The prepared bundle is retained for review even if later validation
or selection fails. `--offline` and explicit local owner inputs use the same
byte/hash checks as normal preparation. There is no discovery or old-snapshot
fallback. `data:version` reports the selected local marker; official version
observation runs in the independent RePoE checkout with `mise run client:version`.
An observed version never edits PoBR locks or selects CURRENT.

Selection requires an installed, hash-valid schema-3 previous snapshot. It verifies the
candidate and requires the entire shared layer to match the installed one. A custom
`--data-root` names that installed directory directly, regardless of its basename.
A change to shared rules/facts requires a separate reviewed migration; this command
does not overwrite them or claim arbitrary cross-version shared compatibility.

The isolated validation root includes the target, common layer and read-only
historical snapshot links. Sequential version, GameData and parity gates run on
that root before publication. `POBR_BLESS_PINS` is removed; the command cannot
bless counts or numeric goldens. Gate logs go to stderr and the final result to
stdout. The new runtime identity and original installed files are checked again
before the existing publication/rollback transaction replaces the target and
atomically changes CURRENT. A failed publication restores the old target; a failed
rollback retains the recoverable backup and reports its path.

A local `patch/` already belonging to the target directory survives a same-version
refresh. It stays outside the sealed runtime identity and must pass the normal
consumer gates. An old version's patch is not inferred to apply to another patch.
Ordinary pinned installs also preserve that explicit local layer and recognized
maintenance caches, while rejecting unknown extra runtime files or changed pins.

For a selection based on a previously observed installed runtime, also pass
`--expected-runtime <64-hex-runtime-identity>`. It requires `--activate`, checks
the previous snapshot before preparation, and checks it again under the selection
lock before validation/publication. This distinguishes two installed compositions
with the same patch number: a late candidate cannot replace a different runtime
that was selected first. The selection receipt records the previous runtime.
It does not replace the separate comparison of default lock/composition metadata
when adopting a candidate in Git.

This operation selects local data only. Source acceptance, interpretation review,
CI, app assets/deployment and rollback of application code remain separate. After
an authorized target/lock update, `mise run data:prepare --lock <lock>` prepares
its fixed build assets and test inputs; `mise run web sync-data` updates the local
browser data. No complete snapshot or archive is committed to PoBR or published
in its data refs. User patches are not silently included in owner packages.
