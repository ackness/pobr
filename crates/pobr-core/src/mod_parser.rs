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

    let mut rest = normalize_spaces(original);
    let unsupported = ["mirrored", "split"];
    if unsupported.contains(&rest.as_str()) {
        return Ok(ParseOutcome {
            mods: Vec::new(),
            status: ParseStatus::Unsupported,
            unparsed: Some(original.into()),
        });
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

    let (form, after_form) = parse_form(&rest).ok_or_else(|| ParseError {
        input: original.into(),
        reason: "unsupported modifier form".into(),
    })?;

    let (mut remainder, mut tags) = strip_tags(after_form);
    let name = parse_name(&remainder).ok_or_else(|| ParseError {
        input: original.into(),
        reason: format!("unknown modifier name: {remainder}"),
    })?;

    if let Some(damage_type) = damage_type_for_name(name.as_str()) {
        tags.push(ModTag::DamageType(damage_type));
    }

    let mod_type = match form.kind {
        FormKind::Base => ModType::Base,
        FormKind::Inc => ModType::Inc,
        FormKind::More => ModType::More,
    };

    let mut modifier = Modifier::number(name, mod_type, form.value).with_source(original);
    if !flags.is_empty() {
        modifier = modifier.with_flags(flags);
    }
    for tag in tags {
        modifier = modifier.with_tag(tag);
    }

    remainder.clear();
    Ok(ParseOutcome {
        mods: vec![modifier],
        status: ParseStatus::Parsed,
        unparsed: None,
    })
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
    let name = match text.trim() {
        "damage" => "Damage",
        "physical damage" => "PhysicalDamage",
        "fire damage" => "FireDamage",
        "cold damage" => "ColdDamage",
        "lightning damage" => "LightningDamage",
        "chaos damage" => "ChaosDamage",
        "attack damage" => "AttackDamage",
        "attack speed" => "AttackSpeed",
        "cast speed" => "CastSpeed",
        "maximum life" => "MaximumLife",
        "life" => "MaximumLife",
        "maximum mana" => "MaximumMana",
        "mana" => "MaximumMana",
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
