//! PoE2 enemy-tier enum and monster scalar constants.
//!
//! The nine per-level monster tables (accuracy / evasion / armour / life /
//! ally life / damage / ally damage / ailment threshold / poise threshold)
//! are **not** here: they are game data, imported as
//! `data/<poe_version>/base/monster_scaling.json` (schema in
//! [`crate::catalog::monster_scaling`]) and read by the engine through the
//! injected [`crate::catalog::RuntimeConstants`] (`cfg.constants.monster_scaling`).
//! The copy of PoB2 `Misc.lua`'s tables this file used to carry was deleted
//! after a level-by-level equality check against the shipped JSON.
//!
//! What remains:
//! - [`EnemyTier`], an L4 framework-level enum (config tier id), and
//!   [`EnemyTierDefaults`], the per-tier stat set `setup_env` assembles from
//!   the injected tables and `enemy_presets`.
//! - Scalar constants still referenced by catalog `Default`s
//!   (`enemy_presets` / `game_constants`) and by tests. Their data-driven
//!   counterparts are `base/enemy_presets.json` / `base/game_constants.json`;
//!   **no new calc-path consumers of these constants are allowed**.
//!
//! Constant sources:
//! - `src/Modules/Data.lua` (PathOfBuilding-PoE2 dev branch) —
//!   `data.misc.MaxEnemyLevel = 85`,
//!   `normalEnemyDPSMult`, `stdBossDPSMult`, `pinnacleBossDPSMult`,
//!   `uberBossDPSMult`, `pinnacleBossPen`, `uberBossPen`, `EnemyMaxResist`,
//!   `EnemyPhysicalDamageReductionCap`.
//! - `src/Data/Bosses.lua` (same library) — the boss
//!   `armourMult`/`evasionMult`/`isUber` list;
//!   `bossStats.PinnacleArmourMean`/`PinnacleEvasionMean`/`UberArmourMean`/`UberEvasionMean`
//!   are computed from this list's means (see the constant comments below;
//!   **this is currently a leftover PoE1 boss list, i.e. placeholder data**).
//!
//! This module contains only pure data definitions, **zero I/O, zero async**.

use serde::{Deserialize, Serialize};

// Constants

/// Default max monster level cap (PoB2's `data.misc.MaxEnemyLevel = 85`).
/// The default `enemyLevel` = `min(MAX_ENEMY_LEVEL, char_level)`.
pub const MAX_ENEMY_LEVEL: u32 = 85;

/// Monster max resistance (PoB2 `EnemyMaxResist = data.monsterConstants["base_maximum_all_resistances_%"] = 75`).
pub const ENEMY_MAX_RESIST: f64 = 75.0;

/// Monster physical damage reduction cap (PoB2 `EnemyPhysicalDamageReductionCap =
/// data.monsterConstants["maximum_physical_damage_reduction_%"] = 75`).
pub const ENEMY_PHYS_DMGRED_CAP: f64 = 75.0;

/// Monster base crit damage bonus (PoB2 `data.monsterConstants["base_critical_hit_damage_bonus"] = 30`).
pub const MONSTER_BASE_CRIT_DAMAGE_BONUS: f64 = 30.0;

/// Normal-monster DPS multiplier (PoB2 `normalEnemyDPSMult = 1/4.40`, used in EHP calc).
pub const NORMAL_ENEMY_DPS_MULT: f64 = 1.0 / 4.40;

/// Standard boss DPS multiplier (PoB2 `stdBossDPSMult = 4/4.40`).
pub const STD_BOSS_DPS_MULT: f64 = 4.0 / 4.40;

/// Pinnacle boss DPS multiplier (PoB2 `pinnacleBossDPSMult = 8/4.40`).
pub const PINNACLE_BOSS_DPS_MULT: f64 = 8.0 / 4.40;

/// Uber boss DPS multiplier (PoB2 `uberBossDPSMult = 10/4.25`).
pub const UBER_BOSS_DPS_MULT: f64 = 10.0 / 4.25;

/// Pinnacle boss elemental penetration (PoB2 `pinnacleBossPen = 15/5 = 3`).
pub const PINNACLE_BOSS_PEN: f64 = 15.0 / 5.0;

/// Uber boss elemental penetration (PoB2 `uberBossPen = 40/5 = 8`).
pub const UBER_BOSS_PEN: f64 = 40.0 / 5.0;

/// Default minimum level for Pinnacle/Uber bosses (PoB2 ConfigOptions.lua: `m_max(config, 82)`).
pub const PINNACLE_MIN_LEVEL: u32 = 82;

// Boss armour/evasion mean multipliers (based on the mean of the Bosses.lua
// list; see Data.lua for the underlying calculation).
// Note: Bosses.lua is currently a leftover PoE1 boss list (Shaper/Sirus/Maven
// etc.) — PoB2 hasn't replaced it with PoE2 bosses yet, so this is
// placeholder data.
// Calculation (22 bosses, 7 of them isUber):
//   pinnacle_armour_mean = 100 + (50+0+0+25+100+0+0+0+0+25+50+50+100+50+75+0+100+75+100+100+100+100)/22
//                        = 100 + 1100/22 = 100 + 50.0 = 150.0
//   pinnacle_evasion_mean = 100 + (0+0+50+0+0+33+33+50+0+50+50+100+0+50+33+33+33+33+0+0+0+0)/22
//                         = 100 + 548/22 ≈ 100 + 24.909 ≈ 124.909
//   uber_armour_mean = 100 + (50+0+0+25+100+0+0)/7 = 100 + 175/7 = 125.0
//   uber_evasion_mean = 100 + (0+0+50+0+0+33+33)/7 = 100 + 116/7 ≈ 116.571

/// Pinnacle boss armour multiplier (percent, 100% = no bonus; PoB2 `bossStats.PinnacleArmourMean`).
pub const PINNACLE_ARMOUR_MEAN: f64 = 150.0;

/// Pinnacle boss evasion multiplier (percent; PoB2 `bossStats.PinnacleEvasionMean`).
pub const PINNACLE_EVASION_MEAN: f64 = 124.909_090_909_090_9;

/// Uber boss armour multiplier (percent; PoB2 `bossStats.UberArmourMean`).
pub const UBER_ARMOUR_MEAN: f64 = 125.0;

/// Uber boss evasion multiplier (percent; PoB2 `bossStats.UberEvasionMean`).
pub const UBER_EVASION_MEAN: f64 = 116.571_428_571_428_57;

// Ailment-threshold game constants (from src/Data/Misc.lua data.gameConstants)

/// Shock chance multiplier: `hitChance = hitAvg / enemyThreshold * SHOCK_CHANCE_MULTIPLIER`.
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["ShockChanceMultiplier"] = 25`.
/// Per the wiki: "every 4% of threshold damage = 1% shock chance" =
/// `1 / 0.04 = 25`.
pub const SHOCK_CHANCE_MULTIPLIER: f64 = 25.0;

/// Ignite chance multiplier: `hitChance = hitAvg / enemyThreshold * IGNITE_CHANCE_MULTIPLIER`.
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["IgniteChanceMultiplier"] = 20`.
pub const IGNITE_CHANCE_MULTIPLIER: f64 = 20.0;

/// Chill effect multiplier (linear scaling): `chillEffect = CHILL_EFFECT_MULTIPLIER * (damage / threshold) * effectMod`.
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["ChillEffectMultiplier"] = 100`.
pub const CHILL_EFFECT_MULTIPLIER: f64 = 100.0;

/// Chill's max effect (% reduced action speed).
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["ChillMaxEffect"] = 50`.
pub const CHILL_MAX_EFFECT: f64 = 50.0;

/// Chill's min effect (default / lowest applicable threshold, % reduced action speed).
///
/// Source: PoB2 `src/Modules/Data.lua::nonDamagingAilment["Chill"].min = 30`.
/// 0.5.0 note: chill's minimum threshold is 30% (was 5% before 0.5.0).
pub const CHILL_MIN_EFFECT: f64 = 30.0;

/// Shock's default/min effect (% increased damage taken).
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["BaseShockMagnitude"] = 20`.
pub const BASE_SHOCK_MAGNITUDE: f64 = 20.0;

/// Shock's max effect cap (% increased damage taken).
///
/// Source: PoB2 `src/Modules/Data.lua::nonDamagingAilment["Shock"].max = 100`.
pub const SHOCK_MAX_EFFECT: f64 = 100.0;

/// Freeze damage scale (monster target): `poiseBuildup = FREEZE_DAMAGE_SCALE / enemyPoiseThreshold * ...`.
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["FreezeDamageScale"] = 2.1`.
/// Note: player targets use `FreezeDamageScalePlayer = 2.0` (this module
/// doesn't cover player defence).
pub const FREEZE_DAMAGE_SCALE: f64 = 2.1;

/// Electrocute damage scale (monster target): `poiseBuildup = ELECTROCUTE_DAMAGE_SCALE / enemyPoiseThreshold * ...`.
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["ElectrocuteDamageScale"] = 1.7`.
pub const ELECTROCUTE_DAMAGE_SCALE: f64 = 1.7;

/// Heavy stun damage scale (monster target).
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["HeavyStunDamageScale"] = 0.58`.
pub const HEAVY_STUN_DAMAGE_SCALE: f64 = 0.58;

/// Pin damage scale (monster target).
///
/// Source: the game table `GameConstants` (patch 0.5.5.4.2, row `PinDamageScale`,
/// 420 / 100 = 4.2). The calculation reads the same value from
/// `base/game_constants.json` (`pin_damage_scale`); this constant is the
/// Rust-side mirror that locks it in.
pub const PIN_DAMAGE_SCALE: f64 = 4.2;

/// Boss poise-threshold MORE correction (%, from ConfigOptions.lua's
/// `enemyModList:NewMod("PoiseThreshold", "MORE", 500, ...)`).
///
/// Applies to Boss/Pinnacle/Uber tiers. Already injected into enemy.mod_db in
/// `setup_env.rs`; this constant is documentation only and isn't reused here.
pub const BOSS_POISE_THRESHOLD_MORE: f64 = 500.0;

/// Player ailment-threshold life ratio (PlayerAilmentThreshold = max life ×
/// this factor).
///
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["PlayerAilmentThresholdLifeFactor"] = 0.5`.
pub const PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR: f64 = 0.5;

// Level helpers

/// Clamps a user-supplied level to `[1, MAX_ENEMY_LEVEL]`, returning the
/// effective level.
///
/// Corresponds to PoB2 `CalcSetup.lua`:
/// ```lua
/// env.enemyLevel = build.configTab.enemyLevel or m_min(data.misc.MaxEnemyLevel, build.characterLevel)
/// ```
#[inline]
pub fn clamp_enemy_level(level: u32) -> u32 {
    level.clamp(1, MAX_ENEMY_LEVEL)
}

/// Computes the minimum applicable Chill threshold (unmitigated cold damage
/// must exceed this for Chill to take effect at all).
///
/// # Formula (PoB2 CalcOffence.lua)
/// ```text
/// chill_minimum_threshold = ailment_threshold_at(level) / CHILL_EFFECT_MULTIPLIER
/// ```
/// In other words: dealing 1% of threshold damage → 1% Chill effect
/// (< 30% is discarded); dealing 30% of threshold damage → the minimum
/// effective Chill of 30%.
/// The `enemy_mod(EnemyAilmentThreshold)` multiplier is already reflected in
/// `enemy_threshold`; this helper takes the already-computed
/// `effective_threshold` (= raw table value × mod multiplier).
///
/// Source: PoB2 `CalcOffence.lua::chillMinimumThreshold = enemyThreshold / ChillEffectMultiplier`.
pub fn chill_minimum_threshold(effective_threshold: f64) -> f64 {
    effective_threshold / CHILL_EFFECT_MULTIPLIER
}

// EnemyTier enum

/// An enemy tier (corresponds to PoB2 ConfigOptions.lua's four-tier
/// `enemyIsBoss`).
///
/// **Defaults to `Pinnacle`** (PoB2's `defaultIndex = 3`) — PoB2 computes DPS
/// against a Guardian/Pinnacle boss by default.
///
/// Source: `src/Modules/ConfigOptions.lua` (enemyIsBoss),
/// `src/Modules/Data.lua` (bossStats/DPSMult/Pen).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EnemyTier {
    /// A normal monster, no extra resistance/damage-reduction bonus.
    None,
    /// A standard boss (map boss, etc.): +30% elemental resistance.
    Boss,
    /// A Pinnacle/Guardian boss (default): +50% elemental resistance,
    /// armour/evasion at the mean multiplier, minor penetration.
    #[default]
    Pinnacle,
    /// An Uber boss: +50% elemental resistance, DamageTaken MORE -70%,
    /// higher penetration.
    Uber,
}

impl EnemyTier {
    /// Parses a tier from PoB2's `enemyIsBoss` config value string.
    ///
    /// PoB2's build XML stores the list-type `<Input name="enemyIsBoss">` as
    /// `string="None|Boss|Pinnacle|Uber"` (the `val` of each item in vendor
    /// `ConfigOptions.lua`'s `enemyIsBoss` list); a string outside the four
    /// tiers returns `None` (the caller is responsible for falling back to a
    /// default).
    pub fn from_pob_str(value: &str) -> Option<Self> {
        match value {
            "None" => Some(EnemyTier::None),
            "Boss" => Some(EnemyTier::Boss),
            "Pinnacle" => Some(EnemyTier::Pinnacle),
            "Uber" => Some(EnemyTier::Uber),
            _ => None,
        }
    }

    /// Whether this tier is any kind of boss (Boss / Pinnacle / Uber).
    pub fn is_boss(self) -> bool {
        !matches!(self, EnemyTier::None)
    }

    /// Whether this tier is Pinnacle or Uber (i.e. has `Condition:PinnacleBoss`).
    pub fn is_pinnacle_or_uber(self) -> bool {
        matches!(self, EnemyTier::Pinnacle | EnemyTier::Uber)
    }

    /// Default minimum monster level (Pinnacle/Uber require at least 82;
    /// normal/Boss have no floor).
    ///
    /// Source: PoB2 ConfigOptions.lua's `m_max(config, 82)`.
    pub fn min_level(self) -> u32 {
        match self {
            EnemyTier::Pinnacle | EnemyTier::Uber => PINNACLE_MIN_LEVEL,
            _ => 1,
        }
    }

    /// Elemental resistance bonus (%, a BASE value injected into
    /// `enemy.mod_db`'s `*Resist BASE`).
    ///
    /// - None: 0
    /// - Boss: +30
    /// - Pinnacle / Uber: +50
    ///
    /// Source: PoB2 ConfigOptions.lua's per-tier enemy modList.
    pub fn elemental_resist_bonus(self) -> f64 {
        match self {
            EnemyTier::None => 0.0,
            EnemyTier::Boss => 30.0,
            EnemyTier::Pinnacle | EnemyTier::Uber => 50.0,
        }
    }

    /// Chaos resistance bonus (%). Currently 0 for every tier (PoB2 doesn't
    /// set a chaos-resist boss bonus).
    pub fn chaos_resist_bonus(self) -> f64 {
        0.0
    }

    /// Armour multiplier (%, used in `monsterArmourTable[lv] * armour_mult_pct / 100.0`).
    ///
    /// - None / Boss: 100% (no bonus)
    /// - Pinnacle: `PINNACLE_ARMOUR_MEAN` (≈150%, PoE1 boss mean, placeholder)
    /// - Uber: `UBER_ARMOUR_MEAN` (≈125%, PoE1 uber boss mean, placeholder)
    pub fn armour_mult_pct(self) -> f64 {
        match self {
            EnemyTier::None | EnemyTier::Boss => 100.0,
            EnemyTier::Pinnacle => PINNACLE_ARMOUR_MEAN,
            EnemyTier::Uber => UBER_ARMOUR_MEAN,
        }
    }

    /// Evasion multiplier (%, used in `monsterEvasionTable[lv] * evasion_mult_pct / 100.0`).
    ///
    /// - None / Boss: 100%
    /// - Pinnacle: `PINNACLE_EVASION_MEAN` (≈124.9%)
    /// - Uber: `UBER_EVASION_MEAN` (≈116.6%)
    pub fn evasion_mult_pct(self) -> f64 {
        match self {
            EnemyTier::None | EnemyTier::Boss => 100.0,
            EnemyTier::Pinnacle => PINNACLE_EVASION_MEAN,
            EnemyTier::Uber => UBER_EVASION_MEAN,
        }
    }

    /// Elemental penetration (%, from the boss's built-in penetration
    /// modifier; injected on the player side or into the enemy modDB, see
    /// Step-2).
    ///
    /// - None / Boss: 0
    /// - Pinnacle: 3 (`pinnacleBossPen = 15/5`)
    /// - Uber: 8 (`uberBossPen = 40/5`)
    pub fn pen(self) -> f64 {
        match self {
            EnemyTier::None | EnemyTier::Boss => 0.0,
            EnemyTier::Pinnacle => PINNACLE_BOSS_PEN,
            EnemyTier::Uber => UBER_BOSS_PEN,
        }
    }

    /// DPS multiplier used for EHP calc (`monsterDamageTable[lv] * 1.5 * dps_mult()`).
    pub fn dps_mult(self) -> f64 {
        match self {
            EnemyTier::None => NORMAL_ENEMY_DPS_MULT,
            EnemyTier::Boss => STD_BOSS_DPS_MULT,
            EnemyTier::Pinnacle => PINNACLE_BOSS_DPS_MULT,
            EnemyTier::Uber => UBER_BOSS_DPS_MULT,
        }
    }

    /// Whether this tier has `DamageTaken MORE -70` (Uber only).
    ///
    /// Source: PoB2 `enemyModList:NewMod("DamageTaken","MORE",-70,"Boss")` (Uber tier).
    pub fn has_damage_taken_penalty(self) -> bool {
        matches!(self, EnemyTier::Uber)
    }

    /// `DamageTaken MORE` value (Uber = -70, otherwise 0).
    pub fn damage_taken_more(self) -> f64 {
        if self.has_damage_taken_penalty() {
            -70.0
        } else {
            0.0
        }
    }
}

/// The default stat set for an enemy tier (consumed by Step-2 `setup_env`,
/// which builds it from the injected `monster_scaling` table and
/// `enemy_presets`), ready to inject into `enemy.mod_db`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnemyTierDefaults {
    /// The actual monster level used (clamped and adjusted for `tier.min_level()`).
    pub level: u32,
    /// Accuracy (BASE, injected as `enemy.mod_db.Accuracy BASE`).
    pub accuracy: u32,
    /// Evasion (already multiplied by the tier's evasion multiplier, BASE,
    /// injected as `Evasion BASE`).
    pub evasion: f64,
    /// Armour (already multiplied by the tier's armour multiplier, BASE,
    /// injected as `Armour BASE`).
    pub armour: f64,
    /// Life (BASE, for EHP reference).
    pub life: u32,
    /// Elemental resistance bonus (%, injected into each of
    /// `{Fire/Cold/Lightning}Resist BASE`).
    pub elemental_resist: f64,
    /// Chaos resistance bonus (%, injected as `ChaosResist BASE`).
    pub chaos_resist: f64,
    /// Penetration (%, injected as a player-side penetration modifier, see Step-2).
    pub pen: f64,
    /// Uber's `DamageTaken MORE` value (-70 or 0).
    pub damage_taken_more: f64,
    /// Base damage for EHP (already multiplied by `1.5 * dps_mult`).
    pub base_damage_for_ehp: f64,
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    // Level bounds

    #[test]
    fn clamp_enemy_level_above_max() {
        assert_eq!(clamp_enemy_level(100), MAX_ENEMY_LEVEL);
        assert_eq!(clamp_enemy_level(85), 85);
        assert_eq!(clamp_enemy_level(84), 84);
    }

    #[test]
    fn enemy_tier_default_is_pinnacle() {
        assert_eq!(EnemyTier::default(), EnemyTier::Pinnacle);
    }

    #[test]
    fn enemy_tier_none_resist() {
        assert_eq!(EnemyTier::None.elemental_resist_bonus(), 0.0);
    }

    #[test]
    fn enemy_tier_boss_resist() {
        assert_eq!(EnemyTier::Boss.elemental_resist_bonus(), 30.0);
    }

    #[test]
    fn enemy_tier_pinnacle_resist() {
        assert_eq!(EnemyTier::Pinnacle.elemental_resist_bonus(), 50.0);
    }

    #[test]
    fn enemy_tier_uber_resist() {
        assert_eq!(EnemyTier::Uber.elemental_resist_bonus(), 50.0);
    }

    #[test]
    fn enemy_tier_none_is_not_boss() {
        assert!(!EnemyTier::None.is_boss());
        assert!(EnemyTier::Boss.is_boss());
        assert!(EnemyTier::Pinnacle.is_boss());
        assert!(EnemyTier::Uber.is_boss());
    }

    #[test]
    fn enemy_tier_pinnacle_min_level() {
        assert_eq!(EnemyTier::Pinnacle.min_level(), PINNACLE_MIN_LEVEL);
        assert_eq!(EnemyTier::Uber.min_level(), PINNACLE_MIN_LEVEL);
        assert_eq!(EnemyTier::None.min_level(), 1);
        assert_eq!(EnemyTier::Boss.min_level(), 1);
    }

    #[test]
    fn enemy_tier_uber_damage_taken() {
        assert!(EnemyTier::Uber.has_damage_taken_penalty());
        assert_eq!(EnemyTier::Uber.damage_taken_more(), -70.0);
        assert!(!EnemyTier::Pinnacle.has_damage_taken_penalty());
        assert_eq!(EnemyTier::None.damage_taken_more(), 0.0);
    }

    #[test]
    fn enemy_tier_pen() {
        assert_eq!(EnemyTier::None.pen(), 0.0);
        assert_eq!(EnemyTier::Boss.pen(), 0.0);
        assert!((EnemyTier::Pinnacle.pen() - 3.0).abs() < 1e-9);
        assert!((EnemyTier::Uber.pen() - 8.0).abs() < 1e-9);
    }

    #[test]
    fn enemy_tier_armour_evasion_mult() {
        assert_eq!(EnemyTier::None.armour_mult_pct(), 100.0);
        assert_eq!(EnemyTier::Boss.armour_mult_pct(), 100.0);
        // Pinnacle armour ≈ 150%
        assert!((EnemyTier::Pinnacle.armour_mult_pct() - 150.0).abs() < 1e-6);
        // Pinnacle evasion ≈ 124.91%
        assert!((EnemyTier::Pinnacle.evasion_mult_pct() - 124.909).abs() < 0.001);
        // Uber armour = 125%
        assert!((EnemyTier::Uber.armour_mult_pct() - 125.0).abs() < 1e-9);
        // Uber evasion ≈ 116.57%
        assert!((EnemyTier::Uber.evasion_mult_pct() - 116.571).abs() < 0.001);
    }

    #[test]
    fn game_constants_values() {
        // Verified by direct transcription from PoB2 Misc.lua
        assert_eq!(SHOCK_CHANCE_MULTIPLIER, 25.0);
        assert_eq!(IGNITE_CHANCE_MULTIPLIER, 20.0);
        assert_eq!(CHILL_EFFECT_MULTIPLIER, 100.0);
        assert_eq!(CHILL_MAX_EFFECT, 50.0);
        assert_eq!(CHILL_MIN_EFFECT, 30.0);
        assert_eq!(BASE_SHOCK_MAGNITUDE, 20.0);
        assert_eq!(SHOCK_MAX_EFFECT, 100.0);
        assert_eq!(FREEZE_DAMAGE_SCALE, 2.1);
        assert_eq!(ELECTROCUTE_DAMAGE_SCALE, 1.7);
        assert_eq!(HEAVY_STUN_DAMAGE_SCALE, 0.58);
        assert_eq!(PIN_DAMAGE_SCALE, 4.2);
        assert_eq!(BOSS_POISE_THRESHOLD_MORE, 500.0);
        assert_eq!(PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR, 0.5);
    }

    // EnemyTier ↔ PoB2 enemyIsBoss strings

    #[test]
    fn enemy_tier_parses_pob2_config_strings() {
        // PoB2 ConfigOptions.lua's `enemyIsBoss` list, the four tiers' val strings.
        assert_eq!(EnemyTier::from_pob_str("None"), Some(EnemyTier::None));
        assert_eq!(EnemyTier::from_pob_str("Boss"), Some(EnemyTier::Boss));
        assert_eq!(
            EnemyTier::from_pob_str("Pinnacle"),
            Some(EnemyTier::Pinnacle)
        );
        assert_eq!(EnemyTier::from_pob_str("Uber"), Some(EnemyTier::Uber));
        // Strings outside the table (including case mismatches) don't force-map to anything.
        assert_eq!(EnemyTier::from_pob_str("uber"), None);
        assert_eq!(EnemyTier::from_pob_str(""), None);
    }
}
