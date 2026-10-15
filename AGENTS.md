# Repository Guidelines

Read the [maintainer guide](docs/maintainer-guide.md) for detailed architecture, validation scope, and
calculation conventions. Use current code and regression tests to resolve
disagreements with historical design notes.

## Project Structure & Module Organization

- `crates/` contains seven libraries: data definitions, game-data loading,
  localization, modifier calculation, passive trees, item editing, and build
  orchestration. `crates/pobr-core/src/` separates `model`, `parse`, `rules`, `ingest`,
  `aggregate`, `calc`, and `attribute`.
- `apps/` contains CLI, WASM/JSON bindings, and a desktop placeholder.
  `web/` is the React/TypeScript application; its backend consumes the
  `apps/pobr-wasm/src/build_api/` JSON contract.
- `tools/` and `pipeline/` generate and verify versioned `data/` snapshots.
  `web/public/` holds static assets and the Pages Worker.
- Rust integration tests live in each crate's `tests/`; frontend unit tests
  accompany source files, with browser tests in `web/e2e/`.

## Build, Test, and Development Commands

Run from the repository root, using the configured toolchains:

```bash
cargo build -p pobr-cli
mise run verify build skills support_gating::
mise run targets
mise run cache
pnpm --dir web dev
pnpm --dir web test src/lib/skillSelection.test.ts
```

These build the CLI, run selected support tests with formatting and Clippy,
list test suites, report cache usage, start Vite, and run a selected Vitest file. See [Web setup](web/README.md)
for initial WASM/data preparation and [development workflow](docs/development-workflow.md)
for cache policy. Reuse one target per worktree and run Cargo commands sequentially.

## Coding Style & Naming Conventions

Use Rust edition 2024 and rustfmt's four-space indentation. Follow existing
two-space TypeScript indentation; use `snake_case` for Rust functions/modules
and `PascalCase` for types and React components. Keep code/comments in English.
Calculations use stable IDs; translate at display boundaries. Preserve raw
imported item text and JSON compatibility.

## Testing Guidelines

Use Rust tests, Vitest `*.test.ts` files, and Playwright `*.spec.ts` files.
Add regression assertions for changed behavior; calculation changes also need
relevant parity coverage. Specify `--test` or `--lib` to limit compilation.
Local commits require relevant checks. Pull requests run lightweight checks
automatically (`.github/workflows/pr.yml`); full merge/release gates run in CI on
tags, manual dispatch, release PRs (a changed workspace version) and nightly
on master, so do not
require a duplicate local full run. Use `mise run ci <pushed-ref>` for manual
cloud validation and `mise run full` for explicit local Rust validation. Reuse
passing checks for unchanged inputs. See [workflow commands](docs/development-workflow.md).
Documentation-only changes need diff/link review, not builds.

## Accuracy Workflow

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
   stats. Implement from game data and item text; PoB2 is a cross-check.
   Parallel agents may write independent fixes without compiling; the main
   session merges and tests.
3. Validate once per batch: rerun `mise run triage` (PoB2's side is cached),
   `mise run accuracy`, the affected suites, the parity `reference_report`
   gate and lint. Raise improved baselines, add a round to
   `examples/reference-builds/README.md`, and commit each cause separately.
4. Open one build (`mise run diagnose`, `mise run compare <build> [Stat ...]`,
   `calculate-build --breakdown <stats> --usage`) only for a row the triage
   does not explain. When a kind of
   row keeps needing that, extend the triage or diagnosis output so the next
   batch shows it; slow per-build investigation means the tooling is missing
   something.
5. When the game's text or data shows a reference wrong, document the
   deliberate difference in the maintainer guide and list its rows in
   `devs/scripts/known-differences.json`.

## Commit & Pull Request Guidelines

Follow existing subjects such as `fix: preserve skill selection` and
`chore: scope agent validation`. Keep commits focused. PRs should explain the
problem, resulting behavior, validation, and remaining limits; link relevant
issues and include screenshots for visible UI changes. Pushes, PR updates,
and releases follow the user's authorized scope. Contributor requirements are in
[CONTRIBUTING.md](CONTRIBUTING.md); maintainers handling fork PRs follow
[the external PR steps](docs/development-workflow.md#处理外部-pr).

## Reference Material

[docs/README.md](docs/README.md) separates current guides and contracts from
dated records. Web feature contracts are in [docs/web-contracts.md](docs/web-contracts.md).

`agent-docs/` contains mechanics research, not final authority.
`devs/docs/architecture/` holds the feature contracts (documents 18–21).
