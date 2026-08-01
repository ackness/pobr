//! Build decoding: PoB Build Code -> structured build JSON
//! (`decode_build_json`), plus a China-server `.build` file -> an
//! isomorphic `BuildJson` (`decode_build_file_json`).

use std::collections::BTreeMap;

use pobr_build::{
    Build, BuildData, SetKind, SetSelection, active_selection, decode_pob_code, derive_loadouts,
    duplicate_set, parse_build, parse_build_sets, parse_notes, parse_raw_items_view, remove_set,
    rename_set, select_sets,
};
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::passive_tree::AttributeChoice;
use serde::{Deserialize, Serialize};

use crate::state;

// 0.1 decode_build_json

#[derive(Debug, Serialize)]
struct CharacterJson {
    level: u32,
    class_name: String,
    ascendancy_name: String,
}

#[derive(Debug, Serialize)]
struct TreeJson {
    allocated_nodes: Vec<u32>,
    tree_version: Option<String>,
    /// Attribute-choice small nodes (node skill id -> `"str"|"dex"|"int"`).
    attribute_choices: BTreeMap<u32, &'static str>,
}

fn attribute_choice_str(choice: AttributeChoice) -> &'static str {
    match choice {
        AttributeChoice::Strength => "str",
        AttributeChoice::Dexterity => "dex",
        AttributeChoice::Intelligence => "int",
    }
}

#[derive(Debug, Serialize)]
struct SlotItemJson {
    slot: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct SocketJewelJson {
    /// The tree node's skill id for the jewel socket.
    socket_node: u32,
    text: String,
}

#[derive(Debug, Serialize)]
struct ItemsJson {
    equipped: Vec<SlotItemJson>,
    jewels: Vec<String>,
    /// Tree-socketed jewels (editable: maintained per-socket on the passive tree page).
    socket_jewels: Vec<SocketJewelJson>,
    flasks: Vec<SlotItemJson>,
}

#[derive(Debug, Serialize)]
struct GemJson {
    skill_id: String,
    level: u32,
    quality: u32,
}

#[derive(Debug, Serialize)]
struct SocketGroupJson {
    slot: Option<String>,
    enabled: bool,
    /// PoB `<Skill source>` (equipment-granted skill groups get `Item:<id>:<name>`); `None` = a manual group.
    /// Must be passed through the whole chain (web state -> request ->
    /// encode); otherwise a granted group loses its source marker after a
    /// share-code round trip, and the engine re-synthesizes it from the
    /// item's mod lines -> the granted skill gets double-counted.
    source: Option<String>,
    active_skill_id: Option<String>,
    gems: Vec<GemJson>,
}

/// The JSON shape for a config `<Input>` value (the three types output directly).
fn config_value_json(value: &ConfigInputValue) -> serde_json::Value {
    match value {
        ConfigInputValue::Bool(b) => serde_json::Value::from(*b),
        ConfigInputValue::Number(n) => serde_json::Value::from(*n),
        ConfigInputValue::Text(t) => serde_json::Value::from(t.clone()),
    }
}

#[derive(Debug, Serialize)]
struct BuildJson {
    character: CharacterJson,
    tree: TreeJson,
    items: ItemsJson,
    socket_groups: Vec<SocketGroupJson>,
    /// The main socket group's index (0-based; `None` = unspecified, and
    /// the calc side falls back to the first enabled group).
    main_socket_group: Option<usize>,
    /// The raw `<Config>` input key/values (the initial state shown/edited on the Config page).
    config_inputs: BTreeMap<String, serde_json::Value>,
    /// Free-text `<Notes>` (PoB's notes page; `null` if that section is absent).
    notes: Option<String>,
    /// The list of switchable loadouts (PoB2's loadout concept: passives /
    /// equipment / skills bound together by a title naming convention).
    /// A single-set build is always exactly one `Default` entry; the
    /// frontend renders its switch dropdown from this.
    loadouts: Vec<LoadoutJson>,
    /// The loadout index (within `loadouts`) matching the current build; `null` if it can't be determined.
    active_loadout: Option<usize>,
}

/// A single switchable loadout. `tree`/`item`/`skill` are 1-based document
/// indices; passing them back as `decodeBuildJson`'s selection switches to
/// it; `null` means that category didn't participate in the binding (the single-set exemption).
#[derive(Debug, Serialize)]
struct LoadoutJson {
    name: String,
    tree: usize,
    item: Option<usize>,
    skill: Option<usize>,
}

fn build_to_json(build: &Build, xml: &str) -> Result<BuildJson, String> {
    let raw_items = parse_raw_items_view(xml).map_err(|e| format!("parse items: {e}"))?;
    // The loadout list plus the current selection: reverse-looked-up
    // against the XML's active triple (after switching and re-decoding, the
    // rewritten active values point at the new group, which the frontend uses to highlight it).
    let sets = parse_build_sets(xml).map_err(|e| format!("parse sets: {e}"))?;
    let active = active_selection(xml);
    let loadouts: Vec<LoadoutJson> = derive_loadouts(&sets)
        .into_iter()
        .map(|l| LoadoutJson {
            name: l.name,
            tree: l.tree,
            item: l.item,
            skill: l.skill,
        })
        .collect();
    let active_loadout = loadouts.iter().position(|l| {
        l.tree == active.tree.unwrap_or(1)
            && l.item.is_none_or(|i| Some(i) == active.item)
            && l.skill.is_none_or(|s| Some(s) == active.skill)
    });
    Ok(BuildJson {
        character: CharacterJson {
            level: build.character.level,
            class_name: build.character.class_name.clone(),
            ascendancy_name: build.character.ascendancy_name.clone(),
        },
        tree: TreeJson {
            allocated_nodes: build.tree.allocated_nodes.iter().map(|n| n.0).collect(),
            tree_version: build.tree_version.clone(),
            attribute_choices: build
                .tree
                .attribute_overrides
                .iter()
                .map(|(node, choice)| (node.0, attribute_choice_str(*choice)))
                .collect(),
        },
        items: ItemsJson {
            equipped: raw_items
                .equipped
                .into_iter()
                .map(|(slot, text)| SlotItemJson { slot, text })
                .collect(),
            jewels: raw_items.jewels,
            socket_jewels: raw_items
                .socket_jewels
                .into_iter()
                .map(|(socket_node, text)| SocketJewelJson { socket_node, text })
                .collect(),
            flasks: raw_items
                .flasks
                .into_iter()
                .map(|(slot, text)| SlotItemJson { slot, text })
                .collect(),
        },
        socket_groups: build
            .socket_groups
            .iter()
            .map(|g| SocketGroupJson {
                slot: g.slot.clone(),
                enabled: g.enabled,
                source: g.source.clone(),
                active_skill_id: g.active_skill_id.clone(),
                gems: g
                    .gem_skills
                    .iter()
                    .map(|gem| GemJson {
                        skill_id: gem.skill_id.clone(),
                        level: gem.gem_level,
                        quality: gem.quality,
                    })
                    .collect(),
            })
            .collect(),
        // Build's internal representation is 1-based (matching PoB XML) -> the contract is 0-based (web index semantics).
        main_socket_group: build.main_socket_group.map(|m| m.saturating_sub(1)),
        config_inputs: build
            .config
            .raw_inputs
            .values
            .iter()
            .map(|(k, v)| (k.clone(), config_value_json(v)))
            .collect(),
        notes: parse_notes(xml).map_err(|e| format!("parse notes: {e}"))?,
        loadouts,
        active_loadout,
    })
}

/// 0.1: PoB Build Code -> structured build JSON (character/tree/equipment text blocks/skill groups/config).
///
/// Pure decoding; doesn't require game data to be initialized.
pub fn decode_build_json(code: &str) -> Result<String, String> {
    decode_build_impl(code).map_err(super::ApiError::into_json)
}

fn decode_build_impl(code: &str) -> Result<String, super::ApiError> {
    decode_selected(code, &SetSelection::default())
}

/// 0.1b: same as [`decode_build_json`], but first switches to a specified loadout (group switching).
///
/// Request shape `{ "code": "...", "tree": 2, "item": 2, "skill": null }` —
/// the three indices are taken from the `loadouts[]` entries in the
/// response; omitted/`null` means that category is left as-is (the
/// single-set exemption). Switching happens at the **XML level** (rewrite
/// the three active attributes, then re-parse), so the result exactly
/// matches manually switching all three dropdowns in PoB2.
pub fn decode_build_loadout_json(request_json: &str) -> Result<String, String> {
    state::cached_response("decode_loadout", request_json, || {
        decode_loadout_impl(request_json).map_err(super::ApiError::into_json)
    })
}

#[derive(Debug, Deserialize)]
struct LoadoutRequest {
    code: String,
    #[serde(default)]
    tree: Option<usize>,
    #[serde(default)]
    item: Option<usize>,
    #[serde(default)]
    skill: Option<usize>,
}

fn decode_loadout_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: LoadoutRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("parse request: {e}")))?;
    decode_selected(
        &req.code,
        &SetSelection {
            tree: req.tree,
            item: req.item,
            skill: req.skill,
        },
    )
}

/// 0.1c: group management — copies / renames / deletes a loadout, returning the **new build code**.
///
/// Request `{ code, op, name?, tree?, item?, skill? }`: `op` is one of
/// `duplicate` / `rename` / `remove`; the three indices specify which set
/// to operate on (default = the currently active one). All three set types
/// are operated on together — a loadout is their combination, and touching
/// only one type would misalign the bindings.
///
/// Copies rather than creating an empty new set (matching PoB2
/// `CustomLoadout`'s `CopyTree`/`CopyItemSet` semantics): a new stage is
/// always derived from the previous one, and an empty tree is useless to the user.
pub fn manage_loadout_json(request_json: &str) -> Result<String, String> {
    manage_loadout_impl(request_json).map_err(super::ApiError::into_json)
}

#[derive(Debug, Deserialize)]
struct ManageRequest {
    code: String,
    op: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tree: Option<usize>,
    #[serde(default)]
    item: Option<usize>,
    #[serde(default)]
    skill: Option<usize>,
}

fn manage_loadout_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: ManageRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("parse request: {e}")))?;
    let xml = decode_pob_code(req.code.trim())
        .map_err(|e| super::ApiError::decode_error(format!("decode build code: {e}")))?;
    let active = active_selection(&xml);
    let targets = [
        (SetKind::Tree, req.tree.or(active.tree).unwrap_or(1)),
        (SetKind::Item, req.item.or(active.item).unwrap_or(1)),
        (SetKind::Skill, req.skill.or(active.skill).unwrap_or(1)),
    ];

    let mut out = xml;
    for (kind, index) in targets {
        let applied = match req.op.as_str() {
            "duplicate" => {
                let name = req.name.as_deref().ok_or_else(|| {
                    super::ApiError::bad_request("name is required for duplicate")
                })?;
                duplicate_set(&out, kind, index, name)
            }
            "rename" => {
                let name = req
                    .name
                    .as_deref()
                    .ok_or_else(|| super::ApiError::bad_request("name is required for rename"))?;
                rename_set(&out, kind, index, name)
            }
            "remove" => remove_set(&out, kind, index),
            other => {
                return Err(super::ApiError::bad_request(format!("unknown op: {other}")));
            }
        };
        // Skipped when a category lacks that set (e.g. only one skill set
        // exists) — normal under the single-set exemption.
        if let Some(next) = applied {
            out = next;
        }
    }
    Ok(serde_json::to_string(
        &pobr_build::encode_pob_code(&out).map_err(|e| format!("encode: {e}"))?,
    )
    .map_err(|e| format!("serialize: {e}"))?)
}

fn decode_selected(code: &str, sel: &SetSelection) -> Result<String, super::ApiError> {
    let raw = decode_pob_code(code.trim())
        .map_err(|e| super::ApiError::decode_error(format!("decode build code: {e}")))?;
    let xml = select_sets(&raw, sel);
    let build = parse_build(&xml)
        .map_err(|e| super::ApiError::decode_error(format!("parse build xml: {e}")))?;
    let json = build_to_json(&build, &xml)?;
    Ok(serde_json::to_string(&json).map_err(|e| format!("serialize: {e}"))?)
}

// decode_build_file_json (a China-server exported `.build` file -> BuildJson)

/// The shape of a China-server `.build` file (exported from PoE2's
/// China-server marketplace: JSON, passives as string slugs, equipment with
/// only Simplified Chinese mod lines, and gems as base metadata ids with no level/quality).
#[derive(Debug, Deserialize)]
struct CnBuildFile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ascendancy: Option<String>,
    #[serde(default)]
    passives: Vec<CnPassive>,
    #[serde(default)]
    items: Vec<CnItem>,
    #[serde(default)]
    skills: Vec<CnSkill>,
}

#[derive(Debug, Deserialize)]
struct CnPassive {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CnItem {
    inventory_id: String,
    #[serde(default)]
    additional_text: String,
}

#[derive(Debug, Deserialize)]
struct CnSkill {
    id: String,
    #[serde(default)]
    support_skills: Vec<CnPassive>,
}

/// `.build` slot -> our equipment slot id. `Weapon2`/`Offhand2` is the
/// second weapon set, unmodeled in v1 (the same known gap as
/// `useSecondWeaponSet` in XML import), so it's skipped.
fn cn_inventory_slot(inventory_id: &str) -> Option<&'static str> {
    Some(match inventory_id {
        "Weapon1" => "weapon1",
        "Offhand1" => "weapon2",
        "Helm1" => "helmet",
        "BodyArmour1" => "bodyarmour",
        "Gloves1" => "gloves",
        "Boots1" => "boots",
        "Amulet1" => "amulet",
        "Ring1" => "ring1",
        "Ring2" => "ring2",
        "Belt1" => "belt",
        _ => return None,
    })
}

/// Gem base metadata id -> the primary granted effect id (`.build` mixes
/// both `Gem`/`Gems` prefixes; normalized before looking up the skill_gems table).
fn cn_gem_effect_id(data: &BuildData, gem_id: &str) -> Option<String> {
    let lookup = |id: &str| {
        data.skill_gems
            .get(id)
            .and_then(|g| g.granted_effect_id.clone())
    };
    lookup(gem_id)
        .or_else(|| lookup(&gem_id.replace("/Gems/", "/Gem/")))
        .or_else(|| lookup(&gem_id.replace("/Gem/", "/Gems/")))
}

/// Parses a China-server `.build` file into JSON isomorphic to [`BuildJson`]
/// (requires game data to be initialized first: both passive slug -> numeric
/// id and gem base id -> effect id need table lookups).
///
/// Known limitations: equipment has no base name/rarity (constructed as
/// RARE plus a placeholder name, with base defence values missing); gems
/// have no level/quality (defaulted to 20/0); the second weapon set is skipped.
pub fn decode_build_file_json(content: &str) -> Result<String, String> {
    decode_build_file_impl(content).map_err(super::ApiError::into_json)
}

fn decode_build_file_impl(content: &str) -> Result<String, super::ApiError> {
    let file: CnBuildFile = serde_json::from_str(content)
        .map_err(|e| super::ApiError::decode_error(format!("invalid .build json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let game = state::game_data().map_err(super::ApiError::not_initialized)?;

    // Level: from `Lv.98` in the build name.
    let level = file
        .name
        .split("Lv")
        .nth(1)
        .and_then(|rest| {
            let digits: String = rest
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse::<u32>().ok()
        })
        .unwrap_or(1);

    // Ascendancy id (e.g. `Sorceress3`) -> class name plus ascendancy name.
    let mut class_name = String::new();
    let mut ascendancy_name = String::new();
    if let Some(asc_id) = &file.ascendancy {
        let meta = game
            .passive_tree_meta()
            .map_err(|e| format!("load tree meta: {e}"))?;
        for class in &meta.classes {
            if let Some(asc) = class.ascendancies.iter().find(|a| &a.id == asc_id) {
                class_name = class.name.clone();
                ascendancy_name = asc.name.clone();
                break;
            }
        }
        if class_name.is_empty() {
            return Err(super::ApiError::decode_error(format!(
                "unknown ascendancy id: {asc_id}"
            )));
        }
    }

    // Passive slug -> numeric skill id.
    let slug_to_skill: std::collections::HashMap<&str, u32> = data
        .passive_nodes
        .values()
        .map(|n| (n.id.as_str(), n.skill))
        .collect();
    let mut allocated_nodes: Vec<u32> = Vec::new();
    let mut unknown_passives = 0usize;
    for p in &file.passives {
        match slug_to_skill.get(p.id.as_str()) {
            Some(&skill) => allocated_nodes.push(skill),
            None => unknown_passives += 1,
        }
    }
    allocated_nodes.sort_unstable();
    allocated_nodes.dedup();

    // Equipment: only mod lines are available -> construct PoB text (RARE
    // plus a placeholder name; Chinese mod lines are handled by the calc-side translation layer).
    let equipped: Vec<SlotItemJson> = file
        .items
        .iter()
        .filter_map(|item| {
            let slot = cn_inventory_slot(&item.inventory_id)?;
            let text = format!(
                "Rarity: RARE\nImported Item\n{}",
                item.additional_text.trim()
            );
            Some(SlotItemJson {
                slot: slot.to_string(),
                text,
            })
        })
        .collect();

    // Skill groups: gem base id -> effect id (level/quality default to 20/0).
    let mut unknown_gems = 0usize;
    let socket_groups: Vec<SocketGroupJson> = file
        .skills
        .iter()
        .filter_map(|group| {
            let active = match cn_gem_effect_id(&data, &group.id) {
                Some(id) => id,
                None => {
                    unknown_gems += 1;
                    return None;
                }
            };
            let mut gems = vec![GemJson {
                skill_id: active.clone(),
                level: 20,
                quality: 0,
            }];
            for support in &group.support_skills {
                match cn_gem_effect_id(&data, &support.id) {
                    Some(id) => gems.push(GemJson {
                        skill_id: id,
                        level: 20,
                        quality: 0,
                    }),
                    None => unknown_gems += 1,
                }
            }
            Some(SocketGroupJson {
                slot: None,
                enabled: true,
                source: None,
                active_skill_id: Some(active),
                gems,
            })
        })
        .collect();

    let json = BuildJson {
        character: CharacterJson {
            level,
            class_name,
            ascendancy_name,
        },
        tree: TreeJson {
            allocated_nodes,
            tree_version: None,
            attribute_choices: BTreeMap::new(),
        },
        items: ItemsJson {
            equipped,
            jewels: Vec::new(),
            socket_jewels: Vec::new(),
            flasks: Vec::new(),
        },
        socket_groups,
        main_socket_group: None,
        config_inputs: BTreeMap::new(),
        notes: (!file.name.trim().is_empty()).then(|| {
            let mut note = file.name.trim().to_string();
            if unknown_passives + unknown_gems > 0 {
                note.push_str(&format!(
                    "\n[import] skipped: {unknown_passives} unknown passives, {unknown_gems} unknown gems"
                ));
            }
            if file.passives.iter().any(|p| p.id.starts_with("jewel_slot")) {
                note.push_str(
                    "\n[import] .build has no jewel body (only socket allocations) -- add jewels manually on the passive tree page",
                );
            }
            note
        }),
        // A China-server `.build` has no multi-set concept: give it one
        // fully-exempt Default entry, so the frontend's dropdown shape stays consistent.
        loadouts: vec![LoadoutJson {
            name: "Default".to_string(),
            tree: 1,
            item: None,
            skill: None,
        }],
        active_loadout: Some(0),
    };
    Ok(serde_json::to_string(&json).map_err(|e| format!("serialize: {e}"))?)
}
