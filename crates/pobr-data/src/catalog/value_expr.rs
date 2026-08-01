//! **Schema types** for the restricted template DSL.
//!
//! The one restricted placeholder language used project-wide: config
//! effects / special_mods / parser templates all share the same expression
//! and predicate types; the **single evaluator implementation** lives in
//! `pobr-core/src/rules/value_expr.rs` (no three dialects). This module has
//! zero logic, it only carries the serde shape.
//!
//! Hard limits (written into the review checklist; the extension gate: a
//! new capability needs ≥20 entries to benefit from it):
//! - Allowed: a numeric placeholder (input), literals, the five operators
//!   `negate / clamp(min,max) / div / mult / base`,
//!   `target(player|enemy|minion)`, restricted predicates (field
//!   references + `eq/ne/gt/lt(+ge/le)` + `and/or`);
//! - Forbidden: loops, recursion, free-form expressions, cross-entry
//!   references, runtime string concatenation.

use serde::{Deserialize, Serialize};

fn one() -> f64 {
    1.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_one(value: &f64) -> bool {
    *value == 1.0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// A numeric expression (a restricted closed set of five operators).
///
/// `Input` folds the three linear operators `mult / div / base` into the
/// canonical form `input × mult ÷ div + base`; `Negate` / `Clamp` are
/// single-layer wrapping operators. There's no way to express loops /
/// recursion / cross-entry references (architecture doc §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueExpr {
    /// A literal.
    Literal {
        /// The literal value.
        value: f64,
    },
    /// A linear combination of the input: `input × mult ÷ div + base`.
    Input {
        /// Multiplier (default 1).
        #[serde(default = "one", skip_serializing_if = "is_one")]
        mult: f64,
        /// Divisor (default 1).
        #[serde(default = "one", skip_serializing_if = "is_one")]
        div: f64,
        /// Addend (default 0).
        #[serde(default, skip_serializing_if = "is_zero")]
        base: f64,
    },
    /// Negation: `-inner`.
    Negate {
        /// The inner expression being negated.
        inner: Box<ValueExpr>,
    },
    /// Clamping to bounds: `clamp(inner, min, max)` (either bound may be absent).
    Clamp {
        /// Lower bound (absent = no lower bound).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        /// Upper bound (absent = no upper bound).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        /// The inner expression being clamped.
        inner: Box<ValueExpr>,
    },
}

impl ValueExpr {
    /// Convenience constructor for a literal.
    pub fn literal(value: f64) -> Self {
        Self::Literal { value }
    }

    /// Convenience constructor for the identity input
    /// (`input × 1 ÷ 1 + 0`).
    pub fn input() -> Self {
        Self::Input {
            mult: 1.0,
            div: 1.0,
            base: 0.0,
        }
    }
}

/// Comparison operators for a restricted predicate (architecture doc §5:
/// the `eq/ne/gt/lt` family).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
}

/// A field reference for a predicate (a restricted closed set).
///
/// Only `input` (a config entry's single input) exists so far; extensions
/// like enums / `:cap` evolve this module in one place per decision §4 —
/// creating a new field-reference shape elsewhere is not allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRef {
    /// The entry's input value.
    Input,
}

/// A restricted predicate: a field reference + comparison + `and/or`
/// combination (no expressive power beyond this recursive-but-bounded
/// combination).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Predicate {
    /// A single comparison: `on <op> rhs`.
    Cmp {
        /// The field being compared.
        on: FieldRef,
        /// The comparison operator.
        op: CmpOp,
        /// The right-hand side (a literal; comparing two fields is forbidden).
        rhs: f64,
    },
    /// Conjunction: all must hold.
    And {
        /// The sub-predicates.
        all: Vec<Predicate>,
    },
    /// Disjunction: any may hold.
    Or {
        /// The sub-predicates.
        any: Vec<Predicate>,
    },
}

impl Predicate {
    /// Convenience constructor for `input > rhs` (the most common emit_if shape).
    pub fn input_gt(rhs: f64) -> Self {
        Self::Cmp {
            on: FieldRef::Input,
            op: CmpOp::Gt,
            rhs,
        }
    }
}

/// The actor an effect writes to (`target(player|enemy|minion)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTarget {
    /// The player's modDB.
    Player,
    /// The enemy's modDB.
    Enemy,
    /// A minion's modDB.
    Minion,
}

/// The restricted tag whitelist (the data-representable subset of PoB2's
/// mod tags).
///
/// Tag shapes outside the whitelist (`SkillType`/`GlobalEffect`/ones with
/// extra fields, etc.) are always downgraded to a `handler_id` entry at
/// extraction time and don't enter the data (architecture doc §5: "a field
/// outside the whitelist → mark this entry with handler_id").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EffectTag {
    /// A condition tag (PoB2's `{ type = "Condition", var, neg }`).
    Condition {
        /// Condition name.
        var: String,
        /// Negation (`neg = true` means it applies when the condition does
        /// NOT hold).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        neg: bool,
    },
    /// A multiplier tag (PoB2's
    /// `{ type = "Multiplier", var, div, limit, actor }`).
    Multiplier {
        /// Multiplier variable name.
        var: String,
        /// Divisor (default 1).
        #[serde(default = "one", skip_serializing_if = "is_one")]
        div: f64,
        /// Multiplier cap (absent = no cap).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<f64>,
        /// The actor dimension (wired up for evaluation later; the data is
        /// transcribed faithfully for now).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<String>,
    },
    /// A cross-actor condition tag (PoB2's
    /// `{ type = "ActorCondition", actor, var, neg }`).
    ///
    /// pobr's `ModTag` currently has no actor dimension; the data is
    /// transcribed faithfully, and until the consumer wires it up, any mod
    /// carrying this tag is recorded in diagnostics and skipped.
    ActorCondition {
        /// Target actor (vendor literals like `enemy`/`parent`).
        actor: String,
        /// Condition name.
        var: String,
        /// Negation.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        neg: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// serde round-trip for every `ValueExpr` variant: default fields
    /// aren't serialized, and the round trip is lossless.
    #[test]
    fn value_expr_serde_round_trip() {
        let exprs = vec![
            ValueExpr::literal(5.0),
            ValueExpr::input(),
            ValueExpr::Input {
                mult: 2.0,
                div: 3.0,
                base: -1.0,
            },
            ValueExpr::Negate {
                inner: Box::new(ValueExpr::input()),
            },
            ValueExpr::Clamp {
                min: Some(0.0),
                max: Some(100.0),
                inner: Box::new(ValueExpr::input()),
            },
        ];
        for expr in exprs {
            let json = serde_json::to_string(&expr).unwrap();
            let back: ValueExpr = serde_json::from_str(&json).unwrap();
            assert_eq!(back, expr, "round trip 失败：{json}");
        }
    }

    /// The identity input serializes to just its kind (mult/div/base all
    /// default and get skipped).
    #[test]
    fn identity_input_serializes_minimal() {
        let json = serde_json::to_string(&ValueExpr::input()).unwrap();
        assert_eq!(json, r#"{"kind":"input"}"#);
    }

    /// Predicate serde round-trip, including nested and/or.
    #[test]
    fn predicate_serde_round_trip() {
        let pred = Predicate::And {
            all: vec![
                Predicate::input_gt(0.0),
                Predicate::Or {
                    any: vec![Predicate::Cmp {
                        on: FieldRef::Input,
                        op: CmpOp::Le,
                        rhs: 100.0,
                    }],
                },
            ],
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: Predicate = serde_json::from_str(&json).unwrap();
        assert_eq!(back, pred);
    }

    /// serde round-trip for the EffectTag whitelist variants; Condition's
    /// default `neg` isn't serialized.
    #[test]
    fn effect_tag_serde_round_trip() {
        let cond = EffectTag::Condition {
            var: "Stationary".to_string(),
            neg: false,
        };
        let json = serde_json::to_string(&cond).unwrap();
        assert_eq!(json, r#"{"type":"condition","var":"Stationary"}"#);

        let mult = EffectTag::Multiplier {
            var: "WitheredStack".to_string(),
            div: 1.0,
            limit: Some(10.0),
            actor: None,
        };
        let json = serde_json::to_string(&mult).unwrap();
        let back: EffectTag = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mult);
    }

    /// EffectTarget serializes to a snake_case literal.
    #[test]
    fn effect_target_snake_case() {
        assert_eq!(
            serde_json::to_string(&EffectTarget::Player).unwrap(),
            r#""player""#
        );
        assert_eq!(
            serde_json::to_string(&EffectTarget::Enemy).unwrap(),
            r#""enemy""#
        );
    }
}
