# Temporary build and real-test data

PoBR tracks code, interpretation rules, contracts and exact input locks. Complete
version directories, dictionaries and seven external shared files are ignored;
`data/CURRENT` selects the intended version but supplies no payload fallback.

Run before a build or a test that reads real game data:

```sh
mise run data:prepare
# Full Rust integration also exercises the exact previous version:
mise exec -- python3 pipeline/build_data.py \
  --lock pipeline/build-data-0.5.5.4.2.lock.json --install data --keep-current
```

CI explicitly prepares data before Rust runtime tests, Web tests/builds and
script tests that inspect the real catalog. Quick Rust format/check does not need
complete data. Pure importer/archive/cache tests use small synthetic fixtures.
The previous version is installed without changing `CURRENT`; it is a byte-bound
compatibility test input, not newly certified official source data. Ordinary
production, display validation and compact release assembly remain separate gates.

[`build-data.lock.json`](build-data.lock.json) fixes the current target,
snapshot manifest bytes/SHA and full runtime identity. Its two owners use
`GitInputs`: repository, full immutable Git commit, path and exact bytes/SHA. ZIP
parts additionally pin their embedded manifest and expanded size. The retained
language Git tree pins its original manifest, then authenticates every listed
file, including evidence, before projecting only `i18n/` into runtime data.
Evidence, reviewed supplement packages and verification sidecars are never player
payloads. Existing v3 and reviewed-only display guards remain independent.

The current v2 build lock additionally requires the separate RePoE crafting facts
package. PoBR projects its category/identified-regular-stat catalog temporarily;
the game part must omit that processed file. Missing facts cannot fall back to
the retained catalog. [Crafting inputs](crafting-inputs.md) defines the eight-slot
projection, raw-value diagnostics and build receipt outside the runtime bundle.
The explicitly selected v1 rollback lock retains its historical processed bytes.

A ZIP part has schema `pobr-build-data-part/v1`, exact `target`, `kind` and a
`files` map of canonical paths to `{bytes, sha256}`. Game members use
`data/<patch>/{base,overlay,generated}/...`, the original snapshot manifest and
an explicit subset of `data/overlay-common/*.json`. Language members use only
`data/<patch>/i18n/...`. Cross-owner paths, intersections, missing members,
wrong targets and undeclared paths reject the whole composition. An archive is
processed data with its original producer/provenance; storing it in RePoE does
not reclassify retained community output as official RePoE facts.

The retained language format `poe2-retained-locale-publication/v1` is also
supported directly. Its `i18n/...` members map to `data/<patch>/i18n/...` without
rewriting bytes; authenticated `evidence/...` stays outside the runtime root.
The target is separate from observed SC client identity, which remains unknown.

Shared ownership follows the original per-file provenance:

| Owner | Files |
| --- | --- |
| RePoE retained game facts | `totem_varieties`, `passive_jewel_slots`, `boss_poise_thresholds`, `item_granted_skills` |
| RePoE retained compatibility | `high_precision_mods`, `vendor_name_aliases`, `buff_definitions` |
| PoBR interpretation/configuration code | `special_mods`, `local_mods`, nine `*_config_options` files |

The lock pins PoBR's eleven local rule files. The composer reads only that list;
external/local intersections fail. The four game facts retain their original
patch and source metadata; the three compatibility files retain their vendor
pins and limitations, including the unresolved buff-reference drift. Game part
metadata separately identifies the nine mixed numeric/interpretation or derived
outputs retained in this batch. Their business bytes remain unchanged; RePoE
storage does not move the interpretation/selection code out of PoBR.

All inputs are authenticated before a fresh complete bundle is published.
`data_bundle` verifies the schema-3 inventory and original runtime identity.
An interrupted import leaves no complete output. Explicit `--out` retains a
verified bundle; `--install data` uses temporary expanded inputs and installs only
the ignored version/external shared files. It never rewrites `CURRENT` or local
rules. Existing corrupt/different files reject rather than being repaired or
silently selected. The default loader and all CLI/WASM DTOs stay unchanged.

Downloads live in an ignored hash-addressed cache. `--offline` rejects a cache
miss or corruption and never consults tracked/history snapshots, local hints,
latest revisions or another data version. Explicit `--game` / `--language`
paths authenticate unpublished local handoffs against exactly the same lock;
`local-only` inputs cannot be fetched as if they had been published. Local paths
are command arguments, never version-lock fields. Deleting an ignored prepared
version and rerunning prepare rebuilds it from its external owner pins.

The source-derived 0.5.5.5 composition uses snapshot `73b2d77e...` and runtime
`dba0615e...`; only crafting source metadata changes from the previous
`2a03ad79...` / `d7ac23c5...` baseline. All 92 catalog business records match.
The immutable inputs are published. Fresh remote Git composition, denied-network
replay and local adoption are verified; the frozen CLI reads the new identity with
identical business output/diagnostics. Full integration CI and gameplay/source
certification remain separate. The [rollback lock](build-data-0.5.5.4.2.lock.json)
preserves snapshot `bfd31a46...` and runtime `dcfc32c2...`. Full hashes live in
the locks. The older 4.5.5.2/4.5.5.3 directories are no longer current-tree
inputs; historical Git objects are not rewritten or used as runtime fallbacks.
`check_data_ownership.py` rejects full payloads forced into PoBR's Git index.

The optional `support_display` lock also imports the translation owner's support
description asset before Web builds/tests. Its manifest, asset and complete target
trade-catalog hashes must match; the asset's original generation version and source
provenance remain unchanged. The ignored Web JSON is installed atomically only for
the selected version, never when installing rollback data with `--keep-current`.
The complete table and its language projection code are maintained in the
translation repository. PoBR retains import and display guards; this auxiliary
asset is outside the GameData bundle and does not change its runtime identity.

Focused tests: `test-build-data.py` covers owner/path separation, retained language
evidence, shared intersections, exact identity, archive/cache corruption,
missing/unpublished pins, rollback installation, compact runtime consumption and
tracked-data rejection. These synthetic contracts are not Rust calculation or
remote CI evidence.

The `test_fixtures` package retains eleven API response samples in RePoE for mock
development and UI contracts. They are opaque historical test inputs, with unknown
original client identity, rather than current game-fact references. Preparation
validates the archive, manifest, target and exact inventory before materializing
ignored `web/src/fixtures/*.json`; complete fixture tables cannot be tracked.
Small synthetic unit fixtures remain in the tests that own their behavior.

Small reviewed regression statistics live in the gamedata test fixture
`repository-test-pins.json`, bound to each exact external snapshot manifest. They
are test metadata, not imported runtime files; tests never bless or mutate the
immutable prepared bundle. Historical COMBAT references similarly select their
declared rollback version; the current-data gate remains separate.
