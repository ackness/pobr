# PoE2 0.5.5 data update

GGG's patch protocol returned `4.5.5.2` on 2026-09-11. That update advanced
the data snapshot from `4.5.4.8`. For the subsequent active snapshot, see
[the 0.5.5c update](adapting-to-0.5.5c.md). Raw tables come from the GGG CDN;
the tree comes from GGG's skill-tree export, supplemented by the pinned
PoB2 tree for coordinates, variants and anoint-only nodes.

The PoB2 reference is pinned to
`ce566eac45ea8a86477f513c7ee65a1ebe60014e` on `dev`, which includes the
[0.5.5 data export](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/49e93925dcb79024175c58d25befadab67b4dd44).
The latest tagged release, v0.23.1, predates this export. The generated
overlays record their exact reference commit and regeneration command.

Newer Lua exports return constructors instead of immediately populating
tables. Extraction supports both forms, including the returned minion
tables. This matters because the old loader could silently emit empty
skill, quality, minion and base-item overlays. The new `IMMUNE` parser
form supports single and paired statuses with PoB2's false-positive guard.
Config extraction discards the display-only `IgnoreCond` marker while
retaining its following calculation conditions, including nested minion
rage modifiers. The existing unverified-handler budget stays unchanged.
The legacy `customMods` text input remains available: newer PoB2 moved its
editor into ConfigTab mod groups, while old builds and PoBR still use this input.
Nested rune bonuses are flattened back to conditional `Bonded:` item lines,
which retain their condition gate. Version-specific
curated special modifiers are carried forward before vendor deduplication;
the common layer alone does not contain all existing corrections.

Regeneration:

```sh
pipeline/bump-version.sh --patch 4.5.5.2 \
  --vendor-sha ce566eac45ea8a86477f513c7ee65a1ebe60014e
```

The pipeline explicitly targets the new snapshot when extracting special
modifiers, before advancing the active version. Golden test pins remain
associated with the data version their tests load.
The precompile artifact-consistency test follows the active data snapshot,
with the existing coverage ratchet unchanged. It validates regenerated
parser artifacts rather than asking new parser code to reproduce archived
coverage reports.
Historical snapshots and `GOLDEN_PARITY_DATA_VERSION` are retained so old
golden values are not falsely treated as 0.5.5 reference calculations.
The hand-curated constants layer is carried forward under the existing
snapshot policy. The Chinese dictionary retains its own upstream version
metadata; it is not advertised as a fresh China-client data export.

This update refreshes game data and the required extraction/parser
compatibility. It does not claim parity with every unreleased PoB2 engine
feature, such as independent skill weapon sets. Existing unsupported-line
and calculation diagnostics continue to apply.
