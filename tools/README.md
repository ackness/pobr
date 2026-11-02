# Development tools

These are maintained programs, with their own source and tests. Everyday commands
enter through [mise tasks](../mise.toml); game-facing commands live in `apps/`.

| Directory | Responsibility |
| --- | --- |
| [sync-pob-catalog/](sync-pob-catalog/README.md) | PoB2 display/output catalog scan and parity checks (test reference only) |
| `precompile-mods/` | Generate and audit precompiled modifier rules |
| `lint-i18n/` | Check language-pack completeness |
| [pob2-oracle/](pob2-oracle/README.md) | Run the pinned Lua reference through a headless bridge |

Data acquisition, source publication and the domain adapter that turns RePoE facts
into PoBR runtime JSON belong to the data owners (ackness/repoe
`tools/pobr-data-adapter`). PoBR import does not certify a new source release. See the
[maintainer guide](../docs/maintainer-guide.md#workspace-结构) for tool boundaries
and the [pipeline guide](../pipeline/README.md) for PoB2-derived compatibility inputs
that arrive in the owner package.

Put generated reports and temporary outputs in `.cache/`. Keep reproducible fixtures,
source declarations and regression expectations alongside the program that reads them.
