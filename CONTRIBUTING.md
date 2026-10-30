# Contributing to PoBR

Thanks for helping. This page covers what a pull request needs; the detailed
rules live in the documents linked below.

## Before you start

Use the latest stable mise for local tools.
`mise bootstrap --only packages,tools,task` prepares the common environment;
run commands with `mise run` or `mise exec --`. Optional tools are installed by
the task that needs them. CI uses current official/maintained Actions with the
same pinned versions.

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
- **Reproduce changed behavior.** Calculation/logic fixes need a focused
  regression assertion. Prose and mechanical edits need appropriate review,
  without mirrored tests. Do not lower a parity baseline to make a test pass.
- **Keep it focused.** One problem per pull request. Code and comments are in
  English.

## Checks

Run the checks that match your change; a full workspace run is not required.

```bash
mise run verify build skills <filter>      # Selected tests, fmt and Clippy
mise run regen                             # After editing a modifier rule
mise run check:workflows                   # Workflow/automation changes; no Rust build
mise run web test <file>
mise run web typecheck
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

For a substantial or risky change, use an independent reviewer and identify the
checked commit plus any limits. Small obvious fixes and documentation do not need
an extra agent or audit form. Keep corrections on the same PR and recheck the
affected evidence after updates. The template is a writing aid, not a CI format.

Maintainers can use `mise run pr-ready <N>` to inspect the live PR/checks and get
the exact head for `--match-head-commit`; it never merges. Equivalent direct review
is fine. Product decisions and non-CI evidence still need judgment.

Normally maintainers request corrections on your PR. If a takeover is needed,
follow-up commits preserve your work and credit; a replacement PR links to yours
and explains its eventual closure. See the [external PR procedure](docs/development-workflow.md#处理外部-pr).
