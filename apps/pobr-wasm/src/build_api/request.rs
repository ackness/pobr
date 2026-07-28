//! 计算请求 DTO + 共享请求处理（解码/新建 build、覆盖项应用、编排选项、会话执行）。
//!
//! 这里的 DTO 字段与共享函数被 `calculate` / `analysis` / `encode` 子模块跨模块
//! 复用，故标 `pub(crate)`（拆分前同处一个模块，字段私有即可）。

use std::collections::BTreeMap;

use pobr_build::build::GemSkillRef;
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session, decode_pob_code, parse_build, radius_jewel_from_text,
};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::item_text::parse_pob_xml_item;
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::monster::EnemyTier;
use pobr_data::passive_tree::{AttributeChoice, NodeId};
use serde::{Deserialize, Serialize};

use super::{ApiError, localize_input_text, slot_from_id};
use crate::state;

/// 单件来源**文本**解析失败的降级记录：跳过该件继续算，进响应 `item_errors`，
/// 前端据此标红该槽而不中断整次计算。结构性错误（未知槽名等客户端 bug）
/// 仍走硬错误 [`ApiError::bad_request`]。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SlotIssue {
    /// 装备槽 id / `Flask N`·`Charm N` 槽名 / `Jewel@<socket_node>`。
    pub(crate) slot: String,
    pub(crate) message: String,
}

pub(crate) fn parse_attribute_choice(s: &str) -> Result<AttributeChoice, String> {
    match s {
        "str" => Ok(AttributeChoice::Strength),
        "dex" => Ok(AttributeChoice::Dexterity),
        "int" => Ok(AttributeChoice::Intelligence),
        other => Err(format!("unknown attribute choice: {other}")),
    }
}

/// 角色身份覆盖（白手起 build 的必要面 / 导入后改等级职业）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CharacterOverride {
    pub(crate) level: Option<u32>,
    pub(crate) class_name: Option<String>,
    pub(crate) ascendancy_name: Option<String>,
}

/// 手动技能组的宝石条目（active 与 support 皆可；组内首个即主动技能，
/// 与 XML 导入同语义）。`gem_id` 不上行——由 `gem_effects` 表按 `skill_id`
/// 反查（support 分类依赖 gem id）。
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GemInput {
    pub(crate) skill_id: String,
    pub(crate) level: u32,
    pub(crate) quality: u32,
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
    pub(crate) slot: Option<String>,
    pub(crate) enabled: bool,
    /// 装备授予技能组的来源标记（decode 透传回来；手动组为 `None`）。
    pub(crate) source: Option<String>,
    pub(crate) gems: Vec<GemInput>,
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
    pub(crate) slot: String,
    pub(crate) text: String,
}

/// 手动树插槽珠宝（整份替换；只有插槽已加点的才生效，与 XML 导入同门控）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JewelInput {
    pub(crate) socket_node: u32,
    pub(crate) text: String,
}

/// 计算请求：`pob_code` 与 `character` 至少给一个——有 code 则解码为基线再套
/// 覆盖项；无 code 时以 `character` 白手起一个空 build（PoB2 新建 build 语义）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CalculateBuildRequest {
    pub(crate) pob_code: String,
    /// 角色身份覆盖（等级 / 职业 / 升华；逐字段可缺省）。
    pub(crate) character: Option<CharacterOverride>,
    /// 覆盖已加点集合（交互加点：整份替换 build 的 allocated_nodes）。
    pub(crate) allocated_nodes: Option<Vec<u32>>,
    /// 覆盖属性小点三选一（node skill id → `"str"|"dex"|"int"`；整份替换）。
    pub(crate) attribute_choices: Option<BTreeMap<u32, String>>,
    /// 覆盖技能组（手动编辑：整份替换；`None` = 保持 code 解码结果）。
    pub(crate) socket_groups: Option<Vec<SocketGroupInput>>,
    /// 覆盖装备（手动编辑：整份替换全部装备槽）。
    pub(crate) items: Option<Vec<SlotItemInput>>,
    /// 覆盖激活态药剂/护符（整份替换 `utility_slots`；槽名 `Flask 1/2`、`Charm 1..3`）。
    pub(crate) flasks: Option<Vec<SlotItemInput>>,
    /// 覆盖树插槽珠宝（含范围珠宝：`in Radius also grant` 行经几何展开改天赋词条）。
    pub(crate) jewels: Option<Vec<JewelInput>>,
    /// 覆盖主技能组（0-based，Skills 页切换主技能用）。
    pub(crate) main_socket_group: Option<usize>,
    /// 有效 DPS 口径（默认 true，与 PoB2 主面板同口径）。
    pub(crate) mode_effective: Option<bool>,
    /// 敌人档位覆盖（`"none" | "boss" | "pinnacle" | "uber"`）。
    pub(crate) enemy_tier: Option<String>,
    /// 额外全局 modifier 文本（调试 / 假设分析）。
    pub(crate) extra_modifiers: Vec<String>,
    /// `<Config>` 输入覆盖（Config 页开关；bool/number/string 三型）。
    pub(crate) config_inputs: BTreeMap<String, serde_json::Value>,
    /// 笔记（仅 `encode_build_json` 写进 `<Notes>`；计算路径忽略）。
    pub(crate) notes: Option<String>,
    /// 导入时的原始 build code（仅 `encode_build_json` 用）：产物以它为底，只替换
    /// 当前 active 的那一套，其余 loadout 原样保留。缺省 = 全量生成单套。
    #[serde(default)]
    pub(crate) base_code: Option<String>,
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
            name_spec: None,
        });
        if let Some(gem_id) = gem_id {
            group.gem_ids.push(gem_id);
        }
    }
    group
}

/// 把请求应用到解码/新建的 build（角色 / 树 / 技能组 / 装备 / 主技能组 / config 覆盖）。
pub(crate) fn apply_request_overrides(
    build: &mut Build,
    req: &CalculateBuildRequest,
    data: &BuildData,
) -> Result<Vec<SlotIssue>, ApiError> {
    let mut issues = Vec::new();
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
            .collect::<Result<_, String>>()
            .map_err(ApiError::bad_request)?;
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
            let slot = slot_from_id(&item.slot)
                .map_err(|e| ApiError::bad_request(e).with_slot(item.slot.as_str()))?;
            let text = localize_input_text(&item.text);
            // 文本解析失败降级：跳过该件继续算，槽位与原因进 issues。
            match parse_pob_xml_item(&text) {
                Ok(parsed) => {
                    build.items.insert(slot, parsed);
                }
                Err(e) => issues.push(SlotIssue {
                    slot: item.slot.clone(),
                    message: format!("{e:?}"),
                }),
            }
        }
    }
    if let Some(flasks) = &req.flasks {
        build.utility_slots.clear();
        for flask in flasks {
            // 与 XML 导入同语义：只有激活槽进列表；槽名限 PoB 的 Flask/Charm 系。
            if !(flask.slot.starts_with("Flask ") || flask.slot.starts_with("Charm ")) {
                return Err(ApiError::bad_request(format!(
                    "unknown flask/charm slot: {}",
                    flask.slot
                ))
                .with_slot(flask.slot.as_str()));
            }
            let text = localize_input_text(&flask.text);
            match parse_pob_xml_item(&text) {
                Ok(parsed) => build.utility_slots.push((flask.slot.clone(), parsed)),
                Err(e) => issues.push(SlotIssue {
                    slot: flask.slot.clone(),
                    message: format!("{e:?}"),
                }),
            }
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
            let parsed = match parse_pob_xml_item(&text) {
                Ok(p) => p,
                Err(e) => {
                    issues.push(SlotIssue {
                        slot: format!("Jewel@{}", jewel.socket_node),
                        message: format!("{e:?}"),
                    });
                    continue;
                }
            };
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
        build.config.raw_inputs.values.insert(
            key.clone(),
            json_to_config_value(value).map_err(ApiError::bad_request)?,
        );
    }

    // 任务奖励在合并后的 config 输入上整份重建（PoB2 defaultState=true 语义）：
    // 有效值 = XML `<Input>` 捕获（raw_inputs）被请求 config_inputs 覆盖的结果；
    // Stat 型省略 = 已领取、显式 false = 放弃，Options 型（string 值）注入所选
    // 词条文本。这让导入 build 后在 Config 页切任务奖励也能生效（解码时 XML 路径
    // 注入的行在此被覆盖）。global_modifier_texts 只承载 quest 行——config_resolve
    // 的解释器通道显式排除 quest 防双计——整份替换是安全的。
    let values = &build.config.raw_inputs.values;
    let mut quest_texts = pobr_build::default_quest_stat_reward_texts(|key| {
        values.get(key).and_then(|v| match v {
            ConfigInputValue::Bool(b) => Some(*b),
            _ => None,
        })
    });
    for (key, value) in values {
        if key.starts_with("quest")
            && let ConfigInputValue::Text(s) = value
        {
            quest_texts.extend(
                s.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from),
            );
        }
    }
    build.config.global_modifier_texts = quest_texts;
    Ok(issues)
}

pub(crate) fn orchestrator_options(
    req: &CalculateBuildRequest,
) -> Result<DataOrchestratorOptions, ApiError> {
    Ok(DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        mode_effective: req.mode_effective.unwrap_or(true),
        enemy_tier: req
            .enemy_tier
            .as_deref()
            .map(parse_enemy_tier)
            .transpose()
            .map_err(ApiError::bad_request)?
            .unwrap_or_default(),
        extra_modifier_texts: req
            .extra_modifiers
            .iter()
            .map(|line| localize_input_text(line))
            .collect(),
        ..Default::default()
    })
}

pub(crate) fn parse_build_from_request(req: &CalculateBuildRequest) -> Result<Build, ApiError> {
    if req.pob_code.trim().is_empty() {
        // 白手起 build（PoB2 新建语义）：无装备/无技能组的空 build，
        // 角色身份来自 character 覆盖（职业必填，等级缺省 1）。
        let ch = req
            .character
            .as_ref()
            .ok_or_else(|| ApiError::bad_request("either pob_code or character is required"))?;
        let class_name = ch.class_name.clone().ok_or_else(|| {
            ApiError::bad_request("character.class_name is required for a scratch build")
        })?;
        // 任务奖励不在此注入：统一由 apply_request_overrides 按合并后的
        // config 输入重建（XML 与直连两路径同一口径，导入后可在 Config 页改）。
        return Ok(Build::new().with_character(CharacterIdentity {
            level: ch.level.unwrap_or(1),
            class_name,
            ascendancy_name: ch.ascendancy_name.clone().unwrap_or_default(),
        }));
    }
    let xml = decode_pob_code(req.pob_code.trim())
        .map_err(|e| ApiError::decode_error(format!("decode: {e}")))?;
    parse_build(&xml).map_err(|e| ApiError::decode_error(format!("parse build: {e}")))
}

pub(crate) fn run_session_for_build(
    build: &Build,
    req: &CalculateBuildRequest,
) -> Result<CalculationSession, ApiError> {
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let opts = orchestrator_options(req)?;
    Ok(calculate_with_data_session(build, &data, &opts).map_err(|e| format!("calculate: {e}"))?)
}
