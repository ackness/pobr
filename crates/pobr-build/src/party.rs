//! PoB2 Party transport. Text blocks remain editable and round-trip verbatim.
use std::collections::BTreeMap;

use pobr_core::calc::session::PartyBuffSpec;
use pobr_core::calc::{BuffKind, BuffSpec};
use pobr_core::{ModValue, Modifier};
use pobr_data::prelude::*;
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};

use crate::{BuildData, BuildError, DataOrchestratorOptions, XmlError};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Party {
    pub members: Vec<PartyMember>,
    /// The source build's outgoing blocks, never applied to itself.
    pub exports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PartyMember {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub blocks: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}
impl Default for PartyMember {
    fn default() -> Self {
        Self {
            id: "1".into(),
            name: "Party member".into(),
            enabled: true,
            blocks: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }
}

pub fn parse_party(xml: &str) -> Result<Party, XmlError> {
    let mut reader = Reader::from_str(xml);
    let mut party = Party::default();
    let mut imported = BTreeMap::new();
    let mut in_party = false;
    let mut block: Option<(String, bool)> = None;
    let mut text = String::new();
    let mut member = None;
    loop {
        match reader
            .read_event()
            .map_err(|e| XmlError::Parse(e.to_string()))?
        {
            Event::Start(e) if e.name().as_ref() == "Party" => in_party = true,
            Event::Start(e) if in_party && e.name().as_ref() == "PoBRMember" => {
                let mut m = PartyMember::default();
                for a in e.attributes().flatten() {
                    let v = quick_xml::escape::unescape(&a.value)
                        .map_err(|e| XmlError::Parse(e.to_string()))?
                        .into_owned();
                    match a.key.as_ref() {
                        "id" => m.id = v,
                        "name" => m.name = v,
                        "enabled" => m.enabled = v != "false",
                        _ => {}
                    }
                }
                member = Some(m);
            }
            Event::Start(e)
                if in_party && matches!(e.name().as_ref(), "ImportedBuffs" | "ExportedBuffs") =>
            {
                let name = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "name")
                    .and_then(|a| {
                        quick_xml::escape::unescape(&a.value)
                            .ok()
                            .map(|v| v.into_owned())
                    })
                    .unwrap_or_default();
                block = Some((name, e.name().as_ref() == "ExportedBuffs"));
                text.clear();
            }
            Event::Text(e) if block.is_some() => text.push_str(&e),
            Event::CData(e) if block.is_some() => text.push_str(&e),
            Event::GeneralRef(e) if block.is_some() => {
                let entity = format!("&{};", &*e);
                text.push_str(
                    &quick_xml::escape::unescape(&entity)
                        .map_err(|e| XmlError::Parse(e.to_string()))?,
                );
            }
            Event::End(e) if matches!(e.name().as_ref(), "ImportedBuffs" | "ExportedBuffs") => {
                if let Some((name, exported)) = block.take() {
                    let dest = if let Some(m) = &mut member {
                        &mut m.blocks
                    } else if exported {
                        &mut party.exports
                    } else {
                        &mut imported
                    };
                    dest.insert(name, text.clone());
                }
            }
            Event::End(e) if e.name().as_ref() == "PoBRMember" => {
                if let Some(m) = member.take() {
                    party.members.push(m);
                }
            }
            Event::End(e) if e.name().as_ref() == "Party" => break,
            Event::Eof => break,
            _ => {}
        }
    }
    if party.members.is_empty() && !imported.is_empty() {
        party.members.push(PartyMember {
            name: "Imported party".into(),
            blocks: imported,
            ..Default::default()
        });
    }
    Ok(party)
}

/// Standard blocks for PoB2, with an ignorable extension retaining member boundaries.
pub fn party_xml(party: &Party) -> String {
    let esc = |s: &str| quick_xml::escape::escape(s).into_owned();
    let blocks = |tag: &str, blocks: &BTreeMap<String, String>| {
        blocks
            .iter()
            .map(|(name, text)| format!("<{tag} name=\"{}\">{}</{tag}>", esc(name), esc(text)))
            .collect::<String>()
    };
    let mut combined: BTreeMap<String, String> = BTreeMap::new();
    for member in party.members.iter().filter(|m| m.enabled) {
        for (name, text) in &member.blocks {
            let entry = combined.entry(name.clone()).or_default();
            if !entry.is_empty() {
                entry.push('\n');
            }
            // Each curse block has one limit header. The combined party uses the largest.
            if name == "Curse" && !entry.is_empty() {
                let mut lines = text.lines().filter(|line| !line.starts_with("--- Curse"));
                let incoming = lines
                    .next()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(1.0);
                let existing = entry
                    .lines()
                    .find_map(|s| s.parse::<f64>().ok())
                    .unwrap_or(1.0);
                let body = entry
                    .lines()
                    .skip_while(|s| s.parse::<f64>().is_err())
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join("\n");
                *entry = format!(
                    "--- Curse Limit ---\n{}\n{}\n{}",
                    incoming.max(existing),
                    body,
                    lines.collect::<Vec<_>>().join("\n")
                );
            } else {
                entry.push_str(text);
            }
        }
    }
    let mut xml = format!(
        "<Party>{}{}",
        blocks("ImportedBuffs", &combined),
        blocks("ExportedBuffs", &party.exports)
    );
    for member in &party.members {
        xml.push_str(&format!(
            "<PoBRMember id=\"{}\" name=\"{}\" enabled=\"{}\">{}</PoBRMember>",
            esc(&member.id),
            esc(&member.name),
            member.enabled,
            blocks("ImportedBuffs", &member.blocks)
        ));
    }
    xml.push_str("</Party>");
    xml
}

pub struct PartyEffects {
    pub buffs: Vec<PartyBuffSpec>,
    pub enemy: Vec<Modifier>,
    pub conditions: Vec<String>,
    pub lines: Vec<(SourceId, String, bool)>,
}

pub fn member_effects(member: &PartyMember, index: usize) -> PartyEffects {
    let mut out = PartyEffects {
        buffs: Vec::new(),
        enemy: Vec::new(),
        conditions: Vec::new(),
        lines: Vec::new(),
    };
    let stats: BTreeMap<_, _> = member
        .blocks
        .get("PartyMemberStats")
        .into_iter()
        .flat_map(|s| s.lines())
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((
                key.split('|').next()?.to_string(),
                value.parse::<f64>().ok()?,
            ))
        })
        .collect();
    for (destination, text) in &member.blocks {
        let kind = match destination.as_str() {
            "Aura" => BuffKind::Aura,
            "Curse" => BuffKind::Curse,
            "Warcry Skills" => BuffKind::Warcry,
            "Link Skills" => BuffKind::Link,
            _ => BuffKind::Debuff,
        };
        let source = SourceId::new(
            SourceKind::Buff,
            format!("party.{}.{}", member.id, destination),
        );
        if destination == "PartyMemberStats" {
            continue;
        }
        let mut limit = 1.0;
        let mut need_limit = kind == BuffKind::Curse;
        let mut spec: Option<BuffSpec> = None;
        let mut phase = 0;
        let mut other_effects = false;
        for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
            if line.starts_with("---") {
                if line == "---" {
                    phase = 0;
                }
                continue;
            }
            if destination == "EnemyConditions" {
                let condition = line.strip_prefix("Condition:").unwrap_or(line);
                out.conditions.push(format!("Enemy{condition}"));
                out.lines.push((source.clone(), line.into(), true));
                continue;
            }
            if need_limit {
                limit = line.parse::<f64>().unwrap_or(1.0).clamp(0.0, 100.0);
                need_limit = false;
                continue;
            }
            if destination == "EnemyMods" && !line.contains('|') {
                continue;
            }
            if !line.contains('|') && phase == 0 {
                other_effects = line == "otherEffects";
                if let Some(spec) = spec.take() {
                    out.buffs.push(PartyBuffSpec {
                        member: index,
                        curse_limit: limit,
                        spec,
                    });
                }
                spec = Some(BuffSpec {
                    name: line.trim_end_matches("_Debuff").into(),
                    kind,
                    skill_id: source.id.clone(),
                    mods: Vec::new(),
                    magnitude: 1.0,
                    slot: None,
                    socket_index: 0,
                    is_mark: false,
                    ignore_curse_limit: false,
                    local_effect_inc: 0.0,
                    local_effect_more: 1.0,
                    skill_types: SkillTypes::NONE,
                });
                phase =
                    if matches!(line, "extraAura" | "otherEffects") || destination == "EnemyMods" {
                        3
                    } else {
                        1
                    };
                continue;
            }
            if phase == 1 {
                if let Some(spec) = &mut spec {
                    spec.magnitude = line.parse::<f64>().unwrap_or(100.0) / 100.0;
                }
                phase = if kind == BuffKind::Curse { 2 } else { 3 };
                continue;
            }
            if phase == 2 {
                if let Some(spec) = &mut spec {
                    spec.is_mark = line == "true";
                }
                phase = 3;
                continue;
            }
            if other_effects {
                let mut fields = line.splitn(3, '|');
                let name = fields.next().unwrap_or_default();
                let strength = fields.next().and_then(|v| v.parse::<f64>().ok());
                let payload = fields.next();
                if let (Some(strength), Some(payload), Some(template)) = (strength, payload, &spec)
                    && let Some(modifier) = parse_modifier(payload, &source, &stats)
                {
                    let mut buff = template.clone();
                    buff.name = name.into();
                    buff.kind = BuffKind::Buff;
                    buff.magnitude = strength / 100.0;
                    buff.mods = vec![modifier];
                    out.buffs.push(PartyBuffSpec {
                        member: index,
                        curse_limit: limit,
                        spec: buff,
                    });
                    continue;
                }
                out.lines.push((source.clone(), line.into(), false));
                continue;
            }
            let parsed = parse_modifier(line, &source, &stats);
            out.lines.push((
                parsed
                    .as_ref()
                    .and_then(|m| m.origin.as_ref())
                    .map(|o| o.source_id.clone())
                    .unwrap_or_else(|| source.clone()),
                line.into(),
                parsed.is_some(),
            ));
            if let Some(modifier) = parsed {
                if destination == "EnemyMods" {
                    out.enemy.push(modifier);
                } else if let Some(spec) = &mut spec {
                    spec.mods.push(modifier);
                }
            }
        }
        if let Some(spec) = spec
            && destination != "EnemyMods"
            && !spec.mods.is_empty()
        {
            out.buffs.push(PartyBuffSpec {
                member: index,
                curse_limit: limit,
                spec,
            });
        }
    }
    out
}

fn parse_modifier(
    line: &str,
    source: &SourceId,
    stats: &BTreeMap<String, f64>,
) -> Option<Modifier> {
    use pobr_core::rules::stat_map_engine::translate_tag;
    use pobr_data::catalog::stat_map::StatMapValue;
    let fields: Vec<_> = line.split('|').collect();
    if fields.len() != 7 {
        return None;
    }
    let mod_type = match fields[3] {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        _ => return None,
    };
    let value = if mod_type == ModType::Flag {
        ModValue::Bool(fields[0] == "true")
    } else {
        ModValue::Number(fields[0].parse::<f64>().ok().filter(|v| v.is_finite())?)
    };
    let enemy = source.id.ends_with(".Curse") || source.id.ends_with(".EnemyMods");
    let name = match fields[2] {
        "Life" if !enemy => "MaximumLife",
        "Mana" if !enemy => "MaximumMana",
        "FireResist" if !enemy => "FireResistance",
        "ColdResist" if !enemy => "ColdResistance",
        "LightningResist" if !enemy => "LightningResistance",
        "ChaosResist" if !enemy => "ChaosResistance",
        "CritChance" if !enemy => "CriticalStrikeChance",
        "CritMultiplier" if !enemy => "CriticalStrikeMultiplier",
        name => name,
    };
    let mut modifier = Modifier::new(name, mod_type, value);
    for name in fields[4].split(',').filter(|s| *s != "-") {
        let flag = match name {
            "Attack" => ModFlags::ATTACK,
            "Spell" => ModFlags::SPELL,
            "Hit" => {
                modifier.keyword_flags = modifier.keyword_flags | KeywordFlags::HIT;
                ModFlags::NONE
            }
            "Dot" => ModFlags::DOT,
            "Cast" => ModFlags::CAST,
            "Thorns" => ModFlags::THORNS,
            "Melee" => ModFlags::MELEE,
            "Area" => ModFlags::AREA,
            "Projectile" => ModFlags::PROJECTILE,
            "Ailment" => ModFlags::AILMENT,
            "MeleeHit" => ModFlags::MELEE_HIT,
            "Weapon" => ModFlags::WEAPON,
            "WeaponMelee" => ModFlags::WEAPON_MELEE,
            "WeaponRanged" => ModFlags::WEAPON_RANGED,
            "Weapon1H" => ModFlags::WEAPON_1H,
            "Weapon2H" => ModFlags::WEAPON_2H,
            other => ModFlags::weapon_type_bit(other)?,
        };
        modifier.flags |= flag;
    }
    for name in fields[5].split(',').filter(|s| *s != "-") {
        modifier.keyword_flags = modifier.keyword_flags | KeywordFlags::from_name(name)?;
    }
    for tag in fields[6].split(',').filter(|s| *s != "-") {
        let mut map = BTreeMap::new();
        for entry in tag.split('/') {
            let (key, value) = entry.split_once('=')?;
            let value = if value == "true" || value == "false" {
                StatMapValue::Bool(value == "true")
            } else if let Ok(number) = value.parse::<f64>() {
                StatMapValue::Number(number)
            } else {
                StatMapValue::Text(value.into())
            };
            map.insert(key.to_string(), value);
        }
        if matches!(map.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect") {
            // This tag routes the buff to its destination; recipient scaling is in buff_pass.
            if map.keys().any(|k| {
                !["type", "effectType", "div", "limit", "warcryPowerBonus"].contains(&k.as_str())
            }) {
                return None;
            }
            if map.contains_key("div") {
                let Some(StatMapValue::Number(power)) = map.get("warcryPowerBonus") else {
                    return None;
                };
                if let ModValue::Number(v) = &mut modifier.value {
                    *v *= power;
                }
            }
            continue;
        }
        if matches!(map.get("actor"), Some(StatMapValue::Text(a)) if ["parent", "partyMembers"].contains(&a.as_str()))
        {
            if !matches!(map.get("type"), Some(StatMapValue::Text(t)) if ["PerStat", "PercentStat", "Multiplier"].contains(&t.as_str()))
            {
                return None;
            }
            if map
                .keys()
                .any(|k| !["type", "actor", "stat", "var", "div", "limit"].contains(&k.as_str()))
            {
                return None;
            }
            let Some(StatMapValue::Text(stat)) = map.get("stat").or_else(|| map.get("var")) else {
                return None;
            };
            let amount = *stats.get(stat)?;
            let div = match map.get("div") {
                Some(StatMapValue::Number(v)) => *v,
                _ => 1.0,
            };
            if div <= 0.0 {
                return None;
            }
            let factor = if matches!(map.get("type"), Some(StatMapValue::Text(t)) if t == "PercentStat")
            {
                amount / 100.0
            } else {
                (amount / div).floor()
            };
            let factor = match map.get("limit") {
                Some(StatMapValue::Number(limit)) => factor.min(*limit),
                _ => factor,
            };
            if let ModValue::Number(v) = &mut modifier.value {
                *v *= factor;
            }
            continue;
        }
        modifier.tags.push(translate_tag(&map).ok()?);
    }
    let id = SourceId::new(SourceKind::Buff, format!("{}.{}", source.id, fields[1]));
    modifier.origin = Some(ModifierSource::new(id).with_raw_text(line));
    Some(modifier)
}

pub fn import_member(code: &str, data: &BuildData) -> Result<PartyMember, BuildError> {
    let mut build = crate::parse_build_from_code(code)?;
    let mut member = PartyMember {
        name: format!(
            "{} level {}",
            build.character.class_name, build.character.level
        ),
        blocks: build.party.exports.clone(),
        ..Default::default()
    };
    build.party = Party::default();
    let session =
        crate::calculate_with_data_session(&build, data, &DataOrchestratorOptions::default())?;
    let output = session.output();
    member.blocks.insert(
        "PartyMemberStats".into(),
        format!(
            "Life={}\nEnergyShield={}\nMana={}\nStrength={}\nDexterity={}\nIntelligence={}",
            output.life,
            output.energy_shield,
            output.mana,
            output.strength,
            output.dexterity,
            output.intelligence
        ),
    );
    let mut resolved: BTreeMap<String, String> = BTreeMap::new();
    for spec in session.party_buff_exports() {
        let destination = if spec.kind == BuffKind::Curse {
            "Curse"
        } else {
            "Aura"
        };
        let mut lines = Vec::new();
        for m in &spec.mods {
            // Unsupported serialized tags stay in the exporting tool's original block.
            if !m.tags.is_empty() || !matches!(m.value, ModValue::Number(_) | ModValue::Bool(_)) {
                member.warnings.push(format!("{}: {} could not be exported; retain the original Party block for this effect.", spec.name, m.name));
                continue;
            }
            let value = match m.value {
                ModValue::Number(v) => v.to_string(),
                ModValue::Bool(v) => v.to_string(),
                _ => unreachable!(),
            };
            let type_name = match m.mod_type {
                ModType::Base => "BASE",
                ModType::Inc => "INC",
                ModType::More => "MORE",
                ModType::Flag => "FLAG",
                ModType::Override => "OVERRIDE",
                _ => continue,
            };
            let names = |v: Vec<String>| {
                if v.is_empty() {
                    "-".into()
                } else {
                    v.join(",")
                }
            };
            let name = match m.name.as_str() {
                "MaximumLife" => "Life",
                "MaximumMana" => "Mana",
                "FireResistance" => "FireResist",
                "ColdResistance" => "ColdResist",
                "LightningResistance" => "LightningResist",
                "ChaosResistance" => "ChaosResist",
                "CriticalStrikeChance" => "CritChance",
                "CriticalStrikeMultiplier" => "CritMultiplier",
                name => name,
            };
            let source = m
                .origin
                .as_ref()
                .map(|o| o.source_id.id.clone())
                .unwrap_or_else(|| format!("Skill:{}", spec.skill_id));
            lines.push(format!("{value}|{source}|{name}|{type_name}|{}|{}|type=GlobalEffect/effectType={destination}", names(m.flags.names()), names(m.keyword_flags.names())));
        }
        if lines.is_empty() {
            continue;
        }
        let block = resolved.entry(destination.into()).or_insert_with(|| {
            if destination == "Curse" {
                format!(
                    "--- Curse Limit ---\n{}\n",
                    output.enemy_curse_limit.max(1.0)
                )
            } else {
                String::new()
            }
        });
        block.push_str(&format!(
            "{}\n{}\n{}{}\n---\n",
            spec.name,
            spec.magnitude * 100.0,
            if spec.kind == BuffKind::Curse {
                format!("{}\n", spec.is_mark)
            } else {
                String::new()
            },
            lines.join("\n")
        ));
    }
    // Compute supported effects from the current build; keep other exported effects.
    for (name, text) in resolved {
        let existing = member.blocks.remove(&name).unwrap_or_default();
        member
            .blocks
            .insert(name.clone(), merge_resolved_blocks(&name, &text, &existing));
    }
    if !member.blocks.contains_key("Aura") && !member.blocks.contains_key("Curse") {
        member
            .warnings
            .push("No shareable aura or curse could be resolved from this build.".into());
    }
    Ok(member)
}

/// Replace supported effects by name, keeping every unmodelled exported block intact.
fn merge_resolved_blocks(destination: &str, resolved: &str, exported: &str) -> String {
    fn sections(text: &str, curse: bool) -> Vec<String> {
        let mut lines = text.lines().filter(|line| !line.starts_with("--- Curse"));
        if curse {
            lines.next();
        }
        let mut sections = Vec::new();
        let mut block = String::new();
        for line in lines {
            if line == "---" {
                if !block.trim().is_empty() {
                    sections.push(std::mem::take(&mut block));
                }
            } else {
                block.push_str(line);
                block.push('\n');
            }
        }
        if !block.trim().is_empty() {
            sections.push(block);
        }
        sections
    }
    let curse = destination == "Curse";
    let native = sections(resolved, curse);
    let names: Vec<_> = native
        .iter()
        .filter_map(|s| s.lines().find(|line| !line.trim().is_empty()))
        .collect();
    let mut result = resolved.to_string();
    for block in sections(exported, curse) {
        if !block
            .lines()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|name| names.contains(&name))
        {
            result.push_str(&block);
            result.push_str("---\n");
        }
    }
    result
}
