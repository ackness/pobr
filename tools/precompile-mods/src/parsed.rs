//! 预解析：逐语料行过数据驱动 parser 引擎，产出 `parsed_mods.json` + 覆盖率统计。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use pobr_core::mod_parser::{CompiledParserRules, ParseStatus, parse_mod_engine};
use pobr_gamedata::GameData;

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
    /// parse 引擎标识（M6-A2 数据驱动穿线后 = `engine`，schema 版本随之 bump）。
    engine: &'a str,
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

const SCHEMA: &str = "parsed_mods/v2";
const GENERATOR: &str = "precompile-mods --data";
// M6 收尾（删 legacy）：预解析走数据驱动 scan 引擎（special 通道编译在内），
// 与运行时（orchestrator / session）同一解析器。
const ENGINE: &str = "scan_engine+special";
const NOTE: &str =
    "M6-T7 离线预解析；运行时（D-T8）作 text→Vec<Modifier> 缓存兜底，cache miss 回退在线 parse";

/// 收集语料 → 逐行过数据驱动 parser 引擎 → 写 `parsed_mods.json`（byte-stable）。
///
/// 引擎规则从 `data_dir` 的游戏数据编译一次、全语料复用（与运行时同一解析器）。
pub fn precompile(corpus: &Corpus, data_dir: &Path) -> Result<PrecompileOutcome, String> {
    // 启动期编译一次 parser 引擎规则（解析规则六表 + special 通道），全语料复用。
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

        // 引擎永不返回硬错误——`err` 计数保留在 schema 里（恒 0），报表形状不变。
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

    // entries 已随 corpus.lines（BTreeMap 字典序）有序，保证 byte-stable。
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

/// 从 `data_dir` 的游戏数据编译引擎规则（解析规则六表 + special 通道拼接），
/// 启动期一次、全语料复用（与 `BuildData::load` 同一编译路径）。
///
/// `overlay/mod_parser_rules.json` 缺失（旧数据包）→ 硬错误：删除 legacy
/// 解析器后没有回退路径，预编译对无规则数据包无意义（fail-fast 优于产出
/// 全 unsupported 的产物）。
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

/// 两空格缩进 + 末尾换行的稳定 pretty JSON（与仓库既有 generated 产物风格一致）。
pub fn serialize_pretty_stable<T: Serialize>(value: &T) -> Result<String, String> {
    let mut json = serde_json::to_string_pretty(value).map_err(|e| format!("序列化失败：{e}"))?;
    json.push('\n');
    Ok(json)
}
