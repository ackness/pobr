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
- [Data pipeline](../../../pipeline/README.md): pinned owner-data import and checks; [RELEASING.md](../../../RELEASING.md) for data bumps and releases.
- [Project workflow](../workflow/SKILL.md): proportionate PR/review handoff.

Available entry points; choose what resolves the current uncertainty:

```bash
mise run setup
mise run data:prepare                                # Import pinned owner data
mise run data:check                                  # Offline: tracked-data ownership, clean worktree
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
Which checks to run is decided by the maintainer guide's
[validation tiers](../../../docs/maintainer-guide.md#验证分层); before new build
caches or a large batch, follow the
[storage decisions](../../../docs/development-workflow.md#storage-decisions).
Report the commands that actually ran; requesting CI or finding a cached binary
does not prove a calculation succeeded.
