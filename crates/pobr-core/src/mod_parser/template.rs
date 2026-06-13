//! 占位符模板实例化（蓝图 §3 / §4）——把规则表里带 `$n` / `:cap` 占位符的
//! tag 模板、flag 名数组实例化为 pobr [`ModTag`] / [`ModFlags`] / [`KeywordFlags`]。
//!
//! 占位符方言与 M5b `rules::special_mod` **同源**（`$n` 捕获、`:cap` 首字母
//! 大写拼接、`negate/div/mult/base` 算子）；数值算子链复用单点求值器
//! `rules::value_expr`（蓝图 §3 裁决：禁第二套方言）。本模块只新增 `:cap`
//! 字符串拼接的展开（M6-B 受限扩展，~139 闭包受益 >> 20 条目闸门）。

use pobr_data::catalog::parser_rules::TagTemplate;
use pobr_data::catalog::stat_map::StatMapValue;

use crate::{ActorRef, ModTag};
use pobr_data::modifier::{KeywordFlags, ModFlags};
use pobr_data::prelude::{DamageType, SkillTypes};

/// 把占位符字符串值按捕获实例化（`$n` 直引、`$n:cap` 首字母大写、段间 `+`
/// 拼接字面量；非占位符段原样）。vendor `firstToUpper(cap) .. "Effect"` →
/// 模板 `"$2:cap+Effect"`。
pub fn interpolate(template: &str, captures: &[String]) -> String {
    // 模板形态：`段1+段2+...`，每段是字面量或 `$n` / `$n:cap`。
    template
        .split('+')
        .map(|seg| interpolate_segment(seg, captures))
        .collect()
}

fn interpolate_segment(seg: &str, captures: &[String]) -> String {
    if let Some(rest) = seg.strip_prefix('$') {
        // `$n` 或 `$n:cap`
        let (idx_str, cap_op) = match rest.split_once(':') {
            Some((n, op)) => (n, Some(op)),
            None => (rest, None),
        };
        if let Ok(idx) = idx_str.parse::<usize>() {
            let raw = captures
                .get(idx.saturating_sub(1))
                .cloned()
                .unwrap_or_default();
            return match cap_op {
                Some("cap") => first_to_upper(&raw),
                _ => raw,
            };
        }
        // 非数字 → 当字面量（保 `$` 前缀）。
        seg.to_string()
    } else {
        seg.to_string()
    }
}

/// Lua `firstToUpper`：首字母大写，其余不变。
fn first_to_upper(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// 解析模板字段值为字符串（含占位符插值）。`$n` / `$n:cap` / 字面量。
fn field_text(value: &StatMapValue, captures: &[String]) -> Option<String> {
    match value {
        StatMapValue::Text(s) => Some(interpolate(s, captures)),
        StatMapValue::Number(n) => Some(n.to_string()),
        StatMapValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// 解析模板字段值为数值（`$n` 捕获取数 / 字面量）。
fn field_number(value: &StatMapValue, captures: &[String]) -> Option<f64> {
    match value {
        StatMapValue::Number(n) => Some(*n),
        StatMapValue::Text(s) => {
            if let Some(rest) = s.strip_prefix('$') {
                let n: usize = rest.split(':').next()?.parse().ok()?;
                captures.get(n.saturating_sub(1))?.parse().ok()
            } else {
                s.parse().ok()
            }
        }
        _ => None,
    }
}

fn field_bool(value: &StatMapValue) -> Option<bool> {
    match value {
        StatMapValue::Bool(b) => Some(*b),
        _ => None,
    }
}

/// 把 [`TagTemplate`] 实例化为 pobr [`ModTag`]（蓝图 §3 / §1.5）。
///
/// **可映射清单**（与 special_mod::compile_tag 同口径，扩展 Multiplier/PerStat/
/// ActorCondition 的 `$n` 字段）：
/// - `Multiplier`（var/div/limit/limitTotal/actor）；
/// - `Condition` / `ActorCondition`（var/neg/actor）；
/// - `SkillType`（skill_type 名）；
/// - `DamageType`（damageType 名）；
/// - `PerStat` / `PercentStat`（stat/div/limit）。
///
/// **不可映射**（无 pobr 落点，返回 `None`，行解析仍可产其余 mod；调用方据此
/// 把整行归为保守失配，见 engine）：`SkillName` / `GlobalEffect` / `ItemCondition`
/// / `MultiplierThreshold` / `StatThreshold` 等。
pub fn compile_tag(tag: &TagTemplate, captures: &[String]) -> Option<ModTag> {
    let f = &tag.fields;
    match tag.tag_type.as_str() {
        "Multiplier" => {
            let var = f.get("var").and_then(|v| field_text(v, captures))?;
            let div = f
                .get("div")
                .and_then(|v| field_number(v, captures))
                .unwrap_or(1.0);
            let limit = f.get("limit").and_then(|v| field_number(v, captures));
            let actor = f.get("actor").and_then(|v| field_text(v, captures));
            Some(ModTag::Multiplier {
                var,
                div,
                limit,
                actor: parse_actor(actor.as_deref()),
                limit_var: None,
                limit_actor: None,
                invert: false,
            })
        }
        "Condition" => {
            let var = f.get("var").and_then(|v| field_text(v, captures))?;
            let neg = f.get("neg").and_then(field_bool).unwrap_or(false);
            Some(ModTag::condition(var, neg))
        }
        "ActorCondition" => {
            let var = f.get("var").and_then(|v| field_text(v, captures))?;
            let neg = f.get("neg").and_then(field_bool).unwrap_or(false);
            let actor = f.get("actor").and_then(|v| field_text(v, captures));
            match actor.as_deref() {
                Some("enemy") => Some(ModTag::condition(format!("Enemy{var}"), neg)),
                _ => Some(ModTag::condition(var, neg)),
            }
        }
        "SkillType" => {
            let name = f.get("skill_type").and_then(|v| field_text(v, captures))?;
            let st = skill_type_bit(&name);
            (st != SkillTypes::NONE).then_some(ModTag::SkillTypes(st))
        }
        "DamageType" => {
            let name = f.get("damageType").and_then(|v| field_text(v, captures))?;
            damage_type_bit(&name).map(ModTag::DamageType)
        }
        "PerStat" | "PercentStat" => {
            let stat = f
                .get("stat")
                .or_else(|| f.get("var"))
                .and_then(|v| field_text(v, captures))?;
            let div = f
                .get("div")
                .and_then(|v| field_number(v, captures))
                .unwrap_or(1.0);
            let limit = f.get("limit").and_then(|v| field_number(v, captures));
            Some(ModTag::PerStat {
                stat,
                div,
                limit,
                limit_var: None,
                actor: None,
            })
        }
        // 未映射 tag 形态：保守跳过（返回 None；engine 据此处置整行）。
        _ => None,
    }
}

/// 是否为本模块「已知但 pobr 无落点」的 tag 类型（区别于真正未知类型，供 engine
/// 决定是否仍按部分支持产出）。当前保守：任何 compile_tag 返回 None 都算失配。
pub fn is_mappable_tag_type(tag_type: &str) -> bool {
    matches!(
        tag_type,
        "Multiplier"
            | "Condition"
            | "ActorCondition"
            | "SkillType"
            | "DamageType"
            | "PerStat"
            | "PercentStat"
    )
}

fn parse_actor(name: Option<&str>) -> Option<ActorRef> {
    match name {
        Some("player") => Some(ActorRef::Player),
        Some("parent") => Some(ActorRef::Parent),
        Some("minion") => Some(ActorRef::Minion),
        _ => None,
    }
}

/// ModFlag 名 → 位（与 special_mod::flag_bit 同口径）。未知名 → `None`。
pub fn flag_bit(name: &str) -> Option<ModFlags> {
    Some(match name {
        "Attack" => ModFlags::ATTACK,
        "Spell" => ModFlags::SPELL,
        "Hit" => ModFlags::HIT,
        "Dot" => ModFlags::DOT,
        "Cast" => ModFlags::CAST,
        "Melee" => ModFlags::MELEE,
        "Area" => ModFlags::AREA,
        "Projectile" => ModFlags::PROJECTILE,
        "Ailment" => ModFlags::AILMENT,
        "Weapon" => ModFlags::WEAPON,
        "Thorns" => ModFlags::THORNS,
        other => return ModFlags::weapon_type_bit(other),
    })
}

/// flag 名数组 → 位集合。
pub fn compile_flags(names: &[String]) -> ModFlags {
    names
        .iter()
        .fold(ModFlags::NONE, |acc, n| match flag_bit(n) {
            Some(bit) => acc | bit,
            None => acc,
        })
}

/// KeywordFlag 名 → 位（与 special_mod::keyword_bit 同口径）。
pub fn keyword_bit(name: &str) -> Option<KeywordFlags> {
    Some(match name {
        "Aura" => KeywordFlags::AURA,
        "Curse" => KeywordFlags::CURSE,
        "Totem" => KeywordFlags::TOTEM,
        "Attack" => KeywordFlags::ATTACK,
        "Spell" => KeywordFlags::SPELL,
        "Hit" => KeywordFlags::HIT,
        "Ailment" => KeywordFlags::AILMENT,
        "Poison" => KeywordFlags::POISON,
        "Bleed" => KeywordFlags::BLEED,
        "Ignite" => KeywordFlags::IGNITE,
        _ => return None,
    })
}

/// keyword 名数组 → 位集合。
pub fn compile_keyword_flags(names: &[String]) -> KeywordFlags {
    names
        .iter()
        .fold(KeywordFlags::NONE, |acc, n| match keyword_bit(n) {
            Some(bit) => acc | bit,
            None => acc,
        })
}

fn skill_type_bit(name: &str) -> SkillTypes {
    let bare = name.strip_prefix("SkillType:").unwrap_or(name);
    match bare {
        "Attack" => SkillTypes::ATTACK,
        "Spell" => SkillTypes::SPELL,
        "Projectile" => SkillTypes::PROJECTILE,
        "Area" => SkillTypes::AREA,
        "Melee" => SkillTypes::MELEE,
        "Triggered" => SkillTypes::TRIGGERED,
        "Minion" => SkillTypes::MINION,
        "Aura" => SkillTypes::AURA,
        "Channel" => SkillTypes::CHANNEL,
        _ => SkillTypes::NONE,
    }
}

fn damage_type_bit(name: &str) -> Option<DamageType> {
    Some(match name {
        "Physical" => DamageType::Physical,
        "Fire" => DamageType::Fire,
        "Cold" => DamageType::Cold,
        "Lightning" => DamageType::Lightning,
        "Chaos" => DamageType::Chaos,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn tag(ty: &str, fields: &[(&str, StatMapValue)]) -> TagTemplate {
        TagTemplate {
            tag_type: ty.to_string(),
            fields: fields
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn interpolate_capture_direct() {
        assert_eq!(interpolate("$1", &["5".into()]), "5");
        assert_eq!(interpolate("Rage", &[]), "Rage");
    }

    #[test]
    fn interpolate_cap_and_concat() {
        // "$2:cap+Effect" with cap "frenzy" → "FrenzyEffect"
        let caps = vec!["5".into(), "frenzy".into()];
        assert_eq!(interpolate("$2:cap+Effect", &caps), "FrenzyEffect");
    }

    #[test]
    fn multiplier_tag_with_capture_div() {
        let t = tag(
            "Multiplier",
            &[
                ("var", StatMapValue::Text("Rage".into())),
                ("div", StatMapValue::Text("$1".into())),
            ],
        );
        let got = compile_tag(&t, &["3".into()]).unwrap();
        match got {
            ModTag::Multiplier { var, div, .. } => {
                assert_eq!(var, "Rage");
                assert_eq!(div, 3.0);
            }
            _ => panic!("expected Multiplier"),
        }
    }

    #[test]
    fn multiplier_tag_cap_var() {
        let t = tag(
            "Multiplier",
            &[
                ("var", StatMapValue::Text("$2:cap+Effect".into())),
                ("div", StatMapValue::Text("$1".into())),
                ("actor", StatMapValue::Text("enemy".into())),
            ],
        );
        let got = compile_tag(&t, &["10".into(), "intimidate".into()]).unwrap();
        match got {
            ModTag::Multiplier {
                var, div, actor, ..
            } => {
                assert_eq!(var, "IntimidateEffect");
                assert_eq!(div, 10.0);
                assert_eq!(actor, None); // "enemy" 非 player/parent/minion → None（保守）
            }
            _ => panic!("expected Multiplier"),
        }
    }

    #[test]
    fn condition_tag() {
        let t = tag(
            "Condition",
            &[("var", StatMapValue::Text("Onslaught".into()))],
        );
        assert_eq!(
            compile_tag(&t, &[]).unwrap(),
            ModTag::condition("Onslaught", false)
        );
    }

    #[test]
    fn unmappable_tag_returns_none() {
        let t = tag(
            "SkillName",
            &[("skillName", StatMapValue::Text("Fireball".into()))],
        );
        assert!(compile_tag(&t, &[]).is_none());
        assert!(!is_mappable_tag_type("SkillName"));
    }

    #[test]
    fn flag_resolution() {
        let flags = compile_flags(&["Mace".into(), "Hit".into()]);
        assert!(flags.intersects(ModFlags::MACE));
        assert!(flags.intersects(ModFlags::HIT));
    }
}
