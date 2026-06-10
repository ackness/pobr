//! PoB Build XML → 完整 [`Build`] 的解析。
//!
//! [`crate::xml_serde::parse_build_header`] 只取 `<Build>` 头部（等级 / 职业 / 升华 /
//! 视图）。本模块在其之上**还原可计算的来源**：天赋树已分配节点、装备（按槽位）、
//! 技能宝石组——使 [`crate::calc_orchestrator::calculate_with_data`] 能直接从一份
//! PoB Build Code 端到端计算，而无需调用方手写 XML 抽取。
//!
//! 解析覆盖范围：
//! - **角色身份**：复用 [`parse_build_header`]（等级 / 职业 / 升华 / `viewMode`）。
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

use pobr_core::CampaignProgress;
use pobr_core::item_text::parse_pob_xml_item;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::passive_tree::{AttributeChoice, NodeId, PassiveTreeSpec};

use crate::build::{Build, CharacterIdentity, RadiusJewel, SocketGroup};
use crate::build_code::decode_pob_code;
use crate::error::{BuildError, XmlError};
use crate::xml_serde::parse_build_header;

/// 槽位装备 + 珠宝（无固定槽位）的解析产物。
type EquippedAndJewels = (Vec<(EquipmentSlot, Item)>, Vec<Item>);
/// 装备槽分配（槽位 → item_id）+ 珠宝 item_id 列表。
type SlotAssignments = (Vec<(EquipmentSlot, u32)>, Vec<u32>);

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

    let allocated_nodes = parse_passive_nodes(xml)?;
    let attribute_overrides = parse_attribute_overrides(xml)?;
    let (items, jewels) = parse_items_and_slots(xml)?;
    let radius_jewels = parse_radius_jewels(xml)?;
    let socket_groups = parse_socket_groups(xml)?;
    let main_socket_group = parse_main_socket_group(xml);

    let mut build = Build::new()
        .with_character(CharacterIdentity {
            level: header.identity.level,
            class_name: header.identity.class_name,
            ascendancy_name: header.identity.ascendancy_name,
        })
        .with_view_mode(header.view_mode)
        .with_tree(PassiveTreeSpec {
            allocated_nodes,
            attribute_overrides,
            ..Default::default()
        });
    if let Some(g) = main_socket_group {
        build = build.with_main_socket_group(g);
    }

    for (slot, item) in items {
        build = build.set_item(slot, item);
    }
    if !jewels.is_empty() {
        build = build.with_jewels(jewels);
    }
    if !radius_jewels.is_empty() {
        build = build.with_radius_jewels(radius_jewels);
    }
    for group in socket_groups {
        build = build.add_socket_group(group);
    }

    // 战斗配置（敌人状态 / 条件 / 倍率）→ BuildConfig，经 to_calc_config 进入 cfg，
    // 供条件型词条（`... against Chilled Enemies` 等）按 PoB 保存的开关生效。
    let parsed = parse_config(xml);
    build.config.conditions.extend(parsed.conditions);
    build.config.multipliers.extend(parsed.multipliers);
    build
        .config
        .global_modifier_texts
        .extend(parsed.global_texts);
    build.config.campaign_progress = parsed.campaign_progress;

    Ok(build)
}

/// PoB2 `ConfigOptions.lua` 中 `defaultState = true` 的布尔型配置项映射表：
/// `(XML <Input name>, 计算侧条件变量名)`。
///
/// PoB2 语义：当某 `<Input>` 在 build XML 中**被省略**时，其值取 `defaultState`
/// 而非一律 false。多数布尔条件默认 false（与 PoBR 全 false 回退一致），但下列项默认
/// **true**，须在导入层补默认值（finding 01-06）。
///
/// 这些条目的 XML `name` 不全带 `condition` 前缀，故无法走通用 `strip_prefix("condition")`
/// 路径——逐条按 PoB2 `apply` 函数实际设置的 `Condition:` 变量名映射。CD-bypass 类
/// （`*BypassCD`）PoB2 未设 `Condition:`，PoBR 计算侧也尚未消费，按原 var 名存入以保留语义。
///
/// 出处：vendor `src/Modules/ConfigOptions.lua` `defaultState = true` 各条目
///   （targetBrandedEnemy:277, ConcPathBypassCD:309, inDemonForm:345,
///    FlickerStrikeBypassCD:387, VigilantStrikeBypassCD:700,
///    companionInPresence:1012, conditionChampionIntimidate:1403）。
const DEFAULT_TRUE_CONDITIONS: &[(&str, &str)] = &[
    ("targetBrandedEnemy", "TargetingBrandedEnemy"),
    ("inDemonForm", "DemonForm"),
    ("companionInPresence", "CompanionInPresence"),
    ("conditionChampionIntimidate", "ChampionIntimidate"),
    ("ConcPathBypassCD", "ConcPathBypassCD"),
    ("FlickerStrikeBypassCD", "FlickerStrikeBypassCD"),
    ("VigilantStrikeBypassCD", "VigilantStrikeBypassCD"),
];

/// `<Config>` 解析产物：条件 / 倍率 / 全局词条 + 顶层标量配置项。
#[derive(Debug, Default)]
struct ParsedConfig {
    /// 布尔条件覆盖（去 `condition` 前缀后的变量名 → 值）。
    conditions: HashMap<String, bool>,
    /// 数值乘子覆盖（去 `multiplier` 前缀后的变量名 → 值）。
    multipliers: HashMap<String, f64>,
    /// 全局注入的词条文本（任务奖励等）。
    global_texts: Vec<String>,
    /// `resistancePenalty`（list 型，XML 存 number）映射到的战役进度。XML 省略
    /// 或值不在 PoB2 七档表内时为 `None`（消费方回退 PoB2 默认 Endgame `-60`）。
    campaign_progress: Option<CampaignProgress>,
}

/// 抽取 `<Config>` 的 `<Input name boolean|number|string>` → [`ParsedConfig`]。
/// 名称去 `condition`/`multiplier` 前缀作为变量名（如 `conditionEnemyChilled` → `EnemyChilled`），
/// 与计算侧 `ModTag::Condition`/`Multiplier` 变量约定对齐。
///
/// **省略=默认值**（PoB2 `defaultState`）：XML 中出现的 `<Input>` 按其值；未出现的
/// 布尔条件中，[`DEFAULT_TRUE_CONDITIONS`] 列出的项补 `true`（其余仍由计算侧回退 false）。
fn parse_config(xml: &str) -> ParsedConfig {
    let mut parsed = ParsedConfig::default();
    // 记录 XML 中**出现过**的 `<Input name>`，用于判定哪些 defaultState 项被省略。
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Input" => {
                let Some(name) = attr_value(&e, b"name") else {
                    continue;
                };
                seen_names.insert(name.clone());
                if let Some(var) = name.strip_prefix("condition")
                    && let Some(b) = attr_value(&e, b"boolean")
                {
                    parsed.conditions.insert(var.to_string(), b == "true");
                } else if let Some(charge_cond) = use_charge_condition(&name)
                    && let Some(b) = attr_value(&e, b"boolean")
                {
                    // PoB2 ConfigOptions：`useXCharges` 复选框 → `Condition:UseXCharges` FLAG。
                    // 充能满层默认（current = max）仅在该条件为真时生效（见 charge_multipliers_panel_default）。
                    parsed
                        .conditions
                        .insert(charge_cond.to_string(), b == "true");
                } else if let Some((_, cond_var)) =
                    DEFAULT_TRUE_CONDITIONS.iter().find(|(n, _)| *n == name)
                    && let Some(b) = attr_value(&e, b"boolean")
                {
                    // defaultState=true 项**显式出现**时按其值（不再走默认补填）。
                    parsed
                        .conditions
                        .insert((*cond_var).to_string(), b == "true");
                } else if let Some(var) = name.strip_prefix("multiplier")
                    && let Some(n) = attr_value(&e, b"number").and_then(|v| v.parse::<f64>().ok())
                {
                    parsed.multipliers.insert(var.to_string(), n);
                } else if name == "resistancePenalty" {
                    // PoB2 ConfigOptions `resistancePenalty`（list 型，XML 存 number）：
                    // 0/-10/…/-60 七档 → CampaignProgress 既有表。值不在档位表内
                    // （理论上 PoB2 不会保存）时保持 None，由消费方回退默认 Endgame。
                    parsed.campaign_progress = attr_value(&e, b"number")
                        .and_then(|v| v.parse::<f64>().ok())
                        .and_then(CampaignProgress::from_resistance_penalty);
                } else if name.starts_with("quest") {
                    // PoB2 任务奖励（`questRewards`）按**全局**作用的永久 modifier 注入：
                    // - Options 型（list）：`string="<所选选项>"`（可多行，逐行注入）；
                    // - Stat 型（check，defaultState=true）：`boolean="true"` 或 XML **省略**
                    //   = 已领取（默认表 [`DEFAULT_QUEST_STAT_REWARDS`] 补注），
                    //   `boolean="false"` = 显式放弃。
                    if let Some(s) = attr_value(&e, b"string") {
                        push_quest_lines(&mut parsed.global_texts, &s);
                    } else if attr_bool(&e, b"boolean")
                        && let Some((_, stat)) = DEFAULT_QUEST_STAT_REWARDS
                            .iter()
                            .find(|(key, _)| *key == name)
                    {
                        push_quest_lines(&mut parsed.global_texts, stat);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    // PoB2 `defaultState = true`：XML 省略的项补 true（已显式出现的不覆盖）。
    for (xml_name, cond_var) in DEFAULT_TRUE_CONDITIONS {
        if !seen_names.contains(*xml_name) {
            parsed
                .conditions
                .entry((*cond_var).to_string())
                .or_insert(true);
        }
    }

    // Stat 型任务奖励 defaultState=true：XML 省略的 key 视作已领取，补注默认奖励
    // （PoB2 ConfigOptions `addQuestModsRewardsConfigOptions` 的 check 默认勾选语义）。
    for (key, stat) in DEFAULT_QUEST_STAT_REWARDS {
        if !seen_names.contains(*key) {
            push_quest_lines(&mut parsed.global_texts, stat);
        }
    }

    parsed
}

/// PoB2 `QuestRewards.lua` 中 Stat 型（check）任务奖励默认表：`(XML <Input name> key,
/// 奖励词条)`。仅含 `useConfig=true` 且 `Stat` 单项奖励（Options 型走 string 路径，
/// 默认 Nothing；`+2 Weapon Set Passive Skill Points` 类 useConfig=false 不参与计算）。
const DEFAULT_QUEST_STAT_REWARDS: &[(&str, &str)] = &[
    ("questAct 1ClearfellBeira", "+10% to Cold Resistance"),
    ("questAct 1FreythornKing In The Mists", "+30 to Spirit"),
    ("questAct 1Ogham ManorCandlemass", "+20 to maximum Life"),
    (
        "questAct 2Spires of DesharSisters of Garukhan Shrine",
        "+10% to Lightning Resistance",
    ),
    ("questAct 3Azak BogIgnagduk", "+30 to Spirit"),
    (
        "questAct 3Jiquani's MachinariumBlackjaw",
        "+10% to Fire Resistance",
    ),
    (
        "questAct 4Eye of HinekoraSilent Hall",
        "5% increased Maximum Mana",
    ),
    (
        "questInterlude 2Khari CrossingMolten Shrine",
        "5% increased maximum Life",
    ),
    ("questInterlude 3Kriar VillageLythara", "+40 to Spirit"),
];

/// 任务奖励文本逐行收集（PoB2 `applyModsFromString` 按行拆分；多行选项如
/// Tribal Medicine 含 `\n\t` 连写多条）。
fn push_quest_lines(out: &mut Vec<String>, text: &str) {
    for line in text.lines() {
        let line = line.trim();
        if !line.is_empty() {
            out.push(line.to_string());
        }
    }
}

/// PoB2 充能使用复选框（`use{Power,Frenzy,Endurance}Charges`）→ 计算侧条件变量名。
/// 命中即把对应 `UseXCharges` 条件置入 build config（gate 充能满层默认）。
fn use_charge_condition(name: &str) -> Option<&'static str> {
    match name {
        "usePowerCharges" => Some("UsePowerCharges"),
        "useFrenzyCharges" => Some("UseFrenzyCharges"),
        "useEnduranceCharges" => Some("UseEnduranceCharges"),
        _ => None,
    }
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

/// 解析激活 Spec 的 `<Overrides><AttributeOverride strNodes/dexNodes/intNodes>` →
/// 属性小点三选一映射（PoB2 `PassiveSpec.lua::SwitchAttributeNode` 语义）。
///
/// 与 [`parse_passive_nodes`] 一致地按 `<Tree activeSpec>` 选 Spec；无 Overrides 的
/// build 返回空 map（全部属性小点不贡献属性）。
fn parse_attribute_overrides(xml: &str) -> Result<HashMap<NodeId, AttributeChoice>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut active_spec: usize = 1;
    let mut specs: Vec<HashMap<NodeId, AttributeChoice>> = Vec::new();

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
                    specs.push(HashMap::new());
                } else if name == "AttributeOverride"
                    && let Some(current) = specs.last_mut()
                {
                    for (attr, choice) in [
                        (b"strNodes".as_slice(), AttributeChoice::Strength),
                        (b"dexNodes".as_slice(), AttributeChoice::Dexterity),
                        (b"intNodes".as_slice(), AttributeChoice::Intelligence),
                    ] {
                        if let Some(v) = attr_value(&e, attr) {
                            for node in parse_node_csv(&v) {
                                current.insert(node, choice);
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    if specs.is_empty() {
        return Ok(HashMap::new());
    }
    let idx = active_spec.saturating_sub(1).min(specs.len() - 1);
    Ok(specs.swap_remove(idx))
}

// ── 装备 + 槽位映射 ───────────────────────────────────────────────────────────

/// 抽取 `<Item id>` 文本块并按 `<Items activeItemSet>` 选中的 `<ItemSet>` 槽位映射，
/// 返回 `(EquipmentSlot, Item)` 列表（按槽位 id 字典序，确定性）。
fn parse_items_and_slots(xml: &str) -> Result<EquippedAndJewels, XmlError> {
    let items = parse_item_blocks(xml)?;
    let (slot_assignments, jewel_ids) = parse_active_item_set(xml)?;

    let mut out: Vec<(EquipmentSlot, Item)> = Vec::new();
    for (slot, item_id) in slot_assignments {
        if let Some(item) = items.get(&item_id) {
            out.push((slot, item.clone()));
        }
    }
    out.sort_by_key(|(slot, _)| slot.id());

    // 树上珠宝在 `<Tree><Spec><Sockets><Socket nodeId itemId/>`（非 ItemSet），单独收集。
    let mut all_jewel_ids = jewel_ids;
    all_jewel_ids.extend(parse_tree_socket_item_ids(xml)?);
    all_jewel_ids.sort_unstable();
    all_jewel_ids.dedup();

    let jewels: Vec<Item> = all_jewel_ids
        .iter()
        .filter_map(|id| items.get(id).cloned())
        .collect();
    Ok((out, jewels))
}

/// 解析 `<Sockets><Socket nodeId="N" itemId="M"/>` → 非零 itemId（树上珠宝）。
fn parse_tree_socket_item_ids(xml: &str) -> Result<Vec<u32>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut ids = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Socket" => {
                if let Some(id) = attr_value(&e, b"itemId").and_then(|v| v.parse::<u32>().ok())
                    && id != 0
                {
                    ids.push(id);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok(ids)
}

/// 解析树插槽 `<Socket nodeId="N" itemId="M"/>` → `(socket_node, item_id)`（itemId≠0）。
fn parse_socket_node_items(xml: &str) -> Result<Vec<(u32, u32)>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Socket" => {
                let node = attr_value(&e, b"nodeId").and_then(|v| v.parse::<u32>().ok());
                let item = attr_value(&e, b"itemId").and_then(|v| v.parse::<u32>().ok());
                if let (Some(node), Some(item)) = (node, item)
                    && item != 0
                {
                    out.push((node, item));
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok(out)
}

/// 解析所有 `<Item id="N">…</Item>` 的**原始文本**为 `id -> 文本块`（保留行结构）。
///
/// 与 [`parse_item_blocks`] 区别：本函数不解析为 [`Item`]，而是保留 `Radius:` /
/// `... in Radius also grant ...` 等被 item 解析丢弃的行，供范围珠宝几何展开。
fn parse_raw_item_texts(xml: &str) -> Result<std::collections::HashMap<u32, String>, XmlError> {
    let mut reader = Reader::from_str(xml);
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
                if current_id > 0 {
                    result.insert(current_id, current_text.clone());
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok(result)
}

/// 解析范围珠宝：把树插槽珠宝（`<Socket nodeId itemId>`）含 `... in Radius also grant ...`
/// 词条的，连同其 `Radius:` 档位一起收集为 [`RadiusJewel`]（几何展开输入）。
///
/// 仅收集**确实带 `also grant` 行**的珠宝；无该词条的珠宝不产生条目（其全局词条仍由
/// `jewels` 路径注入，不重复）。
fn parse_radius_jewels(xml: &str) -> Result<Vec<RadiusJewel>, XmlError> {
    let socket_items = parse_socket_node_items(xml)?;
    let raw_texts = parse_raw_item_texts(xml)?;
    let mut out = Vec::new();
    for (socket_node, item_id) in socket_items {
        let Some(text) = raw_texts.get(&item_id) else {
            continue;
        };
        let grant_lines: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| l.contains("in Radius also grant"))
            .map(strip_brace_tags)
            .collect();
        if grant_lines.is_empty() {
            continue;
        }
        let radius_label = text
            .lines()
            .map(str::trim)
            .find_map(|l| l.strip_prefix("Radius:").map(|r| r.trim().to_string()))
            .filter(|s| !s.is_empty());
        out.push(RadiusJewel {
            socket_node,
            radius_label,
            grant_lines,
        });
    }
    // 确定性：按插槽节点、再按行排序。
    out.sort_by(|a, b| {
        a.socket_node
            .cmp(&b.socket_node)
            .then_with(|| a.grant_lines.cmp(&b.grant_lines))
    });
    Ok(out)
}

/// 去掉 PoB 词条行的 `{crafted}` / `{desecrated}` 等花括号标签前缀。
fn strip_brace_tags(line: &str) -> String {
    let mut s = line.trim();
    while let Some(rest) = s.strip_prefix('{') {
        if let Some(end) = rest.find('}') {
            s = rest[end + 1..].trim_start();
        } else {
            break;
        }
    }
    s.to_string()
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

/// 解析 `<Items activeItemSet>` 选中的 `<ItemSet>`，返回 `(装备槽映射, 珠宝 item_id 列表)`。
/// `itemId="0"`（空槽）与枚举外槽名被忽略；武器组按该组 `useSecondWeaponSet` 切换；
/// `Jewel*` / `*Socket*` 槽位的物品收入珠宝列表（全局词条注入，见 orchestrator）。
fn parse_active_item_set(xml: &str) -> Result<SlotAssignments, XmlError> {
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
        return Ok((Vec::new(), Vec::new()));
    };
    let chosen = active_item_set
        .as_deref()
        .and_then(|id| sets.iter().find(|s| s.id == id))
        .unwrap_or(first_set);

    let mut assignments = Vec::new();
    let mut jewel_ids = Vec::new();
    for (slot_name, item_id) in &chosen.slots {
        if *item_id == 0 {
            continue;
        }
        if let Some(slot) = slot_from_pob_name(slot_name, chosen.use_second_weapon_set) {
            assignments.push((slot, *item_id));
        } else if is_jewel_slot(slot_name) {
            jewel_ids.push(*item_id);
        }
    }
    Ok((assignments, jewel_ids))
}

/// PoB 珠宝/深渊槽名（`Jewel 12345` / `… Abyssal Socket N` / `… Socket N`）→ 收入珠宝列表。
fn is_jewel_slot(name: &str) -> bool {
    name.starts_with("Jewel") || name.contains("Socket")
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
                        // PoB `mainActiveSkill`（1-based，索引该组非辅助技能列表）：标记多主动技能
                        // 组（如 Cast on Crit + Comet）里指定的主技能；真正的「跳过 support/meta、
                        // 按序号选」判定在 resolve_main_skill（那里有 granted_effect 数据）。
                        if let Some(n) =
                            attr_value(&e, b"mainActiveSkill").and_then(|v| v.parse::<usize>().ok())
                        {
                            group = group.with_main_active_skill(n);
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
    fn parses_full_build_identity() {
        let build = parse_build(SAMPLE).expect("parse");
        assert_eq!(build.character.level, 92);
        assert_eq!(build.character.class_name, "Ranger");
        assert_eq!(build.character.ascendancy_name, "Deadeye");
    }

    #[test]
    fn parses_active_spec_nodes() {
        let build = parse_build(SAMPLE).expect("parse");
        let nodes: Vec<u32> = build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        assert_eq!(nodes, vec![100, 200, 300]);
    }

    #[test]
    fn parses_attribute_overrides_from_active_spec() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Tree activeSpec="1">
        <Spec nodes="100,200,300" treeVersion="0_5">
            <Overrides>
                <AttributeOverride dexNodes="100,200" intNodes="300" strNodes=""/>
            </Overrides>
        </Spec>
    </Tree>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let ov = &build.tree.attribute_overrides;
        assert_eq!(ov.get(&NodeId(100)), Some(&AttributeChoice::Dexterity));
        assert_eq!(ov.get(&NodeId(200)), Some(&AttributeChoice::Dexterity));
        assert_eq!(ov.get(&NodeId(300)), Some(&AttributeChoice::Intelligence));
        assert_eq!(ov.len(), 2 + 1);
    }

    #[test]
    fn attribute_overrides_default_empty_without_overrides_element() {
        let build = parse_build(SAMPLE).expect("parse");
        assert!(build.tree.attribute_overrides.is_empty());
    }

    #[test]
    fn quest_stat_rewards_default_to_claimed_when_absent() {
        // SAMPLE 无任何 <Input name="quest…"> → 全部 Stat 型奖励按 defaultState=true 补注。
        let build = parse_build(SAMPLE).expect("parse");
        let texts = &build.config.global_modifier_texts;
        assert!(texts.iter().any(|t| t == "+10% to Fire Resistance"));
        assert!(texts.iter().any(|t| t == "+20 to maximum Life"));
        assert!(texts.iter().any(|t| t == "5% increased maximum Life"));
    }

    #[test]
    fn quest_stat_reward_skipped_when_explicitly_unchecked() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Config>
        <Input name="questAct 3Jiquani's MachinariumBlackjaw" boolean="false"/>
        <Input name="questAct 4Halls Of The DeadNgamahu's Test" string="+5% to Fire Resistance"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let texts = &build.config.global_modifier_texts;
        // 显式放弃的 check 型奖励不注入；其余默认项照常补注。
        assert!(!texts.iter().any(|t| t == "+10% to Fire Resistance"));
        // Options 型按所选 string 注入。
        assert!(texts.iter().any(|t| t == "+5% to Fire Resistance"));
        // 未提及的默认项仍在。
        assert!(texts.iter().any(|t| t == "+10% to Cold Resistance"));
    }

    // ── Config resistancePenalty → CampaignProgress（19-G5 接线）─────────────

    #[test]
    fn resistance_penalty_number_maps_to_campaign_progress() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="40" className="Witch"/>
    <Config>
        <Input name="resistancePenalty" number="-10"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(build.config.campaign_progress, Some(CampaignProgress::Act2));
    }

    #[test]
    fn resistance_penalty_omitted_leaves_progress_unset() {
        // SAMPLE 无 resistancePenalty → None（计算侧回退 PoB2 默认 Endgame -60）。
        let build = parse_build(SAMPLE).expect("parse");
        assert_eq!(build.config.campaign_progress, None);
    }

    #[test]
    fn resistance_penalty_unknown_value_falls_back_to_unset() {
        // 不在 PoB2 七档表内的值（理论上不会出现）不强行映射，保持 None。
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="40" className="Witch"/>
    <Config>
        <Input name="resistancePenalty" number="-15"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(build.config.campaign_progress, None);
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
    fn poe1_root_rejected() {
        // PoE2-only：PoE1 的 `PathOfBuilding` 根不再被接受。
        let xml = r#"<PathOfBuilding><Build level="1" className="Witch"/></PathOfBuilding>"#;
        assert!(matches!(parse_build(xml), Err(XmlError::NotPobRoot(_))));
    }

    #[test]
    fn rejects_non_pob_root() {
        assert!(matches!(
            parse_build("<NotPoB><Build/></NotPoB>"),
            Err(XmlError::NotPobRoot(_))
        ));
    }

    // ── Config defaultState 导入（finding 01-06）─────────────────────────────

    #[test]
    fn omitted_default_true_conditions_fill_to_true() {
        // SAMPLE 无 <Config> → 所有 defaultState=true 项应补 true（PoB2「省略=默认值」）。
        let build = parse_build(SAMPLE).expect("parse");
        for (_, cond_var) in DEFAULT_TRUE_CONDITIONS {
            assert_eq!(
                build.config.conditions.get(*cond_var).copied(),
                Some(true),
                "省略的 defaultState=true 条件 {cond_var} 应补 true"
            );
        }
    }

    #[test]
    fn explicit_false_overrides_default_true() {
        // XML 显式给出 inDemonForm=false → 不应被默认 true 覆盖。
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="1" className="Witch"/>
    <Config>
        <Input name="inDemonForm" boolean="false"/>
        <Input name="conditionChampionIntimidate" boolean="false"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(
            build.config.conditions.get("DemonForm").copied(),
            Some(false)
        );
        assert_eq!(
            build.config.conditions.get("ChampionIntimidate").copied(),
            Some(false)
        );
        // 未出现的其余 defaultState=true 项仍补 true。
        assert_eq!(
            build.config.conditions.get("CompanionInPresence").copied(),
            Some(true)
        );
    }

    #[test]
    fn explicit_true_default_condition_maps_to_calc_var() {
        // 显式 targetBrandedEnemy=true → 计算侧条件名 TargetingBrandedEnemy=true。
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="1" className="Witch"/>
    <Config>
        <Input name="targetBrandedEnemy" boolean="true"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(
            build
                .config
                .conditions
                .get("TargetingBrandedEnemy")
                .copied(),
            Some(true)
        );
    }

    #[test]
    fn ordinary_omitted_conditions_remain_unset() {
        // 非 defaultState=true 的普通条件省略时仍不入表（计算侧回退 false）。
        let build = parse_build(SAMPLE).expect("parse");
        assert!(
            !build.config.conditions.contains_key("EnemyChilled"),
            "普通条件省略时不应被填默认值"
        );
    }
}
