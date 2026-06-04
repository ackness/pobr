//! 通用属性上下界解析（StatBoundary）。
//!
//! 把 `pobr_data::BoundarySpec`（floor / default_max / hard_cap）应用到一个未截断的
//! 属性值上，产出：截断后的最终值、生效的最大值、over-cap（超出最大值的浪费量）、
//! missing（距最大值还差多少）。抗性 / 最大抗性 / 任何带边界的属性共用此原语。
//!
//! 公式（与 `offence::resolve_resistance` 语义一致）：
//! - `effective_max = min(default_max + max_bonus, hard_cap)`（任一为 None 时跳过该约束）
//! - `final = clamp(uncapped, floor, effective_max)`（floor / max 为 None 时不约束该侧）
//! - `over_cap = max(uncapped - effective_max, 0)`
//! - `missing = max(effective_max - final, 0)`

use pobr_data::prelude::*;

use super::round;

/// 属性边界解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatBoundary {
    /// 未截断的原始值。
    pub uncapped: f64,
    /// 实际生效的最大值（含 max_bonus、受 hard_cap 约束）；无最大值约束时为 `f64::INFINITY`。
    pub max: f64,
    /// 截断后的最终值。
    pub final_value: f64,
    /// 超出最大值的浪费量（>= 0）。
    pub over_cap: f64,
    /// 距最大值还差的量（>= 0）；无最大值约束时为 0。
    pub missing: f64,
}

/// 按 `spec` 解析 `uncapped` 的上下界。`max_bonus` 是提高 default_max 的加成之和
/// （如 `+5% to maximum Fire Resistance`）。
pub fn stat_boundary(uncapped: f64, max_bonus: f64, spec: &BoundarySpec) -> StatBoundary {
    let effective_max = spec.default_max.map(|default_max| {
        let raised = default_max + max_bonus;
        match spec.hard_cap {
            Some(hard) => raised.min(hard),
            None => raised,
        }
    });

    let final_value = {
        let mut value = uncapped;
        if let Some(floor) = spec.floor {
            value = value.max(floor);
        }
        if let Some(max) = effective_max {
            value = value.min(max);
        }
        value
    };

    let (max_out, over_cap, missing) = match effective_max {
        Some(max) => (
            round(max),
            round((uncapped - max).max(0.0)),
            round((max - final_value).max(0.0)),
        ),
        None => (f64::INFINITY, 0.0, 0.0),
    };

    StatBoundary {
        uncapped: round(uncapped),
        max: max_out,
        final_value: round(final_value),
        over_cap,
        missing,
    }
}
