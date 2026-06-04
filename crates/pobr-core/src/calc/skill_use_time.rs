//! 技能使用时间与动作速率（08-mechanics §2.1、§3.2；`agent-docs/skill-speed.md`）。
//!
//! - AttackSpeed / CastSpeed / SkillSpeed 同属一个 additive Inc 速度 bucket。
//! - ActionSpeed 是独立乘区，作用于最终速率。
//! - `+# seconds to use time` 类惩罚在速度修正后追加，不被速度缩放。
//! - 非吟唱动作的有效速率被服务器帧上限（约 30.3 actions/s）截断。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

use super::round;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillUseTime {
    pub base_use_time: f64,
    /// additive 速度 bucket 总量（%）。
    pub total_skill_speed: f64,
    /// action speed 独立乘区总量（%）。
    pub total_action_speed: f64,
    pub total_use_time_penalty: f64,
    pub tooltip_use_time: f64,
    pub tooltip_rate: f64,
    pub effective_rate: f64,
    pub capped_by_server_tick: bool,
}

const SPEED_BUCKET: [&str; 3] = ["AttackSpeed", "CastSpeed", "SkillSpeed"];

fn speed_names() -> [ModName; 3] {
    [
        ModName::from(SPEED_BUCKET[0]),
        ModName::from(SPEED_BUCKET[1]),
        ModName::from(SPEED_BUCKET[2]),
    ]
}

/// 计算技能使用时间与有效动作速率。
pub fn calc_skill_use_time(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
) -> SkillUseTime {
    let total_skill_speed = db.sum(ModType::Inc, cfg, &speed_names());
    let total_action_speed = db.sum(ModType::Inc, cfg, &[ModName::from("ActionSpeed")]);

    let tooltip_use_time = if base_use_time > 0.0 {
        base_use_time / (1.0 + total_skill_speed / 100.0) + use_time_penalty
    } else {
        use_time_penalty
    };
    let tooltip_rate = if tooltip_use_time > 0.0 {
        1.0 / tooltip_use_time
    } else {
        0.0
    };

    let action_factor = 1.0 + total_action_speed / 100.0;
    let uncapped_rate = tooltip_rate * action_factor;

    let server_rate = 1.0 / SERVER_TICK_SECONDS;
    let (effective_rate, capped_by_server_tick) = if !is_channelling && uncapped_rate > server_rate
    {
        (server_rate, true)
    } else {
        (uncapped_rate, false)
    };

    SkillUseTime {
        base_use_time,
        total_skill_speed: round(total_skill_speed),
        total_action_speed: round(total_action_speed),
        total_use_time_penalty: use_time_penalty,
        tooltip_use_time: round(tooltip_use_time),
        tooltip_rate: round(tooltip_rate),
        effective_rate: round(effective_rate),
        capped_by_server_tick,
    }
}

/// `calc_skill_use_time` 的追踪版本：记录速度 bucket、action speed、最终速率节点。
pub fn calc_skill_use_time_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
    trace: &mut TraceGraph,
) -> (SkillUseTime, TraceNodeId) {
    let result = calc_skill_use_time(db, cfg, base_use_time, use_time_penalty, is_channelling);

    let base_node = trace.add_source_node(
        "base use time",
        base_use_time,
        SourceId::new(SourceKind::CharacterBase, "base.UseTime"),
    );
    let speed_bucket = db.sum_traced(
        ModType::Inc,
        cfg,
        &speed_names(),
        trace,
        "skill speed bucket (Attack/Cast/Skill)",
    );
    let action_speed = db.sum_traced(
        ModType::Inc,
        cfg,
        &[ModName::from("ActionSpeed")],
        trace,
        "action speed (independent)",
    );
    let rate_node = trace.add_node(
        "effective action rate",
        result.effective_rate,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_node, rate_node);
    trace.add_edge(speed_bucket.node_id, rate_node);
    trace.add_edge(action_speed.node_id, rate_node);

    (result, rate_node)
}

/// 便捷 traced 返回值。
pub fn calc_skill_use_time_traced_value(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
    trace: &mut TraceGraph,
) -> TracedValue {
    let (result, node_id) = calc_skill_use_time_traced(
        db,
        cfg,
        base_use_time,
        use_time_penalty,
        is_channelling,
        trace,
    );
    TracedValue {
        value: result.effective_rate,
        node_id,
    }
}
