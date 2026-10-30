# Repository Guidelines

Read the [maintainer guide](docs/maintainer-guide.md) for architecture, validation scope and
cross-cutting conventions; it links the topic documents (mechanics, outputs,
build model, PoB2 differences, diagnostics). Use current code and regression
tests to resolve disagreements with any document.

## Project Structure & Module Organization

- `crates/` contains seven libraries: data definitions, game-data loading,
  localization, modifier calculation, passive trees, item editing, and build
  orchestration. `crates/pobr-core/src/` separates `model`, `parse`, `rules`, `ingest`,
  `aggregate`, `calc`, and `attribute`.
- `apps/` contains CLI and WASM/JSON bindings.
  `web/` is the React/TypeScript application; its backend consumes the
  `apps/pobr-wasm/src/build_api/` JSON contract.
- `tools/` and `pipeline/` generate and verify versioned `data/` snapshots.
  `web/public/` holds static assets and the Pages Worker.
- Rust integration tests live in each crate's `tests/`; frontend unit tests
  accompany source files, with browser tests in `web/e2e/`.

The project-specific workflow adapter is
[.claude/skills/workflow/SKILL.md](.claude/skills/workflow/SKILL.md).

## Build, Test, and Development Commands

Local and agent commands run through mise: `mise run <task>` or
`mise exec -- <raw command>`. Start a new machine with
`mise bootstrap --only packages,tools,task`; everyday setup does not build the
product. Oracle, browser and artwork dependencies are prepared by their own tasks.
CI may use current stable official/maintained Actions reading the same tool pins.

Run from the repository root:

```bash
mise run test core parser mod_parser::   # one suite, one filter
mise run verify build skills support_gating::
mise run regen                           # coordinator: once after a rule batch
mise run targets
mise run web test src/lib/skillSelection.test.ts
```

For the common small changes (one game line, one Calcs field, one mechanic),
follow [the change recipes](docs/recipes.md): they name the files to edit and
the relevant verification options, so no codebase exploration is needed. See
[Web setup](web/README.md) for WASM/data preparation and
[development workflow](docs/development-workflow.md) for cache policy. Reuse one
target per worktree and run Cargo commands sequentially.

## Validation Budget

Choose evidence by the change's risk and the cost of obtaining it. Local compilation
and tests are available to every agent; they are not a prerequisite for every commit.

- **Documentation:** review the diff, links and examples. No product build or extra
  reviewer is needed for an ordinary prose edit.
- **Implementation:** inspect changed code, callers and relevant tests; add regression
  assertions for changed behavior. Use focused tests, compilation or independent
  review when they resolve real uncertainty, especially algorithms and large changes.
  An expensive local check may be deferred explicitly to the coordinator/CI.
- **Cheap does not mean free:** optional `mise exec -- cargo check --locked -p <crate>
  --lib` still builds dependencies, macros and build scripts. `cargo test --no-run`
  still creates test binaries; a name filter still compiles the selected test target.
  Do not automatically chain check, build, Clippy and test.
- **Per batch:** the coordinator integrates workers first, regenerates rule artifacts
  once when needed, and chooses one integration validation. Prefer
  `mise run ci <pushed-ref>` for a risky batch; `mise run gate` is an explicit local
  alternative on a warm checkout. Do not require both for unchanged inputs.
- **Remote defaults:** PRs get relevant quick checks; master code changes run the full
  reusable CI. High-risk changes may request full CI before merge. Nightly and tag
  validation remain full. PR green alone does not prove full integration passed.
- **Reuse:** keep one target per worktree, stable build settings and passing evidence.
  Warm `target/debug` only if local compilation is useful. Similar items stay in one
  branch; do not create a worktree or build cache per line.
- **Storage:** before new build caches, a large batch or disk pressure, use
  `mise run cache` and `mise exec -- df -h .`. Other worktrees/global stores are
  separate; CoW totals are not exclusive usage. Check active consumers and rebuilding
  cost before targeted cleanup. Setup/tests do not implicitly purge useful stores.
  See [storage decisions](docs/development-workflow.md#storage-decisions).

A worker reports the changed scope, what it inspected, actual commands/results and
unrun checks or remaining risks. Reading code, review and `cargo check` must not be
reported as passing runtime tests. General skill checklists do not override this budget.

## Parallel Agents

- Isolate parallel writers in separate checkouts/branches using the selected
  tool or normal Git workflow. Read-only helpers may share a stable version.
  Never `git stash`: all worktrees share it.
- Delegated workers commit locally; the coordinator owns integration and any
  authorized push/PR. Keep both sides of append-only conflicts (catalog entries,
  labels, rule lists), check wiring, and regenerate rule artifacts once when integrating rule changes.
  Generated files are regenerated, never merged by hand.
- Keep concurrent agents' files disjoint where possible. Two agents adding the same
  display id or rule produce a semantic conflict that goes back to the later one.
- An agent finishes with what it reviewed/ran and what it left for integration.

## Coding Style & Naming Conventions

Use Rust edition 2024 and rustfmt's four-space indentation. Follow existing
two-space TypeScript indentation; use `snake_case` for Rust functions/modules
and `PascalCase` for types and React components. Keep code/comments in English.
Calculations use stable IDs; translate at display boundaries. Preserve raw
imported item text and JSON compatibility.

## Testing Guidelines

Use Rust tests, Vitest `*.test.ts` files and Playwright `*.spec.ts` files. Changed
behavior needs regression assertions; workers may hand off written tests as unrun.
Choose local checks by risk and benefit, and specify `--test` or `--lib` when running
Rust tests. Workflow changes use `mise run check:workflows` without compiling Rust.

PRs select quick checks: Rust formatting/checking, relevant Web checks and automation
contracts. Shared/unknown inputs select every quick area. Master code changes,
manual dispatch, nightly and release tags run full integration; high-risk changes
can request `mise run ci <pushed-ref>` before merge. `PR gate` rejects failed,
cancelled, missing or unexpectedly skipped selected jobs. E2E remains opt-in.
Reuse evidence for unchanged inputs and report its scope accurately. See
[workflow commands](docs/development-workflow.md). Documentation-only changes need
diff/link review, not builds.

## Accuracy Workflow

PoBR primarily simulates a static build under selected conditions, not a full
gameplay timeline. Before implementing or delegating any modifier feature,
evaluate whether it meets the simulator's needs: which target output it affects
under the selected actors, skills, conditions and simulation assumptions; what
concrete error or user need it addresses; and whether that benefit justifies
the added complexity. If there is no clear need, do not implement it merely
because it is unsupported. Judge effects by this reasoning, not a fixed list
of modifier names or trigger types. Do not invent event histories, rates or
uptime to justify an otherwise unnecessary feature.

The gap queue is diagnostic inventory, not an implementation backlog. Frequency
and missing-reader counts alone do not justify a model, config control, display
field or abstraction. Preserve inactive conditions and honest diagnostics for
deferred mechanics; do not mark them globally effectless just because the
current simulation omits them. Prefer the smallest existing-path change;
avoid speculative simulation, abstractions and performance work.

Improve calculation accuracy in batches, not one build at a time:

1. Find the problems of every build at once with `mise run triage`. It runs the
   benchmark and tuned sets against their build codes' references and the
   pinned PoB2, and files each differing row as `engine` or `both` (the
   engine's to fix), `reference` (the reference comes from another exporter
   version; nothing to fix) or `known` (explained in
   `devs/scripts/known-differences.json`). The holdout set is only measured
   with `mise run accuracy`; never triage or diagnose it.
2. Fix the causes the report shares across the most builds, several in one
   batch: its shared leads, the unparsed-line templates, and what each build's
   ledger of lines (`calculate-build --usage`) says the calculation drops
   whether or not a row differs: lines a rule recognises and nothing models,
   modifier names no build reads, conditions nothing sets, rejected gem
   stats. Use `mise run gaps [--kind unique|passive|mods|...]` to rank game-text
   gaps by worked builds and sources, then `mise run gap "<template or text>"`
   for stat IDs, full source text, ledger fates and nearby engine rules.
   Implement from game data and item text; PoB2 is a cross-check.
   Parallel agents write independent fixes and report their review/check evidence;
   the coordinator integrates and chooses the batch validation.
3. Validate once per batch, preferably with remote full CI; use `mise run gate`
   when an explicit local accuracy/parity batch is useful. Then rerun
   `mise run triage` for an accuracy investigation (PoB2's side is cached). Raise improved baselines and
   record the cause and evidence in the commits or PR; keep generated reports local.
4. Open one build (`mise run diagnose`, `mise run compare <build> [Stat ...]`,
   `calculate-build --breakdown <stats> --usage`) only for a row the triage
   does not explain. When a kind of
   row keeps needing that, extend the triage or diagnosis output so the next
   batch shows it; slow per-build investigation means the tooling is missing
   something.
5. When the game's text or data shows a reference wrong, document the
   deliberate difference in [docs/pob2-differences.md](docs/pob2-differences.md)
   and list its rows in `devs/scripts/known-differences.json`. A rule PoB2 does
   not implement only needs `exporter: "absent"`, not a known-difference entry.

## Commit & Pull Request Guidelines

Follow existing subjects such as `fix: preserve skill selection` and
`chore: scope agent validation`. Keep commits focused. PRs should explain the
problem, resulting behavior, validation, and remaining limits; link relevant
issues and include screenshots for visible UI changes. Pushes, PR updates,
and releases follow the user's authorized scope. Use independent review for
substantial or risky changes; trivial edits do not need a new reviewer or audit
form. Review evidence should identify the checked head/base and real limits.
`mise run pr-ready <N>` is an optional read-only helper before an authorized merge;
use the confirmed head with `--match-head-commit`. Never invent passing checks.
Contributor requirements are in
[CONTRIBUTING.md](CONTRIBUTING.md); maintainers handling fork PRs follow
[the external PR steps](docs/development-workflow.md#处理外部-pr).

## Documentation

- [docs/README.md](docs/README.md) organizes current guides by audience and task.
  Web feature contracts are in [docs/web-contracts.md](docs/web-contracts.md).
- A new mechanic convention goes into its section of [docs/mechanics/](docs/mechanics/README.md),
  an output field into [docs/calc-outputs.md](docs/calc-outputs.md), a PoB2
  difference into [docs/pob2-differences.md](docs/pob2-differences.md). Keep the
  maintainer guide to cross-cutting rules: every session loads it in full.
- When code replaces a described behaviour, rewrite or delete the paragraph;
  do not keep superseded history in current documents.
- Put generated reports and personal research in `.cache/` or `study/`.
  Keep reproducible examples, source metadata, active baselines and current
  explanations in version control. Preserve source citation paths when useful.

`agent-docs/` preserves earlier mechanics research cited by source and tests;
check it against current game data and [mechanics](docs/mechanics/README.md).
