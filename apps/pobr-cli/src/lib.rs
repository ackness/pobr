//! The pobr-cli library layer: each subcommand is implemented as a pure
//! function (an input struct -> an output struct plus `Result`), for easy
//! unit testing and reuse. `main.rs` only handles IO glue (arg parsing,
//! reading a file / stdin, printing JSON).
//!
//! Subcommands:
//! - [`calculate`]: builds a [`CalculationSession`] from a base [`MinimalInput`]
//!   plus modifier text, and returns the key fields plus unsupported text after `perform_minimal`.
//! - [`parse_mod`]: the data-driven engine parses a single modifier line, returning a serializable parse report.
//! - [`parse_item`]: calls [`pobr_core::item_text::parse_item_text`] plus
//!   [`pobr_core::item::ingest_item_with_ctx`] to actually parse raw item
//!   text, outputting JSON (the parsed modifiers / sections / unsupported text).
//! - [`encode_code`] / [`decode_code`]: wrap PoB Build Code encoding/decoding.

use std::path::PathBuf;

use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, calculate_with_data, diagnose_tree_version,
    parse_build_from_code,
};
use pobr_core::calc::{CalculationSession, MinimalInput, MinimalOutput, OutputTable};
use pobr_core::item::ingest_item_with_ctx;
use pobr_core::item_text::{ItemTextError, parse_item_text};
use pobr_core::mod_parser::{CompiledParserRules, ParseCtx, ParseStatus, parse_mod_engine};
use pobr_core::{ActorRef, ModTag, ModValue, Modifier};
use pobr_data::item::EquipmentSlot;
use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::GameData;
use serde::Serialize;
use thiserror::Error;

/// The unified error type for the CLI library layer.
#[derive(Debug, Error)]
pub enum CliError {
    /// Modifier text couldn't be parsed (from `pobr_core::mod_parser`).
    #[error("modifier parse error: {0}")]
    ModParse(String),
    /// Build Code encoding/decoding failed.
    #[error("build code error: {0}")]
    BuildCode(#[from] pobr_build::BuildCodeError),
    /// JSON serialization failed.
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
    /// Structural parsing of raw item text failed (empty input / missing Rarity / missing base).
    #[error("item text parse error: {0}")]
    ItemText(#[from] ItemTextError),
    /// Build parsing / calc orchestration failed (from pobr-build).
    #[error("build error: {0}")]
    Build(#[from] pobr_build::BuildError),
    /// Game data failed to load (missing data dir / JSON deserialization failure).
    #[error("game data load error: {0}")]
    GameData(#[from] pobr_gamedata::LoadError),
    /// The feature isn't implemented yet (kept as a placeholder).
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

// calculate

/// Input for the `calculate` subcommand.
#[derive(Debug, Clone)]
pub struct CalculateRequest {
    /// Base attribute input (life / mana / resistances / accuracy / enemy evasion / hit / action rate).
    pub input: MinimalInput,
    /// The modifier text to apply (PoB-compatible English).
    pub modifier_texts: Vec<String>,
}

/// The key output fields from `calculate` (serializable to JSON).
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

/// The `calculate` result: output fields plus unsupported modifier text
/// (recognized text the parser refused to aggregate).
#[derive(Debug, Clone, Serialize)]
pub struct CalculateResult {
    pub output: CalculateOutput,
    pub unsupported: Vec<String>,
}

/// Runs the minimal calculation: builds a [`CalculationSession`] (injecting
/// the engine rules from the default data directory), applies the modifier
/// text, then calls `perform_minimal`.
///
/// The engine **never errors** on unrecognized text — it's collected into
/// `unsupported` without blocking the calculation; [`CliError::ModParse`] is
/// only returned when the default data directory is missing or rule
/// compilation fails.
pub fn calculate(req: &CalculateRequest) -> Result<CalculateResult, CliError> {
    let mut session = CalculationSession::new(req.input);
    // Inject the engine rules (the sole parser) — fail-fast if the default
    // data directory is missing, rather than silently treating every mod line as Unsupported.
    session.set_parser_rules(default_rules()?.rules.clone());
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

/// Renders a [`CalculateResult`] as a pretty-printed JSON string.
pub fn calculate_json(req: &CalculateRequest) -> Result<String, CliError> {
    let result = calculate(req)?;
    Ok(serde_json::to_string_pretty(&result)?)
}

// parse-mod

/// A serializable summary of a single modifier.
#[derive(Debug, Clone, Serialize)]
pub struct ModSummary {
    /// Stable stat name (e.g. `MaximumLife`).
    pub name: String,
    /// Aggregation type (`Base` / `Inc` / `More` / ...).
    pub mod_type: String,
    /// The numeric value (`None` for text-valued modifiers).
    pub value: Option<f64>,
    /// The raw source text.
    pub source: Option<String>,
}

/// The `parse-mod` parse report.
#[derive(Debug, Clone, Serialize)]
pub struct ParseModReport {
    /// `Parsed` or `Unsupported`.
    pub status: String,
    /// The list of parsed modifiers.
    pub mods: Vec<ModSummary>,
    /// The raw text that couldn't be recognized/classified (present only when `Unsupported`).
    pub unparsed: Option<String>,
}

/// The parser rule context, compiled once at startup and reused (the
/// data-driven engine, the sole parser).
///
/// Built from [`GameData`] as `overlay/mod_parser_rules.json` plus special
/// entries (three sources stitched together via [`RuleSet`]), compiled via
/// [`CompiledParserRules::compile_with_special`].
///
/// [`RuleSet`]: pobr_gamedata::ruleset::RuleSet
pub struct ParseModRules {
    rules: std::sync::Arc<CompiledParserRules>,
}

impl ParseModRules {
    /// Compiles the parser rules from game data (the six parse-rule tables plus the special channel).
    ///
    /// Raises [`CliError::ModParse`] if `overlay/mod_parser_rules.json` is
    /// missing (an old data pack) or compilation fails (an invalid pattern /
    /// a duplicate id) — with the legacy parser removed, there's no fallback
    /// path, so failing fast beats silently marking everything Unsupported.
    pub fn from_game_data(data: &GameData) -> Result<Self, CliError> {
        let doc = data.mod_parser_rules()?.ok_or_else(|| {
            CliError::ModParse("数据目录缺 overlay/mod_parser_rules.json（解析规则表）".into())
        })?;
        let special_entries = data.load_ruleset()?.special_mods.unwrap_or_default();
        let rules = CompiledParserRules::compile_with_special(&doc, &special_entries)
            .map_err(|e| CliError::ModParse(format!("parser 规则编译失败: {e:?}")))?;
        Ok(Self {
            rules: std::sync::Arc::new(rules),
        })
    }

    /// The engine's parse context (consumed by [`ingest_item_with_ctx`], etc).
    fn ctx(&self) -> ParseCtx<'_> {
        ParseCtx::with_engine(&self.rules)
    }
}

/// The default rules, cached in-process (from the repo data directory `pobr_gamedata::current_data_dir()`).
///
/// Used by entry points with no `data_dir` parameter ([`parse_mod`] /
/// [`calculate`] / [`parse_item`], etc); a load/compile failure is cached as
/// an error and returned on every call (fail-fast, never silent).
fn default_rules() -> Result<&'static ParseModRules, CliError> {
    use std::sync::LazyLock;
    static RULES: LazyLock<Result<ParseModRules, String>> = LazyLock::new(|| {
        let data = GameData::new(pobr_gamedata::current_data_dir());
        ParseModRules::from_game_data(&data).map_err(|e| e.to_string())
    });
    RULES
        .as_ref()
        .map_err(|e| CliError::ModParse(format!("默认解析规则加载失败：{e}")))
}

/// Parses a single modifier text (using the default data directory's rules).
///
/// The engine **never errors** on unrecognized text — it returns
/// `status == "Unsupported"` with the raw text in `unparsed`.
/// [`CliError::ModParse`] is only returned when the data directory is
/// missing or rule compilation fails.
pub fn parse_mod(text: &str) -> Result<ParseModReport, CliError> {
    parse_mod_with_data(text, default_rules()?)
}

/// Parses a single modifier text (explicit rules, the production path).
pub fn parse_mod_with_data(text: &str, rules: &ParseModRules) -> Result<ParseModReport, CliError> {
    let outcome = parse_mod_engine(text, &rules.rules);

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
                // Text/nested payloads have no scalar value (nested mods are
                // forwarded by the orchestration layer, not expanded in this summary).
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

/// Renders a [`ParseModReport`] as a pretty-printed JSON string.
///
/// `data_dir` points at a version data directory; parser rules are compiled from it once.
pub fn parse_mod_json(text: &str, data_dir: &std::path::Path) -> Result<String, CliError> {
    let data = GameData::new(data_dir);
    let rules = ParseModRules::from_game_data(&data)?;
    let report = parse_mod_with_data(text, &rules)?;
    Ok(serde_json::to_string_pretty(&report)?)
}

// explain-mod (mod anatomy)
//
// Difference from parse-mod: parse-mod only gives name/type/value/source, and
// **drops flags and tags** — but tags (Condition / Multiplier / PerStat /
// SkillTypes / SlotName, ...) are exactly what makes a mod line "under what
// circumstances does it apply, and how does it scale" (the layer behind
// PoB2's "everything is a tagged Mod"). explain-mod lays these out and
// explains them in plain language, so "how does this mod line actually
// work" is visible right on the command line.

/// A plain-language explanation of a single tag.
#[derive(Debug, Clone, Serialize)]
pub struct TagExplain {
    /// The tag category (`Condition` / `Multiplier` / `PerStat` / ...).
    pub kind: String,
    /// A human-readable explanation of its semantics.
    pub detail: String,
}

/// The "anatomy" of a single modifier — everything [`ModSummary`] has, plus
/// flags / keyword_flags / tags (a mod line's conditions and scaling), and a
/// one-line plain-language summary.
#[derive(Debug, Clone, Serialize)]
pub struct ModAnatomy {
    /// Stable stat name (e.g. `FireDamage`).
    pub name: String,
    /// Aggregation type (`BASE` / `INC` / `MORE` / `FLAG` / `OVERRIDE` / `LIST`).
    pub mod_type: String,
    /// The numeric value (`None` for text / nested payloads).
    pub value: Option<f64>,
    /// The text payload (`ModValue::Text` / a nested-mod hint; `None` for numeric values).
    pub text_value: Option<String>,
    /// The matched [`ModFlags`] names (e.g. `Attack` / `Melee` / `Bow`); empty = no flag constraint.
    pub flags: Vec<String>,
    /// The matched [`KeywordFlags`] names (e.g. `Aura` / `Curse` / `Poison`); empty = none.
    pub keyword_flags: Vec<String>,
    /// The tag explanations (the mod line's conditions and scaling); empty = always applies unconditionally.
    pub tags: Vec<TagExplain>,
    /// The raw source text.
    pub source: Option<String>,
    /// A one-line plain-language summary.
    pub plain: String,
}

/// The `explain-mod` anatomy report.
#[derive(Debug, Clone, Serialize)]
pub struct ExplainModReport {
    /// The raw input modifier text.
    pub input: String,
    /// `Parsed` or `Unsupported`.
    pub status: String,
    /// The anatomy of each parsed modifier.
    pub mods: Vec<ModAnatomy>,
    /// The raw text that couldn't be recognized/classified (present only when `Unsupported`).
    pub unparsed: Option<String>,
}

/// Dissects a single modifier text: after parsing, lays out every field of
/// each [`Modifier`] (including flags / tags), with a plain-language
/// explanation. `rules` is the same as in [`parse_mod_with_data`].
pub fn explain_mod(text: &str, rules: &ParseModRules) -> Result<ExplainModReport, CliError> {
    let outcome = parse_mod_engine(text, &rules.rules);

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

/// Breaks a [`Modifier`] apart into a serializable [`ModAnatomy`].
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

/// [`ModFlags`] bits -> name list (individual bits only, no masks).
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

/// [`KeywordFlags`] bits -> name list (with the `MatchAll` marker).
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

/// The suffix for a cross-actor value lookup (empty string for the same actor).
fn actor_label(actor: Option<ActorRef>) -> &'static str {
    match actor {
        None => "",
        Some(ActorRef::Player) => "玩家侧 ",
        Some(ActorRef::Parent) => "父 actor 侧 ",
        Some(ActorRef::Minion) => "召唤物侧 ",
    }
}

/// A single [`ModTag`] -> a plain-language explanation.
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
        ModTag::SkillTypesNeg(st) => TagExplain {
            kind: "SkillTypesNeg".to_string(),
            detail: format!("排除技能类型 {st:?}"),
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

/// A one-line plain-language summary: strings the aggregation bucket, value, and conditions into natural language.
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

/// Renders an [`ExplainModReport`] as human-readable text (the CLI's default output).
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

/// The `explain-mod` JSON output (`--json`): compiles rules from game data once, then dissects.
pub fn explain_mod_json(text: &str, data_dir: &std::path::Path) -> Result<String, CliError> {
    let data = GameData::new(data_dir);
    let rules = ParseModRules::from_game_data(&data)?;
    let report = explain_mod(text, &rules)?;
    Ok(serde_json::to_string_pretty(&report)?)
}

/// The `explain-mod` human-readable text output (the CLI default): compiles rules from game data once, then dissects.
pub fn explain_mod_text(text: &str, data_dir: &std::path::Path) -> Result<String, CliError> {
    let data = GameData::new(data_dir);
    let rules = ParseModRules::from_game_data(&data)?;
    let report = explain_mod(text, &rules)?;
    Ok(render_explain(&report))
}

// parse-item

/// Input for the `parse-item` subcommand.
#[derive(Debug, Clone)]
pub struct ParseItemRequest {
    /// The complete raw item text (PoB-style English export).
    pub text: String,
}

/// A serializable summary of one parsed modifier (specific to `parse-item`).
#[derive(Debug, Clone, Serialize)]
pub struct ParsedModEntry {
    /// The section: `implicit` / `explicit` / `enchant`.
    pub section: String,
    /// Stable ModName.
    pub name: String,
    /// Aggregation type (`Base` / `Inc` / `More` / ...).
    pub mod_type: String,
    /// The numeric value (`None` for text-valued modifiers).
    pub value: Option<f64>,
    /// The attribution source ID (equipment slot plus section suffix).
    pub source_id: String,
}

/// The `parse-item` parse report: base metadata plus per-section modifiers plus unsupported mod lines.
#[derive(Debug, Clone, Serialize)]
pub struct ParseItemReport {
    /// The item's base name.
    pub base: String,
    /// Rarity (`Normal` / `Magic` / `Rare` / `Unique`).
    pub rarity: String,
    /// Quality (0-20).
    pub quality: u8,
    /// The parsed modifiers (including implicit / explicit / enchant / quality).
    pub modifiers: Vec<ParsedModEntry>,
    /// Mod line text that `mod_parser` couldn't recognize (kept verbatim, never errors).
    pub unsupported: Vec<String>,
}

/// Parses raw item text, outputting a structured [`ParseItemReport`].
///
/// Internally calls:
/// 1. [`pobr_core::item_text::parse_item_text`] — segments the text
///    (rarity / sections / annotation stripping) into an `Item`;
/// 2. [`pobr_core::item::ingest_item_with_ctx`] — turns the `Item`'s mod
///    lines into an attributed `Modifier` list (using the default data
///    directory's engine rules).
///
/// The slot defaults to [`EquipmentSlot::Ring1`] (CLI parse-item isn't tied
/// to a specific slot; the attribution ID is only for debug display and
/// doesn't affect damage calculation).
///
/// Structural errors (empty text / missing Rarity / missing base) return
/// [`CliError::ItemText`]; unsupported mod lines are collected into
/// `unsupported` without erroring.
pub fn parse_item(req: &ParseItemRequest) -> Result<ParseItemReport, CliError> {
    let item = parse_item_text(&req.text)?;

    // CLI parse-item isn't tied to a specific equipment slot; Ring1 is used as a placeholder slot (the attribution ID is for debugging).
    let slot = EquipmentSlot::Ring1;
    let ingest = ingest_item_with_ctx(slot, &item, default_rules()?.ctx())
        .map_err(|e| CliError::ModParse(e.to_string()))?;

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
                    // Text/nested payloads have no scalar value (nested mods
                    // are forwarded by the orchestration layer, not expanded in this summary).
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

/// Renders a [`ParseItemReport`] as a pretty-printed JSON string.
pub fn parse_item_json(req: &ParseItemRequest) -> Result<String, CliError> {
    let report = parse_item(req)?;
    Ok(serde_json::to_string_pretty(&report)?)
}

// decode-code / encode-code

/// Decodes a PoB Build Code -> XML.
pub fn decode_code(code: &str) -> Result<String, CliError> {
    Ok(pobr_build::decode_pob_code(code)?)
}

/// Encodes XML -> a PoB Build Code (URL-safe base64 of zlib-compressed XML).
pub fn encode_code(xml: &str) -> Result<String, CliError> {
    Ok(pobr_build::encode_pob_code(xml)?)
}

// calculate-build (PoB Build Code -> a full Build -> end-to-end attributed calculation)

/// Input for the `calculate-build` subcommand.
#[derive(Debug, Clone)]
pub struct CalculateBuildRequest {
    /// PoB Build Code (URL-safe Base64 + zlib).
    pub code: String,
    /// The game data version directory (containing ingested JSON, e.g. `data/4.5.0.3.4`).
    pub data_dir: PathBuf,
    /// Enemy level (`0` = follows the character level).
    pub enemy_level: u32,
    /// Enemy tier (normal / Boss / Pinnacle / Uber).
    pub enemy_tier: EnemyTier,
    /// Effective-DPS basis (`true` -> accounts for hit chance / enemy damage reduction; `false` -> panel basis).
    pub mode_effective: bool,
}

/// A summary of the parsed Build (character identity plus per-source counts).
#[derive(Debug, Clone, Serialize)]
pub struct BuildSummary {
    pub level: u32,
    pub class_name: String,
    pub ascendancy_name: String,
    pub allocated_node_count: usize,
    pub equipped_item_count: usize,
    pub socket_group_count: usize,
}

/// The key output fields from a `calculate-build` result.
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
    /// Damage-over-time (ailment) DPS: bleed / ignite / poison (PoB2 BleedDPS/IgniteDPS/PoisonDPS).
    pub bleed_dps: f64,
    pub ignite_dps: f64,
    pub poison_dps: f64,
    /// The total of all DoT (PoB2 TotalDotDPS).
    pub total_dot_dps: f64,
    /// Active ailment stack counts (diagnostic: bleed/ignite/poison).
    pub bleed_active_stacks: f64,
    pub ignite_active_stacks: f64,
    pub poison_active_stacks: f64,
    /// Ailment stack caps (diagnostic: bleed/ignite/poison; 1 = can't stack).
    pub bleed_max_stacks: f64,
    pub ignite_max_stacks: f64,
    pub poison_max_stacks: f64,
    /// The main skill's action rate (per second, derived from the gem's per-level cast/attack time).
    pub action_rate: f64,
    /// The main skill's cooldown (seconds, derived from its per-level cooldown).
    pub cooldown: f64,
    /// The main skill's mana cost (derived from its per-level cost).
    pub mana_cost: f64,
}

/// Passive-tree version reconciliation diagnostics (gap B): the build's
/// recorded `treeVersion` plus allocated nodes that **aren't in the loaded
/// tree** (calc silently skips their contribution — the actual symptom of a
/// tree-version mismatch). The CLI warns to stderr when `unknown_node_count > 0`.
#[derive(Debug, Clone, Serialize)]
pub struct TreeVersionDiag {
    pub build_tree_version: Option<String>,
    pub unknown_node_count: usize,
    pub unknown_nodes: Vec<u32>,
}

/// The `calculate-build` report: Build summary plus calc output plus passive-tree version reconciliation diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct CalculateBuildReport {
    pub build: BuildSummary,
    pub output: CalculateBuildOutput,
    pub tree_version: TreeVersionDiag,
}

/// Computes end-to-end from a PoB Build Code: decode -> [`parse_build_from_code`]
/// -> [`BuildData::load`] -> [`calculate_with_data`], returning a Build summary plus the key output fields.
///
/// This is the CLI entry point for build-layer integration: it drives every
/// source (equipment / passive tree / skill gems / character base / enemy)
/// into the REAL calc engine, producing scalars that can be compared
/// directly against PoB2's panel.
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

/// Renders a [`CalculateBuildReport`] as a pretty-printed JSON string.
pub fn calculate_build_json(req: &CalculateBuildRequest) -> Result<String, CliError> {
    let report = calculate_build(req)?;
    Ok(serde_json::to_string_pretty(&report)?)
}

/// Parsed [`Build`] -> [`BuildSummary`] (shared by calculate_build and marginal).
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

// Marginal contribution (explain-mod --build)
//
// "What happens if I add this mod line to my current build?" The candidate
// mod text is injected into the player-side pipeline as
// extra_modifier_texts, the calculation is rerun, and every field is
// compared against the baseline — reusing calculate_with_data's existing
// mechanism (extra_modifier_texts flows into the player ModDb via
// session.add_modifier_texts).

/// The marginal change in a single output field (before vs. after adding the candidate mod line).
#[derive(Debug, Clone, Serialize)]
pub struct OutputDelta {
    /// The output field's readable name (e.g. `DPS (hit)` / `Life`).
    pub key: String,
    /// The value before adding the mod line.
    pub before: f64,
    /// The value after adding the mod line.
    pub after: f64,
    /// The difference (after - before).
    pub delta: f64,
    /// The relative change as a percentage (`None` if before == 0).
    pub pct: Option<f64>,
}

/// The marginal-contribution request for `explain-mod --build`.
#[derive(Debug, Clone)]
pub struct MarginalRequest {
    /// PoB Build Code (URL-safe Base64 + zlib).
    pub build_code: String,
    /// The game data version directory.
    pub data_dir: PathBuf,
    /// Enemy level (`0` = follows the character level).
    pub enemy_level: u32,
    /// Enemy tier.
    pub enemy_tier: EnemyTier,
    /// Effective-DPS basis (`true`) / panel basis (`false`).
    pub mode_effective: bool,
    /// The candidate mod text to add to the build (usually the one being explained).
    pub mod_texts: Vec<String>,
}

/// The marginal-contribution report: build summary plus the added mod lines
/// plus the output fields that changed.
#[derive(Debug, Clone, Serialize)]
pub struct MarginalReport {
    /// The build summary (character identity plus per-source counts).
    pub build: BuildSummary,
    /// The candidate mod text added to the build.
    pub added_mod_texts: Vec<String>,
    /// The output-field deltas for fields that **changed** (unchanged fields
    /// are omitted; an empty list means the mod line has no visible impact
    /// on the key outputs).
    pub deltas: Vec<OutputDelta>,
}

/// Computes a candidate mod line's marginal contribution to a build:
/// baseline vs. baseline+mod, diffed field by field.
///
/// The two [`calculate_with_data`] calls only differ in
/// `extra_modifier_texts`; every other orchestration option (enemy /
/// basis / character-base injection) is identical, so the delta is caused purely by the candidate mod line.
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

/// Compares the baseline and post-mod [`OutputTable`] field by field,
/// keeping only fields that **changed** (in a deterministic order).
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

/// Compact numeric formatting: no decimal places when |n|>=100, otherwise 2
/// (avoids a long decimal tail on large numbers like DPS).
fn fmt_num(n: f64) -> String {
    if n.abs() >= 100.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.2}")
    }
}

/// Renders a [`MarginalReport`] as human-readable text (appended after the explain output).
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

/// The combined report of mod-line anatomy plus marginal contribution (`explain-mod --build --json`).
#[derive(Debug, Clone, Serialize)]
pub struct ExplainWithMarginal {
    /// The mod-line anatomy.
    pub explain: ExplainModReport,
    /// The marginal contribution on the build.
    pub marginal: MarginalReport,
}

/// `explain-mod --build` (text): the anatomy plus marginal-contribution output, concatenated.
pub fn explain_mod_with_marginal_text(
    text: &str,
    req: &MarginalRequest,
) -> Result<String, CliError> {
    let data = GameData::new(req.data_dir.clone());
    let rules = ParseModRules::from_game_data(&data)?;
    let explain = explain_mod(text, &rules)?;
    let marginal = marginal_contribution(req)?;
    let mut s = render_explain(&explain);
    s.push_str(&render_marginal(&marginal));
    Ok(s)
}

/// `explain-mod --build --json`: the anatomy plus marginal contribution, combined into one JSON object.
pub fn explain_mod_with_marginal_json(
    text: &str,
    req: &MarginalRequest,
) -> Result<String, CliError> {
    let data = GameData::new(req.data_dir.clone());
    let rules = ParseModRules::from_game_data(&data)?;
    let explain = explain_mod(text, &rules)?;
    let marginal = marginal_contribution(req)?;
    Ok(serde_json::to_string_pretty(&ExplainWithMarginal {
        explain,
        marginal,
    })?)
}

#[cfg(test)]
mod explain_tests {
    use super::*;

    /// An unconditional / unscaled flat mod line: no tags, correct value and bucket.
    #[test]
    fn flat_modifier_has_no_tags() {
        let report = explain_mod("+50 to maximum Life", default_rules().unwrap()).unwrap();
        assert_eq!(report.status, "Parsed");
        assert_eq!(report.mods.len(), 1);
        let m = &report.mods[0];
        assert_eq!(m.name, "MaximumLife");
        assert_eq!(m.mod_type, "BASE");
        assert_eq!(m.value, Some(50.0));
        assert!(m.tags.is_empty(), "平凡词条不应有 tag");
        assert!(m.flags.is_empty());
    }

    /// A mod line with a condition plus a damage type: lays out the
    /// Condition and DamageType tags (parse-mod drops these).
    #[test]
    fn conditional_modifier_surfaces_condition_and_damage_type_tags() {
        let report = explain_mod(
            "25% increased Fire Damage while on Full Life",
            default_rules().unwrap(),
        )
        .unwrap();
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
        // The plain-language summary strings the condition in.
        assert!(m.plain.contains("FullLife"));
    }

    /// A mod line scaled by "per N resource": lays out the Multiplier tag.
    #[test]
    fn per_stat_modifier_surfaces_multiplier_tag() {
        let report = explain_mod(
            "1% increased Attack Damage per 10 Strength",
            default_rules().unwrap(),
        )
        .unwrap();
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

    /// Flag bit decomposition: named bits are correctly translated back to readable names.
    #[test]
    fn mod_flag_names_decomposes_named_bits() {
        let names = mod_flag_names(ModFlags::ATTACK | ModFlags::MELEE | ModFlags::BOW);
        assert!(names.contains(&"Attack".to_string()));
        assert!(names.contains(&"Melee".to_string()));
        assert!(names.contains(&"Bow".to_string()));
        assert!(mod_flag_names(ModFlags::NONE).is_empty());
    }

    /// Smoke test for the text renderer: contains the raw text, status, and tag sections.
    #[test]
    fn render_explain_smoke() {
        let report = explain_mod(
            "25% increased Fire Damage while on Full Life",
            default_rules().unwrap(),
        )
        .unwrap();
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

    /// A real demo build code (shared with the calculate-build integration test).
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

    /// Adding a maximum Life mod line: Life rises, and thanks to the
    /// build's increased Life% multiplier, the delta is >= the base value.
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
        // The base +500 gets amplified by the increased Life% multiplier ->
        // the actual delta is >= 500 (proving it went through the real aggregation pipeline).
        assert!(
            life.delta >= 500.0,
            "Life 增量应被 increased% 乘区放大，实际 {}",
            life.delta
        );
    }

    /// Adding an increased Damage mod line: DPS and Avg Hit rise (a positive marginal contribution).
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

    /// When the marginal report's deltas are empty, the text renderer emits
    /// a "no visible impact" note (independent of parsing / the build).
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
