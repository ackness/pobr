# Ordinary fact conversion fixture

`physical-tables.json` is a small, synthetic five-table input. It is **not** a
client extraction, full source package or runtime artifact.

Column schemas and interval expansion are drawn from F's selected physical
tables (manifest `56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0`,
lock `d7a59a052109f411a62b1398ac844ff31ca8afd6d4158e3488446a19cb75743f`,
international `0.5.5.5`). Unconsumed columns were removed, source-column ordinals
renumbered, and synthetic byte widths set to 1. This is an adapter fixture, not
evidence of a physical DAT layout. Selected full payload hashes:

- Stats: `bef280b241a844972de0020e57c64cc403ac7e44fddf346b1014d17e1372376a`
- Mods: `9d31219bb40ba9c6651fd627905f091e35978beefa84fcd5a8789a45e819df1b`
- ModType: `c022a02f02cdbd0dc9f5199038bcf1d1133b08d4fa5043087250aa300b5ccf14`
- ModFamily: `1ae7eb6173410bbc08d8efa7b499460a05025ff6f90d90dc53fc2400e0893a26`
- Tags: `feaec2df8e2bba282ef479216665f8dc8e9b724b8538445ee6aab2879e61b13e`

Rows were manually adapted to exercise eight bound slots, all-zero and negative
ranges, crossing zero, raw enum zero versus null, nonalphabetic stable IDs,
different ModType group/ModFamily IDs, ordered tags/weights, unknown/empty
families, unnamed stat-bearing mods and a filtered empty shell. A null eighth
stat has nonzero unused endpoints, as also occurs in the real source; the
receipt must retain those endpoints without inventing a stat.

The test harness reuses `../native-facts/items-actors-bc-synthetic.json` for its
reviewed test envelope/lock structure, reseals the selected fixture payloads,
and leaves all unrelated payloads absent. Its trust values authenticate only
these synthetic test bytes; they do not claim new real-source acquisition.

## Base-item fixture

`base-item-tables.json` uses the same F manifest/lock and compact column-metadata
approach for the eleven selected base-item tables. Its hand-adapted rows are
synthetic, with the validation byte width set to 1 and selected source-column
ordinals renumbered. The seven base IDs have shared display names; component
order differs from base order, and ComponentCharges uses stable string IDs.
The fixture covers all existing item DTO fields, placeholder exclusion and a
UtilityFlask whose inherent charm effect is explicitly unavailable.

Secondary utility values deliberately differ from their primary values and one
is null. These are synthetic retention cases, not a claim that this F has those
differences. Raw UtilityBuff references are copied only as unconverted facts;
no referenced buff table or meaning is supplied by this fixture.
