//! pobr-cli 库层：把每个子命令实现为纯函数（输入结构 → 输出结构 + `Result`），
//! 便于单测与复用。`main.rs` 只负责 IO 粘合（参数解析、读文件 / stdin、打印 JSON）。
//!
//! 子命令：
//! - [`calculate`]：从基础 [`MinimalInput`] + modifier 文本构造 [`CalculationSession`]，
//!   `perform_minimal` 后返回关键字段 + 未支持文本。
//! - [`parse_mod`]：包装 [`pobr_core::mod_parser::parse_mod`]，返回可序列化的解析报告。
//! - [`parse_item`]：占位 —— REAL 的 `pobr-item` raw item text 解析尚未实现，
//!   当前返回 [`CliError::NotImplemented`]。
//! - [`encode_code`] / [`decode_code`]：包装 PoB Build Code 编解码。

use pobr_core::ModValue;
use pobr_core::calc::{CalculationSession, MinimalInput, MinimalOutput};
use pobr_core::mod_parser::{ParseStatus, parse_mod as core_parse_mod};
use serde::Serialize;
use thiserror::Error;

/// CLI 库层统一错误。
#[derive(Debug, Error)]
pub enum CliError {
    /// modifier 文本无法解析（来自 `pobr_core::mod_parser`）。
    #[error("modifier parse error: {0}")]
    ModParse(String),
    /// Build Code 编解码失败。
    #[error("build code error: {0}")]
    BuildCode(#[from] pobr_build::BuildCodeError),
    /// JSON 序列化失败。
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
    /// 功能尚未实现（如 raw item text 解析依赖未迁移的 `pobr-item`）。
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

// ---------------------------------------------------------------------------
// calculate
// ---------------------------------------------------------------------------

/// `calculate` 子命令输入。
#[derive(Debug, Clone)]
pub struct CalculateRequest {
    /// 基础属性输入（life / mana / 抗性 / 命中 / 敌人闪避 / hit / action rate）。
    pub input: MinimalInput,
    /// 待应用的 modifier 文本（英文 PoB 兼容）。
    pub modifier_texts: Vec<String>,
}

/// `calculate` 输出的关键字段（可序列化为 JSON）。
#[derive(Debug, Clone, Serialize)]
pub struct CalculateOutput {
    pub life: f64,
    pub mana: f64,
    pub fire_resistance: f64,
    pub cold_resistance: f64,
    pub lightning_resistance: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub total_hit_avg: f64,
    pub hit_chance: f64,
    pub action_rate: f64,
    pub dps: f64,
}

impl From<&MinimalOutput> for CalculateOutput {
    fn from(o: &MinimalOutput) -> Self {
        Self {
            life: o.life,
            mana: o.mana,
            fire_resistance: o.fire_resistance,
            cold_resistance: o.cold_resistance,
            lightning_resistance: o.lightning_resistance,
            crit_chance: o.crit_chance,
            crit_multiplier: o.crit_multiplier,
            total_hit_avg: o.total_hit_avg,
            hit_chance: o.hit_chance,
            action_rate: o.action_rate,
            dps: o.dps,
        }
    }
}

/// `calculate` 结果：输出字段 + 未支持（已识别但解析器拒绝聚合）的 modifier 文本。
#[derive(Debug, Clone, Serialize)]
pub struct CalculateResult {
    pub output: CalculateOutput,
    pub unsupported: Vec<String>,
}

/// 执行最小计算：构造 [`CalculationSession`]，应用 modifier 文本，`perform_minimal`。
///
/// 不可解析的文本会让本函数返回 [`CliError::ModParse`]；可识别但 `Unsupported`
/// 的文本被收集到 `unsupported`，不阻断计算。
pub fn calculate(req: &CalculateRequest) -> Result<CalculateResult, CliError> {
    let mut session = CalculationSession::new(req.input);
    session
        .add_modifier_texts(&req.modifier_texts)
        .map_err(|e| CliError::ModParse(e.to_string()))?;

    let output = session.perform_minimal();
    let unsupported = session.unsupported_modifier_texts().to_vec();

    Ok(CalculateResult {
        output: CalculateOutput::from(&output),
        unsupported,
    })
}

/// 把 [`CalculateResult`] 渲染为美化的 JSON 字符串。
pub fn calculate_json(req: &CalculateRequest) -> Result<String, CliError> {
    let result = calculate(req)?;
    Ok(serde_json::to_string_pretty(&result)?)
}

// ---------------------------------------------------------------------------
// parse-mod
// ---------------------------------------------------------------------------

/// 单条 modifier 的可序列化摘要。
#[derive(Debug, Clone, Serialize)]
pub struct ModSummary {
    /// 稳定 stat 名（如 `MaximumLife`）。
    pub name: String,
    /// 聚合类型（`Base` / `Inc` / `More` / …）。
    pub mod_type: String,
    /// 数值（文本型 modifier 为 `None`）。
    pub value: Option<f64>,
    /// 原始来源文本。
    pub source: Option<String>,
}

/// `parse-mod` 解析报告。
#[derive(Debug, Clone, Serialize)]
pub struct ParseModReport {
    /// `Parsed` 或 `Unsupported`。
    pub status: String,
    /// 解析出的 modifier 列表。
    pub mods: Vec<ModSummary>,
    /// 未能识别归类的原始文本（仅 `Unsupported` 时出现）。
    pub unparsed: Option<String>,
}

/// 解析单条 modifier 文本。
///
/// 完全无法识别的文本返回 [`CliError::ModParse`]；可识别但被拒绝的（如 `mirrored`）
/// 返回 `status == "Unsupported"`。
pub fn parse_mod(text: &str) -> Result<ParseModReport, CliError> {
    let outcome = core_parse_mod(text).map_err(|e| CliError::ModParse(e.to_string()))?;

    let status = match outcome.status {
        ParseStatus::Parsed => "Parsed",
        ParseStatus::Unsupported => "Unsupported",
    };

    let mods = outcome
        .mods
        .iter()
        .map(|m| ModSummary {
            name: m.name.to_string(),
            mod_type: format!("{:?}", m.mod_type),
            value: match &m.value {
                ModValue::Number(n) => Some(*n),
                ModValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                ModValue::Text(_) => None,
            },
            source: m.source.clone(),
        })
        .collect();

    Ok(ParseModReport {
        status: status.to_string(),
        mods,
        unparsed: outcome.unparsed,
    })
}

/// 把 [`ParseModReport`] 渲染为美化的 JSON 字符串。
pub fn parse_mod_json(text: &str) -> Result<String, CliError> {
    let report = parse_mod(text)?;
    Ok(serde_json::to_string_pretty(&report)?)
}

// ---------------------------------------------------------------------------
// parse-item（占位）
// ---------------------------------------------------------------------------

/// `parse-item` 子命令输入。
#[derive(Debug, Clone)]
pub struct ParseItemRequest {
    /// 完整 raw item text。
    pub text: String,
}

/// 解析 raw item text。
///
/// REAL 的 `pobr-item` raw item text 解析尚未实现（当前为占位 crate），故本入口
/// 返回 [`CliError::NotImplemented`]。待 `pobr-item` 提供 `parse_raw_item_text` 后接入。
pub fn parse_item(_req: &ParseItemRequest) -> Result<String, CliError> {
    Err(CliError::NotImplemented(
        "raw item text parsing (pobr-item 尚未实现)",
    ))
}

/// 把 `parse-item` 结果渲染为 JSON 字符串（当前未实现，直接返回错误）。
pub fn parse_item_json(req: &ParseItemRequest) -> Result<String, CliError> {
    parse_item(req)
}

// ---------------------------------------------------------------------------
// decode-code / encode-code
// ---------------------------------------------------------------------------

/// 解码 PoB Build Code → XML。
pub fn decode_code(code: &str) -> Result<String, CliError> {
    Ok(pobr_build::decode_pob_code(code)?)
}

/// 编码 XML → PoB Build Code（URL-safe base64 of zlib-compressed XML）。
pub fn encode_code(xml: &str) -> Result<String, CliError> {
    Ok(pobr_build::encode_pob_code(xml)?)
}
