# Contributing to PoBR

Thanks for helping. This page covers what a pull request needs; the detailed
rules live in the documents linked below.

## Before you start

Use the latest stable mise for local tools.
`mise bootstrap --only packages,tools,task` prepares the common environment;
run commands with `mise run` or `mise exec --`. Optional tools are installed by
the task that needs them. CI uses current official/maintained Actions with the
same pinned versions.

- [Maintainer guide](docs/maintainer-guide.md) (Chinese) has the architecture, validation
  scope and cross-cutting conventions; [AGENTS.md](AGENTS.md) is its short English digest.
  [Mechanics notes](docs/mechanics/README.md) record the rules and assumptions of
  individual game mechanics.
- [docs/](docs/README.md) indexes the feature contracts and guides. To add or
  fix a modifier through data, start with
  [Contributing modifiers](docs/contributing-mods.md).

## Which repository owns the fix

PoBR calculates with data that two other repositories publish. Open the issue or
pull request where the fix belongs:

| Problem | Repository |
| --- | --- |
| Wrong or missing game facts (bases, affixes, skills, passive tree, stat values), client decoding, or how facts are projected into PoBR's runtime JSON (`pobr-data-adapter`) | [ackness/repoe](https://github.com/ackness/repoe) |
| Wrong or missing Chinese game text: affix and stat wording, item and skill names, support-gem descriptions | [ackness/poe2-zh-cn-stats](https://github.com/ackness/poe2-zh-cn-stats) |
| Modifier rules, stat meaning, calculations, build import/export, CLI/WASM, the web app and its interface text | ackness/pobr (here) |

A data fix reaches PoBR when its owner publishes a new Release and a maintainer
moves the data lock to it ([RELEASING.md](RELEASING.md)). Do not add game or
translation data to PoBR itself. If you are unsure, open the issue here and say
so; a maintainer will route it.

## What a good change looks like

- **Follow the game's own data.** Implement a mechanic from the game: stat ids
  and constants in `data/<version>/base/`, the modifier text, official patch
  notes. Cite those in code and tests, for example
  `attack_damage_final_permyriad_per_rage = 100`. Path of Building 2 is the
  regression baseline and a cross-check, not the authority: do not cite its Lua
  line numbers. Where PoBR deliberately differs from PoB2, say why in
  [PoB2 differences](docs/pob2-differences.md). The PoB2 checkout used by
  tests and comparisons is pinned in `devs/reference-sources.json`
  (`mise run vendor` clones it).
- **Prefer a general rule to a per-skill entry.** If the same mechanism affects
  several skills or modifiers, fix the mechanism once instead of adding one
  allow-list line per name, and check which other entries share the bug.
- **Reproduce changed behavior.** Calculation/logic fixes need a focused
  regression assertion. Prose and mechanical edits need appropriate review,
  without mirrored tests. Do not lower a parity baseline to make a test pass.
- **Keep it focused.** One problem per pull request. Code and comments are in
  English.

## Checks

Choose checks by the behavior at risk and their cost
([validation tiers](docs/maintainer-guide.md#验证分层)). Documentation needs diff/link
review. Large changes and algorithms often benefit from focused tests or compilation
before handoff; ordinary commits do not require a local product build. Tests that read
real game data need `mise run data:prepare` first.

```bash
mise exec -- cargo check --locked -p pobr-core --lib  # Optional targeted compiler check
mise run verify build skills <filter>                # Optional tests, fmt and Clippy
mise run regen                                       # Once when integrating a rule batch
mise run check:workflows                             # Automation changes; no Rust build
mise run web test <file>
```

Add regression cases for changed behavior even when execution is deferred. Report
what you inspected, actual commands/results and anything left unrun. A review or
compiler check does not prove runtime assertions passed.

Your PR runs the relevant quick checks automatically (`PR gate`); a maintainer may
request full CI before merging a risky change, and master runs full CI after merge.
There is no need to repeat a full run locally. CI details are in the
[development workflow](docs/development-workflow.md#ci-的责任与触发).

## Pull requests and review

Use one PR for one problem. Describe the problem, resulting behavior, actual
validation and remaining limits using the PR template. Visible UI changes should
include useful before/after captures; documentation needs diff/link review.
A draft may be opened before the review is complete.

For a substantial or risky change, use an independent reviewer and identify the
checked commit plus any limits. Small obvious fixes and documentation do not need
an extra agent or audit form. Keep corrections on the same PR and recheck the
affected evidence after updates. The template is a writing aid, not a CI format.

Normally maintainers request corrections on your PR. If a takeover is needed,
follow-up commits preserve your work and credit; a replacement PR links to yours
and explains its eventual closure. See the [external PR procedure](docs/development-workflow.md#处理外部-pr).
