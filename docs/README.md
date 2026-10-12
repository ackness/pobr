# Documentation index

Repository-wide rules are in [CLAUDE.md](../CLAUDE.md) and the short
[AGENTS.md](../AGENTS.md); contribution requirements are in
[CONTRIBUTING.md](../CONTRIBUTING.md). Mechanics research lives in
`agent-docs/` and is input material, not final authority.

## Current guides and contracts

Keep these in step with the code. Update the relevant one when behavior changes.

| Document | Covers |
|----------|--------|
| [development-workflow.md](development-workflow.md) | Commands, caches, PR checks, CI, releases, handling external PRs |
| [contributing-mods.md](contributing-mods.md) | Adding or fixing modifiers through the overlay data channel |
| [web-contracts.md](web-contracts.md) | Upgrade guidance, build workspace, weapon sets and skill selection |
| [trade-special-affixes.md](trade-special-affixes.md) | Special affix sources and scoring in market searches |
| [jewel-search.md](jewel-search.md) | Passive jewel support and limits |
| [wegame-import.md](wegame-import.md) | WeGame share import |
| [wasm-package.md](wasm-package.md) | Standalone WASM package, release assets and size reports |
| [version-bump-architecture.md](version-bump-architecture.md) | How data version bumps are structured (`overlay-common`, test pins, vendor diffs) |

Related guides outside this directory: [Web README](../web/README.md),
[data pipeline](../pipeline/README.md) and the
[gem quality generator](../pipeline/gem-quality/README.md).

## Dated records

Snapshots of a review, investigation or data update at a point in time. They
explain why things are the way they are; they are not current task lists, and
they are not updated as the code moves on. Check the date and commit at the top
of each before relying on it.

| Document | Snapshot |
|----------|----------|
| [wasm-performance.md](wasm-performance.md) / [中文](wasm-performance.zh-CN.md) | WASM timings and sizes (2026-10-11); supplemental timings, build diagnostics and fresh PoB2 source comparison (2026-10-12) |
| [adapting-to-0.5.5c.md](adapting-to-0.5.5c.md) | Data update to `4.5.5.3` (2026-09-26) |
| [adapting-to-0.5.5.md](adapting-to-0.5.5.md) | Data update to `4.5.5.2` (2026-09-11) |
| [adapting-to-0.5.4b.md](adapting-to-0.5.4b.md) | Engine gap map and plan for game 0.5.4b (2026-07-15) |
| [backlog-reconciliation.md](backlog-reconciliation.md) | Remaining-work review (2026-09-25) |
| [pobr-build-optimization-analysis.md](pobr-build-optimization-analysis.md) | `pobr-build` refactor investigation (2026-09-24) |
| [calcs-display-comparison.md](calcs-display-comparison.md) | Calcs panel source comparison with PoB2 (2026-09-25) |
| [reference-sources-and-engine-review.md](reference-sources-and-engine-review.md) | Build-reference sources and PoB2 alignment review (2026-09-12) |
| [frontend-usability-review.md](frontend-usability-review.md) | Frontend interaction and copy review |

These files stay at their current paths because code comments and scripts
reference them.
