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
    Build, BuildData, DataOrchestratorOptions, calculate_with_data, diagnose_tree_version,
    parse_build_from_code,
};
use pobr_core::calc::{CalculationSession, MinimalInput, MinimalOutput, OutputTable};
use pobr_core::item::ingest_item;
use pobr_core::item_text::{ItemTextError, parse_item_text};
use pobr_core::mod_parser::{ParseStatus, parse_mod_with_rules};
use pobr_core::rules::{HandlerRegistry, SpecialModRules, register_special_handlers};
use pobr_core::{ActorRef, ModTag, ModValue, Modifier};
use pobr_data::item::EquipmentSlot;
use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};
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
// explain-mod（词条解剖）
//
// 与 parse-mod 的区别：parse-mod 只给 name/type/value/source，**丢掉了 flags 和
// tags**——而 tags（Condition / Multiplier / PerStat / SkillTypes / SlotName …）正是
// 一条词条「在什么情境下生效、按什么缩放」的灵魂（PoB2「一切皆带 tag 的 Mod」那一层）。
// explain-mod 把这些摊开并用人话解释，让「词条是怎么运作的」在命令行直接看得见。
// ---------------------------------------------------------------------------

/// 单个 tag 的人话解释。
#[derive(Debug, Clone, Serialize)]
pub struct TagExplain {
    /// tag 类别（`Condition` / `Multiplier` / `PerStat` / …）。
    pub kind: String,
    /// 人类可读的语义说明。
    pub detail: String,
}

/// 单条 modifier 的「解剖」——比 [`ModSummary`] 多出 flags / keyword_flags / tags
/// （词条的条件与缩放灵魂），并附一行人话总结。
#[derive(Debug, Clone, Serialize)]
pub struct ModAnatomy {
    /// 稳定 stat 名（如 `FireDamage`）。
    pub name: String,
    /// 聚合类型（`BASE` / `INC` / `MORE` / `FLAG` / `OVERRIDE` / `LIST`）。
    pub mod_type: String,
    /// 数值（文本 / 嵌套载荷为 `None`）。
    pub value: Option<f64>,
    /// 文本载荷（`ModValue::Text` / 嵌套提示；数值型为 `None`）。
    pub text_value: Option<String>,
    /// 命中的 [`ModFlags`] 名（如 `Attack` / `Melee` / `Bow`）；空 = 无 flag 约束。
    pub flags: Vec<String>,
    /// 命中的 [`KeywordFlags`] 名（如 `Aura` / `Curse` / `Poison`）；空 = 无。
    pub keyword_flags: Vec<String>,
    /// tag 解释（词条的条件与缩放灵魂）；空 = 任何情境下恒定生效。
    pub tags: Vec<TagExplain>,
    /// 原始来源文本。
    pub source: Option<String>,
    /// 一行人话总结。
    pub plain: String,
}

/// `explain-mod` 解剖报告。
#[derive(Debug, Clone, Serialize)]
pub struct ExplainModReport {
    /// 输入的原始 modifier 文本。
    pub input: String,
    /// `Parsed` 或 `Unsupported`。
    pub status: String,
    /// 解析出的每条 modifier 的解剖。
    pub mods: Vec<ModAnatomy>,
    /// 未能识别归类的原始文本（仅 `Unsupported` 时出现）。
    pub unparsed: Option<String>,
}

/// 解剖单条 modifier 文本：解析后摊开每条 [`Modifier`] 的全部字段（含 flags / tags），
/// 附人话解释。`rules` 同 [`parse_mod_with_data`]（`None` = 无 special 规则路径）。
pub fn explain_mod(
    text: &str,
    rules: Option<&ParseModRules>,
) -> Result<ExplainModReport, CliError> {
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

    let mods = outcome.mods.iter().map(anatomy_of).collect();

    Ok(ExplainModReport {
        input: text.to_string(),
        status: status.to_string(),
        mods,
        unparsed: outcome.unparsed,
    })
}

/// 把一条 [`Modifier`] 拆成可序列化的 [`ModAnatomy`]。
fn anatomy_of(m: &Modifier) -> ModAnatomy {
    let (value, text_value) = match &m.value {
        ModValue::Number(n) => (Some(*n), None),
        ModValue::Bool(b) => (Some(if *b { 1.0 } else { 0.0 }), None),
        ModValue::Text(t) => (None, Some(t.clone())),
        ModValue::NestedMods(_) => (None, Some("<嵌套 modifier 载荷>".to_string())),
    };
    let tags: Vec<TagExplain> = m.tags.iter().map(explain_tag).collect();
    let plain = plain_summary(m, &tags);
    ModAnatomy {
        name: m.name.to_string(),
        mod_type: m.mod_type.as_trace_label().to_string(),
        value,
        text_value,
        flags: mod_flag_names(m.flags),
        keyword_flags: keyword_flag_names(m.keyword_flags),
        tags,
        source: m.source.clone(),
        plain,
    }
}

/// [`ModFlags`] 位 → 名字列表（仅独立位，不含 mask）。
fn mod_flag_names(flags: ModFlags) -> Vec<String> {
    const TABLE: &[(ModFlags, &str)] = &[
        (ModFlags::ATTACK, "Attack"),
        (ModFlags::SPELL, "Spell"),
        (ModFlags::HIT, "Hit"),
        (ModFlags::DOT, "Dot"),
        (ModFlags::CAST, "Cast"),
        (ModFlags::THORNS, "Thorns"),
        (ModFlags::MELEE, "Melee"),
        (ModFlags::AREA, "Area"),
        (ModFlags::PROJECTILE, "Projectile"),
        (ModFlags::AILMENT, "Ailment"),
        (ModFlags::MELEE_HIT, "MeleeHit"),
        (ModFlags::WEAPON, "Weapon"),
        (ModFlags::AXE, "Axe"),
        (ModFlags::BOW, "Bow"),
        (ModFlags::CLAW, "Claw"),
        (ModFlags::DAGGER, "Dagger"),
        (ModFlags::MACE, "Mace"),
        (ModFlags::STAFF, "Staff"),
        (ModFlags::SWORD, "Sword"),
        (ModFlags::WAND, "Wand"),
        (ModFlags::UNARMED, "Unarmed"),
        (ModFlags::CROSSBOW, "Crossbow"),
        (ModFlags::FLAIL, "Flail"),
        (ModFlags::SPEAR, "Spear"),
        (ModFlags::WARSTAFF, "Warstaff"),
        (ModFlags::TALISMAN, "Talisman"),
        (ModFlags::FISHING, "Fishing"),
        (ModFlags::WEAPON_MELEE, "WeaponMelee"),
        (ModFlags::WEAPON_RANGED, "WeaponRanged"),
        (ModFlags::WEAPON_1H, "Weapon1H"),
        (ModFlags::WEAPON_2H, "Weapon2H"),
    ];
    TABLE
        .iter()
        .filter(|(bit, _)| flags.intersects(*bit))
        .map(|(_, name)| (*name).to_string())
        .collect()
}

/// [`KeywordFlags`] 位 → 名字列表（带 `MatchAll` 标记）。
fn keyword_flag_names(flags: KeywordFlags) -> Vec<String> {
    const TABLE: &[(KeywordFlags, &str)] = &[
        (KeywordFlags::AURA, "Aura"),
        (KeywordFlags::CURSE, "Curse"),
        (KeywordFlags::TOTEM, "Totem"),
        (KeywordFlags::ATTACK, "Attack"),
        (KeywordFlags::SPELL, "Spell"),
        (KeywordFlags::HIT, "Hit"),
        (KeywordFlags::AILMENT, "Ailment"),
        (KeywordFlags::POISON, "Poison"),
        (KeywordFlags::BLEED, "Bleed"),
        (KeywordFlags::IGNITE, "Ignite"),
        (KeywordFlags::PHYSICAL_DOT, "PhysicalDot"),
        (KeywordFlags::LIGHTNING_DOT, "LightningDot"),
        (KeywordFlags::COLD_DOT, "ColdDot"),
        (KeywordFlags::FIRE_DOT, "FireDot"),
        (KeywordFlags::CHAOS_DOT, "ChaosDot"),
    ];
    let masked = flags.without_match_all();
    let mut names: Vec<String> = TABLE
        .iter()
        .filter(|(bit, _)| masked.intersects(*bit))
        .map(|(_, name)| (*name).to_string())
        .collect();
    if flags.requires_match_all() {
        names.push("MatchAll".to_string());
    }
    names
}

/// 跨 actor 取数后缀（同 actor 为空串）。
fn actor_label(actor: Option<ActorRef>) -> &'static str {
    match actor {
        None => "",
        Some(ActorRef::Player) => "玩家侧 ",
        Some(ActorRef::Parent) => "父 actor 侧 ",
        Some(ActorRef::Minion) => "召唤物侧 ",
    }
}

/// 单个 [`ModTag`] → 人话解释。
fn explain_tag(tag: &ModTag) -> TagExplain {
    match tag {
        ModTag::Condition {
            var,
            negated,
            actor,
        } => TagExplain {
            kind: "Condition".to_string(),
            detail: format!(
                "仅当{}条件 `{}` {}",
                actor_label(*actor),
                var,
                if *negated {
                    "不成立时生效"
                } else {
                    "成立时生效"
                }
            ),
        },
        ModTag::ConditionAnyOf { vars, negated } => TagExplain {
            kind: "ConditionAnyOf".to_string(),
            detail: format!(
                "仅当条件 {} 中任一{}",
                vars.iter()
                    .map(|v| format!("`{v}`"))
                    .collect::<Vec<_>>()
                    .join("/"),
                if *negated {
                    "都不成立时生效"
                } else {
                    "成立时生效"
                }
            ),
        },
        ModTag::Multiplier {
            var,
            div,
            limit,
            actor,
            limit_var,
            limit_actor,
            invert,
            limit_total,
        } => {
            let mut detail = if *div == 1.0 {
                format!("每点{}`{}` 缩放数值", actor_label(*actor), var)
            } else {
                format!("每 {} 点{}`{}` 缩放数值", div, actor_label(*actor), var)
            };
            let cap = if *limit_total { "总量" } else { "计数" };
            if let Some(l) = limit {
                detail.push_str(&format!("，{cap}上限 {l}"));
            } else if let Some(lv) = limit_var {
                detail.push_str(&format!(
                    "，{cap}上限取 {}`{lv}`",
                    actor_label(*limit_actor)
                ));
            }
            if *invert {
                detail.push_str("，取倒数(1/n)");
            }
            TagExplain {
                kind: "Multiplier".to_string(),
                detail,
            }
        }
        ModTag::PerStat {
            stat,
            div,
            limit,
            limit_var,
            actor,
        } => {
            let mut detail = if *div == 1.0 {
                format!("按{}已算出属性 `{}` 缩放数值", actor_label(*actor), stat)
            } else {
                format!(
                    "按{}已算出属性 `{}` / {} 缩放数值",
                    actor_label(*actor),
                    stat,
                    div
                )
            };
            if let Some(l) = limit {
                detail.push_str(&format!("，上限 {l}"));
            } else if let Some(lv) = limit_var {
                detail.push_str(&format!("，上限取 `{lv}`"));
            }
            TagExplain {
                kind: "PerStat".to_string(),
                detail,
            }
        }
        ModTag::PercentStat { stat, percent } => TagExplain {
            kind: "PercentStat".to_string(),
            detail: match percent {
                Some(p) => format!("按已算出属性 `{stat}` 的 {p}% 缩放数值（结果向上取整）"),
                None => format!("按已算出属性 `{stat}` 缩放数值（结果向上取整）"),
            },
        },
        ModTag::MultiplierThreshold {
            var,
            threshold,
            upper,
        } => TagExplain {
            kind: "MultiplierThreshold".to_string(),
            detail: format!(
                "仅当 `{}` {} {} 时生效",
                var,
                if *upper { "≤" } else { "≥" },
                threshold
            ),
        },
        ModTag::StatThreshold {
            stat,
            threshold,
            upper,
        } => TagExplain {
            kind: "StatThreshold".to_string(),
            detail: format!(
                "仅当已算出属性 `{}` {} {} 时生效",
                stat,
                if *upper { "≤" } else { "≥" },
                threshold
            ),
        },
        ModTag::GlobalLimit { value, key } => TagExplain {
            kind: "GlobalLimit".to_string(),
            detail: format!("全局累计上限 {value}（记账桶 `{key}`）"),
        },
        ModTag::DamageType(dt) => TagExplain {
            kind: "DamageType".to_string(),
            detail: format!("限 {dt:?} 伤害"),
        },
        ModTag::SkillTypes(st) => TagExplain {
            kind: "SkillTypes".to_string(),
            detail: format!("限技能类型 {st:?}"),
        },
        ModTag::SkillName { names } => TagExplain {
            kind: "SkillName".to_string(),
            detail: format!("仅当主技能是 `{}` 时生效", names.join("` / `")),
        },
        ModTag::SlotName(slot) => TagExplain {
            kind: "SlotName".to_string(),
            detail: format!("仅作用于装备槽 `{slot}`"),
        },
        ModTag::DistanceRamp { ramp } => TagExplain {
            kind: "DistanceRamp".to_string(),
            detail: format!("随与敌人距离插值缩放（(距离,倍率) 点列：{ramp:?}）"),
        },
    }
}

/// 一行人话总结：把聚合桶 + 数值 + 条件串成自然语言。
fn plain_summary(m: &Modifier, tags: &[TagExplain]) -> String {
    let bucket = match m.mod_type {
        ModType::Base => "基础值(BASE)桶",
        ModType::Inc => "增加(INC)加算桶",
        ModType::More => "更多(MORE)连乘桶",
        ModType::Flag => "开关(FLAG)",
        ModType::Override => "覆盖(OVERRIDE)",
        ModType::List => "列表(LIST)",
    };
    let val = match &m.value {
        ModValue::Number(n) => format!("{n:+}"),
        ModValue::Bool(b) => b.to_string(),
        ModValue::Text(t) => t.clone(),
        ModValue::NestedMods(_) => "<嵌套>".to_string(),
    };
    let cond = if tags.is_empty() {
        String::new()
    } else {
        format!(
            "（{}）",
            tags.iter()
                .map(|t| t.detail.clone())
                .collect::<Vec<_>>()
                .join("；")
        )
    };
    format!("给属性 `{}` 的{bucket} {val}{cond}", m.name)
}

/// 把 [`ExplainModReport`] 渲染为人类可读文本（命令行默认输出）。
pub fn render_explain(report: &ExplainModReport) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(s, "词条原文: {}", report.input);
    let _ = writeln!(s, "状态:     {}", report.status);
    if let Some(un) = &report.unparsed {
        let _ = writeln!(s, "未识别:   {un}");
    }
    if report.mods.is_empty() {
        let _ = writeln!(s, "\n（没有解析出任何 modifier）");
        return s;
    }
    for (i, m) in report.mods.iter().enumerate() {
        let _ = writeln!(s, "\n→ Modifier #{}", i + 1);
        let _ = writeln!(s, "   名字 (ModName): {}", m.name);
        match (m.value, &m.text_value) {
            (Some(v), _) => {
                let _ = writeln!(s, "   类型 / 值:      {} = {v}", m.mod_type);
            }
            (None, Some(t)) => {
                let _ = writeln!(s, "   类型 / 值:      {} = {t}", m.mod_type);
            }
            (None, None) => {
                let _ = writeln!(s, "   类型:           {}", m.mod_type);
            }
        }
        let flags = if m.flags.is_empty() {
            "(无)".to_string()
        } else {
            m.flags.join(" | ")
        };
        let _ = writeln!(s, "   flags:          {flags}");
        let kw = if m.keyword_flags.is_empty() {
            "(无)".to_string()
        } else {
            m.keyword_flags.join(" | ")
        };
        let _ = writeln!(s, "   keyword:        {kw}");
        if m.tags.is_empty() {
            let _ = writeln!(s, "   tags:           (无——任何情境下恒定生效)");
        } else {
            let _ = writeln!(s, "   tags（条件与缩放灵魂）:");
            for t in &m.tags {
                let _ = writeln!(s, "     • {:<14} {}", t.kind, t.detail);
            }
        }
        if let Some(src) = &m.source {
            let _ = writeln!(s, "   来源文本:       {src}");
        }
        let _ = writeln!(s, "   人话:           {}", m.plain);
    }
    s
}

/// `explain-mod` 的 JSON 输出（`--json`）：从游戏数据编译规则一次后解剖。
pub fn explain_mod_json(text: &str, data_dir: &std::path::Path) -> Result<String, CliError> {
    let data = GameData::new(data_dir);
    let rules = ParseModRules::from_game_data(&data)?;
    let report = explain_mod(text, Some(&rules))?;
    Ok(serde_json::to_string_pretty(&report)?)
}

/// `explain-mod` 的人类可读文本输出（命令行默认）：从游戏数据编译规则一次后解剖。
pub fn explain_mod_text(text: &str, data_dir: &std::path::Path) -> Result<String, CliError> {
    let data = GameData::new(data_dir);
    let rules = ParseModRules::from_game_data(&data)?;
    let report = explain_mod(text, Some(&rules))?;
    Ok(render_explain(&report))
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
    /// 异常叠层上限（诊断用：bleed/ignite/poison；1 = 不可叠层）。
    pub bleed_max_stacks: f64,
    pub ignite_max_stacks: f64,
    pub poison_max_stacks: f64,
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

    let summary = build_summary(&build);
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
        bleed_max_stacks: out.bleed_max_stacks,
        ignite_max_stacks: out.ignite_max_stacks,
        poison_max_stacks: out.poison_max_stacks,
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

/// 解析出的 [`Build`] → [`BuildSummary`]（calculate_build 与 marginal 共用）。
fn build_summary(build: &Build) -> BuildSummary {
    BuildSummary {
        level: build.character.level,
        class_name: build.character.class_name.clone(),
        ascendancy_name: build.character.ascendancy_name.clone(),
        allocated_node_count: build.tree.allocated_nodes.len(),
        equipped_item_count: build.items.len(),
        socket_group_count: build.socket_groups.len(),
    }
}

// ---------------------------------------------------------------------------
// 边际贡献（explain-mod --build）
//
// 「在我现有的 build 上加这条词条，会怎样？」把候选词条文本作为 extra_modifier_texts
// 注入玩家侧管线，重算一遍，与基线逐字段对比——复用 calculate_with_data 既有机制
// （extra_modifier_texts 经 session.add_modifier_texts 入玩家 ModDb）。
// ---------------------------------------------------------------------------

/// 单个输出字段的边际变化（加入候选词条前后）。
#[derive(Debug, Clone, Serialize)]
pub struct OutputDelta {
    /// 输出字段可读名（如 `DPS (hit)` / `Life`）。
    pub key: String,
    /// 加词条前的值。
    pub before: f64,
    /// 加词条后的值。
    pub after: f64,
    /// 差值（after − before）。
    pub delta: f64,
    /// 相对变化百分比（before == 0 时为 `None`）。
    pub pct: Option<f64>,
}

/// `explain-mod --build` 的边际贡献请求。
#[derive(Debug, Clone)]
pub struct MarginalRequest {
    /// PoB Build Code（URL-safe Base64 + zlib）。
    pub build_code: String,
    /// 游戏数据版本目录。
    pub data_dir: PathBuf,
    /// 敌人等级（`0` = 跟随角色等级）。
    pub enemy_level: u32,
    /// 敌人档位。
    pub enemy_tier: EnemyTier,
    /// 有效 DPS 口径（`true`）/ 面板口径（`false`）。
    pub mode_effective: bool,
    /// 要加进 build 的候选词条文本（通常即 explain 的那条）。
    pub mod_texts: Vec<String>,
}

/// 边际贡献报告：build 摘要 + 加入的词条 + 有变化的输出字段差值。
#[derive(Debug, Clone, Serialize)]
pub struct MarginalReport {
    /// build 摘要（角色身份 + 各来源计数）。
    pub build: BuildSummary,
    /// 加进 build 的候选词条文本。
    pub added_mod_texts: Vec<String>,
    /// **有变化**的输出字段差值（无变化字段不列；全空 = 该词条对关键输出无可见影响）。
    pub deltas: Vec<OutputDelta>,
}

/// 计算候选词条对一份 build 的边际贡献：基线 vs 基线+词条，逐字段差值。
///
/// 两次 [`calculate_with_data`] 只差 `extra_modifier_texts`；其余编排选项（敌人 /
/// 口径 / 角色基础注入）完全一致，确保差值纯由候选词条引起。
pub fn marginal_contribution(req: &MarginalRequest) -> Result<MarginalReport, CliError> {
    let build = parse_build_from_code(&req.build_code)?;
    let game_data = GameData::new(req.data_dir.clone());
    let build_data = BuildData::load(&game_data)?;

    let base_opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        extra_modifier_texts: Vec::new(),
        inject_character_base: true,
        enemy_level: req.enemy_level,
        enemy_tier: req.enemy_tier,
        mode_effective: req.mode_effective,
        ..Default::default()
    };
    let before = calculate_with_data(&build, &build_data, &base_opts)?;

    let with_opts = DataOrchestratorOptions {
        extra_modifier_texts: req.mod_texts.clone(),
        ..base_opts.clone()
    };
    let after = calculate_with_data(&build, &build_data, &with_opts)?;

    Ok(MarginalReport {
        build: build_summary(&build),
        added_mod_texts: req.mod_texts.clone(),
        deltas: build_deltas(&before, &after),
    })
}

/// 逐字段对比基线与加词条后的 [`OutputTable`]，仅保留**有变化**的字段（确定性序）。
fn build_deltas(before: &OutputTable, after: &OutputTable) -> Vec<OutputDelta> {
    let pairs: [(&str, f64, f64); 17] = [
        ("DPS (hit)", before.dps, after.dps),
        ("Total DoT DPS", before.total_dot_dps, after.total_dot_dps),
        ("Bleed DPS", before.bleed_dps, after.bleed_dps),
        ("Ignite DPS", before.ignite_dps, after.ignite_dps),
        ("Poison DPS", before.poison_dps, after.poison_dps),
        ("Avg Hit", before.total_hit_avg, after.total_hit_avg),
        ("Hit Chance", before.hit_chance, after.hit_chance),
        ("Crit Chance", before.crit_chance, after.crit_chance),
        (
            "Crit Multiplier",
            before.crit_multiplier,
            after.crit_multiplier,
        ),
        ("Life", before.life, after.life),
        ("Mana", before.mana, after.mana),
        ("Energy Shield", before.energy_shield, after.energy_shield),
        ("Armour", before.armour, after.armour),
        ("Evasion", before.evasion, after.evasion),
        ("Fire Res", before.fire_resistance, after.fire_resistance),
        ("Cold Res", before.cold_resistance, after.cold_resistance),
        (
            "Lightning Res",
            before.lightning_resistance,
            after.lightning_resistance,
        ),
    ];
    pairs
        .iter()
        .filter(|(_, b, a)| (a - b).abs() > 1e-6)
        .map(|(key, b, a)| {
            let delta = a - b;
            let pct = if *b != 0.0 {
                Some(delta / b * 100.0)
            } else {
                None
            };
            OutputDelta {
                key: (*key).to_string(),
                before: *b,
                after: *a,
                delta,
                pct,
            }
        })
        .collect()
}

/// 紧凑数值格式：|n|≥100 时不留小数，否则 2 位（DPS 等大数避免冗长尾数）。
fn fmt_num(n: f64) -> String {
    if n.abs() >= 100.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.2}")
    }
}

/// 把 [`MarginalReport`] 渲染为人类可读文本（接在 explain 输出之后）。
pub fn render_marginal(report: &MarginalReport) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(s, "\n═══ 边际贡献（加到 build 上前后对比）═══");
    let b = &report.build;
    let _ = writeln!(
        s,
        "build: Lv{} {}/{} · {} 天赋节点 · {} 装备 · {} 技能组",
        b.level,
        b.class_name,
        b.ascendancy_name,
        b.allocated_node_count,
        b.equipped_item_count,
        b.socket_group_count
    );
    let _ = writeln!(s, "加入词条: {}", report.added_mod_texts.join(" / "));
    if report.deltas.is_empty() {
        let _ = writeln!(
            s,
            "（该词条对当前 build 的关键输出无可见影响——可能条件未满足、不适用主技能，或词条未被解析支持）"
        );
        return s;
    }
    for d in &report.deltas {
        let pct = match d.pct {
            Some(p) => format!("{p:+.2}%"),
            None => "—".to_string(),
        };
        let _ = writeln!(
            s,
            "  {:<16} {} → {}   ({:+}, {})",
            d.key,
            fmt_num(d.before),
            fmt_num(d.after),
            fmt_num(d.delta),
            pct
        );
    }
    s
}

/// explain 解剖 + 边际贡献的合并报告（`explain-mod --build --json`）。
#[derive(Debug, Clone, Serialize)]
pub struct ExplainWithMarginal {
    /// 词条解剖。
    pub explain: ExplainModReport,
    /// 在 build 上的边际贡献。
    pub marginal: MarginalReport,
}

/// `explain-mod --build`（文本）：解剖 + 边际贡献拼接输出。
pub fn explain_mod_with_marginal_text(
    text: &str,
    req: &MarginalRequest,
) -> Result<String, CliError> {
    let data = GameData::new(req.data_dir.clone());
    let rules = ParseModRules::from_game_data(&data)?;
    let explain = explain_mod(text, Some(&rules))?;
    let marginal = marginal_contribution(req)?;
    let mut s = render_explain(&explain);
    s.push_str(&render_marginal(&marginal));
    Ok(s)
}

/// `explain-mod --build --json`：解剖 + 边际贡献的合并 JSON。
pub fn explain_mod_with_marginal_json(
    text: &str,
    req: &MarginalRequest,
) -> Result<String, CliError> {
    let data = GameData::new(req.data_dir.clone());
    let rules = ParseModRules::from_game_data(&data)?;
    let explain = explain_mod(text, Some(&rules))?;
    let marginal = marginal_contribution(req)?;
    Ok(serde_json::to_string_pretty(&ExplainWithMarginal {
        explain,
        marginal,
    })?)
}

#[cfg(test)]
mod explain_tests {
    use super::*;

    /// 无条件 / 无缩放的平凡词条：无 tags，值与桶正确。
    #[test]
    fn flat_modifier_has_no_tags() {
        let report = explain_mod("+50 to maximum Life", None).unwrap();
        assert_eq!(report.status, "Parsed");
        assert_eq!(report.mods.len(), 1);
        let m = &report.mods[0];
        assert_eq!(m.name, "MaximumLife");
        assert_eq!(m.mod_type, "BASE");
        assert_eq!(m.value, Some(50.0));
        assert!(m.tags.is_empty(), "平凡词条不应有 tag");
        assert!(m.flags.is_empty());
    }

    /// 带条件 + 伤害类型的词条：摊开 Condition 与 DamageType tag（parse-mod 会丢掉这些）。
    #[test]
    fn conditional_modifier_surfaces_condition_and_damage_type_tags() {
        let report = explain_mod("25% increased Fire Damage while on Full Life", None).unwrap();
        assert_eq!(report.status, "Parsed");
        let m = report
            .mods
            .iter()
            .find(|m| m.name == "FireDamage")
            .expect("应解析出 FireDamage modifier");
        assert_eq!(m.mod_type, "INC");
        assert!(
            m.tags
                .iter()
                .any(|t| t.kind == "Condition" && t.detail.contains("FullLife")),
            "应有 Condition(FullLife) tag，实际 tags = {:?}",
            m.tags
        );
        assert!(
            m.tags.iter().any(|t| t.kind == "DamageType"),
            "应有 DamageType tag"
        );
        // 人话总结把条件串进去。
        assert!(m.plain.contains("FullLife"));
    }

    /// 带「per N 资源」缩放的词条：摊开 Multiplier tag。
    #[test]
    fn per_stat_modifier_surfaces_multiplier_tag() {
        let report = explain_mod("1% increased Attack Damage per 10 Strength", None).unwrap();
        assert_eq!(report.status, "Parsed");
        let m = report
            .mods
            .iter()
            .find(|m| m.name == "AttackDamage")
            .expect("应解析出 AttackDamage modifier");
        assert!(
            m.tags
                .iter()
                .any(|t| t.kind == "Multiplier" && t.detail.contains("Strength")),
            "应有 Multiplier(Strength) tag，实际 tags = {:?}",
            m.tags
        );
    }

    /// flag 位分解：命名位被正确还原为可读名。
    #[test]
    fn mod_flag_names_decomposes_named_bits() {
        let names = mod_flag_names(ModFlags::ATTACK | ModFlags::MELEE | ModFlags::BOW);
        assert!(names.contains(&"Attack".to_string()));
        assert!(names.contains(&"Melee".to_string()));
        assert!(names.contains(&"Bow".to_string()));
        assert!(mod_flag_names(ModFlags::NONE).is_empty());
    }

    /// 文本渲染器冒烟：含原文、状态、tag 段。
    #[test]
    fn render_explain_smoke() {
        let report = explain_mod("25% increased Fire Damage while on Full Life", None).unwrap();
        let text = render_explain(&report);
        assert!(text.contains("词条原文:"));
        assert!(text.contains("Parsed"));
        assert!(text.contains("Condition"));
        assert!(text.contains("FullLife"));
    }
}

#[cfg(test)]
mod marginal_tests {
    use super::*;

    /// 真实 demo build code（与 calculate-build 集成测试同源）。
    const DEADEYE_CODE: &str = include_str!("../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

    fn deadeye_request(mod_texts: Vec<String>) -> MarginalRequest {
        MarginalRequest {
            build_code: DEADEYE_CODE.to_string(),
            data_dir: pobr_gamedata::current_data_dir(),
            enemy_level: 0,
            enemy_tier: EnemyTier::Pinnacle,
            mode_effective: true,
            mod_texts,
        }
    }

    /// 加 maximum Life 词条：Life 上升，且因 build 的 increased Life% 乘区，增量 ≥ 基础值。
    #[test]
    fn adding_life_raises_life_through_increase_multipliers() {
        let report =
            marginal_contribution(&deadeye_request(vec!["+500 to maximum Life".to_string()]))
                .unwrap();
        let life = report
            .deltas
            .iter()
            .find(|d| d.key == "Life")
            .expect("Life 应有变化");
        assert!(life.after > life.before, "Life 应上升");
        // 基础 +500 经 increased Life% 放大 → 实际增量 ≥ 500（证明走了真实聚合管线）。
        assert!(
            life.delta >= 500.0,
            "Life 增量应被 increased% 乘区放大，实际 {}",
            life.delta
        );
    }

    /// 加 increased Damage 词条：DPS 与 Avg Hit 上升（边际为正）。
    #[test]
    fn adding_increased_damage_raises_dps() {
        let report =
            marginal_contribution(&deadeye_request(vec!["40% increased Damage".to_string()]))
                .unwrap();
        let dps = report
            .deltas
            .iter()
            .find(|d| d.key == "DPS (hit)")
            .expect("DPS 应有变化");
        assert!(dps.delta > 0.0, "增伤词条应提升 DPS");
        assert!(dps.pct.unwrap() > 0.0);
    }

    /// 边际报告空 deltas 时，文本渲染给出"无可见影响"提示（不依赖解析 / build）。
    #[test]
    fn render_marginal_reports_no_impact_when_empty() {
        let report = MarginalReport {
            build: BuildSummary {
                level: 92,
                class_name: "Ranger".to_string(),
                ascendancy_name: "Deadeye".to_string(),
                allocated_node_count: 100,
                equipped_item_count: 9,
                socket_group_count: 6,
            },
            added_mod_texts: vec!["+5 to Fishing Line Strength".to_string()],
            deltas: Vec::new(),
        };
        let text = render_marginal(&report);
        assert!(text.contains("边际贡献"));
        assert!(text.contains("无可见影响"));
    }
}
