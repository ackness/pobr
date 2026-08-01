//! Interpreter for special-mod templates.
//!
//! Input = the concatenation of `overlay/special_mods.json` and
//! `generated/special_derived.json` as a list of [`SpecialTemplateDef`]
//! (schema in [`pobr_data::catalog::parser_rules`]). Compilation happens at
//! load time ([`RegexSet`] prefilter + a per-entry [`Regex`], whole-line
//! anchored, input lowercased); at runtime a single (already-lowercased)
//! line is matched against the whole pattern and instantiated as a
//! [`Modifier`].
//!
//! **Single evaluator**: evaluation of `$n` numeric placeholders, the five
//! operators, and restricted predicates is shared with
//! [`crate::rules::value_expr`] (config / special / parser all use the same
//! restricted language — no third dialect). This module is only responsible
//! for (1) compiling a [`ValueOpDef`] operator chain into a
//! `value_expr::ValueExpr` tree and calling `value_expr::eval`; (2) enum
//! closed-set lookups (a small approved DSL extension — every output is an
//! explicit literal from the table, never string concatenation).
//!
//! **DSL hard boundaries** (20-target-architecture §5): numeric captures are
//! `(\d+(?:\.\d+)?)`, word captures are explicit closed sets, open captures
//! `(.+)` are forbidden (entries that need those go through `handler_id`
//! instead). This interpreter does not enforce pattern shape at compile time
//! (compilation only checks that the regex itself is valid) — shape
//! conformance is enforced by curation plus the gate test (C-4).
//!
//! **Conservative gating**: this batch of entries carries a few native PoB2
//! tag shapes (`ItemCondition` / `GlobalEffect` / complex LIST payloads,
//! etc.) that have no pobr `ModTag` counterpart yet. Such tags are
//! **skipped** at instantiation (the mod is still produced, just without
//! that tag), and the entry stays `verified:false`, guarded by differential
//! testing (Track D) and the parity report. See [`compile_tag`] for the
//! list of what can be mapped.

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
use crate::parse::mod_parser::template::{normalize_attribute_var, normalize_perstat_slot_suffix};
use crate::rules::registry::{HandlerCtx, HandlerRegistry};
use crate::rules::stat_map_engine::damage_bound_mod_name;
use crate::rules::value_expr::eval;

/// Compile-time error (fail-fast at load time, never silent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecialCompileError {
    /// Pattern is not a valid regex.
    BadPattern {
        /// Entry id.
        entry_id: String,
        /// Original pattern.
        pattern: String,
        /// Error reported by the regex crate.
        reason: String,
    },
    /// Duplicate `id` (uniqueness is checked after concatenating both
    /// tables).
    DuplicateId {
        /// Conflicting id.
        entry_id: String,
    },
    /// Out-of-range `enums` reference (the `n` in `{"enum": n}` has no key
    /// in the entry's `enums` table).
    EnumRefMissing {
        /// Entry id.
        entry_id: String,
        /// Referenced capture index.
        capture_index: u32,
    },
    /// Unknown mod_type literal.
    BadModType {
        /// Entry id.
        entry_id: String,
        /// Original literal.
        literal: String,
    },
    /// Both template mods and handler_id are present (mutually exclusive).
    ModsAndHandler {
        /// Entry id.
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

/// A compiled special entry.
#[derive(Debug)]
struct CompiledEntry {
    id: String,
    regex: Regex,
    verified: bool,
    /// Instantiated as template mods (mutually exclusive with `handler_id`).
    template: Option<CompiledTemplate>,
    /// Handler routing (mutually exclusive with template).
    handler_id: Option<String>,
    /// Handler arguments (in `"$n"` form).
    handler_args: Vec<String>,
    /// Enum closed sets (capture index → word → full literal).
    enums: BTreeMap<u32, BTreeMap<String, String>>,
}

/// Compiled template (mod list + already-resolved mod_type).
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
    /// Tags that were successfully mapped (unmappable tags are dropped at
    /// compile time, see `compile_tag`).
    tags: Vec<ModTag>,
    #[allow(dead_code)]
    target: Option<ActorRef>,
}

/// A single special match (produced by [`SpecialModRules::try_match`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialMatch {
    /// Stable id of the matched entry.
    pub entry_id: String,
    /// The instantiated modifiers (already carrying the source mod text).
    pub mods: Vec<Modifier>,
    /// Forwarded for the parity report (`verified:false` gets its own
    /// column).
    pub verified: bool,
    /// Set when `handler_id` isn't registered (the match still hits but
    /// produces empty mods; used for reporting, never panics).
    pub unregistered_handler: Option<String>,
}

/// Compiled special rule set (compiled at load time, read-only at runtime).
#[derive(Debug)]
pub struct SpecialModRules {
    set: RegexSet,
    entries: Vec<CompiledEntry>,
}

impl SpecialModRules {
    /// Compiles at load time. Returns `Err` (fail fast) on an invalid
    /// pattern, a duplicate id, an out-of-range enums reference, or an
    /// unknown mod_type.
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

            // Whole-line anchor + lowercase input (pattern literals are
            // already lowercase, per vendor :6155-6158). Doesn't double-wrap
            // if the pattern already has `^`/`$`.
            let anchored = anchor_pattern(&def.pattern);
            let regex = Regex::new(&anchored).map_err(|e| SpecialCompileError::BadPattern {
                entry_id: def.id.clone(),
                pattern: def.pattern.clone(),
                reason: e.to_string(),
            })?;
            patterns.push(anchored);

            // enums table (keys converted to u32).
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

    /// Empty rule set (no entries; `try_match` always returns `None`) — the
    /// "no data loaded" branch.
    pub fn empty() -> Self {
        Self {
            set: RegexSet::empty(),
            entries: Vec::new(),
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the rule set is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Matches a single (already-lowercased) line against the whole
    /// pattern. Returns `Some` when it hits and can be instantiated.
    ///
    /// When several entries match, takes the **first** one (entries are
    /// ordered as in the data file) — special entries are designed to be
    /// mutually exclusive per line; overlap is a curation issue (the gate
    /// test C-4 checks pattern uniqueness).
    pub fn try_match(&self, line: &str, registry: &HandlerRegistry) -> Option<SpecialMatch> {
        let matches = self.set.matches(line);
        if !matches.matched_any() {
            return None;
        }
        for idx in matches.iter() {
            let entry = &self.entries[idx];
            // RegexSet only prefilters; grab the capture groups with the
            // per-entry Regex (RegexSet itself produces no captures).
            let Some(caps) = entry.regex.captures(line) else {
                continue;
            };
            let captures: Vec<String> = caps
                .iter()
                .skip(1)
                .map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
                .collect();

            // Handler path.
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

            // Template path.
            let Some(template) = &entry.template else {
                // Pure-recognition entry (no mods, no handler): matches but
                // produces no mod.
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

/// Anchors the whole line (doesn't double-wrap if the pattern already has
/// `^`/`$`).
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
        // Validate that the enums reference isn't out of range (name).
        if let TemplateNameDef::Enum { capture_index } = &m.name
            && !enums.contains_key(capture_index)
        {
            return Err(SpecialCompileError::EnumRefMissing {
                entry_id: def.id.clone(),
                capture_index: *capture_index,
            });
        }
        // Fail-fast validation of the nested mod payload (recompiled on
        // demand at instantiation time, see `instantiate_mod_def`).
        validate_nested_value(&def.id, &m.value, enums)?;
        // vendor→PoBR mod name translation (literal names only; the enums
        // table has been audited to contain no affected names).
        let (name, flag_names) = match &m.name {
            TemplateNameDef::Literal(n) => {
                let (translated, remaining) = translate_vendor_name(n, &m.flags);
                (TemplateNameDef::Literal(translated), remaining)
            }
            other => (other.clone(), m.flags.clone()),
        };
        let flags = compile_flags(&flag_names);
        let keyword_flags = compile_keyword_flags(&m.keyword_flags);
        let tags = m.tags.iter().filter_map(compile_tag).collect();
        let target = m.target.as_deref().and_then(parse_target);
        mods.push(CompiledModTemplate {
            name,
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

/// Compile-time validation for `TemplateValueDef::Nested`: inner mod_type is
/// known and enums references aren't out of range (recursive). Non-nested
/// value shapes pass through unchecked.
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

/// ModFlags name → bit (shared vendor names for damage mode / weapon type).
/// Unknown names are skipped (conservative).
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
        // Unmapped keywords (e.g. Arrow) are conservatively skipped — the
        // entry stays verified:false.
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
        // The enemy wrapper (EnemyModifier LIST) is handled by the consumer
        // side in env_finalize stage 2; this batch's enemy-target entries
        // stay verified:false (target is forwarded here purely as
        // metadata).
        "minion" => Some(ActorRef::Minion),
        _ => None,
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

/// Maps a template tag to a pobr `ModTag`. **Mappable list**:
/// - `Condition` / `ActorCondition` (actor=enemy → `Enemy<Var>` condition);
/// - `SkillType` (strips the `SkillType:` prefix, known closed set);
/// - `DamageType`;
/// - `Multiplier` (**literal** var/div/limit; linear scaling by some
///   resource/attribute count, reads `cfg.multiplier(var)`);
/// - `PerStat` (**literal** stat/div/limit; linear scaling by an actor's
///   already-computed stat, reads `EvalContext::stat_lookup` — wired up at
///   runtime via [`ModTag::PerStat`]);
/// - `PercentStat` (V2 slice 2: **literal** stat/percent; scales by a
///   percentage of an already-computed stat,
///   `value = ceil(value × stat × percent/100)`, runtime
///   [`ModTag::PercentStat`]. Vendor's `statList`/`percentVar`/`actor`/
///   `base`/`limit`/`floor` shapes are kept out by the extractor's
///   whitelist);
/// - `MultiplierThreshold` (**literal** var/threshold/upper binary gate,
///   wired up at runtime via [`ModTag::MultiplierThreshold`]);
/// - `StatThreshold` (V2s4: **literal** stat/threshold/upper binary gate,
///   reads the [`CalcConfig::stat`] snapshot, runtime
///   [`ModTag::StatThreshold`]);
/// - `SkillName` (V2: a single `skillName` or a `skillNameList` list is
///   uniformly lowercased into [`ModTag::SkillName`], gated by equality
///   against `cfg.skill_name`; `includeTransfigured` is ignored — PoE2 has
///   no transfigured gems, so vendor's gem-name→gameId equality degenerates
///   to plain name equality. `partialMatch`/`summonSkill`/`neg` never occur
///   in vendor's PoE2 data and are kept out by the extractor's whitelist).
///
/// **Unmappable** (no pobr counterpart): `ItemCondition` / `GlobalEffect` /
/// a `Multiplier` with a `$n`-captured field value — returns `None`, and the
/// entry stays `verified:false` (conservative gating, so we never produce a
/// possibly-wrong tag).
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
            // Full enum table (data-driven A1, single source of truth
            // `SkillTypes::from_pob2_name`): special_vendor names come from
            // a reverse lookup on vendor's enum, so a miss means corrupt
            // data — panics in debug builds (visible per A2), conservatively
            // drops the tag in release.
            let lookup = |name: &str| {
                let bare = name.strip_prefix("SkillType:").unwrap_or(name);
                let st = SkillTypes::from_pob2_name(bare);
                debug_assert!(st.is_some(), "unknown SkillType name: {bare}");
                st
            };
            if let Some(v) = tag.fields.get("skillType") {
                return lookup(&scalar_text(v)?).map(ModTag::SkillTypes);
            }
            // `skillTypeList` (vendor OR over multiple types — the ModStore
            // SkillType branch fires on any match) → folded into a single
            // SkillTypes bitset (ModTag::SkillTypes matches via
            // `intersects`, which is OR semantics). If any name misses, the
            // whole tag is conservatively dropped.
            let TemplateScalarDef::TextList(items) = tag.fields.get("skillTypeList")? else {
                return None;
            };
            let mut acc = SkillTypes::NONE;
            for name in items {
                acc |= lookup(name)?;
            }
            (!acc.is_empty()).then_some(ModTag::SkillTypes(acc))
        }
        "DamageType" => {
            let name = scalar_text(tag.fields.get("damageType")?)?;
            damage_type_bit(&name).map(ModTag::DamageType)
        }
        "Multiplier" => {
            // Multiplier with literal var/div/limit (linear scaling by a
            // resource/attribute, reads cfg.multiplier(var)). A var with a
            // `$n` capture is still conservatively skipped (consistent with
            // the doc-level gating, to avoid misproducing it). This batch
            // only has literal vars (e.g. Blood Mage's
            // `EnergyShieldOnbodyarmour`; the per-slot multiplier is filled
            // in by the orchestrator's per_slot_defence_multipliers).
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
            // Literal stat/div/limit (vendor's `statList`/`base`/`actor`
            // shapes have no counterpart and are kept out by the caller's
            // shape whitelist — fields here can only be these three keys).
            let stat = scalar_text(tag.fields.get("stat")?)?;
            if stat.starts_with('$') {
                return None;
            }
            let stat = normalize_stat_name(&stat);
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
        "PercentStat" => {
            // Literal stat/percent (vendor's statList/percentVar/actor/
            // base/limit/floor shapes are kept out by the extractor's
            // whitelist). A missing percent matches vendor's `(percent and
            // percent/100 or 1)` or-1 branch (mult = the stat itself).
            let stat = scalar_text(tag.fields.get("stat")?)?;
            if stat.starts_with('$') {
                return None;
            }
            let stat = normalize_stat_name(&stat);
            // percent present but not a number (e.g. a hand-written overlay
            // mistakenly using `$n`) → the whole tag is unmappable; must not
            // silently fall back to the or-1 branch (mult would be off by
            // 100x).
            let percent = match tag.fields.get("percent") {
                None => None,
                Some(v) => Some(scalar_number(v)?),
            };
            Some(ModTag::PercentStat { stat, percent })
        }
        "SkillName" => {
            // Either a single skillName or a skillNameList list (vendor
            // ModStore.lua:752-780), lowercased. An empty list or a `$n`
            // capture inside a name → unmappable (defensive; the extractor
            // rejects these too).
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
            // Literal var/threshold/upper (vendor's `thresholdVar`/`actor`
            // shapes are skipped). upper defaults to false, matching
            // vendor's `stat ≥ threshold` active side.
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
        "StatThreshold" => {
            // Literal stat/threshold/upper (vendor's `statList`/
            // `thresholdStat`/`thresholdPercent(Var)`/`actor` shapes are
            // kept out by the extractor's whitelist). The gate reads the
            // cfg.stats snapshot inside `matches`, so it applies across all
            // FLAG/LIST/OVERRIDE query paths (unlike an eval-time tag).
            let stat = scalar_text(tag.fields.get("stat")?)?;
            if stat.starts_with('$') {
                return None;
            }
            let stat = normalize_stat_name(&stat);
            let threshold = tag.fields.get("threshold").and_then(scalar_number)?;
            let upper = tag
                .fields
                .get("upper")
                .and_then(scalar_bool)
                .unwrap_or(false);
            Some(ModTag::StatThreshold {
                stat,
                threshold,
                upper,
            })
        }
        "DistanceRamp" => {
            // Distance interpolation (vendor ModStore.lua:574-590).
            // TemplateScalarDef has no nested-array shape, so ramp points
            // are transcribed as `"distance multiplier"` text pairs (e.g.
            // `["35 0.2", "70 0"]` = vendor's `{ {35,0.2}, {70,0} }`);
            // semantics match the statmap engine's DistanceRamp branch
            // (linear interpolation over `cfg.skill_distance` at eval
            // time).
            let TemplateScalarDef::TextList(points) = tag.fields.get("ramp")? else {
                return None;
            };
            let mut ramp = Vec::with_capacity(points.len());
            for point in points {
                let mut parts = point.split_whitespace();
                let (Some(dist), Some(mult), None) = (parts.next(), parts.next(), parts.next())
                else {
                    return None;
                };
                ramp.push((dist.parse().ok()?, mult.parse().ok()?));
            }
            (!ramp.is_empty()).then_some(ModTag::DistanceRamp { ramp })
        }
        // Unmapped tag shape: conservatively skipped.
        _ => None,
    }
}

/// Normalizes stat names for PerStat/PercentStat/StatThreshold: expands
/// vendor's short attribute names (`Str`→`Strength`) and normalizes the
/// `On<Slot>` slot suffix (`OnBoots`→`Onboots`), matching the statmap
/// engine's convention (`template.rs::compile_tag`) — the key space written
/// back by the orchestrator (`inject_per_x_multipliers` writing
/// `cfg.stats`) is `Strength`/`<Stat>On<slot.id()>`; without normalization
/// the lookup key is missing and reads back as 0.
fn normalize_stat_name(stat: &str) -> String {
    normalize_attribute_var(&normalize_perstat_slot_suffix(stat))
}

/// Translates vendor mod names to PoBR names (a narrow closed set, matching
/// the corresponding dispatch in the statmap engine's `translate_mod_name`).
/// special entry names come straight from vendor Lua literals; entries
/// whose name doesn't match what PoBR's consumers expect just sit unqueried
/// in the db — this is an activation fix, not a behaviour change:
/// - `<Type>Min/Max` → `<Type>DamageMin/Max` (consumed by `calc::damage`:236
///   for the additional damage bucket; [`damage_bound_mod_name`] matches
///   exactly, so `FireResistMax` etc. aren't mistakenly affected);
/// - `CritChance`/`CritMultiplier` → `CriticalStrike{Chance,Multiplier}`
///   (consumed by `calc::crit`);
/// - `Speed` is dispatched by flag: `Attack→AttackSpeed`/`Cast→CastSpeed`/
///   bare→`SkillSpeed` (consumed by `skill_use_time::SPEED_BUCKET`; a bare
///   `Speed` is never queried), and the flag consumed by the dispatch is
///   removed from the passthrough set (same convention as statmap
///   :1528-1539).
///
/// Every other name passes through unchanged (PoBR names go straight
/// through, and the dormant LIST channel is left alone).
fn translate_vendor_name(name: &str, flags: &[String]) -> (String, Vec<String>) {
    if let Some(bound) = damage_bound_mod_name(name) {
        return (bound, flags.to_vec());
    }
    match name {
        "CritChance" => ("CriticalStrikeChance".into(), flags.to_vec()),
        "CritMultiplier" => ("CriticalStrikeMultiplier".into(), flags.to_vec()),
        "Speed" => {
            if let Some(pos) = flags.iter().position(|f| f == "Attack") {
                let mut rest = flags.to_vec();
                rest.remove(pos);
                ("AttackSpeed".into(), rest)
            } else if let Some(pos) = flags.iter().position(|f| f == "Cast") {
                let mut rest = flags.to_vec();
                rest.remove(pos);
                ("CastSpeed".into(), rest)
            } else {
                ("SkillSpeed".into(), flags.to_vec())
            }
        }
        _ => (name.to_string(), flags.to_vec()),
    }
}

/// Precheck for the offline extractor (`sync-pob-catalog extract-lua --what
/// special-mods`): whether a tag can be faithfully mapped by
/// [`compile_tag`]. Unmappable tags are silently dropped at compile time —
/// bulk extraction must **skip these entries entirely** rather than drop
/// the tag (otherwise a conditional mod turns into an always-on one).
pub fn tag_is_mappable(tag: &TemplateTagDef) -> bool {
    compile_tag(tag).is_some()
}

/// Same precheck: whether a ModFlags bit name can be mapped ([`flag_bit`];
/// an unknown name is silently skipped at compile time, which widens the
/// mod's applicability).
pub fn flag_name_is_mappable(name: &str) -> bool {
    flag_bit(name).is_some()
}

/// Same precheck: whether a KeywordFlags bit name can be mapped
/// ([`keyword_bit`]).
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

/// `"$n"` → the nth capture's numeric value (1-based); a non-capture form
/// is parsed as a literal; parse failure → 0.
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

/// `"$3"` → `Some(3)`; anything else → `None`.
fn capture_index(s: &str) -> Option<usize> {
    s.strip_prefix('$').and_then(|n| n.parse::<usize>().ok())
}

/// Compiles a [`ValueOpDef`] operator chain into a `value_expr::ValueExpr`
/// tree (reusing the single evaluator).
///
/// The leading run of linear operators (div/mult/base) is folded into the
/// `Input` node; any wrapping operators (negate/clamp) that follow are
/// layered outward from there. A linear operator appearing *after* a
/// wrapping operator can't be expressed by the single `ValueExpr` (`Input`
/// only reads the raw capture) — every entry in this batch has an operator
/// chain of "one linear segment plus optional negate/clamp", which
/// satisfies this constraint; out-of-scope shapes are conservatively
/// ignored (caught by differential testing as a backstop).
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

/// Instantiates a nested mod template (the inner payload of
/// `TemplateValueDef::Nested`): mod_type / flags / tags are compiled on
/// demand ([`validate_nested_value`] already fail-fast validates mod_type /
/// enums at compile time; nested entries are rare, so the on-demand
/// compilation cost is negligible).
fn instantiate_mod_def(
    def: &pobr_data::catalog::parser_rules::ModTemplateDef,
    captures: &[String],
    enums: &BTreeMap<u32, BTreeMap<String, String>>,
    source: &str,
) -> Option<Modifier> {
    let mod_type = parse_mod_type(&def.mod_type)?;
    let name = resolve_name(&def.name, captures, enums)?;
    // Same vendor→PoBR name translation as compile_template (on-demand
    // compile path for nested payloads).
    let (name, flag_names) = translate_vendor_name(&name, &def.flags);
    let value = instantiate_value(&def.value, captures, mod_type, enums, source)?;
    let mut modifier = Modifier::new(name, mod_type, value).with_source(source);
    let flags = compile_flags(&flag_names);
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

/// Enum closed-set lookup: looks up the full literal for the nth captured
/// word in the `enums[n]` table.
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
                // Literal string (a LIST text value, e.g. a GrantedPassive
                // name).
                Some(ModValue::Text(s.clone()))
            }
        }
        TemplateValueDef::Expr(expr) => Some(ModValue::Number(eval_value_expr_def(expr, captures))),
        // Nested mod payload (the `{ mod = mod(...) }` shape) →
        // ModValue::NestedMods, forwarded by the orchestration layer
        // through `ModDb::list_nested` (EnemyModifier/MinionModifier,
        // etc.). If every inner mod fails to instantiate → None (skip the
        // outer mod rather than producing an empty payload).
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
        // Complex LIST payloads (PoB2 tables like explode/level grant) have
        // no pobr counterpart yet — skip this mod (the entry stays
        // verified:false; a handler_id can take over later).
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

    /// Numeric capture + operator chain: `(\d+)% increased X` → INC,
    /// capture referenced directly.
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

    /// Operator chain negate: "slower" → a negative MORE value.
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

    /// Nested mod payload: a `{"mods":[...]}` value → ModValue::NestedMods
    /// (captures are evaluated in the inner mods), forwarded by the
    /// orchestration layer through list_nested.
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

    /// PerStat tag mapping: scales by an already-computed stat.
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

        // Slot-suffix normalization (normalize_stat_name): vendor
        // `OnBoots` → `Onboots`, matching the `<Stat>On<slot.id()>` key
        // filled in by the orchestration layer's
        // per_slot_defence_multipliers.
        let d = def(r#"{"id":"t2","pattern":"noop","mods":[
                {"name":"X","type":"BASE","value":1,
                 "tags":[{"type":"PerStat","stat":"ArmourOnBoots","div":25.0}]}],"batch":"V2"}"#);
        let r = rules(vec![d]);
        let m = r.try_match("noop", &reg).unwrap();
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::PerStat {
                stat: "ArmourOnboots".into(),
                div: 25.0,
                limit: None,
                limit_var: None,
                actor: None,
            }]
        );
    }

    /// vendor→PoBR mod name translation (translate_vendor_name):
    /// `<Type>Min/Max` for the additional damage family,
    /// CritChance/CritMultiplier, Speed dispatched by flag (consuming the
    /// dispatch flag); PoBR names and dormant-channel names pass through
    /// unchanged.
    #[test]
    fn vendor_mod_names_translate_to_pobr_names() {
        let d = def(
            r#"{"id":"t","pattern":"adds (\\d+)% of your maximum energy shield as cold damage","mods":[
                {"name":"ColdMin","type":"BASE","value":1,
                 "tags":[{"type":"PercentStat","stat":"EnergyShield","percent":"$1"}]},
                {"name":"CritChance","type":"INC","value":50},
                {"name":"Speed","type":"INC","value":8,"flags":["Attack"]},
                {"name":"Speed","type":"MORE","value":5},
                {"name":"FireResistMax","type":"OVERRIDE","value":90},
                {"name":"Armour","type":"BASE","value":100}],"batch":"V2"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r
            .try_match(
                "adds 10% of your maximum energy shield as cold damage",
                &reg,
            )
            .unwrap();
        let names: Vec<&str> = m.mods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "ColdDamageMin", // additional-damage family rewrite (name consumed by damage.rs:236)
                "CriticalStrikeChance", // crit family rewrite
                "AttackSpeed",   // Speed+Attack dispatch
                "SkillSpeed",    // bare Speed dispatch
                "FireResistMax", // not Min/Max after stripping, so untouched
                "Armour",        // PoBR name passthrough
            ]
        );
        // The Speed→AttackSpeed dispatch consumes the Attack flag (same
        // convention as statmap).
        assert!(m.mods[2].flags.is_empty());
    }

    /// PercentStat tag mapping (V2 slice 2): scales by a percentage of an
    /// already-computed stat; percent can be omitted (vendor's or-1
    /// branch).
    #[test]
    fn percent_stat_tag_maps() {
        let d = def(
            r#"{"id":"t","pattern":"gain accuracy equal to (\\d+)% of dexterity","mods":[
                {"name":"Accuracy","type":"BASE","value":1,
                 "tags":[{"type":"PercentStat","stat":"Dex","percent":40.0}]}],"batch":"V2"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r
            .try_match("gain accuracy equal to 40% of dexterity", &reg)
            .unwrap();
        // Stat name normalization (normalize_stat_name): short attribute
        // name Dex → full name Dexterity, matching the key space the
        // orchestration layer fills into cfg.stats.
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::PercentStat {
                stat: "Dexterity".into(),
                percent: Some(40.0),
            }]
        );

        // percent omitted → None (at runtime, mult = the stat itself).
        let d = def(r#"{"id":"t2","pattern":"noop","mods":[
                {"name":"X","type":"BASE","value":1,
                 "tags":[{"type":"PercentStat","stat":"Life"}]}],"batch":"V2"}"#);
        let r = rules(vec![d]);
        let m = r.try_match("noop", &reg).unwrap();
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::PercentStat {
                stat: "Life".into(),
                percent: None,
            }]
        );
    }

    /// StatThreshold tag mapping (V2s4): a binary gate reading the
    /// cfg.stats snapshot.
    #[test]
    fn stat_threshold_tag_maps() {
        let d = def(
            r#"{"id":"t","pattern":"gain (\\d+) rage on hit while at maximum frenzy charges","mods":[
                {"name":"RageOnHit","type":"BASE","value":"$1",
                 "tags":[{"type":"StatThreshold","stat":"FrenzyCharges","threshold":3.0}]}],"batch":"V2"}"#,
        );
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r
            .try_match("gain 2 rage on hit while at maximum frenzy charges", &reg)
            .unwrap();
        assert_eq!(
            m.mods[0].tags,
            vec![ModTag::StatThreshold {
                stat: "FrenzyCharges".into(),
                threshold: 3.0,
                upper: false,
            }]
        );
    }

    /// MultiplierThreshold tag mapping: a binary gate.
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

    /// SkillName tag mapping: a single name or a list, both lowercased
    /// uniformly; includeTransfigured is ignored (PoE2 has no transfigured
    /// gems, so it degenerates to plain equality); a missing name field →
    /// tag skipped.
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

        // Missing name field → tag conservatively skipped (mod is kept,
        // just without the tag).
        let d = def(r#"{"id":"t3","pattern":"noop","mods":[
                {"name":"X","type":"BASE","value":1,
                 "tags":[{"type":"SkillName"}]}],"batch":"V2"}"#);
        let r = rules(vec![d]);
        let m = r.try_match("noop", &reg).unwrap();
        assert!(m.mods[0].tags.is_empty());
    }

    /// Compile-time validation of a nested mod payload: unknown inner
    /// mod_type fails fast.
    #[test]
    fn nested_bad_mod_type_fails_compile() {
        let d = def(r#"{"id":"t","pattern":"x","mods":[
                {"name":"EnemyModifier","type":"LIST","value":{"mods":[
                    {"name":"Armour","type":"MAX","value":1}]}}],"batch":"V0"}"#);
        let err = SpecialModRules::compile(&[d], &HandlerRegistry::new()).unwrap_err();
        assert!(matches!(err, SpecialCompileError::BadModType { .. }));
    }

    /// Linear-folded div: `$1 / 100`.
    #[test]
    fn ops_div_linear() {
        let d = def(r#"{"id":"t","pattern":"gain (\\d+) per cent","mods":[
                {"name":"X","type":"BASE","value":{"ref":"$1","ops":[{"div":100.0}]}}],"batch":"S1"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        let m = r.try_match("gain 50 per cent", &reg).unwrap();
        assert_eq!(m.mods[0].value.as_number(), Some(0.5));
    }

    /// FLAG literal value.
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

    /// LIST text value (keystone/granted-passive name).
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

    /// Whole-line anchoring: extra characters before/after don't match.
    #[test]
    fn anchored_no_partial_match() {
        let d = def(r#"{"id":"t","pattern":"cannot be ignited","mods":[
                {"name":"AvoidIgnite","type":"FLAG","value":true}],"batch":"S2"}"#);
        let r = rules(vec![d]);
        let reg = HandlerRegistry::new();
        assert!(r.try_match("you cannot be ignited", &reg).is_none());
        assert!(r.try_match("cannot be ignited sometimes", &reg).is_none());
    }

    /// Enum closed-set mapping: word → full ModName literal.
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

    /// Condition tag mapping.
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

    /// Literal Multiplier tag mapping (fork-a: Blood Mage's
    /// `MaximumLife BASE 1 × Multiplier`).
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

    /// An unmappable tag (ItemCondition) is silently skipped; the mod is
    /// still produced.
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

    /// handler_id not registered: matches but produces empty mods, and is
    /// flagged.
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

    /// Compile error: duplicate id.
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

    /// Compile error: invalid regex.
    #[test]
    fn bad_pattern_errors() {
        let a = def(
            r#"{"id":"t","pattern":"(unclosed","mods":[{"name":"X","type":"FLAG","value":true}],"batch":"S2"}"#,
        );
        let err = SpecialModRules::compile(&[a], &HandlerRegistry::new()).unwrap_err();
        assert!(matches!(err, SpecialCompileError::BadPattern { .. }));
    }

    /// Compile error: unknown mod_type.
    #[test]
    fn bad_mod_type_errors() {
        let a = def(
            r#"{"id":"t","pattern":"a","mods":[{"name":"X","type":"WAT","value":1.0}],"batch":"S2"}"#,
        );
        let err = SpecialModRules::compile(&[a], &HandlerRegistry::new()).unwrap_err();
        assert!(matches!(err, SpecialCompileError::BadModType { .. }));
    }

    /// Representative entries from the C-2 safe batch (plain INC/BASE
    /// templates for gaps in vendor's name table, verified:false):
    /// instantiation → expected Modifier (name/type/value).
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

    /// Smoke gate: the full corpus of real repo data compiles successfully
    /// (the formal assertions live in special_mods_gate.rs). special_mods
    /// is split into two layers: a version-independent curation layer
    /// `data/overlay-common/` (P1-3) plus a version layer
    /// `data/<ver>/overlay/`. This test reads and concatenates the same
    /// union before compiling (pobr-core doesn't depend on pobr-gamedata,
    /// so it reads the files directly instead of going through the
    /// loader).
    #[test]
    fn repo_special_mods_compile() {
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let mut entries = Vec::new();
        for path in [
            data_root.join("overlay-common/special_mods.json"),
            data_root
                .join(pobr_data::data_version())
                .join("overlay/special_mods.json"),
        ] {
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => panic!("{}: {e}", path.display()),
            };
            let doc: SpecialModsDef = serde_json::from_str(&raw).expect("special_mods.json 可解析");
            entries.extend(doc.entries);
        }
        assert!(!entries.is_empty(), "special_mods 两层皆空？");
        let rules = SpecialModRules::compile(&entries, &HandlerRegistry::new())
            .expect("仓库 special_mods 全量编译成功");
        assert_eq!(rules.len(), entries.len());
    }
}
