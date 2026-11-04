//! Monster per-level scaling table schema (`base/monster_scaling.json`).
//!
//! Corresponds to PoB2 `src/Data/Misc.lua`'s nine per-level monster tables
//! (100 entries each, indexed by monster level - 1; the ultimate source is
//! GGG's `DefaultMonsterStats.dat`, and the adapter will eventually be able
//! to regenerate an equivalent JSON directly from the `.dat`):
//!
//! | JSON field | PoB2 Lua table (Misc.lua line) |
//! |---|---|
//! | `accuracy` | `data.monsterAccuracyTable` (L6) |
//! | `evasion` | `data.monsterEvasionTable` (L5) |
//! | `armour` | `data.monsterArmourTable` (L11) |
//! | `life` | `data.monsterLifeTable` (L7) |
//! | `ally_life` | `data.monsterAllyLifeTable` (L8) |
//! | `damage` | `data.monsterDamageTable` (L9) |
//! | `ally_damage` | `data.monsterAllyDamageTable` (L10) |
//! | `ailment_threshold` | `data.monsterAilmentThresholdTable` (L12) |
//! | `poise_threshold` | `data.monsterPoiseThresholdTable` (L13) |
//!
//! The imported JSON is the only source of these values: the engine reads
//! them through `RuntimeConstants.monster_scaling` (the copy of the nine
//! tables that `pobr_data::monster` used to carry was deleted after a
//! level-by-level equality check against the shipped JSON).
//!
//! Value conventions: `damage` / `ally_damage` keep vendor's noisy f32
//! values (e.g. `9.1599998474121`) rounded to 2 decimal places (`9.16`),
//! matching the `round(..., 2)` precision handling in PoB2
//! `CalcActiveSkill.lua:907`'s hiddenDamageFixup derivation.
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
///
/// `Default` is **empty** (every lookup returns 0), not a copy of the game
/// table: it exists only because [`super::RuntimeConstants`] needs a value
/// before data is injected (zero-I/O engine construction, `BuildData::empty`
/// engine unit tests, and sealed fixture packs without the file). Every
/// published package ships `base/monster_scaling.json`; tests that need
/// monster values load it from the data directory.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
/// path shouldn't trigger under normal operation, only for the empty
/// `Default` used without GameData).
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
    /// clamped).
    pub fn accuracy_at(&self, level: u32) -> u32 {
        lookup_u32(&self.accuracy, level)
    }

    /// Looks up monster base evasion.
    pub fn evasion_at(&self, level: u32) -> u32 {
        lookup_u32(&self.evasion, level)
    }

    /// Looks up monster base armour.
    pub fn armour_at(&self, level: u32) -> u32 {
        lookup_u32(&self.armour, level)
    }

    /// Looks up monster base life.
    pub fn life_at(&self, level: u32) -> u32 {
        lookup_u32(&self.life, level)
    }

    /// Looks up ally (non-hostile summon: minion, totem) base life.
    pub fn ally_life_at(&self, level: u32) -> u32 {
        lookup_u32(&self.ally_life, level)
    }

    /// Looks up monster base damage (for EHP).
    pub fn damage_at(&self, level: u32) -> f64 {
        lookup_f64(&self.damage, level)
    }

    /// Looks up ally (minion) base damage.
    pub fn ally_damage_at(&self, level: u32) -> f64 {
        lookup_f64(&self.ally_damage, level)
    }

    /// Looks up the monster ailment threshold (raw table value, for
    /// Ignite/Shock chance and Chill's minimum; the caller applies
    /// `EnemyAilmentThreshold`).
    pub fn ailment_threshold_at(&self, level: u32) -> u32 {
        lookup_u32(&self.ailment_threshold, level)
    }

    /// Looks up the monster poise threshold (raw table value, for
    /// Freeze/Electrocute/HeavyStun/Pin; the caller applies `PoiseThreshold`
    /// and floors).
    pub fn poise_threshold_at(&self, level: u32) -> u32 {
        lookup_u32(&self.poise_threshold, level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without GameData there is no monster scaling: `Default` is empty
    /// rather than a second copy of the game table, and every lookup is 0.
    /// Published packages ship `base/monster_scaling.json`.
    #[test]
    fn default_is_empty_and_looks_up_zero() {
        let t = MonsterScalingTable::default();
        assert!(t.accuracy.is_empty() && t.ally_life.is_empty() && t.poise_threshold.is_empty());
        assert_eq!(t.life_at(50), 0);
        assert_eq!(t.ally_life_at(50), 0);
        assert_eq!(t.damage_at(50), 0.0);
        assert_eq!(t.ally_damage_at(50), 0.0);
    }

    /// Lookups clamp the level to `[1, table length]` (index = level - 1).
    #[test]
    fn lookups_clamp_to_table_bounds() {
        let t = MonsterScalingTable {
            ally_life: vec![10, 20, 30],
            ally_damage: vec![1.5, 2.5, 3.5],
            ..MonsterScalingTable::default()
        };
        assert_eq!(t.ally_life_at(0), 10);
        assert_eq!(t.ally_life_at(1), 10);
        assert_eq!(t.ally_life_at(2), 20);
        assert_eq!(t.ally_life_at(99), 30);
        assert_eq!(t.ally_damage_at(0), 1.5);
        assert_eq!(t.ally_damage_at(3), 3.5);
    }

    /// Empty-table defense: a lookup against an empty Vec returns 0 (no panic).
    #[test]
    fn empty_table_lookup_is_zero() {
        assert_eq!(lookup_u32(&[], 50), 0);
        assert_eq!(lookup_f64(&[], 50), 0.0);
    }
}
