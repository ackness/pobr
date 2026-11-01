# Target jewel slots for ordinary character imports

`--ordinary-domain passive-jewel-slots` emits the existing
`overlay/passive_jewel_slots.json` schema from pinned `PassiveJewelSlots` and
`PassiveSkills` facts. The version overlay takes precedence over the shared
legacy file through the ordinary `GameData` loader. No DTO, build codec,
allocation law or calculation changes are involved.

Use the ordinary `--ordinary-facts`, `--facts-manifest-sha256`, `--version` and
fresh `--out` arguments, plus both `--supplement <directory>` and
`--supplement-manifest-sha256 <pin>`. The supplement is a matching-patch English
RePoE selected-table export; acquisition and export commands are documented in
[client discovery](../../pipeline/client-index.md) and the RePoE selected-table
guide. Missing pins, partial arguments and unrelated codec/policy arguments fail.

Every physical slot row is emitted in source order. Its exact `Slot` FK supplies
`PassiveSkills.Id` and the raw `PassiveSkillGraphId`; the source
`SinisterJewelSocket` flag is retained. The existing `PassiveJewel_<n>` import
contract uses physical row `n`, independent of graph-node or source passive row
numbers. Retired sockets outside the currently emitted tree are retained;
membership is not proof of allocation legality or China-realm equivalence.

Invalid FKs, non-socket references, invalid booleans or graph IDs outside the
source `u16` type reject. Populated cluster-size, proxy, replacement or start-index
fields also reject: the current ordinary DTO does not express their semantics.
Remaining raw fields and every consumed reference are preserved in the receipt.
Empty membership emits an empty list; no legacy rows are silently refilled.

The source manifest, selected-table identity and output fingerprints are explicit.
The standard domain-only guard, draft status, output protection and CURRENT stay
unchanged. Assemble into a separately identified complete ordinary package before
testing runtime adoption.
