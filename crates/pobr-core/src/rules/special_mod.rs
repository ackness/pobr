//! special 词条模板解释器（M5b 蓝图 §2.2）。
//!
//! 输入 = `overlay/special_mods.json` + `generated/special_derived.json` 拼接后的
//! [`SpecialTemplateDef`] 列表（schema 见 [`pobr_data::catalog::parser_rules`]）。
//! 载入期编译（[`RegexSet`] 预筛 + 逐条 [`Regex`]，整行锚定 + 输入小写规范），
//! 运行期对单行（已小写规范化）做整行匹配并实例化为 [`Modifier`]。
//!
//! **求值器单点（00-index 裁决 §4-1）**：`$n` 数值占位 / 五算子 / 受限谓词的求值
//! 复用 [`crate::rules::value_expr`]（config / special / parser 三处同一套受限语言，
//! 禁三套方言）。本模块只负责：① 把 [`ValueOpDef`] 算子链编译为
//! `value_expr::ValueExpr` 树后调 `value_expr::eval`；② enums 闭集查表（DSL 微扩展，
//! 00-index §4.2-3 已批准——每个输出都是表内显式字面量，非字符串拼接）。
//!
//! **DSL 硬边界**（20-target-architecture §5）：数值捕获 `(\d+(?:\.\d+)?)`、词类捕获
//! 显式闭集、禁 `(.+)` 开放捕获（开放捕获条目走 `handler_id`）。本解释器不强制
//! pattern 形态（编译只校验 regex 合法性），形态合规由策展 + 闸门测试（C-4）守。
//!
//! **保守门控**：本批次条目携带的若干 PoB2 原生 tag 形态（`ItemCondition` /
//! `GlobalEffect` / 复杂 LIST 载荷等）尚无 pobr `ModTag` 落点——
//! 这类 tag 在实例化时被**跳过**（产出 mod 但不挂该 tag），对应条目保持
//! `verified:false`，由 differential（Track D）与 parity 报表把关。能映射的清单见
//! [`compile_tag`]。

use std::collections::BTreeMap;

use pobr_data::catalog::parser_rules::{
    SpecialTemplateDef, TemplateNameDef, TemplateScalarDef, TemplateTagDef, TemplateValueDef,
    ValueExprDef, ValueOpDef,
};
use pobr_data::catalog::value_expr::ValueExpr;
use pobr_data::constants::DamageType;
use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};
use pobr_data::skill::SkillTypes;
use regex::{Regex, RegexSet};

use crate::modifier::{ActorRef, ModTag, ModValue, Modifier};
use crate::rules::registry::{HandlerCtx, HandlerRegistry};
use crate::rules::value_expr::eval;

/// 编译期错误（载入期 fail-fast，不静默）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecialCompileError {
    /// pattern 非法 regex。
    BadPattern {
        /// 条目 id。
        entry_id: String,
        /// 原始 pattern。
        pattern: String,
        /// regex crate 报错。
        reason: String,
    },
    /// `id` 重复（拼接两表后唯一性）。
    DuplicateId {
        /// 冲突 id。
        entry_id: String,
    },
    /// enums 引用越界（`{"enum": n}` 的 n 在条目 `enums` 表中无键）。
    EnumRefMissing {
        /// 条目 id。
        entry_id: String,
        /// 引用的捕获序号。
        capture_index: u32,
    },
    /// 未知 mod_type 字面量。
    BadModType {
        /// 条目 id。
        entry_id: String,
        /// 原始字面量。
        literal: String,
    },
    /// 模板 mods 与 handler_id 同时存在（互斥）。
    ModsAndHandler {
        /// 条目 id。
        entry_id: String,
    },
}

impl std::fmt::Display for SpecialCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadPattern {
                entry_id,
                pattern,
                reason,
            } => write!(
                f,
                "special `{entry_id}` pattern 非法 regex `{pattern}`：{reason}"
            ),
            Self::DuplicateId { entry_id } => write!(f, "special id 重复：`{entry_id}`"),
            Self::EnumRefMissing {
                entry_id,
                capture_index,
            } => write!(
                f,
                "special `{entry_id}` enums 引用越界：${capture_index} 无映射表"
            ),
            Self::BadModType { entry_id, literal } => {
                write!(f, "special `{entry_id}` 未知 mod_type `{literal}`")
            }
            Self::ModsAndHandler { entry_id } => {
                write!(f, "special `{entry_id}` mods 与 handler_id 互斥")
            }
        }
    }
}

impl std::error::Error for SpecialCompileError {}

/// 编译后的单条 special 条目。
#[derive(Debug)]
struct CompiledEntry {
    id: String,
    regex: Regex,
    verified: bool,
    /// 实例化为模板 mods（与 `handler_id` 互斥）。
    template: Option<CompiledTemplate>,
    /// handler 路由（与 template 互斥）。
    handler_id: Option<String>,
    /// handler 实参（`"$n"` 形）。
    handler_args: Vec<String>,
    /// enums 闭集（捕获序号 → 词 → 完整字面量）。
    enums: BTreeMap<u32, BTreeMap<String, String>>,
}

/// 编译后的模板（mod 列表 + 已解析的 mod_type）。
#[derive(Debug)]
struct CompiledTemplate {
    mods: Vec<CompiledModTemplate>,
}

#[derive(Debug)]
struct CompiledModTemplate {
    name: TemplateNameDef,
    mod_type: ModType,
    value: TemplateValueDef,
    flags: ModFlags,
    keyword_flags: KeywordFlags,
    /// 已映射的 tag（无法映射的 tag 在编译期丢弃，见 `compile_tag`）。
    tags: Vec<ModTag>,
    #[allow(dead_code)]
    target: Option<ActorRef>,
}

/// 一次 special 命中（[`SpecialModRules::try_match`] 产出）。
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialMatch {
    /// 命中条目稳定 id。
    pub entry_id: String,
    /// 已实例化的 modifier（已带 source 词条原文）。
    pub mods: Vec<Modifier>,
    /// 透传 parity 报表（`verified:false` 单列）。
    pub verified: bool,
    /// handler_id 未注册时记录（命中但产空 mods + 报表用，不 panic）。
    pub unregistered_handler: Option<String>,
}

/// 编译后的 special 规则集（载入期编译，运行期只读）。
#[derive(Debug)]
pub struct SpecialModRules {
    set: RegexSet,
    entries: Vec<CompiledEntry>,
}

impl SpecialModRules {
    /// 载入期编译。pattern 非法 / id 重复 / enums 越界 / mod_type 未知 →
    /// `Err`（fail fast）。
    pub fn compile(
        defs: &[SpecialTemplateDef],
        _registry: &HandlerRegistry,
    ) -> Result<Self, SpecialCompileError> {
        let mut seen_ids = std::collections::BTreeSet::new();
        let mut entries = Vec::with_capacity(defs.len());
        let mut patterns = Vec::with_capacity(defs.len());

        for def in defs {
            if !seen_ids.insert(def.id.clone()) {
                return Err(SpecialCompileError::DuplicateId {
                    entry_id: def.id.clone(),
                });
            }
            if !def.mods.is_empty() && def.handler_id.is_some() {
                return Err(SpecialCompileError::ModsAndHandler {
                    entry_id: def.id.clone(),
                });
            }

            // 整行锚定 + 输入小写规范（pattern 内部字面量已是小写，参照
            // vendor :6155-6158）。pattern 已含 `^`/`$` 时不重复包。
            let anchored = anchor_pattern(&def.pattern);
            let regex = Regex::new(&anchored).map_err(|e| SpecialCompileError::BadPattern {
                entry_id: def.id.clone(),
                pattern: def.pattern.clone(),
                reason: e.to_string(),
            })?;
            patterns.push(anchored);

            // enums 表（键转 u32）。
            let mut enums = BTreeMap::new();
            for (key, table) in &def.enums {
                if let Ok(idx) = key.parse::<u32>() {
                    enums.insert(idx, table.clone());
                }
            }

            let template = if def.mods.is_empty() {
                None
            } else {
                Some(compile_template(def, &enums)?)
            };

            entries.push(CompiledEntry {
                id: def.id.clone(),
                regex,
                verified: def.verified,
                template,
                handler_id: def.handler_id.clone(),
                handler_args: def.handler_args.clone(),
                enums,
            });
        }

        let set = RegexSet::new(&patterns).map_err(|e| SpecialCompileError::BadPattern {
            entry_id: "<regexset>".into(),
            pattern: "<all>".into(),
            reason: e.to_string(),
        })?;

        Ok(Self { set, entries })
    }

    /// 空规则集（无条目；`try_match` 恒 `None`）——「未加载数据」分支。
    pub fn empty() -> Self {
        Self {
            set: RegexSet::empty(),
            entries: Vec::new(),
        }
    }

    /// 条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 对单行（已小写规范化）做整行匹配。命中且可实例化 → `Some`。
    ///
    /// 多条命中时取**首个**（条目顺序 = 数据文件序）——special 条目设计为整行
    /// 互斥，重叠属策展问题（闸门测试 C-4 校验 pattern 唯一）。
    pub fn try_match(&self, line: &str, registry: &HandlerRegistry) -> Option<SpecialMatch> {
        let matches = self.set.matches(line);
        if !matches.matched_any() {
            return None;
        }
        for idx in matches.iter() {
            let entry = &self.entries[idx];
            // RegexSet 预筛后用单条 Regex 取捕获组（RegexSet 不产捕获）。
            let Some(caps) = entry.regex.captures(line) else {
                continue;
            };
            let captures: Vec<String> = caps
                .iter()
                .skip(1)
                .map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
                .collect();

            // handler 路径。
            if let Some(handler_id) = &entry.handler_id {
                let Some(handler) = registry.get(handler_id) else {
                    return Some(SpecialMatch {
                        entry_id: entry.id.clone(),
                        mods: Vec::new(),
                        verified: entry.verified,
                        unregistered_handler: Some(handler_id.clone()),
                    });
                };
                let nums: Vec<f64> = entry
                    .handler_args
                    .iter()
                    .map(|arg| resolve_capture_number(arg, &captures))
                    .collect();
                let outcome = handler(&HandlerCtx::with_inputs_and_captures(&nums, &captures));
                let mut mods = outcome.player_mods;
                for m in &mut mods {
                    if m.source.is_none() {
                        m.source = Some(line.to_string());
                    }
                }
                return Some(SpecialMatch {
                    entry_id: entry.id.clone(),
                    mods,
                    verified: entry.verified,
                    unregistered_handler: None,
                });
            }

            // 模板路径。
            let Some(template) = &entry.template else {
                // 纯识别条目（无 mods 无 handler）：命中但不产 mod。
                return Some(SpecialMatch {
                    entry_id: entry.id.clone(),
                    mods: Vec::new(),
                    verified: entry.verified,
                    unregistered_handler: None,
                });
            };
            let mods = instantiate_template(template, &captures, &entry.enums, line);
            return Some(SpecialMatch {
                entry_id: entry.id.clone(),
                mods,
                verified: entry.verified,
                unregistered_handler: None,
            });
        }
        None
    }
}

/// 整行锚定（pattern 已含 `^`/`$` 时不重复包）。
fn anchor_pattern(pattern: &str) -> String {
    let head = if pattern.starts_with('^') { "" } else { "^" };
    let tail = if pattern.ends_with('$') { "" } else { "$" };
    format!("{head}{pattern}{tail}")
}

fn parse_mod_type(literal: &str) -> Option<ModType> {
    match literal {
        "BASE" => Some(ModType::Base),
        "INC" => Some(ModType::Inc),
        "MORE" => Some(ModType::More),
        "FLAG" => Some(ModType::Flag),
        "OVERRIDE" => Some(ModType::Override),
        "LIST" => Some(ModType::List),
        _ => None,
    }
}

fn compile_template(
    def: &SpecialTemplateDef,
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
) -> Result<CompiledTemplate, SpecialCompileError> {
    let mut mods = Vec::with_capacity(def.mods.len());
    for m in &def.mods {
        let mod_type =
            parse_mod_type(&m.mod_type).ok_or_else(|| SpecialCompileError::BadModType {
                entry_id: def.id.clone(),
                literal: m.mod_type.clone(),
            })?;
        // enums 引用越界校验（name）。
        if let TemplateNameDef::Enum { capture_index } = &m.name
            && !enums.contains_key(capture_index)
        {
            return Err(SpecialCompileError::EnumRefMissing {
                entry_id: def.id.clone(),
                capture_index: *capture_index,
            });
        }
        // 嵌套 mod 载荷的 fail-fast 校验（实例化期按需再编译，见
        // `instantiate_mod_def`）。
        validate_nested_value(&def.id, &m.value, enums)?;
        let flags = compile_flags(&m.flags);
        let keyword_flags = compile_keyword_flags(&m.keyword_flags);
        let tags = m.tags.iter().filter_map(compile_tag).collect();
        let target = m.target.as_deref().and_then(parse_target);
        mods.push(CompiledModTemplate {
            name: m.name.clone(),
            mod_type,
            value: m.value.clone(),
            flags,
            keyword_flags,
            tags,
            target,
        });
    }
    Ok(CompiledTemplate { mods })
}

/// `TemplateValueDef::Nested` 编译期校验：内层 mod_type 已知、enums 引用不
/// 越界（递归）。非嵌套值形态直接通过。
fn validate_nested_value(
    entry_id: &str,
    value: &TemplateValueDef,
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
) -> Result<(), SpecialCompileError> {
    let TemplateValueDef::Nested { mods } = value else {
        return Ok(());
    };
    for m in mods {
        parse_mod_type(&m.mod_type).ok_or_else(|| SpecialCompileError::BadModType {
            entry_id: entry_id.to_string(),
            literal: m.mod_type.clone(),
        })?;
        if let TemplateNameDef::Enum { capture_index } = &m.name
            && !enums.contains_key(capture_index)
        {
            return Err(SpecialCompileError::EnumRefMissing {
                entry_id: entry_id.to_string(),
                capture_index: *capture_index,
            });
        }
        validate_nested_value(entry_id, &m.value, enums)?;
    }
    Ok(())
}

/// ModFlags 名 → 位（伤害模式 / 武器类型公用 vendor 名）。未知名跳过（保守）。
fn flag_bit(name: &str) -> Option<ModFlags> {
    Some(match name {
        "Attack" => ModFlags::ATTACK,
        "Spell" => ModFlags::SPELL,
        "Hit" => ModFlags::HIT,
        "Dot" => ModFlags::DOT,
        "Cast" => ModFlags::CAST,
        "Melee" => ModFlags::MELEE,
        "Area" => ModFlags::AREA,
        "Projectile" => ModFlags::PROJECTILE,
        "Ailment" => ModFlags::AILMENT,
        "Weapon" => ModFlags::WEAPON,
        other => return ModFlags::weapon_type_bit(other),
    })
}

fn compile_flags(names: &[String]) -> ModFlags {
    let mut flags = ModFlags::NONE;
    for name in names {
        if let Some(bit) = flag_bit(name) {
            flags |= bit;
        }
    }
    flags
}

fn keyword_bit(name: &str) -> Option<KeywordFlags> {
    Some(match name {
        "Aura" => KeywordFlags::AURA,
        "Curse" => KeywordFlags::CURSE,
        "Hit" => KeywordFlags::HIT,
        "Ailment" => KeywordFlags::AILMENT,
        "Poison" => KeywordFlags::POISON,
        "Bleed" => KeywordFlags::BLEED,
        "Ignite" => KeywordFlags::IGNITE,
        // 未映射 keyword（如 Arrow）保守跳过——对应条目保持 verified:false。
        _ => return None,
    })
}

fn compile_keyword_flags(names: &[String]) -> KeywordFlags {
    let mut flags = KeywordFlags::NONE;
    for name in names {
        if let Some(bit) = keyword_bit(name) {
            flags = flags | bit;
        }
    }
    flags
}

fn parse_target(target: &str) -> Option<ActorRef> {
    match target {
        // enemy 包装（EnemyModifier LIST）由消费侧 env_finalize 阶段 2 处理，
        // 本批次 enemy-target 条目保持 verified:false（target 仅作元数据透传）。
        "minion" => Some(ActorRef::Minion),
        _ => None,
    }
}

/// SkillTypes 名 → 位（vendor `SkillType:Spell` 形态去前缀）。未知名 → NONE。
fn skill_type_bit(name: &str) -> SkillTypes {
    let bare = name.strip_prefix("SkillType:").unwrap_or(name);
    match bare {
        "Attack" => SkillTypes::ATTACK,
        "Spell" => SkillTypes::SPELL,
        "Projectile" => SkillTypes::PROJECTILE,
        "Area" => SkillTypes::AREA,
        "Melee" => SkillTypes::MELEE,
        "Triggered" => SkillTypes::TRIGGERED,
        "Minion" => SkillTypes::MINION,
        "Aura" => SkillTypes::AURA,
        "Channel" => SkillTypes::CHANNEL,
        _ => SkillTypes::NONE,
    }
}

fn damage_type_bit(name: &str) -> Option<DamageType> {
    Some(match name {
        "Physical" => DamageType::Physical,
        "Fire" => DamageType::Fire,
        "Cold" => DamageType::Cold,
        "Lightning" => DamageType::Lightning,
        "Chaos" => DamageType::Chaos,
        _ => return None,
    })
}

/// 模板 tag → pobr `ModTag`。**可映射清单**：
/// - `Condition` / `ActorCondition`（actor=enemy → `Enemy<Var>` 条件）；
/// - `SkillType`（去 `SkillType:` 前缀，已知闭集）；
/// - `DamageType`；
/// - `Multiplier`（**字面** var/div/limit；按某资源/属性数量线性缩放，读
///   `cfg.multiplier(var)`）；
/// - `PerStat`（**字面** stat/div/limit；按 actor 已算出 stat 线性缩放，读
///   `EvalContext::stat_lookup`——运行时 [`ModTag::PerStat`] M4-T1 已接通）；
/// - `MultiplierThreshold`（**字面** var/threshold/upper 二元 gate，运行时
///   [`ModTag::MultiplierThreshold`] 已接通）；
/// - `SkillName`（V2：`skillName` 单名 / `skillNameList` 列表统一小写收编为
///   [`ModTag::SkillName`]，按 `cfg.skill_name` 等值 gate；`includeTransfigured`
///   忽略——PoE2 无变体宝石，vendor 的 gem name→gameId 等值退化为名字等值。
///   `partialMatch`/`summonSkill`/`neg` 在 vendor PoE2 数据零出现，由抽取器
///   白名单挡在门外）。
///
/// **不可映射**（pobr 无落点）：`ItemCondition` / `GlobalEffect` /
/// `PercentStat` / 带 `$n` 字段值的 `Multiplier`——返回 `None`，对应条目保持
/// `verified:false`（保守门控，不误产可能错误的 tag）。
fn compile_tag(tag: &TemplateTagDef) -> Option<ModTag> {
    match tag.tag_type.as_str() {
        "Condition" => {
            let var = scalar_text(tag.fields.get("var")?)?;
            let neg = tag.fields.get("neg").and_then(scalar_bool).unwrap_or(false);
            Some(ModTag::condition(var, neg))
        }
        "ActorCondition" => {
            let var = scalar_text(tag.fields.get("var")?)?;
            let neg = tag.fields.get("neg").and_then(scalar_bool).unwrap_or(false);
            let actor = tag.fields.get("actor").and_then(scalar_text);
            match actor.as_deref() {
                Some("enemy") => Some(ModTag::condition(format!("Enemy{var}"), neg)),
                _ => Some(ModTag::condition(var, neg)),
            }
        }
        "SkillType" => {
            let name = scalar_text(tag.fields.get("skillType")?)?;
            let st = skill_type_bit(&name);
            if st == SkillTypes::NONE {
                None
            } else {
                Some(ModTag::SkillTypes(st))
            }
        }
        "DamageType" => {
            let name = scalar_text(tag.fields.get("damageType")?)?;
            damage_type_bit(&name).map(ModTag::DamageType)
        }
        "Multiplier" => {
            // 字面 var/div/limit 的 Multiplier（资源/属性线性缩放，读 cfg.multiplier(var)）。
            // 带 `$n` 捕获的 var 仍保守跳过（与文档门控一致，避免误产）。本批仅字面 var
            //（如 Blood Mage 的 `EnergyShieldOnbodyarmour`，slot 倍率经 orchestrator
            // per_slot_defence_multipliers 填充）。
            let var = scalar_text(tag.fields.get("var")?)?;
            if var.starts_with('$') {
                None
            } else {
                let div = tag.fields.get("div").and_then(scalar_number).unwrap_or(1.0);
                let limit = tag.fields.get("limit").and_then(scalar_number);
                Some(ModTag::multiplier(var, div, limit))
            }
        }
        "PerStat" => {
            // 字面 stat/div/limit（vendor `statList`/`base`/`actor` 形态无落点，
            // 由调用方形态白名单挡在门外——本处 fields 只可能是这三键）。
            let stat = scalar_text(tag.fields.get("stat")?)?;
            if stat.starts_with('$') {
                return None;
            }
            let div = tag.fields.get("div").and_then(scalar_number).unwrap_or(1.0);
            let limit = tag.fields.get("limit").and_then(scalar_number);
            Some(ModTag::PerStat {
                stat,
                div,
                limit,
                limit_var: None,
                actor: None,
            })
        }
        "SkillName" => {
            // skillName 单名或 skillNameList 列表二选一（vendor ModStore.lua:752-780），
            // 小写收编；空列表 / 含 `$n` 捕获 → 不可映射（防御，抽取器同样拦截）。
            let names: Vec<String> =
                match (tag.fields.get("skillName"), tag.fields.get("skillNameList")) {
                    (Some(single), None) => vec![scalar_text(single)?.to_lowercase()],
                    (None, Some(TemplateScalarDef::TextList(list))) if !list.is_empty() => {
                        list.iter().map(|s| s.to_lowercase()).collect()
                    }
                    _ => return None,
                };
            if names.iter().any(|n| n.contains('$')) {
                return None;
            }
            Some(ModTag::SkillName { names })
        }
        "MultiplierThreshold" => {
            // 字面 var/threshold/upper（vendor `thresholdVar`/`actor` 形态跳过）。
            // upper 缺省 false = vendor `stat ≥ threshold` 生效侧。
            let var = scalar_text(tag.fields.get("var")?)?;
            if var.starts_with('$') {
                return None;
            }
            let threshold = tag.fields.get("threshold").and_then(scalar_number)?;
            let upper = tag
                .fields
                .get("upper")
                .and_then(scalar_bool)
                .unwrap_or(false);
            Some(ModTag::MultiplierThreshold {
                var,
                threshold,
                upper,
            })
        }
        // 未映射 tag 形态：保守跳过。
        _ => None,
    }
}

/// 供离线抽取器（`sync-pob-catalog extract-lua --what special-mods`）预检：
/// tag 能否被 [`compile_tag`] 忠实映射。不可映射的 tag 在编译期会被静默丢弃——
/// 批量抽取必须把这类条目**整条跳过**而不是丢 tag（否则条件词条变常驻）。
pub fn tag_is_mappable(tag: &TemplateTagDef) -> bool {
    compile_tag(tag).is_some()
}

/// 同上预检：ModFlags 位名是否可映射（[`flag_bit`]；未知名编译期静默跳过，
/// 会拓宽 mod 适用范围）。
pub fn flag_name_is_mappable(name: &str) -> bool {
    flag_bit(name).is_some()
}

/// 同上预检：KeywordFlags 位名是否可映射（[`keyword_bit`]）。
pub fn keyword_flag_name_is_mappable(name: &str) -> bool {
    keyword_bit(name).is_some()
}

fn scalar_number(scalar: &TemplateScalarDef) -> Option<f64> {
    match scalar {
        TemplateScalarDef::Number(n) => Some(*n),
        _ => None,
    }
}

fn scalar_text(scalar: &TemplateScalarDef) -> Option<String> {
    match scalar {
        TemplateScalarDef::Text(s) => Some(s.clone()),
        TemplateScalarDef::Number(n) => Some(n.to_string()),
        TemplateScalarDef::Bool(b) => Some(b.to_string()),
        TemplateScalarDef::Enum { .. } | TemplateScalarDef::TextList(_) => None,
    }
}

fn scalar_bool(scalar: &TemplateScalarDef) -> Option<bool> {
    match scalar {
        TemplateScalarDef::Bool(b) => Some(*b),
        _ => None,
    }
}

/// `"$n"` → 第 n 个捕获的数值（1-based）；非捕获形当字面量解析；解析失败 → 0。
fn resolve_capture_number(arg: &str, captures: &[String]) -> f64 {
    if let Some(idx) = capture_index(arg) {
        captures
            .get(idx - 1)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
    } else {
        arg.parse::<f64>().unwrap_or(0.0)
    }
}

/// `"$3"` → `Some(3)`；非此形 → `None`。
fn capture_index(s: &str) -> Option<usize> {
    s.strip_prefix('$').and_then(|n| n.parse::<usize>().ok())
}

/// 把 [`ValueOpDef`] 算子链编译为 `value_expr::ValueExpr` 树（求值器单点复用）。
///
/// 首段连续线性算子（div/mult/base）折进 `Input` 节点；之后的包装算子
/// （negate/clamp）逐层向外。包装算子之后再出现的线性算子无法用单点
/// `ValueExpr` 表达（Input 只读原始 capture）——本批次条目算子链均为
/// 「单段线性 + 可选 negate/clamp」满足该约束；越界形态保守忽略（由
/// differential 兜底）。
fn build_value_expr(ops: &[ValueOpDef]) -> ValueExpr {
    let mut mult = 1.0;
    let mut div = 1.0;
    let mut base = 0.0;
    let mut i = 0;
    while i < ops.len() {
        match &ops[i] {
            ValueOpDef::Div(n) => div *= *n,
            ValueOpDef::Mult(n) => mult *= *n,
            ValueOpDef::Base(n) => base += *n,
            _ => break,
        }
        i += 1;
    }
    let mut expr = ValueExpr::Input { mult, div, base };
    for op in &ops[i..] {
        expr = match op {
            ValueOpDef::Negate {} => ValueExpr::Negate {
                inner: Box::new(expr),
            },
            ValueOpDef::Clamp { min, max } => ValueExpr::Clamp {
                min: Some(*min),
                max: Some(*max),
                inner: Box::new(expr),
            },
            ValueOpDef::Div(_) | ValueOpDef::Mult(_) | ValueOpDef::Base(_) => expr,
        };
    }
    expr
}

fn eval_value_expr_def(def: &ValueExprDef, captures: &[String]) -> f64 {
    let capture = capture_index(&def.capture)
        .and_then(|idx| captures.get(idx - 1))
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let expr = build_value_expr(&def.ops);
    eval(&expr, capture)
}

fn instantiate_template(
    template: &CompiledTemplate,
    captures: &[String],
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
    source: &str,
) -> Vec<Modifier> {
    let mut out = Vec::with_capacity(template.mods.len());
    for m in &template.mods {
        let Some(name) = resolve_name(&m.name, captures, enums) else {
            continue;
        };
        let value = match instantiate_value(&m.value, captures, m.mod_type, enums, source) {
            Some(v) => v,
            None => continue,
        };
        let mut modifier = Modifier::new(name, m.mod_type, value).with_source(source);
        if !m.flags.is_empty() {
            modifier = modifier.with_flags(m.flags);
        }
        if !m.keyword_flags.is_empty() {
            modifier = modifier.with_keyword_flags(m.keyword_flags);
        }
        for tag in &m.tags {
            modifier = modifier.with_tag(tag.clone());
        }
        out.push(modifier);
    }
    out
}

/// 嵌套 mod 模板（`TemplateValueDef::Nested` 内层）实例化：mod_type / flags /
/// tags 按需即时编译（编译期已由 [`validate_nested_value`] fail-fast 校验
/// mod_type / enums；嵌套命中频率低，即时编译开销可忽略）。
fn instantiate_mod_def(
    def: &pobr_data::catalog::parser_rules::ModTemplateDef,
    captures: &[String],
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
    source: &str,
) -> Option<Modifier> {
    let mod_type = parse_mod_type(&def.mod_type)?;
    let name = resolve_name(&def.name, captures, enums)?;
    let value = instantiate_value(&def.value, captures, mod_type, enums, source)?;
    let mut modifier = Modifier::new(name, mod_type, value).with_source(source);
    let flags = compile_flags(&def.flags);
    if !flags.is_empty() {
        modifier = modifier.with_flags(flags);
    }
    let keyword_flags = compile_keyword_flags(&def.keyword_flags);
    if !keyword_flags.is_empty() {
        modifier = modifier.with_keyword_flags(keyword_flags);
    }
    for tag in def.tags.iter().filter_map(compile_tag) {
        modifier = modifier.with_tag(tag);
    }
    Some(modifier)
}

fn resolve_name(
    name: &TemplateNameDef,
    captures: &[String],
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
) -> Option<String> {
    match name {
        TemplateNameDef::Literal(s) => Some(s.clone()),
        TemplateNameDef::Enum { capture_index } => resolve_enum(*capture_index, captures, enums),
    }
}

/// enums 闭集查表：用第 n 个捕获词在 `enums[n]` 表里查完整字面量。
fn resolve_enum(
    capture_index: u32,
    captures: &[String],
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
) -> Option<String> {
    let table = enums.get(&capture_index)?;
    let word = captures.get((capture_index as usize).saturating_sub(1))?;
    table.get(word).cloned()
}

fn instantiate_value(
    value: &TemplateValueDef,
    captures: &[String],
    mod_type: ModType,
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
    source: &str,
) -> Option<ModValue> {
    match value {
        TemplateValueDef::Flag(b) => Some(ModValue::Bool(*b)),
        TemplateValueDef::Number(n) => Some(ModValue::Number(*n)),
        TemplateValueDef::Capture(s) => {
            if let Some(idx) = capture_index(s) {
                let raw = captures.get(idx - 1)?;
                if matches!(mod_type, ModType::List) {
                    Some(ModValue::Text(raw.clone()))
                } else {
                    raw.parse::<f64>().ok().map(ModValue::Number)
                }
            } else {
                // 字面量字符串（LIST text 值，如 GrantedPassive 名）。
                Some(ModValue::Text(s.clone()))
            }
        }
        TemplateValueDef::Expr(expr) => Some(ModValue::Number(eval_value_expr_def(expr, captures))),
        // 嵌套 mod 载荷（`{ mod = mod(...) }` 形态）→ ModValue::NestedMods，
        // 编排层经 `ModDb::list_nested` 转发（EnemyModifier/MinionModifier 等）。
        // 内层全部实例化失败 → None（跳过外层 mod，不产空载荷）。
        TemplateValueDef::Nested { mods } => {
            let nested: Vec<Modifier> = mods
                .iter()
                .filter_map(|m| instantiate_mod_def(m, captures, enums, source))
                .collect();
            if nested.is_empty() {
                None
            } else {
                Some(ModValue::NestedMods(nested))
            }
        }
        // 复杂 LIST 载荷（explode/level grant 等 PoB2 table）尚无 pobr 落点——
        // 跳过该 mod（条目保持 verified:false，由 handler_id 接管属后续）。
        TemplateValueDef::List(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::parser_rules::SpecialModsDef;

    fn def(json: &str) -> SpecialTemplateDef {
        serde_json::from_str(json).unwrap()
    }

    fn rules(defs: Vec<SpecialTemplateDef>) -> SpecialModRules {
        SpecialModRules::compile(&defs, &HandlerRegistry::new()).unwrap()
    }

    /// 数值捕获 + 算子链：`(\d+)% increased X` → INC，capture 直引。
    #[test]
    fn number_capture_inc() {
        let d = def(r#"{"id":"t","pattern":"(\\d+)% increased buffs","mods":[
                {"name":"BuffEffect","type":"INC","value":"$1"}],"batch":"S1"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("50% increased buffs", &reg).unwrap();
        assert_eq!(m.entry_id, "t");
        assert_eq!(m.mods.len(), 1);
        assert_eq!(m.mods[0].mod_type, ModType::Inc);
        assert_eq!(m.mods[0].value.as_number(), Some(50.0));
        assert!(!m.verified);
    }

    /// 算子链 negate：slower → MORE 负值。
    #[test]
    fn ops_negate() {
        let d = def(
            r#"{"id":"t","pattern":"buffs expire (\\d+)% slower","mods":[
                {"name":"BuffExpireRate","type":"MORE",
                 "value":{"ref":"$1","ops":[{"negate":{}}]}}],"batch":"S1"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("buffs expire 30% slower", &reg).unwrap();
        assert_eq!(m.mods[0].value.as_number(), Some(-30.0));
    }

    /// 嵌套 mod 载荷：`{"mods":[...]}` 值 → ModValue::NestedMods（捕获在内层
    /// 求值），编排层经 list_nested 转发。
    #[test]
    fn nested_mods_value() {
        let d = def(
            r#"{"id":"t","pattern":"enemies have (\\d+)% reduced armour","mods":[
                {"name":"EnemyModifier","type":"LIST","value":{"mods":[
                    {"name":"Armour","type":"INC",
                     "value":{"ref":"$1","ops":[{"negate":{}}]}}]}}],"batch":"V0"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r
            .try_match("enemies have 20% reduced armour", &reg)
            .unwrap();
        assert_eq!(m.mods.len(), 1);
        assert_eq!(m.mods[0].name, "EnemyModifier".into());
        let ModValue::NestedMods(inner) = &m.mods[0].value else {
            panic!("expected nested mods value");
        };
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].name, "Armour".into());
        assert_eq!(inner[0].mod_type, ModType::Inc);
        assert_eq!(inner[0].value.as_number(), Some(-20.0));
    }

    /// PerStat tag 映射：按已算出 stat 缩放（M4-T1 运行时通道）。
    #[test]
    fn per_stat_tag_maps() {
        let d = def(
            r#"{"id":"t","pattern":"gain (\\d+) armour per 50 life","mods":[
                {"name":"Armour","type":"BASE","value":"$1",
                 "tags":[{"type":"PerStat","stat":"Life","div":50.0}]}],"batch":"V0"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("gain 10 armour per 50 life", &reg).unwrap();
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::PerStat {
                stat: "Life".into(),
                div: 50.0,
                limit: None,
                limit_var: None,
                actor: None,
            }]
        );
    }

    /// MultiplierThreshold tag 映射：二元 gate。
    #[test]
    fn multiplier_threshold_tag_maps() {
        let d = def(
            r#"{"id":"t","pattern":"(\\d+)% more damage at close range","mods":[
                {"name":"Damage","type":"MORE","value":"$1",
                 "tags":[{"type":"MultiplierThreshold","var":"enemyDistance",
                          "threshold":20.0,"upper":true}]}],"batch":"V0"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("30% more damage at close range", &reg).unwrap();
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: 20.0,
                upper: true,
            }]
        );
    }

    /// SkillName tag 映射：单名 / 列表统一小写收编；includeTransfigured 忽略
    ///（PoE2 无变体宝石，等值退化）；缺名字段 → tag 跳过。
    #[test]
    fn skill_name_tag_maps() {
        let d = def(r#"{"id":"t","pattern":"fireball explodes twice","mods":[
                {"name":"FireballExtraExplosion","type":"FLAG","value":true,
                 "tags":[{"type":"SkillName","skillName":"Fireball","includeTransfigured":true}]}],"batch":"V2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("fireball explodes twice", &reg).unwrap();
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::SkillName {
                names: vec!["fireball".into()],
            }]
        );

        let d = def(r#"{"id":"t2","pattern":"strikes chain","mods":[
                {"name":"ChainCountMax","type":"BASE","value":1,
                 "tags":[{"type":"SkillName","skillNameList":["Flicker Strike","Viper Strike"]}]}],"batch":"V2"}"#);
        let r = rules(vec![d]);
        let m = r.try_match("strikes chain", &reg).unwrap();
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::SkillName {
                names: vec!["flicker strike".into(), "viper strike".into()],
            }]
        );

        // 名字段缺失 → tag 保守跳过（mod 保留、不挂 tag）。
        let d = def(r#"{"id":"t3","pattern":"noop","mods":[
                {"name":"X","type":"BASE","value":1,
                 "tags":[{"type":"SkillName"}]}],"batch":"V2"}"#);
        let r = rules(vec![d]);
        let m = r.try_match("noop", &reg).unwrap();
        assert!(m.mods[0].tags.is_empty());
    }

    /// 嵌套 mod 载荷编译期校验：内层未知 mod_type fail-fast。
    #[test]
    fn nested_bad_mod_type_fails_compile() {
        let d = def(r#"{"id":"t","pattern":"x","mods":[
                {"name":"EnemyModifier","type":"LIST","value":{"mods":[
                    {"name":"Armour","type":"MAX","value":1}]}}],"batch":"V0"}"#);
        let err = SpecialModRules::compile(&[d], &HandlerRegistry::new()).unwrap_err();
        assert!(matches!(err, SpecialCompileError::BadModType { .. }));
    }

    /// 线性折叠 div：`$1 / 100`。
    #[test]
    fn ops_div_linear() {
        let d = def(r#"{"id":"t","pattern":"gain (\\d+) per cent","mods":[
                {"name":"X","type":"BASE","value":{"ref":"$1","ops":[{"div":100.0}]}}],"batch":"S1"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("gain 50 per cent", &reg).unwrap();
        assert_eq!(m.mods[0].value.as_number(), Some(0.5));
    }

    /// FLAG 字面量值。
    #[test]
    fn flag_literal() {
        let d = def(r#"{"id":"t","pattern":"cannot be ignited","mods":[
                {"name":"AvoidIgnite","type":"FLAG","value":true}],"batch":"S2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("cannot be ignited", &reg).unwrap();
        assert_eq!(m.mods[0].mod_type, ModType::Flag);
        assert_eq!(m.mods[0].value.as_bool(), Some(true));
    }

    /// LIST text 值（keystone/granted passive 名）。
    #[test]
    fn list_text_capture() {
        let d = def(r#"{"id":"t","pattern":"allocates (.+)","mods":[
                {"name":"GrantedPassive","type":"LIST","value":"$1"}],"batch":"S2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("allocates icebreaker", &reg).unwrap();
        assert_eq!(m.mods[0].mod_type, ModType::List);
        assert_eq!(m.mods[0].value.as_text(), Some("icebreaker"));
    }

    /// 整行锚定：前后多字符不命中。
    #[test]
    fn anchored_no_partial_match() {
        let d = def(r#"{"id":"t","pattern":"cannot be ignited","mods":[
                {"name":"AvoidIgnite","type":"FLAG","value":true}],"batch":"S2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        assert!(r.try_match("you cannot be ignited", &reg).is_none());
        assert!(r.try_match("cannot be ignited sometimes", &reg).is_none());
    }

    /// enums 闭集映射：词 → 完整 ModName 字面量。
    #[test]
    fn enums_name_mapping() {
        let d = def(
            r#"{"id":"t","pattern":"adds (fire|cold) damage taken","enums":{"1":{"fire":"FireDamageTaken","cold":"ColdDamageTaken"}},
                "mods":[{"name":{"enum":1},"type":"BASE","value":10.0}],"batch":"S1"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("adds cold damage taken", &reg).unwrap();
        assert_eq!(m.mods[0].name.as_str(), "ColdDamageTaken");
    }

    /// Condition tag 映射。
    #[test]
    fn condition_tag() {
        let d = def(r#"{"id":"t","pattern":"never crit","mods":[
                {"name":"X","type":"FLAG","value":true,
                 "tags":[{"type":"Condition","var":"NeverCrit","neg":true}]}],"batch":"S2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("never crit", &reg).unwrap();
        assert_eq!(m.mods[0].tags.len(), 1);
        assert!(
            matches!(&m.mods[0].tags[0], ModTag::Condition { var, negated, .. } if var=="NeverCrit" && *negated)
        );
    }

    /// 字面 Multiplier tag 映射（fork-a：Blood Mage `MaximumLife BASE 1 × Multiplier`）。
    #[test]
    fn multiplier_tag_literal() {
        let d = def(r#"{"id":"t","pattern":"life per es on body","mods":[
                {"name":"MaximumLife","type":"BASE","value":1,
                 "tags":[{"type":"Multiplier","var":"EnergyShieldOnbodyarmour","div":1}]}],"batch":"S2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("life per es on body", &reg).unwrap();
        assert_eq!(m.mods[0].name.as_str(), "MaximumLife");
        assert_eq!(m.mods[0].tags.len(), 1);
        assert!(matches!(
            &m.mods[0].tags[0],
            ModTag::Multiplier { var, div, .. } if var == "EnergyShieldOnbodyarmour" && *div == 1.0
        ));
    }

    /// 不可映射 tag（ItemCondition）静默跳过，mod 仍产出。
    #[test]
    fn unmapped_tag_skipped() {
        let d = def(r#"{"id":"t","pattern":"body armour grants x","mods":[
                {"name":"X","type":"FLAG","value":true,
                 "tags":[{"type":"ItemCondition","itemSlot":"Body Armour","rarityCond":"NORMAL"}]}],"batch":"S2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("body armour grants x", &reg).unwrap();
        assert_eq!(m.mods.len(), 1);
        assert!(m.mods[0].tags.is_empty());
    }

    /// handler_id 未注册：命中但产空 mods + 标记。
    #[test]
    fn unregistered_handler_marked() {
        let d = def(
            r#"{"id":"t","pattern":"explode on kill","handler_id":"special:explode","handler_args":["$1"],"batch":"S2"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("explode on kill", &reg).unwrap();
        assert!(m.mods.is_empty());
        assert_eq!(m.unregistered_handler.as_deref(), Some("special:explode"));
    }

    /// 编译错误：重复 id。
    #[test]
    fn duplicate_id_errors() {
        let a = def(
            r#"{"id":"dup","pattern":"a","mods":[{"name":"X","type":"FLAG","value":true}],"batch":"S2"}"#,
        );
        let b = def(
            r#"{"id":"dup","pattern":"b","mods":[{"name":"Y","type":"FLAG","value":true}],"batch":"S2"}"#,
        );
        let err = SpecialModRules::compile(&[a, b], &HandlerRegistry::new()).unwrap_err();
        assert!(matches!(err, SpecialCompileError::DuplicateId { .. }));
    }

    /// 编译错误：非法 regex。
    #[test]
    fn bad_pattern_errors() {
        let a = def(
            r#"{"id":"t","pattern":"(unclosed","mods":[{"name":"X","type":"FLAG","value":true}],"batch":"S2"}"#,
        );
        let err = SpecialModRules::compile(&[a], &HandlerRegistry::new()).unwrap_err();
        assert!(matches!(err, SpecialCompileError::BadPattern { .. }));
    }

    /// 编译错误：未知 mod_type。
    #[test]
    fn bad_mod_type_errors() {
        let a = def(
            r#"{"id":"t","pattern":"a","mods":[{"name":"X","type":"WAT","value":1.0}],"batch":"S2"}"#,
        );
        let err = SpecialModRules::compile(&[a], &HandlerRegistry::new()).unwrap_err();
        assert!(matches!(err, SpecialCompileError::BadModType { .. }));
    }

    /// C-2 安全批次代表性条目（vendor 名表缺口的纯 INC/BASE 模板，verified:false）：
    /// 实例化 → 期望 Modifier（name/type/value）。
    #[test]
    fn c2_safe_batch_representative() {
        let cases = [
            (
                r#"{"id":"increased_skill_effect_duration","pattern":"(\\d+)% increased skill effect duration","mods":[{"name":"Duration","type":"INC","value":"$1"}],"batch":"S1"}"#,
                "12% increased skill effect duration",
                "Duration",
                ModType::Inc,
                12.0,
            ),
            (
                r#"{"id":"charm_slots_colon","pattern":"charm slots: (\\d+)","mods":[{"name":"CharmLimit","type":"BASE","value":"$1"}],"batch":"S1"}"#,
                "charm slots: 3",
                "CharmLimit",
                ModType::Base,
                3.0,
            ),
            (
                r#"{"id":"life_regeneration_per_second","pattern":"(\\d+(?:\\.\\d+)?) life regeneration per second","mods":[{"name":"LifeRegen","type":"BASE","value":"$1"}],"batch":"S1"}"#,
                "5.5 life regeneration per second",
                "LifeRegen",
                ModType::Base,
                5.5,
            ),
        ];
        let reg = HandlerRegistry::new();
        for (json, line, name, mod_type, value) in cases {
            let r = rules(vec![def(json)]);
            let m = r
                .try_match(line, &reg)
                .unwrap_or_else(|| panic!("命中 {line}"));
            assert_eq!(m.mods.len(), 1, "{line}");
            assert_eq!(m.mods[0].name.as_str(), name, "{line}");
            assert_eq!(m.mods[0].mod_type, mod_type, "{line}");
            assert_eq!(m.mods[0].value.as_number(), Some(value), "{line}");
            assert!(!m.verified);
        }
    }

    /// 真实仓库数据全量编译成功（闸门冒烟，正式断言在 special_mods_gate.rs）。
    #[test]
    fn repo_special_mods_compile() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data")
            .join(pobr_data::data_version())
            .join("overlay/special_mods.json");
        let raw = std::fs::read_to_string(path).expect("special_mods.json 可读");
        let doc: SpecialModsDef = serde_json::from_str(&raw).expect("special_mods.json 可解析");
        let rules = SpecialModRules::compile(&doc.entries, &HandlerRegistry::new())
            .expect("仓库 special_mods 全量编译成功");
        assert_eq!(rules.len(), doc.entries.len());
    }
}
