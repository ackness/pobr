//! build 解码：PoB Build Code → 结构化 build JSON（`decode_build_json`），
//! 以及国服 `.build` 文件 → 同构 `BuildJson`（`decode_build_file_json`）。

use std::collections::BTreeMap;

use pobr_build::{
    Build, BuildData, decode_pob_code, parse_build, parse_notes, parse_raw_items_view,
};
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::passive_tree::AttributeChoice;
use serde::{Deserialize, Serialize};

use crate::state;

// ---------------------------------------------------------------------------
// 0.1 decode_build_json
// ---------------------------------------------------------------------------

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
    /// 属性小点三选一（node skill id → `"str"|"dex"|"int"`）。
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
    /// 珠宝插槽的树节点 skill id。
    socket_node: u32,
    text: String,
}

#[derive(Debug, Serialize)]
struct ItemsJson {
    equipped: Vec<SlotItemJson>,
    jewels: Vec<String>,
    /// 树插槽珠宝（可编辑：天赋树页按插槽维护）。
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
    /// PoB `<Skill source>`（装备授予技能组 `Item:<id>:<name>`）；`None`=手动组。
    /// 需全链透传（web 状态 → 请求 → encode），否则分享 code 往返后授予组会
    /// 失去 source 标记，引擎按物品词条再合成一份 → 授予技能重复计数。
    source: Option<String>,
    active_skill_id: Option<String>,
    gems: Vec<GemJson>,
}

/// config `<Input>` 值的 JSON 形状（三型直出）。
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
    /// 主技能组下标（0-based；`None` = 未指定，计算侧退化为首个启用组）。
    main_socket_group: Option<usize>,
    /// `<Config>` 原始输入键值（Config 页展示/编辑的初始状态）。
    config_inputs: BTreeMap<String, serde_json::Value>,
    /// `<Notes>` 自由文本（PoB 笔记页；无该段为 null）。
    notes: Option<String>,
}

fn build_to_json(build: &Build, xml: &str) -> Result<BuildJson, String> {
    let raw_items = parse_raw_items_view(xml).map_err(|e| format!("parse items: {e}"))?;
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
        // Build 内部 1-based（PoB XML 同）→ 契约 0-based（web 下标语义）。
        main_socket_group: build.main_socket_group.map(|m| m.saturating_sub(1)),
        config_inputs: build
            .config
            .raw_inputs
            .values
            .iter()
            .map(|(k, v)| (k.clone(), config_value_json(v)))
            .collect(),
        notes: parse_notes(xml).map_err(|e| format!("parse notes: {e}"))?,
    })
}

/// 0.1：PoB Build Code → 结构化 build JSON（角色/树/装备文本块/技能组/config）。
///
/// 纯解码，不需要游戏数据初始化。
pub fn decode_build_json(code: &str) -> Result<String, String> {
    let xml = decode_pob_code(code.trim()).map_err(|e| format!("decode build code: {e}"))?;
    let build = parse_build(&xml).map_err(|e| format!("parse build xml: {e}"))?;
    let json = build_to_json(&build, &xml)?;
    serde_json::to_string(&json).map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// decode_build_file_json（国服导出 `.build` 文件 → BuildJson）
// ---------------------------------------------------------------------------

/// 国服 `.build` 文件形状（poe2 国服市集导出：JSON，天赋为字符串 slug、
/// 装备只有简中词条行、宝石为基底 metadata id 且无等级/品质）。
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

/// `.build` 槽位 → 我们的装备槽 id。`Weapon2`/`Offhand2` 是第二武器组,
/// v1 不建模（与 XML 导入的 useSecondWeaponSet 同期欠账），跳过。
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

/// 宝石基底 metadata id → 主授予效果 id（`.build` 里 `Gem`/`Gems` 两种前缀混用，
/// 归一化后查 skill_gems 表）。
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

/// 解析国服 `.build` 文件为 [`BuildJson`] 同构 JSON（需先初始化游戏数据：
/// 天赋 slug → 数值 id、宝石基底 id → 效果 id 都要查表）。
///
/// 已知边界：装备无基底名/稀有度（按 RARE + 占位名构造，防御底值缺失）；
/// 宝石无等级/品质（默认 20/0）；第二武器组跳过。
pub fn decode_build_file_json(content: &str) -> Result<String, String> {
    let file: CnBuildFile =
        serde_json::from_str(content).map_err(|e| format!("invalid .build json: {e}"))?;
    let data = state::build_data()?;
    let game = state::game_data()?;

    // 等级：build 名里的 `Lv.98`。
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

    // 升华 id（如 `Sorceress3`）→ 职业名 + 升华名。
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
            return Err(format!("unknown ascendancy id: {asc_id}"));
        }
    }

    // 天赋 slug → 数值 skill id。
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

    // 装备：只有词条行 → 构造 PoB 文本（RARE + 占位名；词条简中由计算侧翻译层处理）。
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

    // 技能组：宝石基底 id → 效果 id（等级/品质缺省 20/0）。
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
                    "\n[import] .build 不含珠宝本体（只有插槽加点）——请在天赋树页手动补珠宝",
                );
            }
            note
        }),
    };
    serde_json::to_string(&json).map_err(|e| format!("serialize: {e}"))
}
