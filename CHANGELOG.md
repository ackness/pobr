# Changelog

Notable changes to PoBR, the browser build planner at [pobr.win](https://pobr.win), its standalone WASM package and the `pobr` CLI. Each release lists its pull requests. The same text is the release's notes on [GitHub Releases](https://github.com/ackness/pobr/releases), followed by the WASM size report.

Versions follow [SemVer](https://semver.org/). While the version is 0.0.x, any release may change behaviour; changes that need action from WASM, CLI or data consumers are listed under **Breaking changes**. How a release section is written: [RELEASING.md](RELEASING.md#4-发版上线).

## [0.0.48] - 2026-11-07

The passive tree's side column now resizes by dragging its edge and remembers its width. Browser tests moved out of cloud CI into a local, tiered runner, and every release now runs the full browser suite before it is tagged. Builds saved in v0.0.47 open unchanged, and the WASM JSON schema version stays at 19.

### Breaking changes

- None.

### Added

- Passive tree: drag the side column's left edge to resize it, between 22 and 44rem and at most 48% of the page. The edge also works from the keyboard (arrow keys, Shift for bigger steps, Home/End), double-click resets it, and each browser remembers the width. This replaces the width slider, which also forgot its value between visits. (#337)

### For developers

- `mise run e2e [smoke|affected|default|release]` runs the Playwright suite locally: a 20-test `@smoke` tier, the specs affected by your changes (the default), today's default run, or the full matrix plus the isolated configs. It rebuilds only stale WASM, data and dist, and gives each worktree its own port on 127.0.0.1. (#338)
- Cloud CI no longer runs the browser suite: `mise run ci` drops `--e2e`, and only the data-bump smoke spec still runs in the cloud when the data lock changes. Releases run `mise run e2e release` locally, and `mise run release:e2e-check` must pass on the merge commit before tagging. (#338)
- `mise run check:pr --quick` runs fmt, `cargo check` of the affected packages, i18n, typecheck and the Vitest tests related to your changes, in seconds. `check:pr` now really passes `--maxWorkers=4` to Vitest. (#338)

## [0.0.47] - 2026-11-06

A maintenance release: the passive tree and the Skills page do less work while you use them, and release notes now come from this changelog. Builds saved in v0.0.46 open unchanged, and the WASM JSON schema version stays at 19.

### Breaking changes

- None.

### Changed

- Passive tree: moving the pointer over nodes no longer recomputes socket labels, point totals and edit previews; a selected node's change totals are calculated when you open them; Simplified and Traditional Chinese stat lines are translated only once you type a search (on the tree or in the node table) and are kept while you keep editing, so Refund and other previews respond sooner. (#335)
- Skills page: the results rail, Overview and Skills share one skill-damage calculation per change instead of running it once each; gem descriptions load when a gem picker or optimizer opens; the gem list sorts faster. (#335)

### Fixed

- The support gem picker's keyboard highlight no longer jumps back to the first entry while the page updates. (#335)

### For developers

- `GemCatalogEntry` has a new `is_unique_only` boolean for skills granted only by unique items (the picker's old `Unique` id-prefix rule, now in the catalog). The field is additive; the schema version is unchanged. (#335)
- Release notes and release PR bodies come from the `CHANGELOG.md` section; `release-policy.py` requires that section on a version bump and gains a `notes` command. (#334)
- `devs/scripts/ci-plan.py` drops the unused `--local` flag. (#335)

### Internal

- Cleanup of code merged since v0.0.45: duplicated rules and helpers merged (`colon`, `formatStat`, `cleanLineText`, one `MainMinionDPS` producer), dead CSS and exports removed, inline disclosure summaries no longer need `data-disclosure-level`, and web unit tests share one hook harness. (#335)

## [0.0.46] - 2026-11-06

Picking a gem now shows what it does, in English, Simplified Chinese and Traditional Chinese. Simplified Chinese stat lines, gem descriptions and minion names now use the China client's official text. Summoner builds lead with the main minion's damage instead of 0, most pages had a round of wording, layout and number-format fixes, and PoBR has its own icon. Builds saved in v0.0.45 open unchanged, and the WASM JSON schema version stays at 19.

### Breaking changes

- None.

### Added

- Gem descriptions: the Add skill group and Add support pickers show the highlighted gem's in-game description, tags and attribute requirements beside the list (below it on phones), with an Add button. Support recommendations use the same panel. A language without the text shows the English text, labelled as the original. (#322)
- A copy button next to every gem name on the Skills page, for pasting into trade or chat. (#295)
- Build baselines: "Save as baseline" keeps one snapshot per build without adding a stage, and Compare opens with the baseline as A and the current build as B. (#317)
- Build library: rename, duplicate and delete on each row; duplicating copies every stage. After an import, a dismissible summary shows the character, calculated skill, headline damage and the number of unsupported modifiers. Changing class lists what the active stage will lose first. (#312)
- Upgrades: four steps (goal, method, equipment analysis, trade check). Item details have "Find a better one on the trade site", which opens the trade step with that slot, goal and equipped item selected. (#314)
- The PoBR mark in the top bar and on the start and error screens, and browser icons (`favicon.svg`, `favicon.ico`, `apple-touch-icon.png`). (#333)
- Passive tree: jewel sockets have readable names and a "locate on tree" action, refunding a node that would drop a branch warns first, and Chinese searches find the same nodes as English ones. (#300, #307)
- Items: library, crafted, unique and pasted-item comparisons share one card with combined DPS, effective HP and life before and after the swap. The unique list filters by slot and shows actual modifier values. (#308)
- Calculation explanations: one three-language catalogue says what each headline number means, formulas lead into folded steps, and source groups fold with counts and subtotals. (#310)
- Config: choosing an enemy tier shows its resistances, level limits and armour and evasion multipliers, and tier names are the same on every page. (#313)

### Changed

- Summoner builds lead with the main minion's hit DPS in the results panel, Overview, Skills, Calcs, Compare, the change summary and the tree heatmap; the player's own DPS is shown second when above zero. The value is one minion's hit DPS, not multiplied by the minion count. (#306)
- Final stats use one precision everywhere: the change bar, results rail, Calcs damage chain, formula steps, gem optimizer and swap cards. (#328)
- Player terms and number formats are the same on every page (for example 技能, 配置, 提升, 综合 DPS, 击中 DPS). Large negative numbers keep their thousands separators. (#311)
- Skills page: "set as calculated skill" is separate from the support optimizer, internal counters fold into the calculation details, and shared templates ask before replacing a group. (#299, #309)
- Compare: B defaults to the nearest stage of the same build instead of a blank character, percentage differences are shown in percentage points, and colours follow whether a higher value is better. The first import replaces a lone blank starter build. (#301, #317)
- Overview: an empty character leads to import or create, search ranks common values first and shows units, attention items link to the relevant modifier details, a resistance summary was added, and energy-shield builds lead with ES. (#302, #318)
- Config page: one filter with three choices (Relevant, Modified, All), and a row you just edited stays in the list. Non-damaging skills no longer show 0 DPS in the Calcs skill overview. (#304)
- Folds, segmented switches, controls, headings and borders share one style and line up across pages; Skills rows and the Config jump list no longer move under the pointer. (#295, #297, #319)
- Clearer wording and empty states on Calcs, the passive tree reports and point counts, upgrade search, items and configuration values. Minion builds with 0 player DPS point to the minion. (#298, #299, #300, #313)

### Fixed

- Chinese text: a rendered line no longer mixes Chinese and English (an untranslatable line stays whole in English), roll ranges such as `+(1-2)` render through the line's own templates, and a signed value prints one sign (`+5%`, not `++5%`). Half-English Chinese lines such as `有 4 Augment 个插槽` are reported as unsupported instead of being misread. (#292, #294, #326)
- The gem picker leaves out the 8 skills only a unique item grants, so Spark, each Herald and Withering Presence appear once. Imported builds keep these skills. (#316)
- Allocating the first node next to a jewel-granted class start (for example Split Personality) no longer stores the start node or costs an extra point, and removing the jewel removes the start. (#315)
- The passive tree no longer stalls for tens of seconds while dragging with reduced motion on; panning, allocating and refunding are faster with animations too. (#324)
- Reopening a row's menu right after cancelling its dialog no longer closes the menu. (#327)
- zh-CN no longer rewrites the official Bonded prefix 羁绊： as 纽带：. (#321)
- Overview skill damage shows "Calculating…" while it runs instead of claiming there is no damage, and Calcs no longer shows "-0" life regeneration. (#297, #319)
- The passive tree, Display settings and Config page fit a 390 px phone at 32 px text, and Chinese labels stay whole in the inspector's modifier table. (#303, #305, #316)

### Data

- Game package `game/0.5.5.5-3` (data contract 1.3) adds the optional `overlay/gem_descriptions.json` with English gem descriptions; calculations are unchanged. (#296, #330)
- Language package `locale/0.5.5.5-4`: Simplified Chinese stat lines, gem descriptions and minion names from the China client's official text, and Traditional Chinese gem descriptions from the international client. 2 of 447 skill gem names per language still have no translation. The support-display asset stays at `locale/0.5.5.5-3`. (#330, #331)
- Supplement translations carry their origin (`official`, `agent` or `human`; data contract 1.2), and official text always wins over a supplement. Minion, spectre and companion names are looked up by monster variety id. The web app does not mark supplement text yet. (#293)

### For developers

- WASM: new export `translateLinesDetailed(lang, linesJson)` returns `[{text, origin}]`, where `origin` is `"agent"`, `"human"` or `null` for official text. Minion catalog entries and calculate-build minion entries gain an optional `monster_variety_id`. While the calculated skill summons a minion, calculate-build stats include `MainMinionDPS`, and the node-power, variant-optimizer and attribution requests accept it as a target. (#293, #306)
- CLI: `--lang` text reports mark supplement lines with `†` and end with a footnote; JSON reports add an `origins` map, left out when empty. `pobr_cli::render_build_text` is exported for library users. (#293)
- The translation repository no longer publishes the support display. Until the PR that retires `support_display` merges, `data:bump` refuses a new game patch or a trade catalogue change. (#331)

### Internal

- Browser tests: the default run leaves out `@extended` cases and per-test traces, the quarantined Config tests were rewritten and `@quarantine` is now empty, and the island-reconnect test checks the intended behaviour. The full Config sweep runs in full CI. (#320, #323, #329)
- `mise run check:pr` runs the pre-push checks for the changed files, and `mise run hooks:install` enables an opt-in pre-push hook. Rust, data and pipeline PRs need a green `mise run ci <branch>` before merging. (#325, #331)
- Guides updated after `game/0.5.5.5-3` and `locale/0.5.5.5-4`. (#331)
- `docs/web-contracts.md` documents gem description lookup, the 羁绊 exception, reduced motion, final-stat precision and the dialog and menu rules. (#296, #321, #322, #324, #327, #328)

## [0.0.45] - 2026-11-05

Gems and skills that showed a raw id such as `Metadata/Items/Gems/SkillGemHeraldOfIce` now show their names. In Chinese, passive and base item names are looked up by id, and stat descriptions the game writes on several lines translate as one description. A new developer command checks what the web app shows for real builds. The WASM JSON schema version stays at 19.

### Breaking changes

- WASM: `translateLines` and `parseLocalizedLines` still return one output per input, but a line that a multi-line description folds into the line before it now comes back as `""`. Skip empty strings or read the output by position. (#291)

### Fixed

- Gems and skills no longer show their metadata id as their name. Base items were indexed by English name, so bases sharing a name (Herald of Ice, Blink, Withering Presence, Lightning Bolt, Ember Fusillade and others) overwrote each other; 20 of 1073 gem catalogue rows were affected. The fix covers the gem catalogue, config, locale tables and WeGame import. (#285)
- WeGame import resolves Herald of Ice, Ash and Thunder and Withering Presence to the ordinary gem rather than the version only a unique item grants. Decompose and weapon default attacks are reported as unknown or ambiguous instead of being picked silently. (#285)
- zh-CN and zh-TW passive and base item names are looked up by id, so a name with several official translations no longer falls back to English. 24 more base names are translated, and the 暗影 class name shows in Chinese. (#289, #290)
- Chinese stat descriptions written on two or more lines translate as one description. Before, the second line stayed in English or was read as another stat: the second line of 心灵升华 was read as mana regeneration rate instead of mana recovery rate, so Chinese builds with it can now calculate differently. (#291)
- The unsupported-modifier lists on Overview and Compare no longer show raw `Tree:12345:` prefixes or `[Curse|Curses]` markup. Calcs shows skill names and short reasons instead of internal stat ids, and the Thorns and Explode skill groups have localized names. (#288)

### Data

- Language package `locale/0.5.5.5-3` adds passive names by id (`PassiveSkills.Name`, covering every named node on the tree); earlier entries are byte-identical to `-2`. The game package stays `game/0.5.5.5-2`. (#290)

### For developers

- `mise run audit:display [--release <tag> | --package <path>] [--strict] <builds>` runs the web app's naming path in Node over real build codes and reports errors, raw ids, untranslated text and calculation coverage; `--strict` fails on errors and raw-id names. (#287, #289, #291)
- `BuildData.base_item_names` (id to English name) sits next to `base_items`. (#285)

### Internal

- `docs/data-status.md` records that v0.0.44 serves the Release-based data. (#284)

## [0.0.44] - 2026-11-05

pobr.win now calculates with game data 0.5.5.5 (was 0.5.5.4.2), imported from the RePoE Release `game/0.5.5.5-2` and the translation Release `locale/0.5.5.5-2`. Several calculation fixes change DPS and EHP for affected builds, and the Items page can mark each flask or charm as selected. For WASM users the JSON schema goes from 16 to 19 and the translation exports are replaced by a language-neutral locale API; the CLI gains a global `--lang` option.

### Breaking changes

- Players: none. Builds saved with 0.0.43 open: equipment-socket jewels are restored from the build and every flask and charm counts as selected. Numbers can differ because of the new data and the fixes below. (#207, #218, #260)
- WASM: `SCHEMA_VERSION` is 19 (was 16). 17 adds `equipment_jewels`, 18 adds the flask and charm `active` flag, and 19 removes `translateLinesToZhCn`. (#207, #260, #275)
- WASM: decoded `items.flasks` also lists equipped flasks and charms that are not selected, each with `active: true|false`. In requests `active` is optional (default `true`), and slot names must be exactly `Flask 1`, `Flask 2`, `Charm 1`, `Charm 2` or `Charm 3`; other names and duplicate slots are rejected. A `flasks` override also replaces the unselected entries. (#260)
- WASM: equipment-socket jewels have their own `equipment_jewels` field in decode responses and calculate requests. Replacing `jewels` no longer drops them; send `equipment_jewels: []` to remove them. (#207)
- WASM: `translateLinesToZhCn` is removed; call `translateLines("zh-CN", linesJson)` instead. `translate(lang, key)` is removed without a replacement, since interface text belongs to the caller. (#263, #275)
- WASM and CLI data: the engine refuses a schema-3 `manifest.json` that does not declare `contract_version` 1, and data without `base/active_skill_types.json`. Data prepared for 0.0.43 does not load; use the data in the 0.0.44 WASM archive or run `mise run data:prepare`. (#267, #276, #277)
- Rust crates used by tag, such as the RePoE adapter: `strip_display_markup` moved to `pobr_data::text`; the built-in monster tables (`EnemyTierDefaults::compute`, `MonsterScalingRow`) were removed in favour of the data's `MonsterScalingTable`; the `pobr-i18n` API was replaced; `catalog::native_runtime` was removed. Depend on `contract/1.x` tags. (#262, #263, #264, #265, #266, #273)

### Added

- Items page: a "Selected for use" checkbox for each equipped flask and charm, kept through edits, saves, reloads, trade comparisons and build-code export. Item rules still apply; for example Reverie can keep an unselected non-unique Life Flask active. (#260)
- CLI: a global `--lang <code>` option (for example `zh-CN`). `calculate --mod`, `parse-mod`, `explain-mod` and `parse-item` accept lines in that language, and reports print unsupported lines, breakdown sources and item bases in it. Numbers are the same in every language. A language without line templates (zh-TW for now) warns and keeps English lines. (#275)
- WASM: `translateLines(lang, linesJson)`, `parseLocalizedLines(lang, linesJson)`, `availableLocales()` and `gameName(lang, kind, id)`, which follow the fallback chain the data package declares. (#264, #275)

### Changed

- Chinese game text comes from the new language-neutral `pobr-i18n` engine. On 47,570 Chinese and 67,663 English lines it matches the old converter, except that 14 Chinese base names matching two English bases now always resolve to the same one. (#264)
- Simplified Chinese falls back to Traditional Chinese names through the chain the data package declares. The 51 skills and 92 bases that exist only in zh-TW still show, converted to Simplified Chinese. (#275)

### Fixed

- Imported builds keep their selected effects in the right scope again: enemy resistance overrides (including 0), selected Shock, infused Flame Wall and Ice Bite, weapon-local guaranteed or unlimited poison, and Full DPS poison totals. Dark Effigy counts distinct eligible chaos damage-over-time effects, and an unlimited poison stack limit shows `∞` instead of `0`. One poison spear build went from 1,382 to 6,379 combined DPS. (#253)
- Dimensional Weakspot's 15% elemental resistance inversion is applied, and partial inversion resolves each hit outcome with its own penetration. Spirit of the Primate's freeze-based energy reads freeze eligibility instead of chill, and "cannot penetrate or ignore elemental resistances" also blocks penetration. (#256)
- Wildsurge Incantation applies its full effect to Storm and Plant spells: more damage, lower cost and shorter duration. (#257)
- The strongest Blind includes Blind from supports on minion attacks (Wolf Pack, Wild Protector); the strongest effect is used, not the sum. (#258)
- Bonuses nested in ring modifiers are scaled by slot effects such as Ingenuity and Mystic Attunement. (#259)
- Gemling Legionnaire: a gem's effective quality reaches compatible supports, Blasphemy and Spell Totem. Twister's additional projectile speed applies. Reverie and Shaman Mantle life-flask effects, Azmeri Brew recovery, Maim conditions and body-armour defence gates are modelled within stated limits, and false "applied" or "inactive" diagnoses are corrected. (#260)
- Saving a build no longer turns a defaulted condition into an explicit `false`, which changed an Infernalist's energy trigger rate after reopening. (#191)
- Config and Calcs keep their layout and scroll position during a recalculation, and equipment-socket jewels are no longer lost when a build is edited or saved. (#207)
- The Config Reset button no longer appears when a checkbox is set back to its default. (#209)
- Chinese support gems show Chinese effect descriptions (534 of 567; the rest show a marked fallback), and Evasion Rating is translated and grouped with the other item properties. (#183)
- Traditional Chinese affix names resolve. (#280)

### Data

- Game data 0.5.5.5 replaces 0.5.5.4.2. PoBR no longer stores game or language snapshots: it imports pinned Releases from RePoE (game) and poe2-zh-cn-stats (language), now `game/0.5.5.5-2` and `locale/0.5.5.5-2`, and fails when a package is missing or a hash does not match. (#218, #219, #221, #245, #270, #271, #272, #277, #278, #280)
- The 0.5.5.5 data is converted from pinned game-client files: stats and mods, item bases, skills, gem quality and effects, monster resistances and scaling, totems, minions and spectres, the passive tree, constants, runes, ascendancies, stat descriptions and the crafting catalogue. 4,624 of 4,913 passive nodes use the game's own stat descriptions. Fields the client does not provide yet are kept from the previous data and marked. The converter now lives in RePoE. (#192, #193, #194, #195, #196, #197, #198, #199, #200, #201, #202, #203, #204, #205, #206, #208, #210, #211, #212, #214, #215, #221, #240)
- Data contract: `contract/1.0` marks the first versioned contract (`CONTRACT_VERSION = 1`), and `contract/1.1` adds optional fields (`base_maximum_rage`, `active_skill_types.json`, per-node passive stat packets). Maximum rage, jewel radii, skill-type names and monster scaling come from the data package instead of tables in the engine; values are unchanged. (#265, #267, #276, #277)
- Translation coverage on 0.5.5.5: zh-CN covers 92–100% of each table (164 stat templates missing); zh-TW covers base items, skills and affixes. Missing text falls back to English. (#274)
- Owner publications were synchronized during the cycle, and the frozen 0.5.5.4.2 rollback package is retired. (#231, #244, #246, #251, #254, #255)

### For developers

- `mise run data:bump <owner> <tag>` adopts a new data Release: it verifies the Release (SHA256SUMS, receipt, contract version), rewrites the lock and `data/CURRENT`, and moves the support display with the language Release. [RELEASING.md](RELEASING.md) covers the process across the three repositories, including rollback. (#270, #278, #281)
- Game and language data are no longer committed under `data/<patch>/`. `pipeline/data.lock.json` pins the owner Releases, and the new `mise run data:prepare` downloads, verifies and installs them; `POBR_DATA_CACHE` points to a shared download cache. (#218, #249, #270)
- The 0.0.43 data-producing tasks (`data`, `data:fetch`, `data:adapt`, `data:regen`, `data:index`, `data:export`, `data:prune`) are removed; data fixes go to the owner repositories. (#235)
- An experimental schema-4 native calculation path (CLI `calculate-native`, WASM lifecycle exports) was added and removed within this cycle; 0.0.43 never had it. (#180, #181, #182, #184, #185, #186, #188, #189, #262)
- Agent skills live in `.agents/skills/` (`.claude/skills/` keeps symlinks); the WASM archive still ships `skills/use-pobr-wasm/`. (#247)

### Internal

- CI: quick PR checks and full integration runs on master, Rust tests in three parallel shards, cached WASM bindings, and the compact release packaged from the prepared owner snapshot. `pr-ready` suggests merge commits. (#176, #217, #236, #250, #255)
- Data pipeline cleanup: PoBR no longer produces or rewrites data. Raw-table, vendor and duplicate acquisition code is retired, the adapter and its policies moved to RePoE, and the unused vendor tree art was removed (the tree looks the same). (#224, #225, #226, #228, #232, #233, #235, #237, #239, #240, #243, #248, #261, #266, #273)
- Data tooling that was added and replaced within this cycle: per-domain lock files and lock schemas, the `--crafting` and `--keep-current` options, and the automatic data-sync workflow (`data-sync.yml`, `data:latest`), all superseded by the single lock and `data:bump` pull requests. (#231, #237, #238, #245, #246, #269)
- Web interface text is one JSON file per language in `web/src/locales/`, checked by `mise run i18n:check` in CI; `mise run i18n:coverage` reports game-text coverage. A smoke E2E runs when the data lock changes. (#268, #274)
- Tests: an obsolete domain-rejection case was removed, and `pobr-gamedata` parses floats exactly in every shard. (#213, #241)
- Docs: the three-repository architecture, data provenance, data status, maintainer guides, READMEs and screenshots. (#175, #187, #190, #220, #222, #223, #229, #230, #234, #242, #252, #279, #281, #282)
- Dependencies: Rust 1.99.1, `sha2` 0.11.1, TypeScript 7.0.2, Vite 8.3.5, Vitest 5.0.5, React 19.3.1 and `actions/cache` v6. (#177, #178, #179, #216, #227)

## Earlier releases

Notes for 0.0.43 and earlier are on [GitHub Releases](https://github.com/ackness/pobr/releases).

[0.0.48]: https://github.com/ackness/pobr/compare/v0.0.47...v0.0.48
[0.0.47]: https://github.com/ackness/pobr/compare/v0.0.46...v0.0.47
[0.0.46]: https://github.com/ackness/pobr/compare/v0.0.45...v0.0.46
[0.0.45]: https://github.com/ackness/pobr/compare/v0.0.44...v0.0.45
[0.0.44]: https://github.com/ackness/pobr/compare/v0.0.43...v0.0.44
