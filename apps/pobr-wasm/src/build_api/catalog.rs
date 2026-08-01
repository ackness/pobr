//! Catalog/text-oriented read-only endpoints: per-line item coloring
//! (`classify_item_lines_json`), the gem picker's catalog
//! (`gem_catalog_json`), the rune/soul core catalog plus re-socketing
//! (`rune_catalog_json` / `reforge_runes_json`), and English -> Simplified
//! Chinese display translation (`translate_lines_to_zh_cn_json`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::state;

// classify_item_lines_json (item text -> per-line categories, for Items panel coloring)

/// A single display line (`text` has annotations stripped; `kind` is used for frontend coloring).
#[derive(Debug, Serialize)]
struct ItemLineJson {
    text: String,
    /// `name` / `base` / `struct` / `implicit` / `explicit` / `enchant` / `rune` / `class_req`.
    kind: &'static str,
    /// The affix tier (1 = strongest in its pool; only given for rare/magic/normal explicit lines with a reverse-lookup hit).
    #[serde(skip_serializing_if = "Option::is_none")]
    tier: Option<u32>,
    /// The total number of tiers rollable on this base within the pool (paired with `tier`).
    #[serde(skip_serializing_if = "Option::is_none")]
    tier_total: Option<u32>,
    /// The affix kind: `prefix` / `suffix` (paired with `tier`).
    #[serde(skip_serializing_if = "Option::is_none")]
    affix: Option<&'static str>,
}

fn display_line_kind_str(kind: pobr_item::DisplayLineKind) -> &'static str {
    use pobr_item::DisplayLineKind::*;
    match kind {
        Name => "name",
        Base => "base",
        Struct => "struct",
        Implicit => "implicit",
        Explicit => "explicit",
        Enchant => "enchant",
        Rune => "rune",
        ClassReq => "class_req",
    }
}

/// Splits a PoB item text block into ordered display lines plus categories
/// (the parsing itself doesn't need game data).
///
/// Reuses `pobr_item::classify_display_lines` (the same bucket
/// classification rules as edit-view parsing); empty or unparseable text
/// returns `[]`, and the frontend falls back to undifferentiated rendering.
///
/// Affix tier (best-effort): explicit lines on rare/magic/normal items are
/// reverse-looked-up via [`crate::state::tier_index`] (the `tier` field is
/// silently omitted when data isn't initialized / an old data pack lacks
/// pool data / the lookup misses — this is a display enhancement, not a hard dependency).
pub fn classify_item_lines_json(text: &str) -> Result<String, String> {
    let tier_ctx = tier_context(text);
    let lines: Vec<ItemLineJson> = pobr_item::classify_display_lines(text)
        .into_iter()
        .map(|l| {
            let tier = match (&tier_ctx, l.kind) {
                (Some((index, tags, domain)), pobr_item::DisplayLineKind::Explicit) => {
                    index.lookup(&l.text, tags, *domain)
                }
                _ => None,
            };
            ItemLineJson {
                text: l.text,
                kind: display_line_kind_str(l.kind),
                tier: tier.as_ref().map(|t| t.tier),
                tier_total: tier.as_ref().map(|t| t.total),
                affix: tier
                    .as_ref()
                    .map(|t| if t.is_prefix { "prefix" } else { "suffix" }),
            }
        })
        .collect();
    serde_json::to_string(&lines).map_err(|e| format!("serialize: {e}"))
}

/// The context needed for tier reverse lookup: (index, base tags, base mod_domain).
///
/// Unique/relic items have fixed rolls with no tier concept, and an
/// unrecognized base (a custom base name) is likewise omitted — better to skip than to be wrong.
fn tier_context(text: &str) -> Option<(std::rc::Rc<pobr_item::TierIndex>, Vec<String>, u32)> {
    let draft = pobr_item::ItemDraft::parse(text).ok()?;
    if matches!(
        draft.header.rarity.to_ascii_uppercase().as_str(),
        "UNIQUE" | "RELIC"
    ) {
        return None;
    }
    let index = state::tier_index()?;
    let (tags, domain) = state::base_item_tags(&draft.header.base_name)?;
    Some((index, tags, domain))
}

// gem_catalog_json (the gem picker's catalog for manual skill editing)

#[derive(Debug, Serialize)]
struct GemCatalogEntry {
    /// The granted effect id (the key sent up as [`GemInput::skill_id`]).
    skill_id: String,
    /// The display name (base_items' canonical name; falls back to the gem id if missing).
    name: String,
    /// The Traditional Chinese name (the `i18n/zh-TW/base_items.json` sidecar; `null` if missing).
    name_zh_tw: Option<String>,
    /// The Simplified Chinese name (the `i18n/zh-CN/base_items.json`
    /// sidecar, transcribed from the China-server dictionary; `null` if missing).
    name_zh_cn: Option<String>,
    /// The gem's colour (`"str"` red / `"dex"` green / `"int"` blue; `null`
    /// if unknown), used for category filtering.
    colour: Option<&'static str>,
    is_support: bool,
    /// Whether it's a Lineage special support gem (determined from the gem
    /// base id; used for the frontend badge plus optimizer candidate filtering).
    is_lineage: bool,
    /// Skill tags (sorted, deduplicated). For active gems, taken from the
    /// granted effect's `skill_types`; for support gems, taken from
    /// `require_skill_types` (i.e. "what it can support"), with logical
    /// connectives like `AND`/`OR`/`NOT` filtered out — those are gating-expression
    /// operators, not tags. The frontend picks readable entries to display via an allowlist.
    tags: Vec<String>,
}

/// The gem catalog: `{skill_id, name, name_zh_tw, colour, is_support}`,
/// sorted by name. Only collects player gems with a linked primary effect
/// (the `gem_effects` overlay is the curated surface of vendor Gems.lua).
pub fn gem_catalog_json() -> Result<String, String> {
    gem_catalog_impl().map_err(super::ApiError::into_json)
}

fn gem_catalog_impl() -> Result<String, super::ApiError> {
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let name_by_gem_id: std::collections::HashMap<&str, &str> = data
        .base_items
        .iter()
        .map(|(name, def)| (def.id.as_str(), name.as_str()))
        .collect();
    // The Chinese name sidecar (gem base id -> localized name); degrades to
    // an empty table when the file is missing (the data pack lacks that language).
    let game = state::game_data()?;
    let zh_names = game.base_item_names("zh-TW").unwrap_or_default();
    let cn_names = game.base_item_names("zh-CN").unwrap_or_default();
    let mut by_skill: BTreeMap<String, GemCatalogEntry> = BTreeMap::new();
    for gem in data.skill_gems.values() {
        let Some(skill_id) = gem.granted_effect_id.clone() else {
            continue;
        };
        let mut tags = data
            .granted_effects
            .get(&skill_id)
            .map(|effect| {
                if gem.is_support {
                    effect
                        .require_skill_types
                        .iter()
                        .filter(|tag| !matches!(tag.as_str(), "AND" | "OR" | "NOT"))
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    effect.skill_types.clone()
                }
            })
            .unwrap_or_default();
        tags.sort();
        tags.dedup();
        by_skill.entry(skill_id.clone()).or_insert(GemCatalogEntry {
            skill_id,
            name: name_by_gem_id
                .get(gem.id.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| gem.id.clone()),
            name_zh_tw: zh_names.get(gem.id.as_str()).cloned(),
            name_zh_cn: cn_names.get(gem.id.as_str()).cloned(),
            colour: match gem.gem_colour {
                Some(1) => Some("str"),
                Some(2) => Some("dex"),
                Some(3) => Some("int"),
                _ => None,
            },
            is_support: gem.is_support,
            is_lineage: gem.id.contains("Lineage"),
            tags,
        });
    }
    let mut entries: Vec<GemCatalogEntry> = by_skill.into_values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.skill_id.cmp(&b.skill_id)));
    Ok(serde_json::to_string(&entries).map_err(|e| format!("serialize: {e}"))?)
}

// rune_catalog_json / reforge_runes_json (rune socket editing: catalog plus re-socketing text rewrite)

#[derive(Debug, Serialize)]
struct RuneCatalogEntry {
    /// The rune name (canonical English; the key used in `Rune:` lines and reforge requests).
    name: String,
    /// The Traditional Chinese name (the base-name sidecar; `null` if missing).
    name_zh_tw: Option<String>,
    /// The Simplified Chinese name (same as above).
    name_zh_cn: Option<String>,
    is_soul_core: bool,
    /// The effect mod lines applicable to the `item_text` base (empty if no
    /// item context or not applicable).
    lines: Vec<String>,
}

/// A rune's mod lines applicable to a given base slot type (collects both
/// if both the broad and specific keys hit, matching PoB2's basis).
fn applicable_rune_lines(
    def: &pobr_data::catalog::RuneDef,
    broad: &str,
    specific: &str,
) -> Vec<String> {
    def.slots
        .iter()
        .filter(|(slot, _)| *slot == broad || *slot == specific)
        .flat_map(|(_, s)| s.lines.iter().cloned())
        .collect()
}

/// The rune/soul core catalog: the full `overlay/runes.json`, sorted by
/// name (the data is already ordered). When `item_text` is non-empty and
/// the base is recognized, each rune gets the effect mod lines applicable to that item attached.
pub fn rune_catalog_json(item_text: &str) -> Result<String, String> {
    rune_catalog_impl(item_text).map_err(super::ApiError::into_json)
}

fn rune_catalog_impl(item_text: &str) -> Result<String, super::ApiError> {
    let game = state::game_data().map_err(super::ApiError::not_initialized)?;
    let runes = game
        .runes()
        .map_err(|e| format!("load runes: {e}"))?
        .ok_or_else(|| String::from("runes overlay missing"))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let zh_tw = game.base_item_names("zh-TW").unwrap_or_default();
    let zh_cn = game.base_item_names("zh-CN").unwrap_or_default();
    // The target slot type: stays None when item text parsing fails or the
    // base is unknown (lines end up all-empty, no error).
    let slot_types = pobr_item::ItemDraft::parse(item_text)
        .ok()
        .and_then(|d| data.base_items.get(&d.header.base_name))
        .map(|base| rune_slot_types(&base.item_class));
    let entries: Vec<RuneCatalogEntry> = runes
        .runes
        .iter()
        .map(|r| {
            let id = data.base_items.get(&r.name).map(|d| d.id.as_str());
            RuneCatalogEntry {
                name: r.name.clone(),
                name_zh_tw: id.and_then(|i| zh_tw.get(i).cloned()),
                name_zh_cn: id.and_then(|i| zh_cn.get(i).cloned()),
                is_soul_core: r.slots.values().any(|s| s.kind == "SoulCore"),
                lines: slot_types
                    .as_ref()
                    .map(|(broad, specific)| applicable_rune_lines(r, broad, specific))
                    .unwrap_or_default(),
            }
        })
        .collect();
    Ok(serde_json::to_string(&entries).map_err(|e| format!("serialize: {e}"))?)
}

/// Base item_class -> rune slot type (broad, specific). Mirrors PoB2
/// `Item.lua:GetSocketedAugmentTypes`: caster = wand/staff/sceptre (no
/// weapon data); specific = the lowercased class name (Warstaff ->
/// quarterstaff, since PoE2's warstaff is the monk's quarterstaff).
fn rune_slot_types(item_class: &str) -> (String, String) {
    let specific = match item_class {
        "Warstaff" => "quarterstaff".to_string(),
        other => other.to_ascii_lowercase(),
    };
    let broad = match item_class {
        "Wand" | "Staff" | "Sceptre" => "caster",
        "Bow" | "Claw" | "Crossbow" | "Dagger" | "Flail" | "Spear" | "Warstaff"
        | "One Hand Axe" | "One Hand Mace" | "One Hand Sword" | "Two Hand Axe"
        | "Two Hand Mace" | "Two Hand Sword" | "FishingRod" => "weapon",
        _ => "armour",
    };
    (broad.to_string(), specific)
}

#[derive(Debug, Deserialize)]
struct ReforgeRunesRequest {
    /// The item's raw PoB text.
    text: String,
    /// The target socketing (rune names in slot order; count must be <= socket capacity).
    runes: Vec<String>,
    /// The target socket count (directly adds/removes sockets, not
    /// simulating a currency item): when given, rewrites/adds/removes the
    /// `Sockets:` line; defaults to keeping the text's current capacity.
    #[serde(default)]
    sockets: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ReforgeRunesResponse {
    text: String,
}

/// Re-sockets runes: wholesale replaces the item text's `Rune:`/`Soul Core:`
/// named lines and `{rune}` mod lines with the target rune set (the mod
/// lines are taken from the runes table by the base's broad/specific slot
/// type, matching PoB2 `Item.lua:1169-1205`'s rules), correcting the
/// `Implicits: N` count in sync; when `sockets` is given, the socket count
/// is rewritten in sync too (adding/rewriting/removing the `Sockets:` line).
pub fn reforge_runes_json(request_json: &str) -> Result<String, String> {
    reforge_runes_impl(request_json).map_err(super::ApiError::into_json)
}

fn reforge_runes_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: ReforgeRunesRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request: {e}")))?;
    let game = state::game_data().map_err(super::ApiError::not_initialized)?;
    let runes_def = game
        .runes()
        .map_err(|e| format!("load runes: {e}"))?
        .ok_or_else(|| String::from("runes overlay missing"))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;

    let draft = pobr_item::ItemDraft::parse(&req.text).map_err(|e| format!("parse item: {e}"))?;
    let base = data
        .base_items
        .get(&draft.header.base_name)
        .ok_or_else(|| format!("unknown base item: {}", draft.header.base_name))?;
    let (broad, specific) = rune_slot_types(&base.item_class);

    // For each rune, collect its applicable mod lines (both broad and
    // specific hits are collected, matching PoB2's basis).
    let mut new_stat_lines: Vec<String> = Vec::new();
    for name in &req.runes {
        let def = runes_def
            .runes
            .iter()
            .find(|r| &r.name == name)
            .ok_or_else(|| format!("unknown rune: {name}"))?;
        let lines = applicable_rune_lines(def, &broad, &specific);
        if lines.is_empty() {
            return Err(super::ApiError::bad_request(format!(
                "{name} 不适用于 {}",
                base.item_class
            )));
        }
        new_stat_lines.extend(lines);
    }

    // Text rewrite: strips the old Rune named lines and {rune} mod lines;
    // records the Sockets / Implicits positions.
    let mut out: Vec<String> = Vec::new();
    let mut sockets_idx: Option<usize> = None;
    let mut socket_capacity = 0usize;
    let mut implicits_idx: Option<usize> = None;
    let mut implicit_n = 0usize;
    // The Implicits window's remaining count (in a PoB export, the
    // Implicits line is immediately followed by N implicit/enchant-section
    // mod lines; any {rune} line stripped within this window must be
    // subtracted from the count).
    let mut window_remaining = 0usize;
    let mut removed_in_window = 0usize;
    for line in req.text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Rune:") || trimmed.starts_with("Soul Core:") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Sockets:") {
            sockets_idx = Some(out.len());
            socket_capacity = rest.split_whitespace().filter(|t| *t == "S").count();
            out.push(line.to_string());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Implicits:") {
            implicit_n = rest.trim().parse().unwrap_or(0);
            window_remaining = implicit_n;
            implicits_idx = Some(out.len());
            out.push(line.to_string());
            continue;
        }
        let in_window = window_remaining > 0;
        if in_window {
            window_remaining -= 1;
        }
        if trimmed.contains("{rune}") {
            if in_window {
                removed_in_window += 1;
            }
            continue;
        }
        out.push(line.to_string());
    }

    // Normalize the socket count: rewrites/adds/removes the `Sockets:` line
    // if the request gives a target count.
    let capacity = req.sockets.unwrap_or(socket_capacity);
    let sockets_line = format!("Sockets: {}", vec!["S"; capacity].join(" "));
    let sockets_idx = match (sockets_idx, capacity) {
        (Some(idx), 0) => {
            // Reduced to 0 sockets: remove the whole line (no named lines to insert afterward).
            out.remove(idx);
            if let Some(imp) = implicits_idx.as_mut()
                && *imp > idx
            {
                *imp -= 1;
            }
            None
        }
        (Some(idx), _) => {
            out[idx] = sockets_line;
            Some(idx)
        }
        (None, 0) => None,
        (None, _) => {
            // An item with no Sockets line gaining sockets: inserted before
            // Implicits; if there's no Implicits, inserted after the `Item
            // Level:` line (always present in a PoB export); as a last
            // resort, inserted after the base-type line (line 3).
            let idx = implicits_idx.unwrap_or_else(|| {
                out.iter()
                    .position(|l| l.trim().starts_with("Item Level:"))
                    .map(|i| i + 1)
                    .unwrap_or(3.min(out.len()))
            });
            out.insert(idx, sockets_line);
            if let Some(imp) = implicits_idx.as_mut()
                && *imp >= idx
            {
                *imp += 1;
            }
            Some(idx)
        }
    };
    if req.runes.len() > capacity {
        return Err(super::ApiError::bad_request(format!(
            "too many runes: {} > socket capacity {capacity}",
            req.runes.len()
        )));
    }
    if !req.runes.is_empty() && sockets_idx.is_none() {
        return Err(super::ApiError::bad_request("item has no rune sockets"));
    }

    // Insert the later section first (mod lines after Implicits), then the
    // earlier section (named lines after Sockets), to avoid index shifting.
    if let Some(idx) = implicits_idx {
        out[idx] = format!(
            "Implicits: {}",
            implicit_n - removed_in_window + new_stat_lines.len()
        );
        for (i, line) in new_stat_lines.iter().enumerate() {
            out.insert(idx + 1 + i, format!("{{rune}}{line}"));
        }
    } else if let Some(idx) = sockets_idx {
        for (i, line) in new_stat_lines.iter().enumerate() {
            out.insert(idx + 1 + i, format!("{{rune}}{line}"));
        }
    }
    if let Some(idx) = sockets_idx {
        for (i, name) in req.runes.iter().enumerate() {
            out.insert(idx + 1 + i, format!("Rune: {name}"));
        }
    }

    Ok(serde_json::to_string(&ReforgeRunesResponse {
        text: out.join("\n"),
    })
    .map_err(|e| format!("serialize: {e}"))?)
}

// translate_lines_json (English -> Simplified Chinese display translation: tree mod tooltips / config options, etc)

/// Batch-translates English mod lines into Simplified Chinese display text
/// (template reverse lookup; unrecognized lines pass through unchanged).
/// Both input and output are JSON string arrays. Everything passes through
/// unchanged when the data pack has no zh-CN templates.
pub fn translate_lines_to_zh_cn_json(lines_json: &str) -> Result<String, String> {
    translate_lines_impl(lines_json).map_err(super::ApiError::into_json)
}

fn translate_lines_impl(lines_json: &str) -> Result<String, super::ApiError> {
    let lines: Vec<String> = serde_json::from_str(lines_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid lines json: {e}")))?;
    let translator = state::en_to_zh_translator();
    let out: Vec<String> = lines
        .into_iter()
        .map(|line| match &translator {
            Some(t) => t.translate_line(&line).unwrap_or(line),
            None => line,
        })
        .collect();
    Ok(serde_json::to_string(&out).map_err(|e| format!("serialize: {e}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_item_lines_json_kind_sequence() {
        let text = "\
Rarity: RARE
Apocalypse Pelt
Falconer's Jacket
Item Level: 81
Sockets: S
Implicits: 2
{enchant}60% increased Armour
{rune}Bonded: +60 to maximum Life
+190 to maximum Life
+34% to Cold Resistance";
        let json = classify_item_lines_json(text).expect("classify");
        let lines: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        let kinds: Vec<&str> = lines.iter().map(|l| l["kind"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec![
                "name", "base", "struct", "struct", "struct", "enchant", "rune", "explicit",
                "explicit",
            ]
        );
        // Mod line text has had its annotation prefix stripped.
        assert_eq!(lines[5]["text"], "60% increased Armour");
        assert_eq!(lines[6]["text"], "Bonded: +60 to maximum Life");
    }

    #[test]
    fn classify_item_lines_json_empty_on_blank() {
        assert_eq!(classify_item_lines_json("  \n").unwrap(), "[]");
    }
}
