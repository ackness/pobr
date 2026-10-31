# Selected physical Spark cells

`spark-physical.json` retains 53 unmodified physical rows, indexed by their original
table row, from RePoE native v4 manifest
`56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0`
(client candidate `0.5.5.5`, producer `e41401224a085a676c84b8f14b144f20fe37c79a`).
The embedded manifest retains raw-resource and complete payload fingerprints.
Source status remains `unreviewed-acquisition-draft`; this is not game accuracy
or CN-realm certification.
Game facts and text retain GGG's rights; the fixture grants no additional rights
to upstream content.

Tests restore sparse original indices in memory and exercise the projection
directly. This is deliberately separate from the complete archive verifier,
which requires all 77 physical tables and the Default graph. Tests change stored
damage and coefficients independently to detect using coefficients as flat
damage or hardcoding the selected amounts. Malformed parallel channels and
ambiguous level rows must fail.

The one-node graph used in tree/loader unit tests is explicitly synthetic. Its
node definition and stat are these actual cells; geometry is a test coordinate,
not the official tree. The real 5,152-node replay is separate acceptance evidence.
