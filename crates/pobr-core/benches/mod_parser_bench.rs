//! Median-latency benchmark for the parser engine.
//!
//! Two groups:
//! 1. `parse_corpus_engine`: line-by-line parse throughput over a fixed corpus (single parser;
//!    the legacy one has been removed);
//! 2. `compile_rules`: the one-time cost of ParserRules → CompiledParserRules (including
//!    building the aho-corasick automaton) — a load-time, not hot-path, cost.
//!
//! Run: `cargo bench -p pobr-core --bench mod_parser_bench`

use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use pobr_core::mod_parser::{CompiledParserRules, parse_mod_engine};
use pobr_data::catalog::parser_rules::ModParserRulesDoc;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_doc() -> ModParserRulesDoc {
    let path = repo_root()
        .join("data")
        .join(pobr_data::data_version())
        .join("overlay/mod_parser_rules.json");
    let json = std::fs::read_to_string(&path).expect("read mod_parser_rules.json");
    serde_json::from_str(&json).expect("deserialize the rule table")
}

/// Fixed mixed corpus (sampled from the item text blocks of the 18-build set, deduplicated,
/// deterministic, truncated to the first 1000 lines).
fn corpus() -> Vec<String> {
    let builds_dir = repo_root().join("examples/demo-bd-test/builds");
    let mut lines = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&builds_dir) {
        for entry in entries.flatten() {
            let xml_path = entry.path().join("decoded.xml");
            if let Ok(xml) = std::fs::read_to_string(&xml_path) {
                let mut in_item = false;
                for raw in xml.lines() {
                    let line = raw.trim();
                    if line.starts_with("<Item") {
                        in_item = true;
                        continue;
                    }
                    if line.starts_with("</Item>") {
                        in_item = false;
                        continue;
                    }
                    if !in_item || line.is_empty() || line.starts_with('<') {
                        continue;
                    }
                    // Strip crafting tag prefixes like `{enchant}{rune}`.
                    let mut rest = line;
                    while let Some(stripped) = rest.strip_prefix('{') {
                        if let Some(end) = stripped.find('}') {
                            rest = &stripped[end + 1..];
                        } else {
                            break;
                        }
                    }
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        lines.push(rest.to_string());
                    }
                }
            }
        }
    }
    lines.sort();
    lines.dedup();
    lines.truncate(1000);
    lines
}

fn bench_parse(c: &mut Criterion) {
    let doc = load_doc();
    let rules = CompiledParserRules::compile(&doc).expect("compile the rule table");
    let lines = corpus();
    assert!(!lines.is_empty(), "bench corpus should not be empty");

    let mut group = c.benchmark_group("mod_parser");
    group.bench_function("parse_corpus_engine", |b| {
        b.iter(|| {
            for line in &lines {
                let _ = std::hint::black_box(parse_mod_engine(std::hint::black_box(line), &rules));
            }
        })
    });
    group.finish();

    c.bench_function("compile_rules", |b| {
        b.iter(|| std::hint::black_box(CompiledParserRules::compile(std::hint::black_box(&doc))))
    });
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
