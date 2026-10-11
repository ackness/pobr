# Contributing to PoBR

Thanks for helping. This page covers what a pull request needs; the detailed
rules live in the documents linked below.

## Before you start

- [AGENTS.md](AGENTS.md) is the short contributor guide: layout, commands, style.
- [CLAUDE.md](CLAUDE.md) has the architecture, validation scope and calculation
  conventions.
- [docs/](docs/README.md) indexes the feature contracts and guides. To add or
  fix a modifier through data, start with
  [Contributing modifiers](docs/contributing-mods.md).

## What a good change looks like

- **Match Path of Building 2.** PoB2 compatibility is the regression baseline.
  When a change follows a PoB2 formula or data entry, cite the file and line in
  the code comment, for example `Item.lua:1232` or `act_dex.lua:11045`. The
  pinned PoB2 commit is recorded in the active data's
  `overlay/mod_parser_rules.json` under `_meta.vendor_commit`.
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
./pobr verify build skills <filter>      # Selected tests, fmt and Clippy
./pobr test build parity no_regression   # Required for calculation, modifier or parser changes
pnpm --dir web test <file> && pnpm --dir web typecheck
```

See [validation scope](CLAUDE.md) for the table of change types and their
minimum checks, and the [development workflow](docs/development-workflow.md)
for commands and caches.

Pull requests run lightweight checks automatically: formatting, Clippy, smoke
tests and the parity regression gate for Rust changes; typecheck and unit tests
for `web/` changes. Runs from first-time contributors wait for a maintainer's
approval. The full gate runs on release tags and on pull requests that change
the workspace version.

## Pull requests

Describe the problem, the resulting behavior, what you validated and any
remaining limits. State plainly what you did not run or verify.

Maintainers may add follow-up commits on top of yours in a separate pull
request. Your commits and authorship are kept. If that pull request is
squash-merged, yours is closed with a link to it rather than shown as merged;
the squash commit credits you as co-author.
