# Build import formats

A BD arrives in one of several shapes. They describe the same character but not
the same things: what one carries another leaves out, and some write the same
field differently. This page records, per format, what the importer reads, what
it has to infer, and what it reports instead of guessing. Treat it as a checklist
when a build "looks wrong after import".

All imports end in the same `Build`, and every loss is written down: a skipped
item or slot in `ImportReport`, an unreadable line in `unsupported_modifiers`,
and for WeGame a line starting `[import]` in the build's notes.

## Where each format is decided

| Input | Decided by | Reader |
|---|---|---|
| PoB2 code (base64 zlib XML) | the text decodes as a code | `pobr-build` `decode_pob_code` + `xml_build` |
| Share link (`pobb.in`, `poe.ninja/poe2/pob`, `pastebin.com`, `rentry.co`, `poe2db.tw/pob`, `maxroll.gg/poe2/pob`) | the host, in `web/public/build-import.mjs` | the page fetches the raw code through the Pages worker, then the PoB2 reader runs. A poe.ninja *profile* page is refused (`ninja_profile`): it has no code |
| WeGame share link | `www.wegame.com.cn/helper/poe2/#/share/<key>` | the worker fetches five profile calls and returns one JSON; `build_api/wegame.rs` reads it |
| WeGame share JSON | the object has `format` | same |
| Game Build Planner v1 `.build` | no `items`, and a `name` | `build_api/planner_file.rs` |
| Older China-server `.build` | an `items` array | `build_api/decode.rs` |
| PoBR save / workspace | `version` + `state`/`workspace` | the session loader; never passed to a permissive decoder |

## What each one carries

| | PoB2 code (incl. poe.ninja, pobb.in) | WeGame share | Planner v1 `.build` | Older CN `.build` |
|---|---|---|---|---|
| Character level, class, ascendancy | yes | yes (level, `class_name` is the ascendancy for ascended characters) | level only if given in a hint, default 1 | level from `Lv.N` in the name |
| Passive tree | node ids, tree version, weapon-set split, attribute choices | node hashes, weapon-set `specialisations`, attribute choices in `skill_overrides` | game node ids, weapon sets | node slugs (mapped to ids) |
| Equipment | full item text with variants, ranges, runes | per slot JSON, text rebuilt by the importer | hints only, no items generated | no base name or rarity: built as RARE with a placeholder |
| Skills and supports | skill ids, level, quality, enabled, `mainActiveSkill`, sets | zh-CN names only; level and quality as property strings | game gem ids, defaults level 1 / quality 0 unless the exact `Level N, Q% Quality` hint is present | gem ids, default level 20 / quality 0 |
| Main skill | yes | **no** | no | no |
| Skills granted by passives or items | marked: `<Skill source="Tree:<node>">`, item grants, `enableGlobal` | **not marked**: they appear as ordinary groups | not carried | not carried |
| Weapon-set binding of skills | yes | **no** | weapon sets only | second set skipped |
| Config (enemy, charges, toggles) | yes, plus reference `PlayerStat` values | quest rewards only | no | no |
| Jewels in the tree | socket node id | socket named `PassiveJewel_<n>` | no | no |

## Per-field differences the importer normalises

### WeGame

- **Gem level and quality.** A value can carry a note: `20(最高等级)` is level 20 at the gem's maximum, `+20%` is 20. The leading number is read. (A whole-string parse once lost the level and calculated the skill at 1.)
- **Gem names.** Only the zh-CN name arrives, so a gem is found by `(name, is_support)`. Some names are shared: Herald of Ice also names the version Coming Calamity grants, and Unleash names a support and active skills. Item-only effects (the catalog's item-granted variants and `Unique`-prefixed effects) yield to the ordinary gem when both exist; a name that is still ambiguous (the default weapon attacks) is reported as `Unknown or ambiguous gem`.
- **Jewel sockets.** `PassiveJewel_<n>` is row `n` of the game's `PassiveJewelSlots` table, which points at the passive that is the socket (`data/overlay-common/passive_jewel_slots.json`). A tree node id works too. A row the tree has no socket for is reported and the jewel is left out.
- **Bonded (羁绊) rune lines** are class-only and not applied; the notes say so.
- **Quest rewards** arrive as text lines; the progress counter `N Broken Boss Faces` is recognised as not modelled.
- **Main skill, skill sets, weapon-set bindings** are not provided. The importer selects the first group, so check the main skill before reading any DPS (the notes carry a reminder). In the one real share checked while writing this, the first group was the default Bow Shot, not the build's skill.

### PoB2 codes (poe.ninja and the others)

- **Granted skills are listed twice.** A character exported from the game lists a skill a passive or an item grants as an ordinary group too (first in its set, at the granted level, with its supports). That group is the granted skill: the tree's own `Tree:` group yields to it and the main group follows. A grant whose skill the build does not list (or lists disabled) is still reported as an unsupported line.
- **Reference values.** `<PlayerStat>` is the exporter's result at export time, possibly from an older exporter version. It is a comparison point, not the truth (`mise run diagnose`, `compare`).
- **Mod text.** Variant tags, range rolls, `Implicits: N` (a serialised boundary that includes enchant and rune lines) and wrapped game text are all read from the item text; a line that is only partly understood is kept as text and reported, not applied.

### Planner v1 and older CN `.build`

- Equipment is a hint at best; generated items are not invented. Gem levels are defaults, so the numbers are a shape, not a result.

## Rules for a new or changed format

1. Normalise at the edge: the reader turns a platform's spelling into the engine's (`20(最高等级)` → 20) so nothing downstream knows the platform.
2. Prefer the game's own table over a guess. When a format uses an id the repo cannot resolve, export the table (as was done for `PassiveJewelSlots`, with its export hash recorded) before inventing a heuristic.
3. Say what was lost. Anything the reader cannot place goes to the notes or `ImportReport`; a silent drop is a bug.
4. A fix for one share needs a synthetic test that covers the shape, not the account: the WeGame tests in `apps/pobr-wasm/tests/wegame.rs` use invented payloads (no account ids, share tokens or player exports).
5. When a format lacks something the others carry (the main skill, the grant markers), do not copy a default that looks plausible into the result; ask for it, or report it.
