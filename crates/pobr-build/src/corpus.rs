//! Modifier text corpus statistics and unsupported-line classification (/A-2; after
//! cleanup, only the engine production classification remains).
//!
//! Source of truth for "migrate in batches ordered by ninja hit frequency": runs the
//! full modifier text of the build fixtures through the data-driven engine's
//! classification, then sorts by normalized-template frequency. This **bypasses**
//! `calc_orchestrator::filter_parseable` (which silently drops unparseable modifiers)
//! and classifies the raw lines directly, so gaps in the corpus stay visible.
//!
//! Reuse note: this module lives in pobr-build so it's reachable (the ninja_parity
//! report section calls it directly). Zero I/O — corpus lines are collected and passed
//! in by the caller.

use std::collections::BTreeMap;

use pobr_core::mod_parser::ParseStatus;

/// Template normalization: numbers → `#`, whitespace collapsed, lowercased, PoB2 bracket
/// markers stripped (`[A|B]`→`B`). Lines with the same template but different values
/// count as one.
pub fn normalize_template(text: &str) -> String {
    let stripped = strip_brackets(text);
    let lower = stripped.to_ascii_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut chars = lower.chars().peekable();
    let mut prev_space = false;
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            // Consume the whole number (including a decimal point) as a single `#`.
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

/// Strips PoB2 bracket markers: `[internal name|display name]`→display name, `[name]`→name.
/// Public because the report side needs the same normalization when comparing against
/// the vendor ModCache golden data (whose keys are the expanded text).
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

/// Modifier source (for statistics bucketing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSource {
    /// Item mod (implicit / explicit / enchant).
    Item,
    /// Passive node stat.
    Passive,
    /// Jewel grant line.
    Jewel,
}

/// One corpus input line (raw text + source + owning build id).
#[derive(Debug, Clone)]
pub struct CorpusLine {
    /// Raw modifier text.
    pub text: String,
    /// Source category.
    pub source: LineSource,
    /// Owning build id (used to count builds_hit).
    pub build_id: String,
}

// Engine production classification (makes A2's silent degradation visible)

/// Line classification under the engine's production behavior (after B3, the gate uses
/// the same parser as ingest — this classification *is* production behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineLineClass {
    /// `Parsed` with no leftover: kept and takes effect in production.
    Parsed,
    /// `Parsed` but with unparsed leftover: the engine recognized part of it, but the B3
    /// gate drops the whole line, following vendor's `list and not extra` semantics —
    /// a **high-value migration candidate** (usually missing just one tag/name data entry).
    Partial,
    /// No rule matched: dropped in production.
    Unsupported,
}

/// Classifies a single line under the engine's production semantics, plus a count of
/// silently-dropped tags.
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

/// Per-template aggregate under the engine's classification (shared by the gap ranking
/// and the dropped-tag ranking).
#[derive(Debug, Clone)]
pub struct EngineTemplateStat {
    /// Normalized template.
    pub template: String,
    /// Classification (first line's class wins for a given template).
    pub class: EngineLineClass,
    /// Number of distinct builds this template hit.
    pub builds_hit: usize,
    /// Total occurrence count.
    pub total_count: usize,
    /// Total pre_flag tags dropped for this template.
    pub dropped_tags: u32,
    /// Representative raw-text samples (up to 3).
    pub samples: Vec<String>,
}

/// Aggregate report under the engine's production classification.
#[derive(Debug, Clone, Default)]
pub struct EngineCorpusReport {
    /// Total line count.
    pub total_lines: usize,
    /// Fully parsed (takes effect in production).
    pub parsed: usize,
    /// Partially parsed (dropped in production; migration candidate).
    pub partial: usize,
    /// No match at all (dropped in production).
    pub unsupported: usize,
    /// Number of lines that dropped at least one tag (risk surface for over-apply due to
    /// widened scope).
    pub lines_with_dropped_tags: usize,
    /// Total dropped tag count.
    pub total_dropped_tags: u32,
    /// Gap template ranking (Partial + Unsupported, sorted by builds_hit desc / count desc).
    pub gap_templates: Vec<EngineTemplateStat>,
    /// Dropped-tag template ranking (includes Parsed lines — an approximation of
    /// "parsed successfully but scope was widened").
    pub dropped_tag_templates: Vec<EngineTemplateStat>,
}

impl EngineCorpusReport {
    /// Production gap rate: (partial + unsupported) / total.
    pub fn gap_rate(&self) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            (self.partial + self.unsupported) as f64 / self.total_lines as f64
        }
    }
}

/// Report under the engine's production classification: classification + dropped-tag
/// counts aggregated by normalized template.
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
        // Same template with different values normalizes the same.
        assert_eq!(
            normalize_template("+50 to maximum Life"),
            normalize_template("+120 to maximum Life")
        );
    }

    #[test]
    fn classify_line_engine_distinguishes_classes() {
        let rules = pobr_core::mod_parser::test_compiled_rules();
        // Parsed: standard form.
        assert_eq!(
            classify_line_engine("20% increased Fire Damage", &rules).0,
            EngineLineClass::Parsed
        );
        // Unsupported: a known, intentionally-unsupported case (mirrored).
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
        // Both frobnicate lines share a template after number normalization →
        // builds_hit=2, ranked first.
        assert_eq!(
            report.gap_templates[0].template,
            "frobnicate the widget # times"
        );
        assert_eq!(report.gap_templates[0].builds_hit, 2);
        assert_eq!(report.gap_templates[0].total_count, 2);
        // gap_rate = 3/4.
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
