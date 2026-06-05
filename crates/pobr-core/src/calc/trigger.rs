//! 触发（Trigger）域：冷却驱动型触发的速率上限建模（初版）。
//!
//! 本模块只建模 agent-docs/triggers.md §三的**触发速率上限 (Trigger Rate Cap)**：
//! 冷却驱动型触发的速率被「触发器冷却 / ICDR 与被触发技能冷却的较大者」决定，再被
//! 服务器帧（`ServerTickRate ≈ 30.3/s`，`SERVER_TICK_SECONDS = 0.033`）向上取整节流。
//!
//! 能量驱动元宝石（Cast on X 的 Energy/Monster Power 模型）与多技能轮转
//! (`calcMultiSpellRotationImpact`) 在本版**defer**，见模块尾注与返回报告 deferred。
//!
//! 出处：
//! - agent-docs/triggers.md §三（TriggerRateCap / ICDR / ServerTick 取整、§3.3 双门控）。
//! - PoB2 `src/Modules/CalcTriggers.lua`（`modActionCooldown = max(triggeredCD, triggerCD/icdr)`、
//!   `rateCapAdjusted = ceil(cd × ServerTickRate)/ServerTickRate`、`SkillTriggerRate = min(cap, sourceRate)`）。
//! - PoB2 `src/Modules/Data.lua`（`ServerTickTime = 0.033`、`ServerTickRate = 1/0.033`）。

use pobr_data::prelude::SERVER_TICK_SECONDS;

use super::round;

/// 触发速率上限的结算结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriggerRate {
    /// 取较大者后、取整前的实际动作冷却（秒）。
    pub action_cooldown: f64,
    /// 向上取整到服务器帧后的冷却（秒）。
    pub rate_cap_cooldown: f64,
    /// 触发速率上限（次/秒）= 1 / rate_cap_cooldown。
    pub trigger_rate_cap: f64,
    /// 实际触发速率（次/秒）= min(上限, 有效源速率)。
    pub skill_trigger_rate: f64,
    /// 是否被源速率门控（源速率 < 上限）。
    pub limited_by_source: bool,
}

/// 服务器帧速率（actions/s），`1 / SERVER_TICK_SECONDS ≈ 30.3`。
/// 出处：PoB2 Data.lua `ServerTickRate = 1/0.033`。
pub fn server_tick_rate() -> f64 {
    1.0 / SERVER_TICK_SECONDS
}

/// 把冷却向上取整到服务器帧：`ceil(cd × rate) / rate`。
///
/// 触发只能发生在帧边界，真实冷却被「四舍五入」到下一帧。这是触发速率出现台阶的根因。
/// 出处：agent-docs/triggers.md §3.2；PoB2 CalcTriggers.lua。
pub fn round_cooldown_to_tick(cooldown: f64, tick_rate: f64) -> f64 {
    if cooldown <= 0.0 || tick_rate <= 0.0 {
        return 0.0;
    }
    round((cooldown * tick_rate).ceil() / tick_rate)
}

/// 触发速率上限纯函数：`cap = 1 / (ceil(cd × rate) / rate)`。
///
/// `cd` 为实际动作冷却（已是 `max(triggeredCD, triggerCD/icdr)` 的结果）；`tick_rate` 为
/// 服务器帧速率（默认 `server_tick_rate()`）。返回每秒触发上限。
/// 出处：agent-docs/triggers.md §3.1；PoB2 CalcTriggers.lua
/// `TriggerRateCap = 1/(ceil(modActionCooldown × ServerTickRate)/ServerTickRate)`。
pub fn trigger_rate_cap(cooldown: f64, tick_rate: f64) -> f64 {
    let rounded = round_cooldown_to_tick(cooldown, tick_rate);
    if rounded > 0.0 {
        round(1.0 / rounded)
    } else {
        0.0
    }
}

/// 计算实际动作冷却：`max(triggeredCD, triggerCD / icdr)`。
///
/// - `trigger_cd`：触发宝石本身冷却（`triggeredBy.grantedEffect.levels[lvl].cooldown`）。
/// - `triggered_cd`：被触发技能冷却（`skillData.cooldown`）；无冷却传 0。
/// - `icdr`：冷却恢复速率乘区（`CooldownRecovery`，INC/MORE 折算后的乘数，≥0），作为**除数**缩短触发宝石冷却。
///
/// 出处：agent-docs/triggers.md §3.1；PoB2 CalcTriggers.lua
/// `modActionCooldown = max(triggeredCD, triggerCD / icdrSkill)`。
pub fn action_cooldown(trigger_cd: f64, triggered_cd: f64, icdr: f64) -> f64 {
    let effective_trigger = if icdr > 0.0 {
        trigger_cd / icdr
    } else {
        trigger_cd
    };
    effective_trigger.max(triggered_cd)
}

/// 端到端：从触发器/被触发技能冷却 + ICDR + 有效源速率求实际触发速率。
///
/// `SkillTriggerRate = min(TriggerRateCap, EffectiveSourceRate)`——伤害再高，若源攻速低或
/// 冷却长，触发也慢（双重门控）。出处：agent-docs/triggers.md §3.3；PoB2 CalcTriggers.lua。
pub fn resolve_trigger_rate(
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    effective_source_rate: f64,
) -> TriggerRate {
    let tick_rate = server_tick_rate();
    let cd = action_cooldown(trigger_cd, triggered_cd, icdr);
    let rate_cap_cooldown = round_cooldown_to_tick(cd, tick_rate);
    let cap = if rate_cap_cooldown > 0.0 {
        1.0 / rate_cap_cooldown
    } else {
        0.0
    };

    let source = effective_source_rate.max(0.0);
    let (skill_rate, limited_by_source) = if source > 0.0 && source < cap {
        (source, true)
    } else {
        (cap, false)
    };

    TriggerRate {
        action_cooldown: round(cd),
        rate_cap_cooldown: round(rate_cap_cooldown),
        trigger_rate_cap: round(cap),
        skill_trigger_rate: round(skill_rate),
        limited_by_source,
    }
}

// ---------------------------------------------------------------------------
// DEFER（本版不实现，见返回报告 deferred）：
// - 能量模型：max_energy = Σ(socketed base_cast_time / 0.1) × 10（total-use-time ×2）；
//   energy_per_trigger = MonsterPower × 原始击中伤害 / 异常阈值；centienergy 基数
//   (Crit/Ignite/Shock=100、Freeze=1000)；energy_generated_+% 等级缩放。
// - 多技能轮转：calcMultiSpellRotationImpact（1000 次触发机会 + 帧对齐冷却 + 几何分布折算触发几率）。
// - CWC 引导间隔版 / SpellCastTimeAddedToCooldownIfTriggered / TriggeredDamage 注入。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_tick_rate_matches_constant() {
        // 1 / 0.033 ≈ 30.30/s。
        assert!((server_tick_rate() - 30.303_030_303).abs() < 1e-6);
    }

    #[test]
    fn cooldown_rounds_up_to_frame() {
        // 0.10s 冷却在 30.3/s 帧率下：ceil(0.10 × 30.303) = ceil(3.03) = 4 帧 → 4/30.303 ≈ 0.132s。
        let rate = server_tick_rate();
        let rounded = round_cooldown_to_tick(0.10, rate);
        assert!((rounded - 4.0 / rate).abs() < 1e-9);
        assert!(rounded > 0.10); // 取整后冷却变长。
    }

    #[test]
    fn cap_is_inverse_of_rounded_cooldown() {
        let rate = server_tick_rate();
        let cd = 0.15;
        let cap = trigger_rate_cap(cd, rate);
        let rounded = round_cooldown_to_tick(cd, rate);
        assert!((cap - 1.0 / rounded).abs() < 1e-6);
    }

    #[test]
    fn icdr_shortens_trigger_cooldown() {
        // trigger_cd=0.3, icdr=1.5 → 0.2；被触发技能无冷却 → action_cd=0.2。
        let cd = action_cooldown(0.3, 0.0, 1.5);
        assert!((cd - 0.2).abs() < 1e-9);
    }

    #[test]
    fn larger_of_two_cooldowns_wins() {
        // triggered_cd=0.5 大于 trigger_cd/icdr=0.3 → action_cd=0.5。
        let cd = action_cooldown(0.3, 0.5, 1.0);
        assert!((cd - 0.5).abs() < 1e-9);
    }

    #[test]
    fn source_rate_gates_trigger_rate() {
        // 上限远高于源速率 2/s → 实际速率被源门控为 2/s。
        let r = resolve_trigger_rate(0.05, 0.0, 1.0, 2.0);
        assert!(r.limited_by_source);
        assert!((r.skill_trigger_rate - 2.0).abs() < 1e-6);
    }

    #[test]
    fn cap_gates_when_source_is_fast() {
        // 源速率 100/s 高于上限 → 实际速率 = 上限。
        let r = resolve_trigger_rate(0.3, 0.0, 1.0, 100.0);
        assert!(!r.limited_by_source);
        assert!((r.skill_trigger_rate - r.trigger_rate_cap).abs() < 1e-9);
    }
}
