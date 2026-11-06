# Repository Guidelines

This is a short English digest for agent tools that load `AGENTS.md`. The rules
themselves live in the [maintainer guide](docs/maintainer-guide.md) (Chinese;
read it in full before changing code) and the documents it links. Where this
digest and the guide disagree, the guide wins; current code and regression
tests win over both.

## Where a change belongs

PoBR is the consumer repository of a three-repository split. Put each fix in
the repository that owns it ([routing](.agents/skills/workflow/SKILL.md),
[contributor table](CONTRIBUTING.md#which-repository-owns-the-fix)):

- `ackness/repoe`: client decoding, game facts and the `tools/pobr-data-adapter`
  policies that project them into runtime JSON.
- `ackness/poe2-zh-cn-stats`: term templates, translations and language packages.
- `ackness/pobr` (here): data contracts, stat meaning, modifier rules, the
  engine, CLI/WASM bindings and the Web app.

PoBR does not produce data. `pipeline/data.lock.json` pins the owners' GitHub
Release assets; `mise run data:prepare` imports them for real-data tests and
builds; `mise run data:bump` moves the lock in a reviewed PR. Never commit game
or language data, and never derive it from PoB2 vendor Lua: PoB2 is a test
reference pinned in `devs/reference-sources.json`. Data design:
[data architecture](docs/data-architecture.md); data bumps and releases:
[RELEASING.md](RELEASING.md).

Layout: `crates/` (seven libraries), `apps/` (CLI, WASM/JSON API), `web/`
(React/TypeScript app and Pages Worker), `pipeline/` (data lock and import),
`devs/` (development scripts, CI baselines), `examples/` (regression corpora).
See [workspace structure](docs/maintainer-guide.md#workspace-结构).

## Commands

Run everything through mise from the repository root: `mise run <task>` or
`mise exec -- <command>`; `mise tasks` lists the tasks. First setup:
`mise bootstrap --only packages,tools,task`.

```bash
mise run check:pr                        # before pushing; optional pre-push hook
mise run data:prepare                    # pinned owner data for real-data tests
mise run test core parser mod_parser::   # one suite, one filter
mise run verify build skills support_gating::
mise run regen                           # coordinator: once after a rule batch
mise run targets
mise run web test src/lib/skillSelection.test.ts
```

Common small changes: [change recipes](docs/recipes.md). Commands, caches, CI,
merge and release mechanics: [development workflow](docs/development-workflow.md).

## Hard rules

- **Validate by risk.** Choose evidence by risk and cost
  ([validation tiers](docs/maintainer-guide.md#验证分层)). Documentation needs
  diff/link review only. Changed behaviour needs regression assertions, which
  may be handed off unrun. Report what you inspected and actually ran; never
  present review or `cargo check` as passing tests, and never lower a parity
  baseline to make a test pass.
- **Before merging.** Rust, data or pipeline changes need a green
  `mise run ci <branch>`; PR gate omits Clippy and Rust tests. Run
  `mise run check:pr` before pushing ([local checks and hook](docs/development-workflow.md#日常命令)).
- **Builds.** One `./target` per worktree and one Cargo command at a time; never
  share a writable `CARGO_TARGET_DIR` across worktrees. Do not change the
  dev/test profile, run `cargo clean` automatically, or chain
  check/build/Clippy/test by default.
- **Parallel agents.** One worktree/branch per writer. Never `git stash`: all
  worktrees share it. Workers commit locally; the coordinator integrates,
  regenerates generated files instead of merging them, and owns any authorized
  push or PR ([details](docs/maintainer-guide.md#并行-agent提交与-pr)).
- **Calculation.** Stable IDs inside the engine, translation at display
  boundaries; deterministic output; preserve raw imported text and JSON
  compatibility. Implement mechanics from game data; PoB2 is a cross-check.
  Before implementing a modifier feature, confirm it changes a supported output
  of the static simulation. Never let an input disappear without a diagnostic.
- **Accuracy work is batched.** Start with `mise run triage`; never triage or
  diagnose the holdout set ([diagnosis loop](docs/maintainer-guide.md#排查数值差异诊断闭环)).
- **Style.** Rust edition 2024 with rustfmt; two-space TypeScript; `snake_case`
  Rust functions and modules, `PascalCase` types and React components; code and
  comments in English.
- **Commits and PRs.** Subjects like `fix: preserve skill selection` or
  `docs(pipeline): …`, one focused change per commit. PRs state the problem,
  resulting behaviour, validation and remaining limits. Push, merge and release
  only within the authorized scope.
- **Documentation.** Rewrite or delete superseded paragraphs; keep no history in
  current documents. Generated reports and personal research go in `.cache/` or
  `study/`. Each rule has one home ([documentation upkeep](docs/maintainer-guide.md#文档维护)).
