//! Coverage report: printed and written to `generated/parse-coverage.json`.
//!
//! The report feeds both the special-mods long-tail cleanup (10-G2) and the
//! coverage-ratchet CI check. `parse-coverage.json` is a byte-stable
//! artifact (checked in the regen-check generated section); the gap top-N is
//! truncated in lexicographic order (the corpus is already deduplicated, so
//! "hit frequency" just means presence — a real frequency-weighted top-N
//! needs C3/C4's counted corpus, deferred to later work).

use std::path::Path;

use serde::Serialize;

use crate::parsed::{Coverage, serialize_pretty_stable};

/// Top-level `parse-coverage.json`.
#[derive(Serialize)]
struct CoverageReport<'a> {
    #[serde(rename = "_meta")]
    meta: ReportMeta<'a>,
    summary: Summary,
    by_source: Vec<SourceRow>,
    gaps_top_n: Vec<crate::parsed::GapEntry>,
}

#[derive(Serialize)]
struct ReportMeta<'a> {
    schema: &'a str,
    generator: &'a str,
    note: &'a str,
}

#[derive(Serialize)]
struct Summary {
    total: usize,
    parsed: usize,
    unsupported: usize,
    err: usize,
    /// parsed / total, rounded to 6 decimal places (comparison precision for the ratchet baseline).
    coverage_ratio: f64,
}

#[derive(Serialize)]
struct SourceRow {
    source: String,
    parsed: usize,
    unsupported: usize,
    err: usize,
}

const SCHEMA: &str = "parse-coverage/v1";
const GENERATOR: &str = "precompile-mods --report";
const NOTE: &str = "M6-T7 覆盖率报表；coverage_ratio 进 devs/ci/parse-coverage-baseline.json 棘轮";

/// Round to 6 decimal places (keeps f64 trailing noise from affecting
/// byte-stability and ratchet comparisons).
pub fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

/// Print the report to stderr and write `generated/parse-coverage.json`.
pub fn emit(cov: &Coverage, top_n: usize, data_dir: &Path) -> Result<(), String> {
    let ratio = round6(cov.coverage_ratio());

    // stderr summary
    eprintln!("--- parse coverage report ---");
    eprintln!(
        "total {} | parsed {} | unsupported {} | err {} | coverage {:.4}",
        cov.total, cov.parsed, cov.unsupported, cov.err, ratio
    );
    eprintln!("by source:");
    for (src, [p, u, e]) in &cov.by_source {
        let t = p + u + e;
        let r = if t == 0 { 1.0 } else { *p as f64 / t as f64 };
        eprintln!(
            "  {src:<12} parsed {p:>5} / total {t:>5}  ({:.1}%)",
            r * 100.0
        );
    }
    eprintln!("gaps top-{top_n} (lexicographic order):");
    for gap in cov.gaps.iter().take(top_n) {
        eprintln!("  [{}/{}] {}", gap.source, gap.status, gap.text);
    }

    // JSON artifact
    let by_source: Vec<SourceRow> = cov
        .by_source
        .iter()
        .map(|(src, [p, u, e])| SourceRow {
            source: (*src).to_string(),
            parsed: *p,
            unsupported: *u,
            err: *e,
        })
        .collect();

    // gaps are already lexicographic (from corpus.lines' BTreeMap order); just truncate to top-N.
    let gaps_top_n: Vec<_> = cov.gaps.iter().take(top_n).cloned().collect();

    let report = CoverageReport {
        meta: ReportMeta {
            schema: SCHEMA,
            generator: GENERATOR,
            note: NOTE,
        },
        summary: Summary {
            total: cov.total,
            parsed: cov.parsed,
            unsupported: cov.unsupported,
            err: cov.err,
            coverage_ratio: ratio,
        },
        by_source,
        gaps_top_n,
    };

    let generated_dir = data_dir.join("generated");
    std::fs::create_dir_all(&generated_dir)
        .map_err(|e| format!("failed to create {}: {e}", generated_dir.display()))?;
    let out_path = generated_dir.join("parse-coverage.json");
    let json = serialize_pretty_stable(&report)?;
    std::fs::write(&out_path, &json)
        .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
    eprintln!("precompile-mods: wrote {}", out_path.display());
    Ok(())
}
