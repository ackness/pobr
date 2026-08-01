//! Global game constants domain schema (`base/game_constants.json`).
//!
//! Three sections: `character` (player-inherent constants) / `monster`
//! (monster-inherent constants) / `game` (mechanic formula magic numbers).
//!
//! Value sourcing (a migration invariant, architecture doc §1.1):
//! - **pobr's own source of truth**: `crates/pobr-data/src/constants.rs`
//!   (top-level consts + `GameConstants::poe2()`'s default set) — the JSON
//!   must be value-equal to it field by field;
//! - **vendor-only** (values pobr's existing Rust doesn't have): extracted
//!   from `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` (auto-exported from
//!   GameConstants.dat / Character.ot / Monster.ot) and
//!   `src/Modules/Data.lua` (the `data.misc` magic-number table, L171-250),
//!   with a file:line-number note on each field.
//!
//! Note the L4 brake: the enum and struct types in `constants.rs`
//! (DamageType / AilmentType / DamageRange / SkillCost, etc.) are PoB
//! internal semantics and stay in Rust, not migrated; this table only
//! migrates plain numeric values.

use serde::{Deserialize, Serialize};

/// Top-level structure of `base/game_constants.json`: the three global
/// constant sections character/monster/game.
///
/// `Default` = the fallback (the combination of each section's own
/// `Default`, value-equal to the stored JSON field by field — see the note
/// at the end of this file).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GameConstantsDef {
    /// Player-inherent constants section (vendor `data.characterConstants`,
    /// sourced from Character.ot).
    pub character: CharacterConstantsDef,
    /// Monster-inherent constants section (vendor `data.monsterConstants`,
    /// sourced from Monster.ot).
    pub monster: MonsterConstantsDef,
    /// Mechanic formula magic numbers section (vendor `data.gameConstants` +
    /// `data.misc`).
    pub game: GameMechanicsConstantsDef,
}

/// The character section: player-inherent constants.
///
/// Note: level/attribute-derived constants (life_per_level, etc.) live in
/// `overlay/character_constants.json` (architecture doc §3.2), not here;
/// this section only holds the player baselines consumed directly by calc
/// mechanic formulas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterConstantsDef {
    /// Default max resistance (%). pobr's source of truth is
    /// `constants.rs::DEFAULT_MAX_RESISTANCE` = 75; vendor Misc.lua:145
    /// `base_maximum_all_resistances_%` = 75.
    pub base_maximum_all_resistances_pct: f64,
    /// Base crit damage bonus for player/minions (+100%). pobr's source of
    /// truth is `constants.rs::PLAYER_BASE_CRIT_DAMAGE_BONUS` = 100; vendor
    /// Misc.lua:156 `base_critical_hit_damage_bonus` = 100.
    pub base_critical_hit_damage_bonus: f64,
    /// vendor-only: player physical damage reduction cap (%). Misc.lua:146
    /// `maximum_physical_damage_reduction_%` = 90 (Data.lua:178
    /// DamageReductionCap).
    pub maximum_physical_damage_reduction_pct: f64,
    /// vendor-only: energy shield recharge rate (%/min). Misc.lua:143
    /// `energy_shield_recharge_rate_per_minute_%` = 750.
    pub energy_shield_recharge_rate_per_minute_pct: f64,
    /// vendor-only: energy shield recharge delay (seconds). Data.lua:197
    /// EnergyShieldRechargeDelay = 4.
    pub energy_shield_recharge_delay_seconds: f64,
    /// vendor-only: inherent mana regeneration rate (%/min). Misc.lua:144
    /// `character_inherent_mana_regeneration_rate_per_minute_%` = 240.
    pub mana_regeneration_rate_per_minute_pct: f64,
}

/// The monster section: monster-inherent constants (the per-level growth
/// tables live in `base/monster_scaling.json`, not here).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonsterConstantsDef {
    /// vendor-only: monster default max resistance (%). Misc.lua:248
    /// `base_maximum_all_resistances_%` = 75 (Data.lua:200 EnemyMaxResist).
    pub base_maximum_all_resistances_pct: f64,
    /// vendor-only: monster physical damage reduction cap (%). Misc.lua:247
    /// = 75 (Data.lua:179 EnemyPhysicalDamageReductionCap).
    pub maximum_physical_damage_reduction_pct: f64,
    /// vendor-only: monster base crit damage bonus (+30%). Misc.lua:250.
    pub base_critical_hit_damage_bonus: f64,
    /// vendor-only: melee-hit stun multiplier (+%). Misc.lua:258 = 33
    /// (Data.lua:221 MeleeStunMult = 33/100).
    pub melee_hit_stun_multiplier_pct: f64,
    /// vendor-only: physical-hit stun multiplier (+%). Misc.lua:259 = 100
    /// (Data.lua:222 PhysicalStunMult = 100/100).
    pub physical_hit_stun_multiplier_pct: f64,
}

/// The game section: mechanic formula magic numbers (resistance boundary /
/// armour / server tick / ailment baseline / stun / cap families).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameMechanicsConstantsDef {
    // pobr's own source of truth (constants.rs, value-equal)
    /// Resistance hard cap (%). pobr `HARD_MAX_RESISTANCE` = 90; vendor
    /// Data.lua:181 MaxResistCap.
    pub resist_hard_cap: f64,
    /// Resistance floor. pobr `RESIST_FLOOR` = -200; vendor Data.lua:180
    /// ResistFloor.
    pub resist_floor: f64,
    /// Non-channelling action server tick time (seconds). pobr
    /// `SERVER_TICK_SECONDS` = 0.033; vendor Data.lua:172 ServerTickTime
    /// (ServerTickRate = 1/0.033 is a derived value, not stored).
    pub server_tick_seconds: f64,
    /// PoE2 armour coefficient. pobr `ARMOUR_RATIO` = 10; vendor
    /// Data.lua:193 ArmourRatio.
    pub armour_ratio: f64,
    /// Block chance hard cap (%). pobr `BLOCK_CHANCE_CAP` = 90; vendor
    /// Data.lua:185 BlockChanceCap.
    pub block_chance_cap: f64,
    /// Global DoT DPS cap ((2^31-1)/60). pobr `DOT_DPS_CAP` = 35791394;
    /// vendor Data.lua:202 DotDpsCap.
    pub dot_dps_cap: f64,
    /// Shock's minimum effective magnitude (%). pobr `SHOCK_MIN_EFFECT` =
    /// 20; vendor Misc.lua:75 BaseShockMagnitude = 20.
    pub shock_min_effect: f64,
    /// Default shock increased-damage-taken magnitude (fraction). pobr
    /// `GameConstants::poe2().shock_default_effect` = 0.2 (=
    /// SHOCK_MIN_EFFECT/100; vendor expresses it as percent-scale 20 — same
    /// semantics, different scale).
    pub shock_default_effect: f64,
    /// Bleed's base magnitude as a fraction of pre-mitigation physical hit
    /// damage (per second). pobr 0.15; vendor Misc.lua:86
    /// BleedingHitDamagePercentPerMinute = 900 (/60/100 = 0.15, Data.lua:203
    /// BleedPercentBase).
    pub bleed_base_fraction: f64,
    /// Bleed's base duration (seconds). pobr 5.0; vendor Misc.lua:90
    /// BaseBleedingDuration = 5.
    pub bleed_base_duration: f64,
    /// Ignite's base magnitude fraction (per second). pobr 0.20; vendor
    /// Misc.lua:87 IgniteHitDamagePercentPerMinute = 1200 (/60/100 = 0.2,
    /// Data.lua:207 IgnitePercentBase).
    pub ignite_base_fraction: f64,
    /// Ignite's base duration (seconds). pobr 4.0; vendor Misc.lua:96
    /// BaseIgniteDuration = 4.
    pub ignite_base_duration: f64,
    /// Poison's base magnitude fraction (per second). pobr 0.20; vendor
    /// Misc.lua:88 PoisonHitDamagePercentPerMinute = 1200 (/60/100 = 0.2,
    /// Data.lua:205 PoisonPercentBase).
    pub poison_base_fraction: f64,
    /// Poison's base duration (seconds). pobr 2.0; vendor Misc.lua:95
    /// BasePoisonDuration = 2.
    pub poison_base_duration: f64,

    // vendor-only (pobr's existing Rust has no such value)
    /// Evade-chance cap (%). Misc.lua:110
    /// DefaultMaxEvadeChancePercent = 95 (Data.lua:182 EvadeChanceCap).
    pub evade_chance_cap: f64,
    /// Deflection-chance cap (%). Data.lua:183 DeflectionChanceCap = 95.
    pub deflection_chance_cap: f64,
    /// Deflect damage-reduction magnitude (%). Misc.lua:111
    /// BasePercentDamageDeflected = 40 (Data.lua:188 DeflectEffect).
    pub deflect_effect: f64,
    /// Dodge-roll chance cap (%). Data.lua:184 DodgeChanceCap = 75.
    pub dodge_chance_cap: f64,
    /// Avoid-chance cap (%). Data.lua:189 AvoidChanceCap = 75.
    pub avoid_chance_cap: f64,
    /// Spell suppression chance cap (%). Data.lua:186
    /// SuppressionChanceCap = 100.
    pub suppression_chance_cap: f64,
    /// Spell suppression damage-reduction magnitude (%). Data.lua:187
    /// SuppressionEffect = 50.
    pub suppression_effect: f64,
    /// Distance where accuracy falloff starts (distance units). Data.lua:190
    /// AccuracyFalloffStart = 20.
    pub accuracy_falloff_start: f64,
    /// Distance where accuracy falloff ends. Data.lua:191
    /// AccuracyFalloffEnd = 90.
    pub accuracy_falloff_end: f64,
    /// Accuracy penalty magnitude at max distance (%). Data.lua:192
    /// MaxAccuracyRangePenalty = 90 (= -Misc.lua:201
    /// `accuracy_rating_+%_final_at_max_distance_scaled` = -(-90)).
    pub max_accuracy_range_penalty: f64,
    /// Chill's max effect (%). Misc.lua:77 ChillMaxEffect = 50.
    pub chill_max_effect: f64,
    /// Chill effect multiplier (%). Misc.lua:76 ChillEffectMultiplier = 100.
    pub chill_effect_multiplier: f64,
    /// Low-life/low-mana threshold (fraction of the pool). Data.lua:175
    /// LowPoolThreshold = 0.35 (the decimal form of Misc.lua:129
    /// DefaultLowStatusThresholdPercent = 35).
    pub low_pool_threshold: f64,
    /// Base leech rate (fraction of the pool per second). Data.lua:201
    /// LeechRateBase = 0.02.
    pub leech_rate_base: f64,
    /// Effective max damage for leech. Misc.lua:130
    /// EffectiveMaxDamageForLeech = 40000.
    pub effective_max_damage_for_leech: f64,
    /// Culling strike threshold — normal monsters (% life remaining).
    /// Misc.lua:104 CullingStrikeNormalThreshold = 35.
    pub culling_strike_normal_threshold: f64,
    /// Culling strike threshold — magic monsters. Misc.lua:105
    /// CullingStrikeMagicThreshold = 20.
    pub culling_strike_magic_threshold: f64,
    /// Culling strike threshold — rare monsters. Misc.lua:106
    /// CullingStrikeRareThreshold = 10.
    pub culling_strike_rare_threshold: f64,
    /// Culling strike threshold — unique monsters. Misc.lua:107
    /// CullingStrikeUniqueThreshold = 5.
    pub culling_strike_unique_threshold: f64,
    /// Minimum stun chance needed for a stun to apply at all (%).
    /// Data.lua:217 MinStunChanceNeeded = 20.
    pub min_stun_chance_needed: f64,
    /// Stun base multiplier. Data.lua:218 StunBaseMult = 200.
    pub stun_base_mult: f64,
    /// Stun base duration (seconds). Data.lua:219 StunBaseDuration = 0.5
    /// (Misc.lua:220 `stun_base_duration_override_ms` = 500 / 1000).
    pub stun_base_duration_seconds: f64,
    /// Light stun minimum chance — against players (%). Misc.lua:46
    /// LightStunMinimumChancePlayer = 15.
    pub light_stun_minimum_chance_player: f64,
    /// Light stun ratio scale — against players. Misc.lua:47
    /// LightStunRatioScalePlayer = 44.
    pub light_stun_ratio_scale_player: f64,
    /// Light stun minimum chance — against monsters (%). Misc.lua:44
    /// LightStunMinimumChance = 15.
    pub light_stun_minimum_chance: f64,
    /// Light stun ratio scale — against monsters. Misc.lua:45
    /// LightStunRatioScale = 58.
    pub light_stun_ratio_scale: f64,
    /// Heavy stun damage scale — against players. Misc.lua:51
    /// HeavyStunDamageScalePlayer = 0.65.
    pub heavy_stun_damage_scale_player: f64,
    /// Heavy stun threshold modifier — against players. Misc.lua:52
    /// HeavyStunThresholdModifierPlayer = 100.
    pub heavy_stun_threshold_modifier_player: f64,
    /// Heavy stun modifier duration — against players (seconds).
    /// Misc.lua:53 HeavyStunModifierDurationPlayer = 10.
    pub heavy_stun_modifier_duration_player: f64,
    /// Heavy stun damage scale — against monsters. Misc.lua:48
    /// HeavyStunDamageScale = 0.58.
    pub heavy_stun_damage_scale: f64,
    /// Heavy stun threshold modifier — against monsters. Misc.lua:49
    /// HeavyStunThresholdModifier = 500.
    pub heavy_stun_threshold_modifier: f64,
    /// Heavy stun modifier duration — against monsters (seconds).
    /// Misc.lua:50 HeavyStunModifierDuration = 16.5.
    pub heavy_stun_modifier_duration: f64,
    /// Negative-armour damage-bonus cap (%). Data.lua:194
    /// NegArmourDmgBonusCap = 100.
    pub neg_armour_dmg_bonus_cap: f64,

    //  EHP loop magic numbers + normal-monster DPS multiplier (vendor-only,
    //  Data.lua:228-239). `#[serde(default)]`: falls back to the same
    //  Default value when old JSON is missing the field (schema backward
    //  compatibility).
    /// EHP loop per-hit damage cap (a precision upper bound). Data.lua:237
    /// ehpCalcMaxDamage = 100000000.
    #[serde(default = "default_ehp_calc_max_damage")]
    pub ehp_calc_max_damage: f64,
    /// EHP loop max iteration count (exceeding it underestimates high EHP).
    /// Data.lua:239 ehpCalcMaxIterationsToCalc = 50.
    #[serde(default = "default_ehp_calc_max_iterations")]
    pub ehp_calc_max_iterations: f64,
    /// EHP recursion speed-up factor (the consumer caps it at 4 during
    /// loss-prevention). Data.lua:235 ehpCalcSpeedUp = 8.
    #[serde(default = "default_ehp_calc_speed_up")]
    pub ehp_calc_speed_up: f64,
    /// Normal-monster DPS multiplier (used to assemble the enemy's
    /// incoming-damage placeholder, = 1/4.40). Data.lua:228
    /// normalEnemyDPSMult.
    ///
    /// Note: stored in JSON as `0.227272727272727265` — this decimal string
    /// parses under serde_json (default feature, no float_roundtrip) to
    /// exactly the same f64, bit for bit, as Rust's `1.0/4.40`; a naive
    /// 17-digit short representation would be off by 1 ULP. The bit-for-bit
    /// equality is locked in by the gamedata load test.
    #[serde(default = "default_normal_enemy_dps_mult")]
    pub normal_enemy_dps_mult: f64,

    //  max-hit conversion smoothing iteration count (vendor-only, Data.lua:241).
    /// Iteration cap for smoothing across multiple max-hit conversion
    /// branches (`useConversionSmoothing`). Data.lua:241
    /// maxHitSmoothingPasses = 8 (consumed by CalcDefence.lua:3669).
    #[serde(default = "default_max_hit_smoothing_passes")]
    pub max_hit_smoothing_passes: f64,

    //  Block panel family (vendor-only).
    /// Base block chance cap (%, the character-inherent BASE for
    /// `BaseBlockChanceMax`). Misc.lua:147
    /// `object_inherent_base_maximum_block_%_from_ot` = 50 (injected as a
    /// `BaseBlockChanceMax` BASE by CalcSetup.lua:28).
    #[serde(default = "default_base_block_chance_max")]
    pub base_block_chance_max: f64,

    //  charm merging (vendor-only).
    /// Cap on how many charms can be active at once (charm limit cap).
    /// CalcPerform.lua:1589's literal 3 in
    /// `m_min(Override(CharmLimit) or Sum(BASE CharmLimit), 3)` (consumed by
    /// `merge_flasks_charms`).
    #[serde(default = "default_charm_limit_cap")]
    pub charm_limit_cap: f64,

    //  debuff duration multiplier band (vendor-only).
    /// Floor for the enemy-side `BuffExpireFaster` aggregate (Data.lua:177
    /// `BuffExpirationSlowCap = 0.25`): `debuffDurationMult =
    /// 1 / max(cap, calcLib.mod(enemyDB, "BuffExpireFaster"))`
    /// (CalcOffence.lua:1833-1835 / :5040 — the Temporal Chains
    /// expire-slower family can stretch debuff/ailment duration by up to
    /// 4x).
    #[serde(default = "default_buff_expiration_slow_cap")]
    pub buff_expiration_slow_cap: f64,
}

// serde default functions (same source as the `Default` impl's values, a
// single numeric source of truth).
fn default_ehp_calc_max_damage() -> f64 {
    100_000_000.0
}
fn default_base_block_chance_max() -> f64 {
    50.0
}
fn default_charm_limit_cap() -> f64 {
    3.0
}
fn default_buff_expiration_slow_cap() -> f64 {
    0.25
}
fn default_ehp_calc_max_iterations() -> f64 {
    50.0
}
fn default_ehp_calc_speed_up() -> f64 {
    8.0
}
fn default_normal_enemy_dps_mult() -> f64 {
    1.0 / 4.40
}
fn default_max_hit_smoothing_passes() -> f64 {
    8.0
}

// Default = fallback values
//
// Semantics: `Default` is "the fallback constant set used when no GameData
// is injected", and must be value-equal field by field to
// `data/<version>/base/game_constants.json` (locked in by the default
// comparison test in `crates/pobr-gamedata/tests/load_game_constants.rs`).
// Fields that have a pobr Rust source of truth **reference it directly** —
// `crate::constants` / `crate::monster` consts or
// `GameConstants::poe2()` fields (a single numeric source of truth, no
// duplicated literals); vendor-only fields (values pobr's old Rust doesn't
// have) are stored as literals, with the source noted in each field's doc.

impl Default for CharacterConstantsDef {
    fn default() -> Self {
        Self {
            base_maximum_all_resistances_pct: crate::constants::DEFAULT_MAX_RESISTANCE,
            base_critical_hit_damage_bonus: crate::constants::PLAYER_BASE_CRIT_DAMAGE_BONUS,
            // vendor-only (Misc.lua:146 / Data.lua:178 DamageReductionCap).
            maximum_physical_damage_reduction_pct: 90.0,
            // vendor-only (Misc.lua:143).
            energy_shield_recharge_rate_per_minute_pct: 750.0,
            // vendor-only (Data.lua:197 EnergyShieldRechargeDelay).
            energy_shield_recharge_delay_seconds: 4.0,
            // vendor-only (Misc.lua:144).
            mana_regeneration_rate_per_minute_pct: 240.0,
        }
    }
}

impl Default for MonsterConstantsDef {
    fn default() -> Self {
        Self {
            base_maximum_all_resistances_pct: crate::monster::ENEMY_MAX_RESIST,
            // vendor-only (Misc.lua:247 / Data.lua:179).
            maximum_physical_damage_reduction_pct: 75.0,
            base_critical_hit_damage_bonus: crate::monster::MONSTER_BASE_CRIT_DAMAGE_BONUS,
            // vendor-only (Misc.lua:258 / Data.lua:221).
            melee_hit_stun_multiplier_pct: 33.0,
            // vendor-only (Misc.lua:259 / Data.lua:222).
            physical_hit_stun_multiplier_pct: 100.0,
        }
    }
}

impl Default for GameMechanicsConstantsDef {
    fn default() -> Self {
        // The pobr-source-of-truth fields all come from `GameConstants::poe2()`
        // (which itself already references the top-level consts), guaranteeing
        // bit-for-bit equality with the old Rust path.
        let legacy = crate::constants::GameConstants::poe2();
        Self {
            resist_hard_cap: legacy.resist_hard_cap,
            resist_floor: legacy.resist_floor,
            server_tick_seconds: legacy.server_tick_seconds,
            armour_ratio: legacy.armour_ratio,
            block_chance_cap: legacy.block_chance_cap,
            dot_dps_cap: crate::constants::DOT_DPS_CAP,
            shock_min_effect: crate::constants::SHOCK_MIN_EFFECT,
            shock_default_effect: legacy.shock_default_effect,
            bleed_base_fraction: legacy.bleed_base_fraction,
            bleed_base_duration: legacy.bleed_base_duration,
            ignite_base_fraction: legacy.ignite_base_fraction,
            ignite_base_duration: legacy.ignite_base_duration,
            poison_base_fraction: legacy.poison_base_fraction,
            poison_base_duration: legacy.poison_base_duration,
            // The fields below are vendor-only (see each field's doc above for
            // the Lua file:line source)
            evade_chance_cap: 95.0,
            deflection_chance_cap: 95.0,
            deflect_effect: 40.0,
            dodge_chance_cap: 75.0,
            avoid_chance_cap: 75.0,
            suppression_chance_cap: 100.0,
            suppression_effect: 50.0,
            accuracy_falloff_start: 20.0,
            accuracy_falloff_end: 90.0,
            max_accuracy_range_penalty: 90.0,
            chill_max_effect: 50.0,
            chill_effect_multiplier: 100.0,
            low_pool_threshold: 0.35,
            leech_rate_base: 0.02,
            effective_max_damage_for_leech: 40000.0,
            culling_strike_normal_threshold: 35.0,
            culling_strike_magic_threshold: 20.0,
            culling_strike_rare_threshold: 10.0,
            culling_strike_unique_threshold: 5.0,
            min_stun_chance_needed: 20.0,
            stun_base_mult: 200.0,
            stun_base_duration_seconds: 0.5,
            light_stun_minimum_chance_player: 15.0,
            light_stun_ratio_scale_player: 44.0,
            light_stun_minimum_chance: 15.0,
            light_stun_ratio_scale: 58.0,
            heavy_stun_damage_scale_player: 0.65,
            heavy_stun_threshold_modifier_player: 100.0,
            heavy_stun_modifier_duration_player: 10.0,
            heavy_stun_damage_scale: 0.58,
            heavy_stun_threshold_modifier: 500.0,
            heavy_stun_modifier_duration: 16.5,
            neg_armour_dmg_bonus_cap: 100.0,
            //  EHP loop magic numbers + normal-monster DPS multiplier
            //  (Data.lua:228/235/237/239).
            ehp_calc_max_damage: default_ehp_calc_max_damage(),
            ehp_calc_max_iterations: default_ehp_calc_max_iterations(),
            ehp_calc_speed_up: default_ehp_calc_speed_up(),
            normal_enemy_dps_mult: default_normal_enemy_dps_mult(),
            //  max-hit conversion smoothing iteration count (Data.lua:241).
            max_hit_smoothing_passes: default_max_hit_smoothing_passes(),
            //  Block panel family (Misc.lua:147 / CalcSetup.lua:28).
            base_block_chance_max: default_base_block_chance_max(),
            //  charm limit cap (CalcPerform.lua:1589).
            charm_limit_cap: default_charm_limit_cap(),
            buff_expiration_slow_cap: default_buff_expiration_slow_cap(),
        }
    }
}
