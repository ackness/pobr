//! PoB Build XML → 完整 [`Build`] 的解析。
//!
//! [`crate::xml_serde::parse_build_header`] 只取 `<Build>` 头部（等级 / 职业 / 升华 /
//! 视图）。本模块在其之上**还原可计算的来源**：天赋树已分配节点、装备（按槽位）、
//! 技能宝石组——使 [`crate::calc_orchestrator::calculate_with_data`] 能直接从一份
//! PoB Build Code 端到端计算，而无需调用方手写 XML 抽取。
//!
//! 解析覆盖范围：
//! - **角色身份**：复用 [`parse_build_header`]（等级 / 职业 / 升华 / `viewMode` /
//!   `PathOfBuilding(2)` 主版本 → [`GameVersion`]）。
//! - **天赋树**：`<Tree activeSpec>` 选中的 `<Spec nodes="…">` 节点 id 数组
//!   （多 Spec 时取 `activeSpec` 1-based 索引，缺省取首个）。
//! - **装备**：`<Item id>` 文本块经 [`parse_pob_xml_item`] 解析为 [`Item`]，再由
//!   `<Items activeItemSet>` 选中的 `<ItemSet>` 的 `<Slot name itemId>` 映射到
//!   [`EquipmentSlot`]（PoB 槽名 → 枚举；不在枚举内的 Charm/Flask/Ring 3 等忽略）。
//! - **技能宝石组**：`<Skills activeSkillSet>` 选中 `<SkillSet>` 下每个 `<Skill>` →
//!   一个 [`SocketGroup`]（启用态来自 `Skill.enabled`，gem id 取启用的 `<Gem gemId>`）。
//!
//! 健壮性：单件装备文本块解析失败（结构性错误）时**跳过该件**而非中止整次导入
//! （PoB 的容错语义）；词条本身的不可解析行交由下游 `calculate_with_data` 过滤。
//!
//! 已知切片（记录，不阻塞）：`masteryEffects` 选择、JewelSocket 内嵌珠宝、第二武器
//! 组的独立 Spec、`<Item>` 的精确基底归一化等留待后续。

use quick_xml::Reader;
use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};

use pobr_core::item_text::parse_pob_xml_item;
use pobr_data::build_config::GameVersion;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};

use crate::build::{Build, CharacterIdentity, SocketGroup};
use crate::build_code::decode_pob_code;
use crate::error::{BuildError, XmlError};
use crate::xml_serde::parse_build_header;

/// 把一份 PoB Build Code 直接解析为完整 [`Build`]（decode → XML → 解析）。
///
/// 等价于 `parse_build(&decode_pob_code(code)?)`，是上层导入最常用的一步入口。
pub fn parse_build_from_code(code: &str) -> Result<Build, BuildError> {
    let xml = decode_pob_code(code.trim())?;
    Ok(parse_build(&xml)?)
}

/// 把一份 PoB Build XML 解析为完整 [`Build`]（角色 + 天赋树 + 装备 + 技能宝石组）。
pub fn parse_build(xml: &str) -> Result<Build, XmlError> {
    let header = parse_build_header(xml)?;
    let game_version = if header.pob_major == 1 {
        GameVersion::Poe1
    } else {
        GameVersion::Poe2
    };

    let allocated_nodes = parse_passive_nodes(xml)?;
    let items = parse_items_and_slots(xml)?;
    let socket_groups = parse_socket_groups(xml)?;
    let main_socket_group = parse_main_socket_group(xml);

    let mut build = Build::new()
        .with_character(CharacterIdentity {
            level: header.identity.level,
            class_name: header.identity.class_name,
            ascendancy_name: header.identity.ascendancy_name,
        })
        .with_game_version(game_version)
        .with_view_mode(header.view_mode)
        .with_tree(PassiveTreeSpec {
            allocated_nodes,
            ..Default::default()
        });
    if let Some(g) = main_socket_group {
        build = build.with_main_socket_group(g);
    }

    for (slot, item) in items {
        build = build.set_item(slot, item);
    }
    for group in socket_groups {
        build = build.add_socket_group(group);
    }

    // 战斗配置（敌人状态 / 条件 / 倍率）→ BuildConfig，经 to_calc_config 进入 cfg，
    // 供条件型词条（`... against Chilled Enemies` 等）按 PoB 保存的开关生效。
    let (conditions, multipliers) = parse_config(xml);
    build.config.conditions.extend(conditions);
    build.config.multipliers.extend(multipliers);

    Ok(build)
}

/// 抽取 `<Config>` 的 `<Input name boolean|number>` → (conditions, multipliers)。
/// 名称去 `condition`/`multiplier` 前缀作为变量名（如 `conditionEnemyChilled` → `EnemyChilled`），
/// 与计算侧 `ModTag::Condition`/`Multiplier` 变量约定对齐。
fn parse_config(xml: &str) -> (HashMap<String, bool>, HashMap<String, f64>) {
    let mut conditions = HashMap::new();
    let mut multipliers = HashMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Input" => {
                let Some(name) = attr_value(&e, b"name") else {
                    continue;
                };
                if let Some(var) = name.strip_prefix("condition") {
                    if let Some(b) = attr_value(&e, b"boolean") {
                        conditions.insert(var.to_string(), b == "true");
                    }
                } else if let Some(var) = name.strip_prefix("multiplier") {
                    if let Some(n) = attr_value(&e, b"number").and_then(|v| v.parse::<f64>().ok()) {
                        multipliers.insert(var.to_string(), n);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    (conditions, multipliers)
}

/// 抽取 `<Build mainSocketGroup="N">`（1-based 主技能组索引）。缺失返回 `None`。
fn parse_main_socket_group(xml: &str) -> Option<usize> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Build" => {
                return attr_value(&e, b"mainSocketGroup").and_then(|v| v.parse::<usize>().ok());
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

// ── 天赋树 ────────────────────────────────────────────────────────────────────

/// 抽取 `<Tree activeSpec>` 选中 `<Spec nodes>` 的已分配节点 id。
///
/// `activeSpec` 为 1-based 索引；越界 / 缺失时取首个 `<Spec>`。无 `<Spec>` 返回空。
fn parse_passive_nodes(xml: &str) -> Result<Vec<NodeId>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut active_spec: usize = 1;
    let mut specs: Vec<Vec<NodeId>> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = element_name(&e);
                if name == "Tree" {
                    if let Some(v) = attr_value(&e, b"activeSpec")
                        && let Ok(n) = v.parse::<usize>()
                        && n >= 1
                    {
                        active_spec = n;
                    }
                } else if name == "Spec" {
                    let nodes = attr_value(&e, b"nodes")
                        .map(|v| parse_node_csv(&v))
                        .unwrap_or_default();
                    specs.push(nodes);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    if specs.is_empty() {
        return Ok(Vec::new());
    }
    let idx = active_spec.saturating_sub(1).min(specs.len() - 1);
    Ok(specs.swap_remove(idx))
}

/// 解析 `nodes="65091,58814,…"` CSV 为 [`NodeId`]，跳过非数字片段。
fn parse_node_csv(value: &str) -> Vec<NodeId> {
    value
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .map(NodeId)
        .collect()
}

// ── 装备 + 槽位映射 ───────────────────────────────────────────────────────────

/// 抽取 `<Item id>` 文本块并按 `<Items activeItemSet>` 选中的 `<ItemSet>` 槽位映射，
/// 返回 `(EquipmentSlot, Item)` 列表（按槽位 id 字典序，确定性）。
fn parse_items_and_slots(xml: &str) -> Result<Vec<(EquipmentSlot, Item)>, XmlError> {
    let items = parse_item_blocks(xml)?;
    let slot_assignments = parse_active_item_set(xml)?;

    let mut out: Vec<(EquipmentSlot, Item)> = Vec::new();
    for (slot, item_id) in slot_assignments {
        if let Some(item) = items.get(&item_id) {
            out.push((slot, item.clone()));
        }
    }
    out.sort_by_key(|(slot, _)| slot.id());
    Ok(out)
}

/// 解析所有 `<Item id="N">…</Item>` 文本块为 `id -> Item`。
///
/// item 文本是 `<Item>` 的文本内容，其中夹杂 `<ModRange>` 子元素（仅取文本部分）。
/// 单块解析失败时跳过该块（容错），不中止整次解析。
fn parse_item_blocks(xml: &str) -> Result<std::collections::HashMap<u32, Item>, XmlError> {
    let mut reader = Reader::from_str(xml);
    // 保留原始换行 / 缩进：item 文本块按行解析。
    reader.config_mut().trim_text(false);

    let mut result = std::collections::HashMap::new();
    let mut in_item = false;
    let mut current_id: u32 = 0;
    let mut current_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if element_name(&e) == "Item" => {
                in_item = true;
                current_text.clear();
                current_id = attr_value(&e, b"id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
            }
            Ok(Event::Text(t)) if in_item => {
                if let Ok(text) = t.unescape() {
                    current_text.push_str(&text);
                }
            }
            Ok(Event::End(e)) if element_name_end(&e) == "Item" && in_item => {
                in_item = false;
                if current_id > 0
                    && let Ok(item) = parse_pob_xml_item(&current_text)
                {
                    result.insert(current_id, item);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok(result)
}

/// 累积一个 `<ItemSet>` 的槽位映射与武器组标志。
struct ItemSetData {
    id: String,
    use_second_weapon_set: bool,
    slots: Vec<(String, u32)>,
}

/// 从 `<ItemSet>` 标签读取 id 与 `useSecondWeaponSet`（槽位随后由 `<Slot>` 填充）。
fn item_set_data(e: &BytesStart<'_>) -> ItemSetData {
    ItemSetData {
        id: attr_value(e, b"id").unwrap_or_default(),
        use_second_weapon_set: attr_bool(e, b"useSecondWeaponSet"),
        slots: Vec::new(),
    }
}

/// 解析 `<Items activeItemSet>` 选中的 `<ItemSet>`，返回 `(EquipmentSlot, item_id)` 映射。
/// `itemId="0"`（空槽）与枚举外槽名被忽略；武器组按该组 `useSecondWeaponSet` 切换。
fn parse_active_item_set(xml: &str) -> Result<Vec<(EquipmentSlot, u32)>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut active_item_set: Option<String> = None;
    let mut sets: Vec<ItemSetData> = Vec::new();
    let mut current: Option<ItemSetData> = None;

    loop {
        match reader.read_event() {
            // `<Items>` / `<ItemSet>` 带子元素：开始标签。
            Ok(Event::Start(e)) => match element_name(&e).as_str() {
                "Items" => active_item_set = attr_value(&e, b"activeItemSet"),
                "ItemSet" => current = Some(item_set_data(&e)),
                _ => {}
            },
            // `<Slot/>` 恒自闭合；`<ItemSet/>`（无槽位）亦可能自闭合。
            Ok(Event::Empty(e)) => match element_name(&e).as_str() {
                "Items" => active_item_set = attr_value(&e, b"activeItemSet"),
                "ItemSet" => sets.push(item_set_data(&e)),
                "Slot" => {
                    if let Some(cur) = current.as_mut()
                        && let (Some(slot_name), Some(item_id)) = (
                            attr_value(&e, b"name"),
                            attr_value(&e, b"itemId").and_then(|v| v.parse::<u32>().ok()),
                        )
                    {
                        cur.slots.push((slot_name, item_id));
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if element_name_end(&e) == "ItemSet" => {
                if let Some(data) = current.take() {
                    sets.push(data);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    let Some(first_set) = sets.first() else {
        return Ok(Vec::new());
    };
    let chosen = active_item_set
        .as_deref()
        .and_then(|id| sets.iter().find(|s| s.id == id))
        .unwrap_or(first_set);

    let mut assignments = Vec::new();
    for (slot_name, item_id) in &chosen.slots {
        if *item_id == 0 {
            continue;
        }
        if let Some(slot) = slot_from_pob_name(slot_name, chosen.use_second_weapon_set) {
            assignments.push((slot, *item_id));
        }
    }
    Ok(assignments)
}

/// PoB `<Slot name>` → [`EquipmentSlot`]。枚举外槽名（Charm/Flask/Ring 3/防具切换组等）
/// 返回 `None`（这些来源不进入当前装备计算）。武器组按 `use_second_weapon_set` 切换。
fn slot_from_pob_name(name: &str, use_second_weapon_set: bool) -> Option<EquipmentSlot> {
    match name {
        "Helmet" => Some(EquipmentSlot::Helmet),
        "Body Armour" => Some(EquipmentSlot::BodyArmour),
        "Gloves" => Some(EquipmentSlot::Gloves),
        "Boots" => Some(EquipmentSlot::Boots),
        "Amulet" => Some(EquipmentSlot::Amulet),
        "Ring 1" => Some(EquipmentSlot::Ring1),
        "Ring 2" => Some(EquipmentSlot::Ring2),
        "Belt" => Some(EquipmentSlot::Belt),
        "Weapon 1" if !use_second_weapon_set => Some(EquipmentSlot::Weapon1),
        "Weapon 2" if !use_second_weapon_set => Some(EquipmentSlot::Weapon2),
        "Weapon 1 Swap" if use_second_weapon_set => Some(EquipmentSlot::Weapon1),
        "Weapon 2 Swap" if use_second_weapon_set => Some(EquipmentSlot::Weapon2),
        _ => None,
    }
}

// ── 技能宝石组 ────────────────────────────────────────────────────────────────

/// 解析 `<Skills activeSkillSet>` 选中 `<SkillSet>` 下每个 `<Skill>` 为一个
/// [`SocketGroup`]（启用态来自 `Skill.enabled`，gem id 取启用的 `<Gem gemId>`）。
fn parse_socket_groups(xml: &str) -> Result<Vec<SocketGroup>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let active_skill_set = active_skill_set_id(xml)?;

    let mut in_target_set = active_skill_set.is_none(); // 无 active 标记时收集首个遇到的集合
    let mut first_set_consumed = false;
    let mut groups: Vec<SocketGroup> = Vec::new();
    let mut current: Option<SocketGroup> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = element_name(&e);
                match name.as_str() {
                    "SkillSet" => {
                        let set_id = attr_value(&e, b"id");
                        in_target_set = match active_skill_set.as_deref() {
                            Some(target) => set_id.as_deref() == Some(target),
                            None => !first_set_consumed,
                        };
                    }
                    "Skill" if in_target_set => {
                        let enabled = attr_bool_default_true(&e, b"enabled");
                        let mut group = SocketGroup::new().with_enabled(enabled);
                        if let Some(slot) = attr_value(&e, b"slot") {
                            group = group.with_slot(slot);
                        }
                        current = Some(group);
                    }
                    "Gem" if in_target_set => {
                        if let Some(cur) = current.as_mut()
                            && attr_bool_default_true(&e, b"enabled")
                            && let Some(gem_id) = attr_value(&e, b"gemId")
                            && !gem_id.is_empty()
                        {
                            // 捕获每个启用 gem 的 skillId + level（active 与 support 皆收）。
                            if let Some(skill_id) = attr_value(&e, b"skillId")
                                && !skill_id.is_empty()
                            {
                                let level = attr_value(&e, b"level")
                                    .and_then(|v| v.parse::<u32>().ok())
                                    .unwrap_or(1);
                                // 组内首个启用 gem 视为主动技能（PoB Gem 列表 active 在前）。
                                if cur.active_skill_id.is_none() {
                                    cur.active_skill_id = Some(skill_id.clone());
                                    cur.active_gem_level = Some(level);
                                }
                                cur.gem_skills.push(crate::build::GemSkillRef {
                                    skill_id,
                                    gem_level: level,
                                });
                            }
                            cur.gem_ids.push(gem_id);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match element_name_end(&e).as_str() {
                "Skill" if in_target_set => {
                    if let Some(group) = current.take()
                        && !group.gem_ids.is_empty()
                    {
                        groups.push(group);
                    }
                }
                "SkillSet" if in_target_set => {
                    first_set_consumed = true;
                    in_target_set = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    Ok(groups)
}

/// 读取 `<Skills activeSkillSet>` 的目标 SkillSet id（缺失返回 `None`）。
fn active_skill_set_id(xml: &str) -> Result<Option<String>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Skills" => {
                return Ok(attr_value(&e, b"activeSkillSet"));
            }
            Ok(Event::Eof) => return Ok(None),
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
}

// ── quick-xml 小工具 ──────────────────────────────────────────────────────────

fn element_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn element_name_end(e: &quick_xml::events::BytesEnd<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn attr_value(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// 布尔属性：缺失或非 `"true"` 视为 `false`。
fn attr_bool(e: &BytesStart<'_>, key: &[u8]) -> bool {
    attr_value(e, key).as_deref() == Some("true")
}

/// 布尔属性：缺失视为 `true`（PoB 的 `enabled` 缺省启用语义）；显式 `"false"` 才为关。
fn attr_bool_default_true(e: &BytesStart<'_>, key: &[u8]) -> bool {
    attr_value(e, key).as_deref() != Some("false")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Tree activeSpec="1">
        <Spec nodes="100,200,300" treeVersion="0_5"/>
    </Tree>
    <Skills activeSkillSet="1">
        <SkillSet id="1">
            <Skill enabled="true">
                <Gem gemId="Metadata/Items/Gem/Active" skillId="FireballPlayer" level="18" enabled="true"/>
                <Gem gemId="Metadata/Items/Gems/Support" enabled="true"/>
                <Gem gemId="Metadata/Items/Gems/Disabled" enabled="false"/>
            </Skill>
            <Skill enabled="false">
                <Gem gemId="Metadata/Items/Gem/Other" enabled="true"/>
            </Skill>
        </SkillSet>
    </Skills>
    <Items activeItemSet="1" useSecondWeaponSet="false">
        <Item id="1">
Rarity: RARE
Dragon Hold
Topaz Ring
Item Level: 80
Implicits: 1
+30% to Lightning Resistance
+50 to maximum Life
        </Item>
        <Item id="2">
Rarity: RARE
Plague Core
Siege Crossbow
Item Level: 81
Implicits: 0
Adds 47 to 86 Physical Damage
        </Item>
        <ItemSet useSecondWeaponSet="false" title="Default" id="1">
            <Slot name="Ring 1" itemId="1"/>
            <Slot name="Weapon 1" itemId="2"/>
            <Slot name="Ring 2" itemId="0"/>
            <Slot name="Charm 1" itemId="9"/>
        </ItemSet>
    </Items>
</PathOfBuilding2>"#;

    #[test]
    fn parses_full_build_identity_and_version() {
        let build = parse_build(SAMPLE).expect("parse");
        assert_eq!(build.character.level, 92);
        assert_eq!(build.character.class_name, "Ranger");
        assert_eq!(build.character.ascendancy_name, "Deadeye");
        assert_eq!(build.game_version, GameVersion::Poe2);
    }

    #[test]
    fn parses_active_spec_nodes() {
        let build = parse_build(SAMPLE).expect("parse");
        let nodes: Vec<u32> = build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        assert_eq!(nodes, vec![100, 200, 300]);
    }

    #[test]
    fn assigns_items_to_mapped_slots_only() {
        let build = parse_build(SAMPLE).expect("parse");
        // Ring 1 (item 1) 与 Weapon 1 (item 2) 映射；Ring 2 (itemId 0) 空槽；Charm 1 枚举外。
        let slots: Vec<EquipmentSlot> =
            build.equipped_items().into_iter().map(|(s, _)| s).collect();
        assert!(slots.contains(&EquipmentSlot::Ring1));
        assert!(slots.contains(&EquipmentSlot::Weapon1));
        assert!(!slots.contains(&EquipmentSlot::Ring2), "空槽不应分配");
        assert_eq!(slots.len(), 2, "Charm 等枚举外槽位被忽略");
    }

    #[test]
    fn item_text_parsed_into_segments() {
        let build = parse_build(SAMPLE).expect("parse");
        let (_, ring) = build
            .equipped_items()
            .into_iter()
            .find(|(s, _)| *s == EquipmentSlot::Ring1)
            .expect("ring present");
        assert!(
            ring.modifier_texts
                .iter()
                .any(|t| t == "+50 to maximum Life"),
            "戒指 explicit 词条应解析: {:?}",
            ring.modifier_texts
        );
        assert_eq!(ring.implicit_texts, vec!["+30% to Lightning Resistance"]);
    }

    #[test]
    fn parses_socket_groups_respecting_enabled() {
        let build = parse_build(SAMPLE).expect("parse");
        // 两个 Skill：首个 enabled（2 个启用 gem，1 个禁用 gem 跳过），次个 disabled。
        assert_eq!(build.socket_groups.len(), 2);
        let enabled: Vec<&SocketGroup> = build.enabled_socket_groups().collect();
        assert_eq!(enabled.len(), 1, "仅首个 Skill 启用");
        assert_eq!(
            enabled[0].gem_ids,
            vec![
                "Metadata/Items/Gem/Active".to_string(),
                "Metadata/Items/Gems/Support".to_string()
            ],
            "禁用 gem 应被跳过"
        );
        // 首个启用 gem 的 skillId + level 被捕获为主动技能（分等级参数解析键）。
        assert_eq!(
            enabled[0].active_skill_id.as_deref(),
            Some("FireballPlayer")
        );
        assert_eq!(enabled[0].active_gem_level, Some(18));
    }

    #[test]
    fn poe1_root_maps_to_poe1_version() {
        let xml = r#"<PathOfBuilding><Build level="1" className="Witch"/></PathOfBuilding>"#;
        let build = parse_build(xml).expect("parse poe1");
        assert_eq!(build.game_version, GameVersion::Poe1);
        assert_eq!(build.character.class_name, "Witch");
        assert!(build.tree.allocated_nodes.is_empty());
        assert!(build.items.is_empty());
        assert!(build.socket_groups.is_empty());
    }

    #[test]
    fn rejects_non_pob_root() {
        assert!(matches!(
            parse_build("<NotPoB><Build/></NotPoB>"),
            Err(XmlError::NotPobRoot(_))
        ));
    }
}
