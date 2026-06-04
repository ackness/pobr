//! PoE2 怪物等级缩放表与 Boss 档位枚举。
//!
//! 数据来源：
//! - `src/Data/Misc.lua`（PathOfBuilding-PoE2 dev 分支）—— `data.monsterAccuracyTable` /
//!   `data.monsterEvasionTable` / `data.monsterArmourTable` / `data.monsterLifeTable` /
//!   `data.monsterDamageTable` / `data.monsterAilmentThresholdTable` /
//!   `data.monsterPoiseThresholdTable`（各 100 项，索引 = 怪物等级 1..=100）。
//! - `src/Modules/Data.lua`（同库）—— `data.misc.MaxEnemyLevel = 85`、
//!   `normalEnemyDPSMult`、`stdBossDPSMult`、`pinnacleBossDPSMult`、`uberBossDPSMult`、
//!   `pinnacleBossPen`、`uberBossPen`、`EnemyMaxResist`、`EnemyPhysicalDamageReductionCap`。
//! - `src/Data/Bosses.lua`（同库）—— boss `armourMult`/`evasionMult`/`isUber` 列表；
//!   `bossStats.PinnacleArmourMean`/`PinnacleEvasionMean`/`UberArmourMean`/`UberEvasionMean`
//!   由该列表均值计算（见下文常量注释；**当前为 PoE1 遗留 boss 列表，属占位数据**）。
//!
//! 架构约定：
//! - 本模块仅含纯数据定义与查表逻辑，**零 I/O、零 async**。
//! - 查表函数以 `level: u32`（1..=100）为参数，超界 clamp 到表边界。
//! - 所有对接 Step-2（`setup_env`）的接口见 [`MonsterScalingRow`] 与 [`EnemyTierDefaults`]。
//! - 异常阈值查表接口见 [`enemy_ailment_threshold`] 与 [`enemy_poise_threshold`]。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 默认最大怪物等级上限（PoB2 `data.misc.MaxEnemyLevel = 85`）。
/// `enemyLevel` 默认值 = `min(MAX_ENEMY_LEVEL, char_level)`。
pub const MAX_ENEMY_LEVEL: u32 = 85;

/// 怪物精准值查表大小（1..=100，Lua 表共 100 项）。
pub const MONSTER_TABLE_LEN: usize = 100;

/// 怪物最大抗性（PoB2 `EnemyMaxResist = data.monsterConstants["base_maximum_all_resistances_%"] = 75`）。
pub const ENEMY_MAX_RESIST: f64 = 75.0;

/// 怪物物理减伤上限（PoB2 `EnemyPhysicalDamageReductionCap =
/// data.monsterConstants["maximum_physical_damage_reduction_%"] = 75`）。
pub const ENEMY_PHYS_DMGRED_CAP: f64 = 75.0;

/// 怪物基础爆伤加成（PoB2 `data.monsterConstants["base_critical_hit_damage_bonus"] = 30`）。
pub const MONSTER_BASE_CRIT_DAMAGE_BONUS: f64 = 30.0;

/// 普通怪 DPS 倍率（PoB2 `normalEnemyDPSMult = 1/4.40`，用于 EHP 计算）。
pub const NORMAL_ENEMY_DPS_MULT: f64 = 1.0 / 4.40;

/// 标准 Boss DPS 倍率（PoB2 `stdBossDPSMult = 4/4.40`）。
pub const STD_BOSS_DPS_MULT: f64 = 4.0 / 4.40;

/// Pinnacle Boss DPS 倍率（PoB2 `pinnacleBossDPSMult = 8/4.40`）。
pub const PINNACLE_BOSS_DPS_MULT: f64 = 8.0 / 4.40;

/// Uber Boss DPS 倍率（PoB2 `uberBossDPSMult = 10/4.25`）。
pub const UBER_BOSS_DPS_MULT: f64 = 10.0 / 4.25;

/// Pinnacle Boss 元素穿透（PoB2 `pinnacleBossPen = 15/5 = 3`）。
pub const PINNACLE_BOSS_PEN: f64 = 15.0 / 5.0;

/// Uber Boss 元素穿透（PoB2 `uberBossPen = 40/5 = 8`）。
pub const UBER_BOSS_PEN: f64 = 40.0 / 5.0;

/// Pinnacle/Uber boss 的默认最低等级（PoB2 ConfigOptions.lua：`m_max(配置, 82)`）。
pub const PINNACLE_MIN_LEVEL: u32 = 82;

// Boss 护甲/闪避均值倍率（基于 Bosses.lua 列表均值，详见 Data.lua 计算逻辑）。
// 注意：当前 Bosses.lua 为 PoE1 遗留 boss 列表（Shaper/Sirus/Maven 等），
// PoB2 尚未替换为 PoE2 boss，此处为占位数据。
// 计算过程（22 个 boss，7 个 isUber）：
//   pinnacle_armour_mean = 100 + (50+0+0+25+100+0+0+0+0+25+50+50+100+50+75+0+100+75+100+100+100+100)/22
//                        = 100 + 1100/22 = 100 + 50.0 = 150.0
//   pinnacle_evasion_mean = 100 + (0+0+50+0+0+33+33+50+0+50+50+100+0+50+33+33+33+33+0+0+0+0)/22
//                         = 100 + 548/22 ≈ 100 + 24.909 ≈ 124.909
//   uber_armour_mean = 100 + (50+0+0+25+100+0+0)/7 = 100 + 175/7 = 125.0
//   uber_evasion_mean = 100 + (0+0+50+0+0+33+33)/7 = 100 + 116/7 ≈ 116.571

/// Pinnacle Boss 护甲倍率（百分比，100% = 不加成；PoB2 `bossStats.PinnacleArmourMean`）。
pub const PINNACLE_ARMOUR_MEAN: f64 = 150.0;

/// Pinnacle Boss 闪避倍率（百分比；PoB2 `bossStats.PinnacleEvasionMean`）。
pub const PINNACLE_EVASION_MEAN: f64 = 124.909_090_909_090_9;

/// Uber Boss 护甲倍率（百分比；PoB2 `bossStats.UberArmourMean`）。
pub const UBER_ARMOUR_MEAN: f64 = 125.0;

/// Uber Boss 闪避倍率（百分比；PoB2 `bossStats.UberEvasionMean`）。
pub const UBER_EVASION_MEAN: f64 = 116.571_428_571_428_57;

// ---------------------------------------------------------------------------
// 异常阈值相关游戏常量（来自 src/Data/Misc.lua data.gameConstants）
// ---------------------------------------------------------------------------

/// 感电几率倍率：`hitChance = hitAvg / enemyThreshold * SHOCK_CHANCE_MULTIPLIER`。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["ShockChanceMultiplier"] = 25`。
/// wiki 说明："每 4% 阈值伤害 = 1% 感电几率" = `1 / 0.04 = 25`。
pub const SHOCK_CHANCE_MULTIPLIER: f64 = 25.0;

/// 点燃几率倍率：`hitChance = hitAvg / enemyThreshold * IGNITE_CHANCE_MULTIPLIER`。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["IgniteChanceMultiplier"] = 20`。
pub const IGNITE_CHANCE_MULTIPLIER: f64 = 20.0;

/// 冰缓效果倍率（线性缩放）：`chillEffect = CHILL_EFFECT_MULTIPLIER * (damage / threshold) * effectMod`。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["ChillEffectMultiplier"] = 100`。
pub const CHILL_EFFECT_MULTIPLIER: f64 = 100.0;

/// 冰缓最大效果（%行动速度降低）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["ChillMaxEffect"] = 50`。
pub const CHILL_MAX_EFFECT: f64 = 50.0;

/// 冰缓最小效果（默认/最低施加阈值，%行动速度降低）。
///
/// 来源：PoB2 `src/Modules/Data.lua::nonDamagingAilment["Chill"].min = 30`。
/// 0.5.0 注：冰缓最小阈值为 30%（0.5.0 之前为 5%）。
pub const CHILL_MIN_EFFECT: f64 = 30.0;

/// 感电默认/最小效果（%目标受到伤害增加）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["BaseShockMagnitude"] = 20`。
pub const BASE_SHOCK_MAGNITUDE: f64 = 20.0;

/// 感电最大效果上限（%目标受到伤害增加）。
///
/// 来源：PoB2 `src/Modules/Data.lua::nonDamagingAilment["Shock"].max = 100`。
pub const SHOCK_MAX_EFFECT: f64 = 100.0;

/// 冰冻伤害缩放（怪物目标）：`poiseBuildup = FREEZE_DAMAGE_SCALE / enemyPoiseThreshold * ...`。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["FreezeDamageScale"] = 2.1`。
/// 注：玩家目标用 `FreezeDamageScalePlayer = 2.0`（本模块不涉及玩家防御）。
pub const FREEZE_DAMAGE_SCALE: f64 = 2.1;

/// 电击伤害缩放（怪物目标）：`poiseBuildup = ELECTROCUTE_DAMAGE_SCALE / enemyPoiseThreshold * ...`。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["ElectrocuteDamageScale"] = 1.7`。
pub const ELECTROCUTE_DAMAGE_SCALE: f64 = 1.7;

/// 重眩晕伤害缩放（怪物目标）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["HeavyStunDamageScale"] = 0.58`。
pub const HEAVY_STUN_DAMAGE_SCALE: f64 = 0.58;

/// 钉刺伤害缩放（怪物目标）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["PinDamageScale"] = 4.2`。
pub const PIN_DAMAGE_SCALE: f64 = 4.2;

/// Boss 姿态阈值 MORE 修正（%，来自 ConfigOptions.lua `enemyModList:NewMod("PoiseThreshold", "MORE", 500, ...)`）。
///
/// 适用于 Boss/Pinnacle/Uber 档位。已在 `setup_env.rs` 中注入 enemy.mod_db，
/// 此常量仅作文档说明，不在本模块重复使用。
pub const BOSS_POISE_THRESHOLD_MORE: f64 = 500.0;

/// 玩家异常阈值生命比例（PlayerAilmentThreshold = 最大生命 × 此系数）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.gameConstants["PlayerAilmentThresholdLifeFactor"] = 0.5`。
pub const PLAYER_AILMENT_THRESHOLD_LIFE_FACTOR: f64 = 0.5;

// ---------------------------------------------------------------------------
// 查表：怪物精准（monsterAccuracyTable，100 项，来自 DefaultMonsterStats.dat）
// 索引 i 对应怪物等级 i+1；表中已按等级 1-indexed。
// 来源：src/Data/Misc.lua data.monsterAccuracyTable
// ---------------------------------------------------------------------------

/// 怪物精准值查表（等级 1..=100）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterAccuracyTable`（DefaultMonsterStats.dat）。
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

/// 怪物闪避值查表（等级 1..=100）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterEvasionTable`（DefaultMonsterStats.dat）。
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

/// 怪物护甲值查表（等级 1..=100）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterArmourTable`（DefaultMonsterStats.dat）。
/// 注：lv82 处护甲 5081 相对 lv65 的 2276 看似回落，因终局区间另起缩放段，
/// 实际整表照搬自 DefaultMonsterStats.dat，不应线性外推。
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

/// 怪物生命值查表（等级 1..=100）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterLifeTable`（DefaultMonsterStats.dat）。
/// 注：lv65（EndgameStartLevel）及之后有大幅跳升（lv69 起 18272…），用于终局/pinnacle 区间；
/// 照搬整表，勿外推。
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

/// 怪物基础伤害查表（等级 1..=100，f64）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterDamageTable`（DefaultMonsterStats.dat）。
/// 用法：`enemyXDamage = monsterDamageTable[lv] * 1.5 * DPSMult`（EHP 计算用，非玩家 DPS）。
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

// ---------------------------------------------------------------------------
// 查表：怪物异常阈值（monsterAilmentThresholdTable，100 项）
// 用于几率派生型异常（点燃 Ignite / 感电 Shock / 冰缓 Chill 最小阈值）。
// 注：与怪物生命**无关**，独立按等级索引。lv65+ 出现大幅跳升（终局/boss 区间）。
// 来源：src/Data/Misc.lua data.monsterAilmentThresholdTable（PathOfBuilding-PoE2 dev）
// PoB2 CalcOffence.lua:
//   enemyThreshold = data.monsterAilmentThresholdTable[env.enemyLevel] * mod(EnemyAilmentThreshold)
// ---------------------------------------------------------------------------

/// 怪物异常阈值查表（等级 1..=100）。
///
/// 用于点燃/感电的几率派生与冰缓的最小阈值计算。
/// 与怪物生命无关，独立按等级索引；lv65（EndgameStartLevel）起大幅跳升。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterAilmentThresholdTable`。
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

// ---------------------------------------------------------------------------
// 查表：怪物姿态阈值（monsterPoiseThresholdTable，100 项）
// 用于积累型 debuff（冰冻 Freeze / 电击 Electrocute / 重眩晕 HeavyStun / 钉刺 Pin）。
// Boss 在此基础上通过 mod_db PoiseThreshold MORE 500 乘以 5 倍（已在 setup_env.rs 注入）。
// 来源：src/Data/Misc.lua data.monsterPoiseThresholdTable（PathOfBuilding-PoE2 dev）
// PoB2 CalcOffence.lua:
//   enemyPoiseThreshold = floor(monsterPoiseThresholdTable[enemyLevel]
//       * mod(PoiseThreshold, ailment.."Threshold", ...EnemyAilmentThreshold))
// ---------------------------------------------------------------------------

/// 怪物姿态阈值查表（等级 1..=100）。
///
/// 用于冰冻/电击/重眩晕/钉刺的积累量计算。
/// lv65（EndgameStartLevel）起同样出现大幅跳升；Boss 档位需在 mod_db 层额外乘以
/// `PoiseThreshold MORE 500`（已由 `setup_env.rs` 注入，此处返回裸表值）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterPoiseThresholdTable`。
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

// ---------------------------------------------------------------------------
// 查表辅助函数
// ---------------------------------------------------------------------------

/// 将用户输入等级 clamp 到 `[1, MAX_ENEMY_LEVEL]`，返回有效等级。
///
/// 对应 PoB2 `CalcSetup.lua`：
/// ```lua
/// env.enemyLevel = build.configTab.enemyLevel or m_min(data.misc.MaxEnemyLevel, build.characterLevel)
/// ```
#[inline]
pub fn clamp_enemy_level(level: u32) -> u32 {
    level.clamp(1, MAX_ENEMY_LEVEL)
}

/// 将等级 clamp 到表边界 `[1, 100]`（内部工具函数）。
#[inline]
fn level_to_index(level: u32) -> usize {
    (level.clamp(1, MONSTER_TABLE_LEN as u32) - 1) as usize
}

/// 查询怪物精准值（等级 1..=100，超界自动 clamp）。
///
/// 来源：PoB2 `data.monsterAccuracyTable`（DefaultMonsterStats.dat）。
/// 在 `CalcSetup.lua` 中以：
/// ```lua
/// enemyDB:NewMod("Accuracy","BASE", data.monsterAccuracyTable[env.enemyLevel], "Base")
/// ```
/// 的方式注入敌人 ModDB。
pub fn monster_accuracy(level: u32) -> u32 {
    MONSTER_ACCURACY_TABLE[level_to_index(level)]
}

/// 查询怪物闪避值（等级 1..=100，超界自动 clamp）。
pub fn monster_evasion(level: u32) -> u32 {
    MONSTER_EVASION_TABLE[level_to_index(level)]
}

/// 查询怪物护甲值（等级 1..=100，超界自动 clamp）。
pub fn monster_armour(level: u32) -> u32 {
    MONSTER_ARMOUR_TABLE[level_to_index(level)]
}

/// 查询怪物生命值（等级 1..=100，超界自动 clamp）。
pub fn monster_life(level: u32) -> u32 {
    MONSTER_LIFE_TABLE[level_to_index(level)]
}

/// 查询怪物基础伤害（等级 1..=100，超界自动 clamp）。
pub fn monster_damage(level: u32) -> f64 {
    MONSTER_DAMAGE_TABLE[level_to_index(level)]
}

/// 查询怪物异常阈值（裸表值，等级 1..=100，超界自动 clamp）。
///
/// 用途：几率派生型异常（点燃 / 感电）与冰缓最小阈值计算。
///
/// # 完整公式（PoB2 CalcOffence.lua）
/// ```text
/// enemy_threshold = monster_ailment_threshold(level) * mod(EnemyAilmentThreshold)
/// ```
/// `EnemyAilmentThreshold` 来自 mod_db 聚合（词条 / boss 修正等）；
/// 本函数只返回裸表值，调用方负责乘以 mod 倍率。
///
/// # 注意
/// - 与怪物生命**无关**，独立按等级索引。
/// - lv65（EndgameStartLevel）起大幅跳升，用于终局/boss 区间。
/// - Boss 档位的 `EnemyAilmentThreshold` 修正通过 mod_db 词条注入（不在本函数处理）。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterAilmentThresholdTable`。
pub fn enemy_ailment_threshold(level: u32) -> u32 {
    MONSTER_AILMENT_THRESHOLD_TABLE[level_to_index(level)]
}

/// 查询怪物姿态阈值（裸表值，等级 1..=100，超界自动 clamp）。
///
/// 用途：积累型 debuff（冰冻 / 电击 / 重眩晕 / 钉刺）的积累量计算。
///
/// # 完整公式（PoB2 CalcOffence.lua）
/// ```text
/// enemy_poise_threshold = floor(
///     monster_poise_threshold(level)
///     * mod(PoiseThreshold, ailment + "Threshold",
///           [EnemyStunThreshold for HeavyStun],
///           [EnemyAilmentThreshold for Freeze/Electrocute])
/// )
/// ```
/// `PoiseThreshold` 来自 mod_db 聚合（Boss 默认 MORE 500，已在 `setup_env.rs` 注入）；
/// 本函数只返回裸表值，调用方负责乘以 mod 倍率后 floor。
///
/// # 注意
/// - lv65（EndgameStartLevel）起大幅跳升（Boss 区间）。
/// - Boss 的 `PoiseThreshold MORE 500` 已在 `setup_env.rs` 注入 enemy.mod_db，
///   **不在本函数重复处理**。
/// - 冰冻 / 电击额外还吃 `EnemyAilmentThreshold` 修正。
///
/// 来源：PoB2 `src/Data/Misc.lua::data.monsterPoiseThresholdTable`。
pub fn enemy_poise_threshold(level: u32) -> u32 {
    MONSTER_POISE_THRESHOLD_TABLE[level_to_index(level)]
}

/// 计算冰缓最小可施加的阈值（unmitigated cold damage 必须超过此值才算有效冰缓）。
///
/// # 公式（PoB2 CalcOffence.lua）
/// ```text
/// chill_minimum_threshold = enemy_ailment_threshold(level) / CHILL_EFFECT_MULTIPLIER
/// ```
/// 即：打满 1% 阈值伤害 → 冰缓效果 1%（< 30% 被丢弃）；
/// 打满 30% 阈值伤害 → 最小有效冰缓 30%。
/// `enemy_mod(EnemyAilmentThreshold)` 的 mod 倍率已在 `enemy_threshold` 中体现；
/// 此工具函数接受已计算好的 `effective_threshold`（= 裸表值 × mod 倍率）。
///
/// 来源：PoB2 `CalcOffence.lua::chillMinimumThreshold = enemyThreshold / ChillEffectMultiplier`。
pub fn chill_minimum_threshold(effective_threshold: f64) -> f64 {
    effective_threshold / CHILL_EFFECT_MULTIPLIER
}

// ---------------------------------------------------------------------------
// 一行缩放数据的聚合结构（供 Step-2 setup_env 消费）
// ---------------------------------------------------------------------------

/// 怪物某等级的基础属性快照（精准/闪避/护甲/生命/伤害）。
///
/// 用途：Step-2 `setup_env` 可调用 [`MonsterScalingRow::at_level`] 取得一组数值，
/// 然后将其转换为若干 `Modifier`（BASE 类型）注入 `enemy.mod_db`。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MonsterScalingRow {
    /// 怪物等级（已 clamp 到 1..=85）。
    pub level: u32,
    /// 基础精准值。
    pub accuracy: u32,
    /// 基础闪避值。
    pub evasion: u32,
    /// 基础护甲值。
    pub armour: u32,
    /// 基础生命值。
    pub life: u32,
    /// 基础伤害（用于 EHP 计算）。
    pub damage: f64,
}

impl MonsterScalingRow {
    /// 按等级构建缩放行（level 自动 clamp 到 `[1, MAX_ENEMY_LEVEL]`）。
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

    /// 根据角色等级推导默认 `enemyLevel`（PoB2：`min(MaxEnemyLevel, charLevel)`）。
    pub fn default_for_char_level(char_level: u32) -> Self {
        let enemy_level = char_level.min(MAX_ENEMY_LEVEL);
        Self::at_level(enemy_level)
    }
}

// ---------------------------------------------------------------------------
// EnemyTier 枚举
// ---------------------------------------------------------------------------

/// 敌人档位（对应 PoB2 ConfigOptions.lua `enemyIsBoss` 四档）。
///
/// **默认档位为 `Pinnacle`**（PoB2 `defaultIndex = 3`）——PoB2 默认对
/// Guardian/Pinnacle Boss 计算 DPS。
///
/// 来源：`src/Modules/ConfigOptions.lua`（enemyIsBoss），`src/Modules/Data.lua`（bossStats/DPSMult/Pen）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EnemyTier {
    /// 普通怪物，无额外抗性/减伤加成。
    None,
    /// 标准 Boss（地图 Boss 等）：+30% 元素抗。
    Boss,
    /// Pinnacle/守护者 Boss（默认）：+50% 元素抗、护甲/闪避取均值倍率、少量穿透。
    #[default]
    Pinnacle,
    /// Uber Boss：+50% 元素抗、DamageTaken MORE -70%、更高穿透。
    Uber,
}

impl EnemyTier {
    /// 该档位是否为任意 Boss（Boss / Pinnacle / Uber）。
    pub fn is_boss(self) -> bool {
        !matches!(self, EnemyTier::None)
    }

    /// 该档位是否为 Pinnacle 或 Uber（有 `Condition:PinnacleBoss`）。
    pub fn is_pinnacle_or_uber(self) -> bool {
        matches!(self, EnemyTier::Pinnacle | EnemyTier::Uber)
    }

    /// 默认怪物等级下界（Pinnacle/Uber 最低 82；普通/Boss 无下界）。
    ///
    /// 来源：PoB2 ConfigOptions.lua `m_max(配置, 82)`。
    pub fn min_level(self) -> u32 {
        match self {
            EnemyTier::Pinnacle | EnemyTier::Uber => PINNACLE_MIN_LEVEL,
            _ => 1,
        }
    }

    /// 元素抗性加成（%，BASE 数值，注入 `enemy.mod_db` 的 `*Resist BASE`）。
    ///
    /// - None：0
    /// - Boss：+30
    /// - Pinnacle / Uber：+50
    ///
    /// 来源：PoB2 ConfigOptions.lua 各档位 enemy modList。
    pub fn elemental_resist_bonus(self) -> f64 {
        match self {
            EnemyTier::None => 0.0,
            EnemyTier::Boss => 30.0,
            EnemyTier::Pinnacle | EnemyTier::Uber => 50.0,
        }
    }

    /// 混沌抗性加成（%）。当前所有档位均为 0（PoB2 未设置混沌抗 boss 加成）。
    pub fn chaos_resist_bonus(self) -> f64 {
        0.0
    }

    /// 护甲倍率（%，用于 `monsterArmourTable[lv] * armour_mult_pct / 100.0`）。
    ///
    /// - None / Boss：100%（不加成）
    /// - Pinnacle：`PINNACLE_ARMOUR_MEAN`（≈150%，PoE1 boss 均值，占位）
    /// - Uber：`UBER_ARMOUR_MEAN`（≈125%，PoE1 uber boss 均值，占位）
    pub fn armour_mult_pct(self) -> f64 {
        match self {
            EnemyTier::None | EnemyTier::Boss => 100.0,
            EnemyTier::Pinnacle => PINNACLE_ARMOUR_MEAN,
            EnemyTier::Uber => UBER_ARMOUR_MEAN,
        }
    }

    /// 闪避倍率（%，用于 `monsterEvasionTable[lv] * evasion_mult_pct / 100.0`）。
    ///
    /// - None / Boss：100%
    /// - Pinnacle：`PINNACLE_EVASION_MEAN`（≈124.9%）
    /// - Uber：`UBER_EVASION_MEAN`（≈116.6%）
    pub fn evasion_mult_pct(self) -> f64 {
        match self {
            EnemyTier::None | EnemyTier::Boss => 100.0,
            EnemyTier::Pinnacle => PINNACLE_EVASION_MEAN,
            EnemyTier::Uber => UBER_EVASION_MEAN,
        }
    }

    /// 元素穿透（%，来自 boss 自带穿透 modifier，注入 player-side 或 enemy modDB 见 Step-2）。
    ///
    /// - None / Boss：0
    /// - Pinnacle：3（`pinnacleBossPen = 15/5`）
    /// - Uber：8（`uberBossPen = 40/5`）
    pub fn pen(self) -> f64 {
        match self {
            EnemyTier::None | EnemyTier::Boss => 0.0,
            EnemyTier::Pinnacle => PINNACLE_BOSS_PEN,
            EnemyTier::Uber => UBER_BOSS_PEN,
        }
    }

    /// EHP 计算用 DPS 倍率（`monsterDamageTable[lv] * 1.5 * dps_mult()`）。
    pub fn dps_mult(self) -> f64 {
        match self {
            EnemyTier::None => NORMAL_ENEMY_DPS_MULT,
            EnemyTier::Boss => STD_BOSS_DPS_MULT,
            EnemyTier::Pinnacle => PINNACLE_BOSS_DPS_MULT,
            EnemyTier::Uber => UBER_BOSS_DPS_MULT,
        }
    }

    /// 是否有 `DamageTaken MORE -70`（仅 Uber）。
    ///
    /// 来源：PoB2 `enemyModList:NewMod("DamageTaken","MORE",-70,"Boss")`（Uber 档）。
    pub fn has_damage_taken_penalty(self) -> bool {
        matches!(self, EnemyTier::Uber)
    }

    /// `DamageTaken MORE` 值（Uber = -70，其余 = 0）。
    pub fn damage_taken_more(self) -> f64 {
        if self.has_damage_taken_penalty() {
            -70.0
        } else {
            0.0
        }
    }
}

/// 敌人档位对应的默认属性集合（供 Step-2 `setup_env` 消费）。
///
/// 调用 [`EnemyTierDefaults::compute`] 传入 `(level, tier)` 即可得到一组
/// 可直接注入 `enemy.mod_db` 的数值。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnemyTierDefaults {
    /// 实际使用的怪物等级（已 clamp 并考虑 `tier.min_level()`）。
    pub level: u32,
    /// 精准值（BASE，注入 `enemy.mod_db.Accuracy BASE`）。
    pub accuracy: u32,
    /// 闪避值（已乘以档位闪避倍率，BASE，注入 `Evasion BASE`）。
    pub evasion: f64,
    /// 护甲值（已乘以档位护甲倍率，BASE，注入 `Armour BASE`）。
    pub armour: f64,
    /// 生命值（BASE，供 EHP 参考）。
    pub life: u32,
    /// 元素抗性加成（%，三种元素各注入 `{Fire/Cold/Lightning}Resist BASE`）。
    pub elemental_resist: f64,
    /// 混沌抗性加成（%，注入 `ChaosResist BASE`）。
    pub chaos_resist: f64,
    /// 穿透（%，注入 player 侧穿透 modifier，见 Step-2）。
    pub pen: f64,
    /// Uber `DamageTaken MORE` 值（-70 or 0）。
    pub damage_taken_more: f64,
    /// EHP 用基础伤害（已乘以 `1.5 * dps_mult`）。
    pub base_damage_for_ehp: f64,
}

impl EnemyTierDefaults {
    /// 根据（用户配置等级, 档位）计算一组默认属性数值。
    ///
    /// `config_level`：用户在配置中指定的怪物等级（0 = 跟随角色等级，此处传入已计算的值）。
    pub fn compute(config_level: u32, tier: EnemyTier) -> Self {
        // 保证等级满足档位最低要求，并 clamp 到 MAX_ENEMY_LEVEL
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

// ---------------------------------------------------------------------------
// 单测
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- 查表边界 ---

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
        // 超过 100 时 clamp 到 100
        assert_eq!(monster_accuracy(200), monster_accuracy(100));
    }

    #[test]
    fn accuracy_table_clamp_zero() {
        // 0 时 clamp 到 1
        assert_eq!(monster_accuracy(0), monster_accuracy(1));
    }

    #[test]
    fn evasion_table_lv85() {
        // PoB2 Misc.lua data.monsterEvasionTable[85] = 996
        assert_eq!(monster_evasion(85), 996, "lv85 evasion");
    }

    #[test]
    fn armour_table_lv65() {
        // PoB2 Misc.lua data.monsterArmourTable[65] = 2023（EndgameStartLevel = 65）
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
        // 大幅跳升发生在 lv66(7079) → lv70(11148) 之间（EndgameStartLevel = 65）
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

    // --- 默认等级推导 ---

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

    // --- EnemyTier 默认值 ---

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

    // --- EnemyTierDefaults 聚合 ---

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
        // 传入 70，但 Uber 最低 82
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
        // armour 无倍率加成
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
        // 超过 MAX_ENEMY_LEVEL 的 row 等级 == MAX_ENEMY_LEVEL
        let row = MonsterScalingRow::at_level(999);
        assert_eq!(row.level, MAX_ENEMY_LEVEL);
    }

    // -----------------------------------------------------------------------
    // 异常阈值（Ailment Threshold）查表测试
    // 参考：PoB2 src/Data/Misc.lua data.monsterAilmentThresholdTable（Lua 1-indexed）
    // -----------------------------------------------------------------------

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
        // lv64=9193, lv65=9649（EndgameStartLevel 出现的加速增长起点）
        let lv64 = enemy_ailment_threshold(64);
        let lv65 = enemy_ailment_threshold(65);
        assert_eq!(lv64, 9193, "lv64 ailment threshold");
        assert_eq!(lv65, 9649, "lv65 ailment threshold");
        // lv69->lv70 大跳升（12181 -> 18272）
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

    // -----------------------------------------------------------------------
    // 姿态阈值（Poise Threshold）查表测试
    // 参考：PoB2 src/Data/Misc.lua data.monsterPoiseThresholdTable（Lua 1-indexed）
    // -----------------------------------------------------------------------

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
        // lv64=10819, lv65=26703（EndgameStartLevel 大幅跳升）
        let lv64 = enemy_poise_threshold(64);
        let lv65 = enemy_poise_threshold(65);
        assert_eq!(lv64, 10819, "lv64 poise threshold");
        assert_eq!(lv65, 26703, "lv65 poise threshold");
        // 跳升幅度验证（lv65 约是 lv64 的 2.47 倍）
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

    // -----------------------------------------------------------------------
    // 姿态阈值 > 异常阈值（Boss 区间合理性）
    // PoB2 中 poise threshold 用于冰冻/电击等积累型 debuff，
    // 在 boss 区间（EndgameStartLevel 之后）大幅高于 ailment threshold。
    // -----------------------------------------------------------------------

    #[test]
    fn poise_gt_ailment_at_endgame() {
        // lv65（EndgameStartLevel）之后，姿态阈值应显著高于异常阈值
        for lv in [65u32, 70, 75, 80, 85] {
            let at = enemy_ailment_threshold(lv);
            let pt = enemy_poise_threshold(lv);
            assert!(
                pt > at,
                "lv{lv}: poise_threshold({pt}) should exceed ailment_threshold({at})"
            );
        }
    }

    // -----------------------------------------------------------------------
    // chill_minimum_threshold 辅助函数
    // -----------------------------------------------------------------------

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
        // effectivethreshold = 79967 * 1.1（EnemyAilmentThreshold +10%）
        let effective = enemy_ailment_threshold(85) as f64 * 1.1;
        let min_thresh = chill_minimum_threshold(effective);
        assert!(
            (min_thresh - 799.67 * 1.1).abs() < 0.1,
            "lv85 chill min threshold with +10% mod ≈ {:.2}, got {min_thresh:.2}",
            799.67 * 1.1
        );
    }

    // -----------------------------------------------------------------------
    // 游戏常量验证
    // -----------------------------------------------------------------------

    #[test]
    fn game_constants_values() {
        // PoB2 Misc.lua 直接抄录验证
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
}
