//! 技能类型**后缀表达式**求值器（support 适用性裁决的原子件）。
//!
//! 对照 PoB2 `Modules/CalcTools.lua:61-82 doesTypeExpressionMatch`：
//! GrantedEffects 的 require/exclude 类型列是后缀 token 流（FK → `ActiveSkillType.Id`，
//! `"AND"/"OR"/"NOT"` 是该表的特殊行）。求值规则：
//! - `OR`/`AND`：弹出一个值与新栈顶就地合并（或 / 与）；
//! - `NOT`：栈顶取反；
//! - 普通 token：压入「主动技能类型集合是否含该 token」；
//! - 收尾：栈内**任一**为真即匹配（残留多值 = 隐式 OR）。
//!
//! 空栈防御按蓝图裁决（m1-skills-gems §T3.3）：弹空按 `false`。
//! `minionTypes` 第二集合（CalcTools.lua:73）defer M5a（minion 技能链路）。

use std::collections::HashSet;

/// 后缀表达式 `expr` 是否匹配主动技能类型集合 `active`。空表达式恒不匹配
/// （require 列「空 = 接受」的判定在调用方 `can_support`，不在本函数）。
pub fn matches(expr: &[String], active: &HashSet<String>) -> bool {
    let mut stack: Vec<bool> = Vec::new();
    for token in expr {
        match token.as_str() {
            "OR" => {
                let other = stack.pop().unwrap_or(false);
                let top = stack.pop().unwrap_or(false);
                stack.push(top || other);
            }
            "AND" => {
                let other = stack.pop().unwrap_or(false);
                let top = stack.pop().unwrap_or(false);
                stack.push(top && other);
            }
            "NOT" => {
                let top = stack.pop().unwrap_or(false);
                stack.push(!top);
            }
            name => stack.push(active.contains(name)),
        }
    }
    stack.into_iter().any(|v| v)
}

#[cfg(test)]
mod tests {
    use super::matches;
    use std::collections::HashSet;

    fn expr(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    fn active(types: &[&str]) -> HashSet<String> {
        types.iter().map(|s| s.to_string()).collect()
    }

    /// 真实 token 流：`SupportAncestralWarriorTotemPlayer` require =
    /// `[Attack, Totemable, AND]`（来自入库 granted_effects.json）。
    #[test]
    fn real_require_and_expression() {
        let e = expr(&["Attack", "Totemable", "AND"]);
        assert!(matches(&e, &active(&["Attack", "Totemable", "Melee"])));
        assert!(!matches(&e, &active(&["Attack"])), "AND 需两者同真");
        assert!(!matches(&e, &active(&["Totemable"])));
    }

    /// 真实 token 流：`SupportMetaTotemBallistaPlayer` exclude =
    /// `[Meta, HasUsageCondition, Cooldown, Triggered, Persistent, UsedByProxy,
    ///   SupportedByBallistaTotem, NOT, AND]`——语义 = `Meta ∨ … ∨ Persistent ∨
    /// (UsedByProxy ∧ ¬SupportedByBallistaTotem)`（残留栈隐式 OR）。
    #[test]
    fn real_exclude_not_and_expression() {
        let e = expr(&[
            "Meta",
            "HasUsageCondition",
            "Cooldown",
            "Triggered",
            "Persistent",
            "UsedByProxy",
            "SupportedByBallistaTotem",
            "NOT",
            "AND",
        ]);
        // 普通远程攻击技能：无任一排除类型 → 不命中。
        assert!(!matches(
            &e,
            &active(&["Attack", "RangedAttack", "Projectile"])
        ));
        // 含 Triggered（残留栈成员）→ 命中。
        assert!(matches(&e, &active(&["Attack", "Triggered"])));
        // UsedByProxy 且未被弩炮图腾支援 → AND 段真 → 命中。
        assert!(matches(&e, &active(&["UsedByProxy"])));
        // UsedByProxy 但已被弩炮图腾支援 → AND 段假，其余皆假 → 不命中。
        assert!(!matches(
            &e,
            &active(&["UsedByProxy", "SupportedByBallistaTotem"])
        ));
    }

    /// 残留多值隐式 OR：`[Attack, Spell]` 任一在集合即匹配。
    #[test]
    fn residual_stack_any_true() {
        let e = expr(&["Attack", "Spell"]);
        assert!(matches(&e, &active(&["Spell"])));
        assert!(matches(&e, &active(&["Attack"])));
        assert!(!matches(&e, &active(&["Minion"])));
    }

    /// OR 操作符与残留隐式 OR 等价。
    #[test]
    fn explicit_or_operator() {
        let e = expr(&["Attack", "Spell", "OR"]);
        assert!(matches(&e, &active(&["Spell"])));
        assert!(!matches(&e, &active(&["Buff"])));
    }

    /// 纯 NOT：`[Triggered, NOT]` 在不含 Triggered 时匹配。
    #[test]
    fn pure_not() {
        let e = expr(&["Triggered", "NOT"]);
        assert!(matches(&e, &active(&["Attack"])));
        assert!(!matches(&e, &active(&["Triggered"])));
    }

    /// 边界：空表达式恒不匹配（「空 require = 接受」由调用方裁决）。
    #[test]
    fn empty_expression_never_matches() {
        assert!(!matches(&[], &active(&["Attack"])));
        assert!(!matches(&[], &active(&[])));
    }

    /// 边界：弹空按 false（蓝图裁决）——孤立操作符不 panic。
    #[test]
    fn pop_on_empty_stack_is_false() {
        // [AND]：false ∧ false = false。
        assert!(!matches(&expr(&["AND"]), &active(&["Attack"])));
        // [OR]：false ∨ false = false。
        assert!(!matches(&expr(&["OR"]), &active(&["Attack"])));
        // [NOT]：¬false = true（压回栈 → 匹配）。
        assert!(matches(&expr(&["NOT"]), &active(&["Attack"])));
    }
}
