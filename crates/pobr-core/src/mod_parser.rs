use std::fmt;

use pobr_data::prelude::*;

use crate::{ModTag, Modifier};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    Parsed,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutcome {
    pub mods: Vec<Modifier>,
    pub status: ParseStatus,
    pub unparsed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub input: String,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to parse modifier {:?}: {}",
            self.input, self.reason
        )
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy)]
enum FormKind {
    Base,
    Inc,
    More,
}

#[derive(Debug, Clone, Copy)]
struct Form {
    kind: FormKind,
    value: f64,
}

pub fn parse_mod(text: &str) -> Result<ParseOutcome, ParseError> {
    let original = text.trim();
    if original.is_empty() {
        return Err(ParseError {
            input: text.into(),
            reason: "empty modifier text".into(),
        });
    }

    // PoB 树/词条的 `[内部名|显示名]` / `[名]` 标记 → 取显示名（解析器按显示文本匹配）。
    let cleaned = strip_pob_brackets(original);
    let mut rest = normalize_spaces(&cleaned);
    let unsupported = ["mirrored", "split"];
    if unsupported.contains(&rest.as_str()) {
        return Ok(ParseOutcome {
            mods: Vec::new(),
            status: ParseStatus::Unsupported,
            unparsed: Some(original.into()),
        });
    }

    // 符文绑定词条前缀（PoB rune「Bonded: <mod>」）——剥离后按普通词条解析。
    if rest.starts_with("bonded: ") {
        rest = rest["bonded: ".len()..].to_string();
    }

    let mut flags = ModFlags::NONE;
    if let Some(stripped) = rest.strip_prefix("attacks deal ") {
        flags |= ModFlags::ATTACK;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("attacks ") {
        flags |= ModFlags::ATTACK;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("spells deal ") {
        flags |= ModFlags::SPELL;
        rest = stripped.into();
    } else if let Some(stripped) = rest.strip_prefix("spells ") {
        flags |= ModFlags::SPELL;
        rest = stripped.into();
    }

    // 「Adds N to M <type> Damage [to Attacks|to Spells]」——区间附加伤害，产出两条
    // `<Type>DamageMin/Max` BASE（attack/spell flag 由后缀决定）。这是攻击 build 装备
    // flat 附加伤害的主要形式，单独处理（parse_form 只产单值）。
    if let Some(mods) = parse_added_damage_range(&rest, flags, original) {
        return Ok(ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
        });
    }

    let (form, after_form) = parse_form(&rest).ok_or_else(|| ParseError {
        input: original.into(),
        reason: "unsupported modifier form".into(),
    })?;

    let (remainder, base_tags) = strip_tags(after_form);
    // 复合词条（`A, B and C`）+ 聚合名（`all elemental resistances`）→ 多个 ModName，共享同一 form。
    let names = resolve_names(&remainder).ok_or_else(|| ParseError {
        input: original.into(),
        reason: format!("unknown modifier name: {remainder}"),
    })?;

    let mod_type = match form.kind {
        FormKind::Base => ModType::Base,
        FormKind::Inc => ModType::Inc,
        FormKind::More => ModType::More,
    };

    let mods = names
        .into_iter()
        .map(|name| {
            let mut tags = base_tags.clone();
            if let Some(damage_type) = damage_type_for_name(name.as_str()) {
                tags.push(ModTag::DamageType(damage_type));
            }
            let mut m = Modifier::number(name, mod_type, form.value).with_source(original);
            if !flags.is_empty() {
                m = m.with_flags(flags);
            }
            for tag in tags {
                m = m.with_tag(tag);
            }
            m
        })
        .collect();

    Ok(ParseOutcome {
        mods,
        status: ParseStatus::Parsed,
        unparsed: None,
    })
}

/// 把词条名部分解析为一个或多个 [`ModName`]：处理聚合名（`all elemental resistances`）
/// 与复合名（`armour, evasion and energy shield`）。任一子名未知则返回 `None`。
fn resolve_names(text: &str) -> Option<Vec<ModName>> {
    let t = text.trim();
    // 聚合名：展开为多条。
    let aggregate: &[&str] = match t {
        "all elemental resistances" => {
            &["fire resistance", "cold resistance", "lightning resistance"]
        }
        "all attributes" | "any attribute" | "attributes" => {
            &["strength", "dexterity", "intelligence"]
        }
        _ => &[],
    };
    if !aggregate.is_empty() {
        return aggregate.iter().map(|n| parse_name(n)).collect();
    }
    // 复合名：`A, B and C` / `A and B` → 拆分。
    let normalized = t.replace(" and ", ", ");
    normalized
        .split(", ")
        .map(|part| parse_name(part.trim()))
        .collect()
}

/// 解析「adds N to M <type> damage [to attacks|to spells]」（`rest` 已小写、规范空格）。
/// 产出 `<Type>DamageMin/Max` BASE 两条。非此形式返回 `None`。
fn parse_added_damage_range(
    rest: &str,
    prefix_flags: ModFlags,
    source: &str,
) -> Option<Vec<Modifier>> {
    let body = rest.strip_prefix("adds ")?;
    let (min_str, after_min) = body.split_once(" to ")?;
    let min: f64 = min_str.trim().parse().ok()?;
    let (max_str, tail) = after_min.split_once(' ')?;
    let max: f64 = max_str.trim().parse().ok()?;

    // 后缀决定作用域（to attacks / to spells）；与前缀 flag 合并。
    let mut flags = prefix_flags;
    let tail = if let Some(t) = tail.strip_suffix(" to attacks") {
        flags |= ModFlags::ATTACK;
        t
    } else if let Some(t) = tail.strip_suffix(" to spells") {
        flags |= ModFlags::SPELL;
        t
    } else {
        tail
    };

    let type_word = tail.strip_suffix(" damage")?;
    let (pascal, damage_type) = match type_word {
        "physical" => ("Physical", DamageType::Physical),
        "fire" => ("Fire", DamageType::Fire),
        "cold" => ("Cold", DamageType::Cold),
        "lightning" => ("Lightning", DamageType::Lightning),
        "chaos" => ("Chaos", DamageType::Chaos),
        _ => return None,
    };

    let mk = |bound: &str, value: f64| {
        let mut m = Modifier::number(format!("{pascal}Damage{bound}"), ModType::Base, value)
            .with_source(source)
            .with_tag(ModTag::DamageType(damage_type));
        if !flags.is_empty() {
            m = m.with_flags(flags);
        }
        m
    };
    Some(vec![mk("Min", min), mk("Max", max)])
}

/// 解析 PoB 词条标记 `[A|B]` → `B`（显示名）、`[A]` → `A`。无标记原样返回。
fn strip_pob_brackets(text: &str) -> String {
    if !text.contains('[') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    break;
                }
                inner.push(ic);
            }
            // `[A|B]` → 取最后一段（显示名 B）；`[A]` → A。
            let display = inner.rsplit('|').next().unwrap_or(&inner);
            out.push_str(display);
        } else {
            out.push(c);
        }
    }
    out
}

fn normalize_spaces(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn parse_form(text: &str) -> Option<(Form, String)> {
    if let Some((number, rest)) = take_number_suffix(text, "% increased ") {
        return Some((
            Form {
                kind: FormKind::Inc,
                value: number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_number_suffix(text, "% reduced ") {
        return Some((
            Form {
                kind: FormKind::Inc,
                value: -number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_number_suffix(text, "% more ") {
        return Some((
            Form {
                kind: FormKind::More,
                value: number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_number_suffix(text, "% less ") {
        return Some((
            Form {
                kind: FormKind::More,
                value: -number,
            },
            rest.into(),
        ));
    }
    if let Some((number, rest)) = take_signed_number(text) {
        let rest = rest
            .strip_prefix("% to ")
            .or_else(|| rest.strip_prefix(" to "))
            .or_else(|| rest.strip_prefix("% "))
            .or_else(|| rest.strip_prefix(' '))
            .unwrap_or(rest);
        return Some((
            Form {
                kind: FormKind::Base,
                value: number,
            },
            rest.into(),
        ));
    }

    None
}

fn take_number_suffix<'a>(text: &'a str, suffix: &str) -> Option<(f64, &'a str)> {
    let (number, rest) = take_unsigned_number(text)?;
    rest.strip_prefix(suffix).map(|rest| (number, rest))
}

fn take_unsigned_number(text: &str) -> Option<(f64, &str)> {
    let end = text
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '.')
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()?;
    let number = text[..end].parse().ok()?;
    Some((number, &text[end..]))
}

fn take_signed_number(text: &str) -> Option<(f64, &str)> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if first != '+' && first != '-' {
        return None;
    }

    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if ch.is_ascii_digit() || ch == '.' {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }

    if end <= 1 {
        return None;
    }

    let number = text[..end].parse().ok()?;
    Some((number, &text[end..]))
}

fn strip_tags(text: String) -> (String, Vec<ModTag>) {
    let mut rest = text;
    let mut tags = Vec::new();

    for _ in 0..2 {
        let before = rest.clone();
        rest = strip_tag_once(&rest, &mut tags);
        if rest == before {
            break;
        }
    }

    (rest.trim().into(), tags)
}

fn strip_tag_once(text: &str, tags: &mut Vec<ModTag>) -> String {
    let known_tags = [
        (
            " while on full life",
            ModTag::Condition {
                var: "FullLife".into(),
                negated: false,
            },
        ),
        (
            " on full life",
            ModTag::Condition {
                var: "FullLife".into(),
                negated: false,
            },
        ),
        (
            " while not on full life",
            ModTag::Condition {
                var: "FullLife".into(),
                negated: true,
            },
        ),
        (
            " per power charge",
            ModTag::Multiplier {
                var: "PowerCharge".into(),
                limit: None,
            },
        ),
        (
            " per frenzy charge",
            ModTag::Multiplier {
                var: "FrenzyCharge".into(),
                limit: None,
            },
        ),
        (
            " per endurance charge",
            ModTag::Multiplier {
                var: "EnduranceCharge".into(),
                limit: None,
            },
        ),
    ];

    for (suffix, tag) in known_tags {
        if let Some(stripped) = text.strip_suffix(suffix) {
            tags.push(tag);
            return stripped.trim().into();
        }
    }

    text.into()
}

fn parse_name(text: &str) -> Option<ModName> {
    // `global` 作用域前缀对面板聚合无影响，剥离后按本体名解析。
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix("global ").unwrap_or(trimmed);
    let name = match trimmed {
        "damage" => "Damage",
        "physical damage" => "PhysicalDamage",
        "fire damage" => "FireDamage",
        "cold damage" => "ColdDamage",
        "lightning damage" => "LightningDamage",
        "chaos damage" => "ChaosDamage",
        "elemental damage" => "ElementalDamage",
        "attack damage" => "AttackDamage",
        "spell damage" => "SpellDamage",
        "projectile damage" => "ProjectileDamage",
        "area damage" => "AreaDamage",
        "elemental damage with attacks" => "ElementalDamage",
        "elemental damage with attack skills" => "ElementalDamage",
        "attack speed" => "AttackSpeed",
        "cast speed" => "CastSpeed",
        "movement speed" => "MovementSpeed",
        // 暴击（PoE2「Critical Hit」= 旧「Critical Strike」；计算读 CriticalStrike* ModName）。
        "critical hit chance" => "CriticalStrikeChance",
        "critical strike chance" => "CriticalStrikeChance",
        "critical hit damage bonus" => "CriticalStrikeMultiplier",
        "critical damage bonus" => "CriticalStrikeMultiplier",
        "critical strike multiplier" => "CriticalStrikeMultiplier",
        // 属性。
        "strength" => "Strength",
        "dexterity" => "Dexterity",
        "intelligence" => "Intelligence",
        "spirit" => "Spirit",
        "maximum life" => "MaximumLife",
        "life" => "MaximumLife",
        "maximum mana" => "MaximumMana",
        "mana" => "MaximumMana",
        "stun threshold" => "StunThreshold",
        "accuracy" => "Accuracy",
        "accuracy rating" => "Accuracy",
        "armour" => "Armour",
        "evasion" => "Evasion",
        "evasion rating" => "Evasion",
        "maximum energy shield" => "EnergyShield",
        "energy shield" => "EnergyShield",
        "fire resistance" => "FireResistance",
        "cold resistance" => "ColdResistance",
        "lightning resistance" => "LightningResistance",
        "chaos resistance" => "ChaosResistance",
        "maximum fire resistance" => "MaximumFireResistance",
        "maximum cold resistance" => "MaximumColdResistance",
        "maximum lightning resistance" => "MaximumLightningResistance",
        "maximum chaos resistance" => "MaximumChaosResistance",
        "all maximum elemental resistances" => "MaximumAllElementalResistances",
        "maximum elemental resistances" => "MaximumAllElementalResistances",
        _ => return None,
    };

    Some(ModName::from(name))
}

fn damage_type_for_name(name: &str) -> Option<DamageType> {
    match name {
        "PhysicalDamage" => Some(DamageType::Physical),
        "FireDamage" => Some(DamageType::Fire),
        "ColdDamage" => Some(DamageType::Cold),
        "LightningDamage" => Some(DamageType::Lightning),
        "ChaosDamage" => Some(DamageType::Chaos),
        _ => None,
    }
}
