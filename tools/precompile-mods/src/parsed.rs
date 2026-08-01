//! Precompile: run each corpus line through the data-driven parser engine,
//! producing `parsed_mods.json` plus coverage stats.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use pobr_core::mod_parser::{CompiledParserRules, ParseStatus, parse_mod_engine};
use pobr_gamedata::GameData;

use crate::canonical::CanonMod;
use crate::corpus::Corpus;

/// Three-way status counts plus per-source-group coverage.
pub struct Coverage {
    pub total: usize,
    pub parsed: usize,
    pub unsupported: usize,
    pub err: usize,
    /// Three-way counts grouped by primary source label.
    pub by_source: BTreeMap<&'static str, [usize; 3]>, // [parsed, unsupported, err]
    /// Unsupported/error text lines (gaps), lexicographic order — the report's top-N draws from here.
    pub gaps: Vec<GapEntry>,
}

/// A gap entry (used for the coverage report's top-N).
#[derive(Debug, Clone, Serialize)]
pub struct GapEntry {
    pub text: String,
    pub status: String,
    pub source: &'static str,
}

impl Coverage {
    pub fn coverage_ratio(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.parsed as f64 / self.total as f64
    }
}

/// Top-level `parsed_mods.json` schema.
#[derive(Serialize)]
struct ParsedModsDoc<'a> {
    #[serde(rename = "_meta")]
    meta: Meta<'a>,
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct Meta<'a> {
    schema: &'a str,
    generator: &'a str,
    note: &'a str,
    /// Total corpus lines (after deduplication).
    corpus_lines: usize,
    /// Parse engine identifier (`engine` once fully data-driven; schema version bumps with it).
    engine: &'a str,
}

#[derive(Serialize)]
struct Entry {
    text: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mods: Vec<CanonMod>,
}

/// Where precompile wrote its output, plus stats.
pub struct PrecompileOutcome {
    pub parsed_mods_path: PathBuf,
    pub entries: usize,
    pub coverage: Coverage,
}

const SCHEMA: &str = "parsed_mods/v2";
const GENERATOR: &str = "precompile-mods --data";
// Post-cleanup (legacy removed): precompile runs through the data-driven
// scan engine (including the special-mod channel compile), the same parser
// the runtime (orchestrator / session) uses.
const ENGINE: &str = "scan_engine+special";
const NOTE: &str =
    "M6-T7 离线预解析；运行时（D-T8）作 text→Vec<Modifier> 缓存兜底，cache miss 回退在线 parse";

/// Collect corpus → run each line through the data-driven parser engine →
/// write `parsed_mods.json` (byte-stable).
///
/// Engine rules are compiled once from `data_dir`'s game data and reused
/// across the whole corpus (same parser the runtime uses).
pub fn precompile(corpus: &Corpus, data_dir: &Path) -> Result<PrecompileOutcome, String> {
    // Compile the parser engine rules once at startup (the six parse-rule
    // tables plus the special channel), reused across the whole corpus.
    let rules = compile_parser_rules(data_dir)?;

    let mut entries = Vec::with_capacity(corpus.lines.len());
    let mut cov = Coverage {
        total: 0,
        parsed: 0,
        unsupported: 0,
        err: 0,
        by_source: BTreeMap::new(),
        gaps: Vec::new(),
    };

    for (text, sources) in &corpus.lines {
        cov.total += 1;
        let label = sources.primary_label();
        let slot = cov.by_source.entry(label).or_insert([0, 0, 0]);

        // The engine never returns a hard error — the `err` count stays in
        // the schema (always 0) so the report shape doesn't change.
        let outcome = parse_mod_engine(text, &rules);
        let (status, mods): (&'static str, Vec<CanonMod>) = match outcome.status {
            ParseStatus::Parsed => {
                cov.parsed += 1;
                slot[0] += 1;
                let mods = outcome.mods.iter().map(CanonMod::from_mod).collect();
                ("parsed", mods)
            }
            ParseStatus::Unsupported => {
                cov.unsupported += 1;
                slot[1] += 1;
                cov.gaps.push(GapEntry {
                    text: text.clone(),
                    status: "unsupported".to_string(),
                    source: label,
                });
                ("unsupported", Vec::new())
            }
        };

        entries.push(Entry {
            text: text.clone(),
            status,
            mods,
        });
    }

    // entries are already ordered by corpus.lines (BTreeMap lexicographic order), keeping output byte-stable.
    let doc = ParsedModsDoc {
        meta: Meta {
            schema: SCHEMA,
            generator: GENERATOR,
            note: NOTE,
            corpus_lines: corpus.lines.len(),
            engine: ENGINE,
        },
        entries,
    };

    let generated_dir = data_dir.join("generated");
    std::fs::create_dir_all(&generated_dir)
        .map_err(|e| format!("创建 {} 失败：{e}", generated_dir.display()))?;
    let out_path = generated_dir.join("parsed_mods.json");
    let json = serialize_pretty_stable(&doc)?;
    std::fs::write(&out_path, &json).map_err(|e| format!("写 {} 失败：{e}", out_path.display()))?;

    Ok(PrecompileOutcome {
        parsed_mods_path: out_path,
        entries: doc.entries.len(),
        coverage: cov,
    })
}

/// Compile engine rules from `data_dir`'s game data (the six parse-rule
/// tables plus the special channel), once at startup and reused across the
/// whole corpus (same compile path as `BuildData::load`).
///
/// A missing `overlay/mod_parser_rules.json` (an old data pack) is a hard
/// error: with the legacy parser removed there's no fallback, and
/// precompiling a ruleless data pack is pointless (fail fast beats
/// producing an all-unsupported artifact).
fn compile_parser_rules(data_dir: &Path) -> Result<CompiledParserRules, String> {
    let data = GameData::new(data_dir);

    let doc = data
        .mod_parser_rules()
        .map_err(|e| format!("加载 mod_parser_rules.json 失败：{e}"))?
        .ok_or_else(|| {
            format!(
                "{} 缺 overlay/mod_parser_rules.json——无解析规则无法预编译",
                data_dir.display()
            )
        })?;
    let special_entries = data
        .load_ruleset()
        .map_err(|e| format!("加载 ruleset（special 条目）失败：{e}"))?
        .special_mods
        .unwrap_or_default();
    CompiledParserRules::compile_with_special(&doc, &special_entries)
        .map_err(|e| format!("parser 规则编译失败：{e:?}"))
}

/// Stable pretty JSON with two-space indent and a trailing newline (matches
/// the repo's existing generated-artifact style).
pub fn serialize_pretty_stable<T: Serialize>(value: &T) -> Result<String, String> {
    let mut json = serde_json::to_string_pretty(value).map_err(|e| format!("序列化失败：{e}"))?;
    json.push('\n');
    Ok(json)
}
