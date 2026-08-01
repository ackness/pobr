//! Per-level parameter table adapter (`GrantedEffectsPerLevel` -> `granted_effect_levels.json`).
//!
//! `GrantedEffectsPerLevel.GrantedEffect`'s integer `_index` -> `GrantedEffects.Id`
//! (the lookup is produced by [`super::effects`]); already wired up: per-level
//! cost / cooldown / attack time, plus reading the cost multiplier / Spirit
//! reservation family / stored uses / attack-speed multiplier / crit chance columns directly.
//!
//! Column value conversion (cross-referenced against vendor
//! `Export/Scripts/skills.lua:226-295`; see `pipeline/README.md`'s
//! cross-reference table for where the community schema's column names diverge from vendor spec):
//!
//! | Output field | Community column | Conversion |
//! |---|---|---|
//! | `mana_multiplier` | `CostMultiplier` | `- 100` (== 100 -> None) |
//! | `spirit_reservation_flat` | `Reservation` (= vendor `SpiritReservation`) | raw value (== 0 -> None) |
//! | `reservation_multiplier` | `EffectOnPlayer` (= vendor `ReservationMultiplier`) | `- 100` (== 100 -> None) |
//! | `stored_uses` | `StoredUses` | raw value (== 0 -> None) |
//! | `attack_speed_multiplier` | `AttackSpeedMultiplier` | raw value (== 0 -> None) |
//! | `crit_chance` | the two crit columns on the stat-set table (see [`super::stat_sets::crit_from_statset_levels`]) | `/100`, overridden by Offhand |
//! | `level_requirement` | **no `.dat` column** (the real source `ItemExperiencePerLevel` isn't downloadable) | always None, filled in via extract-lua |

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::SkillLevelDef;
use serde::Deserialize;

use crate::read_json;

#[derive(Deserialize)]
struct RawGrantedEffectPerLevel {
    /// `GrantedEffects`'s `_index` (a 0-based foreign key).
    #[serde(rename = "GrantedEffect")]
    granted_effect: Option<usize>,
    #[serde(rename = "Level")]
    level: Option<i64>,
    #[serde(rename = "Cooldown")]
    cooldown: Option<i64>,
    #[serde(rename = "AttackTime")]
    attack_time: Option<i64>,
    #[serde(rename = "CostAmounts", default)]
    cost_amounts: Vec<i64>,
    /// Attack speed multiplier (percentage points, can be negative; vendor
    /// `attackSpeedMultiplier`, e.g. Flicker's -50). Read directly from the
    /// table (T4.3 verified this against all 3578 historical overlay values, byte-for-byte).
    #[serde(rename = "AttackSpeedMultiplier")]
    attack_speed_multiplier: Option<f64>,
    /// Base damage multiplier (PoB's `baseMultiplier`, the fallback source
    /// when stat-set's BaseMultiplier is missing). Note: this column isn't
    /// in the community `GrantedEffectsPerLevel` table (always missing ->
    /// None); per-level values still come from the
    /// `overlay/skill_overrides.json` merge (the real source is the
    /// stat-set table's `BaseMultiplier`, whose direct table read belongs to T5's multi-statSet overhaul).
    #[serde(rename = "BaseMultiplier")]
    base_multiplier: Option<f64>,
    /// Raw cost multiplier value (vendor `CostMultiplier`, 100 = no multiplier).
    #[serde(rename = "CostMultiplier")]
    cost_multiplier: Option<f64>,
    /// Flat Spirit reservation amount (community column name `Reservation` = vendor `SpiritReservation`).
    #[serde(rename = "Reservation")]
    spirit_reservation: Option<f64>,
    /// Raw reservation multiplier value (community column name
    /// `EffectOnPlayer` = vendor `ReservationMultiplier`, 100 = no multiplier).
    #[serde(rename = "EffectOnPlayer")]
    reservation_multiplier: Option<f64>,
    /// Number of storable uses (vendor `StoredUses`, 0 = no storage).
    #[serde(rename = "StoredUses")]
    stored_uses: Option<i64>,
}

/// Adapts `GrantedEffectsPerLevel` into `granted_effect_id -> ascending level
/// array` (returns `(lookup, raw row total)`).
///
/// `crit_by_effect`: a crit-chance lookup read directly from the stat-set
/// table (effect id -> gem level -> crit percentage points), produced by
/// [`super::stat_sets::crit_from_statset_levels`] — crit chance lives at the
/// stat-set level in `.dat`, and is joined against this table by (effect, level).
pub(super) fn adapt_levels(
    en: &Path,
    effect_id_by_index: &[String],
    crit_by_effect: &BTreeMap<String, BTreeMap<u32, f64>>,
) -> Result<(BTreeMap<String, Vec<SkillLevelDef>>, usize), String> {
    let raw_levels =
        read_json::<Vec<RawGrantedEffectPerLevel>>(&en.join("GrantedEffectsPerLevel.json"))?;
    let level_rows_total = raw_levels.len();
    let mut levels: BTreeMap<String, Vec<SkillLevelDef>> = BTreeMap::new();
    for raw in raw_levels {
        let Some(level) = raw.level.filter(|&l| l > 0).map(|l| l as u32) else {
            continue; // Level 0 / missing -> a placeholder row, skip
        };
        let Some(idx) = raw.granted_effect else {
            continue;
        };
        let Some(id) = effect_id_by_index
            .get(idx)
            .filter(|s| !s.is_empty())
            .cloned()
        else {
            continue;
        };
        // Trivial-value normalization matches vendor's export guard
        // (`Export/Scripts/skills.lua`): vendor omits the field (-> None)
        // when CostMultiplier == 100 / Reservation == 0 / EffectOnPlayer ==
        // 100 / StoredUses == 0 / AttackSpeedMultiplier == 0.
        let crit_chance = crit_by_effect.get(&id).and_then(|m| m.get(&level)).copied();
        levels.entry(id).or_default().push(SkillLevelDef {
            level,
            cooldown_ms: raw.cooldown.filter(|&c| c > 0).map(|c| c as u32),
            attack_time_ms: raw.attack_time.filter(|&t| t > 0).map(|t| t as u32),
            cost_amounts: raw
                .cost_amounts
                .into_iter()
                .map(|c| c.max(0) as u32)
                .collect(),
            attack_speed_multiplier: raw.attack_speed_multiplier.filter(|&m| m != 0.0),
            base_multiplier: raw.base_multiplier.filter(|&m| (m - 1.0).abs() > 1e-9),
            crit_chance,
            mana_multiplier: raw
                .cost_multiplier
                .filter(|&c| c != 100.0)
                .map(|c| c - 100.0),
            spirit_reservation_flat: raw.spirit_reservation.filter(|&v| v != 0.0),
            reservation_multiplier: raw
                .reservation_multiplier
                .filter(|&v| v != 100.0)
                .map(|v| v - 100.0),
            stored_uses: raw.stored_uses.filter(|&v| v != 0).map(|v| v.max(0) as u32),
            // PoE2's `.dat` has no PlayerLevelReq column (verified during W0); filled in via extract-lua.
            level_requirement: None,
        });
    }
    // Each effect's level array is sorted ascending by level (diff-friendly + deterministic lookups).
    for rows in levels.values_mut() {
        rows.sort_by_key(|r| r.level);
    }
    Ok((levels, level_rows_total))
}
