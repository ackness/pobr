//! 全局游戏常数域 schema（`base/game_constants.json`，架构文档 20 §3.1）。
//!
//! 三段结构：`character`（玩家固有常数）/ `monster`（怪物固有常数）/
//! `game`（机制公式魔数）。
//!
//! 数值来源（搬迁不变式，架构 §1.1）：
//! - **pobr 准源**：`crates/pobr-data/src/constants.rs`（顶层常量 +
//!   `GameConstants::poe2()` 默认集）——JSON 必须与之逐值相等；
//! - **vendor-only**（pobr 现有 Rust 没有的数值）：抽取自
//!   `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua`（GameConstants.dat /
//!   Character.ot / Monster.ot 自动导出）与 `src/Modules/Data.lua`
//!   （`data.misc` 魔数表，L171-250），逐字段注明文件:行号。
//!
//! 注意 L4 刹车（架构 §1 P1）：`constants.rs` 里的枚举与结构类型
//! （DamageType / AilmentType / DamageRange / SkillCost 等）是 PoB 内部语义，
//! 留 Rust 不迁；本表只迁纯数值。

use serde::{Deserialize, Serialize};

/// `base/game_constants.json` 顶层结构：character/monster/game 三段全局常数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameConstantsDef {
    /// 玩家固有常数段（vendor `data.characterConstants`，源 Character.ot）。
    pub character: CharacterConstantsDef,
    /// 怪物固有常数段（vendor `data.monsterConstants`，源 Monster.ot）。
    pub monster: MonsterConstantsDef,
    /// 机制公式魔数段（vendor `data.gameConstants` + `data.misc`）。
    pub game: GameMechanicsConstantsDef,
}

/// character 段：玩家固有常数。
///
/// 注意：等级/属性派生常量（life_per_level 等）属
/// `overlay/character_constants.json`（架构 §3.2），不在本表；
/// 本段只放 calc 机制公式直接消费的玩家基线。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterConstantsDef {
    /// 默认最大抗性（%）。pobr 准源 `constants.rs::DEFAULT_MAX_RESISTANCE` = 75；
    /// vendor Misc.lua:145 `base_maximum_all_resistances_%` = 75。
    pub base_maximum_all_resistances_pct: f64,
    /// 玩家/召唤物暴击伤害加成基础值（+100%）。pobr 准源
    /// `constants.rs::PLAYER_BASE_CRIT_DAMAGE_BONUS` = 100；
    /// vendor Misc.lua:156 `base_critical_hit_damage_bonus` = 100。
    pub base_critical_hit_damage_bonus: f64,
    /// vendor-only：玩家物理减伤上限（%）。Misc.lua:146
    /// `maximum_physical_damage_reduction_%` = 90（Data.lua:178 DamageReductionCap）。
    pub maximum_physical_damage_reduction_pct: f64,
    /// vendor-only：能量护盾充能速率（%/min）。Misc.lua:143
    /// `energy_shield_recharge_rate_per_minute_%` = 750。
    pub energy_shield_recharge_rate_per_minute_pct: f64,
    /// vendor-only：能量护盾充能延迟（秒）。Data.lua:197 EnergyShieldRechargeDelay = 4。
    pub energy_shield_recharge_delay_seconds: f64,
    /// vendor-only：固有法力回复速率（%/min）。Misc.lua:144
    /// `character_inherent_mana_regeneration_rate_per_minute_%` = 240。
    pub mana_regeneration_rate_per_minute_pct: f64,
}

/// monster 段：怪物固有常数（百级成长表属 `base/monster_scaling.json`，不在本表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonsterConstantsDef {
    /// vendor-only：怪物默认最大抗性（%）。Misc.lua:248
    /// `base_maximum_all_resistances_%` = 75（Data.lua:200 EnemyMaxResist）。
    pub base_maximum_all_resistances_pct: f64,
    /// vendor-only：怪物物理减伤上限（%）。Misc.lua:247 = 75
    /// （Data.lua:179 EnemyPhysicalDamageReductionCap）。
    pub maximum_physical_damage_reduction_pct: f64,
    /// vendor-only：怪物暴击伤害加成基础值（+30%）。Misc.lua:250。
    pub base_critical_hit_damage_bonus: f64,
    /// vendor-only：近战击中眩晕倍率（+%）。Misc.lua:258 = 33
    /// （Data.lua:221 MeleeStunMult = 33/100）。
    pub melee_hit_stun_multiplier_pct: f64,
    /// vendor-only：物理击中眩晕倍率（+%）。Misc.lua:259 = 100
    /// （Data.lua:222 PhysicalStunMult = 100/100）。
    pub physical_hit_stun_multiplier_pct: f64,
}

/// game 段：机制公式魔数（抗性边界 / 护甲 / 服务器帧 / 异常基线 / 眩晕 / 上限族）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameMechanicsConstantsDef {
    // ---- pobr 准源（constants.rs，逐值相等）----
    /// 抗性硬上限（%）。pobr `HARD_MAX_RESISTANCE` = 90；vendor Data.lua:181 MaxResistCap。
    pub resist_hard_cap: f64,
    /// 抗性下界。pobr `RESIST_FLOOR` = -200；vendor Data.lua:180 ResistFloor。
    pub resist_floor: f64,
    /// 非吟唱动作服务器帧时间（秒）。pobr `SERVER_TICK_SECONDS` = 0.033；
    /// vendor Data.lua:172 ServerTickTime（ServerTickRate = 1/0.033 为派生值，不入库）。
    pub server_tick_seconds: f64,
    /// PoE2 护甲系数。pobr `ARMOUR_RATIO` = 10；vendor Data.lua:193 ArmourRatio。
    pub armour_ratio: f64,
    /// 格挡几率硬上限（%）。pobr `BLOCK_CHANCE_CAP` = 90；vendor Data.lua:185 BlockChanceCap。
    pub block_chance_cap: f64,
    /// 全局 DoT DPS 上限（(2^31-1)/60）。pobr `DOT_DPS_CAP` = 35791394；
    /// vendor Data.lua:202 DotDpsCap。
    pub dot_dps_cap: f64,
    /// 感电最小有效强度（%）。pobr `SHOCK_MIN_EFFECT` = 20；
    /// vendor Misc.lua:75 BaseShockMagnitude = 20。
    pub shock_min_effect: f64,
    /// 默认感电增伤幅度（小数）。pobr `GameConstants::poe2().shock_default_effect` = 0.2
    /// （= SHOCK_MIN_EFFECT/100；vendor 以百分制 20 表示，语义一致仅刻度不同）。
    pub shock_default_effect: f64,
    /// 流血基础 magnitude 占 pre-mitigation 物理命中比例（每秒）。pobr 0.15；
    /// vendor Misc.lua:86 BleedingHitDamagePercentPerMinute = 900
    /// （/60/100 = 0.15，Data.lua:203 BleedPercentBase）。
    pub bleed_base_fraction: f64,
    /// 流血基础持续（秒）。pobr 5.0；vendor Misc.lua:90 BaseBleedingDuration = 5。
    pub bleed_base_duration: f64,
    /// 点燃基础 magnitude 比例（每秒）。pobr 0.20；
    /// vendor Misc.lua:87 IgniteHitDamagePercentPerMinute = 1200
    /// （/60/100 = 0.2，Data.lua:207 IgnitePercentBase）。
    pub ignite_base_fraction: f64,
    /// 点燃基础持续（秒）。pobr 4.0；vendor Misc.lua:96 BaseIgniteDuration = 4。
    pub ignite_base_duration: f64,
    /// 中毒基础 magnitude 比例（每秒）。pobr 0.20；
    /// vendor Misc.lua:88 PoisonHitDamagePercentPerMinute = 1200
    /// （/60/100 = 0.2，Data.lua:205 PoisonPercentBase）。
    pub poison_base_fraction: f64,
    /// 中毒基础持续（秒）。pobr 2.0；vendor Misc.lua:95 BasePoisonDuration = 2。
    pub poison_base_duration: f64,

    // ---- vendor-only（pobr 现有 Rust 无此值）----
    /// 闪避（evade）几率上限（%）。Misc.lua:110 DefaultMaxEvadeChancePercent = 95
    /// （Data.lua:182 EvadeChanceCap）。
    pub evade_chance_cap: f64,
    /// 偏斜（deflect）几率上限（%）。Data.lua:183 DeflectionChanceCap = 95。
    pub deflection_chance_cap: f64,
    /// 偏斜减伤幅度（%）。Misc.lua:111 BasePercentDamageDeflected = 40
    /// （Data.lua:188 DeflectEffect）。
    pub deflect_effect: f64,
    /// 翻滚闪避（dodge）几率上限（%）。Data.lua:184 DodgeChanceCap = 75。
    pub dodge_chance_cap: f64,
    /// 回避（avoid）几率上限（%）。Data.lua:189 AvoidChanceCap = 75。
    pub avoid_chance_cap: f64,
    /// 法术抑制几率上限（%）。Data.lua:186 SuppressionChanceCap = 100。
    pub suppression_chance_cap: f64,
    /// 法术抑制减伤幅度（%）。Data.lua:187 SuppressionEffect = 50。
    pub suppression_effect: f64,
    /// 命中随距离衰减起点（距离单位）。Data.lua:190 AccuracyFalloffStart = 20。
    pub accuracy_falloff_start: f64,
    /// 命中随距离衰减终点。Data.lua:191 AccuracyFalloffEnd = 90。
    pub accuracy_falloff_end: f64,
    /// 最远距离命中惩罚幅度（%）。Data.lua:192 MaxAccuracyRangePenalty = 90
    /// （= -Misc.lua:201 `accuracy_rating_+%_final_at_max_distance_scaled` = -(-90)）。
    pub max_accuracy_range_penalty: f64,
    /// 冰缓最大效果（%）。Misc.lua:77 ChillMaxEffect = 50。
    pub chill_max_effect: f64,
    /// 冰缓效果乘数（%）。Misc.lua:76 ChillEffectMultiplier = 100。
    pub chill_effect_multiplier: f64,
    /// 低血/低魔阈值（占池比例）。Data.lua:175 LowPoolThreshold = 0.35
    /// （Misc.lua:129 DefaultLowStatusThresholdPercent = 35 的小数形式）。
    pub low_pool_threshold: f64,
    /// 偷取基础速率（每秒占池比例）。Data.lua:201 LeechRateBase = 0.02。
    pub leech_rate_base: f64,
    /// 偷取有效伤害上限。Misc.lua:130 EffectiveMaxDamageForLeech = 40000。
    pub effective_max_damage_for_leech: f64,
    /// 终结打击阈值——普通怪（剩余生命 %）。Misc.lua:104 CullingStrikeNormalThreshold = 35。
    pub culling_strike_normal_threshold: f64,
    /// 终结打击阈值——魔法怪。Misc.lua:105 CullingStrikeMagicThreshold = 20。
    pub culling_strike_magic_threshold: f64,
    /// 终结打击阈值——稀有怪。Misc.lua:106 CullingStrikeRareThreshold = 10。
    pub culling_strike_rare_threshold: f64,
    /// 终结打击阈值——传奇怪。Misc.lua:107 CullingStrikeUniqueThreshold = 5。
    pub culling_strike_unique_threshold: f64,
    /// 眩晕几率生效下限（%）。Data.lua:217 MinStunChanceNeeded = 20。
    pub min_stun_chance_needed: f64,
    /// 眩晕基础倍率。Data.lua:218 StunBaseMult = 200。
    pub stun_base_mult: f64,
    /// 眩晕基础持续（秒）。Data.lua:219 StunBaseDuration = 0.5
    /// （Misc.lua:220 `stun_base_duration_override_ms` = 500 / 1000）。
    pub stun_base_duration_seconds: f64,
    /// 轻眩晕最小几率——对玩家（%）。Misc.lua:46 LightStunMinimumChancePlayer = 15。
    pub light_stun_minimum_chance_player: f64,
    /// 轻眩晕比例系数——对玩家。Misc.lua:47 LightStunRatioScalePlayer = 44。
    pub light_stun_ratio_scale_player: f64,
    /// 轻眩晕最小几率——对怪物（%）。Misc.lua:44 LightStunMinimumChance = 15。
    pub light_stun_minimum_chance: f64,
    /// 轻眩晕比例系数——对怪物。Misc.lua:45 LightStunRatioScale = 58。
    pub light_stun_ratio_scale: f64,
    /// 重眩晕伤害系数——对玩家。Misc.lua:51 HeavyStunDamageScalePlayer = 0.65。
    pub heavy_stun_damage_scale_player: f64,
    /// 重眩晕阈值修正——对玩家。Misc.lua:52 HeavyStunThresholdModifierPlayer = 100。
    pub heavy_stun_threshold_modifier_player: f64,
    /// 重眩晕修正持续——对玩家（秒）。Misc.lua:53 HeavyStunModifierDurationPlayer = 10。
    pub heavy_stun_modifier_duration_player: f64,
    /// 重眩晕伤害系数——对怪物。Misc.lua:48 HeavyStunDamageScale = 0.58。
    pub heavy_stun_damage_scale: f64,
    /// 重眩晕阈值修正——对怪物。Misc.lua:49 HeavyStunThresholdModifier = 500。
    pub heavy_stun_threshold_modifier: f64,
    /// 重眩晕修正持续——对怪物（秒）。Misc.lua:50 HeavyStunModifierDuration = 16.5。
    pub heavy_stun_modifier_duration: f64,
    /// 负护甲增伤上限（%）。Data.lua:194 NegArmourDmgBonusCap = 100。
    pub neg_armour_dmg_bonus_cap: f64,
}
