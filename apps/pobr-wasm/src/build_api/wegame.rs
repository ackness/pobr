//! Public WeGame share responses -> the editable BuildJson contract.
//! Wire fields follow WeGame's Profile endpoints and PoB2 ImportTab.lua.
//! No networking here: the application supplies a versioned JSON bundle.

use super::*;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

#[derive(Deserialize)]
struct Share {
    format: String,
    version: u32,
    role: Role,
    equipments: Vec<Value>,
    talent_tree: TalentTree,
    jewel_data: String,
    skills: Vec<Value>,
}

#[derive(Deserialize)]
struct Role {
    level: u32,
    class_name: String,
}

#[derive(Deserialize)]
struct TalentTree {
    hashes: Vec<u32>,
    #[serde(default)]
    skill_overrides: BTreeMap<u32, Value>,
    #[serde(default)]
    specialisations: BTreeMap<String, Vec<u32>>,
    quest_stats: Vec<String>,
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// GGG keyword markup is display text, not a PoB annotation. Jewel roll
/// ranges are informational: keep the actual rolled number before `(min-max)`.
fn clean(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(']') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &rest[start + 1..start + end];
        out.push_str(inner.rsplit('|').next().unwrap_or(inner));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    let mut result = String::new();
    rest = &out;
    while let Some(start) = rest.find(" (") {
        result.push_str(&rest[..start]);
        let Some(end) = rest[start + 2..].find(')') else {
            result.push_str(&rest[start..]);
            return result;
        };
        let range = &rest[start + 2..start + 2 + end];
        if !range.contains('-')
            || !range
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
        {
            result.push_str(&rest[start..start + end + 3]);
        }
        rest = &rest[start + end + 3..];
    }
    result.push_str(rest);
    result
}

fn canonical(input: &str) -> String {
    let line = clean(input);
    state::zh_translator()
        .and_then(|t| t.translate_line(&line))
        .unwrap_or(line)
}

fn property(item: &Value, property_type: u64) -> Option<u32> {
    array(item, "properties").iter().find_map(|p| {
        if p["type"].as_u64() != Some(property_type) {
            return None;
        }
        let raw = p["values"][0][0].as_str()?;
        raw.trim_start_matches('+')
            .trim_end_matches('%')
            .parse()
            .ok()
    })
}

fn rarity(value: u64) -> &'static str {
    match value {
        0 => "NORMAL",
        1 => "MAGIC",
        3 => "UNIQUE",
        _ => "RARE",
    }
}

fn mod_lines(item: &Value, field: &str) -> Vec<String> {
    array(item, field)
        .iter()
        .flat_map(|m| {
            let raw = m.as_str().unwrap_or_else(|| text(m, "description"));
            raw.lines()
                .filter(|line| !line.trim().is_empty())
                .map(canonical)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn item_text(item: &Value, warnings: &mut BTreeSet<String>) -> Result<String, String> {
    let base = canonical(if text(item, "baseType").is_empty() {
        text(item, "typeLine")
    } else {
        text(item, "baseType")
    });
    if base.is_empty() {
        return Err("WeGame item is missing its base type".into());
    }
    let rarity = rarity(item["frameType"].as_u64().unwrap_or(2));
    let mut lines = vec![format!("Rarity: {rarity}")];
    if rarity != "NORMAL" {
        let name = canonical(text(item, "name"));
        lines.push(if name.is_empty() {
            "Imported Item".into()
        } else {
            name
        });
    }
    lines.extend([base, "--------".into()]);
    if let Some(level) = item["ilvl"].as_u64() {
        lines.push(format!("Item Level: {level}"));
    }
    if let Some(quality) = property(item, 6) {
        lines.push(format!("Quality: {quality}"));
    }
    let sockets = array(item, "sockets");
    if !sockets.is_empty() {
        let kinds: Vec<_> = sockets
            .iter()
            .map(|s| if text(s, "type") == "jewel" { "J" } else { "S" })
            .collect();
        lines.push(format!("Sockets: {}", kinds.join(" ")));
        for (index, socket) in sockets.iter().enumerate() {
            if text(socket, "type") == "jewel" {
                warnings.insert("Equipment-socketed jewels require manual review.".into());
                continue;
            }
            let rune = array(item, "socketedItems")
                .iter()
                .find(|r| r["socket"].as_u64() == Some(index as u64));
            let name = rune
                .map(|r| canonical(text(r, "baseType")))
                .unwrap_or_else(|| "None".into());
            lines.push(format!("Rune: {name}"));
        }
    }
    // Rolled defences are consumed by the existing item parser, which
    // prevents local modifiers from applying a second time.
    for p in array(item, "properties") {
        let label = match p["type"].as_u64() {
            Some(16) => "Armour",
            Some(17) => "Evasion Rating",
            Some(18) => "Energy Shield",
            _ => continue,
        };
        if let Some(value) = p["values"][0][0].as_str() {
            lines.push(format!("{label}: {value}"));
        }
    }
    if item["corrupted"].as_bool() == Some(true) {
        lines.push("Corrupted".into());
    }
    lines.push("--------".into());
    let implicit = mod_lines(item, "implicitMods");
    lines.push(format!("Implicits: {}", implicit.len()));
    lines.extend(implicit);
    lines.push("--------".into());
    for field in [
        "enchantMods",
        "runeMods",
        "explicitMods",
        "craftedMods",
        "fracturedMods",
    ] {
        for line in mod_lines(item, field) {
            // Shaman bond lines require a separate class-only model. Do not
            // turn the displayed conditional bonus into an unconditional mod.
            if line.starts_with("羁绊")
                || line.starts_with("Bonded:")
                || line.contains("ShamanOnlyMods")
            {
                warnings.insert("Shaman-only rune bonuses were not applied.".into());
                continue;
            }
            let prefix = match field {
                "enchantMods" => "{enchant}",
                "runeMods" => "{rune}",
                _ => "",
            };
            lines.push(format!("{prefix}{line}"));
        }
    }
    Ok(lines.join("\n"))
}

fn slot(item: &Value) -> Option<String> {
    Some(
        match text(item, "inventoryId") {
            "Weapon" => "weapon1",
            "Offhand" => "weapon2",
            "Helm" => "helmet",
            "BodyArmour" => "bodyarmour",
            "Gloves" => "gloves",
            "Boots" => "boots",
            "Amulet" => "amulet",
            "Belt" => "belt",
            "Ring" => "ring1",
            "Ring2" => "ring2",
            "Flask" => {
                return item["x"].as_u64().filter(|x| *x <= 4).map(|x| {
                    if x < 2 {
                        format!("Flask {}", x + 1)
                    } else {
                        format!("Charm {}", x - 1)
                    }
                });
            }
            _ => return None,
        }
        .into(),
    )
}

fn gem_names(data: &BuildData) -> Result<HashMap<String, BTreeSet<String>>, String> {
    let names = state::game_data()?
        .base_item_names("zh-CN")
        .map_err(|e| e.to_string())?;
    let mut result: HashMap<String, BTreeSet<String>> = HashMap::new();
    let base_names: HashMap<_, _> = data
        .base_items
        .values()
        .map(|b| (b.id.as_str(), b.name.as_str()))
        .collect();
    for gem in data.skill_gems.values() {
        if let Some(effect) = &gem.granted_effect_id {
            for name in [
                names.get(&gem.id).map(String::as_str),
                base_names.get(gem.id.as_str()).copied(),
            ]
            .into_iter()
            .flatten()
            {
                result
                    .entry(name.to_string())
                    .or_default()
                    .insert(effect.clone());
            }
        }
    }
    Ok(result)
}

fn gem(
    item: &Value,
    names: &HashMap<String, BTreeSet<String>>,
    warnings: &mut BTreeSet<String>,
) -> Option<GemJson> {
    let name = clean(text(item, "baseType"));
    let ids = names.get(&name);
    let Some(id) = ids.filter(|ids| ids.len() == 1).and_then(|ids| ids.first()) else {
        warnings.insert(format!("Unknown or ambiguous gem: {name}"));
        return None;
    };
    let level = property(item, 5).unwrap_or_else(|| {
        if item["support"].as_bool() != Some(true) {
            warnings.insert(format!("Missing gem level; using level 1: {name}"));
        }
        1
    });
    Some(GemJson {
        skill_id: id.clone(),
        level,
        quality: property(item, 6).unwrap_or(0),
    })
}

pub(super) fn decode(value: Value) -> Result<String, super::super::ApiError> {
    use super::super::ApiError;
    let file: Share = serde_json::from_value(value)
        .map_err(|e| ApiError::decode_error(format!("invalid WeGame share: {e}")))?;
    if file.format != "wegame" || file.version != 1 || !(1..=100).contains(&file.role.level) {
        return Err(ApiError::decode_error(
            "unsupported WeGame format, version, or character level",
        ));
    }
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let game = state::game_data().map_err(ApiError::not_initialized)?;
    let meta = game.passive_tree_meta().map_err(|e| e.to_string())?;
    let mut character = None;
    for class in &meta.classes {
        if class.name == file.role.class_name {
            character = Some(CharacterJson {
                level: file.role.level,
                class_name: class.name.clone(),
                ascendancy_name: String::new(),
            });
        }
        if let Some(asc) = class
            .ascendancies
            .iter()
            .find(|a| a.name == file.role.class_name)
        {
            character = Some(CharacterJson {
                level: file.role.level,
                class_name: class.name.clone(),
                ascendancy_name: asc.name.clone(),
            });
        }
    }
    let character = character.ok_or_else(|| {
        ApiError::decode_error(format!("unknown WeGame class: {}", file.role.class_name))
    })?;
    let mut warnings = BTreeSet::new();
    let mut nodes: BTreeSet<u32> = file.talent_tree.hashes.into_iter().collect();
    for (set, ids) in file.talent_tree.specialisations {
        if set == "set1" {
            nodes.extend(ids);
        } else if !ids.is_empty() {
            warnings.insert(format!("Inactive passive specialisation omitted: {set}"));
        }
    }
    for id in &nodes {
        if !data.passive_nodes.contains_key(id) {
            warnings.insert(format!("Unknown passive node: {id}"));
        }
    }
    let mut choices = BTreeMap::new();
    for (id, node) in file.talent_tree.skill_overrides {
        for (field, choice) in [
            ("grantedStrength", "str"),
            ("grantedDexterity", "dex"),
            ("grantedIntelligence", "int"),
        ] {
            if node[field].as_u64().is_some_and(|v| v > 0) {
                choices.insert(id, choice);
            }
        }
    }
    let mut equipped = Vec::new();
    let mut flasks = Vec::new();
    for item in file.equipments {
        let Some(slot) = slot(&item) else {
            warnings.insert(format!(
                "Equipment slot omitted: {}",
                text(&item, "inventoryId")
            ));
            continue;
        };
        let entry = SlotItemJson {
            slot,
            text: item_text(&item, &mut warnings)?,
        };
        if entry.slot.starts_with("Flask ") || entry.slot.starts_with("Charm ") {
            flasks.push(entry);
        } else {
            equipped.push(entry);
        }
    }
    // WeGame returns an extra quote-escape layer without outer JSON quotes.
    // Try ordinary JSON first so escaped backslashes in valid JSON stay intact.
    let jewels: Vec<Value> = serde_json::from_str(&file.jewel_data)
        .or_else(|_| serde_json::from_str(&file.jewel_data.replace("\\\"", "\"")))
        .map_err(|e| ApiError::decode_error(format!("invalid WeGame jewel data: {e}")))?;
    let mut socket_jewels = Vec::new();
    for entry in jewels {
        let jewel = &entry["jewel"];
        if jewel.is_null() {
            continue;
        }
        let socket = text(&entry, "socket_id");
        let Some(node) = data.passive_nodes.values().find(|n| n.id == socket) else {
            warnings.insert(format!("Unknown jewel socket: {socket}"));
            continue;
        };
        let base = data
            .base_items
            .values()
            .find(|b| b.id == text(jewel, "id"))
            .map(|b| b.name.clone())
            .unwrap_or_else(|| canonical(text(jewel, "name")));
        let mods: Vec<_> = array(jewel, "mod_descriptions")
            .iter()
            .flat_map(|group| array(group, "values").iter().cloned())
            .collect();
        let item = serde_json::json!({"baseType":base, "name":text(jewel,"display_name"), "frameType":jewel["rarity"], "explicitMods":mods});
        socket_jewels.push(SocketJewelJson {
            socket_node: node.skill,
            text: item_text(&item, &mut warnings)?,
        });
    }
    let names = gem_names(&data)?;
    let mut groups = Vec::new();
    for skill in file.skills {
        let Some(active) = gem(&skill, &names, &mut warnings) else {
            continue;
        };
        let active_id = active.skill_id.clone();
        let mut gems = vec![active];
        for support in array(&skill, "socketedItems") {
            if let Some(gem) = gem(support, &names, &mut warnings) {
                gems.push(gem);
            }
        }
        groups.push(SocketGroupJson {
            slot: None,
            enabled: true,
            source: None,
            active_skill_id: Some(active_id),
            gems,
        });
    }
    // Actual quest rewards replace the default fully-completed campaign.
    let mut config_inputs = BTreeMap::new();
    pobr_build::default_quest_stat_reward_texts(|key| {
        config_inputs.insert(key.to_string(), Value::Bool(false));
        Some(false)
    });
    config_inputs.insert(
        "questWeGame".into(),
        Value::String(
            file.talent_tree
                .quest_stats
                .iter()
                .map(|s| canonical(s))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    );
    warnings.insert("Check the main skill, combat configuration and resistance penalty before comparing upgrades.".into());
    let notes = format!(
        "WeGame import\n{}",
        warnings
            .into_iter()
            .map(|s| format!("[import] {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let json = BuildJson {
        character,
        tree: TreeJson {
            allocated_nodes: nodes.into_iter().collect(),
            tree_version: None,
            attribute_choices: choices,
        },
        items: ItemsJson {
            equipped,
            jewels: Vec::new(),
            socket_jewels,
            flasks,
        },
        socket_groups: groups,
        main_socket_group: None,
        config_inputs,
        notes: Some(notes),
        loadouts: vec![LoadoutJson {
            name: "Default".into(),
            tree: 1,
            item: None,
            skill: None,
        }],
        active_loadout: Some(0),
    };
    serde_json::to_string(&json).map_err(|e| ApiError::decode_error(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyword_markup_and_roll_ranges_keep_actual_values() {
        assert_eq!(
            clean("[Attack|攻击]附加 4 - 9 [Physical|物理]伤害"),
            "攻击附加 4 - 9 物理伤害"
        );
        assert_eq!(clean("速度提高 4 (4-8)% (local)"), "速度提高 4% (local)");
    }
}
