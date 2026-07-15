//! 词条语料统计与 unsupported 分类（M5b 蓝图 A-1/A-2；M6 收尾后仅存 engine
//! 生产口径）。
//!
//! 「按 ninja 命中频率分批迁移」的事实来源：对 build fixture 的全量词条文本走
//! 数据驱动引擎分类，模板归一化后按频率排序。**绕开**
//! `calc_orchestrator::filter_parseable`（它把不可解析词条静默丢弃），直接对
//! 原始行分类，使缺口语料可见。
//!
//! 复用约定：本模块是 pobr-build 可达位置（ninja_parity 报表段直接调用）。
//! 零 I/O——语料行由调用方收集传入。

use std::collections::BTreeMap;

use pobr_core::mod_parser::ParseStatus;

/// 模板归一化：数字 → `#`、压缩空白、小写、剥离 PoB2 方括号标记 `[A|B]`→`B`。
/// 同模板异数值的行合并计数。
pub fn normalize_template(text: &str) -> String {
    let stripped = strip_brackets(text);
    let lower = stripped.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut chars = lower.chars().peekable();
    let mut prev_space = false;
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            // 吞掉整个数字（含小数点）→ 单个 `#`。
            while let Some(&n) = chars.peek() {
                if n.is_ascii_digit() || n == '.' {
                    chars.next();
                } else {
                    break;
                }
            }
            out.push('#');
            prev_space = false;
        } else if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// 剥离 PoB2 方括号标记：`[内部名|显示名]`→显示名、`[名]`→名。
/// pub：报表侧对照 vendor ModCache golden（其 key 为展开后文本）时同一口径。
pub fn strip_brackets(text: &str) -> String {
    if !text.contains('[') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    break;
                }
                inner.push(ic);
            }
            let display = inner.rsplit('|').next().unwrap_or(&inner);
            out.push_str(display);
        } else {
            out.push(c);
        }
    }
    out
}

/// 词条来源（统计分桶用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSource {
    /// 装备词条（implicit / explicit / enchant）。
    Item,
    /// 天赋节点 stat。
    Passive,
    /// 珠宝授予行。
    Jewel,
}

/// 单条语料输入（原文 + 来源 + 所属 build 标识）。
#[derive(Debug, Clone)]
pub struct CorpusLine {
    /// 原始词条文本。
    pub text: String,
    /// 来源类别。
    pub source: LineSource,
    /// 所属 build 标识（统计 builds_hit 用）。
    pub build_id: String,
}

// ---------------------------------------------------------------------------
// engine 生产口径（A2 静默降级可见化）
// ---------------------------------------------------------------------------

/// engine 生产口径的行分类（B3 后闸门与 ingest 同一 parser——本分类即生产行为）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineLineClass {
    /// `Parsed` 且无残留：生产保留并生效。
    Parsed,
    /// `Parsed` 但有 unparsed 残留：引擎已识别一部分，但 B3 闸门按 vendor
    /// `list and not extra` 语义丢弃整行——**高价值迁移候选**（缺的往往只是
    /// 一条 tag/name 数据条目）。
    Partial,
    /// 无规则命中：生产丢弃。
    Unsupported,
}

/// engine 口径单行分类 + 静默降级计数（丢 tag 数）。
pub fn classify_line_engine(
    text: &str,
    rules: &pobr_core::mod_parser::CompiledParserRules,
) -> (EngineLineClass, u16) {
    let (outcome, diag) = pobr_core::mod_parser::parse_mod_engine_diag(text, rules);
    let class = match outcome.status {
        ParseStatus::Parsed if outcome.unparsed.is_none() => EngineLineClass::Parsed,
        ParseStatus::Parsed => EngineLineClass::Partial,
        ParseStatus::Unsupported => EngineLineClass::Unsupported,
    };
    (class, diag.dropped_pre_flag_tags)
}

/// engine 口径的模板聚合（gap 榜与丢 tag 榜共用）。
#[derive(Debug, Clone)]
pub struct EngineTemplateStat {
    /// 归一化模板。
    pub template: String,
    /// 分类（同模板取首条）。
    pub class: EngineLineClass,
    /// 命中的不同 build 数。
    pub builds_hit: usize,
    /// 总出现次数。
    pub total_count: usize,
    /// 该模板累计丢弃的 pre_flag tag 数。
    pub dropped_tags: u32,
    /// 代表性原文样本（≤3）。
    pub samples: Vec<String>,
}

/// engine 生产口径聚合报表。
#[derive(Debug, Clone, Default)]
pub struct EngineCorpusReport {
    /// 词条总数。
    pub total_lines: usize,
    /// 完整解析（生产生效）。
    pub parsed: usize,
    /// 部分解析（生产丢弃；迁移候选）。
    pub partial: usize,
    /// 无命中（生产丢弃）。
    pub unsupported: usize,
    /// 丢 tag 的行数（作用域放大 over-apply 风险面）。
    pub lines_with_dropped_tags: usize,
    /// 累计丢弃 tag 数。
    pub total_dropped_tags: u32,
    /// 缺口模板榜（Partial + Unsupported，builds_hit desc / count desc）。
    pub gap_templates: Vec<EngineTemplateStat>,
    /// 丢 tag 模板榜（含 Parsed 行——解析成功但作用域被放大的近似）。
    pub dropped_tag_templates: Vec<EngineTemplateStat>,
}

impl EngineCorpusReport {
    /// 生产缺口率（(partial + unsupported) / total）。
    pub fn gap_rate(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.partial + self.unsupported) as f64 / self.total_lines as f64
        }
    }
}

/// engine 生产口径报表：分类 + 丢 tag 计数按归一化模板聚合。
pub fn build_report_engine(
    lines: &[CorpusLine],
    rules: &pobr_core::mod_parser::CompiledParserRules,
) -> EngineCorpusReport {
    struct Agg {
        builds: std::collections::BTreeSet<String>,
        total: usize,
        class: EngineLineClass,
        dropped: u32,
        samples: Vec<String>,
    }
    let mut report = EngineCorpusReport::default();
    let mut gaps: BTreeMap<String, Agg> = BTreeMap::new();
    let mut droppers: BTreeMap<String, Agg> = BTreeMap::new();

    for line in lines {
        report.total_lines += 1;
        let (class, dropped) = classify_line_engine(&line.text, rules);
        match class {
            EngineLineClass::Parsed => report.parsed += 1,
            EngineLineClass::Partial => report.partial += 1,
            EngineLineClass::Unsupported => report.unsupported += 1,
        }
        if dropped > 0 {
            report.lines_with_dropped_tags += 1;
            report.total_dropped_tags += u32::from(dropped);
        }
        let absorb = |map: &mut BTreeMap<String, Agg>| {
            let agg = map
                .entry(normalize_template(&line.text))
                .or_insert_with(|| Agg {
                    builds: std::collections::BTreeSet::new(),
                    total: 0,
                    class,
                    dropped: 0,
                    samples: Vec::new(),
                });
            agg.builds.insert(line.build_id.clone());
            agg.total += 1;
            agg.dropped += u32::from(dropped);
            if agg.samples.len() < 3 && !agg.samples.iter().any(|s| s == &line.text) {
                agg.samples.push(line.text.clone());
            }
        };
        if class != EngineLineClass::Parsed {
            absorb(&mut gaps);
        }
        if dropped > 0 {
            absorb(&mut droppers);
        }
    }

    let finalize = |map: BTreeMap<String, Agg>| -> Vec<EngineTemplateStat> {
        let mut v: Vec<EngineTemplateStat> = map
            .into_iter()
            .map(|(template, agg)| EngineTemplateStat {
                template,
                class: agg.class,
                builds_hit: agg.builds.len(),
                total_count: agg.total,
                dropped_tags: agg.dropped,
                samples: agg.samples,
            })
            .collect();
        v.sort_by(|a, b| {
            b.builds_hit
                .cmp(&a.builds_hit)
                .then(b.total_count.cmp(&a.total_count))
                .then(a.template.cmp(&b.template))
        });
        v
    };
    report.gap_templates = finalize(gaps);
    report.dropped_tag_templates = finalize(droppers);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_numbers_and_brackets() {
        assert_eq!(
            normalize_template("+50 to maximum Life"),
            "+# to maximum life"
        );
        assert_eq!(
            normalize_template("12.5% increased  [Critical|Critical Hit] Chance"),
            "#% increased critical hit chance"
        );
        // 同模板异数值归一一致。
        assert_eq!(
            normalize_template("+50 to maximum Life"),
            normalize_template("+120 to maximum Life")
        );
    }

    #[test]
    fn classify_line_engine_distinguishes_classes() {
        let rules = pobr_core::mod_parser::test_compiled_rules();
        // Parsed：标准 form。
        assert_eq!(
            classify_line_engine("20% increased Fire Damage", &rules).0,
            EngineLineClass::Parsed
        );
        // Unsupported：已知软不支持（mirrored）。
        assert_eq!(
            classify_line_engine("Mirrored", &rules).0,
            EngineLineClass::Unsupported
        );
    }

    #[test]
    fn report_sorts_gaps_by_frequency() {
        let rules = pobr_core::mod_parser::test_compiled_rules();
        let lines = vec![
            CorpusLine {
                text: "frobnicate the widget 5 times".into(),
                source: LineSource::Item,
                build_id: "a".into(),
            },
            CorpusLine {
                text: "frobnicate the widget 9 times".into(),
                source: LineSource::Passive,
                build_id: "b".into(),
            },
            CorpusLine {
                text: "wibble the doohickey".into(),
                source: LineSource::Item,
                build_id: "a".into(),
            },
            CorpusLine {
                text: "20% increased Fire Damage".into(),
                source: LineSource::Item,
                build_id: "a".into(),
            },
        ];
        let report = build_report_engine(&lines, &rules);
        assert_eq!(report.total_lines, 4);
        assert_eq!(report.parsed, 1);
        // 两个 frobnicate 行同模板（数字归一）→ builds_hit=2，排首位。
        assert_eq!(
            report.gap_templates[0].template,
            "frobnicate the widget # times"
        );
        assert_eq!(report.gap_templates[0].builds_hit, 2);
        assert_eq!(report.gap_templates[0].total_count, 2);
        // gap_rate = 3/4。
        assert!((report.gap_rate() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn empty_corpus_is_zero_rate() {
        let rules = pobr_core::mod_parser::test_compiled_rules();
        let report = build_report_engine(&[], &rules);
        assert_eq!(report.total_lines, 0);
        assert_eq!(report.gap_rate(), 0.0);
    }
}
