//! Per-level damage stat set adapter (`GrantedEffectStatSets` +
//! `GrantedEffectStatSetsPerLevel` -> `granted_effect_stat_sets.json`).

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{SkillDamageStat, SkillStatSetDef, SkillStatSetLevel, StatSetDef};
use serde::Deserialize;

use crate::read_json;

#[derive(Deserialize)]
struct RawStatId {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct RawGrantedEffectStatSetLink {
    #[serde(rename = "Id")]
    id: String,
    /// `GrantedEffectStatSets` row index (negative/missing -> no stat set).
    #[serde(rename = "StatSet")]
    stat_set: Option<i64>,
    /// **Additional** statSet row indices (multiple sets, FK -> `GrantedEffectStatSets`, column order preserved).
    #[serde(rename = "AdditionalStatSets", default)]
    additional_stat_sets: Vec<i64>,
}

#[derive(Deserialize)]
struct RawStatSet {
    /// The stable statSet id (e.g. `IceNovaColdInfusedPlayer`).
    #[serde(rename = "Id", default)]
    id: String,
    #[serde(rename = "BaseEffectiveness")]
    base_effectiveness: Option<f64>,
    /// Level-independent constant stats (`Stats` row indices) and their
    /// values (positionally paired; e.g. a support's `damage_+%_final`).
    #[serde(rename = "ConstantStats", default)]
    constant_stats: Vec<usize>,
    #[serde(rename = "ConstantStatsValues", default)]
    constant_stats_values: Vec<i64>,
    /// (Backlog #7-2) Stats this set should **remove** (`Stats` row
    /// indices) — the community schema's column name `IgnoredStats` =
    /// vendor `Export/spec.lua`'s `RemoveStats`. Vendor's export
    /// (`Export/Scripts/skills.lua:572-597`) zeroes these stats' values in
    /// merged rows (only the **first occurrence**, i.e. the main set's copy
    /// spliced in by base-merge). Typical example: Essence Drain's DoT set
    /// removes the main set's `spell_min/max_base_chaos_damage` (pure DoT
    /// has no hit damage). A missing column (from an old table download) -> empty = nothing removed (a resilience degradation, matching existing missing-column handling).
    #[serde(rename = "IgnoredStats", default)]
    ignored_stats: Vec<usize>,
}

#[derive(Deserialize)]
struct RawStatSetPerLevel {
    /// `GrantedEffectStatSets` row index.
    #[serde(rename = "StatSet")]
    stat_set: Option<usize>,
    #[serde(rename = "GemLevel")]
    gem_level: Option<i64>,
    /// Per-level float stats (`Stats` row indices) and their resolved values (positionally paired).
    #[serde(rename = "FloatStats", default)]
    float_stats: Vec<usize>,
    #[serde(rename = "BaseResolvedValues", default)]
    base_resolved_values: Vec<i64>,
    /// Additional stats (`Stats` row indices) and their values (positionally paired).
    #[serde(rename = "AdditionalStats", default)]
    additional_stats: Vec<usize>,
    #[serde(rename = "AdditionalStatsValues", default)]
    additional_stats_values: Vec<i64>,
    /// Raw skill damage multiplier value (permyriad); multiplier = `1 + BaseMultiplier/10000`.
    #[serde(rename = "BaseMultiplier")]
    base_multiplier: Option<i64>,
}

/// The adapted per-level stat set output.
pub struct StatSetsBundle {
    /// Granted effects with at least one stat (main set + additional sets, T5.2), sorted by effect id.
    pub sets: Vec<SkillStatSetDef>,
    /// Total `GrantedEffectStatSets` row count (for reporting).
    pub sets_total: usize,
    /// Total per-level rows stored (including additional sets).
    pub damage_levels_total: usize,
}

/// Reads the base skill crit chance directly from
/// `GrantedEffectStatSetsPerLevel`: `effect id -> gem level -> crit chance
/// (percentage points)`, feeding [`super::levels::adapt_levels`]'s
/// `SkillLevelDef::crit_chance` (replacing the `overlay/skill_overrides.json` crit-merge source).
///
/// Column-name mismatch note (community schema vs vendor `Export/spec.lua`;
/// see `pipeline/README.md` for the cross-reference table):
/// - Community `SpellCritChance` = vendor `AttackCritChance` (the primary column, `/100`);
/// - Community `AttackCritChance` = vendor `OffhandCritChance` (**overrides**
///   the primary column when nonzero, vendor `Export/Scripts/skills.lua:281-286`).
///
/// statSet attribution (matching vendor's `GrantedEffect`-joined behavior):
/// the main `StatSet` takes priority; if the main set has no crit chance
/// anywhere but an **additional** set (`AdditionalStatSets`, FK ->
/// `GrantedEffectStatSets`) does, take the first additional set that has
/// one (e.g. GalvanicFieldBuffPlayer's main set 164 has no crit chance,
/// while additional set 900 has 9.0 — verified during W0 against 201/201
/// matches with vendor's rule).
///
/// Function-boundary note: T4 only adds this function within this file (a
/// T5-owned file) without touching existing logic; T5's multi-statSet
/// overhaul should align the call sites accordingly.
pub(super) fn crit_from_statset_levels(
    en: &Path,
) -> Result<BTreeMap<String, BTreeMap<u32, f64>>, String> {
    /// A minimal row read for just the two crit columns (kept separate from
    /// [`RawStatSetPerLevel`]: this function only cares about crit columns).
    #[derive(Deserialize)]
    struct RawCritRow {
        #[serde(rename = "StatSet")]
        stat_set: Option<usize>,
        #[serde(rename = "GemLevel")]
        gem_level: Option<i64>,
        /// Community column name (= vendor's primary `AttackCritChance` column), in 1/100 percentage points (e.g. 900 = 9%).
        #[serde(rename = "SpellCritChance")]
        spell_crit_chance: Option<f64>,
        /// Community column name (= vendor's `OffhandCritChance` override column).
        #[serde(rename = "AttackCritChance")]
        attack_crit_chance: Option<f64>,
    }
    /// The three statSet-attribution columns on a `GrantedEffects` row.
    #[derive(Deserialize)]
    struct RawEffectSetLink {
        #[serde(rename = "Id")]
        id: String,
        #[serde(rename = "StatSet")]
        stat_set: Option<i64>,
        #[serde(rename = "AdditionalStatSets", default)]
        additional_stat_sets: Vec<i64>,
    }

    let rows = read_json::<Vec<RawCritRow>>(&en.join("GrantedEffectStatSetsPerLevel.json"))?;
    // set row index -> {gem level -> crit chance} (a later write at the same
    // level overwrites within file order; Offhand overrides the primary column within a row).
    let mut crit_by_set: BTreeMap<usize, BTreeMap<u32, f64>> = BTreeMap::new();
    for row in &rows {
        let (Some(si), Some(level)) = (row.stat_set, row.gem_level.filter(|&l| l > 0)) else {
            continue;
        };
        let spell = row.spell_crit_chance.unwrap_or(0.0);
        let offhand = row.attack_crit_chance.unwrap_or(0.0);
        let value = if offhand != 0.0 {
            Some(offhand / 100.0)
        } else if spell != 0.0 {
            Some(spell / 100.0)
        } else {
            None
        };
        if let Some(v) = value {
            crit_by_set.entry(si).or_default().insert(level as u32, v);
        }
    }

    let links = read_json::<Vec<RawEffectSetLink>>(&en.join("GrantedEffects.json"))?;
    let mut out: BTreeMap<String, BTreeMap<u32, f64>> = BTreeMap::new();
    for link in links {
        if link.id.is_empty() {
            continue;
        }
        // Candidate sets: the main StatSet first, then AdditionalStatSets in column order; take the first with a crit value.
        let candidates = link
            .stat_set
            .into_iter()
            .chain(link.additional_stat_sets)
            .filter(|&i| i >= 0)
            .map(|i| i as usize);
        for si in candidates {
            if let Some(map) = crit_by_set.get(&si).filter(|m| !m.is_empty()) {
                out.insert(link.id, map.clone());
                break;
            }
        }
    }
    Ok(out)
}

/// Adapts `GrantedEffectStatSets` + `GrantedEffectStatSetsPerLevel` (plus the
/// `Stats` / `GrantedEffects` foreign keys) into the "effect id -> multiple
/// statSets, each with per-level stats" domain.
///
/// Resolution approach (cross-referenced against PoB2
/// `Export/Scripts/skills.lua`'s statSets handling):
/// - Main set: `GrantedEffects.StatSet` row index -> a `GrantedEffectStatSets` row;
/// - Additional sets: `GrantedEffects.AdditionalStatSets` in column order
///   (verified during W0 that the FK target is `GrantedEffectStatSets` — an
///   earlier belief that it "points to another GrantedEffects row" was wrong and has been corrected);
/// - Per row, `FloatStats[i]` <-> `BaseResolvedValues[i]` and
///   `AdditionalStats[i]` <-> `AdditionalStatsValues[i]` are positionally
///   paired, and stat row indices resolve to stable ids via `Stats`;
/// - Additional sets are **merged and stored together** with the main set,
///   following vendor's export semantics (`skills.lua:498-553`): constant
///   stats are concatenated (main ++ this set), per-level rows are
///   concatenated by array-position pairing with the main set's rows,
///   `BaseEffectiveness` falls back to the main set when it's the default 1,
///   and `BaseMultiplier != 0` takes this row's value, otherwise falls back
///   to the paired main-set row — the consumption side just picks a set and uses it, no runtime merge needed.
///
/// **Full stat storage**: the data layer no longer applies any stat
/// whitelist (the adapter-side whitelist suffix predicate has been removed)
/// — the statmap data engine (`pobr-core::rules::stat_map_engine`, T2) needs
/// to see every stat to exhaustively cross-reference them.
/// **Migration-invariant guarantee**: the same predicate has been moved to
/// the consumption-side legacy path (`pobr-build::legacy_stat_filter`,
/// filtering at the Legacy channel's entry point in
/// `mapped_stat_modifiers`), keeping ninja parity byte-for-byte unchanged;
/// that consumption-side filter will be removed alongside legacy in T2.4.
///
/// Verification baseline: FireballPlayer L1 -> `spell_minimum/maximum_base_fire_damage` = 8 / 12
/// (matches PoB's own parsed `Data/Skills/act_int.lua` byte-for-byte; the
/// main set's content matches the single-set era value-for-value — the
/// consumption side defaults to the main set, which is the migration invariant).
pub fn adapt_stat_sets(en: &Path) -> Result<StatSetsBundle, String> {
    // stat row index -> stable id (positioned by `_index`, out-of-range entries stay an empty string).
    let raw_stats = read_json::<Vec<RawStatId>>(&en.join("Stats.json"))?;
    let max_stat = raw_stats.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut stat_id = vec![String::new(); max_stat];
    for r in &raw_stats {
        stat_id[r.index] = r.id.clone();
    }

    let sets = read_json::<Vec<RawStatSet>>(&en.join("GrantedEffectStatSets.json"))?;
    let sets_total = sets.len();
    let links = read_json::<Vec<RawGrantedEffectStatSetLink>>(&en.join("GrantedEffects.json"))?;

    // Per-level rows are grouped by StatSet row index (file order = vendor's GetRowList order, the basis for positional pairing).
    let per_level =
        read_json::<Vec<RawStatSetPerLevel>>(&en.join("GrantedEffectStatSetsPerLevel.json"))?;
    let mut rows_by_set: BTreeMap<usize, Vec<&RawStatSetPerLevel>> = BTreeMap::new();
    for row in &per_level {
        if let Some(si) = row.stat_set {
            rows_by_set.entry(si).or_default().push(row);
        }
    }

    // Positional-pairing resolution of a single row's stats (stored in full, no filtering).
    let resolve_row_stats = |row: &RawStatSetPerLevel| -> Vec<SkillDamageStat> {
        row.float_stats
            .iter()
            .zip(row.base_resolved_values.iter())
            .chain(
                row.additional_stats
                    .iter()
                    .zip(row.additional_stats_values.iter()),
            )
            .filter_map(|(&stat_idx, &value)| {
                stat_id
                    .get(stat_idx)
                    .filter(|s| !s.is_empty())
                    .map(|sid| SkillDamageStat {
                        stat: sid.clone(),
                        value: value as f64,
                    })
            })
            .collect()
    };
    // Positional-pairing resolution of constant stats.
    let resolve_constants = |set: &RawStatSet| -> Vec<SkillDamageStat> {
        set.constant_stats
            .iter()
            .zip(set.constant_stats_values.iter())
            .filter_map(|(&stat_idx, &value)| {
                stat_id
                    .get(stat_idx)
                    .filter(|s| !s.is_empty())
                    .map(|sid| SkillDamageStat {
                        stat: sid.clone(),
                        value: value as f64,
                    })
            })
            .collect()
    };
    let raw_multiplier = |row: &RawStatSetPerLevel| {
        1.0 + f64::from(row.base_multiplier.unwrap_or(0) as i32) / 10000.0
    };
    // (Backlog #7-2) RemoveStats/IgnoredStats zeroing (vendor
    // skills.lua:589-596): for every stat a set declares as removed, zero
    // out its **first occurrence** within a row (in a merged row, the
    // earlier occurrence is the main set's copy spliced in by base-merge);
    // any other occurrence is left alone.
    // ponytail: when the same stat appears more than once in IgnoredStats,
    // vendor zeroes multiple placeholders — current data never has this shape, so a repeated entry is handled as a single occurrence.
    let apply_ignored = |stats: &mut Vec<SkillDamageStat>, ignored: &[usize]| {
        for &idx in ignored {
            let Some(sid) = stat_id.get(idx).filter(|s| !s.is_empty()) else {
                continue;
            };
            if let Some(slot) = stats.iter_mut().find(|s| &s.stat == sid) {
                slot.value = 0.0;
            }
        }
    };

    let mut out = Vec::new();
    let mut damage_levels_total = 0usize;
    for link in &links {
        if link.id.is_empty() {
            continue;
        }
        let Some(main_idx) = link.stat_set.filter(|&i| i >= 0).map(|i| i as usize) else {
            continue;
        };
        let Some(main_set) = sets.get(main_idx) else {
            continue;
        };

        let main_constants = resolve_constants(main_set);
        let main_rows = rows_by_set.get(&main_idx).map(Vec::as_slice).unwrap_or(&[]);

        // Main set: parsed as-is, the same as the single-set era (the migration-invariant anchor).
        let mut main_levels = Vec::new();
        for row in main_rows {
            let Some(gem_level) = row.gem_level.filter(|&l| l > 0).map(|l| l as u32) else {
                continue;
            };
            let mut stats = resolve_row_stats(row);
            apply_ignored(&mut stats, &main_set.ignored_stats);
            let damage_multiplier = raw_multiplier(row);
            // Include level rows that have a stat or a non-trivial multiplier.
            if !stats.is_empty() || (damage_multiplier - 1.0).abs() > f64::EPSILON {
                main_levels.push(SkillStatSetLevel {
                    gem_level,
                    damage_multiplier,
                    stats,
                });
            }
        }
        main_levels.sort_by_key(|l| l.gem_level);

        let mut def_sets = vec![StatSetDef {
            set_id: main_set.id.clone(),
            // label / the vendor export index come from
            // overlay/stat_set_labels.json (vendor-extracted, since the
            // `.dat` Label column's FK target table isn't downloadable),
            // merged in during loading; left empty during adaptation.
            label: None,
            vendor_set_index: None,
            base_effectiveness: main_set.base_effectiveness.unwrap_or(0.0),
            constant_stats: main_constants.clone(),
            // statSet baseMods (e.g. Flicker's `Speed MORE 285`) aren't in
            // GGG's `.dat` tables — they're PoB2's own built-in constants,
            // merged in from overlay/skill_overrides.json during loading; left empty during adaptation.
            skill_attack_speed_more: None,
            // dotIs* are likewise vendor baseMods booleans with no `.dat`
            // column, merged in from the overlay during loading; kept at their conservative default (all false) during adaptation.
            dot_flags: Default::default(),
            explode_corpse: false,
            // Implicit stats come from the overlay (extract-lua's curated whitelist); left empty during adaptation.
            implicit_stats: Vec::new(),
            levels: main_levels,
        }];

        // Additional sets (vendor's base-merge semantics, skills.lua:498-553).
        for &add_raw in &link.additional_stat_sets {
            let Some(add_idx) = usize::try_from(add_raw).ok() else {
                continue;
            };
            let Some(add_set) = sets.get(add_idx) else {
                continue;
            };
            // Constants: main ++ this set (:502-504 tableConcat).
            let mut constant_stats = main_constants.clone();
            constant_stats.extend(resolve_constants(add_set));
            // Effectiveness: falls back to the main set when this set's raw value is the export default 1 (:506-508).
            let base_effectiveness = match add_set.base_effectiveness {
                Some(v) if v != 1.0 => v,
                _ => main_set.base_effectiveness.unwrap_or(0.0),
            };
            // Per-level: this set's rows are paired with the main set's rows
            // by array position (:521 `skill.baseStatRow[indx]`), stats =
            // paired main row ++ this row (:541-549 tableConcat).
            let add_rows = rows_by_set.get(&add_idx).map(Vec::as_slice).unwrap_or(&[]);
            let mut levels = Vec::new();
            for (indx, row) in add_rows.iter().enumerate() {
                let Some(gem_level) = row.gem_level.filter(|&l| l > 0).map(|l| l as u32) else {
                    continue;
                };
                let paired = main_rows.get(indx);
                let mut stats = paired.map(|p| resolve_row_stats(p)).unwrap_or_default();
                stats.extend(resolve_row_stats(row));
                // This set's RemoveStats acts on the **merged** row (vendor
                // tableConcats first, then zeroes): Essence Drain's DoT set
                // zeroes the 62/115 hit-damage segment spliced in from the main set.
                apply_ignored(&mut stats, &add_set.ignored_stats);
                // Multiplier: takes this row's value when its BaseMultiplier
                // != 0 (:533-541, both branches resolve to this row's
                // `/10000+1`; the UseSetAttackMulti column isn't downloaded), otherwise falls back to the paired main-set row.
                let damage_multiplier = if row.base_multiplier.unwrap_or(0) != 0 {
                    raw_multiplier(row)
                } else {
                    paired.map(|p| raw_multiplier(p)).unwrap_or(1.0)
                };
                if !stats.is_empty() || (damage_multiplier - 1.0).abs() > f64::EPSILON {
                    levels.push(SkillStatSetLevel {
                        gem_level,
                        damage_multiplier,
                        stats,
                    });
                }
            }
            levels.sort_by_key(|l| l.gem_level);
            def_sets.push(StatSetDef {
                set_id: add_set.id.clone(),
                label: None,
                vendor_set_index: None,
                base_effectiveness,
                constant_stats,
                skill_attack_speed_more: None,
                dot_flags: Default::default(),
                explode_corpse: false,
                // Implicit stats come from the overlay (extract-lua's curated whitelist); left empty during adaptation.
                implicit_stats: Vec::new(),
                levels,
            });
        }

        // All sets are empty (no constants, no level rows) -> skip this effect (matching the single-set era's behavior).
        if def_sets
            .iter()
            .all(|s| s.levels.is_empty() && s.constant_stats.is_empty())
        {
            continue;
        }
        damage_levels_total += def_sets.iter().map(|s| s.levels.len()).sum::<usize>();
        out.push(SkillStatSetDef {
            effect_id: link.id.clone(),
            sets: def_sets,
        });
    }
    out.sort_by(|a, b| a.effect_id.cmp(&b.effect_id));

    Ok(StatSetsBundle {
        sets: out,
        sets_total,
        damage_levels_total,
    })
}
