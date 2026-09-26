# PoE2 0.5.5c data update

On 2026-09-26, GGG's patch protocol returned `4.5.5.3`, replacing the
previous active snapshot `4.5.5.2`. The latest
[official patch notes are 0.5.5c](https://www.pathofexile.com/forum/view-thread/4006357)
(2026-09-18), covering quest, encounter and crash fixes.

The configured English and Traditional Chinese tables were exported with
`pathofexile-dat@15` from `https://patch-poe2.poecdn.com/4.5.5.3/`.
The official passive tree was refreshed from
[GGG's export](https://github.com/grindinggear/poe2-skilltree-export).
PoB2's `dev` and default HEAD still resolved to the existing reference
`ce566eac45ea8a86477f513c7ee65a1ebe60014e`, so the vendor pin is unchanged.

The resulting `base/` files and gem-quality effect values are identical to
4.5.5.2. This includes the equipment, skill, modifier and passive-tree data
used by PoBR; it does not imply that every game-client asset is unchanged.
The quality receipt records unchanged effect/stat IDs and scopes, identical
input-table hashes and no changed effect values. It attests successful
official-table export, not an independent CDN bundle-byte comparison.

Regeneration also brings derived artifacts in line with the existing engine:
62 stat-ID mappings gain `GlobalLimit` tags or Arrow keyword restrictions,
and eight precompiled modifier entries gain the corresponding limit tags.
These are regenerated mapping changes, not new game affixes. Curated constants
and special modifiers remain unchanged. Historical snapshots and the
`4.5.4.8` numerical golden reference are retained.

The Simplified Chinese dictionary is pinned to upstream commit
`28d683c99600eb407b4e014ccaf8247532fb4607`. Its content is unchanged and its
source timestamp remains 2026-07-06; it is not a new China-client export.
The downloader now resolves the default HEAD instead of assuming `master`.
Vendor setup now creates an independent repository before fetching and
propagates failures without recording a successful pin.

To run a future update, query and download the then-current patch:

```sh
bash pipeline/bump-version.sh
```

GGG may remove old CDN snapshots. `--skip-download` requires local raw tables
and an export receipt matching the requested version; it is not an offline
re-download of historical client data. In a checkout without Web dependencies,
the final static-data copy can run directly with `node web/scripts/sync-data.mjs`.

The update gate audits all shipped modifier sources against pinned PoB2,
loads and calculates across committed snapshots, tests `pobr-gamedata` against
the candidate, and runs `parity_no_regression` on its unchanged numerical
golden reference. Parser recognition is not proof of complete calculation
support; existing unsupported effects remain diagnostics.

Validation completed for this snapshot:

- Modifier audit: 41,750 samples, zero regressions and zero new gaps against
  4.5.5.2. Existing PoBR/upstream gaps remain in the report.
- Multi-version calculation: two tests passed across all six committed versions.
- Candidate `pobr-gamedata`: 208 unit/integration tests passed; no doctests.
  The version-discovery test now honors the runtime override so candidates can
  be checked before promotion. Its default-marker path also passed separately.
- Numerical `parity_no_regression` passed with the original golden baseline.
- Workflow suite: 29 tests run, one skipped; dictionary suite: three passed.
  Formatting, targeted Clippy and shell checks passed.

After the gate, `data/CURRENT` was advanced and Web static data synchronized.
This is a local data update; no site deployment or release was performed.
