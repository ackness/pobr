# Repository Guidelines

Read [CLAUDE.md](CLAUDE.md) for detailed architecture, validation scope, and
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
./pobr verify build skills support_gating::
./pobr targets
pnpm --dir web dev
pnpm --dir web test src/lib/mainSkill.test.ts
```

These build the CLI, test support behavior, check formatting/Clippy, start Vite,
and run a selected Vitest file. See [Web setup](web/README.md) for initial
WASM/data preparation. Run one Cargo command at a time per worktree target.

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
Local commits require relevant checks. Full merge/release gates run in CI; do
not require a duplicate local full run. Use `./pobr ci <pushed-ref>` for manual
cloud validation and `./pobr full` for explicit local Rust validation. Reuse
passing checks for unchanged inputs. See [workflow commands](docs/development-workflow.md).
Documentation-only changes need diff/link review, not builds.

## Commit & Pull Request Guidelines

Follow existing subjects such as `fix: preserve skill selection` and
`chore: scope agent validation`. Keep commits focused. PRs should explain the
problem, resulting behavior, validation, and remaining limits; link relevant
issues and include screenshots for visible UI changes. Pushes, PR updates,
and releases follow the user's authorized scope.

## Reference Material

`agent-docs/` contains mechanics research, not final authority. Older
`devs/docs/architecture/` notes may exist only locally; they are optional
historical references, not fresh-checkout prerequisites or current task lists.
