---
name: workflow
description: "Route PoBR work across the pobr/repoe/poe2-zh-cn-stats three-repository framework and to the documents that own PR, review, validation, CI and release rules. Use for preparing/reviewing a PoBR PR, deciding which repository owns a change, bumping data or releasing, or modifying development automation."
---

# PoBR workflow adapter

This adapter owns one thing: routing work across PoBR's three repositories. Every
other rule has a single home; read it there instead of copying it into a checklist.
The optional personal `repo-workflow` skill supports cross-project research and
process redesign; using this adapter does not require it.

| Need | Owner document |
| --- | --- |
| Validation tiers, review, conventions, accuracy loop, parallel-agent and commit rules | The full [maintainer guide](../../../docs/maintainer-guide.md) ([English digest](../../../AGENTS.md)) |
| Commands, caches, CI routing, `pr-ready`, merge and release mechanics, external PRs | [development-workflow.md](../../../docs/development-workflow.md) |
| External contributor expectations and which repository owns a fix | [CONTRIBUTING.md](../../../CONTRIBUTING.md) |
| Data bumps, releases, deployment checks and rollback | [RELEASING.md](../../../RELEASING.md) |
| Data design and current gaps | [data-architecture.md](../../../docs/data-architecture.md), [data-status.md](../../../docs/data-status.md) |

## Repository framework

PoBR is one of three owned repositories; data work crosses repo boundaries:

- `ackness/pobr` (this repo): engine, data contracts, the single data lock
  [`pipeline/data.lock.json`](../../../pipeline/data.lock.json) and the pinned
  import/release pipeline. Game data does not live in Git.
- `ackness/repoe`: client resource decoding, game facts and the
  `tools/pobr-data-adapter` domain policies that project facts into PoBR's
  runtime JSON. Publishes `game/<patch>-<n>` Releases and test fixtures.
- `ackness/poe2-zh-cn-stats`: locale/term curation, translation sources and
  language artifacts. Publishes `locale/<patch>-<n>` Releases with the support
  display.

PoBR adopts an owner Release only through a reviewed `mise run data:bump` PR that
changes the lock; never through a branch-tracked pull or automatic sync. The
PoB2-derived compatibility bytes in the RePoE package are frozen inputs, not a
source PoBR regenerates.

Put each fix in the owning repository: decoding, raw facts and adapter
policies → RePoE; term templates and translations → poe2-zh-cn-stats; stat
meaning, modifier rules, build semantics, engine and display wiring → PoBR. A
cross-repo handoff records producer commit, artifact digests, changed
semantics and consumer impact.

Keep one checkout per repository. Parallel agent work uses separate
worktrees/branches; when a branch merges, remove its worktree and any
orchestration checkpoint refs instead of accumulating local debris. Refs
outside `refs/heads|remotes|tags` that the project deliberately keeps
(`refs/artifacts/*` in the translation repo) are retention pins, not debris.

## CI and release boundaries

[pr.yml](../../../.github/workflows/pr.yml) owns automatic change routing and the
stable quick/integration aggregate; [ci.yml](../../../.github/workflows/ci.yml) owns
reusable/manual/nightly full validation; [release.yml](../../../.github/workflows/release.yml)
owns tag validation, artifact publication and production deployment. Keep these
roles separate while sharing policy scripts.

Data bumps and releases follow [RELEASING.md](../../../RELEASING.md): a bump PR
changes the lock (plus `data/CURRENT` and the data-bound test baselines when the
game package changes) but never the version; a separate version-bump PR plus an
immutable `v0.x.y` tag ships whatever master holds.

When changing process, update executable policy and tests first, then the owning
document plus affected entry points/templates. Do not claim GitHub enforcement
from local files alone. Record local/committed/pushed/enforced state accurately.
