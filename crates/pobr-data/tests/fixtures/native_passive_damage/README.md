# Native passive damage packets

`packets.json` is a small controlled fixture of eight ordinary passive facts,
not a production data snapshot. Each packet records the native stat ID and
value, passive/stat row references, source manifest and raw-source hashes,
patch `0.5.5.5`, and the exact GGG exported node text. The packets attest the
reviewed unconditional player context: no requirements, choices, grants,
alternate actor, local weapon scope, or periodic/secondary behavior. The
fixture itself does not modify a snapshot. The guarded assembler now reuses
these core rules for complete single-stat nodes matching its reviewed ordinary
raw-context cohort; generated packets need no invented English display text.

Native source package: `items-actors-bc` (native v4), manifest
`56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0`.
Raw resource SHA-256 values are repeated in each row's references, alongside
exact decoded-table JSON pointers. The portable `evidence.json` retains all
eight physical source rows and their context flags, plus the client CSD branch
addresses, spans and hashes. The values are fixed regression facts extracted
from these inputs; production mapping contains no node-specific amounts.
Its official tree SHA-256 is
`b52be9c4f17e4114064255ef1b8c58292e9db0e395d95af235a8d3fef0d44642`, revision
`bd87e6512c92b868542eddfb1ba4ea8b6dc2da36`. Exact official export patch is
unknown; its declared upstream version is 0.5.5, so it corroborates wording,
not patch-wide equivalence.

| Node | Native row / stat row | ID and value | GGG node text |
| --- | --- | --- | --- |
| 52695 `chaos6` | 3241 / 26 | `physical_damage_+%` 10 | `10% increased [Physical] Damage` |
| 32845 `ignite_mitigation1` | 1671 / 42 | `fire_damage_+%` 10 | `10% increased [Fire] Damage` |
| 22713 `cold3` | 3096 / 49 | `cold_damage_+%` 10 | `10% increased [Cold] Damage` |
| 20649 `shock_mitigation1__` | 1696 / 56 | `lightning_damage_+%` 10 | `10% increased [Lightning] Damage` |
| 12367 `chaos3` | 3238 / 63 | `chaos_damage_+%` 7 | `7% increased [Chaos] Damage` |
| 22821 `cold2` | 3095 / 476 | `elemental_damage_+%` 10 | `10% increased [ElementalDamage|Elemental Damage]` |
| 53975 `spells1` | 2762 / 25 | `spell_damage_+%` 10 | `10% increased [Spell] Damage` |
| 33053 `ranged1` | 2949 / 488 | `projectile_damage_+%` 10 | `10% increased [Projectile] Damage` |

The separately pinned description package has manifest
`c8a475ba97b2fda3231ab05d23c4fdafa6b8593dec4a3d8307339b9cb6a794f2`.
Physical damage uses the passive-specific CSD entry; the other seven use
`stat_descriptions.csd`, included by that root. Positive branches use the raw
percentage without a handler. Negative branches use `negate 1` to display
“reduced”; the signed numerical input remains negative. These are `INC` modifiers
to the existing damage buckets, without additional flags or keywords. The skill
query selects SpellDamage and ProjectileDamage; ElementalDamage applies to Fire,
Cold and Lightning, excluding Physical and Chaos.

The Rust regressions cover parser-free native ingestion, existing noncritical
damage components, additive increases, skill/type scopes, value changes and
source attribution. The whole-packet gate and fallback remain in force. They do
not establish assembled-build admission, pools, crit or DPS behavior, and do not
change the active runtime data or admit allocations in the conditional-hit profile.
