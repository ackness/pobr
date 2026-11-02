# Development tools

These are maintained programs, with their own source and tests. Everyday commands
enter through [mise tasks](../mise.toml); game-facing commands live in `apps/`.

| Directory | Responsibility |
| --- | --- |
| `pobr-data-adapter/` | Adapt declared game inputs to PoBR data contracts |
| [sync-pob-catalog/](sync-pob-catalog/README.md) | Explicit PoB2 extraction and catalog/parity checks |
| `precompile-mods/` | Generate and audit precompiled modifier rules |
| `lint-i18n/` | Check language-pack completeness |
| [pob2-oracle/](pob2-oracle/README.md) | Run the pinned Lua reference through a headless bridge |

Data acquisition and source publication belong to the data owners. PoBR import
or adaptation does not certify a new source release. See the
[maintainer guide](../docs/maintainer-guide.md#workspace-结构) for tool boundaries
and the [pipeline guide](../pipeline/README.md) for remaining compatibility inputs.

Put generated reports and temporary outputs in `.cache/`. Keep reproducible fixtures,
source declarations and regression expectations alongside the program that reads them.
