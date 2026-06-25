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

use std::path::PathBuf;

use pobr_build::{
    BuildData, DataOrchestratorOptions, calculate_with_data, diagnose_tree_version,
    parse_build_from_code,
};
use pobr_core::ModValue;
use pobr_core::calc::{CalculationSession, MinimalInput, MinimalOutput};
use pobr_core::item::ingest_item;
use pobr_core::item_text::{ItemTextError, parse_item_text};
use pobr_core::mod_parser::{ParseStatus, parse_mod_with_rules};
use pobr_core::rules::{HandlerRegistry, SpecialModRules, register_special_handlers};
use pobr_data::item::EquipmentSlot;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::GameData;
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
    /// Build 解析 / 计算编排失败（来自 pobr-build）。
    #[error("build error: {0}")]
    Build(#[from] pobr_build::BuildError),
    /// 游戏数据加载失败（缺数据目录 / JSON 反序列化）。
    #[error("game data load error: {0}")]
    GameData(#[from] pobr_gamedata::LoadError),
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

/// 启动期编译一次、复用的 parser 规则上下文（M6-A2 数据驱动穿线）。
///
/// special 词条规则集 + handler 注册表从 [`GameData`] 构造。规则缺失（缺
/// `overlay/special_mods.json`）时 `rules == None`，[`parse_mod`] 退回历史
/// 无规则路径（逐值不变）。
pub struct ParseModRules {
    rules: Option<SpecialModRules>,
    registry: HandlerRegistry,
}

impl ParseModRules {
    /// 从游戏数据编译 parser 规则（special 表 + handler 注册表）。
    ///
    /// `overlay/special_mods.json` 缺失 → `rules = None`（解析行为退回历史
    /// 无规则路径）；编译失败（pattern 非法 / id 重复）上抛 [`CliError::ModParse`]。
    pub fn from_game_data(data: &GameData) -> Result<Self, CliError> {
        let mut registry = HandlerRegistry::new();
        register_special_handlers(&mut registry)
            .map_err(|e| CliError::ModParse(format!("special handler 注册失败: {e}")))?;

        let rules = match data.special_mods()? {
            Some(def) if !def.entries.is_empty() => Some(
                SpecialModRules::compile(&def.entries, &registry)
                    .map_err(|e| CliError::ModParse(format!("special 规则编译失败: {e}")))?,
            ),
            _ => None,
        };

        Ok(Self { rules, registry })
    }
}

/// 解析单条 modifier 文本（无规则路径——保留供测试 / 无数据目录场景）。
///
/// 完全无法识别的文本返回 [`CliError::ModParse`]；可识别但被拒绝的（如 `mirrored`）
/// 返回 `status == "Unsupported"`。等价 `parse_mod_with_data(text, None)`，逐值不变。
pub fn parse_mod(text: &str) -> Result<ParseModReport, CliError> {
    parse_mod_with_data(text, None)
}

/// 解析单条 modifier 文本，可注入数据驱动 special 规则（M6-A2 生产路径）。
///
/// `rules = Some` 时走 special 规则增强路径；`None` 时等价历史 [`parse_mod`]，
/// 逐值不变。
pub fn parse_mod_with_data(
    text: &str,
    rules: Option<&ParseModRules>,
) -> Result<ParseModReport, CliError> {
    let (special, registry) = match rules {
        Some(r) => (r.rules.as_ref(), Some(&r.registry)),
        None => (None, None),
    };
    let outcome = parse_mod_with_rules(text, special, registry)
        .map_err(|e| CliError::ModParse(e.to_string()))?;

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
                // 文本/嵌套载荷无标量值（嵌套 mod 由编排层转发，不在摘要里展开）。
                ModValue::Text(_) | ModValue::NestedMods(_) => None,
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
///
/// `data_dir` 指向版本数据目录；从中编译 parser 规则一次（M6-A2 数据驱动穿线）。
/// special 表缺失时退回历史无规则解析。
pub fn parse_mod_json(text: &str, data_dir: &std::path::Path) -> Result<String, CliError> {
    let data = GameData::new(data_dir);
    let rules = ParseModRules::from_game_data(&data)?;
    let report = parse_mod_with_data(text, Some(&rules))?;
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
                    // 文本/嵌套载荷无标量值（嵌套 mod 由编排层转发，不在摘要里展开）。
                    ModValue::Text(_) | ModValue::NestedMods(_) => None,
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

// ---------------------------------------------------------------------------
// calculate-build（PoB Build Code → 完整 Build → 端到端归因计算）
// ---------------------------------------------------------------------------

/// `calculate-build` 子命令输入。
#[derive(Debug, Clone)]
pub struct CalculateBuildRequest {
    /// PoB Build Code（URL-safe Base64 + zlib）。
    pub code: String,
    /// 游戏数据版本目录（含入库 JSON，如 `data/4.5.0.3.4`）。
    pub data_dir: PathBuf,
    /// 敌人等级（`0` = 跟随角色等级）。
    pub enemy_level: u32,
    /// 敌人档位（普通 / Boss / Pinnacle / Uber）。
    pub enemy_tier: EnemyTier,
    /// 有效 DPS 口径（`true` → 计入命中 / 敌人减伤；`false` → 面板口径）。
    pub mode_effective: bool,
}

/// 解析出的 Build 摘要（角色身份 + 各来源计数）。
#[derive(Debug, Clone, Serialize)]
pub struct BuildSummary {
    pub level: u32,
    pub class_name: String,
    pub ascendancy_name: String,
    pub allocated_node_count: usize,
    pub equipped_item_count: usize,
    pub socket_group_count: usize,
}

/// `calculate-build` 计算结果的关键输出字段。
#[derive(Debug, Clone, Serialize)]
pub struct CalculateBuildOutput {
    pub life: f64,
    pub mana: f64,
    pub energy_shield: f64,
    pub armour: f64,
    pub evasion: f64,
    pub fire_resistance: f64,
    pub cold_resistance: f64,
    pub lightning_resistance: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub hit_chance: f64,
    pub total_hit_avg: f64,
    pub dps: f64,
    /// 持续伤害（异常）DPS：流血 / 点燃 / 中毒（PoB2 BleedDPS/IgniteDPS/PoisonDPS）。
    pub bleed_dps: f64,
    pub ignite_dps: f64,
    pub poison_dps: f64,
    /// 全部 DoT 合计（PoB2 TotalDotDPS）。
    pub total_dot_dps: f64,
    /// 异常活跃叠层数（诊断用：bleed/ignite/poison）。
    pub bleed_active_stacks: f64,
    pub ignite_active_stacks: f64,
    pub poison_active_stacks: f64,
    /// 主技能行动速率（次/秒，来自宝石分等级 cast/attack 时间）。
    pub action_rate: f64,
    /// 主技能冷却（秒，来自分等级 cooldown）。
    pub cooldown: f64,
    /// 主技能法力消耗（来自分等级 cost）。
    pub mana_cost: f64,
}

/// 天赋树版本对账诊断（gap B）：build 记录的 `treeVersion` + 已分配但**不在已加载树**
/// 的节点（calc 会静默跳过其贡献——树版本失配的实际症状）。`unknown_node_count > 0`
/// 时 CLI 向 stderr 告警。
#[derive(Debug, Clone, Serialize)]
pub struct TreeVersionDiag {
    pub build_tree_version: Option<String>,
    pub unknown_node_count: usize,
    pub unknown_nodes: Vec<u32>,
}

/// `calculate-build` 报告：Build 摘要 + 计算输出 + 天赋树版本对账诊断。
#[derive(Debug, Clone, Serialize)]
pub struct CalculateBuildReport {
    pub build: BuildSummary,
    pub output: CalculateBuildOutput,
    pub tree_version: TreeVersionDiag,
}

/// 从一份 PoB Build Code 端到端计算：decode → [`parse_build_from_code`] →
/// [`BuildData::load`] → [`calculate_with_data`]，返回 Build 摘要 + 关键输出字段。
///
/// 这是 build-layer 集成的 CLI 入口：把「装备 / 天赋树 / 技能宝石 / 角色基础 / 敌人」
/// 全来源驱动进 REAL 计算引擎，输出可直接与 PoB2 面板对照的标量。
pub fn calculate_build(req: &CalculateBuildRequest) -> Result<CalculateBuildReport, CliError> {
    let build = parse_build_from_code(&req.code)?;

    let game_data = GameData::new(req.data_dir.clone());
    let build_data = BuildData::load(&game_data)?;

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        extra_modifier_texts: Vec::new(),
        inject_character_base: true,
        enemy_level: req.enemy_level,
        enemy_tier: req.enemy_tier,
        mode_effective: req.mode_effective,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &build_data, &opts)?;
    let tree_report = diagnose_tree_version(&build, &build_data);

    let summary = BuildSummary {
        level: build.character.level,
        class_name: build.character.class_name.clone(),
        ascendancy_name: build.character.ascendancy_name.clone(),
        allocated_node_count: build.tree.allocated_nodes.len(),
        equipped_item_count: build.items.len(),
        socket_group_count: build.socket_groups.len(),
    };
    let output = CalculateBuildOutput {
        life: out.life,
        mana: out.mana,
        energy_shield: out.energy_shield,
        armour: out.armour,
        evasion: out.evasion,
        fire_resistance: out.fire_resistance,
        cold_resistance: out.cold_resistance,
        lightning_resistance: out.lightning_resistance,
        crit_chance: out.crit_chance,
        crit_multiplier: out.crit_multiplier,
        hit_chance: out.hit_chance,
        total_hit_avg: out.total_hit_avg,
        dps: out.dps,
        bleed_dps: out.bleed_dps,
        ignite_dps: out.ignite_dps,
        poison_dps: out.poison_dps,
        total_dot_dps: out.total_dot_dps,
        bleed_active_stacks: out.bleed_active_stacks,
        ignite_active_stacks: out.ignite_active_stacks,
        poison_active_stacks: out.poison_active_stacks,
        action_rate: out.action_rate,
        cooldown: out.cooldown,
        mana_cost: out.mana_cost,
    };

    Ok(CalculateBuildReport {
        build: summary,
        output,
        tree_version: TreeVersionDiag {
            build_tree_version: tree_report.build_tree_version,
            unknown_node_count: tree_report.unknown_nodes.len(),
            unknown_nodes: tree_report.unknown_nodes,
        },
    })
}

/// 把 [`CalculateBuildReport`] 渲染为美化的 JSON 字符串。
pub fn calculate_build_json(req: &CalculateBuildRequest) -> Result<String, CliError> {
    let report = calculate_build(req)?;
    Ok(serde_json::to_string_pretty(&report)?)
}
