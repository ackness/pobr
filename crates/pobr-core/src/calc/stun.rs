//! Stun 体系（M2 Track E，13-G12；PoB2 `CalcDefence.lua:2525-2643` stun 段）。
//!
//! 产出三件：`StunThreshold`（基底切换 + AddESToStunThreshold + BASE/INC/MORE 聚合）、
//! `SelfStunChance`（`StunBaseMult × 有效伤 / 阈值`，物理 ×0.25 加权）、
//! `StunDuration`（按服务器帧上取整）。
//!
//! 常量全部经 `cfg.constants` 注入（game_constants 已有 stun 全套，蓝图明令勿再加）；
//! keystone flag（ChaosInoculation）由调用方从 C-1 `DefenceKeystones` 快照经
//! [`StunInputs`] 传入，本模块不散读 keystone（蓝图 §3.3 契约 2）。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::round;

/// Stun 计算输入（池值/受击参考值/上游 avoidance 产出 + keystone 开关）。
///
/// `total_taken_hit` / `physical_taken_hit` 对应 PoB2 `output.totalTakenHit` /
/// `output.PhysicalTakenHit`（:2617）——Track F 接线前由调用方以单击参考伤害近似，
/// F 接线后换扣池管线真值（公式不变）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StunInputs {
    /// 当前生命池（`output.Life`，阈值默认基底，:2542-2543）。
    pub life: f64,
    /// 玩家角色面板 flat 基础生命（CI 分支「CI 前 Life」的标量部分；
    /// vendor `Sum("BASE","Life")` 含角色基础，pobr 基础在 ActorBaseStats，须由调用方喂入）。
    pub life_base_flat: f64,
    /// 当前 ES 池（ES 基底切换 / AddESToStunThreshold 用，:2529-2548）。
    pub energy_shield: f64,
    /// 当前法力池（Mana 基底切换用，:2533-2536）。
    pub mana: f64,
    /// 受击总承伤（`output.totalTakenHit`，:2617）。
    pub total_taken_hit: f64,
    /// 受击物理承伤（`output.PhysicalTakenHit`，:2617 物理 ×0.25 加权项）。
    pub physical_taken_hit: f64,
    /// 眩晕规避几率（[`super::calc_avoidance`] 产出 `avoid_stun`，已含 ES 隐式减半；
    /// `notAvoidChance = 100 − avoid_stun`，:2554-2558）。
    pub avoid_stun: f64,
    /// CI keystone（阈值基底取「CI 前 Life」，:2537-2539）；
    /// 调用方从 C-1 `DefenceKeystones::chaos_inoculation` 快照传入（蓝图 §3.3 契约 2）。
    pub chaos_inoculation: bool,
}

/// Stun 计算结果（写 W0.2 的 `stun_threshold` / `self_stun_chance` / `stun_duration`）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StunResult {
    /// 眩晕阈值（:2552）。
    pub threshold: f64,
    /// 受眩晕几率（%，:2623-2624，已乘 1−规避）。
    pub self_stun_chance: f64,
    /// 眩晕持续（秒，按服务器帧上取整，:2594；规避 ≥100% → 0，:2584-2586）。
    pub stun_duration: f64,
}

/// 眩晕阈值（PoB2 CalcDefence.lua:2526-2552）。
///
/// 基底切换（互斥分支，:2528-2543）：
/// - `StunThresholdBasedOnEnergyShieldInsteadOfLife` → `ES × ΣBASE StunThresholdEnergyShieldPercent / 100`；
/// - `StunThresholdBasedOnManaInsteadOfLife` → `Mana × ΣBASE StunThresholdManaPercent / 100`；
/// - `ChaosInoculation` → CI 前 Life（flat 基础 + ΣBASE MaximumLife，:2537-2539）；
/// - 默认 → `output.Life`。
///
/// 追加（:2544-2548）：`AddESToStunThreshold` → `+ ES × ΣBASE ESToStunThresholdPercent / 100`。
/// 聚合（:2549-2552）：`(基底 + ΣBASE StunThreshold) × (1 + ΣINC/100) × ΠMORE`。
pub fn calc_stun_threshold(db: &ModDb, cfg: &CalcConfig, inp: &StunInputs) -> f64 {
    let names = [ModName::from("StunThreshold")];
    let base = if db.flag(
        cfg,
        ModName::from("StunThresholdBasedOnEnergyShieldInsteadOfLife"),
    ) {
        let pct = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("StunThresholdEnergyShieldPercent")],
        );
        inp.energy_shield * pct / 100.0
    } else if db.flag(cfg, ModName::from("StunThresholdBasedOnManaInsteadOfLife")) {
        let pct = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("StunThresholdManaPercent")],
        );
        inp.mana * pct / 100.0
    } else if inp.chaos_inoculation {
        // CI 前 Life（vendor `Sum("BASE","Life")`；pobr 生命池 BASE 词条名为 MaximumLife，
        // 角色 flat 基础经 life_base_flat 喂入）。
        inp.life_base_flat + db.sum(ModType::Base, cfg, &[ModName::from("MaximumLife")])
    } else {
        inp.life
    };
    let base = if db.flag(cfg, ModName::from("AddESToStunThreshold")) {
        let pct = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ESToStunThresholdPercent")],
        );
        base + inp.energy_shield * pct / 100.0
    } else {
        base
    };
    let flat = db.sum(ModType::Base, cfg, &names);
    let inc = 1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0;
    let more = db.more(cfg, &names);
    round((base + flat) * inc * more)
}

/// Stun 全套（PoB2 CalcDefence.lua:2525-2643；阈值 → 时长 → 受眩晕几率）。
///
/// # 公式（逐行对照）
/// - 时长（:2584-2595）：`StunAvoidChance ≥ 100` → 0；否则
///   `StunDuration = ceil(StunBaseDuration × (1+INC StunDuration) / (1+INC StunRecovery)
///   × ServerTickRate) / ServerTickRate`（帧上取整；`ServerTickRate = 1/server_tick_seconds`，
///   Data.lua:172-173）。
/// - 有效眩晕伤害（:2617）：`totalTakenHit + PhysicalTakenHit × 0.25`；
///   damageCategoryConfig 默认 "Average" 分支再 `× PhysicalStunMult`（:2620-2621，
///   `monster.physical_hit_stun_multiplier_pct/100 = 1.0`）。非 Average 的
///   `× PhysicalStunMult × (1 + MeleeStunMult×3)/4` 分支（:2618-2619）依赖 config
///   输入，M3 config_interpreter 接入后扩参，公式骨架不变。
/// - 几率（:2623-2624）：`baseStunChance = min(StunBaseMult × 有效伤 / 阈值, 100)`；
///   低于 `MinStunChanceNeeded`(20) 归 0；再 `× notAvoidChance / 100`。
pub fn calc_stun(db: &ModDb, cfg: &CalcConfig, inp: &StunInputs) -> StunResult {
    let game = cfg.constants.game();
    let threshold = calc_stun_threshold(db, cfg, inp);

    // :2554-2558 notAvoidChance 由上游 avoid_stun 还原（已含 StunImmune/ES 隐式减半）。
    let not_avoid = (100.0 - inp.avoid_stun).max(0.0);

    // --- 时长（:2584-2595）---
    let stun_duration = if not_avoid <= 0.0 {
        // :2584-2586 「Cannot be Stunned」→ 0。
        0.0
    } else {
        let dur_inc = 1.0 + db.sum(ModType::Inc, cfg, &[ModName::from("StunDuration")]) / 100.0;
        let recovery = 1.0 + db.sum(ModType::Inc, cfg, &[ModName::from("StunRecovery")]) / 100.0;
        let tick = game.server_tick_seconds;
        // :2594 m_ceil(base × dur / recovery × ServerTickRate) / ServerTickRate。
        let raw = game.stun_base_duration_seconds * dur_inc / recovery;
        round((raw / tick).ceil() * tick)
    };

    // --- 受眩晕几率（:2617-2624）---
    // :2617 物理 ×0.25 加权；:2620-2621 默认 "Average" 口径 × PhysicalStunMult。
    let effective_damage = (inp.total_taken_hit + inp.physical_taken_hit * 0.25)
        * (cfg.constants.monster().physical_hit_stun_multiplier_pct / 100.0);
    // :2623 阈值为 0 时 vendor 为除零（实战阈值 ≥1）；防御性处理：有伤害即必晕、无伤害不晕。
    let base_chance = if threshold > 0.0 {
        (game.stun_base_mult * effective_damage / threshold).min(100.0)
    } else if effective_damage > 0.0 {
        100.0
    } else {
        0.0
    };
    // :2624 低于 MinStunChanceNeeded(20) 归 0，再乘 (1 − 规避)。
    let gated = if base_chance > game.min_stun_chance_needed {
        base_chance
    } else {
        0.0
    };
    let self_stun_chance = round(gated * not_avoid / 100.0);

    StunResult {
        threshold,
        self_stun_chance,
        stun_duration,
    }
}
