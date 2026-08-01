//! Evaluator for the skill type **postfix expression** (the atomic unit of
//! support-applicability decisions).
//!
//! Mirrors PoB2 `Modules/CalcTools.lua:61-82 doesTypeExpressionMatch`:
//! a GrantedEffect's require/exclude type list is a postfix token stream (an
//! FK into `ActiveSkillType.Id`, with `"AND"/"OR"/"NOT"` as special rows in
//! that table). Evaluation rules:
//! - `OR`/`AND`: pop one value and merge it into the new top of stack (or /
//!   and) in place;
//! - `NOT`: negate the top of stack;
//! - an ordinary token: push whether the active skill type set contains
//!   that token;
//! - finally: a match if **any** value left on the stack is true (leftover
//!   multiple values = implicit OR).
//!
//! Empty-stack defense: popping an empty stack yields `false`.
//! `minionTypes`, the second set (CalcTools.lua:73), is deferred (minion
//! skill pipeline).

use std::collections::HashSet;

/// Whether postfix expression `expr` matches the active skill type set
/// `active`. An empty expression never matches — the "empty require list =
/// accept" rule lives in the caller (`can_support`), not here.
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

    /// A real token stream: `SupportAncestralWarriorTotemPlayer` require =
    /// `[Attack, Totemable, AND]` (taken from ingested granted_effects.json).
    #[test]
    fn real_require_and_expression() {
        let e = expr(&["Attack", "Totemable", "AND"]);
        assert!(matches(&e, &active(&["Attack", "Totemable", "Melee"])));
        assert!(!matches(&e, &active(&["Attack"])), "AND 需两者同真");
        assert!(!matches(&e, &active(&["Totemable"])));
    }

    /// A real token stream: `SupportMetaTotemBallistaPlayer` exclude =
    /// `[Meta, HasUsageCondition, Cooldown, Triggered, Persistent, UsedByProxy,
    ///   SupportedByBallistaTotem, NOT, AND]` — semantics = `Meta ∨ … ∨
    /// Persistent ∨ (UsedByProxy ∧ ¬SupportedByBallistaTotem)` (leftover
    /// stack = implicit OR).
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
        // A plain ranged-attack skill: none of the exclude types apply -> no match.
        assert!(!matches(
            &e,
            &active(&["Attack", "RangedAttack", "Projectile"])
        ));
        // Has Triggered (a leftover stack member) -> matches.
        assert!(matches(&e, &active(&["Attack", "Triggered"])));
        // UsedByProxy and not supported by a ballista totem -> the AND
        // segment is true -> matches.
        assert!(matches(&e, &active(&["UsedByProxy"])));
        // UsedByProxy but already supported by a ballista totem -> the AND
        // segment is false, and everything else is false too -> no match.
        assert!(!matches(
            &e,
            &active(&["UsedByProxy", "SupportedByBallistaTotem"])
        ));
    }

    /// Leftover-stack implicit OR: `[Attack, Spell]` matches if either is in
    /// the set.
    #[test]
    fn residual_stack_any_true() {
        let e = expr(&["Attack", "Spell"]);
        assert!(matches(&e, &active(&["Spell"])));
        assert!(matches(&e, &active(&["Attack"])));
        assert!(!matches(&e, &active(&["Minion"])));
    }

    /// The OR operator is equivalent to leftover implicit OR.
    #[test]
    fn explicit_or_operator() {
        let e = expr(&["Attack", "Spell", "OR"]);
        assert!(matches(&e, &active(&["Spell"])));
        assert!(!matches(&e, &active(&["Buff"])));
    }

    /// Pure NOT: `[Triggered, NOT]` matches when Triggered is absent.
    #[test]
    fn pure_not() {
        let e = expr(&["Triggered", "NOT"]);
        assert!(matches(&e, &active(&["Attack"])));
        assert!(!matches(&e, &active(&["Triggered"])));
    }

    /// Edge case: an empty expression never matches ("empty require =
    /// accept" is the caller's decision).
    #[test]
    fn empty_expression_never_matches() {
        assert!(!matches(&[], &active(&["Attack"])));
        assert!(!matches(&[], &active(&[])));
    }

    /// Edge case: popping an empty stack yields false — a lone operator
    /// doesn't panic.
    #[test]
    fn pop_on_empty_stack_is_false() {
        // [AND]: false ∧ false = false.
        assert!(!matches(&expr(&["AND"]), &active(&["Attack"])));
        // [OR]: false ∨ false = false.
        assert!(!matches(&expr(&["OR"]), &active(&["Attack"])));
        // [NOT]: ¬false = true (pushed back onto the stack -> matches).
        assert!(matches(&expr(&["NOT"]), &active(&["Attack"])));
    }
}
