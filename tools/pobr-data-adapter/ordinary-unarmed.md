# Ordinary unarmed bases from RePoE

`--ordinary-domain unarmed-data` emits the existing `base/unarmed_data.json`.
It reuses the physical reader and existing player-source verifier; it does not
select a native calculation profile or change the core's unarmed calculation.

Required inputs are `--ordinary-facts`, `--facts-manifest-sha256`, `--version`,
`--out`, all four `--codec-source`/SHA256/`--codec-policy`/SHA256 arguments,
and paired `--player-context`/`--player-manifest-sha256`. The codec source is the
exact legacy unarmed array, and the policy is
[`0.5.5.5-facts-policy.json`](../../pipeline/policies/unarmed/0.5.5.5-facts-policy.json).
The policy also pins the player source lock. All pins and semantic validation
complete before reserving a fresh output; every input directory is protected.

The selected parent tables are Characters, Stats, PassiveSkills and
ClassPassiveSkillOverrides. The latter two serve the existing player archive's
physical-reference checks, not unarmed damage or passive allocation. The complete
eight-file player source is verified by the unchanged verifier and archived in
flat `player-*` sidecars. It remains a source draft, not a newly approved source.

| Ordinary field | Source and interpretation |
| --- | --- |
| class_name / class_id | Characters.Name / IntegerId, joined by exact stable Id |
| physical_min / physical_max | Characters.MinDamage / MaxDamage |
| attack_rate | 1000 / Characters.WeaponSpeed milliseconds, without rounding |
| crit_chance | One plain Stats `unarmed_base_critical_strike_chance` assignment in Character.ot /10000 into the existing serialized fraction |
| weapon_type | Existing model vocabulary `None`; retained on matched legacy entries |

The common critical-strike field binding is explicit PoBR interpretation of the
generic Character.ot stat. It does not assert an AO-to-OT inheritance edge,
merge unrelated actor stats, or alter a selected native actor policy. The core
already converts the serialized fraction to percentage points once.

All target character rows are emitted. The policy binds eight historical class
entries to stable target IDs, so source name, class ID and numeric changes
propagate without using old values as an acceptance condition. Unknown old entry
fields remain unchanged. Four source-only classes receive their own source
fields. The exact historical Scion entry is retained separately; its class_id0
overlaps target Marauder0, explicitly as compatibility. The current consumer
uses class names, not these numeric IDs. It is not a target Scion fact.

Duplicate target identities/names/class IDs, a missing legacy-bound target,
nonpositive attack time, negative/inverted damage, malformed/nonliteral/ambiguous
critical strike, unknown legacy dispositions, stale historical exceptions and
unconsumed policy entries reject. No zero or partial fallback replaces missing
target facts. Source-only fields not represented by the existing DTO remain at
their pinned source and are disclosed as unconsumed.

The receipt records each physical row, source values, exact unit conversion,
legacy entry association, metadata assignment and retained historical entry.
The ordinary domain marker blocks direct publication. Package assembly,
calculation comparison, source approval and CURRENT adoption remain separate.

Focused validation:

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::unarmed::
```

Projection tests use synthetic metadata; actual producer execution additionally
exercises the existing complete player-source verifier. Both are required to
claim this source-to-output path was executed. Source speed606ms produces
`1000/606`, rather than the historical rounded1.65; record that numerical change
separately from unrelated build differences.
