//! The **single evaluator** for the restricted template DSL.
//!
//! The schema types live in `pobr_data::catalog::value_expr` (the
//! zero-logic layer); this module is the shared implementation of the five
//! operators plus restricted predicates used by config effects,
//! special_mods, and parser templates — all three are the same restricted
//! language, and **must not fork into three dialects**. special_mods'
//! closed enum set and parser's `:cap` operator are both restricted
//! extensions of this module (each gated behind a ≥20-entry benefit
//! threshold plus architecture review).
//!
//! The evaluation context is just a single input, `input` (a config entry's
//! single input value); field-reference expansion evolves through the
//! single extension point [`FieldRef`].

use pobr_data::catalog::value_expr::{CmpOp, FieldRef, Predicate, ValueExpr};

/// Evaluates a numeric expression.
///
/// The five operators' semantics (architecture §5):
/// - `Literal` -> the literal value;
/// - `Input { mult, div, base }` -> `input × mult ÷ div + base` (`div = 0` is
///   defensively treated as 1 — the schema shouldn't produce a zero
///   divisor, but the evaluator must not panic);
/// - `Negate` -> `-eval(inner)`;
/// - `Clamp { min, max }` -> clamps max first, then min (matching vendor's
///   `m_max(m_min(v, max), min)`).
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

/// Evaluates a restricted predicate (field reference + `eq/ne/gt/ge/lt/le` +
/// `and/or`).
///
/// An empty `And` is true (universal quantification over an empty set); an
/// empty `Or` is false (existential quantification over an empty set).
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

/// Resolves a field reference (the closed set is just `input`).
fn resolve_field(field: FieldRef, input: f64) -> f64 {
    match field {
        FieldRef::Input => input,
    }
}

#[cfg(test)]
mod tests {
    use pobr_data::catalog::value_expr::{CmpOp, FieldRef, Predicate, ValueExpr};

    use super::*;

    /// A literal is unaffected by input.
    #[test]
    fn literal_ignores_input() {
        assert_eq!(eval(&ValueExpr::literal(42.0), 0.0), 42.0);
        assert_eq!(eval(&ValueExpr::literal(42.0), 99.0), 42.0);
    }

    /// Input's standard linear combination: input × mult ÷ div + base.
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

    /// div = 0 is defensively treated as 1 (no panic, no inf).
    #[test]
    fn zero_div_is_defensive_one() {
        let expr = ValueExpr::Input {
            mult: 1.0,
            div: 0.0,
            base: 0.0,
        };
        assert_eq!(eval(&expr, 5.0), 5.0);
    }

    /// Negate flips the sign, and nests.
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

    /// Clamp's lower/upper bounds (matches vendor's
    /// `m_max(m_min(val, 100), 0)` semantics — the
    /// multiplierCurrentManaPercentage shape).
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

    /// One-sided clamp: only min, or only max.
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

    /// Clamp wrapping a linear expression: the CoveredInAsh shape,
    /// min(effect, 20).
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

    /// Verifies all six comparison operators one by one.
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

    /// and/or composition plus empty-set semantics (empty And = true, empty
    /// Or = false).
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

    /// End-to-end conditionStationary shape: emit_if input > 0.
    #[test]
    fn stationary_emit_if_shape() {
        let pred = Predicate::input_gt(0.0);
        assert!(eval_predicate(&pred, 5.0));
        assert!(!eval_predicate(&pred, 0.0));
    }
}
