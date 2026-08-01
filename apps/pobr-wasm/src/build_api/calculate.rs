//! 完整 build 计算：`calculate_build_json`（display_catalog 全量 + breakdown +
//! 主技能分解）与 `full_dps_json`（逐技能组 DPS + FullDPS 汇总）。

use std::collections::BTreeMap;

use pobr_build::{Build, BuildData};
use pobr_core::calc::CalculationSession;
use serde::Serialize;

use super::request::{
    CalculateBuildRequest, apply_request_overrides, orchestrator_options, parse_build_from_request,
    run_session_for_build,
};
use crate::state;

// 0.2 + 0.3 calculate_build_json（display_catalog 全量 + breakdown）

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

/// 主技能击中伤害的单类型分量（非暴击腿，玩家侧、敌减伤前——PoB2 Calcs 页
/// 伤害分解同口径；占比展示用 `avg`）。
#[derive(Debug, Serialize)]
struct HitDamagePartJson {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`。
    damage_type: String,
    min: f64,
    max: f64,
    /// `(min + max) / 2`。
    avg: f64,
}

/// 主技能（引擎实际计算围绕的技能）身份 + 伤害分解（PoB2 左侧栏 Main Skill +
/// Calcs 伤害分解区的对应物）。每次重算随响应返回——装备/天赋一变即时更新。
#[derive(Debug, Serialize)]
struct MainSkillJson {
    /// 选中技能组（0-based，与请求的 `socket_groups` 对齐）。
    group_index: usize,
    /// 该组主技能的授予效果 id。
    skill_id: String,
    /// 按伤害类型拆分的击中分量。
    hit_damage: Vec<HitDamagePartJson>,
    /// 击中 DPS（`TotalDPS`）。
    hit_dps: f64,
    /// 全部持续伤害合计 DPS（`TotalDotDPS`）。
    dot_dps: f64,
    /// 综合 DPS（`CombinedDPS`）。
    combined_dps: f64,
}

fn main_skill_json(
    build: &Build,
    data: &BuildData,
    output: &pobr_core::calc::OutputTable,
) -> Option<MainSkillJson> {
    let (group_index, skill_id) = pobr_build::resolve_main_skill_selection(build, data)?;
    let hit_damage = output
        .damage_components
        .iter()
        .filter(|c| c.kind == pobr_data::prelude::DamageKind::Hit)
        .map(|c| HitDamagePartJson {
            damage_type: format!("{:?}", c.damage_type),
            min: c.min,
            max: c.max,
            avg: (c.min + c.max) / 2.0,
        })
        .collect();
    Some(MainSkillJson {
        group_index,
        skill_id,
        hit_damage,
        hit_dps: output.dps,
        dot_dps: output.total_dot_dps,
        combined_dps: output.combined_dps,
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
    /// 主技能身份 + 伤害分解（`null` = build 无可解析的伤害主技能）。
    main_skill: Option<MainSkillJson>,
    /// 单件装备/药剂/珠宝文本解析失败的降级记录（该件被跳过，其余照算；
    /// 前端据 slot 标红）。空数组 = 全部解析成功。
    item_errors: Vec<super::request::SlotIssue>,
}

/// 0.2 + 0.3：完整 build 计算 → display_catalog 全量键值 + breakdown + 主技能分解。
///
/// 需先初始化游戏数据（`init` 系列入口）。
pub fn calculate_build_json(request_json: &str) -> Result<String, String> {
    state::cached_response("calculate_build", request_json, || {
        calculate_build_impl(request_json).map_err(super::ApiError::into_json)
    })
}

fn calculate_build_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let mut build = parse_build_from_request(&req)?;
    let item_errors = apply_request_overrides(&mut build, &req, &data)?;
    let session = run_session_for_build(&build, &req)?;
    let stats = pobr_core::extract_display_values(session.output());
    let breakdowns = BREAKDOWN_MOD_NAMES
        .iter()
        .filter_map(|name| breakdown_for(&session, name).map(|b| (name.to_string(), b)))
        .collect();
    let response = CalculateBuildResponse {
        stats,
        unsupported_modifiers: session.unsupported_modifier_texts().to_vec(),
        breakdowns,
        main_skill: main_skill_json(&build, &data, session.output()),
        item_errors,
    };
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}

// full_dps_json（逐技能组 DPS + FullDPS 汇总）

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
    state::cached_response("full_dps", request_json, || {
        full_dps_impl(request_json).map_err(super::ApiError::into_json)
    })
}

fn full_dps_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let mut build = parse_build_from_request(&req)?;
    // 降级记录不进本响应——主面板的 calculate 已报告同一份 item_errors。
    let _ = apply_request_overrides(&mut build, &req, &data)?;
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
    Ok(serde_json::to_string(&response).map_err(|e| format!("serialize: {e}"))?)
}
