//! 受限模板 DSL 的 **schema 类型**（架构文档 20 §5 / 00-index 裁决 §4-1）。
//!
//! 全项目唯一的受限占位符语言：config effects（M3）/ special_mods（M5b）/
//! parser 模板（M6）共用同一套表达式与谓词类型；**求值器唯一实现**在
//! `pobr-core/src/rules/value_expr.rs`（禁三套方言）。本模块零逻辑，
//! 仅承载 serde 形状。
//!
//! 硬边界（写入 review checklist，扩展闸门 = 新能力须 ≥20 条目受益）：
//! - 允许：数值占位（input）、字面量、`negate / clamp(min,max) / div / mult /
//!   base` 五种算子、`target(player|enemy|minion)`、受限谓词
//!   （字段引用 + `eq/ne/gt/lt(+ge/le)` + `and/or`）；
//! - 禁止：循环、递归、自由表达式、跨条目引用、字符串拼接求值。

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

/// 数值表达式（五算子受限闭集）。
///
/// `Input` 把 `mult / div / base` 三个线性算子合并为标准形
/// `input × mult ÷ div + base`；`Negate` / `Clamp` 为单层包装算子。
/// 不存在循环 / 递归 / 跨条目引用的表达能力（架构 §5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueExpr {
    /// 字面量。
    Literal {
        /// 字面值。
        value: f64,
    },
    /// 输入线性组合：`input × mult ÷ div + base`。
    Input {
        /// 乘数（缺省 1）。
        #[serde(default = "one", skip_serializing_if = "is_one")]
        mult: f64,
        /// 除数（缺省 1）。
        #[serde(default = "one", skip_serializing_if = "is_one")]
        div: f64,
        /// 加数（缺省 0）。
        #[serde(default, skip_serializing_if = "is_zero")]
        base: f64,
    },
    /// 取负：`-inner`。
    Negate {
        /// 被取负的内层表达式。
        inner: Box<ValueExpr>,
    },
    /// 上下界截断：`clamp(inner, min, max)`（任一界可缺省）。
    Clamp {
        /// 下界（缺省无下界）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        /// 上界（缺省无上界）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        /// 被截断的内层表达式。
        inner: Box<ValueExpr>,
    },
}

impl ValueExpr {
    /// 字面量便捷构造。
    pub fn literal(value: f64) -> Self {
        Self::Literal { value }
    }

    /// 恒等输入（`input × 1 ÷ 1 + 0`）便捷构造。
    pub fn input() -> Self {
        Self::Input {
            mult: 1.0,
            div: 1.0,
            base: 0.0,
        }
    }
}

/// 受限谓词的比较算子（架构 §5：`eq/ne/gt/lt` 族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    /// 等于。
    Eq,
    /// 不等于。
    Ne,
    /// 大于。
    Gt,
    /// 大于等于。
    Ge,
    /// 小于。
    Lt,
    /// 小于等于。
    Le,
}

/// 谓词字段引用（受限闭集）。
///
/// M3 仅 `input`（config 条目单输入）；M5b enums / M6 `:cap` 等扩展按
/// 00-index 裁决 §4 走本模块单点演化，禁止旁路新建字段引用形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRef {
    /// 条目输入值。
    Input,
}

/// 受限谓词：字段引用 + 比较 + `and/or` 组合（无递归自由组合之外的能力）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Predicate {
    /// 单比较：`on <op> rhs`。
    Cmp {
        /// 被比较的字段。
        on: FieldRef,
        /// 比较算子。
        op: CmpOp,
        /// 右值（字面量；禁字段间比较）。
        rhs: f64,
    },
    /// 合取：全部成立。
    And {
        /// 子谓词列表。
        all: Vec<Predicate>,
    },
    /// 析取：任一成立。
    Or {
        /// 子谓词列表。
        any: Vec<Predicate>,
    },
}

impl Predicate {
    /// `input > rhs` 便捷构造（emit_if 最常见形态）。
    pub fn input_gt(rhs: f64) -> Self {
        Self::Cmp {
            on: FieldRef::Input,
            op: CmpOp::Gt,
            rhs,
        }
    }
}

/// 效果写入目标 actor（`target(player|enemy|minion)`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTarget {
    /// 玩家 modDB。
    Player,
    /// 敌人 modDB。
    Enemy,
    /// 召唤物 modDB。
    Minion,
}

/// 受限 tag 白名单（对应 PoB2 mod tag 的可数据化子集）。
///
/// 白名单外的 tag 形态（`SkillType`/`GlobalEffect`/带额外字段等）一律在
/// 抽取侧降级为 `handler_id` 条目，不进数据（架构 §5「白名单外字段 →
/// 本条目标记 handler_id」）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EffectTag {
    /// 条件 tag（PoB2 `{ type = "Condition", var, neg }`）。
    Condition {
        /// 条件名。
        var: String,
        /// 取反（`neg = true` 时条件不成立才生效）。
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        neg: bool,
    },
    /// 倍率 tag（PoB2 `{ type = "Multiplier", var, div, limit, actor }`）。
    Multiplier {
        /// 倍率变量名。
        var: String,
        /// 除数（缺省 1）。
        #[serde(default = "one", skip_serializing_if = "is_one")]
        div: f64,
        /// 倍率上限（缺省无上限）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<f64>,
        /// actor 维度（M3-T5-E1 接通求值；数据先忠实转录）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<String>,
    },
    /// 跨 actor 条件 tag（PoB2 `{ type = "ActorCondition", actor, var, neg }`）。
    ///
    /// pobr `ModTag` 当前无 actor 维度（M3-T5-E1 落地）；数据忠实转录，
    /// 消费侧未接通前把携带本 tag 的 mod 记入 diagnostics 跳过。
    ActorCondition {
        /// 目标 actor（`enemy`/`parent` 等 vendor 字面量）。
        actor: String,
        /// 条件名。
        var: String,
        /// 取反。
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        neg: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ValueExpr 各变体 serde 往返：缺省字段不落盘、可逆。
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

    /// 恒等输入序列化后只有 kind（mult/div/base 全缺省跳过）。
    #[test]
    fn identity_input_serializes_minimal() {
        let json = serde_json::to_string(&ValueExpr::input()).unwrap();
        assert_eq!(json, r#"{"kind":"input"}"#);
    }

    /// 谓词 serde 往返 + 嵌套 and/or。
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

    /// EffectTag 白名单变体 serde 往返；Condition 缺省 neg 不落盘。
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

    /// EffectTarget 序列化为 snake_case 字面量。
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
