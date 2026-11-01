# Item grant reservation flags from target facts

`--ordinary-domain item-grants` regenerates the existing
`overlay/item_granted_skills.json.reservation_free_bases` from target
`ItemInherentSkills` and `BaseItemTypes`, verifying `SkillsGranted` references
against `SkillGems`. The version overlay takes precedence over the shared file
through the existing ordinary loader. No DTO, reservation formula, item matcher
or calculation code changes.

The raw schema calls the third physical boolean **IsWeapon**. Fixed community
PoB schema/export code calls that same position **NoReservation**. The
[explicit interpretation policy](../../pipeline/item-grants/0.5.5.5-community-semantics-policy.json)
binds both schema identities and the three community evidence file hashes.
This is a retained community semantic interpretation, **not independently
confirmed official naming**. RePoE raw fields and source locks are not renamed
or promoted. Evidence paths in the policy are authoring references; the producer
verifies the policy bytes, not a local vendor checkout, and executes no Lua.

Use the standard `--ordinary-facts`, `--facts-manifest-sha256`, `--version`, and
fresh `--out`, plus all four pinned compatibility arguments:
`--codec-source`, `--codec-source-sha256`, `--codec-policy`, and
`--codec-policy-sha256`. The compatibility source is the original
`item_granted_skills.json`, not a full snapshot manifest. Policy version, raw
schema revision, column position/type and interpreted semantic must match.

Every source row participates. True flags supply target base names; false flags
remove membership, and an empty true set emits an empty list. Old names/counts
are not an admission gate. Duplicate item references, invalid flags/references,
empty emitted names or equal base names with conflicting flag ownership reject.
Repeated skill references remain ordered in the receipt. A name-only DTO cannot
resolve a collision by choosing a different item arbitrarily.

All fields other than `reservation_free_bases` remain exact legacy JSON Values.
Original metadata is nested intact under the new `_meta.legacy_metadata`, with
presence recorded. Original file and policy bytes are archived. Four unique
skill variants and historical exporter aliases remain compatibility; this does
not produce complete unique-to-mod or roll relationships.

The receipt retains all source rows, raw flags, interpreted flags, item and skill
references, both schema declarations, and explicit ownership. The standard
source-draft status, domain guard, protected output rules and CURRENT remain
unchanged.
