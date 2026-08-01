//! `overlay/skill_overrides.json` loader + a dedicated merge — per-skill
//! override values (critChance / attackSpeedMultiplier / baseMultiplier /
//! statSet Speed MORE, extracted from vendor PoB2 Lua); schema in
//! [`pobr_data::catalog::skill_overrides`].
//!
//! Why not go through the generic [`crate::overlay`] merge engine: this
//! table is a flat "(skill, stat) → value" list, not a JSON tree shaped
//! like base (base's side is an `effect id → level-row array` /
//! `SkillStatSetDef` array) — the shape doesn't fit key-level recursive
//! merge, so this domain has its own merge functions, locked in by unit tests.
//!
//! Merge semantics (aligned with `tools/pobr-data-adapter`'s field
//! normalization rules, so "plain base + overlay merge" is value-equal to
//! the historical hand-patched base):
//!
//! 1. A single `value` → applied to **every** level row of that skill (the
//!    extraction side only compresses to a single value when it appears at
//!    every vendor level with the same value); a `per_level` breakdown →
//!    only overrides the listed levels, missing levels aren't filled in
//!    (faithful to vendor — e.g. RisenArbalestSnipe only has
//!    baseMultiplier at L1).
//! 2. Normalization (matching how the adapter handles the same-named
//!    `.dat` columns): `attack_speed_multiplier == 0` and
//!    `base_multiplier ≈ 1.0` are trivial values, skipped without writing
//!    (the base field stays `None`); `crit_chance` is written as-is
//!    (including 0, distinct from missing).
//! 3. An overlay skill absent from base → skipped (a vendor-only skill
//!    with no corresponding `GrantedEffects` row in the `.dat`, e.g.
//!    `EnemyExplode`).
//! 4. An unknown `stat` name → an error, not silenced (schema evolution
//!    must stay in lockstep with the consumer).
//! 5. `skill_attack_speed_more`: written into [`SkillStatSetDef`] by
//!    effect id; when several entries share an id (multiple statSets),
//!    **the first one wins** (`stat_set` is sorted ascending for
//!    determinism); when base has no such effect, a minimal entry (empty
//!    stat, empty levels) is **appended** so the value isn't lost.

use std::collections::BTreeMap;

use pobr_data::catalog::skill_overrides::{
    OVERRIDE_DOT_FLAG_STATS, OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER, OVERRIDE_STAT_BASE_MULTIPLIER,
    OVERRIDE_STAT_CRIT_CHANCE, OVERRIDE_STAT_DOT_IS_AREA, OVERRIDE_STAT_DOT_IS_ATTACK,
    OVERRIDE_STAT_DOT_IS_HIT, OVERRIDE_STAT_DOT_IS_PROJECTILE, OVERRIDE_STAT_DOT_IS_SPELL,
    OVERRIDE_STAT_EXPLODE_CORPSE, OVERRIDE_STAT_IMPLICIT_STAT,
    OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE, SkillOverridesDef,
};
use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef, StatSetDef};

/// Whether this is a statSet-level stat (consumed by
/// [`apply_stat_set_overrides`] / [`apply_dot_flag_overrides`] /
/// [`apply_implicit_stat_overrides`]; the level-domain merge skips it).
fn is_stat_set_stat(stat: &str) -> bool {
    stat == OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE
        || stat == OVERRIDE_STAT_EXPLODE_CORPSE
        || stat == OVERRIDE_STAT_IMPLICIT_STAT
        || OVERRIDE_DOT_FLAG_STATS.contains(&stat)
}

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the per-skill override-value overlay (always resolved under
    /// `overlay/`). Returns `Ok(None)` when the file is missing (an old
    /// data pack without the overlay layer) — the consumer behaves as
    /// plain base, backward compatible; other I/O / parse errors still
    /// propagate, not silenced.
    pub fn skill_overrides(&self) -> Result<Option<SkillOverridesDef>, LoadError> {
        match self.load_json_at::<SkillOverridesDef>(self.overlay_path("skill_overrides.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

/// Normalizes a trivial value (matching the adapter's `.dat` column
/// handling): returns `None` to mean skip without writing.
fn normalized_level_value(stat: &str, value: f64) -> Option<f64> {
    match stat {
        // adapter: `raw.attack_speed_multiplier.filter(|&m| m != 0)`
        OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER if value == 0.0 => None,
        // adapter: `raw.base_multiplier.filter(|&m| (m - 1.0).abs() > 1e-9)`
        OVERRIDE_STAT_BASE_MULTIPLIER if (value - 1.0).abs() <= 1e-9 => None,
        _ => Some(value),
    }
}

/// Gets the target field on a [`SkillLevelDef`] by stat name. An unknown
/// stat returns `Err` (rule 4).
fn level_field<'row>(
    row: &'row mut SkillLevelDef,
    stat: &str,
) -> Result<&'row mut Option<f64>, String> {
    match stat {
        OVERRIDE_STAT_CRIT_CHANCE => Ok(&mut row.crit_chance),
        OVERRIDE_STAT_ATTACK_SPEED_MULTIPLIER => Ok(&mut row.attack_speed_multiplier),
        OVERRIDE_STAT_BASE_MULTIPLIER => Ok(&mut row.base_multiplier),
        other => Err(format!(
            "skill_overrides has an unknown level stat `{other}` (not wired up on the consumer side, refusing to silently drop it)"
        )),
    }
}

/// Merges the overlay's level-domain override values (crit_chance /
/// attack_speed_multiplier / base_multiplier) into the
/// `granted_effect_levels` domain. See the module doc for the semantics.
pub fn apply_level_overrides(
    levels: &mut BTreeMap<String, Vec<SkillLevelDef>>,
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    for entry in &overrides.overrides {
        // statSet-level overrides are consumed by apply_stat_set_overrides /
        // apply_dot_flag_overrides; skipped here.
        if is_stat_set_stat(&entry.stat) {
            continue;
        }
        // Rule 3: a vendor-only skill (no corresponding effect in the .dat) is skipped.
        let Some(rows) = levels.get_mut(&entry.skill) else {
            continue;
        };
        match (&entry.value, &entry.per_level) {
            // A single value → every level row.
            (Some(value), None) => {
                if let Some(v) = normalized_level_value(&entry.stat, *value) {
                    for row in rows.iter_mut() {
                        *level_field(row, &entry.stat)? = Some(v);
                    }
                }
            }
            // A breakdown → only the listed levels (rows are ascending by
            // level, check the breakdown row by row).
            (None, Some(per_level)) => {
                let by_level: BTreeMap<u32, f64> = per_level.iter().copied().collect();
                for row in rows.iter_mut() {
                    if let Some(&value) = by_level.get(&row.level)
                        && let Some(v) = normalized_level_value(&entry.stat, value)
                    {
                        *level_field(row, &entry.stat)? = Some(v);
                    }
                }
            }
            _ => {
                return Err(format!(
                    "skill_overrides entry (skill `{}`, stat `{}`) must have exactly one of value / per_level",
                    entry.skill, entry.stat
                ));
            }
        }
    }
    Ok(())
}

/// Merges the overlay's statSet-level override value
/// (skill_attack_speed_more) into the `granted_effect_stat_sets` domain.
/// See the module doc's rule 5 for the semantics.
///
/// Under the T5.2 multi-set model, this writes to the **primary set**
/// (`sets[0]`) — equivalent to "the first one wins" from the single-set
/// era (defaults to the primary set on consumption; precise per-set
/// attribution is deferred until an overlay entry carries a set id).
pub fn apply_stat_set_overrides(
    sets: &mut Vec<SkillStatSetDef>,
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    let mut appended = false;
    for entry in &overrides.overrides {
        if entry.stat != OVERRIDE_STAT_SKILL_ATTACK_SPEED_MORE {
            continue;
        }
        let Some(value) = entry.value else {
            return Err(format!(
                "skill_overrides entry (skill `{}`, stat `{}`) is missing value (statSet-level override values are always single-valued)",
                entry.skill, entry.stat
            ));
        };
        match sets
            .iter_mut()
            .find(|s| s.effect_id == entry.skill)
            .and_then(|def| def.sets.first_mut())
        {
            // The first one wins (when several overlay entries share a
            // skill, take the first one by `stat_set` ascending order).
            Some(main_set) => {
                if main_set.skill_attack_speed_more.is_none() {
                    main_set.skill_attack_speed_more = Some(value);
                }
            }
            // base has no stat-set entry for this effect → append a
            // minimal entry (synthesizing a primary set), so the value
            // isn't lost.
            None => {
                sets.push(SkillStatSetDef {
                    effect_id: entry.skill.clone(),
                    sets: vec![StatSetDef {
                        set_id: entry.skill.clone(),
                        label: None,
                        vendor_set_index: None,
                        base_effectiveness: 0.0,
                        constant_stats: Vec::new(),
                        skill_attack_speed_more: Some(value),
                        dot_flags: Default::default(),
                        explode_corpse: false,
                        implicit_stats: Vec::new(),
                        levels: Vec::new(),
                    }],
                });
                appended = true;
            }
        }
    }
    // Restore sorting by effect id after appending (matches the base
    // domain's sort-order contract, for deterministic consumption).
    if appended {
        sets.sort_by(|a, b| a.effect_id.cmp(&b.effect_id));
    }
    Ok(())
}

/// Merges the overlay's statSet-level **dotIs\* booleans** (`dot_is_area`
/// etc.) into the `granted_effect_stat_sets` domain, and marks them
/// `verified`.
///
/// Must be called **after** the stat_set_labels merge (locating the set
/// depends on [`StatSetDef::vendor_set_index`] — an overlay entry's
/// `stat_set` is vendor's 1-based `statSets` index, sharing its source
/// with the label sidecar's `set_index`; e.g. TornadoShotPlayer's
/// `dotIsArea` hangs off vendor statSets\[2\] "Tornado" = the `.dat`
/// side's `TornadoShotNovaPlayer` set).
///
/// Merge semantics:
/// 1. Locating the set: `stat_set = Some(i)` → the set matching
///    `vendor_set_index == i`; `None` → the primary set (`sets[0]`).
/// 2. No match (base has no such effect / no set with that vendor index)
///    → **skipped** — the conservative default (all-false, no flag
///    stripped) is exactly the intended fallback; no empty set is synthesized.
/// 3. On a match, the corresponding boolean is written (value ≠ 0 = true)
///    and `verified = true` is set (the parity report lists unverified
///    skills separately based on this).
/// 4. An unknown dot-stat name never reaches here (list-driven: only
///    entries in [`OVERRIDE_DOT_FLAG_STATS`] are consumed; everything else
///    is caught by the level-domain merge's rule 4).
pub fn apply_dot_flag_overrides(
    sets: &mut [SkillStatSetDef],
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    for entry in &overrides.overrides {
        let is_dot_flag = OVERRIDE_DOT_FLAG_STATS.contains(&entry.stat.as_str());
        // explode_corpse shares a channel with dotIs* (statSet baseMods
        // booleans, the same set-lookup semantics), only the target field differs.
        if !is_dot_flag && entry.stat != OVERRIDE_STAT_EXPLODE_CORPSE {
            continue;
        }
        let Some(value) = entry.value else {
            return Err(format!(
                "skill_overrides entry (skill `{}`, stat `{}`) is missing value (statSet booleans are always single-valued)",
                entry.skill, entry.stat
            ));
        };
        let Some(def) = sets.iter_mut().find(|s| s.effect_id == entry.skill) else {
            continue; // Rule 2: a vendor-only skill, conservative default.
        };
        let target = match entry.stat_set {
            Some(idx) => def
                .sets
                .iter_mut()
                .find(|s| s.vendor_set_index == Some(idx)),
            None => def.sets.first_mut(),
        };
        let Some(set) = target else {
            continue; // Rule 2: no set with this vendor index (curated out
            // by the template), conservative default.
        };
        let flag = value != 0.0;
        match entry.stat.as_str() {
            OVERRIDE_STAT_DOT_IS_AREA => set.dot_flags.area = flag,
            OVERRIDE_STAT_DOT_IS_PROJECTILE => set.dot_flags.projectile = flag,
            OVERRIDE_STAT_DOT_IS_SPELL => set.dot_flags.spell = flag,
            OVERRIDE_STAT_DOT_IS_ATTACK => set.dot_flags.attack = flag,
            OVERRIDE_STAT_DOT_IS_HIT => set.dot_flags.hit = flag,
            OVERRIDE_STAT_EXPLODE_CORPSE => {
                set.explode_corpse = flag;
                continue; // Doesn't touch dot_flags.verified (the dot
                // verification marker has independent semantics).
            }
            _ => unreachable!("statSet boolean list already filtered"),
        }
        set.dot_flags.verified = true;
    }
    Ok(())
}

/// Merges the overlay's statSet-level **implicit stats** (`implicit_stat`
/// entries) into the `granted_effect_stat_sets` domain.
///
/// Shares the same set-lookup semantics as [`apply_dot_flag_overrides`]
/// (vendor index first, `None` → primary set; no match is skipped — the
/// conservative default means that stat isn't injected, an under-count
/// that's safe). Deduplicated + sorted by stat id within a set (for
/// deterministic consumption). A missing `stat_id` is an error, not silenced.
pub fn apply_implicit_stat_overrides(
    sets: &mut [SkillStatSetDef],
    overrides: &SkillOverridesDef,
) -> Result<(), String> {
    for entry in &overrides.overrides {
        if entry.stat != OVERRIDE_STAT_IMPLICIT_STAT {
            continue;
        }
        let Some(stat_id) = &entry.stat_id else {
            return Err(format!(
                "skill_overrides entry (skill `{}`, stat `{}`) is missing stat_id",
                entry.skill, entry.stat
            ));
        };
        let Some(def) = sets.iter_mut().find(|s| s.effect_id == entry.skill) else {
            continue; // A vendor-only skill, conservative default.
        };
        let target = match entry.stat_set {
            Some(idx) => def
                .sets
                .iter_mut()
                .find(|s| s.vendor_set_index == Some(idx)),
            None => def.sets.first_mut(),
        };
        let Some(set) = target else {
            continue; // No set with this vendor index (curated out by the
            // template), conservative default.
        };
        if !set.implicit_stats.iter().any(|s| s == stat_id) {
            set.implicit_stats.push(stat_id.clone());
            set.implicit_stats.sort();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pobr_data::catalog::skill_overrides::{SkillOverrideEntry, SkillOverridesDef};
    use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef, StatSetDef};

    use super::{apply_level_overrides, apply_stat_set_overrides};

    /// Helper: builds a single override entry.
    fn entry(
        skill: &str,
        stat: &str,
        value: Option<f64>,
        per_level: Option<Vec<(u32, f64)>>,
    ) -> SkillOverrideEntry {
        SkillOverrideEntry {
            skill: skill.into(),
            stat: stat.into(),
            stat_set: None,
            value,
            per_level,
            stat_id: None,
        }
    }

    /// Two bare level rows (level 1/2, everything else empty).
    fn bare_rows() -> Vec<SkillLevelDef> {
        [1u32, 2]
            .into_iter()
            .map(|level| SkillLevelDef {
                level,
                cooldown_ms: None,
                attack_time_ms: None,
                cost_amounts: Vec::new(),
                attack_speed_multiplier: None,
                base_multiplier: None,
                crit_chance: None,
                mana_multiplier: None,
                spirit_reservation_flat: None,
                reservation_multiplier: None,
                stored_uses: None,
                level_requirement: None,
            })
            .collect()
    }

    fn doc(overrides: Vec<SkillOverrideEntry>) -> SkillOverridesDef {
        SkillOverridesDef { overrides }
    }

    /// A bare statSet (every field empty, with the vendor export index specified as needed).
    fn bare_set(set_id: &str, vendor_set_index: Option<u32>) -> StatSetDef {
        StatSetDef {
            set_id: set_id.into(),
            label: None,
            vendor_set_index,
            base_effectiveness: 0.0,
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            dot_flags: Default::default(),
            explode_corpse: false,
            implicit_stats: Vec::new(),
            levels: Vec::new(),
        }
    }

    /// Rule 1a: a single value applies to every level row; rule 3: an
    /// entry for a skill absent from base is skipped.
    #[test]
    fn constant_value_applies_to_all_levels_and_unknown_skill_is_skipped() {
        let mut levels = BTreeMap::from([("Arc".to_string(), bare_rows())]);
        let ov = doc(vec![
            entry("Arc", "crit_chance", Some(9.0), None),
            entry("VendorOnly", "crit_chance", Some(5.0), None),
        ]);
        apply_level_overrides(&mut levels, &ov).unwrap();
        assert!(levels["Arc"].iter().all(|r| r.crit_chance == Some(9.0)));
        assert!(!levels.contains_key("VendorOnly"));
    }

    /// Rule 1b: a per_level breakdown only touches the listed levels;
    /// missing levels stay None.
    #[test]
    fn per_level_detail_only_touches_listed_levels() {
        let mut levels = BTreeMap::from([("Snipe".to_string(), bare_rows())]);
        let ov = doc(vec![entry(
            "Snipe",
            "base_multiplier",
            None,
            Some(vec![(1, 2.65)]),
        )]);
        apply_level_overrides(&mut levels, &ov).unwrap();
        assert_eq!(levels["Snipe"][0].base_multiplier, Some(2.65));
        assert_eq!(
            levels["Snipe"][1].base_multiplier, None,
            "L2 must not be filled in"
        );
    }

    /// Rule 2: trivial-value normalization — asm 0 / base_multiplier ≈1.0
    /// are skipped; crit 0 is written as-is.
    #[test]
    fn trivial_values_are_normalized_like_adapter() {
        let mut levels = BTreeMap::from([("S".to_string(), bare_rows())]);
        let ov = doc(vec![
            entry("S", "attack_speed_multiplier", Some(0.0), None),
            entry("S", "base_multiplier", Some(1.0), None),
            entry("S", "crit_chance", Some(0.0), None),
        ]);
        apply_level_overrides(&mut levels, &ov).unwrap();
        assert_eq!(levels["S"][0].attack_speed_multiplier, None);
        assert_eq!(levels["S"][0].base_multiplier, None);
        assert_eq!(
            levels["S"][0].crit_chance,
            Some(0.0),
            "crit 0 is distinct from missing"
        );
    }

    /// Rule 4: an unknown stat name errors out, not silenced; violating
    /// the value/per_level either-or also errors out.
    #[test]
    fn unknown_stat_and_malformed_entry_error_out() {
        let mut levels = BTreeMap::from([("S".to_string(), bare_rows())]);
        let ov = doc(vec![entry("S", "made_up_stat", Some(1.0), None)]);
        assert!(apply_level_overrides(&mut levels, &ov).is_err());

        let ov = doc(vec![entry("S", "crit_chance", None, None)]);
        assert!(apply_level_overrides(&mut levels, &ov).is_err());
    }

    /// Rule 5: skill_attack_speed_more writes to an existing effect's
    /// **primary set** (the first one wins); when base has no entry, a
    /// minimal entry (synthesizing a primary set) is appended and sort
    /// order is preserved.
    #[test]
    fn stat_set_speed_more_merges_or_appends() {
        let mut sets = vec![SkillStatSetDef {
            effect_id: "Flicker".into(),
            sets: vec![bare_set("Flicker", None)],
        }];
        let mut first = entry("Flicker", "skill_attack_speed_more", Some(285.0), None);
        first.stat_set = Some(1);
        let mut second = entry("Flicker", "skill_attack_speed_more", Some(999.0), None);
        second.stat_set = Some(2);
        let appended = entry("Aardvark", "skill_attack_speed_more", Some(50.0), None);
        let ov = doc(vec![first, second, appended]);
        apply_stat_set_overrides(&mut sets, &ov).unwrap();

        assert_eq!(sets.len(), 2);
        assert_eq!(
            sets[0].effect_id, "Aardvark",
            "sorted by effect id after appending"
        );
        assert_eq!(sets[0].sets[0].skill_attack_speed_more, Some(50.0));
        assert!(sets[0].sets[0].levels.is_empty());
        assert_eq!(
            sets[1].sets[0].skill_attack_speed_more,
            Some(285.0),
            "with multiple entries for the same skill, the first one wins (written to the primary set)"
        );
    }

    /// dotIs* merge rules 1/3: locates the set by vendor index (a
    /// non-primary set can match), writes the boolean and sets the
    /// verified marker; the level-domain merge skips dot stats without erroring.
    #[test]
    fn dot_flags_merge_targets_set_by_vendor_index() {
        use super::apply_dot_flag_overrides;
        let mut sets = vec![SkillStatSetDef {
            effect_id: "TornadoShotPlayer".into(),
            sets: vec![
                bare_set("TornadoShotPlayer", Some(1)),
                bare_set("TornadoShotNovaPlayer", Some(2)),
            ],
        }];
        let mut e = entry("TornadoShotPlayer", "dot_is_area", Some(1.0), None);
        e.stat_set = Some(2);
        let ov = doc(vec![e]);

        // Level domain: a dot stat is statSet-level, must not be
        // misreported by rule 4.
        let mut levels = BTreeMap::from([("TornadoShotPlayer".to_string(), bare_rows())]);
        apply_level_overrides(&mut levels, &ov).unwrap();

        apply_dot_flag_overrides(&mut sets, &ov).unwrap();
        let main = &sets[0].sets[0];
        assert!(
            !main.dot_flags.area,
            "the primary set must not be miswritten"
        );
        assert!(!main.dot_flags.verified);
        let nova = &sets[0].sets[1];
        assert!(
            nova.dot_flags.area,
            "the set with vendor index 2 should match"
        );
        assert!(nova.dot_flags.verified, "a match implies verified");
        assert!(
            !nova.dot_flags.spell,
            "unlisted bits stay conservatively false"
        );
    }

    /// dotIs* merge rule 2: a vendor-only skill / unmatched index →
    /// skipped (conservative default); a missing value errors out, not silenced.
    #[test]
    fn dot_flags_merge_skips_unmatched_and_rejects_missing_value() {
        use super::apply_dot_flag_overrides;
        let mut sets = vec![SkillStatSetDef {
            effect_id: "Known".into(),
            sets: vec![bare_set("Known", Some(1))],
        }];
        let mut miss_skill = entry("VendorOnly", "dot_is_spell", Some(1.0), None);
        miss_skill.stat_set = Some(1);
        let mut miss_index = entry("Known", "dot_is_hit", Some(1.0), None);
        miss_index.stat_set = Some(9);
        apply_dot_flag_overrides(&mut sets, &doc(vec![miss_skill, miss_index])).unwrap();
        assert!(
            sets[0].sets[0].dot_flags.is_default(),
            "no match must not write anything"
        );

        let bad = entry("Known", "dot_is_hit", None, None);
        assert!(apply_dot_flag_overrides(&mut sets, &doc(vec![bad])).is_err());
    }

    /// implicit_stat merge: locates the set by vendor index, pushes with
    /// dedup and sorting; no match is skipped; a missing stat_id errors
    /// out; the level-domain merge skips this stat without erroring.
    #[test]
    fn implicit_stat_merge_targets_set_and_dedupes() {
        use super::apply_implicit_stat_overrides;
        let mut sets = vec![SkillStatSetDef {
            effect_id: "SupportGarukhansResolvePlayer".into(),
            sets: vec![bare_set("SupportGarukhansResolvePlayer", Some(1))],
        }];
        let mut e = entry("SupportGarukhansResolvePlayer", "implicit_stat", None, None);
        e.stat_set = Some(1);
        e.stat_id = Some("attacks_roll_crits_twice".into());
        let dup = e.clone();
        let mut miss = entry("VendorOnly", "implicit_stat", None, None);
        miss.stat_id = Some("whatever".into());
        let ov = doc(vec![e, dup, miss]);

        // Level domain: implicit_stat is statSet-level, must not be
        // misreported by rule 4.
        let mut levels =
            BTreeMap::from([("SupportGarukhansResolvePlayer".to_string(), bare_rows())]);
        apply_level_overrides(&mut levels, &ov).unwrap();

        apply_implicit_stat_overrides(&mut sets, &ov).unwrap();
        assert_eq!(
            sets[0].sets[0].implicit_stats,
            vec!["attacks_roll_crits_twice".to_string()],
            "single entry after dedup"
        );

        let bad = entry("SupportGarukhansResolvePlayer", "implicit_stat", None, None);
        assert!(apply_implicit_stat_overrides(&mut sets, &doc(vec![bad])).is_err());
    }
}
