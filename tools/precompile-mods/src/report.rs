//! 覆盖率报表（蓝图 §6.3）：打印 + 写 `generated/parse-coverage.json`。
//!
//! 报表是 special 长尾收尾（10-G2）与覆盖率棘轮 CI 的共同输入。`parse-coverage.json`
//! 是 byte-stable 产物（进 regen-check generated 段校验）；缺口 top-N 按字典序
//! 截断（语料已去重，「命中频率」= 是否出现，故 top-N 取字典序前 N 的缺口行——
//! 频率维度需 C3/C4 带计数的语料，归后续）。

use std::path::Path;

use serde::Serialize;

use crate::parsed::{Coverage, serialize_pretty_stable};

/// `parse-coverage.json` 顶层。
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
    /// parsed / total，6 位小数（棘轮 baseline 比较口径）。
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

/// 四舍五入到 6 位小数（避免 f64 尾噪声影响 byte-stable 与棘轮比较）。
pub fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

/// 打印报表到 stderr + 写 `generated/parse-coverage.json`。
pub fn emit(cov: &Coverage, top_n: usize, data_dir: &Path) -> Result<(), String> {
    let ratio = round6(cov.coverage_ratio());

    // --- stderr 摘要 ---
    eprintln!("─── parse 覆盖率报表 ───");
    eprintln!(
        "总计 {} | parsed {} | unsupported {} | err {} | 覆盖率 {:.4}",
        cov.total, cov.parsed, cov.unsupported, cov.err, ratio
    );
    eprintln!("按来源：");
    for (src, [p, u, e]) in &cov.by_source {
        let t = p + u + e;
        let r = if t == 0 { 1.0 } else { *p as f64 / t as f64 };
        eprintln!(
            "  {src:<12} parsed {p:>5} / total {t:>5}  ({:.1}%)",
            r * 100.0
        );
    }
    eprintln!("缺口 top-{top_n}（字典序）：");
    for gap in cov.gaps.iter().take(top_n) {
        eprintln!("  [{}/{}] {}", gap.source, gap.status, gap.text);
    }

    // --- JSON 产物 ---
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

    // gaps 已字典序（来自 BTreeMap 顺序的 corpus.lines），截断 top-N。
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
        .map_err(|e| format!("创建 {} 失败：{e}", generated_dir.display()))?;
    let out_path = generated_dir.join("parse-coverage.json");
    let json = serialize_pretty_stable(&report)?;
    std::fs::write(&out_path, &json).map_err(|e| format!("写 {} 失败：{e}", out_path.display()))?;
    eprintln!("precompile-mods: 写出 {}", out_path.display());
    Ok(())
}
