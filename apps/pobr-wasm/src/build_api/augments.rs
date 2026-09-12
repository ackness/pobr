//! Safe socket editing. Capacity and augment restrictions come from pinned PoB2 data.
use std::collections::BTreeMap;

use pobr_build::BuildData;
use pobr_data::catalog::{BaseItemDef, RuneDef, RuneSlotDef};
use pobr_gamedata::GameData;
use pobr_item::{ItemDraft, LineBucket, ModLineDraft};
use serde::{Deserialize, Serialize};

use super::{ApiError, localize_input_text};
use crate::state;

// Input/resource bounds, not game socket limits. Validate before any resize/allocation.
const MAX_ITEM_BYTES: usize = 131_072;
const MAX_INPUT_SOCKETS: u32 = 64;

#[derive(Debug, Serialize)]
struct RuneCatalogEntry {
    name: String,
    name_zh_tw: Option<String>,
    name_zh_cn: Option<String>,
    is_soul_core: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_level: Option<u32>,
    lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit_id: Option<String>,
    #[serde(skip)]
    socket_bound: bool,
}

#[derive(Debug, Serialize)]
struct ItemAugmentInfo {
    sockets: u32,
    max_sockets: u32,
    runes: Vec<String>,
    editable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    options: Vec<RuneCatalogEntry>,
}

impl ItemAugmentInfo {
    fn reject(&mut self, reason: &'static str) {
        self.editable = false;
        self.reason.get_or_insert(reason);
    }
}

struct ItemContext {
    draft: ItemDraft,
    info: ItemAugmentInfo,
}

fn normalize_input(text: &str, data: &BuildData, game: &GameData) -> Result<String, ApiError> {
    if text.len() > MAX_ITEM_BYTES {
        return Err(ApiError::bad_request("item_too_large"));
    }
    let localized = localize_input_text(text);
    let mut lines: Vec<String> = localized
        .lines()
        .map(|line| line.trim().to_string())
        .collect();
    let start = lines
        .iter()
        .position(|line| line.starts_with("Rarity:"))
        .ok_or_else(|| ApiError::bad_request("invalid_item"))?;
    lines.drain(..start);
    lines.retain(|line| !line.is_empty());
    let rarity = lines[0]
        .trim_start_matches("Rarity:")
        .trim()
        .to_ascii_uppercase();
    lines[0] = format!("Rarity: {rarity}");
    let rare = matches!(rarity.as_str(), "RARE" | "UNIQUE" | "RELIC");
    let base_index = if rare { 2 } else { 1 };
    let name = lines
        .get(base_index)
        .ok_or_else(|| ApiError::bad_request("invalid_item"))?
        .clone();
    let translations = [
        game.base_item_names("zh-CN").unwrap_or_default(),
        game.base_item_names("zh-TW").unwrap_or_default(),
    ];
    let resolve = |name: &str, allow_affixed: bool| -> Option<String> {
        if data.base_items.contains_key(name) {
            return Some(name.to_string());
        }
        if let Some(base) = data.base_items.values().find(|base| {
            translations
                .iter()
                .any(|map| map.get(&base.id).is_some_and(|alias| alias == name))
        }) {
            return Some(base.name.clone());
        }
        if !allow_affixed {
            return None;
        }
        data.base_items
            .values()
            .filter_map(|base| {
                let aliases = std::iter::once(base.name.as_str()).chain(
                    translations
                        .iter()
                        .filter_map(|map| map.get(&base.id).map(String::as_str)),
                );
                aliases
                    .filter(|alias| {
                        name.match_indices(alias).any(|(at, found)| {
                            crate::zh::has_cjk(found)
                                || (at == 0 || name[..at].ends_with(' '))
                                    && (at + found.len() == name.len()
                                        || name[at + found.len()..].starts_with(' '))
                        })
                    })
                    .map(|alias| (alias.len(), base.name.clone()))
                    .max_by_key(|(length, _)| *length)
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, name)| name)
    };
    // Game clipboard magic items can provide both their affixed name and a base line.
    let separate_base = !rare
        && lines
            .get(2)
            .is_some_and(|line| data.base_items.contains_key(line));
    let canonical = if separate_base {
        lines[2].clone()
    } else {
        resolve(&name, !rare).unwrap_or(name.clone())
    };
    lines[base_index] = canonical.clone();
    if separate_base {
        lines.remove(2);
    }
    if !rare && canonical != name {
        lines.insert(base_index + 1, format!("Note: {name}"));
    }
    // Header values can contain localized names even when the key is English.
    for line in &mut lines {
        for prefix in ["Rune:", "Soul Core:", "Idol:"] {
            if let Some(name) = line.strip_prefix(prefix) {
                let name = name.trim();
                *line = format!(
                    "Rune: {}",
                    resolve(name, false).unwrap_or_else(|| name.to_string())
                );
                break;
            }
        }
    }
    Ok(lines.join("\n"))
}

fn slot_types(base: &BaseItemDef, subtype: Option<&str>) -> Option<(&'static str, String)> {
    let specific = match (base.item_class.as_str(), subtype) {
        ("Warstaff", _) | (_, Some("Warstaff")) => "quarterstaff".into(),
        ("Shield", Some("Evasion")) => "buckler".into(),
        (name, _) => name.to_ascii_lowercase(),
    };
    let broad = if base.weapon.is_some() {
        "weapon"
    } else if base.armour.is_some() {
        "armour"
    } else if matches!(base.item_class.as_str(), "Wand" | "Staff" | "Sceptre") {
        "caster"
    } else {
        return None;
    };
    Some((broad, specific))
}

fn matching_slots<'a>(rune: &'a RuneDef, broad: &str, specific: &str) -> Vec<&'a RuneSlotDef> {
    rune.slots
        .iter()
        .filter(|(key, _)| key.as_str() == broad || key.as_str() == specific)
        .map(|(_, slot)| slot)
        .collect()
}

fn entry(
    rune: &RuneDef,
    slots: &[&RuneSlotDef],
    data: &BuildData,
    names: &[BTreeMap<String, String>; 2],
) -> RuneCatalogEntry {
    let id = data.base_items.get(&rune.name).map(|base| base.id.as_str());
    RuneCatalogEntry {
        name: rune.name.clone(),
        name_zh_tw: id.and_then(|id| names[0].get(id).cloned()),
        name_zh_cn: id.and_then(|id| names[1].get(id).cloned()),
        is_soul_core: rune.slots.values().any(|slot| slot.kind == "SoulCore"),
        kind: slots
            .first()
            .map(|slot| slot.kind.clone())
            .or_else(|| rune.slots.values().next().map(|slot| slot.kind.clone())),
        required_level: slots.iter().filter_map(|slot| slot.required_level).max(),
        lines: slots
            .iter()
            .flat_map(|slot| slot.lines.iter().cloned())
            .collect(),
        // Limits apply to the entire active loadout, independent of the queried slot.
        // Keep them in the context-free catalog used to identify other equipped augments.
        limit: rune.slots.values().filter_map(|slot| slot.limit).min(),
        limit_id: rune.slots.values().find_map(|slot| slot.limit_id.clone()),
        socket_bound: slots.iter().any(|slot| slot.socket_bound),
    }
}

fn unsupported_socket_mechanic(text: &str) -> bool {
    let line = text.to_ascii_lowercase();
    [
        "effect of socketed runes",
        "effect of socketed soul cores",
        "effect of socketed augment items",
        "upgrades a socketed rune",
        "socketed runes have",
        "socketed soul cores have",
        "socketed augments",
        "socketed idols",
        "jewel socket",
        "chakra",
        "augment sockets are",
        "runes are treated",
        "soul cores are treated",
        "augment items are treated",
        "this item gains bonuses from socketed",
    ]
    .iter()
    .any(|term| line.contains(term))
}

fn unmodeled_bonded_local(text: &str) -> bool {
    let Some(body) = text.strip_prefix("Bonded: ") else {
        return false;
    };
    let lower = body.to_ascii_lowercase();
    !lower.contains("global")
        && !lower.contains(" if ")
        && !lower.contains(" while ")
        && (lower.starts_with('+')
            && [
                " to armour",
                " to evasion rating",
                " to maximum energy shield",
            ]
            .iter()
            .any(|tail| lower.ends_with(tail))
            || lower.contains("% increased ")
                && [
                    "increased armour",
                    "increased evasion rating",
                    "increased energy shield",
                ]
                .iter()
                .any(|tail| lower.ends_with(tail)))
}

fn item_context(text: &str) -> Result<ItemContext, ApiError> {
    let game = state::game_data().map_err(ApiError::not_initialized)?;
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let normalized = normalize_input(text, &data, &game)?;
    let mut draft =
        ItemDraft::parse(&normalized).map_err(|_| ApiError::bad_request("invalid_item"))?;
    // Clipboard suffix annotations do not occupy separate lines in the implicit window.
    for line in &mut draft.lines {
        for (suffix, bucket) in [
            (" (implicit)", LineBucket::Implicit),
            (" (enchant)", LineBucket::Enchant),
            (" (rune)", LineBucket::Rune),
        ] {
            if let Some(body) = line.text.strip_suffix(suffix) {
                line.text = body.to_string();
                line.bucket = bucket;
                line.annotations.rune = bucket == LineBucket::Rune;
                line.annotations.enchant = bucket == LineBucket::Enchant;
                break;
            }
        }
    }
    let mut info = ItemAugmentInfo {
        sockets: draft.header.socket_count.min(MAX_INPUT_SOCKETS),
        max_sockets: 0,
        runes: draft
            .header
            .runes
            .iter()
            .take(MAX_INPUT_SOCKETS as usize)
            .map(|name| {
                if name == "None" {
                    String::new()
                } else {
                    name.clone()
                }
            })
            .collect(),
        editable: true,
        reason: None,
        options: vec![],
    };
    if normalized
        .lines()
        .filter_map(|line| line.strip_prefix("Sockets:"))
        .any(|value| {
            value
                .split_whitespace()
                .any(|socket| socket != "S" && socket != "J")
        })
        || draft.header.socket_count > MAX_INPUT_SOCKETS
        || draft.header.runes.len() > info.sockets as usize
    {
        info.reject("invalid_sockets");
    }
    info.runes.resize(info.sockets as usize, String::new());
    info.max_sockets = info.sockets;
    if draft.header.jewel_socket_count > 0 {
        info.reject("jewel_sockets");
    }
    let Some(base) = data.base_items.get(&draft.header.base_name) else {
        info.reject("unknown_base");
        return Ok(ItemContext { draft, info });
    };
    let overrides = game.base_item_overrides().map_err(|e| e.to_string())?;
    let base_override = overrides
        .as_ref()
        .and_then(|all| all.overrides.iter().find(|entry| entry.name == base.name));
    let limit = base_override.and_then(|entry| entry.socket_limit);
    let subtype = base_override.and_then(|entry| entry.sub_type.as_deref());
    let Some((broad, specific)) = slot_types(base, subtype) else {
        info.reject("unsupported_item_type");
        return Ok(ItemContext { draft, info });
    };
    match limit {
        Some(limit) if limit <= MAX_INPUT_SOCKETS => {
            if !draft.states.corrupted && !draft.states.mirrored && !draft.states.sanctified {
                info.max_sockets = info.sockets.max(limit);
            }
            if limit == 0 && info.sockets == 0 {
                info.reject("no_augment_sockets");
            }
        }
        _ => info.reject("unknown_socket_limit"),
    }
    if draft
        .lines
        .iter()
        .any(|line| unsupported_socket_mechanic(&line.text))
        || !draft.variant.names.is_empty()
    {
        info.reject("special_augment_rules");
    }
    if draft
        .lines
        .iter()
        .any(|line| line.bucket == LineBucket::Rune)
        && !info.runes.iter().any(|name| !name.is_empty())
    {
        info.reject("unknown_augments");
    }
    let runes = game
        .runes()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| ApiError::bad_request("missing_augment_data"))?;
    let names = [
        game.base_item_names("zh-TW").unwrap_or_default(),
        game.base_item_names("zh-CN").unwrap_or_default(),
    ];
    let unique = matches!(draft.header.rarity.as_str(), "UNIQUE" | "RELIC");
    let restricted_kind = if draft.lines.iter().any(|line| {
        line.text
            .eq_ignore_ascii_case("Only Soul Cores can be Socketed in this Item")
    }) {
        Some("SoulCore")
    } else if draft.lines.iter().any(|line| {
        line.text
            .eq_ignore_ascii_case("Only Runes can be Socketed in this Item")
    }) {
        Some("Rune")
    } else {
        None
    };
    for rune in &runes.runes {
        let slots: Vec<_> = matching_slots(rune, broad, &specific)
            .into_iter()
            .filter(|slot| {
                (!unique || slot.can_socket_in_unique_items)
                    && (!draft.states.corrupted && !draft.states.sanctified
                        || slot.can_socket_in_corrupted_sanctified)
                    && restricted_kind.is_none_or(|kind| kind == slot.kind)
                    && !slot.socket_bound
                    && !slot.lines.iter().any(|line| {
                        unsupported_socket_mechanic(line)
                            || broad == "armour" && unmodeled_bonded_local(line)
                            || line.contains("maximum Runic Ward")
                            || draft.header.spirit.is_some()
                                && line.to_ascii_lowercase().ends_with(" spirit")
                    })
            })
            .collect();
        if !slots.is_empty() {
            info.options.push(entry(rune, &slots, &data, &names));
        }
    }
    for name in info.runes.clone().iter().filter(|name| !name.is_empty()) {
        if !info.options.iter().any(|option| &option.name == name) {
            info.reject("unknown_augments");
        }
    }
    Ok(ItemContext { draft, info })
}

pub fn item_augment_info_json(text: &str) -> Result<String, String> {
    (|| {
        let info = item_context(text)?.info;
        serde_json::to_string(&info).map_err(|e| ApiError::bad_request(e.to_string()))
    })()
    .map_err(ApiError::into_json)
}

/// The older picker still receives the complete catalog, including empty lines for nonmatching entries.
pub fn rune_catalog_json(text: &str) -> Result<String, String> {
    (|| {
        let game = state::game_data().map_err(ApiError::not_initialized)?;
        let data = state::build_data().map_err(ApiError::not_initialized)?;
        let runes = game
            .runes()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| ApiError::bad_request("missing_augment_data"))?;
        let names = [
            game.base_item_names("zh-TW").unwrap_or_default(),
            game.base_item_names("zh-CN").unwrap_or_default(),
        ];
        let context = (!text.is_empty())
            .then(|| item_context(text).ok())
            .flatten();
        let entries: Vec<_> = runes
            .runes
            .iter()
            .map(|rune| {
                let mut result = entry(rune, &[], &data, &names);
                if let Some(option) = context.as_ref().and_then(|ctx| {
                    ctx.info
                        .options
                        .iter()
                        .find(|option| option.name == rune.name)
                }) {
                    result.lines = option.lines.clone();
                    result.required_level = option.required_level;
                }
                result
            })
            .collect();
        serde_json::to_string(&entries).map_err(|e| ApiError::bad_request(e.to_string()))
    })()
    .map_err(ApiError::into_json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReforgeRequest {
    text: String,
    runes: Vec<String>,
    #[serde(default)]
    sockets: Option<u32>,
}

pub fn reforge_runes_json(request: &str) -> Result<String, String> {
    reforge(request).map_err(ApiError::into_json)
}

fn reforge(request: &str) -> Result<String, ApiError> {
    if request.len() > MAX_ITEM_BYTES * 2 {
        return Err(ApiError::bad_request("item_too_large"));
    }
    let req: ReforgeRequest =
        serde_json::from_str(request).map_err(|_| ApiError::bad_request("invalid_request"))?;
    let ItemContext { mut draft, info } = item_context(&req.text)?;
    if !info.editable {
        return Err(ApiError::bad_request(
            info.reason.unwrap_or("unsupported_augments"),
        ));
    }
    let capacity = req.sockets.unwrap_or(info.sockets);
    if capacity > info.max_sockets || req.runes.len() > capacity as usize {
        return Err(ApiError::bad_request("socket_capacity"));
    }
    let mut counts: BTreeMap<&str, (u32, Option<u32>)> = BTreeMap::new();
    let mut new_lines = Vec::new();
    let mut names = Vec::new();
    let mut required_level = draft.header.level_req.unwrap_or(0);
    for name in &req.runes {
        if name.is_empty() || name == "None" {
            names.push("None".to_string());
            continue;
        }
        let option = info
            .options
            .iter()
            .find(|option| option.name == *name)
            .ok_or_else(|| ApiError::bad_request("incompatible_augment"))?;
        let key = option.limit_id.as_deref().unwrap_or(name);
        let (count, limit) = counts.entry(key).or_default();
        *count += 1;
        if let Some(next_limit) = option.limit {
            *limit = Some(limit.map_or(next_limit, |previous| previous.min(next_limit)));
        }
        if limit.is_some_and(|limit| *count > limit) || option.socket_bound {
            return Err(ApiError::bad_request("augment_limit"));
        }
        names.push(name.clone());
        required_level = required_level.max(option.required_level.unwrap_or(0));
        for line in &option.lines {
            new_lines.push(ModLineDraft {
                text: line.clone(),
                annotations: pobr_item::annotations::ModLineAnnotations {
                    rune: true,
                    ..Default::default()
                },
                bucket: LineBucket::Rune,
            });
        }
    }
    names.resize(capacity as usize, "None".into());
    draft.header.socket_count = capacity;
    draft.header.runes = names;
    if required_level > 0 {
        draft.header.level_req = Some(required_level);
    }
    draft.lines.retain(|line| line.bucket != LineBucket::Rune);
    draft.lines.extend(new_lines);
    // Displayed item defences already contain the previous runes. Known bases are
    // recomputed by the calc layer; stale import values must never become new bases.
    draft.header.armour = None;
    draft.header.evasion = None;
    draft.header.energy_shield = None;
    serde_json::to_string(&serde_json::json!({ "text": draft.build_raw() }))
        .map_err(|e| ApiError::bad_request(e.to_string()))
}
