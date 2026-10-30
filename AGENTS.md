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

The project-specific workflow adapter is [.claude/skills/workflow/SKILL.md](.claude/skills/workflow/SKILL.md);
reusable workflow design belongs to the independently installed global `repo-workflow` skill.

## Build, Test, and Development Commands

Run from the repository root, using the configured toolchains:

```bash
mise run test core parser mod_parser::   # one suite, one filter
mise run verify build skills support_gating::
mise run regen                           # after any special_mods / parser rule edit
mise run targets
pnpm --dir web test src/lib/skillSelection.test.ts
```

For the common small changes (one game line, one Calcs field, one mechanic),
follow [the change recipes](docs/recipes.md): they name the files to edit and
the single check to run, so no codebase exploration is needed. See
[Web setup](web/README.md) for WASM/data preparation and
[development workflow](docs/development-workflow.md) for cache policy. Reuse one
target per worktree and run Cargo commands sequentially.

## Validation Budget

Compilation dominates the cost of a small change. Each test target, the CLI and
the parity binary are separate builds, and a new worktree starts with no build
cache at all.

- **Per change: one focused check.** `mise run verify <crate> <suite> [filter]`
  runs the test of what you touched, then fmt and Clippy for that target
  (`cargo test -q -p <crate> --test <suite> <filter>` works too); add
  `mise run regen` after a rule edit. That is all a commit needs.
- **Do not run per change:** the whole `cargo test -p pobr-core`, `--test parity`,
  the WASM/CLI suites, `mise run accuracy`, `triage`, `diagnose`, `compare`,
  `skills:sweep` or the coverage scripts. They compile the parity binary or the
  CLI and recompute every build.
- **Per batch: one gate.** Whoever merges several changes runs `mise run gate`
  once on a warm checkout. It runs every step and lists the failures;
  `mise run gate --only <steps>` reruns just those after the fix.
- **New worktree:** `mise run warm <warm checkout>` clones its build cache
  copy-on-write. `POBR_BIN=<warm checkout>/target/debug/pobr` and
  `POBR_CACHE_DIR=<warm checkout>/.cache` make the analysis tools reuse its CLI,
  ledgers and PoB2 results instead of building and recomputing them.
- **Many similar items go to one branch,** done one after another with only each
  item's check, then one regen and one commit series. Do not open a worktree,
  agent or full validation per line.

## Parallel Agents

- One worktree and one branch per agent. Never `git stash`: all worktrees share it.
- Agents commit locally and never push. Before the last commit, merge the working
  branch, keep both sides of append-only conflicts (catalog entries, labels,
  rule lists), check catalog wiring and labels, and rerun `mise run regen` when
  rules changed. Generated files are regenerated, never merged by hand.
- Keep concurrent agents' files disjoint where possible. Two agents adding the same
  display id or rule produce a semantic conflict that goes back to the later one.
- An agent finishes with what it ran and what it left for the gate.

## Coding Style & Naming Conventions

Use Rust edition 2024 and rustfmt's four-space indentation. Follow existing
two-space TypeScript indentation; use `snake_case` for Rust functions/modules
and `PascalCase` for types and React components. Keep code/comments in English.
Calculations use stable IDs; translate at display boundaries. Preserve raw
imported item text and JSON compatibility.

## Testing Guidelines

Use Rust tests, Vitest `*.test.ts` files, and Playwright `*.spec.ts` files.
Add regression assertions for changed behavior; calculation changes are measured
against parity by the batch gate. Specify `--test` or `--lib` to limit
compilation. Local commits require the focused check above; workflow changes use
`mise run check:workflows` without compiling Rust. Pull requests select relevant
checks automatically; version, dependency, toolchain,
workspace manifest and workflow-policy changes select the reusable full gate.
`PR gate` rejects failed, cancelled or unexpectedly skipped selected jobs. Full
validation also runs nightly, manually and before tag publication. E2E stays explicit
opt-in. Use `mise run ci <pushed-ref>` when review identifies broader integration
risk, or `mise run full` for explicit local Rust validation. Reuse
passing checks for unchanged inputs. See [workflow commands](docs/development-workflow.md).
Documentation-only changes need diff/link review, not builds.

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
   Parallel agents write independent fixes with only focused checks; the main
   session merges and runs the gate.
3. Validate once per batch: `mise run gate` (includes accuracy and parity), then
   rerun `mise run triage` (PoB2's side is cached). Raise improved baselines, add a round to
   `examples/reference-builds/README.md`, and commit each cause separately.
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
and releases follow the user's authorized scope. Before marking a PR ready, an
independent reviewer audits the current head/base;
record the real result in the template's `pobr-audit` JSON. `PR handoff` enforces
content/freshness and resolved blockers, not independent reasoning. Before an
authorized merge, run `mise run pr-ready <N>` and use the reported head with
`--match-head-commit`. Never invent an audit, reviewer identity or passing checks.
Contributor requirements are in
[CONTRIBUTING.md](CONTRIBUTING.md); maintainers handling fork PRs follow
[the external PR steps](docs/development-workflow.md#处理外部-pr).

## Documentation

- [docs/README.md](docs/README.md) separates current guides and contracts from
  dated records. Web feature contracts are in [docs/web-contracts.md](docs/web-contracts.md).
- A new mechanic convention goes into its section of [docs/mechanics/](docs/mechanics/README.md),
  an output field into [docs/calc-outputs.md](docs/calc-outputs.md), a PoB2
  difference into [docs/pob2-differences.md](docs/pob2-differences.md). Keep the
  maintainer guide to cross-cutting rules: every session loads it in full.
- When code replaces a described behaviour, rewrite or delete the paragraph;
  do not keep superseded history in current documents.

`agent-docs/` contains mechanics research, not final authority.
`devs/docs/architecture/` holds the feature contracts (documents 18–21).
