//! pobr-cli 库层：把每个子命令实现为纯函数（输入结构 → 输出结构 + `Result`），
//! 便于单测与复用。`main.rs` 只负责 IO 粘合（参数解析、读文件 / stdin、打印 JSON）。
//!
//! 子命令：
//! - [`calculate`]：从基础 [`MinimalInput`] + modifier 文本构造 [`CalculationSession`]，
//!   `perform_minimal` 后返回关键字段 + 未支持文本。
//! - [`parse_mod`]：包装 [`pobr_core::mod_parser::parse_mod`]，返回可序列化的解析报告。
//! - [`parse_item`]：调用 [`pobr_core::item_text::parse_item_text`] +
//!   [`pobr_core::item::ingest_item`] 真正解析 raw item 文本，输出 JSON（解析出的
//!   modifier / section / unsupported）。
//! - [`encode_code`] / [`decode_code`]：包装 PoB Build Code 编解码。

use pobr_core::ModValue;
use pobr_core::calc::{CalculationSession, MinimalInput, MinimalOutput};
use pobr_core::item::ingest_item;
use pobr_core::item_text::{ItemTextError, parse_item_text};
use pobr_core::mod_parser::{ParseStatus, parse_mod as core_parse_mod};
use pobr_data::item::EquipmentSlot;
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
    /// raw item text 结构性解析失败（空输入 / 缺 Rarity / 缺基底）。
    #[error("item text parse error: {0}")]
    ItemText(#[from] ItemTextError),
    /// 功能尚未实现（占位保留）。
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
// parse-item
// ---------------------------------------------------------------------------

/// `parse-item` 子命令输入。
#[derive(Debug, Clone)]
pub struct ParseItemRequest {
    /// 完整 raw item text（PoB 风格英文导出）。
    pub text: String,
}

/// 单条已解析 modifier 的可序列化摘要（`parse-item` 专用）。
#[derive(Debug, Clone, Serialize)]
pub struct ParsedModEntry {
    /// section：`implicit` / `explicit` / `enchant`。
    pub section: String,
    /// 稳定 ModName。
    pub name: String,
    /// 聚合类型（`Base` / `Inc` / `More` / …）。
    pub mod_type: String,
    /// 数值（文本型 modifier 为 `None`）。
    pub value: Option<f64>,
    /// 归因来源 ID（装备槽 + section 后缀）。
    pub source_id: String,
}

/// `parse-item` 解析报告：基础元数据 + 各 section modifier + 未支持词条。
#[derive(Debug, Clone, Serialize)]
pub struct ParseItemReport {
    /// 物品基底名。
    pub base: String,
    /// 稀有度（`Normal` / `Magic` / `Rare` / `Unique`）。
    pub rarity: String,
    /// 品质（0–20）。
    pub quality: u8,
    /// 解析出的 modifier（含 implicit / explicit / enchant / quality）。
    pub modifiers: Vec<ParsedModEntry>,
    /// 无法被 mod_parser 识别的词条文本（保留原始，不报错）。
    pub unsupported: Vec<String>,
}

/// 解析 raw item text，输出结构化 [`ParseItemReport`]。
///
/// 内部调用：
/// 1. [`pobr_core::item_text::parse_item_text`] — 文本分段（rarity / sections / annotations 剥离）→ `Item`；
/// 2. [`pobr_core::item::ingest_item`] — `Item` 词条 → 带归因 `Modifier` 列表。
///
/// 槽位默认为 [`EquipmentSlot::Ring1`]（CLI parse-item 不关联具体槽位，归因 ID 仅供
/// 调试显示，不影响伤害计算）。
///
/// 结构性错误（空文本 / 缺 Rarity / 缺基底）返回 [`CliError::ItemText`]；
/// 词条解析不支持时收入 `unsupported`，不报错。
pub fn parse_item(req: &ParseItemRequest) -> Result<ParseItemReport, CliError> {
    let item = parse_item_text(&req.text)?;

    // CLI parse-item 不关联具体装备槽；使用 Ring1 作为占位槽（归因 ID 供调试）。
    let slot = EquipmentSlot::Ring1;
    let ingest = ingest_item(slot, &item).map_err(|e| CliError::ModParse(e.to_string()))?;

    let modifiers = ingest
        .modifiers
        .iter()
        .map(|m| {
            let (section, sid) = if let Some(origin) = &m.origin {
                let id = &origin.source_id.id;
                let section = if id.contains(".implicit") {
                    "implicit"
                } else if id.contains(".enchant") {
                    "enchant"
                } else if id.contains(".quality") {
                    "quality"
                } else {
                    "explicit"
                };
                (section, id.clone())
            } else {
                ("explicit", String::new())
            };
            ParsedModEntry {
                section: section.to_string(),
                name: m.name.to_string(),
                mod_type: format!("{:?}", m.mod_type),
                value: match &m.value {
                    ModValue::Number(n) => Some(*n),
                    ModValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                    ModValue::Text(_) => None,
                },
                source_id: sid,
            }
        })
        .collect();

    Ok(ParseItemReport {
        base: item.base.to_string(),
        rarity: format!("{:?}", item.rarity),
        quality: item.quality,
        modifiers,
        unsupported: ingest.unsupported,
    })
}

/// 把 [`ParseItemReport`] 渲染为美化的 JSON 字符串。
pub fn parse_item_json(req: &ParseItemRequest) -> Result<String, CliError> {
    let report = parse_item(req)?;
    Ok(serde_json::to_string_pretty(&report)?)
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
