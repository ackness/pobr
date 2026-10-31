# Runtime constants from target facts and three fixed compatibility files

`--ordinary-domain runtime-constants` produces the original game constants,
character constants and jewel radii DTOs as one domain candidate. Policy-selected
numeric leaves come from target `GameConstants`; every other field remains
explicit fixed compatibility from three supplied old snapshot files. Consumers,
formulas and Rust defaults are unchanged.

```sh
/path/to/fixed/pobr-data-adapter \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 --ordinary-domain runtime-constants \
  --codec-source /absolute/path/to/data/0.5.5.4.2/manifest.json \
  --codec-source-sha256 bfd31a4607da98af53b139ed101f31a67a3122e4520434fd2e3fda14672fcca4 \
  --codec-policy /absolute/path/to/pipeline/runtime-constants/0.5.5.5-facts-compat-policy.json \
  --codec-policy-sha256 49908ceb0454dac8904524e0a66cd16f9c0a038583b003f31a5cdc78483ee448 \
  --out /absolute/path/to/existing-parent/fresh-constants-candidate
```

The four codec flags are required. Quality and branch-codec options are rejected;
missing or duplicate arguments fail. F is authenticated by the existing
`reader::open` / `Tables::selected` path, selecting only `GameConstants` and
`source-lock.json`. The target patch and policy manifest identity must match F.

## Selected legacy inputs

The codec source is a pinned schema-3 snapshot manifest, not a numeric template.
Its version must match the policy declaration; `files` contains SHA strings.
Only these closed, policy-declared paths are opened under its directory:

| Path | Original SHA256 |
| --- | --- |
| `base/game_constants.json` | `7a90760b853088493bbcd377d274c5bf6582384c2ad8d59a30dbe0221f2e0e22` |
| `base/character_constants.json` | `e64bd63f5c2288447ff22e878297752aae8ec5ee9a6135e460f514914ed22150` |
| `base/jewel_radii.json` | `00c1e71af6666521c2e1fc3afd7af7f5eb9cb14b1a4537dbb44e8f2de6d7d336` |

Each selected pin must match both policy and manifest before the actual file is
hashed and parsed by `read_pinned`. An unknown filename/schema, altered relative
path or a link escaping the snapshot directory fails. No other manifest entry,
shared overlay, vendor, Lua, historical evidence or default numeric source is
opened. The supplied manifest and three selected files are actually verified
inputs, unlike policy evidence paths, which are authoring declarations only.
Unselected file contents and whole-snapshot target applicability are not certified.

## Numeric ownership

Bindings choose exact, nonempty source `Id` strings; physical rows are evidence,
not business IDs. Trailing whitespace is preserved, including unused IDs. For
each consumed binding, `Value` and `Divisor` must be exact schema-declared i32:

1. Convert both to f64 and divide `Value / Divisor`.
2. Divide by each `additional_divisors` entry in the declared order.
3. Require finite results and finite, nonzero evaluated divisors.

Signed values, zero and negative nonzero divisors are accepted. No rounding,
combined-divisor optimization, old-value equality gate or missing-ID fallback is
used. Unconsumed rows are inventoried with their original values and are not
evaluated as model constants.

The fixed policy currently binds 31 destinations to 30 source IDs: 29 game
fields, character ward regeneration and the jewel distance multiplier.
`BaseShockMagnitude` intentionally supplies two destinations with different
unit normalization. Percent-to-fraction adds `/100`; per-minute damage percent
adds `/60` then `/100`. These are the existing DTO units, not new formulas.

Input/output DTO deserialization validates known fields. Typed serialization is
used only to recognize known numeric object-leaf paths; its values, including
any defaulted values, are discarded. Output starts from raw JSON Values, so
unknown fields, arrays, order, unowned numbers and absent fields remain intact.
Duplicate destinations, unknown DTO leaves, missing object parents and invalid
raw numeric types fail.

A binding may explicitly allow a missing known leaf. The current policy uses
this only for `character_constants.ward_regeneration_per_minute_pct`, absent in
the old raw file. It is inserted from target `BaseWardRegenerationPercentPerMinute`,
not the DTO default. Receipt records old absence rather than an invented old
number. Non-owned absent defaults stay absent.

Historical jewel radii and unowned character/monster/model constants stay fixed
compatibility with their original version. No geometry, deflection cap, ES delay,
status, proc, actor or time model is inferred from names or adjacent constants.

## Output and receipt

The fresh directory has twelve files:

- `_domain_candidate.json` first
- Three ordinary files under `base/`, without added `_meta` wrappers
- `legacy-manifest.json` and `codec-policy.json`, verbatim
- The three verbatim originals under `legacy-constants/`
- `source-manifest.json` and `source-lock.json`
- `producer-receipt.json` last

Validation and serialization finish before reservation. `Output` reuses its
existing held-directory layer writer for `legacy-constants`, including exclusive
creation and namespace checks. Output may not overlap F, the supplied snapshot,
selected file parents, policy parent or production `data/`; overwrite and dangling
symlink targets are rejected. No full manifest, profile, `CURRENT` or adoption.

Receipt binds all input/output bytes, selected legacy paths/version/schema,
each binding and actual source row/Id/raw integers, distinct source pointers,
ordered arithmetic steps and output value. Old presence/value/equality are
comparison evidence only. It also records shared-source occurrences, unconsumed
rows/columns, and each file's hash of the complete Value after removing exactly
the owned leaves. This preserves evidence for unknown fields and full arrays
without copying every retained model value into the receipt.

The compiled revision includes the converter, policy validator, existing Output
and all three original DTO source files. Source acquisition remains draft,
`complete_runtime=false`, and adoption is `not-adopted`. These 31 fields do not
measure complete-package or ordinary BD/Web coverage.
