# Contributing modifiers (overlay channel)

**How to add or fix a modifier in PoBR without writing any Rust.**

> **Source & update responsibility.** This document describes the on-disk
> schema of `data/<version>/overlay/*.json`, defined in code at
> `crates/pobr-data/src/catalog/parser_rules.rs` (the `ModParserRulesDoc` and
> `SpecialModsDef` families). **That file is the authority.** If you change the
> schema (add/rename a field, add a value-DSL operator, move a table), update
> this doc in the same PR. Command lines below were run and verified against
> `data/4.5.0.3.4`.

---

## 1. The big picture

PoBR parses a modifier line (e.g. `35% increased Fire Damage`) into structured
`Modifier`s that the calc engine aggregates. Two layers do this:

1. **The generic parser** (`overlay/mod_parser_rules.json`) — a data-driven port
   of Path of Building's `Modules/ModParser.lua`. It already handles the vast
   majority of "regular" phrasing (`N% increased X`, `+N to X`, `N% more X`, per-
   stat / conditional suffixes, …). **You rarely touch this file** — see §3.
2. **The special channel** (`overlay/special_mods.json`) — a **hand-curated**
   table of whole-line, anchored special cases (uniques, keystones, odd
   phrasings) that the generic parser can't express. **This is where community
   modifier additions go** — see §4.

A line that neither layer understands is not an error: it is collected as
`ParseStatus::Unsupported` and shows up in the coverage report. Nothing crashes.

At app startup, `pobr-build::build_data` loads both files and compiles them via
`CompiledParserRules::compile_with_special`. A malformed edit either errors at
load time or — worse — is **silently ignored** (unknown fields are dropped by
serde). Run `--check` (§7) before you commit to catch both.

---

## 2. The overlay directory

Everything under `data/<version>/overlay/` is the curated correction layer on
top of the machine-generated `.dat` import. The two files this guide is about:

| File | Curated by | Syntax | You edit it to… |
|------|-----------|--------|-----------------|
| `special_mods.json` | **hand** | Rust regex + value DSL | add/fix a whole-line special modifier (uniques, keystones, unusual phrasings) |
| `mod_parser_rules.json` | **extracted from vendor Lua** | Lua pattern | (rarely) extend the generic grammar — but see the warning in §3 |

> **Where to add a curated `special_mods` entry — use `data/overlay-common/`.**
> `special_mods.json` is loaded in **two layers** and merged by `pobr-gamedata`:
>
> 1. `data/overlay-common/special_mods.json` — **version-independent** curated
>    layer. Vendor-semantics fixes that do not change with the game patch live
>    here; a new data version directory inherits them for free (no manual copy).
>    **This is where new curated entries almost always go.**
> 2. `data/<version>/overlay/special_mods.json` — **version-specific** layer,
>    merged on top. Only entries that genuinely differ for one game version
>    belong here; a same-`id` entry here overrides the common layer, and new ids
>    are appended.
>
> Practically: add your entry to `data/overlay-common/special_mods.json` unless
> it is a correction that only applies to a single game version. `regen-all.sh`
> no longer carries `special_mods.json` forward between versions — the common
> layer replaces that manual step (see `docs/version-bump-architecture.md` P1-3).

The remaining overlay files are separate domains (base-item overrides, uniques,
gem effects, buff/curse definitions, stat descriptions, …). They follow their
own schemas in `crates/pobr-data` and are out of scope here; the same
"add JSON, no Rust" philosophy applies, and `--check` will grow to cover them if
they become a common contribution surface.

> **User patch layer (no PR needed).** For local-only additions you don't want
> to upstream, drop a JSON at `data/<version>/patch/<same-relative-path>` (e.g.
> `patch/overlay/uniques.json`). `pobr-gamedata` merges it over the official data
> at load time (object keys override, arrays merge by `id`). See
> `GameData::load_json_at` in `crates/pobr-gamedata/src/lib.rs`.

---

## 3. `mod_parser_rules.json` — the generic grammar (reference)

> **⚠ Do not hand-edit this file for new modifiers.** It is regenerated from
> vendor Lua by
> `cargo run -p sync-pob-catalog -- extract-lua --what parser-rules --vendor-root vendor/PathOfBuilding-PoE2/src --out data/4.5.0.3.4/overlay/mod_parser_rules.json`
> (see its `_meta.regen_command`). A manual edit is **clobbered** on the next
> regen. Extending the generic grammar means changing vendor Lua upstream or the
> extractor — that's a maintainer task. This section is here so you can *read*
> the file to understand why a line parses the way it does.

Top-level keys (each an array of rows; `patterns` are **Lua** patterns, e.g.
`(%d+)`, `%%`). Consumed by `crates/pobr-core/src/parse/mod_parser/compiled.rs`.

| Key | Vendor table | Role |
|-----|--------------|------|
| `forms` | `formList` | line-shape patterns → form id (`INC`/`BASE`/`MORE`/`PEN`/`DMG`/…); extracts the numeric value |
| `name_map` | `modNameList` | phrase (plain substring) → `ModName`(s) + optional effects |
| `flag_phrases` | `modFlagList` | phrase → `ModFlag`/`KeywordFlag` bits + optional tags |
| `pre_flags` | `preFlagList` | line-prefix pattern → flags / tags / wrap directives |
| `tag_phrases` | `modTagList` | per-X / conditional suffix pattern → tag template |
| `suffix_types` | `suffixTypes` | suffix scan for BASE/GAIN/GRANTS forms |
| `damage_types` | `dmgTypes` | damage-type table for DMG forms |
| `pen_types` | `penTypes` | penetration target for PEN form |
| `regen_types` / `degen_types` | `regenTypes` / `degenTypes` | resource regen/degen name sets |
| `cost_types_map` / `base_cost_types` | `costTypes` / `baseCostTypes` | skill-cost name sets |
| `flag_types` | `flagTypes` | FLAG-form phrase → `Condition:X` string or embedded mod |
| `unsupported` | `unsupportedModList` | lines to skip silently (vendor list) |
| `unsupported_pobr_extra` | — | PoBR's own additions to the skip list |

Shared effect fields (`flags`, `keyword_flags`, `tags`, `player_tags`,
`add_to_minion`, `mod_suffix`, …) are flattened into `name_map` / `flag_phrases`
/ `pre_flags` / `tag_phrases` rows via `RuleEffectsDef`. See `parser_rules.rs`
for the exact set.

`literal` and `anchored` on pattern rows are **derived index hints** (longest
literal substring for the aho-corasick pre-filter; `^` anchoring). You never
write them by hand — the extractor fills them.

---

## 4. `special_mods.json` — where you add a modifier

Add the entry to `data/overlay-common/special_mods.json` (the version-independent
layer — see §2) unless it is a correction specific to one game version. Each entry
is a `SpecialTemplateDef` (schema in `parser_rules.rs`). The parser
matches the **whole line** (case-insensitive, auto-anchored `^…$`) against
`pattern` (**Rust regex** — `(\d+)`, alternation, *no* look-around/back-refs),
and instantiates the `mods` template using the captures.

Minimal real entry (a keystone-style FLAG):

```json
{
  "id": "eternal_life_while_energy_shield",
  "pattern": "your life cannot change while you have energy shield",
  "vendor_pattern": "your life cannot change while you have energy shield",
  "mods": [
    { "name": "EternalLife", "type": "FLAG", "value": true }
  ],
  "verified": false,
  "batch": "fork-a",
  "source_note": "ModParser.lua:3144 (Lich specialModList)"
}
```

Entry with a numeric capture and a closed word-set (`enums`):

```json
{
  "id": "has_to_defence_per_player_level",
  "pattern": "has \\+(\\d+) to (armour|evasion|energy shield) per player level",
  "mods": [
    { "name": { "enum": 2 }, "type": "BASE", "value": "$1" }
  ],
  "enums": {
    "2": {
      "armour": "ArmourPerLevel",
      "evasion": "EvasionPerLevel",
      "energy shield": "EnergyShieldPerLevel"
    }
  },
  "verified": false,
  "batch": "S2"
}
```

Field summary:

| Field | Required | Meaning |
|-------|----------|---------|
| `id` | ✔ | stable snake_case id (rename = delete + add) |
| `pattern` | ✔ | Rust regex, whole-line, captures = `$1..$n` |
| `mods` | one of `mods`/`handler_id` | list of `ModTemplateDef` to emit (see below) |
| `handler_id` | one of `mods`/`handler_id` | Rust handler id for logic JSON can't express (§6) |
| `handler_args` | with `handler_id` | captures passed through, `["$1","$2"]` |
| `enums` | optional | `{"<capture#>": {"<word>": "<literal>"}}` closed word→name maps |
| `vendor_pattern` | optional | original Lua pattern (for `sync-pob-catalog check --special-coverage`) |
| `verified` | ✔ (`false` ok) | `true` only after an oracle diff confirmed the numbers |
| `batch` | ✔ | curation batch tag (`S0`/`S1`/`S2`/…) |
| `source_note` | optional | where it came from (unique name, `ModParser.lua:NNNN`, …) |

A `ModTemplateDef` (`mods[]`):

```jsonc
{
  "name": "FireDamage",        // literal ModName, or {"enum": 2}
  "type": "BASE",              // BASE | INC | MORE | FLAG | OVERRIDE | LIST
  "value": "$1",               // see the value forms below
  "flags": ["Attack"],         // optional ModFlag names
  "keyword_flags": ["Fire"],   // optional KeywordFlag names
  "tags": [ { "type": "Condition", "var": "Combat" } ],
  "target": "enemy"            // optional: player (default) | enemy | minion
}
```

---

## 5. Placeholder & value mini-language

Captures from the regex are referenced as `$1..$n` (1-based). There are two
related surfaces:

### 5a. `value` in a `mods[]` template (`special_mods.json`)

| Form | Example | Result |
|------|---------|--------|
| number literal | `50` | constant `50` |
| flag literal | `true` | for `"type":"FLAG"` |
| capture | `"$1"` | the captured number |
| expression | `{ "ref": "$1", "ops": [ {"negate":{}}, {"div":100} ] }` | capture through an operator chain |
| nested mod | `{ "mods": [ … ] }` | vendor `LIST { mod = mod(...) }` payload |
| list table | `{ "Key": "…" }` | structured LIST value (literals / `$n` / enums only) |

**Value operators** (the whitelist, applied left-to-right; single evaluator in
`crates/pobr-core/src/rules/value_expr.rs`):

| Op | JSON | `v →` |
|----|------|-------|
| negate | `{"negate":{}}` | `-v` |
| clamp | `{"clamp":{"min":0,"max":100}}` | `min(max(v,min),max)` |
| div | `{"div":100}` | `v / 100` |
| mult | `{"mult":10}` | `v * 10` |
| base | `{"base":6}` | `v + 6` |

### 5b. Inline placeholders in `mod_parser_rules.json` tag fields / `handler_args`

Tag field strings (and handler string args) use an inline dialect
(`crates/pobr-core/src/parse/mod_parser/template.rs`):

| Form | Example | Result |
|------|---------|--------|
| capture | `"$1"` | capture 1 |
| capitalise + concat | `"$2:cap+Effect"` | `firstToUpper($2) . "Effect"` → e.g. `FrenzyEffect` |
| numeric op | `"$1:mult(10)"` / `"$1:div(N)"` / `"$1:base(N)"` | capture × / ÷ / + `N` |

### DSL hard boundary

Allowed: `$n`, the five value operators, `:cap`/`:mult`/`:div`/`:base`, `enums`
closed sets, `target(player|enemy|minion)`. **Forbidden:** loops, recursion,
free expressions, cross-entry references, string concatenation of arbitrary
values. If a modifier needs any of that, it goes to a handler (§6). Adding a new
DSL capability requires ≥20 entries that would benefit — otherwise use a handler.

---

## 6. When JSON isn't enough → a handler

Use `handler_id` when the modifier needs real branching logic, a cross-domain
`LIST` payload, or reproduces a PoB2 closure constructor — anything the DSL
forbids. The entry carries only a stable id and the captures:

```json
{
  "id": "allocates_passive",
  "pattern": "allocates (.+)",
  "handler_id": "special:granted_passive",
  "handler_args": ["$1"],
  "batch": "S2"
}
```

Then register the handler in Rust:

- **Special-mod handlers** (`handler_id: "special:<name>"`) live in
  `crates/pobr-core/src/rules/handlers/` — one small module per handler, wired
  into `register_special_handlers` in that directory's `mod.rs`. See
  `handlers/explode.rs` for a complete minimal example (reads
  `ctx.inputs`, returns `HandlerOutcome::player_mods(...)`).
- **Config / buff handlers** (`config:<name>` / `buff:<name>`, a different
  channel used by `overlay/config_options.json` / `buff_definitions.json`) are
  registered in `crates/pobr-build/src/handlers.rs::build_registry`.

**Budget:** total handlers must stay `< 100`, and special handlers must stay
under 10% of `special_mods.json` entries — enforced by
`crates/pobr-core/tests/parser/special_mods_gate.rs`. Hitting the ceiling is a
signal the data split failed; template it instead.

---

## 7. Validate, test, and open a PR

**Step 1 — validate the JSON you edited** (cheap, run this first and always):

```bash
cargo run -p precompile-mods -- --data data/4.5.0.3.4 --check
```

This deserializes `mod_parser_rules.json` + `special_mods.json` (+
`generated/special_derived.json`; `generated/special_vendor.json` is
**required** — a missing file is an error, regenerate it via
`extract-lua --what special-mods`), reports **unknown/misspelled fields**
(via `serde_ignored`) and **type/syntax errors**, then compiles the rules —
catching **bad regex/Lua patterns, duplicate special `id`s, and `handler_id`s
with no registered handler**. Any problem → non-zero exit with every error
listed.

Two limits to be aware of:
- It does **not** validate `ModName`/`StatId` spelling — `StatId` is an open
  string with no registry, so a typo'd name just aggregates to nothing. The
  parity tests (below) are the backstop.
- Unknown-field detection sees top-level and non-flattened fields
  (e.g. a typo in `forms[]`, or a stray `special_mods` entry key). Fields that
  land in the flattened `RuleEffectsDef` (`flags`, `keyword_flags`, `tags`, …)
  are a serde-flatten blind spot — double-check those by eye.

**Step 2 — regenerate the precompiled corpus** (only if your edit changes how
existing lines parse — e.g. a new `special_mods` entry that now matches a line
already in the corpus):

```bash
cargo run -p precompile-mods -- --data data/4.5.0.3.4 --report
```

Commit the regenerated `data/4.5.0.3.4/generated/parsed_mods.json` and
`parse-coverage.json`. The committed coverage is golden-checked, so a stale
product fails CI.

**Step 3 — run the relevant tests:**

```bash
cargo nextest run -p precompile-mods                       # --check + corpus determinism
cargo nextest run -p pobr-core --test parser               # parser + special_mods gate
cargo test    -p pobr-build --test parity -- --nocapture   # PoB2 parity dashboard
```

`parity_no_regression` (inside the parity suite) is the regression gate: your
change must not drop the aggregate hit rate below the recorded baseline.

**Step 4 — open the PR.** Include: what modifier you added/fixed, the
`source_note` provenance (unique/keystone name or `ModParser.lua:NNNN`), whether
you set `verified: true` (only after an oracle diff), and the `--check` + test
output. Keep `verified: false` if you couldn't confirm the exact numbers — the
line still works, it's just flagged in the parity report.
