# Controlled native passive fixture

This is a **synthetic baseline containing one real selected passive fact**, not a
production data pack or an old active dataset relabelled as a newer patch. The
schema-3 version is `controlled-native-passive-0.5.5.5`. Empty item/gem/effect tables
and the zero-attribute `Fixture` class are controlled inputs. There are no parser
rules. Tests import allocation 62518 and run the ordinary full build calculation
with character injection and imported default quest rewards disabled: FireResist
is 0 without allocation and 5 with it. A synthetic effect passive proves 100%
extra small-passive effect gives 10 through the same ingestion channel.

The real fact is `maximum_resistances1`, Default graph node 62518:

- Native `Stats` physical row 139: `base_fire_damage_resistance_%`.
- Native `PassiveSkills` physical row 1390: `Stats = [139]`, `Stat1Value = 5`.
- No ascendancy, requirement, variant, mastery, attribute choice, grant, or jewel
  socket in the selected source row. Physical enum fields do not define the rule.
- GGG tree text: `+5% to [Resistances|Fire Resistance]`, corroborating the additive
  player fire-resistance interpretation owned by Rust.

[evidence.json](evidence.json) retains selected raw fields, decoded-resource
hashes, raw input hashes, source status and source addresses. The runtime packet
also retains the source manifest hash, definition/stat rows and references. Hashes
and selected records were rechecked against the sealed local A0 payloads when this
fixture was created. Raw input hashes come from A0's source lock; the raw decoder
was not rerun. No runtime loader opens these evidence paths.

Native source: RePoE A0 `core-skills-a0`, patch **0.5.5.5**, producer
`aa2fb163aa422317db5c32031607eb199214e7b7`, manifest
`e4bd693d45d8eaa4768d9a86c3e7e5ee84f1f4d8fc05112ae3b290d51bb901e5`.
Its status remains `partial-candidate` / `unreviewed-acquisition-draft`.

| Raw resource | SHA-256 |
| --- | --- |
| `Data/Balance/Stats.datc64` | `e808452ef330de79cca6d496982bc8810e7967c2c0c681e61dc63506177af8a5` |
| `Data/Balance/PassiveSkills.datc64` | `49af4f7dfab787b57efa22e06e2673c268cbd90c4f40140aa085cb8fbb602bc2` |
| `Metadata/PassiveSkillGraph.psg` | `071cba806a621a0bdae09828cc5ad8e34259159659e950a1898770790730c344` |

Text source: [GGG tree export at bd87e651](https://github.com/grindinggear/poe2-skilltree-export/blob/bd87e6512c92b868542eddfb1ba4ea8b6dc2da36/data.json),
`/nodes/62518/stats/0`, SHA-256
`b52be9c4f17e4114064255ef1b8c58292e9db0e395d95af235a8d3fef0d44642`.
Its declared upstream version is **0.5.5** and exact game patch is **UNKNOWN**.
The matching node does not prove patch-wide equivalence with native 0.5.5.5.
The active 0.5.5.4.2 pack was neither copied nor changed.

Only `manifest.json` and its six inventoried `base/` files form the runtime pack.
The loader validates their hashes on disk and in memory. `evidence.json` and this
README are documentation. Core tests deserialize the same selected node to check
source attribution without I/O inside the engine. Build tests load the sealed pack
through GameData and BuildData before allocation. Additional values, node IDs,
variants, jewel families and conditional texts in tests are explicitly synthetic
edge cases; they are not additional source discoveries.

Focused verification (sequential; batch gate belongs to integration):

```text
mise run verify core sources passive_source::
mise run verify build skills native_passive_stats::
```
