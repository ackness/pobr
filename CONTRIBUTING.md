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
mise run test build parity no_regression   # Parity regression gate (also run by the PR checks)
pnpm --dir web test <file> && pnpm --dir web typecheck
```

See [validation scope](docs/maintainer-guide.md) for the table of change types and their
minimum checks, and the [development workflow](docs/development-workflow.md)
for commands and caches.

Pull requests run lightweight checks automatically: formatting, Clippy, smoke
tests and the parity regression gate for Rust changes; typecheck and unit tests
for `web/` changes. Runs from first-time contributors wait for a maintainer's
approval. The full gate runs on release tags, on pull requests that change
the workspace version and nightly on `master`.

## Pull requests

Describe the problem, the resulting behavior, what you validated and any
remaining limits. State plainly what you did not run or verify.

Maintainers may add follow-up commits on top of yours in a separate pull
request. Your commits and authorship are kept. If that pull request is
squash-merged, yours is closed with a link to it rather than shown as merged;
the squash commit credits you as co-author.
