# Reference sources and PoB2 alignment review

Checked on 2026-09-12. Build references are temporarily hidden; their components,
matching logic, public historical fixtures and catalog generator are retained.
Future player sharing can reuse `BuildReference` and the existing equipment
comparison flow. No sharing service or new upstream integration was added.

## Candidate data sources

| Source | Verified surface | Fit for PoBR |
| --- | --- | --- |
| [Mobalytics PoE2](https://mobalytics.gg/poe-2) | Current 0.5.5 guides, creator builds and a build planner. The public [builds widget documentation](https://mobalytics.gg/builds-widget-documentation) describes LoL champions and roles, not PoE2. | Link to guides. No documented third-party PoE2 build API was found in this review; seek a supported integration before using internal requests. |
| [RePoE PoE2 export](https://repoe-fork.github.io/poe2/) | Version 4.5.5.2 JSON files for mods, base items, skills, gems, augments and stat translations. [Project documentation](https://repoe-fork.github.io/) identifies it as data for tool developers and also links a PoB data export. | Useful for offline data comparisons or future missing-domain imports. It contains game definitions, not a current-season player-build index. Keep our existing GGG extraction pipeline and explicit version pins. |
| [PoE2DB](https://poe2db.tw/us/) | Current patch, item, modifier and mechanic reference pages. No documented public third-party API was found in the pages/search reviewed. | Human-readable cross-checks and source links; no automatic build ingestion proposed. |
| [poe.ninja API](https://poe.ninja/docs/api) | Public economy endpoints; builds, profiles and PoB endpoints are explicitly internal and unavailable for third-party use. | No automatic build synchronization, including scheduled offline scraping. |

The PoE2 Wiki API sandbox encountered its anti-bot challenge during this review,
so its programmatic availability was not verified. No challenge bypass or
internal endpoint reverse engineering was attempted.

For future shared builds, retain source, league/mode, data version and update
time. Similarity should be evaluated within an explicitly chosen compatible
reference set; old fixture scores must not be presented as current-season
player rankings. This is a future product constraint, not a new backend contract.

## Current upstream alignment

- `git ls-remote` returned
  [`ce566eac45ea8a86477f513c7ee65a1ebe60014e`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/ce566eac45ea8a86477f513c7ee65a1ebe60014e)
  for both upstream `HEAD` and `dev`. The local vendor checkout and active
  generated parser/skill overlays already use this commit, dated September 10.
- GGG's patch-protocol query returned `4.5.5.2`, matching `data/CURRENT` and the
  active dataset. RePoE independently lists the same patch version.
- The existing [0.5.5 update](adapting-to-0.5.5.md) already includes extraction
  compatibility for the current Lua exports. No newer vendor or game-data
  version was available to import. The vendor's local runtime shim was preserved.
- `overlay/special_mods.json` is a curated layer and retains historical source
  provenance. The current automatic extraction is in
  `generated/special_vendor.json`, pinned to `ce566eac`; an old provenance field
  in the curated layer alone is not evidence that regeneration was missed.

## Calculation checks and their limits

The existing targeted checks passed without changing formulas or data:

- `cargo test -p pobr-build --test parity ninja_parity:: -- --nocapture`:
  seven tests passed, including defensive/offensive/DoT regression and the
  unsupported-line report. These tests intentionally use golden data version
  `4.5.4.8`, not the active 0.5.5 dataset.
- `cargo test -p pobr-build --test parity special_parsemod_differential -- --ignored --nocapture`:
  the live PoB2 parser ran successfully; all seven instantiable samples from the
  version-specific curated layer had matching modifier-name sets. This check
  does not cover the entire common/generated modifier library or prove numeric,
  condition and tag equivalence.
- `cargo test -p pobr-build --test parity multi_version:: -- --nocapture`:
  both tests passed across all five committed datasets, including `4.5.5.2`.
  They check sane output and determinism, not current-version numeric parity.

No new calculation defect was established by this bounded review. Existing
unsupported and partial modifier diagnostics remain meaningful: the corpus
report includes unmodeled effects, separately handled lines and entries without
an oracle verdict. Neither a passing regression gate nor refreshed JSON proves
that every game mechanic is implemented. Future numerical fixes should start
from a reproducible input and a same-version PoB2 comparison.

## Frontend changes and validation

- Navigation and saved-tab restoration exclude historical build references;
  existing saved builds are untouched.
- Main content can grow to 2400px. At viewport widths of at least 1800px, skills
  use two columns, equipment uses more slot columns, and calculation sections
  adapt to available width. Narrow-screen rules remain in place.
- Web build and eight matching unit tests passed. The relevant Playwright run
  passed 17 tests covering 320/768/1024/1440/1920/2560/3440px, populated workflows,
  saved-tab fallback and navigation. Six retained reference-panel workflows
  are explicitly skipped while that feature is hidden.
- Wide populated skills, equipment and calculations were visually inspected.
  No Rust/WASM contract was changed, and no full workspace/release gate was run.
