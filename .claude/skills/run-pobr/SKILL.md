---
name: run-pobr
description: "Choose and run PoBR development commands through mise: setup, focused checks, rule regeneration, data tools and calculation diagnostics. Use for running/building/testing PoBR; PR/review policy is covered by the project workflow skill."
---

# Run PoBR commands

Run from the repository root through `mise run <task> [args]`; use `mise tasks`
for the current task list. Keep command implementation in `mise.toml` and
`devs/scripts`, rather than duplicating environment versions or CI policy here.
The existing `driver.sh` in this skill is a compatibility forwarder.

Choose only the relevant source:

- [Maintainer guide](../../../docs/maintainer-guide.md): architecture and validation scope.
- [Change recipes](../../../docs/recipes.md): files and focused check for common changes.
- [Development workflow](../../../docs/development-workflow.md): setup, cache, CI and release operations.
- [Diagnostics](../../../docs/diagnostics.md): triage, line usage and build comparisons.
- [Data pipeline](../../../pipeline/README.md): generation and version updates.
- [Project workflow](../workflow/SKILL.md): proportionate PR/review handoff.

Available entry points; choose what resolves the current uncertainty:

```bash
mise run setup
mise run targets
mise run cache
mise exec -- cargo check --locked -p pobr-core --lib  # Optional compiler check
mise run verify build skills <filter>                # Optional runtime tests/lints
mise run regen
mise run web test <file>
mise run triage
mise run compare <build> [Stat ...]
mise run gap "<modifier text>"
```

Setup is lightweight; oracle, browser and artwork tasks prepare their own extra
dependencies. A test already compiles its target; do not prepend a product build.
Workers may use review/static evidence or run local tests/builds when useful; report
which were actually performed. Algorithms and large changes can justify earlier
focused tests. Integrate once before batch validation, prefer remote CI, and reuse
passing checks for unchanged inputs. `cargo check` still compiles dependencies;
`test --no-run` still creates binaries. Avoid automatic check/build/test chains.
Before creating build caches or running a large batch, inspect `mise run cache`
and free disk space; follow the workflow guide's storage decisions. Reuse warm
artifacts and scope any cleanup to confirmed stale data with no active consumer.
Documentation changes need diff/link review. Record observed results and limits;
requesting CI or finding a cached binary does not prove a calculation succeeded.
