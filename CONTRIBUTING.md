# Contributing to PoBR

Thanks for helping. This page covers what a pull request needs; the detailed
rules live in the documents linked below.

## Before you start

- [AGENTS.md](AGENTS.md) is the short contributor guide: layout, commands, style.
- [Maintainer guide](docs/maintainer-guide.md) has the architecture, validation scope and
  cross-cutting conventions; [mechanics notes](docs/mechanics/README.md) record the rules
  and assumptions of individual game mechanics.
- [docs/](docs/README.md) indexes the feature contracts and guides. To add or
  fix a modifier through data, start with
  [Contributing modifiers](docs/contributing-mods.md).

## What a good change looks like

- **Follow the game's own data.** Implement a mechanic from the game: stat ids
  and constants in `data/<version>/base/`, the modifier text, official patch
  notes. Cite those in code and tests, for example
  `attack_damage_final_permyriad_per_rage = 100`. Path of Building 2 is the
  regression baseline and a cross-check, not the authority: do not cite its Lua
  line numbers. Where PoBR deliberately differs from PoB2, say why in
  [PoB2 differences](docs/pob2-differences.md). The pinned PoB2 commit is
  recorded in the active data's `overlay/mod_parser_rules.json` under
  `_meta.vendor_commit`.
- **Prefer a general rule to a per-skill entry.** If the same mechanism affects
  several skills or modifiers, fix the mechanism once instead of adding one
  allow-list line per name, and check which other entries share the bug.
- **Add an assertion that reproduces the problem.** A fix needs a test that
  fails before it and passes after it. Do not lower a parity baseline to make a
  test pass.
- **Keep it focused.** One problem per pull request. Code and comments are in
  English.

## Checks

Run the checks that match your change; a full workspace run is not required.

```bash
mise run verify build skills <filter>      # Selected tests, fmt and Clippy
mise run regen                             # After editing a modifier rule
mise run check:workflows                   # Workflow/automation changes; no Rust build
pnpm --dir web test <file> && pnpm --dir web typecheck
```

See [validation scope](docs/maintainer-guide.md) for the table of change types and their
minimum checks, and the [development workflow](docs/development-workflow.md)
for commands and caches.

Pull requests select checks by changed area. Dependency, toolchain, workspace
manifest and workflow-policy changes run full validation automatically. Ordinary
Rust/Web changes get the relevant lighter checks; local focused evidence remains
your responsibility. `PR gate` rejects missing or unexpectedly skipped checks.
Parity/accuracy measurement belongs to the batch gate, not every local change.
See the [workflow guide](docs/development-workflow.md) for exact coverage.

## Pull requests and review

Use one PR for one problem. Describe the problem, resulting behavior, actual
validation and remaining limits using the PR template. Visible UI changes should
include useful before/after captures; documentation needs diff/link review.
A draft may be opened before the review is complete.

Before marking ready, an independent agent/reviewer audits the current head and
base. Put the actual result in the template's `pobr-audit` JSON record, including
full SHAs, reviewer model/session or review link, verdict and findings. Resolve
blocking findings on the same PR. `PR handoff` checks required content, freshness
and unresolved blockers. A new head/base requires renewed review; CI cannot prove
independent reasoning or that a free-text command was really executed.

Maintainers run `mise run pr-ready <N>` before an authorized merge. It verifies the
live handoff/checks and prints the exact head for `--match-head-commit`; it never
merges. Product decisions and non-CI evidence still need review. Bot feedback and
execution approval are separate from merge authorization.

Normally maintainers request corrections on your PR. If a takeover is needed,
follow-up commits preserve your work and credit; a replacement PR links to yours
and explains its eventual closure. See the [external PR procedure](docs/development-workflow.md#处理外部-pr).
