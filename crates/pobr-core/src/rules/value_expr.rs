//! 受限模板 DSL 的**唯一求值器**。
//!
//! schema 类型在 `pobr_data::catalog::value_expr`（零逻辑层）；本模块是
//! config effects / special_mods / parser 模板共用的
//! 五算子 + 受限谓词求值实现——三处是同一套受限语言，**禁止三套方言**。
//! special_mods 的 enums 闭集、parser 的 `:cap` 算子均为本模块的受限扩展（各自走
//! ≥20 条目受益闸门 + 架构 review）。
//!
//! 求值上下文仅单输入 `input`（config 条目单输入值）；字段引用扩展
//! 沿 [`FieldRef`] 单点演化。

use pobr_data::catalog::value_expr::{CmpOp, FieldRef, Predicate, ValueExpr};

/// 求值数值表达式。
///
/// 五算子语义（架构 §5）：
/// - `Literal` → 字面值；
/// - `Input { mult, div, base }` → `input × mult ÷ div + base`（`div = 0`
///   防御性按 1 处理——schema 不应产出 0 除数，求值器不 panic）；
/// - `Negate` → `-eval(inner)`；
/// - `Clamp { min, max }` → 先 max 后 min 截断（与 vendor
///   `m_max(m_min(v, max), min)` 一致）。
pub fn eval(expr: &ValueExpr, input: f64) -> f64 {
    match expr {
        ValueExpr::Literal { value } => *value,
        ValueExpr::Input { mult, div, base } => {
            let div = if *div == 0.0 { 1.0 } else { *div };
            input * mult / div + base
        }
        ValueExpr::Negate { inner } => -eval(inner, input),
        ValueExpr::Clamp { min, max, inner } => {
            let mut value = eval(inner, input);
            if let Some(max) = max {
                value = value.min(*max);
            }
            if let Some(min) = min {
                value = value.max(*min);
            }
            value
        }
    }
}

/// 求值受限谓词（字段引用 + `eq/ne/gt/ge/lt/le` + `and/or`）。
///
/// 空的 `And` 为真（全称量化空集）、空的 `Or` 为假（存在量化空集）。
pub fn eval_predicate(pred: &Predicate, input: f64) -> bool {
    match pred {
        Predicate::Cmp { on, op, rhs } => {
            let lhs = resolve_field(*on, input);
            match op {
                CmpOp::Eq => lhs == *rhs,
                CmpOp::Ne => lhs != *rhs,
                CmpOp::Gt => lhs > *rhs,
                CmpOp::Ge => lhs >= *rhs,
                CmpOp::Lt => lhs < *rhs,
                CmpOp::Le => lhs <= *rhs,
            }
        }
        Predicate::And { all } => all.iter().all(|p| eval_predicate(p, input)),
        Predicate::Or { any } => any.iter().any(|p| eval_predicate(p, input)),
    }
}

/// 字段引用解析（闭集 = `input`）。
fn resolve_field(field: FieldRef, input: f64) -> f64 {
    match field {
        FieldRef::Input => input,
    }
}

#[cfg(test)]
mod tests {
    use pobr_data::catalog::value_expr::{CmpOp, FieldRef, Predicate, ValueExpr};

    use super::*;

    /// 字面量不受输入影响。
    #[test]
    fn literal_ignores_input() {
        assert_eq!(eval(&ValueExpr::literal(42.0), 0.0), 42.0);
        assert_eq!(eval(&ValueExpr::literal(42.0), 99.0), 42.0);
    }

    /// Input 标准线性组合：input × mult ÷ div + base。
    #[test]
    fn input_linear_combination() {
        assert_eq!(eval(&ValueExpr::input(), 17.0), 17.0);
        let expr = ValueExpr::Input {
            mult: 2.0,
            div: 4.0,
            base: 3.0,
        };
        // 10 × 2 ÷ 4 + 3 = 8
        assert_eq!(eval(&expr, 10.0), 8.0);
    }

    /// div = 0 防御性按 1 处理（不 panic、不产 inf）。
    #[test]
    fn zero_div_is_defensive_one() {
        let expr = ValueExpr::Input {
            mult: 1.0,
            div: 0.0,
            base: 0.0,
        };
        assert_eq!(eval(&expr, 5.0), 5.0);
    }

    /// Negate 取负，可嵌套。
    #[test]
    fn negate_nested() {
        let expr = ValueExpr::Negate {
            inner: Box::new(ValueExpr::input()),
        };
        assert_eq!(eval(&expr, 7.0), -7.0);
        let double = ValueExpr::Negate {
            inner: Box::new(expr),
        };
        assert_eq!(eval(&double, 7.0), 7.0);
    }

    /// Clamp 上下界（vendor m_max(m_min(val, 100), 0) 语义——
    /// multiplierCurrentManaPercentage 形态）。
    #[test]
    fn clamp_min_max() {
        let expr = ValueExpr::Clamp {
            min: Some(0.0),
            max: Some(100.0),
            inner: Box::new(ValueExpr::input()),
        };
        assert_eq!(eval(&expr, 50.0), 50.0);
        assert_eq!(eval(&expr, 250.0), 100.0);
        assert_eq!(eval(&expr, -5.0), 0.0);
    }

    /// 单边 clamp：只有 min 或只有 max。
    #[test]
    fn clamp_single_sided() {
        let min_only = ValueExpr::Clamp {
            min: Some(0.0),
            max: None,
            inner: Box::new(ValueExpr::input()),
        };
        assert_eq!(eval(&min_only, -3.0), 0.0);
        assert_eq!(eval(&min_only, 3.0), 3.0);

        let max_only = ValueExpr::Clamp {
            min: None,
            max: Some(20.0),
            inner: Box::new(ValueExpr::input()),
        };
        assert_eq!(eval(&max_only, 50.0), 20.0);
        assert_eq!(eval(&max_only, 5.0), 5.0);
    }

    /// clamp 包线性：CoveredInAsh 形态 min(effect, 20)。
    #[test]
    fn clamp_wraps_linear() {
        let expr = ValueExpr::Clamp {
            min: None,
            max: Some(20.0),
            inner: Box::new(ValueExpr::Input {
                mult: 2.0,
                div: 1.0,
                base: 0.0,
            }),
        };
        assert_eq!(eval(&expr, 5.0), 10.0);
        assert_eq!(eval(&expr, 15.0), 20.0);
    }

    /// 六个比较算子逐一验证。
    #[test]
    fn predicate_cmp_operators() {
        let cmp = |op: CmpOp, rhs: f64, input: f64| {
            eval_predicate(
                &Predicate::Cmp {
                    on: FieldRef::Input,
                    op,
                    rhs,
                },
                input,
            )
        };
        assert!(cmp(CmpOp::Eq, 5.0, 5.0) && !cmp(CmpOp::Eq, 5.0, 4.0));
        assert!(cmp(CmpOp::Ne, 5.0, 4.0) && !cmp(CmpOp::Ne, 5.0, 5.0));
        assert!(cmp(CmpOp::Gt, 5.0, 6.0) && !cmp(CmpOp::Gt, 5.0, 5.0));
        assert!(cmp(CmpOp::Ge, 5.0, 5.0) && !cmp(CmpOp::Ge, 5.0, 4.0));
        assert!(cmp(CmpOp::Lt, 5.0, 4.0) && !cmp(CmpOp::Lt, 5.0, 5.0));
        assert!(cmp(CmpOp::Le, 5.0, 5.0) && !cmp(CmpOp::Le, 5.0, 6.0));
    }

    /// and/or 组合 + 空集语义（And 空 = 真，Or 空 = 假）。
    #[test]
    fn predicate_and_or_and_empty() {
        let and = Predicate::And {
            all: vec![Predicate::input_gt(0.0), Predicate::input_gt(10.0)],
        };
        assert!(eval_predicate(&and, 11.0));
        assert!(!eval_predicate(&and, 5.0));

        let or = Predicate::Or {
            any: vec![Predicate::input_gt(10.0), Predicate::input_gt(100.0)],
        };
        assert!(eval_predicate(&or, 11.0));
        assert!(!eval_predicate(&or, 5.0));

        assert!(eval_predicate(&Predicate::And { all: Vec::new() }, 0.0));
        assert!(!eval_predicate(&Predicate::Or { any: Vec::new() }, 0.0));
    }

    /// conditionStationary 形态端到端：emit_if input > 0。
    #[test]
    fn stationary_emit_if_shape() {
        let pred = Predicate::input_gt(0.0);
        assert!(eval_predicate(&pred, 5.0));
        assert!(!eval_predicate(&pred, 0.0));
    }
}
