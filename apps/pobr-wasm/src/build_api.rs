//! Web 前端 JSON 契约：build 解码 / 完整计算 / breakdown / 归因。
//!
//! 契约原则（TODO.md P0）：前端只消费这里的 JSON 形状，不 import Rust 类型、
//! 不复刻计算——所有数值由本模块调用现有 crate 能力算好。JSON 形状由
//! `tests/contract_golden.rs` 钉住，`web/src/api/types.ts` 手写同构 TS 类型；
//! 改形状必须同时动两处 + golden。
//!
//! 全部入口为 `&str -> Result<String, String>`（JSON 入出），错误消息人类可读，
//! wasm 边界直接透传为 JS 异常。

use std::collections::BTreeMap;

use pobr_build::build::GemSkillRef;
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session, decode_pob_code, encode_pob_code, parse_build, parse_notes,
    parse_raw_items_view, radius_jewel_from_text,
};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::item_text::parse_pob_xml_item;
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::item::EquipmentSlot;
use pobr_data::monster::EnemyTier;
use pobr_data::passive_tree::{AttributeChoice, NodeId};
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

fn parse_attribute_choice(s: &str) -> Result<AttributeChoice, String> {
    match s {
        "str" => Ok(AttributeChoice::Strength),
        "dex" => Ok(AttributeChoice::Dexterity),
        "int" => Ok(AttributeChoice::Intelligence),
        other => Err(format!("unknown attribute choice: {other}")),
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
// 0.2 + 0.3 calculate_build_json（display_catalog 全量 + breakdown）
// ---------------------------------------------------------------------------

/// 角色身份覆盖（白手起 build 的必要面 / 导入后改等级职业）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CharacterOverride {
    level: Option<u32>,
    class_name: Option<String>,
    ascendancy_name: Option<String>,
}

/// 手动技能组的宝石条目（active 与 support 皆可；组内首个即主动技能，
/// 与 XML 导入同语义）。`gem_id` 不上行——由 `gem_effects` 表按 `skill_id`
/// 反查（support 分类依赖 gem id）。
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GemInput {
    skill_id: String,
    level: u32,
    quality: u32,
}

impl Default for GemInput {
    fn default() -> Self {
        Self {
            skill_id: String::new(),
            level: 20,
            quality: 0,
        }
    }
}

/// 手动技能组（整份替换 build 的 socket_groups）。
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SocketGroupInput {
    slot: Option<String>,
    enabled: bool,
    /// 装备授予技能组的来源标记（decode 透传回来；手动组为 `None`）。
    source: Option<String>,
    gems: Vec<GemInput>,
}

impl Default for SocketGroupInput {
    fn default() -> Self {
        Self {
            slot: None,
            enabled: true,
            source: None,
            gems: Vec::new(),
        }
    }
}

/// 手动装备（PoB 原始文本块，与导入路径同一解析器；整份替换装备槽）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SlotItemInput {
    slot: String,
    text: String,
}

/// 手动树插槽珠宝（整份替换；只有插槽已加点的才生效，与 XML 导入同门控）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JewelInput {
    socket_node: u32,
    text: String,
}

/// 计算请求：`pob_code` 与 `character` 至少给一个——有 code 则解码为基线再套
/// 覆盖项；无 code 时以 `character` 白手起一个空 build（PoB2 新建 build 语义）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CalculateBuildRequest {
    pob_code: String,
    /// 角色身份覆盖（等级 / 职业 / 升华；逐字段可缺省）。
    character: Option<CharacterOverride>,
    /// 覆盖已加点集合（交互加点：整份替换 build 的 allocated_nodes）。
    allocated_nodes: Option<Vec<u32>>,
    /// 覆盖属性小点三选一（node skill id → `"str"|"dex"|"int"`；整份替换）。
    attribute_choices: Option<BTreeMap<u32, String>>,
    /// 覆盖技能组（手动编辑：整份替换；`None` = 保持 code 解码结果）。
    socket_groups: Option<Vec<SocketGroupInput>>,
    /// 覆盖装备（手动编辑：整份替换全部装备槽）。
    items: Option<Vec<SlotItemInput>>,
    /// 覆盖激活态药剂/护符（整份替换 `utility_slots`；槽名 `Flask 1/2`、`Charm 1..3`）。
    flasks: Option<Vec<SlotItemInput>>,
    /// 覆盖树插槽珠宝（含范围珠宝：`in Radius also grant` 行经几何展开改天赋词条）。
    jewels: Option<Vec<JewelInput>>,
    /// 覆盖主技能组（0-based，Skills 页切换主技能用）。
    main_socket_group: Option<usize>,
    /// 有效 DPS 口径（默认 true，与 PoB2 主面板同口径）。
    mode_effective: Option<bool>,
    /// 敌人档位覆盖（`"none" | "boss" | "pinnacle" | "uber"`）。
    enemy_tier: Option<String>,
    /// 额外全局 modifier 文本（调试 / 假设分析）。
    extra_modifiers: Vec<String>,
    /// `<Config>` 输入覆盖（Config 页开关；bool/number/string 三型）。
    config_inputs: BTreeMap<String, serde_json::Value>,
    /// 笔记（仅 `encode_build_json` 写进 `<Notes>`；计算路径忽略）。
    notes: Option<String>,
}

fn parse_enemy_tier(s: &str) -> Result<EnemyTier, String> {
    match s {
        "none" => Ok(EnemyTier::None),
        "boss" => Ok(EnemyTier::Boss),
        "pinnacle" => Ok(EnemyTier::Pinnacle),
        "uber" => Ok(EnemyTier::Uber),
        other => Err(format!("unknown enemy_tier: {other}")),
    }
}

fn json_to_config_value(v: &serde_json::Value) -> Result<ConfigInputValue, String> {
    match v {
        serde_json::Value::Bool(b) => Ok(ConfigInputValue::Bool(*b)),
        serde_json::Value::Number(n) => Ok(ConfigInputValue::Number(n.as_f64().unwrap_or(0.0))),
        serde_json::Value::String(s) => Ok(ConfigInputValue::Text(s.clone())),
        other => Err(format!("unsupported config value: {other}")),
    }
}

/// [`EquipmentSlot`] → PoB `<Slot name>`（`slot_from_pob_name` 的逆向，encode 用）。
fn pob_slot_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Weapon1 => "Weapon 1",
        EquipmentSlot::Weapon2 => "Weapon 2",
        EquipmentSlot::Helmet => "Helmet",
        EquipmentSlot::BodyArmour => "Body Armour",
        EquipmentSlot::Gloves => "Gloves",
        EquipmentSlot::Boots => "Boots",
        EquipmentSlot::Amulet => "Amulet",
        EquipmentSlot::Ring1 => "Ring 1",
        EquipmentSlot::Ring2 => "Ring 2",
        EquipmentSlot::Ring3 => "Ring 3",
        EquipmentSlot::Belt => "Belt",
    }
}

/// 装备槽稳定 id → [`EquipmentSlot`]（与 `EquipmentSlot::id()` 互逆）。
fn slot_from_id(id: &str) -> Result<EquipmentSlot, String> {
    const ALL: [EquipmentSlot; 11] = [
        EquipmentSlot::Weapon1,
        EquipmentSlot::Weapon2,
        EquipmentSlot::Helmet,
        EquipmentSlot::BodyArmour,
        EquipmentSlot::Gloves,
        EquipmentSlot::Boots,
        EquipmentSlot::Amulet,
        EquipmentSlot::Ring1,
        EquipmentSlot::Ring2,
        EquipmentSlot::Ring3,
        EquipmentSlot::Belt,
    ];
    ALL.into_iter()
        .find(|s| s.id() == id)
        .ok_or_else(|| format!("unknown equipment slot: {id}"))
}

/// 手动技能组 → [`SocketGroup`]：主动技能 = 组内首个非 support 宝石（查数据表，
/// 比 XML 的「首个即 active」更鲁棒）；gem id 由 `gem_effects` 表按效果 id 反查
/// （support 分类依赖它）。
fn socket_group_from_input(input: &SocketGroupInput, data: &BuildData) -> SocketGroup {
    let mut group = SocketGroup {
        slot: input.slot.clone(),
        enabled: input.enabled,
        source: input.source.clone(),
        ..SocketGroup::default()
    };
    for gem in &input.gems {
        if gem.skill_id.is_empty() {
            continue;
        }
        let gem_id = data
            .gem_effects
            .get(&gem.skill_id)
            .map(|e| e.gem_id.clone());
        let is_support = gem_id
            .as_deref()
            .and_then(|id| data.is_support_gem(id))
            .unwrap_or(false);
        if group.active_skill_id.is_none() && !is_support {
            group.active_skill_id = Some(gem.skill_id.clone());
            group.active_gem_level = Some(gem.level);
            group.active_gem_quality = Some(gem.quality);
        }
        group.gem_skills.push(GemSkillRef {
            skill_id: gem.skill_id.clone(),
            gem_level: gem.level,
            quality: gem.quality,
            stat_set_index: None,
        });
        if let Some(gem_id) = gem_id {
            group.gem_ids.push(gem_id);
        }
    }
    group
}

/// 把请求应用到解码/新建的 build（角色 / 树 / 技能组 / 装备 / 主技能组 / config 覆盖）。
fn apply_request_overrides(
    build: &mut Build,
    req: &CalculateBuildRequest,
    data: &BuildData,
) -> Result<(), String> {
    if let Some(ch) = &req.character {
        if let Some(level) = ch.level {
            build.character.level = level;
        }
        if let Some(class_name) = &ch.class_name {
            build.character.class_name = class_name.clone();
        }
        if let Some(ascendancy_name) = &ch.ascendancy_name {
            build.character.ascendancy_name = ascendancy_name.clone();
        }
    }
    if let Some(nodes) = &req.allocated_nodes {
        build.tree.allocated_nodes = nodes.iter().map(|&n| NodeId(n)).collect();
    }
    if let Some(choices) = &req.attribute_choices {
        build.tree.attribute_overrides = choices
            .iter()
            .map(|(&node, choice)| Ok((NodeId(node), parse_attribute_choice(choice)?)))
            .collect::<Result<_, String>>()?;
    }
    if let Some(groups) = &req.socket_groups {
        build.socket_groups = groups
            .iter()
            .map(|g| socket_group_from_input(g, data))
            .collect();
    }
    if let Some(items) = &req.items {
        build.items.clear();
        for item in items {
            let slot = slot_from_id(&item.slot)?;
            let text = localize_input_text(&item.text);
            let parsed = parse_pob_xml_item(&text)
                .map_err(|e| format!("parse item in slot {}: {e:?}", item.slot))?;
            build.items.insert(slot, parsed);
        }
    }
    if let Some(flasks) = &req.flasks {
        build.utility_slots.clear();
        for flask in flasks {
            // 与 XML 导入同语义：只有激活槽进列表；槽名限 PoB 的 Flask/Charm 系。
            if !(flask.slot.starts_with("Flask ") || flask.slot.starts_with("Charm ")) {
                return Err(format!("unknown flask/charm slot: {}", flask.slot));
            }
            let text = localize_input_text(&flask.text);
            let parsed = parse_pob_xml_item(&text)
                .map_err(|e| format!("parse item in slot {}: {e:?}", flask.slot))?;
            build.utility_slots.push((flask.slot.clone(), parsed));
        }
    }
    if let Some(jewels) = &req.jewels {
        // 门控：只收插槽已加点的珠宝（与 XML 导入 parse_radius_jewels 同语义）。
        let allocated: std::collections::HashSet<u32> =
            build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        let mut plain = Vec::new();
        let mut radius = Vec::new();
        for jewel in jewels {
            if !allocated.contains(&jewel.socket_node) {
                continue;
            }
            let text = localize_input_text(&jewel.text);
            let parsed = parse_pob_xml_item(&text)
                .map_err(|e| format!("parse jewel at node {}: {e:?}", jewel.socket_node))?;
            plain.push(parsed);
            if let Some(rj) = radius_jewel_from_text(jewel.socket_node, &text) {
                radius.push(rj);
            }
        }
        build.jewels = plain;
        build.radius_jewels = radius;
    }
    if let Some(main) = req.main_socket_group {
        // 契约是 0-based（web 下标），Build 内部与 PoB XML 同为 1-based。
        build.main_socket_group = Some(main + 1);
    }
    for (key, value) in &req.config_inputs {
        build
            .config
            .raw_inputs
            .values
            .insert(key.clone(), json_to_config_value(value)?);
    }
    Ok(())
}

/// 中文输入预处理（Phase 7.1）：含 CJK 的行尝试「模板反查 → 英文 canonical」
/// 翻译；不认识 / 无 zh-CN 数据时原样保留（parser 记 unsupported，前端可见）。
fn localize_input_text(text: &str) -> String {
    if !crate::zh::has_cjk(text) {
        return text.to_string();
    }
    let Some(translator) = state::zh_translator() else {
        return text.to_string();
    };
    text.lines()
        .map(|line| {
            if crate::zh::has_cjk(line) {
                translator
                    .translate_line(line)
                    .unwrap_or_else(|| line.to_string())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn orchestrator_options(req: &CalculateBuildRequest) -> Result<DataOrchestratorOptions, String> {
    Ok(DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        mode_effective: req.mode_effective.unwrap_or(true),
        enemy_tier: req
            .enemy_tier
            .as_deref()
            .map(parse_enemy_tier)
            .transpose()?
            .unwrap_or_default(),
        extra_modifier_texts: req
            .extra_modifiers
            .iter()
            .map(|line| localize_input_text(line))
            .collect(),
        ..Default::default()
    })
}

/// 跑一次完整编排（decode → Build → calculate_with_data_session）。
fn run_session(req: &CalculateBuildRequest) -> Result<CalculationSession, String> {
    let data = state::build_data()?;
    let mut build = parse_build_from_request(req)?;
    apply_request_overrides(&mut build, req, &data)?;
    run_session_for_build(&build, req)
}

fn parse_build_from_request(req: &CalculateBuildRequest) -> Result<Build, String> {
    if req.pob_code.trim().is_empty() {
        // 白手起 build（PoB2 新建语义）：无装备/无技能组的空 build，
        // 角色身份来自 character 覆盖（职业必填，等级缺省 1）。
        let ch = req
            .character
            .as_ref()
            .ok_or("either pob_code or character is required")?;
        let class_name = ch
            .class_name
            .clone()
            .ok_or("character.class_name is required for a scratch build")?;
        return Ok(Build::new().with_character(CharacterIdentity {
            level: ch.level.unwrap_or(1),
            class_name,
            ascendancy_name: ch.ascendancy_name.clone().unwrap_or_default(),
        }));
    }
    let xml = decode_pob_code(req.pob_code.trim()).map_err(|e| format!("decode: {e}"))?;
    parse_build(&xml).map_err(|e| format!("parse build: {e}"))
}

fn run_session_for_build(
    build: &Build,
    req: &CalculateBuildRequest,
) -> Result<CalculationSession, String> {
    let data = state::build_data()?;
    let opts = orchestrator_options(req)?;
    calculate_with_data_session(build, &data, &opts).map_err(|e| format!("calculate: {e}"))
}

/// breakdown 面向的聚合 ModName（PoB2 侧边栏常驻属性；派生量如 TotalDPS 无
/// 单一聚合名，不在此列——其构成经归因接口看）。
const BREAKDOWN_MOD_NAMES: &[&str] = &[
    "Life",
    "Mana",
    "EnergyShield",
    "Spirit",
    "Armour",
    "Evasion",
    "FireResist",
    "ColdResist",
    "LightningResist",
    "ChaosResist",
    "Speed",
    "CritChance",
    "CritMultiplier",
    "Accuracy",
    "MovementSpeed",
];

#[derive(Debug, Serialize)]
struct BreakdownModJson {
    /// `BASE` / `INC` / `MORE` / `FLAG` / `OVERRIDE` / `LIST`。
    mod_type: &'static str,
    /// 数值视图（Flag/Text 词条为 null）。
    value: Option<f64>,
    /// 词条原文（解析来源文本）。
    source_text: Option<String>,
    /// 归因来源类别（`SourceKind` Debug 名，如 `PassiveNode` / `ItemAffix`）。
    origin_kind: Option<String>,
    /// 归因来源稳定 id（节点 id / 物品槽 / 宝石 id）。
    origin_id: Option<String>,
    /// 来源槽位（装备词条）。
    slot: Option<String>,
}

#[derive(Debug, Serialize)]
struct BreakdownJson {
    /// BASE 词条合计（不含职业/基础注入以外的表达式细节，直接 Σ）。
    base_total: f64,
    /// INC 词条合计（百分点）。
    inc_total: f64,
    /// 逐词条来源列表。
    mods: Vec<BreakdownModJson>,
}

fn breakdown_for(session: &CalculationSession, name: &str) -> Option<BreakdownJson> {
    let mods = session.mods_named(name);
    if mods.is_empty() {
        return None;
    }
    let mut base_total = 0.0;
    let mut inc_total = 0.0;
    let mut entries: Vec<BreakdownModJson> = mods
        .iter()
        .map(|m| {
            let value = m.value.as_number();
            match m.mod_type {
                pobr_data::modifier::ModType::Base => base_total += value.unwrap_or(0.0),
                pobr_data::modifier::ModType::Inc => inc_total += value.unwrap_or(0.0),
                _ => {}
            }
            BreakdownModJson {
                mod_type: m.mod_type.as_trace_label(),
                value,
                source_text: m.source.clone(),
                origin_kind: m.origin.as_ref().map(|o| format!("{:?}", o.source_id.kind)),
                origin_id: m.origin.as_ref().map(|o| o.source_id.id.clone()),
                slot: m.origin.as_ref().and_then(|o| o.slot.clone()),
            }
        })
        .collect();
    // 定序：ModDb 迭代序受上游 HashMap 实例影响，不同数据后端会产生不同顺序；
    // 输出按 (类型, 来源, 词条文本, 数值) 排序，保证契约字节级确定 + UI 展示稳定。
    entries.sort_by(|a, b| {
        (a.mod_type, &a.origin_kind, &a.origin_id, &a.source_text)
            .cmp(&(b.mod_type, &b.origin_kind, &b.origin_id, &b.source_text))
            .then(
                a.value
                    .partial_cmp(&b.value)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    Some(BreakdownJson {
        base_total,
        inc_total,
        mods: entries,
    })
}

#[derive(Debug, Serialize)]
struct CalculateBuildResponse {
    /// display_catalog 全量 Computed 字段（id/value/category）。
    stats: Vec<pobr_data::display_stat::DisplayStatValue>,
    /// 未能解析的 modifier 文本（前端提示区直出）。
    unsupported_modifiers: Vec<String>,
    /// 聚合属性的词条分解（键 = ModName，见 [`BREAKDOWN_MOD_NAMES`]）。
    breakdowns: BTreeMap<String, BreakdownJson>,
}

/// 0.2 + 0.3：完整 build 计算 → display_catalog 全量键值 + breakdown。
///
/// 需先初始化游戏数据（`init` 系列入口）。
pub fn calculate_build_json(request_json: &str) -> Result<String, String> {
    let req: CalculateBuildRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    let session = run_session(&req)?;
    let stats = pobr_core::extract_display_values(session.output());
    let breakdowns = BREAKDOWN_MOD_NAMES
        .iter()
        .filter_map(|name| breakdown_for(&session, name).map(|b| (name.to_string(), b)))
        .collect();
    let response = CalculateBuildResponse {
        stats,
        unsupported_modifiers: session.unsupported_modifier_texts().to_vec(),
        breakdowns,
    };
    serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// full_dps_json（逐技能组 DPS + FullDPS 汇总）
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SkillDpsJson {
    /// 技能组下标（0-based，与 socket_groups 对齐）。
    group_index: usize,
    /// 该组主动技能的授予效果 id。
    skill_id: String,
    dps: f64,
}

#[derive(Debug, Serialize)]
struct FullDpsResponse {
    /// 全部启用伤害技能组的 CombinedDPS 之和。
    full_dps: f64,
    per_skill: Vec<SkillDpsJson>,
}

/// 逐技能组 DPS（请求形状同 [`CalculateBuildRequest`]）。
///
/// 计算量 = `1 + 启用伤害组数` 次完整编排；供点击触发的技能 DPS 面板，
/// 不在每次重算时调用（与归因同模式）。
pub fn full_dps_json(request_json: &str) -> Result<String, String> {
    let req: CalculateBuildRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    let data = state::build_data()?;
    let mut build = parse_build_from_request(&req)?;
    apply_request_overrides(&mut build, &req, &data)?;
    let opts = orchestrator_options(&req)?;
    let report = pobr_build::calculate_full_dps(&build, &data, &opts)
        .map_err(|e| format!("calculate: {e}"))?;
    let response = FullDpsResponse {
        full_dps: report.full_dps,
        per_skill: report
            .per_skill
            .into_iter()
            .map(|s| SkillDpsJson {
                group_index: s.group_index,
                skill_id: s.skill_id,
                dps: s.combined_dps,
            })
            .collect(),
    };
    serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// encode_build_json（编辑态 → PoB2 Build XML → 分享 code）
// ---------------------------------------------------------------------------

/// 当前数据版本对应的 PoB2 树版本标注。
///
/// ponytail: 由 `GOLDEN_PARITY_DATA_VERSION`（`4.<n>.…` = PoE2 `0.<n>`）派生；
/// 数据大版本升级时随黄金版本常量自动跟进，无独立配置项。
fn current_tree_version() -> String {
    let minor = pobr_data::GOLDEN_PARITY_DATA_VERSION
        .split('.')
        .nth(1)
        .unwrap_or("5");
    format!("0_{minor}")
}

/// 编辑态 → PoB2 分享 code。
///
/// 请求形状与 [`CalculateBuildRequest`] 相同（web 端始终发全量覆盖 + 可选
/// `notes`）；`character.class_name` 必填。往返契约：产出的 code 重新解码计算
/// 与直接按请求计算结果一致（`contract_golden::encode_build_roundtrip`）。
pub fn encode_build_json(request_json: &str) -> Result<String, String> {
    let req: CalculateBuildRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    let data = state::build_data()?;
    let ch = req
        .character
        .as_ref()
        .ok_or("character is required to encode")?;
    let class_name = ch
        .class_name
        .clone()
        .ok_or("character.class_name is required to encode")?;

    let mut items: Vec<(String, String)> = Vec::new();
    for item in req.items.as_deref().unwrap_or_default() {
        let slot = slot_from_id(&item.slot)?;
        items.push((pob_slot_name(slot).to_string(), item.text.clone()));
    }
    let mut flasks: Vec<(String, String)> = Vec::new();
    for flask in req.flasks.as_deref().unwrap_or_default() {
        if !(flask.slot.starts_with("Flask ") || flask.slot.starts_with("Charm ")) {
            return Err(format!("unknown flask/charm slot: {}", flask.slot));
        }
        flasks.push((flask.slot.clone(), flask.text.clone()));
    }
    let jewels: Vec<(u32, String)> = req
        .jewels
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|j| (j.socket_node, j.text.clone()))
        .collect();
    let socket_groups: Vec<crate::xml_write::XmlSkillGroup> = req
        .socket_groups
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|g| {
            let mut gems: Vec<(String, String, u32, u32)> = g
                .gems
                .iter()
                .filter(|gem| !gem.skill_id.is_empty())
                .map(|gem| {
                    let gem_id = data
                        .gem_effects
                        .get(&gem.skill_id)
                        .map(|e| e.gem_id.clone())
                        .unwrap_or_default();
                    (gem_id, gem.skill_id.clone(), gem.level, gem.quality)
                })
                .collect();
            // XML 解析路径按「组内首个 gem = 主动技能」判定；计算路径按「首个非
            // support」（查数据表）。把首个非 support gem 前置，使两种判定收敛，
            // 保证 encode → decode 往返后主动技能不漂移。
            if let Some(active_pos) = gems.iter().position(|(gem_id, _, _, _)| {
                // gemId 为空的 gem 会被 XML 解析丢弃，不能当 active 候选。
                !gem_id.is_empty() && !data.is_support_gem(gem_id).unwrap_or(false)
            }) && active_pos > 0
            {
                let active = gems.remove(active_pos);
                gems.insert(0, active);
            }
            crate::xml_write::XmlSkillGroup {
                slot: g.slot.clone(),
                enabled: g.enabled,
                source: g.source.clone(),
                gems,
            }
        })
        .collect();

    // XML 路径对省略的 quest 奖励 / 默认 true 条件按 PoB2 defaultState=true 补注，
    // 请求直连路径没有——对未显式设置的这些 key 写显式 false，两条路径语义收敛。
    let mut config_inputs = req.config_inputs.clone();
    for key in pobr_build::default_on_config_keys() {
        config_inputs
            .entry(key.to_string())
            .or_insert(serde_json::Value::Bool(false));
    }

    let empty_choices = BTreeMap::new();
    let tree_version = current_tree_version();
    let xml = crate::xml_write::write_build_xml(&crate::xml_write::XmlInput {
        level: ch.level.unwrap_or(1),
        class_name: &class_name,
        ascendancy_name: ch.ascendancy_name.as_deref().unwrap_or(""),
        tree_version: &tree_version,
        allocated_nodes: req.allocated_nodes.as_deref().unwrap_or_default(),
        attribute_choices: req.attribute_choices.as_ref().unwrap_or(&empty_choices),
        items,
        flasks,
        jewels,
        socket_groups,
        main_socket_group: req.main_socket_group,
        config_inputs: &config_inputs,
        notes: req.notes.as_deref(),
    });
    encode_pob_code(&xml).map_err(|e| format!("encode: {e}"))
}

// ---------------------------------------------------------------------------
// 0.4 attribution_json（重算差值口径的来源贡献）
// ---------------------------------------------------------------------------

/// 归因请求：对每个来源（装备槽 / 技能组 / 药剂）做「移除后重算」，报告其对
/// 指定展示字段的边际贡献（marginal via recompute——复用完整管线，零新计算逻辑）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AttributionRequest {
    pob_code: String,
    /// 归因目标展示字段（display_catalog id）；缺省 `TotalDPS`/`Life`/`TotalEHP`。
    fields: Vec<String>,
    character: Option<CharacterOverride>,
    allocated_nodes: Option<Vec<u32>>,
    attribute_choices: Option<BTreeMap<u32, String>>,
    socket_groups: Option<Vec<SocketGroupInput>>,
    items: Option<Vec<SlotItemInput>>,
    flasks: Option<Vec<SlotItemInput>>,
    jewels: Option<Vec<JewelInput>>,
    main_socket_group: Option<usize>,
    mode_effective: Option<bool>,
    enemy_tier: Option<String>,
}

const DEFAULT_ATTRIBUTION_FIELDS: &[&str] = &["TotalDPS", "Life", "TotalEHP"];

#[derive(Debug, Serialize)]
struct AttributionEntryJson {
    /// 来源类别：`item` / `socket_group` / `flask`。
    kind: &'static str,
    /// 来源稳定 id（装备槽 id / 组下标 / 药剂槽名）。
    id: String,
    /// 该来源对每个字段的边际贡献（`baseline - 移除后值`；正 = 增益）。
    deltas: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct AttributionResponse {
    /// 基线（完整 build）各字段值。
    baseline: BTreeMap<String, f64>,
    entries: Vec<AttributionEntryJson>,
}

fn display_values_map(session: &CalculationSession, fields: &[String]) -> BTreeMap<String, f64> {
    let all = pobr_core::extract_display_values(session.output());
    fields
        .iter()
        .filter_map(|f| {
            all.iter()
                .find(|v| v.id.as_str() == f.as_str())
                .map(|v| (f.clone(), v.value))
        })
        .collect()
}

/// 0.4：来源贡献归因（重算差值口径）。
///
/// 计算量 = `1 + 来源数` 次完整编排；供点击触发的归因面板，不在每次重算时调用。
pub fn attribution_json(request_json: &str) -> Result<String, String> {
    let req: AttributionRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    let fields: Vec<String> = if req.fields.is_empty() {
        DEFAULT_ATTRIBUTION_FIELDS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        req.fields.clone()
    };
    let calc_req = CalculateBuildRequest {
        pob_code: req.pob_code.clone(),
        character: req.character.clone(),
        allocated_nodes: req.allocated_nodes.clone(),
        attribute_choices: req.attribute_choices.clone(),
        socket_groups: req.socket_groups.clone(),
        items: req.items.clone(),
        flasks: req.flasks.clone(),
        jewels: req.jewels.clone(),
        main_socket_group: req.main_socket_group,
        mode_effective: req.mode_effective,
        enemy_tier: req.enemy_tier.clone(),
        ..Default::default()
    };
    let data = state::build_data()?;
    let mut build = parse_build_from_request(&calc_req)?;
    apply_request_overrides(&mut build, &calc_req, &data)?;

    let baseline_session = run_session_for_build(&build, &calc_req)?;
    let baseline = display_values_map(&baseline_session, &fields);

    // 变体清单：装备槽（移除物品）/ 启用技能组（禁用）/ 药剂槽（移除）。
    // 珠宝暂不逐个归因（radius 珠宝与树插槽几何耦合，v1 跳过）。
    let mut variants: Vec<(&'static str, String, Build)> = Vec::new();
    let mut slots: Vec<_> = build.items.keys().copied().collect();
    slots.sort_by_key(|s| s.id());
    for slot in slots {
        let mut v = build.clone();
        v.items.remove(&slot);
        variants.push(("item", slot.id().to_string(), v));
    }
    for (idx, group) in build.socket_groups.iter().enumerate() {
        if !group.enabled {
            continue;
        }
        let mut v = build.clone();
        v.socket_groups[idx].enabled = false;
        variants.push(("socket_group", idx.to_string(), v));
    }
    for (idx, (slot_name, _)) in build.utility_slots.iter().enumerate() {
        let mut v = build.clone();
        v.utility_slots.remove(idx);
        variants.push(("flask", slot_name.clone(), v));
    }

    let entries = variants
        .into_iter()
        .map(|(kind, id, variant)| {
            let session = run_session_for_build(&variant, &calc_req)?;
            let without = display_values_map(&session, &fields);
            let deltas = fields
                .iter()
                .map(|f| {
                    let base = baseline.get(f).copied().unwrap_or(0.0);
                    let removed = without.get(f).copied().unwrap_or(0.0);
                    (f.clone(), base - removed)
                })
                .collect();
            Ok(AttributionEntryJson { kind, id, deltas })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let response = AttributionResponse { baseline, entries };
    serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// gem_catalog_json（手动技能编辑的宝石选择器目录）
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct GemCatalogEntry {
    /// 授予效果 id（[`GemInput::skill_id`] 上行用的键）。
    skill_id: String,
    /// 展示名（base_items canonical 名；缺失回退 gem id）。
    name: String,
    /// 繁中名（`i18n/zh-TW/base_items.json` 边车；缺条目为 null）。
    name_zh_tw: Option<String>,
    /// 简中名（`i18n/zh-CN/base_items.json` 边车，国服词典转录；缺条目为 null）。
    name_zh_cn: Option<String>,
    /// 宝石颜色（`"str"` 红 / `"dex"` 绿 / `"int"` 蓝；未知为 null），分类筛选用。
    colour: Option<&'static str>,
    is_support: bool,
}

/// 宝石目录：`{skill_id, name, name_zh_tw, colour, is_support}` 按名称排序。
/// 只收带主效果连边的玩家宝石（`gem_effects` overlay 即 vendor Gems.lua 的策展面）。
pub fn gem_catalog_json() -> Result<String, String> {
    let data = state::build_data()?;
    let name_by_gem_id: std::collections::HashMap<&str, &str> = data
        .base_items
        .iter()
        .map(|(name, def)| (def.id.as_str(), name.as_str()))
        .collect();
    // 中文名边车（gem 基底 id → 本地化名）；缺文件（数据包无该语言）降级为空表。
    let game = state::game_data()?;
    let zh_names = game.base_item_names("zh-TW").unwrap_or_default();
    let cn_names = game.base_item_names("zh-CN").unwrap_or_default();
    let mut by_skill: BTreeMap<String, GemCatalogEntry> = BTreeMap::new();
    for gem in data.skill_gems.values() {
        let Some(skill_id) = gem.granted_effect_id.clone() else {
            continue;
        };
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
        });
    }
    let mut entries: Vec<GemCatalogEntry> = by_skill.into_values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.skill_id.cmp(&b.skill_id)));
    serde_json::to_string(&entries).map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// translate_lines_json（英文 → 简中显示翻译：树词条 tooltip / 配置选项等）
// ---------------------------------------------------------------------------

/// 批量把英文词条行翻译为简中显示文本（模板反查；不认识原样返回）。
/// 入参/出参均为 JSON 字符串数组。数据包无 zh-CN 模板时原样全返。
pub fn translate_lines_to_zh_cn_json(lines_json: &str) -> Result<String, String> {
    let lines: Vec<String> =
        serde_json::from_str(lines_json).map_err(|e| format!("invalid lines json: {e}"))?;
    let translator = state::en_to_zh_translator();
    let out: Vec<String> = lines
        .into_iter()
        .map(|line| match &translator {
            Some(t) => t.translate_line(&line).unwrap_or(line),
            None => line,
        })
        .collect();
    serde_json::to_string(&out).map_err(|e| format!("serialize: {e}"))
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

/// `.build` 槽位 → 我们的装备槽 id。`Weapon2`/`Offhand2` 是第二武器组，
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
