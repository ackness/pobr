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

Typical entry points:

```bash
mise run setup
mise run targets
mise run verify build skills <filter>
mise run regen
mise run web test <file>
mise run triage
mise run compare <build> [Stat ...]
mise run gap "<modifier text>"
```

Setup is lightweight; oracle, browser and artwork tasks prepare their own extra
dependencies. A test already compiles its target; do not prepend a product build.
Use the focused validation budget and reuse passing checks for unchanged inputs.
Documentation changes need diff/link review. Record observed results and limits;
requesting CI or finding a cached binary does not prove a calculation succeeded.
