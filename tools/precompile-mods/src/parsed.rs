//! 预解析：逐语料行过 `parse_mod`，产出 `parsed_mods.json` + 覆盖率统计。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use pobr_core::mod_parser::{ParseStatus, parse_mod};

use crate::canonical::CanonMod;
use crate::corpus::Corpus;

/// 三态计数 + 分组覆盖率（蓝图 §6.3）。
pub struct Coverage {
    pub total: usize,
    pub parsed: usize,
    pub unsupported: usize,
    pub err: usize,
    /// 按主来源标签分组的三态计数。
    pub by_source: BTreeMap<&'static str, [usize; 3]>, // [parsed, unsupported, err]
    /// 未支持/出错的文本行（缺口），字典序——报表 top-N 从此取。
    pub gaps: Vec<GapEntry>,
}

/// 缺口条目（覆盖率报表 top-N 用）。
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

/// `parsed_mods.json` 顶层 schema（蓝图 §6.1）。
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
    /// 语料行总数（去重后）。
    corpus_lines: usize,
    /// parse 引擎标识（B 引擎切换后改值，schema 版本随之 bump）。
    parser_engine: &'a str,
}

#[derive(Serialize)]
struct Entry {
    text: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mods: Vec<CanonMod>,
}

/// precompile 产物落点与统计。
pub struct PrecompileOutcome {
    pub parsed_mods_path: PathBuf,
    pub entries: usize,
    pub coverage: Coverage,
}

const SCHEMA: &str = "parsed_mods/v1";
const GENERATOR: &str = "precompile-mods --data";
const PARSER_ENGINE: &str = "legacy/mod_parser.rs"; // B 引擎切换后改 "scan/mod_parser"
const NOTE: &str =
    "M6-T7 离线预解析；运行时（D-T8）作 text→Vec<Modifier> 缓存兜底，cache miss 回退在线 parse_mod";

/// 收集语料 → 逐行 parse → 写 `parsed_mods.json`（byte-stable）。
pub fn precompile(corpus: &Corpus, data_dir: &Path) -> Result<PrecompileOutcome, String> {
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

        let (status, mods): (&'static str, Vec<CanonMod>) = match parse_mod(text) {
            Ok(outcome) => match outcome.status {
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
            },
            Err(_) => {
                cov.err += 1;
                slot[2] += 1;
                cov.gaps.push(GapEntry {
                    text: text.clone(),
                    status: "err".to_string(),
                    source: label,
                });
                ("err", Vec::new())
            }
        };

        entries.push(Entry {
            text: text.clone(),
            status,
            mods,
        });
    }

    // entries 已随 corpus.lines（BTreeMap 字典序）有序，保证 byte-stable。
    let doc = ParsedModsDoc {
        meta: Meta {
            schema: SCHEMA,
            generator: GENERATOR,
            note: NOTE,
            corpus_lines: corpus.lines.len(),
            parser_engine: PARSER_ENGINE,
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

/// 两空格缩进 + 末尾换行的稳定 pretty JSON（与仓库既有 generated 产物风格一致）。
pub fn serialize_pretty_stable<T: Serialize>(value: &T) -> Result<String, String> {
    let mut json = serde_json::to_string_pretty(value).map_err(|e| format!("序列化失败：{e}"))?;
    json.push('\n');
    Ok(json)
}
