//! M6-B parser 引擎 vs legacy 中位耗时 bench（蓝图 §9）。
//!
//! 门禁口径：`engine ≤ 1.10 × legacy` 中位耗时（roadmap「parse bench 退化 ≤10%」
//! 操作化）。本 bench 需 `parser-engine` feature（见 Cargo.toml required-features）。
//!
//! 三组：
//! 1. `parse_corpus_legacy` vs `parse_corpus_engine`：同一固定语料逐行解析吞吐；
//! 2. `compile_rules`：ParserRules → CompiledParserRules（含 aho-corasick 构建）
//!    一次性成本（载入期、非热路径）。
//!
//! 运行：`cargo bench -p pobr-core --features parser-engine --bench mod_parser_bench`

use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use pobr_core::mod_parser::{CompiledParserRules, parse_mod, parse_mod_engine};
use pobr_data::catalog::parser_rules::ModParserRulesDoc;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_doc() -> ModParserRulesDoc {
    let path = repo_root().join("data/4.5.0.3.4/overlay/mod_parser_rules.json");
    let json = std::fs::read_to_string(&path).expect("读取 mod_parser_rules.json");
    serde_json::from_str(&json).expect("反序列化规则表")
}

/// 固定混合语料（从 18-build Item 文本块抽样 + 去重，确定性，截前 1000 行）。
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
                    // 去 crafting 标签前缀 `{enchant}{rune}` 等。
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
    let rules = CompiledParserRules::compile(&doc).expect("编译规则表");
    let lines = corpus();
    assert!(!lines.is_empty(), "bench 语料不应为空");

    let mut group = c.benchmark_group("mod_parser");
    group.bench_function("parse_corpus_legacy", |b| {
        b.iter(|| {
            for line in &lines {
                let _ = std::hint::black_box(parse_mod(std::hint::black_box(line)));
            }
        })
    });
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
