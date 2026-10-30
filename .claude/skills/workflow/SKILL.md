---
name: workflow
description: "Operate PoBR's agent-authored PR, independent audit, CI and release handoff with its focused validation budget. Use for preparing/reviewing a PoBR PR or changing this repository's development automation; use the global repo-workflow skill for transferable workflow design."
---

# PoBR workflow adapter

This is the project adapter. The reusable design logic lives in the independently
installed global `repo-workflow` skill; use it when researching or redesigning the
process. Ordinary work follows the existing PoBR contract, without redoing research.

Read [AGENTS.md](../../../AGENTS.md) and the full
[maintainer guide](../../../docs/maintainer-guide.md). The single operational source
is [development-workflow.md](../../../docs/development-workflow.md); contributor
expectations are in [CONTRIBUTING.md](../../../CONTRIBUTING.md). Do not copy those
policies into another checklist here.

## Apply the current contract

- Choose one focused check from [recipes.md](../../../docs/recipes.md) or the
  maintainer validation table. No per-change Cargo workspace/parity/accuracy run.
  Rule edits require regeneration; batch integration owns `mise run gate` once.
  Workflow changes use `mise run check:workflows` without compiling the product.
- Keep one reviewable problem per PR, with truthful Problem, Change, Validation
  and Remaining limits sections. Open a draft when publishing a PR is authorized
  but independent audit is unfinished. Do not infer push/PR/merge/release authority
  from a request for a local edit.
- Assign a separate reviewer the final head/base and relevant code/evidence.
  Fix blocking findings in the same PR, then write the actual `pobr-audit` result
  using the [PR template](../../../.github/pull_request_template.md). Never invent
  a review, session identity or passing command. Freshness/schema checks do not
  establish independent reasoning under shared credentials.
- Before an authorized merge, run `mise run pr-ready <N>` and use the returned
  head with `--match-head-commit`. It is read-only. If head/base changes, renew the
  affected audit/integration evidence. `PR gate` and `PR handoff` must both pass;
  hosted strict required-check rules are configured separately after first rollout.
- For calculations, cite game text/stat IDs and preserve diagnostics; PoB2 is a
  cross-check. An unsupported modifier is not automatically in scope for static
  simulation. An external contribution normally stays in its original PR.

## CI and release boundaries

[pr.yml](../../../.github/workflows/pr.yml) owns automatic change routing and the
stable code gate; [handoff.yml](../../../.github/workflows/handoff.yml) owns cheap
metadata checks; [ci.yml](../../../.github/workflows/ci.yml) owns reusable/manual/
nightly full validation; [release.yml](../../../.github/workflows/release.yml) owns
tag validation, artifact publication and production deployment. Keep these roles
separate while sharing policy scripts.

`mise run ci <pushed-ref> [--e2e]` requests full validation; verify its exact SHA and
result. E2E remains explicit opt-in. Tag publication is never implied by validation.
Use the release runbook for application/independent library/data versions, immutable
tags, SemVer/lock checks, same-run artifacts and stable-v0 deployment ordering.

When changing process, update executable policy and tests first, then the owning
workflow guide plus affected entry points/templates. Do not claim GitHub enforcement
from local files alone. Record local/committed/pushed/enforced state accurately.
