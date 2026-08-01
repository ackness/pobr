//! Monster per-level scaling table schema (`base/monster_scaling.json`).
//!
//! Corresponds to PoB2 `src/Data/Misc.lua`'s nine per-level monster tables
//! (100 entries each, indexed by monster level - 1; the ultimate source is
//! GGG's `DefaultMonsterStats.dat`, and the adapter will eventually be able
//! to regenerate an equivalent JSON directly from the `.dat`):
//!
//! | JSON field | PoB2 Lua table (Misc.lua line) | pobr source of truth (pre-migration) |
//! |---|---|---|
//! | `accuracy` | `data.monsterAccuracyTable` (L6) | `monster.rs::MONSTER_ACCURACY_TABLE` |
//! | `evasion` | `data.monsterEvasionTable` (L5) | `monster.rs::MONSTER_EVASION_TABLE` |
//! | `armour` | `data.monsterArmourTable` (L11) | `monster.rs::MONSTER_ARMOUR_TABLE` |
//! | `life` | `data.monsterLifeTable` (L7) | `monster.rs::MONSTER_LIFE_TABLE` |
//! | `ally_life` | `data.monsterAllyLifeTable` (L8) | none (vendor-only) |
//! | `damage` | `data.monsterDamageTable` (L9) | `monster.rs::MONSTER_DAMAGE_TABLE` |
//! | `ally_damage` | `data.monsterAllyDamageTable` (L10) | none (vendor-only) |
//! | `ailment_threshold` | `data.monsterAilmentThresholdTable` (L12) | `monster.rs::MONSTER_AILMENT_THRESHOLD_TABLE` |
//! | `poise_threshold` | `data.monsterPoiseThresholdTable` (L13) | `monster.rs::MONSTER_POISE_THRESHOLD_TABLE` |
//!
//! Value conventions:
//! - For the seven tables that have a pobr Rust source of truth, the JSON
//!   is value-equal to the Rust table (a migration invariant); `damage`
//!   keeps pobr's existing convention — vendor's noisy f32 values (e.g.
//!   `9.1599998474121`) are rounded to 2 decimal places (`9.16`), matching
//!   vendor value-for-value at that precision.
//! - `ally_life` / `ally_damage` are vendor-only fields (not previously
//!   migrated in pobr), extracted from
//!   `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L8 / L10; `ally_damage`
//!   is likewise rounded to 2 decimal places (matching `damage`'s
//!   convention, and also matching the `round(..., 2)` precision handling
//!   in PoB2 `CalcActiveSkill.lua:907`'s hiddenDamageFixup derivation).
//!
//! Consumers (PoB2-side usage; pobr's counterparts are `setup_env` / EHP /
//! minion assembly):
//! - `accuracy`/`evasion`/`armour`/`life`: the BASE values `CalcSetup.lua`
//!   injects into the enemy ModDB; `damage`: EHP calc's
//!   `monsterDamageTable[lv] * 1.5 * DPSMult`.
//! - `ailment_threshold`: chance-derived ailments (Ignite/Shock's chance,
//!   Chill's minimum threshold); `CalcOffence.lua`'s
//!   `enemyThreshold = table value × mod(EnemyAilmentThreshold)`.
//! - `poise_threshold`: accumulating debuffs (Freeze/Electrocute/HeavyStun/Pin);
//!   a boss tier's `PoiseThreshold MORE 500` is injected separately at the
//!   mod_db layer — this table's value is the raw value.
//! - `ally_life`/`ally_damage`: baseline life/damage for minions
//!   (non-hostile summons) (`CalcActiveSkill.lua:899-908`), and also feed
//!   the hiddenDamageFixup derivation:
//!   `hiddenDamageFixup = round(allyDamage[lv] / damageTable[lv] × SpectreBeastDamageFixup, 2) - 1`
//!   (`SpectreBeastDamageFixup = 1.25` is a misc constant, living in the
//!   `game_constants` domain).

use serde::{Deserialize, Serialize};

use crate::monster::{
    MONSTER_ACCURACY_TABLE, MONSTER_AILMENT_THRESHOLD_TABLE, MONSTER_ARMOUR_TABLE,
    MONSTER_DAMAGE_TABLE, MONSTER_EVASION_TABLE, MONSTER_LIFE_TABLE, MONSTER_POISE_THRESHOLD_TABLE,
};

/// Length of the monster per-level tables (level 1..=100; every array is
/// always 100 entries).
pub const MONSTER_SCALING_TABLE_LEN: usize = 100;

/// The monster per-level scaling table (nine parallel per-level arrays,
/// indexed by level - 1).
///
/// This parallel-array shape mirrors vendor `Misc.lua` /
/// `DefaultMonsterStats.dat`, so the adapter can regenerate it by copying
/// each table verbatim; every array's length is always
/// [`MONSTER_SCALING_TABLE_LEN`], enforced by a loader-side test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonsterScalingTable {
    /// Monster base accuracy (`data.monsterAccuracyTable`).
    pub accuracy: Vec<u32>,
    /// Monster base evasion (`data.monsterEvasionTable`).
    pub evasion: Vec<u32>,
    /// Monster base armour (`data.monsterArmourTable`).
    pub armour: Vec<u32>,
    /// Monster base life (`data.monsterLifeTable`).
    pub life: Vec<u32>,
    /// Ally (minion) base life (`data.monsterAllyLifeTable`, vendor-only).
    pub ally_life: Vec<u32>,
    /// Monster base damage (`data.monsterDamageTable`, 2-decimal-place convention).
    pub damage: Vec<f64>,
    /// Ally (minion) base damage (`data.monsterAllyDamageTable`,
    /// vendor-only, 2-decimal-place convention; feeds the hiddenDamageFixup
    /// derivation).
    pub ally_damage: Vec<f64>,
    /// Monster ailment threshold (`data.monsterAilmentThresholdTable`, for
    /// Ignite/Shock/Chill).
    pub ailment_threshold: Vec<u32>,
    /// Monster poise threshold (`data.monsterPoiseThresholdTable`, for
    /// Freeze/Electrocute/HeavyStun/Pin).
    pub poise_threshold: Vec<u32>,
}

/// A generic level lookup (u32 table): the level is clamped to
/// `[1, table length]`, index = level - 1; an empty table returns 0 (a
/// loader-side test enforces every table is always 100 entries, so this
/// path shouldn't trigger under normal operation). Matches
/// `monster.rs::level_to_index`'s clamp semantics value-for-value (a
/// migration invariant).
fn lookup_u32(table: &[u32], level: u32) -> u32 {
    if table.is_empty() {
        return 0;
    }
    table[(level.clamp(1, table.len() as u32) - 1) as usize]
}

/// A generic level lookup (f64 table): same semantics as [`lookup_u32`].
fn lookup_f64(table: &[f64], level: u32) -> f64 {
    if table.is_empty() {
        return 0.0;
    }
    table[(level.clamp(1, table.len() as u32) - 1) as usize]
}

impl MonsterScalingTable {
    /// Looks up monster base accuracy (level 1..=100, out-of-range
    /// clamped; corresponds to `monster_accuracy`).
    pub fn accuracy_at(&self, level: u32) -> u32 {
        lookup_u32(&self.accuracy, level)
    }

    /// Looks up monster base evasion (corresponds to `monster_evasion`).
    pub fn evasion_at(&self, level: u32) -> u32 {
        lookup_u32(&self.evasion, level)
    }

    /// Looks up monster base armour (corresponds to `monster_armour`).
    pub fn armour_at(&self, level: u32) -> u32 {
        lookup_u32(&self.armour, level)
    }

    /// Looks up monster base life (corresponds to `monster_life`).
    pub fn life_at(&self, level: u32) -> u32 {
        lookup_u32(&self.life, level)
    }

    /// Looks up monster base damage (for EHP; corresponds to `monster_damage`).
    pub fn damage_at(&self, level: u32) -> f64 {
        lookup_f64(&self.damage, level)
    }

    /// Looks up the monster ailment threshold (raw table value;
    /// corresponds to `enemy_ailment_threshold`).
    pub fn ailment_threshold_at(&self, level: u32) -> u32 {
        lookup_u32(&self.ailment_threshold, level)
    }

    /// Looks up the monster poise threshold (raw table value; corresponds
    /// to `enemy_poise_threshold`).
    pub fn poise_threshold_at(&self, level: u32) -> u32 {
        lookup_u32(&self.poise_threshold, level)
    }
}

/// The fallback (used when no GameData is injected): **value-equal** to
/// `base/monster_scaling.json` field by field (a migration invariant; the
/// W2 test already locks JSON == this Rust source of truth).
///
/// - The eight tables that have a pobr Rust source of truth (including
///   `ally_life`, upgraded in #12 to reference
///   `MONSTER_ALLY_LIFE_TABLE` — consumed for deriving minion base life)
///   reference `crate::monster`'s existing consts directly (zero literal
///   duplication);
/// - `ally_damage` is vendor-only (nothing in calc consumes it yet), a
///   literal transcribed from
///   `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L10 (same source, same
///   value as `base/monster_scaling.json`; 2-decimal-place convention).
impl Default for MonsterScalingTable {
    fn default() -> Self {
        Self {
            accuracy: MONSTER_ACCURACY_TABLE.to_vec(),
            evasion: MONSTER_EVASION_TABLE.to_vec(),
            armour: MONSTER_ARMOUR_TABLE.to_vec(),
            life: MONSTER_LIFE_TABLE.to_vec(),
            ally_life: crate::monster::MONSTER_ALLY_LIFE_TABLE.to_vec(),
            damage: MONSTER_DAMAGE_TABLE.to_vec(),
            ally_damage: vec![
                3.11, 4.42, 5.82, 7.31, 8.92, 10.63, 12.46, 14.42, // lv1-8
                16.51, 18.73, 21.1, 23.62, 26.31, 29.16, 32.19, 35.42, // lv9-16
                38.83, 42.46, 46.31, 50.39, 54.71, 59.29, 64.14, 69.27, // lv17-24
                74.69, 80.43, 86.5, 92.91, 99.69, 106.84, 114.4, 122.37, // lv25-32
                130.79, 139.67, 149.04, 158.91, 169.32, 180.29, 191.86, 204.04, // lv33-40
                216.86, 230.37, 244.6, 259.57, 275.32, 291.9, 309.34, 327.69, // lv41-48
                346.98, 367.27, 388.59, 411.01, 434.57, 459.32, 485.33, 512.66, // lv49-56
                541.35, 571.49, 603.14, 636.37, 671.26, 707.87, 746.3, 786.63, // lv57-64
                828.94, 873.34, 919.91, 968.76, 1019.99, 1073.72, 1130.06, 1189.13, // lv65-72
                1251.06, 1315.98, 1384.03, 1455.34, 1530.08, 1608.4, 1690.46,
                1776.43, // lv73-80
                1866.5, 1960.84, 2059.66, 2163.16, 2271.56, 2385.06, 2503.91,
                2628.36, // lv81-88
                2758.64, 2895.03, 3037.8, 3187.24, 3343.66, 3507.35, 3678.66,
                3857.93, // lv89-96
                4045.51, 4241.77, 4447.11, 4661.93, // lv97-100
            ],
            ailment_threshold: MONSTER_AILMENT_THRESHOLD_TABLE.to_vec(),
            poise_threshold: MONSTER_POISE_THRESHOLD_TABLE.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monster;

    /// Fallback invariant: `Default`'s seven source-of-truth tables agree
    /// with `monster.rs`'s lookup functions at every level (including
    /// clamp semantics: 0 → lv1, >100 → lv100).
    #[test]
    fn default_lookups_match_legacy_monster_tables() {
        let t = MonsterScalingTable::default();
        for lv in 0..=120u32 {
            assert_eq!(
                t.accuracy_at(lv),
                monster::monster_accuracy(lv),
                "lv{lv} accuracy"
            );
            assert_eq!(
                t.evasion_at(lv),
                monster::monster_evasion(lv),
                "lv{lv} evasion"
            );
            assert_eq!(
                t.armour_at(lv),
                monster::monster_armour(lv),
                "lv{lv} armour"
            );
            assert_eq!(t.life_at(lv), monster::monster_life(lv), "lv{lv} life");
            assert_eq!(
                t.damage_at(lv),
                monster::monster_damage(lv),
                "lv{lv} damage"
            );
            assert_eq!(
                t.ailment_threshold_at(lv),
                monster::enemy_ailment_threshold(lv),
                "lv{lv} ailment_threshold"
            );
            assert_eq!(
                t.poise_threshold_at(lv),
                monster::enemy_poise_threshold(lv),
                "lv{lv} poise_threshold"
            );
        }
    }

    /// Spot checks for the two vendor-only ally tables (matching
    /// `base/monster_scaling.json` / Misc.lua L8, L10).
    #[test]
    fn default_ally_tables_spot_checks() {
        let t = MonsterScalingTable::default();
        assert_eq!(t.ally_life.len(), MONSTER_SCALING_TABLE_LEN);
        assert_eq!(t.ally_damage.len(), MONSTER_SCALING_TABLE_LEN);
        assert_eq!(t.ally_life[0], 51, "lv1 ally_life");
        assert_eq!(t.ally_life[84], 11708, "lv85 ally_life");
        assert_eq!(t.ally_life[99], 17980, "lv100 ally_life");
        assert_eq!(t.ally_damage[0], 3.11, "lv1 ally_damage");
        assert_eq!(t.ally_damage[84], 2271.56, "lv85 ally_damage");
        assert_eq!(t.ally_damage[99], 4661.93, "lv100 ally_damage");
    }

    /// Empty-table defense: a lookup against an empty Vec returns 0 (no panic).
    #[test]
    fn empty_table_lookup_is_zero() {
        assert_eq!(lookup_u32(&[], 50), 0);
        assert_eq!(lookup_f64(&[], 50), 0.0);
    }
}
