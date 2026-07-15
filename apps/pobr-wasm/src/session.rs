//! 计算入口的 JSON 包装：`calculate_json` 把一段 JSON（最小输入 + modifier 文本列表）
//! 跑过 [`CalculationSession`]，返回 [`MinimalOutput`] 关键字段的 JSON 字符串。
//!
//! 设计目标：宿主（非 wasm）也能直接编译、测试，因此本模块只用 `serde_json`
//! 处理边界，不引入 wasm-bindgen。错误统一收敛为 `Result<String, String>`，
//! 便于在 wasm 边界透传为 JS 异常字符串。

use pobr_core::calc::{CalculationSession, MinimalInput, MinimalOutput};
use serde::{Deserialize, Serialize};

/// `calculate_json` 的输入信封。
///
/// 数值字段全部带默认值（缺省即 0），`modifiers` 缺省为空列表，因此调用方
/// 可以只提供关心的字段。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CalculateRequest {
    base_life: f64,
    base_mana: f64,
    base_fire_resistance: f64,
    base_cold_resistance: f64,
    base_lightning_resistance: f64,
    base_accuracy: f64,
    enemy_evasion: f64,
    base_hit_min: f64,
    base_hit_max: f64,
    base_action_rate: f64,
    modifiers: Vec<String>,
}

impl From<&CalculateRequest> for MinimalInput {
    fn from(req: &CalculateRequest) -> Self {
        Self {
            base_life: req.base_life,
            base_mana: req.base_mana,
            base_fire_resistance: req.base_fire_resistance,
            base_cold_resistance: req.base_cold_resistance,
            base_lightning_resistance: req.base_lightning_resistance,
            base_accuracy: req.base_accuracy,
            enemy_evasion: req.enemy_evasion,
            base_hit_min: req.base_hit_min,
            base_hit_max: req.base_hit_max,
            base_action_rate: req.base_action_rate,
        }
    }
}

/// `calculate_json` 的输出信封：[`MinimalOutput`] 的关键标量字段 + 未能解析的
/// modifier 文本列表（便于前端提示）。`breakdown` 不在最小输出里暴露。
#[derive(Debug, Clone, PartialEq, Serialize)]
struct CalculateResponse {
    life: f64,
    mana: f64,
    fire_resistance: f64,
    cold_resistance: f64,
    lightning_resistance: f64,
    crit_chance: f64,
    crit_multiplier: f64,
    total_hit_avg: f64,
    hit_chance: f64,
    action_rate: f64,
    dps: f64,
    unsupported_modifiers: Vec<String>,
}

impl CalculateResponse {
    fn from_output(output: &MinimalOutput, unsupported: &[String]) -> Self {
        Self {
            life: output.life,
            mana: output.mana,
            fire_resistance: output.fire_resistance,
            cold_resistance: output.cold_resistance,
            lightning_resistance: output.lightning_resistance,
            crit_chance: output.crit_chance,
            crit_multiplier: output.crit_multiplier,
            total_hit_avg: output.total_hit_avg,
            hit_chance: output.hit_chance,
            action_rate: output.action_rate,
            dps: output.dps,
            unsupported_modifiers: unsupported.to_vec(),
        }
    }
}

/// 解析 `input_json` 为最小输入 + modifier 文本列表，跑一次最小计算，返回
/// [`MinimalOutput`] 关键字段的 JSON 字符串。
///
/// modifier 解析走已初始化数据（[`crate::state`]）里的引擎规则（唯一解析器，
/// legacy 已删）。`modifiers` 非空但数据未初始化 → 显式报错（fail-fast，不把
/// 词条静默当 Unsupported）；无 modifier 的纯 base 计算不需要数据。无法识别的
/// modifier（`ParseStatus::Unsupported`）不会让本函数失败，而是收集进输出的
/// `unsupported_modifiers`。
pub fn calculate_json(input_json: &str) -> Result<String, String> {
    let request: CalculateRequest =
        serde_json::from_str(input_json).map_err(|err| format!("invalid input json: {err}"))?;

    let input = MinimalInput::from(&request);
    let mut session = CalculationSession::new(input);
    if !request.modifiers.is_empty() {
        let data = crate::state::build_data()
            .map_err(|e| format!("cannot parse modifiers without game data: {e}"))?;
        let rules = data
            .parser_rules
            .clone()
            .ok_or("game data has no parser rules (overlay/mod_parser_rules.json)")?;
        session.set_parser_rules(rules);
    }
    session
        .add_modifier_texts(&request.modifiers)
        .map_err(|err| format!("failed to parse modifier: {err}"))?;

    let output = session.perform_minimal();
    let response = CalculateResponse::from_output(&output, session.unsupported_modifier_texts());

    serde_json::to_string(&response).map_err(|err| format!("failed to serialize output: {err}"))
}
