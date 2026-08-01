//! `precompile-mods`: offline precompile tool.
//!
//! Collects the four corpus layers (§5.1: C1 build XML / C2 passive_tree /
//! special_derived expansion / `--corpus-extra` add-on), deduplicates them,
//! and runs each line through `pobr-core`'s data-driven scan engine to
//! produce two `data/<version>/generated/` artifacts plus a coverage report:
//!
//! - `generated/parsed_mods.json`: `{ _meta, entries: [{ text, status, mods }] }`
//!   (text in lexicographic order, byte-stable). The runtime (D-T8) lazily
//!   loads this via gamedata as a `text → Vec<Modifier>` cache, so the hot
//!   path never parses.
//! - Coverage report (printed and written to `parse-coverage.json` with
//!   `--report`): parsed / unsupported / err counts, plus a gap top-N sorted
//!   by hit frequency.
//!
//! Usage:
//! ```text
//! cargo run -p precompile-mods -- --data data/4.5.0.3.4 [--corpus-extra <file>] [--report]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use precompile_mods::{check, corpus, parsed, report};

/// Parsed command-line arguments.
struct Args {
    /// Version data directory (e.g. `data/4.5.0.3.4`).
    data_dir: PathBuf,
    /// Optional add-on corpus file (one mod text per line; `#`-prefixed and blank lines ignored).
    corpus_extra: Option<PathBuf>,
    /// Print the coverage report and write `generated/parse-coverage.json`.
    report: bool,
    /// Number of top gaps to report (default 40).
    top_n: usize,
    /// Only validate overlay JSON (no precompile artifacts); non-zero exit on invalid data.
    check_only: bool,
}

const DEFAULT_TOP_N: usize = 40;

fn parse_args() -> Result<Args, String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut corpus_extra: Option<PathBuf> = None;
    let mut report = false;
    let mut top_n = DEFAULT_TOP_N;
    let mut check_only = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--data" => {
                data_dir = Some(PathBuf::from(it.next().ok_or("--data is missing a value")?));
            }
            "--corpus-extra" => {
                corpus_extra = Some(PathBuf::from(
                    it.next().ok_or("--corpus-extra is missing a value")?,
                ));
            }
            "--report" => report = true,
            "--check" => check_only = true,
            "--top-n" => {
                top_n = it
                    .next()
                    .ok_or("--top-n is missing a value")?
                    .parse()
                    .map_err(|_| "--top-n must be a positive integer".to_string())?;
            }
            "-h" | "--help" => {
                return Err("HELP".to_string());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let data_dir = data_dir.ok_or("missing required argument --data <version_dir>")?;
    Ok(Args {
        data_dir,
        corpus_extra,
        report,
        top_n,
        check_only,
    })
}

fn usage() {
    eprintln!(
        "precompile-mods -- M6-T7 offline mod precompiler\n\
         \n\
         usage:\n  \
         precompile-mods --data <version_dir> [--corpus-extra <file>] [--report] [--top-n N]\n\
         \n\
         arguments:\n  \
         --data <dir>          version data directory (e.g. data/4.5.0.3.4), required\n  \
         --corpus-extra <file> add-on corpus file (one mod text per line)\n  \
         --report               print the coverage report + write generated/parse-coverage.json\n  \
         --check                only validate overlay JSON (deserialize + unknown fields + compile),\n                        \
                               non-zero exit on invalid data; writes no artifacts\n  \
         --top-n N              number of top coverage gaps to report (default {DEFAULT_TOP_N})\n\
         \n\
         artifacts:\n  \
         <data>/generated/parsed_mods.json     precompiled corpus (byte-stable)\n  \
         <data>/generated/parse-coverage.json  coverage report (with --report)"
    );
}

fn run() -> Result<(), String> {
    let args = parse_args().map_err(|e| {
        if e == "HELP" {
            usage();
            std::process::exit(0);
        }
        e
    })?;

    let data_dir = args
        .data_dir
        .canonicalize()
        .unwrap_or_else(|_| args.data_dir.clone());
    if !data_dir.is_dir() {
        return Err(format!(
            "--data directory does not exist: {}",
            data_dir.display()
        ));
    }

    // --check: validate overlay JSON only; non-zero exit on invalid data, no artifacts written.
    if args.check_only {
        check::check(&data_dir)?;
        eprintln!(
            "precompile-mods: overlay JSON validation passed ({})",
            data_dir.display()
        );
        return Ok(());
    }

    // 1) Collect the corpus (four layers, deduplicated, lexicographic order).
    let corpus = corpus::collect(&data_dir, args.corpus_extra.as_deref())?;
    eprintln!(
        "precompile-mods: corpus {} line(s) (deduplicated), sources {}",
        corpus.lines.len(),
        corpus.source_summary()
    );

    // 2) Precompile line by line, producing parsed_mods.json + coverage stats.
    let outcome = parsed::precompile(&corpus, &data_dir)?;
    eprintln!(
        "precompile-mods: wrote {} ({} entries)",
        outcome.parsed_mods_path.display(),
        outcome.entries
    );

    // 3) Coverage report.
    let cov = &outcome.coverage;
    eprintln!(
        "precompile-mods: coverage {:.4} (parsed {} / unsupported {} / err {} / total {})",
        cov.coverage_ratio(),
        cov.parsed,
        cov.unsupported,
        cov.err,
        cov.total
    );
    if args.report {
        report::emit(cov, args.top_n, &data_dir)?;
    }

    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("precompile-mods: error -- {e}");
            ExitCode::FAILURE
        }
    }
}
