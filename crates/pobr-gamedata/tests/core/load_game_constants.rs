//! `base/game_constants.json` 加载测试：
//! - pobr 准源字段逐值断言等于 `pobr_data::constants`（搬迁不变式，架构 §1.1 P8）；
//! - vendor-only 字段抽样断言写死期望值，并引用 vendor 文件:行号。

use pobr_data::catalog::game_constants::GameConstantsDef;
use pobr_data::constants::{
    ARMOUR_RATIO, BLOCK_CHANCE_CAP, DEFAULT_MAX_RESISTANCE, DOT_DPS_CAP, GameConstants,
    HARD_MAX_RESISTANCE, PLAYER_BASE_CRIT_DAMAGE_BONUS, RESIST_FLOOR, SERVER_TICK_SECONDS,
    SHOCK_MIN_EFFECT,
};
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn load() -> GameConstantsDef {
    GameData::new(repo_data_root().join(VERSION))
        .game_constants()
        .expect("game_constants 可加载")
}

/// pobr 准源（constants.rs 顶层常量）逐值相等。
#[test]
fn migrated_top_level_constants_match_rust_source() {
    let gc = load();
    assert_eq!(
        gc.character.base_maximum_all_resistances_pct,
        DEFAULT_MAX_RESISTANCE
    );
    assert_eq!(
        gc.character.base_critical_hit_damage_bonus,
        PLAYER_BASE_CRIT_DAMAGE_BONUS
    );
    assert_eq!(gc.game.resist_hard_cap, HARD_MAX_RESISTANCE);
    assert_eq!(gc.game.resist_floor, RESIST_FLOOR);
    assert_eq!(gc.game.server_tick_seconds, SERVER_TICK_SECONDS);
    assert_eq!(gc.game.armour_ratio, ARMOUR_RATIO);
    assert_eq!(gc.game.block_chance_cap, BLOCK_CHANCE_CAP);
    assert_eq!(gc.game.dot_dps_cap, DOT_DPS_CAP);
    assert_eq!(gc.game.shock_min_effect, SHOCK_MIN_EFFECT);
}

/// pobr 准源（`GameConstants::poe2()` 默认集）逐值相等。
#[test]
fn migrated_poe2_defaults_match_rust_source() {
    let gc = load();
    let rust = GameConstants::poe2();
    assert_eq!(gc.game.resist_hard_cap, rust.resist_hard_cap);
    assert_eq!(gc.game.resist_floor, rust.resist_floor);
    assert_eq!(gc.game.server_tick_seconds, rust.server_tick_seconds);
    assert_eq!(gc.game.armour_ratio, rust.armour_ratio);
    assert_eq!(gc.game.bleed_base_fraction, rust.bleed_base_fraction);
    assert_eq!(gc.game.bleed_base_duration, rust.bleed_base_duration);
    assert_eq!(gc.game.ignite_base_fraction, rust.ignite_base_fraction);
    assert_eq!(gc.game.ignite_base_duration, rust.ignite_base_duration);
    assert_eq!(gc.game.poison_base_fraction, rust.poison_base_fraction);
    assert_eq!(gc.game.poison_base_duration, rust.poison_base_duration);
    assert_eq!(gc.game.shock_default_effect, rust.shock_default_effect);
    assert_eq!(
        gc.character.base_critical_hit_damage_bonus,
        rust.player_base_crit_damage_bonus
    );
    assert_eq!(gc.game.block_chance_cap, rust.block_chance_cap);
    assert_eq!(
        gc.character.base_maximum_all_resistances_pct,
        rust.resist_default_max
    );
}

/// vendor-only 字段抽样断言（期望值写死，来源见各行注释）。
#[test]
fn vendor_only_values_pinned_to_pob2_source() {
    let gc = load();

    // character 段：vendor Data/Misc.lua（Character.ot 导出）。
    // Misc.lua:146 maximum_physical_damage_reduction_% = 90
    assert_eq!(gc.character.maximum_physical_damage_reduction_pct, 90.0);
    // Misc.lua:143 energy_shield_recharge_rate_per_minute_% = 750
    assert_eq!(
        gc.character.energy_shield_recharge_rate_per_minute_pct,
        750.0
    );
    // Modules/Data.lua:197 EnergyShieldRechargeDelay = 4
    assert_eq!(gc.character.energy_shield_recharge_delay_seconds, 4.0);
    // Misc.lua:144 character_inherent_mana_regeneration_rate_per_minute_% = 240
    assert_eq!(gc.character.mana_regeneration_rate_per_minute_pct, 240.0);

    // monster 段：vendor Data/Misc.lua（Monster.ot 导出）。
    // Misc.lua:248 base_maximum_all_resistances_% = 75
    assert_eq!(gc.monster.base_maximum_all_resistances_pct, 75.0);
    // Misc.lua:247 maximum_physical_damage_reduction_% = 75
    assert_eq!(gc.monster.maximum_physical_damage_reduction_pct, 75.0);
    // Misc.lua:250 base_critical_hit_damage_bonus = 30
    assert_eq!(gc.monster.base_critical_hit_damage_bonus, 30.0);
    // Misc.lua:258 / :259 眩晕倍率 33 / 100
    assert_eq!(gc.monster.melee_hit_stun_multiplier_pct, 33.0);
    assert_eq!(gc.monster.physical_hit_stun_multiplier_pct, 100.0);

    // game 段：闪避/偏斜/抑制/回避上限族。
    // Misc.lua:110 DefaultMaxEvadeChancePercent = 95（Data.lua:182 EvadeChanceCap）
    assert_eq!(gc.game.evade_chance_cap, 95.0);
    // Modules/Data.lua:183 DeflectionChanceCap = 95
    assert_eq!(gc.game.deflection_chance_cap, 95.0);
    // Misc.lua:111 BasePercentDamageDeflected = 40（Data.lua:188 DeflectEffect）
    assert_eq!(gc.game.deflect_effect, 40.0);
    // Modules/Data.lua:184 / :189 DodgeChanceCap / AvoidChanceCap = 75
    assert_eq!(gc.game.dodge_chance_cap, 75.0);
    assert_eq!(gc.game.avoid_chance_cap, 75.0);
    // Modules/Data.lua:186-187 SuppressionChanceCap = 100 / SuppressionEffect = 50
    assert_eq!(gc.game.suppression_chance_cap, 100.0);
    assert_eq!(gc.game.suppression_effect, 50.0);

    // game 段：命中衰减（Modules/Data.lua:190-192）。
    assert_eq!(gc.game.accuracy_falloff_start, 20.0);
    assert_eq!(gc.game.accuracy_falloff_end, 90.0);
    assert_eq!(gc.game.max_accuracy_range_penalty, 90.0);

    // game 段：冰缓（Misc.lua:76-77）。
    assert_eq!(gc.game.chill_max_effect, 50.0);
    assert_eq!(gc.game.chill_effect_multiplier, 100.0);

    // game 段：低池阈值 / 偷取（Data.lua:175 / :201、Misc.lua:130）。
    assert_eq!(gc.game.low_pool_threshold, 0.35);
    assert_eq!(gc.game.leech_rate_base, 0.02);
    assert_eq!(gc.game.effective_max_damage_for_leech, 40000.0);

    // game 段：终结打击阈值（Misc.lua:104-107：35/20/10/5）。
    assert_eq!(gc.game.culling_strike_normal_threshold, 35.0);
    assert_eq!(gc.game.culling_strike_magic_threshold, 20.0);
    assert_eq!(gc.game.culling_strike_rare_threshold, 10.0);
    assert_eq!(gc.game.culling_strike_unique_threshold, 5.0);

    // game 段：眩晕全套。
    // Modules/Data.lua:217-219 MinStunChanceNeeded=20 / StunBaseMult=200 / StunBaseDuration=0.5
    assert_eq!(gc.game.min_stun_chance_needed, 20.0);
    assert_eq!(gc.game.stun_base_mult, 200.0);
    assert_eq!(gc.game.stun_base_duration_seconds, 0.5);
    // Misc.lua:44-47 轻眩晕（怪/玩家）：15/58、15/44
    assert_eq!(gc.game.light_stun_minimum_chance, 15.0);
    assert_eq!(gc.game.light_stun_ratio_scale, 58.0);
    assert_eq!(gc.game.light_stun_minimum_chance_player, 15.0);
    assert_eq!(gc.game.light_stun_ratio_scale_player, 44.0);
    // Misc.lua:48-53 重眩晕（怪/玩家）：0.58/500/16.5、0.65/100/10
    assert_eq!(gc.game.heavy_stun_damage_scale, 0.58);
    assert_eq!(gc.game.heavy_stun_threshold_modifier, 500.0);
    assert_eq!(gc.game.heavy_stun_modifier_duration, 16.5);
    assert_eq!(gc.game.heavy_stun_damage_scale_player, 0.65);
    assert_eq!(gc.game.heavy_stun_threshold_modifier_player, 100.0);
    assert_eq!(gc.game.heavy_stun_modifier_duration_player, 10.0);

    // game 段：负护甲增伤上限（Modules/Data.lua:194）。
    assert_eq!(gc.game.neg_armour_dmg_bonus_cap, 100.0);
}

/// M2-W0.4：EHP 循环魔数 + 普通怪 DPS 乘数逐值锁定
/// （vendor Modules/Data.lua:228 / :235 / :237 / :239）。
#[test]
fn m2_ehp_calc_constants_pinned_to_pob2_source() {
    let gc = load();

    // Modules/Data.lua:237 ehpCalcMaxDamage = 100000000
    assert_eq!(gc.game.ehp_calc_max_damage, 100_000_000.0);
    // Modules/Data.lua:239 ehpCalcMaxIterationsToCalc = 50
    assert_eq!(gc.game.ehp_calc_max_iterations, 50.0);
    // Modules/Data.lua:235 ehpCalcSpeedUp = 8
    assert_eq!(gc.game.ehp_calc_speed_up, 8.0);
    // Modules/Data.lua:228 normalEnemyDPSMult = 1 / 4.40（IEEE754 逐 bit 相等）
    assert_eq!(gc.game.normal_enemy_dps_mult, 1.0 / 4.40);
}

/// M2-F：max-hit 转换平滑迭代数锁定（vendor Modules/Data.lua:241
/// maxHitSmoothingPasses = 8，CalcDefence.lua:3669 消费）。
#[test]
fn m2_max_hit_smoothing_passes_pinned_to_pob2_source() {
    let gc = load();

    assert_eq!(gc.game.max_hit_smoothing_passes, 8.0);
}

/// M2-D：Block 面板族常量锁定——基础格挡上限 50%
/// （vendor Data/Misc.lua:147 `object_inherent_base_maximum_block_%_from_ot`，
/// CalcSetup.lua:28 注入 `BaseBlockChanceMax` BASE）。
#[test]
fn m2_block_constants_pinned_to_pob2_source() {
    let gc = load();
    assert_eq!(gc.game.base_block_chance_max, 50.0);
    // Data.lua:185 BlockChanceCap = 90（既有字段，一并核对消费侧 clamp 边界）。
    assert_eq!(gc.game.block_chance_cap, 90.0);
}

/// vendor `data.misc` 派生口径核对：异常基线 fraction = PercentPerMinute/60/100，
/// JSON 与 vendor 派生值一致（Misc.lua:86-88 → 900/1200/1200）。
#[test]
fn ailment_fractions_consistent_with_vendor_per_minute_form() {
    let gc = load();
    assert_eq!(gc.game.bleed_base_fraction, 900.0 / 60.0 / 100.0);
    assert_eq!(gc.game.ignite_base_fraction, 1200.0 / 60.0 / 100.0);
    assert_eq!(gc.game.poison_base_fraction, 1200.0 / 60.0 / 100.0);
    // 感电刻度：pobr 用小数 0.2，vendor BaseShockMagnitude 用百分制 20。
    assert_eq!(
        gc.game.shock_default_effect,
        gc.game.shock_min_effect / 100.0
    );
}

/// M0-W3 fallback 不变式：`GameConstantsDef::default()`（无 GameData 时的回退常量集）
/// 与入库 JSON **整结构逐值相等**——保证「注入」与「回退」两条 calc 路径输出一致
/// （搬迁不变式的结构锁，架构文档 20 §1 P8）。
#[test]
fn default_fallback_equals_loaded_json_exactly() {
    assert_eq!(load(), GameConstantsDef::default());
}
