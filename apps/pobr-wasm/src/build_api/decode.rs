//! build 解码：PoB Build Code → 结构化 build JSON（`decode_build_json`），
//! 以及国服 `.build` 文件 → 同构 `BuildJson`（`decode_build_file_json`）。

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
    /// 成组切换清单（PoB2 loadout：天赋/装备/技能按 title 命名约定绑定）。
    /// 单套 build 恒为一条 `Default`；前端据此渲染切换下拉。
    loadouts: Vec<LoadoutJson>,
    /// 当前 build 对应的 loadout 下标（`loadouts` 内），无法判定时为 null。
    active_loadout: Option<usize>,
}

/// 一个可切换的 loadout。`tree`/`item`/`skill` 是 1-based 文档序，回传
/// `decodeBuildJson` 的 selection 即可切过去；`null` = 该类未参与绑定（单套豁免）。
#[derive(Debug, Serialize)]
struct LoadoutJson {
    name: String,
    tree: usize,
    item: Option<usize>,
    skill: Option<usize>,
}

fn build_to_json(build: &Build, xml: &str) -> Result<BuildJson, String> {
    let raw_items = parse_raw_items_view(xml).map_err(|e| format!("parse items: {e}"))?;
    // Loadout 清单 + 当前选中项：按 XML 的 active 三元组反查（切换后重新 decode
    // 时，改写过的 active 会指向新组，前端据此高亮）。
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
        loadouts,
        active_loadout,
    })
}

/// 0.1：PoB Build Code → 结构化 build JSON（角色/树/装备文本块/技能组/config）。
///
/// 纯解码，不需要游戏数据初始化。
pub fn decode_build_json(code: &str) -> Result<String, String> {
    decode_build_impl(code).map_err(super::ApiError::into_json)
}

fn decode_build_impl(code: &str) -> Result<String, super::ApiError> {
    decode_selected(code, &SetSelection::default())
}

/// 0.1b：同 [`decode_build_json`]，但先切到指定的 loadout（成组切换）。
///
/// 请求形状 `{ "code": "...", "tree": 2, "item": 2, "skill": null }`——三个序号取自
/// 响应里 `loadouts[]` 的条目；省略/`null` 表示该类保持原样（单套豁免）。切换在
/// **XML 层**完成（改写三个 active 属性后重新解析），因此结果与用 PoB2 手动切三个
/// 下拉完全一致。
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

/// 0.1c：组管理——复制 / 重命名 / 删除一个 loadout，返回**新的 build code**。
///
/// 请求 `{ code, op, name?, tree?, item?, skill? }`：`op` ∈ `duplicate` /
/// `rename` / `remove`；三个序号指定操作哪一套（缺省 = 当前 active）。三类 set
/// 一并操作——loadout 是它们的组合，只动一类会让绑定错位。
///
/// 复制而非新建空套（同 PoB2 `CustomLoadout` 的 `CopyTree`/`CopyItemSet` 语义）：
/// 新阶段总是从上一阶段改出来的，空树对用户没用。
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
        // 某一类缺该套（如只有一套技能集）时跳过——单套豁免下这是正常形态。
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

// decode_build_file_json（国服导出 `.build` 文件 → BuildJson）

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
    decode_build_file_impl(content).map_err(super::ApiError::into_json)
}

fn decode_build_file_impl(content: &str) -> Result<String, super::ApiError> {
    let file: CnBuildFile = serde_json::from_str(content)
        .map_err(|e| super::ApiError::decode_error(format!("invalid .build json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let game = state::game_data().map_err(super::ApiError::not_initialized)?;

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
            return Err(super::ApiError::decode_error(format!(
                "unknown ascendancy id: {asc_id}"
            )));
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
        // 国服 `.build` 无多套概念：给一条全豁免的 Default，前端下拉形状一致。
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
