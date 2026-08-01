//! PoE2 monster level-scaling tables and boss-tier enum.
//!
//! **Deprecation note**: this file's **numeric section** (the per-level
//! tables + multiplier/mean constants + the `EnemyTierDefaults::compute`
//! lookup path) has been downgraded from the calc data's source of truth to
//! a **fallback layer** — the source of truth has moved to
//! `data/<poe_version>/base/`'s `monster_scaling.json` + `enemy_presets.json`
//! (schema in [`crate::catalog::monster_scaling`] /
//! [`crate::catalog::enemy_presets`]; the W2 value-by-value comparison test
//! has locked these as equal to this file).
//!
//! - The calc consumer side (pobr-core's `setup_env.rs` enemy assembly) has
//!   switched to reading the injected [`crate::catalog::RuntimeConstants`]
//!   (`cfg.constants.monster_scaling` / `.enemy_presets`); **no new calc-path
//!   consumers of this file's numbers are allowed**;
//! - What this file still does: (1) provide the single numeric source for
//!   the catalog table types' `Default` (the fallback used when there's no
//!   GameData) — `Default` refers directly to the tables/constants here, so
//!   there's no double authority of literals; (2) [`EnemyTier`] is an L4
//!   framework-level enum (config tier id) that lives here long-term;
//!   (3) existing tests anchor their expected values on this file (not in
//!   conflict with "calc paths may not consume it").
//! - Consumers not yet switched over (to be handled in a later wave):
//!   `perform.rs`'s ailment/poise threshold lookups
//!   (`enemy_ailment_threshold`/`enemy_poise_threshold`), `minion.rs`'s
//!   `MonsterScalingRow` (minion baselines — its function signature has no
//!   cfg channel).
//! - The numeric section will be deleted once all fallback dependents are
//!   cleared out.
//!
//! Data sources:
//! - `src/Data/Misc.lua` (PathOfBuilding-PoE2 dev branch) —
//!   `data.monsterAccuracyTable` / `data.monsterEvasionTable` /
//!   `data.monsterArmourTable` / `data.monsterLifeTable` /
//!   `data.monsterDamageTable` / `data.monsterAilmentThresholdTable` /
//!   `data.monsterPoiseThresholdTable` (100 entries each, indexed by monster
//!   level 1..=100).
//! - `src/Modules/Data.lua` (same library) — `data.misc.MaxEnemyLevel = 85`,
//!   `normalEnemyDPSMult`, `stdBossDPSMult`, `pinnacleBossDPSMult`,
//!   `uberBossDPSMult`, `pinnacleBossPen`, `uberBossPen`, `EnemyMaxResist`,
//!   `EnemyPhysicalDamageReductionCap`.
//! - `src/Data/Bosses.lua` (same library) — the boss
//!   `armourMult`/`evasionMult`/`isUber` list;
//!   `bossStats.PinnacleArmourMean`/`PinnacleEvasionMean`/`UberArmourMean`/`UberEvasionMean`
//!   are computed from this list's means (see the constant comments below;
//!   **this is currently a leftover PoE1 boss list, i.e. placeholder data**).
//!
//! Architectural conventions:
//! - This module contains only pure data definitions and lookup logic,
//!   **zero I/O, zero async**.
//! - Lookup functions take `level: u32` (1..=100) and clamp out-of-range
//!   values to the table bounds.
//! - All interfaces feeding into Step-2 (`setup_env`) are
//!   [`MonsterScalingRow`] and [`EnemyTierDefaults`].
//! - Ailment-threshold lookup interfaces are [`enemy_ailment_threshold`] and
//!   [`enemy_poise_threshold`].

use serde::{Deserialize, Serialize};

// Constants

/// Default max monster level cap (PoB2's `data.misc.MaxEnemyLevel = 85`).
/// The default `enemyLevel` = `min(MAX_ENEMY_LEVEL, char_level)`.
pub const MAX_ENEMY_LEVEL: u32 = 85;

/// Monster stat lookup-table length (1..=100, the Lua tables each have 100 entries).
pub const MONSTER_TABLE_LEN: usize = 100;

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
/// Source: PoB2 `src/Data/Misc.lua::data.gameConstants["PinDamageScale"] = 4.2`.
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

// Lookup table: monster accuracy (monsterAccuracyTable, 100 entries, from
// DefaultMonsterStats.dat)
// Index i corresponds to monster level i+1; the table is 1-indexed by level.
// Source: src/Data/Misc.lua data.monsterAccuracyTable

/// Monster accuracy lookup table (level 1..=100).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterAccuracyTable` (DefaultMonsterStats.dat).
pub const MONSTER_ACCURACY_TABLE: [u32; MONSTER_TABLE_LEN] = [
    32, 35, 39, 43, 48, 52, 57, 62, 67, 72, // lv1-10
    78, 84, 90, 96, 103, 110, 117, 124, 132, 140, // lv11-20
    149, 158, 167, 176, 186, 196, 207, 218, 230, 242, // lv21-30
    254, 267, 281, 295, 309, 325, 340, 356, 373, 391, // lv31-40
    409, 428, 447, 468, 489, 511, 533, 557, 581, 606, // lv41-50
    632, 659, 688, 717, 747, 778, 810, 844, 878, 914, // lv51-60
    951, 990, 1030, 1071, 1114, 1158, 1204, 1251, 1300, 1351, // lv61-70
    1403, 1457, 1514, 1572, 1632, 1694, 1758, 1824, 1893, 1964, // lv71-80
    2038, 2114, 2192, 2273, 2357, 2444, 2534, 2626, 2722, 2821, // lv81-90
    2923, 3029, 3138, 3251, 3368, 3488, 3613, 3741, 3874, 4011, // lv91-100
];

/// Monster evasion lookup table (level 1..=100).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterEvasionTable` (DefaultMonsterStats.dat).
pub const MONSTER_EVASION_TABLE: [u32; MONSTER_TABLE_LEN] = [
    24, 30, 36, 43, 49, 56, 63, 70, 77, 84, // lv1-10
    91, 98, 105, 113, 120, 128, 136, 144, 152, 160, // lv11-20
    168, 176, 185, 193, 202, 211, 220, 229, 238, 247, // lv21-30
    257, 266, 276, 286, 296, 306, 316, 326, 337, 347, // lv31-40
    358, 369, 380, 391, 403, 414, 426, 438, 449, 462, // lv41-50
    474, 486, 499, 511, 524, 537, 551, 564, 578, 591, // lv51-60
    605, 619, 634, 648, 663, 677, 692, 708, 723, 738, // lv61-70
    754, 770, 786, 803, 819, 836, 853, 870, 887, 905, // lv71-80
    923, 941, 959, 977, 996, 1015, 1034, 1053, 1073, 1093, // lv81-90
    1113, 1133, 1154, 1174, 1195, 1217, 1238, 1260, 1282, 1304, // lv91-100
];

/// Monster armour lookup table (level 1..=100).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterArmourTable` (DefaultMonsterStats.dat).
/// Note: armour 5081 at lv82 looks like it dips relative to 2276 at lv65
/// because the endgame range starts a new scaling segment; the table is
/// copied verbatim from DefaultMonsterStats.dat and shouldn't be
/// linearly extrapolated.
pub const MONSTER_ARMOUR_TABLE: [u32; MONSTER_TABLE_LEN] = [
    3, 6, 8, 10, 13, 16, 19, 22, 26, 30, // lv1-10
    34, 39, 43, 49, 54, 60, 67, 73, 81, 89, // lv11-20
    97, 106, 116, 126, 137, 149, 161, 174, 189, 204, // lv21-30
    220, 237, 255, 274, 295, 317, 340, 364, 391, 418, // lv31-40
    448, 479, 512, 547, 585, 624, 666, 711, 758, 808, // lv41-50
    861, 917, 976, 1039, 1105, 1176, 1250, 1329, 1412, 1500, // lv51-60
    1594, 1692, 1796, 1906, 2023, 2146, 2276, 2413, 2558, 2712, // lv61-70
    2874, 3044, 3225, 3416, 3617, 3829, 4053, 4290, 4540, 4803, // lv71-80
    5081, 5375, 5684, 6011, 6355, 6718, 7101, 7505, 7930, 8379, // lv81-90
    8852, 9351, 9877, 10431, 11015, 11630, 12279, 12962, 13682, 14441, // lv91-100
];

/// Monster life lookup table (level 1..=100).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterLifeTable` (DefaultMonsterStats.dat).
/// Note: there's a large jump starting at lv65 (EndgameStartLevel) and after
/// (18272... from lv69), used for the endgame/pinnacle range; the table is
/// copied verbatim — don't extrapolate.
pub const MONSTER_LIFE_TABLE: [u32; MONSTER_TABLE_LEN] = [
    15, 20, 24, 28, 33, 38, 45, 50, 58, 67, // lv1-10
    78, 89, 103, 118, 134, 158, 178, 200, 224, 249, // lv11-20
    276, 305, 335, 366, 400, 434, 472, 510, 551, 593, // lv21-30
    637, 683, 731, 790, 853, 921, 995, 1074, 1160, 1253, // lv31-40
    1353, 1462, 1578, 1705, 1841, 1967, 2101, 2244, 2395, 2556, // lv41-50
    2726, 2909, 3102, 3307, 3525, 3756, 4002, 4264, 4540, 4834, // lv51-60
    5147, 5478, 5829, 6203, 6555, 7079, 7646, 8257, 8918, 11148, // lv61-70
    11984, 12882, 13849, 14887, 18609, 20005, 21505, 23118, 24852, 31065, // lv71-80
    31997, 32956, 33945, 34963, 36012, 37093, 38206, 39352, 40532, 41748, // lv81-90
    43001, 44291, 45619, 46988, 48398, 49850, 51345, 52885, 54472, 56106, // lv91-100
];

/// Ally (non-hostile summon) life lookup table (level 1..=100).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterAllyLifeTable` (L8). A
/// minion's base life = `floor(monsterAllyLifeTable[level] × minionData.life)`
/// (CalcPerform.lua:1046; hostile summons use `monsterLifeTable` ×
/// mapLevelLifeMult instead). Unlike the enemy table, there's no lv65+
/// endgame jump — it's smooth and monotonic.
pub const MONSTER_ALLY_LIFE_TABLE: [u32; MONSTER_TABLE_LEN] = [
    51, 83, 116, 150, 186, 223, 261, 300, 341, 382, // lv1-10
    426, 471, 517, 565, 614, 665, 718, 772, 828, 886, // lv11-20
    945, 1007, 1070, 1135, 1203, 1272, 1344, 1417, 1493, 1571, // lv21-30
    1652, 1734, 1820, 1907, 1998, 2091, 2186, 2285, 2386, 2490, // lv31-40
    2598, 2708, 2821, 2938, 3058, 3181, 3307, 3438, 3571, 3709, // lv41-50
    3850, 3995, 4144, 4298, 4455, 4617, 4783, 4953, 5128, 5308, // lv51-60
    5493, 5682, 5877, 6077, 6282, 6492, 6708, 6930, 7157, 7391, // lv61-70
    7630, 7876, 8128, 8387, 8652, 8924, 9203, 9489, 9783, 10084, // lv71-80
    10393, 10710, 11034, 11367, 11708, 12058, 12417, 12785, 13161, 13548, // lv81-90
    13944, 14350, 14766, 15192, 15629, 16076, 16535, 17005, 17486, 17980, // lv91-100
];

/// Monster base damage lookup table (level 1..=100, f64).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterDamageTable` (DefaultMonsterStats.dat).
/// Usage: `enemyXDamage = monsterDamageTable[lv] * 1.5 * DPSMult` (used for
/// EHP calc, not player DPS).
pub const MONSTER_DAMAGE_TABLE: [f64; MONSTER_TABLE_LEN] = [
    9.16, 10.26, 11.39, 12.57, 13.78, 15.03, 16.32, 17.65, 19.02, 20.44, // lv1-10
    21.90, 23.41, 24.97, 26.57, 28.23, 29.93, 31.69, 33.50, 35.37, 37.29, // lv11-20
    39.27, 41.31, 43.41, 45.57, 47.80, 50.09, 52.45, 54.88, 57.37, 59.94, // lv21-30
    62.59, 65.31, 68.10, 70.98, 73.94, 76.98, 80.11, 83.32, 86.63, 90.02, // lv31-40
    93.51, 97.10, 100.79, 104.57, 108.46, 112.46, 116.57, 120.78, 125.12, 129.56, // lv41-50
    134.13, 138.82, 143.64, 148.58, 153.66, 158.87, 164.21, 169.70, 175.34, 181.12, // lv51-60
    187.05, 193.14, 199.38, 205.79, 212.36, 219.11, 226.03, 233.12, 240.40, 247.86, // lv61-70
    255.52, 263.37, 271.42, 279.68, 288.14, 296.82, 305.72, 314.84, 324.19, 333.78, // lv71-80
    343.60, 353.67, 364.00, 374.58, 385.42, 396.53, 407.92, 419.58, 431.54, 443.79, // lv81-90
    456.34, 469.20, 482.38, 495.87, 509.70, 523.86, 538.37, 553.23, 568.46,
    584.05, // lv91-100
];

// Lookup table: monster ailment threshold (monsterAilmentThresholdTable, 100
// entries)
// Used for chance-derived ailments (Ignite / Shock / Chill's minimum
// threshold).
// Note: unrelated to monster life — indexed independently by level. There's a
// large jump starting at lv65+ (endgame/boss range).
// Source: src/Data/Misc.lua data.monsterAilmentThresholdTable (PathOfBuilding-PoE2 dev)
// PoB2 CalcOffence.lua:
//   enemyThreshold = data.monsterAilmentThresholdTable[env.enemyLevel] * mod(EnemyAilmentThreshold)

/// Monster ailment-threshold lookup table (level 1..=100).
///
/// Used for chance-derived Ignite/Shock and for computing Chill's minimum
/// threshold. Unrelated to monster life — indexed independently by level;
/// there's a large jump starting at lv65 (EndgameStartLevel).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterAilmentThresholdTable`.
pub const MONSTER_AILMENT_THRESHOLD_TABLE: [u32; MONSTER_TABLE_LEN] = [
    15, 20, 24, 28, 34, 39, 46, 52, 60, 70, // lv1-10
    81, 95, 110, 126, 144, 171, 193, 218, 245, 275, // lv11-20
    306, 340, 376, 413, 455, 497, 543, 590, 641, 695, // lv21-30
    752, 812, 874, 950, 1033, 1123, 1220, 1326, 1442, 1568, // lv31-40
    1705, 1854, 2015, 2192, 2384, 2564, 2757, 2966, 3188, 3426, // lv41-50
    3681, 3955, 4247, 4560, 4895, 5254, 5638, 6049, 6489, 6959, // lv51-60
    7462, 8001, 8576, 9193, 9649, 10228, 10841, 11492, 12181, 18272, // lv61-70
    19369, 20531, 21763, 23068, 34602, 36679, 38879, 41212, 43685, 65527, // lv71-80
    68415, 71303, 74191, 77079, 79967, 82855, 85743, 88631, 91519, 94407, // lv81-90
    97295, 100183, 103071, 105959, 108847, 111735, 114623, 117511, 120399, 123287, // lv91-100
];

// Lookup table: monster poise threshold (monsterPoiseThresholdTable, 100
// entries)
// Used for accumulating debuffs (Freeze / Electrocute / HeavyStun / Pin).
// Bosses additionally multiply this by 5x via mod_db's PoiseThreshold MORE
// 500 (already injected in setup_env.rs).
// Source: src/Data/Misc.lua data.monsterPoiseThresholdTable (PathOfBuilding-PoE2 dev)
// PoB2 CalcOffence.lua:
//   enemyPoiseThreshold = floor(monsterPoiseThresholdTable[enemyLevel]
//       * mod(PoiseThreshold, ailment.."Threshold", ...EnemyAilmentThreshold))

/// Monster poise-threshold lookup table (level 1..=100).
///
/// Used to compute accumulation for Freeze/Electrocute/HeavyStun/Pin.
/// Also shows a large jump starting at lv65 (EndgameStartLevel); boss tiers
/// need an additional `PoiseThreshold MORE 500` multiplier at the mod_db
/// layer (already injected by `setup_env.rs` — this function returns the
/// raw table value).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterPoiseThresholdTable`.
pub const MONSTER_POISE_THRESHOLD_TABLE: [u32; MONSTER_TABLE_LEN] = [
    30, 40, 48, 57, 67, 79, 93, 106, 122, 142, // lv1-10
    165, 192, 220, 254, 290, 344, 390, 437, 488, 542, // lv11-20
    599, 659, 724, 791, 862, 937, 1015, 1097, 1183, 1273, // lv21-30
    1367, 1464, 1567, 1660, 1758, 1864, 1976, 2093, 2219, 2352, // lv31-40
    2494, 2644, 2804, 2971, 3150, 3369, 3598, 3846, 4109, 4387, // lv41-50
    4685, 5002, 5338, 5697, 6078, 6485, 6915, 7377, 7866, 8386, // lv51-60
    8940, 9528, 10153, 10819, 26703, 28651, 30662, 32890, 35192, 53405, // lv61-70
    57263, 61392, 65810, 70537, 106973, 114630, 122820, 131580, 140949, 213635, // lv71-80
    225270, 236905, 248540, 260175, 271810, 283445, 295080, 306715, 318350, 329985, // lv81-90
    341620, 353255, 364890, 376525, 388160, 399795, 411430, 423065, 434700,
    446335, // lv91-100
];

// Lookup helper functions

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

/// Clamps a level to the table bounds `[1, 100]` (internal helper).
#[inline]
fn level_to_index(level: u32) -> usize {
    (level.clamp(1, MONSTER_TABLE_LEN as u32) - 1) as usize
}

/// Looks up monster accuracy (level 1..=100, out-of-range clamped
/// automatically).
///
/// Source: PoB2 `data.monsterAccuracyTable` (DefaultMonsterStats.dat).
/// Injected into the enemy ModDB in `CalcSetup.lua` as:
/// ```lua
/// enemyDB:NewMod("Accuracy","BASE", data.monsterAccuracyTable[env.enemyLevel], "Base")
/// ```
pub fn monster_accuracy(level: u32) -> u32 {
    MONSTER_ACCURACY_TABLE[level_to_index(level)]
}

/// Looks up monster evasion (level 1..=100, out-of-range clamped automatically).
pub fn monster_evasion(level: u32) -> u32 {
    MONSTER_EVASION_TABLE[level_to_index(level)]
}

/// Looks up monster armour (level 1..=100, out-of-range clamped automatically).
pub fn monster_armour(level: u32) -> u32 {
    MONSTER_ARMOUR_TABLE[level_to_index(level)]
}

/// Looks up monster life (level 1..=100, out-of-range clamped automatically).
pub fn monster_life(level: u32) -> u32 {
    MONSTER_LIFE_TABLE[level_to_index(level)]
}

/// Looks up ally (non-hostile summon) life (level 1..=100, out-of-range clamped automatically).
pub fn monster_ally_life(level: u32) -> u32 {
    MONSTER_ALLY_LIFE_TABLE[level_to_index(level)]
}

/// Looks up monster base damage (level 1..=100, out-of-range clamped automatically).
pub fn monster_damage(level: u32) -> f64 {
    MONSTER_DAMAGE_TABLE[level_to_index(level)]
}

/// Looks up the monster ailment threshold (raw table value, level 1..=100,
/// out-of-range clamped automatically).
///
/// Used for: chance-derived ailments (Ignite / Shock) and computing Chill's
/// minimum threshold.
///
/// # Full formula (PoB2 CalcOffence.lua)
/// ```text
/// enemy_threshold = monster_ailment_threshold(level) * mod(EnemyAilmentThreshold)
/// ```
/// `EnemyAilmentThreshold` comes from mod_db aggregation (mods / boss
/// corrections, etc.); this function only returns the raw table value — the
/// caller is responsible for applying the mod multiplier.
///
/// # Note
/// - Unrelated to monster life — indexed independently by level.
/// - Large jump starting at lv65 (EndgameStartLevel), for the endgame/boss
///   range.
/// - Boss-tier `EnemyAilmentThreshold` corrections are injected via mod_db
///   mods (not handled by this function).
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterAilmentThresholdTable`.
pub fn enemy_ailment_threshold(level: u32) -> u32 {
    MONSTER_AILMENT_THRESHOLD_TABLE[level_to_index(level)]
}

/// Looks up the monster poise threshold (raw table value, level 1..=100,
/// out-of-range clamped automatically).
///
/// Used for: computing accumulation for Freeze / Electrocute / HeavyStun / Pin.
///
/// # Full formula (PoB2 CalcOffence.lua)
/// ```text
/// enemy_poise_threshold = floor(
///     monster_poise_threshold(level)
///     * mod(PoiseThreshold, ailment + "Threshold",
///           [EnemyStunThreshold for HeavyStun],
///           [EnemyAilmentThreshold for Freeze/Electrocute])
/// )
/// ```
/// `PoiseThreshold` comes from mod_db aggregation (bosses default to MORE
/// 500, already injected in `setup_env.rs`); this function only returns the
/// raw table value — the caller applies the mod multiplier and floors it.
///
/// # Note
/// - Large jump starting at lv65 (EndgameStartLevel, boss range).
/// - A boss's `PoiseThreshold MORE 500` is already injected into
///   enemy.mod_db in `setup_env.rs`, **not handled again by this function**.
/// - Freeze / Electrocute also apply the `EnemyAilmentThreshold` correction.
///
/// Source: PoB2 `src/Data/Misc.lua::data.monsterPoiseThresholdTable`.
pub fn enemy_poise_threshold(level: u32) -> u32 {
    MONSTER_POISE_THRESHOLD_TABLE[level_to_index(level)]
}

/// Computes the minimum applicable Chill threshold (unmitigated cold damage
/// must exceed this for Chill to take effect at all).
///
/// # Formula (PoB2 CalcOffence.lua)
/// ```text
/// chill_minimum_threshold = enemy_ailment_threshold(level) / CHILL_EFFECT_MULTIPLIER
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

// Struct aggregating one level's worth of scaling data (consumed by Step-2 setup_env)

/// A snapshot of a monster's base stats at a given level (accuracy /
/// evasion / armour / life / damage).
///
/// Usage: Step-2 `setup_env` calls [`MonsterScalingRow::at_level`] to get a
/// set of values, then converts them into several `Modifier`s (BASE type)
/// injected into `enemy.mod_db`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MonsterScalingRow {
    /// Monster level (already clamped to 1..=85).
    pub level: u32,
    /// Base accuracy value.
    pub accuracy: u32,
    /// Base evasion value.
    pub evasion: u32,
    /// Base armour value.
    pub armour: u32,
    /// Base life value.
    pub life: u32,
    /// Base damage (used for EHP calc).
    pub damage: f64,
}

impl MonsterScalingRow {
    /// Builds a scaling row for a level (`level` is automatically clamped to
    /// `[1, MAX_ENEMY_LEVEL]`).
    pub fn at_level(raw_level: u32) -> Self {
        let level = clamp_enemy_level(raw_level);
        Self {
            level,
            accuracy: monster_accuracy(level),
            evasion: monster_evasion(level),
            armour: monster_armour(level),
            life: monster_life(level),
            damage: monster_damage(level),
        }
    }

    /// Derives the default `enemyLevel` from the character level (PoB2:
    /// `min(MaxEnemyLevel, charLevel)`).
    pub fn default_for_char_level(char_level: u32) -> Self {
        let enemy_level = char_level.min(MAX_ENEMY_LEVEL);
        Self::at_level(enemy_level)
    }
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

/// The default stat set for an enemy tier (consumed by Step-2 `setup_env`).
///
/// Calling [`EnemyTierDefaults::compute`] with `(level, tier)` gives a set of
/// values ready to inject into `enemy.mod_db`.
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

impl EnemyTierDefaults {
    /// Computes a set of default stats from (user-configured level, tier).
    ///
    /// `config_level`: the monster level specified in user config (0 means
    /// follow the character level — the already-computed value is passed in
    /// here).
    pub fn compute(config_level: u32, tier: EnemyTier) -> Self {
        // Ensure the level meets the tier's minimum requirement and clamp to MAX_ENEMY_LEVEL
        let level = config_level.max(tier.min_level()).min(MAX_ENEMY_LEVEL);
        let row = MonsterScalingRow::at_level(level);
        let armour_mult = tier.armour_mult_pct() / 100.0;
        let evasion_mult = tier.evasion_mult_pct() / 100.0;
        Self {
            level,
            accuracy: row.accuracy,
            evasion: row.evasion as f64 * evasion_mult,
            armour: row.armour as f64 * armour_mult,
            life: row.life,
            elemental_resist: tier.elemental_resist_bonus(),
            chaos_resist: tier.chaos_resist_bonus(),
            pen: tier.pen(),
            damage_taken_more: tier.damage_taken_more(),
            base_damage_for_ehp: row.damage * 1.5 * tier.dps_mult(),
        }
    }
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    // Lookup table bounds

    #[test]
    fn accuracy_table_lv1() {
        assert_eq!(monster_accuracy(1), 32, "lv1 accuracy");
    }

    #[test]
    fn accuracy_table_lv85() {
        // PoB2 Misc.lua data.monsterAccuracyTable[85] = 2357 (Lua 1-indexed = level 85)
        // Note: agent-docs/accuracy-and-enemy.md §4.1 table shows 2444 for "lv85"
        // which is actually lv86 — the Lua source is authoritative.
        assert_eq!(
            monster_accuracy(85),
            2357,
            "lv85 accuracy (MAX_ENEMY_LEVEL)"
        );
    }

    #[test]
    fn accuracy_table_lv100() {
        assert_eq!(monster_accuracy(100), 4011, "lv100 accuracy (table max)");
    }

    #[test]
    fn accuracy_table_clamp_above_100() {
        // clamps to 100 when above 100
        assert_eq!(monster_accuracy(200), monster_accuracy(100));
    }

    #[test]
    fn accuracy_table_clamp_zero() {
        // clamps to 1 when 0
        assert_eq!(monster_accuracy(0), monster_accuracy(1));
    }

    #[test]
    fn evasion_table_lv85() {
        // PoB2 Misc.lua data.monsterEvasionTable[85] = 996
        assert_eq!(monster_evasion(85), 996, "lv85 evasion");
    }

    #[test]
    fn armour_table_lv65() {
        // PoB2 Misc.lua data.monsterArmourTable[65] = 2023 (EndgameStartLevel = 65)
        // Note: agent-docs table showed 2276 for "lv65" which is actually lv67.
        assert_eq!(monster_armour(65), 2023, "lv65 armour = EndgameStartLevel");
    }

    #[test]
    fn armour_table_lv82() {
        // PoB2 Misc.lua data.monsterArmourTable[82] = 5375
        // lv82 armour 5375 vs lv65 armour 2023; both monotone ascending in the source table.
        // Note: agent-docs showed 5081 for lv82 which is actually lv81 in the Lua table.
        assert_eq!(monster_armour(82), 5375, "lv82 armour");
    }

    #[test]
    fn life_table_lv20() {
        assert_eq!(monster_life(20), 249, "lv20 life");
    }

    #[test]
    fn life_table_lv65_jump() {
        // PoB2 Misc.lua: data.monsterLifeTable[65] = 6555, [66] = 7079
        // The big jump happens between lv66 (7079) and lv70 (11148) (EndgameStartLevel = 65)
        assert_eq!(monster_life(65), 6555);
        assert_eq!(monster_life(66), 7079);
        assert_eq!(monster_life(70), 11148);
        assert!(monster_life(70) > monster_life(65), "lv65->70 jump exists");
    }

    #[test]
    fn damage_table_lv85() {
        // PoB2 Misc.lua data.monsterDamageTable[85] = 385.42001342773
        // Note: agent-docs showed 523.9 for "lv85" which is actually lv95+ in the Lua table.
        let d = monster_damage(85);
        assert!((d - 385.42).abs() < 0.1, "lv85 damage ≈ 385.42, got {}", d);
    }

    // Default level derivation

    #[test]
    fn clamp_enemy_level_above_max() {
        assert_eq!(clamp_enemy_level(100), MAX_ENEMY_LEVEL);
        assert_eq!(clamp_enemy_level(85), 85);
        assert_eq!(clamp_enemy_level(84), 84);
    }

    #[test]
    fn default_for_char_level_100_gives_max() {
        let row = MonsterScalingRow::default_for_char_level(100);
        assert_eq!(row.level, MAX_ENEMY_LEVEL);
    }

    #[test]
    fn default_for_char_level_60() {
        let row = MonsterScalingRow::default_for_char_level(60);
        assert_eq!(row.level, 60);
        assert_eq!(row.accuracy, monster_accuracy(60));
    }

    // EnemyTier defaults

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

    // EnemyTierDefaults aggregation

    #[test]
    fn tier_defaults_pinnacle_lv85() {
        let d = EnemyTierDefaults::compute(85, EnemyTier::Pinnacle);
        assert_eq!(d.level, 85);
        // accuracy = data.monsterAccuracyTable[85] = 2357
        assert_eq!(d.accuracy, 2357);
        // evasion = data.monsterEvasionTable[85] * PINNACLE_EVASION_MEAN/100 = 996 * 1.24909
        assert!((d.evasion - 996.0 * PINNACLE_EVASION_MEAN / 100.0).abs() < 0.5);
        // armour = data.monsterArmourTable[85] * PINNACLE_ARMOUR_MEAN/100 = 6355 * 1.5
        assert!((d.armour - 6355.0 * PINNACLE_ARMOUR_MEAN / 100.0).abs() < 0.5);
        assert_eq!(d.elemental_resist, 50.0);
        assert!((d.pen - 3.0).abs() < 1e-9);
        assert_eq!(d.damage_taken_more, 0.0);
    }

    #[test]
    fn tier_defaults_uber_enforces_min_level() {
        // pass in 70, but Uber requires at least 82
        let d = EnemyTierDefaults::compute(70, EnemyTier::Uber);
        assert_eq!(d.level, 82, "Uber enforces min_level=82");
        assert_eq!(d.damage_taken_more, -70.0);
        assert!((d.pen - 8.0).abs() < 1e-9);
    }

    #[test]
    fn tier_defaults_none_lv60() {
        let d = EnemyTierDefaults::compute(60, EnemyTier::None);
        assert_eq!(d.level, 60);
        assert_eq!(d.accuracy, monster_accuracy(60));
        assert_eq!(d.elemental_resist, 0.0);
        assert_eq!(d.pen, 0.0);
        assert_eq!(d.damage_taken_more, 0.0);
        // armour has no multiplier bonus
        assert!((d.armour - monster_armour(60) as f64).abs() < 1e-6);
    }

    #[test]
    fn tier_defaults_boss_lv82() {
        let d = EnemyTierDefaults::compute(82, EnemyTier::Boss);
        assert_eq!(d.level, 82);
        assert_eq!(d.elemental_resist, 30.0);
        assert_eq!(d.pen, 0.0);
        // armour = data.monsterArmourTable[82] * 100% = 5375
        assert!((d.armour - 5375.0).abs() < 1e-6);
    }

    #[test]
    fn scaling_row_at_level_20_spot_check() {
        let row = MonsterScalingRow::at_level(20);
        assert_eq!(row.accuracy, 140, "lv20 acc from agent-docs table");
        assert_eq!(row.evasion, 160, "lv20 evasion");
        assert_eq!(row.armour, 89, "lv20 armour");
        assert_eq!(row.life, 249, "lv20 life");
        assert!((row.damage - 37.29).abs() < 0.01, "lv20 damage");
    }

    #[test]
    fn scaling_row_at_level_clamp() {
        // a row above MAX_ENEMY_LEVEL clamps its level to MAX_ENEMY_LEVEL
        let row = MonsterScalingRow::at_level(999);
        assert_eq!(row.level, MAX_ENEMY_LEVEL);
    }

    // Ailment threshold lookup-table tests
    // Reference: PoB2 src/Data/Misc.lua data.monsterAilmentThresholdTable (Lua 1-indexed)

    #[test]
    fn ailment_threshold_lv1() {
        // PoB2 monsterAilmentThresholdTable[1] = 15
        assert_eq!(enemy_ailment_threshold(1), 15, "lv1 ailment threshold");
    }

    #[test]
    fn ailment_threshold_lv10() {
        // PoB2 monsterAilmentThresholdTable[10] = 70
        assert_eq!(enemy_ailment_threshold(10), 70, "lv10 ailment threshold");
    }

    #[test]
    fn ailment_threshold_lv60() {
        // PoB2 monsterAilmentThresholdTable[60] = 6959
        assert_eq!(enemy_ailment_threshold(60), 6959, "lv60 ailment threshold");
    }

    #[test]
    fn ailment_threshold_lv64_to_lv65_jump() {
        // lv64=9193, lv65=9649 (EndgameStartLevel is where the accelerated growth begins)
        let lv64 = enemy_ailment_threshold(64);
        let lv65 = enemy_ailment_threshold(65);
        assert_eq!(lv64, 9193, "lv64 ailment threshold");
        assert_eq!(lv65, 9649, "lv65 ailment threshold");
        // lv69->lv70 big jump (12181 -> 18272)
        let lv69 = enemy_ailment_threshold(69);
        let lv70 = enemy_ailment_threshold(70);
        assert_eq!(lv69, 12181, "lv69 ailment threshold");
        assert_eq!(lv70, 18272, "lv70 ailment threshold");
        assert!(lv70 > lv69 * 140 / 100, "lv70 has major jump vs lv69");
    }

    #[test]
    fn ailment_threshold_lv85() {
        // MAX_ENEMY_LEVEL=85: PoB2 monsterAilmentThresholdTable[85] = 79967
        assert_eq!(
            enemy_ailment_threshold(85),
            79967,
            "lv85 (MAX_ENEMY_LEVEL) ailment threshold"
        );
    }

    #[test]
    fn ailment_threshold_lv100() {
        // PoB2 monsterAilmentThresholdTable[100] = 123287
        assert_eq!(
            enemy_ailment_threshold(100),
            123287,
            "lv100 ailment threshold"
        );
    }

    #[test]
    fn ailment_threshold_clamp_above_100() {
        assert_eq!(enemy_ailment_threshold(200), enemy_ailment_threshold(100));
    }

    #[test]
    fn ailment_threshold_clamp_zero() {
        assert_eq!(enemy_ailment_threshold(0), enemy_ailment_threshold(1));
    }

    // Poise threshold lookup-table tests
    // Reference: PoB2 src/Data/Misc.lua data.monsterPoiseThresholdTable (Lua 1-indexed)

    #[test]
    fn poise_threshold_lv1() {
        // PoB2 monsterPoiseThresholdTable[1] = 30
        assert_eq!(enemy_poise_threshold(1), 30, "lv1 poise threshold");
    }

    #[test]
    fn poise_threshold_lv10() {
        // PoB2 monsterPoiseThresholdTable[10] = 142
        assert_eq!(enemy_poise_threshold(10), 142, "lv10 poise threshold");
    }

    #[test]
    fn poise_threshold_lv60() {
        // PoB2 monsterPoiseThresholdTable[60] = 8386
        assert_eq!(enemy_poise_threshold(60), 8386, "lv60 poise threshold");
    }

    #[test]
    fn poise_threshold_lv64_to_lv65_jump() {
        // lv64=10819, lv65=26703 (EndgameStartLevel big jump)
        let lv64 = enemy_poise_threshold(64);
        let lv65 = enemy_poise_threshold(65);
        assert_eq!(lv64, 10819, "lv64 poise threshold");
        assert_eq!(lv65, 26703, "lv65 poise threshold");
        // Verify jump magnitude (lv65 is roughly 2.47x lv64)
        assert!(
            lv65 > lv64 * 2,
            "lv65 poise threshold has major jump (lv65={lv65} > 2*lv64={lv64})"
        );
    }

    #[test]
    fn poise_threshold_lv85() {
        // MAX_ENEMY_LEVEL=85: PoB2 monsterPoiseThresholdTable[85] = 271810
        assert_eq!(
            enemy_poise_threshold(85),
            271810,
            "lv85 (MAX_ENEMY_LEVEL) poise threshold"
        );
    }

    #[test]
    fn poise_threshold_lv100() {
        // PoB2 monsterPoiseThresholdTable[100] = 446335
        assert_eq!(enemy_poise_threshold(100), 446335, "lv100 poise threshold");
    }

    #[test]
    fn poise_threshold_clamp_above_100() {
        assert_eq!(enemy_poise_threshold(200), enemy_poise_threshold(100));
    }

    #[test]
    fn poise_threshold_clamp_zero() {
        assert_eq!(enemy_poise_threshold(0), enemy_poise_threshold(1));
    }

    // poise threshold > ailment threshold (sanity check for the boss range)
    // In PoB2, poise threshold is used for accumulating debuffs like
    // Freeze/Electrocute, and is significantly higher than ailment threshold
    // in the boss range (past EndgameStartLevel).

    #[test]
    fn poise_gt_ailment_at_endgame() {
        // Past lv65 (EndgameStartLevel), poise threshold should be significantly above ailment threshold
        for lv in [65u32, 70, 75, 80, 85] {
            let at = enemy_ailment_threshold(lv);
            let pt = enemy_poise_threshold(lv);
            assert!(
                pt > at,
                "lv{lv}: poise_threshold({pt}) should exceed ailment_threshold({at})"
            );
        }
    }

    // chill_minimum_threshold helper function

    #[test]
    fn chill_minimum_threshold_lv85() {
        // effective_threshold = 79967 * 1.0 (no mod) = 79967.0
        // chill_minimum_threshold = 79967.0 / 100.0 = 799.67
        let effective = enemy_ailment_threshold(85) as f64;
        let min_thresh = chill_minimum_threshold(effective);
        assert!(
            (min_thresh - 799.67).abs() < 0.01,
            "lv85 chill min threshold ≈ 799.67, got {min_thresh}"
        );
    }

    #[test]
    fn chill_minimum_threshold_with_mod() {
        // effective_threshold = 79967 * 1.1 (EnemyAilmentThreshold +10%)
        let effective = enemy_ailment_threshold(85) as f64 * 1.1;
        let min_thresh = chill_minimum_threshold(effective);
        assert!(
            (min_thresh - 799.67 * 1.1).abs() < 0.1,
            "lv85 chill min threshold with +10% mod ≈ {:.2}, got {min_thresh:.2}",
            799.67 * 1.1
        );
    }

    // Game constant verification

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
