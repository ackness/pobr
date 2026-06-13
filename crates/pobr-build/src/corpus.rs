//! 词条语料统计与 unsupported 分类（M5b 蓝图 A-1/A-2）。
//!
//! 「按 ninja 命中频率分批迁移」的事实来源：对 build fixture 的全量词条文本走
//! [`pobr_core::mod_parser::parse_mod`] 分三类（Parsed / Unsupported / Err），
//! 模板归一化后按频率排序。**绕开** `calc_orchestrator::filter_parseable`
//! （它把 `Err` 词条静默丢弃），直接对原始行分类，使缺口语料可见。
//!
//! 复用约定：本模块是 pobr-build 可达位置（A-2 ninja_parity 报表段直接调用；
//! A-1 的 CLI `corpus-report` 子命令反向复用同一分类函数）。零 I/O——语料行由
//! 调用方收集传入。

use std::collections::BTreeMap;

use pobr_core::mod_parser::{ParseStatus, parse_mod, parse_mod_with_rules};
use pobr_core::rules::{HandlerRegistry, SpecialModRules};

/// 单条词条的解析分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClass {
    /// `Ok(Parsed)`：成功产出 modifier。
    Parsed,
    /// `Ok(Unsupported)`：识别但不产数值（软不支持）。
    Unsupported,
    /// `Err(_)`：结构性解析失败（硬失败）。
    Err,
}

impl LineClass {
    /// 是否属「缺口语料」（Unsupported / Err 合并——迁移目标候选）。
    pub fn is_gap(self) -> bool {
        matches!(self, Self::Unsupported | Self::Err)
    }
}

/// 对单行词条分类（已含 strip-brackets / normalize 由 parse_mod 内部处理）。
pub fn classify_line(text: &str) -> LineClass {
    match parse_mod(text) {
        Ok(outcome) => match outcome.status {
            ParseStatus::Parsed => LineClass::Parsed,
            ParseStatus::Unsupported => LineClass::Unsupported,
        },
        Err(_) => LineClass::Err,
    }
}

/// special 规则增强版分类（M5b A-2 曲线）：special 整行命中优先（同
/// `parse_mod_with_rules` 口径），用于「unsupported 词条率下降曲线」——B-4
/// 激活后 special 条目命中的缺口语料从 Err/Unsupported 转 Parsed。
pub fn classify_line_with_rules(
    text: &str,
    rules: &SpecialModRules,
    registry: &HandlerRegistry,
) -> LineClass {
    match parse_mod_with_rules(text, Some(rules), Some(registry)) {
        Ok(outcome) => match outcome.status {
            ParseStatus::Parsed => LineClass::Parsed,
            ParseStatus::Unsupported => LineClass::Unsupported,
        },
        Err(_) => LineClass::Err,
    }
}

/// special 规则增强版报表（M5b A-2）：与 [`build_report`] 同口径但分类走
/// [`classify_line_with_rules`]——B-4 激活后的缺口语料曲线。
pub fn build_report_with_rules(
    lines: &[CorpusLine],
    rules: &SpecialModRules,
    registry: &HandlerRegistry,
) -> CorpusReport {
    build_report_inner(lines, |t| classify_line_with_rules(t, rules, registry))
}

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
fn strip_brackets(text: &str) -> String {
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

/// 一个归一化模板的聚合统计。
#[derive(Debug, Clone)]
pub struct TemplateStat {
    /// 归一化模板。
    pub template: String,
    /// 命中的不同 build 数。
    pub builds_hit: usize,
    /// 总出现次数。
    pub total_count: usize,
    /// 分类（同模板分类一致；取首条）。
    pub class: LineClass,
    /// 代表性原文样本（≤3）。
    pub samples: Vec<String>,
    /// 来源计数（item / passive / jewel）。
    pub item_count: usize,
    /// 同上。
    pub passive_count: usize,
    /// 同上。
    pub jewel_count: usize,
}

/// 全语料聚合报表。
#[derive(Debug, Clone, Default)]
pub struct CorpusReport {
    /// 词条总数。
    pub total_lines: usize,
    /// Parsed 计数。
    pub parsed: usize,
    /// Unsupported 计数。
    pub unsupported: usize,
    /// Err 计数。
    pub err: usize,
    /// 按 (builds_hit desc, total_count desc, 模板字典序) 排序的缺口模板清单。
    pub gap_templates: Vec<TemplateStat>,
}

impl CorpusReport {
    /// 缺口词条率（(unsupported + err) / total）。
    pub fn gap_rate(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.unsupported + self.err) as f64 / self.total_lines as f64
        }
    }

    /// parsed 率。
    pub fn parsed_rate(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            self.parsed as f64 / self.total_lines as f64
        }
    }
}

/// 从语料行构建报表（plain `parse_mod` 分类）。缺口模板按 ninja 命中频率排序
/// （C-2 选批事实来源）。
pub fn build_report(lines: &[CorpusLine]) -> CorpusReport {
    build_report_inner(lines, classify_line)
}

/// 报表构建内核（分类函数可注入：plain 或 special-aware）。
fn build_report_inner(lines: &[CorpusLine], classify: impl Fn(&str) -> LineClass) -> CorpusReport {
    let mut report = CorpusReport::default();
    // 模板 → 聚合中间态。
    struct Agg {
        builds: std::collections::BTreeSet<String>,
        total: usize,
        class: LineClass,
        samples: Vec<String>,
        item: usize,
        passive: usize,
        jewel: usize,
    }
    let mut gaps: BTreeMap<String, Agg> = BTreeMap::new();

    for line in lines {
        report.total_lines += 1;
        let class = classify(&line.text);
        match class {
            LineClass::Parsed => report.parsed += 1,
            LineClass::Unsupported => report.unsupported += 1,
            LineClass::Err => report.err += 1,
        }
        if !class.is_gap() {
            continue;
        }
        let template = normalize_template(&line.text);
        let agg = gaps.entry(template).or_insert_with(|| Agg {
            builds: std::collections::BTreeSet::new(),
            total: 0,
            class,
            samples: Vec::new(),
            item: 0,
            passive: 0,
            jewel: 0,
        });
        agg.builds.insert(line.build_id.clone());
        agg.total += 1;
        if agg.samples.len() < 3 && !agg.samples.iter().any(|s| s == &line.text) {
            agg.samples.push(line.text.clone());
        }
        match line.source {
            LineSource::Item => agg.item += 1,
            LineSource::Passive => agg.passive += 1,
            LineSource::Jewel => agg.jewel += 1,
        }
    }

    let mut templates: Vec<TemplateStat> = gaps
        .into_iter()
        .map(|(template, agg)| TemplateStat {
            template,
            builds_hit: agg.builds.len(),
            total_count: agg.total,
            class: agg.class,
            samples: agg.samples,
            item_count: agg.item,
            passive_count: agg.passive,
            jewel_count: agg.jewel,
        })
        .collect();
    // (builds_hit desc, total_count desc, 模板字典序)。
    templates.sort_by(|a, b| {
        b.builds_hit
            .cmp(&a.builds_hit)
            .then(b.total_count.cmp(&a.total_count))
            .then(a.template.cmp(&b.template))
    });
    report.gap_templates = templates;
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
    fn classify_distinguishes_three_classes() {
        // Parsed：标准 form。
        assert_eq!(
            classify_line("20% increased Fire Damage"),
            LineClass::Parsed
        );
        // Unsupported：已知软不支持（mirrored）。
        assert_eq!(classify_line("Mirrored"), LineClass::Unsupported);
    }

    #[test]
    fn report_sorts_gaps_by_frequency() {
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
        let report = build_report(&lines);
        assert_eq!(report.total_lines, 4);
        assert_eq!(report.parsed, 1);
        // 两个 frobnicate 行同模板（数字归一）→ builds_hit=2，排首位。
        assert_eq!(
            report.gap_templates[0].template,
            "frobnicate the widget # times"
        );
        assert_eq!(report.gap_templates[0].builds_hit, 2);
        assert_eq!(report.gap_templates[0].total_count, 2);
        assert_eq!(report.gap_templates[0].item_count, 1);
        assert_eq!(report.gap_templates[0].passive_count, 1);
        // gap_rate = 3/4。
        assert!((report.gap_rate() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn empty_corpus_is_zero_rate() {
        let report = build_report(&[]);
        assert_eq!(report.total_lines, 0);
        assert_eq!(report.gap_rate(), 0.0);
        assert_eq!(report.parsed_rate(), 0.0);
    }
}
