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

use pobr_build::{
    Build, DataOrchestratorOptions, calculate_with_data_session, decode_pob_code, parse_build,
    parse_raw_items_view,
};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::monster::EnemyTier;
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
}

#[derive(Debug, Serialize)]
struct SlotItemJson {
    slot: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct ItemsJson {
    equipped: Vec<SlotItemJson>,
    jewels: Vec<String>,
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
        },
        items: ItemsJson {
            equipped: raw_items
                .equipped
                .into_iter()
                .map(|(slot, text)| SlotItemJson { slot, text })
                .collect(),
            jewels: raw_items.jewels,
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
        main_socket_group: build.main_socket_group,
        config_inputs: build
            .config
            .raw_inputs
            .values
            .iter()
            .map(|(k, v)| (k.clone(), config_value_json(v)))
            .collect(),
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

/// 计算请求：`pob_code` 必填，其余覆盖项可缺省。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CalculateBuildRequest {
    pob_code: String,
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

/// 把请求应用到解码出的 build（主技能组 / config 覆盖）。
fn apply_request_overrides(build: &mut Build, req: &CalculateBuildRequest) -> Result<(), String> {
    if let Some(main) = req.main_socket_group {
        build.main_socket_group = Some(main);
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
        extra_modifier_texts: req.extra_modifiers.clone(),
        ..Default::default()
    })
}

/// 跑一次完整编排（decode → Build → calculate_with_data_session）。
fn run_session(req: &CalculateBuildRequest) -> Result<CalculationSession, String> {
    let mut build = parse_build_from_request(req)?;
    apply_request_overrides(&mut build, req)?;
    run_session_for_build(&build, req)
}

fn parse_build_from_request(req: &CalculateBuildRequest) -> Result<Build, String> {
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
        main_socket_group: req.main_socket_group,
        mode_effective: req.mode_effective,
        enemy_tier: req.enemy_tier.clone(),
        ..Default::default()
    };
    let mut build = parse_build_from_request(&calc_req)?;
    apply_request_overrides(&mut build, &calc_req)?;

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
