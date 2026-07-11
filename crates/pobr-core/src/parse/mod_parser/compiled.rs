//! [`CompiledParserRules`]——把 Track A 的 [`ModParserRulesDoc`]（serde 数据）
//! 编译为带索引的运行时形态（蓝图 §2.3）。
//!
//! - **plain 表**（name_map / flag_phrases / suffix / pen / damage / regen /
//!   degen / cost / base_cost / flag_types）：纯子串匹配，用
//!   `AhoCorasick(MatchKind::LeftmostLongest)`——该语义 = 「最早开始、同起点
//!   取最长」，与 vendor `scan(..., plain=true)` 完全等价。
//! - **pattern 表**（forms / pre_flags / tag_phrases）：每条编译为
//!   [`LuaPattern`]；用 `literal` 派生字段建混合 AC 预过滤桶，line 命中某
//!   literal 才对对应 pattern 跑 [`LuaPattern::find`]；无 literal / anchored
//!   的进 always-check 桶。最终在候选集合上跑 vendor §2.1 的「最早 + 最长 +
//!   `#pattern` 长 + 字典序」四级 tie-break。
//!
//! **编译在 core**（gamedata 只 load + merge，保 P9）：`CompiledParserRules::
//! compile(&ModParserRulesDoc)`。

use aho_corasick::{AhoCorasick, MatchKind};
use pobr_data::catalog::parser_rules::{
    FlagTypeDef, FormDef, ModParserRulesDoc, NameMapDef, PhraseNamesDef, PhraseValueDef,
    PreFlagDef, RuleEffectsDef, TagPhraseDef,
};

use super::scan::{LuaMatch, LuaPattern, PatternError};
use crate::rules::{HandlerRegistry, SpecialModRules, register_special_handlers};
use pobr_data::catalog::parser_rules::SpecialTemplateDef;

/// 编译错误（pattern 语法 / AC 构建失败）。
#[derive(Debug, Clone)]
pub enum CompileError {
    /// 某条 pattern 不在子集语法内。
    Pattern(PatternError),
    /// aho-corasick 构建失败（理论上不触发——固定数据）。
    Index(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pattern(e) => write!(f, "{e}"),
            Self::Index(s) => write!(f, "aho-corasick build failed: {s}"),
        }
    }
}

impl std::error::Error for CompileError {}

impl From<PatternError> for CompileError {
    fn from(e: PatternError) -> Self {
        Self::Pattern(e)
    }
}

/// 一条编译后的 pattern 表条目。
#[derive(Debug, Clone)]
pub struct PatternEntry<T> {
    /// 编译后的匹配器。
    pub pattern: LuaPattern,
    /// 表内序号（命中后取数据 + 字典序 tie-break 用——条目按 pattern 字典序入库）。
    pub index: usize,
    /// 关联载荷。
    pub payload: T,
}

/// 一条 plain 表条目（子串匹配 → 载荷）。
#[derive(Debug, Clone)]
pub struct PlainEntry<T> {
    /// 匹配短语（小写）。
    pub phrase: String,
    /// 关联载荷。
    pub payload: T,
}

/// plain 表 = 短语列表 + LeftmostLongest AC 索引。
#[derive(Debug)]
pub struct PlainTable<T> {
    entries: Vec<PlainEntry<T>>,
    ac: Option<AhoCorasick>,
}

impl<T> PlainTable<T> {
    fn build(entries: Vec<PlainEntry<T>>) -> Result<Self, CompileError> {
        let ac = if entries.is_empty() {
            None
        } else {
            Some(
                AhoCorasick::builder()
                    .match_kind(MatchKind::LeftmostLongest)
                    .build(entries.iter().map(|e| e.phrase.as_bytes()))
                    .map_err(|e| CompileError::Index(e.to_string()))?,
            )
        };
        Ok(Self { entries, ac })
    }

    /// vendor `scan(line, table, plain=true)`：返回最早+最长子串命中的载荷 +
    /// 剩余文本（命中段切除，保留原大小写）。`text` 应已小写化。返回
    /// `(payload_index, remaining)`。
    pub fn scan(&self, lower: &str, original: &str) -> Option<(usize, String)> {
        let ac = self.ac.as_ref()?;
        let m = ac.find(lower)?;
        let payload_index = m.pattern().as_usize();
        let remaining = splice_out(original, m.start(), m.end());
        Some((payload_index, remaining))
    }

    /// 命中载荷（不切文本，FLAG/PEN 等需要先看 payload）。
    pub fn payload(&self, index: usize) -> &T {
        &self.entries[index].payload
    }

    /// 总条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// pattern 表 = 编译条目 + literal 预过滤桶。
#[derive(Debug)]
pub struct PatternTable<T> {
    entries: Vec<PatternEntry<T>>,
    /// literal → 该 literal 所属的 entries 序号（混合 AC 预过滤）。
    literal_ac: Option<AhoCorasick>,
    literal_owners: Vec<Vec<usize>>,
    /// 无 literal / 需总是检查的 entries 序号（always-check 桶）。
    always_check: Vec<usize>,
}

impl<T> PatternTable<T> {
    fn build(
        entries: Vec<PatternEntry<T>>,
        literals: Vec<Option<String>>,
    ) -> Result<Self, CompileError> {
        let mut literal_strings: Vec<String> = Vec::new();
        let mut literal_owners: Vec<Vec<usize>> = Vec::new();
        let mut always_check: Vec<usize> = Vec::new();
        // literal → owner 多对一去重：同 literal 多条 pattern 共用一个 AC pattern。
        let mut literal_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (i, lit) in literals.iter().enumerate() {
            match lit {
                // literal 长度 <3 的退化为 always-check（蓝图 §2.3：短 literal 过滤
                // 效益低反增误命中）。
                Some(l) if l.len() >= 3 => {
                    let owner = *literal_index.entry(l.clone()).or_insert_with(|| {
                        literal_strings.push(l.clone());
                        literal_owners.push(Vec::new());
                        literal_strings.len() - 1
                    });
                    literal_owners[owner].push(i);
                }
                _ => always_check.push(i),
            }
        }
        let literal_ac = if literal_strings.is_empty() {
            None
        } else {
            Some(
                AhoCorasick::builder()
                    .match_kind(MatchKind::Standard)
                    .build(&literal_strings)
                    .map_err(|e| CompileError::Index(e.to_string()))?,
            )
        };
        Ok(Self {
            entries,
            literal_ac,
            literal_owners,
            always_check,
        })
    }

    /// vendor `scan(line, table)`：在候选 pattern 上跑 §2.1 四级 tie-break，
    /// 返回胜者载荷 index + 捕获 + 剩余文本。`lower` 已小写化、`original` 原大小写。
    pub fn scan(&self, lower: &str, original: &str) -> Option<(usize, LuaMatch, String)> {
        let mut best: Option<(usize, LuaMatch)> = None;
        let mut visited = vec![false; self.entries.len()];

        let try_entry = |idx: usize, best: &mut Option<(usize, LuaMatch)>| {
            let entry = &self.entries[idx];
            if let Some(m) = entry.pattern.find(lower)
                && is_better(
                    &m,
                    entry,
                    best.as_ref().map(|(i, mm)| (&self.entries[*i], mm)),
                )
            {
                *best = Some((idx, m));
            }
        };

        // 候选 1：literal 预过滤命中的 pattern。
        if let Some(ac) = &self.literal_ac {
            for mat in ac.find_overlapping_iter(lower) {
                for &owner_entry in &self.literal_owners[mat.pattern().as_usize()] {
                    if !visited[owner_entry] {
                        visited[owner_entry] = true;
                        try_entry(owner_entry, &mut best);
                    }
                }
            }
        }
        // 候选 2：always-check 桶。
        for &idx in &self.always_check {
            if !visited[idx] {
                visited[idx] = true;
                try_entry(idx, &mut best);
            }
        }

        let (idx, m) = best?;
        let remaining = splice_out(original, m.start, m.end);
        Some((idx, m, remaining))
    }

    /// 命中载荷。
    pub fn payload(&self, index: usize) -> &T {
        &self.entries[index].payload
    }

    /// 总条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// vendor §2.1 胜者比较：`start 更小` > `start 等且 end 更大` > `start/end 等
/// 且 #pattern 更长` > （PoBR 第四级）pattern 字典序小者胜（条目已按字典序入库，
/// index 小 = 字典序靠前）。
fn is_better<T>(
    candidate: &LuaMatch,
    cand_entry: &PatternEntry<T>,
    incumbent: Option<(&PatternEntry<T>, &LuaMatch)>,
) -> bool {
    let Some((inc_entry, inc_m)) = incumbent else {
        return true;
    };
    if candidate.start != inc_m.start {
        return candidate.start < inc_m.start;
    }
    if candidate.end != inc_m.end {
        return candidate.end > inc_m.end;
    }
    let cand_len = cand_entry.pattern.raw_len();
    let inc_len = inc_entry.pattern.raw_len();
    if cand_len != inc_len {
        return cand_len > inc_len;
    }
    // 第四级：字典序——index 小者（字典序靠前）胜。
    cand_entry.index < inc_entry.index
}

/// 切除 `[start, end)` 段（字节 offset），拼接前后部分（vendor
/// `line:sub(1, start-1) .. line:sub(end+1)` 的 0-based 半开等价）。
fn splice_out(text: &str, start: usize, end: usize) -> String {
    let mut out = String::with_capacity(text.len() - (end - start));
    out.push_str(&text[..start]);
    out.push_str(&text[end..]);
    out
}

/// 编译后的全部解析规则表 + 索引。
#[derive(Debug)]
pub struct CompiledParserRules {
    /// formList（pattern → form id 字符串）。
    pub forms: PatternTable<String>,
    /// modNameList（plain → names + effects）。
    pub name_map: PlainTable<NameMapPayload>,
    /// modFlagList（plain → effects）。
    pub flag_phrases: PlainTable<RuleEffectsDef>,
    /// preFlagList（pattern → effects + inferred/handler）。
    pub pre_flags: PatternTable<PreFlagPayload>,
    /// modTagList（pattern → effects + inferred/handler）。
    pub tag_phrases: PatternTable<TagPhrasePayload>,
    /// suffixTypes（plain → 后缀名）。
    pub suffix_types: PlainTable<String>,
    /// dmgTypes（plain → 伤害类型名）。
    pub damage_types: PlainTable<String>,
    /// penTypes（plain → \*Penetration 名）。
    pub pen_types: PlainTable<String>,
    /// regenTypes（plain → 名集）。
    pub regen_types: PlainTable<Vec<String>>,
    /// degenTypes（plain → 名集）。
    pub degen_types: PlainTable<Vec<String>>,
    /// costTypes（plain → 名集）。
    pub cost_types_map: PlainTable<Vec<String>>,
    /// baseCostTypes（plain → 名集）。
    pub base_cost_types: PlainTable<Vec<String>>,
    /// flagTypes（pattern → 条件字符串 / 内嵌 mod；vendor 表混有 Lua pattern
    /// 条目，见 [`compile_flag_types`]）。
    pub flag_types: PatternTable<FlagTypePayload>,
    /// unsupportedModList（vendor + pobr 自加，小写整行查）。
    pub unsupported: std::collections::HashSet<String>,
    /// specialModList 通道（M5b `overlay/special_mods.json` +
    /// `generated/special_derived.json`，M6-conv2 接入）。vendor `parseMod` 在
    /// formList 之前查 specialModList 整行表（`ModParser.lua:6151-6160`）——引擎
    /// 在 form 扫描前查本表，命中即返回，对齐 vendor specialModList 锚定优先级。
    /// 未注入数据时 [`SpecialModRules::empty`]（query 恒 `None`，行为 = 旧引擎）。
    pub special: SpecialModRules,
    /// special 通道 handler 注册表（template-less 条目走 Rust 侧逻辑）。
    pub special_handlers: HandlerRegistry,
    /// 运行时解析 memo（text → outcome）。解析对（text, 规则集）纯函数，同一
    /// 行文本（装备逐次重算重新 ingest / 树 stat / per-gem GemProperty 扫描等
    /// 热路径的大量重复行）直接命中缓存，跳过 scan 引擎——这是 M6 预编译语料
    /// 「运行时 text→mods 缓存」的在线实现（覆盖任意用户文本，不限语料）。
    pub memo: ParseMemo,
}

/// [`CompiledParserRules`] 的运行时解析 memo（见字段文档）。
///
/// RwLock 保守取线程安全（native 测试可跨线程共享 `Arc<CompiledParserRules>`；
/// wasm 单线程零竞争）。只增不减——键空间 = build 里出现过的词条行，天然有界。
#[derive(Default)]
pub struct ParseMemo(std::sync::RwLock<std::collections::HashMap<String, super::ParseOutcome>>);

impl ParseMemo {
    pub(crate) fn get(&self, text: &str) -> Option<super::ParseOutcome> {
        self.0.read().ok()?.get(text).cloned()
    }

    pub(crate) fn insert(&self, text: &str, outcome: &super::ParseOutcome) {
        if let Ok(mut map) = self.0.write() {
            map.insert(text.to_string(), outcome.clone());
        }
    }
}

impl std::fmt::Debug for ParseMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.0.read().map(|m| m.len()).unwrap_or(0);
        write!(f, "ParseMemo({len} entries)")
    }
}

/// name_map 载荷。
#[derive(Debug, Clone)]
pub struct NameMapPayload {
    /// ModName 列表。
    pub names: Vec<String>,
    /// 附带效果（tag / flags）。
    pub effects: RuleEffectsDef,
}

/// pre_flags 载荷。
#[derive(Debug, Clone)]
pub struct PreFlagPayload {
    /// 效果字段全集。
    pub effects: RuleEffectsDef,
    /// 探针推断模板标记。
    pub inferred: bool,
    /// 推断失败兜底 handler id。
    pub handler_id: Option<String>,
}

/// tag_phrases 载荷。
#[derive(Debug, Clone)]
pub struct TagPhrasePayload {
    /// 效果字段全集（多数仅 tags）。
    pub effects: RuleEffectsDef,
    /// 探针推断模板标记。
    pub inferred: bool,
    /// 推断失败兜底 handler id。
    pub handler_id: Option<String>,
}

/// flag_types 载荷（条件字符串 / 内嵌 mod，二选一）。
#[derive(Debug, Clone)]
pub struct FlagTypePayload {
    /// 字符串形态：`Condition:X` / `NoLifeRegen` 等。
    pub condition: Option<String>,
    /// 表形态：内嵌 mod（hexproof 特例）的 (name, mod_type, value)。
    pub mod_def: Option<(String, String, f64)>,
}

impl CompiledParserRules {
    /// 编译全部规则表（不含 special 通道——special 表恒空，query 恒 `None`，
    /// 行为 = M6-conv1 引擎）。pattern 子集语法越界 → `Err`（数据固定、不应触发）。
    pub fn compile(doc: &ModParserRulesDoc) -> Result<Self, CompileError> {
        Self::compile_with_special(doc, &[])
    }

    /// 编译全部规则表 + special 通道（M6-conv2）。`special_defs` =
    /// `overlay/special_mods.json` + `generated/special_derived.json` 的
    /// 拼接条目（id 冲突在 [`SpecialModRules::compile`] fail-fast）。
    ///
    /// **编译在 core**（P9）：gamedata 只 load + merge，注入拼接后的条目数组。
    pub fn compile_with_special(
        doc: &ModParserRulesDoc,
        special_defs: &[SpecialTemplateDef],
    ) -> Result<Self, CompileError> {
        let mut special_handlers = HandlerRegistry::new();
        register_special_handlers(&mut special_handlers)
            .map_err(|e| CompileError::Index(format!("special handler 注册: {e}")))?;
        let special = SpecialModRules::compile(special_defs, &special_handlers)
            .map_err(|e| CompileError::Index(format!("special compile: {e}")))?;
        Ok(Self {
            forms: compile_forms(&doc.forms)?,
            name_map: compile_name_map(&doc.name_map)?,
            flag_phrases: compile_flag_phrases(&doc.flag_phrases)?,
            pre_flags: compile_pre_flags(&doc.pre_flags)?,
            tag_phrases: compile_tag_phrases(&doc.tag_phrases)?,
            suffix_types: compile_phrase_value(&doc.suffix_types)?,
            damage_types: compile_phrase_value(&doc.damage_types)?,
            pen_types: compile_phrase_value(&doc.pen_types)?,
            regen_types: compile_phrase_names(&doc.regen_types)?,
            degen_types: compile_phrase_names(&doc.degen_types)?,
            cost_types_map: compile_phrase_names(&doc.cost_types_map)?,
            base_cost_types: compile_phrase_names(&doc.base_cost_types)?,
            flag_types: compile_flag_types(&doc.flag_types)?,
            unsupported: doc
                .unsupported
                .iter()
                .chain(doc.unsupported_pobr_extra.iter())
                .map(|s| s.to_ascii_lowercase())
                .collect(),
            special,
            special_handlers,
            memo: ParseMemo::default(),
        })
    }
}

fn compile_forms(defs: &[FormDef]) -> Result<PatternTable<String>, CompileError> {
    let mut entries = Vec::with_capacity(defs.len());
    let mut literals = Vec::with_capacity(defs.len());
    for (i, d) in defs.iter().enumerate() {
        entries.push(PatternEntry {
            pattern: LuaPattern::compile(&d.pattern)?,
            index: i,
            payload: d.form.clone(),
        });
        literals.push(d.literal.clone());
    }
    PatternTable::build(entries, literals)
}

fn compile_pre_flags(defs: &[PreFlagDef]) -> Result<PatternTable<PreFlagPayload>, CompileError> {
    let mut entries = Vec::with_capacity(defs.len());
    let mut literals = Vec::with_capacity(defs.len());
    for (i, d) in defs.iter().enumerate() {
        entries.push(PatternEntry {
            pattern: LuaPattern::compile(&d.pattern)?,
            index: i,
            payload: PreFlagPayload {
                effects: d.effects.clone(),
                inferred: d.inferred,
                handler_id: d.handler_id.clone(),
            },
        });
        literals.push(d.literal.clone());
    }
    PatternTable::build(entries, literals)
}

fn compile_tag_phrases(
    defs: &[TagPhraseDef],
) -> Result<PatternTable<TagPhrasePayload>, CompileError> {
    let mut entries = Vec::with_capacity(defs.len());
    let mut literals = Vec::with_capacity(defs.len());
    for (i, d) in defs.iter().enumerate() {
        entries.push(PatternEntry {
            pattern: LuaPattern::compile(&d.pattern)?,
            index: i,
            payload: TagPhrasePayload {
                effects: d.effects.clone(),
                inferred: d.inferred,
                handler_id: d.handler_id.clone(),
            },
        });
        literals.push(d.literal.clone());
    }
    PatternTable::build(entries, literals)
}

fn compile_name_map(defs: &[NameMapDef]) -> Result<PlainTable<NameMapPayload>, CompileError> {
    PlainTable::build(
        defs.iter()
            .map(|d| PlainEntry {
                phrase: d.phrase.clone(),
                payload: NameMapPayload {
                    names: d.names.clone(),
                    effects: d.effects.clone(),
                },
            })
            .collect(),
    )
}

fn compile_flag_phrases(
    defs: &[pobr_data::catalog::parser_rules::FlagPhraseDef],
) -> Result<PlainTable<RuleEffectsDef>, CompileError> {
    PlainTable::build(
        defs.iter()
            .map(|d| PlainEntry {
                phrase: d.phrase.clone(),
                payload: d.effects.clone(),
            })
            .collect(),
    )
}

fn compile_phrase_value(defs: &[PhraseValueDef]) -> Result<PlainTable<String>, CompileError> {
    PlainTable::build(
        defs.iter()
            .map(|d| PlainEntry {
                phrase: d.phrase.clone(),
                payload: d.value.clone(),
            })
            .collect(),
    )
}

fn compile_phrase_names(defs: &[PhraseNamesDef]) -> Result<PlainTable<Vec<String>>, CompileError> {
    PlainTable::build(
        defs.iter()
            .map(|d| PlainEntry {
                phrase: d.phrase.clone(),
                payload: d.names.clone(),
            })
            .collect(),
    )
}

fn compile_flag_types(defs: &[FlagTypeDef]) -> Result<PatternTable<FlagTypePayload>, CompileError> {
    // vendor flagTypes 混有 Lua pattern 条目（如 `hindered,? with (%d+)%% reduced
    // movement speed`，ModParser.lua:6376）——纯子串 PlainTable 永远匹配不到这些
    // 条目（B3 实测 blood-mage 该行残留尾巴被闸门丢弃）。PatternTable 对字面量
    // 条目同样工作，且 scan 选择规则 = vendor `scan`（最早 + 最长）。
    let mut entries = Vec::with_capacity(defs.len());
    let mut literals = Vec::with_capacity(defs.len());
    for (i, d) in defs.iter().enumerate() {
        entries.push(PatternEntry {
            pattern: LuaPattern::compile(&d.phrase)?,
            index: i,
            payload: FlagTypePayload {
                condition: d.condition.clone(),
                mod_def: d
                    .mod_def
                    .as_ref()
                    .map(|m| (m.name.clone(), m.mod_type.clone(), m.value)),
            },
        });
        // 纯字面量 phrase 进 AC 预过滤桶；含 pattern 元字符的进 always-check 桶。
        let is_literal = !d.phrase.contains(['%', '?', '(', '[', '+', '*', '-']);
        literals.push(is_literal.then(|| d.phrase.clone()));
    }
    PatternTable::build(entries, literals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::parser_rules::ModParserRulesDoc;

    fn doc_from(json: &str) -> ModParserRulesDoc {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn plain_table_leftmost_longest() {
        let doc = doc_from(
            r#"{"forms":[],"name_map":[
                {"phrase":"fire","names":["FireDamage"]},
                {"phrase":"fire damage","names":["FireDamage"]}],
                "flag_phrases":[],"pre_flags":[],"tag_phrases":[]}"#,
        );
        let c = CompiledParserRules::compile(&doc).unwrap();
        // "increased fire damage" → 命中 "fire damage"（最长），剩余 "increased "
        let (idx, rest) = c
            .name_map
            .scan("increased fire damage", "increased Fire Damage")
            .unwrap();
        assert_eq!(c.name_map.payload(idx).names, vec!["FireDamage"]);
        assert_eq!(rest, "increased ");
    }

    #[test]
    fn pattern_table_earliest_longest_tiebreak() {
        // 两条都从同起点命中，取最长。
        let doc = doc_from(
            r#"{"forms":[
                {"pattern":"^(%d+)%% increased","form":"INC","literal":"% increased","anchored":true},
                {"pattern":"^(%d+)%%","form":"BASE","anchored":true}],
                "name_map":[],"flag_phrases":[],"pre_flags":[],"tag_phrases":[]}"#,
        );
        let c = CompiledParserRules::compile(&doc).unwrap();
        let (idx, m, rest) = c
            .forms
            .scan("50% increased damage", "50% increased damage")
            .unwrap();
        assert_eq!(c.forms.payload(idx), "INC");
        assert_eq!(m.captures, vec!["50"]);
        assert_eq!(rest, " damage");
    }

    #[test]
    fn unsupported_set_lowercased() {
        let doc = doc_from(
            r#"{"forms":[],"name_map":[],"flag_phrases":[],"pre_flags":[],"tag_phrases":[],
                "unsupported":["Mirrored"],"unsupported_pobr_extra":["Split"]}"#,
        );
        let c = CompiledParserRules::compile(&doc).unwrap();
        assert!(c.unsupported.contains("mirrored"));
        assert!(c.unsupported.contains("split"));
    }

    /// 仓库真实数据全表编译成功（pattern 子集语法全覆盖——无越界）。
    #[test]
    fn compiles_real_data() {
        // 加载**活动版本**的真实规则表并全量编译——版本无关 sanity：计数随版本增长
        // （4.5.0.3.4=91/775/682，4.5.2.1.3=95/788/686），故断言"充分填充"的下界而非
        // 精确值，使本测试在任一数据版本上都成立（与 multi_version smoke 同口径）。
        let path = crate::mod_parser::engine::test_support::real_rules_path();
        let json = std::fs::read_to_string(path).unwrap();
        let doc: ModParserRulesDoc = serde_json::from_str(&json).unwrap();
        let c = CompiledParserRules::compile(&doc).expect("真实规则表应全部编译成功");
        assert!(c.forms.len() >= 85, "forms={}", c.forms.len());
        assert!(c.name_map.len() >= 750, "name_map={}", c.name_map.len());
        assert!(
            c.tag_phrases.len() >= 650,
            "tag_phrases={}",
            c.tag_phrases.len()
        );
    }
}
