//! [`CompiledParserRules`] compiles Track A's [`ModParserRulesDoc`] (serde data)
//! into an indexed runtime form.
//!
//! - **Plain tables** (name_map / flag_phrases / suffix / pen / damage / regen /
//!   degen / cost / base_cost / flag_types): pure substring matching via
//!   `AhoCorasick(MatchKind::LeftmostLongest)` — "earliest start, longest at
//!   that start", exactly matching vendor `scan(..., plain=true)`.
//! - **Pattern tables** (forms / pre_flags / tag_phrases): each entry compiles
//!   to a [`LuaPattern`]. The `literal` field builds a mixed AC pre-filter
//!   bucket — [`LuaPattern::find`] only runs on a pattern once a line hits its
//!   literal; entries with no literal (or anchored ones) fall into an
//!   always-check bucket. The winner among candidates is picked by vendor
//!   §2.1's four-level tie-break: earliest start, then longest match, then
//!   longest `#pattern`, then lexicographic order.
//!
//! **Compiled in core** (gamedata only loads and merges, keeping I/O
//! contained): `CompiledParserRules::compile(&ModParserRulesDoc)`.

use aho_corasick::{AhoCorasick, MatchKind};
use pobr_data::catalog::parser_rules::{
    FlagTypeDef, FormDef, ModParserRulesDoc, NameMapDef, PhraseNamesDef, PhraseValueDef,
    PreFlagDef, RuleEffectsDef, TagPhraseDef,
};

use super::scan::{LuaMatch, LuaPattern, PatternError};
use crate::rules::{HandlerRegistry, SpecialModRules, register_special_handlers};
use pobr_data::catalog::parser_rules::SpecialTemplateDef;

/// Compile error (invalid pattern syntax, or AC index build failure).
#[derive(Debug, Clone)]
pub enum CompileError {
    /// A pattern falls outside the supported subset syntax.
    Pattern(PatternError),
    /// aho-corasick index build failed (should never trigger — data is fixed).
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

/// A compiled pattern-table entry.
#[derive(Debug, Clone)]
pub struct PatternEntry<T> {
    /// The compiled matcher.
    pub pattern: LuaPattern,
    /// Index within the table (used both to fetch the payload on a hit and as
    /// the lexicographic tie-break — entries are stored in pattern
    /// lexicographic order).
    pub index: usize,
    /// Associated payload.
    pub payload: T,
}

/// A plain-table entry (substring match -> payload).
#[derive(Debug, Clone)]
pub struct PlainEntry<T> {
    /// Match phrase (lowercased).
    pub phrase: String,
    /// Associated payload.
    pub payload: T,
}

/// A plain table = a phrase list plus a LeftmostLongest AC index.
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

    /// vendor `scan(line, table, plain=true)`: returns the payload of the
    /// earliest + longest substring match plus the remaining text (with the
    /// matched span spliced out, original casing preserved). `text` must
    /// already be lowercased. Returns `(payload_index, remaining)`.
    pub fn scan(&self, lower: &str, original: &str) -> Option<(usize, String)> {
        let ac = self.ac.as_ref()?;
        let m = ac.find(lower)?;
        let payload_index = m.pattern().as_usize();
        let remaining = splice_out(original, m.start(), m.end());
        Some((payload_index, remaining))
    }

    /// Payload for a hit (without splicing text — FLAG/PEN etc. need to
    /// inspect the payload first).
    pub fn payload(&self, index: usize) -> &T {
        &self.entries[index].payload
    }

    /// Total entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A pattern table = compiled entries plus a literal pre-filter bucket.
#[derive(Debug)]
pub struct PatternTable<T> {
    entries: Vec<PatternEntry<T>>,
    /// literal -> indices of the entries that own it (mixed AC pre-filter).
    literal_ac: Option<AhoCorasick>,
    literal_owners: Vec<Vec<usize>>,
    /// Indices of entries with no literal, or that must always be checked
    /// (the always-check bucket).
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
        // Dedup literal -> owner (many-to-one): patterns sharing a literal
        // reuse one AC pattern.
        let mut literal_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (i, lit) in literals.iter().enumerate() {
            match lit {
                // Literals shorter than 3 chars degrade to always-check
                // (short literals give little filtering benefit and add
                // false hits).
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

    /// vendor `scan(line, table)`: runs the §2.1 four-level tie-break over the
    /// candidate patterns and returns the winner's payload index, captures,
    /// and remaining text. `lower` is already lowercased; `original` keeps
    /// its casing.
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

        // Candidate set 1: patterns whose literal pre-filter hit.
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
        // Candidate set 2: the always-check bucket.
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

    /// Payload for a hit.
    pub fn payload(&self, index: usize) -> &T {
        &self.entries[index].payload
    }

    /// Total entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// vendor §2.1 winner comparison: smaller `start` wins > equal `start` with
/// larger `end` wins > equal `start`/`end` with longer `#pattern` wins >
/// (PoBR's fourth level) smaller pattern in lexicographic order wins (entries
/// are stored in lex order, so a smaller index means earlier in that order).
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
    // Fourth level: lexicographic order — the smaller index (earlier
    // lexicographically) wins.
    cand_entry.index < inc_entry.index
}

/// Splices out the `[start, end)` span (byte offsets), concatenating what's
/// before and after (the 0-based half-open equivalent of vendor
/// `line:sub(1, start-1) .. line:sub(end+1)`).
fn splice_out(text: &str, start: usize, end: usize) -> String {
    let mut out = String::with_capacity(text.len() - (end - start));
    out.push_str(&text[..start]);
    out.push_str(&text[end..]);
    out
}

/// All compiled parser rule tables plus their indices.
#[derive(Debug)]
pub struct CompiledParserRules {
    /// formList (pattern -> form id string).
    pub forms: PatternTable<String>,
    /// modNameList (plain -> names + effects).
    pub name_map: PlainTable<NameMapPayload>,
    /// modFlagList (plain -> effects).
    pub flag_phrases: PlainTable<RuleEffectsDef>,
    /// preFlagList (pattern -> effects + inferred/handler).
    pub pre_flags: PatternTable<PreFlagPayload>,
    /// modTagList (pattern -> effects + inferred/handler).
    pub tag_phrases: PatternTable<TagPhrasePayload>,
    /// suffixTypes (plain -> suffix name).
    pub suffix_types: PlainTable<String>,
    /// dmgTypes (plain -> damage type name).
    pub damage_types: PlainTable<String>,
    /// penTypes (plain -> \*Penetration name).
    pub pen_types: PlainTable<String>,
    /// regenTypes (plain -> name set).
    pub regen_types: PlainTable<Vec<String>>,
    /// degenTypes (plain -> name set).
    pub degen_types: PlainTable<Vec<String>>,
    /// costTypes (plain -> name set).
    pub cost_types_map: PlainTable<Vec<String>>,
    /// baseCostTypes (plain -> name set).
    pub base_cost_types: PlainTable<Vec<String>>,
    /// flagTypes (pattern -> condition string / embedded mod; the vendor
    /// table mixes in Lua pattern entries, see [`compile_flag_types`]).
    pub flag_types: PatternTable<FlagTypePayload>,
    /// unsupportedModList (vendor entries plus pobr's own additions, looked
    /// up as a lowercased whole line).
    pub unsupported: std::collections::HashSet<String>,
    /// The specialModList channel (`overlay/special_mods.json` +
    /// `generated/special_derived.json`, wired in). vendor `parseMod` checks
    /// the specialModList whole-line table before formList
    /// (`ModParser.lua:6151-6160`) — the engine checks this table before form
    /// scanning and returns on a hit, matching vendor's specialModList
    /// anchoring priority. When no data is injected this is
    /// [`SpecialModRules::empty`] (queries always return `None`, matching
    /// old-engine behavior).
    pub special: SpecialModRules,
    /// Handler registry for the special channel (template-less entries fall
    /// back to Rust-side logic).
    pub special_handlers: HandlerRegistry,
    /// Runtime parse memo (text -> outcome). Parsing is a pure function of
    /// (text, ruleset), so the same line of text — which recurs heavily on
    /// hot paths like re-ingesting an item on every recalculation, tree stat
    /// scans, or per-gem GemProperty scans — hits the cache directly and
    /// skips the scan engine. This is the online counterpart to the
    /// precompiled corpus's "runtime text->mods cache", covering arbitrary
    /// user text rather than just the corpus.
    pub memo: ParseMemo,
}

/// Runtime parse memo for [`CompiledParserRules`] (see the field doc).
///
/// Uses an `RwLock` conservatively for thread safety (native tests can share
/// an `Arc<CompiledParserRules>` across threads; wasm is single-threaded with
/// zero contention). Grow-only — the key space is the set of modifier lines
/// that appear in a build, which is naturally bounded.
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

/// name_map payload.
#[derive(Debug, Clone)]
pub struct NameMapPayload {
    /// List of ModNames.
    pub names: Vec<String>,
    /// Attached effects (tags / flags).
    pub effects: RuleEffectsDef,
}

/// pre_flags payload.
#[derive(Debug, Clone)]
pub struct PreFlagPayload {
    /// The full set of effect fields.
    pub effects: RuleEffectsDef,
    /// Marks a probe-inferred template.
    pub inferred: bool,
    /// Fallback handler id used when inference fails.
    pub handler_id: Option<String>,
}

/// tag_phrases payload.
#[derive(Debug, Clone)]
pub struct TagPhrasePayload {
    /// The full set of effect fields (mostly just tags).
    pub effects: RuleEffectsDef,
    /// Marks a probe-inferred template.
    pub inferred: bool,
    /// Fallback handler id used when inference fails.
    pub handler_id: Option<String>,
}

/// flag_types payload (either a condition string or an embedded mod).
#[derive(Debug, Clone)]
pub struct FlagTypePayload {
    /// String form: `Condition:X`, `NoLifeRegen`, etc.
    pub condition: Option<String>,
    /// Table form: the (name, mod_type, value) of an embedded mod (the
    /// hexproof special case).
    pub mod_def: Option<(String, String, f64)>,
}

impl CompiledParserRules {
    /// Compiles all rule tables without the special channel — the special
    /// table stays empty and queries always return `None`, matching
    /// old-engine behavior. Patterns falling outside the supported subset
    /// syntax return `Err` (data is fixed, should never trigger).
    pub fn compile(doc: &ModParserRulesDoc) -> Result<Self, CompileError> {
        Self::compile_with_special(doc, &[])
    }

    /// Compiles all rule tables plus the special channel. `special_defs` is
    /// the concatenated entries from `overlay/special_mods.json` and
    /// `generated/special_derived.json` (id conflicts fail fast in
    /// [`SpecialModRules::compile`]).
    ///
    /// **Compiled in core**: gamedata only loads and merges, then injects the
    /// concatenated entry array.
    pub fn compile_with_special(
        doc: &ModParserRulesDoc,
        special_defs: &[SpecialTemplateDef],
    ) -> Result<Self, CompileError> {
        let mut special_handlers = HandlerRegistry::new();
        register_special_handlers(&mut special_handlers)
            .map_err(|e| CompileError::Index(format!("special handler registration: {e}")))?;
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
    // The vendor flagTypes table mixes in Lua pattern entries (e.g.
    // `hindered,? with (%d+)%% reduced movement speed`, ModParser.lua:6376)
    // — a pure-substring PlainTable can never match these (confirmed by B3:
    // the blood-mage build's leftover tail on that line got dropped by the
    // gate). PatternTable also works for literal entries, and its scan
    // selection rule matches vendor `scan` (earliest + longest).
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
        // Pure-literal phrases go into the AC pre-filter bucket; those
        // containing pattern metacharacters go into the always-check bucket.
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
        // "increased fire damage" -> hits "fire damage" (the longest), leaving "increased "
        let (idx, rest) = c
            .name_map
            .scan("increased fire damage", "increased Fire Damage")
            .unwrap();
        assert_eq!(c.name_map.payload(idx).names, vec!["FireDamage"]);
        assert_eq!(rest, "increased ");
    }

    #[test]
    fn pattern_table_earliest_longest_tiebreak() {
        // Both entries match from the same start; the longer one wins.
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

    /// The repository's real rule data compiles successfully across all
    /// tables (full coverage of the pattern subset syntax — nothing falls
    /// outside it).
    #[test]
    fn compiles_real_data() {
        // Load the real rule table for the **active** data version and
        // compile it fully — this is a version-independent sanity check:
        // counts grow with each version (91/775/682 for 4.5.0.3.4, 95/788/686
        // for 4.5.2.1.3), so we assert lower bounds ("sufficiently
        // populated") rather than exact values, keeping the test valid
        // across any data version (same approach as the multi_version smoke
        // test).
        let path = crate::mod_parser::engine::test_support::real_rules_path();
        let json = std::fs::read_to_string(path).unwrap();
        let doc: ModParserRulesDoc = serde_json::from_str(&json).unwrap();
        let c = CompiledParserRules::compile(&doc)
            .expect("the real rule table should compile completely");
        assert!(c.forms.len() >= 85, "forms={}", c.forms.len());
        assert!(c.name_map.len() >= 750, "name_map={}", c.name_map.len());
        assert!(
            c.tag_phrases.len() >= 650,
            "tag_phrases={}",
            c.tag_phrases.len()
        );
    }
}
