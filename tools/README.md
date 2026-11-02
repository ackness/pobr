# Development tools

These are maintained programs, with their own source and tests. Everyday commands
enter through [mise tasks](../mise.toml); game-facing commands live in `apps/`.

| Directory | Responsibility |
| --- | --- |
| `pobr-data-adapter/` | Adapt pinned RePoE owner facts and the historical tree package to PoBR data contracts; reads no vendor Lua or local table export |
| [sync-pob-catalog/](sync-pob-catalog/README.md) | PoB2 display/output catalog scan and parity checks (test reference only) |
| `precompile-mods/` | Generate and audit precompiled modifier rules |
| `lint-i18n/` | Check language-pack completeness |
| [pob2-oracle/](pob2-oracle/README.md) | Run the pinned Lua reference through a headless bridge |

Data acquisition and source publication belong to the data owners. PoBR import
or adaptation does not certify a new source release. See the
[maintainer guide](../docs/maintainer-guide.md#workspace-结构) for tool boundaries
and the [pipeline guide](../pipeline/README.md) for PoB2-derived compatibility inputs
that arrive in the owner package.

Put generated reports and temporary outputs in `.cache/`. Keep reproducible fixtures,
source declarations and regression expectations alongside the program that reads them.
