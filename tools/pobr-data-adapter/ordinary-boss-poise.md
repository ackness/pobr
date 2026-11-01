# Boss preset poise values from target facts

`--ordinary-domain boss-poise` projects the selected target `MonsterVarieties`
into the existing `overlay/boss_poise_thresholds.json` DTO. The ordinary loader
already prefers this version overlay over the historical shared file. No
calculation, rarity modifier or percent conversion changes.

Use the usual `--ordinary-facts`, `--facts-manifest-sha256`, `--version`, and fresh
`--out` arguments with `--catalog-policy` and `--catalog-policy-sha256` pointing
to the pinned [preset policy](../../pipeline/boss-poise/0.5.5.5-preset-policy.json).
The target patch must match the policy. The existing reader verifies the selected
physical table and source lock. The policy is copied verbatim to the candidate.

Numeric values come from `PoiseThreshold`, in raw percent. The policy retains
three existing **simulation preset choices**, separately from those facts:

- Boss: the unique mode among rows with `BossHealthBar = true` and `MAP` in their
  exact stable Id.
- Pinnacle: the exact Xesht Id in the policy.
- Uber: explicitly reuse the pinnacle representative.

This does not assert that all bosses, pinnacles or Ubers have these values.
No old numeric value is an input or acceptance gate. Changes to source values
or qualifying membership recompute the mode. Empty/tied modes, missing or
duplicate identities, invalid flags, and selected values outside nonnegative
i32 reject before output. Unselected poise values are not evaluated or declared
ineffective. Original Ids, including trailing whitespace, are preserved.

The receipt lists every qualifying row, frequencies, selected representative,
source pointers and the explicit Uber alias. Existing output protection,
acquisition-draft status, domain-candidate guard and CURRENT remain unchanged.
The shared legacy file stays available to older packages; it is not relabeled
or rewritten as target data.
