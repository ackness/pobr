# Item grants from target facts

`--ordinary-domain item-grants` regenerates the existing
`overlay/item_granted_skills.json.reservation_free_bases` from target
`ItemInherentSkills` and `BaseItemTypes`, verifying `SkillsGranted` references
against `SkillGems`. The version overlay takes precedence over the shared file
through the existing ordinary loader. No DTO, reservation formula, item matcher
or calculation code changes.

The raw schema calls the third physical boolean **IsWeapon**. Fixed community
PoB schema/export code calls that same position **NoReservation**. The
[explicit interpretation policy](../../pipeline/policies/item-grants/0.5.5.5-community-semantics-policy.json)
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

Without a supplement, all fields other than `reservation_free_bases` remain exact legacy JSON Values.
Original metadata is nested intact under the new `_meta.legacy_metadata`, with
presence recorded. Original file and policy bytes are archived. Four unique
skill variants and historical exporter aliases remain compatibility; this does
not produce complete unique-to-mod or roll relationships.

The receipt retains all source rows, raw flags, interpreted flags, item and skill
references, both schema declarations, and explicit ownership. The standard
source-draft status, domain guard, protected output rules and CURRENT remain
unchanged.

## Target skill links

Optionally supply `--supplement <selected-table-directory>` and
`--supplement-manifest-sha256 <pin>` for a matching-patch RePoE
`ModGrantedSkills` export. The existing selected-table verifier accepts the
original producer and the exact `21ace02` producer fingerprint, with the same
English format, schema, decoder and source-identity checks. Localized envelopes
remain rejected.

Each retained variant's `game_mod_id` selects exactly one target row, joined
through `SkillGems → BaseItemTypes` for its `skill_name` and
`SkillGems → GemEffects → GrantedEffects` for its `granted_effect_id`.
A selected row needs exactly one primary effect. Missing or ambiguous matches,
invalid references, empty selected names or changed schema fail before output;
there is no first-effect guess or old-value fallback. New target names/effects
propagate without requiring equality to old values.

All source links and ordered effect references are retained in the receipt,
including links not selected by a legacy variant. Only those two variant fields
are replaced. Item names, base-name lists, variant membership, historical
`exported_effect_id` aliases and unknown fields retain their fixed compatibility
source. A mod-to-skill link does not establish the unique item that owns it or its
roll ranges. This mode does not claim complete unique data migration.

The existing DTO and ordinary consumers remain unchanged. The output still
archives the original legacy bytes and interpretation policy; supplement
identity and field ownership are recorded independently.
