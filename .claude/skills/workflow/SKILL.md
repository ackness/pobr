---
name: workflow
description: "Operate PoBR's agent-authored PR, risk-based review, CI and release handoff with its focused validation budget. Use for preparing/reviewing a PoBR PR or changing this repository's development automation."
---

# PoBR workflow adapter

This is the project adapter. Follow the current PoBR contract for ordinary work.
The optional personal `repo-workflow` skill supports cross-project research and
process redesign; using this adapter does not require installing it.

Read [AGENTS.md](../../../AGENTS.md) and the full
[maintainer guide](../../../docs/maintainer-guide.md). The single operational source
is [development-workflow.md](../../../docs/development-workflow.md); contributor
expectations are in [CONTRIBUTING.md](../../../CONTRIBUTING.md). Do not copy those
policies into another checklist here.

## Apply the current contract

Local tools and commands use the latest stable mise with the project's exact pins.
Start with `mise bootstrap --only packages,tools,task`; use `mise run` / `mise exec --`.
The common setup is lightweight. Oracle, browser and artwork tasks automatically
provision their extra tools only when used. CI may use current stable native
Actions reading the same version source.

- Choose verification by risk and cost. Prose needs diff/link review; algorithms,
  large changes and uncertain contracts may justify focused compilation, tests or
  independent review before handoff. Workers can commit without a local product
  build, while reporting unrun checks and providing needed regression cases.
  The coordinator integrates first, regenerates rule artifacts once when needed,
  and prefers remote batch validation. `verify`, `gate` and local full builds remain
  optional tools. Workflow changes use `mise run check:workflows` without Rust.
- Account for storage before new build caches or a large batch: use `mise run cache`,
  check free disk space and reuse warm artifacts. Follow
  [storage decisions](../../../docs/development-workflow.md#storage-decisions).
  A dependency-store hit still requires frozen installation and actual checks.
- Describe the problem, resulting behavior, actual checks and remaining limits.
  The PR template is a writing aid, not a mandatory schema. A small obvious fix or
  prose edit does not need a new reviewer, audit record or screenshot.
- Use an independent reviewer for substantial/high-risk changes. Give them relevant
  code and evidence, identify the reviewed head/base and recheck changed risks.
  Keep corrections in the same PR. Never invent reviews or passing commands.
- `mise run pr-ready <N>` is an optional read-only merge preflight. Confirm the
  current head/checks before an authorized merge and use `--match-head-commit`.
  Do not infer publish/merge authority from a local edit. CI requires real selected
  checks, not an audit JSON or a fixed count of approvals.
- For calculations, cite game text/stat IDs and preserve diagnostics; PoB2 is a
  cross-check. An unsupported modifier is not automatically in scope for static
  simulation. An external contribution normally stays in its original PR.

## CI and release boundaries

[pr.yml](../../../.github/workflows/pr.yml) owns automatic change routing and the
stable quick/integration aggregate; [ci.yml](../../../.github/workflows/ci.yml) owns reusable/manual/
nightly full validation; [release.yml](../../../.github/workflows/release.yml) owns
tag validation, artifact publication and production deployment. Keep these roles
separate while sharing policy scripts.

PRs run relevant quick checks. Master code pushes, nightly and release tags run
full validation. A risky batch may use `mise run ci <pushed-ref> [--e2e]` before
merge; verify its exact SHA, result and relevant base. `PR gate` on a PR proves
quick checks, not full integration. E2E stays opt-in; validation does not publish.
Use the release runbook for application/independent library/data versions, immutable
tags, SemVer/lock checks, same-run artifacts and stable-v0 deployment ordering.

When changing process, update executable policy and tests first, then the owning
workflow guide plus affected entry points/templates. Do not claim GitHub enforcement
from local files alone. Record local/committed/pushed/enforced state accurately.
