# Change recipes

Exact steps for the most common small changes. Each recipe names the files to
edit, the one check to run, and what to leave for the batch gate. Use these
instead of exploring the codebase. Conventions and reasons are in the
[maintainer guide](maintainer-guide.md); rule syntax is in
[contributing-mods](contributing-mods.md).

**Budget.** A recipe change compiles at most one small test target. Do not run
`cargo test -p pobr-core` (all targets), `--test parity`, the WASM/CLI suites,
`mise run accuracy`, `triage`, `diagnose`, `compare` or the coverage scripts for
it. Those compile the large parity binary or the CLI and belong to the batch
gate (`mise run gate`), which the person merging runs once for many changes.

## Find what already exists (no compilation)

| Question | Command |
|----------|---------|
| Game stat id, full text, sources, ledger fate, nearby rules for a line | `mise run gap "<line or template>"` (in a new worktree, set `POBR_BIN` and `POBR_CACHE_DIR`, below, so it neither builds a CLI nor recomputes the ledgers) |
| A skill, stat, base, node or unique in game data | `mise run lookup <id or name>` |
| Who reads modifier name `X` | `rg -n '"X"' crates/pobr-core/src crates/pobr-build/src` (readers use `named("X")`, `sum(.., "X")`, `flag(.., "X")`) |
| An existing rule for similar text | `rg -n -i '<a distinctive word>' data/overlay-common/special_mods.json` |
| An existing skill-stat meaning | `rg -n '<stat_id>' crates/pobr-core/src/rules/skill_stats.rs` |
| Test suites | `mise run targets` |

## Add one game line (item, passive, jewel, flask/charm text)

1. Find the stat id and existing reader with the table above.
2. Decide where the line goes:

   | The line is … | Edit |
   |---------------|------|
   | an existing modifier name with a value or condition | an entry in `data/overlay-common/special_mods.json` |
   | a skill/support gem stat | `crates/pobr-core/src/rules/skill_stats.rs` (stat id → modifier), and `exporter_rule` there for provenance |
   | logic JSON cannot express | a handler in `crates/pobr-core/src/rules/handlers/` + `handler_id` in the entry |
   | recognised but changing no output | entry with `"mods": []`, `"batch": "S0"` |
   | recognised, mechanic not built yet | entry with `"mods": []`, `"batch": "unmodelled"`, `source_note` naming the missing model |

3. Write the entry. Required fields: `id`, `pattern` (whole line, lower case,
   Rust regex), `mods`, `batch` (`S2` for a curated rule), `verified: false`,
   `game_source` (the stat id), `exporter` (`"absent"` when PoB2 does not
   implement it, `"differs"` when it does differently; omit for `"same"`), and
   `examples` with the exact game text. `examples` are matched against the rule
   by the check in step 4, so they are the parse test.
4. Check and regenerate: `mise run regen` (validates every rule and example,
   rewrites `data/<version>/generated/parsed_mods.json` and
   `parse-coverage.json`, then tests precompile-mods). Commit the regenerated
   files; never edit them by hand.
5. Only when the modifier name had no reader, or the reader needs new
   behaviour: add the reader and one engine test in the closest existing file
   under `crates/pobr-core/tests/<suite>/`, then run
   `cargo test -q -p pobr-core --test <suite> <test name>`.
6. One sentence in the maintainer guide when the line sets a convention
   (an assumption, a PoB2 difference, an untested game behaviour).

That is the whole change. The ledger, accuracy and parity effects are measured
by the batch gate.

## Add one Calcs display field

1. The value must already exist in `OutputTable` (`crates/pobr-core/src/calc/output.rs`)
   or be computed from retained calculation terms. Never add a second
   computation of something calculated elsewhere.
2. Edit, in order:
   - `crates/pobr-core/src/display_catalog.rs`: a `computed("<Id>", Cat::…, Vt::…, "<PoB key>")`
     entry and the `"<Id>" => output.<field>` arm.
   - Detail kind: a derived formula in `crates/pobr-core/src/calc/session/detail_views.rs`
     (add the id to `DERIVED_STATS`) whose inputs rebuild the value, or a source
     group from the same ModDb query the calculation uses. `pobr_build::breakdown::detail::detail_kind` must agree.
   - Labels: `crates/pobr-i18n/locales/en-US/stats.toml`, `crates/pobr-i18n/locales/zh-TW/stats.toml`,
     `web/src/lib/detailLabels.ts`.
   - The locked count in `crates/pobr-core/tests/golden/display_catalog.rs` (+N, with a comment).
3. Test: `cargo test -q -p pobr-core --test golden display_catalog` and one
   engine test that checks the value and that the derived inputs rebuild it.
4. Compare with PoB2 on two or three builds only, using a CLI the gate already
   built: `target/debug/pobr calculate-build --file <code.txt> --json` against
   `.cache/pob2-oracle/<hash>.json` → `calcsOutput.<PoB key>`. The full
   `output-coverage.py` run belongs to the gate.

## Implement a missing mechanic

Same as above, plus the source: game stats in `data/<version>/base/`, game
constants, keyword text or official patch notes, quoted in the guide paragraph.
PoB2 (`vendor/`) is a cross-check only; never cite its line numbers. State
every assumption that has no in-game test.

## Batches and the gate

Several items in one area go to one branch. Implement them one after another
with only each item's check from the recipes above, run `mise run regen` once
after the last rule edit, and commit.

The person merging runs the batch gate once after merging several branches:

```bash
mise run gate                    # every step, then a summary of failures
mise run gate --only core,parity # rerun only what failed, after fixing it
```

## Working in a new worktree

A new worktree has an empty `target/` and an empty `.cache/`. Without help,
the first test rebuilds every dependency, and the analysis tools (`gap`,
`gaps`, `triage`, `compare`, `config:coverage`, `skills:sweep`,
`output-coverage.py`) build a CLI of their own and recompute every build's
ledger and PoB2 oracle result. Point them at a checkout that already has these:

```bash
mise run warm /path/to/warm/checkout                         # once: copy-on-write clone of its target/debug
export POBR_BIN=/path/to/warm/checkout/target/debug/pobr     # analysis tools use this CLI, build none
export POBR_CACHE_DIR=/path/to/warm/checkout/.cache          # share its ledgers and oracle results
```

`POBR_BIN` runs that checkout's engine, not this worktree's code: use it to
read game data, ledgers and PoB2 values, not to measure your own change. Your
change is measured by your focused test, and by the gate after merging.
